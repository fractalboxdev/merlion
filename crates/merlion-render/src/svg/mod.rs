//! SVG writer (specs/svg-output.md).
//!
//! Document structure:
//!
//! ```text
//! <svg …root attributes…>
//!   <title id="{id}-title">  <desc id="{id}-desc">  <style>
//!   <defs> markers actually used, ids {id}-arrow / {id}-circle / {id}-cross </defs>
//!   <rect class="merlion-bg">                      (background: true only)
//!   <g class="merlion-diagram" font-family font-size>
//!     clusters, edges and nodes in the order of `tree`
//!   </g>
//! </svg>
//! ```
//!
//! Every colour, font and stroke is written twice (specs/svg-output.md#theming): as a
//! presentation attribute carrying the literal default, and as an id-scoped CSS rule in
//! `<style>` reading the custom property with the same fallback. Source styles become
//! id-scoped rules keyed by a class; the output has no `style` attribute built from
//! source text. Every number is printed by `numfmt`.

mod color;
pub(crate) mod escape;
mod label;
mod link;
mod outline;
mod path;
mod roles;
mod shapes;
mod style;
pub mod theme;
mod tree;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::diag::{Diagnostics, Severity, Span};
use crate::geometry::{EdgeGeom, Geometry};
use crate::ids;
use crate::model::{Arrow, Edge, Flowchart, Node, Stroke, Style};
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};

use escape::push_escaped;
use roles::{Kind, Tone, BUILT_IN, TONE_CLUSTER_FILL, TONE_FILL, TONE_TEXT};
use style::{RoleRule, SourceRule};
use theme::Role;

pub use escape::escape;
pub use outline::plain_label;
pub use shapes::ALL_SHAPES;
pub use theme::EMBED_FONT_FAMILY;

pub struct DrawOutput {
    pub svg: String,
    /// Plain-text outline (specs/svg-output.md#text-alternative).
    pub outline: String,
}

/// Corner radius of the edge label chip and the cluster box, in px.
const CHIP_RADIUS: f64 = 3.0;
const CLUSTER_RADIUS: f64 = 4.0;
/// Highest semantic-zoom rank (specs/viewer.md#semantic-zoom).
const MAX_RANK: u8 = 15;

fn attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    push_escaped(out, value);
    out.push('"');
}

fn attr_num(out: &mut String, name: &str, v: f64) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    push_num(out, v);
    out.push('"');
}

fn stroke_attr() -> String {
    let mut s = String::new();
    push_num(&mut s, theme::STROKE);
    s
}

/// The root id, defensively: a value outside `[a-z][a-z0-9-]{0,31}` would reach CSS
/// selectors and XML ids, so it is replaced by `m` + its injective encoding.
fn safe_root_id(id: &str) -> String {
    if ids::is_valid_id_prefix(id) {
        String::from(id)
    } else {
        let mut s = String::from("m");
        s.push_str(&ids::encode_id(id));
        s
    }
}

/// `v1;{dir};0:a,b;1:c` (specs/svg-output.md#layout-hint).
pub fn layout_hint(chart: &Flowchart, geom: &Geometry) -> String {
    let mut s = String::from("v1;");
    s.push_str(geom.direction.as_str());
    for (li, layer) in geom.layers.iter().enumerate() {
        let _ = write!(s, ";{}:", li);
        let mut first = true;
        for &ni in layer {
            let Some(n) = chart.nodes.get(ni) else {
                continue;
            };
            if !first {
                s.push(',');
            }
            s.push_str(&ids::encode_id(&n.id));
            first = false;
        }
    }
    s
}

fn title_text(chart: &Flowchart) -> String {
    let pick = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(String::from)
    };
    pick(&chart.meta.acc_title)
        .or_else(|| pick(&chart.meta.title))
        .unwrap_or_else(|| String::from("Flowchart diagram"))
}

