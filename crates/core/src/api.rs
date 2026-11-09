// SPDX-License-Identifier: Apache-2.0
//! SSC-025: in-memory entry points shared by the CLI and the browser engine
//! (`crates/web`), so both print the same JSON for the same inputs. Nothing
//! here touches the file system or the network.

use serde_json::{json, Value};
use stellar_xdr::{LedgerEntryData, ScVal};

use crate::decode::Decoder;
use crate::ledger::{self, executable_of, Report};
use crate::rusttype::{type_to_rust, udt_name};
use crate::{sidecar, wasm, KeyPattern, Schema};

/// Error text for callers; the CLI prints it after `error: `.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ApiError(pub String);

fn err(e: impl std::fmt::Display) -> ApiError {
    ApiError(e.to_string())
}

/// A sidecar file's text with the label it is reported under (the CLI uses
/// the file name, so `schema_source` reads `sidecar:<label>`).
pub struct Named<'a> {
    pub label: &'a str,
    pub text: &'a str,
}

pub fn parse_sidecar(s: &Named) -> Result<Schema, ApiError> {
    sidecar::parse(s.text).map_err(|e| ApiError(format!("{}: {e}", s.label)))
}

/// Decoder from an optional wasm plus sidecars. Order: embedded schema,
/// sidecars, then contractspecv0 types (SPEC.md 6.2).
pub fn build_decoder(wasm_bytes: Option<&[u8]>, sidecars: &[Named], notes: &mut Vec<String>) -> Result<Decoder, ApiError> {
    let mut d = Decoder::new();
    let mut spec = vec![];
    if let Some(w) = wasm_bytes {
        let m = crate::load_module(w).map_err(err)?;
        match &m.storage {
            Some(s) => d.add_schema(s, "embedded"),
            None => notes.push("wasm has no contractstoragev0 section".into()),
        }
        spec = m.spec;
    }
    for s in sidecars {
        d.add_schema(&parse_sidecar(s)?, &format!("sidecar:{}", s.label));
    }
    d.add_spec(&spec);
    Ok(d)
}

/// Decode `getLedgerEntries` responses (each a full response, its `result`,
/// or a bare entry array). `notes` are placed before the decoder's own.
pub fn decode(decoder: &Decoder, entries: &[Named], latest_ledger: Option<u32>, notes: Vec<String>) -> Result<Report, ApiError> {
    let mut raw = vec![];
    let mut latest = latest_ledger;
    for e in entries {
        let (mut got, l) = ledger::parse_get_ledger_entries(e.text).map_err(|x| ApiError(format!("{}: {x}", e.label)))?;
        latest = latest.or(l);
        raw.append(&mut got);
    }
    let mut report = ledger::decode_entries(decoder, &raw, latest);
    report.notes.splice(0..0, notes);
    Ok(report)
}

/// Human-readable key pattern, e.g. `[Symbol("Balance"), Address]`.
pub fn format_pattern(p: &KeyPattern) -> String {
    match p {
        KeyPattern::Exact(ScVal::Symbol(s)) => format!("Symbol({:?})", s.0.to_utf8_string_lossy()),
        KeyPattern::Exact(v) => format!("exact {}", crate::decode::shape(v)),
        KeyPattern::Tagged { tag, fields } => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| if f.name.is_empty() { type_to_rust(&f.type_) } else { format!("{}: {}", f.name, type_to_rust(&f.type_)) })
                .collect();
            if fs.is_empty() {
                format!("[Symbol({tag:?})]")
            } else {
                format!("[Symbol({tag:?}), {}]", fs.join(", "))
            }
        }
        KeyPattern::Typed(t) => format!("any {}", type_to_rust(t)),
    }
}

