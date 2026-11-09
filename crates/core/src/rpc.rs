// SPDX-License-Identifier: Apache-2.0
//! Read-only Soroban JSON-RPC client: `getLedgerEntries` and `getLatestLedger`
//! only. No signing, no submission. Calls are spaced by `min_interval` to
//! stay under the programme's 2 req/s budget for public RPC.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use stellar_xdr::{ContractExecutable, LedgerEntryData, LedgerKey, ScAddress, ScVal};

use crate::ledger::{code_ledger_key, instance_ledger_key, ledger_key_b64, parse_get_ledger_entries, RawEntry};

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("http: {0}")]
    Http(String),
    #[error("{0}")]
    Ledger(#[from] crate::ledger::LedgerError),
    #[error("{0}")]
    NotFound(String),
}

pub struct Client {
    pub url: String,
    pub min_interval: Duration,
    last: Option<Instant>,
}

/// What a contract runs.
pub enum Executable {
    Wasm { hash: [u8; 32], code: Vec<u8> },
    StellarAsset,
    ExternalRef,
}

impl Client {
    pub fn new(url: &str) -> Self {
        Client { url: url.to_string(), min_interval: Duration::from_millis(600), last: None }
    }

    fn pace(&mut self) {
        if let Some(t) = self.last {
            let el = t.elapsed();
            if el < self.min_interval {
                std::thread::sleep(self.min_interval - el);
            }
        }
        self.last = Some(Instant::now());
    }

    /// Raw JSON-RPC call; returns the response body text.
    pub fn call(&mut self, method: &str, params: Value) -> Result<String, RpcError> {
        self.pace();
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let resp = ureq::post(&self.url)
            .set("content-type", "application/json")
            .set("user-agent", "sik-storage-schema/0.1 (read-only)")
            .timeout(Duration::from_secs(30))
            .send_json(body)
            .map_err(|e| RpcError::Http(e.to_string()))?;
        resp.into_string().map_err(|e| RpcError::Http(e.to_string()))
    }

    /// `getLedgerEntries` for up to 200 keys; returns the raw response text
    /// (for fixtures) and the parsed entries.
    pub fn get_ledger_entries(&mut self, keys: &[LedgerKey]) -> Result<(String, Vec<RawEntry>, Option<u32>), RpcError> {
        let ks: Vec<String> = keys.iter().map(ledger_key_b64).collect();
        let text = self.call("getLedgerEntries", json!({ "keys": ks }))?;
        let (entries, latest) = parse_get_ledger_entries(&text)?;
        Ok((text, entries, latest))
    }

    pub fn fetch_instance(&mut self, contract: &ScAddress) -> Result<(String, RawEntry), RpcError> {
        let (text, mut entries, _) = self.get_ledger_entries(&[instance_ledger_key(contract)])?;
        let e = entries.pop().ok_or_else(|| RpcError::NotFound(format!("no instance entry for {contract}")))?;
        Ok((text, e))
    }

    /// Fetch what a contract executes: its Wasm bytes, or the fact that it is
    /// a built-in Stellar Asset Contract (which has no Wasm and so no
    /// embedded schema; use a sidecar).
    pub fn fetch_executable(&mut self, contract: &ScAddress) -> Result<Executable, RpcError> {
        let (_, inst) = self.fetch_instance(contract)?;
        let LedgerEntryData::ContractData(cd) = &inst.data else { return Err(RpcError::NotFound("instance is not contract data".into())) };
        let ScVal::ContractInstance(ci) = &cd.val else { return Err(RpcError::NotFound("instance value is not ContractInstance".into())) };
        match &ci.executable {
            ContractExecutable::StellarAsset => Ok(Executable::StellarAsset),
            ContractExecutable::ExternalRef(_) => Ok(Executable::ExternalRef),
            ContractExecutable::Wasm(h) => {
                let (_, mut entries, _) = self.get_ledger_entries(&[code_ledger_key(h.0)])?;
                let e = entries.pop().ok_or_else(|| RpcError::NotFound(format!("no code entry for wasm {}", hex::encode(h.0))))?;
                let LedgerEntryData::ContractCode(code) = e.data else { return Err(RpcError::NotFound("not a code entry".into())) };
                Ok(Executable::Wasm { hash: h.0, code: code.code.to_vec() })
            }
        }
    }
}