fn marker_name(a: Arrow) -> Option<&'static str> {
    match a {
        Arrow::Arrow => Some("arrow"),
        Arrow::Circle => Some("circle"),
        Arrow::Cross => Some("cross"),
        Arrow::None => None,
    }
}

/// One marker definition: a head kind and the role classes of the edges that use it.
/// An edge's marker carries the edge's role classes because a marker inherits from
/// `<defs>`, not from the edge that references it (specs/svg-output.md#roles).
#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkerDef {
    kind: Arrow,
    roles: Vec<String>,
    /// The XML id suffix after `{id}-`.
    name: String,
    /// The literal fill (or stroke, for `cross`).
    colour: String,
}

/// The markers the drawn edges use: role-less markers first in the fixed order arrow,
/// circle, cross, then role markers in first-use order.
fn collect_markers(chart: &Flowchart, geom: &Geometry) -> Vec<MarkerDef> {
    let mut plain: Vec<MarkerDef> = Vec::new();
    let mut roled: Vec<MarkerDef> = Vec::new();
    let mut sets: Vec<Vec<String>> = Vec::new();
    for (e, _) in chart.edges.iter().zip(geom.edges.iter()) {
        if e.stroke == Stroke::Invisible {
            continue;
        }
        let roles = valid_classes(&e.classes);
        for a in [e.arrow_start, e.arrow_end] {
            let Some(kind) = marker_name(a) else {
                continue;
            };
            let target = if roles.is_empty() {
                &mut plain
            } else {
                &mut roled
            };
            if target.iter().any(|m| m.kind == a && m.roles == roles) {
                continue;
            }
            let name = match roles.as_slice() {
                [] => String::from(kind),
                [one] => alloc::format!("{}-c-{}", kind, one),
                _ => {
                    let k = match sets.iter().position(|s| *s == roles) {
                        Some(k) => k,
                        None => {
                            sets.push(roles.clone());
                            sets.len() - 1
                        }
                    };
                    alloc::format!("{}-r{}", kind, k)
                }
            };
            let colour =
                edge_role_tone(&roles).unwrap_or_else(|| String::from(Role::Edge.default_value()));
            target.push(MarkerDef {
                kind: a,
                roles: roles.clone(),
                name,
                colour,
            });
        }
    }
    let order = |a: Arrow| match a {
        Arrow::Arrow => 0,
        Arrow::Circle => 1,
        _ => 2,
    };
    plain.sort_by_key(|m| order(m.kind));
    plain.extend(roled);
    plain
}

/// The marker id suffix for an edge head.
fn marker_ref<'a>(markers: &'a [MarkerDef], kind: Arrow, roles: &[String]) -> Option<&'a str> {
    markers
        .iter()
        .find(|m| m.kind == kind && m.roles == roles)
        .map(|m| m.name.as_str())
}

/// `<defs>` with each used marker once. Markers are 10 × 10 user units with the tip at
/// x = 9, oriented with `auto-start-reverse` so the same marker serves `marker-start`.
fn push_defs(out: &mut String, id: &str, markers: &[MarkerDef]) {
    if markers.is_empty() {
        return;
    }
    out.push_str("<defs>");
    for m in markers {
        let _ = write!(out, "<marker id=\"{}-{}\"", id, m.name);
        if !m.roles.is_empty() {
            let mut class = String::new();
            for r in &m.roles {
                if !class.is_empty() {
                    class.push(' ');
                }
                class.push_str("merlion-c-");
                class.push_str(r);
            }
            attr(out, "class", &class);
        }
        out.push_str(
            " viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"10\" \
             markerHeight=\"10\" markerUnits=\"userSpaceOnUse\" orient=\"auto-start-reverse\">",
        );
        match m.kind {
            Arrow::Circle => {
                out.push_str(
                    "<path class=\"merlion-marker-fill\" d=\"M1 5A4 4 0 1 1 9 5A4 4 0 1 1 1 5Z\"",
                );
                attr(out, "fill", &m.colour);
                out.push_str(" stroke=\"none\"/>");
            }
            Arrow::Cross => {
                out.push_str(
                    "<path class=\"merlion-marker-stroke\" d=\"M2 1L9 9M2 9L9 1\" fill=\"none\"",
                );
                attr(out, "stroke", &m.colour);
                out.push_str(" stroke-width=\"1.5\"/>");
            }
            _ => {
                out.push_str("<path class=\"merlion-marker-fill\" d=\"M0 0L10 5L0 10Z\"");
                attr(out, "fill", &m.colour);
                out.push_str(" stroke=\"none\"/>");
            }
        }
        out.push_str("</marker>");
    }
    out.push_str("</defs>");
}