/// `inspect --format json`: what a module carries.
pub fn inspect(bytes: &[u8]) -> Result<Value, ApiError> {
    let m = crate::load_module(bytes).map_err(err)?;
    let spec_udts: Vec<String> = m.spec.iter().filter_map(udt_name).collect();
    let secs = wasm::sections(bytes).map_err(err)?;
    let custom: Vec<Value> = secs.iter().filter(|s| s.id == 0).map(|s| json!({"name": s.name, "bytes": s.range.len()})).collect();
    let storage = m.storage.as_ref().map(|s| {
        json!({
            "sections": m.storage_section_count,
            "payload_bytes": m.storage_bytes,
            "keys": s.keys.iter().map(|k| json!({
                "name": k.name, "lib": k.lib, "durability": k.durability,
                "key": format_pattern(&k.key),
                "value": type_to_rust(&k.value)})).collect::<Vec<_>>(),
            "types": s.types.iter().filter_map(udt_name).collect::<Vec<_>>(),
        })
    });
    Ok(json!({
        "wasm_bytes": bytes.len(),
        "custom_sections": custom,
        "contractstoragev0": storage,
        "contractspecv0": {"entries": m.spec.len(), "udts": spec_udts},
    }))
}

/// `lint` output: findings plus notes.
pub fn lint(wasm_bytes: Option<&[u8]>, sidecars: &[Named]) -> Result<Value, ApiError> {
    let mut notes = vec![];
    let d = build_decoder(wasm_bytes, sidecars, &mut notes)?;
    Ok(json!({"findings": crate::lint::lint(&d), "notes": notes}))
}

/// What a contract executes, read from a `getLedgerEntries` response that
/// holds its instance entry: `{contract, executable, wasm_hash?}`.
pub fn executable(instance_response: &str) -> Result<Value, ApiError> {
    let (entries, latest) = ledger::parse_get_ledger_entries(instance_response).map_err(err)?;
    for e in &entries {
        if let LedgerEntryData::ContractData(cd) = &e.data {
            if let (ScVal::LedgerKeyContractInstance, ScVal::ContractInstance(ci)) = (&cd.key, &cd.val) {
                let (kind, hash) = executable_of(&ci.executable);
                let mut v = json!({"contract": cd.contract.to_string(), "executable": kind, "latest_ledger": latest});
                if let Some(h) = hash {
                    v["wasm_hash"] = json!(h);
                }
                return Ok(v);
            }
        }
    }
    Err(ApiError(
        "no contract instance entry in the response (the contract may not exist on this network, or its instance is archived)".into(),
    ))
}

/// The wasm bytes from a `getLedgerEntries` response that holds a
/// ContractCode entry. The code's SHA-256 is checked against `expect_hash`
/// (hex), so a wrong or tampered response is refused.
pub fn code(code_response: &str, expect_hash: &str) -> Result<Vec<u8>, ApiError> {
    use sha2::{Digest, Sha256};
    let (entries, _) = ledger::parse_get_ledger_entries(code_response).map_err(err)?;
    for e in entries {
        if let LedgerEntryData::ContractCode(cc) = e.data {
            let got = hex::encode(Sha256::digest(cc.code.as_slice()));
            if got != expect_hash.to_ascii_lowercase() {
                return Err(ApiError(format!("code sha256 {got} does not match the instance's wasm hash {expect_hash}")));
            }
            return Ok(cc.code.to_vec());
        }
    }
    Err(ApiError("no contract code entry in the response (archived or missing)".into()))
}

/// Ledger keys (base64 XDR) for live reads: the instance, a code hash, or a
/// schema entry with string arguments.
pub fn instance_key(contract: &str) -> Result<String, ApiError> {
    Ok(ledger::ledger_key_b64(&ledger::instance_ledger_key(&parse_contract(contract)?)))
}

pub fn code_key(hash_hex: &str) -> Result<String, ApiError> {
    let b = hex::decode(hash_hex).map_err(err)?;
    let h: [u8; 32] = b.try_into().map_err(|_| ApiError("wasm hash must be 32 bytes".into()))?;
    Ok(ledger::ledger_key_b64(&ledger::code_ledger_key(h)))
}

pub fn entry_key(decoder: &Decoder, contract: &str, entry: &str, args: &[String]) -> Result<String, ApiError> {
    let e = decoder.key_entries().find(|k| k.name == entry).ok_or_else(|| ApiError(format!("no schema entry named {entry}")))?;
    Ok(ledger::ledger_key_b64(&ledger::build_ledger_key(&parse_contract(contract)?, e, args).map_err(err)?))
}

