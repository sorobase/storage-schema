// SPDX-License-Identifier: Apache-2.0
//! End to end for a raw-Symbol instance key (`symbol_short!("COUNTER")`).

use sik_storage_schema_core::decode::{Decoder, Status};
use sik_storage_schema_core::{ledger, load_module};
use soroban_sdk::xdr::ScAddress;
use soroban_sdk::{Env, Symbol, Val};

const WASM: &[u8] = include_bytes!("../../../fixtures/wasm/ssc_demo_increment.wasm");

#[test]
fn counter_decodes_from_instance_storage() {
    let env = Env::default();
    let id = env.register(WASM, ());
    for _ in 0..3 {
        env.invoke_contract::<Val>(&id, &Symbol::new(&env, "increment"), soroban_sdk::vec![&env]);
    }
    let m = load_module(WASM).unwrap();
    let mut d = Decoder::new();
    d.add_schema(&m.storage.expect("schema embedded"), "embedded");
    let contract: ScAddress = id.into();
    let raw = ledger::raw_from_snapshot(&env.to_ledger_snapshot().ledger_entries, &contract);
    ssc_capture::maybe_write("increment", &raw);
    let report = ledger::decode_entries(&d, &raw, None);
    assert_eq!(report.entries.len(), 1, "{report:?}");
    let e = &report.entries[0];
    assert_eq!(e.status, Status::Matched);
    assert_eq!(e.name.as_deref(), Some("COUNTER"));
    assert_eq!(e.source.ledger_entry, "instance_storage");
    assert_eq!(e.value, 3);
}
