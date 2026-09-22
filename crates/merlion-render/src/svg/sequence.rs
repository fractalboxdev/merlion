//! Drawing sequence diagrams (specs/sequence.md#svg-output).
//!
//! Participants carry the node classes and messages the edge classes, so the token
//! reset, the role rules, the markers, the CSS hover layer and the viewer's `interact`
//! module reach them with no sequence-specific rule of their own.
//!
//! The body of `.merlion-diagram` paints in the order of
//! specs/sequence.md#groups-and-data-attributes — boxes, lifelines, fragments,
//! activations, notes, messages — so a message's label chip covers the fragment box
//! behind it. Fragments are flat sibling groups rather than containers: the geometry
//! gives each one an absolute box, and keeping them flat keeps a fragment's tone off the
//! activations, notes and messages drawn over it.

mod marks;
mod outline;
mod walk;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::diag::{Diagnostics, Severity, Span};
use crate::geometry::sequence::{Rect, SequenceGeometry, ACTIVATION_W, NUMBER_R};
use crate::ids;
use crate::layout::sequence::{ACTOR_FIGURE, SELF_WIDTH};
use crate::model::sequence::{
    Central, FragmentKind, Head, Message, MessageLine, ParticipantKind, Sequence,
};
use crate::model::{Color, Style};
use crate::numfmt::push_num;
use crate::options::{FontMode, RenderOptions};

use super::escape::push_escaped;
use super::roles::{Kind, Tone, BUILT_IN, TONE_CLUSTER_FILL, TONE_FILL, TONE_TEXT};
use super::style::{self, RoleRule, SourceRule};
use super::theme::Role;
use super::{
    attr, attr_num, color, label as label_svg, roles as role_css, safe_root_id, stroke_attr, theme,
    DrawOutput, Layer, Paint, Roles,
};

use walk::{walk, Ev, Step};

/// Radius of the dot a central connection ends in: a 4 px dot
/// (specs/sequence.md#messages).
const CENTRAL_R: f64 = 2.0;
/// The autonumber badge's text, as a fraction of the diagram's font size.
const NUMBER_SCALE: f64 = 0.75;

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
    outline::outline(seq)
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

// --------------------------------------------------------------- roles and tones

/// The automatic tone of a participant kind (specs/sequence.md#roles-and-automatic-tones).
fn participant_role(kind: ParticipantKind) -> Option<&'static str> {
    match kind {
        ParticipantKind::Actor => Some("accent"),
        ParticipantKind::Database | ParticipantKind::Collections | ParticipantKind::Queue => {
            Some("store")
        }
        _ => None,
    }
}

/// The six built-in tone roles, which sequences extend to clusters at the cluster mix
/// (specs/svg-output.md#built-in-roles).
const CLUSTER_TONES: [(&str, Role); 6] = [
    ("accent", Role::Accent),
    ("ok", Role::Ok),
    ("warn", Role::Warn),
    ("danger", Role::Danger),
    ("muted", Role::Muted),
    ("store", Role::Store),
];

/// The roles of every drawn element. Sequences carry no source styles, so the only
/// roles are the automatic ones; `auto_tone` off leaves every list empty.
///
/// The cluster index space is the fragments in pre-order, then the boxes.
fn sequence_roles(seq: &Sequence, frags: &[FragmentKind], on: bool) -> Roles {
    let one = |name: Option<&'static str>| -> (Vec<String>, bool) {
        match name.filter(|_| on) {
            Some(n) => (alloc::vec![String::from(n)], true),
            None => (Vec::new(), false),
        }
    };
    let (mut node, mut node_auto) = (Vec::new(), Vec::new());
    for p in &seq.participants {
        let (r, a) = one(participant_role(p.kind));
        node.push(r);
        node_auto.push(a);
    }
    let (mut cluster, mut cluster_auto) = (Vec::new(), Vec::new());
    for k in frags {
        let (r, a) = one(k.auto_role());
        cluster.push(r);
        cluster_auto.push(a);
    }
    for _ in &seq.boxes {
        cluster.push(Vec::new());
        cluster_auto.push(false);
    }
    let edge = alloc::vec![Vec::new(); seq.messages as usize];
    let mut used = alloc::collections::BTreeSet::new();
    for r in node.iter().flatten() {
        used.insert((Kind::Node, r.clone()));
    }
    for r in cluster.iter().flatten() {
        used.insert((Kind::Cluster, r.clone()));
    }
    Roles {
        node,
        edge,
        cluster,
        node_auto,
        cluster_auto,
        used,
    }
}

