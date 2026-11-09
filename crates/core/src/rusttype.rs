// SPDX-License-Identifier: Apache-2.0
//! Rust type syntax <-> `ScSpecTypeDef` / `ScSpecEntry`.
//!
//! One mapping serves both the proc-macro (which sees `syn::Type`s in a
//! contract's source) and sidecar files (where people write types as Rust
//! strings, e.g. `"Vec<Address>"` or `"pub struct BalanceValue { .. }"`).
//! The mapping follows soroban-sdk-macros `map_type` (sdk 28.0.0) for the
//! built-in names; it is an independent implementation, not a copy.

use stellar_xdr::{
    ScSpecEntry, ScSpecTypeBytesN, ScSpecTypeDef, ScSpecTypeMap, ScSpecTypeOption, ScSpecTypeResult, ScSpecTypeTuple, ScSpecTypeUdt,
    ScSpecTypeVec, ScSpecUdtEnumCaseV0, ScSpecUdtEnumV0, ScSpecUdtStructFieldV0, ScSpecUdtStructV0, ScSpecUdtUnionCaseTupleV0,
    ScSpecUdtUnionCaseV0, ScSpecUdtUnionCaseVoidV0, ScSpecUdtUnionV0, StringM,
};
use syn::{Expr, Fields, GenericArgument, Lit, PathArguments, Type};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TypeError(pub String);

fn err<T>(msg: impl Into<String>) -> Result<T, TypeError> {
    Err(TypeError(msg.into()))
}

fn sm<const N: u32>(s: &str) -> Result<StringM<N>, TypeError> {
    StringM::<N>::try_from(s).map_err(|_| TypeError(format!("{s:?} exceeds {N} bytes")))
}

