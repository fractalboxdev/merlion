//! The typed `merlion-chart/1` spec, read from JSON (specs/chart.md#spec).
//!
//! Reading checks shape only: member names, member types and the combinations each mark
//! accepts. An unknown member is an error, so a misspelt key never silently draws a
//! different chart. Cell values are checked against their axis in [`super::resolve`].

use alloc::string::String;
use alloc::vec::Vec;

use super::json::{self, Kind, Value};
use crate::diag::{Diagnostics, Severity};

/// Codes (specs/chart.md#diagnostics).
pub const E_JSON: &str = "E020";
pub const E_SPEC: &str = "E021";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleType {
    Band,
    Linear,
    Time,
}

impl ScaleType {
    pub fn as_str(self) -> &'static str {
        match self {
            ScaleType::Band => "band",
            ScaleType::Linear => "linear",
            ScaleType::Time => "time",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumberFormat {
    pub decimals: Option<u8>,
    pub prefix: String,
    pub suffix: String,
    pub group: bool,
}

/// A literal as written in the spec: a string or a number lexeme, with its byte span.
#[derive(Clone, Debug, PartialEq)]
pub struct Lit {
    pub text: String,
    pub is_number: bool,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub scale: ScaleType,
    pub label: Option<String>,
    pub domain: Option<Vec<Lit>>,
    pub zero: Option<bool>,
    pub format: NumberFormat,
    pub time_format: Option<String>,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Channel {
    Field(String),
    Const(Lit),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkType {
    Bar,
    Line,
    Area,
    Dot,
    Interval,
    Rule,
    Span,
}

impl MarkType {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkType::Bar => "bar",
            MarkType::Line => "line",
            MarkType::Area => "area",
            MarkType::Dot => "dot",
            MarkType::Interval => "interval",
            MarkType::Rule => "rule",
            MarkType::Span => "span",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Series,
    Neutral,
    Accent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub kind: MarkType,
    pub x: Option<Channel>,
    pub y: Option<Channel>,
    pub x2: Option<Channel>,
    pub y2: Option<Channel>,
    pub series: Option<Channel>,
    pub label: Option<String>,
    pub text: Option<String>,
    pub tone: Tone,
    pub thickness: Option<f64>,
    pub dash: bool,
    pub start: usize,
    pub end: usize,
}

/// One cell of a data row.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Str(String),
    /// A JSON number, as written.
    Num(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub cells: Vec<(String, Cell, usize, usize)>,
    pub start: usize,
    pub end: usize,
}

impl Row {
    pub fn get(&self, field: &str) -> Option<(&Cell, usize, usize)> {
        self.cells
            .iter()
            .find(|(k, ..)| k == field)
            .map(|(_, c, s, e)| (c, *s, *e))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub title: String,
    pub description: Option<String>,
    pub x: Axis,
    pub y: Axis,
    pub data: Vec<Row>,
    pub marks: Vec<Mark>,
    pub series_domain: Option<Vec<Lit>>,
    pub legend: Option<bool>,
    pub height: Option<f64>,
}

/// Limits of one spec (specs/chart.md#limits).
pub const MAX_ROWS: usize = 10_000;
pub const MAX_MARKS: usize = 16;
pub const MAX_FIELDS: usize = 64;
pub const MAX_TITLE: usize = 200;
pub const MAX_DESCRIPTION: usize = 2_000;
pub const MAX_LABEL: usize = 80;
pub const MAX_AFFIX: usize = 8;

struct Reader<'a> {
    src: &'a str,
    diags: &'a mut Diagnostics,
}

impl Reader<'_> {
    fn error(&mut self, start: usize, end: usize, path: &str, message: &str) {
        self.diags.emit(
            Severity::Error,
            E_SPEC,
            json::span(self.src, start, end),
            alloc::format!("`{}`: {}", path, message),
        );
    }

    fn members<'v>(&mut self, v: &'v Value, path: &str) -> Option<&'v [(String, Value)]> {
        match &v.kind {
            Kind::Obj(m) => Some(m),
            _ => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("expected an object, found {}", v.type_name()),
                );
                None
            }
        }
    }

    /// Reports every member of `members` whose name is not in `known`.
    fn known(&mut self, members: &[(String, Value)], known: &[&str], path: &str) {
        for (k, v) in members {
            if !known.contains(&k.as_str()) {
                let mut list = String::new();
                for (i, n) in known.iter().enumerate() {
                    if i > 0 {
                        list.push_str(", ");
                    }
                    list.push_str(n);
                }
                self.error(
                    v.start,
                    v.end,
                    &child(path, k),
                    &alloc::format!("unknown member; expected one of {}", list),
                );
            }
        }
    }

    fn string(&mut self, v: &Value, path: &str, max: usize) -> Option<String> {
        match &v.kind {
            Kind::Str(s) if s.trim().is_empty() => {
                self.error(v.start, v.end, path, "must not be empty");
                None
            }
            Kind::Str(s) if s.chars().count() > max => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("longer than {} characters", max),
                );
                None
            }
            Kind::Str(s) => Some(String::from(s.trim())),
            _ => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("expected a string, found {}", v.type_name()),
                );
                None
            }
        }
    }

    fn boolean(&mut self, v: &Value, path: &str) -> Option<bool> {
        match v.kind {
            Kind::Bool(b) => Some(b),
            _ => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("expected a boolean, found {}", v.type_name()),
                );
                None
            }
        }
    }

    fn number(&mut self, v: &Value, path: &str, lo: f64, hi: f64) -> Option<f64> {
        let parsed = match &v.kind {
            Kind::Num(n) => n.parse::<f64>().ok(),
            _ => None,
        };
        match parsed {
            Some(n) if n.is_finite() && (lo..=hi).contains(&n) => Some(n),
            Some(_) => {
                let mut range = String::new();
                crate::numfmt::push_num(&mut range, lo);
                range.push_str(" to ");
                crate::numfmt::push_num(&mut range, hi);
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("must be a number from {}", range),
                );
                None
            }
            None => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("expected a number, found {}", v.type_name()),
                );
                None
            }
        }
    }

    fn lit(&mut self, v: &Value, path: &str) -> Option<Lit> {
        match &v.kind {
            Kind::Str(s) => Some(Lit {
                text: s.clone(),
                is_number: false,
                start: v.start,
                end: v.end,
            }),
            Kind::Num(n) => Some(Lit {
                text: n.clone(),
                is_number: true,
                start: v.start,
                end: v.end,
            }),
            _ => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("expected a string or a number, found {}", v.type_name()),
                );
                None
            }
        }
    }

    fn lits(&mut self, v: &Value, path: &str, min: usize, max: usize) -> Option<Vec<Lit>> {
        let Kind::Arr(items) = &v.kind else {
            self.error(
                v.start,
                v.end,
                path,
                &alloc::format!("expected an array, found {}", v.type_name()),
            );
            return None;
        };
        if items.len() < min || items.len() > max {
            let msg = if min == max {
                alloc::format!("expected {} items, found {}", min, items.len())
            } else {
                alloc::format!("expected {} to {} items, found {}", min, max, items.len())
            };
            self.error(v.start, v.end, path, &msg);
            return None;
        }
        let mut out = Vec::with_capacity(items.len());
        let mut ok = true;
        for (i, it) in items.iter().enumerate() {
            match self.lit(it, &index(path, i)) {
                Some(l) => out.push(l),
                None => ok = false,
            }
        }
        ok.then_some(out)
    }

    fn format(&mut self, v: &Value, path: &str) -> Option<NumberFormat> {
        let m = self.members(v, path)?;
        self.known(m, &["decimals", "prefix", "suffix", "group"], path);
        let mut f = NumberFormat {
            group: true,
            ..NumberFormat::default()
        };
        for (k, v) in m {
            let p = child(path, k);
            match k.as_str() {
                "decimals" => match &v.kind {
                    Kind::Num(n) if n.len() == 1 && matches!(n.as_bytes()[0], b'0'..=b'6') => {
                        f.decimals = Some(n.as_bytes()[0] - b'0')
                    }
                    _ => self.error(v.start, v.end, &p, "must be a whole number from 0 to 6"),
                },
                "prefix" => f.prefix = self.affix(v, &p)?,
                "suffix" => f.suffix = self.affix(v, &p)?,
                "group" => f.group = self.boolean(v, &p)?,
                _ => {}
            }
        }
        Some(f)
    }

    fn affix(&mut self, v: &Value, path: &str) -> Option<String> {
        match &v.kind {
            Kind::Str(s) if s.chars().count() <= MAX_AFFIX => Some(s.clone()),
            Kind::Str(_) => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("longer than {} characters", MAX_AFFIX),
                );
                None
            }
            _ => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    &alloc::format!("expected a string, found {}", v.type_name()),
                );
                None
            }
        }
    }

    fn axis(&mut self, v: &Value, path: &str) -> Option<Axis> {
        let m = self.members(v, path)?;
        self.known(m, &["type", "label", "domain", "zero", "format"], path);
        let scale = match m.iter().find(|(k, _)| k == "type") {
            Some((_, t)) => match t.as_str() {
                Some("band") => ScaleType::Band,
                Some("linear") => ScaleType::Linear,
                Some("time") => ScaleType::Time,
                _ => {
                    self.error(
                        t.start,
                        t.end,
                        &child(path, "type"),
                        "expected \"band\", \"linear\" or \"time\"",
                    );
                    return None;
                }
            },
            None => {
                self.error(v.start, v.end, path, "missing member `type`");
                return None;
            }
        };
        let mut axis = Axis {
            scale,
            label: None,
            domain: None,
            zero: None,
            format: NumberFormat {
                group: true,
                ..NumberFormat::default()
            },
            time_format: None,
            start: v.start,
            end: v.end,
        };
        for (k, v) in m {
            let p = child(path, k);
            match k.as_str() {
                "label" => axis.label = self.string(v, &p, MAX_LABEL),
                "domain" => {
                    axis.domain = match scale {
                        ScaleType::Band => self.lits(v, &p, 1, super::MAX_CATEGORIES),
                        _ => self.lits(v, &p, 2, 2),
                    }
                }
                "zero" if scale == ScaleType::Linear => axis.zero = self.boolean(v, &p),
                "zero" => self.error(v.start, v.end, &p, "applies only to a linear axis"),
                "format" => match scale {
                    ScaleType::Linear => {
                        if let Some(f) = self.format(v, &p) {
                            axis.format = f;
                        }
                    }
                    ScaleType::Time => {
                        let f = self.string(v, &p, 32);
                        if let Some(f) = &f {
                            if crate::model::gantt::calendar::format_axis(0, f).is_none() {
                                self.error(v.start, v.end, &p, "unsupported time format");
                            }
                        }
                        axis.time_format = f;
                    }
                    ScaleType::Band => {
                        self.error(v.start, v.end, &p, "does not apply to a band axis")
                    }
                },
                _ => {}
            }
        }
        Some(axis)
    }

    fn channel(&mut self, v: &Value, path: &str) -> Option<Channel> {
        match &v.kind {
            Kind::Str(s) if !s.is_empty() && s.chars().count() <= 64 => {
                Some(Channel::Field(s.clone()))
            }
            Kind::Obj(m) => {
                self.known(m, &["value"], path);
                match m.iter().find(|(k, _)| k == "value") {
                    Some((_, c)) => self.lit(c, &child(path, "value")).map(Channel::Const),
                    None => {
                        self.error(v.start, v.end, path, "missing member `value`");
                        None
                    }
                }
            }
            _ => {
                self.error(
                    v.start,
                    v.end,
                    path,
                    "expected a field name or {\"value\": …}",
                );
                None
            }
        }
    }

    fn field(&mut self, v: &Value, path: &str) -> Option<String> {
        match &v.kind {
            Kind::Str(s) if !s.is_empty() && s.chars().count() <= 64 => Some(s.clone()),
            _ => {
                self.error(v.start, v.end, path, "expected a field name");
                None
            }
        }
    }

    fn mark(&mut self, v: &Value, path: &str) -> Option<Mark> {
        let m = self.members(v, path)?;
        self.known(
            m,
            &[
                "type",
                "x",
                "y",
                "x2",
                "y2",
                "series",
                "label",
                "text",
                "tone",
                "thickness",
                "dash",
            ],
            path,
        );
        let kind = match m.iter().find(|(k, _)| k == "type") {
            Some((_, t)) => match t.as_str() {
                Some("bar") => MarkType::Bar,
                Some("line") => MarkType::Line,
                Some("area") => MarkType::Area,
                Some("dot") => MarkType::Dot,
                Some("interval") => MarkType::Interval,
                Some("rule") => MarkType::Rule,
                Some("span") => MarkType::Span,
                _ => {
                    self.error(
                        t.start,
                        t.end,
                        &child(path, "type"),
                        "expected \"bar\", \"line\", \"area\", \"dot\", \"interval\", \"rule\" or \"span\"",
                    );
                    return None;
                }
            },
            None => {
                self.error(v.start, v.end, path, "missing member `type`");
                return None;
            }
        };
        let mut mark = Mark {
            kind,
            x: None,
            y: None,
            x2: None,
            y2: None,
            series: None,
            label: None,
            text: None,
            tone: Tone::Series,
            thickness: None,
            dash: false,
            start: v.start,
            end: v.end,
        };
        for (k, v) in m {
            let p = child(path, k);
            match k.as_str() {
                "x" => mark.x = self.channel(v, &p),
                "y" => mark.y = self.channel(v, &p),
                "x2" => mark.x2 = self.channel(v, &p),
                "y2" => mark.y2 = self.channel(v, &p),
                "series" => mark.series = self.channel(v, &p),
                "label" => mark.label = self.field(v, &p),
                "text" => mark.text = self.string(v, &p, MAX_LABEL),
                "tone" => match v.as_str() {
                    Some("series") => mark.tone = Tone::Series,
                    Some("neutral") => mark.tone = Tone::Neutral,
                    Some("accent") => mark.tone = Tone::Accent,
                    _ => self.error(
                        v.start,
                        v.end,
                        &p,
                        "expected \"series\", \"neutral\" or \"accent\"",
                    ),
                },
                "thickness" => mark.thickness = self.number(v, &p, 1.0, 40.0),
                "dash" => mark.dash = self.boolean(v, &p).unwrap_or(false),
                _ => {}
            }
        }
        self.check_mark(&mark, m, path);
        Some(mark)
    }

    /// The members each mark type requires and accepts (specs/chart.md#marks).
    fn check_mark(&mut self, mark: &Mark, m: &[(String, Value)], path: &str) {
        use MarkType::*;
        let has = |k: &str| m.iter().any(|(n, _)| n == k);
        let require = |r: &mut Self, k: &str| {
            if !has(k) {
                r.error(
                    mark.start,
                    mark.end,
                    path,
                    &alloc::format!("a {} mark needs `{}`", mark.kind.as_str(), k),
                );
            }
        };
        let allowed: &[&str] = match mark.kind {
            Bar => &["type", "x", "y", "series", "label", "tone"],
            Line => &["type", "x", "y", "series", "label", "tone", "dash"],
            Area => &["type", "x", "y", "y2", "series", "tone"],
            Dot => &["type", "x", "y", "series", "label", "tone"],
            Interval => &[
                "type",
                "x",
                "y",
                "x2",
                "y2",
                "series",
                "label",
                "tone",
                "thickness",
            ],
            Rule => &["type", "x", "y", "text", "tone", "dash", "thickness"],
            Span => &["type", "x", "y", "x2", "y2", "text"],
        };
        for (k, v) in m {
            if !allowed.contains(&k.as_str())
                && [
                    "type",
                    "x",
                    "y",
                    "x2",
                    "y2",
                    "series",
                    "label",
                    "text",
                    "tone",
                    "thickness",
                    "dash",
                ]
                .contains(&k.as_str())
            {
                self.error(
                    v.start,
                    v.end,
                    &child(path, k),
                    &alloc::format!("does not apply to a {} mark", mark.kind.as_str()),
                );
            }
        }
        match mark.kind {
            Bar | Line | Area | Dot => {
                require(self, "x");
                require(self, "y");
            }
            Interval => {
                require(self, "x");
                require(self, "y");
                if has("x2") == has("y2") {
                    self.error(
                        mark.start,
                        mark.end,
                        path,
                        "an interval mark needs exactly one of `x2` and `y2`",
                    );
                }
            }
            Rule => {
                if has("x") == has("y") {
                    self.error(
                        mark.start,
                        mark.end,
                        path,
                        "a rule mark needs exactly one of `x` and `y`",
                    );
                }
            }
            Span => {
                let xs = has("x") && has("x2");
                let ys = has("y") && has("y2");
                let any_x = has("x") || has("x2");
                let any_y = has("y") || has("y2");
                if xs == ys || (xs && any_y) || (ys && any_x) {
                    self.error(
                        mark.start,
                        mark.end,
                        path,
                        "a span mark needs `x` and `x2`, or `y` and `y2`",
                    );
                }
            }
        }
    }

    fn row(&mut self, v: &Value, path: &str) -> Option<Row> {
        let m = self.members(v, path)?;
        if m.len() > MAX_FIELDS {
            self.error(
                v.start,
                v.end,
                path,
                &alloc::format!("more than {} fields", MAX_FIELDS),
            );
            return None;
        }
        let mut cells = Vec::with_capacity(m.len());
        for (k, c) in m {
            let cell = match &c.kind {
                Kind::Null => Cell::Null,
                Kind::Str(s) => Cell::Str(s.clone()),
                Kind::Num(n) => Cell::Num(n.clone()),
                _ => {
                    self.error(
                        c.start,
                        c.end,
                        &child(path, k),
                        &alloc::format!(
                            "expected a string, a number or null, found {}",
                            c.type_name()
                        ),
                    );
                    continue;
                }
            };
            cells.push((k.clone(), cell, c.start, c.end));
        }
        Some(Row {
            cells,
            start: v.start,
            end: v.end,
        })
    }
}

