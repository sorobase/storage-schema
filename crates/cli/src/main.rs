// SPDX-License-Identifier: Apache-2.0
//! `storage-schema`: inspect, inject, strip, lint and decode Soroban
//! `contractstoragev0` storage schemas. Network use is read-only
//! (`getLedgerEntries` only); nothing is signed or sent.

mod text;

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use sik_storage_schema_core as core;
use sik_storage_schema_core::decode::{Decoder, Status};
use sik_storage_schema_core::ledger::{self, RawEntry};
use sik_storage_schema_core::{api, sidecar, wasm, Schema};
use stellar_xdr::{LedgerKey, Limits, ReadXdr, ScAddress};

#[derive(Parser)]
#[command(name = "storage-schema", version, about = "Off-chain storage schemas for Soroban contracts (contractstoragev0)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum InspectFormat {
    Summary,
    Json,
    Sidecar,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutFormat {
    Json,
    Text,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show the storage schema (and spec summary) a wasm carries.
    Inspect {
        wasm: PathBuf,
        #[arg(long, value_enum, default_value = "summary")]
        format: InspectFormat,
    },
    /// Write a sidecar schema into a wasm as a contractstoragev0 section.
    Inject {
        wasm: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        /// Remove existing contractstoragev0 sections first.
        #[arg(long)]
        replace: bool,
    },
    /// Remove contractstoragev0 sections.
    Strip {
        wasm: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Check a schema for key collisions (sdk#2088) and undefined types.
    Lint {
        #[arg(long)]
        wasm: Option<PathBuf>,
        #[arg(long)]
        schema: Vec<PathBuf>,
        /// Exit 2 on warnings as well as errors.
        #[arg(long)]
        deny_warnings: bool,
    },
    /// Decode contract-data ledger entries into named, typed fields.
    Decode {
        /// Wasm to read contractstoragev0 / contractspecv0 from.
        #[arg(long)]
        wasm: Option<PathBuf>,
        /// Sidecar schema(s), merged with any embedded schema.
        #[arg(long)]
        schema: Vec<PathBuf>,
        /// getLedgerEntries JSON response file(s).
        #[arg(long)]
        entries: Vec<PathBuf>,
        /// Contract to fetch from RPC (instance entry, and wasm if not given).
        #[arg(long)]
        contract: Option<String>,
        #[arg(long)]
        rpc: Option<String>,
        /// Also fetch: `NAME=arg1,arg2` (schema entry name and key arguments).
        #[arg(long)]
        fetch: Vec<String>,
        #[arg(long, value_enum, default_value = "json")]
        format: OutFormat,
        /// Exit 2 if any entry is not `matched`.
        #[arg(long)]
        strict: bool,
    },
    /// Build the ledger key for a schema entry (base64 LedgerKey XDR).
    Key {
        #[arg(long)]
        wasm: Option<PathBuf>,
        #[arg(long)]
        schema: Vec<PathBuf>,
        #[arg(long)]
        contract: String,
        #[arg(long)]
        entry: String,
        args: Vec<String>,
    },
    /// Read-only RPC helpers (for capturing fixtures).
    Rpc {
        #[command(subcommand)]
        cmd: RpcCmd,
    },
    /// Size report: section bytes, code digest, cost against a baseline build.
    Size {
        wasm: PathBuf,
        /// Build of the same source without the schema.
        #[arg(long)]
        baseline: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum RpcCmd {
    /// getLedgerEntries for base64 LedgerKey XDRs; prints the raw response.
    Entries {
        #[arg(long)]
        rpc: String,
        #[arg(long = "key-xdr", required = true)]
        keys: Vec<String>,
    },
    /// Fetch a contract's wasm (or report that it is a Stellar Asset Contract).
    Wasm {
        #[arg(long)]
        rpc: String,
        #[arg(long)]
        contract: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

pub type Res<T> = Result<T, String>;

fn read(p: &PathBuf) -> Res<Vec<u8>> {
    std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn write(p: &PathBuf, b: &[u8]) -> Res<()> {
    std::fs::write(p, b).map_err(|e| format!("{}: {e}", p.display()))
}

fn load_sidecar(p: &PathBuf) -> Res<Schema> {
    let s = String::from_utf8(read(p)?).map_err(|e| e.to_string())?;
    sidecar::parse(&s).map_err(|e| format!("{}: {e}", p.display()))
}

fn print_json(v: &impl serde::Serialize) {
    println!("{}", serde_json::to_string_pretty(v).expect("serialises"));
}

fn e2s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Sidecar files read into memory, labelled with their file names.
fn read_sidecars(paths: &[PathBuf]) -> Res<Vec<(String, String)>> {
    paths
        .iter()
        .map(|p| {
            let label = p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            Ok((label, String::from_utf8(read(p)?).map_err(e2s)?))
        })
        .collect()
}

fn named(v: &[(String, String)]) -> Vec<api::Named<'_>> {
    v.iter().map(|(label, text)| api::Named { label, text }).collect()
}

/// Decoder from an optional wasm plus sidecars (core::api, SPEC.md 6.2).
fn build_decoder(wasm_bytes: Option<&[u8]>, sidecars: &[PathBuf], notes: &mut Vec<String>) -> Res<Decoder> {
    let files = read_sidecars(sidecars)?;
    api::build_decoder(wasm_bytes, &named(&files), notes).map_err(e2s)
}

fn parse_contract(s: &str) -> Res<ScAddress> {
    ScAddress::from_str(s).map_err(|_| format!("not a contract address: {s}"))
}

fn inspect(path: PathBuf, format: InspectFormat) -> Res<ExitCode> {
    let bytes = read(&path)?;
    let m = core::load_module(&bytes).map_err(e2s)?;
    if let InspectFormat::Sidecar = format {
        let s = m.storage.ok_or("wasm has no contractstoragev0 section")?;
        println!("{}", sidecar::to_json(&s, &format!("extracted from {}", path.file_name().unwrap_or_default().to_string_lossy())));
        return Ok(ExitCode::SUCCESS);
    }
    let spec_udts: Vec<String> = m.spec.iter().filter_map(core::rusttype::udt_name).collect();
    if let InspectFormat::Json = format {
        print_json(&api::inspect(&bytes).map_err(e2s)?);
    } else {
        text::print_inspect(&path, &bytes, &m, &spec_udts);
    }
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
fn decode(
    w: Option<PathBuf>,
    schema: Vec<PathBuf>,
    entries: Vec<PathBuf>,
    contract: Option<String>,
    rpc: Option<String>,
    fetch: Vec<String>,
    format: OutFormat,
    strict: bool,
) -> Res<ExitCode> {
    let mut notes = vec![];
    let mut raw: Vec<RawEntry> = vec![];
    let mut latest = None;
    let mut wasm_bytes = w.as_ref().map(read).transpose()?;
    let mut client = rpc.as_deref().map(core::rpc::Client::new);
    if let Some(c) = &contract {
        if let Some(cl) = client.as_mut() {
            let addr = parse_contract(c)?;
            if wasm_bytes.is_none() {
                match cl.fetch_executable(&addr).map_err(e2s)? {
                    core::rpc::Executable::Wasm { hash, code } => {
                        notes.push(format!("fetched wasm {} ({} bytes) from RPC", hex::encode(hash), code.len()));
                        wasm_bytes = Some(code);
                    }
                    core::rpc::Executable::StellarAsset => {
                        notes.push("contract is a built-in Stellar Asset Contract: no wasm, so no embedded schema; use a sidecar".into())
                    }
                    core::rpc::Executable::ExternalRef => notes.push("contract executable is an external reference; not resolved".into()),
                }
            }
            let (_, inst) = cl.fetch_instance(&addr).map_err(e2s)?;
            raw.push(inst);
        }
    }
    let decoder = build_decoder(wasm_bytes.as_deref(), &schema, &mut notes)?;
    if !fetch.is_empty() {
        let addr = parse_contract(contract.as_deref().ok_or("--fetch needs --contract")?)?;
        let mut keys: Vec<LedgerKey> = vec![];
        for f in &fetch {
            let (name, args) = f.split_once('=').unwrap_or((f.as_str(), ""));
            let args: Vec<String> = if args.is_empty() { vec![] } else { args.split(',').map(str::to_string).collect() };
            let entry = decoder.key_entries().find(|k| k.name == name).ok_or_else(|| format!("no schema entry named {name}"))?;
            if entry.durability != core::Durability::Instance {
                keys.push(ledger::build_ledger_key(&addr, entry, &args).map_err(e2s)?);
            }
        }
        if !keys.is_empty() {
            let cl = client.as_mut().ok_or("--fetch needs --rpc")?;
            let (_, mut got, l) = cl.get_ledger_entries(&keys).map_err(e2s)?;
            if got.len() < keys.len() {
                notes.push(format!("{} of {} requested entries do not exist (or are archived)", keys.len() - got.len(), keys.len()));
            }
            latest = l;
            raw.append(&mut got);
        }
    }
    for p in &entries {
        let t = String::from_utf8(read(p)?).map_err(e2s)?;
        let (mut got, l) = ledger::parse_get_ledger_entries(&t).map_err(|e| format!("{}: {e}", p.display()))?;
        latest = latest.or(l);
        raw.append(&mut got);
    }
    // Same steps as core::api::decode (used by the browser engine).
    let mut report = ledger::decode_entries(&decoder, &raw, latest);
    report.notes.splice(0..0, notes);
    match format {
        OutFormat::Json => print_json(&report),
        OutFormat::Text => text::print_report(&report),
    }
    let all_ok = report.entries.iter().all(|e| e.status == Status::Matched);
    Ok(if strict && !all_ok { ExitCode::from(2) } else { ExitCode::SUCCESS })
}

fn size(path: PathBuf, baseline: Option<PathBuf>) -> Res<ExitCode> {
    let bytes = read(&path)?;
    let m = core::load_module(&bytes).map_err(e2s)?;
    let secs = wasm::sections(&bytes).map_err(e2s)?;
    let section_bytes: usize = secs.iter().filter(|s| s.name.as_deref() == Some(core::SECTION_NAME)).map(|s| s.range.len()).sum();
    let mut report = json!({
        "wasm": path.file_name().unwrap_or_default().to_string_lossy(),
        "wasm_bytes": bytes.len(),
        "contractstoragev0_section_bytes": section_bytes,
        "contractstoragev0_payload_bytes": m.storage_bytes,
        "key_entries": m.storage.as_ref().map(|s| s.keys.len()).unwrap_or(0),
        "type_entries": m.storage.as_ref().map(|s| s.types.len()).unwrap_or(0),
        "share_of_wasm_percent": if bytes.is_empty() { 0.0 } else { (10000.0 * section_bytes as f64 / bytes.len() as f64).round() / 100.0 },
        "code_digest": wasm::code_digest(&bytes).map_err(e2s)?,
    });
    if let Some(b) = baseline {
        let bb = read(&b)?;
        let stripped = wasm::strip_custom(&bytes, core::SECTION_NAME).map_err(e2s)?;
        report["baseline"] = json!({
            "wasm": b.file_name().unwrap_or_default().to_string_lossy(),
            "wasm_bytes": bb.len(),
            "delta_bytes": bytes.len() as i64 - bb.len() as i64,
            "code_identical": wasm::code_digest(&bb).map_err(e2s)? == wasm::code_digest(&bytes).map_err(e2s)?,
            "identical_after_strip": stripped == bb,
        });
    }
    print_json(&report);
    Ok(ExitCode::SUCCESS)
}

fn run(cli: Cli) -> Res<ExitCode> {
    match cli.cmd {
        Cmd::Inspect { wasm, format } => inspect(wasm, format),
        Cmd::Inject { wasm: path, schema, out, replace } => {
            let bytes = read(&path)?;
            let new = core::inject(&bytes, &load_sidecar(&schema)?, replace).map_err(e2s)?;
            write(&out, &new)?;
            eprintln!("wrote {} ({} -> {} bytes, {:+})", out.display(), bytes.len(), new.len(), new.len() as i64 - bytes.len() as i64);
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Strip { wasm: path, out } => {
            let bytes = read(&path)?;
            let new = wasm::strip_custom(&bytes, core::SECTION_NAME).map_err(e2s)?;
            write(&out, &new)?;
            eprintln!("wrote {} ({} -> {} bytes)", out.display(), bytes.len(), new.len());
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Lint { wasm: w, schema, deny_warnings } => {
            let bytes = w.as_ref().map(read).transpose()?;
            let mut notes = vec![];
            let d = build_decoder(bytes.as_deref(), &schema, &mut notes)?;
            let findings = core::lint::lint(&d);
            // Same shape as core::api::lint (used by the browser engine).
            print_json(&json!({"findings": findings, "notes": notes}));
            let fail = match findings.iter().map(|f| f.severity).max() {
                Some(core::lint::Severity::Error) => true,
                Some(core::lint::Severity::Warning) => deny_warnings,
                _ => false,
            };
            Ok(if fail { ExitCode::from(2) } else { ExitCode::SUCCESS })
        }
        Cmd::Decode { wasm, schema, entries, contract, rpc, fetch, format, strict } => {
            decode(wasm, schema, entries, contract, rpc, fetch, format, strict)
        }
        Cmd::Key { wasm: w, schema, contract, entry, args } => {
            let bytes = w.as_ref().map(read).transpose()?;
            let d = build_decoder(bytes.as_deref(), &schema, &mut vec![])?;
            let e = d.key_entries().find(|k| k.name == entry).ok_or_else(|| format!("no schema entry named {entry}"))?;
            println!("{}", ledger::ledger_key_b64(&ledger::build_ledger_key(&parse_contract(&contract)?, e, &args).map_err(e2s)?));
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Rpc { cmd: RpcCmd::Entries { rpc, keys } } => {
            let parsed = keys
                .iter()
                .map(|k| LedgerKey::from_xdr_base64(k, Limits::none()).map_err(|e| format!("bad key {k}: {e}")))
                .collect::<Res<Vec<_>>>()?;
            let (t, _, _) = core::rpc::Client::new(&rpc).get_ledger_entries(&parsed).map_err(e2s)?;
            println!("{t}");
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Rpc { cmd: RpcCmd::Wasm { rpc, contract, out } } => {
            match core::rpc::Client::new(&rpc).fetch_executable(&parse_contract(&contract)?).map_err(e2s)? {
                core::rpc::Executable::Wasm { hash, code } => {
                    println!("{}", json!({"executable": "wasm", "wasm_hash": hex::encode(hash), "bytes": code.len()}));
                    if let Some(o) = out {
                        write(&o, &code)?;
                    }
                }
                core::rpc::Executable::StellarAsset => println!("{}", json!({"executable": "stellar_asset"})),
                core::rpc::Executable::ExternalRef => println!("{}", json!({"executable": "external_ref"})),
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Size { wasm, baseline } => size(wasm, baseline),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
