//! Render options as a flat JSON object, parsed by hand (RFC 8259; zero dependencies).
//!
//! Accepted keys: `width`, `direction` (`"auto"` | `"source"`), `edgeStyle`, `font`,
//! `strict`, `idPrefix`, `hint`, `fuel`, and `palette`, the canonical palette string
//! (`merlion_render::stylesheet::Palette::parse`). `null` keeps the default; an unknown
//! key or a nested object or array is an error. The parser is iterative and never
//! recurses.

use merlion_render::stylesheet::Palette;
use merlion_render::{DirectionOption, EdgeStyle, FontMode, RenderOptions};

/// A scalar JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
}

/// Parses `{ "key": scalar, … }`. Empty input is an empty object.
pub fn parse_object(src: &[u8]) -> Result<Vec<(String, Value)>, String> {
    let mut p = Parser { src, pos: 0 };
    p.ws();
    if p.pos == src.len() {
        return Ok(Vec::new());
    }
    p.expect(b'{')?;
    let mut out = Vec::new();
    p.ws();
    if p.eat(b'}') {
        return p.end(out);
    }
    loop {
        p.ws();
        let key = p.string()?;
        p.ws();
        p.expect(b':')?;
        p.ws();
        let value = p.value()?;
        out.push((key, value));
        p.ws();
        if p.eat(b',') {
            continue;
        }
        p.expect(b'}')?;
        return p.end(out);
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
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

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.eat(b) {
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", b as char, self.pos))
        }
    }

    fn end<T>(&mut self, v: T) -> Result<T, String> {
        self.ws();
        if self.pos == self.src.len() {
            Ok(v)
        } else {
            Err(format!("trailing data at byte {}", self.pos))
        }
    }

    fn literal(&mut self, word: &[u8], v: Value) -> Result<Value, String> {
        if self.src.get(self.pos..self.pos + word.len()) == Some(word) {
            self.pos += word.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.literal(b"true", Value::Bool(true)),
            Some(b'f') => self.literal(b"false", Value::Bool(false)),
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'{' | b'[') => Err("options must be a flat object".into()),
            _ => Err(format!("expected a value at byte {}", self.pos)),
        }
    }

    /// RFC 8259 §6: `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        self.eat(b'-');
        let digits = |p: &mut Self| {
            let s = p.pos;
            while matches!(p.peek(), Some(b'0'..=b'9')) {
                p.pos += 1;
            }
            p.pos - s
        };
        if self.eat(b'0') {
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err("leading zero in number".into());
            }
        } else if digits(self) == 0 {
            return Err(format!("invalid number at byte {start}"));
        }
        if self.eat(b'.') && digits(self) == 0 {
            return Err(format!("invalid fraction at byte {start}"));
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if digits(self) == 0 {
                return Err(format!("invalid exponent at byte {start}"));
            }
        }
        core::str::from_utf8(&self.src[start..self.pos])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .map(Value::Num)
            .ok_or_else(|| format!("invalid number at byte {start}"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let s = self
            .src
            .get(self.pos..self.pos + 4)
            .and_then(|h| core::str::from_utf8(h).ok())
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("invalid \\u escape at byte {}", self.pos))?;
        self.pos += 4;
        Ok(s)
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                return Err("unterminated string".into());
            };
            self.pos += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = self.peek().ok_or("unterminated escape")?;
                    self.pos += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xd800..0xdc00).contains(&hi) {
                                // A high surrogate must be followed by `\u` + low surrogate.
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err("unpaired surrogate".into());
                                }
                                let lo = self.hex4()?;
                                if !(0xdc00..0xe000).contains(&lo) {
                                    return Err("unpaired surrogate".into());
                                }
                                0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                            } else {
                                hi
                            };
                            char::from_u32(cp).ok_or("unpaired surrogate")?
                        }
                        _ => return Err(format!("invalid escape at byte {}", self.pos)),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                0x00..=0x1f => return Err("control character in string".into()),
                _ => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| "string is not UTF-8".into())
    }
}

fn string_of<'a>(key: &str, v: &'a Value) -> Result<&'a str, String> {
    match v {
        Value::Str(s) => Ok(s),
        _ => Err(format!("`{key}` must be a string")),
    }
}

/// Builds render options from the options JSON.
pub fn render_options(src: &[u8]) -> Result<RenderOptions, String> {
    let mut o = RenderOptions::default();
    for (key, v) in parse_object(src)? {
        if v == Value::Null {
            continue;
        }
        match key.as_str() {
            "width" => match v {
                Value::Num(w) if w.is_finite() && w > 0.0 => o.target_width = w,
                _ => return Err("`width` must be a positive number".into()),
            },
            "direction" => {
                o.direction = match string_of(&key, &v)? {
                    "auto" => DirectionOption::Auto,
                    "source" => DirectionOption::FromSource,
                    d => return Err(format!("unknown direction `{d}`")),
                }
            }
            "edgeStyle" => {
                o.edge_style = match string_of(&key, &v)? {
                    "orthogonal" => EdgeStyle::Orthogonal,
                    "polyline" => EdgeStyle::Polyline,
                    "spline" => EdgeStyle::Spline,
                    s => return Err(format!("unknown edge style `{s}`")),
                }
            }
            "font" => {
                o.font = match string_of(&key, &v)? {
                    "link" => FontMode::Link,
                    "embed" => FontMode::Embed,
                    "system" => FontMode::System,
                    f => return Err(format!("unknown font mode `{f}`")),
                }
            }
            "strict" => match v {
                Value::Bool(b) => o.strict = b,
                _ => return Err("`strict` must be a boolean".into()),
            },
            "idPrefix" => o.id_prefix = Some(string_of(&key, &v)?.to_string()),
            "hint" => o.hint = Some(string_of(&key, &v)?.to_string()),
            "fuel" => match v {
                // Integers up to 2^53 are exact in f64 and in JavaScript.
                Value::Num(f)
                    if (0.0..=9_007_199_254_740_992.0).contains(&f) && f == (f as u64) as f64 =>
                {
                    o.fuel = f as u64
                }
                _ => return Err("`fuel` must be a whole number from 0 to 2^53".into()),
            },
            "palette" => {
                let p = Palette::parse(string_of(&key, &v)?)
                    .map_err(|why| format!("invalid `palette`: {why}"))?;
                o.palette = Some(p);
            }
            k => return Err(format!("unknown option `{k}`")),
        }
    }
    Ok(o)
}