/// Cluster rules for the tone roles a fragment takes. `BUILT_IN` lists those six roles
/// on nodes only, so a flowchart's bytes are unchanged and a fragment still tones.
fn cluster_tone_rules(roles: &Roles, layer: &Layer) -> Vec<RoleRule> {
    let mut out = Vec::new();
    for (name, role) in CLUSTER_TONES {
        if !roles.uses(Kind::Cluster, name) {
            continue;
        }
        let tone = Tone::of_role(&layer.table, role);
        out.extend(role_css::cluster_rules(
            &layer.table,
            name,
            Some(&tone),
            None,
        ));
    }
    out
}

// ------------------------------------------------------------------ source colours

fn box_style_class(i: usize) -> String {
    alloc::format!("merlion-bs-{}", i)
}

fn fragment_style_class(i: usize) -> String {
    alloc::format!("merlion-fs-{}", i)
}

/// The id-scoped rules of the `box` and `rect` colours, which are source literals
/// (specs/sequence.md#boxes, #fragments). Returns the rules and, per box and per
/// fragment, whether it carries the generated class.
fn source_rules(seq: &Sequence, frags: &[FragmentKind]) -> (Vec<SourceRule>, Vec<bool>, Vec<bool>) {
    let mut rules = Vec::new();
    let mut add = |class: String, c: &Color| -> bool {
        let style = Style {
            fill: Some(*c),
            ..Style::default()
        };
        let body = color::shape_decls(&style, true, None);
        if body.is_empty() {
            return false;
        }
        rules.push(SourceRule {
            selector: alloc::format!(".{}>.merlion-cluster-box", class),
            body,
        });
        true
    };
    let boxes: Vec<bool> = seq
        .boxes
        .iter()
        .enumerate()
        .map(|(i, b)| match &b.color {
            Some(c) => add(box_style_class(i), c),
            None => false,
        })
        .collect();
    let fragments: Vec<bool> = frags
        .iter()
        .enumerate()
        .map(|(i, k)| match k {
            FragmentKind::Rect(Some(c)) => add(fragment_style_class(i), c),
            _ => false,
        })
        .collect();
    (rules, boxes, fragments)
}

// ------------------------------------------------------------------------ painting

fn participant_paint(roles: &[String], layer: &Layer) -> Paint {
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
    p
}

fn cluster_paint(roles: &[String], fixed: Option<&Color>, layer: &Layer) -> Paint {
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
    // The six tone roles reach clusters through [`cluster_tone_rules`], which `BUILT_IN`
    // does not list; their literals belong on the presentation attributes too, so a
    // CSS-less renderer draws the tone (specs/svg-output.md#theming).
    for (name, role) in CLUSTER_TONES {
        if !roles.iter().any(|r| r == name) {
            continue;
        }
        let tone = layer.lit(role);
        p.fill = role_css::mix_lit(&tone, &layer.lit(Role::ClusterBg), TONE_CLUSTER_FILL);
        p.text = role_css::mix_lit(&tone, &layer.lit(Role::Fg), TONE_TEXT);
        p.stroke = tone;
    }
    if let Some(v) = fixed.and_then(color::color_css) {
        p.fill = v;
    }
    p
}

// -------------------------------------------------------------------- primitives

fn push_rect(out: &mut String, class: &str, r: Rect, rx: Option<f64>) {
    out.push_str("<rect");
    attr(out, "class", class);
    attr_num(out, "x", finite(r.x));
    attr_num(out, "y", finite(r.y));
    attr_num(out, "width", nonneg(r.w));
    attr_num(out, "height", nonneg(r.h));
    if let Some(rx) = rx {
        attr_num(out, "rx", rx);
    }
}

fn push_fill_stroke(out: &mut String, p: &Paint, width: &str) {
    attr(out, "fill", &p.fill);
    attr(out, "stroke", &p.stroke);
    attr(out, "stroke-width", width);
    if let Some(d) = &p.dash {
        attr(out, "stroke-dasharray", d);
    }
}

