//! Configuration from front matter and `%%{init}%%` (specs/parser.md#front-matter-and-directives).
//!
//! Both sources parse into the same [`Value`] tree. Every accepted key takes an
//! enumerated value; anything else is ignored with a diagnostic:
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | `W016` | Warning | Unknown key, or a value outside the key's set, ignored |
//! | `I011` | Info | `theme`, `themeVariables` or `look` ignored: themes are CSS |
//!
//! Merlion's own keys live under `merlion`: `merlion.autoTone` (`true` | `false`).
//! `quadrantChart` and the quadrant colours of `themeVariables` configure quadrant
//! charts (specs/quadrant.md#configuration).

use alloc::string::String;
use alloc::vec::Vec;

use super::cursor::LineIndex;
use crate::diag::{excerpt, Diagnostics, Severity};
use crate::model::gantt::{calendar, DisplayMode};
use crate::model::quadrant::{XAxisPosition, YAxisPosition, NUMERIC_KEYS};
use crate::model::Meta;

/// A parsed YAML or JSON value. YAML scalars are always strings.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Value>),
    Map(Vec<Entry>),
}

/// One mapping entry; `start..end` is the key's absolute byte range in the source.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub start: usize,
    pub end: usize,
    pub value: Value,
}

/// Mermaid's curve names (`config.flowchart.curve`).
pub const CURVES: &[&str] = &[
    "basis",
    "basisClosed",
    "basisOpen",
    "bumpX",
    "bumpY",
    "bundle",
    "cardinal",
    "cardinalClosed",
    "cardinalOpen",
    "catmullRom",
    "catmullRomClosed",
    "catmullRomOpen",
    "linear",
    "linearClosed",
    "monotoneX",
    "monotoneY",
    "natural",
    "step",
    "stepAfter",
    "stepBefore",
];

/// `config.layout` values; all are laid out by Merlion's engine.
pub const LAYOUTS: &[&str] = &["dagre", "elk", "merlion"];

/// Applies top-level front matter keys: `title`, `accTitle`, `accDescr`, `config`.
pub fn apply_front_matter(
    entries: &[Entry],
    idx: &LineIndex,
    meta: &mut Meta,
    diags: &mut Diagnostics,
) {
    for e in entries {
        match e.key.as_str() {
            "displayMode" => apply_gantt(core::slice::from_ref(e), idx, meta, diags),
            "title" | "accTitle" | "accDescr" => match &e.value {
                Value::Str(v) => {
                    let slot = match e.key.as_str() {
                        "title" => &mut meta.title,
                        "accTitle" => &mut meta.acc_title,
                        _ => &mut meta.acc_descr,
                    };
                    *slot = Some(v.clone());
                }
                _ => bad_value(e, idx, diags, "a string"),
            },
            "config" => match &e.value {
                Value::Map(m) => apply_config(m, idx, meta, diags),
                _ => bad_value(e, idx, diags, "a mapping"),
            },
            _ => unknown_key(e, "", idx, diags),
        }
    }
}

/// Applies a config mapping (front matter `config:` or the `%%{init}%%` object).
pub fn apply_config(entries: &[Entry], idx: &LineIndex, meta: &mut Meta, diags: &mut Diagnostics) {
    for e in entries {
        match e.key.as_str() {
            "themeVariables" => apply_theme_variables(e, idx, meta),
            "theme" | "look" => diags.emit(
                Severity::Info,
                "I011",
                idx.span(e.start, e.end),
                alloc::format!(
                    "`{}` is ignored: Merlion themes are CSS custom properties",
                    excerpt(&e.key)
                ),
            ),
            "quadrantChart" => match &e.value {
                Value::Map(m) => apply_quadrant(m, idx, meta, diags),
                _ => bad_value(e, idx, diags, "a mapping"),
            },
            "layout" => match enum_value(&e.value, LAYOUTS) {
                Some(v) => meta.layout = Some(v),
                None => bad_value(e, idx, diags, "one of `dagre`, `elk` or `merlion`"),
            },
            "flowchart" => match &e.value {
                Value::Map(m) => {
                    for inner in m {
                        if inner.key == "curve" {
                            match enum_value(&inner.value, CURVES) {
                                Some(v) => meta.curve = Some(v),
                                None => {
                                    bad_value(inner, idx, diags, "one of Mermaid's curve names")
                                }
                            }
                        } else {
                            unknown_key(inner, "flowchart.", idx, diags);
                        }
                    }
                }
                _ => bad_value(e, idx, diags, "a mapping"),
            },
            "gantt" => match &e.value {
                Value::Map(m) => apply_gantt(m, idx, meta, diags),
                _ => bad_value(e, idx, diags, "a mapping"),
            },
            "merlion" => match &e.value {
                Value::Map(m) => {
                    for inner in m {
                        if inner.key == "autoTone" {
                            match bool_value(&inner.value) {
                                Some(v) => meta.auto_tone = Some(v),
                                None => bad_value(inner, idx, diags, "`true` or `false`"),
                            }
                        } else {
                            unknown_key(inner, "merlion.", idx, diags);
                        }
                    }
                }
                _ => bad_value(e, idx, diags, "a mapping"),
            },
            _ => unknown_key(e, "", idx, diags),
        }
    }
}

