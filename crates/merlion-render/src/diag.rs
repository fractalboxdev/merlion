//! Diagnostics shared by every stage (specs/parser.md#diagnostics).

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Repair,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Repair => "repair",
            Severity::Info => "info",
        }
    }
}

/// 1-based line; 1-based column in Unicode scalar values; UTF-8 byte offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub column: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub span: Span,
    pub message: String,
    pub fix: Option<Fix>,
}

/// Collects diagnostics. `strict` promotes every Warning and Repair to Error.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
    pub strict: bool,
}

impl Diagnostics {
    pub fn new(strict: bool) -> Self {
        Diagnostics {
            items: Vec::new(),
            strict,
        }
    }

    pub fn push(&mut self, mut d: Diagnostic) {
        if self.strict && matches!(d.severity, Severity::Warning | Severity::Repair) {
            d.severity = Severity::Error;
        }
        self.items.push(d);
    }

    pub fn emit(
        &mut self,
        severity: Severity,
        code: &'static str,
        span: Span,
        message: impl Into<String>,
    ) {
        self.push(Diagnostic {
            severity,
            code,
            span,
            message: message.into(),
            fix: None,
        });
    }

    /// Adds a diagnostic unless one with the same code already exists (e.g. `I010` once per diagram).
    pub fn emit_once(
        &mut self,
        severity: Severity,
        code: &'static str,
        span: Span,
        message: impl Into<String>,
    ) {
        if !self.items.iter().any(|d| d.code == code) {
            self.emit(severity, code, span, message);
        }
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }
}
