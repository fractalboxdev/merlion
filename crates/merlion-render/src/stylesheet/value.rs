//! Tokens, selectors and values of the stylesheet subset (specs/svg-output.md#stylesheet).

use alloc::string::String;
use alloc::vec::Vec;

use crate::color::{oklab_to_rgba8, oklch_to_rgba8, Rgba8};
use crate::parse::style::{parse_color, parse_number};
use crate::svg::theme::Role;

/// A `classDef` colour token property: `--merlion-c-{name}-{fill|stroke|color}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClassProp {
    Fill,
    Stroke,
    Color,
}

impl ClassProp {
    pub fn name(self) -> &'static str {
        match self {
            ClassProp::Fill => "fill",
            ClassProp::Stroke => "stroke",
            ClassProp::Color => "color",
        }
    }
}

/// A token a theme block may set, in canonical output order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Token {
    Colour(Role),
    Series(u8),
    Stroke,
    Class(String, ClassProp),
}

impl Token {
    /// The property name without the `--merlion-` prefix.
    pub fn name(&self) -> String {
        match self {
            Token::Colour(r) => String::from(r.name()),
            Token::Series(n) => alloc::format!("series-{}", n),
            Token::Stroke => String::from("stroke"),
            Token::Class(c, p) => alloc::format!("c-{}-{}", c, p.name()),
        }
    }

    /// The value grammar of the token.
    pub fn kind(&self) -> Kind {
        match self {
            Token::Stroke => Kind::Stroke,
            Token::Class(_, ClassProp::Color) => Kind::Colour,
            Token::Class(..) => Kind::Paint,
            _ => Kind::Colour,
        }
    }
}

/// The value grammars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A colour.
    Colour,
    /// A colour or `none` (`classDef` fill and stroke).
    Paint,
    /// A number from 0 to 20, optionally `px`.
    Stroke,
    /// `none` or up to 8 numbers from 0 to 100.
    Dash,
}

/// What a declaration's name denotes.
#[derive(Clone, Debug, PartialEq)]
pub enum Name {
    Theme(Token),
    Tone,
    Dash,
    /// `--<ident>` outside the `--merlion-` namespace, usable through `var()` only.
    Private(String),
    /// Anything else: rejected with `W018`.
    Rejected,
}

/// `[A-Za-z_][A-Za-z0-9_-]{0,63}` (the `classDef` grammar).
pub fn is_class_name(n: &str) -> bool {
    crate::svg::is_valid_class_name(n)
}

/// `[a-z][a-z0-9-]{0,31}`.
pub fn is_theme_name(t: &str) -> bool {
    crate::ids::is_valid_id_prefix(t)
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// Classifies a declaration name.
pub fn classify(name: &str) -> Name {
    let Some(rest) = name.strip_prefix("--") else {
        return Name::Rejected;
    };
    let Some(t) = rest.strip_prefix("merlion-") else {
        return if is_ident(rest) {
            Name::Private(String::from(name))
        } else {
            Name::Rejected
        };
    };
    if let Some(r) = Role::from_name(t) {
        return Name::Theme(Token::Colour(r));
    }
    match t {
        "stroke" => return Name::Theme(Token::Stroke),
        "tone" => return Name::Tone,
        "dash" => return Name::Dash,
        _ => {}
    }
    if let Some(n) = t.strip_prefix("series-") {
        if let [d @ b'1'..=b'8'] = n.as_bytes() {
            return Name::Theme(Token::Series(d - b'0'));
        }
        return Name::Rejected;
    }
    if let Some(c) = t.strip_prefix("c-") {
        for p in [ClassProp::Fill, ClassProp::Stroke, ClassProp::Color] {
            if let Some(class) = c.strip_suffix(p.name()).and_then(|x| x.strip_suffix('-')) {
                if is_class_name(class) {
                    return Name::Theme(Token::Class(String::from(class), p));
                }
            }
        }
    }
    Name::Rejected
}

/// A dash pattern: `none` or 1 to 8 numbers from 0 to 100.
#[derive(Clone, Debug, PartialEq)]
pub enum Dash {
    None,
    Pattern(Vec<f64>),
}

/// A resolved literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Colour(Rgba8),
    /// `none` for a `classDef` fill or stroke token.
    NoPaint,
    Stroke(f64),
    Dash(Dash),
}

