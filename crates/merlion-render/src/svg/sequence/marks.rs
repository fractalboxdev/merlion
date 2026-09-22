//! Arrowheads and participant symbols (specs/sequence.md#messages, #participants).
//!
//! Markers keep the flowchart's geometry — 10 × 10 user units, tip at x = 9,
//! `orient="auto-start-reverse"` — so `arrow` and `cross` are the shapes flowcharts
//! already define and the CSS classes `merlion-marker-fill` / `merlion-marker-stroke`
//! style every head with the rules that exist.

use alloc::string::String;

use crate::geometry::sequence::Rect;
use crate::model::sequence::{Head, ParticipantKind};
use crate::model::Shape;
use crate::numfmt::push_num;

/// How a marker paints: a filled outline or open strokes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Fill,
    Stroke,
}

/// One arrowhead: the `{id}-` suffix of its marker and the path it draws.
pub struct Mark {
    pub name: &'static str,
    pub d: &'static str,
    pub ink: Ink,
}

/// The arrowhead a head kind draws, or `None` for an open line end.
///
/// `Filled` and `Cross` are the flowchart `arrow` and `cross` markers. `Open` is
/// mermaid's async head: the filled triangle's outline, left open. The half heads add
/// the bar the `|` in `-|\` writes; the stick heads are the barb alone.
pub fn mark(h: Head) -> Option<Mark> {
    let m = |name, d, ink| Some(Mark { name, d, ink });
    match h {
        Head::None => None,
        Head::Filled => m("arrow", "M0 0L10 5L0 10Z", Ink::Fill),
        Head::Open => m("open", "M1 1L9 5L1 9", Ink::Stroke),
        Head::Cross => m("cross", "M2 1L9 9M2 9L9 1", Ink::Stroke),
        Head::HalfTop => m("half-top", "M9 1V9M1 0L9 5", Ink::Stroke),
        Head::HalfBottom => m("half-bottom", "M9 1V9M1 10L9 5", Ink::Stroke),
        Head::StickTop => m("stick-top", "M1 0L9 5", Ink::Stroke),
        Head::StickBottom => m("stick-bottom", "M1 10L9 5", Ink::Stroke),
    }
}

/// The head kinds in the order `<defs>` writes the markers they use.
pub const HEADS: [Head; 7] = [
    Head::Filled,
    Head::Open,
    Head::Cross,
    Head::HalfTop,
    Head::HalfBottom,
    Head::StickTop,
    Head::StickBottom,
];

/// The stick figure of an `Actor`, drawn in a `w` × `h` box with its top-left at
/// (`x`, `y`): head, spine, arms and legs, proportioned from the 24 × 32 default.
fn actor_d(x: f64, y: f64, w: f64, h: f64) -> String {
    let (u, v) = (w / 24.0, h / 32.0);
    let cx = x + w / 2.0;
    let r = 5.0 * v;
    let mut d = String::new();
    let mut cmd = |c: char, pts: &[f64]| {
        d.push(c);
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                d.push(' ');
            }
            push_num(&mut d, *p);
        }
    };
    // Head: two half arcs, as `shapes::ellipse` draws a circle.
    cmd('M', &[cx - r, y + r]);
    cmd('A', &[r, r, 0.0, 0.0, 1.0, cx + r, y + r]);
    cmd('A', &[r, r, 0.0, 0.0, 1.0, cx - r, y + r]);
    cmd('M', &[cx, y + 2.0 * r]);
    cmd('V', &[y + 22.0 * v]);
    cmd('M', &[cx - 9.0 * u, y + 14.0 * v]);
    cmd('H', &[cx + 9.0 * u]);
    cmd('M', &[cx - 8.0 * u, y + h]);
    cmd('L', &[cx, y + 22.0 * v]);
    cmd('L', &[cx + 8.0 * u, y + h]);
    d
}

