//! Link URL re-check (specs/svg-output.md#links). The parser already validates `click …
//! href` URLs; the draw stage checks again so a hand-built or corrupted model cannot put
//! an active URL into the output.

use alloc::string::String;

/// The URL to write, or `None` when it is rejected.
///
/// ASCII tab, newline and carriage return are removed (as browsers do before parsing).
/// Browsers also trim leading and trailing C0 controls and spaces before reading the
/// scheme, so the scheme is checked on the trimmed text: `" javascript:…"` is rejected.
/// A URL is accepted when it has no scheme (relative) or its scheme, compared
/// case-insensitively, is `https`, `http` or `mailto`.
pub fn safe_href(url: &str) -> Option<String> {
    let cleaned: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c <= ' ');
    if trimmed.is_empty() {
        return None;
    }
    if let Some(scheme) = scheme_of(trimmed) {
        let ok = ["https", "http", "mailto"]
            .iter()
            .any(|s| scheme.eq_ignore_ascii_case(s));
        if !ok {
            return None;
        }
    }
    Some(String::from(trimmed))
}

/// The scheme per the WHATWG URL parser: `[A-Za-z][A-Za-z0-9+.-]*` followed by `:`.
/// Anything else before the first `:` makes the URL relative.
fn scheme_of(url: &str) -> Option<&str> {
    let colon = url.find(':')?;
    let s = url.get(..colon)?;
    let mut b = s.bytes();
    match b.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return None,
    }
    b.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-'))
        .then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_schemes_and_relative() {
        for u in [
            "https://example.com/a?b=c#d",
            "HTTP://x",
            "mailto:a@b.c",
            "/docs/page",
            "page.html",
            "#frag",
            "?q=1",
            "a/b:c",
        ] {
            assert_eq!(safe_href(u).as_deref(), Some(u), "{}", u);
        }
    }

    #[test]
    fn active_schemes_are_rejected() {
        for u in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            " javascript:alert(1)",
            "\u{1}javascript:alert(1)",
            "data:text/html,<script>",
            "vbscript:x",
            "file:///etc/passwd",
            "",
            "   ",
        ] {
            assert_eq!(safe_href(u), None, "{:?}", u);
        }
    }

    #[test]
    fn whitespace_is_removed_from_accepted_urls() {
        assert_eq!(
            safe_href("ht\ttps://exa\nmple.com").as_deref(),
            Some("https://example.com")
        );
    }
}
