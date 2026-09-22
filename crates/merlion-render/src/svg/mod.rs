//! SVG writer (specs/svg-output.md). STUB: owned by the SVG workstream.

use alloc::string::String;

use crate::diag::Diagnostics;
use crate::geometry::Geometry;
use crate::model::Flowchart;
use crate::options::RenderOptions;

pub struct DrawOutput {
    pub svg: String,
    /// Plain-text outline (specs/svg-output.md#text-alternative).
    pub outline: String,
}

/// `id` is the validated `id_prefix` or the default hash id.
pub fn draw_flowchart(
    _chart: &Flowchart,
    _geom: &Geometry,
    _opts: &RenderOptions,
    _id: &str,
    _diags: &mut Diagnostics,
) -> DrawOutput {
    DrawOutput {
        svg: String::new(),
        outline: String::new(),
    }
}

/// Outline only, without layout (for `merlion outline`).
pub fn outline_flowchart(_chart: &Flowchart) -> String {
    String::new()
}
