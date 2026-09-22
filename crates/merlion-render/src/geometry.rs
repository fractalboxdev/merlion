//! Layout output, input to the draw stage. Indices align with the model:
//! `nodes[i]` is `Flowchart::nodes[i]`, `edges[i]` is `Flowchart::edges[i]`,
//! `clusters[i]` is `Flowchart::subgraphs[i]`. Coordinates are px, origin top-left,
//! already transformed for the final direction and container fit.

use alloc::vec::Vec;

use crate::options::Direction;
use crate::text::LabelLayout;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeGeom {
    /// Centre.
    pub x: f64,
    pub y: f64,
    /// Outer size of the shape, padding included.
    pub w: f64,
    pub h: f64,
    pub label: LabelLayout,
    /// Semantic-zoom rank 0..=15: dominator depth for flowcharts (specs/svg-output.md#ids-and-data-attributes).
    pub rank: u8,
}

/// Padding of an edge label chip around its text, in px (horizontal, vertical).
pub const CHIP_PAD: (f64, f64) = (4.0, 2.0);

/// Outer size of the chip drawn behind a label of `label`'s size.
pub fn chip_size(label: &LabelLayout) -> (f64, f64) {
    (
        label.width + 2.0 * CHIP_PAD.0,
        label.height + 2.0 * CHIP_PAD.1,
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeLabelGeom {
    /// Centre of the label chip.
    pub x: f64,
    pub y: f64,
    pub label: LabelLayout,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeGeom {
    /// Polyline from the source boundary to the target boundary, in the edge's
    /// original (source → target) direction even for back-edges. Orthogonal routes
    /// are drawn with 6 px rounded corners at interior points.
    pub points: Vec<Point>,
    pub label: Option<EdgeLabelGeom>,
    /// Reversed during cycle removal (`data-merlion-back="true"`).
    pub back: bool,
    /// Crosses a container-fit wrap (`data-merlion-wrap="true"`).
    pub wrap: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClusterGeom {
    /// Top-left corner and size.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub label: LabelLayout,
    /// Centre of the title.
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    pub width: f64,
    pub height: f64,
    /// Final direction (after `direction: auto`).
    pub direction: Direction,
    pub nodes: Vec<NodeGeom>,
    pub edges: Vec<EdgeGeom>,
    pub clusters: Vec<ClusterGeom>,
    /// Phase-2 layers (before container-fit pseudo-layers), each the ordered node
    /// indices of real nodes; written as the layout hint (specs/svg-output.md#layout-hint).
    pub layers: Vec<Vec<usize>>,
    /// Informational diagnostics are pushed to `Diagnostics`; fuel used by layout.
    pub fuel_used: u64,
}
