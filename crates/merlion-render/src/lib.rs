//! Merlion renders Mermaid diagrams to static, themeable, accessible SVG.
//!
//! Pipeline (specs/architecture.md): parse → measure → layout → draw. Every stage is a
//! pure function of its inputs; identical source, options and hint give byte-identical
//! SVG on every target.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod color;
pub mod diag;
pub mod fuel;
pub mod geometry;
pub mod ids;
pub mod json;
pub mod layout;
pub mod math;
pub mod model;
pub mod numfmt;
pub mod options;
pub mod parse;
pub mod stylesheet;
pub mod svg;
pub mod text;

use alloc::string::String;
use alloc::vec::Vec;

pub use diag::{Diagnostic, Diagnostics, Severity, Span};
pub use options::{Direction, DirectionOption, EdgeStyle, FontMode, Limits, RenderOptions};

use model::Diagram;

/// Crate version, written as `data-merlion-version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    UnsupportedDiagram {
        header: String,
    },
    /// Parsing failed; the diagnostics say why.
    Parse,
    /// A limit or mandatory-phase fuel was exceeded (CLI exit code 3).
    TooLarge {
        what: &'static str,
    },
}

#[derive(Clone, Debug)]
pub struct RenderResult {
    pub svg: Option<String>,
    pub outline: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<RenderError>,
    pub fuel_used: u64,
}

/// The `Error` diagnostic for a failure: `E002` parse, `E003` unsupported diagram,
/// `E004` a size limit or exhausted mandatory fuel (specs/parser.md#codes).
pub fn error_diagnostic(e: &RenderError) -> Diagnostic {
    let (code, message) = match e {
        RenderError::UnsupportedDiagram { header } if header.is_empty() => {
            ("E003", String::from("no supported diagram type found"))
        }
        RenderError::UnsupportedDiagram { header } => (
            "E003",
            alloc::format!("unsupported diagram type `{}`", diag::excerpt(header)),
        ),
        RenderError::TooLarge { what } => ("E004", alloc::format!("{} exceeds its limit", what)),
        RenderError::Parse => ("E002", String::from("the diagram failed to parse")),
    };
    Diagnostic {
        severity: Severity::Error,
        code,
        span: Span::default(),
        message,
        fix: None,
    }
}

/// Appends the diagnostic of `error` unless an `Error` diagnostic already explains it.
fn explain(diagnostics: &mut Vec<Diagnostic>, error: &RenderError) {
    if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        diagnostics.push(error_diagnostic(error));
    }
}

impl RenderResult {
    /// A failed result for an error found before the core runs (an input the caller
    /// refused to read, for example), with its diagnostic.
    pub fn from_error(error: RenderError) -> Self {
        Self::failed(error, Vec::new(), 0)
    }

    fn failed(error: RenderError, mut diagnostics: Vec<Diagnostic>, fuel_used: u64) -> Self {
        explain(&mut diagnostics, &error);
        RenderResult {
            svg: None,
            outline: None,
            diagnostics,
            error: Some(error),
            fuel_used,
        }
    }
}

fn parse_opts(opts: &RenderOptions) -> parse::ParseOptions {
    parse::ParseOptions {
        strict: opts.strict,
        limits: opts.limits,
    }
}

fn map_parse_error(e: parse::ParseError) -> RenderError {
    match e {
        parse::ParseError::UnsupportedDiagram { header } => {
            RenderError::UnsupportedDiagram { header }
        }
        parse::ParseError::Failed => RenderError::Parse,
        parse::ParseError::TooLarge { what } => RenderError::TooLarge { what },
    }
}

