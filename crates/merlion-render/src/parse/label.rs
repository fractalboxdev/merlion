//! Label text: quoted strings, Mermaid entity codes, line breaks and truncation
//! (specs/parser.md#error-tolerance, specs/svg-output.md#text).

use alloc::string::String;
use alloc::vec::Vec;

/// A quoted string found in the source. Offsets are absolute byte positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quoted {
    pub content_start: usize,
    pub content_end: usize,
    /// Just past the closing quote.
    pub end: usize,
    /// Byte ranges of typographic delimiters (`R003`), opener first.
    pub typographic: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuoteScan {
    NotQuote,
    /// An opening quote without a closing one; `typographic` is true for `“ ” ‘`.
    Unterminated {
        typographic: bool,
    },
    Found(Quoted),
}

/// Characters that open a quoted string. `“ ” ‘` are typographic quotes used as
/// delimiters, which `R003` replaces with ASCII quotes.
pub fn is_quote_start(c: char) -> bool {
    matches!(c, '"' | '“' | '”' | '‘')
}

fn closers(open: char) -> &'static [char] {
    match open {
        '"' => &['"'],
        '“' => &['”', '“', '"'],
        '”' => &['”', '"'],
        '‘' => &['’', '‘'],
        _ => &[],
    }
}

/// Scans a quoted string starting at `pos`. Quoted strings may span lines (Mermaid's
/// Markdown strings do).
pub fn scan_quoted(src: &str, pos: usize) -> QuoteScan {
    let Some(open) = src.get(pos..).and_then(|r| r.chars().next()) else {
        return QuoteScan::NotQuote;
    };
    if !is_quote_start(open) {
        return QuoteScan::NotQuote;
    }
    let content_start = pos + open.len_utf8();
    let rest = src.get(content_start..).unwrap_or("");
    let close = closers(open);
    match rest.char_indices().find(|&(_, c)| close.contains(&c)) {
        None => QuoteScan::Unterminated {
            typographic: open != '"',
        },
        Some((i, c)) => {
            let content_end = content_start + i;
            let end = content_end + c.len_utf8();
            let mut typographic = Vec::new();
            if open != '"' {
                typographic.push((pos, content_start));
            }
            if c != '"' {
                typographic.push((content_end, end));
            }
            QuoteScan::Found(Quoted {
                content_start,
                content_end,
                end,
                typographic,
            })
        }
    }
}

/// Whether an unquoted label needs `R001`: Mermaid rejects `()`, `[]`, `{}` and `:`
/// inside an unquoted node label.
pub fn needs_quoting(raw: &str) -> bool {
    raw.contains(['(', ')', '[', ']', '{', '}', ':'])
}

/// The `R001` replacement for an unquoted label: the trimmed text in double quotes,
/// with any `"` written as the entity code `#quot;`.
pub fn quote_label(raw: &str) -> String {
    let mut s = String::from("\"");
    for c in raw.trim().chars() {
        if c == '"' {
            s.push_str("#quot;");
        } else {
            s.push(c);
        }
    }
    s.push('"');
    s
}

/// Normalises label text for the model:
/// - a Markdown string (`` "`…`" ``) loses its backticks;
/// - `<br>`, `<br/>`, `<br />` in any case, and a newline inside a quoted label,
///   become a `\n`, with each line trimmed;
/// - Mermaid entity codes (`#quot;`, `#35;`, `#x2665;`) are decoded in each line.
///
/// The lines are split before the codes are decoded, so `#lt;br#gt;` — the way a
/// source writes a literal `<br>` — stays text instead of becoming a break. A code
/// can never decode to a `\n`, because `decode_entity` refuses control characters,
/// so the separator the text stage splits on is unforgeable
/// (specs/parser.md#labels-and-entity-codes).
pub fn clean_label(raw: &str) -> String {
    let mut s = raw.trim();
    if s.len() >= 2 && s.starts_with('`') && s.ends_with('`') {
        s = s.get(1..s.len() - 1).unwrap_or("").trim();
    }
    let broken = normalise_br(s);
    let mut out = String::with_capacity(broken.len());
    for (i, line) in broken.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&decode_entities(line.trim()));
    }
    out
}