/// Map a Rust type to a spec type. UDT paths encountered (at any depth) are
/// pushed to `udts` so the macro can embed their definitions.
pub fn map_type(t: &Type, udts: &mut Vec<syn::Path>) -> Result<ScSpecTypeDef, TypeError> {
    match t {
        Type::Reference(r) => map_type(&r.elem, udts),
        Type::Paren(p) => map_type(&p.elem, udts),
        Type::Group(g) => map_type(&g.elem, udts),
        Type::Tuple(tt) => {
            if tt.elems.is_empty() {
                return Ok(ScSpecTypeDef::Void);
            }
            let v = tt.elems.iter().map(|e| map_type(e, udts)).collect::<Result<Vec<_>, _>>()?;
            let value_types = v.try_into().map_err(|_| TypeError("tuple has more than 12 elements".into()))?;
            Ok(ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple { value_types })))
        }
        Type::Path(tp) => {
            let seg = tp.path.segments.last().ok_or_else(|| TypeError("empty type path".into()))?;
            let name = seg.ident.to_string();
            let args: Vec<&GenericArgument> = match &seg.arguments {
                PathArguments::AngleBracketed(a) => a.args.iter().collect(),
                PathArguments::None => vec![],
                PathArguments::Parenthesized(_) => return err(format!("unsupported type {name}(..)")),
            };
            let ty_arg = |i: usize| -> Result<&Type, TypeError> {
                match args.get(i) {
                    Some(GenericArgument::Type(t)) => Ok(t),
                    _ => Err(TypeError(format!("{name} needs a type argument at position {i}"))),
                }
            };
            let simple = match name.as_str() {
                "Val" => Some(ScSpecTypeDef::Val),
                "bool" => Some(ScSpecTypeDef::Bool),
                "Error" => Some(ScSpecTypeDef::Error),
                "u32" => Some(ScSpecTypeDef::U32),
                "i32" => Some(ScSpecTypeDef::I32),
                "u64" => Some(ScSpecTypeDef::U64),
                "i64" => Some(ScSpecTypeDef::I64),
                "Timepoint" => Some(ScSpecTypeDef::Timepoint),
                "Duration" => Some(ScSpecTypeDef::Duration),
                "u128" => Some(ScSpecTypeDef::U128),
                "i128" => Some(ScSpecTypeDef::I128),
                "U256" => Some(ScSpecTypeDef::U256),
                "I256" => Some(ScSpecTypeDef::I256),
                "Bytes" => Some(ScSpecTypeDef::Bytes),
                "String" => Some(ScSpecTypeDef::String),
                "Symbol" => Some(ScSpecTypeDef::Symbol),
                "Address" => Some(ScSpecTypeDef::Address),
                "MuxedAddress" => Some(ScSpecTypeDef::MuxedAddress),
                _ => None,
            };
            if let Some(s) = simple {
                if !args.is_empty() {
                    return err(format!("{name} takes no generic arguments"));
                }
                return Ok(s);
            }
            match name.as_str() {
                "Option" => Ok(ScSpecTypeDef::Option(Box::new(ScSpecTypeOption { value_type: Box::new(map_type(ty_arg(0)?, udts)?) }))),
                "Result" => Ok(ScSpecTypeDef::Result(Box::new(ScSpecTypeResult {
                    ok_type: Box::new(map_type(ty_arg(0)?, udts)?),
                    error_type: Box::new(map_type(ty_arg(1)?, udts)?),
                }))),
                "Vec" => Ok(ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec { element_type: Box::new(map_type(ty_arg(0)?, udts)?) }))),
                "Map" => Ok(ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
                    key_type: Box::new(map_type(ty_arg(0)?, udts)?),
                    value_type: Box::new(map_type(ty_arg(1)?, udts)?),
                }))),
                "BytesN" | "Hash" => {
                    let n = match args.first() {
                        Some(GenericArgument::Const(Expr::Lit(l))) => lit_u32(&l.lit),
                        Some(GenericArgument::Const(Expr::Block(b))) => match b.block.stmts.first() {
                            Some(syn::Stmt::Expr(Expr::Lit(l), _)) => lit_u32(&l.lit),
                            _ => None,
                        },
                        // `BytesN<32>` parses the 32 as a const argument; some
                        // syn versions surface a type path for idents.
                        _ => None,
                    };
                    match n {
                        Some(n) => Ok(ScSpecTypeDef::BytesN(ScSpecTypeBytesN { n })),
                        None => err(format!("{name} needs an integer literal length, e.g. {name}<32>")),
                    }
                }
                _ => {
                    if !args.is_empty() {
                        return err(format!("generic user type {name}<..> is not supported by contract specs"));
                    }
                    udts.push(tp.path.clone());
                    Ok(ScSpecTypeDef::Udt(ScSpecTypeUdt { name: sm::<60>(&name)? }))
                }
            }
        }
        other => err(format!("unsupported type syntax: {}", quote::quote!(#other))),
    }
}

fn lit_u32(l: &Lit) -> Option<u32> {
    match l {
        Lit::Int(i) => i.base10_parse().ok(),
        _ => None,
    }
}

/// Parse a type written as a Rust string, e.g. `"Map<Address, i128>"`.
pub fn parse_type_str(s: &str) -> Result<ScSpecTypeDef, TypeError> {
    let t: Type = syn::parse_str(s).map_err(|e| TypeError(format!("cannot parse type {s:?}: {e}")))?;
    map_type(&t, &mut Vec::new())
}

/// Render a spec type back to Rust syntax (inverse of `parse_type_str`).
pub fn type_to_rust(t: &ScSpecTypeDef) -> String {
    match t {
        ScSpecTypeDef::Val => "Val".into(),
        ScSpecTypeDef::Bool => "bool".into(),
        ScSpecTypeDef::Void => "()".into(),
        ScSpecTypeDef::Error => "Error".into(),
        ScSpecTypeDef::U32 => "u32".into(),
        ScSpecTypeDef::I32 => "i32".into(),
        ScSpecTypeDef::U64 => "u64".into(),
        ScSpecTypeDef::I64 => "i64".into(),
        ScSpecTypeDef::Timepoint => "Timepoint".into(),
        ScSpecTypeDef::Duration => "Duration".into(),
        ScSpecTypeDef::U128 => "u128".into(),
        ScSpecTypeDef::I128 => "i128".into(),
        ScSpecTypeDef::U256 => "U256".into(),
        ScSpecTypeDef::I256 => "I256".into(),
        ScSpecTypeDef::Bytes => "Bytes".into(),
        ScSpecTypeDef::String => "String".into(),
        ScSpecTypeDef::Symbol => "Symbol".into(),
        ScSpecTypeDef::Address => "Address".into(),
        ScSpecTypeDef::MuxedAddress => "MuxedAddress".into(),
        ScSpecTypeDef::Option(o) => format!("Option<{}>", type_to_rust(&o.value_type)),
        ScSpecTypeDef::Result(r) => format!("Result<{}, {}>", type_to_rust(&r.ok_type), type_to_rust(&r.error_type)),
        ScSpecTypeDef::Vec(v) => format!("Vec<{}>", type_to_rust(&v.element_type)),
        ScSpecTypeDef::Map(m) => format!("Map<{}, {}>", type_to_rust(&m.key_type), type_to_rust(&m.value_type)),
        ScSpecTypeDef::Tuple(t) => {
            let inner: Vec<String> = t.value_types.iter().map(type_to_rust).collect();
            if inner.len() == 1 {
                format!("({},)", inner[0])
            } else {
                format!("({})", inner.join(", "))
            }
        }
        ScSpecTypeDef::BytesN(b) => format!("BytesN<{}>", b.n),
        ScSpecTypeDef::Udt(u) => u.name.to_utf8_string_lossy(),
    }
}

fn doc_of(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for a in attrs {
        if a.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &a.meta {
                if let Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) = &nv.value {
                    lines.push(s.value().trim().to_string());
                }
            }
        }
    }
    let mut d = lines.join("\n");
    d.truncate(1024);
    d
}

