//! State-diagram lowering (specs/state.md#lowering): a `StateMachine` becomes the same
//! graph a flowchart is, and [`layout_state`] runs [`crate::layout::layout_flowchart`]
//! over it. None of the seven phases of specs/layout.md changes for a state machine,
//! and this module adds none of its own.
//!
//! | Model | Lowered as |
//! |---|---|
//! | `Simple` state | Node, `Shape::Rect` |
//! | `Composite` state with members | Cluster, its members inside |
//! | `Composite` state with no members | Node, `Shape::Rect`, so a transition naming it lands |
//! | `Choice` | Node, `Shape::Rhombus`, empty label |
//! | `Fork`, `Join` | Node, `Shape::Fork`: the 70 × 10 bar |
//! | `Start` | Node, `Shape::FilledCircle` |
//! | `End` | Node, `Shape::FramedCircle` |
//! | `Region` | Cluster nested in its composite's cluster |
//! | `Transition` | Edge, `Stroke::Normal`, `Arrow::Arrow` at the target |
//! | `Note` | Not a graph element: reserved in its state's extent, placed after layout |
//!
//! The lowering is a scaffold: it produces an empty graph so the pipeline is wired end
//! to end while the mapping lands.
//!
//! TODO(owner): build the graph, hold the six guarantees of
//! specs/state.md#what-the-lowering-guarantees, inflate each noted state's extent and
//! carve the note boxes back out after layout.

use alloc::vec::Vec;

use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::state::StateGeometry;
use crate::model::state::{StateKind, StateMachine};
use crate::model::{Flowchart, Shape};
use crate::options::RenderOptions;

use super::LayoutError;

/// Padding inside a note box, per side (specs/state.md#notes-2).
pub const NOTE_PAD: (f64, f64) = (10.0, 8.0);
/// Distance from a state's rect to the note box beside it.
pub const NOTE_GAP: f64 = 16.0;
/// Wrap width of note text.
pub const NOTE_WRAP: f64 = 180.0;
/// Gap between two notes on the same state.
pub const NOTE_STACK: f64 = 8.0;

/// The shape a state kind lowers to. `Composite` has none: it becomes a cluster unless
/// it holds no member, and an empty composite lowers as `Simple` does.
pub fn node_shape(kind: StateKind) -> Option<Shape> {
    match kind {
        StateKind::Simple => Some(Shape::Rect),
        StateKind::Composite => None,
        StateKind::Choice => Some(Shape::Rhombus),
        StateKind::Fork | StateKind::Join => Some(Shape::Fork),
        StateKind::Start => Some(Shape::FilledCircle),
        StateKind::End => Some(Shape::FramedCircle),
    }
}

/// What a lowered cluster stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterOrigin {
    /// `StateMachine::states[i]`, a `Composite`.
    Composite(usize),
    /// `StateMachine::regions[i]`.
    Region(usize),
}

/// The lowered graph and the maps back to the model. Every map is total over its side,
/// so the draw stage never searches for the state a lowered element came from.
#[derive(Clone, Debug, Default)]
pub struct Lowering {
    /// The graph the flowchart engine lays out.
    pub graph: Flowchart,
    /// `graph.nodes[i]` stands for `StateMachine::states[node_of[i]]`.
    pub node_of: Vec<usize>,
    /// `graph.subgraphs[i]` stands for `cluster_of[i]`.
    pub cluster_of: Vec<ClusterOrigin>,
    /// `graph.edges[i]` is `StateMachine::transitions[edge_of[i]]`.
    pub edge_of: Vec<usize>,
    /// The lowered node of every state; `None` for a composite that became a cluster.
    pub node_for: Vec<Option<usize>>,
    /// The lowered cluster of every state; `None` for everything but a composite.
    pub cluster_for: Vec<Option<usize>>,
}

/// A laid-out state diagram: the lowered graph, the maps back to the model, and the
/// geometry the draw stage reads.
#[derive(Clone, Debug)]
pub struct StateLayout {
    pub lowering: Lowering,
    pub geometry: StateGeometry,
}

/// Lowers `sm` onto the graph the layered engine takes. Charges one mandatory fuel unit
/// per state, transition, note and region before it allocates (specs/state.md#fuel).
pub fn lower(
    sm: &StateMachine,
    opts: &RenderOptions,
    fuel: &mut Fuel,
) -> Result<Lowering, LayoutError> {
    let _ = opts;
    let units = sm
        .states
        .len()
        .saturating_add(sm.transitions.len())
        .saturating_add(sm.notes.len())
        .saturating_add(sm.regions.len()) as u64;
    if fuel.burn(units).is_err() {
        return Err(LayoutError::TooLarge { what: "fuel" });
    }
    Ok(Lowering {
        graph: Flowchart {
            meta: sm.meta.clone(),
            direction: sm.direction,
            class_defs: sm.class_defs.clone(),
            ..Flowchart::default()
        },
        node_for: alloc::vec![None; sm.states.len()],
        cluster_for: alloc::vec![None; sm.states.len()],
        ..Lowering::default()
    })
}

/// Lowers, lays the graph out with the flowchart engine, and places the notes. Hint
/// problems are `I020`/`I021`/`I022` diagnostics, never errors, exactly as for a
/// flowchart (specs/state.md#phases-and-options).
pub fn layout_state(
    sm: &StateMachine,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<StateLayout, LayoutError> {
    let lowering = lower(sm, opts, fuel)?;
    let graph = super::layout_flowchart(&lowering.graph, opts, fuel, diags)?;
    let geometry = StateGeometry {
        graph,
        notes: Vec::new(),
    };
    Ok(StateLayout { lowering, geometry })
}
