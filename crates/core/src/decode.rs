// SPDX-License-Identifier: Apache-2.0
//! Decoder: maps a `(durability, key ScVal, value ScVal)` triple to a schema
//! entry and renders both sides as typed JSON (SPEC.md sections 4 and 6).
//!
//! The decoder never guesses. Every result carries a `status`:
//! `matched`, `ambiguous` (several entries claim the key; #2088 collisions
//! end up here), `value_mismatch` (a key matched but the value does not
//! conform: schema drift, an upgrade, a collision, or a wrong schema), or
//! `unknown` (no entry claims the key).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Map as JsonMap, Value};
use stellar_xdr::{Limits, ScSpecEntry, ScSpecTypeDef, ScSpecUdtUnionCaseV0, ScVal, WriteXdr};

use crate::model::{Durability, KeyEntry, KeyPattern, Schema};
use crate::rusttype::{type_to_rust, udt_name};

const MAX_DEPTH: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Matched,
    Ambiguous,
    ValueMismatch,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    pub path: String,
    pub expected: String,
    pub found: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "at {}: expected {}, found {}", self.path, self.expected, self.found)
    }
}

/// Where a decoded entry came from on the ledger.
#[derive(Clone, Debug, Default, Serialize)]
pub struct EntrySource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    /// `contract_data` (its own ledger entry) or `instance_storage` (an
    /// element of the contract instance's storage map).
    pub ledger_entry: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_ledger: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_until_ledger: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lib: String,
    pub schema_source: String,
    pub key: Value,
    pub value_conforms: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatch: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Raw {
    pub key_xdr: String,
    pub val_xdr: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DecodedEntry {
    pub durability: Durability,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lib: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_source: Option<String>,
    /// Typed key when matched, otherwise stellar-xdr JSON of the raw key.
    pub key: Value,
    /// Typed value when matched, otherwise stellar-xdr JSON of the raw value.
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
    /// Structural summary of the raw key, e.g. `[Symbol("Balance"), Address]`.
    pub key_shape: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub source: EntrySource,
    pub raw: Raw,
}

/// UDT definitions, by name. Storage-schema types win over contract-spec
/// types (the spec may be shaken; SPEC.md 6.2).
#[derive(Clone, Debug, Default)]
pub struct TypeRegistry {
    types: BTreeMap<String, (ScSpecEntry, String)>,
    pub conflicts: Vec<String>,
}

impl TypeRegistry {
    pub fn add(&mut self, e: &ScSpecEntry, origin: &str) {
        let Some(name) = udt_name(e) else { return };
        match self.types.get(&name) {
            None => {
                self.types.insert(name, (e.clone(), origin.to_string()));
            }
            Some((existing, first)) if !same_shape(existing, e) => {
                self.conflicts.push(format!("type {name} defined differently in {first} and {origin}; using {first}"));
            }
            Some(_) => {}
        }
    }
    pub fn get(&self, name: &str) -> Option<&ScSpecEntry> {
        self.types.get(name).map(|(e, _)| e)
    }
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
}

/// Equal ignoring docs and lib.
fn same_shape(a: &ScSpecEntry, b: &ScSpecEntry) -> bool {
    crate::rusttype::type_def_to_rust(a) == crate::rusttype::type_def_to_rust(b)
}

fn xdr_b64(v: &ScVal) -> String {
    v.to_xdr_base64(Limits::none()).unwrap_or_default()
}

/// stellar-xdr JSON of a value (the form `stellar xdr decode` prints).
pub fn untyped(v: &ScVal) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn kind(v: &ScVal) -> &'static str {
    match v {
        ScVal::Bool(_) => "Bool",
        ScVal::Void => "Void",
        ScVal::Error(_) => "Error",
        ScVal::U32(_) => "U32",
        ScVal::I32(_) => "I32",
        ScVal::U64(_) => "U64",
        ScVal::I64(_) => "I64",
        ScVal::Timepoint(_) => "Timepoint",
        ScVal::Duration(_) => "Duration",
        ScVal::U128(_) => "U128",
        ScVal::I128(_) => "I128",
        ScVal::U256(_) => "U256",
        ScVal::I256(_) => "I256",
        ScVal::Bytes(_) => "Bytes",
        ScVal::String(_) => "String",
        ScVal::Symbol(_) => "Symbol",
        ScVal::Vec(_) => "Vec",
        ScVal::Map(_) => "Map",
        ScVal::Address(_) => "Address",
        ScVal::ContractInstance(_) => "ContractInstance",
        ScVal::LedgerKeyContractInstance => "LedgerKeyContractInstance",
        ScVal::LedgerKeyNonce(_) => "LedgerKeyNonce",
        ScVal::ExecutableTag(_) => "ExecutableTag",
    }
}