/// The `d` of a participant's `.merlion-shape` inside its head or foot box.
///
/// specs/sequence.md fixes "head box or actor figure"; the store kinds take the
/// flowchart shape that already draws them, and `boundary`, `control` and `entity`
/// draw the plain head box and are told apart by `data-merlion-kind`.
pub fn symbol_d(kind: ParticipantKind, r: Rect, figure: (f64, f64)) -> String {
    let shape = match kind {
        ParticipantKind::Actor => {
            let (w, h) = (min(figure.0, r.w), min(figure.1, r.h));
            return actor_d(r.x + (r.w - w) / 2.0, r.y, w, h);
        }
        ParticipantKind::Database => Shape::Cylinder,
        ParticipantKind::Collections => Shape::StackedRect,
        ParticipantKind::Queue => Shape::HorizontalCylinder,
        ParticipantKind::Participant
        | ParticipantKind::Boundary
        | ParticipantKind::Control
        | ParticipantKind::Entity => Shape::Rect,
    };
    super::super::shapes::shape_d(shape, r.x + r.w / 2.0, r.y + r.h / 2.0, r.w, r.h)
}

/// The centre a participant's label sits on: below the figure for an `Actor`, the
/// middle of the box for every other kind.
pub fn label_centre(kind: ParticipantKind, r: Rect, figure: (f64, f64)) -> (f64, f64) {
    let cx = r.x + r.w / 2.0;
    if kind == ParticipantKind::Actor {
        let top = r.y + min(figure.1, r.h) + 4.0;
        return (cx, (top + r.y + r.h) / 2.0);
    }
    (cx, r.y + r.h / 2.0)
}

/// The cross that ends a destroyed participant's lifeline
/// (specs/sequence.md#activations-lifelines-create-and-destroy).
pub fn destroy_d(cx: f64, cy: f64, r: f64) -> String {
    let mut d = String::new();
    for (a, b, c, e) in [
        (cx - r, cy - r, cx + r, cy + r),
        (cx - r, cy + r, cx + r, cy - r),
    ] {
        d.push('M');
        push_num(&mut d, a);
        d.push(' ');
        push_num(&mut d, b);
        d.push('L');
        push_num(&mut d, c);
        d.push(' ');
        push_num(&mut d, e);
    }
    d
}

/// The kind tab at a fragment's top-left corner: a rectangle with its bottom-right
/// corner cut, so the tab reads as a folded label rather than a second box.
pub fn tab_d(r: Rect) -> String {
    let cut = min(6.0, min(r.w, r.h) / 2.0);
    let mut d = String::new();
    let mut cmd = |c: char, pts: &[f64]| {
        d.push(c);
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                d.push(' ');
            }
            push_num(&mut d, *p);
        }
    };
    cmd('M', &[r.x, r.y]);
    cmd('H', &[r.x + r.w]);
    cmd('V', &[r.y + r.h - cut]);
    cmd('L', &[r.x + r.w - cut, r.y + r.h]);
    cmd('H', &[r.x]);
    d.push('Z');
    d
}

fn min(a: f64, b: f64) -> f64 {
    if a.is_finite() && (!b.is_finite() || a < b) {
        a
    } else if b.is_finite() {
        b
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_head_but_none_names_one_marker() {
        assert!(mark(Head::None).is_none());
        let mut names: alloc::vec::Vec<&str> = HEADS
            .iter()
            .filter_map(|h| mark(*h))
            .map(|m| m.name)
            .collect();
        assert_eq!(names.len(), HEADS.len());
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), HEADS.len(), "marker names collide");
        for n in names {
            assert!(
                n.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "marker id {n:?} is not safe in a selector"
            );
        }
    }

    #[test]
    fn the_tab_cuts_its_bottom_right_corner() {
        let r = Rect {
            x: 10.0,
            y: 20.0,
            w: 40.0,
            h: 18.0,
        };
        assert_eq!(tab_d(r), "M10 20H50V32L44 38H10Z");
    }

    #[test]
    fn a_degenerate_box_still_draws_a_closed_path() {
        let d = tab_d(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        assert!(d.ends_with('Z') && !d.contains("NaN"), "{d}");
    }
}