/// Valid role names of an element, deduplicated, in source order.
fn valid_classes(classes: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in classes {
        if color::is_valid_class_name(c) && !out.contains(c) {
            out.push(c.clone());
        }
    }
    out
}

/// The literal tone of the last toned built-in edge role in `roles`, in rule order.
fn edge_role_tone(roles: &[String]) -> Option<String> {
    BUILT_IN
        .iter()
        .filter(|b| b.kind == Kind::Edge && roles.iter().any(|r| r == b.name))
        .rev()
        .find_map(|b| b.tone)
        .map(|t| String::from(t.default_value()))
}

/// The literal paint of one element: what its presentation attributes carry.
struct Paint {
    fill: String,
    stroke: String,
    text: String,
    dash: Option<String>,
}

impl Paint {
    /// Applies built-in roles of `kind` in rule order.
    fn built_in(
        &mut self,
        kind: Kind,
        roles: &[String],
        fill_base: Role,
        fill_pct: u8,
        text_base: Role,
    ) {
        for b in BUILT_IN
            .iter()
            .filter(|b| b.kind == kind && roles.iter().any(|r| r == b.name))
        {
            if let Some(t) = b.tone {
                let t = t.default_value();
                self.fill = roles::mix_lit(t, fill_base.default_value(), fill_pct);
                self.stroke = String::from(t);
                self.text = roles::mix_lit(t, text_base.default_value(), TONE_TEXT);
            }
            if let Some(d) = b.dash {
                self.dash = Some(String::from(d));
            }
        }
    }

    /// Applies the literals of a source style.
    fn style(&mut self, st: &Style, with_fill: bool) {
        if with_fill {
            if let Some(v) = st.fill.as_ref().and_then(color::color_css) {
                self.fill = v;
            }
        }
        if let Some(v) = st.stroke.as_ref().and_then(color::color_css) {
            self.stroke = v;
        }
        if let Some(v) = st
            .color
            .as_ref()
            .filter(|c| **c != crate::model::Color::None)
            .and_then(color::color_css)
        {
            self.text = v;
        }
        if let Some(v) = st.stroke_dasharray.as_deref().and_then(color::dash_css) {
            self.dash = Some(v);
        }
    }

    /// Applies the `classDef` styles of `roles`, in `classDef` order (the rule order).
    fn class_defs(&mut self, chart: &Flowchart, roles: &[String], with_fill: bool) {
        for cd in &chart.class_defs {
            if roles.contains(&cd.name) {
                self.style(&cd.style, with_fill);
            }
        }
    }
}

/// Collects id-scoped source-style rules and the classes that key them.
struct SourceStyles {
    rules: Vec<SourceRule>,
    fixed_colour: bool,
}

impl SourceStyles {
    /// Rules for `class` (a selector such as `.merlion-c-hot`); `token` names the
    /// `classDef` whose colours read their overridable tokens.
    fn add(
        &mut self,
        class: &str,
        style: &Style,
        shape_sel: &str,
        with_fill: bool,
        token: Option<&str>,
    ) -> bool {
        self.fixed_colour |= color::is_fixed(&style.fill)
            || color::is_fixed(&style.stroke)
            || color::is_fixed(&style.color);
        let mut any = false;
        for (sel, body) in [
            (String::from(class), color::group_decls(style)),
            (
                alloc::format!("{}>{}", class, shape_sel),
                color::shape_decls(style, with_fill, token),
            ),
            (
                alloc::format!("{} text", class),
                color::text_decls(style, token),
            ),
        ] {
            if !body.is_empty() {
                self.rules.push(SourceRule {
                    selector: sel,
                    body,
                });
                any = true;
            }
        }
        any
    }
}

