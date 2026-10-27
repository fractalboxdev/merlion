//! Gantt acceptance through the public core API, shared by native and WASM callers.

use merlion_render::{check, outline, render, RenderError, RenderOptions, Severity};

const DELIVERY: &str = include_str!("fixtures/gantt/delivery.mmd");

#[test]
fn delivery_schedule_renders_through_the_public_api() {
    let out = render(
        DELIVERY,
        &RenderOptions {
            strict: true,
            ..Default::default()
        },
    );
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    let svg = out.svg.expect("static SVG");
    assert!(svg.contains("merlion-gantt"));
    assert!(svg.contains("role=\"img\""));
    assert!(svg.contains("<title"));
    assert!(svg.contains("<desc"));
    for id in ["api", "review", "release"] {
        assert!(svg.contains(&format!("data-merlion-id=\"{id}\"")), "{id}");
    }
    let text = out.outline.expect("accessible schedule");
    for label in [
        "Build",
        "API complete",
        "Review",
        "Release",
        "2026-10-01",
        "2026-10-08",
        "2026-10-13",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
    assert_eq!(outline(DELIVERY).unwrap(), text);
    assert!(!check(DELIVERY, true)
        .iter()
        .any(|d| d.severity == Severity::Error));
}

#[test]
fn repeated_and_self_hinted_renders_have_identical_bytes() {
    let opts = RenderOptions::default();
    let first = render(DELIVERY, &opts);
    assert_eq!(first.error, None, "{:?}", first.diagnostics);
    let svg = first.svg.unwrap();
    assert_eq!(render(DELIVERY, &opts).svg.as_deref(), Some(svg.as_str()));
    assert_eq!(
        render(
            DELIVERY,
            &RenderOptions {
                hint: Some(svg.clone()),
                ..opts
            }
        )
        .svg
        .as_deref(),
        Some(svg.as_str())
    );
}

#[test]
fn mandatory_gantt_work_respects_the_fuel_budget() {
    let out = render(
        DELIVERY,
        &RenderOptions {
            fuel: 0,
            ..Default::default()
        },
    );
    assert!(matches!(
        out.error,
        Some(RenderError::TooLarge { what: "fuel" })
    ));
    assert!(out.svg.is_none());
}

#[test]
fn unsupported_schedule_statements_have_source_locations() {
    let out = render(
        "gantt\n  unsupportedDirective value\n",
        &RenderOptions::default(),
    );
    assert!(out.svg.is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.span.line == 2 && d.span.column > 0),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn untrusted_titles_and_task_labels_remain_inert_svg_text() {
    let src = "gantt\n title <script>alert(1)</script> & schedule\n todayMarker off\n Task <img src=x onerror=alert(1)> :x, 2026-10-01, 1d\n";
    let out = render(src, &RenderOptions::default());
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    let svg = out.svg.unwrap();
    assert!(!svg.contains("<script>"));
    assert!(!svg.contains("<img"));
    assert!(!svg.contains("<foreignObject"));
    assert!(svg.contains("&lt;script&gt;"));
    assert!(svg.contains("&lt;img"));
}
