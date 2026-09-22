//! `%%{init: …}%%` directives: a JSON subset (specs/parser.md#front-matter-and-directives).
//!
//! Mermaid writes directive JSON loosely, so the parser also accepts single-quoted
//! strings, bare identifier keys and trailing commas. Nesting beyond the depth limit
//! and strings beyond the length limit are `E012 DirectiveTooLarge`.

use alloc::string::String;
use alloc::vec::Vec;

use super::config::{Entry, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonError {
    /// Nesting or string length over the limit (`E012`).
    TooLarge {
        start: usize,
        end: usize,
        message: String,
    },
    /// Not JSON.
    Malformed {
        start: usize,
        end: usize,
        message: String,
    },
}

/// A parsed directive: `init: {…}` has kind `init` and a value; `wrap` has none.
#[derive(Clone, Debug, PartialEq)]
pub struct Directive {
    pub kind: String,
    pub value: Option<Value>,
}

/// Parses the text between `%%{` and `}%%`; `base` is its byte offset in the source.
pub fn parse_directive(
    inner: &str,
    base: usize,
    max_depth: usize,
    max_string: usize,
) -> Result<Directive, JsonError> {
    let mut p = Json {
        s: inner,
        pos: 0,
        base,
        max_depth,
        max_string,
    };
    p.skip_ws();
    let kind_start = p.pos;
    while p
        .peek()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        p.pos += 1;
    }
    if p.pos == kind_start {
        return Err(p.malformed("expected a directive name such as `init`"));
    }
    let kind = String::from(inner.get(kind_start..p.pos).unwrap_or(""));
    p.skip_ws();
    let value = if p.peek() == Some(':') {
        p.pos += 1;
        Some(p.value(0)?)
    } else {
        None
    };
    p.skip_ws();
    if p.pos < inner.len() {
        return Err(p.malformed("unexpected text after the directive"));
    }
    Ok(Directive { kind, value })
}

struct Json<'a> {
    s: &'a str,
    pos: usize,
    base: usize,
    max_depth: usize,
    max_string: usize,
}