/// Build a spec entry from a Rust `struct` or `enum` definition, following
/// `#[contracttype]` rules: named struct -> UdtStructV0; tuple struct ->
/// UdtStructV0 with fields "0", "1", ..; enum with only explicit
/// discriminants -> UdtEnumV0; other enums -> UdtUnionV0 (unit and tuple
/// variants only).
pub fn spec_entry_from_derive(d: &syn::DeriveInput) -> Result<ScSpecEntry, TypeError> {
    let name = d.ident.to_string();
    let doc = doc_of(&d.attrs);
    match &d.data {
        syn::Data::Struct(s) => {
            let mut fields = Vec::new();
            match &s.fields {
                Fields::Named(n) => {
                    for f in &n.named {
                        fields.push(ScSpecUdtStructFieldV0 {
                            doc: sm::<1024>(&doc_of(&f.attrs))?,
                            name: sm::<30>(&f.ident.as_ref().unwrap().to_string())?,
                            type_: map_type(&f.ty, &mut Vec::new())?,
                        });
                    }
                }
                Fields::Unnamed(u) => {
                    for (i, f) in u.unnamed.iter().enumerate() {
                        fields.push(ScSpecUdtStructFieldV0 {
                            doc: sm::<1024>(&doc_of(&f.attrs))?,
                            name: sm::<30>(&i.to_string())?,
                            type_: map_type(&f.ty, &mut Vec::new())?,
                        });
                    }
                }
                Fields::Unit => return err(format!("unit struct {name} is not a contracttype")),
            }
            Ok(ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
                doc: sm(&doc)?,
                lib: sm("")?,
                name: sm(&name)?,
                fields: fields.try_into().map_err(|_| TypeError("too many fields".into()))?,
            }))
        }
        syn::Data::Enum(e) => {
            let all_int = !e.variants.is_empty() && e.variants.iter().all(|v| v.discriminant.is_some() && matches!(v.fields, Fields::Unit));
            if all_int {
                let mut cases = Vec::new();
                for v in &e.variants {
                    let value = match &v.discriminant.as_ref().unwrap().1 {
                        Expr::Lit(l) => lit_u32(&l.lit),
                        _ => None,
                    }
                    .ok_or_else(|| TypeError(format!("{}::{} needs an integer literal discriminant", name, v.ident)))?;
                    cases.push(ScSpecUdtEnumCaseV0 { doc: sm(&doc_of(&v.attrs))?, name: sm(&v.ident.to_string())?, value });
                }
                return Ok(ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
                    doc: sm(&doc)?,
                    lib: sm("")?,
                    name: sm(&name)?,
                    cases: cases.try_into().map_err(|_| TypeError("too many cases".into()))?,
                }));
            }
            let mut cases = Vec::new();
            for v in &e.variants {
                let cdoc = sm(&doc_of(&v.attrs))?;
                let cname = sm(&v.ident.to_string())?;
                match &v.fields {
                    Fields::Unit => cases.push(ScSpecUdtUnionCaseV0::VoidV0(ScSpecUdtUnionCaseVoidV0 { doc: cdoc, name: cname })),
                    Fields::Unnamed(u) => {
                        let ts = u.unnamed.iter().map(|f| map_type(&f.ty, &mut Vec::new())).collect::<Result<Vec<_>, _>>()?;
                        cases.push(ScSpecUdtUnionCaseV0::TupleV0(ScSpecUdtUnionCaseTupleV0 {
                            doc: cdoc,
                            name: cname,
                            type_: ts.try_into().map_err(|_| TypeError("too many tuple fields".into()))?,
                        }))
                    }
                    Fields::Named(_) => return err(format!("{name}::{}: named-field variants are not contracttypes", v.ident)),
                }
            }
            Ok(ScSpecEntry::UdtUnionV0(ScSpecUdtUnionV0 {
                doc: sm(&doc)?,
                lib: sm("")?,
                name: sm(&name)?,
                cases: cases.try_into().map_err(|_| TypeError("too many cases".into()))?,
            }))
        }
        syn::Data::Union(_) => err("Rust unions are not contracttypes"),
    }
}

/// Parse a type definition written as Rust source.
pub fn parse_type_def_str(s: &str) -> Result<ScSpecEntry, TypeError> {
    let d: syn::DeriveInput = syn::parse_str(s).map_err(|e| TypeError(format!("cannot parse type definition: {e}")))?;
    spec_entry_from_derive(&d)
}

