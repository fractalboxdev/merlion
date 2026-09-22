//! Repair helpers (specs/parser.md#error-tolerance): applying `Diagnostic.fix` edits to
//! source text, as `merlion check --fix` and editor quick fixes do, and recognising
//! Markdown code fences left in the source (`R006`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::{Diagnostic, Fix};

/// Applies every fix in `diags` to `source` and returns the edited text.
///
/// Fixes are applied from the end of the source backwards so earlier byte offsets stay
/// valid. A fix that overlaps one already applied is skipped, as is a fix whose range
/// is out of bounds or off a character boundary. Zero-width insertions at the same
/// offset are all applied, in diagnostic order.
pub fn apply_fixes(source: &str, diags: &[Diagnostic]) -> String {
    let mut fixes: Vec<(usize, &Fix)> = diags
        .iter()
        .filter_map(|d| d.fix.as_ref())
        .enumerate()
        .collect();
    // Descending by start, then by end; among equal ranges, later diagnostics first so
    // that insertions at one offset keep their diagnostic order in the output.
    fixes.sort_by(|(ia, a), (ib, b)| {
        b.span
            .byte_start
            .cmp(&a.span.byte_start)
            .then(b.span.byte_end.cmp(&a.span.byte_end))
            .then(ib.cmp(ia))
    });
    let mut out = String::from(source);
    let mut floor = usize::MAX; // start of the leftmost applied range
    for (_, fix) in fixes {
        let start = fix.span.byte_start as usize;
        let end = fix.span.byte_end as usize;
        if start > end
            || end > source.len()
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end)
            || end > floor
        {
            continue;
        }
        out.replace_range(start..end, &fix.replacement);
        floor = start;
    }
    out
}

/// Whether `line` (without its newline) is a Markdown code fence: optional
/// indentation, then three or more backticks or tildes, then an optional info string
/// such as `mermaid`.
pub fn is_fence_line(line: &str) -> bool {
    let t = line.trim();
    let marker = match t.chars().next() {
        Some(c @ ('`' | '~')) => c,
        _ => return false,
    };
    let run = t.chars().take_while(|&c| c == marker).count();
    if run < 3 {
        return false;
    }
    let info = t.get(run..).unwrap_or("").trim();
    info.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::{Severity, Span};

    fn fix(start: u32, end: u32, text: &str) -> Diagnostic {
        Diagnostic {
            severity: Severity::Repair,
            code: "R000",
            span: Span::default(),
            message: String::new(),
            fix: Some(Fix {
                span: Span {
                    line: 1,
                    column: 1,
                    byte_start: start,
                    byte_end: end,
                },
                replacement: String::from(text),
            }),
        }
    }

    #[test]
    fn applies_in_any_order() {
        let src = "abcdef";
        let d = [fix(0, 1, "A"), fix(4, 6, "EF!"), fix(2, 2, "+")];
        assert_eq!(apply_fixes(src, &d), "Ab+cdEF!");
    }

    #[test]
    fn insertions_at_one_offset_keep_order() {
        let d = [fix(3, 3, "1"), fix(3, 3, "2")];
        assert_eq!(apply_fixes("abc", &d), "abc12");
    }

    #[test]
    fn skips_overlaps_and_bad_ranges() {
        let d = [
            fix(1, 4, "X"),
            fix(2, 3, "Y"),
            fix(5, 99, "Z"),
            fix(2, 1, "W"),
        ];
        // The later-starting fix wins an overlap.
        assert_eq!(apply_fixes("abcdef", &d), "abYdef");
        // Off a char boundary.
        assert_eq!(apply_fixes("é", &[fix(1, 2, "x")]), "é");
    }

    #[test]
    fn fence_lines() {
        for l in [
            "```",
            "```mermaid",
            "  ```mermaid  ",
            "~~~",
            "````",
            "~~~ mermaid",
        ] {
            assert!(is_fence_line(l), "{l:?}");
        }
        for l in ["``", "A-->B", "```mermaid A-->B", "~~~~ x y", ""] {
            assert!(!is_fence_line(l), "{l:?}");
        }
    }
}
