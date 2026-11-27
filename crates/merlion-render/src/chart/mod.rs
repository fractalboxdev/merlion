//! Data charts from a JSON spec, `merlion-chart/1` (specs/chart.md).
//!
//! A chart is data, not diagram text: the spec carries rows of values and marks that
//! read named fields, so a number is written once and every mark, label, tooltip and
//! outline line drawn from it shows the same text. The pipeline mirrors the diagram
//! pipeline: read → resolve → layout → draw, each a pure function of its inputs.

pub mod decimal;
pub mod json;
pub mod layout;
pub mod resolve;
pub mod scale;
pub mod spec;

use alloc::vec::Vec;

use crate::diag::{Diagnostic, Diagnostics, Severity};
use crate::options::RenderOptions;
use crate::{RenderError, RenderResult};

/// Colours in the categorical palette, `--merlion-series-1` … `-8`.
pub const MAX_SERIES: usize = 8;
/// Categories on one band axis.
pub const MAX_CATEGORIES: usize = 500;

fn failed(error: RenderError, diags: Diagnostics, fuel: u64) -> RenderResult {
    let mut items = diags.items;
    if !items.iter().any(|d| d.severity == Severity::Error) {
        items.push(crate::error_diagnostic(&error));
    }
    RenderResult {
        svg: None,
        outline: None,
        diagnostics: items,
        error: Some(error),
        fuel_used: fuel,
    }
}

/// Renders one chart spec. Never panics; failures are reported in `error` and
/// `diagnostics`, every spec error with its JSON path and source position.
pub fn render(src: &str, opts: &RenderOptions) -> RenderResult {
    let mut diags = Diagnostics::with_limit(opts.strict, opts.limits.diagnostics);
    if src.len() > opts.limits.input_bytes {
        return failed(RenderError::TooLarge { what: "input" }, diags, 0);
    }
    let Some(spec) = spec::read(src, opts.limits.nesting, &mut diags) else {
        return failed(RenderError::Parse, diags, 0);
    };
    let mut fuel = crate::fuel::Fuel::new(opts.fuel);
    let resolved = match resolve::resolve(src, &spec, &mut diags, &mut fuel) {
        Ok(Some(resolved)) => resolved,
        Ok(None) => return failed(RenderError::Parse, diags, fuel.used()),
        Err(error) => return failed(error, diags, fuel.used()),
    };
    if diags.has_errors() {
        return failed(RenderError::Parse, diags, 0);
    }
    let geom = match layout::layout(&spec, &resolved, opts, &mut diags) {
        Ok(g) => g,
        Err(crate::layout::LayoutError::TooLarge { what }) => {
            return failed(RenderError::TooLarge { what }, diags, fuel.used())
        }
    };
    let id = crate::diagram_id(src, opts, "chart");
    let out = crate::svg::chart::draw_chart(&spec, &resolved, &geom, opts, &id, &mut diags);
    if diags.has_errors() {
        return failed(RenderError::Parse, diags, fuel.used());
    }
    RenderResult {
        svg: Some(out.svg),
        outline: Some(out.outline),
        diagnostics: diags.items,
        error: None,
        fuel_used: fuel.used(),
    }
}

/// Reads and resolves a spec without drawing it (`merlion check --chart`).
pub fn check(src: &str, strict: bool) -> Vec<Diagnostic> {
    let opts = RenderOptions {
        strict,
        ..RenderOptions::default()
    };
    let mut diags = Diagnostics::with_limit(strict, opts.limits.diagnostics);
    if src.len() > opts.limits.input_bytes {
        diags
            .items
            .push(crate::error_diagnostic(&RenderError::TooLarge {
                what: "input",
            }));
        return diags.items;
    }
    if let Some(spec) = spec::read(src, opts.limits.nesting, &mut diags) {
        let mut fuel = crate::fuel::Fuel::new(opts.fuel);
        if let Err(error) = resolve::resolve(src, &spec, &mut diags, &mut fuel) {
            diags.items.push(crate::error_diagnostic(&error));
        }
    }
    diags.items
}
