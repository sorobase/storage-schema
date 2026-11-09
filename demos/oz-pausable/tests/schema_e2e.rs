// SPDX-License-Identifier: Apache-2.0
//! End to end for a contract that never adopts the macro: the UNMODIFIED
//! OpenZeppelin v0.8.0 fungible-pausable wasm, with the sidecar schema
//! injected post-build (`storage-schema inject`). Keys come from three crates
//! (the example itself, stellar-tokens, stellar-contract-utils).

use sik_storage_schema_core::decode::{Decoder, Status};
use sik_storage_schema_core::{ledger, load_module};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ScAddress;
use soroban_sdk::{vec, Address, Env, IntoVal, String, Symbol, Val};

const PLAIN: &[u8] = include_bytes!("../../../fixtures/wasm/ssc_demo_oz_pausable.wasm");
const INJECTED: &[u8] = include_bytes!("../../../fixtures/wasm/ssc_demo_oz_pausable.injected.wasm");

fn run(wasm: &[u8]) -> (Env, ScAddress) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let bob = Address::generate(&env);
    let id = env.register(wasm, (String::from_str(&env, "OZ Demo"), String::from_str(&env, "OZD"), owner.clone(), 5000i128));
    let call = |f: &str, args: soroban_sdk::Vec<Val>| env.invoke_contract::<Val>(&id, &Symbol::new(&env, f), args);
    call("transfer", vec![&env, owner.into_val(&env), bob.into_val(&env), 1200i128.into_val(&env)]);
    call("approve", vec![&env, owner.into_val(&env), bob.into_val(&env), 7i128.into_val(&env), 500u32.into_val(&env)]);
    call("pause", vec![&env, owner.into_val(&env)]);
    let c: ScAddress = id.into();
    (env, c)
}

#[test]
fn injected_sidecar_decodes_every_entry() {
    let (env, c) = run(INJECTED);
    let m = load_module(INJECTED).unwrap();
    let mut d = Decoder::new();
    d.add_schema(&m.storage.expect("injected schema"), "embedded");
    d.add_spec(&m.spec);
    let raw = ledger::raw_from_snapshot(&env.to_ledger_snapshot().ledger_entries, &c);
    // The injected and the plain wasm write the same entries (same code).
    ssc_capture::maybe_write("oz-pausable", &raw);
    let report = ledger::decode_entries(&d, &raw, None);
    assert!(report.entries.iter().all(|e| e.status == Status::Matched), "{}", serde_json::to_string_pretty(&report).unwrap());
    let names: Vec<std::string::String> = report.entries.iter().filter_map(|e| e.name.clone()).collect();
    for want in [
        "OWNER",
        "FungibleStorageKey::Meta",
        "FungibleStorageKey::Balance",
        "FungibleStorageKey::Allowance",
        "TotalSupplyStorageKey::TotalSupply",
        "PausableStorageKey::Paused",
    ] {
        assert!(names.iter().any(|n| n == want), "missing {want}: {names:?}");
    }
    let supply = report.entries.iter().find(|e| e.name.as_deref() == Some("TotalSupplyStorageKey::TotalSupply")).unwrap();
    assert_eq!(supply.value, "5000");
    assert_eq!(supply.durability, sik_storage_schema_core::Durability::Persistent);
}

#[test]
fn without_a_schema_everything_is_unknown_not_guessed() {
    let (env, c) = run(PLAIN);
    let m = load_module(PLAIN).unwrap();
    assert!(m.storage.is_none());
    let mut d = Decoder::new();
    d.add_spec(&m.spec);
    let report = ledger::decode_entries(&d, &ledger::raw_from_snapshot(&env.to_ledger_snapshot().ledger_entries, &c), None);
    assert!(!report.entries.is_empty());
    assert!(report.entries.iter().all(|e| e.status == Status::Unknown));
}
