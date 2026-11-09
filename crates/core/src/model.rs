// SPDX-License-Identifier: Apache-2.0
//! The `contractstoragev0` data model and its XDR codec (SPEC.md section 3).
//!
//! The section payload is a concatenation of `SCStorageEntry` XDR values with
//! no outer length, exactly like `contractspecv0`. Multiple sections with the
//! same name are concatenated in module order, so entries emitted by several
//! crates (or by the macro and by a post-build inject) form one stream.

use std::io::{Read, Write};

use stellar_xdr::{Error as XdrError, Limited, Limits, ReadXdr, ScSpecEntry, ScSpecTypeDef, ScVal, StringM, VecM, WriteXdr};

/// Wasm custom section name. Versioned in the name, like `contractspecv0`.
pub const SECTION_NAME: &str = "contractstoragev0";

/// XDR discriminant of `SC_STORAGE_ENTRY_TYPE_V0`, big-endian. The macro
/// prefixes a `#[contracttype]`'s own `spec_xdr()` bytes with this.
pub const ENTRY_KIND_TYPE_V0_BYTES: [u8; 4] = [0, 0, 0, 1];

/// Durability class of a storage entry. Discriminants extend
/// `ContractDataDurability` (TEMPORARY = 0, PERSISTENT = 1) with INSTANCE = 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Durability {
    Temporary = 0,
    Persistent = 1,
    Instance = 2,
}

impl Durability {
    pub fn as_str(self) -> &'static str {
        match self {
            Durability::Temporary => "temporary",
            Durability::Persistent => "persistent",
            Durability::Instance => "instance",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "temporary" => Some(Durability::Temporary),
            "persistent" => Some(Durability::Persistent),
            "instance" => Some(Durability::Instance),
            _ => None,
        }
    }
}

/// One positional field of a tagged key. `name` is optional (empty for
/// `enum` tuple variants; set for `#[contractdata]`-style `#[key]` fields).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyField {
    pub name: String,
    pub type_: ScSpecTypeDef,
}

/// How a key `ScVal` is recognised (SPEC.md section 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyPattern {
    /// The key is exactly this value (raw `Symbol` keys such as
    /// `symbol_short!("COUNTER")`, integer keys, any literal).
    Exact(ScVal),
    /// The key is `Vec[Symbol(tag), f1, ..., fn]` with each `fi` conforming to
    /// `fields[i]`. This is how `#[contracttype] enum` variants encode
    /// (unit variant: `Vec[Symbol(tag)]`) and how `(Symbol("NS"), k1, ..)`
    /// tuple keys (sdk#2127 `namespace`) encode.
    Tagged { tag: String, fields: Vec<KeyField> },
    /// The key is any value conforming to this type (catch-all; lowest
    /// precedence).
    Typed(ScSpecTypeDef),
}

impl KeyPattern {
    /// Precedence used by the matcher: exact > tagged > typed.
    pub fn specificity(&self) -> u8 {
        match self {
            KeyPattern::Exact(_) => 3,
            KeyPattern::Tagged { .. } => 2,
            KeyPattern::Typed(_) => 1,
        }
    }
}

/// `SCStorageKeyEntryV0`: one declared storage slot family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEntry {
    pub doc: String,
    /// Origin namespace: crate (and optionally version) that writes the entry,
    /// e.g. `stellar-tokens@0.8.0`. Empty means the contract itself.
    pub lib: String,
    /// Display name, e.g. `DataKey::Balance`.
    pub name: String,
    pub durability: Durability,
    pub key: KeyPattern,
    pub value: ScSpecTypeDef,
}

/// `SCStorageEntry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaEntry {
    Key(KeyEntry),
    /// A type definition (struct / union / enum) carried with the storage
    /// schema because spec shaking (soroban-sdk >= 28) removes storage-only
    /// types from `contractspecv0`. Same encoding as a `contractspecv0` entry.
    Type(ScSpecEntry),
}

