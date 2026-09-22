//! Drawing sequence diagrams (specs/sequence.md#svg-output).
//!
//! Participants carry the node classes and messages the edge classes, so the token
//! reset, the role rules, the markers, the CSS hover layer and the viewer's `interact`
//! module reach them with no sequence-specific rule of their own.
//!
//! The body of the drawing is a scaffold: the root element, the accessible name, the
//! embedded style and the layout hint are final; the elements inside
//! `.merlion-diagram` land with the layout solver.
//!
//! TODO(owner): draw the boxes, lifelines, fragments, activations, notes and messages
//! in that order (specs/sequence.md#groups-and-data-attributes), and write the grouped,
//! numbered outline (specs/sequence.md#text-alternative).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::diag::Diagnostics;
use crate::geometry::sequence::SequenceGeometry;
use crate::ids;
use crate::model::sequence::Sequence;
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};

use super::escape::push_escaped;
use super::roles::BUILT_IN;
use super::style::{self, RoleRule, SourceRule};
use super::{attr, attr_num, safe_root_id, theme, DrawOutput, Layer};

/// `v1;SEQ;0:{participants}` (specs/sequence.md#layout-hint). Participant order is the
/// source's, so the hint is written and never read; it keeps the id hash defined over
/// the drawn layout, which makes a re-render hinted with its own SVG reproduce it.
pub fn layout_hint(seq: &Sequence) -> String {
    let mut s = String::from("v1;SEQ;0:");
    for (i, p) in seq.participants.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&ids::encode_id(&p.id));
    }
    s
}

/// `<title>`: `accTitle`, else the front-matter `title`, else the type name
/// (specs/svg-output.md#text-alternative).
fn title_text(seq: &Sequence) -> String {
    seq.meta
        .acc_title
        .as_deref()
        .or(seq.meta.title.as_deref())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map_or_else(|| String::from("Sequence diagram"), String::from)
}

/// The plain-text outline (specs/sequence.md#text-alternative).
pub fn outline_sequence(seq: &Sequence) -> String {
    let mut out = String::from("Sequence diagram. ");
    let _ = write!(
        out,
        "{} participant{}, {} message{}.",
        seq.participants.len(),
        if seq.participants.len() == 1 { "" } else { "s" },
        seq.messages,
        if seq.messages == 1 { "" } else { "s" }
    );
    out.push('\n');
    out
}

/// A negative or non-finite extent would leave an invalid `viewBox`.
fn nonneg(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

/// Draws a laid-out sequence diagram. `id` is the validated `id_prefix` or the default
/// hash id.
pub fn draw_sequence(
    seq: &Sequence,
    geom: &SequenceGeometry,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let _ = diags;
    let id = safe_root_id(id);
    let outline_text = outline_sequence(seq);

    let builtins: Vec<_> = BUILT_IN.iter().collect();
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|d| Layer::new(Some(d), builtins));

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
    out.push_str(" class=\"merlion merlion-sequence\"");
    attr(&mut out, "data-merlion-version", crate::VERSION);
    attr(&mut out, "data-merlion-layout", &layout_hint(seq));
    out.push_str(">\n");

    let _ = write!(out, "<title id=\"{}-title\">", id);
    push_escaped(&mut out, &title_text(seq));
    out.push_str("</title>\n");
    let _ = write!(out, "<desc id=\"{}-desc\">", id);
    let desc = seq
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
    let light_layer = style::Layer {
        table: &light.table,
        roles: no_roles,
        source: no_source,
    };
    let dark_layer = dark.as_ref().map(|d| style::Layer {
        table: &d.table,
        roles: no_roles,
        source: no_source,
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
