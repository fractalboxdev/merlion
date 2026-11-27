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

#[test]
fn line_and_area_points_keep_raw_channels_rows_and_tooltips() {
    for (kind, secondary) in [("line", ""), ("area", ""), ("area", r#", "y2":"lo""#)] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"linear"}},"y":{{"type":"linear"}},"data":[{{"x":"2.00","v":"-2.40","lo":"-3.40","s":"Example"}},{{"x":"1.00","v":"-3.15","lo":"-4.15","s":"Example"}}],"marks":[{{"type":"{kind}","x":"x","y":"v","series":"s"{secondary}}}]}}"#
        );
        let out = render_chart(&src, &strict());
        assert_eq!(out.error, None, "{:?}", out.diagnostics);
        let svg = out.svg.unwrap();
        assert!(svg.contains("data-merlion-points=\"2\""));
        let points: Vec<_> = svg
            .split("<g class=\"merlion-chart-point\"")
            .skip(1)
            .map(|p| p.split("</g>").next().unwrap())
            .collect();
        assert_eq!(points.len(), 2, "{kind}{secondary}");
        // The path sorts by x while each point keeps its original row identity.
        assert!(points[0].contains("data-merlion-row=\"1\""));
        for (row, x, y, y2) in [(0, "2.00", "-2.40", "-3.40"), (1, "1.00", "-3.15", "-4.15")] {
            let point = points
                .iter()
                .find(|p| p.contains(&format!("data-merlion-row=\"{row}\"")))
                .unwrap();
            assert!(point.contains(&format!("data-merlion-x=\"{x}\"")));
            assert!(point.contains(&format!("data-merlion-y=\"{y}\"")));
            assert!(point.contains("data-merlion-series=\"Example\""));
            let title = point
                .split("<title>")
                .nth(1)
                .unwrap()
                .split("</title>")
                .next()
                .unwrap();
            assert!(
                title.contains("Example") && title.contains(x) && title.contains(y),
                "{title}"
            );
            if !secondary.is_empty() {
                assert!(point.contains(&format!("data-merlion-y2=\"{y2}\"")));
                assert!(title.contains(y2));
                assert!(out.outline.as_ref().unwrap().contains(y2));
            }
            assert!(out.outline.as_ref().unwrap().contains(y));
        }
    }
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

#[test]
fn authored_domains_include_implicit_zero_baselines() {
    for (x, y, mark) in [
        (
            r#"{"type":"band"}"#,
            r#"{"type":"linear","domain":[5,10]}"#,
            r#"{"type":"bar","x":"a","y":"v"}"#,
        ),
        (
            r#"{"type":"linear","domain":[-10,-5]}"#,
            r#"{"type":"band"}"#,
            r#"{"type":"bar","x":"v","y":"a"}"#,
        ),
        (
            r#"{"type":"band"}"#,
            r#"{"type":"linear","domain":[5,10]}"#,
            r#"{"type":"area","x":"a","y":"v"}"#,
        ),
    ] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{x},"y":{y},"data":[{{"a":"x","v":7}}],"marks":[{mark}]}}"#
        );
        let d = check_chart(&src, false);
        assert!(
            d.iter().any(|d| d.code == "E022"
                && d.message.contains("domain")
                && d.message.contains("include zero")),
            "{d:?}"
        );
        assert!(render_chart(&src, &strict()).svg.is_none());
    }
    for mark in [
        r#"{"type":"line","x":"a","y":"v"}"#,
        r#"{"type":"area","x":"a","y":"v","y2":"lo"}"#,
        r#"{"type":"dot","x":"a","y":"v"}"#,
    ] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear","domain":[5,10]}},"data":[{{"a":"x","v":7,"lo":6}}],"marks":[{mark}]}}"#
        );
        assert_eq!(render_chart(&src, &strict()).error, None, "{mark}");
    }
}

