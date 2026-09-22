//! YAML front matter subset (specs/parser.md#front-matter-and-directives).
//!
//! Accepted: block mappings, plain and quoted scalars, and single-line flow
//! sequences. Anchors, aliases, tags, block sequences, block scalars, flow mappings,
//! multi-document markers and duplicate keys are rejected, which rules out
//! alias-expansion attacks. The caller reports a rejection as `E011`.

use alloc::string::String;
use alloc::vec::Vec;

use super::config::{Entry, Value};

/// Why the front matter falls outside the subset, with the offending byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontMatterError {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// Parses the text between the `---` delimiters. `base` is the byte offset of `body`
/// in the source; every range in the result is absolute.
pub fn parse_yaml(
    body: &str,
    base: usize,
    max_depth: usize,
) -> Result<Vec<Entry>, FrontMatterError> {
    // One frame per open block mapping. The stack depth is the nesting depth, and
    // the parser is iterative, so deep input cannot exhaust the call stack.
    let mut stack: Vec<Frame> = Vec::new();
    let mut offset = 0usize;
    for raw_line in body.split_inclusive('\n') {
        let line_abs = base + offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let lead_len = line.len() - line.trim_start_matches([' ', '\t']).len();
        let lead = line.get(..lead_len).unwrap_or("");
        let rest = line.get(lead_len..).unwrap_or("");
        if rest.trim().is_empty() || rest.starts_with('#') {
            continue;
        }
        let err = |s: usize, e: usize, message: &str| FrontMatterError {
            start: line_abs + s,
            end: line_abs + e,
            message: String::from(message),
        };
        let whole = err(lead_len, line.len(), "");
        let fail = |message: &str| {
            Err(FrontMatterError {
                message: String::from(message),
                ..whole.clone()
            })
        };
        if lead.contains('\t') {
            return Err(err(
                0,
                lead_len,
                "tab characters are not allowed in YAML indentation",
            ));
        }
        let indent = lead_len;
        if indent == 0 && (rest.starts_with("---") || rest.starts_with("...")) {
            return fail("multiple YAML documents are outside the accepted front matter subset");
        }
        if rest.starts_with('%') {
            return fail("YAML directives are outside the accepted front matter subset");
        }
        if rest == "-" || rest.starts_with("- ") {
            return fail("block sequences are outside the accepted front matter subset; use a flow sequence `[a, b]`");
        }
        if rest == "?" || rest.starts_with("? ") {
            return fail("complex keys are outside the accepted front matter subset");
        }

        // Resolve indentation against the open mappings.
        match stack.last() {
            None => stack.push(Frame::new(indent)),
            Some(top) if indent > top.indent => {
                if top.pending.is_none() {
                    return fail("unexpected indentation; multi-line plain scalars are outside the accepted front matter subset");
                }
                if stack.len() >= max_depth {
                    return Err(FrontMatterError {
                        message: alloc::format!(
                            "front matter nests deeper than {} levels",
                            max_depth
                        ),
                        ..whole
                    });
                }
                stack.push(Frame::new(indent));
            }
            Some(_) => {
                while stack.len() > 1 && stack.last().is_some_and(|f| indent < f.indent) {
                    close_top(&mut stack);
                }
                let Some(top) = stack.last_mut() else {
                    return fail("inconsistent indentation");
                };
                if top.indent != indent {
                    return fail("inconsistent indentation");
                }
                top.flush_pending();
            }
        }

        // `key: value`
        let rest_abs = line_abs + lead_len;
        let (key, k_start, k_end, after_key) = parse_key(rest, rest_abs)?;
        let Some(top) = stack.last_mut() else {
            return fail("inconsistent indentation");
        };
        if top.has_key(&key) {
            return Err(FrontMatterError {
                start: k_start,
                end: k_end,
                message: alloc::format!("duplicate key `{}`", key),
            });
        }
        let value_text = rest.get(after_key..).unwrap_or("");
        let trimmed = value_text.trim_start_matches([' ', '\t']);
        let value_abs = rest_abs + after_key + (value_text.len() - trimmed.len());
        if trimmed.trim_end().is_empty() || trimmed.starts_with('#') {
            top.pending = Some((key, k_start, k_end));
            continue;
        }
        let value = parse_block_value(trimmed, value_abs, max_depth)?;
        top.entries.push(Entry {
            key,
            start: k_start,
            end: k_end,
            value,
        });
    }
    while stack.len() > 1 {
        close_top(&mut stack);
    }
    Ok(match stack.pop() {
        Some(mut root) => {
            root.flush_pending();
            root.entries
        }
        None => Vec::new(),
    })
}

