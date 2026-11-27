//! Chart SVG (specs/chart.md#svg). Marks paint with `currentColor`; a class on each
//! mark sets `color` from `--merlion-series-{k}` or a neutral role, so light, dark and
//! custom themes switch by CSS alone, and every element also carries the literal colour
//! as a presentation attribute for renderers without CSS.

use super::escape::push_escaped;
use super::outline::plain_label;
use super::roles::BUILT_IN;
use super::style::{self, SourceRule};
use super::theme::{Role, Table};
use super::{attr, attr_num, label, safe_root_id, theme, DrawOutput, Layer};
use crate::chart::layout::{ChartGeometry, End, ItemGeom, Shape};
use crate::chart::resolve::Resolved;
use crate::chart::spec::{MarkType, Spec, Tone};
use crate::diag::Diagnostics;
use crate::geometry::gantt::{Rect, TextGeom};
use crate::math::min;
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// The plain-text alternative: a summary line, then one line per data row with the
/// fields the marks read, as written in the spec.
pub fn outline_chart(spec: &Spec, r: &Resolved) -> String {
    let mut out = alloc::format!("Chart. {}.", plain_label(&spec.title));
    let _ = write!(out, " {} rows.", spec.data.len());
    if !r.series.is_empty() {
        out.push_str(" Series: ");
        for (i, s) in r.series.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&plain_label(s));
        }
        out.push('.');
    }
    if !r.fields.is_empty() {
        out.push('\n');
        out.push_str(&r.fields.join(" | "));
        for row in &spec.data {
            out.push('\n');
            for (i, f) in r.fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                match row.get(f).map(|c| c.0) {
                    Some(crate::chart::spec::Cell::Str(s)) => out.push_str(&plain_label(s)),
                    Some(crate::chart::spec::Cell::Num(n)) => out.push_str(n),
                    _ => out.push('-'),
                }
            }
        }
    }
    out
}