/// `M x1 y1L x2 y2`, the straight path of a message between two columns.
fn line_d(a: (f64, f64), b: (f64, f64)) -> String {
    let mut d = String::from("M");
    push_num(&mut d, finite(a.0));
    d.push(' ');
    push_num(&mut d, finite(a.1));
    d.push('L');
    push_num(&mut d, finite(b.0));
    d.push(' ');
    push_num(&mut d, finite(b.1));
    d
}

/// The bracket of a self-message: out to `SELF_WIDTH` right of the further of its two
/// ends, down to the returning row, and back (specs/sequence.md#rows).
fn self_d(a: (f64, f64), b: (f64, f64)) -> String {
    let (ax, ay, bx, by) = (finite(a.0), finite(a.1), finite(b.0), finite(b.1));
    let right = if ax > bx { ax } else { bx } + SELF_WIDTH;
    let mut d = String::from("M");
    push_num(&mut d, ax);
    d.push(' ');
    push_num(&mut d, ay);
    d.push('H');
    push_num(&mut d, right);
    d.push('V');
    push_num(&mut d, by);
    d.push('H');
    push_num(&mut d, bx);
    d
}

// ------------------------------------------------------------------------- defs

/// The arrowheads the drawn messages use, in [`marks::HEADS`] order.
fn used_heads(steps: &[Step<'_>]) -> Vec<Head> {
    let mut seen = [false; marks::HEADS.len()];
    for s in steps {
        let Ev::Message(m) = s.ev else { continue };
        for h in [m.head, m.tail] {
            if let Some(i) = marks::HEADS.iter().position(|k| *k == h) {
                seen[i] = true;
            }
        }
    }
    marks::HEADS
        .iter()
        .zip(seen.iter())
        .filter(|(_, on)| **on)
        .map(|(h, _)| *h)
        .collect()
}

fn push_defs(out: &mut String, id: &str, heads: &[Head], layer: &Layer) {
    if heads.is_empty() {
        return;
    }
    let colour = layer.lit(Role::Edge);
    out.push_str("<defs>");
    for h in heads {
        let Some(m) = marks::mark(*h) else { continue };
        let _ = write!(out, "<marker id=\"{}-{}\"", id, m.name);
        out.push_str(
            " viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"10\" \
             markerHeight=\"10\" markerUnits=\"userSpaceOnUse\" orient=\"auto-start-reverse\">",
        );
        match m.ink {
            marks::Ink::Fill => {
                out.push_str("<path class=\"merlion-marker-fill\"");
                attr(out, "d", m.d);
                attr(out, "fill", &colour);
                out.push_str(" stroke=\"none\"/>");
            }
            marks::Ink::Stroke => {
                out.push_str("<path class=\"merlion-marker-stroke\"");
                attr(out, "d", m.d);
                out.push_str(" fill=\"none\"");
                attr(out, "stroke", &colour);
                out.push_str(" stroke-width=\"1.5\"/>");
            }
        }
        out.push_str("</marker>");
    }
    out.push_str("</defs>");
}

// ------------------------------------------------------------------------- body

struct Ctx<'a> {
    seq: &'a Sequence,
    geom: &'a SequenceGeometry,
    id: &'a str,
    roles: &'a Roles,
    light: &'a Layer<'a>,
    /// `.merlion-bs-{i}` / `.merlion-fs-{i}` flags from the source colours.
    box_class: Vec<bool>,
    frag_class: Vec<bool>,
    font_size: f64,
    stroke: String,
}

impl Ctx<'_> {
    fn participant_id(&self, i: usize) -> &str {
        self.seq
            .participants
            .get(i)
            .map(|p| p.id.as_str())
            .unwrap_or("")
    }

    fn detail(&self) -> String {
        self.light.lit(Role::NodeDetail)
    }
}

fn push_boxes(out: &mut String, cx: &Ctx) {
    for (i, g) in cx.geom.boxes.iter().enumerate() {
        let colour = cx.seq.boxes.get(i).and_then(|b| b.color.as_ref());
        let mut class = String::from("merlion-cluster merlion-box");
        if cx.box_class.get(i).copied().unwrap_or(false) {
            class.push(' ');
            class.push_str(&box_style_class(i));
        }
        out.push_str("<g");
        attr(out, "class", &class);
        let _ = write!(out, " data-merlion-index=\"{}\">", i);
        let paint = cluster_paint(&[], colour, cx.light);
        push_rect(
            out,
            "merlion-cluster-box",
            g.box_,
            Some(super::CLUSTER_RADIUS),
        );
        push_fill_stroke(out, &paint, &cx.stroke);
        out.push_str("/>");
        label_svg::push_label(
            out,
            &g.label,
            g.label_x,
            g.label_y,
            "merlion-cluster-title",
            &paint.text,
            &cx.detail(),
        );
        out.push_str("</g>\n");
    }
}