pub fn parse_contract(s: &str) -> Result<stellar_xdr::ScAddress, ApiError> {
    use std::str::FromStr;
    match stellar_xdr::ScAddress::from_str(s.trim()) {
        Ok(a @ stellar_xdr::ScAddress::Contract(_)) => Ok(a),
        _ => Err(ApiError(format!("not a contract address: {s}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Status;

    const TOKEN: &[u8] = include_bytes!("../../../fixtures/wasm/ssc_demo_token.wasm");
    const SAC: &str = include_str!("../../../sidecars/stellar-asset-contract.storage.json");
    const OZ: &str = include_str!("../../../sidecars/oz-fungible-pausable-v0.8.0.storage.json");
    const INSTANCE: &str = include_str!("../../../fixtures/mainnet/usdc-sac.instance.json");
    const BALANCES: &str = include_str!("../../../fixtures/mainnet/usdc-sac.balances.json");

    fn n<'a>(label: &'a str, text: &'a str) -> Named<'a> {
        Named { label, text }
    }

    #[test]
    fn inspect_lists_macro_keys() {
        let v = inspect(TOKEN).unwrap();
        let names: Vec<&str> = v["contractstoragev0"]["keys"].as_array().unwrap().iter().map(|k| k["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["DataKey::Allowance", "DataKey::Balance", "DataKey::Admin", "METADATA"]);
        assert_eq!(v["contractstoragev0"]["keys"][1]["key"], "[Symbol(\"Balance\"), Address]");
    }

    #[test]
    fn sac_sidecar_matches_mainnet_capture() {
        let mut notes = vec![];
        let d = build_decoder(None, &[n("stellar-asset-contract.storage.json", SAC)], &mut notes).unwrap();
        let r = decode(&d, &[n("i", INSTANCE), n("b", BALANCES)], None, notes).unwrap();
        assert_eq!(r.summary.total, 8);
        assert!(r.entries.iter().all(|e| e.status == Status::Matched));
        assert_eq!(r.entries[0].schema_source.as_deref(), Some("sidecar:stellar-asset-contract.storage.json"));
    }

    #[test]
    fn wrong_schema_is_value_mismatch_not_a_decode() {
        // The token stores Balance -> i128; the SAC stores Balance -> BalanceValue struct.
        let mut notes = vec![];
        let d = build_decoder(Some(TOKEN), &[], &mut notes).unwrap();
        let r = decode(&d, &[n("b", BALANCES)], None, notes).unwrap();
        assert!(!r.entries.is_empty());
        assert!(r.entries.iter().all(|e| e.status == Status::ValueMismatch), "{r:?}");
    }

    #[test]
    fn lint_reports_2088_collision() {
        let v = lint(Some(TOKEN), &[n("oz", OZ)]).unwrap();
        assert!(v["findings"].as_array().unwrap().iter().any(|f| f["code"] == "collision"), "{v}");
    }

    #[test]
    fn live_helpers() {
        let x = executable(INSTANCE).unwrap();
        assert_eq!(x["executable"], "stellar_asset");
        assert_eq!(x["contract"], "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75");
        assert!(executable(BALANCES).is_err());
        let k = instance_key("CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75").unwrap();
        assert!(INSTANCE.contains(&k), "instance key matches the captured response's key");
        assert!(instance_key("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN").is_err());
        assert!(code_key("00").is_err());
        assert!(code_key(&"ab".repeat(32)).is_ok());
    }

    #[test]
    fn code_refuses_a_hash_mismatch() {
        use sha2::{Digest, Sha256};
        use stellar_xdr::{ContractCodeEntry, ContractCodeEntryExt, Hash, Limits, WriteXdr};
        let bytes = TOKEN.to_vec();
        let hash: [u8; 32] = Sha256::digest(&bytes).into();
        let entry = LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash: Hash(hash),
            code: bytes.clone().try_into().unwrap(),
        });
        let resp = json!({"result": {"entries": [{"xdr": entry.to_xdr_base64(Limits::none()).unwrap()}]}}).to_string();
        assert_eq!(code(&resp, &hex::encode(hash)).unwrap(), bytes);
        assert!(code(&resp, &"00".repeat(32)).is_err());
    }
}
