// SPDX-License-Identifier: Apache-2.0
//! Human-readable output for `inspect` and `decode --format text`.

use std::path::Path;

use sik_storage_schema_core as core;
use sik_storage_schema_core::ledger::Report;
use sik_storage_schema_core::rusttype::type_to_rust;

pub fn print_inspect(path: &Path, bytes: &[u8], m: &core::ModuleSchemas, spec_udts: &[String]) {
    println!("{} ({} bytes)", path.display(), bytes.len());
    match &m.storage {
        None => println!("  contractstoragev0: absent"),
        Some(s) => {
            println!(
                "  contractstoragev0: {} key entries, {} types, {} payload bytes in {} section(s)",
                s.keys.len(),
                s.types.len(),
                m.storage_bytes,
                m.storage_section_count
            );
            for k in &s.keys {
                let lib = if k.lib.is_empty() { String::new() } else { format!("  [{}]", k.lib) };
                println!(
                    "    {:<10} {:<32} {} -> {}{}",
                    k.durability.as_str(),
                    k.name,
                    core::api::format_pattern(&k.key),
                    type_to_rust(&k.value),
                    lib
                );
            }
            let types: Vec<String> = s.types.iter().filter_map(core::rusttype::udt_name).collect();
            if !types.is_empty() {
                println!("    types: {}", types.join(", "));
            }
        }
    }
    println!("  contractspecv0: {} entries ({} UDTs: {})", m.spec.len(), spec_udts.len(), spec_udts.join(", "));
}

pub fn print_report(r: &Report) {
    for i in &r.instances {
        println!(
            "instance {} executable={}{} storage_entries={}",
            i.contract,
            i.executable,
            i.wasm_hash.as_ref().map(|h| format!(" wasm={h}")).unwrap_or_default(),
            i.storage_entries
        );
    }
    for e in &r.entries {
        let status = serde_json::to_value(e.status).unwrap();
        println!(
            "{:<14} {:<10} {:<30} key={} value={}",
            status.as_str().unwrap_or("?"),
            e.durability.as_str(),
            e.name.clone().unwrap_or_else(|| "?".into()),
            serde_json::to_string(&e.key).unwrap(),
            serde_json::to_string(&e.value).unwrap()
        );
        if let Some(reason) = &e.reason {
            println!("{:<14} reason: {reason}", "");
        }
        for c in &e.candidates {
            println!("{:<14} candidate {} [{}] value_conforms={}", "", c.name, c.lib, c.value_conforms);
        }
        for w in &e.warnings {
            println!("{:<14} warning: {w}", "");
        }
    }
    println!("summary: {}", serde_json::to_string(&r.summary).unwrap());
    for n in &r.notes {
        println!("note: {n}");
    }
}
