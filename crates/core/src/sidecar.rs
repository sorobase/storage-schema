// SPDX-License-Identifier: Apache-2.0
//! Sidecar JSON: the human-writable form of a `contractstoragev0` schema
//! (SPEC.md section 5, JSON Schema in `schemas/storage-schema.v0.schema.json`).
//! It maps 1:1 onto the XDR model, so `inject` and `extract` round-trip.

use serde::{Deserialize, Serialize};
use stellar_xdr::{ScSpecEntry, ScSymbol, ScVal};

use crate::model::{Durability, KeyEntry, KeyField, KeyPattern, Schema};
use crate::rusttype::{parse_type_def_str, parse_type_str, type_def_to_rust, type_to_rust};

pub const SIDECAR_SCHEMA_ID: &str = "https://sik.invalid/storage-schema/v0.schema.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarFile {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Format version; must be 0 (matches the `v0` in the section name).
    pub storage_schema: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub keys: Vec<SidecarKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<SidecarType>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarKey {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lib: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    pub durability: Durability,
    pub key: SidecarPattern,
    /// Value type in Rust syntax, e.g. `"i128"`, `"Option<BalanceValue>"`.
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SidecarPattern {
    /// Sugar for `exact` with a Symbol.
    Symbol(String),
    /// Any literal key, as stellar-xdr JSON (`{"u32": 7}`, `{"symbol": "X"}`).
    Exact(serde_json::Value),
    Tagged {
        tag: String,
        #[serde(default)]
        fields: Vec<SidecarField>,
    },
    /// Catch-all: any key conforming to this type.
    Typed(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SidecarField {
    Type(String),
    Named {
        name: String,
        #[serde(rename = "type")]
        type_: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SidecarType {
    /// Rust source of a `#[contracttype]` struct or enum.
    Rust(String),
    /// stellar-xdr JSON of an `SCSpecEntry` (lossless, keeps docs).
    Xdr(Box<ScSpecEntry>),
}

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("key {name:?}: {msg}")]
    Key { name: String, msg: String },
    #[error("type definition #{index}: {msg}")]
    Type { index: usize, msg: String },
    #[error("unsupported storage_schema version {0}; this tool reads version 0")]
    Version(u32),
    #[error("{0}")]
    Model(#[from] crate::model::Error),
}

pub fn parse(json: &str) -> Result<Schema, SidecarError> {
    let f: SidecarFile = serde_json::from_str(json)?;
    from_file(f)
}

pub fn from_file(f: SidecarFile) -> Result<Schema, SidecarError> {
    if f.storage_schema != 0 {
        return Err(SidecarError::Version(f.storage_schema));
    }
    let mut schema = Schema::default();
    for k in f.keys {
        let kerr = |msg: String| SidecarError::Key { name: k.name.clone(), msg };
        let key = match &k.key {
            SidecarPattern::Symbol(s) => KeyPattern::Exact(ScVal::Symbol(ScSymbol(
                s.as_str().try_into().map_err(|_| kerr(format!("symbol {s:?} longer than 32 bytes")))?,
            ))),
            SidecarPattern::Exact(v) => {
                KeyPattern::Exact(serde_json::from_value::<ScVal>(v.clone()).map_err(|e| kerr(format!("exact: not an ScVal: {e}")))?)
            }
            SidecarPattern::Tagged { tag, fields } => {
                let mut out = Vec::new();
                for f in fields {
                    let (name, ty) = match f {
                        SidecarField::Type(t) => (String::new(), t),
                        SidecarField::Named { name, type_ } => (name.clone(), type_),
                    };
                    out.push(KeyField { name, type_: parse_type_str(ty).map_err(|e| kerr(e.0))? });
                }
                KeyPattern::Tagged { tag: tag.clone(), fields: out }
            }
            SidecarPattern::Typed(t) => KeyPattern::Typed(parse_type_str(t).map_err(|e| kerr(e.0))?),
        };
        let entry = KeyEntry {
            doc: k.doc.clone(),
            lib: k.lib.clone(),
            name: k.name.clone(),
            durability: k.durability,
            key,
            value: parse_type_str(&k.value).map_err(|e| kerr(e.0))?,
        };
        entry.validate().map_err(|e| kerr(e.to_string()))?;
        schema.push(crate::model::SchemaEntry::Key(entry));
    }
    for (index, t) in f.types.into_iter().enumerate() {
        let e = match t {
            SidecarType::Rust(s) => parse_type_def_str(&s).map_err(|e| SidecarError::Type { index, msg: e.0 })?,
            SidecarType::Xdr(e) => *e,
        };
        schema.push(crate::model::SchemaEntry::Type(e));
    }
    Ok(schema)
}

/// Render a schema as a sidecar file. Types use Rust syntax when that is
/// lossless, otherwise the stellar-xdr JSON form.
pub fn to_file(s: &Schema, description: &str) -> SidecarFile {
    let keys = s
        .keys
        .iter()
        .map(|k| SidecarKey {
            name: k.name.clone(),
            lib: k.lib.clone(),
            doc: k.doc.clone(),
            durability: k.durability,
            key: match &k.key {
                KeyPattern::Exact(ScVal::Symbol(sym)) => SidecarPattern::Symbol(sym.0.to_utf8_string_lossy()),
                KeyPattern::Exact(v) => SidecarPattern::Exact(serde_json::to_value(v).expect("ScVal serialises")),
                KeyPattern::Tagged { tag, fields } => SidecarPattern::Tagged {
                    tag: tag.clone(),
                    fields: fields
                        .iter()
                        .map(|f| {
                            if f.name.is_empty() {
                                SidecarField::Type(type_to_rust(&f.type_))
                            } else {
                                SidecarField::Named { name: f.name.clone(), type_: type_to_rust(&f.type_) }
                            }
                        })
                        .collect(),
                },
                KeyPattern::Typed(t) => SidecarPattern::Typed(type_to_rust(t)),
            },
            value: type_to_rust(&k.value),
        })
        .collect();
    let types = s
        .types
        .iter()
        .map(|t| match type_def_to_rust(t) {
            Some(r) if parse_type_def_str(&r).ok().as_ref() == Some(t) => SidecarType::Rust(r),
            _ => SidecarType::Xdr(Box::new(t.clone())),
        })
        .collect();
    SidecarFile { schema: Some(SIDECAR_SCHEMA_ID.to_string()), storage_schema: 0, description: description.to_string(), keys, types }
}

pub fn to_json(s: &Schema, description: &str) -> String {
    serde_json::to_string_pretty(&to_file(s, description)).expect("sidecar serialises")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "storage_schema": 0,
      "keys": [
        {"name": "DataKey::Balance", "durability": "persistent",
         "key": {"tagged": {"tag": "Balance", "fields": ["Address"]}}, "value": "BalanceValue"},
        {"name": "METADATA", "lib": "soroban-env-host", "durability": "instance",
         "key": {"symbol": "METADATA"}, "value": "StellarAssetContractMetadata"},
        {"name": "Seven", "durability": "temporary", "key": {"exact": {"u32": 7}}, "value": "bool"},
        {"name": "Deposit", "durability": "persistent",
         "key": {"tagged": {"tag": "USER_DEPOSIT", "fields": [{"name": "user", "type": "Address"}]}}, "value": "(u32, u32)"},
        {"name": "Any", "durability": "temporary", "key": {"typed": "Address"}, "value": "Val"}
      ],
      "types": [
        "pub struct BalanceValue { pub amount: i128, pub authorized: bool, pub clawback: bool }",
        "pub struct StellarAssetContractMetadata { pub decimal: u32, pub name: String, pub symbol: String }"
      ]
    }"#;

    #[test]
    fn parse_and_roundtrip() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.keys.len(), 5);
        assert_eq!(s.types.len(), 2);
        let again = parse(&to_json(&s, "")).unwrap();
        assert_eq!(again, s);
        // and through XDR
        let bytes = s.to_section_bytes().unwrap();
        assert_eq!(Schema::from_section_bytes(&bytes).unwrap(), s);
    }

    #[test]
    fn errors_name_the_key() {
        let bad = SAMPLE.replace("\"BalanceValue\"}", "\"Vec<\"}");
        let e = parse(&bad).unwrap_err().to_string();
        assert!(e.contains("DataKey::Balance"), "{e}");
        assert!(parse(&SAMPLE.replace("\"storage_schema\": 0", "\"storage_schema\": 1")).is_err());
        assert!(parse(&SAMPLE.replace("\"durability\": \"instance\"", "\"durability\": \"forever\"")).is_err());
    }
}
