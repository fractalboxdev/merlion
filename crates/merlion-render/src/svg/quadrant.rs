//! Quadrant chart SVG (specs/quadrant.md#svg). Quadrant fills are the accent mixed into
//! the background at four strengths, points take the accent, and every colour reads its
//! token with the literal as fallback, so a theme restyles the chart through CSS alone.
//! `themeVariables` quadrant colours, `classDef` and point styles are fixed colours on
//! top, in mermaid's precedence: point style, then class, then theme.

use super::escape::push_escaped;
use super::label;
use super::outline::plain_label;
use super::roles::{self, BuiltIn, Kind, Tone, BUILT_IN};
use super::style::{self, RoleRule, SourceRule};
use super::theme::Role;
use super::{attr, attr_num, color, safe_root_id, theme, DrawOutput, Layer};
use crate::diag::{Diagnostics, Severity, Span};
use crate::geometry::quadrant::{QuadrantGeometry, Rect, TextGeom};
use crate::ids;
use crate::model::quadrant::{Point, PointStyle, Quadrant};
use crate::model::{Color, Style};
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};
use crate::stylesheet::ClassProp;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// Strength of the accent in each quadrant's fill, quadrant 1 first. mermaid lightens
/// its quadrants in the same order.
pub const QUADRANT_MIX: [u8; 4] = [12, 9, 6, 3];

/// Names of the quadrants in the outline.
const PLACES: [&str; 4] = ["top right", "top left", "bottom left", "bottom right"];

/// `v1;QUADRANT;0:` and the point labels, encoded (specs/svg-output.md#layout-hint).
pub fn layout_hint(q: &Quadrant) -> String {
    let mut out = String::from("v1;QUADRANT;0:");
    for (i, p) in q.points.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&ids::encode_id(&p.label));
    }
    out
}

/// A coordinate as written in the model: the shortest decimal that reads back exactly.
fn value(v: f64) -> String {
    alloc::format!("{}", v + 0.0)
}

/// The quadrant a point lies in, 0-based; `None` on a dividing line.
fn quadrant_of(p: &Point) -> Option<usize> {
    if p.x == 0.5 || p.y == 0.5 {
        return None;
    }
    Some(match (p.x > 0.5, p.y > 0.5) {
        (true, true) => 0,
        (false, true) => 1,
        (false, false) => 2,
        (true, false) => 3,
    })
}

/// The plain-text outline (specs/quadrant.md#text-alternative).
pub fn outline_quadrant(q: &Quadrant) -> String {
    let mut out = alloc::format!("Quadrant chart. {} points.", q.points.len());
    if let Some(title) = q.meta.title.as_deref().filter(|t| !t.trim().is_empty()) {
        let _ = write!(out, " {}", plain_label(title));
    }
    for (name, axis) in [("x-axis", &q.x_axis), ("y-axis", &q.y_axis)] {
        match (&axis.low, &axis.high) {
            (Some(l), Some(h)) => {
                let _ = write!(out, "\n{name}: {} → {}", plain_label(l), plain_label(h));
            }
            (Some(l), None) => {
                let _ = write!(out, "\n{name}: {}", plain_label(l));
            }
            _ => {}
        }
    }
    for (i, label) in q.quadrants.iter().enumerate() {
        if let Some(l) = label.as_deref().filter(|t| !t.trim().is_empty()) {
            let _ = write!(
                out,
                "\nQuadrant {}, {}: {}",
                i + 1,
                PLACES[i],
                plain_label(l)
            );
        }
    }
    for p in &q.points {
        let _ = write!(
            out,
            "\n{}: x {}, y {}",
            plain_label(&p.label),
            value(p.x),
            value(p.y)
        );
        if let Some(i) = quadrant_of(p) {
            let _ = write!(out, ", quadrant {}", i + 1);
            if let Some(l) = q.quadrants[i].as_deref().filter(|t| !t.trim().is_empty()) {
                let _ = write!(out, " ({})", plain_label(l));
            }
        }
    }
    out
}