fn push_participants(out: &mut String, cx: &Ctx) {
    for (i, g) in cx.geom.participants.iter().enumerate() {
        let Some(p) = cx.seq.participants.get(i) else {
            continue;
        };
        let mut class = String::from("merlion-node merlion-participant");
        super::push_role_classes(&mut class, "merlion-c-", cx.roles.node(i));
        if cx.roles.node_is_auto(i) {
            class.push(' ');
            class.push_str(super::auto::MARKER);
        }
        out.push_str("<g");
        attr(out, "class", &class);
        attr(out, "data-merlion-id", &p.id);
        attr(out, "data-merlion-kind", p.kind.as_str());
        out.push_str(" data-merlion-rank=\"0\"");
        let _ = write!(out, " id=\"{}-n{}\">", cx.id, i);

        let paint = participant_paint(cx.roles.node(i), cx.light);
        // The lifeline is the group's first child, so every box paints over it.
        out.push_str("<line class=\"merlion-lifeline\"");
        attr_num(out, "x1", finite(g.x));
        attr_num(out, "y1", finite(g.lifeline.0));
        attr_num(out, "x2", finite(g.x));
        attr_num(out, "y2", finite(g.lifeline.1));
        out.push_str(" fill=\"none\"");
        attr(out, "stroke", &cx.light.lit(Role::Line));
        attr(out, "stroke-width", &cx.stroke);
        out.push_str(" stroke-dasharray=\"4 4\"/>");

        for (which, r) in [Some(g.head), g.foot]
            .into_iter()
            .enumerate()
            .filter_map(|(k, r)| r.map(|r| (k, r)))
        {
            out.push_str("<path");
            attr(
                out,
                "class",
                if which == 0 {
                    "merlion-shape"
                } else {
                    "merlion-shape merlion-participant-foot"
                },
            );
            attr(out, "d", &marks::symbol_d(p.kind, r, ACTOR_FIGURE));
            push_fill_stroke(out, &paint, &cx.stroke);
            out.push_str("/>");
            let (lx, ly) = marks::label_centre(p.kind, r, ACTOR_FIGURE);
            label_svg::push_label(
                out,
                &g.label,
                lx,
                ly,
                "merlion-label",
                &paint.text,
                &cx.detail(),
            );
        }
        if p.destroyed_by.is_some() {
            out.push_str("<path class=\"merlion-destroy\"");
            attr(
                out,
                "d",
                &marks::destroy_d(finite(g.x), finite(g.lifeline.1), 8.0),
            );
            out.push_str(" fill=\"none\"");
            attr(out, "stroke", &cx.light.lit(Role::Line));
            attr(out, "stroke-width", &cx.stroke);
            out.push_str("/>");
        }
        out.push_str("</g>\n");
    }
}

