//! Sequence layout (specs/sequence.md#layout): participants are columns in source
//! order and items are rows, so none of the seven phases of specs/layout.md runs. The
//! module shares the text measurement, the `target_width` fit and the fuel counter.
//!
//! The geometry builder is a scaffold: it sizes an empty drawing so the pipeline is
//! wired end to end while the column and row solver lands.
//!
//! TODO(owner): measure the participant labels, solve the column gaps against the
//! message and note requirements, walk the item tree into rows, and fit the container
//! (specs/sequence.md#columns, #rows, #fragments-and-container-fit).

use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::sequence::SequenceGeometry;
use crate::model::sequence::Sequence;
use crate::options::RenderOptions;

use super::pipeline::MARGIN;
use super::LayoutError;

/// Padding inside a participant head box, per side (specs/sequence.md#constants).
pub const HEAD_PAD: (f64, f64) = (12.0, 8.0);
/// Smallest head box.
pub const HEAD_MIN: (f64, f64) = (80.0, 32.0);
/// The stick figure an `Actor` draws above its label.
pub const ACTOR_FIGURE: (f64, f64) = (24.0, 32.0);
/// Space above a message label and below its arrow.
pub const ROW_GAP: f64 = 12.0;
/// Padding of a message label over the line it sits on.
pub const LABEL_PAD: (f64, f64) = (6.0, 2.0);
/// Height of a self-message bracket.
pub const SELF_HEIGHT: f64 = 34.0;
/// How far a self-message reaches right of its own lifeline.
pub const SELF_WIDTH: f64 = 40.0;
/// Padding inside a note box, per side.
pub const NOTE_PAD: (f64, f64) = (10.0, 8.0);
/// Space between a fragment box and the content it encloses.
pub const FRAGMENT_PAD: f64 = 8.0;
/// Space between a participant box and the head boxes it encloses.
pub const BOX_PAD: f64 = 8.0;

/// Measures, lays out and fits a sequence diagram. Fuel exhaustion in a mandatory phase
/// is `TooLarge`; an optional pass stops and keeps the previous result.
pub fn layout_sequence(
    seq: &Sequence,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<SequenceGeometry, LayoutError> {
    let _ = diags;
    // One mandatory unit per participant and per top-level item
    // (specs/sequence.md#fuel).
    let units = seq.participants.len().saturating_add(seq.items.len()) as u64;
    if fuel.burn(units).is_err() {
        return Err(LayoutError::TooLarge { what: "fuel" });
    }
    let width = if opts.target_width.is_finite() && opts.target_width > 0.0 {
        opts.target_width
    } else {
        2.0 * MARGIN + HEAD_MIN.0
    };
    Ok(SequenceGeometry {
        width,
        height: 2.0 * MARGIN + HEAD_MIN.1,
        fuel_used: units,
        ..SequenceGeometry::default()
    })
}
