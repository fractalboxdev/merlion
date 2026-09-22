//! `check --fix`: applies `Repair` fixes to source text (specs/parser.md#diagnostics).

/// A byte range of the text to replace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Applies non-overlapping edits from the end of the text backwards, so earlier offsets
/// stay valid. An edit that overlaps one already applied, or whose range is reversed,
/// past the end, or not on a character boundary, is skipped. Returns the new text and the
/// number of edits applied.
pub fn apply(text: &str, edits: &[Edit]) -> (String, usize) {
    let mut order: Vec<&Edit> = edits
        .iter()
        .filter(|e| {
            e.start <= e.end
                && e.end <= text.len()
                && text.is_char_boundary(e.start)
                && text.is_char_boundary(e.end)
        })
        .collect();
    order.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
    let mut out = text.to_string();
    // Everything at or after `limit` has been rewritten; later edits must end before it.
    let mut limit = text.len();
    let mut applied = 0;
    for e in order {
        if e.end > limit {
            continue;
        }
        out.replace_range(e.start..e.end, &e.replacement);
        limit = e.start;
        applied += 1;
    }
    (out, applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(start: usize, end: usize, r: &str) -> Edit {
        Edit {
            start,
            end,
            replacement: r.into(),
        }
    }

    #[test]
    fn applies_in_any_input_order() {
        let (out, n) = apply("a “b” c", &[e(2, 5, "\""), e(6, 9, "\"")]);
        assert_eq!(out, "a \"b\" c");
        assert_eq!(n, 2);
        let (out, _) = apply("abc", &[e(0, 1, "X"), e(2, 3, "Z")]);
        assert_eq!(out, "XbZ");
    }

    #[test]
    fn skips_overlaps_and_invalid_ranges() {
        // The later edit (by start) wins; the overlapping earlier one is skipped.
        let (out, n) = apply("abcdef", &[e(1, 4, "X"), e(3, 5, "Y")]);
        assert_eq!(out, "abcYf");
        assert_eq!(n, 1);
        let (out, n) = apply("é", &[e(1, 2, "x"), e(2, 1, "x"), e(0, 99, "x")]);
        assert_eq!(out, "é");
        assert_eq!(n, 0);
    }

    #[test]
    fn insertions_at_one_point_both_apply() {
        let (out, n) = apply("ab", &[e(1, 1, "X"), e(1, 1, "Y")]);
        assert_eq!(n, 2);
        assert!(out == "aXYb" || out == "aYXb", "{out}");
    }
}