fn push_fragments(out: &mut String, cx: &Ctx, steps: &[Step<'_>]) {
    for s in steps {
        let Ev::Section { frag, f, k } = s.ev else {
            continue;
        };
        if k != 0 {
            continue;
        }
        let Some(g) = cx.geom.fragments.get(f) else {
            continue;
        };
        let mut class = String::from("merlion-cluster merlion-fragment");
        if cx.frag_class.get(f).copied().unwrap_or(false) {
            class.push(' ');
            class.push_str(&fragment_style_class(f));
        }
        let roles = cx.roles.cluster(f);
        super::push_role_classes(&mut class, "merlion-cc-", roles);
        if cx.roles.cluster_is_auto(f) {
            class.push(' ');
            class.push_str(super::auto::MARKER);
        }
        let fixed = match &frag.kind {
            FragmentKind::Rect(c) => c.as_ref(),
            _ => None,
        };
        let paint = cluster_paint(roles, fixed, cx.light);
        out.push_str("<g");
        attr(out, "class", &class);
        attr(out, "data-merlion-kind", g.kind.as_str());
        let _ = write!(out, " data-merlion-index=\"{}\">", f);

        push_rect(
            out,
            "merlion-cluster-box",
            g.box_,
            Some(super::CLUSTER_RADIUS),
        );
        push_fill_stroke(out, &paint, &cx.stroke);
        out.push_str("/>");

        out.push_str("<path class=\"merlion-fragment-tab\"");
        attr(out, "d", &marks::tab_d(g.tab));
        out.push_str(" fill=\"none\"");
        attr(out, "stroke", &cx.light.lit(Role::Muted));
        attr(out, "stroke-width", &cx.stroke);
        out.push_str("/>");
        push_kind_word(out, g, &paint.text);
        label_svg::push_label(
            out,
            &g.label,
            g.label_x,
            g.label_y,
            "merlion-fragment-label",
            &paint.text,
            &cx.detail(),
        );
        for sec in &g.sections {
            out.push_str("<path class=\"merlion-fragment-divider\"");
            attr(
                out,
                "d",
                &line_d((g.box_.x, sec.y), (g.box_.x + g.box_.w, sec.y)),
            );
            out.push_str(" fill=\"none\"");
            attr(out, "stroke", &paint.stroke);
            attr(out, "stroke-width", &cx.stroke);
            out.push_str(" stroke-dasharray=\"4 4\"/>");
            label_svg::push_label(
                out,
                &sec.label,
                sec.label_x,
                sec.label_y,
                "merlion-fragment-section",
                &cx.light.lit(Role::Muted),
                &cx.detail(),
            );
        }
        out.push_str("</g>\n");
    }
}

/// The kind word in the tab, drawn as the fragment's `.merlion-cluster-title`. The
/// geometry measures no text for it, so it sits a fixed inset from the tab's corner.
fn push_kind_word(out: &mut String, g: &crate::geometry::sequence::FragmentGeom, fill: &str) {
    let word = g.kind.as_str();
    out.push_str("<text class=\"merlion-cluster-title\"");
    attr(out, "fill", fill);
    out.push('>');
    out.push_str("<tspan");
    attr_num(out, "x", finite(g.tab.x) + 6.0);
    attr_num(out, "y", finite(g.tab.y) + finite(g.tab.h) - 5.0);
    out.push('>');
    push_escaped(out, word);
    out.push_str("</tspan></text>");
}

fn push_activations(out: &mut String, cx: &Ctx) {
    for a in &cx.geom.activations {
        out.push_str("<g class=\"merlion-activation\"");
        attr(out, "data-merlion-id", cx.participant_id(a.participant));
        let _ = write!(out, " data-merlion-depth=\"{}\">", a.depth);
        let bar = Rect {
            w: if a.bar.w > 0.0 { a.bar.w } else { ACTIVATION_W },
            ..a.bar
        };
        push_rect(out, "merlion-shape", bar, None);
        attr(out, "fill", &cx.light.lit(Role::NodeBg));
        attr(out, "stroke", &cx.light.lit(Role::NodeBorder));
        attr(out, "stroke-width", &cx.stroke);
        out.push_str("/></g>\n");
    }
}

fn push_notes(out: &mut String, cx: &Ctx, steps: &[Step<'_>]) {
    for s in steps {
        let Ev::Note(n, i) = s.ev else { continue };
        let Some(g) = cx.geom.notes.get(i) else {
            continue;
        };
        out.push_str("<g class=\"merlion-note\"");
        attr(out, "data-merlion-from", cx.participant_id(n.from));
        attr(out, "data-merlion-to", cx.participant_id(n.to));
        attr(
            out,
            "data-merlion-placement",
            match n.placement {
                crate::model::sequence::Placement::LeftOf => "left",
                crate::model::sequence::Placement::RightOf => "right",
                crate::model::sequence::Placement::Over => "over",
            },
        );
        out.push('>');
        push_rect(out, "merlion-note-box", g.box_, None);
        attr(out, "fill", &cx.light.lit(Role::Surface));
        attr(out, "stroke", &cx.light.lit(Role::Border));
        attr(out, "stroke-width", &cx.stroke);
        out.push_str("/>");
        label_svg::push_label(
            out,
            &g.label,
            g.box_.x + g.box_.w / 2.0,
            g.box_.y + g.box_.h / 2.0,
            "merlion-label",
            &cx.light.lit(Role::NodeText),
            &cx.detail(),
        );
        out.push_str("</g>\n");
    }
}

fn push_messages(out: &mut String, cx: &Ctx, steps: &[Step<'_>]) {
    for s in steps {
        let Ev::Message(m) = s.ev else { continue };
        let Some(g) = cx.geom.messages.get(m.index as usize) else {
            continue;
        };
        out.push_str("<g class=\"merlion-edge merlion-message\"");
        attr(out, "data-merlion-from", cx.participant_id(m.from));
        attr(out, "data-merlion-to", cx.participant_id(m.to));
        let _ = write!(
            out,
            " data-merlion-index=\"{0}\" id=\"{1}-e{0}\">",
            m.index, cx.id
        );

        let mut class = String::from("merlion-edge-path");
        if m.line == MessageLine::Dotted {
            class.push_str(" merlion-dotted");
        }
        out.push_str("<path");
        attr(out, "class", &class);
        attr(
            out,
            "d",
            &if g.self_loop {
                self_d(g.from, g.to)
            } else {
                line_d(g.from, g.to)
            },
        );
        out.push_str(" fill=\"none\"");
        attr(out, "stroke", &cx.light.lit(Role::Edge));
        attr(out, "stroke-width", &cx.stroke);
        if m.line == MessageLine::Dotted {
            out.push_str(" stroke-dasharray=\"3 3\"");
        }
        for (h, which) in [(m.tail, "marker-start"), (m.head, "marker-end")] {
            if let Some(mk) = marks::mark(h) {
                let _ = write!(out, " {}=\"url(#{}-{})\"", which, cx.id, mk.name);
            }
        }
        out.push_str("/>");

        push_message_label(out, cx, g);
        push_central(out, cx, m, g);
        push_number(out, cx, m, g);
        out.push_str("</g>\n");
    }
}

fn push_message_label(out: &mut String, cx: &Ctx, g: &crate::geometry::sequence::MessageGeom) {
    let Some((x, y, l)) = g.label.as_ref().map(|(x, y, l)| (*x, *y, l)) else {
        return;
    };
    if l.lines.is_empty() {
        return;
    }
    let (w, h) = crate::geometry::chip_size(l);
    out.push_str("<g class=\"merlion-edge-label\"><rect class=\"merlion-edge-label-bg\"");
    attr_num(out, "x", finite(x) - w / 2.0);
    attr_num(out, "y", finite(y) - h / 2.0);
    attr_num(out, "width", nonneg(w));
    attr_num(out, "height", nonneg(h));
    attr_num(out, "rx", super::CHIP_RADIUS);
    attr(out, "fill", &cx.light.lit(Role::EdgeLabelBg));
    out.push_str("/>");
    label_svg::push_label(
        out,
        l,
        x,
        y,
        "merlion-edge-text",
        &cx.light.lit(Role::Fg),
        &cx.detail(),
    );
    out.push_str("</g>");
}

/// The dot a central connection ends on, drawn on the participant's lifeline rather
/// than on the activation bar's edge (specs/sequence.md#messages).
fn push_central(
    out: &mut String,
    cx: &Ctx,
    m: &Message,
    g: &crate::geometry::sequence::MessageGeom,
) {
    let ends: &[(usize, (f64, f64))] = match m.central {
        Central::None => &[],
        Central::Source => &[(m.from, g.from)],
        Central::Target => &[(m.to, g.to)],
        Central::Both => &[(m.from, g.from), (m.to, g.to)],
    };
    for (p, (_, y)) in ends {
        let x = cx
            .geom
            .participants
            .get(*p)
            .map(|c| c.x)
            .unwrap_or(f64::NAN);
        out.push_str("<circle class=\"merlion-central\"");
        attr_num(out, "cx", finite(x));
        attr_num(out, "cy", finite(*y));
        attr_num(out, "r", CENTRAL_R);
        attr(out, "fill", &cx.light.lit(Role::Edge));
        out.push_str(" stroke=\"none\"/>");
    }
}

/// The autonumber badge (specs/sequence.md#autonumber). The geometry gives the badge's
/// centre and no measured text, so the number is centred with `text-anchor`.
fn push_number(
    out: &mut String,
    cx: &Ctx,
    m: &Message,
    g: &crate::geometry::sequence::MessageGeom,
) {
    let (Some(a), Some((x, y))) = (cx.seq.autonumber.filter(|a| a.visible), g.number) else {
        return;
    };
    let (x, y) = (finite(x), finite(y));
    let size = cx.font_size * NUMBER_SCALE;
    out.push_str("<g class=\"merlion-message-number\"><circle class=\"merlion-number-bg\"");
    attr_num(out, "cx", x);
    attr_num(out, "cy", y);
    attr_num(out, "r", NUMBER_R);
    attr(out, "fill", &cx.light.lit(Role::EdgeLabelBg));
    attr(out, "stroke", &cx.light.lit(Role::Border));
    attr(out, "stroke-width", &cx.stroke);
    out.push_str("/><text class=\"merlion-number-text\" text-anchor=\"middle\"");
    attr_num(out, "x", x);
    attr_num(out, "y", y + size * 0.35);
    attr_num(out, "font-size", size);
    attr(out, "fill", &cx.light.lit(Role::Fg));
    out.push('>');
    push_num(out, a.value(m.index) as f64 / 100.0);
    out.push_str("</text></g>");
}

// ------------------------------------------------------------------------- draw

/// Draws a laid-out sequence diagram. `id` is the validated `id_prefix` or the default
/// hash id.
pub fn draw_sequence(
    seq: &Sequence,
    geom: &SequenceGeometry,
    opts: &RenderOptions,
    id: &str,
    diags: &mut Diagnostics,
) -> DrawOutput {
    let id = safe_root_id(id);
    let outline_text = outline::outline(seq);
    let steps = walk(seq);
    let frags: Vec<FragmentKind> = steps
        .iter()
        .filter_map(|s| match s.ev {
            Ev::Section { frag, k: 0, .. } => Some(frag.kind),
            _ => None,
        })
        .collect();

    let builtins: Vec<_> = BUILT_IN.iter().collect();
    let light = Layer::new(opts.palette.as_ref().map(|p| &p.light), builtins.clone());
    let dark = opts
        .palette
        .as_ref()
        .and_then(|p| p.dark.as_ref())
        .map(|d| Layer::new(Some(d), builtins));

    let auto_on = opts.auto_tone && seq.meta.auto_tone != Some(false);
    let roles = sequence_roles(seq, &frags, auto_on);
    let (src_rules, box_class, frag_class) = source_rules(seq, &frags);
    if !src_rules.is_empty() {
        diags.emit_once(
            Severity::Info,
            "I030",
            Span::default(),
            crate::parse::style::FIXED_COLOUR_MESSAGE,
        );
    }
    let (palette_light, palette_dark) =
        super::palette_rules(&roles, &id, &light, dark.as_ref(), diags);
    let mut role_rules = super::built_in_rules(&roles, &light);
    role_rules.extend(cluster_tone_rules(&roles, &light));
    role_rules.extend(palette_light);
    let dark_rules = dark.as_ref().map(|d| {
        let mut r = super::built_in_rules(&roles, d);
        r.extend(cluster_tone_rules(&roles, d));
        r.extend(palette_dark);
        r
    });

    let (w, h) = (nonneg(geom.width), nonneg(geom.height));
    let mut out = String::with_capacity(2048 + seq.participants.len() * 400);

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
    out.push_str("<style>");
    let light_layer = style::Layer {
        table: &light.table,
        roles: &role_rules,
        source: &src_rules,
        sequence: true,
    };
    let dark_layer = match (&dark, &dark_rules) {
        (Some(d), Some(r)) => Some(style::Layer {
            table: &d.table,
            roles: r,
            source: &src_rules,
            sequence: true,
        }),
        _ => None,
    };
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

    let heads = used_heads(&steps);
    push_defs(&mut out, &id, &heads, &light);

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

    let cx = Ctx {
        seq,
        geom,
        id: &id,
        roles: &roles,
        light: &light,
        box_class,
        frag_class,
        font_size,
        stroke: stroke_attr(&light.table),
    };
    push_boxes(&mut out, &cx);
    push_fragments(&mut out, &cx, &steps);
    push_participants(&mut out, &cx);
    push_activations(&mut out, &cx);
    push_notes(&mut out, &cx, &steps);
    push_messages(&mut out, &cx, &steps);

    out.push_str("</g>\n</svg>\n");

    DrawOutput {
        svg: out,
        outline: outline_text,
    }
}
