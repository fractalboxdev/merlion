//! A small CSS cascade over a parsed SVG, for tests that compare *computed* values
//! rather than bytes (specs/svg-output.md#theming, #roles, #palette).
//!
//! It models what the embedded style needs: id, class, type and `[data-theme]`
//! selectors with descendant and child combinators, `:root`, `:where(…)`,
//! `:not(:is([data-theme="light"] *))`, specificity then source order, `@supports`
//! and `@media (prefers-color-scheme: dark)` blocks, inheritance, custom properties
//! with `var()` substitution and `initial`, presentation attributes, and
//! `color-mix(in oklab, …)` evaluated with the CSS Color 4 algorithm in `f64`.
#![allow(dead_code)]

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------
// DOM
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct El {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub parent: Option<usize>,
}

impl El {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn classes(&self) -> Vec<&str> {
        self.attr("class")
            .map(|c| c.split_ascii_whitespace().collect())
            .unwrap_or_default()
    }
    pub fn has_class(&self, c: &str) -> bool {
        self.classes().contains(&c)
    }
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Parses `svg` under a synthetic `<html>` root carrying `host` attributes. Element 0
/// is `html`; text and comments are skipped. The style text is returned separately.
pub fn parse_dom(svg: &str, host: &[(&str, &str)]) -> (Vec<El>, String) {
    let mut els = vec![El {
        tag: "html".into(),
        attrs: host
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect(),
        parent: None,
    }];
    let mut stack = vec![0usize];
    let mut style = String::new();
    let b = svg.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if svg[i..].starts_with("<!--") {
            i += svg[i..].find("-->").expect("comment end") + 3;
            continue;
        }
        if svg[i..].starts_with("</") {
            i += svg[i..].find('>').expect("close") + 1;
            stack.pop();
            continue;
        }
        let end = i + svg[i..].find('>').expect("tag end");
        let inner = &svg[i + 1..end];
        let self_close = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let (tag, rest) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        let mut attrs = Vec::new();
        let mut r = rest.trim();
        while let Some(eq) = r.find("=\"") {
            let name = r[..eq].trim().to_string();
            let after = &r[eq + 2..];
            let close = after.find('"').expect("attr end");
            attrs.push((name, unescape(&after[..close])));
            r = after[close + 1..].trim_start();
        }
        els.push(El {
            tag: tag.to_string(),
            attrs,
            parent: stack.last().copied(),
        });
        let idx = els.len() - 1;
        i = end + 1;
        if tag == "style" {
            let close = svg[i..].find("</style>").expect("style end");
            style = unescape(&svg[i..i + close]);
            i += close + "</style>".len();
            continue;
        }
        if !self_close {
            stack.push(idx);
        }
    }
    (els, style)
}

