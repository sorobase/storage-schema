// SPDX-License-Identifier: Apache-2.0
//! Static checks over a merged schema (SPEC.md section 7). The main one finds
//! key patterns that can address the same ledger entry: the sdk#2088 class
//! of collision, caught before deploy instead of in production.

use serde::Serialize;
use stellar_xdr::{ScSpecEntry, ScSpecTypeDef};

use crate::decode::{match_pattern, Decoder};
use crate::model::{KeyEntry, KeyPattern};
use crate::rusttype::type_to_rust;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub code: &'static str,
    pub entries: Vec<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlap {
    Yes,
    Maybe,
    No,
}

fn label(k: &KeyEntry) -> String {
    if k.lib.is_empty() {
        k.name.clone()
    } else {
        format!("{} ({})", k.name, k.lib)
    }
}

/// The ScVal kind a type encodes to, if fixed.
fn class(d: &Decoder, t: &ScSpecTypeDef) -> Option<&'static str> {
    use ScSpecTypeDef as T;
    Some(match t {
        T::Val | T::Option(_) | T::Result(_) => return None,
        T::Bool => "Bool",
        T::Void => "Void",
        T::Error => "Error",
        T::U32 => "U32",
        T::I32 => "I32",
        T::U64 => "U64",
        T::I64 => "I64",
        T::Timepoint => "Timepoint",
        T::Duration => "Duration",
        T::U128 => "U128",
        T::I128 => "I128",
        T::U256 => "U256",
        T::I256 => "I256",
        T::Bytes | T::BytesN(_) => "Bytes",
        T::String => "String",
        T::Symbol => "Symbol",
        T::Address | T::MuxedAddress => "Address",
        T::Vec(_) | T::Tuple(_) => "Vec",
        T::Map(_) => "Map",
        T::Udt(u) => match d.registry.get(&u.name.to_utf8_string_lossy()) {
            Some(ScSpecEntry::UdtStructV0(s)) => {
                if !s.fields.is_empty() && s.fields.iter().enumerate().all(|(i, f)| f.name.to_utf8_string_lossy() == i.to_string()) {
                    "Vec"
                } else {
                    "Map"
                }
            }
            Some(ScSpecEntry::UdtUnionV0(_)) => "Vec",
            Some(ScSpecEntry::UdtEnumV0(_)) => "U32",
            _ => return None,
        },
    })
}

fn types_overlap(d: &Decoder, a: &ScSpecTypeDef, b: &ScSpecTypeDef) -> Overlap {
    if a == b || matches!(a, ScSpecTypeDef::Val) || matches!(b, ScSpecTypeDef::Val) {
        return Overlap::Yes;
    }
    if let ScSpecTypeDef::Option(o) = a {
        return if matches!(b, ScSpecTypeDef::Void | ScSpecTypeDef::Option(_)) { Overlap::Yes } else { types_overlap(d, &o.value_type, b) };
    }
    if let ScSpecTypeDef::Option(_) = b {
        return types_overlap(d, b, a);
    }
    match (class(d, a), class(d, b)) {
        (Some(x), Some(y)) if x != y => Overlap::No,
        (Some("Address"), Some("Address")) => Overlap::Yes,
        _ => Overlap::Maybe,
    }
}

fn patterns_overlap(d: &Decoder, a: &KeyPattern, b: &KeyPattern) -> Overlap {
    use KeyPattern as P;
    let mut w = Vec::new();
    match (a, b) {
        (P::Exact(x), P::Exact(y)) => {
            if x == y {
                Overlap::Yes
            } else {
                Overlap::No
            }
        }
        (P::Exact(x), other) | (other, P::Exact(x)) => {
            if match_pattern(&d.registry, other, x, &mut w).is_some() {
                Overlap::Yes
            } else {
                Overlap::No
            }
        }
        (P::Tagged { tag: t1, fields: f1 }, P::Tagged { tag: t2, fields: f2 }) => {
            if t1 != t2 || f1.len() != f2.len() {
                return Overlap::No;
            }
            let mut acc = Overlap::Yes;
            for (x, y) in f1.iter().zip(f2) {
                match types_overlap(d, &x.type_, &y.type_) {
                    Overlap::No => return Overlap::No,
                    Overlap::Maybe => acc = Overlap::Maybe,
                    Overlap::Yes => {}
                }
            }
            acc
        }
        (P::Tagged { .. }, P::Typed(t)) | (P::Typed(t), P::Tagged { .. }) => match class(d, t) {
            None => Overlap::Maybe,
            Some("Vec") => Overlap::Maybe,
            Some(_) => Overlap::No,
        },
        (P::Typed(x), P::Typed(y)) => types_overlap(d, x, y),
    }
}

