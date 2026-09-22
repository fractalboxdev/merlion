//! The stylesheet compiler (specs/svg-output.md#stylesheet, specs/adr/0009-stylesheet.md).
//!
//! A stylesheet is a CSS subset that sets tokens. [`compile`] parses it into a typed
//! [`Stylesheet`]; nothing from the input is passed through as text. The model has two
//! emitters: [`Stylesheet::to_css`] writes page CSS with fixed selector shapes and
//! literal values only, and [`Stylesheet::palette`] resolves the literals one theme
//! bakes into a standalone SVG ([`Palette`], `RenderOptions::palette`).
//!
//! Parsing and resolution take `O(n log n)` in the input: the block structure is scanned
//! with an explicit depth counter, each theme's declarations are indexed once in a sorted
//! map shared by the theme block and all its role rules, `var()` references resolve
//! memoised to a depth of 8, and the input is capped at 64 KiB. Nothing here panics on
//! any input.

mod scan;
pub mod value;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::color::Rgba8;
use crate::diag::{Diagnostics, Severity, Span};
use crate::numfmt::push_num;
use crate::svg::theme::{Role, Table};

pub use value::{ClassProp, Dash, Token};
use value::{Kind, Name, Selector, Value};

/// Stylesheet limits (specs/architecture.md#boundaries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StylesheetLimits {
    pub bytes: usize,
    pub rules: usize,
    pub declarations: usize,
    /// Selectors per list, in rules that declare a token.
    pub selectors: usize,
    pub themes: usize,
    pub role_selectors: usize,
    pub depth: usize,
    pub var_depth: usize,
    pub diagnostics: usize,
}

impl Default for StylesheetLimits {
    fn default() -> Self {
        StylesheetLimits {
            bytes: 64 * 1024,
            rules: 512,
            declarations: 32,
            selectors: 8,
            themes: 16,
            role_selectors: 256,
            depth: 2,
            var_depth: 8,
            diagnostics: 100,
        }
    }
}

/// Which theme block a declaration belongs to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThemeKey {
    Root,
    Named(String),
    /// `:root:not([data-theme])` inside `@media (prefers-color-scheme: dark)`.
    AutoDark,
}

/// A theme block: resolved token values in canonical order.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeBlock {
    pub key: ThemeKey,
    pub decls: Vec<(Token, Value)>,
}

/// A role rule: `--merlion-tone` and `--merlion-dash` for `.merlion-c-{name}` (nodes,
/// edges) or `.merlion-cc-{name}` (clusters), optionally under a named theme.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleRule {
    pub theme: Option<String>,
    /// Under the automatic dark theme (`:root:not([data-theme])` inside the dark media
    /// block); `theme` is `None`. Page CSS only: palettes never read it.
    pub auto_dark: bool,
    pub cluster: bool,
    pub name: String,
    pub tone: Option<Rgba8>,
    pub dash: Option<Dash>,
}

/// A compiled stylesheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stylesheet {
    /// `Root` first, then named themes in first-use order, then `AutoDark`.
    pub themes: Vec<ThemeBlock>,
    /// In source order.
    pub roles: Vec<RoleRule>,
}

// ---------------------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------------------

struct Diags<'a> {
    src: &'a str,
    out: Diagnostics,
    limit: usize,
    dropped: usize,
    /// The code of the first diagnostic past the limit, which the summary carries.
    dropped_code: &'static str,
}

impl Diags<'_> {
    fn span(&self, start: usize, end: usize) -> Span {
        let start = start.min(self.src.len());
        let before = self.src.get(..start).unwrap_or("");
        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let column = before.get(line_start..).map_or(0, |t| t.chars().count()) + 1;
        Span {
            line: line as u32,
            column: column as u32,
            byte_start: start as u32,
            byte_end: end.min(self.src.len()) as u32,
        }
    }

    fn emit(&mut self, sev: Severity, code: &'static str, at: (usize, usize), msg: String) {
        if self.out.items.len() >= self.limit {
            if self.dropped == 0 {
                self.dropped_code = code;
            }
            self.dropped += 1;
            return;
        }
        let span = self.span(at.0, at.1);
        self.out.emit(sev, code, span, msg);
    }

    fn warn(&mut self, code: &'static str, at: (usize, usize), msg: String) {
        self.emit(Severity::Warning, code, at, msg);
    }

    fn finish(mut self) -> Diagnostics {
        if self.dropped > 0 {
            let n = self.dropped;
            self.out.emit(
                Severity::Info,
                self.dropped_code,
                Span::default(),
                format!("{} more diagnostics not shown", n),
            );
        }
        self.out
    }
}

fn too_large(d: &mut Diags, what: &str) {
    d.limit = usize::MAX;
    d.emit(
        Severity::Error,
        "E013",
        (0, 0),
        format!("stylesheet exceeds its limit on {}", what),
    );
}

// ---------------------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------------------

