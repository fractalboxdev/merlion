//! Chart acceptance through the public core API (specs/chart.md#validation).

use merlion_render::{check_chart, render_chart, RenderOptions, Severity};

const FIXTURES: [(&str, &str); 9] = [
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
    (
        "stacked-area",
        include_str!("fixtures/chart/stacked-area.json"),
    ),
    (
        "stacked-bars",
        include_str!("fixtures/chart/stacked-bars.json"),
    ),
    (
        "stacked-signs",
        include_str!("fixtures/chart/stacked-signs.json"),
    ),
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
        // A stacked item costs a second unit for its running sum and total.
        (
            r#"[{"a":"x","v":1,"s":"p"},{"a":"x","v":2,"s":"q"},{"a":"y","v":null,"s":"p"}]"#,
            r#"[{"type":"bar","x":"a","y":"v","series":"s","stack":true,"total":true}]"#,
            5,
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

/// The rows of a fixture, read with the core's own spec reader.
fn rows(src: &str) -> Vec<merlion_render::chart::spec::Row> {
    let mut diagnostics = merlion_render::Diagnostics::new(true);
    merlion_render::chart::spec::read(src, 64, &mut diagnostics)
        .expect("spec")
        .data
}

fn cell(row: &merlion_render::chart::spec::Row, field: &str) -> String {
    use merlion_render::chart::spec::Cell;
    match row.get(field).map(|c| c.0) {
        Some(Cell::Str(s)) | Some(Cell::Num(s)) => s.clone(),
        _ => String::from("-"),
    }
}

/// `name="value"` inside one element's markup.
fn attr<'a>(markup: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let rest = &markup[markup.find(&key)? + key.len()..];
    rest.split('"').next()
}

fn title(markup: &str) -> &str {
    markup
        .split("<title>")
        .nth(1)
        .and_then(|t| t.split("</title>").next())
        .unwrap_or("")
}

fn fixture(name: &str) -> &'static str {
    FIXTURES.iter().find(|(n, _)| *n == name).unwrap().1
}

/// A stacked segment's attributes, tooltip and outline line carry its own value as
/// written, never the running total it ends at.
#[test]
fn stacked_segments_carry_their_own_values() {
    let src = fixture("stacked-bars");
    let out = render_chart(src, &strict());
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    let svg = out.svg.unwrap();
    let rows = rows(src);
    let items: Vec<_> = svg
        .split("<g class=\"merlion-chart-item ")
        .skip(1)
        .map(|g| g.split("</g>").next().unwrap())
        .collect();
    assert_eq!(items.len(), rows.len());
    for item in items {
        let row: usize = attr(item, "data-merlion-row").unwrap().parse().unwrap();
        let (team, hours, activity) = (
            cell(&rows[row], "team"),
            cell(&rows[row], "hours"),
            cell(&rows[row], "activity"),
        );
        assert_eq!(attr(item, "data-merlion-x"), Some(hours.as_str()));
        assert_eq!(attr(item, "data-merlion-y"), Some(team.as_str()));
        assert_eq!(attr(item, "data-merlion-series"), Some(activity.as_str()));
        assert_eq!(title(item), format!("{activity}: {hours}, {team}"));
    }
    let outline = out.outline.unwrap();
    assert!(outline.contains("team | hours | activity"), "{outline}");
    for row in &rows {
        let line = format!(
            "{} | {} | {}",
            cell(row, "team"),
            cell(row, "hours"),
            cell(row, "activity")
        );
        assert!(outline.lines().any(|l| l == line), "{line}");
    }
    // Totals are exact sums at the mark's widest written precision.
    assert!(
        outline.ends_with(
            "team | total\nPlatform | 95.25\nPayments | 95.50\nMobile | 78.00\nData | 62.25"
        ),
        "{outline}"
    );
    for total in ["95.25", "95.50", "78.00", "62.25"] {
        assert!(svg.contains(&format!(">{total}</tspan>")), "{total}");
    }
    // A running total never appears as an item value: Platform's Design + Build ends at
    // 60.5, Test at 76.75.
    for running in ["60.5", "76.75"] {
        assert!(!svg.contains(&format!("=\"{running}\"")), "{running}");
        assert!(!svg.contains(&format!(": {running},")), "{running}");
    }
    assert!(svg.contains("<desc id=\"") && svg.contains(">stacked bar chart, "));
}