/// A whole schema: the de-duplicated contents of every `contractstoragev0`
/// section of a module, or of a sidecar file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    pub keys: Vec<KeyEntry>,
    pub types: Vec<ScSpecEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("xdr: {0}")]
    Xdr(#[from] XdrError),
    #[error("{0}")]
    Invalid(String),
}

fn limits_for(len: usize) -> Limits {
    Limits { depth: 500, len: len.max(64) }
}

fn sm<const N: u32>(s: &str, what: &str) -> Result<StringM<N>, Error> {
    StringM::<N>::try_from(s.as_bytes().to_vec()).map_err(|_| Error::Invalid(format!("{what} is longer than {N} bytes: {s:?}")))
}

fn sm_to_string<const N: u32>(s: &StringM<N>, what: &str) -> Result<String, XdrError> {
    let _ = what;
    String::from_utf8(s.to_vec()).map_err(|_| XdrError::Invalid)
}

impl ReadXdr for Durability {
    fn read_xdr<R: Read>(r: &mut Limited<R>) -> Result<Self, XdrError> {
        match u32::read_xdr(r)? {
            0 => Ok(Durability::Temporary),
            1 => Ok(Durability::Persistent),
            2 => Ok(Durability::Instance),
            _ => Err(XdrError::Invalid),
        }
    }
}
impl WriteXdr for Durability {
    fn write_xdr<W: Write>(&self, w: &mut Limited<W>) -> Result<(), XdrError> {
        (*self as u32).write_xdr(w)
    }
}

impl ReadXdr for KeyField {
    fn read_xdr<R: Read>(r: &mut Limited<R>) -> Result<Self, XdrError> {
        {
            let name = StringM::<30>::read_xdr(r)?;
            Ok(KeyField { name: sm_to_string(&name, "field name")?, type_: ScSpecTypeDef::read_xdr(r)? })
        }
    }
}
impl WriteXdr for KeyField {
    fn write_xdr<W: Write>(&self, w: &mut Limited<W>) -> Result<(), XdrError> {
        {
            StringM::<30>::try_from(self.name.as_bytes().to_vec()).map_err(|_| XdrError::LengthExceedsMax)?.write_xdr(w)?;
            self.type_.write_xdr(w)
        }
    }
}

impl ReadXdr for KeyPattern {
    fn read_xdr<R: Read>(r: &mut Limited<R>) -> Result<Self, XdrError> {
        match u32::read_xdr(r)? {
            0 => Ok(KeyPattern::Exact(ScVal::read_xdr(r)?)),
            1 => {
                let tag = StringM::<32>::read_xdr(r)?;
                let fields = VecM::<KeyField>::read_xdr(r)?;
                Ok(KeyPattern::Tagged { tag: sm_to_string(&tag, "tag")?, fields: fields.to_vec() })
            }
            2 => Ok(KeyPattern::Typed(ScSpecTypeDef::read_xdr(r)?)),
            _ => Err(XdrError::Invalid),
        }
    }
}
impl WriteXdr for KeyPattern {
    fn write_xdr<W: Write>(&self, w: &mut Limited<W>) -> Result<(), XdrError> {
        match self {
            KeyPattern::Exact(v) => {
                0u32.write_xdr(w)?;
                v.write_xdr(w)
            }
            KeyPattern::Tagged { tag, fields } => {
                1u32.write_xdr(w)?;
                StringM::<32>::try_from(tag.as_bytes().to_vec()).map_err(|_| XdrError::LengthExceedsMax)?.write_xdr(w)?;
                VecM::<KeyField>::try_from(fields.clone()).map_err(|_| XdrError::LengthExceedsMax)?.write_xdr(w)
            }
            KeyPattern::Typed(t) => {
                2u32.write_xdr(w)?;
                t.write_xdr(w)
            }
        }
    }
}

