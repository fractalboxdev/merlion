//! `quadrantChart` parsing (specs/quadrant.md#syntax). One statement per line, or per
//! `;` outside a quoted string; keywords match in any case, as in mermaid's lexer.
//! `title`, `accTitle` and `accDescr` take the rest of their line.
//!
//! Diagnostic codes this module adds to the table in specs/parser.md#codes:
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | `E024` | Error | A point coordinate outside 0–1 |
//! | `R021` | Repair | A coordinate in 0–1 written as mermaid's lexer rejects (`1.0`, `.5`) becomes the form it accepts |

use super::{cursor::LineIndex, label, style, ParseOptions, Stop};
use crate::diag::{excerpt, Diagnostic, Diagnostics, Fix, Severity, Span};
use crate::model::quadrant::{Point, PointClass, PointStyle, Quadrant};
use crate::model::{Color, Meta};
use alloc::{format, string::String, vec::Vec};

/// What the parser expects when a statement is none of the quadrant statements.
const EXPECTED: &str = "expected `title`, `x-axis`, `y-axis`, `quadrant-1` … `quadrant-4`, `classDef` or a point `Name: [x, y]`";

fn fail(diags: &mut Diagnostics, span: Span, message: impl Into<String>) -> Stop {
    diags.emit(Severity::Error, "E002", span, message);
    Stop::Failed
}

/// Normalised label text within the label byte limit (`W012`).
fn bounded(value: &str, opts: &ParseOptions, span: Span, diags: &mut Diagnostics) -> String {
    let mut text = label::clean_label(value);
    if label::truncate(&mut text, opts.limits.label_bytes) {
        diags.emit(
            Severity::Warning,
            "W012",
            span,
            "quadrant chart label exceeds the label byte limit; truncated",
        );
    }
    text
}

/// An axis, quadrant or point text: a `"…"` string loses its quotes, then the label
/// normalisation applies (a `` "`…`" `` Markdown string also loses its backticks).
fn text_value(raw: &str, opts: &ParseOptions, span: Span, diags: &mut Diagnostics) -> String {
    let t = raw.trim();
    let inner = if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t.get(1..t.len() - 1).unwrap_or("")
    } else {
        t
    };
    bounded(inner, opts, span, diags)
}

/// The text after keyword `kw` when `s` starts with it in any case and a space, tab or
/// the end follows.
fn keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let head = s.get(..kw.len())?;
    if !head.eq_ignore_ascii_case(kw) {
        return None;
    }
    let rest = s.get(kw.len()..)?;
    (rest.is_empty() || rest.starts_with([' ', '\t'])).then_some(rest)
}

/// The text after `kw` and a `:` (`accTitle: …`), spaces allowed before the colon.
fn colon_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let head = s.get(..kw.len())?;
    if !head.eq_ignore_ascii_case(kw) {
        return None;
    }
    s.get(kw.len()..)?.trim_start().strip_prefix(':')
}

/// Whether a statement takes the rest of its line, `;` included.
fn takes_line(s: &str) -> bool {
    let s = s.trim_start();
    keyword(s, "title").is_some()
        || colon_keyword(s, "accTitle").is_some()
        || colon_keyword(s, "accDescr").is_some()
        || s.get(..8)
            .is_some_and(|h| h.eq_ignore_ascii_case("accDescr"))
}

