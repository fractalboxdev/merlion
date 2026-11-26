//! Checks every cell against its axis and resolves marks to drawable values
//! (specs/chart.md#values).
//!
//! A linear value is a JSON number or a decimal string (`"-3.15"`); a time value is an
//! ISO 8601 date in UTC; a band value is a string or a number lexeme. Every drawn value
//! keeps the exact text it came from, which the SVG carries in `data-merlion-*`
//! attributes and the outline prints, so a reader can check a mark against its source.

use alloc::string::String;
use alloc::vec::Vec;

use super::json;
use super::spec::{Axis, Cell, Channel, Lit, Mark, MarkType, Row, ScaleType, Spec, Tone};
use crate::diag::{Diagnostics, Severity};
use crate::model::gantt::calendar;

pub const E_VALUE: &str = "E022";
pub const E_SERIES: &str = "E023";
pub const I_SKIPPED: &str = "I041";

/// A value placed on an axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pos {
    /// Index into the axis categories.
    Cat(usize),
    /// A linear value, or UTC milliseconds on a time axis.
    Num(f64),
}

impl Pos {
    pub fn num(self) -> f64 {
        match self {
            Pos::Cat(i) => i as f64,
            Pos::Num(v) => v,
        }
    }
}

/// A resolved value and the text it was written as.
#[derive(Clone, Debug, PartialEq)]
pub struct Val {
    pub pos: Pos,
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The data row, or `None` for a constant (`{"value": …}`) rule or span.
    pub row: Option<usize>,
    pub x: Option<Val>,
    pub y: Option<Val>,
    pub x2: Option<Val>,
    pub y2: Option<Val>,
    /// Index into [`Resolved::series`].
    pub series: Option<usize>,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkData {
    pub kind: MarkType,
    pub tone: Tone,
    pub items: Vec<Item>,
    /// Series indices this mark draws, in series order (bar groups are laid out by it).
    pub series: Vec<usize>,
    /// A bar or interval whose category axis is `y`.
    pub horizontal: bool,
    pub text: Option<String>,
    pub thickness: Option<f64>,
    pub dash: bool,
    pub has_labels: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisData {
    pub scale: ScaleType,
    /// Band categories in drawing order.
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub x: AxisData,
    pub y: AxisData,
    pub marks: Vec<MarkData>,
    pub series: Vec<String>,
    /// Fields any mark reads, in first-use order (the outline's columns).
    pub fields: Vec<String>,
    /// The axis domains authored in the spec, resolved.
    pub x_domain: Option<(f64, f64)>,
    pub y_domain: Option<(f64, f64)>,
}

/// `-?[0-9]+(\.[0-9]+)?`: the decimal grammar of a value written as a string.
pub fn decimal(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let int = i;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == int {
        return None;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        let frac = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == frac {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn two(b: &[u8], at: usize) -> Option<u8> {
    let d = |k: usize| b.get(k).filter(|c| c.is_ascii_digit()).map(|c| c - b'0');
    Some(d(at)? * 10 + d(at + 1)?)
}

/// `YYYY`, `YYYY-MM`, `YYYY-MM-DD`, or `YYYY-MM-DDTHH:MM[:SS]Z`, as UTC milliseconds.
pub fn iso_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let year = s.get(0..4)?.parse::<i32>().ok().filter(|y| *y >= 1)?;
    if !b.get(0..4)?.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let (month, day, rest) = match b.len() {
        4 => (1, 1, 4),
        _ => {
            if b.get(4) != Some(&b'-') {
                return None;
            }
            let month = two(b, 5)?;
            if b.len() == 7 {
                (month, 1, 7)
            } else {
                if b.get(7) != Some(&b'-') {
                    return None;
                }
                (month, two(b, 8)?, 10)
            }
        }
    };
    let days = calendar::days_from_civil(year, month, day)?;
    if day > calendar::month_days(year, month)? {
        return None;
    }
    let mut ms = days.checked_mul(calendar::DAY_MS)?;
    if rest == b.len() {
        return Some(ms);
    }
    if rest != 10 || b.get(10) != Some(&b'T') || b.last() != Some(&b'Z') {
        return None;
    }
    let hour = two(b, 11)?;
    if b.get(13) != Some(&b':') {
        return None;
    }
    let minute = two(b, 14)?;
    let second = match b.len() {
        17 => 0,
        20 if b.get(16) == Some(&b':') => two(b, 17)?,
        _ => return None,
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    ms += (i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second)) * 1_000;
    Some(ms)
}

struct Ctx<'a> {
    src: &'a str,
    diags: &'a mut Diagnostics,
}

impl Ctx<'_> {
    fn error(&mut self, code: &'static str, start: usize, end: usize, message: String) {
        self.diags.emit(
            Severity::Error,
            code,
            json::span(self.src, start, end),
            message,
        );
    }
}

/// The text a cell is written as, or `None` for null.
fn cell_text(c: &Cell) -> Option<(&str, bool)> {
    match c {
        Cell::Null => None,
        Cell::Str(s) => Some((s, false)),
        Cell::Num(n) => Some((n, true)),
    }
}

/// Reads `text` as a value on `axis` (categories are resolved later).
fn read_value(
    cx: &mut Ctx,
    axis: &Axis,
    text: &str,
    is_number: bool,
    at: (usize, usize),
    path: &str,
) -> Option<Val> {
    let pos = match axis.scale {
        ScaleType::Band => Pos::Cat(usize::MAX),
        ScaleType::Linear => {
            let v = if is_number {
                text.parse::<f64>().ok().filter(|v| v.is_finite())
            } else {
                decimal(text)
            };
            match v {
                Some(v) => Pos::Num(v),
                None => {
                    cx.error(
                        E_VALUE,
                        at.0,
                        at.1,
                        alloc::format!(
                            "`{}`: expected a number or a decimal string such as \"-3.15\", found \"{}\"",
                            path,
                            crate::diag::excerpt(text)
                        ),
                    );
                    return None;
                }
            }
        }
        ScaleType::Time => match (!is_number).then(|| iso_time(text)).flatten() {
            Some(ms) => Pos::Num(ms as f64),
            None => {
                cx.error(
                    E_VALUE,
                    at.0,
                    at.1,
                    alloc::format!(
                        "`{}`: expected an ISO 8601 date such as \"2026-11-20\" (UTC), found \"{}\"",
                        path,
                        crate::diag::excerpt(text)
                    ),
                );
                return None;
            }
        },
    };
    Some(Val {
        pos,
        raw: String::from(text),
    })
}

fn lit_value(cx: &mut Ctx, axis: &Axis, lit: &Lit, path: &str) -> Option<Val> {
    read_value(
        cx,
        axis,
        &lit.text,
        lit.is_number,
        (lit.start, lit.end),
        path,
    )
}

/// Which axis a mark channel reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ch {
    X,
    Y,
    X2,
    Y2,
}

impl Ch {
    fn name(self) -> &'static str {
        match self {
            Ch::X => "x",
            Ch::Y => "y",
            Ch::X2 => "x2",
            Ch::Y2 => "y2",
        }
    }
}

