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

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::diag::{Diagnostics, Severity, Span};
use crate::geometry::{EdgeGeom, Geometry};
use crate::ids;
use crate::model::{Arrow, Edge, Flowchart, Node, Stroke, Style};
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};

use crate::stylesheet::{ClassProp, PaletteTable, PaletteTone};
use color::ClassToken;
use escape::push_escaped;
use roles::{BuiltIn, Kind, Tone, BUILT_IN, TONE_CLUSTER_FILL, TONE_FILL, TONE_TEXT};
use style::{RoleRule, SourceRule};
use theme::{Role, Table};

pub use color::is_valid_class_name;
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

fn stroke_attr(t: &Table) -> String {
    let mut s = String::new();
    push_num(&mut s, t.stroke_px());
    s
}

/// One literal table and the palette table it comes from (specs/svg-output.md#palette).
struct Layer<'a> {
    table: Table,
    palette: Option<&'a PaletteTable>,
    /// The built-in roles in effect: those the source does not define with a `classDef`
    /// of the same name (specs/svg-output.md#built-in-roles).
    builtins: Vec<&'static BuiltIn>,
    /// Each palette tone's index in cascade order, by (cluster, role name).
    tone_at: BTreeMap<(bool, &'a str), usize>,
}

/// The built-in roles a diagram gets: a `classDef` of the same name replaces one.
fn active_builtins(chart: &Flowchart) -> Vec<&'static BuiltIn> {
    BUILT_IN
        .iter()
        .filter(|b| !chart.class_defs.iter().any(|cd| cd.name == b.name))
        .collect()
}

impl<'a> Layer<'a> {
    fn new(palette: Option<&'a PaletteTable>, builtins: Vec<&'static BuiltIn>) -> Self {
        let mut tone_at = BTreeMap::new();
        for (i, t) in palette.map_or(&[][..], |p| &p.tones[..]).iter().enumerate() {
            if color::is_valid_class_name(&t.name) {
                tone_at.insert((t.cluster, t.name.as_str()), i);
            }
        }
        Layer {
            table: palette.map(PaletteTable::theme_table).unwrap_or_default(),
            palette,
            builtins,
            tone_at,
        }
    }

    /// The palette tone of role `name` on elements of `kind`.
    fn tone(&self, kind: Kind, name: &str) -> Option<(usize, &'a PaletteTone)> {
        let i = *self.tone_at.get(&(kind == Kind::Cluster, name))?;
        Some((i, self.palette?.tones.get(i)?))
    }

    /// The palette tones of `roles` on `kind`, in cascade order.
    fn tones_of(&self, kind: Kind, roles: &[String]) -> Vec<&'a PaletteTone> {
        let mut found: Vec<(usize, &PaletteTone)> =
            roles.iter().filter_map(|r| self.tone(kind, r)).collect();
        found.sort_by_key(|x| x.0);
        found.into_iter().map(|x| x.1).collect()
    }

    fn lit(&self, r: Role) -> String {
        self.table.lit(r)
    }

    /// The palette value of a `classDef` token: a hex colour or `none`.
    fn class_value(&self, name: &str, prop: ClassProp) -> Option<String> {
        self.palette?
            .class_colour(name, prop)
            .map(|c| c.map_or_else(|| String::from("none"), |c| c.to_hex()))
    }

    /// The literal tone and dash an element with `roles` draws with: built-in roles in
    /// rule order, then palette tones in cascade order.
    fn role_paint(&self, kind: Kind, roles: &[String]) -> (Option<String>, Option<String>) {
        let mut tone = None;
        let mut dash = None;
        for b in self
            .builtins
            .iter()
            .filter(|b| b.kind == kind && roles.iter().any(|r| r == b.name))
        {
            if let Some(t) = b.tone {
                tone = Some(self.lit(t));
            }
            if let Some(d) = b.dash {
                dash = Some(String::from(d));
            }
        }
        for t in self.tones_of(kind, roles) {
            if let Some(c) = t.tone {
                tone = Some(c.to_hex());
            }
            if let Some(d) = &t.dash {
                dash = Some(palette_dash(d));
            }
        }
        (tone, dash)
    }
}