/// The SVG root id: the `id_prefix` when valid, else `m` + 8 hex of FNV-1a 64 over
/// (source, options, palette digest when a palette is given, layout). `layout` is the
/// drawn layout's hint string, not the input hint: re-rendering in place with the
/// previous SVG as the hint then reproduces the same bytes, because the same layout gives
/// the same id. With no palette nothing is hashed for it, so ids stay as they were.
pub fn diagram_id(source: &str, opts: &RenderOptions, layout: &str) -> String {
    if let Some(p) = &opts.id_prefix {
        if ids::is_valid_id_prefix(p) {
            return p.clone();
        }
    }
    let opt_key = alloc::format!(
        "{}|{}|{:?}|{:?}|{}|{}|{}|{}|{:?}|{}|{}|{}",
        opts.target_width,
        opts.max_aspect,
        opts.direction,
        opts.edge_style,
        opts.node_spacing,
        opts.rank_spacing,
        opts.stability,
        opts.fuel,
        opts.font,
        opts.font_size,
        opts.wrap_width,
        opts.background
    );
    let digest = opts
        .palette
        .as_ref()
        .map(|p| alloc::format!("{:016x}", p.digest()));
    match &digest {
        Some(d) => ids::default_id(ids::fnv1a64_parts(&[
            source.as_bytes(),
            opt_key.as_bytes(),
            d.as_bytes(),
            layout.as_bytes(),
        ])),
        None => ids::default_id(ids::fnv1a64_parts(&[
            source.as_bytes(),
            opt_key.as_bytes(),
            layout.as_bytes(),
        ])),
    }
}

/// Renders one diagram. Never panics; failures are reported in `error` and `diagnostics`.
pub fn render(source: &str, opts: &RenderOptions) -> RenderResult {
    let mut diags = Diagnostics::new(opts.strict);
    if source.len() > opts.limits.input_bytes {
        return RenderResult::failed(RenderError::TooLarge { what: "input" }, diags.items, 0);
    }
    let diagram = match parse::parse(source, &parse_opts(opts), &mut diags) {
        Ok(d) => d,
        Err(e) => return RenderResult::failed(map_parse_error(e), diags.items, 0),
    };
    if diags.has_errors() {
        return RenderResult::failed(RenderError::Parse, diags.items, 0);
    }
    let mut fuel = fuel::Fuel::new(opts.fuel);
    match &diagram {
        Diagram::Flowchart(chart) => {
            // Draw-time role work: one unit per class applied to an element and per
            // palette tone (specs/adr/0008-deterministic-work-budget.md).
            if fuel.burn(svg::role_units(chart, opts)).is_err() {
                return RenderResult::failed(
                    RenderError::TooLarge { what: "fuel" },
                    diags.items,
                    fuel.used(),
                );
            }
            let geom = match layout::layout_flowchart(chart, opts, &mut fuel, &mut diags) {
                Ok(g) => g,
                Err(layout::LayoutError::TooLarge { what }) => {
                    return RenderResult::failed(
                        RenderError::TooLarge { what },
                        diags.items,
                        fuel.used(),
                    )
                }
            };
            let id = diagram_id(source, opts, &svg::layout_hint(chart, &geom));
            let out = svg::draw_flowchart(chart, &geom, opts, &id, &mut diags);
            if diags.has_errors() {
                return RenderResult::failed(RenderError::Parse, diags.items, fuel.used());
            }
            RenderResult {
                svg: Some(out.svg),
                outline: Some(out.outline),
                diagnostics: diags.items,
                error: None,
                fuel_used: fuel.used(),
            }
        }
    }
}

/// Parses without rendering (`merlion check`).
pub fn check(source: &str, strict: bool) -> Vec<Diagnostic> {
    let opts = RenderOptions {
        strict,
        ..RenderOptions::default()
    };
    let mut diags = Diagnostics::new(strict);
    if source.len() > opts.limits.input_bytes {
        explain(&mut diags.items, &RenderError::TooLarge { what: "input" });
        return diags.items;
    }
    if let Err(e) = parse::parse(source, &parse_opts(&opts), &mut diags) {
        explain(&mut diags.items, &map_parse_error(e));
    }
    diags.items
}

/// Plain-text outline without layout (`merlion outline`).
pub fn outline(source: &str) -> Result<String, RenderError> {
    let opts = RenderOptions::default();
    let mut diags = Diagnostics::new(false);
    match parse::parse(source, &parse_opts(&opts), &mut diags).map_err(map_parse_error)? {
        Diagram::Flowchart(c) => Ok(svg::outline_flowchart(&c)),
    }
}
