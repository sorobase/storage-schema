// SPDX-License-Identifier: Apache-2.0
//! SSC-026: the engine entry point (`handle`, the function the C ABI wraps)
//! returns exactly the golden CLI output for every scenario.

use std::path::PathBuf;

use serde_json::{json, Value};
use sik_storage_schema_web::handle;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn text(p: &str) -> String {
    std::fs::read_to_string(root().join(p)).unwrap()
}

fn label(p: &str) -> String {
    p.rsplit('/').next().unwrap().to_string()
}

#[test]
fn engine_matches_golden_for_every_scenario() {
    let sc: Value = serde_json::from_str(&text("fixtures/scenarios.json")).unwrap();
    for s in sc["scenarios"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let wasm = s["wasm"].as_str().map(|p| std::fs::read(root().join(p)).unwrap()).unwrap_or_default();
        let sidecars: Vec<Value> = s["sidecars"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| json!({"label": label(p.as_str().unwrap()), "text": text(p.as_str().unwrap())}))
            .collect();
        let entries: Vec<Value> =
            s["entries"].as_array().unwrap().iter().map(|p| json!({"label": p, "text": text(p.as_str().unwrap())})).collect();
        let req = json!({"op": "decode", "sidecars": sidecars, "entries": entries, "has_wasm": s["wasm"].is_string()});
        let out = handle(&req, &wasm);
        assert_eq!(out["ok"], true, "{id}: {out}");
        let want: Value = serde_json::from_str(&text(&format!("fixtures/golden/{id}.decode.json"))).unwrap();
        assert_eq!(out["report"], want, "{id}");
    }
}

#[test]
fn inspect_and_lint_and_keys() {
    let token = std::fs::read(root().join("fixtures/wasm/ssc_demo_token.wasm")).unwrap();
    let out = handle(&json!({"op": "inspect"}), &token);
    assert_eq!(out["inspect"]["contractstoragev0"]["keys"].as_array().unwrap().len(), 4);
    let oz = text("sidecars/oz-fungible-pausable-v0.8.0.storage.json");
    let out = handle(&json!({"op": "lint", "has_wasm": true, "sidecars": [{"label": "oz.json", "text": oz}]}), &token);
    assert!(out["lint"]["findings"].as_array().unwrap().iter().any(|f| f["code"] == "collision"));
    let usdc = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
    let out = handle(&json!({"op": "instance_key", "contract": usdc}), &[]);
    assert!(text("fixtures/mainnet/usdc-sac.instance.json").contains(out["key"].as_str().unwrap()));
    let sac = text("sidecars/stellar-asset-contract.storage.json");
    let holder = "CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE";
    let out = handle(
        &json!({"op": "entry_key", "contract": usdc, "entry": "DataKey::Balance", "args": [holder],
                "sidecars": [{"label": "sac.json", "text": sac}]}),
        &[],
    );
    assert!(text("fixtures/mainnet/usdc-sac.balances.json").contains(out["key"].as_str().unwrap()), "{out}");
    let out = handle(&json!({"op": "executable", "response": text("fixtures/mainnet/holder.instance.json")}), &[]);
    assert_eq!(out["wasm_hash"], "d691135aade93ff0f7c229e009cde042130a05124cf7202b03d11246b4f9b473");
}

#[test]
fn errors_are_reported_not_panicked() {
    assert_eq!(handle(&json!({"op": "nope"}), &[])["ok"], false);
    assert_eq!(handle(&json!({"op": "inspect"}), b"not wasm")["ok"], false);
    let out = handle(&json!({"op": "decode", "entries": [{"label": "x", "text": "{"}]}), &[]);
    assert_eq!(out["ok"], false);
    assert!(out["error"].as_str().unwrap().starts_with("x: "));
    let out = handle(&json!({"op": "decode", "sidecars": [{"label": "bad.json", "text": "{\"storage_schema\": 1}"}], "entries": []}), &[]);
    assert_eq!(out["ok"], false);
}
