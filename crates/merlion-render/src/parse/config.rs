//! Configuration from front matter and `%%{init}%%` (specs/parser.md#front-matter-and-directives).
//!
//! Both sources parse into the same [`Value`] tree. Every accepted key takes an
//! enumerated value; anything else is ignored with a diagnostic:
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | `W016` | Warning | Unknown key, or a value outside the key's set, ignored |
//! | `I011` | Info | `theme`, `themeVariables` or `look` ignored: themes are CSS |

use alloc::string::String;
use alloc::vec::Vec;

use super::cursor::LineIndex;
use crate::diag::{excerpt, Diagnostics, Severity};
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
            "theme" | "themeVariables" | "look" => diags.emit(
                Severity::Info,
                "I011",
                idx.span(e.start, e.end),
                alloc::format!(
                    "`{}` is ignored: Merlion themes are CSS custom properties",
                    excerpt(&e.key)
                ),
            ),
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
            _ => unknown_key(e, "", idx, diags),
        }
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
        assert_eq!(
            codes(&d),
            alloc::vec![
                ("I011", Severity::Info),
                ("I011", Severity::Info),
                ("I011", Severity::Info),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
                ("W016", Severity::Warning),
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
            entry("displayMode", s("compact")),
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
