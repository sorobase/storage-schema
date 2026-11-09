// SPDX-License-Identifier: Apache-2.0
//! End to end: the macro-annotated token wasm (built by scripts/build-demos.sh)
//! runs in the soroban test host; every contract-data entry it writes must
//! decode as `matched` using only the schema embedded in that wasm.

use sik_storage_schema_core::decode::{Decoder, Status};
use sik_storage_schema_core::{ledger, load_module};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::xdr::ScAddress;
use soroban_sdk::{vec, Address, Env, IntoVal, String, Symbol, Val};

const WASM: &[u8] = include_bytes!("../../../fixtures/wasm/ssc_demo_token.wasm");

#[test]
fn every_entry_written_by_the_token_decodes() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let id = env.register(WASM, (admin.clone(), 7u32, String::from_str(&env, "Demo"), String::from_str(&env, "DEMO")));
    let call = |f: &str, args: soroban_sdk::Vec<Val>| env.invoke_contract::<Val>(&id, &Symbol::new(&env, f), args);
    call("mint", vec![&env, alice.into_val(&env), 1000i128.into_val(&env)]);
    call("transfer", vec![&env, alice.into_val(&env), bob.into_val(&env), 250i128.into_val(&env)]);
    call("approve", vec![&env, alice.into_val(&env), bob.into_val(&env), 40i128.into_val(&env), 200u32.into_val(&env)]);

    let m = load_module(WASM).unwrap();
    let schema = m.storage.expect("wasm carries contractstoragev0");
    let mut d = Decoder::new();
    d.add_schema(&schema, "embedded");
    d.add_spec(&m.spec);

    let contract: ScAddress = id.clone().into();
    let raw = ledger::raw_from_snapshot(&env.to_ledger_snapshot().ledger_entries, &contract);
    let report = ledger::decode_entries(&d, &raw, None);
    ssc_capture::maybe_write("token", &raw);
    let names: Vec<_> = report.entries.iter().map(|e| (e.name.clone().unwrap_or_default(), e.status)).collect();
    assert!(report.entries.iter().all(|e| e.status == Status::Matched), "{}", serde_json::to_string_pretty(&report).unwrap());
    for want in ["DataKey::Admin", "METADATA", "DataKey::Balance", "DataKey::Allowance"] {
        assert!(names.iter().any(|(n, _)| n == want), "missing {want}: {names:?}");
    }
    let bal = report.entries.iter().find(|e| e.name.as_deref() == Some("DataKey::Balance") && e.value == "750").unwrap();
    let alice_sc: ScAddress = (&alice).into();
    assert_eq!(bal.key["fields"][0], alice_sc.to_string());
    let allowance = report.entries.iter().find(|e| e.name.as_deref() == Some("DataKey::Allowance")).unwrap();
    assert_eq!(allowance.value["amount"], "40");
    assert_eq!(allowance.value["live_until_ledger"], 200);
    let meta = report.entries.iter().find(|e| e.name.as_deref() == Some("METADATA")).unwrap();
    assert_eq!(meta.value["symbol"], "DEMO");
    assert_eq!(meta.lib.as_deref(), Some("soroban-token-sdk@28.0.0"));
}