// ---------------------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Rule {
    pub selectors: String,
    pub decls: Vec<(String, String)>,
    pub supports: bool,
    pub dark: bool,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(a) = rest.find("/*") {
        out.push_str(&rest[..a]);
        rest = match rest[a + 2..].find("*/") {
            Some(b) => &rest[a + 2 + b + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote = false;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '"' => quote = !quote,
            '(' if !quote => depth += 1,
            ')' if !quote => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 && !quote {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

/// Parses rules, descending into `@supports` and `@media (prefers-color-scheme: dark)`.
pub fn parse_css(css: &str) -> Vec<Rule> {
    let css = strip_comments(css);
    let mut rules = Vec::new();
    let mut groups: Vec<(bool, bool)> = Vec::new(); // (supports, dark)
    let mut cur = String::new();
    let mut quote = false;
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            quote = !quote;
        }
        if quote {
            cur.push(c);
            continue;
        }
        match c {
            '{' => {
                let prelude = cur.trim().to_string();
                cur.clear();
                if prelude.starts_with("@supports") || prelude.starts_with("@media") {
                    let (s, d) = groups.last().copied().unwrap_or((false, false));
                    groups.push((
                        s || prelude.starts_with("@supports"),
                        d || prelude.contains("prefers-color-scheme: dark"),
                    ));
                    continue;
                }
                let mut body = String::new();
                let mut q = false;
                for c in chars.by_ref() {
                    if c == '"' {
                        q = !q;
                    }
                    if c == '}' && !q {
                        break;
                    }
                    body.push(c);
                }
                let decls = split_top(&body, ';')
                    .into_iter()
                    .filter_map(|d| {
                        let (p, v) = d.split_once(':')?;
                        Some((p.trim().to_string(), v.trim().to_string()))
                    })
                    .collect();
                let (supports, dark) = groups.last().copied().unwrap_or((false, false));
                if !prelude.starts_with('@') {
                    rules.push(Rule {
                        selectors: prelude,
                        decls,
                        supports,
                        dark,
                    });
                }
            }
            '}' => {
                groups.pop();
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    rules
}

/// (ids, classes and attributes, types)
type Spec = (u32, u32, u32);

fn add(a: Spec, b: Spec) -> Spec {
    (a.0 + b.0, a.1 + b.1, a.2 + b.2)
}

/// One compound selector.
#[derive(Clone, Debug, Default)]
struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    /// `[name]` or `[name="value"]`.
    attrs: Vec<(String, Option<String>)>,
    root: bool,
    /// `:not(:is([data-theme="light"] *))`
    not_light: bool,
}

fn parse_compound(s: &str) -> Compound {
    let mut c = Compound::default();
    let mut rest = s;
    if let Some(r) = rest.strip_prefix(":not(:is([data-theme=\"light\"] *))") {
        c.not_light = true;
        rest = r;
    }
    while !rest.is_empty() {
        let (kind, body) = rest.split_at(1);
        let stop = |t: &str| {
            t.find(['.', '#', '[', ':'])
                .map_or(t.len(), |i| if i == 0 { t.len() } else { i })
        };
        match kind {
            "." => {
                let n = stop(body);
                c.classes.push(body[..n].to_string());
                rest = &body[n..];
            }
            "#" => {
                let n = stop(body);
                c.id = Some(body[..n].to_string());
                rest = &body[n..];
            }
            "[" => {
                let n = body.find(']').expect("attr end");
                let inner = &body[..n];
                match inner.split_once('=') {
                    Some((a, v)) => c
                        .attrs
                        .push((a.to_string(), Some(v.trim_matches('"').to_string()))),
                    None => c.attrs.push((inner.to_string(), None)),
                }
                rest = &body[n + 1..];
            }
            ":" => {
                if let Some(r) = body.strip_prefix("not(:is([data-theme=\"light\"] *))") {
                    c.not_light = true;
                    rest = r;
                } else if let Some(r) = body.strip_prefix("root") {
                    c.root = true;
                    rest = r;
                } else if let Some(r) = body.strip_prefix("not([data-theme])") {
                    c.attrs.push(("!data-theme".into(), None));
                    rest = r;
                } else {
                    panic!("unsupported pseudo {:?}", s);
                }
            }
            _ => {
                let n = rest.find(['.', '#', '[', ':']).unwrap_or(rest.len());
                c.tag = Some(rest[..n].to_string());
                rest = &rest[n..];
            }
        }
    }
    c
}

fn compound_spec(c: &Compound) -> Spec {
    (
        c.id.is_some() as u32,
        (c.classes.len() + c.attrs.len() + c.root as usize + c.not_light as usize) as u32,
        c.tag.is_some() as u32,
    )
}

/// A complex selector, right-most compound last. Combinator before compound i (i ≥ 1):
/// `true` for child.
#[derive(Clone, Debug)]
struct Complex {
    parts: Vec<Compound>,
    child: Vec<bool>,
    spec: Spec,
}

fn parse_complex(s: &str, zero: bool) -> Complex {
    // Tokenise on whitespace and `>`, keeping brackets and parentheses intact.
    let mut parts = Vec::new();
    let mut child = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let mut pending_child = false;
    let flush =
        |cur: &mut String, parts: &mut Vec<Compound>, child: &mut Vec<bool>, pc: &mut bool| {
            if !cur.is_empty() {
                if !parts.is_empty() {
                    child.push(*pc);
                }
                parts.push(parse_compound(cur));
                cur.clear();
                *pc = false;
            }
        };
    for ch in s.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                cur.push(ch)
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(ch)
            }
            ' ' if depth == 0 => flush(&mut cur, &mut parts, &mut child, &mut pending_child),
            '>' if depth == 0 => {
                flush(&mut cur, &mut parts, &mut child, &mut pending_child);
                pending_child = true;
            }
            _ => cur.push(ch),
        }
    }
    flush(&mut cur, &mut parts, &mut child, &mut pending_child);
    let spec = if zero {
        (0, 0, 0)
    } else {
        parts
            .iter()
            .fold((0, 0, 0), |a, c| add(a, compound_spec(c)))
    };
    Complex { parts, child, spec }
}

/// Every complex selector of a selector list, with `:where(…)` expanded at zero specificity.
fn parse_selector_list(s: &str) -> Vec<Complex> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix(":where(").and_then(|r| r.strip_suffix(')')) {
        return split_top(inner, ',')
            .iter()
            .map(|p| parse_complex(p.trim(), true))
            .collect();
    }
    split_top(s, ',')
        .iter()
        .map(|p| parse_complex(p.trim(), false))
        .collect()
}

pub struct Engine {
    pub els: Vec<El>,
    rules: Vec<(Vec<Complex>, Rule)>,
    supports: bool,
    dark: bool,
}

