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

/// Whether a message is safe to print to a terminal: no C0/C1 controls, no bidi
/// controls (specs/parser.md#diagnostics).
fn printable(m: &str) -> bool {
    !m.chars()
        .any(|c| c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
}

#[test]
fn diagnostics_never_carry_control_characters() {
    let sources = [
        "flowchart TD\n  A --> B\n  click A href \"x\" \u{1b}]0;pwned\u{7}\u{1b}[2J\u{1b}[31mRED\n",
        "flowchart TD\n  A --> B\n  style A fill:red\u{1b}[2Jx\n",
        "HOME=/home/runner\0GITHUB_TOKEN=ghs_x\0PATH=/usr/bin\0\n",
        "flowchart \u{1b}[2J\n  A --> B\n",
        "flowchart TD\n  A --> B\n  class A bad\u{7}name\n",
    ];
    for src in sources {
        let r = render(src, &RenderOptions::default());
        assert!(!r.diagnostics.is_empty(), "{:?}", src);
        for d in r.diagnostics.iter().chain(check(src, false).iter()) {
            assert!(printable(&d.message), "{} {:?}", d.code, d.message);
        }
        if let Some(RenderError::UnsupportedDiagram { header }) = &r.error {
            assert!(printable(header), "{:?}", header);
        }
    }
}

#[test]
fn quoted_source_excerpts_are_bounded() {
    let token = "x".repeat(1_000_000);
    let d = check(&token, false);
    assert_eq!(errors(&d), vec!["E003"]);
    assert!(d[0].message.len() < 200, "{} bytes", d[0].message.len());
    assert!(d[0].message.contains('…'));
    let r = render(&token, &RenderOptions::default());
    match &r.error {
        Some(RenderError::UnsupportedDiagram { header }) => assert!(header.len() < 200),
        e => panic!("{:?}", e),
    }
}