/// Name of a UDT spec entry (None for functions and events).
pub fn udt_name(e: &ScSpecEntry) -> Option<String> {
    match e {
        ScSpecEntry::UdtStructV0(s) => Some(s.name.to_utf8_string_lossy()),
        ScSpecEntry::UdtUnionV0(u) => Some(u.name.to_utf8_string_lossy()),
        ScSpecEntry::UdtEnumV0(e) => Some(e.name.to_utf8_string_lossy()),
        ScSpecEntry::UdtErrorEnumV0(e) => Some(e.name.to_utf8_string_lossy()),
        _ => None,
    }
}

/// Render a UDT spec entry as Rust source (one line).
pub fn type_def_to_rust(e: &ScSpecEntry) -> Option<String> {
    match e {
        ScSpecEntry::UdtStructV0(s) => {
            let tuple = !s.fields.is_empty() && s.fields.iter().enumerate().all(|(i, f)| f.name.to_utf8_string_lossy() == i.to_string());
            if tuple {
                let fs: Vec<String> = s.fields.iter().map(|f| format!("pub {}", type_to_rust(&f.type_))).collect();
                Some(format!("pub struct {}({});", s.name.to_utf8_string_lossy(), fs.join(", ")))
            } else {
                let fs: Vec<String> =
                    s.fields.iter().map(|f| format!("pub {}: {}", f.name.to_utf8_string_lossy(), type_to_rust(&f.type_))).collect();
                Some(format!("pub struct {} {{ {} }}", s.name.to_utf8_string_lossy(), fs.join(", ")))
            }
        }
        ScSpecEntry::UdtUnionV0(u) => {
            let cs: Vec<String> = u
                .cases
                .iter()
                .map(|c| match c {
                    ScSpecUdtUnionCaseV0::VoidV0(v) => v.name.to_utf8_string_lossy(),
                    ScSpecUdtUnionCaseV0::TupleV0(t) => {
                        format!("{}({})", t.name.to_utf8_string_lossy(), t.type_.iter().map(type_to_rust).collect::<Vec<_>>().join(", "))
                    }
                })
                .collect();
            Some(format!("pub enum {} {{ {} }}", u.name.to_utf8_string_lossy(), cs.join(", ")))
        }
        ScSpecEntry::UdtEnumV0(en) => {
            let cs: Vec<String> = en.cases.iter().map(|c| format!("{} = {}", c.name.to_utf8_string_lossy(), c.value)).collect();
            Some(format!("pub enum {} {{ {} }}", en.name.to_utf8_string_lossy(), cs.join(", ")))
        }
        ScSpecEntry::UdtErrorEnumV0(en) => {
            let cs: Vec<String> = en.cases.iter().map(|c| format!("{} = {}", c.name.to_utf8_string_lossy(), c.value)).collect();
            Some(format!("pub enum {} {{ {} }}", en.name.to_utf8_string_lossy(), cs.join(", ")))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_roundtrip_through_rust_syntax() {
        for s in [
            "i128",
            "u32",
            "Address",
            "Vec<Address>",
            "Map<Symbol, i128>",
            "Option<AllowanceValue>",
            "BytesN<32>",
            "(Address, u32)",
            "(u32,)",
            "()",
            "Result<u32, Error>",
            "MuxedAddress",
            "Vec<Option<Map<Address, BytesN<4>>>>",
        ] {
            let t = parse_type_str(s).unwrap();
            assert_eq!(type_to_rust(&t), s, "{s}");
            assert_eq!(parse_type_str(&type_to_rust(&t)).unwrap(), t);
        }
    }

    #[test]
    fn records_udt_paths() {
        let t: Type = syn::parse_str("Vec<crate::storage::AllowanceValue>").unwrap();
        let mut u = Vec::new();
        map_type(&t, &mut u).unwrap();
        assert_eq!(u.len(), 1);
        assert_eq!(quote::quote!(#(#u)*).to_string().replace(' ', ""), "crate::storage::AllowanceValue");
    }

    #[test]
    fn type_defs_roundtrip() {
        for s in [
            "pub struct BalanceValue { pub amount: i128, pub authorized: bool, pub clawback: bool }",
            "pub struct Pair(pub Address, pub u32);",
            "pub enum AssetInfo { Native, AlphaNum4(AlphaNum4AssetInfo), AlphaNum12(AlphaNum12AssetInfo) }",
            "pub enum Level { Low = 1, High = 2 }",
        ] {
            let e = parse_type_def_str(s).unwrap();
            assert_eq!(type_def_to_rust(&e).unwrap(), s);
        }
    }

    #[test]
    fn rejects_generic_udt_and_named_variants() {
        assert!(parse_type_str("Foo<u32>").is_err());
        assert!(parse_type_def_str("enum A { B { x: u32 } }").is_err());
        assert!(parse_type_str("BytesN<N>").is_err());
    }
}