/// `compileStylesheet` options: `(theme, autoDark, strict)`.
pub fn stylesheet_options(src: &[u8]) -> Result<(Option<String>, Option<String>, bool), String> {
    let (mut theme, mut auto_dark, mut strict) = (None, None, false);
    for (key, v) in parse_object(src)? {
        if v == Value::Null {
            continue;
        }
        match key.as_str() {
            "theme" => theme = Some(string_of(&key, &v)?.to_string()),
            "autoDark" => auto_dark = Some(string_of(&key, &v)?.to_string()),
            "strict" => match v {
                Value::Bool(b) => strict = b,
                _ => return Err("`strict` must be a boolean".into()),
            },
            k => return Err(format!("unknown option `{k}`")),
        }
    }
    Ok((theme, auto_dark, strict))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars_and_escapes() {
        let got = parse_object(
            br#" { "a" : 1.5e2, "b":true,"c":null, "d":"x\"\\\/\b\f\n\r\t\u00e9\ud83d\ude00", "e": -0 } "#,
        )
        .unwrap();
        assert_eq!(
            got,
            vec![
                ("a".into(), Value::Num(150.0)),
                ("b".into(), Value::Bool(true)),
                ("c".into(), Value::Null),
                ("d".into(), Value::Str("x\"\\/\u{8}\u{c}\n\r\té😀".into())),
                ("e".into(), Value::Num(-0.0)),
            ]
        );
        assert_eq!(parse_object(b"").unwrap(), vec![]);
        assert_eq!(parse_object(b"{}").unwrap(), vec![]);
    }

    #[test]
    fn rejects_malformed_json() {
        for bad in [
            &b"{"[..],
            b"[]",
            b"{\"a\":{}}",
            b"{\"a\":[1]}",
            b"{\"a\":1,}",
            b"{\"a\" 1}",
            b"{\"a\":01}",
            b"{\"a\":1.}",
            b"{\"a\":tru}",
            b"{\"a\":\"\\x\"}",
            b"{\"a\":\"\\ud800\"}",
            b"{\"a\":\"\x01\"}",
            b"{\"a\":1} x",
            b"{\"a\":\"\xff\"}",
            b"{a:1}",
        ] {
            assert!(
                parse_object(bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn maps_options() {
        let o = render_options(
            br#"{"width":640,"direction":"auto","edgeStyle":"spline","font":"system",
                "strict":true,"idPrefix":"d1","hint":"<svg/>","fuel":1000}"#,
        )
        .unwrap();
        assert_eq!(o.target_width, 640.0);
        assert_eq!(o.direction, DirectionOption::Auto);
        assert_eq!(o.edge_style, EdgeStyle::Spline);
        assert_eq!(o.font, FontMode::System);
        assert!(o.strict);
        assert_eq!(o.id_prefix.as_deref(), Some("d1"));
        assert_eq!(o.hint.as_deref(), Some("<svg/>"));
        assert_eq!(o.fuel, 1000);
    }

    #[test]
    fn nulls_keep_defaults_and_unknown_keys_are_errors() {
        let o = render_options(br#"{"width":null,"palette":null}"#).unwrap();
        assert_eq!(o, RenderOptions::default());
        assert_eq!(render_options(b"").unwrap(), RenderOptions::default());
        let e = render_options(br#"{"zoom":3}"#).unwrap_err();
        assert!(e.contains("`zoom`"), "{e}");
    }

    #[test]
    fn palette_is_the_canonical_string() {
        let o = render_options(br#"{"palette":"palette-v1|bg=#000000;c-a:#ff0000/4 2;"}"#).unwrap();
        let p = o.palette.expect("palette");
        assert_eq!(
            p.light.colour("bg").map(|c| c.to_hex()).as_deref(),
            Some("#000000")
        );
        for bad in [
            &br#"{"palette":"palette-v1|bg=red;"}"#[..],
            br#"{"palette":"bg=#000000;"}"#,
            br#"{"palette":1}"#,
        ] {
            let e = render_options(bad).unwrap_err();
            assert!(e.contains("palette"), "{e}");
        }
    }

    #[test]
    fn rejects_wrong_types_and_ranges() {
        for bad in [
            &br#"{"width":0}"#[..],
            br#"{"width":"1"}"#,
            br#"{"direction":"up"}"#,
            br#"{"edgeStyle":1}"#,
            br#"{"font":"bold"}"#,
            br#"{"strict":1}"#,
            br#"{"fuel":-1}"#,
            br#"{"fuel":1.5}"#,
            br#"{"fuel":1e300}"#,
            br#"{"idPrefix":false}"#,
        ] {
            assert!(
                render_options(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }
}