/// A declaration as written, before resolution.
#[derive(Clone)]
struct RawDecl {
    name: String,
    value: String,
    at: (usize, usize),
}

/// Theme blocks as written, merged by key in first-use order.
#[derive(Default)]
struct RawThemes {
    blocks: Vec<(ThemeKey, Vec<RawDecl>)>,
}

impl RawThemes {
    fn block(&mut self, key: &ThemeKey) -> &mut Vec<RawDecl> {
        let i = match self.blocks.iter().position(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                self.blocks.push((key.clone(), Vec::new()));
                self.blocks.len() - 1
            }
        };
        &mut self.blocks[i].1
    }

    fn get(&self, key: &ThemeKey) -> Option<&Vec<RawDecl>> {
        self.blocks.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

struct RawRole {
    theme: Option<String>,
    auto_dark: bool,
    cluster: bool,
    name: String,
    tone: Option<RawDecl>,
    dash: Option<RawDecl>,
}

struct Parser<'a> {
    src: &'a str,
    limits: StylesheetLimits,
    d: Diags<'a>,
    themes: RawThemes,
    roles: Vec<RawRole>,
    rules: usize,
    ignored: usize,
    theme_names: Vec<String>,
    role_selectors: usize,
}

enum Stop {
    TooLarge(&'static str),
}

impl<'a> Parser<'a> {
    fn text(&self, r: (usize, usize)) -> &'a str {
        self.src.get(r.0..r.1).unwrap_or("")
    }

    fn level(
        &mut self,
        start: usize,
        end: usize,
        depth: usize,
        in_media: bool,
    ) -> Result<(), Stop> {
        let items = scan::items(self.src, start, end, depth, self.limits.depth)
            .map_err(|_| Stop::TooLarge("block depth"))?;
        for (item, nested) in items {
            self.rules += 1;
            if self.rules > self.limits.rules {
                return Err(Stop::TooLarge("rules"));
            }
            let prelude = scan::normalise(self.text(item.prelude));
            if prelude.starts_with('@') {
                let dark = prelude.eq_ignore_ascii_case("@media (prefers-color-scheme: dark)");
                match item.body {
                    Some((s, e)) if dark && !in_media => self.level(s, e, depth + 1, true)?,
                    _ => self.d.warn(
                        "W017",
                        item.prelude,
                        format!(
                            "at-rule `{}` outside the stylesheet subset; dropped",
                            crate::diag::excerpt(&prelude)
                        ),
                    ),
                }
                continue;
            }
            let Some(body) = item.body else {
                self.d.warn(
                    "W017",
                    item.prelude,
                    String::from("text outside a rule; dropped"),
                );
                continue;
            };
            self.rule(item.prelude, &prelude, body, nested, in_media)?;
        }
        Ok(())
    }

