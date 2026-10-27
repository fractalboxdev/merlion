//! Accessible static Gantt SVG using the shared font and semantic palette layers.

use super::escape::push_escaped;
use super::outline::plain_label;
use super::roles::{Kind, BUILT_IN, TONE_FILL};
use super::style::{self, RoleRule, SourceRule};
use super::theme::Role;
use super::{
    attr, attr_num, label, safe_root_id, stroke_attr, theme, DrawOutput, Layer, Paint, Roles,
};
use crate::diag::Diagnostics;
use crate::geometry::gantt::{GanttGeometry, Rect, TextGeom};
use crate::ids;
use crate::model::gantt::{calendar, Gantt, TaskFlags};
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

pub fn layout_hint(gantt: &Gantt) -> String {
    let mut out = String::from("v1;GANTT;0:");
    for (i, t) in gantt.tasks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&ids::encode_id(&t.id));
    }
    out
}

fn flags(f: TaskFlags) -> String {
    let mut out = String::new();
    for (yes, name) in [
        (f.active, "active"),
        (f.done, "done"),
        (f.crit, "crit"),
        (f.milestone, "milestone"),
        (f.vert, "vert"),
    ] {
        if yes {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(name);
        }
    }
    out
}

fn date(gantt: &Gantt, time: i64) -> String {
    if calendar::time_only_format(&gantt.date_format) {
        calendar::format_axis(time, "%H:%M:%S.%L").unwrap_or_default()
    } else {
        calendar::format_iso(time)
    }
}

pub fn outline_gantt(gantt: &Gantt) -> String {
    let mut out = alloc::format!("Gantt diagram. {} tasks.", gantt.tasks.len());
    if let Some(title) = &gantt.meta.title {
        let _ = write!(out, " {}", plain_label(title));
    }
    let mut previous = None;
    for t in &gantt.tasks {
        if previous != t.section {
            if let Some(section) = t.section.and_then(|i| gantt.sections.get(i)) {
                let _ = write!(out, "\nSection: {}", plain_label(section));
            }
            previous = t.section;
        }
        let _ = write!(
            out,
            "\n{} ({}): {} to {}",
            plain_label(&t.label),
            plain_label(&t.id),
            date(gantt, t.start),
            date(gantt, t.end)
        );
        let f = flags(t.flags);
        if !f.is_empty() {
            let _ = write!(out, " [{}]", f);
        }
        if !t.dependencies.is_empty() {
            out.push_str("; depends on ");
            for (n, &i) in t.dependencies.iter().enumerate() {
                if n > 0 {
                    out.push_str(", ");
                }
                if let Some(dep) = gantt.tasks.get(i) {
                    out.push_str(&plain_label(&dep.id));
                }
            }
        }
    }
    out
}