impl Json<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.pos..).and_then(|r| r.chars().next())
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn malformed(&self, message: &str) -> JsonError {
        JsonError::Malformed {
            start: self.base + self.pos,
            end: self.base + self.pos,
            message: String::from(message),
        }
    }

    fn too_large(&self, start: usize, message: String) -> JsonError {
        JsonError::TooLarge {
            start: self.base + start,
            end: self.base + self.pos,
            message,
        }
    }

    /// Parses one value. `depth` counts the containers already open; the check runs
    /// before descending, so recursion never goes deeper than `max_depth` frames.
    fn value(&mut self, depth: usize) -> Result<Value, JsonError> {
        self.skip_ws();
        match self.peek() {
            Some(c @ ('{' | '[')) => {
                if depth >= self.max_depth {
                    return Err(self.too_large(
                        self.pos,
                        alloc::format!("directive JSON nests deeper than {}", self.max_depth),
                    ));
                }
                self.pos += 1;
                if c == '{' {
                    self.object(depth + 1)
                } else {
                    self.array(depth + 1)
                }
            }
            Some('"' | '\'') => self.string().map(Value::Str),
            Some('-' | '0'..='9') => self.number(),
            Some(_) => {
                for (word, v) in [
                    ("true", Value::Bool(true)),
                    ("false", Value::Bool(false)),
                    ("null", Value::Null),
                ] {
                    if self.s.get(self.pos..).is_some_and(|r| r.starts_with(word)) {
                        self.pos += word.len();
                        return Ok(v);
                    }
                }
                Err(self.malformed("expected a JSON value"))
            }
            None => Err(self.malformed("expected a JSON value")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, JsonError> {
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some('}') {
                self.pos += 1;
                return Ok(Value::Map(entries));
            }
            let key_start = self.pos;
            let key = match self.peek() {
                Some('"' | '\'') => self.string()?,
                Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {
                    while self
                        .peek()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
                    {
                        self.pos += 1;
                    }
                    String::from(self.s.get(key_start..self.pos).unwrap_or(""))
                }
                _ => return Err(self.malformed("expected an object key")),
            };
            let key_end = self.pos;
            self.skip_ws();
            if self.peek() != Some(':') {
                return Err(self.malformed("expected `:` after the key"));
            }
            self.pos += 1;
            let value = self.value(depth)?;
            entries.push(Entry {
                key,
                start: self.base + key_start,
                end: self.base + key_end,
                value,
            });
            self.skip_ws();
            match self.peek() {
                Some(',') => self.pos += 1,
                Some('}') => {}
                _ => return Err(self.malformed("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, JsonError> {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(']') {
                self.pos += 1;
                return Ok(Value::List(items));
            }
            items.push(self.value(depth)?);
            self.skip_ws();
            match self.peek() {
                Some(',') => self.pos += 1,
                Some(']') => {}
                _ => return Err(self.malformed("expected `,` or `]`")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let mut v = 0u32;
        for _ in 0..4 {
            let d = self
                .peek()
                .and_then(|c| c.to_digit(16))
                .ok_or_else(|| self.malformed("invalid `\\u` escape"))?;
            self.pos += 1;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        let start = self.pos;
        let quote = self.peek().unwrap_or('"');
        self.pos += 1;
        let mut out = String::new();
        loop {
            if out.len() > self.max_string {
                return Err(self.too_large(
                    start,
                    alloc::format!("directive string longer than {} bytes", self.max_string),
                ));
            }
            let Some(c) = self.peek() else {
                return Err(self.malformed("unterminated string"));
            };
            self.pos += c.len_utf8();
            if c == quote {
                return Ok(out);
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            let Some(e) = self.peek() else {
                return Err(self.malformed("unterminated string"));
            };
            self.pos += e.len_utf8();
            match e {
                '"' | '\'' | '\\' | '/' => out.push(e),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hi = self.hex4()?;
                    let code = if (0xD800..0xDC00).contains(&hi) {
                        // A high surrogate must be followed by `\u` and a low surrogate.
                        let rest = self.s.get(self.pos..).unwrap_or("");
                        if !rest.starts_with("\\u") {
                            return Err(self.malformed("unpaired surrogate in `\\u` escape"));
                        }
                        self.pos += 2;
                        let lo = self.hex4()?;
                        if !(0xDC00..0xE000).contains(&lo) {
                            return Err(self.malformed("unpaired surrogate in `\\u` escape"));
                        }
                        0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                    } else {
                        hi
                    };
                    let ch = char::from_u32(code)
                        .ok_or_else(|| self.malformed("unpaired surrogate in `\\u` escape"))?;
                    out.push(ch);
                }
                _ => return Err(self.malformed("invalid escape")),
            }
        }
    }

    fn number(&mut self) -> Result<Value, JsonError> {
        let start = self.pos;
        let bytes = self.s.as_bytes();
        let mut i = self.pos;
        if bytes.get(i) == Some(&b'-') {
            i += 1;
        }
        let digits = |i: &mut usize| {
            let s = *i;
            while bytes.get(*i).is_some_and(u8::is_ascii_digit) {
                *i += 1;
            }
            *i - s
        };
        if digits(&mut i) == 0 {
            return Err(self.malformed("invalid number"));
        }
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            if digits(&mut i) == 0 {
                return Err(self.malformed("invalid number"));
            }
        }
        if matches!(bytes.get(i), Some(b'e' | b'E')) {
            i += 1;
            if matches!(bytes.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            if digits(&mut i) == 0 {
                return Err(self.malformed("invalid number"));
            }
        }
        self.pos = i;
        let text = self.s.get(start..i).unwrap_or("");
        text.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(Value::Num)
            .ok_or_else(|| self.malformed("invalid number"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Value {
        Value::Str(String::from(v))
    }

    fn entries(v: &Value) -> &[Entry] {
        match v {
            Value::Map(m) => m,
            other => panic!("expected a map, got {other:?}"),
        }
    }

    #[test]
    fn init_with_json_and_loose_json() {
        let d = parse_directive(
            "init: {\"theme\": \"dark\", 'flowchart': {'curve': 'basis', n: -1.5e2, t: true, f: false, z: null, l: [1, 'a',],},}",
            3,
            64,
            4096,
        )
        .unwrap();
        assert_eq!(d.kind, "init");
        let v = d.value.unwrap();
        let top = entries(&v);
        assert_eq!(top[0].key, "theme");
        assert_eq!(top[0].value, s("dark"));
        // Key ranges are absolute: `"theme"` starts at 3 + 7.
        assert_eq!((top[0].start, top[0].end), (10, 17));
        let fc = entries(&top[1].value);
        assert_eq!(fc[0].value, s("basis"));
        assert_eq!(fc[1].value, Value::Num(-150.0));
        assert_eq!(fc[2].value, Value::Bool(true));
        assert_eq!(fc[3].value, Value::Bool(false));
        assert_eq!(fc[4].value, Value::Null);
        assert_eq!(
            fc[5].value,
            Value::List(alloc::vec![Value::Num(1.0), s("a")])
        );
    }

    #[test]
    fn string_escapes() {
        let d = parse_directive(
            r#"init: {"a": "q\"\\\/\n\t\u00e9\ud83d\ude00"}"#,
            0,
            64,
            4096,
        )
        .unwrap();
        let v = d.value.unwrap();
        assert_eq!(entries(&v)[0].value, s("q\"\\/\n\té\u{1f600}"));
    }

    #[test]
    fn bare_directives() {
        let d = parse_directive(" wrap ", 0, 64, 4096).unwrap();
        assert_eq!(d.kind, "wrap");
        assert_eq!(d.value, None);
        let d = parse_directive("initialize: {}", 0, 64, 4096).unwrap();
        assert_eq!(d.kind, "initialize");
    }

    #[test]
    fn malformed() {
        for inner in [
            "init: {",
            "init: {\"a\" 1}",
            "init: {\"a\": }",
            "init: {\"a\": 1} x",
            "init: \"unterminated",
            "init: {\"a\": \"\\q\"}",
            "init: {\"a\": \"\\ud800\"}",
            "init: [1 2]",
            "init: tru",
            ": {}",
            "init: 01x",
        ] {
            match parse_directive(inner, 0, 64, 4096) {
                Err(JsonError::Malformed { .. }) => {}
                other => panic!("{inner:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn limits() {
        let deep = alloc::format!("init: {}1{}", "[".repeat(65), "]".repeat(65));
        assert!(matches!(
            parse_directive(&deep, 0, 64, 4096),
            Err(JsonError::TooLarge { .. })
        ));
        let ok = alloc::format!("init: {}1{}", "[".repeat(64), "]".repeat(64));
        assert!(parse_directive(&ok, 0, 64, 4096).is_ok());
        let long = alloc::format!("init: {{\"a\": \"{}\"}}", "x".repeat(4097));
        assert!(matches!(
            parse_directive(&long, 0, 64, 4096),
            Err(JsonError::TooLarge { .. })
        ));
        let fits = alloc::format!("init: {{\"a\": \"{}\"}}", "x".repeat(4096));
        assert!(parse_directive(&fits, 0, 64, 4096).is_ok());
        // Deep nesting far beyond the limit returns an error instead of exhausting the stack.
        let very_deep = alloc::format!("init: {}", "{\"a\":".repeat(100_000));
        assert!(matches!(
            parse_directive(&very_deep, 0, 64, 4096),
            Err(JsonError::TooLarge { .. })
        ));
    }
}
