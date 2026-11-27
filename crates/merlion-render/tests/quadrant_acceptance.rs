//! Quadrant chart acceptance through the public core API, shared by native and WASM
//! callers.

use merlion_render::{check, outline, render, RenderOptions, Severity};

const FIXTURES: [(&str, &str); 4] = [
    ("campaigns", include_str!("fixtures/quadrant/campaigns.mmd")),
    (
        "priorities",
        include_str!("fixtures/quadrant/priorities.mmd"),
    ),
    ("styled", include_str!("fixtures/quadrant/styled.mmd")),
    ("theme", include_str!("fixtures/quadrant/theme.mmd")),
];

#[test]
fn every_fixture_renders_strictly_with_its_outline() {
    for (name, src) in FIXTURES {
        let out = render(
            src,
            &RenderOptions {
                strict: true,
                ..Default::default()
            },
        );
        assert_eq!(out.error, None, "{name}: {:?}", out.diagnostics);
        let svg = out.svg.unwrap();
        assert!(svg.contains("merlion-quadrant"), "{name}");
        assert!(svg.contains("role=\"img\""), "{name}");
        let text = out.outline.unwrap();
        assert!(text.starts_with("Quadrant chart."), "{name}: {text}");
        assert_eq!(outline(src).unwrap(), text, "{name}");
        assert!(
            !check(src, true)
                .iter()
                .any(|d| d.severity == Severity::Error),
            "{name}"
        );
    }
}

#[test]
fn out_of_range_points_fail_with_a_located_error() {
    let out = render(
        "quadrantChart\n  Point: [1.5, 0.5]\n",
        &RenderOptions::default(),
    );
    assert!(out.svg.is_none());
    let e = out.diagnostics.iter().find(|d| d.code == "E024").unwrap();
    assert_eq!((e.severity, e.span.line), (Severity::Error, 2));
}