/// Draw-time class lookup comparisons, applied classes and palette tones, charged
/// before layout and drawing (specs/quadrant.md#layout).
pub fn role_units(q: &Quadrant, opts: &RenderOptions) -> u64 {
    let classes = q.points.iter().filter(|p| p.class.is_some()).count() as u64;
    let tones = opts.palette.as_ref().map_or(0, |p| {
        p.light.tones.len() + p.dark.as_ref().map_or(0, |d| d.tones.len())
    }) as u64;
    // Role selection and literal painting each look up the point's class. Built-in
    // role selection checks every built-in name against the class declarations.
    classes
        .saturating_add(tones)
        .saturating_add(q.class_lookup_units().saturating_mul(2))
        .saturating_add((BUILT_IN.len() as u64).saturating_mul(q.class_defs.len() as u64))
}

/// The built-in roles in effect: a `classDef` of the same name replaces one.
fn builtins(q: &Quadrant) -> Vec<&'static BuiltIn> {
    BUILT_IN
        .iter()
        .filter(|b| !q.class_defs.iter().any(|c| c.name == b.name))
        .collect()
}

/// The point classes that name no `classDef`: roles, in first-use order.
fn point_roles(q: &Quadrant) -> Vec<&str> {
    let mut seen = BTreeSet::new();
    q.points
        .iter()
        .filter_map(|p| p.class.as_deref())
        .filter(|c| q.class_def(c).is_none() && seen.insert(*c))
        .collect()
}

fn css(c: &Option<Color>) -> Option<String> {
    c.as_ref().and_then(color::color_css)
}

/// The colours a point's style sets, as a source style for its dot.
fn dot_style(s: &PointStyle) -> Style {
    Style {
        fill: s.color,
        stroke: s.stroke_color,
        stroke_width: s.stroke_width,
        ..Style::default()
    }
}

fn px(v: f64) -> String {
    let mut s = String::new();
    push_num(&mut s, v);
    s.push_str("px");
    s
}

/// Rules that read the theme: quadrant fills, borders, text and the default point.
/// A `themeVariables` colour replaces the token with its literal.
fn base_rules(
    q: &Quadrant,
    layer: &Layer,
    geom: &QuadrantGeometry,
) -> (Vec<RoleRule>, Vec<SourceRule>) {
    let t = &layer.table;
    let theme = &q.meta.quadrant.theme;
    let mut mixed = Vec::new();
    for (i, pct) in QUADRANT_MIX.iter().enumerate() {
        let selector = alloc::format!(".merlion-quadrant-{}>.merlion-quadrant-box", i + 1);
        match css(&theme.fill[i]) {
            Some(lit) => mixed.push(RoleRule {
                selector,
                plain: alloc::format!("fill:{lit};"),
                mixed: String::new(),
            }),
            None => mixed.push(RoleRule {
                selector,
                plain: alloc::format!(
                    "fill:{};",
                    roles::mix_lit(&t.lit(Role::Accent), &t.lit(Role::Bg), *pct)
                ),
                mixed: alloc::format!(
                    "fill:color-mix(in oklab, {} {}%, {});",
                    t.var(Role::Accent, true),
                    pct,
                    t.var(Role::Bg, true)
                ),
            }),
        }
    }
    let or = |c: &Option<Color>, r: Role| css(c).unwrap_or_else(|| t.var(r, false));
    let rule = |selector: &str, body: String| SourceRule {
        selector: String::from(selector),
        body,
    };
    let mut src = alloc::vec![
        rule(
            ".merlion-quadrant-text",
            alloc::format!("fill:{};", t.var(Role::Fg, false))
        ),
        rule(
            ".merlion-quadrant-title",
            alloc::format!(
                "fill:{};font-size:{};font-weight:600;",
                or(&theme.title, Role::Fg),
                px(geom.sizes.title)
            )
        ),
        rule(
            ".merlion-quadrant-label",
            alloc::format!("font-size:{};", px(geom.sizes.quadrant))
        ),
        rule(
            ".merlion-quadrant-axis-x",
            alloc::format!(
                "fill:{};font-size:{};",
                or(&theme.x_axis_text, Role::Fg),
                px(geom.sizes.x_axis)
            )
        ),
        rule(
            ".merlion-quadrant-axis-y",
            alloc::format!(
                "fill:{};font-size:{};",
                or(&theme.y_axis_text, Role::Fg),
                px(geom.sizes.y_axis)
            )
        ),
        rule(
            ".merlion-quadrant-point-label",
            alloc::format!(
                "fill:{};font-size:{};",
                or(&theme.point_text, Role::Fg),
                px(geom.sizes.point)
            )
        ),
    ];
    // The border role is a mix of the foundations: inside `@supports` its chain ends in
    // `color-mix`, so a page that sets only `--merlion-fg` and `--merlion-bg` moves it.
    for (selector, colour) in [
        (".merlion-quadrant-border", &theme.external_border),
        (".merlion-quadrant-divider", &theme.internal_border),
    ] {
        mixed.push(match css(colour) {
            Some(lit) => RoleRule {
                selector: String::from(selector),
                plain: alloc::format!("stroke:{lit};fill:none;"),
                mixed: String::new(),
            },
            None => RoleRule {
                selector: String::from(selector),
                plain: alloc::format!("stroke:{};fill:none;", t.var(Role::Border, false)),
                mixed: alloc::format!("stroke:{};", t.var(Role::Border, true)),
            },
        });
    }
    let point = match css(&theme.point_fill) {
        Some(lit) => lit,
        None => t.var(Role::Accent, false),
    };
    src.push(rule(
        ".merlion-quadrant-dot",
        alloc::format!("fill:var(--merlion-tone, {point});stroke:var(--merlion-tone, {point});"),
    ));
    for (i, c) in theme.text.iter().enumerate() {
        if let Some(lit) = css(c) {
            src.push(rule(
                &alloc::format!(".merlion-quadrant-{}>.merlion-quadrant-label", i + 1),
                alloc::format!("fill:{lit};"),
            ));
        }
    }
    (mixed, src)
}