/// Structural summary of a value; used in `unknown` reports so a schema
/// author can see what to declare.
pub fn shape(v: &ScVal) -> String {
    shape_d(v, 0)
}

fn shape_d(v: &ScVal, d: u32) -> String {
    if d > 8 {
        return "..".into();
    }
    match v {
        ScVal::Symbol(s) => format!("Symbol({:?})", s.0.to_utf8_string_lossy()),
        ScVal::U32(n) => format!("U32({n})"),
        ScVal::I32(n) => format!("I32({n})"),
        ScVal::Bytes(b) => format!("Bytes[{}]", b.len()),
        ScVal::Vec(Some(items)) => format!("[{}]", items.iter().map(|i| shape_d(i, d + 1)).collect::<Vec<_>>().join(", ")),
        ScVal::Vec(None) => "Vec(None)".into(),
        ScVal::Map(Some(m)) => {
            let parts: Vec<String> = m
                .iter()
                .map(|e| match &e.key {
                    ScVal::Symbol(s) => format!("{}: {}", s.0.to_utf8_string_lossy(), shape_d(&e.val, d + 1)),
                    k => format!("{}: {}", shape_d(k, d + 1), shape_d(&e.val, d + 1)),
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        other => kind(other).to_string(),
    }
}

fn mm(path: &str, expected: impl Into<String>, found: &ScVal) -> Mismatch {
    Mismatch { path: path.to_string(), expected: expected.into(), found: shape(found) }
}

fn i128_of(hi: i64, lo: u64) -> i128 {
    ((hi as i128) << 64) | lo as i128
}
fn u128_of(hi: u64, lo: u64) -> u128 {
    ((hi as u128) << 64) | lo as u128
}

/// Typed rendering of `v` against `t`. Errors are conformance failures.
pub fn render(reg: &TypeRegistry, v: &ScVal, t: &ScSpecTypeDef, path: &str, warn: &mut Vec<String>) -> Result<Value, Mismatch> {
    render_d(reg, v, t, path, warn, 0)
}

fn render_d(reg: &TypeRegistry, v: &ScVal, t: &ScSpecTypeDef, path: &str, warn: &mut Vec<String>, d: u32) -> Result<Value, Mismatch> {
    if d > MAX_DEPTH {
        return Err(mm(path, "nesting within 64 levels", v));
    }
    let tn = || type_to_rust(t);
    Ok(match (t, v) {
        (ScSpecTypeDef::Val, _) => untyped(v),
        (ScSpecTypeDef::Bool, ScVal::Bool(b)) => json!(b),
        (ScSpecTypeDef::Void, ScVal::Void) => Value::Null,
        (ScSpecTypeDef::Error, ScVal::Error(_)) => untyped(v),
        (ScSpecTypeDef::U32, ScVal::U32(n)) => json!(n),
        (ScSpecTypeDef::I32, ScVal::I32(n)) => json!(n),
        (ScSpecTypeDef::U64, ScVal::U64(n)) => json!(n.to_string()),
        (ScSpecTypeDef::I64, ScVal::I64(n)) => json!(n.to_string()),
        (ScSpecTypeDef::Timepoint, ScVal::Timepoint(n)) => json!(n.0.to_string()),
        (ScSpecTypeDef::Duration, ScVal::Duration(n)) => json!(n.0.to_string()),
        (ScSpecTypeDef::U128, ScVal::U128(p)) => json!(u128_of(p.hi, p.lo).to_string()),
        (ScSpecTypeDef::I128, ScVal::I128(p)) => json!(i128_of(p.hi, p.lo).to_string()),
        (ScSpecTypeDef::U256, ScVal::U256(p)) => {
            json!(format!("0x{:016x}{:016x}{:016x}{:016x}", p.hi_hi, p.hi_lo, p.lo_hi, p.lo_lo))
        }
        (ScSpecTypeDef::I256, ScVal::I256(p)) => {
            json!(format!("0x{:016x}{:016x}{:016x}{:016x}", p.hi_hi as u64, p.hi_lo, p.lo_hi, p.lo_lo))
        }
        (ScSpecTypeDef::Bytes, ScVal::Bytes(b)) => json!(hex::encode(b.as_slice())),
        (ScSpecTypeDef::BytesN(n), ScVal::Bytes(b)) => {
            if b.len() as u32 != n.n {
                return Err(mm(path, format!("BytesN<{}>", n.n), v));
            }
            json!(hex::encode(b.as_slice()))
        }
        (ScSpecTypeDef::String, ScVal::String(s)) => match std::str::from_utf8(s.0.as_slice()) {
            Ok(st) => json!(st),
            Err(_) => {
                warn.push(format!("{path}: String is not UTF-8; rendered as hex"));
                json!({ "hex": hex::encode(s.0.as_slice()) })
            }
        },
        (ScSpecTypeDef::Symbol, ScVal::Symbol(s)) => json!(s.0.to_utf8_string_lossy()),
        (ScSpecTypeDef::Address | ScSpecTypeDef::MuxedAddress, ScVal::Address(a)) => json!(a.to_string()),
        (ScSpecTypeDef::Option(o), ScVal::Void) => {
            let _ = o;
            Value::Null
        }
        (ScSpecTypeDef::Option(o), _) => render_d(reg, v, &o.value_type, path, warn, d + 1)?,
        (ScSpecTypeDef::Result(r), ScVal::Error(_)) => {
            let _ = r;
            json!({ "err": untyped(v) })
        }
        (ScSpecTypeDef::Result(r), _) => json!({ "ok": render_d(reg, v, &r.ok_type, path, warn, d + 1)? }),
        (ScSpecTypeDef::Vec(et), ScVal::Vec(Some(items))) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, it)| render_d(reg, it, &et.element_type, &format!("{path}[{i}]"), warn, d + 1))
                .collect::<Result<_, _>>()?,
        ),
        (ScSpecTypeDef::Map(mt), ScVal::Map(Some(m))) => {
            let mut pairs = Vec::new();
            for (i, e) in m.iter().enumerate() {
                let k = render_d(reg, &e.key, &mt.key_type, &format!("{path}{{key {i}}}"), warn, d + 1)?;
                let val = render_d(reg, &e.val, &mt.value_type, &format!("{path}{{{i}}}"), warn, d + 1)?;
                pairs.push((k, val));
            }
            let all_str = pairs.iter().all(|(k, _)| k.is_string());
            if all_str {
                let mut o = JsonMap::new();
                for (k, val) in pairs {
                    o.insert(k.as_str().unwrap().to_string(), val);
                }
                Value::Object(o)
            } else {
                Value::Array(pairs.into_iter().map(|(k, val)| json!({ "key": k, "value": val })).collect())
            }
        }
        (ScSpecTypeDef::Tuple(tt), ScVal::Vec(Some(items))) => {
            if items.len() != tt.value_types.len() {
                return Err(mm(path, tn(), v));
            }
            Value::Array(
                items
                    .iter()
                    .zip(tt.value_types.iter())
                    .enumerate()
                    .map(|(i, (it, ty))| render_d(reg, it, ty, &format!("{path}.{i}"), warn, d + 1))
                    .collect::<Result<_, _>>()?,
            )
        }
        (ScSpecTypeDef::Udt(u), _) => {
            let name = u.name.to_utf8_string_lossy();
            match reg.get(&name) {
                None => {
                    warn.push(format!("{path}: type {name} is not defined in the storage schema or the contract spec; rendered untyped"));
                    untyped(v)
                }
                Some(def) => render_udt(reg, v, def, &name, path, warn, d)?,
            }
        }
        _ => return Err(mm(path, tn(), v)),
    })
}