struct Frame {
    indent: usize,
    entries: Vec<Entry>,
    /// A key whose value is on the following, more indented lines.
    pending: Option<(String, usize, usize)>,
}

impl Frame {
    fn new(indent: usize) -> Self {
        Frame {
            indent,
            entries: Vec::new(),
            pending: None,
        }
    }

    /// A key with no nested lines has a null value.
    fn flush_pending(&mut self) {
        if let Some((key, start, end)) = self.pending.take() {
            self.entries.push(Entry {
                key,
                start,
                end,
                value: Value::Null,
            });
        }
    }

    fn has_key(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key == key)
            || self.pending.as_ref().is_some_and(|(k, _, _)| k == key)
    }
}

/// Pops the innermost mapping and stores it under its parent's pending key.
fn close_top(stack: &mut Vec<Frame>) {
    let Some(mut frame) = stack.pop() else {
        return;
    };
    frame.flush_pending();
    if let Some(parent) = stack.last_mut() {
        if let Some((key, start, end)) = parent.pending.take() {
            parent.entries.push(Entry {
                key,
                start,
                end,
                value: Value::Map(frame.entries),
            });
        }
    }
}

/// The message for a construct outside the subset that starts with `c`, if any.
fn rejected_indicator(c: char) -> Option<&'static str> {
    Some(match c {
        '&' => "anchors are outside the accepted front matter subset",
        '*' => "aliases are outside the accepted front matter subset",
        '!' => "tags are outside the accepted front matter subset",
        '|' | '>' => "block scalars are outside the accepted front matter subset",
        '{' => "flow mappings are outside the accepted front matter subset",
        '@' | '`' => "reserved YAML indicators cannot start a plain scalar",
        _ => return None,
    })
}

/// Parses `key:` at the start of `rest`; returns the key, its absolute range and the
/// byte offset in `rest` just past the `:`.
fn parse_key(rest: &str, abs: usize) -> Result<(String, usize, usize, usize), FrontMatterError> {
    let err = |message: String| FrontMatterError {
        start: abs,
        end: abs + rest.len(),
        message,
    };
    let first = rest.chars().next().unwrap_or(' ');
    if first == '"' || first == '\'' {
        let (key, used) = parse_quoted(rest, abs)?;
        let after = rest.get(used..).unwrap_or("");
        let trimmed = after.trim_start_matches([' ', '\t']);
        let colon_at = used + (after.len() - trimmed.len());
        if !trimmed.starts_with(':') {
            return Err(err(String::from("expected `key: value`")));
        }
        return Ok((key, abs, abs + used, colon_at + 1));
    }
    if let Some(m) = rejected_indicator(first) {
        return Err(err(String::from(m)));
    }
    if first == '[' {
        return Err(err(String::from(
            "flow sequences as keys are outside the accepted front matter subset",
        )));
    }
    let bytes = rest.as_bytes();
    let mut colon = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && i > 0 && matches!(bytes.get(i - 1), Some(b' ' | b'\t')) {
            break;
        }
        if b == b':' && matches!(bytes.get(i + 1), None | Some(b' ' | b'\t')) {
            colon = Some(i);
            break;
        }
    }
    let Some(colon) = colon else {
        return Err(err(String::from("expected `key: value`")));
    };
    let key = rest.get(..colon).unwrap_or("").trim_end();
    if key.is_empty() {
        return Err(err(String::from("expected `key: value`")));
    }
    Ok((String::from(key), abs, abs + key.len(), colon + 1))
}

/// Parses a value in block context: a quoted scalar, a flow sequence or a plain scalar,
/// optionally followed by a comment.
fn parse_block_value(text: &str, abs: usize, max_depth: usize) -> Result<Value, FrontMatterError> {
    let text = text.trim_end();
    let err = |message: &str| FrontMatterError {
        start: abs,
        end: abs + text.len(),
        message: String::from(message),
    };
    let first = text.chars().next().unwrap_or(' ');
    let (value, used) = match first {
        '"' | '\'' => {
            let (s, used) = parse_quoted(text, abs)?;
            (Value::Str(s), used)
        }
        '[' => parse_flow_seq(text, abs, 1, max_depth)?,
        '-' if text == "-" || text.starts_with("- ") => {
            return Err(err(
                "block sequences are outside the accepted front matter subset",
            ));
        }
        c => {
            if let Some(m) = rejected_indicator(c) {
                return Err(err(m));
            }
            let plain = strip_comment(text).trim_end();
            if plain.contains(": ") || plain.ends_with(':') {
                return Err(err(
                    "`: ` inside a plain scalar is outside the accepted front matter subset; quote the value",
                ));
            }
            return Ok(Value::Str(String::from(plain)));
        }
    };
    let tail = text.get(used..).unwrap_or("");
    let tail_trim = tail.trim_start_matches([' ', '\t']);
    if tail_trim.is_empty() || (tail_trim.starts_with('#') && tail_trim.len() < tail.len()) {
        Ok(value)
    } else {
        Err(err("unexpected text after the value"))
    }
}