/// Role rules for the points: the built-in tone roles, then the palette's tones, which
/// colour a point's fill and stroke at full strength.
fn role_rules(
    layer: &Layer,
    used: &[&str],
    budget: &mut usize,
    prefix: usize,
    diags: Option<&mut Diagnostics>,
) -> Vec<RoleRule> {
    let mut out = Vec::new();
    let dot = |name: &str, tone: &Tone| RoleRule {
        selector: alloc::format!(".merlion-c-{name}>.merlion-quadrant-dot"),
        plain: alloc::format!("fill:{0};stroke:{0};", tone.plain),
        mixed: if tone.mixed == tone.plain {
            String::new()
        } else {
            alloc::format!("fill:{0};stroke:{0};", tone.mixed)
        },
    };
    for b in &layer.builtins {
        if b.kind == Kind::Node && used.contains(&b.name) {
            if let Some(tone) = b.tone {
                out.push(dot(b.name, &Tone::of(&layer.table, tone)));
            }
        }
    }
    let mut tones: Vec<(usize, &str, String)> = used
        .iter()
        .filter_map(|name| {
            let (i, t) = layer.tone(Kind::Node, name)?;
            Some((i, *name, t.tone?.to_hex()))
        })
        .collect();
    tones.sort_by_key(|x| x.0);
    let mut diags = diags;
    for (_, name, hex) in tones {
        let r = dot(name, &Tone::literal(hex));
        let bytes = prefix + r.selector.len() + r.plain.len() + 2;
        if *budget < bytes {
            if let Some(d) = diags.as_deref_mut() {
                d.emit(
                    Severity::Warning,
                    "W017",
                    Span::default(),
                    alloc::format!(
                        "role `{}` left out of the embedded style: the palette's rules exceed 16 KiB",
                        crate::diag::excerpt(name)
                    ),
                );
            }
            continue;
        }
        *budget -= bytes;
        out.push(r);
    }
    out
}