impl SourceStyles {
    /// Rules for a cluster: its box and its title, reached by child combinators so the
    /// members nested in the cluster keep their own colours.
    fn add_cluster(&mut self, class: &str, style: &Style, token: Option<&str>) -> bool {
        self.fixed_colour |= color::is_fixed(&style.fill)
            || color::is_fixed(&style.stroke)
            || color::is_fixed(&style.color);
        let mut any = false;
        for (sel, body) in [
            (
                alloc::format!("{}>.merlion-cluster-box", class),
                color::shape_decls(style, true, token),
            ),
            (
                alloc::format!("{}>.merlion-cluster-title", class),
                color::text_decls(style, token),
            ),
        ] {
            if !body.is_empty() {
                self.rules.push(SourceRule {
                    selector: sel,
                    body,
                });
                any = true;
            }
        }
        any
    }
}

fn node_style_class(i: usize) -> String {
    alloc::format!("merlion-ns-{}", i)
}

fn edge_style_class(i: usize) -> String {
    alloc::format!("merlion-es-{}", i)
}

fn cluster_style_class(i: usize) -> String {
    alloc::format!("merlion-ss-{}", i)
}

struct Ctx<'a> {
    chart: &'a Flowchart,
    geom: &'a Geometry,
    opts: &'a RenderOptions,
    id: &'a str,
    /// Per node: the generated style class, when its `style` produced rules.
    node_class: Vec<bool>,
    edge_class: Vec<bool>,
    /// Per cluster: whether its `style` produced rules; and the `classDef` names used
    /// by some cluster whose rules exist.
    cluster_class: Vec<bool>,
    markers: Vec<MarkerDef>,
}

fn node_id(chart: &Flowchart, i: usize) -> &str {
    chart.nodes.get(i).map(|n| n.id.as_str()).unwrap_or("")
}

fn push_node(out: &mut String, cx: &Ctx, i: usize, diags: &mut Diagnostics) {
    let (Some(node), Some(g)) = (cx.chart.nodes.get(i), cx.geom.nodes.get(i)) else {
        return;
    };
    let href = node.link.as_ref().and_then(|l| {
        let h = link::safe_href(&l.url);
        if h.is_none() {
            diags.emit(
                Severity::Warning,
                "W013",
                node.span,
                "link URL outside the accepted schemes dropped",
            );
        }
        h.map(|h| (h, l.target_blank))
    });
    if let Some((h, blank)) = &href {
        out.push_str("<a");
        attr(out, "href", h);
        if *blank {
            out.push_str(" target=\"_blank\" rel=\"noopener noreferrer\"");
        }
        out.push('>');
    }
    out.push_str("<g");
    attr(out, "class", &node_classes(node, i, cx));
    attr(out, "data-merlion-id", &node.id);
    let rank = if g.rank > MAX_RANK { MAX_RANK } else { g.rank };
    let _ = write!(out, " data-merlion-rank=\"{}\">", rank);
    let paint = node_paint(cx.chart, node);
    out.push_str("<path class=\"merlion-shape\"");
    attr(out, "d", &shapes::shape_d(node.shape, g.x, g.y, g.w, g.h));
    attr(out, "fill", &paint.fill);
    attr(out, "stroke", &paint.stroke);
    attr(out, "stroke-width", &stroke_attr());
    out.push_str("/>");
    label::push_label(
        out,
        &g.label,
        g.x,
        g.y + crate::layout::measure::label_offset(node.shape, g.w, g.h, g.label.height),
        "merlion-label",
        &paint.text,
    );
    out.push_str("</g>");
    if href.is_some() {
        out.push_str("</a>");
    }
    out.push('\n');
}