/// A palette dash as CSS: `none` for an empty list.
fn palette_dash(d: &[f64]) -> String {
    color::dash_css(d).unwrap_or_else(|| String::from("none"))
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
fn collect_markers(chart: &Flowchart, geom: &Geometry, roles: &Roles, layer: &Layer) -> Markers {
    let mut plain: Vec<MarkerDef> = Vec::new();
    let mut roled: Vec<MarkerDef> = Vec::new();
    let mut seen: BTreeSet<(u8, &[String])> = BTreeSet::new();
    let mut sets: BTreeMap<&[String], usize> = BTreeMap::new();
    for (i, (e, _)) in chart.edges.iter().zip(geom.edges.iter()).enumerate() {
        if e.stroke == Stroke::Invisible {
            continue;
        }
        let r = roles.edge(i);
        for a in [e.arrow_start, e.arrow_end] {
            let Some(kind) = marker_name(a) else {
                continue;
            };
            if !seen.insert((marker_order(a), r)) {
                continue;
            }
            let name = match r {
                [] => String::from(kind),
                [one] => alloc::format!("{}-c-{}", kind, one),
                _ => {
                    let next = sets.len();
                    let k = *sets.entry(r).or_insert(next);
                    alloc::format!("{}-r{}", kind, k)
                }
            };
            let colour = layer
                .role_paint(Kind::Edge, r)
                .0
                .unwrap_or_else(|| layer.lit(Role::Edge));
            let target = if r.is_empty() { &mut plain } else { &mut roled };
            target.push(MarkerDef {
                kind: a,
                roles: r.to_vec(),
                name,
                colour,
            });
        }
    }
    plain.sort_by_key(|m| marker_order(m.kind));
    plain.extend(roled);
    let by_key = plain
        .iter()
        .enumerate()
        .map(|(i, m)| ((marker_order(m.kind), m.roles.clone()), i))
        .collect();
    Markers {
        defs: plain,
        by_key,
    }
}

fn marker_order(a: Arrow) -> u8 {
    match a {
        Arrow::Arrow => 0,
        Arrow::Circle => 1,
        _ => 2,
    }
}

/// The markers in `<defs>` order, and each one's index by (head kind, roles).
struct Markers {
    defs: Vec<MarkerDef>,
    by_key: BTreeMap<(u8, Vec<String>), usize>,
}

impl Markers {
    /// The marker id suffix for an edge head.
    fn find(&self, kind: Arrow, roles: &[String]) -> Option<&str> {
        marker_name(kind)?;
        let i = *self.by_key.get(&(marker_order(kind), roles.to_vec()))?;
        self.defs.get(i).map(|m| m.name.as_str())
    }
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

/// The valid roles of every element, computed once per draw, and the (kind, role) pairs
/// some drawn element carries.
struct Roles {
    node: Vec<Vec<String>>,
    edge: Vec<Vec<String>>,
    cluster: Vec<Vec<String>>,
    used: BTreeSet<(Kind, String)>,
}

impl Roles {
    fn new(chart: &Flowchart) -> Self {
        let node: Vec<Vec<String>> = chart
            .nodes
            .iter()
            .map(|n| valid_classes(&n.classes))
            .collect();
        let edge: Vec<Vec<String>> = chart
            .edges
            .iter()
            .map(|e| valid_classes(&e.classes))
            .collect();
        let cluster: Vec<Vec<String>> = chart
            .subgraphs
            .iter()
            .map(|s| valid_classes(&s.classes))
            .collect();
        let mut used = BTreeSet::new();
        for r in node.iter().flatten() {
            used.insert((Kind::Node, r.clone()));
        }
        for (r, e) in edge.iter().zip(chart.edges.iter()) {
            if e.stroke != Stroke::Invisible {
                for x in r {
                    used.insert((Kind::Edge, x.clone()));
                }
            }
        }
        for r in cluster.iter().flatten() {
            used.insert((Kind::Cluster, r.clone()));
        }
        Roles {
            node,
            edge,
            cluster,
            used,
        }
    }

    fn node(&self, i: usize) -> &[String] {
        self.node.get(i).map_or(&[], Vec::as_slice)
    }

    fn edge(&self, i: usize) -> &[String] {
        self.edge.get(i).map_or(&[], Vec::as_slice)
    }

    fn cluster(&self, i: usize) -> &[String] {
        self.cluster.get(i).map_or(&[], Vec::as_slice)
    }

    /// Whether some drawn element of `kind` carries role `name`.
    fn uses(&self, kind: Kind, name: &str) -> bool {
        self.used.contains(&(kind, String::from(name)))
    }
}

/// The literal paint of one element: what its presentation attributes carry.
struct Paint {
    fill: String,
    stroke: String,
    text: String,
    dash: Option<String>,
}

impl Paint {
    /// Applies the role tone and dash of `roles` (specs/svg-output.md#built-in-roles).
    fn roles(
        &mut self,
        layer: &Layer,
        kind: Kind,
        roles: &[String],
        fill_base: Role,
        fill_pct: u8,
        text_base: Role,
    ) {
        let (tone, dash) = layer.role_paint(kind, roles);
        if let Some(t) = tone {
            if kind != Kind::Edge {
                self.fill = roles::mix_lit(&t, &layer.lit(fill_base), fill_pct);
            }
            self.text = roles::mix_lit(&t, &layer.lit(text_base), TONE_TEXT);
            self.stroke = t;
        }
        if dash.is_some() {
            self.dash = dash;
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

    /// Applies the `classDef` styles of `roles`, in `classDef` order (the rule order); a
    /// palette value for a class token replaces the source literal.
    fn class_defs(&mut self, chart: &Flowchart, roles: &[String], with_fill: bool, layer: &Layer) {
        let sets =
            |c: &Option<crate::model::Color>| c.as_ref().and_then(color::color_css).is_some();
        for cd in &chart.class_defs {
            if !roles.contains(&cd.name) {
                continue;
            }
            self.style(&cd.style, with_fill);
            if with_fill && sets(&cd.style.fill) {
                if let Some(v) = layer.class_value(&cd.name, ClassProp::Fill) {
                    self.fill = v;
                }
            }
            if sets(&cd.style.stroke) {
                if let Some(v) = layer.class_value(&cd.name, ClassProp::Stroke) {
                    self.stroke = v;
                }
            }
            let colour = cd
                .style
                .color
                .as_ref()
                .filter(|c| **c != crate::model::Color::None)
                .and_then(color::color_css);
            if colour.is_some() {
                if let Some(v) = layer.class_value(&cd.name, ClassProp::Color) {
                    self.text = v;
                }
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
        token: Option<&ClassToken>,
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
    fn add_cluster(&mut self, class: &str, style: &Style, token: Option<&ClassToken>) -> bool {
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
    markers: Markers,
    roles: &'a Roles,
    light: Layer<'a>,
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
    attr(out, "class", &node_classes(i, cx));
    attr(out, "data-merlion-id", &node.id);
    let rank = if g.rank > MAX_RANK { MAX_RANK } else { g.rank };
    let _ = write!(out, " data-merlion-rank=\"{}\">", rank);
    let paint = node_paint(cx.chart, node, cx.roles.node(i), &cx.light);
    out.push_str("<path class=\"merlion-shape\"");
    attr(out, "d", &shapes::shape_d(node.shape, g.x, g.y, g.w, g.h));
    attr(out, "fill", &paint.fill);
    attr(out, "stroke", &paint.stroke);
    attr(out, "stroke-width", &stroke_attr(&cx.light.table));
    if let Some(dash) = &paint.dash {
        attr(out, "stroke-dasharray", dash);
    }
    out.push_str("/>");
    label::push_label(
        out,
        &g.label,
        g.x,
        g.y + crate::layout::measure::label_offset(node.shape, g.w, g.h, g.label.height),
        "merlion-label",
        &paint.text,
        &cx.light.lit(Role::NodeDetail),
    );
    out.push_str("</g>");
    if href.is_some() {
        out.push_str("</a>");
    }
    out.push('\n');
}

fn node_paint(chart: &Flowchart, node: &Node, roles: &[String], layer: &Layer) -> Paint {
    let mut p = Paint {
        fill: layer.lit(Role::NodeBg),
        stroke: layer.lit(Role::NodeBorder),
        text: layer.lit(Role::NodeText),
        dash: None,
    };
    p.roles(
        layer,
        Kind::Node,
        roles,
        Role::NodeBg,
        TONE_FILL,
        Role::NodeText,
    );
    p.class_defs(chart, roles, true, layer);
    p.style(&node.style, true);
    p
}

fn edge_paint(chart: &Flowchart, e: &Edge, roles: &[String], layer: &Layer) -> Paint {
    let mut p = Paint {
        fill: String::from("none"),
        stroke: layer.lit(Role::Edge),
        text: layer.lit(Role::Fg),
        dash: (e.stroke == Stroke::Dotted).then(|| String::from("3 3")),
    };
    p.roles(layer, Kind::Edge, roles, Role::Edge, 0, Role::Fg);
    p.class_defs(chart, roles, false, layer);
    p.style(&e.style, false);
    p
}

fn cluster_paint(
    chart: &Flowchart,
    sg: &crate::model::Subgraph,
    roles: &[String],
    layer: &Layer,
) -> Paint {
    let mut p = Paint {
        fill: layer.lit(Role::ClusterBg),
        stroke: layer.lit(Role::ClusterBorder),
        text: layer.lit(Role::Fg),
        dash: None,
    };
    p.roles(
        layer,
        Kind::Cluster,
        roles,
        Role::ClusterBg,
        TONE_CLUSTER_FILL,
        Role::Fg,
    );
    p.class_defs(chart, roles, true, layer);
    p.style(&sg.style, true);
    p
}

fn push_role_classes(c: &mut String, prefix: &str, roles: &[String]) {
    for name in roles {
        c.push(' ');
        c.push_str(prefix);
        c.push_str(name);
    }
}

fn node_classes(i: usize, cx: &Ctx) -> String {
    let mut c = String::from("merlion-node");
    push_role_classes(&mut c, "merlion-c-", cx.roles.node(i));
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
    push_role_classes(&mut class, "merlion-c-", cx.roles.edge(ei));
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
    let paint = edge_paint(cx.chart, e, cx.roles.edge(ei), &cx.light);
    push_edge_path(out, cx, ei, e, &d, &paint);
    push_edge_label(out, g, &paint.text, &cx.light);
    out.push_str("</g>\n");
}

fn push_edge_path(out: &mut String, cx: &Ctx, ei: usize, e: &Edge, d: &str, paint: &Paint) {
    let mut class = String::from("merlion-edge-path");
    let mut width = cx.light.table.stroke_px();
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
    let roles = cx.roles.edge(ei);
    for (a, which) in [(e.arrow_start, "marker-start"), (e.arrow_end, "marker-end")] {
        if let Some(m) = cx.markers.find(a, roles) {
            let _ = write!(out, " {}=\"url(#{}-{})\"", which, cx.id, m);
        }
    }
    out.push_str("/>");
}

fn push_edge_label(out: &mut String, g: &EdgeGeom, text: &str, layer: &Layer) {
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
    attr(out, "fill", &layer.lit(Role::EdgeLabelBg));
    out.push_str("/>");
    label::push_label(
        out,
        &l.label,
        l.x,
        l.y,
        "merlion-edge-text",
        text,
        &layer.lit(Role::NodeDetail),
    );
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
    push_role_classes(&mut class, "merlion-cc-", cx.roles.cluster(si));
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
    let paint = cluster_paint(cx.chart, sg, cx.roles.cluster(si), &cx.light);
    attr(out, "fill", &paint.fill);
    attr(out, "stroke", &paint.stroke);
    attr(out, "stroke-width", &stroke_attr(&cx.light.table));
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
        &cx.light.lit(Role::NodeDetail),
    );
    out.push('\n');
}

fn kind_rules(
    t: &Table,
    kind: Kind,
    name: &str,
    tone: Option<&Tone>,
    dash: Option<&str>,
) -> Vec<RoleRule> {
    match kind {
        Kind::Node => roles::node_rules(t, name, tone, dash),
        Kind::Edge => roles::edge_rules(t, name, tone, dash),
        Kind::Cluster => roles::cluster_rules(t, name, tone, dash),
    }
}

/// Rules of the built-in roles the diagram uses on their element kind, in table order
/// (specs/svg-output.md#built-in-roles).
fn built_in_rules(roles: &Roles, layer: &Layer) -> Vec<RoleRule> {
    let mut out = Vec::new();
    for b in layer.builtins.iter().filter(|b| roles.uses(b.kind, b.name)) {
        let tone = b.tone.map(|r| Tone::of_role(&layer.table, r));
        out.extend(kind_rules(
            &layer.table,
            b.kind,
            b.name,
            tone.as_ref(),
            b.dash,
        ));
    }
    out
}

/// Bytes a palette may add to the embedded style (specs/architecture.md#boundaries),
/// less room for the `@supports` wrappers.
const PALETTE_STYLE_BUDGET: usize = 16 * 1024 - 128;

fn rules_bytes(prefix: usize, rules: &[RoleRule]) -> usize {
    rules
        .iter()
        .map(|r| {
            let one = |b: &str| {
                if b.is_empty() {
                    0
                } else {
                    prefix + r.selector.len() + b.len() + 2
                }
            };
            one(&r.plain) + one(&r.mixed)
        })
        .sum()
}

/// Rules of the palette's role tones for the roles the diagram uses, light and dark,
/// within [`PALETTE_STYLE_BUDGET`]; a role past the budget is left out with `W017`.
fn palette_rules(
    roles: &Roles,
    id: &str,
    light: &Layer,
    dark: Option<&Layer>,
    diags: &mut Diagnostics,
) -> (Vec<RoleRule>, Vec<RoleRule>) {
    // The (kind, role) pairs that some drawn element uses and some table tones, in the
    // light palette's cascade order, then the dark palette's.
    let mut keys: Vec<(Kind, &str)> = Vec::new();
    let mut seen: BTreeSet<(Kind, &str)> = BTreeSet::new();
    for layer in core::iter::once(light).chain(dark) {
        for kind in [Kind::Node, Kind::Edge, Kind::Cluster] {
            let cluster = kind == Kind::Cluster;
            for t in layer.palette.map_or(&[][..], |p| &p.tones[..]) {
                if t.cluster == cluster
                    && layer.tone(kind, &t.name).is_some()
                    && roles.uses(kind, &t.name)
                    && seen.insert((kind, t.name.as_str()))
                {
                    keys.push((kind, t.name.as_str()));
                }
            }
        }
    }
    let rules_for = |layer: &Layer, kind: Kind, name: &str| -> Vec<RoleRule> {
        let Some((_, t)) = layer.tone(kind, name) else {
            return Vec::new();
        };
        let tone = t.tone.map(|c| Tone::literal(c.to_hex()));
        let dash = t.dash.as_deref().map(palette_dash);
        kind_rules(&layer.table, kind, name, tone.as_ref(), dash.as_deref())
    };
    let (light_prefix, dark_prefix) = (id.len() + 2, style::dark_prefix(id).len());
    let mut used = 0usize;
    let (mut lo, mut dk) = (Vec::new(), Vec::new());
    // Emit in the light palette's cascade order so a later tone wins, as in the page CSS.
    for (kind, name) in keys {
        let l = rules_for(light, kind, name);
        let d = dark.map(|d| rules_for(d, kind, name)).unwrap_or_default();
        let bytes = rules_bytes(light_prefix, &l) + rules_bytes(dark_prefix, &d);
        if used + bytes > PALETTE_STYLE_BUDGET {
            diags.emit(
                Severity::Warning,
                "W017",
                Span::default(),
                alloc::format!(
                    "role `{}` left out of the embedded style: the palette's rules exceed 16 KiB",
                    crate::diag::excerpt(name)
                ),
            );
            continue;
        }
        used += bytes;
        lo.extend(l);
        dk.extend(d);
    }
    (lo, dk)
}

/// `I033 ToneMasked` (specs/svg-output.md#precedence): a source literal overrides a
/// palette tone on the same element: a `style` colour, or a `classDef` colour whose
/// token the palette leaves unset.
fn tone_masked(chart: &Flowchart, roles: &Roles, layer: &Layer) -> bool {
    let toned = |kind: Kind, roles: &[String]| {
        roles
            .iter()
            .any(|r| layer.tone(kind, r).is_some_and(|(_, t)| t.tone.is_some()))
    };
    fn colours(st: &Style) -> [(ClassProp, &Option<crate::model::Color>); 3] {
        [
            (ClassProp::Fill, &st.fill),
            (ClassProp::Stroke, &st.stroke),
            (ClassProp::Color, &st.color),
        ]
    }
    let masks = |roles: &[String], own: &Style| {
        colours(own).iter().any(|(_, c)| color::is_fixed(c))
            || chart.class_defs.iter().any(|cd| {
                roles.contains(&cd.name)
                    && colours(&cd.style).iter().any(|(p, c)| {
                        color::is_fixed(c) && layer.class_value(&cd.name, *p).is_none()
                    })
            })
    };
    let node = chart.nodes.iter().enumerate().any(|(i, n)| {
        let r = roles.node(i);
        toned(Kind::Node, r) && masks(r, &n.style)
    });
    let edge = chart.edges.iter().enumerate().any(|(i, e)| {
        let r = roles.edge(i);
        toned(Kind::Edge, r) && masks(r, &e.style)
    });
    let cluster = chart.subgraphs.iter().enumerate().any(|(i, s)| {
        let r = roles.cluster(i);
        toned(Kind::Cluster, r) && masks(r, &s.style)
    });
    node || edge || cluster
}

/// Per-element flags: whether a node, edge or cluster `style` produced rules.
struct StyleClasses {
    node: Vec<bool>,
    edge: Vec<bool>,
    cluster: Vec<bool>,
    fixed_colour: bool,
}

/// Source-style rules for one layer: classDef rules (nodes, then edges that use the
/// class), cluster classDef rules, then node `style`, cluster `style` and `linkStyle`.
fn source_rules(
    chart: &Flowchart,
    roles: &Roles,
    layer: &Layer,
) -> (Vec<SourceRule>, StyleClasses) {
    let mut src = SourceStyles {
        rules: Vec::new(),
        fixed_colour: false,
    };
    let values = |name: &str| {
        [ClassProp::Fill, ClassProp::Stroke, ClassProp::Color].map(|p| layer.class_value(name, p))
    };
    for cd in &chart.class_defs {
        if !color::is_valid_class_name(&cd.name) {
            continue;
        }
        let v = values(&cd.name);
        let token = ClassToken {
            name: &cd.name,
            fill: v[0].as_deref(),
            stroke: v[1].as_deref(),
            color: v[2].as_deref(),
        };
        let class = alloc::format!(".merlion-c-{}", cd.name);
        src.add(&class, &cd.style, ".merlion-shape", true, Some(&token));
        if roles.edge.iter().any(|r| r.contains(&cd.name)) {
            let body = color::shape_decls(&cd.style, false, Some(&token));
            if !body.is_empty() {
                src.rules.push(SourceRule {
                    selector: alloc::format!("{}>.merlion-edge-path", class),
                    body,
                });
            }
        }
    }
    for cd in &chart.class_defs {
        if color::is_valid_class_name(&cd.name) && roles.uses(Kind::Cluster, &cd.name) {
            let v = values(&cd.name);
            let token = ClassToken {
                name: &cd.name,
                fill: v[0].as_deref(),
                stroke: v[1].as_deref(),
                color: v[2].as_deref(),
            };
            src.add_cluster(
                &alloc::format!(".merlion-cc-{}", cd.name),
                &cd.style,
                Some(&token),
            );
        }
    }
    let cluster: Vec<bool> = chart
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
    let node: Vec<bool> = chart
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
    let edge: Vec<bool> = chart
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
    let fixed_colour = src.fixed_colour;
    (
        src.rules,
        StyleClasses {
            node,
            edge,
            cluster,
            fixed_colour,
        },
    )
}

/// Fuel units of the role work a draw does: one per class on a node, edge or cluster,
/// and one per palette tone in each table.
pub fn role_units(chart: &Flowchart, opts: &RenderOptions) -> u64 {
    let classes: usize = chart.nodes.iter().map(|n| n.classes.len()).sum::<usize>()
        + chart.edges.iter().map(|e| e.classes.len()).sum::<usize>()
        + chart
            .subgraphs
            .iter()
            .map(|s| s.classes.len())
            .sum::<usize>();
    let tones = opts.palette.as_ref().map_or(0, |p| {
        p.light.tones.len() + p.dark.as_ref().map_or(0, |d| d.tones.len())
    });
    (classes + tones) as u64
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

    // Literal tables: the built-in defaults, or the palette's light and dark tables.
    let builtins = active_builtins(chart);
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|d| Layer::new(Some(d), builtins));
    let roles = Roles::new(chart);
    let (src_rules, flags) = source_rules(chart, &roles, &light);
    if flags.fixed_colour {
        diags.emit_once(
            Severity::Info,
            "I030",
            Span::default(),
            crate::parse::style::FIXED_COLOUR_MESSAGE,
        );
    }
    if tone_masked(chart, &roles, &light)
        || dark.as_ref().is_some_and(|d| tone_masked(chart, &roles, d))
    {
        diags.emit_once(
            Severity::Info,
            "I033",
            Span::default(),
            "a source colour overrides a stylesheet tone on the same element",
        );
    }
    let (palette_light, palette_dark) = palette_rules(&roles, &id, &light, dark.as_ref(), diags);
    let mut role_rules = built_in_rules(&roles, &light);
    role_rules.extend(palette_light);
    let dark_rules = dark.as_ref().map(|d| {
        let mut r = built_in_rules(&roles, d);
        r.extend(palette_dark);
        (r, source_rules(chart, &roles, d).0)
    });
    let markers = collect_markers(chart, geom, &roles, &light);
    let cx = Ctx {
        chart,
        geom,
        opts,
        id: &id,
        node_class: flags.node,
        edge_class: flags.edge,
        cluster_class: flags.cluster,
        markers,
        roles: &roles,
        light,
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
    let light_layer = style::Layer {
        table: &cx.light.table,
        roles: &role_rules,
        source: &src_rules,
    };
    let dark_layer = match (&dark, &dark_rules) {
        (Some(d), Some((roles, source))) => Some(style::Layer {
            table: &d.table,
            roles,
            source,
        }),
        _ => None,
    };
    out.push_str(&style::build(
        &id,
        opts.font,
        font_size,
        detail_size,
        font_css.as_deref(),
        &light_layer,
        dark_layer.as_ref(),
    ));
    out.push_str("</style>\n");

    push_defs(&mut out, &id, &cx.markers.defs);

    if opts.background {
        out.push_str("<rect class=\"merlion-bg\" x=\"0\" y=\"0\"");
        attr_num(&mut out, "width", w);
        attr_num(&mut out, "height", h);
        attr(&mut out, "fill", &cx.light.lit(Role::Bg));
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
