//! `click … href` URL rules (specs/svg-output.md#links, specs/security.md#output).

use alloc::string::String;

/// Returns the URL to emit, or `None` when it must be dropped with `W013 LinkRejected`.
///
/// The check follows the WHATWG URL parser's preprocessing so that it judges the
/// scheme a browser would see: ASCII tab, newline and carriage return are removed
/// everywhere, and leading and trailing C0 controls and spaces are trimmed. The
/// URL is then accepted when it is relative (no scheme) or its scheme, compared
/// case-insensitively, is `https`, `http` or `mailto`.
pub fn check_url(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c <= ' ');
    if trimmed.is_empty() {
        return None;
    }
    match scheme(trimmed) {
        None => Some(String::from(trimmed)),
        Some(s)
            if ["https", "http", "mailto"]
                .iter()
                .any(|ok| s.eq_ignore_ascii_case(ok)) =>
        {
            Some(String::from(trimmed))
        }
        Some(_) => None,
    }
}

/// The scheme of an absolute URL: `[A-Za-z][A-Za-z0-9+.-]*` followed by `:`.
/// A `/`, `?` or `#` before the first `:` makes the URL relative.
fn scheme(url: &str) -> Option<&str> {
    let colon = url.find(':')?;
    let candidate = url.get(..colon)?;
    let mut chars = candidate.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        // A leading non-letter (for example a C0 control inside the text) cannot
        // start a scheme; treat anything with a `:` that is not a clean scheme as absolute
        // unless a path, query or fragment delimiter comes first.
        return if candidate.contains(['/', '?', '#']) {
            None
        } else {
            Some(candidate)
        };
    }
    if chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
        Some(candidate)
    } else if candidate.contains(['/', '?', '#']) {
        None
    } else {
        // `a b:c` or `ja\u{1}vascript:` — not a valid scheme, but ambiguous; reject.
        Some(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_urls() {
        let cases = [
            ("https://example.com/a?b=c#d", "https://example.com/a?b=c#d"),
            ("http://example.com", "http://example.com"),
            ("HTTPS://EXAMPLE.COM", "HTTPS://EXAMPLE.COM"),
            ("mailto:a@example.com", "mailto:a@example.com"),
            ("/docs/page", "/docs/page"),
            ("page.html", "page.html"),
            ("../up", "../up"),
            ("#anchor", "#anchor"),
            ("?q=1", "?q=1"),
            ("//cdn.example.com/x", "//cdn.example.com/x"),
            ("docs/a:b", "docs/a:b"),
            ("ht\ttps://example.com", "https://example.com"),
            ("  https://example.com  ", "https://example.com"),
        ];
        for (input, want) in cases {
            assert_eq!(check_url(input).as_deref(), Some(want), "{input:?}");
        }
    }

    #[test]
    fn rejected_urls() {
        for input in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            " javascript:alert(1)",
            "\u{1}javascript:alert(1)",
            "data:text/html,<script>",
            "vbscript:x",
            "file:///etc/passwd",
            "ftp://example.com",
            "",
            "   ",
        ] {
            assert_eq!(check_url(input), None, "{input:?}");
        }
    }
}
