// SPDX-License-Identifier: Apache-2.0
//! SSC-013 end to end: a contract built on OpenZeppelin `stellar-tokens` /
//! `stellar-contract-utils` 0.8.0, which do not emit a storage schema. The
//! keys those crates write are declared with `storage_schema_extern!` in a
//! separate library crate (`ssc-demo-oz-keys`), not in the contract crate and
//! not by a post-build inject. The wasm comes from `stellar contract build`
//! (scripts/build-demos.sh). Every entry must decode `matched` from the
//! schema embedded in that wasm alone.

use sik_storage_schema_core::decode::{Decoder, Status};
use sik_storage_schema_core::{ledger, load_module};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ScAddress;
use soroban_sdk::{vec, Address, Env, IntoVal, Symbol, Val};

const WASM: &[u8] = include_bytes!("../../../fixtures/wasm/ssc_demo_oz_extern.wasm");

#[test]
fn library_declared_extern_keys_reach_the_wasm() {
    let m = load_module(WASM).unwrap();
    let s = m.storage.expect("wasm carries contractstoragev0");
    let names: Vec<&str> = s.keys.iter().map(|k| k.name.as_str()).collect();
    for want in
        ["OWNER", "FungibleStorageKey::Meta", "FungibleStorageKey::Balance", "FungibleStorageKey::Allowance", "PausableStorageKey::Paused"]
    {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }
    let bal = s.keys.iter().find(|k| k.name == "FungibleStorageKey::Balance").unwrap();
    assert_eq!(bal.lib, "stellar-tokens@0.8.0");
}

#[test]
fn every_entry_written_through_oz_decodes() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let bob = Address::generate(&env);
    let id = env.register(WASM, (owner.clone(), 9000i128));
    let call = |f: &str, args: soroban_sdk::Vec<Val>| env.invoke_contract::<Val>(&id, &Symbol::new(&env, f), args);
    call("transfer", vec![&env, owner.into_val(&env), bob.into_val(&env), 400i128.into_val(&env)]);
    call("approve", vec![&env, owner.into_val(&env), bob.into_val(&env), 25i128.into_val(&env), 300u32.into_val(&env)]);
    call("pause", vec![&env, owner.into_val(&env)]);

    let m = load_module(WASM).unwrap();
    let mut d = Decoder::new();
    d.add_schema(&m.storage.expect("schema"), "embedded");
    d.add_spec(&m.spec);
    let c: ScAddress = id.into();
    let raw = ledger::raw_from_snapshot(&env.to_ledger_snapshot().ledger_entries, &c);
    let report = ledger::decode_entries(&d, &raw, None);
    let pretty = serde_json::to_string_pretty(&report).unwrap();
    assert!(report.entries.iter().all(|e| e.status == Status::Matched), "{pretty}");
    assert!(report.entries.iter().all(|e| e.warnings.is_empty()), "{pretty}");
    let names: Vec<std::string::String> = report.entries.iter().filter_map(|e| e.name.clone()).collect();
    for want in
        ["OWNER", "FungibleStorageKey::Meta", "FungibleStorageKey::Balance", "FungibleStorageKey::Allowance", "PausableStorageKey::Paused"]
    {
        assert!(names.iter().any(|n| n == want), "missing {want}: {names:?}");
    }
    let meta = report.entries.iter().find(|e| e.name.as_deref() == Some("FungibleStorageKey::Meta")).unwrap();
    assert_eq!(meta.value["symbol"], "XDM");
    let allowance = report.entries.iter().find(|e| e.name.as_deref() == Some("FungibleStorageKey::Allowance")).unwrap();
    assert_eq!(allowance.value["amount"], "25");
    let paused = report.entries.iter().find(|e| e.name.as_deref() == Some("PausableStorageKey::Paused")).unwrap();
    assert_eq!(paused.value, true);
    ssc_capture::maybe_write("oz-extern", &raw);
}
