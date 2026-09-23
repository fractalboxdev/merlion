//! State-diagram geometry (specs/state.md#lowering): the flowchart engine's
//! [`Geometry`] over the lowered graph, plus the note boxes.
//!
//! A note is not a graph element. The lowering inflates its state's measured extent on
//! the order axis, so the layout reserves the space like any other node width, and
//! `layout::state::place_notes` then carves the box back out of the laid-out node box.
//! The note therefore overlaps no node, no cluster title and no routed edge.

use alloc::vec::Vec;

use crate::geometry::{Geometry, Point};
use crate::model::state::NotePlacement;
use crate::text::LabelLayout;

/// One note box, carved out of the extent the lowering reserved for its state
/// (specs/state.md#notes-2).
#[derive(Clone, Debug, PartialEq)]
pub struct StateNote {
    /// Index into `StateMachine::notes`.
    pub index: usize,
    /// The state it sits beside, as an index into `StateMachine::states`.
    pub state: usize,
    pub placement: NotePlacement,
    /// Top-left corner and outer size of the box.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub label: LabelLayout,
    /// Where the dashed connector meets the state's own rect.
    pub anchor: Point,
}

/// The drawing (specs/state.md#svg-output). `graph` indices align with the lowered
/// graph, not with the model: `layout::state::Lowering` maps them back.
#[derive(Clone, Debug, PartialEq)]
pub struct StateGeometry {
    pub graph: Geometry,
    /// One per `StateMachine::notes`, in source order.
    pub notes: Vec<StateNote>,
}