#[test]
fn extreme_numeric_values_render_inside_the_svg() {
    for values in [
        "1e100,2e100",
        "-2e100,-1e100",
        "1e-100,2e-100",
        "-2e-300,-1e-300",
        "-1.7976931348623157e308,1.7976931348623157e308",
        "5e-324,1e-323",
        "100000000000000000000,100000000000000016384",
    ] {
        let rows: Vec<_> = values
            .split(',')
            .enumerate()
            .map(|(i, value)| format!(r#"{{"x":{i},"v":{value}}}"#))
            .collect();
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"linear"}},"y":{{"type":"linear"}},"data":[{}],"marks":[{{"type":"dot","x":"x","y":"v"}}]}}"#,
            rows.join(",")
        );
        let out = render_chart(&src, &strict());
        assert_eq!(out.error, None, "{values}: {:?}", out.diagnostics);
        let svg = out.svg.unwrap();
        let viewbox = svg
            .split("viewBox=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let dimensions: Vec<f64> = viewbox
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        let circles: Vec<_> = svg.split("<circle ").skip(1).collect();
        assert_eq!(circles.len(), 2, "{values}");
        for circle in circles {
            for (attr, bound) in [("cx", dimensions[2]), ("cy", dimensions[3])] {
                let coordinate: f64 = circle
                    .split(&format!("{attr}=\""))
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(
                    (0.0..=bound).contains(&coordinate),
                    "{values}: {attr}={coordinate}"
                );
            }
        }
    }
    for value in ["1e309", "-1e309", "1e-999", "-1e-999"] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"linear"}},"y":{{"type":"linear"}},"data":[{{"x":0,"v":{value}}}],"marks":[{{"type":"dot","x":"x","y":"v"}}]}}"#
        );
        assert_eq!(codes(&src), ["E022"], "{value}");
        assert!(render_chart(&src, &strict()).svg.is_none());
    }
}

#[test]
fn skipped_rows_do_not_register_or_validate_series() {
    let mut rows: Vec<_> = (0..8)
        .map(|i| format!(r#"{{"a":"x","v":null,"s":"skip{i}"}}"#))
        .collect();
    rows.push(String::from(r#"{"a":"x","v":7,"s":"drawn"}"#));
    for domain in ["", r#", "series":{"domain":["drawn"]}"#] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear"}},"data":[{}],"marks":[{{"type":"bar","x":"a","y":"v","series":"s"}}]{domain}}}"#,
            rows.join(",")
        );
        let out = render_chart(&src, &strict());
        assert_eq!(out.error, None, "{:?}", out.diagnostics);
        let skipped: Vec<_> = out
            .diagnostics
            .iter()
            .filter(|d| d.code == "I041")
            .collect();
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].message.contains("8 of 9 rows"));
        let svg = out.svg.unwrap();
        assert_eq!(svg.matches("class=\"merlion-chart-item ").count(), 1);
        assert!(svg.contains("data-merlion-series=\"drawn\""));
        assert!(!svg.contains("merlion-chart-legend"));
        assert!(!svg.contains("data-merlion-series=\"skip"));
    }
}

#[test]
fn bound_labels_skip_missing_and_null_cells() {
    for absent in [r#", "label":null"#, ""] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear"}},"data":[{{"a":"x","v":1,"label":"one","s":"drawn"}},{{"a":"y","v":2,"s":"skipped"{absent}}}],"marks":[{{"type":"dot","x":"a","y":"v","label":"label","series":"s"}}]}}"#
        );
        let out = render_chart(&src, &strict());
        assert_eq!(out.error, None, "{:?}", out.diagnostics);
        let skipped: Vec<_> = out
            .diagnostics
            .iter()
            .filter(|d| d.code == "I041")
            .collect();
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].message.contains("1 of 2 rows"));
        let svg = out.svg.unwrap();
        assert_eq!(svg.matches("class=\"merlion-chart-item ").count(), 1);
        assert!(svg.contains("data-merlion-row=\"0\""));
        assert!(!svg.contains("data-merlion-row=\"1\""));
        assert!(!svg.contains("merlion-chart-legend"));
    }
}