/// Removes a ` # comment` suffix from a plain scalar.
fn strip_comment(text: &str) -> &str {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && i > 0 && matches!(bytes.get(i - 1), Some(b' ' | b'\t')) {
            return text.get(..i).unwrap_or(text);
        }
    }
    text
}

/// Parses a single- or double-quoted scalar at the start of `text`; returns the
/// decoded string and the bytes consumed.
fn parse_quoted(text: &str, abs: usize) -> Result<(String, usize), FrontMatterError> {
    let mut chars = text.char_indices();
    let quote = match chars.next() {
        Some((_, q @ ('"' | '\''))) => q,
        _ => {
            return Err(FrontMatterError {
                start: abs,
                end: abs + text.len(),
                message: String::from("expected a quoted scalar"),
            })
        }
    };
    let err = |at: usize, message: &str| FrontMatterError {
        start: abs + at,
        end: abs + text.len(),
        message: String::from(message),
    };
    let mut out = String::new();
    let mut chars = chars.peekable();
    while let Some((i, c)) = chars.next() {
        if c == quote {
            if quote == '\'' && chars.peek().map(|&(_, n)| n) == Some('\'') {
                // `''` is an escaped single quote.
                chars.next();
                out.push('\'');
                continue;
            }
            return Ok((out, i + 1));
        }
        if c == '\\' && quote == '"' {
            let Some((j, e)) = chars.next() else {
                break;
            };
            match e {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                'x' | 'u' => {
                    let n = if e == 'x' { 2 } else { 4 };
                    let mut v = 0u32;
                    for _ in 0..n {
                        let d = chars
                            .next()
                            .and_then(|(_, h)| h.to_digit(16))
                            .ok_or_else(|| err(j, "invalid escape in quoted scalar"))?;
                        v = v * 16 + d;
                    }
                    out.push(
                        char::from_u32(v)
                            .ok_or_else(|| err(j, "invalid escape in quoted scalar"))?,
                    );
                }
                _ => return Err(err(j, "invalid escape in quoted scalar")),
            }
            continue;
        }
        if c == '\n' {
            break;
        }
        out.push(c);
    }
    Err(err(0, "unterminated quoted scalar"))
}