/// `classDef` and point style rules, after the role rules so they win.
fn source_rules(q: &Quadrant, layer: &Layer) -> Vec<SourceRule> {
    let mut out = Vec::new();
    for cd in &q.class_defs {
        let values = [ClassProp::Fill, ClassProp::Stroke, ClassProp::Color]
            .map(|p| layer.class_value(&cd.name, p));
        let token = color::ClassToken {
            name: &cd.name,
            fill: values[0].as_deref(),
            stroke: values[1].as_deref(),
            color: values[2].as_deref(),
        };
        let body = color::shape_decls(&dot_style(&cd.style), true, Some(&token));
        if !body.is_empty() {
            out.push(SourceRule {
                selector: alloc::format!(".merlion-c-{}>.merlion-quadrant-dot", cd.name),
                body,
            });
        }
    }
    for (i, p) in q.points.iter().enumerate() {
        let body = color::shape_decls(&dot_style(&p.style), true, None);
        if !body.is_empty() {
            out.push(SourceRule {
                selector: alloc::format!(".merlion-ps-{i}>.merlion-quadrant-dot"),
                body,
            });
        }
    }
    out
}

/// The literal fill and stroke a point draws with where CSS does not apply.
fn point_paint(q: &Quadrant, p: &Point, layer: &Layer) -> (String, String) {
    let theme = &q.meta.quadrant.theme;
    let base = css(&theme.point_fill).unwrap_or_else(|| layer.lit(Role::Accent));
    let (mut fill, mut stroke) = (base.clone(), base);
    if let Some(class) = p.class.as_deref() {
        match q.class_def(class) {
            Some(cd) => {
                if let Some(c) = css(&cd.style.color) {
                    fill = layer.class_value(class, ClassProp::Fill).unwrap_or(c);
                }
                if let Some(c) = css(&cd.style.stroke_color) {
                    stroke = layer.class_value(class, ClassProp::Stroke).unwrap_or(c);
                }
            }
            None => {
                let (tone, _) = layer.role_paint(Kind::Node, &[String::from(class)]);
                if let Some(t) = tone {
                    fill = t.clone();
                    stroke = t;
                }
            }
        }
    }
    if let Some(c) = css(&p.style.color) {
        fill = c;
    }
    if let Some(c) = css(&p.style.stroke_color) {
        stroke = c;
    }
    (fill, stroke)
}

fn text_attrs(size: f64, base: f64, bold: bool) -> String {
    let mut s = String::new();
    if size != base {
        s.push_str(" font-size=\"");
        push_num(&mut s, size);
        s.push('"');
    }
    if bold {
        s.push_str(" font-weight=\"600\"");
    }
    s
}

fn text(out: &mut String, g: &TextGeom, class: &str, fill: &str, attrs: &str) {
    label::push_label_with(out, &g.label, g.x, g.y, class, fill, fill, attrs);
}

fn rect(out: &mut String, class: &str, r: Rect, fill: &str) {
    out.push_str("<rect");
    attr(out, "class", class);
    attr_num(out, "x", r.x);
    attr_num(out, "y", r.y);
    attr_num(out, "width", r.w);
    attr_num(out, "height", r.h);
    attr(out, "fill", fill);
    out.push_str("/>");
}

#[allow(clippy::too_many_arguments)]
fn line(out: &mut String, class: &str, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str, w: f64) {
    out.push_str("<line");
    attr(out, "class", class);
    attr_num(out, "x1", x1);
    attr_num(out, "y1", y1);
    attr_num(out, "x2", x2);
    attr_num(out, "y2", y2);
    attr(out, "stroke", stroke);
    attr_num(out, "stroke-width", w);
    out.push_str("/>");
}