impl Engine {
    /// `page` sheets precede the SVG's embedded style in source order.
    pub fn new(
        svg: &str,
        host: &[(&str, &str)],
        page: &[&str],
        supports: bool,
        dark: bool,
    ) -> Self {
        let (els, style) = parse_dom(svg, host);
        let mut rules = Vec::new();
        for css in page.iter().copied().chain(std::iter::once(style.as_str())) {
            for r in parse_css(css) {
                rules.push((parse_selector_list(&r.selectors), r));
            }
        }
        Engine {
            els,
            rules,
            supports,
            dark,
        }
    }

    fn matches_compound(&self, e: usize, c: &Compound) -> bool {
        let el = &self.els[e];
        if let Some(t) = &c.tag {
            if &el.tag != t {
                return false;
            }
        }
        if c.root && el.tag != "html" {
            return false;
        }
        if let Some(id) = &c.id {
            if el.attr("id") != Some(id.as_str()) {
                return false;
            }
        }
        if !c.classes.iter().all(|k| el.has_class(k)) {
            return false;
        }
        for (a, v) in &c.attrs {
            if a == "!data-theme" {
                if el.attr("data-theme").is_some() {
                    return false;
                }
                continue;
            }
            match (el.attr(a), v) {
                (None, _) => return false,
                (Some(x), Some(v)) if x != v => return false,
                _ => {}
            }
        }
        if c.not_light {
            let mut p = el.parent;
            while let Some(i) = p {
                if self.els[i].attr("data-theme") == Some("light") {
                    return false;
                }
                p = self.els[i].parent;
            }
        }
        true
    }

    fn matches_from(&self, e: usize, x: &Complex, k: usize) -> bool {
        if !self.matches_compound(e, &x.parts[k]) {
            return false;
        }
        if k == 0 {
            return true;
        }
        let child = x.child[k - 1];
        let mut p = self.els[e].parent;
        while let Some(a) = p {
            if self.matches_from(a, x, k - 1) {
                return true;
            }
            if child {
                return false;
            }
            p = self.els[a].parent;
        }
        false
    }

    fn matches(&self, e: usize, x: &Complex) -> bool {
        !x.parts.is_empty() && self.matches_from(e, x, x.parts.len() - 1)
    }

    /// The cascaded (specified) value of `prop` on `e`, if a rule sets it.
    fn cascaded(&self, e: usize, prop: &str) -> Option<String> {
        let mut best: Option<(Spec, usize, String)> = None;
        for (order, (sels, r)) in self.rules.iter().enumerate() {
            if (r.supports && !self.supports) || (r.dark && !self.dark) {
                continue;
            }
            let Some(v) = r
                .decls
                .iter()
                .rev()
                .find(|(p, _)| p == prop)
                .map(|d| d.1.clone())
            else {
                continue;
            };
            for x in sels {
                if self.matches(e, x) && best.as_ref().is_none_or(|b| (x.spec, order) >= (b.0, b.1))
                {
                    best = Some((x.spec, order, v.clone()));
                }
            }
        }
        best.map(|b| b.2)
    }

    /// The computed value of a custom property: `None` is the guaranteed-invalid value.
    pub fn custom(&self, e: usize, name: &str) -> Option<String> {
        self.custom_depth(e, name, 0)
    }

    fn custom_depth(&self, e: usize, name: &str, depth: u32) -> Option<String> {
        if depth > 32 {
            return None;
        }
        match self.cascaded(e, name) {
            Some(v) if v == "initial" => None,
            Some(v) => self.substitute(e, &v, depth + 1),
            None => self.els[e]
                .parent
                .and_then(|p| self.custom_depth(p, name, depth + 1)),
        }
    }