fn render_udt(
    reg: &TypeRegistry,
    v: &ScVal,
    def: &ScSpecEntry,
    name: &str,
    path: &str,
    warn: &mut Vec<String>,
    d: u32,
) -> Result<Value, Mismatch> {
    match def {
        ScSpecEntry::UdtStructV0(s) => {
            let tuple = !s.fields.is_empty() && s.fields.iter().enumerate().all(|(i, f)| f.name.to_utf8_string_lossy() == i.to_string());
            if tuple {
                let ScVal::Vec(Some(items)) = v else { return Err(mm(path, name, v)) };
                if items.len() != s.fields.len() {
                    return Err(mm(path, name, v));
                }
                return Ok(Value::Array(
                    items
                        .iter()
                        .zip(s.fields.iter())
                        .enumerate()
                        .map(|(i, (it, f))| render_d(reg, it, &f.type_, &format!("{path}.{i}"), warn, d + 1))
                        .collect::<Result<_, _>>()?,
                ));
            }
            let ScVal::Map(Some(m)) = v else { return Err(mm(path, format!("{name} (Map)"), v)) };
            if m.len() != s.fields.len() {
                return Err(mm(path, format!("{name} with {} fields", s.fields.len()), v));
            }
            let mut o = JsonMap::new();
            for f in s.fields.iter() {
                let fname = f.name.to_utf8_string_lossy();
                let ent = m
                    .iter()
                    .find(|e| matches!(&e.key, ScVal::Symbol(sym) if sym.0.to_utf8_string_lossy() == fname))
                    .ok_or_else(|| mm(path, format!("{name}.{fname}"), v))?;
                o.insert(fname.clone(), render_d(reg, &ent.val, &f.type_, &format!("{path}.{fname}"), warn, d + 1)?);
            }
            Ok(Value::Object(o))
        }
        ScSpecEntry::UdtUnionV0(un) => {
            let ScVal::Vec(Some(items)) = v else { return Err(mm(path, format!("{name} (union)"), v)) };
            let Some(ScVal::Symbol(tag)) = items.first() else { return Err(mm(path, format!("{name} (union)"), v)) };
            let tag = tag.0.to_utf8_string_lossy();
            for c in un.cases.iter() {
                match c {
                    ScSpecUdtUnionCaseV0::VoidV0(vc) if vc.name.to_utf8_string_lossy() == tag => {
                        if items.len() != 1 {
                            return Err(mm(path, format!("{name}::{tag}"), v));
                        }
                        return Ok(json!(tag));
                    }
                    ScSpecUdtUnionCaseV0::TupleV0(tc) if tc.name.to_utf8_string_lossy() == tag => {
                        if items.len() != 1 + tc.type_.len() {
                            return Err(mm(path, format!("{name}::{tag}"), v));
                        }
                        let vals = items[1..]
                            .iter()
                            .zip(tc.type_.iter())
                            .enumerate()
                            .map(|(i, (it, ty))| render_d(reg, it, ty, &format!("{path}.{tag}.{i}"), warn, d + 1))
                            .collect::<Result<Vec<_>, _>>()?;
                        return Ok(json!({ tag: vals }));
                    }
                    _ => {}
                }
            }
            Err(mm(path, format!("{name} case"), v))
        }
        ScSpecEntry::UdtEnumV0(en) => {
            let ScVal::U32(n) = v else { return Err(mm(path, format!("{name} (u32 enum)"), v)) };
            en.cases
                .iter()
                .find(|c| c.value == *n)
                .map(|c| json!(c.name.to_utf8_string_lossy()))
                .ok_or_else(|| mm(path, format!("{name} case value"), v))
        }
        ScSpecEntry::UdtErrorEnumV0(en) => {
            let code = match v {
                ScVal::U32(n) => Some(*n),
                ScVal::Error(stellar_xdr::ScError::Contract(n)) => Some(*n),
                _ => None,
            };
            code.and_then(|n| en.cases.iter().find(|c| c.value == n))
                .map(|c| json!(c.name.to_utf8_string_lossy()))
                .ok_or_else(|| mm(path, format!("{name} (error enum)"), v))
        }
        _ => Err(mm(path, name, v)),
    }
}