#[test]
fn stacked_area_points_carry_their_own_values() {
    let src = fixture("stacked-area");
    let out = render_chart(src, &strict());
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    let svg = out.svg.unwrap();
    let rows = rows(src);
    assert_eq!(svg.matches("class=\"merlion-chart-layer\"").count(), 6);
    assert_eq!(svg.matches("data-merlion-points=\"37\"").count(), 7);
    assert!(svg.contains("stroke-dasharray=\"5 3\""));
    assert!(svg.contains(".merlion-chart-layer"));
    let points: Vec<_> = svg
        .split("<g class=\"merlion-chart-point\"")
        .skip(1)
        .map(|p| p.split("</g>").next().unwrap())
        .collect();
    assert_eq!(points.len(), 6 * 37 + 37);
    for point in points {
        let row: usize = attr(point, "data-merlion-row").unwrap().parse().unwrap();
        let value = match rows[row].get("cost") {
            Some(_) => cell(&rows[row], "cost"),
            None => cell(&rows[row], "budget"),
        };
        assert_eq!(attr(point, "data-merlion-y"), Some(value.as_str()));
        assert!(title(point).ends_with(&format!(", {value}")), "{point}");
    }
    let outline = out.outline.unwrap();
    assert!(
        outline.contains("\n2026-01 | 46.5 | Rent | - | -\n"),
        "{outline}"
    );
}

/// Segments of one stack touch at exact sums, 2 px apart, and only the outermost segment
/// on each side of zero has a rounded end.
#[test]
fn stack_geometry_follows_series_order_and_signs() {
    use merlion_render::chart::{layout, resolve, spec};
    for (name, horizontal) in [("stacked-bars", true), ("stacked-signs", false)] {
        let src = fixture(name);
        let mut diagnostics = merlion_render::Diagnostics::new(true);
        let spec = spec::read(src, 64, &mut diagnostics).unwrap();
        let mut fuel = merlion_render::fuel::Fuel::new(u64::MAX);
        let resolved = resolve::resolve(src, &spec, &mut diagnostics, &mut fuel)
            .unwrap()
            .unwrap();
        let geometry = layout::layout(&spec, &resolved, &strict(), &mut diagnostics).unwrap();
        let mark = &geometry.marks[0];
        // The zero line runs across the value axis: (x1, y1, x2, y2).
        let (zx, zy, _, _) = geometry.zero.unwrap();
        let zero = if horizontal { zx } else { zy };
        let categories = if horizontal {
            &resolved.y.categories
        } else {
            &resolved.x.categories
        };
        for (c, category) in categories.iter().enumerate() {
            for positive in [true, false] {
                // This category's segments on one side of zero, in series order.
                let mut segments: Vec<_> = resolved.marks[0]
                    .items
                    .iter()
                    .zip(&mark.items)
                    .filter(|(it, _)| {
                        let key = if horizontal { &it.y } else { &it.x };
                        let value = if horizontal { &it.x } else { &it.y };
                        key.as_ref().unwrap().raw == *category
                            && (value.as_ref().unwrap().pos.num() >= 0.0) == positive
                    })
                    .collect();
                segments.sort_by_key(|(it, _)| it.series);
                // (near end, far end, rounded) in px, outward from zero.
                let ends: Vec<(f64, f64, bool)> = segments
                    .iter()
                    .map(|(_, g)| {
                        let layout::Shape::Bar { rect, round, .. } = g.shape else {
                            panic!("bar")
                        };
                        let (near, far) = match (horizontal, positive) {
                            (true, true) => (rect.x, rect.x + rect.w),
                            (true, false) => (rect.x + rect.w, rect.x),
                            (false, true) => (rect.y + rect.h, rect.y),
                            (false, false) => (rect.y, rect.y + rect.h),
                        };
                        (near, far, round)
                    })
                    .collect();
                let Some(first) = ends.first() else { continue };
                assert!(
                    (first.0 - zero).abs() < 1e-9,
                    "{name} {category}: {first:?}"
                );
                for w in ends.windows(2) {
                    assert!(
                        ((w[1].0 - w[0].1).abs() - 2.0).abs() < 1e-9,
                        "{name} {category}: {w:?}"
                    );
                    assert!(!w[0].2, "{name} {category}: inner segment rounded");
                }
                assert!(
                    ends.last().unwrap().2,
                    "{name} {category}: outer end square"
                );
                let total = &resolved.marks[0].totals[c];
                assert_eq!(total.category, c);
                if total.negative != positive {
                    let label = &mark.totals[c];
                    let far = ends.last().unwrap().1;
                    let (w, h) = (label.label.width / 2.0, label.label.height / 2.0);
                    let clearance = match (horizontal, positive) {
                        (true, true) => label.x - w - far,
                        (true, false) => far - (label.x + w),
                        (false, true) => far - (label.y + h),
                        (false, false) => label.y - h - far,
                    };
                    assert!(
                        clearance > 0.0,
                        "{name} {category}: total overlaps its stack"
                    );
                }
            }
        }
    }
    let signs = render_chart(fixture("stacked-signs"), &strict());
    let outline = signs.outline.unwrap();
    assert!(
        outline.ends_with("quarter | total\nQ1 | 1.35\nQ2 | 0.90\nQ3 | -2.30\nQ4 | 2.70"),
        "{outline}"
    );
}

