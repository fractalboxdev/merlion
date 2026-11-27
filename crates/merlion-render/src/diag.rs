//! Diagnostics shared by every stage (specs/parser.md#diagnostics).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// Longest quoted source excerpt in a message, in characters.
pub const EXCERPT_CHARS: usize = 64;

/// Longest message, in characters, after control characters are escaped.
pub const MESSAGE_CHARS: usize = 512;

/// Characters a diagnostic never carries raw: C0 and C1 controls (tab and newline
/// included, since a message is one line), bidi controls and non-characters, the
/// characters [`crate::svg::escape::is_dropped`] removes from SVG text.
fn is_unprintable(c: char) -> bool {
    c == '\t' || c == '\n' || crate::svg::escape::is_dropped(c)
}

/// Appends `s` with every unprintable character written as `\u{…}`, stopping after
/// `max` characters with `…`.
fn push_printable(out: &mut String, s: &str, max: usize) {
    for (n, c) in s.chars().enumerate() {
        if n == max {
            out.push('…');
            return;
        }
        if is_unprintable(c) {
            let _ = write!(out, "\\u{{{:x}}}", c as u32);
        } else {
            out.push(c);
        }
    }
}

/// A source excerpt for a message: at most [`EXCERPT_CHARS`] characters, then `…`, with
/// control characters escaped, so untrusted source never reaches a terminal raw.
pub fn excerpt(s: &str) -> String {
    let mut out = String::new();
    push_printable(&mut out, s, EXCERPT_CHARS);
    out
}

/// A message safe to print: control characters escaped and at most [`MESSAGE_CHARS`]
/// characters. Applied to every diagnostic the core returns.
pub fn printable(message: &str) -> String {
    let mut out = String::new();
    push_printable(&mut out, message, MESSAGE_CHARS);
    out
}

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

/// Diagnostics kept when no caller says otherwise
/// ([`crate::options::Limits::diagnostics`]).
pub const DIAGNOSTIC_LIMIT: usize = 4_000;

/// Collects diagnostics. `strict` promotes every Warning and Repair to Error.
///
/// The list is bounded: a source can carry a repair every two bytes, and each one costs
/// about a hundred bytes of message, so an unbounded list turns a 1 MiB input into tens
/// of megabytes of diagnostics. Past `limit` a diagnostic is counted rather than kept,
/// and the last slot holds `I034` naming how many were dropped.
#[derive(Clone, Debug)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
    pub strict: bool,
    /// Most `items` may hold, the `I034` summary included. Never zero.
    limit: usize,
    /// Diagnostics counted instead of kept.
    dropped: usize,
    /// Whether any of them was an Error, so [`Self::has_errors`] stays truthful.
    dropped_error: bool,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Diagnostics::new(false)
    }
}

impl Diagnostics {
    pub fn new(strict: bool) -> Self {
        Diagnostics::with_limit(strict, DIAGNOSTIC_LIMIT)
    }

    pub fn with_limit(strict: bool, limit: usize) -> Self {
        Diagnostics {
            items: Vec::new(),
            strict,
            limit: limit.max(1),
            dropped: 0,
            dropped_error: false,
        }
    }

    pub fn push(&mut self, mut d: Diagnostic) {
        if d.message.chars().any(is_unprintable) || d.message.len() > MESSAGE_CHARS {
            d.message = printable(&d.message);
        }
        if self.strict && matches!(d.severity, Severity::Warning | Severity::Repair) {
            d.severity = Severity::Error;
        }
        // The last slot is the summary's, so a full list always ends by saying so.
        if self.items.len() + 1 < self.limit {
            self.items.push(d);
        } else {
            self.count_dropped(d);
        }
    }

    /// Records one diagnostic the limit leaves out, and keeps the summary in the last
    /// slot current. The span is the first dropped diagnostic's: it points at where the
    /// source ran past the limit.
    fn count_dropped(&mut self, d: Diagnostic) {
        self.dropped += 1;
        self.dropped_error |= d.severity == Severity::Error;
        if self.dropped == 1 {
            self.items.push(Diagnostic {
                severity: Severity::Info,
                code: "I034",
                span: d.span,
                message: String::new(),
                fix: None,
            });
        }
        let (dropped, limit) = (self.dropped, self.limit);
        if let Some(last) = self.items.last_mut() {
            last.message.clear();
            let _ = write!(
                last.message,
                "{dropped} further diagnostics past the limit of {limit} are not reported"
            );
        }
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

    /// Builds the span and message only while a diagnostic or its first dropped
    /// span is retained. Later diagnostics update the summary and error state.
    pub fn emit_lazy(
        &mut self,
        severity: Severity,
        code: &'static str,
        make: impl FnOnce() -> (Span, String),
    ) {
        if self.dropped == 0 {
            let (span, message) = make();
            self.emit(severity, code, span, message);
        } else {
            let severity =
                if self.strict && matches!(severity, Severity::Warning | Severity::Repair) {
                    Severity::Error
                } else {
                    severity
                };
            self.count_dropped(Diagnostic {
                severity,
                code,
                span: Span::default(),
                message: String::new(),
                fix: None,
            });
        }
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
        self.dropped_error || self.items.iter().any(|d| d.severity == Severity::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn lazy_diagnostics_stop_building_after_the_first_dropped_span() {
        let built = Cell::new(0);
        let mut diagnostics = Diagnostics::with_limit(true, 2);
        diagnostics.emit(Severity::Info, "I000", Span::default(), "info");
        for _ in 0..10 {
            diagnostics.emit_lazy(Severity::Warning, "W000", || {
                built.set(built.get() + 1);
                (
                    Span {
                        line: 7,
                        ..Span::default()
                    },
                    String::from("warning"),
                )
            });
        }
        assert_eq!(built.get(), 1);
        assert_eq!(diagnostics.items.len(), 2);
        assert_eq!(diagnostics.items[1].code, "I034");
        assert_eq!(diagnostics.items[1].span.line, 7);
        assert!(diagnostics.items[1]
            .message
            .starts_with("10 further diagnostics"));
        assert!(diagnostics.has_errors());
    }
}
