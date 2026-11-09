// SPDX-License-Identifier: Apache-2.0
//! SSC-024: when `SSC_CAPTURE_DIR` is set (scripts/capture-testhost.sh sets
//! it), write the contract-data entries a demo test produced to
//! `$SSC_CAPTURE_DIR/<name>.entries.json` in the `getLedgerEntries` response
//! shape. Without the variable this does nothing, so `cargo test` stays
//! read-only on the source tree.

use sik_storage_schema_core::ledger::{to_get_ledger_entries_json, RawEntry};

pub fn maybe_write(name: &str, raw: &[RawEntry]) {
    let Some(dir) = std::env::var_os("SSC_CAPTURE_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("capture dir");
    let path = dir.join(format!("{name}.entries.json"));
    std::fs::write(&path, to_get_ledger_entries_json(raw, None) + "\n").expect("write capture");
}