/// Draws the chart with literal presentation attributes and token CSS.
pub fn draw_quadrant(
    q: &Quadrant,
    geom: &QuadrantGeometry,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let id = safe_root_id(id);
    let outline = outline_quadrant(q);
    let builtins = builtins(q);
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|p| Layer::new(Some(p), builtins));
    let used = point_roles(q);
    let mut budget = 16 * 1024 - 128;
    let (mut light_roles, light_base) = base_rules(q, &light, geom);
    light_roles.extend(role_rules(
        &light,
        &used,
        &mut budget,
        id.len() + 2,
        Some(diags),
    ));
    let mut light_source = light_base;
    light_source.extend(source_rules(q, &light));
    let dark_rules = dark.as_ref().map(|d| {
        let (mut r, mut s) = base_rules(q, d, geom);
        r.extend(role_rules(
            d,
            &used,
            &mut budget,
            style::dark_prefix(&id).len(),
            None,
        ));
        s.extend(source_rules(q, d));
        (r, s)
    });

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
    out.push_str(" class=\"merlion merlion-quadrant\"");
    attr(&mut out, "data-merlion-version", crate::VERSION);
    attr(&mut out, "data-merlion-layout", &layout_hint(q));
    out.push_str(">\n");
    let _ = write!(out, "<title id=\"{id}-title\">");
    push_escaped(
        &mut out,
        q.meta
            .acc_title
            .as_deref()
            .or(q.meta.title.as_deref())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("Quadrant chart"),
    );
    out.push_str("</title>\n");
    let _ = write!(out, "<desc id=\"{id}-desc\">");
    push_escaped(
        &mut out,
        q.meta
            .acc_descr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&outline),
    );
    out.push_str("</desc>\n");
    let font_css = if opts.font == FontMode::Embed {
        out.push_str(&crate::text::ofl_xml_comment());
        Some(crate::text::embedded_font_css(theme::EMBED_FONT_FAMILY))
    } else {
        None
    };
    let light_style = style::Layer {
        table: &light.table,
        roles: &light_roles,
        source: &light_source,
        sequence: false,
    };
    let dark_style = match (&dark, &dark_rules) {
        (Some(d), Some((roles, source))) => Some(style::Layer {
            table: &d.table,
            roles,
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
    let _ = write!(
        out,
        ":where(#{id} .merlion-node, #{id} .merlion-quadrant-point){{--merlion-tone:initial;--merlion-dash:initial;}}"
    );
    out.push_str("</style>\n");
    if opts.background {
        rect(
            &mut out,
            "merlion-bg",
            Rect {
                x: 0.0,
                y: 0.0,
                w: geom.width,
                h: geom.height,
            },
            &light.lit(Role::Bg),
        );
    }
    out.push_str("<g class=\"merlion-diagram\"");
    attr(&mut out, "font-family", theme::font_stack(opts.font));
    attr_num(&mut out, "font-size", geom.font_size);
    out.push_str(">\n");

    let theme_vars = &q.meta.quadrant.theme;
    let t = &light.table;
    let fg = light.lit(Role::Fg);
    let base = geom.font_size;
    for (i, g) in geom.quadrants.iter().enumerate() {
        let fill = css(&theme_vars.fill[i]).unwrap_or_else(|| {
            roles::mix_lit(&t.lit(Role::Accent), &t.lit(Role::Bg), QUADRANT_MIX[i])
        });
        let _ = write!(
            out,
            "<g class=\"merlion-quadrant-region merlion-quadrant-{0}\" data-merlion-quadrant=\"{0}\">",
            i + 1
        );
        rect(&mut out, "merlion-quadrant-box", g.rect, &fill);
        if let Some(l) = &g.label {
            text(
                &mut out,
                l,
                "merlion-label merlion-quadrant-text merlion-quadrant-label",
                &css(&theme_vars.text[i]).unwrap_or_else(|| fg.clone()),
                &text_attrs(geom.sizes.quadrant, base, false),
            );
        }
        out.push_str("</g>\n");
    }

    let p = geom.plot;
    let (ext, int) = (geom.external_border, geom.internal_border);
    let border = css(&theme_vars.external_border).unwrap_or_else(|| light.lit(Role::Border));
    let divider = css(&theme_vars.internal_border).unwrap_or_else(|| light.lit(Role::Border));
    out.push_str("<g class=\"merlion-quadrant-borders\">");
    let h = ext / 2.0;
    for (x1, y1, x2, y2) in [
        (p.x - h, p.y, p.x + p.w + h, p.y),
        (p.x + p.w, p.y + h, p.x + p.w, p.y + p.h - h),
        (p.x - h, p.y + p.h, p.x + p.w + h, p.y + p.h),
        (p.x, p.y + h, p.x, p.y + p.h - h),
    ] {
        if ext > 0.0 {
            line(
                &mut out,
                "merlion-quadrant-border",
                x1,
                y1,
                x2,
                y2,
                &border,
                ext,
            );
        }
    }
    if int > 0.0 {
        let (mx, my) = (p.x + p.w / 2.0, p.y + p.h / 2.0);
        line(
            &mut out,
            "merlion-quadrant-divider",
            mx,
            p.y + h,
            mx,
            p.y + p.h - h,
            &divider,
            int,
        );
        line(
            &mut out,
            "merlion-quadrant-divider",
            p.x + h,
            my,
            p.x + p.w - h,
            my,
            &divider,
            int,
        );
    }
    out.push_str("</g>\n");

    let point_text = css(&theme_vars.point_text).unwrap_or_else(|| fg.clone());
    // Later points first, so an earlier point draws on top where two overlap, as in
    // mermaid.
    for g in geom.points.iter().rev() {
        let Some(pt) = q.points.get(g.index) else {
            continue;
        };
        let mut class = String::from("merlion-quadrant-point");
        if let Some(c) = &pt.class {
            class.push_str(" merlion-c-");
            class.push_str(c);
        }
        if !dot_style(&pt.style).is_empty() {
            let _ = write!(class, " merlion-ps-{}", g.index);
        }
        out.push_str("<g");
        attr(&mut out, "class", &class);
        attr(&mut out, "data-merlion-id", &pt.label);
        let _ = write!(
            out,
            " id=\"{}-point-{}\" data-merlion-x=\"{}\" data-merlion-y=\"{}\">",
            id,
            g.index,
            value(pt.x),
            value(pt.y)
        );
        out.push_str("<title>");
        push_escaped(
            &mut out,
            &alloc::format!(
                "{}: x {}, y {}",
                plain_label(&pt.label),
                value(pt.x),
                value(pt.y)
            ),
        );
        out.push_str("</title><circle class=\"merlion-quadrant-dot\"");
        attr_num(&mut out, "cx", g.cx);
        attr_num(&mut out, "cy", g.cy);
        attr_num(&mut out, "r", g.radius);
        let (fill, stroke) = point_paint(q, pt, &light);
        attr(&mut out, "fill", &fill);
        if g.stroke_width > 0.0 {
            attr(&mut out, "stroke", &stroke);
            attr_num(&mut out, "stroke-width", g.stroke_width);
        } else {
            out.push_str(" stroke-width=\"0\"");
        }
        out.push_str("/>");
        text(
            &mut out,
            &g.label,
            "merlion-label merlion-quadrant-text merlion-quadrant-point-label",
            &point_text,
            &text_attrs(geom.sizes.point, base, false),
        );
        out.push_str("</g>\n");
    }

    for (axis, labels, fill, size) in [
        (
            "x",
            &geom.x_labels,
            &theme_vars.x_axis_text,
            geom.sizes.x_axis,
        ),
        (
            "y",
            &geom.y_labels,
            &theme_vars.y_axis_text,
            geom.sizes.y_axis,
        ),
    ] {
        for l in labels {
            let _ = write!(
                out,
                "<g class=\"merlion-quadrant-axis\" data-merlion-axis=\"{axis}\" data-merlion-end=\"{}\"",
                if l.high { "high" } else { "low" }
            );
            if axis == "y" {
                out.push_str(" transform=\"rotate(-90 ");
                push_num(&mut out, l.text.x);
                out.push(' ');
                push_num(&mut out, l.text.y);
                out.push_str(")\"");
            }
            out.push('>');
            text(
                &mut out,
                &l.text,
                &alloc::format!("merlion-label merlion-quadrant-text merlion-quadrant-axis-{axis}"),
                &css(fill).unwrap_or_else(|| fg.clone()),
                &text_attrs(size, base, false),
            );
            out.push_str("</g>\n");
        }
    }

    if let Some(title) = &geom.title {
        text(
            &mut out,
            title,
            "merlion-label merlion-quadrant-text merlion-quadrant-title",
            &css(&theme_vars.title).unwrap_or_else(|| fg.clone()),
            &text_attrs(geom.sizes.title, base, true),
        );
    }
    out.push_str("</g>\n</svg>\n");
    DrawOutput { svg: out, outline }
}
