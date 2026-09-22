//! Layered layout (specs/layout.md). STUB: owned by the layout workstream.

mod acyclic;
mod coords;
mod layering;
mod lgraph;
mod measure;
mod order;

use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::Geometry;
use crate::model::Flowchart;
use crate::options::RenderOptions;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// A size limit or mandatory-phase fuel was exceeded.
    TooLarge { what: &'static str },
}

/// Measures labels (via `crate::text::layout_label`), lays out and routes the flowchart.
/// Hint problems are `I020`/`I021`/`I022` diagnostics, never errors.
pub fn layout_flowchart(
    _chart: &Flowchart,
    _opts: &RenderOptions,
    _fuel: &mut Fuel,
    _diags: &mut Diagnostics,
) -> Result<Geometry, LayoutError> {
    Err(LayoutError::TooLarge {
        what: "unimplemented",
    })
}