/// Why a value text was refused, before any grammar applies (`W018`).
pub fn forbidden(v: &str) -> Option<&'static str> {
    let lower = v.to_ascii_lowercase();
    if v.contains('\\') {
        return Some("CSS escapes are not accepted");
    }
    if v.contains('"') || v.contains('\'') {
        return Some("quoted strings are not accepted");
    }
    if lower.contains('!') {
        return Some("`!important` is not accepted");
    }
    if !v.is_ascii() {
        return Some("values are ASCII");
    }
    for f in [
        "url(",
        "color-mix(",
        "calc(",
        "env(",
        "attr(",
        "image-set(",
        "image(",
        "element(",
        "paint(",
        "src(",
    ] {
        if lower.contains(f) {
            return Some("only literal colours, numbers and `var()` are accepted");
        }
    }
    if v.contains(['{', '}', ';', '<', '>', '&', '@', '*']) {
        return Some("unexpected character");
    }
    let functional = ["rgb", "hsl", "oklab(", "oklch("]
        .iter()
        .any(|f| lower.starts_with(f));
    if v.contains('/') && !functional {
        return Some("unexpected character");
    }
    None
}

/// `var(--name)` or `var(--name, fallback)`: `Some((name, fallback))`. A nested `var()`
/// in the fallback is refused by the caller.
pub fn parse_var(v: &str) -> Option<(&str, Option<&str>)> {
    let inner = v.strip_prefix("var(")?.strip_suffix(')')?;
    let (name, fallback) = match inner.split_once(',') {
        Some((n, f)) => (n.trim(), Some(f.trim())),
        None => (inner.trim(), None),
    };
    if !name.starts_with("--") || !is_ident(&name[2..]) {
        return None;
    }
    Some((name, fallback))
}

fn number_or_pct(s: &str, pct_scale: f64) -> Option<f64> {
    let s = s.trim();
    match s.strip_suffix('%') {
        Some(p) => parse_number(p).map(|v| v / 100.0 * pct_scale),
        None => parse_number(s),
    }
}

/// `oklab(l a b [/ alpha])` and `oklch(l c h [/ alpha])`, space-separated.
fn parse_ok(v: &str) -> Option<Option<Rgba8>> {
    let lower = v.to_ascii_lowercase();
    let (lch, inner) = match lower.strip_prefix("oklab(") {
        Some(r) => (false, r),
        None => (true, lower.strip_prefix("oklch(")?),
    };
    let Some(inner) = inner.strip_suffix(')') else {
        return Some(None);
    };
    let (main, alpha) = match inner.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (inner, None),
    };
    let parts: Vec<&str> = main.split_ascii_whitespace().collect();
    let [l, x, y] = parts.as_slice() else {
        return Some(None);
    };
    let alpha = match alpha {
        None => 1.0,
        Some(a) => match number_or_pct(a, 1.0) {
            Some(a) if (0.0..=1.0).contains(&a) => a,
            _ => return Some(None),
        },
    };
    let Some(l) = number_or_pct(l, 1.0) else {
        return Some(None);
    };
    let Some(x) = number_or_pct(x, 0.4) else {
        return Some(None);
    };
    let y = if lch {
        let h = y.strip_suffix("deg").unwrap_or(y);
        parse_number(h)
    } else {
        number_or_pct(y, 0.4)
    };
    let Some(y) = y else {
        return Some(None);
    };
    Some(if lch {
        oklch_to_rgba8(l, x, y, alpha)
    } else {
        oklab_to_rgba8(l, x, y, alpha)
    })
}