/// Does `key` match `pattern`? Returns the typed rendering of the key.
pub fn match_pattern(reg: &TypeRegistry, pattern: &KeyPattern, key: &ScVal, warn: &mut Vec<String>) -> Option<Value> {
    match pattern {
        KeyPattern::Exact(lit) => (lit == key).then(|| match key {
            ScVal::Symbol(s) => json!(s.0.to_utf8_string_lossy()),
            other => untyped(other),
        }),
        KeyPattern::Tagged { tag, fields } => {
            let ScVal::Vec(Some(items)) = key else { return None };
            let Some(ScVal::Symbol(s)) = items.first() else { return None };
            if s.0.as_slice() != tag.as_bytes() || items.len() != fields.len() + 1 {
                return None;
            }
            let named = fields.iter().any(|f| !f.name.is_empty());
            let mut arr = Vec::new();
            let mut obj = JsonMap::new();
            for (i, (it, f)) in items[1..].iter().zip(fields.iter()).enumerate() {
                let r = render(reg, it, &f.type_, &format!("key.{i}"), warn).ok()?;
                if named {
                    obj.insert(if f.name.is_empty() { i.to_string() } else { f.name.clone() }, r);
                } else {
                    arr.push(r);
                }
            }
            Some(if named { json!({ "tag": tag, "fields": obj }) } else { json!({ "tag": tag, "fields": arr }) })
        }
        KeyPattern::Typed(t) => render(reg, key, t, "key", warn).ok(),
    }
}