fn node_paint(chart: &Flowchart, node: &Node) -> Paint {
    let roles = valid_classes(&node.classes);
    let mut p = Paint {
        fill: String::from(Role::NodeBg.default_value()),
        stroke: String::from(Role::NodeBorder.default_value()),
        text: String::from(Role::NodeText.default_value()),
        dash: None,
    };
    p.built_in(Kind::Node, &roles, Role::NodeBg, TONE_FILL, Role::NodeText);
    p.class_defs(chart, &roles, true);
    p.style(&node.style, true);
    p
}

fn edge_paint(chart: &Flowchart, e: &Edge) -> Paint {
    let roles = valid_classes(&e.classes);
    let mut p = Paint {
        fill: String::from("none"),
        stroke: String::from(Role::Edge.default_value()),
        text: String::from(Role::Fg.default_value()),
        dash: (e.stroke == Stroke::Dotted).then(|| String::from("3 3")),
    };
    p.built_in(Kind::Edge, &roles, Role::Edge, 0, Role::Fg);
    p.fill = String::from("none");
    p.class_defs(chart, &roles, false);
    p.style(&e.style, false);
    p
}

fn cluster_paint(chart: &Flowchart, sg: &crate::model::Subgraph) -> Paint {
    let roles = valid_classes(&sg.classes);
    let mut p = Paint {
        fill: String::from(Role::ClusterBg.default_value()),
        stroke: String::from(Role::ClusterBorder.default_value()),
        text: String::from(Role::Fg.default_value()),
        dash: None,
    };
    p.built_in(
        Kind::Cluster,
        &roles,
        Role::ClusterBg,
        TONE_CLUSTER_FILL,
        Role::Fg,
    );
    p.class_defs(chart, &roles, true);
    p.style(&sg.style, true);
    p
}

fn push_role_classes(c: &mut String, prefix: &str, classes: &[String]) {
    for name in valid_classes(classes) {
        c.push(' ');
        c.push_str(prefix);
        c.push_str(&name);
    }
}

fn node_classes(node: &Node, i: usize, cx: &Ctx) -> String {
    let mut c = String::from("merlion-node");
    push_role_classes(&mut c, "merlion-c-", &node.classes);
    if cx.node_class.get(i).copied().unwrap_or(false) {
        c.push(' ');
        c.push_str(&node_style_class(i));
    }
    c
}

fn push_edge(out: &mut String, cx: &Ctx, ei: usize) {
    let (Some(e), Some(g)) = (cx.chart.edges.get(ei), cx.geom.edges.get(ei)) else {
        return;
    };
    if e.stroke == Stroke::Invisible {
        return;
    }
    let d = path::edge_d(&g.points, cx.opts.edge_style);
    out.push_str("<g");
    let mut class = String::from("merlion-edge");
    push_role_classes(&mut class, "merlion-c-", &e.classes);
    if cx.edge_class.get(ei).copied().unwrap_or(false) {
        class.push(' ');
        class.push_str(&edge_style_class(ei));
    }
    attr(out, "class", &class);
    attr(out, "data-merlion-from", node_id(cx.chart, e.from));
    attr(out, "data-merlion-to", node_id(cx.chart, e.to));
    if g.back {
        out.push_str(" data-merlion-back=\"true\"");
    }
    if g.wrap {
        out.push_str(" data-merlion-wrap=\"true\"");
    }
    out.push('>');
    let paint = edge_paint(cx.chart, e);
    push_edge_path(out, cx, e, &d, &paint);
    push_edge_label(out, g, &paint.text);
    out.push_str("</g>\n");
}

