// SPDX-License-Identifier: Apache-2.0
//! Ledger-entry plumbing: parse `getLedgerEntries` JSON, expand the contract
//! instance's storage map into instance entries, summarise results, and build
//! ledger keys from schema entries for lookups.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Serialize;
use serde_json::Value;
use stellar_xdr::{
    ContractDataDurability, ContractExecutable, Hash, Int128Parts, LedgerEntryData, LedgerKey, LedgerKeyContractCode,
    LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScBytes, ScString, ScSymbol, ScVal, ScVec, UInt128Parts, WriteXdr,
};

use crate::decode::{DecodedEntry, Decoder, EntrySource, Status};
use crate::model::{Durability, KeyEntry, KeyPattern};
use crate::rusttype::type_to_rust;

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("xdr: {0}")]
    Xdr(#[from] stellar_xdr::Error),
    #[error("{0}")]
    Invalid(String),
}

/// One entry as returned by `getLedgerEntries`.
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub data: LedgerEntryData,
    pub last_modified: Option<u32>,
    pub live_until: Option<u32>,
}

/// Accepts a full JSON-RPC response, its `result` object, or a bare array of
/// `{xdr, lastModifiedLedgerSeq, liveUntilLedgerSeq}` objects.
pub fn parse_get_ledger_entries(json: &str) -> Result<(Vec<RawEntry>, Option<u32>), LedgerError> {
    let v: Value = serde_json::from_str(json)?;
    if let Some(e) = v.get("error") {
        return Err(LedgerError::Invalid(format!("RPC error: {e}")));
    }
    let result = v.get("result").unwrap_or(&v);
    let latest = result.get("latestLedger").and_then(Value::as_u64).map(|n| n as u32);
    let arr = if let Some(a) = result.as_array() {
        a
    } else {
        match result.get("entries") {
            Some(Value::Array(a)) => a,
            Some(Value::Null) | None => return Ok((vec![], latest)),
            _ => return Err(LedgerError::Invalid("`entries` is not an array".into())),
        }
    };
    let mut out = Vec::new();
    for e in arr {
        let xdr = e.get("xdr").and_then(Value::as_str).ok_or_else(|| LedgerError::Invalid("entry without `xdr`".into()))?;
        out.push(RawEntry {
            data: LedgerEntryData::from_xdr_base64(xdr, Limits::none())?,
            last_modified: e.get("lastModifiedLedgerSeq").and_then(Value::as_u64).map(|n| n as u32),
            live_until: e.get("liveUntilLedgerSeq").and_then(Value::as_u64).map(|n| n as u32),
        });
    }
    Ok((out, latest))
}