/// Parses a flow sequence `[a, 'b', [c]]` that closes on the same line.
fn parse_flow_seq(
    text: &str,
    abs: usize,
    depth: usize,
    max_depth: usize,
) -> Result<(Value, usize), FrontMatterError> {
    let err = |at: usize, message: &str| FrontMatterError {
        start: abs + at,
        end: abs + text.len(),
        message: String::from(message),
    };
    if depth > max_depth {
        return Err(FrontMatterError {
            message: alloc::format!("front matter nests deeper than {} levels", max_depth),
            ..err(0, "")
        });
    }
    let bytes = text.as_bytes();
    let mut items = Vec::new();
    let mut i = 1usize; // past `[`
    loop {
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        match bytes.get(i) {
            None => return Err(err(0, "flow sequence is not closed on its line")),
            Some(b']') => return Ok((Value::List(items), i + 1)),
            _ => {}
        }
        let rest = text.get(i..).unwrap_or("");
        let first = rest.chars().next().unwrap_or(' ');
        let used = match first {
            '"' | '\'' => {
                let (s, used) = parse_quoted(rest, abs + i)?;
                items.push(Value::Str(s));
                used
            }
            '[' => {
                let (v, used) = parse_flow_seq(rest, abs + i, depth + 1, max_depth)?;
                items.push(v);
                used
            }
            c => {
                if let Some(m) = rejected_indicator(c) {
                    return Err(err(i, m));
                }
                let end = rest.find([',', ']', '[', '{', '}']).unwrap_or(rest.len());
                if rest.get(end..end + 1) == Some("[") || rest.get(end..end + 1) == Some("{") {
                    return Err(err(
                        i,
                        "flow mappings are outside the accepted front matter subset",
                    ));
                }
                let item = rest.get(..end).unwrap_or("").trim_end();
                items.push(Value::Str(String::from(item)));
                end
            }
        };
        i += used;
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        match bytes.get(i) {
            Some(b',') => i += 1,
            Some(b']') => return Ok((Value::List(items), i + 1)),
            _ => return Err(err(i, "expected `,` or `]` in flow sequence")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::Str(String::from(v))
    }

    fn get<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Value> {
        entries.iter().find(|e| e.key == key).map(|e| &e.value)
    }

    fn map(v: Option<&Value>) -> &[Entry] {
        match v {
            Some(Value::Map(m)) => m,
            other => panic!("expected a map, got {other:?}"),
        }
    }

    #[test]
    fn nested_block_mappings_and_scalars() {
        let body = "title: Hello world\nconfig:\n  layout: elk\n  flowchart:\n    curve: 'basis' # c\n  theme: \"dark\"\naccDescr: a: b\n";
        let r = parse_yaml(body, 0, 64);
        // `a: b` is not a plain scalar in YAML.
        assert!(r.is_err());
        let body = "title: Hello world\nconfig:\n  layout: elk\n  flowchart:\n    curve: 'basis' # c\n  theme: \"da\\\"rk\"\n# comment\n\naccTitle: x #y\n";
        let e = parse_yaml(body, 10, 64).unwrap();
        assert_eq!(get(&e, "title"), Some(&s("Hello world")));
        assert_eq!(get(&e, "accTitle"), Some(&s("x")));
        let config = map(get(&e, "config"));
        assert_eq!(get(config, "layout"), Some(&s("elk")));
        assert_eq!(get(config, "theme"), Some(&s("da\"rk")));
        let fc = map(get(config, "flowchart"));
        assert_eq!(get(fc, "curve"), Some(&s("basis")));
        // Key ranges are absolute.
        let t = &e[0];
        assert_eq!((t.start, t.end), (10, 15));
    }

    #[test]
    fn flow_sequences_and_empty_values() {
        let e = parse_yaml("a: [x, \"y z\", 'w']\nb:\nc: ''\nd: []", 0, 64).unwrap();
        assert_eq!(
            get(&e, "a"),
            Some(&Value::List(alloc::vec![s("x"), s("y z"), s("w")]))
        );
        assert_eq!(get(&e, "b"), Some(&Value::Null));
        assert_eq!(get(&e, "c"), Some(&s("")));
        assert_eq!(get(&e, "d"), Some(&Value::List(Vec::new())));
    }

    #[test]
    fn single_quote_escape_and_crlf() {
        let e = parse_yaml("t: 'it''s'\r\nu: v\r\n", 0, 64).unwrap();
        assert_eq!(get(&e, "t"), Some(&s("it's")));
        assert_eq!(get(&e, "u"), Some(&s("v")));
    }

    #[test]
    fn rejected_constructs() {
        let cases = [
            ("a: &anchor x", "anchor"),
            ("a: *alias", "alias"),
            ("a: !tag x", "tag"),
            ("a: |\n  text", "block scalar"),
            ("a: >\n  text", "block scalar"),
            ("a: {b: c}", "flow mapping"),
            ("a:\n  - x", "block sequence"),
            ("- x", "block sequence"),
            ("a: x\na: y", "duplicate"),
            ("a: x\n...\n", "document"),
            ("a: x\n---\nb: y", "document"),
            ("? a\n: b", "complex key"),
            ("a: b\n  c: d", "indentation"),
            ("a:\n\tb: c", "tab"),
            ("just text", "key: value"),
            ("a: [x, y", "flow sequence"),
            ("a: \"unterminated", "quoted"),
            ("a: [&x y]", "anchor"),
            ("&a b: c", "anchor"),
            ("a:\n    b: c\n  d: e", "indentation"),
        ];
        for (body, needle) in cases {
            match parse_yaml(body, 0, 64) {
                Ok(v) => panic!("{body:?} accepted as {v:?}"),
                Err(e) => assert!(
                    e.message.contains(needle),
                    "{body:?}: {} does not mention {needle}",
                    e.message
                ),
            }
        }
    }

    #[test]
    fn nesting_limit() {
        let mut body = String::new();
        for i in 0..70 {
            for _ in 0..i {
                body.push_str("  ");
            }
            body.push_str("k:\n");
        }
        let err = parse_yaml(&body, 0, 64).unwrap_err();
        assert!(err.message.contains("64"), "{}", err.message);
        let mut ok = String::new();
        for i in 0..64 {
            for _ in 0..i {
                ok.push(' ');
            }
            ok.push_str("k:\n");
        }
        assert!(parse_yaml(&ok, 0, 64).is_ok());
    }

    #[test]
    fn duplicate_keys_in_different_maps_are_fine() {
        assert!(parse_yaml("a:\n  x: 1\nb:\n  x: 2", 0, 64).is_ok());
    }
}