fn push_edge_path(out: &mut String, cx: &Ctx, e: &Edge, d: &str, paint: &Paint) {
    let mut class = String::from("merlion-edge-path");
    let mut width = theme::STROKE;
    match e.stroke {
        Stroke::Thick => {
            class.push_str(" merlion-thick");
            width *= 2.0;
        }
        Stroke::Dotted => class.push_str(" merlion-dotted"),
        Stroke::Normal | Stroke::Invisible => {}
    }
    out.push_str("<path");
    attr(out, "class", &class);
    attr(out, "d", d);
    out.push_str(" fill=\"none\"");
    attr(out, "stroke", &paint.stroke);
    attr_num(out, "stroke-width", width);
    if let Some(dash) = &paint.dash {
        attr(out, "stroke-dasharray", dash);
    }
    let roles = valid_classes(&e.classes);
    for (a, which) in [(e.arrow_start, "marker-start"), (e.arrow_end, "marker-end")] {
        if let Some(m) = marker_ref(&cx.markers, a, &roles) {
            let _ = write!(out, " {}=\"url(#{}-{})\"", which, cx.id, m);
        }
    }
    out.push_str("/>");
}

fn push_edge_label(out: &mut String, g: &EdgeGeom, text: &str) {
    let Some(l) = &g.label else {
        return;
    };
    if l.label.lines.is_empty() {
        return;
    }
    let (w, h) = crate::geometry::chip_size(&l.label);
    out.push_str("<g class=\"merlion-edge-label\"><rect class=\"merlion-edge-label-bg\"");
    attr_num(out, "x", l.x - w / 2.0);
    attr_num(out, "y", l.y - h / 2.0);
    attr_num(out, "width", nonneg(w));
    attr_num(out, "height", nonneg(h));
    attr_num(out, "rx", CHIP_RADIUS);
    attr(out, "fill", Role::EdgeLabelBg.default_value());
    out.push_str("/>");
    label::push_label(out, &l.label, l.x, l.y, "merlion-edge-text", text);
    out.push_str("</g>");
}

