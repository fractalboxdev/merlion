//! Chart acceptance through the public core API (specs/chart.md#validation).

use merlion_render::{check_chart, render_chart, RenderOptions, Severity};

const FIXTURES: [(&str, &str); 6] = [
    (
        "ranked-bars",
        include_str!("fixtures/chart/ranked-bars.json"),
    ),
    ("dumbbell", include_str!("fixtures/chart/dumbbell.json")),
    ("ranges", include_str!("fixtures/chart/ranges.json")),
    ("time-lines", include_str!("fixtures/chart/time-lines.json")),
    (
        "grouped-bars",
        include_str!("fixtures/chart/grouped-bars.json"),
    ),
    ("area-range", include_str!("fixtures/chart/area-range.json")),
];

fn strict() -> RenderOptions {
    RenderOptions {
        strict: true,
        ..Default::default()
    }
}

/// Writes every fixture's SVG to `$MERLION_CHART_OUT` for visual review.
#[test]
fn fixtures_render_strictly() {
    let out_dir = std::env::var_os("MERLION_CHART_OUT");
    for (name, src) in FIXTURES {
        let out = render_chart(src, &strict());
        assert_eq!(out.error, None, "{name}: {:?}", out.diagnostics);
        let svg = out.svg.expect("svg");
        assert!(
            svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
            "{name}"
        );
        assert!(svg.contains("class=\"merlion merlion-chart\""), "{name}");
        assert!(svg.contains("role=\"img\""), "{name}");
        assert!(!svg.contains("<script"), "{name}");
        assert!(
            check_chart(src, true)
                .iter()
                .all(|d| d.severity != Severity::Error),
            "{name}"
        );
        if let Some(dir) = &out_dir {
            let path = std::path::Path::new(dir).join(format!("{name}.svg"));
            std::fs::write(path, &svg).unwrap();
        }
    }
}

#[test]
fn renders_are_byte_identical() {
    for (name, src) in FIXTURES {
        let a = render_chart(src, &RenderOptions::default()).svg;
        let b = render_chart(src, &RenderOptions::default()).svg;
        assert_eq!(a, b, "{name}");
    }
}

/// Every drawn value carries the exact text it was written as.
#[test]
fn marks_carry_their_source_values() {
    let out = render_chart(FIXTURES[0].1, &strict());
    let svg = out.svg.unwrap();
    for v in ["-1.8", "-2.40", "-0.6", "0.9", "-6.25", "-3.1"] {
        assert!(svg.contains(&format!("data-merlion-x=\"{v}\"")), "{v}");
    }
    for region in ["North", "South", "Islands"] {
        assert!(
            svg.contains(&format!("data-merlion-y=\"{region}\"")),
            "{region}"
        );
    }
    let outline = out.outline.unwrap();
    assert!(outline.contains("region | change"), "{outline}");
    assert!(outline.contains("South | -6.25"), "{outline}");
}

/// Bars start at zero: the zero line sits inside the plot when values change sign.
#[test]
fn bar_value_axis_includes_zero() {
    let svg = render_chart(FIXTURES[0].1, &strict()).svg.unwrap();
    assert!(svg.contains("class=\"merlion-chart-zero\""));
    assert!(svg.contains("data-merlion-value=\"0%\""));
}

#[test]
fn two_series_get_a_legend_and_one_series_none() {
    let grouped = render_chart(FIXTURES[4].1, &strict()).svg.unwrap();
    assert!(grouped.contains("merlion-chart-legend"));
    assert!(grouped.contains("data-merlion-series=\"Plan\""));
    let single = render_chart(FIXTURES[0].1, &strict()).svg.unwrap();
    assert!(!single.contains("merlion-chart-legend"));
}

fn codes(src: &str) -> Vec<&'static str> {
    check_chart(src, false)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn spec_errors_name_their_json_path() {
    let bad = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear"},
        "data":[{"a":"x","v":"1"}],"marks":[{"type":"bar","x":"a","y":"v","colour":"red"}]}"#;
    let d = check_chart(bad, false);
    let e = d.iter().find(|d| d.code == "E021").expect("E021");
    assert!(e.message.contains("/marks/0/colour"), "{}", e.message);
    assert_eq!(e.span.line, 2);

    assert_eq!(codes("{\"chart\":1,"), ["E020"]);
    let thousands = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear"},
        "data":[{"a":"x","v":"6,538.76"}],"marks":[{"type":"bar","x":"a","y":"v"}]}"#;
    assert_eq!(codes(thousands), ["E022"]);
    let typo = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear"},
        "data":[{"a":"x","v":"1"}],"marks":[{"type":"bar","x":"a","y":"vv"}]}"#;
    let d = check_chart(typo, false);
    assert!(d
        .iter()
        .any(|d| d.code == "E021" && d.message.contains("no row has the field `vv`")));
}

#[test]
fn nine_series_are_refused() {
    let rows: Vec<String> = (0..9)
        .map(|i| format!(r#"{{"k":"c","s":"s{i}","v":{i}}}"#))
        .collect();
    let src = format!(
        r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear"}},"data":[{}],"marks":[{{"type":"bar","x":"k","y":"v","series":"s"}}]}}"#,
        rows.join(",")
    );
    assert_eq!(codes(&src), ["E023"]);
}

#[test]
fn bars_refuse_a_non_zero_axis_and_out_of_domain_values() {
    let no_zero = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear","zero":false},
        "data":[{"a":"x","v":"1"}],"marks":[{"type":"bar","x":"a","y":"v"}]}"#;
    assert_eq!(codes(no_zero), ["E021"]);
    let outside = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear","domain":["0","5"]},
        "data":[{"a":"x","v":"7.2"}],"marks":[{"type":"bar","x":"a","y":"v"}]}"#;
    assert_eq!(codes(outside), ["E022"]);
}

#[test]
fn null_cells_skip_rows_with_an_info() {
    let src = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear"},
        "data":[{"a":"x","v":"1"},{"a":"y","v":null}],"marks":[{"type":"bar","x":"a","y":"v"}]}"#;
    let out = render_chart(src, &RenderOptions::default());
    assert_eq!(out.error, None);
    assert!(out.diagnostics.iter().any(|d| d.code == "I041"));
    assert_eq!(out.svg.unwrap().matches("merlion-chart-item").count(), 1);
}
