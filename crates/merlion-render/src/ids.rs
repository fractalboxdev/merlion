//! Id hashing and encoding (specs/svg-output.md#ids-and-data-attributes).

use alloc::string::String;
use core::fmt::Write;

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Streaming FNV-1a 64 over several byte slices, each followed by a 0xff separator
/// (a byte that never occurs in UTF-8), so ("ab","c") and ("a","bc") differ.
pub fn fnv1a64_parts(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for &b in p.iter().chain(core::iter::once(&0xffu8)) {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// `m` + first 8 hex characters of the hash.
pub fn default_id(hash: u64) -> String {
    let mut s = String::from("m");
    let _ = write!(s, "{:08x}", (hash >> 32) as u32);
    s
}

/// `[a-z][a-z0-9-]{0,31}`
pub fn is_valid_id_prefix(p: &str) -> bool {
    let b = p.as_bytes();
    !b.is_empty()
        && b.len() <= 32
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// Injective encoding into `[A-Za-z0-9-_]`: letters, digits and `-` stay, `_` becomes `__`,
/// every other UTF-8 byte becomes `_` + two lowercase hex digits.
pub fn encode_id(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for &b in src.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' {
            out.push(b as char);
        } else if b == b'_' {
            out.push_str("__");
        } else {
            let _ = write!(out, "_{:02x}", b);
        }
    }
    out
}

/// Inverse of [`encode_id`]; `None` on malformed input.
pub fn decode_id(enc: &str) -> Option<String> {
    let b = enc.as_bytes();
    let mut out = alloc::vec::Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_alphanumeric() || c == b'-' {
            out.push(c);
            i += 1;
        } else if c == b'_' {
            if b.get(i + 1) == Some(&b'_') {
                out.push(b'_');
                i += 2;
            } else {
                let hex = enc.get(i + 1..i + 3)?;
                if !hex
                    .bytes()
                    .all(|h| h.is_ascii_digit() || (b'a'..=b'f').contains(&h))
                {
                    return None;
                }
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
        } else {
            return None;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_round_trips() {
        for s in ["a", "a_b", "a b", "héllo", "x;y:z,w", "_", "__x", "end-1"] {
            assert_eq!(decode_id(&encode_id(s)).as_deref(), Some(s));
        }
        assert_eq!(encode_id("a_b c"), "a__b_20c");
    }

    #[test]
    fn encoding_is_injective_on_tricky_pairs() {
        assert_ne!(encode_id("a_20"), encode_id("a "));
    }

    #[test]
    fn id_prefix_validation() {
        assert!(is_valid_id_prefix("m1-a"));
        assert!(!is_valid_id_prefix("1a"));
        assert!(!is_valid_id_prefix("A"));
        assert!(!is_valid_id_prefix(""));
    }

    #[test]
    fn fnv_known_vector() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
    }
}