fn nonneg(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

fn push_cluster_open(out: &mut String, cx: &Ctx, si: usize) {
    let Some(sg) = cx.chart.subgraphs.get(si) else {
        return;
    };
    let mut class = String::from("merlion-cluster");
    push_role_classes(&mut class, "merlion-cc-", &sg.classes);
    if cx.cluster_class.get(si).copied().unwrap_or(false) {
        class.push(' ');
        class.push_str(&cluster_style_class(si));
    }
    out.push_str("<g");
    attr(out, "class", &class);
    attr(out, "data-merlion-id", &sg.id);
    out.push_str(">\n");
    let Some(g) = cx.geom.clusters.get(si) else {
        return;
    };
    out.push_str("<rect class=\"merlion-cluster-box\"");
    attr_num(out, "x", g.x);
    attr_num(out, "y", g.y);
    attr_num(out, "width", nonneg(g.w));
    attr_num(out, "height", nonneg(g.h));
    attr_num(out, "rx", CLUSTER_RADIUS);
    let paint = cluster_paint(cx.chart, sg);
    attr(out, "fill", &paint.fill);
    attr(out, "stroke", &paint.stroke);
    attr(out, "stroke-width", &stroke_attr());
    if let Some(dash) = &paint.dash {
        attr(out, "stroke-dasharray", dash);
    }
    out.push_str("/>");
    label::push_label(
        out,
        &g.label,
        g.label_x,
        g.label_y,
        "merlion-cluster-title",
        &paint.text,
    );
    out.push('\n');
}

/// Rules of the built-in roles the diagram uses on their element kind, in table order
/// (specs/svg-output.md#built-in-roles).
fn built_in_rules(chart: &Flowchart) -> Vec<RoleRule> {
    let uses = |b: &roles::BuiltIn| match b.kind {
        Kind::Node => chart
            .nodes
            .iter()
            .any(|n| valid_classes(&n.classes).iter().any(|c| c == b.name)),
        Kind::Edge => chart
            .edges
            .iter()
            .filter(|e| e.stroke != Stroke::Invisible)
            .any(|e| valid_classes(&e.classes).iter().any(|c| c == b.name)),
        Kind::Cluster => chart
            .subgraphs
            .iter()
            .any(|s| valid_classes(&s.classes).iter().any(|c| c == b.name)),
    };
    let mut out = Vec::new();
    for b in BUILT_IN.iter().filter(|b| uses(b)) {
        let tone = b.tone.map(Tone::of_role);
        out.extend(match b.kind {
            Kind::Node => roles::node_rules(b.name, tone.as_ref(), b.dash),
            Kind::Edge => roles::edge_rules(b.name, tone.as_ref(), b.dash),
            Kind::Cluster => roles::cluster_rules(b.name, tone.as_ref(), b.dash),
        });
    }
    out
}

/// Outline only, without layout (for `merlion outline`), in the source direction.
pub fn outline_flowchart(chart: &Flowchart) -> String {
    outline::outline(chart, chart.direction)
}

/// Draws a laid-out flowchart. `id` is the validated `id_prefix` or the default hash id.
///
/// The outline is written in the drawn direction (`geom.direction`, which differs from
/// the source under `direction: auto`); `accDescr` replaces it in `<desc>` only.
pub fn draw_flowchart(
    chart: &Flowchart,
    geom: &Geometry,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let id = safe_root_id(id);
    let outline_text = outline::outline(chart, geom.direction);

    // Source styles: classDef rules (nodes, then edges that use the class), cluster
    // classDef rules, then node `style`, then `linkStyle`.
    let mut src = SourceStyles {
        rules: Vec::new(),
        fixed_colour: false,
    };
    let edge_roles: Vec<Vec<String>> = chart
        .edges
        .iter()
        .map(|e| valid_classes(&e.classes))
        .collect();
    for cd in &chart.class_defs {
        if color::is_valid_class_name(&cd.name) {
            let class = alloc::format!(".merlion-c-{}", cd.name);
            src.add(&class, &cd.style, ".merlion-shape", true, Some(&cd.name));
            if edge_roles.iter().any(|r| r.contains(&cd.name)) {
                let body = color::shape_decls(&cd.style, false, Some(&cd.name));
                if !body.is_empty() {
                    src.rules.push(SourceRule {
                        selector: alloc::format!("{}>.merlion-edge-path", class),
                        body,
                    });
                }
            }
        }
    }
    // Clusters: classDef rules for the names some cluster uses, then `style`.
    for cd in &chart.class_defs {
        if color::is_valid_class_name(&cd.name)
            && chart.subgraphs.iter().any(|s| s.classes.contains(&cd.name))
        {
            src.add_cluster(
                &alloc::format!(".merlion-cc-{}", cd.name),
                &cd.style,
                Some(&cd.name),
            );
        }
    }
    let cluster_class: Vec<bool> = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            !s.style.is_empty()
                && src.add_cluster(
                    &alloc::format!(".{}", cluster_style_class(i)),
                    &s.style,
                    None,
                )
        })
        .collect();
    let node_class: Vec<bool> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            !n.style.is_empty()
                && src.add(
                    &alloc::format!(".{}", node_style_class(i)),
                    &n.style,
                    ".merlion-shape",
                    true,
                    None,
                )
        })
        .collect();
    let edge_class: Vec<bool> = chart
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            !e.style.is_empty()
                && src.add(
                    &alloc::format!(".{}", edge_style_class(i)),
                    &e.style,
                    ".merlion-edge-path",
                    false,
                    None,
                )
        })
        .collect();
    if src.fixed_colour {
        diags.emit_once(
            Severity::Info,
            "I030",
            Span::default(),
            crate::parse::style::FIXED_COLOUR_MESSAGE,
        );
    }

    let role_rules = built_in_rules(chart);
    let markers = collect_markers(chart, geom);
    let cx = Ctx {
        chart,
        geom,
        opts,
        id: &id,
        node_class,
        edge_class,
        cluster_class,
        markers,
    };

    let (w, h) = (nonneg(geom.width), nonneg(geom.height));
    let mut out = String::with_capacity(4096 + chart.nodes.len() * 400);

    // Root element (specs/svg-output.md#root-element), attributes in the spec's order.
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    attr(&mut out, "id", &id);
    out.push_str(" viewBox=\"0 0 ");
    push_num(&mut out, w);
    out.push(' ');
    push_num(&mut out, h);
    out.push_str("\" width=\"100%\" style=\"max-width:");
    push_num(&mut out, w);
    out.push_str("px\" role=\"img\"");
    let _ = write!(out, " aria-labelledby=\"{0}-title {0}-desc\"", id);
    out.push_str(" class=\"merlion merlion-flowchart\"");
    attr(&mut out, "data-merlion-version", crate::VERSION);
    attr(&mut out, "data-merlion-layout", &layout_hint(chart, geom));
    out.push_str(">\n");

    let _ = write!(out, "<title id=\"{}-title\">", id);
    push_escaped(&mut out, &title_text(chart));
    out.push_str("</title>\n");
    let _ = write!(out, "<desc id=\"{}-desc\">", id);
    let desc = chart
        .meta
        .acc_descr
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .unwrap_or(&outline_text);
    push_escaped(&mut out, desc);
    out.push_str("</desc>\n");

    // `font: "embed"` (specs/text-measurement.md#serving-the-font): the OFL notice as an
    // XML comment, then the WOFF2 subset as the only `@font-face` of the style.
    let font_css = if opts.font == FontMode::Embed {
        out.push_str(&crate::text::ofl_xml_comment());
        out.push('\n');
        Some(crate::text::embedded_font_css(theme::EMBED_FONT_FAMILY))
    } else {
        None
    };
    let font_size = if opts.font_size.is_finite() && opts.font_size > 0.0 {
        opts.font_size
    } else {
        14.0
    };
    // The measured size of detail lines, when some node label has them.
    let detail_size = geom
        .nodes
        .iter()
        .flat_map(|n| n.label.lines.iter())
        .find(|l| l.detail)
        .map(|l| l.size);
    out.push_str("<style>");
    // The style text contains no `<` or `&`: ids, class names and colours are validated.
    out.push_str(&style::build(
        &id,
        opts.font,
        font_size,
        detail_size,
        font_css.as_deref(),
        &role_rules,
        &src.rules,
    ));
    out.push_str("</style>\n");

    push_defs(&mut out, &id, &cx.markers);

    if opts.background {
        out.push_str("<rect class=\"merlion-bg\" x=\"0\" y=\"0\"");
        attr_num(&mut out, "width", w);
        attr_num(&mut out, "height", h);
        attr(&mut out, "fill", Role::Bg.default_value());
        out.push_str("/>\n");
    }

    out.push_str("<g class=\"merlion-diagram\"");
    attr(&mut out, "font-family", theme::font_stack(opts.font));
    attr_num(&mut out, "font-size", font_size);
    out.push_str(">\n");

    let parents = tree::effective_parents(chart);
    let n_sg = chart.subgraphs.len();
    let node_home: Vec<Option<usize>> = chart
        .nodes
        .iter()
        .map(|n| n.subgraph.filter(|&s| s < n_sg))
        .collect();
    let edge_home: Vec<Option<usize>> = chart
        .edges
        .iter()
        .map(|e| {
            let a = node_home.get(e.from).copied().flatten();
            let b = node_home.get(e.to).copied().flatten();
            tree::common_cluster(&parents, a, b)
        })
        .collect();
    for item in tree::draw_order(&parents, &node_home, &edge_home) {
        match item {
            tree::Item::Open(s) => push_cluster_open(&mut out, &cx, s),
            tree::Item::Close(s) => {
                if s < n_sg {
                    out.push_str("</g>\n");
                }
            }
            tree::Item::Edge(e) => push_edge(&mut out, &cx, e),
            tree::Item::Node(i) => push_node(&mut out, &cx, i, diags),
        }
    }
    out.push_str("</g>\n</svg>\n");

    DrawOutput {
        svg: out,
        outline: outline_text,
    }
}