fn channel(mark: &Mark, ch: Ch) -> Option<&Channel> {
    match ch {
        Ch::X => mark.x.as_ref(),
        Ch::Y => mark.y.as_ref(),
        Ch::X2 => mark.x2.as_ref(),
        Ch::Y2 => mark.y2.as_ref(),
    }
}

/// Axis-role rules of each mark type (specs/chart.md#marks).
fn check_roles(cx: &mut Ctx, spec: &Spec, i: usize, m: &Mark) -> bool {
    use MarkType::*;
    let (xs, ys) = (spec.x.scale, spec.y.scale);
    let path = alloc::format!("/marks/{}", i);
    let fail = |cx: &mut Ctx, msg: &str| {
        cx.error(
            super::spec::E_SPEC,
            m.start,
            m.end,
            alloc::format!("`{}`: {}", path, msg),
        );
        false
    };
    let continuous = |s: ScaleType| s != ScaleType::Band;
    match m.kind {
        Bar => {
            let ok = (xs == ScaleType::Band && ys == ScaleType::Linear)
                || (ys == ScaleType::Band && xs == ScaleType::Linear);
            if !ok {
                return fail(cx, "a bar mark needs one band axis and one linear axis");
            }
            let value = if xs == ScaleType::Band {
                &spec.y
            } else {
                &spec.x
            };
            if value.zero == Some(false) {
                return fail(
                    cx,
                    "a bar mark draws from zero, so its value axis cannot set `zero: false`",
                );
            }
        }
        Line | Area => {
            if ys != ScaleType::Linear {
                return fail(cx, "a line or area mark needs a linear y axis");
            }
            if m.kind == Area && spec.y.zero == Some(false) && m.y2.is_none() {
                return fail(
                    cx,
                    "an area without `y2` fills to zero, so the y axis cannot set `zero: false`",
                );
            }
        }
        Dot => {}
        Interval => {
            let end_axis = if m.x2.is_some() { xs } else { ys };
            if !continuous(end_axis) {
                return fail(cx, "an interval runs along a linear or time axis");
            }
        }
        Rule | Span => {
            let axis = if m.x.is_some() { xs } else { ys };
            if !continuous(axis) {
                return fail(cx, "a rule or span sits on a linear or time axis");
            }
        }
    }
    if m.series.is_some() && m.tone != Tone::Series {
        return fail(
            cx,
            "`series` colours a mark by series; drop it or drop `tone`",
        );
    }
    true
}