fn gantt_roles(gantt: &Gantt, on: bool) -> Roles {
    let mut used = BTreeSet::new();
    let node = gantt
        .tasks
        .iter()
        .map(|t| {
            let tone = if t.flags.crit {
                Some("danger")
            } else if t.flags.active {
                Some("accent")
            } else if t.flags.done {
                Some("ok")
            } else {
                None
            };
            match tone.filter(|_| on) {
                Some(name) => {
                    let name = String::from(name);
                    used.insert((Kind::Node, name.clone()));
                    alloc::vec![name]
                }
                None => Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    let node_auto = node.iter().map(|r| !r.is_empty()).collect();
    Roles {
        node,
        node_auto,
        edge: Vec::new(),
        cluster: Vec::new(),
        cluster_auto: Vec::new(),
        used,
    }
}

fn rules(layer: &Layer) -> Vec<SourceRule> {
    alloc::vec![
        SourceRule {
            selector: String::from(".merlion-shape"),
            body: alloc::format!("fill:{};stroke:{};stroke-width:var(--merlion-stroke,{});stroke-dasharray:var(--merlion-dash,none);",layer.table.var(Role::NodeBg,false),layer.table.var(Role::NodeBorder,false),stroke_attr(&layer.table)),
        },
        SourceRule {
            selector: String::from(".merlion-gantt-grid"),
            body: alloc::format!(
                "stroke:{};stroke-width:1;fill:none;opacity:0.35;",
                layer.table.var(Role::Line, false)
            )
        },
        SourceRule {
            selector: String::from(".merlion-gantt-axis"),
            body: alloc::format!("fill:{};", layer.table.var(Role::Muted, false))
        },
        SourceRule {
            selector: String::from(".merlion-gantt-section"),
            body: alloc::format!(
                "fill:{};stroke:none;",
                layer.table.var(Role::Surface, false)
            )
        },
        SourceRule {
            selector: String::from(".merlion-gantt-excluded"),
            body: alloc::format!(
                "fill:{};stroke:none;opacity:0.14;",
                layer.table.var(Role::Muted, false)
            )
        },
        SourceRule {
            selector: String::from(".merlion-gantt-heading"),
            body: alloc::format!("fill:{};", layer.table.var(Role::Fg, false))
        },
        SourceRule {
            selector: String::from(".merlion-gantt-done>.merlion-shape"),
            body: String::from("stroke-dasharray:4 2;")
        },
    ]
}

fn neutral_rule(layer: &Layer) -> RoleRule {
    RoleRule {
        selector: String::from(".merlion-gantt-neutral>.merlion-shape"),
        plain: alloc::format!(
            "stroke:var(--merlion-tone,{});",
            layer.table.var(Role::Line, false)
        ),
        mixed: alloc::format!(
            "stroke:var(--merlion-tone,{});",
            layer.table.var(Role::Line, true)
        ),
    }
}

fn rect(out: &mut String, class: &str, r: Rect, fill: &str) {
    out.push_str("<rect");
    attr(out, "class", class);
    attr_num(out, "x", r.x);
    attr_num(out, "y", r.y);
    attr_num(out, "width", r.w);
    attr_num(out, "height", r.h);
    attr(out, "fill", fill);
}
fn text(out: &mut String, g: &TextGeom, class: &str, fill: &str, detail: &str) {
    label::push_label(out, &g.label, g.x, g.y, class, fill, detail);
}
fn open_line(
    out: &mut String,
    class: &str,
    x: f64,
    top: f64,
    bottom: f64,
    stroke: &str,
    width: &str,
) {
    out.push_str("<line");
    attr(out, "class", class);
    attr_num(out, "x1", x);
    attr_num(out, "x2", x);
    attr_num(out, "y1", top);
    attr_num(out, "y2", bottom);
    attr(out, "stroke", stroke);
    attr(out, "stroke-width", width);
}

/// Draws calendar geometry with literal presentation fallbacks and token CSS.
pub fn draw_gantt(
    gantt: &Gantt,
    geom: &GanttGeometry,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let id = safe_root_id(id);
    let outline = outline_gantt(gantt);
    let builtins: Vec<_> = BUILT_IN.iter().collect();
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|p| Layer::new(Some(p), builtins));
    let roles = gantt_roles(gantt, opts.auto_tone && gantt.meta.auto_tone != Some(false));
    let (palette_light, palette_dark) =
        super::palette_rules(&roles, &id, &light, dark.as_ref(), diags);
    let mut light_roles = super::built_in_rules(&roles, &light);
    light_roles.push(neutral_rule(&light));
    light_roles.extend(palette_light);
    let dark_roles = dark.as_ref().map(|d| {
        let mut r = super::built_in_rules(&roles, d);
        r.push(neutral_rule(d));
        r.extend(palette_dark);
        r
    });
    let light_rules = rules(&light);
    let dark_rules = dark.as_ref().map(rules);
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
    out.push_str(" class=\"merlion merlion-gantt\"");
    attr(&mut out, "data-merlion-version", crate::VERSION);
    attr(&mut out, "data-merlion-layout", &layout_hint(gantt));
    out.push_str(">\n");
    let _ = write!(out, "<title id=\"{id}-title\">");
    push_escaped(
        &mut out,
        gantt
            .meta
            .acc_title
            .as_deref()
            .or(gantt.meta.title.as_deref())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("Gantt diagram"),
    );
    out.push_str("</title>\n");
    let _ = write!(out, "<desc id=\"{id}-desc\">");
    push_escaped(
        &mut out,
        gantt
            .meta
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
        source: &light_rules,
        sequence: false,
    };
    let dark_style = match (&dark, &dark_roles, &dark_rules) {
        (Some(d), Some(roles), Some(source)) => Some(style::Layer {
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
    let _=write!(out,":where(#{0} .merlion-node, #{0} .merlion-gantt-task){{--merlion-tone:initial;--merlion-dash:initial;}}",id);
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
        out.push_str("/>");
    }
    out.push_str("<g class=\"merlion-diagram\"");
    attr(&mut out, "font-family", theme::font_stack(opts.font));
    attr_num(&mut out, "font-size", geom.font_size);
    out.push('>');
    let fg = light.lit(Role::Fg);
    let detail = light.lit(Role::NodeDetail);
    let stroke = stroke_attr(&light.table);
    if let Some(title) = &geom.title {
        text(
            &mut out,
            title,
            "merlion-label merlion-gantt-heading",
            &fg,
            &detail,
        );
    }
    for s in &geom.sections {
        out.push_str("<g");
        attr(
            &mut out,
            "data-merlion-section",
            gantt.sections.get(s.index).map_or("", String::as_str),
        );
        out.push('>');
        rect(
            &mut out,
            "merlion-gantt-section",
            s.band,
            &light.lit(Role::Surface),
        );
        out.push_str(" stroke=\"none\"/>");
        out.push_str("</g>");
    }
    for r in &geom.exclusions {
        rect(
            &mut out,
            "merlion-gantt-excluded",
            *r,
            &light.lit(Role::Muted),
        );
        out.push_str(" opacity=\"0.14\" stroke=\"none\"/>");
    }
    for tick in &geom.ticks {
        open_line(
            &mut out,
            "merlion-gantt-grid",
            tick.x,
            geom.plot.y,
            geom.plot.y + geom.plot.h,
            &light.lit(Role::Line),
            "1",
        );
        let _ = write!(out, " data-merlion-time=\"{}\"/>", tick.time);
        text(
            &mut out,
            &tick.bottom,
            "merlion-label merlion-gantt-axis",
            &light.lit(Role::Muted),
            &detail,
        );
        if let Some(top) = &tick.top {
            text(
                &mut out,
                top,
                "merlion-label merlion-gantt-axis",
                &light.lit(Role::Muted),
                &detail,
            );
        }
    }
    for g in &geom.tasks {
        let Some(t) = gantt.tasks.get(g.index) else {
            continue;
        };
        let mut class = String::from("merlion-gantt-task");
        let neutral = roles.node(g.index).is_empty();
        if neutral {
            class.push_str(" merlion-gantt-neutral");
        }
        super::push_role_classes(&mut class, "merlion-c-", roles.node(g.index));
        if t.flags.done {
            class.push_str(" merlion-gantt-done");
        }
        if roles.node_is_auto(g.index) {
            class.push(' ');
            class.push_str(super::auto::MARKER);
        }
        out.push_str("<g");
        attr(&mut out, "class", &class);
        attr(&mut out, "data-merlion-id", &t.id);
        let _ = write!(
            out,
            " id=\"{}-task-{}\" data-merlion-start=\"{}\" data-merlion-end=\"{}\"",
            id, g.index, t.start, t.end
        );
        attr(
            &mut out,
            "data-merlion-section",
            t.section
                .and_then(|i| gantt.sections.get(i))
                .map_or("", String::as_str),
        );
        attr(&mut out, "data-merlion-flags", &flags(t.flags));
        out.push('>');
        let mut paint = Paint {
            fill: light.lit(Role::NodeBg),
            stroke: light.lit(if neutral {
                Role::Line
            } else {
                Role::NodeBorder
            }),
            text: light.lit(Role::NodeText),
            dash: None,
        };
        paint.roles(
            &light,
            Kind::Node,
            roles.node(g.index),
            Role::NodeBg,
            TONE_FILL,
            Role::NodeText,
        );
        out.push_str("<title>");
        push_escaped(
            &mut out,
            &alloc::format!(
                "{}: {} to {} {}",
                plain_label(&t.label),
                date(gantt, t.start),
                date(gantt, t.end),
                flags(t.flags)
            ),
        );
        out.push_str("</title>");
        if t.flags.vert {
            open_line(
                &mut out,
                "merlion-shape",
                g.bar.x,
                g.bar.y,
                g.bar.y + g.bar.h,
                &paint.stroke,
                &stroke,
            );
            out.push_str("/>");
        } else {
            if t.flags.milestone {
                let x = g.bar.x + g.bar.w / 2.0;
                let y = g.bar.y + g.bar.h / 2.0;
                out.push_str("<path class=\"merlion-shape\"");
                let mut d = String::from("M");
                push_num(&mut d, x);
                d.push(' ');
                push_num(&mut d, y - 7.0);
                d.push('L');
                push_num(&mut d, x + 7.0);
                d.push(' ');
                push_num(&mut d, y);
                d.push('L');
                push_num(&mut d, x);
                d.push(' ');
                push_num(&mut d, y + 7.0);
                d.push('L');
                push_num(&mut d, x - 7.0);
                d.push(' ');
                push_num(&mut d, y);
                d.push('Z');
                attr(&mut out, "d", &d);
            } else {
                rect(&mut out, "merlion-shape", g.bar, &paint.fill);
                attr_num(&mut out, "rx", 3.0);
            }
            if t.flags.milestone {
                attr(&mut out, "fill", &paint.fill);
            }
            attr(&mut out, "stroke", &paint.stroke);
            attr(&mut out, "stroke-width", &stroke);
            if t.flags.done {
                out.push_str(" stroke-dasharray=\"4 2\"");
            }
            out.push_str("/>");
        }
        text(
            &mut out,
            &g.text,
            "merlion-label merlion-gantt-task-text",
            &paint.text,
            &detail,
        );
        out.push_str("</g>\n");
    }
    for s in &geom.sections {
        text(
            &mut out,
            &s.text,
            "merlion-label merlion-gantt-heading",
            &fg,
            &detail,
        );
    }
    out.push_str("</g>\n</svg>\n");
    DrawOutput { svg: out, outline }
}
