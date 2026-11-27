//! RFC 8259 JSON reader for chart specs (specs/chart.md#input).
//!
//! Every value keeps its byte span, so a diagnostic points at the offending member.
//! Numbers keep their source lexeme: the chart reads decimals from the text the author
//! wrote, never from a value already rounded to binary. Nesting is bounded by
//! `max_depth`, so the recursion is bounded too. A duplicate object key is an error,
//! because two readers of the same document must not see two different charts.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;

/// The largest object and array shapes allowed by a chart: a row and the data array.
pub const MAX_MEMBERS: usize = 64;
pub const MAX_ITEMS: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Null,
    Bool(bool),
    /// The number exactly as written.
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self.kind {
            Kind::Null => "null",
            Kind::Bool(_) => "a boolean",
            Kind::Num(_) => "a number",
            Kind::Str(_) => "a string",
            Kind::Arr(_) => "an array",
            Kind::Obj(_) => "an object",
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            Kind::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError {
    pub at: usize,
    pub message: String,
}

/// The 1-based line and character column of byte `start`, as every Merlion span reports.
pub fn span(src: &str, start: usize, end: usize) -> Span {
    let start = start.min(src.len());
    let end = end.clamp(start, src.len());
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, c) in src.char_indices() {
        if i >= start {
            break;
        }
        if c == '\n' {
            line = line.saturating_add(1);
            col = 1;
        } else {
            col = col.saturating_add(1);
        }
    }
    Span {
        line,
        column: col,
        byte_start: u32::try_from(start).unwrap_or(u32::MAX),
        byte_end: u32::try_from(end).unwrap_or(u32::MAX),
    }
}

/// Indexed source positions for repeated diagnostics. ASCII columns need only line
/// starts; multibyte characters carry cumulative extra bytes for scalar columns.
pub(super) struct Positions<'a> {
    src: &'a str,
    lines: Vec<usize>,
    wide: Vec<(usize, usize)>,
}

impl<'a> Positions<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut lines = alloc::vec![0];
        let mut wide = Vec::new();
        let mut extra = 0usize;
        for (at, c) in src.char_indices() {
            if c == '\n' {
                lines.push(at + 1);
            }
            if c.len_utf8() > 1 {
                extra += c.len_utf8() - 1;
                wide.push((at + c.len_utf8(), extra));
            }
        }
        Self { src, lines, wide }
    }

    fn extra_bytes(&self, at: usize) -> usize {
        let n = self.wide.partition_point(|&(end, _)| end <= at);
        n.checked_sub(1).map_or(0, |i| self.wide[i].1)
    }

    pub fn span(&self, start: usize, end: usize) -> Span {
        let start = start.min(self.src.len());
        let end = end.clamp(start, self.src.len());
        let line = self.lines.partition_point(|&at| at <= start) - 1;
        let line_start = self.lines[line];
        let mut scalar_end = start;
        while !self.src.is_char_boundary(scalar_end) {
            scalar_end += 1;
        }
        let column =
            scalar_end - line_start - (self.extra_bytes(scalar_end) - self.extra_bytes(line_start))
                + 1;
        Span {
            line: u32::try_from(line + 1).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
            byte_start: u32::try_from(start).unwrap_or(u32::MAX),
            byte_end: u32::try_from(end).unwrap_or(u32::MAX),
        }
    }
}

/// Parses one JSON document. Leading and trailing whitespace is allowed; anything else
/// after the value is an error.
pub fn parse(src: &str, max_depth: usize) -> Result<Value, JsonError> {
    let mut p = Parser {
        src: src.as_bytes(),
        text: src,
        pos: 0,
        max_depth,
    };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.pos != p.src.len() {
        return Err(p.err("unexpected text after the JSON value"));
    }
    Ok(v)
}

struct Parser<'a> {
    src: &'a [u8],
    text: &'a str,
    pos: usize,
    max_depth: usize,
}