fn apply_gantt(entries: &[Entry], idx: &LineIndex, meta: &mut Meta, diags: &mut Diagnostics) {
    for e in entries {
        let span = idx.span(e.start, e.end);
        match (e.key.as_str(), &e.value) {
            ("displayMode", Value::Str(value)) if value == "compact" || value == "default" => {
                meta.gantt.display_mode = Some(if value == "compact" {
                    DisplayMode::Compact
                } else {
                    DisplayMode::Default
                })
            }
            ("topAxis", value) => match bool_value(value) {
                Some(v) => meta.gantt.top_axis = Some(v),
                None => bad_value(e, idx, diags, "a boolean"),
            },
            ("axisFormat", Value::Str(value))
                if value.len() <= 4_096 && calendar::format_axis(0, value).is_some() =>
            {
                meta.gantt.axis_format = Some(value.clone());
                meta.gantt.axis_format_span = span;
            }
            ("tickInterval", Value::Str(value)) if calendar::tick_interval(value).is_some() => {
                meta.gantt.tick_interval = calendar::tick_interval(value);
                meta.gantt.tick_interval_span = span;
            }
            ("displayMode" | "axisFormat" | "tickInterval", _) => diags.emit(
                Severity::Error,
                "E002",
                span,
                "invalid Gantt configuration value",
            ),
            _ => unknown_key(e, "gantt.", idx, diags),
        }
    }
}

/// The `I011` message for `themeVariables`, which names the quadrant colours it keeps.
pub(crate) fn theme_variables_ignored(quadrant_keys: bool) -> &'static str {
    if quadrant_keys {
        "`themeVariables` is ignored except its quadrant chart colours: Merlion themes are CSS custom properties"
    } else {
        "`themeVariables` is ignored: Merlion themes are CSS custom properties"
    }
}

/// `themeVariables`: the quadrant chart colours are read (specs/quadrant.md#theme-variables);
/// any other key is ignored with `I011`. A mapping of quadrant colours alone defers its
/// `I011` to the header, which shows whether a quadrant chart reads them.
fn apply_theme_variables(e: &Entry, idx: &LineIndex, meta: &mut Meta) {
    let mut other = !matches!(e.value, Value::Map(_));
    let mut quadrant_keys = false;
    if let Value::Map(m) = &e.value {
        for inner in m {
            let Some(slot) = meta.quadrant.theme.slot(&inner.key) else {
                other = true;
                continue;
            };
            quadrant_keys = true;
            match &inner.value {
                Value::Str(v) => match quadrant_colour(v) {
                    Some(c) => {
                        *slot = Some(c);
                        meta.quadrant
                            .theme
                            .span
                            .get_or_insert(idx.span(inner.start, inner.end));
                    }
                    None => meta
                        .quadrant
                        .theme
                        .pending_invalid
                        .push((idx.span(inner.start, inner.end), excerpt(&inner.key))),
                },
                _ => meta
                    .quadrant
                    .theme
                    .pending_invalid
                    .push((idx.span(inner.start, inner.end), excerpt(&inner.key))),
            }
        }
    }
    let span = idx.span(e.start, e.end);
    meta.quadrant.theme.pending_other |= other || !quadrant_keys;
    meta.quadrant.theme.pending_ignored.get_or_insert(span);
}