pub fn resolve(src: &str, spec: &Spec, diags: &mut Diagnostics) -> Option<Resolved> {
    let mut cx = Ctx { src, diags };
    if spec.x.scale == spec.y.scale && spec.x.scale != ScaleType::Linear {
        cx.error(
            super::spec::E_SPEC,
            spec.y.start,
            spec.y.end,
            alloc::format!(
                "`/y`: x and y cannot both be {} axes",
                spec.x.scale.as_str()
            ),
        );
        return None;
    }
    let mut roles_ok = true;
    for (i, m) in spec.marks.iter().enumerate() {
        roles_ok &= check_roles(&mut cx, spec, i, m);
    }
    if !roles_ok {
        return None;
    }

    // Every referenced field must exist in at least one row: a misspelt field would
    // otherwise draw an empty mark.
    let mut fields: Vec<String> = Vec::new();
    for (i, m) in spec.marks.iter().enumerate() {
        let mut names: Vec<(&str, &str)> = Vec::new();
        for ch in [Ch::X, Ch::Y, Ch::X2, Ch::Y2] {
            if let Some(Channel::Field(f)) = channel(m, ch) {
                names.push((ch.name(), f));
            }
        }
        if let Some(Channel::Field(f)) = &m.series {
            names.push(("series", f));
        }
        if let Some(f) = &m.label {
            names.push(("label", f));
        }
        for (member, f) in names {
            if !spec.data.iter().any(|r| r.get(f).is_some()) {
                cx.error(
                    super::spec::E_SPEC,
                    m.start,
                    m.end,
                    alloc::format!(
                        "`/marks/{}/{}`: no row has the field `{}`",
                        i,
                        member,
                        crate::diag::excerpt(f)
                    ),
                );
            } else if !fields.iter().any(|g| g == f) {
                fields.push(String::from(f));
            }
        }
    }

    // The outline reads like a table: category columns first, then values.
    let on_band = |f: &str| {
        spec.marks.iter().any(|m| {
            [Ch::X, Ch::Y, Ch::X2, Ch::Y2].iter().any(|&ch| {
                let axis = if matches!(ch, Ch::X | Ch::X2) {
                    &spec.x
                } else {
                    &spec.y
                };
                axis.scale == ScaleType::Band
                    && matches!(channel(m, ch), Some(Channel::Field(g)) if g == f)
            })
        })
    };
    let (mut first, rest): (Vec<String>, Vec<String>) =
        fields.into_iter().partition(|f| on_band(f));
    first.extend(rest);
    let fields = first;

    let axis_of = |ch: Ch| {
        if matches!(ch, Ch::X | Ch::X2) {
            &spec.x
        } else {
            &spec.y
        }
    };

    // Band categories: the authored domain, else first appearance in mark, then row, order.
    let mut cats: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    for (k, axis) in [&spec.x, &spec.y].into_iter().enumerate() {
        if axis.scale != ScaleType::Band {
            continue;
        }
        if let Some(d) = &axis.domain {
            for l in d {
                if cats[k].contains(&l.text) {
                    cx.error(
                        super::spec::E_SPEC,
                        l.start,
                        l.end,
                        alloc::format!(
                            "`/{}/domain`: \"{}\" is listed twice",
                            if k == 0 { "x" } else { "y" },
                            crate::diag::excerpt(&l.text)
                        ),
                    );
                } else {
                    cats[k].push(l.text.clone());
                }
            }
        }
    }

    let mut series: Vec<String> = Vec::new();
    let explicit_series = spec.series_domain.is_some();
    if let Some(d) = &spec.series_domain {
        for l in d {
            if !series.contains(&l.text) {
                series.push(l.text.clone());
            }
        }
    }

    let x_domain = domain(&mut cx, &spec.x, "/x/domain");
    let y_domain = domain(&mut cx, &spec.y, "/y/domain");

    let mut marks = Vec::with_capacity(spec.marks.len());
    for (mi, m) in spec.marks.iter().enumerate() {
        let horizontal = match m.kind {
            MarkType::Bar => spec.y.scale == ScaleType::Band,
            MarkType::Interval => m.x2.is_some(),
            _ => false,
        };
        let row_based = [Ch::X, Ch::Y, Ch::X2, Ch::Y2]
            .iter()
            .any(|&c| matches!(channel(m, c), Some(Channel::Field(_))));
        let rows: Vec<Option<(usize, &Row)>> = if row_based {
            spec.data.iter().enumerate().map(Some).collect()
        } else {
            alloc::vec![None]
        };
        let mut items = Vec::new();
        let mut skipped = 0usize;
        for r in rows {
            let mut item = Item {
                row: r.map(|(i, _)| i),
                x: None,
                y: None,
                x2: None,
                y2: None,
                series: None,
                label: None,
            };
            let mut missing = false;
            for ch in [Ch::X, Ch::Y, Ch::X2, Ch::Y2] {
                let Some(c) = channel(m, ch) else { continue };
                let axis = axis_of(ch);
                let v = match (c, r) {
                    (Channel::Const(l), _) => lit_value(
                        &mut cx,
                        axis,
                        l,
                        &alloc::format!("/marks/{}/{}/value", mi, ch.name()),
                    ),
                    (Channel::Field(f), Some((ri, row))) => match row.get(f) {
                        Some((cell, s, e)) => match cell_text(cell) {
                            Some((t, num)) => read_value(
                                &mut cx,
                                axis,
                                t,
                                num,
                                (s, e),
                                &alloc::format!("/data/{}/{}", ri, f),
                            ),
                            None => {
                                missing = true;
                                None
                            }
                        },
                        None => {
                            missing = true;
                            None
                        }
                    },
                    (Channel::Field(_), None) => None,
                };
                if missing {
                    break;
                }
                let Some(v) = v else { continue };
                match ch {
                    Ch::X => item.x = Some(v),
                    Ch::Y => item.y = Some(v),
                    Ch::X2 => item.x2 = Some(v),
                    Ch::Y2 => item.y2 = Some(v),
                }
            }
            let named: Option<(String, usize, usize, String)> = match (&m.series, r) {
                (Some(Channel::Const(l)), _) => Some((
                    l.text.clone(),
                    l.start,
                    l.end,
                    alloc::format!("/marks/{}/series/value", mi),
                )),
                (Some(Channel::Field(f)), Some((ri, row))) => {
                    match row
                        .get(f)
                        .and_then(|(c, s, e)| cell_text(c).map(|t| (t.0, s, e)))
                    {
                        Some((name, s, e)) => Some((
                            String::from(name),
                            s,
                            e,
                            alloc::format!("/data/{}/{}", ri, f),
                        )),
                        None => {
                            missing = true;
                            None
                        }
                    }
                }
                _ => None,
            };
            if let Some((name, s, e, path)) = named {
                match series.iter().position(|n| *n == name) {
                    Some(k) => item.series = Some(k),
                    None if explicit_series => {
                        cx.error(
                            E_VALUE,
                            s,
                            e,
                            alloc::format!(
                                "`{}`: series \"{}\" is not in `/series/domain`",
                                path,
                                crate::diag::excerpt(&name)
                            ),
                        );
                    }
                    None => {
                        series.push(name);
                        item.series = Some(series.len() - 1);
                    }
                }
            }
            if let (Some(f), Some((_, row))) = (&m.label, r) {
                item.label = row
                    .get(f)
                    .and_then(|(c, ..)| cell_text(c))
                    .map(|(t, _)| String::from(t));
            }
            if missing {
                skipped += 1;
                continue;
            }
            items.push(item);
        }
        if skipped > 0 {
            cx.diags.emit(
                Severity::Info,
                I_SKIPPED,
                json::span(src, m.start, m.end),
                alloc::format!(
                    "`/marks/{}`: {} of {} rows have no value for this mark and are not drawn",
                    mi,
                    skipped,
                    spec.data.len()
                ),
            );
        }
        let mut used: Vec<usize> = items.iter().filter_map(|it| it.series).collect();
        used.sort_unstable();
        used.dedup();
        marks.push(MarkData {
            kind: m.kind,
            tone: m.tone,
            items,
            series: used,
            horizontal,
            text: m.text.clone(),
            thickness: m.thickness,
            dash: m.dash,
            has_labels: m.label.is_some(),
        });
    }
    if cx.diags.has_errors() {
        return None;
    }

    // Categories in first-appearance order, then index every band value.
    for (k, axis) in [&spec.x, &spec.y].into_iter().enumerate() {
        if axis.scale != ScaleType::Band {
            continue;
        }
        let explicit = axis.domain.is_some();
        for m in &mut marks {
            for it in &mut m.items {
                let vals = if k == 0 {
                    [it.x.as_mut(), it.x2.as_mut()]
                } else {
                    [it.y.as_mut(), it.y2.as_mut()]
                };
                for v in vals.into_iter().flatten() {
                    let idx = match cats[k].iter().position(|c| *c == v.raw) {
                        Some(i) => i,
                        None if explicit => {
                            let row = it.row.unwrap_or(0);
                            let at = spec.data.get(row).map_or((0, 0), |r| (r.start, r.end));
                            cx.error(
                                E_VALUE,
                                at.0,
                                at.1,
                                alloc::format!(
                                    "`/data/{}`: \"{}\" is not in `/{}/domain`",
                                    row,
                                    crate::diag::excerpt(&v.raw),
                                    if k == 0 { "x" } else { "y" }
                                ),
                            );
                            continue;
                        }
                        None => {
                            if cats[k].len() == super::MAX_CATEGORIES {
                                cx.error(
                                    E_VALUE,
                                    axis.start,
                                    axis.end,
                                    alloc::format!(
                                        "`/{}`: more than {} categories",
                                        if k == 0 { "x" } else { "y" },
                                        super::MAX_CATEGORIES
                                    ),
                                );
                                return None;
                            }
                            cats[k].push(v.raw.clone());
                            cats[k].len() - 1
                        }
                    };
                    v.pos = Pos::Cat(idx);
                }
            }
        }
    }
    if series.len() > super::MAX_SERIES {
        let at = spec.marks.iter().find(|m| m.series.is_some());
        cx.error(
            E_SERIES,
            at.map_or(0, |m| m.start),
            at.map_or(0, |m| m.end),
            alloc::format!(
                "{} series, but the categorical palette has {} colours: fold the smallest into \"Other\" or split the chart",
                series.len(),
                super::MAX_SERIES
            ),
        );
    }
    if cx.diags.has_errors() {
        return None;
    }
    // An authored domain is a promise about every value: a value outside it would draw
    // past the plot edge or be clipped, and a clipped bar misstates its value.
    for (axis_name, dom, on_x) in [("x", x_domain, true), ("y", y_domain, false)] {
        let Some((d0, d1)) = dom else { continue };
        for (mi, m) in marks.iter().enumerate() {
            for it in &m.items {
                let vals = if on_x {
                    [&it.x, &it.x2]
                } else {
                    [&it.y, &it.y2]
                };
                for v in vals.into_iter().flatten() {
                    let n = v.pos.num();
                    if n < d0 || n > d1 {
                        let (s, e) = match it.row.and_then(|r| spec.data.get(r)) {
                            Some(row) => (row.start, row.end),
                            None => spec.marks.get(mi).map_or((0, 0), |m| (m.start, m.end)),
                        };
                        cx.error(
                            E_VALUE,
                            s,
                            e,
                            alloc::format!(
                                "`/marks/{}`: {} lies outside `/{}/domain`",
                                mi,
                                crate::diag::excerpt(&v.raw),
                                axis_name
                            ),
                        );
                    }
                }
            }
        }
    }
    if cx.diags.has_errors() {
        return None;
    }
    let [xc, yc] = cats;
    Some(Resolved {
        x: AxisData {
            scale: spec.x.scale,
            categories: xc,
        },
        y: AxisData {
            scale: spec.y.scale,
            categories: yc,
        },
        marks,
        series,
        fields,
        x_domain,
        y_domain,
    })
}

