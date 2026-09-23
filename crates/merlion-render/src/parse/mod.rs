//! Parsers (specs/parser.md): hand-written recursive descent, one per diagram type,
//! with no parser-generator runtime.
//!
//! [`parse`] handles what precedes the diagram header (Markdown fences, YAML front
//! matter, `%%{init}%%` directives, comments), reads the header and dispatches to the
//! diagram parser. Every parser works on the original source, so every span and every
//! repair fix refers to the text the caller passed in.
//!
//! Diagnostic codes this module adds to the table in specs/parser.md#codes (`E002`,
//! `E003` and `E004` are listed there):
//!
//! | Code | Severity | Meaning |
//! |---|---|---|
//! | `W015` | Warning | `@{ shape: … }` shape or key Merlion does not draw; the node is a rectangle |
//! | `W016` | Warning | Unknown configuration key, value outside its set, or malformed directive |
//! | `I011` | Info | `theme`, `themeVariables` or `look` ignored: themes are CSS |

pub mod config;
pub mod cursor;
pub mod directive;
pub mod flowchart;
pub mod frontmatter;
pub mod label;
pub mod link;
pub mod repair;
pub mod sequence;
pub mod state;
pub mod style;

use alloc::string::String;

use crate::diag::{excerpt, Diagnostic, Diagnostics, Fix, Severity};
use crate::model::{Diagram, Meta};
use crate::options::Limits;
use cursor::{Cursor, LineIndex};

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

/// Why a diagram parser stopped early. A `Failed` stop has already recorded its
/// `Error` diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Failed,
    TooLarge(&'static str),
}

/// Longest string accepted inside a `%%{init}%%` directive (specs/parser.md, `E012`).
const DIRECTIVE_STRING_BYTES: usize = 4_096;

