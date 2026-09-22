//! Diagnostics shared by every stage (specs/parser.md#diagnostics).

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

/// Longest quoted source excerpt in a message, in characters.
pub const EXCERPT_CHARS: usize = 64;

/// Longest message, in characters, after control characters are escaped.
pub const MESSAGE_CHARS: usize = 512;

/// Most warnings, repairs and infos one render reports. A document that repeats a
/// malformed statement on every line produces one diagnostic per line, each carrying a
/// message and a replacement string, so the report grows with the input while the
/// drawing does not; past this count the rest are counted and dropped, and one `I012`
/// says so. Errors are never dropped, so the cap cannot turn a failing render into a
/// succeeding one (specs/parser.md#codes).
pub const MAX_DIAGNOSTICS: usize = 2_000;

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

/// Collects diagnostics. `strict` promotes every Warning and Repair to Error.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
    pub strict: bool,
    /// Diagnostics dropped by [`MAX_DIAGNOSTICS`].
    suppressed: u64,
}

impl Diagnostics {
    pub fn new(strict: bool) -> Self {
        Diagnostics {
            items: Vec::new(),
            strict,
            suppressed: 0,
        }
    }

    pub fn push(&mut self, mut d: Diagnostic) {
        if d.severity != Severity::Error && self.items.len() >= MAX_DIAGNOSTICS {
            self.suppressed = self.suppressed.saturating_add(1);
            if self.suppressed == 1 {
                let span = d.span;
                self.items.push(Diagnostic {
                    severity: Severity::Info,
                    code: "I012",
                    span,
                    message: alloc::format!(
                        "more than {MAX_DIAGNOSTICS} diagnostics; the rest are suppressed"
                    ),
                    fix: None,
                });
            }
            return;
        }
        if d.message.chars().any(is_unprintable) || d.message.len() > MESSAGE_CHARS {
            d.message = printable(&d.message);
        }
        if self.strict && matches!(d.severity, Severity::Warning | Severity::Repair) {
            d.severity = Severity::Error;
        }
        self.items.push(d);
    }

    /// How many diagnostics the cap dropped.
    pub fn suppressed(&self) -> u64 {
        self.suppressed
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
