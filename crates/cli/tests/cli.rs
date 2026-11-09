// SPDX-License-Identifier: Apache-2.0
//! CLI tests over committed fixtures (offline). Wasm fixtures come from
//! scripts/build-demos.sh; mainnet fixtures from scripts/capture-mainnet.sh.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fx(p: &str) -> String {
    root().join("fixtures").join(p).to_string_lossy().into_owned()
}

fn side(p: &str) -> String {
    root().join("sidecars").join(p).to_string_lossy().into_owned()
}

fn run(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_storage-schema")).args(args).output().expect("runs");
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned())
}

fn json(args: &[&str]) -> Value {
    let (code, out, err) = run(args);
    assert_eq!(code, 0, "{args:?}: {err}");
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{args:?}: {e}\n{out}"))
}

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("ssc-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().into_owned()
}

#[test]
fn inspect_shows_macro_emitted_schema() {
    let r = json(&["inspect", &fx("wasm/ssc_demo_token.wasm"), "--format", "json"]);
    let keys = r["contractstoragev0"]["keys"].as_array().unwrap();
    let names: Vec<&str> = keys.iter().map(|k| k["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["DataKey::Allowance", "DataKey::Balance", "DataKey::Admin", "METADATA"]);
    let types: Vec<&str> = r["contractstoragev0"]["types"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
    for t in ["AllowanceValue", "AllowanceDataKey", "TokenMetadata"] {
        assert!(types.contains(&t), "{types:?}");
    }
}

#[test]
fn inject_extract_roundtrip() {
    let out = tmp("oz.injected.wasm");
    let (c, _, e) = run(&[
        "inject",
        &fx("wasm/ssc_demo_oz_pausable.wasm"),
        "--schema",
        &side("oz-fungible-pausable-v0.8.0.storage.json"),
        "-o",
        &out,
        "--replace",
    ]);
    assert_eq!(c, 0, "{e}");
    let committed = std::fs::read(fx("wasm/ssc_demo_oz_pausable.injected.wasm")).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), committed, "inject is deterministic");
    let (c, extracted, e) = run(&["inspect", &out, "--format", "sidecar"]);
    assert_eq!(c, 0, "{e}");
    let ex = tmp("extracted.json");
    std::fs::write(&ex, &extracted).unwrap();
    let again = tmp("again.wasm");
    assert_eq!(run(&["inject", &fx("wasm/ssc_demo_oz_pausable.wasm"), "--schema", &ex, "-o", &again, "--replace"]).0, 0);
    assert_eq!(std::fs::read(&again).unwrap(), committed, "sidecar -> section -> sidecar -> section is stable");
    let stripped = tmp("stripped.wasm");
    assert_eq!(run(&["strip", &out, "-o", &stripped]).0, 0);
    assert_eq!(std::fs::read(&stripped).unwrap(), std::fs::read(fx("wasm/ssc_demo_oz_pausable.wasm")).unwrap());
}

#[test]
fn size_cost_is_section_bytes_only() {
    for (w, b) in [
        ("wasm/ssc_demo_token.raw.wasm", "wasm/ssc_demo_token.noschema.wasm"),
        ("wasm/ssc_demo_increment.raw.wasm", "wasm/ssc_demo_increment.noschema.wasm"),
        ("wasm/ssc_demo_oz_pausable.injected.wasm", "wasm/ssc_demo_oz_pausable.wasm"),
    ] {
        let r = json(&["size", &fx(w), "--baseline", &fx(b)]);
        assert_eq!(r["baseline"]["code_identical"], true, "{w}: {r}");
        assert_eq!(r["baseline"]["identical_after_strip"], true, "{w}: {r}");
        assert_eq!(r["baseline"]["delta_bytes"], r["contractstoragev0_section_bytes"], "{w}: {r}");
    }
}

#[test]
fn lint_flags_2088_collision_and_passes_demos() {
    let r = json(&["lint", "--wasm", &fx("wasm/ssc_demo_token.wasm")]);
    assert!(r["findings"].as_array().unwrap().iter().all(|f| f["severity"] != "error"), "{r}");
    let r = json(&["lint", "--schema", &side("oz-fungible-pausable-v0.8.0.storage.json")]);
    assert!(r["findings"].as_array().unwrap().iter().all(|f| f["severity"] != "error"), "{r}");
    // A contract that combines the OZ fungible keys with the soroban-examples token keys:
    // both declare Balance(Address) as persistent -> one ledger entry, two meanings.
    let (code, out, _) =
        run(&["lint", "--wasm", &fx("wasm/ssc_demo_token.wasm"), "--schema", &side("oz-fungible-pausable-v0.8.0.storage.json")]);
    assert_eq!(code, 2, "{out}");
    let r: Value = serde_json::from_str(&out).unwrap();
    let f = &r["findings"][0];
    assert_eq!(f["code"], "collision");
    assert!(
        f["entries"].to_string().contains("DataKey::Balance") && f["entries"].to_string().contains("FungibleStorageKey::Balance"),
        "{f}"
    );
}

#[test]
fn decode_mainnet_usdc_sac_via_sidecar() {
    let r = json(&[
        "decode",
        "--schema",
        &side("stellar-asset-contract.storage.json"),
        "--entries",
        &fx("mainnet/usdc-sac.instance.json"),
        "--entries",
        &fx("mainnet/usdc-sac.balances.json"),
        "--strict",
    ]);
    assert_eq!(r["instances"][0]["executable"], "stellar_asset");
    let entries = r["entries"].as_array().unwrap();
    assert!(entries.iter().all(|e| e["status"] == "matched"), "{r}");
    let info = entries.iter().find(|e| e["name"] == "InstanceDataKey::AssetInfo").expect("AssetInfo");
    assert_eq!(info["value"]["AlphaNum4"][0]["asset_code"], "USDC");
    let meta = entries.iter().find(|e| e["name"] == "METADATA").expect("METADATA");
    assert_eq!(meta["value"]["decimal"], 7);
    let balances: Vec<&Value> = entries.iter().filter(|e| e["name"] == "DataKey::Balance").collect();
    assert!(!balances.is_empty());
    for b in balances {
        assert!(b["key"]["fields"][0].as_str().unwrap().starts_with('C'));
        assert!(b["value"]["amount"].as_str().unwrap().parse::<i128>().is_ok());
        assert!(b["value"]["authorized"].is_boolean());
    }
}

#[test]
fn decode_mainnet_without_schema_reports_unknown() {
    let (code, out, err) = run(&["decode", "--entries", &fx("mainnet/holder.instance.json"), "--strict"]);
    assert_eq!(code, 2, "{err}");
    let r: Value = serde_json::from_str(&out).unwrap();
    let entries = r["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    for e in entries {
        assert_eq!(e["status"], "unknown");
        assert!(e["reason"].as_str().unwrap().contains("no storage schema"));
        assert!(e["key_shape"].is_string() && e["raw"]["key_xdr"].is_string());
    }
}

#[test]
fn key_builder_and_bad_input() {
    let (c, out, _) = run(&[
        "key",
        "--schema",
        &side("stellar-asset-contract.storage.json"),
        "--contract",
        "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        "--entry",
        "DataKey::Balance",
        "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
    ]);
    assert_eq!(c, 0);
    assert!(out.trim().len() > 40);
    let (c, _, e) = run(&["key", "--schema", &side("stellar-asset-contract.storage.json"), "--contract", "nope", "--entry", "X"]);
    assert_eq!(c, 1);
    assert!(e.contains("error"));
    let (c, _, _) = run(&["inspect", &side("stellar-asset-contract.storage.json")]);
    assert_eq!(c, 1, "a JSON file is not a wasm");
}

/// SSC-015: every scenario in fixtures/scenarios.json decodes exactly to its
/// golden file (scripts/golden.sh). A decoder change that alters any output
/// shows up here; regenerate the goldens only after reviewing the diff.
#[test]
fn golden_outputs_match() {
    let sc: Value = serde_json::from_str(&std::fs::read_to_string(fx("scenarios.json")).unwrap()).unwrap();
    let mut statuses = std::collections::BTreeSet::new();
    for s in sc["scenarios"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let mut args: Vec<String> = vec!["decode".into(), "--format".into(), "json".into()];
        if let Some(w) = s["wasm"].as_str() {
            args.extend(["--wasm".into(), root().join(w).to_string_lossy().into_owned()]);
        }
        for p in s["sidecars"].as_array().unwrap() {
            args.extend(["--schema".into(), root().join(p.as_str().unwrap()).to_string_lossy().into_owned()]);
        }
        for p in s["entries"].as_array().unwrap() {
            args.extend(["--entries".into(), root().join(p.as_str().unwrap()).to_string_lossy().into_owned()]);
        }
        let got = json(&args.iter().map(String::as_str).collect::<Vec<_>>());
        let want: Value = serde_json::from_str(&std::fs::read_to_string(fx(&format!("golden/{id}.decode.json"))).unwrap()).unwrap();
        assert_eq!(got, want, "scenario {id} differs from fixtures/golden/{id}.decode.json");
        for (k, n) in want["summary"]["by_status"].as_object().unwrap() {
            if n.as_u64().unwrap() > 0 {
                statuses.insert(k.clone());
            }
        }
    }
    // The scenarios between them exercise all four statuses on real entries.
    assert_eq!(statuses.into_iter().collect::<Vec<_>>(), ["ambiguous", "matched", "unknown", "value_mismatch"]);
}
