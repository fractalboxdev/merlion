//! Drawing state diagrams (specs/state.md#svg-output).
//!
//! States carry the node classes, transitions the edge classes and composite states the
//! cluster classes, because they are the nodes, edges and clusters of the graph the
//! model lowers to. The token reset, the role and `classDef` rules, the markers, the CSS
//! hover layer and the viewer's `interact` module therefore reach them with no rule of
//! their own, and the automatic tones fall out of the shapes the lowering picks
//! (specs/state.md#roles-and-automatic-tones).
//!
//! The body of `.merlion-diagram` paints in the flowchart's order — clusters, edges,
//! nodes — with the notes last, so a note box covers the cluster tint behind it and
//! nothing covers a note. Concurrency regions are clusters of the lowered graph drawn as
//! `.merlion-region`: they have no box and no title, so the dashed divider between two
//! of them is the whole mark.

mod marks;
mod outline;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::diag::{Diagnostics, Severity, Span};
use crate::layout::state::ClusterOrigin;
use crate::layout::{StateLayout, CLUSTER_PAD};
use crate::model::state::{StateKind, StateMachine};
use crate::model::Stroke;
use crate::numfmt::push_num;
use crate::options::{Direction, FontMode, RenderOptions};

use super::escape::push_escaped;
use super::style;
use super::theme::Role;
use super::{attr, attr_num, safe_root_id, stroke_attr, theme, Ctx, DrawOutput, Layer};

/// `v1;{dir};{layer}:{ids}` over the lowered node ids (specs/state.md#svg-output). The
/// hint is the flowchart's, read and written, so `I020`, `I021` and `I022` all reach a
/// state diagram.
pub fn layout_hint(layout: &StateLayout) -> String {
    super::layout_hint(&layout.lowering.graph, &layout.geometry.graph)
}

/// Fuel units of the draw's role work, charged before layout as a flowchart's is: one
/// per class on a state and per palette tone in each table. A transition takes no
/// source style, because the grammar gives it no id to name.
pub fn role_units(sm: &StateMachine, opts: &RenderOptions) -> u64 {
    let classes: usize = sm.states.iter().map(|s| s.classes.len()).sum();
    let tones = opts.palette.as_ref().map_or(0, |p| {
        p.light.tones.len() + p.dark.as_ref().map_or(0, |d| d.tones.len())
    });
    (classes + tones) as u64
}

/// `<title>`: `accTitle`, else the front-matter `title`, else the type name
/// (specs/svg-output.md#text-alternative).
fn title_text(sm: &StateMachine) -> String {
    sm.meta
        .acc_title
        .as_deref()
        .or(sm.meta.title.as_deref())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map_or_else(|| String::from("State diagram"), String::from)
}

/// The plain-text outline (specs/state.md#text-alternative). It reads the state machine,
/// not the lowered graph, so a transition naming a composite state prints that state's
/// name and a generated `[*]` state prints as `start` or `end`.
pub fn outline_state(sm: &StateMachine) -> String {
    outline::outline(sm)
}

/// A negative or non-finite extent would leave an invalid `viewBox`.
fn nonneg(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// The class a pseudo-state adds to the state group, which also carries its ink
/// (specs/state.md#groups-and-data-attributes, #theme-tokens).
fn kind_class(kind: StateKind) -> Option<&'static str> {
    match kind {
        StateKind::Start => Some("merlion-state-start"),
        StateKind::End => Some("merlion-state-end"),
        StateKind::Fork | StateKind::Join => Some("merlion-state-bar"),
        StateKind::Choice => Some("merlion-state-choice"),
        StateKind::Simple | StateKind::Composite => None,
    }
}

/// Whether a kind draws in ink rather than in the node tokens: a start disc, an end ring
/// and a fork or join bar take `--merlion-fg`, so no tone tints them.
fn draws_in_ink(kind: StateKind) -> bool {
    matches!(
        kind,
        StateKind::Start | StateKind::End | StateKind::Fork | StateKind::Join
    )
}

/// The flowchart draw context plus the state machine and the maps back to it.
struct StateCtx<'a> {
    base: Ctx<'a>,
    sm: &'a StateMachine,
    layout: &'a StateLayout,
    stroke: String,
}

impl StateCtx<'_> {
    /// The kind of the state lowered node `i` stands for; `Simple` when the map does not
    /// reach one, which is what a plain box draws.
    fn node_kind(&self, i: usize) -> StateKind {
        self.layout
            .lowering
            .node_of
            .get(i)
            .and_then(|&s| self.sm.states.get(s))
            .map_or(StateKind::Simple, |s| s.kind)
    }

    fn cluster_origin(&self, si: usize) -> Option<ClusterOrigin> {
        self.layout.lowering.cluster_of.get(si).copied()
    }

    fn detail(&self) -> String {
        self.base.light.lit(Role::NodeDetail)
    }
}