#[test]
fn field_diagnostics_escape_json_pointer_segments() {
    for (field, escaped) in [("a/b", "a~1b"), ("a~b", "a~0b"), ("a~/b", "a~0~1b")] {
        for (value, series, mark) in [
            (
                r#""bad""#,
                "",
                format!(r#"{{"type":"dot","x":"x","y":"{field}"}}"#),
            ),
            (
                r#""bad""#,
                r#", "series":{"domain":["good"]}"#,
                format!(r#"{{"type":"dot","x":"x","y":"v","series":"{field}"}}"#),
            ),
        ] {
            let src = format!(
                r#"{{"chart":1,"title":"t","x":{{"type":"linear"}},"y":{{"type":"linear"}},"data":[{{"x":0,"v":1,"{field}":{value}}}],"marks":[{mark}]{series}}}"#
            );
            let diagnostics = check_chart(&src, false);
            let error = diagnostics.iter().find(|d| d.code == "E022").unwrap();
            assert!(
                error.message.starts_with(&format!("`/data/0/{escaped}`:")),
                "{}",
                error.message
            );
            assert_eq!(
                &src[error.span.byte_start as usize..error.span.byte_end as usize],
                value
            );
        }
    }
}

#[test]
fn fuel_charges_each_drawn_item_and_mark_once() {
    for (data, marks, required) in [
        (
            r#"[{"a":"x","v":1},{"a":"y","v":2}]"#,
            r#"[{"type":"bar","x":"a","y":"v"}]"#,
            3,
        ),
        (
            r#"[{"a":"x","v":null},{"a":"y","v":null}]"#,
            r#"[{"type":"bar","x":"a","y":"v"}]"#,
            1,
        ),
        (
            r#"[{"a":"x","v":1},{"a":"y","v":2}]"#,
            r#"[{"type":"rule","y":{"value":1}}]"#,
            2,
        ),
        (
            r#"[{"a":"x","v":1},{"a":"y","v":2}]"#,
            r#"[{"type":"bar","x":"a","y":"v"},{"type":"rule","y":{"value":1}}]"#,
            5,
        ),
        (
            r#"[{"a":"x","v":1,"z":null},{"a":"y","v":2,"z":null}]"#,
            r#"[{"type":"bar","x":"a","y":"v"},{"type":"dot","x":"a","y":"z"}]"#,
            4,
        ),
        (
            r#"[{"a":"x","v":1,"label":"one"},{"a":"y","v":2,"label":null}]"#,
            r#"[{"type":"dot","x":"a","y":"v","label":"label"}]"#,
            2,
        ),
        (
            r#"[{"a":"x","v":1},{"a":"y","v":2}]"#,
            r#"[{"type":"line","x":"a","y":"v"}]"#,
            3,
        ),
    ] {
        let src = format!(
            r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear"}},"data":{data},"marks":{marks}}}"#
        );
        let out = render_chart(
            &src,
            &RenderOptions {
                fuel: required,
                ..strict()
            },
        );
        assert_eq!(out.error, None, "{marks}: {:?}", out.diagnostics);
        assert_eq!(out.fuel_used, required, "{marks}");
        let out = render_chart(
            &src,
            &RenderOptions {
                fuel: required - 1,
                ..strict()
            },
        );
        assert!(
            matches!(
                out.error,
                Some(merlion_render::RenderError::TooLarge { what: "fuel" })
            ),
            "{marks}: {:?}",
            out.error
        );
        assert_eq!(out.fuel_used, required - 1);
        assert!(out.svg.is_none());
    }
}

#[test]
fn constant_source_text_is_shared_after_fuel_preflight() {
    use merlion_render::{
        chart::{layout, resolve, spec},
        diag::Diagnostics,
        fuel::Fuel,
    };
    use std::rc::Rc;

    let constant = "a".repeat(4_096);
    let rows = vec![r#"{"v":1}"#; 256].join(",");
    let src = format!(
        r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear"}},"data":[{rows}],"marks":[{{"type":"dot","x":{{"value":"{constant}"}},"y":"v"}}]}}"#
    );
    let out = render_chart(
        &src,
        &RenderOptions {
            fuel: 0,
            ..strict()
        },
    );
    assert!(matches!(
        out.error,
        Some(merlion_render::RenderError::TooLarge { what: "fuel" })
    ));
    assert_eq!(out.fuel_used, 0);
    let mut diagnostics = Diagnostics::new(true);
    let spec = spec::read(&src, 64, &mut diagnostics).unwrap();
    let mut fuel = Fuel::new(257);
    let resolved = resolve::resolve(&src, &spec, &mut diagnostics, &mut fuel)
        .unwrap()
        .unwrap();
    assert_eq!(fuel.used(), 257);
    let first = &resolved.marks[0].items[0].x.as_ref().unwrap().raw;
    assert_eq!(first.as_ref(), constant);
    assert!(Rc::ptr_eq(first, &resolved.x.categories[0]));
    for item in &resolved.marks[0].items {
        assert!(Rc::ptr_eq(first, &item.x.as_ref().unwrap().raw));
    }
    let geometry = layout::layout(&spec, &resolved, &strict(), &mut diagnostics).unwrap();
    for item in &geometry.marks[0].items {
        let value = &item
            .data
            .iter()
            .find(|(k, _)| *k == "data-merlion-x")
            .unwrap()
            .1;
        assert!(Rc::ptr_eq(first, value));
        assert!(Rc::ptr_eq(first, &item.tip[0]));
    }
}

#[test]
fn repeated_source_text_is_bounded_before_layout_and_serialization() {
    let constant = "a".repeat(4_096);
    let rows = vec![r#"{"v":1}"#; 1_024].join(",");
    let src = format!(
        r#"{{"chart":1,"title":"t","x":{{"type":"band"}},"y":{{"type":"linear"}},"data":[{rows}],"marks":[{{"type":"dot","x":{{"value":"{constant}"}},"y":"v"}}]}}"#
    );
    let out = render_chart(&src, &strict());
    assert!(matches!(
        out.error,
        Some(merlion_render::RenderError::TooLarge { what: "chart text" })
    ));
    assert!(out.svg.is_none());
    assert!(out.outline.is_none());
    assert!(out.diagnostics.iter().any(|d| d.code == "E004"));
    assert!(check_chart(&src, true).iter().any(|d| d.code == "E004"));
}

#[test]
fn adversarial_diagnostics_keep_the_cap_count_and_source_positions() {
    let unknown: Vec<_> = (0..50).map(|i| format!(r#""é{i}":{i}"#)).collect();
    let src = format!(
        r#"{{{},"chart":1,"title":"é😀","x":{{"type":"linear"}},"y":{{"type":"linear"}},"data":[{{"x":0,"v":1}}],"marks":[{{"type":"dot","x":"x","y":"v"}}]}}"#,
        unknown.join(",")
    );
    let mut opts = strict();
    opts.fuel = 0;
    opts.limits.diagnostics = 2;
    let out = render_chart(&src, &opts);
    assert!(out.svg.is_none());
    assert_eq!(out.diagnostics.len(), 2);
    assert_eq!(out.diagnostics[0].code, "E021");
    assert!(out.diagnostics[0].message.starts_with("`/é0`:"));
    assert_eq!(out.diagnostics[1].code, "I034");
    assert!(out.diagnostics[1]
        .message
        .starts_with("49 further diagnostics"));
    for diagnostic in &out.diagnostics {
        let start = diagnostic.span.byte_start as usize;
        assert_eq!(
            diagnostic.span.column as usize,
            src[..start].chars().count() + 1
        );
    }

    let rows = vec![r#"{"x":0,"v":"bad"}"#; 1_000].join(",");
    let src = format!(
        r#"{{"chart":1,"title":"é😀","x":{{"type":"linear"}},"y":{{"type":"linear"}},"data":[{rows}],"marks":[{{"type":"dot","x":"x","y":"v"}}]}}"#
    );
    opts.fuel = 1_001;
    opts.limits.diagnostics = 8;
    let out = render_chart(&src, &opts);
    assert!(out.svg.is_none());
    assert_eq!(out.diagnostics.len(), 8);
    assert_eq!(out.diagnostics[6].code, "E022");
    assert!(out.diagnostics[6].message.starts_with("`/data/6/v`:"));
    assert_eq!(out.diagnostics[7].code, "I034");
    assert!(out.diagnostics[7]
        .message
        .starts_with("993 further diagnostics"));
    for diagnostic in &out.diagnostics {
        let start = diagnostic.span.byte_start as usize;
        assert_eq!(
            diagnostic.span.column as usize,
            src[..start].chars().count() + 1
        );
        assert_eq!(&src[start..diagnostic.span.byte_end as usize], r#""bad""#);
    }
}