impl ReadXdr for KeyEntry {
    fn read_xdr<R: Read>(r: &mut Limited<R>) -> Result<Self, XdrError> {
        {
            let doc = StringM::<1024>::read_xdr(r)?;
            let lib = StringM::<80>::read_xdr(r)?;
            let name = StringM::<60>::read_xdr(r)?;
            Ok(KeyEntry {
                doc: sm_to_string(&doc, "doc")?,
                lib: sm_to_string(&lib, "lib")?,
                name: sm_to_string(&name, "name")?,
                durability: Durability::read_xdr(r)?,
                key: KeyPattern::read_xdr(r)?,
                value: ScSpecTypeDef::read_xdr(r)?,
            })
        }
    }
}
impl WriteXdr for KeyEntry {
    fn write_xdr<W: Write>(&self, w: &mut Limited<W>) -> Result<(), XdrError> {
        {
            let e = |_| XdrError::LengthExceedsMax;
            StringM::<1024>::try_from(self.doc.as_bytes().to_vec()).map_err(e)?.write_xdr(w)?;
            StringM::<80>::try_from(self.lib.as_bytes().to_vec()).map_err(e)?.write_xdr(w)?;
            StringM::<60>::try_from(self.name.as_bytes().to_vec()).map_err(e)?.write_xdr(w)?;
            self.durability.write_xdr(w)?;
            self.key.write_xdr(w)?;
            self.value.write_xdr(w)
        }
    }
}

impl ReadXdr for SchemaEntry {
    fn read_xdr<R: Read>(r: &mut Limited<R>) -> Result<Self, XdrError> {
        match u32::read_xdr(r)? {
            0 => Ok(SchemaEntry::Key(KeyEntry::read_xdr(r)?)),
            1 => Ok(SchemaEntry::Type(ScSpecEntry::read_xdr(r)?)),
            _ => Err(XdrError::Invalid),
        }
    }
}
impl WriteXdr for SchemaEntry {
    fn write_xdr<W: Write>(&self, w: &mut Limited<W>) -> Result<(), XdrError> {
        match self {
            SchemaEntry::Key(k) => {
                0u32.write_xdr(w)?;
                k.write_xdr(w)
            }
            SchemaEntry::Type(t) => {
                1u32.write_xdr(w)?;
                t.write_xdr(w)
            }
        }
    }
}

impl KeyEntry {
    /// Validate string limits before encoding, with a readable message.
    pub fn validate(&self) -> Result<(), Error> {
        sm::<1024>(&self.doc, "doc")?;
        sm::<80>(&self.lib, "lib")?;
        sm::<60>(&self.name, "name")?;
        if let KeyPattern::Tagged { tag, fields } = &self.key {
            sm::<32>(tag, "tag")?;
            if !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err(Error::Invalid(format!("tag {tag:?} is not a valid Symbol (a-zA-Z0-9_)")));
            }
            for f in fields {
                sm::<30>(&f.name, "field name")?;
            }
        }
        Ok(())
    }
}

/// Encode one entry as XDR bytes.
pub fn encode_entry(e: &SchemaEntry) -> Result<Vec<u8>, Error> {
    if let SchemaEntry::Key(k) = e {
        k.validate()?;
    }
    Ok(e.to_xdr(Limits::none())?)
}

/// Decode a section payload (or the concatenation of several) into entries.
pub fn decode_stream(bytes: &[u8]) -> Result<Vec<SchemaEntry>, Error> {
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);
    let mut r = Limited::new(&mut cursor, limits_for(bytes.len()));
    while (r.inner.position() as usize) < bytes.len() {
        let e = SchemaEntry::read_xdr(&mut r)?;
        out.push(e);
    }
    Ok(out)
}

impl Schema {
    /// Build from a list of entries, de-duplicating identical key entries and
    /// identical type definitions (SPEC.md 3.4).
    pub fn from_entries(entries: Vec<SchemaEntry>) -> Self {
        let mut s = Schema::default();
        for e in entries {
            s.push(e);
        }
        s
    }

    pub fn push(&mut self, e: SchemaEntry) {
        match e {
            SchemaEntry::Key(k) => {
                if !self.keys.contains(&k) {
                    self.keys.push(k)
                }
            }
            SchemaEntry::Type(t) => {
                if !self.types.contains(&t) {
                    self.types.push(t)
                }
            }
        }
    }

    pub fn merge(&mut self, other: Schema) {
        for k in other.keys {
            self.push(SchemaEntry::Key(k));
        }
        for t in other.types {
            self.push(SchemaEntry::Type(t));
        }
    }

