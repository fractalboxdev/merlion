//! The `sequenceDiagram` parser (specs/sequence.md#syntax).
//!
//! The body reader is a scaffold: it consumes the diagram body and returns a
//! [`Sequence`] carrying the preamble's [`Meta`] and no statements, so the pipeline is
//! wired end to end while the statement grammar lands.
//!
//! TODO(owner): read participants, messages, notes, fragments, boxes and autonumber
//! into the model, with the diagnostics and repairs of specs/sequence.md#diagnostics
//! (`W021`–`W023`, `R009`–`R013`).

use crate::diag::Diagnostics;
use crate::model::sequence::Sequence;
use crate::model::Meta;

use super::cursor::LineIndex;
use super::{ParseOptions, Stop};

/// Parses the body of a `sequenceDiagram` starting at `pos`, which is the offset just
/// after the header word. Repairs, warnings and infos go to `diags`.
pub(crate) fn parse_sequence(
    idx: &LineIndex,
    pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Sequence, Stop> {
    let _ = (idx, pos, opts, diags);
    Ok(Sequence {
        meta,
        ..Sequence::default()
    })
}
