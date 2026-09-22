//! Every failed render carries an `Error` diagnostic that explains it, so the CLI, the
//! WASM module and any other caller report the same codes (specs/parser.md#codes).

use merlion_render::{
    check, error_diagnostic, render, Diagnostic, RenderError, RenderOptions, RenderResult, Severity,
};

fn errors(ds: &[Diagnostic]) -> Vec<&'static str> {
    ds.iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn unsupported_diagram_reports_e003() {
    let r = render("pie title x\n\"a\": 1\n", &RenderOptions::default());
    assert_eq!(
        r.error,
        Some(RenderError::UnsupportedDiagram {
            header: "pie".into()
        })
    );
    assert_eq!(errors(&r.diagnostics), vec!["E003"]);
    assert!(r.diagnostics[0].message.contains("`pie`"));
    assert_eq!(errors(&check("pie\n", false)), vec!["E003"]);
}

#[test]
fn oversized_input_reports_e004_on_render_and_check() {
    let src = "flowchart LR\n".to_string() + &"%% pad\n".repeat(200_000);
    let r = render(&src, &RenderOptions::default());
    assert_eq!(r.error, Some(RenderError::TooLarge { what: "input" }));
    assert_eq!(errors(&r.diagnostics), vec!["E004"]);
    let c = check(&src, false);
    assert_eq!(c, r.diagnostics);
}

#[test]
fn exhausted_fuel_reports_e004() {
    let src = "flowchart LR\n".to_string()
        + &(0..60)
            .map(|i| format!("n{i} --> n{}\n", (i * 7 + 3) % 60))
            .collect::<String>();
    let r = render(
        &src,
        &RenderOptions {
            fuel: 1,
            ..RenderOptions::default()
        },
    );
    assert!(
        matches!(r.error, Some(RenderError::TooLarge { .. })),
        "{:?}",
        r.error
    );
    assert_eq!(errors(&r.diagnostics), vec!["E004"]);
}

#[test]
fn a_syntax_error_is_explained_once() {
    let r = render("flowchart LR\nA -->\n", &RenderOptions::default());
    assert_eq!(r.error, Some(RenderError::Parse));
    assert_eq!(errors(&r.diagnostics), vec!["E002"]);
}

#[test]
fn from_error_carries_the_same_diagnostic() {
    let e = RenderError::TooLarge { what: "input" };
    let r = RenderResult::from_error(e.clone());
    assert_eq!(r.error, Some(e.clone()));
    assert_eq!(r.diagnostics, vec![error_diagnostic(&e)]);
    assert!(r.svg.is_none() && r.outline.is_none());
    assert_eq!(error_diagnostic(&RenderError::Parse).code, "E002");
    assert_eq!(
        error_diagnostic(&RenderError::UnsupportedDiagram {
            header: String::new()
        })
        .message,
        "no supported diagram type found"
    );
}

#[test]
fn successful_renders_gain_no_error() {
    let r = render("flowchart LR\nA --> B\n", &RenderOptions::default());
    assert!(r.error.is_none() && r.svg.is_some());
    assert!(errors(&r.diagnostics).is_empty());
}
