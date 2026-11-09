// SPDX-License-Identifier: Apache-2.0
//! `contractstoragev0`: an informational, off-chain storage schema for Soroban
//! contracts, carried in a Wasm custom section separate from `contractspecv0`.
//!
//! - [`model`]: data model and XDR codec (normative wire format).
//! - [`sidecar`]: the equivalent JSON file for contracts that never adopt the macro.
//! - [`wasm`]: read / inject / strip custom sections.
//! - [`api`]: in-memory entry points shared by the CLI and the browser engine.
//! - [`decode`]: map ledger keys to schema entries and render typed JSON.
//! - [`ledger`]: `getLedgerEntries` parsing, instance-map expansion, key building.
//! - [`lint`]: static collision checks (sdk#2088).
//! - [`rpc`] (feature `rpc`): read-only JSON-RPC client.

pub mod api;
pub mod decode;
pub mod ledger;
pub mod lint;
pub mod model;
pub mod rusttype;
pub mod sidecar;
pub mod wasm;

#[cfg(feature = "rpc")]
pub mod rpc;

pub use model::{Durability, KeyEntry, KeyField, KeyPattern, Schema, SchemaEntry, SECTION_NAME};

use stellar_xdr::{Limited, Limits, ReadXdr, ScSpecEntry};

/// Name of the callable-interface spec section written by soroban-sdk.
pub const SPEC_SECTION_NAME: &str = "contractspecv0";

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{0}")]
    Wasm(#[from] wasm::WasmError),
    #[error("contractstoragev0: {0}")]
    Schema(#[from] model::Error),
    #[error("contractspecv0: {0}")]
    Spec(stellar_xdr::Error),
}

/// What a module carries.
pub struct ModuleSchemas {
    /// `None` when the module has no `contractstoragev0` section.
    pub storage: Option<Schema>,
    pub storage_section_count: usize,
    pub storage_bytes: usize,
    pub spec: Vec<ScSpecEntry>,
}

pub fn read_spec(bytes: &[u8]) -> Result<Vec<ScSpecEntry>, stellar_xdr::Error> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut r = Limited::new(&mut cursor, Limits { depth: 500, len: bytes.len().max(64) });
    let mut out = Vec::new();
    while (r.inner.position() as usize) < bytes.len() {
        out.push(ScSpecEntry::read_xdr(&mut r)?);
    }
    Ok(out)
}

pub fn load_module(wasm_bytes: &[u8]) -> Result<ModuleSchemas, LoadError> {
    let storage_raw = wasm::read_custom(wasm_bytes, SECTION_NAME)?;
    let storage_section_count = wasm::count_custom(wasm_bytes, SECTION_NAME)?;
    let storage_bytes = storage_raw.as_ref().map(|b| b.len()).unwrap_or(0);
    let storage = storage_raw.map(|b| Schema::from_section_bytes(&b)).transpose()?;
    let spec = match wasm::read_custom(wasm_bytes, SPEC_SECTION_NAME)? {
        Some(b) => read_spec(&b).map_err(LoadError::Spec)?,
        None => vec![],
    };
    Ok(ModuleSchemas { storage, storage_section_count, storage_bytes, spec })
}

/// Inject a schema into a module. With `replace`, existing
/// `contractstoragev0` sections are removed first; otherwise the new section
/// is appended and readers concatenate (and de-duplicate) both.
pub fn inject(wasm_bytes: &[u8], schema: &Schema, replace: bool) -> Result<Vec<u8>, LoadError> {
    let base = if replace { wasm::strip_custom(wasm_bytes, SECTION_NAME)? } else { wasm_bytes.to_vec() };
    Ok(wasm::append_custom(&base, SECTION_NAME, &schema.to_section_bytes()?)?)
}