    fn rule(
        &mut self,
        at: (usize, usize),
        prelude: &str,
        body: (usize, usize),
        nested: bool,
        in_media: bool,
    ) -> Result<(), Stop> {
        let decls: Vec<RawDecl> = scan::declarations(self.src, body.0, body.1)
            .into_iter()
            .map(|(n, v)| RawDecl {
                name: String::from(self.text(n)),
                value: String::from(self.text(v)),
                at: (n.0, v.1),
            })
            .collect();
        if decls.len() > self.limits.declarations {
            return Err(Stop::TooLarge("declarations per rule"));
        }
        // A nested block hides its declarations from the split: any token text counts.
        let merlion = decls.iter().any(|d| d.name.starts_with("--merlion-"))
            || (nested && self.text(body).contains("--merlion-"));
        let list = scan::split_list(prelude);
        let sels: Vec<Option<Selector>> = list
            .iter()
            .map(|s| value::parse_selector(s, in_media))
            .collect();
        let custom = decls.iter().any(|d| d.name.starts_with("--"));
        let all_theme = sels.iter().all(|s| {
            matches!(
                s,
                Some(Selector::Root | Selector::Theme(_) | Selector::AutoDark)
            )
        });
        if !merlion && !(custom && all_theme) {
            self.ignored += 1;
            return Ok(());
        }
        if list.len() > self.limits.selectors {
            return Err(Stop::TooLarge("selectors per list"));
        }
        if nested {
            self.d.warn(
                "W017",
                at,
                String::from("nested rules are outside the stylesheet subset; rule dropped"),
            );
            return Ok(());
        }
        let mut targets: Vec<Selector> = Vec::new();
        for (text, sel) in list.iter().zip(sels) {
            match sel {
                Some(s) => {
                    if let Selector::Theme(t) | Selector::Role { theme: Some(t), .. } = &s {
                        if !self.theme_names.contains(t) {
                            self.theme_names.push(t.clone());
                            if self.theme_names.len() > self.limits.themes {
                                return Err(Stop::TooLarge("theme names"));
                            }
                        }
                    }
                    if matches!(s, Selector::Role { .. }) {
                        self.role_selectors += 1;
                        if self.role_selectors > self.limits.role_selectors {
                            return Err(Stop::TooLarge("role selectors"));
                        }
                    }
                    targets.push(s);
                }
                None => self.d.warn(
                    "W017",
                    at,
                    format!(
                        "selector `{}` is outside the stylesheet subset; dropped",
                        crate::diag::excerpt(text)
                    ),
                ),
            }
        }
        if targets.is_empty() {
            return Ok(());
        }
        let has_theme = targets.iter().any(|s| !matches!(s, Selector::Role { .. }));
        let has_role = targets.iter().any(|s| matches!(s, Selector::Role { .. }));
        // Declarations valid for every selector of the list; others get one W018 each.
        let mut theme_decls: Vec<RawDecl> = Vec::new();
        let mut tone: Option<RawDecl> = None;
        let mut dash: Option<RawDecl> = None;
        for d in decls {
            let reject =
                |why: &str| format!("`{}` {}; dropped", crate::diag::excerpt(&d.name), why);
            if let Some(why) = value::forbidden(&d.value) {
                let m = reject(why);
                self.d.warn("W018", d.at, m);
                continue;
            }
            let ok = match value::classify(&d.name) {
                Name::Theme(_) | Name::Private(_) => !has_role,
                Name::Tone | Name::Dash => !has_theme,
                Name::Rejected => false,
            };
            if !ok {
                let m = reject("is not a token this selector may set");
                self.d.warn("W018", d.at, m);
                continue;
            }
            match value::classify(&d.name) {
                Name::Tone => tone = Some(d),
                Name::Dash => dash = Some(d),
                _ => theme_decls.push(d),
            }
        }
        for s in targets {
            match s {
                Selector::Root => self
                    .themes
                    .block(&ThemeKey::Root)
                    .extend(theme_decls.iter().cloned()),
                Selector::Theme(t) => self
                    .themes
                    .block(&ThemeKey::Named(t))
                    .extend(theme_decls.iter().cloned()),
                Selector::AutoDark => self
                    .themes
                    .block(&ThemeKey::AutoDark)
                    .extend(theme_decls.iter().cloned()),
                Selector::Role {
                    theme,
                    auto_dark,
                    cluster,
                    name,
                } => {
                    if tone.is_some() || dash.is_some() {
                        self.roles.push(RawRole {
                            theme,
                            auto_dark,
                            cluster,
                            name,
                            tone: tone.clone(),
                            dash: dash.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------------------

/// The declarations visible in one theme: `:root`, overlaid by the theme's own block,
/// the last declaration of a name winning. Built once per theme in `O(n log n)` and
/// shared, with its memo, by the theme block and every role rule of that theme.
struct Env<'a> {
    decls: BTreeMap<&'a str, &'a RawDecl>,
    /// Resolved names with the number of `var()` hops their value took.
    memo: BTreeMap<String, (Value, usize)>,
}

impl<'a> Env<'a> {
    fn new(root: Option<&'a Vec<RawDecl>>, own: Option<&'a Vec<RawDecl>>) -> Self {
        let mut decls: BTreeMap<&str, &RawDecl> = BTreeMap::new();
        for d in root.into_iter().flatten().chain(own.into_iter().flatten()) {
            decls.insert(d.name.as_str(), d);
        }
        Env {
            decls,
            memo: BTreeMap::new(),
        }
    }

    fn find(&self, name: &str) -> Option<&'a RawDecl> {
        self.decls.get(name).copied()
    }
}

/// The environment of theme `key`, built on first use.
fn env_for<'e, 'a>(
    envs: &'e mut BTreeMap<ThemeKey, Env<'a>>,
    themes: &'a RawThemes,
    key: &ThemeKey,
) -> &'e mut Env<'a> {
    envs.entry(key.clone()).or_insert_with(|| {
        let own = if *key == ThemeKey::Root {
            None
        } else {
            themes.get(key)
        };
        Env::new(themes.get(&ThemeKey::Root), own)
    })
}

type Resolved = Result<(Value, usize), (&'static str, String)>;

/// Resolves `text` as a value of `kind` in `env`, having taken `depth` `var()` hops:
/// the value and the hops it takes from here, or `(code, message)`.
fn resolve(
    env: &mut Env,
    text: &str,
    kind: Kind,
    depth: usize,
    max: usize,
    stack: &mut Vec<String>,
) -> Resolved {
    let t = text.trim();
    if !t.to_ascii_lowercase().starts_with("var(") {
        return value::parse_literal(t, kind)
            .map(|v| (v, 0))
            .map_err(|m| ("W018", String::from(m)));
    }
    let Some((name, fallback)) = value::parse_var(t) else {
        return Err(("W018", String::from("malformed `var()`")));
    };
    if fallback.is_some_and(|f| f.to_ascii_lowercase().contains("var(")) {
        return Err(("W018", String::from("a `var()` fallback must be a literal")));
    }
    let too_deep = || ("W019", format!("`var()` nested deeper than {}", max));
    if depth >= max {
        return Err(too_deep());
    }
    if stack.iter().any(|s| s == name) {
        return Err(("W019", format!("`{}` refers to itself", name)));
    }
    let found = match env.memo.get(name) {
        Some((v, hops)) => Some(Ok((v.clone(), *hops))),
        None => env.find(name).map(|d| {
            stack.push(String::from(name));
            let target_kind = match value::classify(name) {
                Name::Theme(tok) => tok.kind(),
                _ => Kind::Colour,
            };
            let r = resolve(env, &d.value.clone(), target_kind, depth + 1, max, stack);
            stack.pop();
            if let Ok((v, hops)) = &r {
                env.memo.insert(String::from(name), (v.clone(), *hops));
            }
            r
        }),
    };
    match found {
        Some(Ok((v, hops))) => {
            if depth + 1 + hops > max {
                Err(too_deep())
            } else if value_fits(&v, kind) {
                Ok((v, hops + 1))
            } else {
                Err(("W018", format!("`{}` has the wrong type here", name)))
            }
        }
        Some(Err(e)) => Err(e),
        None => match fallback {
            Some(f) => value::parse_literal(f, kind)
                .map(|v| (v, 0))
                .map_err(|m| ("W018", String::from(m))),
            None => Err(("W019", format!("`{}` is not declared", name))),
        },
    }
}

fn value_fits(v: &Value, kind: Kind) -> bool {
    matches!(
        (v, kind),
        (Value::Colour(_), Kind::Colour | Kind::Paint)
            | (Value::NoPaint, Kind::Paint)
            | (Value::Stroke(_), Kind::Stroke)
            | (Value::Dash(_), Kind::Dash)
    )
}

fn theme_order(k: &ThemeKey, first_use: &[ThemeKey]) -> (u8, usize) {
    match k {
        ThemeKey::Root => (0, 0),
        ThemeKey::Named(_) => (1, first_use.iter().position(|x| x == k).unwrap_or(0)),
        ThemeKey::AutoDark => (2, 0),
    }
}

/// Compiles a stylesheet. `None` with an `E013` error when a limit is exceeded; every
/// dropped rule or declaration is reported and the rest is kept.
pub fn compile(css: &str, limits: &StylesheetLimits) -> (Option<Stylesheet>, Diagnostics) {
    let mut d = Diags {
        src: css,
        out: Diagnostics::new(false),
        limit: limits.diagnostics,
        dropped: 0,
        dropped_code: "W017",
    };
    if css.len() > limits.bytes {
        too_large(&mut d, "size");
        return (None, d.finish());
    }
    let mut p = Parser {
        src: css,
        limits: *limits,
        d,
        themes: RawThemes::default(),
        roles: Vec::new(),
        rules: 0,
        ignored: 0,
        theme_names: Vec::new(),
        role_selectors: 0,
    };
    if let Err(Stop::TooLarge(what)) = p.level(0, css.len(), 1, false) {
        too_large(&mut p.d, what);
        return (None, p.d.finish());
    }
    let Parser {
        mut d,
        themes,
        roles,
        ignored,
        ..
    } = p;
    if ignored > 0 {
        d.emit(
            Severity::Info,
            "I032",
            (0, 0),
            format!(
                "{} rule{} declaring no `--merlion-*` token ignored",
                ignored,
                if ignored == 1 { "" } else { "s" }
            ),
        );
    }
    let max = limits.var_depth;
    let first_use: Vec<ThemeKey> = themes.blocks.iter().map(|(k, _)| k.clone()).collect();
    let mut out = Stylesheet::default();
    let mut envs: BTreeMap<ThemeKey, Env> = BTreeMap::new();
    for (key, raw) in &themes.blocks {
        let env = env_for(&mut envs, &themes, key);
        let mut decls: Vec<(Token, Value)> = Vec::new();
        for rd in raw {
            let Name::Theme(tok) = value::classify(&rd.name) else {
                continue; // private tokens resolve through `var()` only
            };
            match resolve(env, &rd.value, tok.kind(), 0, max, &mut Vec::new()) {
                Ok((v, _)) => {
                    decls.retain(|(t, _)| *t != tok);
                    decls.push((tok, v));
                }
                Err((code, why)) => d.warn(
                    code,
                    rd.at,
                    format!("`{}`: {}; dropped", crate::diag::excerpt(&rd.name), why),
                ),
            }
        }
        decls.sort_by(|a, b| a.0.cmp(&b.0));
        out.themes.push(ThemeBlock {
            key: key.clone(),
            decls,
        });
    }
    out.themes.sort_by_key(|b| theme_order(&b.key, &first_use));
    out.themes.retain(|b| !b.decls.is_empty());
    for r in roles {
        // A role rule resolves `var()` in its own theme: `:root` overlaid by the named
        // theme, or by the automatic dark block.
        let key = match (&r.theme, r.auto_dark) {
            (Some(t), _) => ThemeKey::Named(t.clone()),
            (None, true) => ThemeKey::AutoDark,
            (None, false) => ThemeKey::Root,
        };
        let env = env_for(&mut envs, &themes, &key);
        let mut get = |rd: &Option<RawDecl>, kind: Kind, d: &mut Diags| -> Option<Value> {
            let rd = rd.as_ref()?;
            match resolve(env, &rd.value, kind, 0, max, &mut Vec::new()) {
                Ok((v, _)) => Some(v),
                Err((code, why)) => {
                    d.warn(
                        code,
                        rd.at,
                        format!("`{}`: {}; dropped", crate::diag::excerpt(&rd.name), why),
                    );
                    None
                }
            }
        };
        let tone = match get(&r.tone, Kind::Colour, &mut d) {
            Some(Value::Colour(c)) => Some(c),
            _ => None,
        };
        let dash = match get(&r.dash, Kind::Dash, &mut d) {
            Some(Value::Dash(x)) => Some(x),
            _ => None,
        };
        if tone.is_some() || dash.is_some() {
            out.roles.push(RoleRule {
                theme: r.theme,
                auto_dark: r.auto_dark,
                cluster: r.cluster,
                name: r.name,
                tone,
                dash,
            });
        }
    }
    // The compiled output must itself compile (a fixed point): selector lists expand to
    // one block per selector, so a small input can serialise past the size limit.
    if out.to_css().len() > limits.bytes {
        too_large(&mut d, "compiled size");
        return (None, d.finish());
    }
    (Some(out), d.finish())
}

// ---------------------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------------------

fn value_css(v: &Value) -> String {
    match v {
        Value::Colour(c) => c.to_hex(),
        Value::NoPaint => String::from("none"),
        Value::Stroke(n) => {
            let mut s = String::new();
            push_num(&mut s, *n);
            s.push_str("px");
            s
        }
        Value::Dash(d) => dash_css(d),
    }
}

/// `none` or the numbers separated by spaces.
pub fn dash_css(d: &Dash) -> String {
    match d {
        Dash::None => String::from("none"),
        Dash::Pattern(p) => {
            let mut s = String::new();
            for (i, n) in p.iter().take(8).enumerate() {
                if i > 0 {
                    s.push(' ');
                }
                push_num(&mut s, crate::math::clamp(*n, 0.0, 100.0));
            }
            s
        }
    }
}

impl Stylesheet {
    /// Page CSS: literal values only, selectors in the fixed shapes `:root`,
    /// `[data-theme="<t>"]`, `:root:not([data-theme])` inside the one media query, and
    /// role selectors prefixed with `.merlion `. Compiling the output yields the same bytes.
    pub fn to_css(&self) -> String {
        let mut out = String::new();
        for b in &self.themes {
            let (open, pad, close) = match &b.key {
                ThemeKey::Root => (String::from(":root {\n"), "  ", "}\n"),
                ThemeKey::Named(t) => (format!("[data-theme=\"{}\"] {{\n", t), "  ", "}\n"),
                ThemeKey::AutoDark => (
                    String::from(
                        "@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]) {\n",
                    ),
                    "    ",
                    "  }\n}\n",
                ),
            };
            out.push_str(&open);
            for (t, v) in &b.decls {
                let _ = writeln!(out, "{}--merlion-{}: {};", pad, t.name(), value_css(v));
            }
            out.push_str(close);
        }
        // Roles in source order; consecutive automatic-dark roles share one media block.
        let mut in_media = false;
        for r in &self.roles {
            if r.auto_dark != in_media {
                out.push_str(if r.auto_dark {
                    "@media (prefers-color-scheme: dark) {\n"
                } else {
                    "}\n"
                });
                in_media = r.auto_dark;
            }
            let pad = if in_media { "  " } else { "" };
            out.push_str(pad);
            if in_media {
                out.push_str(":root:not([data-theme]) ");
            }
            if let Some(t) = &r.theme {
                let _ = write!(out, "[data-theme=\"{}\"] ", t);
            }
            let _ = writeln!(
                out,
                ".merlion .merlion-{}-{} {{",
                if r.cluster { "cc" } else { "c" },
                r.name
            );
            if let Some(c) = r.tone {
                let _ = writeln!(out, "{}  --merlion-tone: {};", pad, c.to_hex());
            }
            if let Some(d) = &r.dash {
                let _ = writeln!(out, "{}  --merlion-dash: {};", pad, dash_css(d));
            }
            out.push_str(pad);
            out.push_str("}\n");
        }
        if in_media {
            out.push_str("}\n");
        }
        out
    }

    /// The named themes, in first-use order.
    pub fn theme_names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = Vec::new();
        for b in &self.themes {
            if let ThemeKey::Named(t) = &b.key {
                v.push(t);
            }
        }
        for r in &self.roles {
            if let Some(t) = &r.theme {
                if !v.contains(&t.as_str()) {
                    v.push(t);
                }
            }
        }
        v
    }

    fn block(&self, key: &ThemeKey) -> Option<&ThemeBlock> {
        self.themes.iter().find(|b| b.key == *key)
    }

    fn table(&self, theme: Option<&str>) -> PaletteTable {
        let mut t = PaletteTable::default();
        let mut blocks = alloc::vec![self.block(&ThemeKey::Root)];
        if let Some(name) = theme {
            blocks.push(self.block(&ThemeKey::Named(String::from(name))));
        }
        for b in blocks.into_iter().flatten() {
            for (tok, v) in &b.decls {
                t.set(tok, v);
            }
        }
        // Per role and property: a themed rule beats an unthemed one, then source order.
        let mut keys: Vec<(bool, &str)> = Vec::new();
        for r in self.roles.iter().filter(|r| !r.auto_dark) {
            if !keys.contains(&(r.cluster, r.name.as_str())) {
                keys.push((r.cluster, r.name.as_str()));
            }
        }
        let mut tones: Vec<(usize, PaletteTone)> = Vec::new();
        for (cluster, name) in keys {
            let mut tone: Option<(bool, usize, Rgba8)> = None;
            let mut dash: Option<(bool, usize, Dash)> = None;
            for (i, r) in self.roles.iter().enumerate() {
                if r.cluster != cluster || r.name != name || r.auto_dark {
                    continue;
                }
                let themed = match (&r.theme, theme) {
                    (None, _) => false,
                    (Some(a), Some(b)) if a == b => true,
                    _ => continue,
                };
                if let Some(c) = r.tone {
                    if tone.as_ref().is_none_or(|x| (themed, i) >= (x.0, x.1)) {
                        tone = Some((themed, i, c));
                    }
                }
                if let Some(d) = &r.dash {
                    if dash.as_ref().is_none_or(|x| (themed, i) >= (x.0, x.1)) {
                        dash = Some((themed, i, d.clone()));
                    }
                }
            }
            if tone.is_none() && dash.is_none() {
                continue;
            }
            let order = tone
                .as_ref()
                .map_or(0, |x| x.1)
                .max(dash.as_ref().map_or(0, |x| x.1));
            tones.push((
                order,
                PaletteTone {
                    name: String::from(name),
                    cluster,
                    tone: tone.map(|x| x.2),
                    dash: dash.map(|x| match x.2 {
                        Dash::None => Vec::new(),
                        Dash::Pattern(p) => p,
                    }),
                },
            ));
        }
        // Node and edge tones first, then cluster tones, each in cascade order: the two
        // never meet on one element, and a fixed partition keeps the canonical form (and
        // the diagram id) equal after a round trip through the WASM palette JSON.
        tones.sort_by_key(|x| (x.1.cluster, x.0));
        t.tones = tones.into_iter().map(|x| x.1).collect();
        t
    }

    /// The palette for one theme (`None`: `:root` alone) and, optionally, a named theme
    /// as the `prefers-color-scheme: dark` variant. `None` when a name is not defined.
    pub fn palette(&self, theme: Option<&str>, auto_dark: Option<&str>) -> Option<Palette> {
        let names = self.theme_names();
        for t in [theme, auto_dark].into_iter().flatten() {
            if !names.contains(&t) {
                return None;
            }
        }
        Some(Palette {
            light: self.table(theme),
            dark: auto_dark.map(|t| self.table(Some(t))),
        })
    }
}

// ---------------------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------------------

/// A per-role tone and dash of a palette. An empty `dash` is `none`.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteTone {
    pub name: String,
    pub cluster: bool,
    pub tone: Option<Rgba8>,
    pub dash: Option<Vec<f64>>,
}

/// The literals of one theme.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteTable {
    /// Colour tokens the stylesheet sets, in token order.
    pub colours: Vec<(Role, Rgba8)>,
    /// `--merlion-series-1` … `-8`.
    pub series: Vec<(u8, Rgba8)>,
    /// `--merlion-stroke` in px.
    pub stroke: Option<f64>,
    /// `--merlion-c-{name}-{prop}`; `None` is `none`. Sorted by (name, property), each
    /// pair once.
    pub classes: Vec<(String, ClassProp, Option<Rgba8>)>,
    /// Role tones in cascade order, node and edge tones before cluster tones: a later
    /// entry wins on an element with both roles.
    pub tones: Vec<PaletteTone>,
}

impl PaletteTable {
    fn set(&mut self, tok: &Token, v: &Value) {
        match (tok, v) {
            (Token::Colour(r), Value::Colour(c)) => {
                self.colours.retain(|x| x.0 != *r);
                self.colours.push((*r, *c));
                self.colours.sort_by_key(|x| x.0);
            }
            (Token::Series(n), Value::Colour(c)) => {
                self.series.retain(|x| x.0 != *n);
                self.series.push((*n, *c));
                self.series.sort_by_key(|x| x.0);
            }
            (Token::Stroke, Value::Stroke(s)) => self.stroke = Some(*s),
            (Token::Class(name, p), v) => {
                let c = match v {
                    Value::Colour(c) => Some(*c),
                    _ => None,
                };
                self.classes.retain(|x| !(x.0 == *name && x.1 == *p));
                self.classes.push((name.clone(), *p, c));
                self.classes.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
            }
            _ => {}
        }
    }

    /// The value a colour token (name without `--merlion-`) is set to, if any.
    pub fn colour(&self, token: &str) -> Option<Rgba8> {
        if let Some(r) = Role::from_name(token) {
            return self.colours.iter().find(|x| x.0 == r).map(|x| x.1);
        }
        let n = token.strip_prefix("series-")?.parse::<u8>().ok()?;
        self.series.iter().find(|x| x.0 == n).map(|x| x.1)
    }

    /// The colour a role draws with: its value, or the mix or alias it derives from.
    pub fn resolved(&self, token: &str) -> Option<Rgba8> {
        let r = Role::from_name(token)?;
        Some(self.theme_table().rgba(r))
    }

    /// The literal table of the renderer.
    pub fn theme_table(&self) -> Table {
        Table::new(self.colours.clone(), self.stroke).with_series(self.series.clone())
    }

    /// The tone of role `name` on clusters (`cluster`) or on nodes and edges.
    pub fn tone(&self, name: &str, cluster: bool) -> Option<&PaletteTone> {
        self.tones
            .iter()
            .find(|t| t.name == name && t.cluster == cluster)
    }

    /// A `classDef` token value: `Some(None)` is `none`.
    pub fn class_colour(&self, name: &str, prop: ClassProp) -> Option<Option<Rgba8>> {
        // `classes` is sorted by (name, property) and holds each pair once.
        self.classes
            .binary_search_by(|x| (x.0.as_str(), x.1).cmp(&(name, prop)))
            .ok()
            .and_then(|i| self.classes.get(i))
            .map(|x| x.2)
    }

    fn canonical(&self, out: &mut String) {
        for (r, c) in &self.colours {
            let _ = write!(out, "{}={};", r.name(), c.to_hex());
        }
        for (n, c) in &self.series {
            let _ = write!(out, "series-{}={};", n, c.to_hex());
        }
        if let Some(s) = self.stroke {
            out.push_str("stroke=");
            push_num(out, s);
            out.push(';');
        }
        for (n, p, c) in &self.classes {
            let _ = write!(
                out,
                "c-{}-{}={};",
                n,
                p.name(),
                c.map_or(String::from("none"), |c| c.to_hex())
            );
        }
        for t in &self.tones {
            let _ = write!(out, "{}{}:", if t.cluster { "cc-" } else { "c-" }, t.name);
            if let Some(c) = t.tone {
                out.push_str(&c.to_hex());
            }
            out.push('/');
            if let Some(d) = &t.dash {
                out.push_str(&dash_css(&if d.is_empty() {
                    Dash::None
                } else {
                    Dash::Pattern(d.clone())
                }));
            }
            out.push(';');
        }
    }
}

/// Literals resolved from a stylesheet for one theme, baked into a render
/// (specs/svg-output.md#palette).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub light: PaletteTable,
    /// The `prefers-color-scheme: dark` variant.
    pub dark: Option<PaletteTable>,
}

impl Palette {
    /// The canonical serialisation: equal palettes serialise equally, whatever the
    /// formatting of the stylesheet they came from.
    pub fn canonical(&self) -> String {
        let mut s = String::from("palette-v1|");
        self.light.canonical(&mut s);
        if let Some(d) = &self.dark {
            s.push_str("|dark|");
            d.canonical(&mut s);
        }
        s
    }

    /// FNV-1a 64 of [`Palette::canonical`]; joins the default diagram id hash.
    pub fn digest(&self) -> u64 {
        crate::ids::fnv1a64(self.canonical().as_bytes())
    }

    /// Parses [`Palette::canonical`] output: the wire form of the WASM `palette` option.
    /// Every entry is checked against its token's grammar: colours are `#` hex, `stroke`
    /// a number from 0 to 20, class tokens name a `classDef`-grammar class, tone dashes
    /// `none` or up to 8 numbers from 0 to 100. Input over [`MAX_PALETTE_BYTES`], an
    /// unknown token or a repeated tone is refused. Linear in the input; never panics.
    pub fn parse(s: &str) -> Result<Palette, &'static str> {
        if s.len() > MAX_PALETTE_BYTES {
            return Err("palette too large");
        }
        let body = s
            .strip_prefix("palette-v1|")
            .ok_or("not a palette-v1 string")?;
        let (light, dark) = match body.split_once("|dark|") {
            Some((l, d)) => (l, Some(d)),
            None => (body, None),
        };
        Ok(Palette {
            light: PaletteTable::parse(light)?,
            dark: dark.map(PaletteTable::parse).transpose()?,
        })
    }
}

/// The largest canonical palette [`Palette::parse`] accepts. A compiled stylesheet is
/// at most 64 KiB, and each palette entry is shorter than the page-CSS line it comes
/// from, so one table stays under 64 KiB and a light and a dark table under 128 KiB.
pub const MAX_PALETTE_BYTES: usize = 128 * 1024;

/// The most tones one palette table holds: a compiled stylesheet has at most this many
/// role selectors ([`StylesheetLimits::role_selectors`]).
pub const MAX_PALETTE_TONES: usize = 256;

impl PaletteTable {
    /// Entries are collected, then sorted once, so parsing stays `O(n log n)`; a token
    /// or tone given twice is refused.
    fn parse(s: &str) -> Result<PaletteTable, &'static str> {
        let mut t = PaletteTable::default();
        let Some(entries) = s.strip_suffix(';') else {
            return if s.is_empty() {
                Ok(t)
            } else {
                Err("entry without `;`")
            };
        };
        let mut seen_tones = alloc::collections::BTreeSet::new();
        for e in entries.split(';') {
            match (e.find('='), e.find(':')) {
                (Some(i), None) => t.parse_token(&e[..i], &e[i + 1..])?,
                (None, Some(i)) => t.parse_tone(&e[..i], &e[i + 1..], &mut seen_tones)?,
                _ => return Err("malformed entry"),
            }
        }
        if t.tones.len() > MAX_PALETTE_TONES {
            return Err("more than 256 tones in one table");
        }
        t.tones.sort_by_key(|x| x.cluster);
        t.colours.sort_by_key(|x| x.0);
        t.series.sort_by_key(|x| x.0);
        t.classes.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        let dup = t.colours.windows(2).any(|w| w[0].0 == w[1].0)
            || t.series.windows(2).any(|w| w[0].0 == w[1].0)
            || t.classes
                .windows(2)
                .any(|w| w[0].0 == w[1].0 && w[0].1 == w[1].1);
        if dup {
            return Err("repeated token");
        }
        Ok(t)
    }

    fn parse_token(&mut self, key: &str, v: &str) -> Result<(), &'static str> {
        let Name::Theme(tok) = value::classify(&format!("--merlion-{}", key)) else {
            return Err("unknown token");
        };
        let hex = |v: &str| Rgba8::from_hex(v).ok_or("not a hex colour");
        match (tok, v) {
            (Token::Colour(r), v) => self.colours.push((r, hex(v)?)),
            (Token::Series(n), v) => self.series.push((n, hex(v)?)),
            (Token::Stroke, v) if v.ends_with("px") => return Err("stroke is a bare number"),
            (Token::Stroke, v) => match value::parse_literal(v, Kind::Stroke) {
                Ok(Value::Stroke(x)) if self.stroke.is_none() => self.stroke = Some(x),
                Ok(_) => return Err("repeated token"),
                Err(_) => return Err("stroke out of range"),
            },
            (Token::Class(name, ClassProp::Color), v) => {
                self.classes.push((name, ClassProp::Color, Some(hex(v)?)))
            }
            (Token::Class(name, p), "none") => self.classes.push((name, p, None)),
            (Token::Class(name, p), v) => self.classes.push((name, p, Some(hex(v)?))),
        }
        Ok(())
    }