/// A literal of `kind`, or why it is refused.
pub fn parse_literal(v: &str, kind: Kind) -> Result<Value, &'static str> {
    let v = v.trim();
    match kind {
        Kind::Colour | Kind::Paint => {
            if let Some(c) = parse_ok(v) {
                return c
                    .map(Value::Colour)
                    .ok_or("colour outside sRGB or malformed");
            }
            match parse_color(v) {
                Some(crate::model::Color::None) if kind == Kind::Paint => Ok(Value::NoPaint),
                Some(c) => Rgba8::from_color(&c)
                    .map(Value::Colour)
                    .ok_or("not a colour"),
                None => Err("not a colour"),
            }
        }
        Kind::Stroke => {
            let n = v.strip_suffix("px").unwrap_or(v);
            match parse_number(n.trim_end()) {
                Some(x) if (0.0..=20.0).contains(&x) => Ok(Value::Stroke(x)),
                _ => Err("not a number from 0 to 20"),
            }
        }
        Kind::Dash => {
            if v.eq_ignore_ascii_case("none") {
                return Ok(Value::Dash(Dash::None));
            }
            let mut out = Vec::new();
            for part in v
                .split([' ', ',', '\t', '\n', '\r'])
                .filter(|p| !p.is_empty())
            {
                match parse_number(part) {
                    Some(x) if (0.0..=100.0).contains(&x) => out.push(x),
                    _ => return Err("not a dash array"),
                }
                if out.len() > 8 {
                    return Err("more than 8 dash numbers");
                }
            }
            if out.is_empty() {
                return Err("not a dash array");
            }
            Ok(Value::Dash(Dash::Pattern(out)))
        }
    }
}

/// One selector of the subset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Root,
    Theme(String),
    AutoDark,
    Role {
        theme: Option<String>,
        cluster: bool,
        name: String,
    },
}

fn theme_attr(s: &str) -> Option<String> {
    let inner = s.strip_prefix("[data-theme=")?.strip_suffix(']')?;
    let t = inner
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| inner.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))?;
    is_theme_name(t).then(|| String::from(t))
}

fn role_class(s: &str) -> Option<(bool, String)> {
    if let Some(n) = s.strip_prefix(".merlion-cc-") {
        return is_class_name(n).then(|| (true, String::from(n)));
    }
    let n = s.strip_prefix(".merlion-c-")?;
    is_class_name(n).then(|| (false, String::from(n)))
}

/// Parses one selector (whitespace already collapsed to single spaces). `in_media` is
/// true inside `@media (prefers-color-scheme: dark)`.
pub fn parse_selector(s: &str, in_media: bool) -> Option<Selector> {
    if !s.is_ascii() {
        return None;
    }
    if in_media {
        return (s == ":root:not([data-theme])").then_some(Selector::AutoDark);
    }
    if s == ":root" {
        return Some(Selector::Root);
    }
    if let Some(t) = theme_attr(s) {
        return Some(Selector::Theme(t));
    }
    let parts: Vec<&str> = s.split(' ').collect();
    let (theme, rest) = match parts.as_slice() {
        [first, rest @ ..] if first.starts_with('[') => (Some(theme_attr(first)?), rest),
        rest => (None, rest),
    };
    let class = match rest {
        [c] | [".merlion", c] => *c,
        _ => return None,
    };
    let (cluster, name) = role_class(class)?;
    Some(Selector::Role {
        theme,
        cluster,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            classify("--merlion-bg"),
            Name::Theme(Token::Colour(Role::Bg))
        );
        assert_eq!(
            classify("--merlion-series-8"),
            Name::Theme(Token::Series(8))
        );
        assert_eq!(classify("--merlion-series-9"), Name::Rejected);
        assert_eq!(classify("--merlion-font"), Name::Rejected);
        assert_eq!(
            classify("--merlion-c-a-b-color"),
            Name::Theme(Token::Class("a-b".into(), ClassProp::Color))
        );
        assert_eq!(classify("--merlion-c--fill"), Name::Rejected);
        assert_eq!(classify("--brand"), Name::Private("--brand".into()));
        assert_eq!(classify("fill"), Name::Rejected);
        assert_eq!(classify("--"), Name::Rejected);
    }

    #[test]
    fn selectors() {
        assert_eq!(parse_selector(":root", false), Some(Selector::Root));
        assert_eq!(parse_selector(":root", true), None);
        assert_eq!(
            parse_selector("[data-theme='a-1']", false),
            Some(Selector::Theme("a-1".into()))
        );
        assert_eq!(
            parse_selector("[data-theme=\"d\"] .merlion .merlion-cc-x", false),
            Some(Selector::Role {
                theme: Some("d".into()),
                cluster: true,
                name: "x".into()
            })
        );
        assert_eq!(
            parse_selector(".merlion .merlion .merlion-c-x", false),
            None
        );
        assert_eq!(parse_selector(".merlion-c-x.merlion-c-y", false), None);
        assert_eq!(parse_selector("[data-theme=d]", false), None);
    }
}
