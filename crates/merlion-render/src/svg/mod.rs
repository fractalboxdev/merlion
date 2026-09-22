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
mod escape;
mod label;
mod link;
mod outline;
mod path;
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
use style::SourceRule;
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

/// Markers used by the drawn edges, in a fixed order.
#[derive(Default)]
struct MarkerUse {
    arrow: bool,
    circle: bool,
    cross: bool,
}

impl MarkerUse {
    fn note(&mut self, a: Arrow) {
        match a {
            Arrow::Arrow => self.arrow = true,
            Arrow::Circle => self.circle = true,
            Arrow::Cross => self.cross = true,
            Arrow::None => {}
        }
    }
}

fn marker_name(a: Arrow) -> Option<&'static str> {
    match a {
        Arrow::Arrow => Some("arrow"),
        Arrow::Circle => Some("circle"),
        Arrow::Cross => Some("cross"),
        Arrow::None => None,
    }
}

/// `<defs>` with each used marker once. Markers are 10 × 10 user units with the tip at
/// x = 9, oriented with `auto-start-reverse` so the same marker serves `marker-start`.
fn push_defs(out: &mut String, id: &str, used: &MarkerUse) {
    if !(used.arrow || used.circle || used.cross) {
        return;
    }
    let line = Role::Edge.default_value();
    out.push_str("<defs>");
    let head = |out: &mut String, name: &str| {
        let _ = write!(
            out,
            "<marker id=\"{}-{}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"10\" \
             markerHeight=\"10\" markerUnits=\"userSpaceOnUse\" orient=\"auto-start-reverse\">",
            id, name
        );
    };
    if used.arrow {
        head(out, "arrow");
        let _ = write!(
            out,
            "<path class=\"merlion-marker-fill\" d=\"M0 0L10 5L0 10Z\" fill=\"{}\" stroke=\"none\"/></marker>",
            line
        );
    }
    if used.circle {
        head(out, "circle");
        let _ = write!(
            out,
            "<path class=\"merlion-marker-fill\" d=\"M1 5A4 4 0 1 1 9 5A4 4 0 1 1 1 5Z\" fill=\"{}\" stroke=\"none\"/></marker>",
            line
        );
    }
    if used.cross {
        head(out, "cross");
        let _ = write!(
            out,
            "<path class=\"merlion-marker-stroke\" d=\"M2 1L9 9M2 9L9 1\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\"/></marker>",
            line
        );
    }
    out.push_str("</defs>");
}

/// Collects id-scoped source-style rules and the classes that key them.
struct SourceStyles {
    rules: Vec<SourceRule>,
    fixed_colour: bool,
}