/// Statements of one line: split at `;` outside a quoted string, cut at a `%%` comment
/// outside one.
fn statements(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let (mut start, mut quote, mut i) = (0usize, false, 0usize);
    let mut end = line.len();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => quote = !quote,
            b'%' if !quote
                && bytes.get(i + 1) == Some(&b'%')
                && !takes_line(line.get(start..i).unwrap_or("")) =>
            {
                end = i;
                break;
            }
            b';' if !quote && !takes_line(line.get(start..i).unwrap_or("")) => {
                out.push(line.get(start..i).unwrap_or(""));
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(line.get(start..end).unwrap_or(""));
    out
}

/// Byte offset of `part` inside the source, when it is a slice of it.
fn offset(idx: &LineIndex, part: &str) -> usize {
    (part.as_ptr() as usize).saturating_sub(idx.src().as_ptr() as usize)
}

fn span_of(idx: &LineIndex, part: &str) -> Span {
    let start = offset(idx, part);
    idx.span(start, start + part.len())
}

/// The position of the first `--…->` delimiter outside a quoted string, and its length.
fn axis_delimiter(s: &str) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut quote = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => quote = !quote,
            b'-' if !quote && bytes.get(i + 1) == Some(&b'-') => {
                let mut j = i;
                while bytes.get(j) == Some(&b'-') {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'>') {
                    return Some((i, j + 1 - i));
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `x-axis` / `y-axis`: (low, high) labels.
fn axis(
    rest: &str,
    name: &str,
    span: Span,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<(Option<String>, Option<String>), Stop> {
    let (low, high) = match axis_delimiter(rest) {
        Some((at, len)) => (
            rest.get(..at).unwrap_or(""),
            Some(rest.get(at + len..).unwrap_or("")),
        ),
        None => (rest, None),
    };
    if low.trim().is_empty() {
        return Err(fail(
            diags,
            span,
            format!("`{name}` needs a label before `-->`"),
        ));
    }
    let low_text = text_value(low, opts, span, diags);
    Ok(match high.map(str::trim) {
        // The one-sided form `x-axis Low -->` marks the direction on its only label.
        Some("") => (Some(format!("{low_text} \u{2192}")), None),
        Some(h) => (Some(low_text), Some(text_value(h, opts, span, diags))),
        None => (Some(low_text), None),
    })
}

/// Records `I030 FixedColour` once per diagram.
fn fixed_colour(diags: &mut Diagnostics, span: Span) {
    diags.emit_once(Severity::Info, "I030", span, style::FIXED_COLOUR_MESSAGE);
}

/// Splits a style list at commas outside parentheses, so `rgb(1, 2, 3)` stays whole.
fn style_items(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                out.push(s.get(start..i).unwrap_or(""));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s.get(start..).unwrap_or(""));
    out
}

/// A size in px: a plain decimal, optionally followed by `px`, within `lo..=hi`.
fn px(v: &str, lo: f64, hi: f64) -> Option<f64> {
    let v = v.trim();
    let n = v.strip_suffix("px").unwrap_or(v).trim_end();
    style::parse_number(n).filter(|x| *x >= lo && *x <= hi)
}

/// Point styles (`radius`, `color`, `stroke-color`, `stroke-width`); anything else is
/// dropped with `W010`.
fn point_style(idx: &LineIndex, text: &str, diags: &mut Diagnostics) -> PointStyle {
    let mut out = PointStyle::default();
    if text.trim().is_empty() {
        return out;
    }
    for item in style_items(text) {
        let span = span_of(idx, item.trim());
        let parsed = item.split_once(':').map(|(k, v)| (k.trim(), v.trim()));
        let ok = match parsed {
            Some(("radius", v)) => px(v, 0.0, 100.0).map(|r| out.radius = Some(r)),
            Some(("stroke-width", v)) => px(v, 0.0, 20.0).map(|w| out.stroke_width = Some(w)),
            Some((key @ ("color" | "stroke-color"), v)) => colour(v).map(|c| {
                if key == "color" {
                    out.color = Some(c);
                } else {
                    out.stroke_color = Some(c);
                }
                fixed_colour(diags, span);
            }),
            _ => None,
        };
        if ok.is_none() {
            diags.emit(
                Severity::Warning,
                "W010",
                span,
                format!(
                    "point style `{}` is outside the accepted set (radius, color, stroke-color, stroke-width); ignored",
                    excerpt(item.trim())
                ),
            );
        }
    }
    out
}

/// A point colour: the accepted colour grammar, or hex digits without the `#`, which
/// mermaid accepts too.
fn colour(v: &str) -> Option<Color> {
    super::config::quadrant_colour(v)
}

/// Whether `v` is written as mermaid's lexer reads a coordinate: `1`, `0` or `0.` and
/// digits.
fn mermaid_form(v: &str) -> bool {
    v == "1"
        || v == "0"
        || v.strip_prefix("0.")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// The form mermaid's lexer reads for a coordinate `value` in 0–1 written as `text`.
fn canonical(text: &str, value: f64) -> String {
    if value == 1.0 {
        return String::from("1");
    }
    let body = text.trim_start_matches('+');
    let frac = body.split_once('.').map_or("", |(_, f)| f);
    if value == 0.0 || frac.is_empty() {
        String::from("0")
    } else {
        format!("0.{frac}")
    }
}

/// One coordinate, with `E024` outside 0–1 and `R021` for a form mermaid rejects.
fn coordinate(
    idx: &LineIndex,
    raw: &str,
    axis: &str,
    diags: &mut Diagnostics,
) -> Result<f64, Stop> {
    let text = raw.trim();
    let span = span_of(idx, text);
    let Some(value) = style::parse_number(text) else {
        return Err(fail(
            diags,
            span,
            format!(
                "point {axis} `{}` is not a number from 0 to 1",
                excerpt(text)
            ),
        ));
    };
    if !(0.0..=1.0).contains(&value) {
        diags.emit(
            Severity::Error,
            "E024",
            span,
            format!(
                "point {axis} `{}` is outside 0–1; quadrant chart coordinates run from 0 to 1",
                excerpt(text)
            ),
        );
        return Err(Stop::Failed);
    }
    if !mermaid_form(text) {
        let replacement = canonical(text, value);
        diags.push(Diagnostic {
            severity: Severity::Repair,
            code: "R021",
            span,
            message: format!(
                "point {axis} `{}` is written as `{replacement}`, the form mermaid reads",
                excerpt(text)
            ),
            fix: Some(Fix { span, replacement }),
        });
    }
    // `-0` reads as 0, so the model never holds a negative zero.
    Ok(value + 0.0)
}

/// `Name[:::class]: [x, y] [styles]`, or `None` when the statement is not a point.
fn point(
    idx: &LineIndex,
    stmt: &str,
    span: Span,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Option<Point>, Stop> {
    let (name, rest) = if let Some(quoted) = stmt.strip_prefix('"') {
        match quoted.find('"') {
            Some(end) => (
                stmt.get(..end + 2).unwrap_or(""),
                quoted.get(end + 1..).unwrap_or(""),
            ),
            None => return Err(fail(diags, span, "unterminated `\"` string")),
        }
    } else {
        match stmt.find(':') {
            Some(at) => (stmt.get(..at).unwrap_or(""), stmt.get(at..).unwrap_or("")),
            None => return Ok(None),
        }
    };
    let mut rest = rest.trim_start();
    let mut class = None;
    if let Some(after) = rest.strip_prefix(":::") {
        let end = after
            .find(|c: char| c == ':' || c.is_whitespace())
            .unwrap_or(after.len());
        let name_text = after.get(..end).unwrap_or("");
        if crate::svg::is_valid_class_name(name_text) {
            class = Some(String::from(name_text));
        } else {
            diags.emit(
                Severity::Warning,
                "W011",
                span_of(idx, name_text),
                format!(
                    "class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; dropped",
                    excerpt(name_text)
                ),
            );
        }
        rest = after.get(end..).unwrap_or("").trim_start();
    }
    let Some(open) = rest
        .strip_prefix(':')
        .map(str::trim_start)
        .and_then(|r| r.strip_prefix('['))
    else {
        return Ok(None);
    };
    if name.trim().is_empty() {
        return Err(fail(diags, span, "a point needs a name before `:`"));
    }
    let Some(close) = open.find(']') else {
        return Err(fail(diags, span, "a point needs `[x, y]` closed by `]`"));
    };
    let coords = open.get(..close).unwrap_or("");
    let Some((x, y)) = coords.split_once(',') else {
        return Err(fail(diags, span, "a point needs two coordinates `[x, y]`"));
    };
    let x = coordinate(idx, x, "x", diags)?;
    let y = coordinate(idx, y, "y", diags)?;
    let style = point_style(idx, open.get(close + 1..).unwrap_or(""), diags);
    Ok(Some(Point {
        label: text_value(name, opts, span, diags),
        x,
        y,
        class,
        style,
        span,
    }))
}

pub(crate) fn parse_quadrant(
    idx: &LineIndex,
    mut pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Quadrant, Stop> {
    let mut q = Quadrant {
        meta,
        ..Quadrant::default()
    };
    if let Some(span) = q
        .meta
        .quadrant
        .theme
        .span
        .filter(|_| q.meta.quadrant.theme.any())
    {
        diags.emit_once(
            Severity::Info,
            "I030",
            span,
            "the source sets a colour: a quadrant chart `themeVariables` colour stays fixed in every theme",
        );
    }
    q.meta.quadrant.theme.pending_ignored = None;
    let src = idx.src();
    let mut description: Option<(usize, String)> = None;
    while pos < src.len() {
        let next = idx.next_line_start(pos);
        let line = src
            .get(pos..next)
            .unwrap_or("")
            .trim_end_matches(['\n', '\r']);
        let line_pos = pos;
        pos = next;
        if let Some((start, body)) = &mut description {
            if let Some((before, after)) = line.split_once('}') {
                body.push_str(before.trim());
                let span = idx.span(*start, line_pos + line.len());
                if !after.trim().is_empty() {
                    return Err(fail(diags, span, "unexpected text after accDescr block"));
                }
                q.meta.acc_descr = Some(bounded(body, opts, span, diags));
                description = None;
            } else {
                if body.len().saturating_add(line.len()) > opts.limits.label_bytes {
                    return Err(Stop::TooLarge("accessibility description"));
                }
                body.push_str(line.trim());
                body.push('\n');
            }
            continue;
        }
        if line.trim_start().starts_with("%%") {
            continue;
        }
        if super::repair::is_fence_line(line.trim()) {
            super::strip_fence_line(idx, line_pos, diags);
            continue;
        }
        for raw in statements(line) {
            let stmt = raw.trim();
            if stmt.is_empty() {
                continue;
            }
            let span = span_of(idx, stmt);
            if let Some(rest) = keyword(stmt, "title") {
                q.meta.title = Some(bounded(rest, opts, span, diags));
            } else if let Some(rest) = colon_keyword(stmt, "accTitle") {
                q.meta.acc_title = Some(bounded(rest, opts, span, diags));
            } else if let Some(rest) = colon_keyword(stmt, "accDescr") {
                q.meta.acc_descr = Some(bounded(rest, opts, span, diags));
            } else if let Some(body) = stmt
                .get(..8)
                .filter(|h| h.eq_ignore_ascii_case("accDescr"))
                .and_then(|_| stmt.get(8..))
                .map(str::trim_start)
                .and_then(|r| r.strip_prefix('{'))
            {
                if let Some((text, after)) = body.split_once('}') {
                    if !after.trim().is_empty() {
                        return Err(fail(diags, span, "unexpected text after accDescr block"));
                    }
                    q.meta.acc_descr = Some(bounded(text, opts, span, diags));
                } else {
                    let mut text = String::from(body.trim());
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    description = Some((offset(idx, stmt), text));
                }
            } else if let Some(rest) = keyword(stmt, "x-axis") {
                let (low, high) = axis(rest, "x-axis", span, opts, diags)?;
                q.x_axis.low = low;
                q.x_axis.high = high;
            } else if let Some(rest) = keyword(stmt, "y-axis") {
                let (low, high) = axis(rest, "y-axis", span, opts, diags)?;
                q.y_axis.low = low;
                q.y_axis.high = high;
            } else if let Some((n, rest)) = (1..=4)
                .find_map(|n: usize| keyword(stmt, &format!("quadrant-{n}")).map(|rest| (n, rest)))
            {
                if rest.trim().is_empty() {
                    return Err(fail(diags, span, format!("`quadrant-{n}` needs a label")));
                }
                q.quadrants[n - 1] = Some(text_value(rest, opts, span, diags));
            } else if let Some(rest) = keyword(stmt, "classDef") {
                let rest = rest.trim_start();
                let (name, styles) = rest
                    .split_once(char::is_whitespace)
                    .map_or((rest, ""), |(a, b)| (a, b));
                if name.is_empty() {
                    return Err(fail(diags, span, "`classDef` needs a class name"));
                }
                if !crate::svg::is_valid_class_name(name) {
                    diags.emit(
                        Severity::Warning,
                        "W011",
                        span_of(idx, name),
                        format!(
                            "class name `{}` is outside `[A-Za-z_][A-Za-z0-9_-]{{0,63}}`; dropped",
                            excerpt(name)
                        ),
                    );
                    continue;
                }
                let style = point_style(idx, styles, diags);
                match q.class_defs.iter_mut().find(|c| c.name == name) {
                    Some(c) => c.style = style,
                    None => {
                        if q.class_defs.len() >= opts.limits.nodes {
                            return Err(Stop::TooLarge("quadrant classes"));
                        }
                        q.class_defs.push(PointClass {
                            name: String::from(name),
                            style,
                        });
                    }
                }
            } else if let Some(p) = point(idx, stmt, span, opts, diags)? {
                if q.points.len() >= opts.limits.nodes {
                    return Err(Stop::TooLarge("quadrant points"));
                }
                q.points.push(p);
            } else {
                return Err(fail(
                    diags,
                    span,
                    format!("unexpected `{}`: {EXPECTED}", excerpt(stmt)),
                ));
            }
        }
    }
    if let Some((start, _)) = description {
        return Err(fail(
            diags,
            idx.span(start, src.len()),
            "unterminated accDescr block",
        ));
    }
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates_in_mermaid_form() {
        for ok in ["0", "1", "0.5", "0.000", "0.75"] {
            assert!(mermaid_form(ok), "{ok}");
        }
        for bad in ["1.0", ".5", "+0.5", "00.5", "0.", "1.00"] {
            assert!(!mermaid_form(bad), "{bad}");
        }
        assert_eq!(canonical("1.0", 1.0), "1");
        assert_eq!(canonical(".5", 0.5), "0.5");
        assert_eq!(canonical("+0.25", 0.25), "0.25");
        assert_eq!(canonical("00.5", 0.5), "0.5");
        assert_eq!(canonical("0.", 0.0), "0");
    }

    #[test]
    fn statements_split_outside_quotes_and_titles_keep_semicolons() {
        let s: Vec<&str> = statements("a: [0.1, 0.2]; \"b;c\": [0, 1] %% note")
            .into_iter()
            .map(str::trim)
            .collect();
        assert_eq!(s, ["a: [0.1, 0.2]", "\"b;c\": [0, 1]"]);
        assert_eq!(statements("title A; B"), ["title A; B"]);
    }

    #[test]
    fn axis_delimiters_need_a_closing_angle_and_skip_strings() {
        assert_eq!(axis_delimiter("Low --> High"), Some((4, 3)));
        assert_eq!(axis_delimiter("Low ----> High"), Some((4, 5)));
        assert_eq!(axis_delimiter("\"a --> b\" --> c"), Some((10, 3)));
        assert_eq!(axis_delimiter("x-axis-2"), None);
        assert_eq!(axis_delimiter("a -- b"), None);
    }
}
