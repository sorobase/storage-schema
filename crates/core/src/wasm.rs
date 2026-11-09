// SPDX-License-Identifier: Apache-2.0
//! Minimal Wasm section walker: list, read (concatenating same-name custom
//! sections), append and strip custom sections. No dependency on a Wasm
//! parser crate: only the binary framing (magic, version, `id size payload`)
//! is touched, so code sections are left byte-identical.

use std::ops::Range;

const MAGIC: [u8; 8] = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error("not a wasm module (bad magic or version)")]
    BadMagic,
    #[error("truncated or malformed wasm at byte {0}")]
    Malformed(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Section id (0 = custom).
    pub id: u8,
    /// Custom section name (None for non-custom sections).
    pub name: Option<String>,
    /// Whole section including id and size bytes.
    pub range: Range<usize>,
    /// Payload after the name (custom) or after the size (others).
    pub data: Range<usize>,
}

fn read_leb_u32(b: &[u8], pos: &mut usize) -> Result<u32, WasmError> {
    let start = *pos;
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = *b.get(*pos).ok_or(WasmError::Malformed(start))?;
        *pos += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return Err(WasmError::Malformed(start));
        }
    }
    u32::try_from(result).map_err(|_| WasmError::Malformed(start))
}

fn write_leb_u32(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// List all sections in module order.
pub fn sections(wasm: &[u8]) -> Result<Vec<Section>, WasmError> {
    if wasm.len() < 8 || wasm[..8] != MAGIC {
        return Err(WasmError::BadMagic);
    }
    let mut pos = 8;
    let mut out = Vec::new();
    while pos < wasm.len() {
        let start = pos;
        let id = wasm[pos];
        pos += 1;
        let size = read_leb_u32(wasm, &mut pos)? as usize;
        let body_start = pos;
        let end = body_start.checked_add(size).filter(|e| *e <= wasm.len()).ok_or(WasmError::Malformed(start))?;
        let (name, data_start) = if id == 0 {
            let mut p = body_start;
            let nlen = read_leb_u32(wasm, &mut p)? as usize;
            let nend = p.checked_add(nlen).filter(|e| *e <= end).ok_or(WasmError::Malformed(start))?;
            let name = String::from_utf8_lossy(&wasm[p..nend]).into_owned();
            (Some(name), nend)
        } else {
            (None, body_start)
        };
        out.push(Section { id, name, range: start..end, data: data_start..end });
        pos = end;
    }
    Ok(out)
}

/// Concatenation of every custom section named `name`, in module order.
/// `None` when no such section exists (distinct from an empty section).
pub fn read_custom(wasm: &[u8], name: &str) -> Result<Option<Vec<u8>>, WasmError> {
    let mut found = false;
    let mut out = Vec::new();
    for s in sections(wasm)? {
        if s.name.as_deref() == Some(name) {
            found = true;
            out.extend_from_slice(&wasm[s.data]);
        }
    }
    Ok(found.then_some(out))
}

/// Number of custom sections named `name` (the linker normally merges
/// same-name `#[link_section]` statics into one).
pub fn count_custom(wasm: &[u8], name: &str) -> Result<usize, WasmError> {
    Ok(sections(wasm)?.iter().filter(|s| s.name.as_deref() == Some(name)).count())
}

/// Remove every custom section named `name`.
pub fn strip_custom(wasm: &[u8], name: &str) -> Result<Vec<u8>, WasmError> {
    let secs = sections(wasm)?;
    let mut out = wasm[..8].to_vec();
    for s in secs {
        if s.name.as_deref() != Some(name) {
            out.extend_from_slice(&wasm[s.range]);
        }
    }
    Ok(out)
}

/// Append a custom section at the end of the module. Custom sections may
/// appear anywhere, so this never disturbs other sections.
pub fn append_custom(wasm: &[u8], name: &str, payload: &[u8]) -> Result<Vec<u8>, WasmError> {
    sections(wasm)?; // validate framing first
    let mut body = Vec::with_capacity(payload.len() + name.len() + 5);
    write_leb_u32(&mut body, name.len() as u32);
    body.extend_from_slice(name.as_bytes());
    body.extend_from_slice(payload);
    let mut out = wasm.to_vec();
    out.push(0);
    write_leb_u32(&mut out, body.len() as u32);
    out.extend(body);
    Ok(out)
}

/// Bytes a custom section of this payload costs in the module (id + LEB size
/// + LEB name length + name + payload).
pub fn custom_section_cost(name: &str, payload_len: usize) -> usize {
    let mut body = Vec::new();
    write_leb_u32(&mut body, name.len() as u32);
    let body_len = body.len() + name.len() + payload_len;
    let mut size = Vec::new();
    write_leb_u32(&mut size, body_len as u32);
    1 + size.len() + body_len
}

/// SHA-256 over every non-custom section, in order. Equal digests mean the
/// executable part of two modules is byte-identical.
pub fn code_digest(wasm: &[u8]) -> Result<String, WasmError> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for s in sections(wasm)? {
        if s.id != 0 {
            h.update(&wasm[s.range]);
        }
    }
    Ok(hex::encode(h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> Vec<u8> {
        // magic + version + type section (id 1) with zero types.
        let mut m = MAGIC.to_vec();
        m.extend([1, 1, 0]);
        m
    }

    #[test]
    fn append_read_strip_roundtrip() {
        let m = tiny();
        assert_eq!(read_custom(&m, "x").unwrap(), None);
        let m2 = append_custom(&m, "x", b"abc").unwrap();
        let m3 = append_custom(&m2, "x", b"def").unwrap();
        assert_eq!(count_custom(&m3, "x").unwrap(), 2);
        assert_eq!(read_custom(&m3, "x").unwrap().unwrap(), b"abcdef");
        assert_eq!(strip_custom(&m3, "x").unwrap(), m);
        assert_eq!(m2.len() - m.len(), custom_section_cost("x", 3));
        assert_eq!(code_digest(&m).unwrap(), code_digest(&m3).unwrap());
    }

    #[test]
    fn large_payload_leb() {
        let m = tiny();
        let payload = vec![7u8; 300];
        let m2 = append_custom(&m, "contractstoragev0", &payload).unwrap();
        assert_eq!(read_custom(&m2, "contractstoragev0").unwrap().unwrap(), payload);
        assert_eq!(m2.len() - m.len(), custom_section_cost("contractstoragev0", 300));
    }

    #[test]
    fn rejects_garbage() {
        assert!(sections(b"nope").is_err());
        let mut m = tiny();
        m.extend([0, 50, 1]); // custom section claiming 50 bytes
        assert!(sections(&m).is_err());
    }
}
