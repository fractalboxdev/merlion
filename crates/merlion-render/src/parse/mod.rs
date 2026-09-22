//! Parsers (specs/parser.md). STUB: owned by the parser workstream.

use crate::diag::Diagnostics;
use crate::model::Diagram;
use crate::options::Limits;
use alloc::string::String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Header is not a supported diagram type.
    UnsupportedDiagram { header: String },
    /// An `Error` diagnostic was recorded (syntax error, strict-mode repair, E010–E012).
    Failed,
    /// Input exceeds a size limit.
    TooLarge { what: &'static str },
}

pub struct ParseOptions {
    pub strict: bool,
    pub limits: Limits,
}

/// Parses any supported diagram. Repairs, warnings and infos go to `diags`.
pub fn parse(
    _source: &str,
    _opts: &ParseOptions,
    _diags: &mut Diagnostics,
) -> Result<Diagram, ParseError> {
    Err(ParseError::UnsupportedDiagram {
        header: String::new(),
    })
}