/// Applies theme diagnostics after the header determines which keys have an effect.
pub(crate) fn finish_theme_variables(meta: &mut Meta, quadrant: bool, diags: &mut Diagnostics) {
    let theme = &mut meta.quadrant.theme;
    if quadrant {
        for (span, key) in theme.pending_invalid.drain(..) {
            diags.emit(
                Severity::Warning,
                "W016",
                span,
                alloc::format!("`{key}` must be a colour; ignored"),
            );
        }
    } else {
        theme.pending_invalid.clear();
    }
    if let Some(span) = theme
        .pending_ignored
        .take()
        .filter(|_| !quadrant || theme.pending_other)
    {
        diags.emit(
            Severity::Info,
            "I011",
            span,
            theme_variables_ignored(quadrant && theme.span.is_some()),
        );
    }
    theme.pending_other = false;
}

/// A quadrant colour: the accepted colour grammar, or three or six hex digits without
/// the `#`, which mermaid's point styles also accept.
pub(crate) fn quadrant_colour(v: &str) -> Option<crate::model::Color> {
    let v = v.trim();
    let bare = matches!(v.len(), 3 | 6) && v.bytes().all(|b| b.is_ascii_hexdigit());
    if bare {
        super::style::parse_color(&alloc::format!("#{}", v))
    } else {
        super::style::parse_color(v)
    }
}

/// `config.quadrantChart` (specs/quadrant.md#configuration).
fn apply_quadrant(entries: &[Entry], idx: &LineIndex, meta: &mut Meta, diags: &mut Diagnostics) {
    for e in entries {
        match e.key.as_str() {
            "xAxisPosition" => match enum_value(&e.value, &["top", "bottom"]).as_deref() {
                Some("top") => meta.quadrant.x_axis_position = Some(XAxisPosition::Top),
                Some(_) => meta.quadrant.x_axis_position = Some(XAxisPosition::Bottom),
                None => bad_value(e, idx, diags, "`top` or `bottom`"),
            },
            "yAxisPosition" => match enum_value(&e.value, &["left", "right"]).as_deref() {
                Some("left") => meta.quadrant.y_axis_position = Some(YAxisPosition::Left),
                Some(_) => meta.quadrant.y_axis_position = Some(YAxisPosition::Right),
                None => bad_value(e, idx, diags, "`left` or `right`"),
            },
            "useMaxWidth" => match bool_value(&e.value) {
                Some(true) => {}
                Some(false) => diags.emit(
                    Severity::Warning,
                    "W016",
                    idx.span(e.start, e.end),
                    "`useMaxWidth: false` has no effect: Merlion's SVG always scales to its container; ignored",
                ),
                None => bad_value(e, idx, diags, "`true` or `false`"),
            },
            name => match NUMERIC_KEYS.iter().find(|k| k.name == name) {
                Some(k) => {
                    let v = number_value(&e.value).filter(|v| *v >= k.min && *v <= k.max);
                    match (v, meta.quadrant.numeric(name)) {
                        (Some(v), Some(slot)) => *slot = Some(v),
                        _ => bad_value(
                            e,
                            idx,
                            diags,
                            &alloc::format!("a number from {} to {}", k.min, k.max),
                        ),
                    }
                }
                None => unknown_key(e, "quadrantChart.", idx, diags),
            },
        }
    }
}

/// A finite number: a JSON number, or a YAML scalar in the plain decimal grammar.
fn number_value(v: &Value) -> Option<f64> {
    match v {
        Value::Num(n) if n.is_finite() => Some(*n),
        Value::Str(s) => super::style::parse_number(s.trim()),
        _ => None,
    }
}

/// A boolean: JSON `true` / `false`, or the YAML scalars `true` / `false`.
fn bool_value(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Str(s) if s == "true" => Some(true),
        Value::Str(s) if s == "false" => Some(false),
        _ => None,
    }
}

/// The value as an owned string when it is a string in `allowed`.
fn enum_value(v: &Value, allowed: &[&str]) -> Option<String> {
    match v {
        Value::Str(s) if allowed.contains(&s.as_str()) => Some(s.clone()),
        _ => None,
    }
}

fn bad_value(e: &Entry, idx: &LineIndex, diags: &mut Diagnostics, expected: &str) {
    diags.emit(
        Severity::Warning,
        "W016",
        idx.span(e.start, e.end),
        alloc::format!("`{}` must be {}; ignored", excerpt(&e.key), expected),
    );
}