impl SourceStyles {
    fn add(&mut self, class: &str, style: &Style, shape_sel: &str, with_fill: bool) -> bool {
        self.fixed_colour |= color::is_fixed(&style.fill)
            || color::is_fixed(&style.stroke)
            || color::is_fixed(&style.color);
        let mut any = false;
        for (sel, body) in [
            (String::from(class), color::group_decls(style)),
            (
                alloc::format!("{}>{}", class, shape_sel),
                color::shape_decls(style, with_fill),
            ),
            (alloc::format!("{} text", class), color::text_decls(style)),
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
    fn add_cluster(&mut self, class: &str, style: &Style) -> bool {
        self.fixed_colour |= color::is_fixed(&style.fill)
            || color::is_fixed(&style.stroke)
            || color::is_fixed(&style.color);
        let mut any = false;
        for (sel, body) in [
            (
                alloc::format!("{}>.merlion-cluster-box", class),
                color::shape_decls(style, true),
            ),
            (
                alloc::format!("{}>.merlion-cluster-title", class),
                color::text_decls(style),
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
    cluster_defs: Vec<String>,
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
    out.push_str("<path class=\"merlion-shape\"");
    attr(out, "d", &shapes::shape_d(node.shape, g.x, g.y, g.w, g.h));
    attr(out, "fill", Role::NodeBg.default_value());
    attr(out, "stroke", Role::NodeBorder.default_value());
    attr(out, "stroke-width", &stroke_attr());
    out.push_str("/>");
    label::push_label(
        out,
        &g.label,
        g.x,
        g.y,
        "merlion-label",
        Role::NodeText.default_value(),
    );
    out.push_str("</g>");
    if href.is_some() {
        out.push_str("</a>");
    }
    out.push('\n');
}

fn node_classes(node: &Node, i: usize, cx: &Ctx) -> String {
    let mut c = String::from("merlion-node");
    let mut seen: Vec<&str> = Vec::new();
    for name in &node.classes {
        if color::is_valid_class_name(name) && !seen.contains(&name.as_str()) {
            seen.push(name);
            c.push_str(" merlion-c-");
            c.push_str(name);
        }
    }
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
    push_edge_path(out, cx, e, &d);
    push_edge_label(out, g);
    out.push_str("</g>\n");
}

fn push_edge_path(out: &mut String, cx: &Ctx, e: &Edge, d: &str) {
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
    attr(out, "stroke", Role::Edge.default_value());
    attr_num(out, "stroke-width", width);
    if e.stroke == Stroke::Dotted {
        out.push_str(" stroke-dasharray=\"3 3\"");
    }
    for (a, which) in [(e.arrow_start, "marker-start"), (e.arrow_end, "marker-end")] {
        if let Some(m) = marker_name(a) {
            let _ = write!(out, " {}=\"url(#{}-{})\"", which, cx.id, m);
        }
    }
    out.push_str("/>");
}

fn push_edge_label(out: &mut String, g: &EdgeGeom) {
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
    label::push_label(
        out,
        &l.label,
        l.x,
        l.y,
        "merlion-edge-text",
        Role::Fg.default_value(),
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
    let mut seen: Vec<&str> = Vec::new();
    for name in &sg.classes {
        if cx.cluster_defs.contains(name) && !seen.contains(&name.as_str()) {
            seen.push(name);
            class.push_str(" merlion-cc-");
            class.push_str(name);
        }
    }
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
    attr(out, "fill", Role::ClusterBg.default_value());
    attr(out, "stroke", Role::ClusterBorder.default_value());
    attr(out, "stroke-width", &stroke_attr());
    out.push_str("/>");
    label::push_label(
        out,
        &g.label,
        g.label_x,
        g.label_y,
        "merlion-cluster-title",
        Role::Fg.default_value(),
    );
    out.push('\n');
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

    // Source styles: classDef rules, then node `style`, then `linkStyle`.
    let mut src = SourceStyles {
        rules: Vec::new(),
        fixed_colour: false,
    };
    for cd in &chart.class_defs {
        if color::is_valid_class_name(&cd.name) {
            src.add(
                &alloc::format!(".merlion-c-{}", cd.name),
                &cd.style,
                ".merlion-shape",
                true,
            );
        }
    }
    // Clusters: classDef rules for the names some cluster uses, then `style`.
    let mut cluster_defs: Vec<String> = Vec::new();
    for cd in &chart.class_defs {
        if color::is_valid_class_name(&cd.name)
            && !cluster_defs.contains(&cd.name)
            && chart.subgraphs.iter().any(|s| s.classes.contains(&cd.name))
            && src.add_cluster(&alloc::format!(".merlion-cc-{}", cd.name), &cd.style)
        {
            cluster_defs.push(cd.name.clone());
        }
    }
    let cluster_class: Vec<bool> = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            !s.style.is_empty()
                && src.add_cluster(&alloc::format!(".{}", cluster_style_class(i)), &s.style)
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
                )
        })
        .collect();
    if src.fixed_colour {
        diags.emit_once(
            Severity::Info,
            "I030",
            Span::default(),
            "source sets a colour that stays fixed in every theme",
        );
    }

    let cx = Ctx {
        chart,
        geom,
        opts,
        id: &id,
        node_class,
        edge_class,
        cluster_class,
        cluster_defs,
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
    out.push_str("<style>");
    // The style text contains no `<` or `&`: ids, class names and colours are validated.
    out.push_str(&style::build(
        &id,
        opts.font,
        font_size,
        font_css.as_deref(),
        &src.rules,
    ));
    out.push_str("</style>\n");

    let mut used = MarkerUse::default();
    for (e, _) in chart.edges.iter().zip(geom.edges.iter()) {
        if e.stroke != Stroke::Invisible {
            used.note(e.arrow_start);
            used.note(e.arrow_end);
        }
    }
    push_defs(&mut out, &id, &used);

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