impl Parser<'_> {
    fn err(&self, message: &str) -> JsonError {
        JsonError {
            at: self.pos,
            message: String::from(message),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &str, kind: Kind) -> Result<Value, JsonError> {
        let start = self.pos;
        if self.src.get(start..start + word.len()) == Some(word.as_bytes()) {
            self.pos += word.len();
            Ok(Value {
                kind,
                start,
                end: self.pos,
            })
        } else {
            Err(self.err("expected a JSON value"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, JsonError> {
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => {
                let start = self.pos;
                let s = self.string()?;
                Ok(Value {
                    kind: Kind::Str(s),
                    start,
                    end: self.pos,
                })
            }
            Some(b't') => self.literal("true", Kind::Bool(true)),
            Some(b'f') => self.literal("false", Kind::Bool(false)),
            Some(b'n') => self.literal("null", Kind::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("expected a JSON value")),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn enter(&self, depth: usize) -> Result<usize, JsonError> {
        if depth >= self.max_depth {
            Err(self.err("nesting exceeds its limit"))
        } else {
            Ok(depth + 1)
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, JsonError> {
        let depth = self.enter(depth)?;
        let start = self.pos;
        self.pos += 1;
        let mut members: Vec<(String, Value)> = Vec::new();
        let mut names = BTreeSet::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Value {
                kind: Kind::Obj(members),
                start,
                end: self.pos,
            });
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return Err(self.err("expected a member name in double quotes"));
            }
            let key_at = self.pos;
            let key = self.string()?;
            if !names.insert(key.clone()) {
                return Err(JsonError {
                    at: key_at,
                    message: alloc::format!("duplicate member `{}`", crate::diag::excerpt(&key)),
                });
            }
            if members.len() == MAX_MEMBERS {
                return Err(JsonError {
                    at: key_at,
                    message: alloc::format!("object exceeds the limit of {} members", MAX_MEMBERS),
                });
            }
            self.ws();
            if !self.eat(b':') {
                return Err(self.err("expected `:` after the member name"));
            }
            self.ws();
            let v = self.value(depth)?;
            members.push((key, v));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value {
                    kind: Kind::Obj(members),
                    start,
                    end: self.pos,
                });
            }
            return Err(self.err("expected `,` or `}`"));
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, JsonError> {
        let depth = self.enter(depth)?;
        let start = self.pos;
        self.pos += 1;
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Value {
                kind: Kind::Arr(items),
                start,
                end: self.pos,
            });
        }
        loop {
            self.ws();
            if items.len() == MAX_ITEMS {
                return Err(self.err("array exceeds the limit of 10000 items"));
            }
            items.push(self.value(depth)?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value {
                    kind: Kind::Arr(items),
                    start,
                    end: self.pos,
                });
            }
            return Err(self.err("expected `,` or `]`"));
        }
    }

    fn digits(&mut self) -> usize {
        let from = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - from
    }

    /// `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?` (RFC 8259 §6).
    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.pos;
        self.eat(b'-');
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.err("expected a digit")),
        }
        if self.eat(b'.') && self.digits() == 0 {
            return Err(self.err("expected a digit after `.`"));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if self.digits() == 0 {
                return Err(self.err("expected a digit in the exponent"));
            }
        }
        Ok(Value {
            kind: Kind::Num(String::from(self.text.get(start..self.pos).unwrap_or(""))),
            start,
            end: self.pos,
        })
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut v = 0u32;
        for _ in 0..4 {
            let d = match self.peek() {
                Some(c @ b'0'..=b'9') => c - b'0',
                Some(c @ b'a'..=b'f') => c - b'a' + 10,
                Some(c @ b'A'..=b'F') => c - b'A' + 10,
                _ => return Err(self.err("expected four hex digits after `\\u`")),
            };
            v = v * 16 + u32::from(d);
            self.pos += 1;
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            let run = self.pos;
            while let Some(c) = self.peek() {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            // `pos` only advances over whole UTF-8 sequences: it stops at ASCII bytes.
            out.push_str(self.text.get(run..self.pos).unwrap_or(""));
            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let Some(e) = self.peek() else {
                        return Err(self.err("unterminated string"));
                    };
                    self.pos += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(self.err("unpaired surrogate in `\\u` escape"));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("unpaired surrogate in `\\u` escape"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err(self.err("unpaired surrogate in `\\u` escape"));
                            } else {
                                hi
                            };
                            match char::from_u32(code) {
                                Some(c) => out.push(c),
                                None => return Err(self.err("invalid `\\u` escape")),
                            }
                        }
                        _ => return Err(self.err("invalid escape in string")),
                    }
                }
                Some(_) => return Err(self.err("control character in string")),
                None => return Err(self.err("unterminated string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Value {
        parse(s, 64).expect(s)
    }

    #[test]
    fn numbers_keep_their_lexeme() {
        assert_eq!(ok("-3.150").kind, Kind::Num("-3.150".into()));
        assert_eq!(ok("1e3").kind, Kind::Num("1e3".into()));
        for bad in ["01", "1.", "-", ".5", "1e", "+1"] {
            assert!(parse(bad, 64).is_err(), "{bad}");
        }
    }

    #[test]
    fn strings_decode_escapes_and_reject_lone_surrogates() {
        assert_eq!(ok(r#""S&P 😀 é""#).kind, Kind::Str("S&P 😀 é".into()));
        assert!(parse(r#""\ud800""#, 64).is_err());
        assert!(parse("\"a\nb\"", 64).is_err());
    }

    #[test]
    fn objects_keep_order_and_reject_duplicates() {
        let Kind::Obj(m) = ok(r#"{"b":1,"a":[true,null]}"#).kind else {
            panic!()
        };
        assert_eq!(m[0].0, "b");
        assert_eq!(m[1].0, "a");
        let e = parse(r#"{"a":1,"a":2}"#, 64).unwrap_err();
        assert_eq!(e.at, 7);
        assert!(e.message.contains("duplicate"));
    }

    #[test]
    fn depth_and_trailing_text_are_bounded() {
        assert!(parse("[[[1]]]", 3).is_ok());
        assert!(parse("[[[[1]]]]", 3).is_err());
        assert!(parse("{} x", 64).is_err());
        assert!(parse("", 64).is_err());
    }

    #[test]
    fn spans_report_lines_and_columns() {
        let src = "{\n  \"a\": é1\n}";
        let s = span(src, 9, 10);
        assert_eq!((s.line, s.column), (2, 8));
    }

    #[test]
    fn indexed_spans_match_scalar_columns_at_every_byte() {
        for src in ["ascii", "é😀x\nαβ\n", "\n\né\r\nx"] {
            let positions = Positions::new(src);
            for start in 0..=src.len() + 2 {
                assert_eq!(
                    positions.span(start, start + 1),
                    span(src, start, start + 1),
                    "{src:?}:{start}"
                );
            }
        }
    }

    #[test]
    fn object_and_array_limits_reject_before_reading_the_excess_values() {
        let members: Vec<_> = (0..16_000)
            .map(|i| alloc::format!("\"k{}\":0", i))
            .collect();
        let src = alloc::format!("{{{}}}", members.join(","));
        let error = parse(&src, 64).unwrap_err();
        assert_eq!(error.at, src.find("\"k64\"").unwrap());
        assert!(error.message.contains("64 members"));
        assert!(parse(
            &alloc::format!("{{{}}}", members[..MAX_MEMBERS].join(",")),
            64
        )
        .is_ok());
        let duplicate = alloc::format!("{{{},\"k0\":1}}", members[..MAX_MEMBERS].join(","));
        assert!(parse(&duplicate, 64)
            .unwrap_err()
            .message
            .contains("duplicate"));

        let src = alloc::format!("[{}]", alloc::vec!["0"; MAX_ITEMS + 1].join(","));
        let error = parse(&src, 64).unwrap_err();
        assert_eq!(error.at, 1 + 2 * MAX_ITEMS);
    }
}