/// Parses any supported diagram. Repairs, warnings and infos go to `diags`.
pub fn parse(
    source: &str,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Diagram, ParseError> {
    if source.len() > opts.limits.input_bytes {
        return Err(ParseError::TooLarge { what: "input" });
    }
    let first_diag = diags.items.len();
    let idx = LineIndex::new(source);
    let mut meta = Meta::default();
    let mut c = Cursor::new(source, 0);
    c.eat("\u{feff}");

    // Preamble: fences, front matter (only before anything else), directives, comments.
    let mut front_matter_allowed = true;
    loop {
        c.skip_ws();
        if c.at_eof() {
            break;
        }
        // Fences and front matter occupy whole lines; the line is only read at a line
        // start, which keeps the preamble linear on long lines.
        let at_start = c.at_line_start();
        let line = if at_start {
            source.get(c.pos..c.eol()).unwrap_or("")
        } else {
            ""
        };
        if at_start && repair::is_fence_line(line) {
            c.pos = strip_fence_line(&idx, c.pos, diags);
            continue;
        }
        if front_matter_allowed && at_start && line.trim_end() == "---" {
            front_matter_allowed = false;
            match front_matter(&idx, c.pos, &mut meta, opts, diags) {
                Some(next) => c.pos = next,
                None => return Err(ParseError::Failed),
            }
            continue;
        }
        front_matter_allowed = false;
        if c.starts_with("%%{") {
            c.pos = directive(&idx, c.pos, &mut meta, opts, diags);
            continue;
        }
        if c.starts_with("%%") {
            c.skip_to_eol();
            continue;
        }
        break;
    }

    if c.at_eof() {
        diags.emit(
            Severity::Error,
            "E002",
            idx.span(c.pos, c.pos),
            "expected a diagram header such as `flowchart TD`",
        );
        return Err(ParseError::Failed);
    }
    let header_start = c.pos;
    while c.peek().is_some_and(|ch| !ch.is_whitespace() && ch != ';') {
        c.bump();
    }
    let header = source.get(header_start..c.pos).unwrap_or("");
    let result = match header {
        "flowchart" | "graph" | "flowchart-elk" => {
            flowchart::parse_flowchart(&idx, c.pos, meta, opts, diags).map(Diagram::Flowchart)
        }
        "sequenceDiagram" => {
            sequence::parse_sequence(&idx, c.pos, meta, opts, diags).map(Diagram::Sequence)
        }
        "stateDiagram" | "stateDiagram-v2" => {
            state::parse_state(&idx, c.pos, meta, opts, diags).map(Diagram::State)
        }
        _ => {
            return Err(ParseError::UnsupportedDiagram {
                header: excerpt(header),
            })
        }
    };
    match result {
        Ok(d) => {
            let failed = diags
                .items
                .get(first_diag..)
                .is_some_and(|new| new.iter().any(|x| x.severity == Severity::Error));
            if failed {
                Err(ParseError::Failed)
            } else {
                Ok(d)
            }
        }
        Err(Stop::Failed) => Err(ParseError::Failed),
        Err(Stop::TooLarge(what)) => Err(ParseError::TooLarge { what }),
    }
}

/// Records `R006` for the fence line containing `pos`, with a fix deleting the whole
/// line, and returns the offset of the next line.
pub(crate) fn strip_fence_line(idx: &LineIndex, pos: usize, diags: &mut Diagnostics) -> usize {
    let start = idx.line_start(pos);
    let next = idx.next_line_start(pos);
    let text_end = idx
        .src()
        .get(start..next)
        .map_or(next, |l| start + l.trim_end_matches(['\n', '\r']).len());
    diags.push(Diagnostic {
        severity: Severity::Repair,
        code: "R006",
        span: idx.span(start, text_end),
        message: String::from("Markdown code fence inside the diagram source; stripped"),
        fix: Some(Fix {
            span: idx.span(start, next),
            replacement: String::new(),
        }),
    });
    next
}

/// Records `R003` for a typographic quote used as a delimiter, with a fix replacing it
/// by an ASCII double quote.
pub(crate) fn typographic_quote(
    idx: &LineIndex,
    start: usize,
    end: usize,
    diags: &mut Diagnostics,
) {
    diags.push(Diagnostic {
        severity: Severity::Repair,
        code: "R003",
        span: idx.span(start, end),
        message: String::from("typographic quote used as a string delimiter; read as `\"`"),
        fix: Some(Fix {
            span: idx.span(start, end),
            replacement: String::from("\""),
        }),
    });
}

/// Parses the front matter whose opening `---` line starts at `pos`. Returns the offset
/// after the closing `---` line, or `None` when the block never closes (an `E011` has
/// been recorded and nothing after it can be read as a diagram).
fn front_matter(
    idx: &LineIndex,
    pos: usize,
    meta: &mut Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Option<usize> {
    let src = idx.src();
    let body_start = idx.next_line_start(pos);
    let mut line_start = body_start;
    while line_start < src.len() {
        let next = idx.next_line_start(line_start);
        let line = src.get(line_start..next).unwrap_or("");
        if line.trim_end() == "---" {
            let body = src.get(body_start..line_start).unwrap_or("");
            match frontmatter::parse_yaml(body, body_start, opts.limits.nesting) {
                Ok(entries) => config::apply_front_matter(&entries, idx, meta, diags),
                Err(e) => diags.emit(Severity::Error, "E011", idx.span(e.start, e.end), e.message),
            }
            return Some(next);
        }
        line_start = next;
    }
    let open_end = src
        .get(pos..)
        .and_then(|r| r.find('\n'))
        .map_or(src.len(), |i| pos + i);
    diags.emit(
        Severity::Error,
        "E011",
        idx.span(pos, open_end),
        "front matter is not closed by a `---` line",
    );
    None
}

/// Handles the `%%{…}%%` directive starting at `pos` and returns the offset after it.
pub(crate) fn directive(
    idx: &LineIndex,
    pos: usize,
    meta: &mut Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> usize {
    let src = idx.src();
    let inner_start = pos + 3;
    let rest = src.get(inner_start..).unwrap_or("");
    let Some(close) = rest.find("}%%") else {
        let eol = rest.find('\n').map_or(src.len(), |i| inner_start + i);
        diags.emit(
            Severity::Warning,
            "W016",
            idx.span(pos, eol),
            "`%%{` directive is not closed by `}%%`; ignored",
        );
        return eol;
    };
    let inner = rest.get(..close).unwrap_or("");
    let end = inner_start + close + 3;
    let span = idx.span(pos, end);
    match directive::parse_directive(
        inner,
        inner_start,
        opts.limits.nesting,
        DIRECTIVE_STRING_BYTES,
    ) {
        Ok(d) if d.kind == "init" || d.kind == "initialize" => match d.value {
            Some(config::Value::Map(entries)) => config::apply_config(&entries, idx, meta, diags),
            _ => diags.emit(
                Severity::Warning,
                "W016",
                span,
                "`%%{init}%%` expects a JSON object; ignored",
            ),
        },
        Ok(d) => diags.emit(
            Severity::Warning,
            "W016",
            span,
            alloc::format!("directive `{}` is not supported; ignored", excerpt(&d.kind)),
        ),
        Err(directive::JsonError::TooLarge {
            start,
            end,
            message,
        }) => diags.emit(Severity::Error, "E012", idx.span(start, end), message),
        Err(directive::JsonError::Malformed {
            start,
            end,
            message,
        }) => diags.emit(
            Severity::Warning,
            "W016",
            idx.span(start, end),
            alloc::format!("malformed `%%{{init}}%%` directive ({}); ignored", message),
        ),
    }
    end
}