    fn parse_tone(
        &mut self,
        key: &str,
        v: &str,
        seen: &mut alloc::collections::BTreeSet<(bool, String)>,
    ) -> Result<(), &'static str> {
        let (cluster, name) = match (key.strip_prefix("cc-"), key.strip_prefix("c-")) {
            (Some(n), _) => (true, n),
            (None, Some(n)) => (false, n),
            _ => return Err("tone key is not c- or cc-"),
        };
        if !value::is_class_name(name) {
            return Err("tone names a class outside the classDef grammar");
        }
        if !seen.insert((cluster, String::from(name))) {
            return Err("repeated tone");
        }
        let (tone, dash) = v.split_once('/').ok_or("tone without `/`")?;
        let tone = match tone {
            "" => None,
            h => Some(Rgba8::from_hex(h).ok_or("not a hex colour")?),
        };
        let dash = match dash {
            "" => None,
            d => match value::parse_literal(d, Kind::Dash) {
                Ok(Value::Dash(Dash::None)) => Some(Vec::new()),
                Ok(Value::Dash(Dash::Pattern(p))) => Some(p),
                _ => return Err("not a dash array"),
            },
        };
        self.tones.push(PaletteTone {
            name: String::from(name),
            cluster,
            tone,
            dash,
        });
        Ok(())
    }
}