/// Stacks follow `series.domain`, sum exactly against an authored bound, and widen a
/// computed domain to their extent.
#[test]
fn stacks_sum_exactly_in_series_order() {
    let src = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear","domain":["0","0.3"]},
        "data":[{"a":"x","s":"B","v":"0.2"},{"a":"x","s":"A","v":"0.1"}],"series":{"domain":["A","B"]},
        "marks":[{"type":"bar","x":"a","y":"v","series":"s","stack":true,"total":true}]}"#;
    let out = render_chart(src, &strict());
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    assert!(out.outline.unwrap().ends_with("a | total\nx | 0.3"));
    let mut diagnostics = merlion_render::Diagnostics::new(true);
    let spec = merlion_render::chart::spec::read(src, 64, &mut diagnostics).unwrap();
    let mut fuel = merlion_render::fuel::Fuel::new(u64::MAX);
    let resolved = merlion_render::chart::resolve::resolve(src, &spec, &mut diagnostics, &mut fuel)
        .unwrap()
        .unwrap();
    let items = &resolved.marks[0].items;
    let segment = |row: usize| {
        items
            .iter()
            .find(|i| i.row == Some(row))
            .unwrap()
            .segment
            .unwrap()
    };
    assert_eq!((segment(1).start, segment(1).end), (0.0, 0.1));
    assert_eq!((segment(0).start, segment(0).end), (0.1, 0.3));
    assert_ne!(0.1 + 0.2, 0.3);

    let src = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear"},
        "data":[{"a":"x","s":"p","v":6},{"a":"x","s":"q","v":5},{"a":"y","s":"p","v":0},{"a":"y","s":"q","v":-3}],
        "marks":[{"type":"bar","x":"a","y":"v","series":"s","stack":true,"total":true}]}"#;
    let out = render_chart(src, &strict());
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    assert!(out.outline.unwrap().ends_with("a | total\nx | 11\ny | -3"));
    // A computed domain spans the stacks, so neither end is clamped to the plot edge.
    let mut diagnostics = merlion_render::Diagnostics::new(true);
    let spec = merlion_render::chart::spec::read(src, 64, &mut diagnostics).unwrap();
    let mut fuel = merlion_render::fuel::Fuel::new(u64::MAX);
    let resolved = merlion_render::chart::resolve::resolve(src, &spec, &mut diagnostics, &mut fuel)
        .unwrap()
        .unwrap();
    let geometry =
        merlion_render::chart::layout::layout(&spec, &resolved, &strict(), &mut diagnostics)
            .unwrap();
    let tick = |v: &str| geometry.y_ticks.iter().find(|t| t.value == v).unwrap().pos;
    let per_unit = (tick("0") - tick("10")) / 10.0;
    let at = |v: f64| tick("0") - v * per_unit;
    let rects: Vec<_> = geometry.marks[0]
        .items
        .iter()
        .map(|g| match g.shape {
            merlion_render::chart::layout::Shape::Bar { rect, .. } => rect,
            _ => panic!("bar"),
        })
        .collect();
    let plot = geometry.plot;
    assert!((rects[1].y - at(11.0)).abs() < 1e-9 && rects[1].y > plot.y);
    let bottom = rects[3].y + rects[3].h;
    assert!((bottom - at(-3.0)).abs() < 1e-9 && bottom < plot.y + plot.h);
}