/// Rewrites every `<br>` variant (`<br/>`, `<br />`, `<BR>`) to a `\n`.
fn normalise_br(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut last = 0usize;
    while i < bytes.len() {
        if bytes.get(i) == Some(&b'<')
            && bytes
                .get(i + 1)
                .is_some_and(|b| b.eq_ignore_ascii_case(&b'b'))
            && bytes
                .get(i + 2)
                .is_some_and(|b| b.eq_ignore_ascii_case(&b'r'))
        {
            let mut j = i + 3;
            while bytes.get(j) == Some(&b' ') {
                j += 1;
            }
            if bytes.get(j) == Some(&b'/') {
                j += 1;
            }
            if bytes.get(j) == Some(&b'>') {
                out.push_str(s.get(last..i).unwrap_or(""));
                out.push('\n');
                i = j + 1;
                last = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(s.get(last..).unwrap_or(""));
    out
}

/// Named entity codes Mermaid labels commonly use (`#name;`).
const NAMED: &[(&str, char)] = &[
    ("amp", '&'),
    ("apos", '\''),
    ("bull", '•'),
    ("cent", '¢'),
    ("copy", '©'),
    ("darr", '↓'),
    ("deg", '°'),
    ("divide", '÷'),
    ("euro", '€'),
    ("ge", '≥'),
    ("gt", '>'),
    ("harr", '↔'),
    ("hearts", '♥'),
    ("hellip", '…'),
    ("infin", '∞'),
    ("laquo", '«'),
    ("larr", '←'),
    ("le", '≤'),
    ("lt", '<'),
    ("mdash", '—'),
    ("middot", '·'),
    ("nbsp", '\u{a0}'),
    ("ndash", '–'),
    ("ne", '≠'),
    ("para", '¶'),
    ("plusmn", '±'),
    ("pound", '£'),
    ("quot", '"'),
    ("raquo", '»'),
    ("rarr", '→'),
    ("reg", '®'),
    ("sect", '§'),
    ("times", '×'),
    ("trade", '™'),
    ("uarr", '↑'),
    ("yen", '¥'),
];

/// Decodes one entity body (between `#` and `;`), or `None` to keep it literal.
/// Control characters, surrogates and out-of-range code points stay literal.
fn decode_entity(body: &str) -> Option<char> {
    let code = if let Some(hex) = body.strip_prefix(['x', 'X']) {
        if hex.is_empty() || hex.len() > 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16).ok()?
    } else if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
        if body.len() > 7 {
            return None;
        }
        body.parse::<u32>().ok()?
    } else {
        return NAMED
            .binary_search_by(|(n, _)| n.cmp(&body))
            .ok()
            .and_then(|i| NAMED.get(i))
            .map(|&(_, c)| c);
    };
    let c = char::from_u32(code)?;
    if c.is_control() {
        return None;
    }
    Some(c)
}

pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('#') {
        out.push_str(rest.get(..i).unwrap_or(""));
        let after = rest.get(i + 1..).unwrap_or("");
        // Entity bodies are short; look for `;` within the next 10 bytes only.
        let semi = after
            .char_indices()
            .take(11)
            .find(|&(_, c)| c == ';')
            .map(|(j, _)| j);
        match semi.and_then(|j| after.get(..j).and_then(decode_entity).map(|c| (j, c))) {
            Some((j, c)) => {
                out.push(c);
                rest = after.get(j + 1..).unwrap_or("");
            }
            None => {
                out.push('#');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Truncates `s` to at most `max` bytes at a character boundary; returns whether it cut.
pub fn truncate(s: &mut String, max: usize) -> bool {
    if s.len() <= max {
        return false;
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_table_is_sorted() {
        assert!(NAMED.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn quoted_scans() {
        let q = |s: &str| scan_quoted(s, 0);
        assert_eq!(q("x"), QuoteScan::NotQuote);
        assert_eq!(q("\"abc"), QuoteScan::Unterminated { typographic: false });
        match q("“a’b” tail") {
            QuoteScan::Found(f) => {
                assert_eq!(&"“a’b” tail"[f.content_start..f.content_end], "a’b");
                assert_eq!(f.typographic.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        match q("\"a\nb\"") {
            QuoteScan::Found(f) => assert_eq!((f.content_end, f.end), (4, 5)),
            other => panic!("{other:?}"),
        }
        match q("“mixed\"") {
            QuoteScan::Found(f) => assert_eq!(f.typographic, alloc::vec![(0, 3)]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cleaning() {
        assert_eq!(clean_label("  a  "), "a");
        assert_eq!(clean_label("`**x**`"), "**x**");
        assert_eq!(clean_label("a\n  b\n c"), "a\nb\nc");
        assert_eq!(clean_label("a<br />b<BR/>c<br"), "a\nb\nc<br");
        // An escaped break is text, and splitting before decoding keeps it one line.
        assert_eq!(clean_label("a#lt;br#gt;b"), "a<br>b");
        assert_eq!(
            clean_label("#35;#quot;#amp;#x41;#65;#bogus;#"),
            "#\"&AA#bogus;#"
        );
        assert_eq!(clean_label("#1;#127;"), "#1;#127;");
    }

    #[test]
    fn quoting_and_truncation() {
        assert_eq!(
            quote_label(" say \"hi\" (x) "),
            "\"say #quot;hi#quot; (x)\""
        );
        assert!(needs_quoting("a:b") && needs_quoting("f(x)") && !needs_quoting("plain"));
        let mut s = String::from("aé");
        assert!(truncate(&mut s, 2));
        assert_eq!(s, "a");
        let mut s = String::from("ab");
        assert!(!truncate(&mut s, 2));
    }
}