pub(super) fn child(path: &str, key: &str) -> String {
    let mut p = String::from(path);
    p.push('/');
    // RFC 6901 escapes.
    for c in key.chars() {
        match c {
            '~' => p.push_str("~0"),
            '/' => p.push_str("~1"),
            c => p.push(c),
        }
    }
    p
}

fn index(path: &str, i: usize) -> String {
    alloc::format!("{}/{}", path, i)
}

/// Reads a spec. Errors go to `diags`; `None` means the spec cannot be drawn.
pub fn read(src: &str, max_depth: usize, diags: &mut Diagnostics) -> Option<Spec> {
    let root = match json::parse(src, max_depth) {
        Ok(v) => v,
        Err(e) => {
            diags.emit(
                Severity::Error,
                E_JSON,
                json::span(src, e.at, e.at + 1),
                alloc::format!("invalid JSON: {}", e.message),
            );
            return None;
        }
    };
    let mut r = Reader { src, diags };
    let m = r.members(&root, "")?;
    r.known(
        m,
        &[
            "chart",
            "title",
            "description",
            "x",
            "y",
            "data",
            "marks",
            "series",
            "legend",
            "height",
        ],
        "",
    );
    let get = |k: &str| m.iter().find(|(n, _)| n == k).map(|(_, v)| v);
    match get("chart") {
        Some(v) if matches!(&v.kind, Kind::Num(n) if n == "1") => {}
        Some(v) => r.error(v.start, v.end, "/chart", "expected the format version 1"),
        None => r.error(
            root.start,
            root.end,
            "",
            "missing member `chart` (the format version, 1)",
        ),
    }
    let missing = |r: &mut Reader, k: &str| {
        r.error(
            root.start,
            root.start + 1,
            "",
            &alloc::format!("missing member `{}`", k),
        )
    };
    let title = match get("title") {
        Some(v) => r.string(v, "/title", MAX_TITLE),
        None => {
            missing(&mut r, "title");
            None
        }
    };
    let description = get("description").and_then(|v| r.string(v, "/description", MAX_DESCRIPTION));
    let x = match get("x") {
        Some(v) => r.axis(v, "/x"),
        None => {
            missing(&mut r, "x");
            None
        }
    };
    let y = match get("y") {
        Some(v) => r.axis(v, "/y"),
        None => {
            missing(&mut r, "y");
            None
        }
    };
    let mut data = Vec::new();
    match get("data") {
        Some(v) => match &v.kind {
            Kind::Arr(items) if items.is_empty() => {
                r.error(v.start, v.end, "/data", "needs at least one row")
            }
            Kind::Arr(items) if items.len() > MAX_ROWS => r.error(
                v.start,
                v.end,
                "/data",
                &alloc::format!("more than {} rows", MAX_ROWS),
            ),
            Kind::Arr(items) => {
                for (i, it) in items.iter().enumerate() {
                    if let Some(row) = r.row(it, &index("/data", i)) {
                        data.push(row);
                    }
                }
            }
            _ => r.error(
                v.start,
                v.end,
                "/data",
                &alloc::format!("expected an array of rows, found {}", v.type_name()),
            ),
        },
        None => missing(&mut r, "data"),
    }
    let mut marks = Vec::new();
    match get("marks") {
        Some(v) => match &v.kind {
            Kind::Arr(items) if items.is_empty() || items.len() > MAX_MARKS => r.error(
                v.start,
                v.end,
                "/marks",
                &alloc::format!("expected 1 to {} marks, found {}", MAX_MARKS, items.len()),
            ),
            Kind::Arr(items) => {
                for (i, it) in items.iter().enumerate() {
                    if let Some(mk) = r.mark(it, &index("/marks", i)) {
                        marks.push(mk);
                    }
                }
            }
            _ => r.error(
                v.start,
                v.end,
                "/marks",
                &alloc::format!("expected an array of marks, found {}", v.type_name()),
            ),
        },
        None => missing(&mut r, "marks"),
    }
    let series_domain = match get("series") {
        Some(v) => match r.members(v, "/series") {
            Some(sm) => {
                r.known(sm, &["domain"], "/series");
                sm.iter()
                    .find(|(k, _)| k == "domain")
                    .and_then(|(_, d)| r.lits(d, "/series/domain", 1, super::MAX_SERIES))
            }
            None => None,
        },
        None => None,
    };
    let legend = get("legend").and_then(|v| r.boolean(v, "/legend"));
    let height = get("height").and_then(|v| r.number(v, "/height", 120.0, 2_000.0));
    if r.diags.has_errors() {
        return None;
    }
    Some(Spec {
        title: title?,
        description,
        x: x?,
        y: y?,
        data,
        marks,
        series_domain,
        legend,
        height,
    })
}