fn unknown_key(e: &Entry, prefix: &str, idx: &LineIndex, diags: &mut Diagnostics) {
    diags.emit(
        Severity::Warning,
        "W016",
        idx.span(e.start, e.end),
        alloc::format!("unknown configuration key `{}{}`; ignored", prefix, e.key),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, value: Value) -> Entry {
        Entry {
            key: String::from(key),
            start: 0,
            end: key.len(),
            value,
        }
    }

    fn s(v: &str) -> Value {
        Value::Str(String::from(v))
    }

    fn codes(d: &Diagnostics) -> Vec<(&'static str, Severity)> {
        d.items.iter().map(|x| (x.code, x.severity)).collect()
    }

    #[test]
    fn front_matter_keys() {
        let idx = LineIndex::new("x");
        let mut meta = Meta::default();
        let mut d = Diagnostics::new(false);
        let fm = alloc::vec![
            entry("title", s("T")),
            entry("accTitle", s("AT")),
            entry("accDescr", s("AD")),
            entry(
                "config",
                Value::Map(alloc::vec![
                    entry("layout", s("elk")),
                    entry(
                        "flowchart",
                        Value::Map(alloc::vec![entry("curve", s("stepAfter"))])
                    ),
                ])
            ),
        ];
        apply_front_matter(&fm, &idx, &mut meta, &mut d);
        assert!(d.items.is_empty(), "{:?}", d.items);
        assert_eq!(meta.title.as_deref(), Some("T"));
        assert_eq!(meta.acc_title.as_deref(), Some("AT"));
        assert_eq!(meta.acc_descr.as_deref(), Some("AD"));
        assert_eq!(meta.layout.as_deref(), Some("elk"));
        assert_eq!(meta.curve.as_deref(), Some("stepAfter"));
    }

    #[test]
    fn merlion_auto_tone_key() {
        let idx = LineIndex::new("x");
        for (v, want) in [
            (Value::Bool(false), Some(false)),
            (s("false"), Some(false)),
            (Value::Bool(true), Some(true)),
            (s("true"), Some(true)),
        ] {
            let mut meta = Meta::default();
            let mut d = Diagnostics::new(false);
            let cfg = alloc::vec![entry(
                "merlion",
                Value::Map(alloc::vec![entry("autoTone", v)])
            )];
            apply_config(&cfg, &idx, &mut meta, &mut d);
            assert!(d.items.is_empty(), "{:?}", d.items);
            assert_eq!(meta.auto_tone, want);
        }
        let mut meta = Meta::default();
        let mut d = Diagnostics::new(false);
        let cfg = alloc::vec![
            entry(
                "merlion",
                Value::Map(alloc::vec![
                    entry("autoTone", s("no")),
                    entry("palette", s("x")),
                ])
            ),
            entry("merlion", Value::Bool(false)),
        ];
        apply_config(&cfg, &idx, &mut meta, &mut d);
        assert_eq!(codes(&d), alloc::vec![("W016", Severity::Warning); 3]);
        assert!(d.items[1].message.contains("`merlion.palette`"));
        assert_eq!(meta, Meta::default());
    }

    #[test]
    fn ignored_and_rejected_keys() {
        let idx = LineIndex::new("x");
        let mut meta = Meta::default();
        let mut d = Diagnostics::new(false);
        let cfg = alloc::vec![
            entry("theme", s("dark")),
            entry("themeVariables", Value::Map(Vec::new())),
            entry("look", s("handDrawn")),
            entry("layout", s("circo")),
            entry("securityLevel", s("loose")),
            entry(
                "flowchart",
                Value::Map(alloc::vec![
                    entry("curve", s("wobbly")),
                    entry("htmlLabels", Value::Bool(false)),
                ])
            ),
            entry("flowchart", s("x")),
        ];
        apply_config(&cfg, &idx, &mut meta, &mut d);
        finish_theme_variables(&mut meta, false, &mut d);
        assert_eq!(
            codes(&d),
            alloc::vec![
                ("I011", Severity::Info),
                ("I011", Severity::Info),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("I011", Severity::Info),
            ]
        );
        assert_eq!(meta, Meta::default());
    }

    #[test]
    fn unknown_front_matter_key_and_non_string_title() {
        let idx = LineIndex::new("x");
        let mut meta = Meta::default();
        let mut d = Diagnostics::new(true);
        let fm = alloc::vec![
            entry("unsupportedSetting", s("compact")),
            entry("title", Value::List(Vec::new())),
            entry("config", s("nope")),
        ];
        apply_front_matter(&fm, &idx, &mut meta, &mut d);
        // Strict mode promotes the warnings to errors.
        assert_eq!(
            codes(&d),
            alloc::vec![
                ("W016", Severity::Error),
                ("W016", Severity::Error),
                ("W016", Severity::Error),
            ]
        );
        assert_eq!(meta.title, None);
    }
}