#[derive(Clone, Debug, Serialize)]
pub struct InstanceInfo {
    pub contract: String,
    /// `wasm`, `stellar_asset` or `external_ref`.
    pub executable: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_hash: Option<String>,
    pub storage_entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_until_ledger: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Summary {
    pub total: usize,
    pub by_status: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_ledger: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<InstanceInfo>,
    pub summary: Summary,
    pub entries: Vec<DecodedEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub fn executable_of(e: &ContractExecutable) -> (String, Option<String>) {
    match e {
        ContractExecutable::Wasm(h) => ("wasm".into(), Some(hex::encode(h.0))),
        ContractExecutable::StellarAsset => ("stellar_asset".into(), None),
        ContractExecutable::ExternalRef(_) => ("external_ref".into(), None),
    }
}

/// Decode every contract-data entry; the instance entry's storage map is
/// expanded into `instance` entries (SPEC.md 4.1).
pub fn decode_entries(decoder: &Decoder, entries: &[RawEntry], latest_ledger: Option<u32>) -> Report {
    let mut out = Vec::new();
    let mut instances = Vec::new();
    let mut notes = Vec::new();
    for e in entries {
        let LedgerEntryData::ContractData(cd) = &e.data else {
            notes.push(format!("skipped a {} ledger entry (not contract data)", e.data.name()));
            continue;
        };
        let contract = cd.contract.to_string();
        if let (ScVal::LedgerKeyContractInstance, ScVal::ContractInstance(inst)) = (&cd.key, &cd.val) {
            let (executable, wasm_hash) = executable_of(&inst.executable);
            let n = inst.storage.as_ref().map(|m| m.len()).unwrap_or(0);
            instances.push(InstanceInfo {
                contract: contract.clone(),
                executable,
                wasm_hash,
                storage_entries: n,
                live_until_ledger: e.live_until,
            });
            if let Some(m) = &inst.storage {
                for me in m.iter() {
                    out.push(decoder.decode(
                        Durability::Instance,
                        &me.key,
                        &me.val,
                        EntrySource {
                            contract: Some(contract.clone()),
                            ledger_entry: "instance_storage",
                            last_modified_ledger: e.last_modified,
                            live_until_ledger: e.live_until,
                        },
                    ));
                }
            }
            continue;
        }
        let durability = match cd.durability {
            ContractDataDurability::Persistent => Durability::Persistent,
            ContractDataDurability::Temporary => Durability::Temporary,
        };
        out.push(decoder.decode(
            durability,
            &cd.key,
            &cd.val,
            EntrySource {
                contract: Some(contract),
                ledger_entry: "contract_data",
                last_modified_ledger: e.last_modified,
                live_until_ledger: e.live_until,
            },
        ));
    }
    let mut summary = Summary { total: out.len(), by_status: BTreeMap::new() };
    for s in [Status::Matched, Status::Ambiguous, Status::ValueMismatch, Status::Unknown] {
        let n = out.iter().filter(|d| d.status == s).count();
        summary.by_status.insert(serde_json::to_value(s).unwrap().as_str().unwrap().to_string(), n);
    }
    Report { latest_ledger, instances, summary, entries: out, notes }
}

pub fn instance_ledger_key(contract: &ScAddress) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: contract.clone(),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

pub fn code_ledger_key(hash: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode { hash: Hash(hash) })
}

/// Ledger key for a schema entry and string arguments (one per key field).
/// Instance entries have no ledger key of their own (they live in the
/// instance entry), so the instance key is returned for them.
pub fn build_ledger_key(contract: &ScAddress, entry: &KeyEntry, args: &[String]) -> Result<LedgerKey, LedgerError> {
    if entry.durability == Durability::Instance {
        return Ok(instance_ledger_key(contract));
    }
    let key = build_key_val(entry, args)?;
    Ok(LedgerKey::ContractData(LedgerKeyContractData {
        contract: contract.clone(),
        key,
        durability: if entry.durability == Durability::Persistent {
            ContractDataDurability::Persistent
        } else {
            ContractDataDurability::Temporary
        },
    }))
}

pub fn build_key_val(entry: &KeyEntry, args: &[String]) -> Result<ScVal, LedgerError> {
    match &entry.key {
        KeyPattern::Exact(v) => {
            if !args.is_empty() {
                return Err(LedgerError::Invalid(format!("{} is an exact key and takes no arguments", entry.name)));
            }
            Ok(v.clone())
        }
        KeyPattern::Tagged { tag, fields } => {
            if args.len() != fields.len() {
                return Err(LedgerError::Invalid(format!("{} takes {} argument(s), got {}", entry.name, fields.len(), args.len())));
            }
            let mut items = vec![ScVal::Symbol(ScSymbol(tag.as_str().try_into().map_err(|_| LedgerError::Invalid("tag".into()))?))];
            for (f, a) in fields.iter().zip(args) {
                items.push(scalar_from_str(&f.type_, a)?);
            }
            Ok(ScVal::Vec(Some(ScVec(items.try_into().map_err(|_| LedgerError::Invalid("too many fields".into()))?))))
        }
        KeyPattern::Typed(t) => {
            let [a] = args else { return Err(LedgerError::Invalid(format!("{} takes exactly one argument", entry.name))) };
            scalar_from_str(t, a)
        }
    }
}

/// Parse a scalar argument for key building. UDT-typed fields are not
/// supported from the command line (pass the key as XDR instead).
pub fn scalar_from_str(t: &stellar_xdr::ScSpecTypeDef, s: &str) -> Result<ScVal, LedgerError> {
    use stellar_xdr::ScSpecTypeDef as T;
    let bad = |what: &str| LedgerError::Invalid(format!("cannot parse {s:?} as {what}"));
    Ok(match t {
        T::Address | T::MuxedAddress => ScVal::Address(ScAddress::from_str(s).map_err(|_| bad("Address"))?),
        T::Symbol => ScVal::Symbol(ScSymbol(s.try_into().map_err(|_| bad("Symbol"))?)),
        T::String => ScVal::String(ScString(s.try_into().map_err(|_| bad("String"))?)),
        T::Bool => ScVal::Bool(s.parse().map_err(|_| bad("bool"))?),
        T::U32 => ScVal::U32(s.parse().map_err(|_| bad("u32"))?),
        T::I32 => ScVal::I32(s.parse().map_err(|_| bad("i32"))?),
        T::U64 => ScVal::U64(s.parse().map_err(|_| bad("u64"))?),
        T::I64 => ScVal::I64(s.parse().map_err(|_| bad("i64"))?),
        T::U128 => {
            let n: u128 = s.parse().map_err(|_| bad("u128"))?;
            ScVal::U128(UInt128Parts { hi: (n >> 64) as u64, lo: n as u64 })
        }
        T::I128 => {
            let n: i128 = s.parse().map_err(|_| bad("i128"))?;
            ScVal::I128(Int128Parts { hi: (n >> 64) as i64, lo: n as u64 })
        }
        T::Bytes | T::BytesN(_) => {
            let b = hex::decode(s.trim_start_matches("0x")).map_err(|_| bad("hex bytes"))?;
            if let T::BytesN(n) = t {
                if b.len() as u32 != n.n {
                    return Err(bad(&format!("BytesN<{}>", n.n)));
                }
            }
            ScVal::Bytes(ScBytes(b.try_into().map_err(|_| bad("bytes"))?))
        }
        other => {
            return Err(LedgerError::Invalid(format!("key fields of type {} cannot be built from the command line", type_to_rust(other))))
        }
    })
}

/// Contract-data entries of one contract from a soroban-sdk test ledger
/// snapshot (`env.to_ledger_snapshot().ledger_entries`). Takes plain XDR
/// types, so this crate needs no soroban-sdk dependency.
#[allow(clippy::type_complexity)]
pub fn raw_from_snapshot(
    entries: &[(Box<LedgerKey>, (Box<stellar_xdr::LedgerEntry>, Option<u32>))],
    contract: &ScAddress,
) -> Vec<RawEntry> {
    entries
        .iter()
        .filter_map(|(_, (e, live))| match &e.data {
            LedgerEntryData::ContractData(cd) if &cd.contract == contract => {
                Some(RawEntry { data: e.data.clone(), last_modified: Some(e.last_modified_ledger_seq), live_until: *live })
            }
            _ => None,
        })
        .collect()
}

/// The ledger key RPC reports for an entry (contract data and code only).
pub fn ledger_key_of(data: &LedgerEntryData) -> Option<LedgerKey> {
    match data {
        LedgerEntryData::ContractData(cd) => Some(LedgerKey::ContractData(LedgerKeyContractData {
            contract: cd.contract.clone(),
            key: cd.key.clone(),
            durability: cd.durability,
        })),
        LedgerEntryData::ContractCode(cc) => Some(LedgerKey::ContractCode(LedgerKeyContractCode { hash: cc.hash.clone() })),
        _ => None,
    }
}

/// Write entries in the `getLedgerEntries` response shape, so captures from
/// the soroban test host read exactly like RPC captures
/// ([`parse_get_ledger_entries`] reads them back).
pub fn to_get_ledger_entries_json(entries: &[RawEntry], latest_ledger: Option<u32>) -> String {
    let rows: Vec<Value> = entries
        .iter()
        .map(|e| {
            let mut o = serde_json::Map::new();
            if let Some(k) = ledger_key_of(&e.data) {
                o.insert("key".into(), Value::String(ledger_key_b64(&k)));
            }
            o.insert("xdr".into(), Value::String(e.data.to_xdr_base64(Limits::none()).expect("entry encodes")));
            if let Some(n) = e.last_modified {
                o.insert("lastModifiedLedgerSeq".into(), n.into());
            }
            if let Some(n) = e.live_until {
                o.insert("liveUntilLedgerSeq".into(), n.into());
            }
            Value::Object(o)
        })
        .collect();
    let mut result = serde_json::Map::new();
    result.insert("entries".into(), Value::Array(rows));
    if let Some(l) = latest_ledger {
        result.insert("latestLedger".into(), l.into());
    }
    serde_json::to_string_pretty(&serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": result})).expect("serialises")
}

pub fn ledger_key_b64(k: &LedgerKey) -> String {
    k.to_xdr_base64(Limits::none()).expect("ledger key encodes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar;

    #[test]
    fn build_balance_key() {
        let s = sidecar::parse(
            r#"{"storage_schema":0,"keys":[{"name":"B","durability":"persistent","key":{"tagged":{"tag":"Balance","fields":["Address"]}},"value":"i128"}]}"#,
        )
        .unwrap();
        let c = ScAddress::from_str("CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75").unwrap();
        let k = build_ledger_key(&c, &s.keys[0], &["GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()]).unwrap();
        let LedgerKey::ContractData(cd) = &k else { panic!() };
        assert_eq!(crate::decode::shape(&cd.key), "[Symbol(\"Balance\"), Address]");
        assert!(build_ledger_key(&c, &s.keys[0], &[]).is_err());
        assert!(build_ledger_key(&c, &s.keys[0], &["nope".into()]).is_err());
    }

    #[test]
    fn rpc_json_roundtrips_contract_data() {
        let (raw, _) = parse_get_ledger_entries(include_str!("../../../fixtures/mainnet/usdc-sac.balances.json")).unwrap();
        let text = to_get_ledger_entries_json(&raw, Some(77));
        let v: Value = serde_json::from_str(&text).unwrap();
        // the key is the ledger key derived from the entry, as RPC returns it
        let original: Value = serde_json::from_str(include_str!("../../../fixtures/mainnet/usdc-sac.balances.json")).unwrap();
        assert_eq!(v["result"]["entries"][0]["key"], original["result"]["entries"][0]["key"]);
        let (back, latest) = parse_get_ledger_entries(&text).unwrap();
        assert_eq!(latest, Some(77));
        assert_eq!(back.len(), raw.len());
        for (a, b) in back.iter().zip(&raw) {
            assert_eq!(a.data, b.data);
            assert_eq!(a.last_modified, b.last_modified);
            assert_eq!(a.live_until, b.live_until);
        }
    }

    #[test]
    fn parses_rpc_shapes() {
        let (e, l) = parse_get_ledger_entries(r#"{"jsonrpc":"2.0","id":1,"result":{"entries":null,"latestLedger":5}}"#).unwrap();
        assert!(e.is_empty());
        assert_eq!(l, Some(5));
        assert!(parse_get_ledger_entries(r#"{"error":{"code":-1}}"#).is_err());
    }
}
