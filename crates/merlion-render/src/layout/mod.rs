//! Layered layout (specs/layout.md): the Sugiyama framework with dominator-based
//! cycle removal, container fit and stable layout. Every algorithm is implemented from
//! the published descriptions cited at its implementation site.
//!
//! | Phase | Module |
//! |---|---|
//! | 1. Cycle removal | [`acyclic`] |
//! | 2. Layer assignment, dummies, clusters | [`layering`], [`lgraph`] |
//! | 3. Crossing minimisation | [`order`] |
//! | 4. Coordinate assignment | [`coords`] |
//! | 5. Container fit | [`fit`], [`pipeline`] |
//! | 6. Edge routing | [`route`] |
//! | 7. Clusters | [`lgraph`], [`coords`], [`pipeline`] |
//! | Stable layout | [`hint`], [`order`] |

mod acyclic;
mod coords;
mod fit;
pub mod hint;
mod layering;
mod lgraph;
pub mod measure;
pub mod metrics;
mod order;
mod pipeline;
mod route;

use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::Geometry;
use crate::model::Flowchart;
use crate::options::RenderOptions;

pub use pipeline::{CLUSTER_PAD, MARGIN};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// A size limit or mandatory-phase fuel was exceeded.
    TooLarge { what: &'static str },
}

/// Measures labels (via `crate::text::layout_label`), lays out and routes the flowchart.
/// Hint problems are `I020`/`I021`/`I022` diagnostics, never errors.
pub fn layout_flowchart(
    chart: &Flowchart,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<Geometry, LayoutError> {
    pipeline::run(chart, opts, fuel, diags)
}
