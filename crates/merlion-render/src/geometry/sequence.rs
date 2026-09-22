//! Sequence-diagram geometry (specs/sequence.md#layout): the output of
//! [`crate::layout::layout_sequence`] and the input of [`crate::svg::draw_sequence`].
//! Coordinates are px, origin top-left, already fitted to the container.
//!
//! Index alignment with the model: `participants[i]` is `Sequence::participants[i]`,
//! `boxes[i]` is `Sequence::boxes[i]`, and `messages[k]` is the message whose
//! `Message::index` is `k`. Notes, fragments and activations are listed in the
//! pre-order of the item tree, which is source order.

use alloc::vec::Vec;

use crate::model::sequence::{FragmentKind, Placement};
use crate::text::LabelLayout;

/// Smallest gap between two head boxes that container fit may shrink to
/// (specs/sequence.md#constants).
pub const COLUMN_GAP_MIN: f64 = 8.0;
/// Width of an activation bar.
pub const ACTIVATION_W: f64 = 10.0;
/// Horizontal offset of each nested activation bar.
pub const ACTIVATION_NEST: f64 = 5.0;
/// Height of the kind tab at a fragment's top-left corner.
pub const FRAGMENT_TAB: f64 = 18.0;
/// Radius of an autonumber badge.
pub const NUMBER_R: f64 = 9.0;

/// An axis-aligned box: top-left corner and size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One column (specs/sequence.md#columns).
#[derive(Clone, Debug, PartialEq)]
pub struct ParticipantGeom {
    /// Centre of the column: the x of the lifeline and of both boxes.
    pub x: f64,
    /// Head box. A `create`d participant's head sits at its creating message's row.
    pub head: Rect,
    /// Foot box, absent for a destroyed participant, which draws a cross instead.
    pub foot: Option<Rect>,
    /// Top and bottom of the lifeline.
    pub lifeline: (f64, f64),
    pub label: LabelLayout,
}

/// One message row (specs/sequence.md#rows).
#[derive(Clone, Debug, PartialEq)]
pub struct MessageGeom {
    /// Start and end of the drawn line: the activation-bar edges when a bar is open at
    /// that end, else the lifelines.
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// A self-message draws a bracket right of its own lifeline, `SELF_WIDTH` wide and
    /// `SELF_HEIGHT` tall, instead of a straight line.
    pub self_loop: bool,
    /// Centre of the label, above the line.
    pub label: Option<(f64, f64, LabelLayout)>,
    /// Centre of the autonumber badge, when the diagram numbers its messages.
    pub number: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoteGeom {
    pub index: usize,
    pub placement: Placement,
    pub box_: Rect,
    pub label: LabelLayout,
}

/// One section divider of a fragment: the y of the dashed line and the label beside it.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionGeom {
    pub y: f64,
    pub label: LabelLayout,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FragmentGeom {
    pub kind: FragmentKind,
    pub box_: Rect,
    /// Nesting depth, 0 at the top level.
    pub depth: u32,
    /// The kind tab at the top-left corner.
    pub tab: Rect,
    /// The header label, right of the tab.
    pub label: LabelLayout,
    pub label_x: f64,
    pub label_y: f64,
    /// One per section after the first.
    pub sections: Vec<SectionGeom>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivationGeom {
    pub participant: usize,
    /// Nesting depth on that participant, 0 for the outermost open bar.
    pub depth: u32,
    pub bar: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoxGeom {
    pub box_: Rect,
    pub label: LabelLayout,
    pub label_x: f64,
    pub label_y: f64,
}

/// The laid-out diagram.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SequenceGeometry {
    pub width: f64,
    pub height: f64,
    pub participants: Vec<ParticipantGeom>,
    /// Indexed by `Message::index`.
    pub messages: Vec<MessageGeom>,
    pub notes: Vec<NoteGeom>,
    pub fragments: Vec<FragmentGeom>,
    pub activations: Vec<ActivationGeom>,
    pub boxes: Vec<BoxGeom>,
    pub fuel_used: u64,
}