/// A decoder over one contract: merged schema entries (each labelled with
/// its source) plus a type registry.
#[derive(Clone, Debug, Default)]
pub struct Decoder {
    keys: Vec<(KeyEntry, String)>,
    pub registry: TypeRegistry,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a schema; `source` labels it in output (`embedded`, `sidecar:<file>`).
    pub fn add_schema(&mut self, s: &Schema, source: &str) {
        for k in &s.keys {
            if !self.keys.iter().any(|(e, _)| e == k) {
                self.keys.push((k.clone(), source.to_string()));
            }
        }
        for t in &s.types {
            self.registry.add(t, source);
        }
    }

    /// Add `contractspecv0` entries (UDTs only are used). Call after
    /// `add_schema` so storage-schema types take precedence.
    pub fn add_spec(&mut self, spec: &[ScSpecEntry]) {
        for e in spec {
            self.registry.add(e, "contractspecv0");
        }
    }

    pub fn key_entries(&self) -> impl Iterator<Item = &KeyEntry> {
        self.keys.iter().map(|(k, _)| k)
    }

    pub fn has_keys(&self) -> bool {
        !self.keys.is_empty()
    }

    pub fn decode(&self, durability: Durability, key: &ScVal, val: &ScVal, source: EntrySource) -> DecodedEntry {
        let mut warnings = Vec::new();
        let raw = Raw { key_xdr: xdr_b64(key), val_xdr: xdr_b64(val) };
        let key_shape = shape(key);

        let mut hits: Vec<(usize, Value)> = Vec::new();
        let mut other_durability: Vec<String> = Vec::new();
        for (i, (k, _)) in self.keys.iter().enumerate() {
            let mut w = Vec::new();
            if let Some(rk) = match_pattern(&self.registry, &k.key, key, &mut w) {
                if k.durability == durability {
                    warnings.extend(w);
                    hits.push((i, rk));
                } else {
                    other_durability.push(format!("{} (declared {})", k.name, k.durability.as_str()));
                }
            }
        }
        let best = hits.iter().map(|(i, _)| self.keys[*i].0.key.specificity()).max();
        let top: Vec<&(usize, Value)> = hits.iter().filter(|(i, _)| Some(self.keys[*i].0.key.specificity()) == best).collect();

        let base = |status, reason: Option<String>, warnings: Vec<String>| DecodedEntry {
            durability,
            status,
            name: None,
            lib: None,
            schema_source: None,
            key: untyped(key),
            value: untyped(val),
            value_type: None,
            key_shape: key_shape.clone(),
            candidates: vec![],
            reason,
            warnings,
            source: source.clone(),
            raw: raw.clone(),
        };

        match top.len() {
            0 => {
                let reason = if !self.has_keys() {
                    "no storage schema available for this contract".to_string()
                } else if !other_durability.is_empty() {
                    format!("key shape {key_shape} matches only entries of another durability: {}", other_durability.join(", "))
                } else {
                    format!("no {} schema entry matches key shape {key_shape}", durability.as_str())
                };
                let mut w = warnings;
                w.extend(self.registry.conflicts.iter().cloned());
                base(Status::Unknown, Some(reason), w)
            }
            1 => {
                let (i, rk) = top[0];
                let (k, src) = &self.keys[*i];
                let mut w = warnings;
                let rendered = render(&self.registry, val, &k.value, "value", &mut w);
                let mut out = base(Status::Matched, None, vec![]);
                out.name = Some(k.name.clone());
                out.lib = (!k.lib.is_empty()).then(|| k.lib.clone());
                out.schema_source = Some(src.clone());
                out.key = rk.clone();
                out.value_type = Some(type_to_rust(&k.value));
                match rendered {
                    Ok(v) => out.value = v,
                    Err(m) => {
                        out.status = Status::ValueMismatch;
                        out.reason = Some(format!("key matched {} but value does not conform: {m}", k.name));
                    }
                }
                out.warnings = w;
                out
            }
            _ => {
                let mut cands = Vec::new();
                for (i, rk) in &top {
                    let (k, src) = &self.keys[*i];
                    let mut w = Vec::new();
                    let r = render(&self.registry, val, &k.value, "value", &mut w);
                    cands.push(Candidate {
                        name: k.name.clone(),
                        lib: k.lib.clone(),
                        schema_source: src.clone(),
                        key: rk.clone(),
                        value_conforms: r.is_ok(),
                        value: r.as_ref().ok().cloned(),
                        mismatch: r.err().map(|m| m.to_string()),
                    });
                }
                let n_ok = cands.iter().filter(|c| c.value_conforms).count();
                let reason = format!(
                    "{} schema entries claim key shape {key_shape} ({}); the ledger cannot tell them apart; {} of them accept the value",
                    cands.len(),
                    cands
                        .iter()
                        .map(|c| if c.lib.is_empty() { c.name.clone() } else { format!("{}@{}", c.name, c.lib) })
                        .collect::<Vec<_>>()
                        .join(", "),
                    n_ok
                );
                let mut out = base(Status::Ambiguous, Some(reason), warnings);
                out.candidates = cands;
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar;
    use stellar_xdr::{Int128Parts, ScAddress, ScMap, ScMapEntry, ScSymbol, ScVec};

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol(s.try_into().unwrap()))
    }
    fn addr() -> ScVal {
        ScVal::Address(ScAddress::Contract(stellar_xdr::ContractId(stellar_xdr::Hash([7; 32]))))
    }
    fn i128v(n: i128) -> ScVal {
        ScVal::I128(Int128Parts { hi: (n >> 64) as i64, lo: n as u64 })
    }
    fn vecv(items: Vec<ScVal>) -> ScVal {
        ScVal::Vec(Some(ScVec(items.try_into().unwrap())))
    }

    fn dec(json: &str) -> Decoder {
        let mut d = Decoder::new();
        d.add_schema(&sidecar::parse(json).unwrap(), "sidecar:test");
        d
    }

    const TOKEN: &str = r#"{"storage_schema":0,"keys":[
      {"name":"DataKey::Balance","durability":"persistent","key":{"tagged":{"tag":"Balance","fields":["Address"]}},"value":"i128"},
      {"name":"DataKey::Admin","durability":"instance","key":{"tagged":{"tag":"Admin"}},"value":"Address"},
      {"name":"COUNTER","durability":"instance","key":{"symbol":"COUNTER"},"value":"u32"},
      {"name":"AnySymbol","durability":"instance","key":{"typed":"Symbol"},"value":"Val"},
      {"name":"DataKey::Allowance","durability":"temporary","key":{"tagged":{"tag":"Allowance","fields":["AllowanceDataKey"]}},"value":"AllowanceValue"}
    ],"types":[
      "pub struct AllowanceDataKey { pub from: Address, pub spender: Address }",
      "pub struct AllowanceValue { pub amount: i128, pub live_until_ledger: u32 }"
    ]}"#;

    fn src() -> EntrySource {
        EntrySource { ledger_entry: "contract_data", ..Default::default() }
    }

    #[test]
    fn matches_tagged_and_renders() {
        let d = dec(TOKEN);
        let r = d.decode(Durability::Persistent, &vecv(vec![sym("Balance"), addr()]), &i128v(-5), src());
        assert_eq!(r.status, Status::Matched);
        assert_eq!(r.name.as_deref(), Some("DataKey::Balance"));
        assert_eq!(r.value, json!("-5"));
        assert!(r.key["fields"][0].as_str().unwrap().starts_with('C'));
    }

    #[test]
    fn exact_beats_typed() {
        let d = dec(TOKEN);
        let r = d.decode(Durability::Instance, &sym("COUNTER"), &ScVal::U32(3), src());
        assert_eq!(r.status, Status::Matched);
        assert_eq!(r.name.as_deref(), Some("COUNTER"));
        let r2 = d.decode(Durability::Instance, &sym("OTHER"), &ScVal::U32(3), src());
        assert_eq!(r2.name.as_deref(), Some("AnySymbol"));
    }

    #[test]
    fn durability_partitions_and_reports() {
        let d = dec(TOKEN);
        let r = d.decode(Durability::Temporary, &vecv(vec![sym("Balance"), addr()]), &i128v(1), src());
        assert_eq!(r.status, Status::Unknown);
        assert!(r.reason.unwrap().contains("declared persistent"));
    }

    #[test]
    fn value_mismatch_is_reported_not_hidden() {
        let d = dec(TOKEN);
        let r = d.decode(Durability::Persistent, &vecv(vec![sym("Balance"), addr()]), &ScVal::U32(1), src());
        assert_eq!(r.status, Status::ValueMismatch);
        assert!(r.reason.unwrap().contains("expected i128"));
    }

    #[test]
    fn struct_udt_key_and_value() {
        let d = dec(TOKEN);
        let k = vecv(vec![
            sym("Allowance"),
            ScVal::Map(Some(ScMap(
                vec![ScMapEntry { key: sym("from"), val: addr() }, ScMapEntry { key: sym("spender"), val: addr() }].try_into().unwrap(),
            ))),
        ]);
        let v = ScVal::Map(Some(ScMap(
            vec![ScMapEntry { key: sym("amount"), val: i128v(10) }, ScMapEntry { key: sym("live_until_ledger"), val: ScVal::U32(99) }]
                .try_into()
                .unwrap(),
        )));
        let r = d.decode(Durability::Temporary, &k, &v, src());
        assert_eq!(r.status, Status::Matched, "{r:?}");
        assert_eq!(r.value, json!({"amount": "10", "live_until_ledger": 99}));
        // wrong arity in the key struct -> no match
        let bad = vecv(vec![sym("Allowance"), ScVal::Map(Some(ScMap(vec![].try_into().unwrap())))]);
        assert_eq!(d.decode(Durability::Temporary, &bad, &v, src()).status, Status::Unknown);
    }

    #[test]
    fn collisions_are_ambiguous_with_value_evidence() {
        // #2088: two libraries both declare `Balance(Address)`.
        let d = dec(&TOKEN.replace(
            r#"{"name":"COUNTER""#,
            r#"{"name":"FungibleStorageKey::Balance","lib":"other-lib","durability":"persistent","key":{"tagged":{"tag":"Balance","fields":["Address"]}},"value":"u32"},
               {"name":"COUNTER""#,
        ));
        let r = d.decode(Durability::Persistent, &vecv(vec![sym("Balance"), addr()]), &i128v(1), src());
        assert_eq!(r.status, Status::Ambiguous);
        assert_eq!(r.candidates.len(), 2);
        assert_eq!(r.candidates.iter().filter(|c| c.value_conforms).count(), 1);
    }

    #[test]
    fn no_schema_is_unknown() {
        let d = Decoder::new();
        let r = d.decode(Durability::Persistent, &sym("X"), &ScVal::Void, src());
        assert_eq!(r.status, Status::Unknown);
        assert_eq!(r.key_shape, "Symbol(\"X\")");
        assert_eq!(r.key, json!({"symbol": "X"}));
    }

    #[test]
    fn missing_udt_warns_and_renders_untyped() {
        let d = dec(r#"{"storage_schema":0,"keys":[{"name":"K","durability":"persistent","key":{"symbol":"K"},"value":"Mystery"}]}"#);
        let r = d.decode(Durability::Persistent, &sym("K"), &ScVal::U32(1), src());
        assert_eq!(r.status, Status::Matched);
        assert!(r.warnings[0].contains("Mystery"));
    }
}