/// The authored `[min, max]` of a linear or time axis.
fn domain(cx: &mut Ctx, axis: &Axis, path: &str) -> Option<(f64, f64)> {
    if axis.scale == ScaleType::Band {
        return None;
    }
    let d = axis.domain.as_ref()?;
    let lo = lit_value(cx, axis, d.first()?, &alloc::format!("{}/0", path))?;
    let hi = lit_value(cx, axis, d.get(1)?, &alloc::format!("{}/1", path))?;
    if lo.pos.num() >= hi.pos.num() {
        cx.error(
            E_VALUE,
            axis.start,
            axis.end,
            alloc::format!("`{}`: the minimum must be below the maximum", path),
        );
        return None;
    }
    Some((lo.pos.num(), hi.pos.num()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_strings_are_strict() {
        assert_eq!(decimal("-3.15"), Some(-3.15));
        assert_eq!(decimal("6538.76"), Some(6538.76));
        for bad in ["", "-", "1.", ".5", "1e3", "6,538.76", "+1", " 1", "1 "] {
            assert_eq!(decimal(bad), None, "{bad}");
        }
    }

    #[test]
    fn iso_times_are_utc() {
        assert_eq!(iso_time("1970-01-02"), Some(86_400_000));
        assert_eq!(iso_time("1970-02"), Some(31 * 86_400_000));
        assert_eq!(iso_time("1970"), Some(0));
        assert_eq!(iso_time("1970-01-01T01:00Z"), Some(3_600_000));
        assert_eq!(iso_time("1970-01-01T00:00:01Z"), Some(1_000));
        for bad in [
            "2026-02-30",
            "2026-13-01",
            "2026-11-20T10:00",
            "2026-11-20 10:00Z",
            "26-11-20",
            "2026/11/20",
        ] {
            assert_eq!(iso_time(bad), None, "{bad}");
        }
    }
}