/// Each invalid fixture fails with its code and JSON path, and draws nothing.
#[test]
fn stacking_errors_name_their_json_path() {
    for (src, code, path) in [
        (
            include_str!("fixtures/chart/invalid/stack-without-series.json"),
            "E021",
            "`/marks/0/stack`: stacking needs `series`",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-area-y2.json"),
            "E021",
            "`/marks/0/stack`: an area with `y2`",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-line.json"),
            "E021",
            "`/marks/0/stack`: does not apply to a line mark",
        ),
        (
            include_str!("fixtures/chart/invalid/total-without-stack.json"),
            "E021",
            "`/marks/0/total`: needs `stack: true`",
        ),
        (
            include_str!("fixtures/chart/invalid/total-area.json"),
            "E021",
            "`/marks/0/total`: does not apply",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-bar-label.json"),
            "E021",
            "`/marks/0/label`: a stacked bar does not take `label`",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-area-missing-point.json"),
            "E022",
            "`/marks/0`: a stacked area needs a value for every series at every x; series \"Wind\" has none at \"2026-03\"",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-duplicate.json"),
            "E022",
            "`/data/2`: series \"Solar\" already has a value at \"North\"",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-outside-domain.json"),
            "E022",
            "`/marks/0`: the stack at \"North\" reaches 10.75 with series \"Wind\", outside `/y/domain`",
        ),
        (
            include_str!("fixtures/chart/invalid/stack-overflow.json"),
            "E022",
            "`/marks/0`: the stack at \"North\" sums past the binary64 range",
        ),
    ] {
        let diagnostics = check_chart(src, false);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{path}: {errors:?}");
        assert_eq!(errors[0].code, code, "{path}");
        assert!(errors[0].message.starts_with(path), "{}", errors[0].message);
        assert!(errors[0].span.line > 1, "{path}");
        assert!(render_chart(src, &strict()).svg.is_none(), "{path}");
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Every fixture renders to the bytes recorded in `fixtures/chart-digests.txt`: FNV-1a 64
/// of the SVG with `data-merlion-version` blanked, and of the outline, under default
/// options. `packages/merlion-wasm/test/glue.test.mjs` checks the WASM build against the
/// same file, so native and WASM output are byte-identical. Run with `MERLION_BLESS=1`
/// to rewrite it after an intended change, and read the diff before committing it.
#[test]
fn chart_fixtures_render_the_recorded_bytes() {
    const DIGESTS: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chart-digests.txt"
    );
    let mut fresh = String::new();
    for (name, src) in FIXTURES {
        let out = render_chart(src, &RenderOptions::default());
        let svg = out.svg.expect("svg");
        let key = "data-merlion-version=\"";
        let i = svg.find(key).unwrap() + key.len();
        let end = i + svg[i..].find('"').unwrap();
        let blanked = format!("{}{}", &svg[..i], &svg[end..]);
        fresh.push_str(&format!(
            "{name}.json {:016x} {:016x}\n",
            fnv1a64(blanked.as_bytes()),
            fnv1a64(out.outline.unwrap().as_bytes())
        ));
    }
    if std::env::var_os("MERLION_BLESS").is_some() {
        std::fs::write(DIGESTS, &fresh).expect("write digests");
        return;
    }
    let recorded = std::fs::read_to_string(DIGESTS).expect("read digests");
    assert_eq!(
        recorded, fresh,
        "chart output changed; rerun with MERLION_BLESS=1 when the change is intended"
    );
}

/// A total's printed length is checked against the drawn text limit before it is built.
#[test]
fn stack_totals_count_toward_the_text_limit() {
    let src = r#"{"chart":1,"title":"t","x":{"type":"band"},"y":{"type":"linear"},
        "data":[{"a":"x","s":"p","v":1},{"a":"x","s":"q","v":0e-99999999}],
        "marks":[{"type":"bar","x":"a","y":"v","series":"s","stack":true,"total":true}]}"#;
    let out = render_chart(src, &strict());
    assert!(matches!(
        out.error,
        Some(merlion_render::RenderError::TooLarge { what: "chart text" })
    ));
    assert!(out.svg.is_none());
    assert!(check_chart(src, true).iter().any(|d| d.code == "E004"));
    // Without `total` nothing is printed, so the same values stack and draw.
    let quiet = src.replace(r#","total":true"#, "");
    assert_eq!(render_chart(&quiet, &strict()).error, None);
}