    pub fn from_section_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self::from_entries(decode_stream(bytes)?))
    }

    /// Encode as one section payload: key entries first, then types.
    pub fn to_section_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        for k in &self.keys {
            out.extend(encode_entry(&SchemaEntry::Key(k.clone()))?);
        }
        for t in &self.types {
            out.extend(encode_entry(&SchemaEntry::Type(t.clone()))?);
        }
        Ok(out)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.types.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{ScSpecTypeUdt, ScSymbol};

    fn sample() -> Schema {
        Schema {
            keys: vec![
                KeyEntry {
                    doc: "balance".into(),
                    lib: String::new(),
                    name: "DataKey::Balance".into(),
                    durability: Durability::Persistent,
                    key: KeyPattern::Tagged {
                        tag: "Balance".into(),
                        fields: vec![KeyField { name: String::new(), type_: ScSpecTypeDef::Address }],
                    },
                    value: ScSpecTypeDef::I128,
                },
                KeyEntry {
                    doc: String::new(),
                    lib: "soroban-examples/increment".into(),
                    name: "COUNTER".into(),
                    durability: Durability::Instance,
                    key: KeyPattern::Exact(ScVal::Symbol(ScSymbol("COUNTER".try_into().unwrap()))),
                    value: ScSpecTypeDef::U32,
                },
                KeyEntry {
                    doc: String::new(),
                    lib: String::new(),
                    name: "ByAddress".into(),
                    durability: Durability::Temporary,
                    key: KeyPattern::Typed(ScSpecTypeDef::Address),
                    value: ScSpecTypeDef::Udt(ScSpecTypeUdt { name: "Thing".try_into().unwrap() }),
                },
            ],
            types: vec![],
        }
    }

    #[test]
    fn roundtrip_section_bytes() {
        let s = sample();
        let bytes = s.to_section_bytes().unwrap();
        assert_eq!(Schema::from_section_bytes(&bytes).unwrap(), s);
    }

    #[test]
    fn concatenation_is_a_stream_and_dedups() {
        let s = sample();
        let mut bytes = s.to_section_bytes().unwrap();
        bytes.extend(s.to_section_bytes().unwrap());
        assert_eq!(Schema::from_section_bytes(&bytes).unwrap(), s);
    }

    #[test]
    fn rejects_trailing_garbage_and_bad_tags() {
        let mut bytes = sample().to_section_bytes().unwrap();
        bytes.extend([0, 0, 0, 9]);
        assert!(Schema::from_section_bytes(&bytes).is_err());
        let mut k = sample().keys[0].clone();
        k.key = KeyPattern::Tagged { tag: "has space".into(), fields: vec![] };
        assert!(encode_entry(&SchemaEntry::Key(k)).is_err());
    }

    #[test]
    fn wire_layout_is_stable() {
        // Unit variant `Admin`, instance, value Address, no doc/lib.
        let k = KeyEntry {
            doc: String::new(),
            lib: String::new(),
            name: "A".into(),
            durability: Durability::Instance,
            key: KeyPattern::Tagged { tag: "Admin".into(), fields: vec![] },
            value: ScSpecTypeDef::Address,
        };
        let b = encode_entry(&SchemaEntry::Key(k)).unwrap();
        let expect: Vec<u8> = [
            &[0, 0, 0, 0][..],            // SC_STORAGE_ENTRY_KEY_V0
            &[0, 0, 0, 0],                // doc ""
            &[0, 0, 0, 0],                // lib ""
            &[0, 0, 0, 1, b'A', 0, 0, 0], // name "A"
            &[0, 0, 0, 2],                // INSTANCE
            &[0, 0, 0, 1],                // TAGGED
            &[0, 0, 0, 5, b'A', b'd', b'm', b'i', b'n', 0, 0, 0],
            &[0, 0, 0, 0],    // 0 fields
            &[0, 0, 0, 0x13], // SC_SPEC_TYPE_ADDRESS = 19
        ]
        .concat();
        assert_eq!(b, expect);
    }
}