/// A one-sentence description when the spec has none.
fn summary(spec: &Spec, r: &Resolved) -> String {
    let mut kinds: Vec<&str> = Vec::new();
    for m in &spec.marks {
        let k = m.kind.as_str();
        if !matches!(m.kind, MarkType::Rule | MarkType::Span) && !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    let axis = |a: &crate::chart::spec::Axis, name: &str| -> String {
        match &a.label {
            Some(l) => alloc::format!("{} axis {} ({})", name, plain_label(l), a.scale.as_str()),
            None => alloc::format!("{} axis {}", name, a.scale.as_str()),
        }
    };
    let mut s = alloc::format!(
        "{} chart, {}, {}, {} rows",
        if kinds.is_empty() {
            String::from("Reference")
        } else {
            kinds.join(" and ")
        },
        axis(&spec.x, "x"),
        axis(&spec.y, "y"),
        spec.data.len()
    );
    if !r.series.is_empty() {
        let _ = write!(s, ", {} series", r.series.len());
    }
    s.push('.');
    s
}

fn series_class(k: usize) -> String {
    alloc::format!("merlion-chart-s{}", k % crate::chart::MAX_SERIES + 1)
}

/// The colour class and literal of an item of a mark with `tone`.
fn paint(table: &Table, tone: Tone, kind: MarkType, series: Option<usize>) -> (String, String) {
    match (tone, series) {
        (Tone::Neutral, _) => (String::from("merlion-chart-neutral"), table.lit(Role::Line)),
        (Tone::Accent, _) => (
            String::from("merlion-chart-accent"),
            table.lit(Role::Accent),
        ),
        (Tone::Series, Some(k)) => (
            series_class(k),
            table.series_lit((k % crate::chart::MAX_SERIES + 1) as u8),
        ),
        (Tone::Series, None) if matches!(kind, MarkType::Rule) => (
            String::from("merlion-chart-rule-tone"),
            table.lit(Role::Muted),
        ),
        (Tone::Series, None) => (series_class(0), table.series_lit(1)),
    }
}

fn rules(t: &Table, small: f64) -> Vec<SourceRule> {
    let mut px = String::new();
    push_num(&mut px, small);
    let rule = |selector: &str, body: String| SourceRule {
        selector: String::from(selector),
        body,
    };
    let mut out = alloc::vec![
        rule(
            ".merlion-chart-small",
            alloc::format!("font-size:{}px;", px)
        ),
        rule(
            ".merlion-chart-grid",
            alloc::format!(
                "stroke:{};stroke-width:1;opacity:0.35;fill:none;",
                t.var(Role::Line, false)
            ),
        ),
        rule(
            ".merlion-chart-zero",
            alloc::format!(
                "stroke:{};stroke-width:1;fill:none;",
                t.var(Role::Muted, false)
            ),
        ),
        rule(
            ".merlion-chart-tick",
            alloc::format!("fill:{};", t.var(Role::Muted, false))
        ),
        rule(
            ".merlion-chart-note",
            alloc::format!("fill:{};", t.var(Role::Muted, false))
        ),
        rule(
            ".merlion-chart-title",
            alloc::format!("fill:{};", t.var(Role::Muted, false))
        ),
        rule(
            ".merlion-chart-label",
            alloc::format!("fill:{};", t.var(Role::Fg, false))
        ),
        rule(
            ".merlion-chart-key-text",
            alloc::format!("fill:{};", t.var(Role::Fg, false))
        ),
        rule(
            ".merlion-chart-shade",
            alloc::format!(
                "fill:{};opacity:0.12;stroke:none;",
                t.var(Role::Muted, false)
            ),
        ),
        rule(
            ".merlion-chart-neutral",
            alloc::format!("color:{};", t.var(Role::Line, false))
        ),
        rule(
            ".merlion-chart-accent",
            alloc::format!("color:{};", t.var(Role::Accent, false))
        ),
        rule(
            ".merlion-chart-rule-tone",
            alloc::format!("color:{};", t.var(Role::Muted, false))
        ),
        rule(
            ".merlion-chart-fill",
            String::from("fill:currentColor;stroke:none;")
        ),
        rule(
            ".merlion-chart-stroke",
            String::from("stroke:currentColor;fill:none;")
        ),
        rule(
            ".merlion-chart-area",
            String::from("fill:currentColor;fill-opacity:0.18;stroke:none;")
        ),
        rule(
            ".merlion-chart-dot",
            alloc::format!(
                "fill:currentColor;stroke:{};stroke-width:1.5;",
                t.var(Role::Bg, false)
            ),
        ),
    ];
    for k in 1..=crate::chart::MAX_SERIES as u8 {
        out.push(rule(
            &alloc::format!(".merlion-chart-s{}", k),
            alloc::format!("color:{};", t.series_var(k)),
        ));
    }
    out
}

fn text(out: &mut String, g: &TextGeom, class: &str, fill: &str) {
    label::push_label(out, &g.label, g.x, g.y, class, fill, fill);
}

fn line(out: &mut String, class: &str, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str) {
    out.push_str("<line");
    attr(out, "class", class);
    attr_num(out, "x1", x1);
    attr_num(out, "y1", y1);
    attr_num(out, "x2", x2);
    attr_num(out, "y2", y2);
    attr(out, "stroke", stroke);
}

fn pt(d: &mut String, cmd: char, x: f64, y: f64) {
    d.push(cmd);
    push_num(d, x);
    d.push(' ');
    push_num(d, y);
}

/// A bar path with its data end rounded and its baseline end square.
fn bar_path(r: Rect, end: End) -> String {
    let rad = min(3.0, min(r.w, r.h) / 2.0).max(0.0);
    let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let mut d = String::new();
    match end {
        End::Top => {
            pt(&mut d, 'M', x0, y1);
            pt(&mut d, 'L', x0, y0 + rad);
            pt(&mut d, 'Q', x0, y0);
            pt(&mut d, ' ', x0 + rad, y0);
            pt(&mut d, 'L', x1 - rad, y0);
            pt(&mut d, 'Q', x1, y0);
            pt(&mut d, ' ', x1, y0 + rad);
            pt(&mut d, 'L', x1, y1);
        }
        End::Bottom => {
            pt(&mut d, 'M', x0, y0);
            pt(&mut d, 'L', x1, y0);
            pt(&mut d, 'L', x1, y1 - rad);
            pt(&mut d, 'Q', x1, y1);
            pt(&mut d, ' ', x1 - rad, y1);
            pt(&mut d, 'L', x0 + rad, y1);
            pt(&mut d, 'Q', x0, y1);
            pt(&mut d, ' ', x0, y1 - rad);
        }
        End::Right => {
            pt(&mut d, 'M', x0, y0);
            pt(&mut d, 'L', x1 - rad, y0);
            pt(&mut d, 'Q', x1, y0);
            pt(&mut d, ' ', x1, y0 + rad);
            pt(&mut d, 'L', x1, y1 - rad);
            pt(&mut d, 'Q', x1, y1);
            pt(&mut d, ' ', x1 - rad, y1);
            pt(&mut d, 'L', x0, y1);
        }
        End::Left => {
            pt(&mut d, 'M', x1, y0);
            pt(&mut d, 'L', x1, y1);
            pt(&mut d, 'L', x0 + rad, y1);
            pt(&mut d, 'Q', x0, y1);
            pt(&mut d, ' ', x0, y1 - rad);
            pt(&mut d, 'L', x0, y0 + rad);
            pt(&mut d, 'Q', x0, y0);
            pt(&mut d, ' ', x0 + rad, y0);
        }
    }
    d.push('Z');
    d
}

fn item(
    out: &mut String,
    it: &ItemGeom,
    kind: MarkType,
    class: &str,
    colour: &str,
    dash: bool,
    bg: &str,
) {
    out.push_str("<g");
    attr(
        out,
        "class",
        &alloc::format!("merlion-chart-item {}", class),
    );
    if let Some(row) = it.row {
        let _ = write!(out, " data-merlion-row=\"{}\"", row);
    }
    for (k, v) in &it.data {
        attr(out, k, v);
    }
    attr(out, "color", colour);
    out.push_str("><title>");
    for part in &it.tip {
        push_escaped(out, part);
    }
    out.push_str("</title>");
    match &it.shape {
        Shape::Bar { rect, end } => {
            out.push_str("<path class=\"merlion-chart-fill\"");
            attr(out, "d", &bar_path(*rect, *end));
            attr(out, "fill", colour);
            out.push_str("/>");
        }
        Shape::Path { d, fill, points } => {
            out.push_str("<path");
            if *fill {
                attr(out, "class", "merlion-chart-area");
                attr(out, "d", d);
                attr(out, "fill", colour);
                out.push_str(" fill-opacity=\"0.18\"");
            } else {
                attr(out, "class", "merlion-chart-stroke");
                attr(out, "d", d);
                out.push_str(" fill=\"none\"");
                attr(out, "stroke", colour);
                out.push_str(
                    " stroke-width=\"2\" stroke-linejoin=\"round\" stroke-linecap=\"round\"",
                );
                if dash {
                    out.push_str(" stroke-dasharray=\"5 3\"");
                }
            }
            out.push_str("/>");
            for point in points {
                out.push_str("<g class=\"merlion-chart-point\"");
                if let Some(row) = point.row {
                    let _ = write!(out, " data-merlion-row=\"{}\"", row);
                }
                for (name, value) in &point.data {
                    attr(out, name, value);
                }
                out.push_str("><title>");
                for part in &point.tip {
                    push_escaped(out, part);
                }
                out.push_str("</title><circle class=\"merlion-chart-point-hit\"");
                attr_num(out, "cx", point.cx);
                attr_num(out, "cy", point.cy);
                out.push_str(" r=\"6\" fill=\"transparent\" pointer-events=\"all\"/></g>");
            }
        }
        Shape::Dot { cx, cy, r } => {
            out.push_str("<circle class=\"merlion-chart-dot\"");
            attr_num(out, "cx", *cx);
            attr_num(out, "cy", *cy);
            attr_num(out, "r", *r);
            attr(out, "fill", colour);
            attr(out, "stroke", bg);
            out.push_str(" stroke-width=\"1.5\"/>");
        }
        Shape::Segment {
            x1,
            y1,
            x2,
            y2,
            width,
        } => {
            line(out, "merlion-chart-stroke", *x1, *y1, *x2, *y2, colour);
            attr_num(out, "stroke-width", *width);
            if kind == MarkType::Interval {
                out.push_str(" stroke-linecap=\"round\"");
            }
            if dash {
                out.push_str(" stroke-dasharray=\"4 3\"");
            }
            out.push_str("/>");
        }
        Shape::Shade { rect } => {
            out.push_str("<rect class=\"merlion-chart-shade\"");
            attr_num(out, "x", rect.x);
            attr_num(out, "y", rect.y);
            attr_num(out, "width", rect.w);
            attr_num(out, "height", rect.h);
            attr(out, "fill", colour);
            out.push_str(" opacity=\"0.12\"/>");
        }
    }
    out.push_str("</g>");
}

pub fn draw_chart(
    spec: &Spec,
    r: &Resolved,
    geom: &ChartGeometry,
    opts: &RenderOptions,
    id: &str,
    _diags: &mut Diagnostics,
) -> DrawOutput {
    let id = safe_root_id(id);
    let outline = outline_chart(spec, r);
    let builtins: Vec<_> = BUILT_IN.iter().collect();
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|p| Layer::new(Some(p), builtins));
    let light_rules = rules(&light.table, geom.small);
    let dark_rules = dark.as_ref().map(|d| rules(&d.table, geom.small));
    let t = &light.table;

    let mut out = String::new();
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    attr(&mut out, "id", &id);
    out.push_str(" viewBox=\"0 0 ");
    push_num(&mut out, geom.width);
    out.push(' ');
    push_num(&mut out, geom.height);
    out.push_str("\" width=\"100%\" style=\"max-width:");
    push_num(&mut out, geom.width);
    out.push_str("px\" role=\"img\"");
    attr(
        &mut out,
        "aria-labelledby",
        &alloc::format!("{id}-title {id}-desc"),
    );
    out.push_str(" class=\"merlion merlion-chart\" data-merlion-chart=\"1\"");
    attr(&mut out, "data-merlion-version", crate::VERSION);
    out.push_str(">\n");
    let _ = write!(out, "<title id=\"{id}-title\">");
    push_escaped(&mut out, &spec.title);
    out.push_str("</title>\n");
    let _ = write!(out, "<desc id=\"{id}-desc\">");
    match &spec.description {
        Some(d) => push_escaped(&mut out, d),
        None => push_escaped(&mut out, &summary(spec, r)),
    }
    out.push_str("</desc>\n");
    let font_css = if opts.font == FontMode::Embed {
        out.push_str(&crate::text::ofl_xml_comment());
        Some(crate::text::embedded_font_css(theme::EMBED_FONT_FAMILY))
    } else {
        None
    };
    let light_style = style::Layer {
        table: &light.table,
        roles: &[],
        source: &light_rules,
        sequence: false,
    };
    let dark_style = match (&dark, &dark_rules) {
        (Some(d), Some(source)) => Some(style::Layer {
            table: &d.table,
            roles: &[],
            source,
            sequence: false,
        }),
        _ => None,
    };
    out.push_str("<style>");
    out.push_str(&style::build(
        &id,
        opts.font,
        geom.font_size,
        None,
        font_css.as_deref(),
        &light_style,
        dark_style.as_ref(),
    ));
    out.push_str("</style>\n");
    if opts.background {
        out.push_str("<rect class=\"merlion-bg\" x=\"0\" y=\"0\"");
        attr_num(&mut out, "width", geom.width);
        attr_num(&mut out, "height", geom.height);
        attr(&mut out, "fill", &light.lit(Role::Bg));
        out.push_str("/>");
    }
    out.push_str("<g class=\"merlion-diagram\"");
    attr(&mut out, "font-family", theme::font_stack(opts.font));
    attr_num(&mut out, "font-size", geom.font_size);
    out.push_str(">\n");

    let muted = t.lit(Role::Muted);
    let bg = t.lit(Role::Bg);
    let fg = t.lit(Role::Fg);
    let small = |c: &str| alloc::format!("{} merlion-chart-small", c);

    if !geom.legend.is_empty() {
        out.push_str("<g class=\"merlion-chart-legend\">");
        for k in &geom.legend {
            let name = r.series.get(k.series).map_or("", AsRef::as_ref);
            let (class, colour) = paint(t, Tone::Series, MarkType::Bar, Some(k.series));
            out.push_str("<g");
            attr(
                &mut out,
                "class",
                &alloc::format!("merlion-chart-key {}", class),
            );
            attr(&mut out, "data-merlion-series", name);
            attr(&mut out, "color", &colour);
            out.push_str("><rect class=\"merlion-chart-fill\"");
            attr_num(&mut out, "x", k.swatch.x);
            attr_num(&mut out, "y", k.swatch.y);
            attr_num(&mut out, "width", k.swatch.w);
            attr_num(&mut out, "height", k.swatch.h);
            out.push_str(" rx=\"2\"");
            attr(&mut out, "fill", &colour);
            out.push_str("/>");
            text(&mut out, &k.text, &small("merlion-chart-key-text"), &fg);
            out.push_str("</g>");
        }
        out.push_str("</g>\n");
    }
    if let Some(g) = &geom.y_title {
        text(
            &mut out,
            g,
            &small("merlion-chart-title merlion-chart-y-title"),
            &muted,
        );
    }

    let mark_open = |out: &mut String, m: &crate::chart::layout::MarkGeom| {
        out.push_str("<g");
        attr(
            out,
            "class",
            &alloc::format!("merlion-chart-mark merlion-chart-{}", m.kind.as_str()),
        );
        let _ = write!(out, " data-merlion-mark=\"{}\">", m.index);
    };

    for m in geom.marks.iter().filter(|m| m.kind == MarkType::Span) {
        mark_open(&mut out, m);
        for it in &m.items {
            item(
                &mut out,
                it,
                m.kind,
                "merlion-chart-span-item",
                &muted,
                false,
                &bg,
            );
        }
        out.push_str("</g>\n");
    }

    out.push_str("<g class=\"merlion-chart-axes\">");
    let plot = geom.plot;
    let line_lit = t.lit(Role::Line);
    if geom.y_grid {
        for tk in &geom.y_ticks {
            line(
                &mut out,
                "merlion-chart-grid",
                plot.x,
                tk.pos,
                plot.x + plot.w,
                tk.pos,
                &line_lit,
            );
            out.push_str(" stroke-width=\"1\" opacity=\"0.35\"");
            attr(&mut out, "data-merlion-value", &tk.value);
            out.push_str("/>");
        }
    }
    if geom.x_grid {
        for tk in &geom.x_ticks {
            line(
                &mut out,
                "merlion-chart-grid",
                tk.pos,
                plot.y,
                tk.pos,
                plot.y + plot.h,
                &line_lit,
            );
            out.push_str(" stroke-width=\"1\" opacity=\"0.35\"");
            attr(&mut out, "data-merlion-value", &tk.value);
            out.push_str("/>");
        }
    }
    if let Some((x1, y1, x2, y2)) = geom.zero {
        line(&mut out, "merlion-chart-zero", x1, y1, x2, y2, &muted);
        out.push_str(" stroke-width=\"1\" data-merlion-value=\"0\"/>");
    }
    for tk in geom.x_ticks.iter().chain(geom.y_ticks.iter()) {
        text(&mut out, &tk.text, &small("merlion-chart-tick"), &muted);
    }
    out.push_str("</g>\n");

    for m in geom.marks.iter().filter(|m| m.kind != MarkType::Span) {
        mark_open(&mut out, m);
        for it in &m.items {
            let (class, colour) = paint(t, m.tone, m.kind, it.series);
            item(&mut out, it, m.kind, &class, &colour, m.dash, &bg);
        }
        out.push_str("</g>\n");
    }

    out.push_str("<g class=\"merlion-chart-labels\">");
    for m in &geom.marks {
        for n in &m.notes {
            text(&mut out, n, &small("merlion-chart-note"), &muted);
        }
        for it in &m.items {
            if let Some(l) = &it.label {
                text(&mut out, l, &small("merlion-chart-label"), &fg);
            }
        }
    }
    out.push_str("</g>\n");
    if let Some(g) = &geom.x_title {
        text(
            &mut out,
            g,
            &small("merlion-chart-title merlion-chart-x-title"),
            &muted,
        );
    }
    out.push_str("</g>\n</svg>\n");
    DrawOutput { svg: out, outline }
}
