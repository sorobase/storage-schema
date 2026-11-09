// SPDX-License-Identifier: Apache-2.0
//! Browser engine for storage-schema (SSC-026).
//!
//! `sik-storage-schema-core` (feature `rpc` off) compiled to
//! `wasm32-unknown-unknown` behind a small C ABI, so the docs-site playground
//! runs the same decoder as the CLI, in the page, with no server and no
//! wasm-bindgen. One entry point, [`handle`], takes a JSON request plus an
//! optional byte payload (a contract wasm) and returns JSON:
//!
//! | `op` | request fields | payload | response (besides `ok`) |
//! |---|---|---|---|
//! | `version` | | | `tool`, `version`, `section` |
//! | `inspect` | `code`? | wasm | `inspect` (as `storage-schema inspect --format json`) |
//! | `decode` | `has_wasm`?, `code`?, `sidecars`?, `entries`, `latest_ledger`? | wasm | `report` (as `storage-schema decode`) |
//! | `lint` | `has_wasm`?, `code`?, `sidecars`? | wasm | `lint` (as `storage-schema lint`) |
//! | `instance_key` | `contract` | | `key` (base64 `LedgerKey` XDR) |
//! | `code_key` | `hash` | | `key` |
//! | `entry_key` | `contract`, `entry`, `args`, `has_wasm`?, `code`?, `sidecars`? | wasm | `key` |
//! | `executable` | `response` | | `contract`, `executable`, `wasm_hash`?, `latest_ledger`? |
//!
//! `sidecars` and `entries` are `[{label, text}]`; `entries` texts are
//! `getLedgerEntries` responses. `code` is `{response, hash}`: a
//! `getLedgerEntries` response holding a ContractCode entry, whose SHA-256 is
//! checked against `hash` before the wasm is used (live mode). With
//! `has_wasm: true` the payload is the wasm instead.
//!
//! Errors are `{"ok": false, "error": "..."}`. The engine performs no I/O:
//! the page fetches ledger entries from a public RPC (read-only) and passes
//! the response text in.
//!
//! ABI (lengths in bytes): `ssc_alloc(len) -> ptr`, `ssc_free(ptr, len)`,
//! `ssc_call(req_ptr, req_len, data_ptr, data_len) -> out_len`,
//! `ssc_out_ptr() -> ptr`. The output buffer stays valid until the next call.

use std::cell::RefCell;

use serde_json::{json, Value};
use sik_storage_schema_core::api::{self, ApiError, Named};
use sik_storage_schema_core::SECTION_NAME;

fn fail(msg: impl std::fmt::Display) -> Value {
    json!({"ok": false, "error": msg.to_string()})
}

fn str_field<'a>(req: &'a Value, k: &str) -> Result<&'a str, ApiError> {
    req.get(k).and_then(Value::as_str).ok_or_else(|| ApiError(format!("request field `{k}` (string) is required")))
}

/// `[{label, text}]` from the request.
fn pairs(req: &Value, k: &str) -> Result<Vec<(String, String)>, ApiError> {
    let Some(v) = req.get(k) else { return Ok(vec![]) };
    let arr = v.as_array().ok_or_else(|| ApiError(format!("`{k}` must be an array of {{label, text}}")))?;
    arr.iter()
        .map(|o| {
            let label = o.get("label").and_then(Value::as_str).unwrap_or("input").to_string();
            let text = o.get("text").and_then(Value::as_str).ok_or_else(|| ApiError(format!("`{k}[].text` is required")))?;
            Ok((label, text.to_string()))
        })
        .collect()
}

fn named(v: &[(String, String)]) -> Vec<Named<'_>> {
    v.iter().map(|(label, text)| Named { label, text }).collect()
}

/// The wasm for this request: verified live code, the payload, or none.
fn wasm_of(req: &Value, data: &[u8]) -> Result<Option<Vec<u8>>, ApiError> {
    if let Some(c) = req.get("code").filter(|c| !c.is_null()) {
        return Ok(Some(api::code(str_field(c, "response")?, str_field(c, "hash")?)?));
    }
    let has = req.get("has_wasm").and_then(Value::as_bool).unwrap_or(!data.is_empty());
    Ok(if has { Some(data.to_vec()) } else { None })
}

fn run(req: &Value, data: &[u8]) -> Result<Value, ApiError> {
    let op = str_field(req, "op")?;
    match op {
        "version" => Ok(json!({"tool": "sik-storage-schema-web", "version": env!("CARGO_PKG_VERSION"), "section": SECTION_NAME})),
        "inspect" => {
            let w = wasm_of(req, data)?.ok_or_else(|| ApiError("inspect needs a wasm".into()))?;
            Ok(json!({"inspect": api::inspect(&w)?}))
        }
        "decode" => {
            let w = wasm_of(req, data)?;
            let sc = pairs(req, "sidecars")?;
            let en = pairs(req, "entries")?;
            let mut notes = vec![];
            let d = api::build_decoder(w.as_deref(), &named(&sc), &mut notes)?;
            let latest = req.get("latest_ledger").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
            let report = api::decode(&d, &named(&en), latest, notes)?;
            Ok(json!({"report": report}))
        }
        "lint" => {
            let w = wasm_of(req, data)?;
            let sc = pairs(req, "sidecars")?;
            Ok(json!({"lint": api::lint(w.as_deref(), &named(&sc))?}))
        }
        "instance_key" => Ok(json!({"key": api::instance_key(str_field(req, "contract")?)?})),
        "code_key" => Ok(json!({"key": api::code_key(str_field(req, "hash")?)?})),
        "entry_key" => {
            let w = wasm_of(req, data)?;
            let sc = pairs(req, "sidecars")?;
            let d = api::build_decoder(w.as_deref(), &named(&sc), &mut vec![])?;
            let args: Vec<String> = req
                .get("args")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            Ok(json!({"key": api::entry_key(&d, str_field(req, "contract")?, str_field(req, "entry")?, &args)?}))
        }
        "executable" => api::executable(str_field(req, "response")?),
        other => Err(ApiError(format!("unknown op `{other}`"))),
    }
}

/// Handle one request. Never panics on bad input: errors come back as
/// `{"ok": false, "error": ..}`.
pub fn handle(req: &Value, data: &[u8]) -> Value {
    match run(req, data) {
        Ok(mut v) => {
            v["ok"] = json!(true);
            v
        }
        Err(e) => fail(e),
    }
}

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Allocate `len` bytes for the caller to fill. Free with [`ssc_free`].
#[no_mangle]
pub extern "C" fn ssc_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` and `len` must come from one [`ssc_alloc`] call, freed once.
#[no_mangle]
pub unsafe extern "C" fn ssc_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// # Safety
/// The pointers must address `req_len` / `data_len` initialised bytes
/// (from [`ssc_alloc`]).
#[no_mangle]
pub unsafe extern "C" fn ssc_call(req_ptr: *const u8, req_len: usize, data_ptr: *const u8, data_len: usize) -> usize {
    let req = std::slice::from_raw_parts(req_ptr, req_len);
    let data = if data_len == 0 { &[][..] } else { std::slice::from_raw_parts(data_ptr, data_len) };
    let out = match serde_json::from_slice::<Value>(req) {
        Ok(r) => handle(&r, data),
        Err(e) => fail(format!("request is not JSON: {e}")),
    };
    let bytes = serde_json::to_vec(&out).unwrap_or_else(|_| br#"{"ok":false,"error":"response did not serialise"}"#.to_vec());
    OUT.with(|o| {
        *o.borrow_mut() = bytes;
        o.borrow().len()
    })
}

#[no_mangle]
pub extern "C" fn ssc_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}
