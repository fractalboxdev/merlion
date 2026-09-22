//! The `stateDiagram` / `stateDiagram-v2` parser (specs/state.md#syntax).
//!
//! Both headers parse to the same model: mermaid keeps two renderers and points
//! `stateDiagram` at the older one, which is a difference of appearance, not of meaning.
//!
//! The body reader is a scaffold: it consumes the diagram body and returns a
//! [`StateMachine`] carrying the preamble's [`Meta`] and no statements, so the pipeline
//! is wired end to end while the statement grammar lands.
//!
//! TODO(owner): read states, transitions, composite states, concurrency regions,
//! notes, `direction` and the style statements into the model, resolving `[*]` to one
//! start and one end state per scope (specs/state.md#start-and-end), with the
//! diagnostics and repairs of specs/state.md#diagnostics (`W024`, `W025`, `R014`–`R018`).

use crate::diag::Diagnostics;
use crate::model::state::StateMachine;
use crate::model::Meta;

use super::cursor::LineIndex;
use super::{ParseOptions, Stop};

/// Parses the body of a state diagram starting at `pos`, which is the offset just after
/// the header word. Repairs, warnings and infos go to `diags`.
pub(crate) fn parse_state(
    idx: &LineIndex,
    pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<StateMachine, Stop> {
    let _ = (idx, pos, opts, diags);
    Ok(StateMachine {
        meta,
        ..StateMachine::default()
    })
}
