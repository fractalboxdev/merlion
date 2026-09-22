//! The stable-layout hint (specs/svg-output.md#layout-hint, specs/layout.md#stable-layout).
//!
//! Format: `v1;{direction};{layer}:{id},{id};…` with ids encoded by
//! [`crate::ids::encode_id`]. The hint is untrusted input: the parser accepts exactly
//! this grammar and rejects anything else, including duplicate layers or ids and hints
//! beyond the node, layer or byte limits.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use crate::ids::{decode_id, encode_id};
use crate::options::Direction;

const ATTR: &str = "data-merlion-layout=\"";

/// A parsed hint: the direction and, per source node id, its layer and index.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub direction: Direction,
    pub place: BTreeMap<String, (usize, usize)>,
}

/// The hint text inside `text`: the value of the first `data-merlion-layout="…"`
/// attribute when present (a whole previous SVG), else `text` itself, trimmed. An
/// unterminated attribute yields the empty string, which does not parse.
pub fn extract(text: &str) -> &str {
    match text.find(ATTR) {
        Some(at) => {
            let rest = &text[at + ATTR.len()..];
            match rest.find('"') {
                Some(end) => &rest[..end],
                None => "",
            }
        }
        None => text.trim(),
    }
}

fn parse_direction(s: &str) -> Option<Direction> {
    match s {
        "TB" => Some(Direction::TB),
        "BT" => Some(Direction::BT),
        "LR" => Some(Direction::LR),
        "RL" => Some(Direction::RL),
        _ => None,
    }
}

/// Parses a hint (bare, or a whole SVG carrying it). `None` for anything malformed, of
/// an unknown version, longer than `max_bytes`, or with more than `max_nodes` ids or a
/// layer number of `max_layers` or more.
pub fn parse(text: &str, max_nodes: usize, max_layers: usize, max_bytes: usize) -> Option<Hint> {
    let s = extract(text);
    if s.len() > max_bytes {
        return None;
    }
    let mut parts = s.split(';');
    if parts.next()? != "v1" {
        return None;
    }
    let direction = parse_direction(parts.next()?)?;
    let mut place = BTreeMap::new();
    let mut seen_layers = BTreeSet::new();
    for part in parts {
        let (num, ids) = part.split_once(':')?;
        if num.is_empty() || num.len() > 9 || !num.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let layer: usize = num.parse().ok()?;
        if layer >= max_layers || !seen_layers.insert(layer) {
            return None;
        }
        for (index, enc) in ids.split(',').enumerate() {
            if enc.is_empty() {
                return None;
            }
            let id = decode_id(enc)?;
            if place.insert(id, (layer, index)).is_some() || place.len() > max_nodes {
                return None;
            }
        }
    }
    Some(Hint { direction, place })
}

/// Writes a hint: `layers` holds node indices into `ids`, in order; empty layers are
/// left out. Indices out of range are skipped.
pub fn format(direction: Direction, layers: &[Vec<usize>], ids: &[&str]) -> String {
    let mut out = String::from("v1;");
    out.push_str(direction.as_str());
    for (l, layer) in layers.iter().enumerate() {
        let names: Vec<String> = layer
            .iter()
            .filter_map(|&v| ids.get(v))
            .map(|id| encode_id(id))
            .collect();
        if names.is_empty() {
            continue;
        }
        out.push(';');
        out.push_str(&alloc::format!("{}:", l));
        out.push_str(&names.join(","));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn p(s: &str) -> Option<Hint> {
        parse(s, 100, 100, 10_000)
    }

    #[test]
    fn parses_the_documented_example() {
        let h = p("v1;TB;0:a,b;1:c,d,e;2:f").unwrap();
        assert_eq!(h.direction, Direction::TB);
        assert_eq!(h.place.get("a"), Some(&(0, 0)));
        assert_eq!(h.place.get("b"), Some(&(0, 1)));
        assert_eq!(h.place.get("e"), Some(&(1, 2)));
        assert_eq!(h.place.get("f"), Some(&(2, 0)));
        assert_eq!(h.place.len(), 6);
    }

    #[test]
    fn decodes_encoded_ids() {
        let h = p("v1;LR;0:a__b,x_20y").unwrap();
        assert_eq!(h.direction, Direction::LR);
        assert!(h.place.contains_key("a_b"));
        assert!(h.place.contains_key("x y"));
    }

    #[test]
    fn extracts_from_a_whole_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" id="m1" data-merlion-version="0.1.0" data-merlion-layout="v1;RL;0:a;1:b"><title>x</title></svg>"#;
        assert_eq!(extract(svg), "v1;RL;0:a;1:b");
        let h = p(svg).unwrap();
        assert_eq!(h.direction, Direction::RL);
        assert_eq!(h.place.get("b"), Some(&(1, 0)));
        assert_eq!(extract("  v1;TB;0:a \n"), "v1;TB;0:a");
    }

    #[test]
    fn rejects_malformed_hints() {
        for bad in [
            "",
            "v2;TB;0:a",
            "v1;XX;0:a",
            "v1;TB;0:a;0:b",
            "v1;TB;0:a,a",
            "v1;TB;0:a;1:a",
            "v1;TB;x:a",
            "v1;TB;0a",
            "v1;TB;0:",
            "v1;TB;0:a,,b",
            "v1;TB;0:a;",
            "v1;TB;-1:a",
            "v1;TB;0:a_zz",
            "v1;TB;0:a;b",
            "v1;TB;0:a:b",
            "<svg data-merlion-layout=\"v1;TB;0:a",
            "v1;TB;1000:a",
            "v1;TB;99999999999999999999:a",
        ] {
            assert!(p(bad).is_none(), "accepted {:?}", bad);
        }
        assert!(p("v1;TB").is_some());
    }

    #[test]
    fn rejects_oversized_hints() {
        let many: alloc::vec::Vec<String> = (0..101).map(|i| alloc::format!("n{}", i)).collect();
        let s = alloc::format!("v1;TB;0:{}", many.join(","));
        assert!(p(&s).is_none());
        assert!(parse("v1;TB;0:abcdef", 100, 100, 8).is_none());
    }

    #[test]
    fn format_round_trips() {
        let ids = ["a", "b c", "d_e"];
        let s = format(Direction::BT, &[vec![0, 1], vec![2]], &ids);
        assert_eq!(s, "v1;BT;0:a,b_20c;1:d__e");
        let h = p(&s).unwrap();
        assert_eq!(h.place.get("b c"), Some(&(0, 1)));
    }
}
