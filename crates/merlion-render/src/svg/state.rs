//! Drawing state diagrams (specs/state.md#svg-output).
//!
//! States carry the node classes, transitions the edge classes and composite states the
//! cluster classes, because they are the nodes, edges and clusters of the graph the
//! model lowers to. The token reset, the role and `classDef` rules, the markers, the CSS
//! hover layer and the viewer's `interact` module therefore reach them with no rule of
//! their own, and the automatic tones fall out of the shapes the lowering picks
//! (specs/state.md#roles-and-automatic-tones).
//!
//! The body of the drawing is a scaffold: the root element, the accessible name, the
//! embedded style and the layout hint are final; the elements inside
//! `.merlion-diagram` land with the lowering.
//!
//! TODO(owner): draw the clusters, transitions, states and notes in that order
//! (specs/state.md#groups-and-data-attributes), and write the grouped outline
//! (specs/state.md#text-alternative).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::diag::Diagnostics;
use crate::layout::StateLayout;
use crate::model::state::StateMachine;
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};

use super::escape::push_escaped;
use super::roles::BUILT_IN;
use super::style::{self, RoleRule, SourceRule};
use super::{attr, attr_num, safe_root_id, theme, DrawOutput, Layer};

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
    let mut out = String::from("State diagram, ");
    let _ = write!(
        out,
        "{}. {} state{}, {} transition{}.",
        direction_phrase(sm.direction),
        sm.states.len(),
        if sm.states.len() == 1 { "" } else { "s" },
        sm.transitions.len(),
        if sm.transitions.len() == 1 { "" } else { "s" }
    );
    out.push('\n');
    out
}

fn direction_phrase(d: crate::options::Direction) -> &'static str {
    use crate::options::Direction;
    match d {
        Direction::TB => "top to bottom",
        Direction::BT => "bottom to top",
        Direction::LR => "left to right",
        Direction::RL => "right to left",
    }
}

/// A negative or non-finite extent would leave an invalid `viewBox`.
fn nonneg(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

/// Draws a laid-out state diagram. `id` is the validated `id_prefix` or the default
/// hash id.
pub fn draw_state(
    sm: &StateMachine,
    layout: &StateLayout,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let _ = diags;
    let id = safe_root_id(id);
    let outline_text = outline_state(sm);

    let builtins: Vec<_> = BUILT_IN.iter().collect();
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|d| Layer::new(Some(d), builtins));

    let geom = &layout.geometry.graph;
    let (w, h) = (nonneg(geom.width), nonneg(geom.height));
    let mut out = String::with_capacity(2048);

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
    let (no_roles, no_source): (&[RoleRule], &[SourceRule]) = (&[], &[]);
    out.push_str("<style>");
    // TODO(owner): the state rules — the ink fill of the start, end, fork and join
    // marks, the note box and its connector, the region divider
    // (specs/state.md#theme-tokens) — join the base set the way `sequence` does.
    let light_layer = style::Layer {
        table: &light.table,
        roles: no_roles,
        source: no_source,
        sequence: false,
    };
    let dark_layer = dark.as_ref().map(|d| style::Layer {
        table: &d.table,
        roles: no_roles,
        source: no_source,
        sequence: false,
    });
    out.push_str(&style::build(
        &id,
        opts.font,
        font_size,
        None,
        font_css.as_deref(),
        &light_layer,
        dark_layer.as_ref(),
    ));
    out.push_str("</style>\n");

    if opts.background {
        out.push_str("<rect class=\"merlion-bg\" x=\"0\" y=\"0\"");
        attr_num(&mut out, "width", w);
        attr_num(&mut out, "height", h);
        attr(&mut out, "fill", &light.lit(theme::Role::Bg));
        out.push_str("/>\n");
    }

    out.push_str("<g class=\"merlion-diagram\"");
    attr(&mut out, "font-family", theme::font_stack(opts.font));
    attr_num(&mut out, "font-size", font_size);
    out.push_str(">\n");
    out.push_str("</g>\n</svg>\n");

    DrawOutput {
        svg: out,
        outline: outline_text,
    }
}