// ------------------------------------------------------------------------- states

fn push_state(out: &mut String, cx: &StateCtx, i: usize, diags: &mut Diagnostics) {
    let chart = cx.base.chart;
    let (Some(node), Some(g)) = (chart.nodes.get(i), cx.base.geom.nodes.get(i)) else {
        return;
    };
    let kind = cx.node_kind(i);
    let href = node.link.as_ref().and_then(|l| {
        let h = super::link::safe_href(&l.url);
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

    let mut class = String::from("merlion-node merlion-state-node");
    if let Some(extra) = kind_class(kind) {
        class.push(' ');
        class.push_str(extra);
    }
    super::push_role_classes(&mut class, "merlion-c-", cx.base.roles.node(i));
    if cx.base.roles.node_is_auto(i) {
        class.push(' ');
        class.push_str(super::auto::MARKER);
    }
    if cx.base.node_class.get(i).copied().unwrap_or(false) {
        class.push(' ');
        class.push_str(&super::node_style_class(i));
    }

    out.push_str("<g");
    attr(out, "class", &class);
    attr(out, "data-merlion-id", &node.id);
    attr(out, "data-merlion-kind", kind.as_str());
    let rank = if g.rank > super::MAX_RANK {
        super::MAX_RANK
    } else {
        g.rank
    };
    let _ = write!(out, " data-merlion-rank=\"{}\"", rank);
    let _ = write!(out, " id=\"{}-n{}\">", cx.base.id, i);

    let mut paint = super::node_paint(chart, node, cx.base.roles.node(i), &cx.base.light);
    if draws_in_ink(kind) {
        paint.fill = cx.base.light.lit(Role::Fg);
        paint.stroke = cx.base.light.lit(Role::NodeBorder);
    }
    out.push_str("<path class=\"merlion-shape\"");
    attr(
        out,
        "d",
        &super::shapes::shape_d(node.shape, g.x, g.y, g.w, g.h),
    );
    attr(out, "fill", &paint.fill);
    attr(out, "stroke", &paint.stroke);
    attr(out, "stroke-width", &cx.stroke);
    if let Some(dash) = &paint.dash {
        attr(out, "stroke-dasharray", dash);
    }
    out.push_str("/>");
    super::label::push_label(
        out,
        &g.label,
        g.x,
        g.y + crate::layout::measure::label_offset(node.shape, g.w, g.h, g.label.height),
        "merlion-label",
        &paint.text,
        &cx.detail(),
    );
    out.push_str("</g>");
    if href.is_some() {
        out.push_str("</a>");
    }
    out.push('\n');
}

// -------------------------------------------------------------------- transitions

fn push_transition(out: &mut String, cx: &StateCtx, ei: usize) {
    let chart = cx.base.chart;
    let (Some(e), Some(g)) = (chart.edges.get(ei), cx.base.geom.edges.get(ei)) else {
        return;
    };
    if e.stroke == Stroke::Invisible {
        return;
    }
    let index = cx.layout.lowering.edge_of.get(ei).copied().unwrap_or(ei);
    let mut class = String::from("merlion-edge merlion-transition");
    super::push_role_classes(&mut class, "merlion-c-", cx.base.roles.edge(ei));
    if cx.base.edge_class.get(ei).copied().unwrap_or(false) {
        class.push(' ');
        class.push_str(&super::edge_style_class(ei));
    }
    out.push_str("<g");
    attr(out, "class", &class);
    attr(out, "data-merlion-from", super::node_id(chart, e.from));
    attr(out, "data-merlion-to", super::node_id(chart, e.to));
    if g.back {
        out.push_str(" data-merlion-back=\"true\"");
    }
    if g.wrap {
        out.push_str(" data-merlion-wrap=\"true\"");
    }
    let _ = write!(out, " data-merlion-index=\"{}\"", index);
    let _ = write!(out, " id=\"{}-e{}\">", cx.base.id, index);
    let d = super::path::edge_d(&g.points, cx.base.opts.edge_style);
    let paint = super::edge_paint(chart, e, cx.base.roles.edge(ei), &cx.base.light);
    super::push_edge_path(out, &cx.base, ei, e, &d, &paint);
    super::push_edge_label(out, g, &paint.text, &cx.base.light);
    out.push_str("</g>\n");
}

// ---------------------------------------------------- composites and regions

fn push_composite_open(out: &mut String, cx: &StateCtx, si: usize) {
    let Some(sg) = cx.base.chart.subgraphs.get(si) else {
        return;
    };
    let mut class = String::from("merlion-cluster merlion-composite");
    super::push_role_classes(&mut class, "merlion-cc-", cx.base.roles.cluster(si));
    if cx.base.roles.cluster_is_auto(si) {
        class.push(' ');
        class.push_str(super::auto::MARKER);
    }
    if cx.base.cluster_class.get(si).copied().unwrap_or(false) {
        class.push(' ');
        class.push_str(&super::cluster_style_class(si));
    }
    out.push_str("<g");
    attr(out, "class", &class);
    attr(out, "data-merlion-id", &sg.id);
    out.push_str(">\n");
    let Some(g) = cx.base.geom.clusters.get(si) else {
        return;
    };
    let paint = super::cluster_paint(cx.base.chart, sg, cx.base.roles.cluster(si), &cx.base.light);
    out.push_str("<rect class=\"merlion-cluster-box\"");
    attr_num(out, "x", finite(g.x));
    attr_num(out, "y", finite(g.y));
    attr_num(out, "width", nonneg(g.w));
    attr_num(out, "height", nonneg(g.h));
    attr_num(out, "rx", super::CLUSTER_RADIUS);
    attr(out, "fill", &paint.fill);
    attr(out, "stroke", &paint.stroke);
    attr(out, "stroke-width", &cx.stroke);
    if let Some(dash) = &paint.dash {
        attr(out, "stroke-dasharray", dash);
    }
    out.push_str("/>");
    super::label::push_label(
        out,
        &g.label,
        g.label_x,
        g.label_y,
        "merlion-cluster-title",
        &paint.text,
        &cx.detail(),
    );
    out.push('\n');
}

/// The dashed separator between region `ri` and its predecessor: the composite's inner
/// width in `TB` / `BT` and its inner height in `LR` / `RL`, at the boundary between the
/// two region boxes. `None` for the first region of a composite, and for a region whose
/// composite, predecessor or geometry is missing.
fn region_divider(cx: &StateCtx, si: usize, ri: usize) -> Option<String> {
    let region = cx.sm.regions.get(ri)?;
    if region.index == 0 {
        return None;
    }
    let parent = *cx.layout.lowering.cluster_for.get(region.parent)?;
    let outer = cx.base.geom.clusters.get(parent?)?;
    let prev_ri = cx
        .sm
        .regions
        .iter()
        .position(|r| r.parent == region.parent && r.index + 1 == region.index)?;
    let prev_si = cx
        .layout
        .lowering
        .cluster_of
        .iter()
        .position(|o| *o == ClusterOrigin::Region(prev_ri))?;
    let prev = cx.base.geom.clusters.get(prev_si)?;
    let cur = cx.base.geom.clusters.get(si)?;
    let pad = CLUSTER_PAD;
    // The boundary lies between the two boxes, whichever of them comes first on the axis.
    let between = |pa: f64, pb: f64, ca: f64, cb: f64| {
        if pa + pb / 2.0 <= ca + cb / 2.0 {
            (pa + pb + ca) / 2.0
        } else {
            (ca + cb + pa) / 2.0
        }
    };
    Some(match cx.base.geom.direction {
        Direction::TB | Direction::BT => {
            let y = between(finite(prev.y), nonneg(prev.h), finite(cur.y), nonneg(cur.h));
            marks::line_d(
                (finite(outer.x) + pad, y),
                (finite(outer.x) + nonneg(outer.w) - pad, y),
            )
        }
        Direction::LR | Direction::RL => {
            let x = between(finite(prev.x), nonneg(prev.w), finite(cur.x), nonneg(cur.w));
            marks::line_d(
                (x, finite(outer.y) + pad),
                (x, finite(outer.y) + nonneg(outer.h) - pad),
            )
        }
    })
}

fn push_region_open(out: &mut String, cx: &StateCtx, si: usize, ri: usize) {
    let id = match cx.sm.regions.get(ri) {
        Some(r) => {
            let parent = cx.sm.states.get(r.parent).map_or("", |s| s.id.as_str());
            alloc::format!("{}-r{}", parent, r.index)
        }
        None => cx
            .base
            .chart
            .subgraphs
            .get(si)
            .map_or_else(String::new, |s| s.id.clone()),
    };
    out.push_str("<g class=\"merlion-region\"");
    attr(out, "data-merlion-id", &id);
    let index = cx.sm.regions.get(ri).map_or(ri, |r| r.index);
    let _ = write!(out, " data-merlion-index=\"{}\">", index);
    out.push('\n');
    if let Some(d) = region_divider(cx, si, ri) {
        out.push_str("<path class=\"merlion-region-divider\"");
        attr(out, "d", &d);
        out.push_str(" fill=\"none\"");
        attr(out, "stroke", &cx.base.light.lit(Role::ClusterBorder));
        attr(out, "stroke-width", &cx.stroke);
        let _ = writeln!(out, " stroke-dasharray=\"{}\"/>", marks::DASH);
    }
}

fn push_cluster_open(out: &mut String, cx: &StateCtx, si: usize) {
    match cx.cluster_origin(si) {
        Some(ClusterOrigin::Region(ri)) => push_region_open(out, cx, si, ri),
        _ => push_composite_open(out, cx, si),
    }
}

// -------------------------------------------------------------------------- notes

/// The notes, last of all, so a note box covers the cluster tint behind it and nothing
/// covers a note (specs/state.md#groups-and-data-attributes).
fn push_notes(out: &mut String, cx: &StateCtx) {
    for n in &cx.layout.geometry.notes {
        let state_id = cx.sm.states.get(n.state).map_or("", |s| s.id.as_str());
        out.push_str("<g class=\"merlion-note\"");
        attr(out, "data-merlion-id", state_id);
        attr(out, "data-merlion-placement", n.placement.as_str());
        out.push('>');

        out.push_str("<path class=\"merlion-note-link\"");
        attr(
            out,
            "d",
            &marks::link_d((n.anchor.x, n.anchor.y), n.x, n.y, n.w, n.h),
        );
        out.push_str(" fill=\"none\"");
        attr(out, "stroke", &cx.base.light.lit(Role::Line));
        attr(out, "stroke-width", &cx.stroke);
        let _ = write!(out, " stroke-dasharray=\"{}\"/>", marks::DASH);

        let (w, h) = (nonneg(n.w), nonneg(n.h));
        let (x, y) = (finite(n.x), finite(n.y));
        out.push_str("<rect class=\"merlion-note-box\"");
        attr_num(out, "x", x);
        attr_num(out, "y", y);
        attr_num(out, "width", w);
        attr_num(out, "height", h);
        attr(out, "fill", &cx.base.light.lit(Role::Surface));
        attr(out, "stroke", &cx.base.light.lit(Role::Border));
        attr(out, "stroke-width", &cx.stroke);
        out.push_str("/>");

        super::label::push_label(
            out,
            &n.label,
            x + w / 2.0,
            y + h / 2.0,
            "merlion-label",
            &cx.base.light.lit(Role::NodeText),
            &cx.detail(),
        );
        out.push_str("</g>\n");
    }
}

// --------------------------------------------------------------------------- draw

/// Draws a laid-out state diagram. `id` is the validated `id_prefix` or the default
/// hash id.
pub fn draw_state(
    sm: &StateMachine,
    layout: &StateLayout,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let id = safe_root_id(id);
    let outline_text = outline::outline(sm);
    let chart = &layout.lowering.graph;
    let geom = &layout.geometry.graph;

    // Literal tables and roles: the lowered graph is a flowchart, so every rule of
    // specs/svg-output.md applies to it unchanged.
    let builtins = super::active_builtins(chart);
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|d| Layer::new(Some(d), builtins));
    let roles = super::Roles::new(
        chart,
        super::auto::enabled(chart, opts.auto_tone).then_some(light.builtins.as_slice()),
    );
    let (src_rules, flags) = super::source_rules(chart, &roles, &light);
    if flags.fixed_colour {
        diags.emit_once(
            Severity::Info,
            "I030",
            Span::default(),
            crate::parse::style::FIXED_COLOUR_MESSAGE,
        );
    }
    if super::tone_masked(chart, &roles, &light)
        || dark
            .as_ref()
            .is_some_and(|d| super::tone_masked(chart, &roles, d))
    {
        diags.emit_once(
            Severity::Info,
            "I033",
            Span::default(),
            "a source colour overrides a stylesheet tone on the same element",
        );
    }
    let (palette_light, palette_dark) =
        super::palette_rules(&roles, &id, &light, dark.as_ref(), diags);
    // The state marks come after the built-in and palette rules, so an automatic tone
    // never tints a start, end, fork or join mark, and before the source styles, so a
    // written `classDef` still wins (specs/state.md#theme-tokens).
    let mut role_rules = super::built_in_rules(&roles, &light);
    role_rules.extend(palette_light);
    role_rules.extend(marks::state_rules(&light.table));
    let dark_rules = dark.as_ref().map(|d| {
        let mut r = super::built_in_rules(&roles, d);
        r.extend(palette_dark);
        r.extend(marks::state_rules(&d.table));
        (r, super::source_rules(chart, &roles, d).0)
    });
    let markers = super::collect_markers(chart, geom, &roles, &light);

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
    out.push_str(" class=\"merlion merlion-state\"");
    attr(&mut out, "data-merlion-version", crate::VERSION);
    attr(&mut out, "data-merlion-layout", &layout_hint(layout));
    out.push_str(">\n");

    let _ = write!(out, "<title id=\"{}-title\">", id);
    push_escaped(&mut out, &title_text(sm));
    out.push_str("</title>\n");
    let _ = write!(out, "<desc id=\"{}-desc\">", id);
    let desc = sm
        .meta
        .acc_descr
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .unwrap_or(&outline_text);
    push_escaped(&mut out, desc);
    out.push_str("</desc>\n");

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
    let detail_size = geom
        .nodes
        .iter()
        .flat_map(|n| n.label.lines.iter())
        .find(|l| l.detail)
        .map(|l| l.size);
    out.push_str("<style>");
    let light_layer = style::Layer {
        table: &light.table,
        roles: &role_rules,
        source: &src_rules,
        sequence: false,
    };
    let dark_layer = match (&dark, &dark_rules) {
        (Some(d), Some((rules, source))) => Some(style::Layer {
            table: &d.table,
            roles: rules,
            source,
            sequence: false,
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

    super::push_defs(&mut out, &id, &markers.defs);

    if opts.background {
        out.push_str("<rect class=\"merlion-bg\" x=\"0\" y=\"0\"");
        attr_num(&mut out, "width", w);
        attr_num(&mut out, "height", h);
        attr(&mut out, "fill", &light.lit(Role::Bg));
        out.push_str("/>\n");
    }

    out.push_str("<g class=\"merlion-diagram\"");
    attr(&mut out, "font-family", theme::font_stack(opts.font));
    attr_num(&mut out, "font-size", font_size);
    out.push_str(">\n");

    let stroke = stroke_attr(&light.table);
    let cx = StateCtx {
        base: Ctx {
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
        },
        sm,
        layout,
        stroke,
    };

    let parents = super::tree::effective_parents(chart);
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
            super::tree::common_cluster(&parents, a, b)
        })
        .collect();
    for item in super::tree::draw_order(&parents, &node_home, &edge_home) {
        match item {
            super::tree::Item::Open(s) => push_cluster_open(&mut out, &cx, s),
            super::tree::Item::Close(s) => {
                if s < n_sg {
                    out.push_str("</g>\n");
                }
            }
            super::tree::Item::Edge(e) => push_transition(&mut out, &cx, e),
            super::tree::Item::Node(i) => push_state(&mut out, &cx, i, diags),
        }
    }
    push_notes(&mut out, &cx);

    out.push_str("</g>\n</svg>\n");

    DrawOutput {
        svg: out,
        outline: outline_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pseudo_state_has_a_class_and_the_labelled_kinds_have_none() {
        for k in [
            StateKind::Start,
            StateKind::End,
            StateKind::Fork,
            StateKind::Join,
            StateKind::Choice,
        ] {
            assert!(kind_class(k).is_some(), "{k:?}");
        }
        assert_eq!(kind_class(StateKind::Simple), None);
        assert_eq!(kind_class(StateKind::Composite), None);
        // The choice diamond takes the node tokens and the flowchart's `warn` tone; only
        // the four bare marks draw in ink (specs/state.md#theme-tokens).
        assert!(!draws_in_ink(StateKind::Choice));
        assert!(!draws_in_ink(StateKind::Simple));
        for k in [
            StateKind::Start,
            StateKind::End,
            StateKind::Fork,
            StateKind::Join,
        ] {
            assert!(draws_in_ink(k), "{k:?}");
        }
    }

    #[test]
    fn a_title_falls_back_to_the_type_name() {
        let mut sm = StateMachine::default();
        assert_eq!(title_text(&sm), "State diagram");
        sm.meta.title = Some(String::from("  "));
        assert_eq!(title_text(&sm), "State diagram");
        sm.meta.title = Some(String::from("Lamp"));
        assert_eq!(title_text(&sm), "Lamp");
        sm.meta.acc_title = Some(String::from("Lamp states"));
        assert_eq!(title_text(&sm), "Lamp states");
    }
}
