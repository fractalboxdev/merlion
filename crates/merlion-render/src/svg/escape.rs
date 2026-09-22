//! XML escaping and character filtering (specs/svg-output.md#text, specs/security.md#output rule 4).
//!
//! The same function serves text content and attribute values: the five XML specials
//! become entities in both, so a value can never close an element or an attribute.

use alloc::string::String;

/// True for characters that never reach the output: control characters other than tab
/// and newline (C0, DEL and C1), the non-characters U+FFFE and U+FFFF, and the
/// bidirectional formatting characters U+202A–U+202E and U+2066–U+2069.
///
/// The text stage strips bidi controls with `W014`; they are dropped here as well so a
/// hand-built model cannot reorder a label either.
pub fn is_dropped(c: char) -> bool {
    match c {
        '\t' | '\n' => false,
        '\u{FFFE}' | '\u{FFFF}' => true,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => true,
        c => c.is_control(),
    }
}

/// Appends `s` to `out` with `&`, `<`, `>`, `"`, `'` as entities and dropped characters removed.
pub fn push_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if is_dropped(c) => {}
            c => out.push(c),
        }
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    push_escaped(&mut out, s);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_specials_become_entities() {
        assert_eq!(escape(r#"a&b<c>d"e'f"#), "a&amp;b&lt;c&gt;d&quot;e&#39;f");
    }

    #[test]
    fn hostile_markup_is_inert() {
        let e = escape("</text><script>alert(1)</script>");
        assert!(!e.contains('<') && !e.contains('>'));
        let e = escape(r#"" onload="x"#);
        assert!(!e.contains('"'));
    }

    #[test]
    fn controls_and_non_characters_are_dropped() {
        assert_eq!(escape("a\u{0}b\u{7}c\u{1b}d\u{7f}e\u{85}f"), "abcdef");
        assert_eq!(escape("a\u{FFFE}b\u{FFFF}c"), "abc");
        assert_eq!(escape("a\rb"), "ab");
    }

    #[test]
    fn tab_newline_and_unicode_survive() {
        assert_eq!(escape("a\tb\nc héllo 日本 👍"), "a\tb\nc héllo 日本 👍");
    }

    #[test]
    fn bidi_controls_are_dropped() {
        assert_eq!(escape("a\u{202E}b\u{2066}c\u{2069}"), "abc");
    }

    #[test]
    fn existing_entities_are_escaped_again() {
        assert_eq!(escape("&amp;"), "&amp;amp;");
    }
}