fn collect_udts(t: &ScSpecTypeDef, out: &mut Vec<String>) {
    use ScSpecTypeDef as T;
    match t {
        T::Udt(u) => out.push(u.name.to_utf8_string_lossy()),
        T::Option(o) => collect_udts(&o.value_type, out),
        T::Result(r) => {
            collect_udts(&r.ok_type, out);
            collect_udts(&r.error_type, out)
        }
        T::Vec(v) => collect_udts(&v.element_type, out),
        T::Map(m) => {
            collect_udts(&m.key_type, out);
            collect_udts(&m.value_type, out)
        }
        T::Tuple(tt) => tt.value_types.iter().for_each(|x| collect_udts(x, out)),
        _ => {}
    }
}

fn entry_udts(k: &KeyEntry) -> Vec<String> {
    let mut v = Vec::new();
    collect_udts(&k.value, &mut v);
    match &k.key {
        KeyPattern::Tagged { fields, .. } => fields.iter().for_each(|f| collect_udts(&f.type_, &mut v)),
        KeyPattern::Typed(t) => collect_udts(t, &mut v),
        KeyPattern::Exact(_) => {}
    }
    v
}

pub fn lint(d: &Decoder) -> Vec<Finding> {
    let keys: Vec<&KeyEntry> = d.key_entries().collect();
    let mut out = Vec::new();
    for (i, a) in keys.iter().enumerate() {
        for b in keys.iter().skip(i + 1) {
            if a.durability != b.durability {
                continue;
            }
            let ov = patterns_overlap(d, &a.key, &b.key);
            if ov == Overlap::No {
                continue;
            }
            let entries = vec![label(a), label(b)];
            if a.key.specificity() != b.key.specificity() {
                out.push(Finding {
                    severity: Severity::Info,
                    code: "shadowed",
                    entries,
                    message: format!(
                        "{} keys can match both; the more specific pattern wins by precedence (exact > tagged > typed)",
                        a.durability.as_str()
                    ),
                });
                continue;
            }
            let same_value = a.value == b.value;
            let (severity, code) = match ov {
                Overlap::Yes => (Severity::Error, "collision"),
                _ => (Severity::Warning, "possible_collision"),
            };
            out.push(Finding {
                severity,
                code,
                entries,
                message: format!(
                    "both address the same {} ledger entry when keys coincide (sdk#2088); value types {} {}",
                    a.durability.as_str(),
                    if same_value { "are equal" } else { "differ" },
                    if same_value {
                        "(silent sharing)".to_string()
                    } else {
                        format!("({} vs {}; whichever writes first breaks the other)", type_to_rust(&a.value), type_to_rust(&b.value))
                    }
                ),
            });
        }
        for u in entry_udts(a) {
            if !d.registry.contains(&u) {
                out.push(Finding {
                    severity: Severity::Warning,
                    code: "undefined_type",
                    entries: vec![label(a)],
                    message: format!(
                        "type {u} is defined neither in the storage schema nor in contractspecv0; decoders will render it untyped"
                    ),
                });
            }
        }
        if matches!(a.value, ScSpecTypeDef::Val) {
            out.push(Finding {
                severity: Severity::Info,
                code: "untyped_value",
                entries: vec![label(a)],
                message: "value type is Val; decoders cannot check or name its fields".into(),
            });
        }
    }
    for c in &d.registry.conflicts {
        out.push(Finding { severity: Severity::Warning, code: "type_conflict", entries: vec![], message: c.clone() });
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar;

    fn dec(keys: &str) -> Decoder {
        let mut d = Decoder::new();
        d.add_schema(&sidecar::parse(&format!(r#"{{"storage_schema":0,"keys":[{keys}]}}"#)).unwrap(), "t");
        d
    }

    #[test]
    fn finds_2088_collision() {
        let d = dec(r#"{"name":"A::Count","lib":"a","durability":"instance","key":{"tagged":{"tag":"Count"}},"value":"u32"},
               {"name":"B::Count","lib":"b","durability":"instance","key":{"tagged":{"tag":"Count"}},"value":"i128"}"#);
        let f = lint(&d);
        assert_eq!(f[0].code, "collision");
        assert!(f[0].message.contains("differ"));
    }

    #[test]
    fn different_durability_or_arity_is_fine() {
        let d = dec(r#"{"name":"A","durability":"instance","key":{"tagged":{"tag":"Count"}},"value":"u32"},
               {"name":"B","durability":"persistent","key":{"tagged":{"tag":"Count"}},"value":"u32"},
               {"name":"C","durability":"persistent","key":{"tagged":{"tag":"Count","fields":["Address"]}},"value":"u32"},
               {"name":"D","durability":"persistent","key":{"tagged":{"tag":"Count","fields":["u32"]}},"value":"u32"}"#);
        assert!(lint(&d).iter().all(|f| f.severity < Severity::Warning), "{:?}", lint(&d));
    }

    #[test]
    fn exact_vs_typed_is_info_and_undefined_type_warns() {
        let d = dec(r#"{"name":"X","durability":"instance","key":{"symbol":"X"},"value":"Thing"},
               {"name":"AnySym","durability":"instance","key":{"typed":"Symbol"},"value":"u32"}"#);
        let f = lint(&d);
        assert!(f.iter().any(|x| x.code == "shadowed" && x.severity == Severity::Info));
        assert!(f.iter().any(|x| x.code == "undefined_type"));
    }
}