    /// `v` with every `var()` replaced; `None` when a reference without fallback fails.
    fn substitute(&self, e: usize, v: &str, depth: u32) -> Option<String> {
        let Some(at) = v.find("var(") else {
            return Some(v.to_string());
        };
        let body_start = at + 4;
        let mut d = 1;
        let mut end = body_start;
        for (i, c) in v[body_start..].char_indices() {
            match c {
                '(' => d += 1,
                ')' => {
                    d -= 1;
                    if d == 0 {
                        end = body_start + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &v[body_start..end];
        let (name, fallback) = match body.find(',') {
            Some(c) => (body[..c].trim(), Some(body[c + 1..].trim())),
            None => (body.trim(), None),
        };
        let value = match self.custom_depth(e, name, depth + 1) {
            Some(x) => x,
            None => self.substitute(e, fallback?, depth + 1)?,
        };
        let rest = self.substitute(e, &v[end + 1..], depth + 1)?;
        Some(format!("{}{}{}", &v[..at], value, rest))
    }

    /// The computed value of an ordinary property on `e`: cascade, then the presentation
    /// attribute, then inheritance for `fill`, `stroke` and `stroke-dasharray`. `Err` means
    /// the declaration was invalid at computed-value time.
    pub fn computed(&self, e: usize, prop: &str) -> Result<String, String> {
        if let Some(v) = self.cascaded(e, prop) {
            return self
                .substitute(e, &v, 0)
                .ok_or_else(|| format!("{} invalid at computed-value time: {}", prop, v));
        }
        if let Some(a) = self.els[e].attr(prop) {
            return Ok(a.to_string());
        }
        let inherited = matches!(prop, "fill" | "stroke" | "stroke-dasharray");
        match self.els[e].parent {
            Some(p) if inherited => self.computed(p, prop),
            _ => Ok(match prop {
                "fill" => "#000000".into(),
                "stroke" | "stroke-dasharray" => "none".into(),
                _ => String::new(),
            }),
        }
    }

    /// Element indices whose classes include `class`.
    pub fn with_class(&self, class: &str) -> Vec<usize> {
        (0..self.els.len())
            .filter(|&i| self.els[i].has_class(class))
            .collect()
    }

    /// The `<marker>` an edge path references with `marker-end`, if any.
    pub fn marker_path(&self, path: usize) -> Option<usize> {
        let r = self.els[path].attr("marker-end")?;
        let id = r.strip_prefix("url(#")?.strip_suffix(')')?;
        let m = (0..self.els.len()).find(|&i| self.els[i].attr("id") == Some(id))?;
        (0..self.els.len()).find(|&i| self.els[i].parent == Some(m))
    }
}

// ---------------------------------------------------------------------------------------
// Colour evaluation (CSS Color 4, f64 reference)
// ---------------------------------------------------------------------------------------

fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_gamma(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Björn Ottosson, "A perceptual color space for image processing" (2020).
pub fn oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let (r, g, b) = (to_linear(r), to_linear(g), to_linear(b));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

pub fn srgb([ll, a, b]: [f64; 3]) -> [f64; 3] {
    let l = (ll + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (ll - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (ll - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    [
        to_gamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
        to_gamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
        to_gamma(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
    ]
}

/// A colour as unit-range sRGB channels plus alpha.
pub type Rgba = [f64; 4];

fn hex(s: &str) -> Option<Rgba> {
    let h = s.strip_prefix('#')?;
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => (v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17, 255),
        6 => (v(0, 2)?, v(2, 2)?, v(4, 2)?, 255),
        8 => (v(0, 2)?, v(2, 2)?, v(4, 2)?, v(6, 2)?),
        _ => return None,
    };
    Some([r, g, b, a].map(|x| x as f64 / 255.0))
}

/// Evaluates a computed colour: a hex literal, `transparent`, or a (nested)
/// `color-mix(in oklab, A p%, B)`; premultiplied alpha as in CSS Color 5.
pub fn eval(v: &str) -> Option<Rgba> {
    let v = v.trim();
    if v == "transparent" {
        return Some([0.0; 4]);
    }
    if let Some(inner) = v
        .strip_prefix("color-mix(in oklab,")
        .and_then(|r| r.strip_suffix(')'))
    {
        let args = split_top(inner, ',');
        let [a, b] = args.as_slice() else {
            return None;
        };
        let (a, p) = a.trim().rsplit_once(' ')?;
        let p: f64 = p.strip_suffix('%')?.parse::<f64>().ok()? / 100.0;
        let (ca, cb) = (eval(a)?, eval(b)?);
        let (la, lb) = (oklab([ca[0], ca[1], ca[2]]), oklab([cb[0], cb[1], cb[2]]));
        let alpha = ca[3] * p + cb[3] * (1.0 - p);
        if alpha == 0.0 {
            return Some([0.0; 4]);
        }
        let m = [0, 1, 2].map(|i| (la[i] * ca[3] * p + lb[i] * cb[3] * (1.0 - p)) / alpha);
        let c = srgb(m);
        return Some([c[0], c[1], c[2], alpha]);
    }
    hex(v)
}

/// 8-bit channels of an evaluated colour.
pub fn rgba8(v: &str) -> Option<[u8; 4]> {
    eval(v).map(|c| c.map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8))
}

/// True when two computed values agree: colours to ±`tol` per 8-bit channel, anything
/// else by normalised text.
pub fn same(a: &str, b: &str, tol: u8) -> bool {
    match (rgba8(a), rgba8(b)) {
        (Some(x), Some(y)) => x.iter().zip(y.iter()).all(|(p, q)| p.abs_diff(*q) <= tol),
        _ => norm(a) == norm(b),
    }
}

fn norm(s: &str) -> String {
    s.replace(',', " ")
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `name → value` custom properties written as a `:root` rule, for host themes.
pub fn root_rule(tokens: &BTreeMap<&str, &str>) -> String {
    let mut s = String::from(":root{");
    for (k, v) in tokens {
        s.push_str(&format!("{}:{};", k, v));
    }
    s.push('}');
    s
}
