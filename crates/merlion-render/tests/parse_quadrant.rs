//! `quadrantChart` syntax, configuration and diagnostics (specs/quadrant.md).
mod parse_support;

use merlion_render::diag::{Diagnostic, Severity};
use merlion_render::model::quadrant::{Quadrant, XAxisPosition, YAxisPosition};
use merlion_render::model::{Color, Diagram};
use merlion_render::options::Limits;
use merlion_render::parse::ParseError;
use parse_support::{codes, has, run, try_parse};

fn parse(src: &str) -> (Quadrant, Vec<Diagnostic>) {
    let (r, d) = try_parse(src);
    match r {
        Ok(Diagram::Quadrant(q)) => (q, d),
        other => panic!("{other:?}: {d:#?}"),
    }
}

fn hex(r: u8, g: u8, b: u8) -> Option<Color> {
    Some(Color::Rgba { r, g, b, a: 255 })
}

#[test]
fn reads_every_statement_of_the_documented_example() {
    let (q, d) = parse(
        "quadrantChart\n  title Reach and engagement of campaigns\n  x-axis Low Reach --> High Reach\n  y-axis Low Engagement --> High Engagement\n  quadrant-1 We should expand\n  quadrant-2 Need to promote\n  quadrant-3 Re-evaluate\n  quadrant-4 May be improved\n  Campaign A: [0.3, 0.6]\n  Campaign B: [0.45, 0.23]\n",
    );
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(
        q.meta.title.as_deref(),
        Some("Reach and engagement of campaigns")
    );
    assert_eq!(q.x_axis.low.as_deref(), Some("Low Reach"));
    assert_eq!(q.x_axis.high.as_deref(), Some("High Reach"));
    assert_eq!(q.y_axis.low.as_deref(), Some("Low Engagement"));
    assert_eq!(q.y_axis.high.as_deref(), Some("High Engagement"));
    assert_eq!(
        q.quadrants.clone().map(|l| l.unwrap()),
        [
            "We should expand",
            "Need to promote",
            "Re-evaluate",
            "May be improved"
        ]
    );
    assert_eq!(q.points.len(), 2);
    assert_eq!(
        (q.points[0].label.as_str(), q.points[0].x, q.points[0].y),
        ("Campaign A", 0.3, 0.6)
    );
    assert_eq!(q.points[1].span.line, 10);
}

#[test]
fn keywords_match_in_any_case_and_statements_split_at_semicolons() {
    let (q, d) = parse("quadrantChart\n  TiTle Case title\n  X-AxIs Urgent --> Not Urgent\n  QuaDrant-1 Plan; QUADRANT-2 Do\n  a: [0, 1]; b: [1, 0]\n");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(q.meta.title.as_deref(), Some("Case title"));
    assert_eq!(q.x_axis.high.as_deref(), Some("Not Urgent"));
    assert_eq!(q.quadrants[0].as_deref(), Some("Plan"));
    assert_eq!(q.quadrants[1].as_deref(), Some("Do"));
    assert_eq!(q.points.len(), 2);
    // A title takes the rest of its line, `;` included, as mermaid's lexer does.
    let (q, _) = parse("quadrantChart\ntitle A; B\n");
    assert_eq!(q.meta.title.as_deref(), Some("A; B"));
}

#[test]
fn quoted_text_holds_delimiters_and_one_sided_axes_mark_their_direction() {
    let (q, d) = parse(
        "quadrantChart\n  x-axis \"Urgent (* +=[❤\" --> \"a --> b\"\n  y-axis Engagement -->\n  \"Point: one\": [1, 0]\n  \"`Markdown`\": [0.5, 0.5]\n",
    );
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(q.x_axis.low.as_deref(), Some("Urgent (* +=[❤"));
    assert_eq!(q.x_axis.high.as_deref(), Some("a --> b"));
    assert_eq!(q.y_axis.low.as_deref(), Some("Engagement \u{2192}"));
    assert_eq!(q.y_axis.high, None);
    assert_eq!(q.points[0].label, "Point: one");
    assert_eq!(q.points[1].label, "Markdown");
    let (q, _) = parse("quadrantChart\n  x-axis Only left\n");
    assert_eq!(
        (q.x_axis.low.as_deref(), q.x_axis.high.as_deref()),
        (Some("Only left"), None)
    );
}

#[test]
fn point_styles_and_classes_follow_mermaids_grammar() {
    let (q, d) = parse(
        "quadrantChart\n  A:::hot: [0.9, 0.0] radius: 12\n  B: [0.8, 0.1] color: #ff3300, radius : 10, stroke-color: 10f0f0, stroke-width: 5px\n  classDef hot color: #109060, radius : 10, stroke-color: #310085, stroke-width: 10px\n  classDef hot color: #f00fff\n",
    );
    assert_eq!(codes(&d), ["I030"]);
    assert_eq!(q.points[0].class.as_deref(), Some("hot"));
    assert_eq!(q.points[0].style.radius, Some(12.0));
    let b = &q.points[1].style;
    assert_eq!((b.radius, b.stroke_width), (Some(10.0), Some(5.0)));
    assert_eq!(b.color, hex(0xff, 0x33, 0x00));
    assert_eq!(b.stroke_color, hex(0x10, 0xf0, 0xf0));
    // A later `classDef` of the same name replaces the earlier style.
    assert_eq!(q.class_defs.len(), 1);
    assert_eq!(q.class_defs[0].style.color, hex(0xf0, 0x0f, 0xff));
    assert_eq!(q.class_defs[0].style.radius, None);
    // The point's own style wins over its class's.
    let s = q.effective_style(&q.points[0]);
    assert_eq!((s.radius, s.color), (Some(12.0), hex(0xf0, 0x0f, 0xff)));
}

#[test]
fn styles_outside_the_accepted_set_are_dropped_with_w010() {
    let (q, d) = parse("quadrantChart\n  A: [0.5, 0.5] fill: #ff0000, radius: big, stroke-width: 30px, color: #00ff00\n  B:::9bad: [0.1, 0.1]\n");
    assert_eq!(codes(&d), ["W010", "W010", "W010", "I030", "W011"]);
    assert_eq!(q.points[0].style.color, hex(0, 0xff, 0));
    assert_eq!(q.points[0].style.radius, None);
    assert_eq!(q.points[1].class, None);
    let (r, d) = run(
        "quadrantChart\n  A: [0.5, 0.5] fill: red\n",
        true,
        Limits::default(),
    );
    assert!(matches!(r, Err(ParseError::Failed)), "{d:?}");
}

#[test]
fn coordinates_outside_zero_to_one_fail_with_e024() {
    let (r, d) = try_parse("quadrantChart\n  Point1 : [1.2, 0.4]\n");
    assert!(matches!(r, Err(ParseError::Failed)));
    let e = d.iter().find(|x| x.code == "E024").expect("E024");
    assert_eq!(e.severity, Severity::Error);
    assert_eq!((e.span.line, e.span.column), (2, 13));
    let (r, d) = try_parse("quadrantChart\n  Point1 : [0.4, -0.1]\n");
    assert!(matches!(r, Err(ParseError::Failed)) && has(&d, "E024"));
}

#[test]
fn coordinates_mermaid_cannot_read_are_repaired_with_r021() {
    let src = "quadrantChart\n  A: [1.0, .5]\n";
    let (q, d) = parse(src);
    assert_eq!((q.points[0].x, q.points[0].y), (1.0, 0.5));
    assert_eq!(codes(&d), ["R021", "R021"]);
    let fixes: Vec<(&str, u32)> = d
        .iter()
        .map(|x| {
            let f = x.fix.as_ref().unwrap();
            (f.replacement.as_str(), f.span.column)
        })
        .collect();
    assert_eq!(fixes, [("1", 7), ("0.5", 12)]);
    let (r, _) = run(src, true, Limits::default());
    assert!(matches!(r, Err(ParseError::Failed)));
}

#[test]
fn unknown_and_malformed_statements_fail_with_their_location() {
    for (src, line) in [
        ("quadrantChart\n  quadrant-5 Nope\n", 2),
        ("quadrantChart\n  title ok\n  just words\n", 3),
        ("quadrantChart\n  A: [0.5]\n", 2),
        ("quadrantChart\n  A: [0.5, 0.5\n", 2),
        ("quadrantChart\n  A: [x, 0.5]\n", 2),
        ("quadrantChart\n  x-axis --> High\n", 2),
        ("quadrantChart\n  quadrant-1\n", 2),
        ("quadrantChart\n  : [0.5, 0.5]\n", 2),
    ] {
        let (r, d) = try_parse(src);
        assert!(matches!(r, Err(ParseError::Failed)), "{src:?}");
        let e = d.iter().find(|x| x.code == "E002").expect(src);
        assert_eq!(e.span.line, line, "{src:?}");
    }
}

#[test]
fn accessibility_statements_and_comments() {
    let (q, d) = parse("quadrantChart\n%% a comment\n  accTitle: Risk\n  accDescr {\n    Two lines\n    of description\n  }\n  A: [0.1, 0.9] %% trailing\n");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(q.meta.acc_title.as_deref(), Some("Risk"));
    assert_eq!(
        q.meta.acc_descr.as_deref(),
        Some("Two lines\nof description")
    );
    assert_eq!(q.points.len(), 1);
    let (r, _) = try_parse("quadrantChart\n  accDescr {\n  never closed\n");
    assert!(matches!(r, Err(ParseError::Failed)));
    let (q, d) = parse("quadrantChart\n");
    assert!(d.is_empty() && q.points.is_empty() && q.x_axis.is_empty());
}

#[test]
fn metadata_preserves_literal_comment_markers() {
    let (q, d) = parse("quadrantChart; title Revenue 90%% retained; literal\naccTitle: Efficiency 90%% remaining\naccDescr: CPU is 90%% utilized\n");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(
        q.meta.title.as_deref(),
        Some("Revenue 90%% retained; literal")
    );
    assert_eq!(
        q.meta.acc_title.as_deref(),
        Some("Efficiency 90%% remaining")
    );
    assert_eq!(q.meta.acc_descr.as_deref(), Some("CPU is 90%% utilized"));
    let (q, d) = parse("quadrantChart\naccDescr {CPU is 90%% utilized}\nA: [0, 1] %% comment\n");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(q.meta.acc_descr.as_deref(), Some("CPU is 90%% utilized"));
    assert_eq!(q.points.len(), 1);
    let (q, d) = parse("quadrantChart\naccDescr {\nCPU is 90%% utilized\n}\n");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(q.meta.acc_descr.as_deref(), Some("CPU is 90%% utilized"));
}

#[test]
fn quadrant_chart_config_from_directives_and_front_matter() {
    let (q, d) = parse(
        "%%{init: {\"quadrantChart\": {\"chartWidth\": 600, \"chartHeight\": 400, \"pointRadius\": 8, \"xAxisPosition\": \"bottom\", \"yAxisPosition\": \"right\", \"useMaxWidth\": true}}}%%\nquadrantChart\n",
    );
    assert!(d.is_empty(), "{d:#?}");
    let c = &q.meta.quadrant;
    assert_eq!(
        (c.chart_width, c.chart_height, c.point_radius),
        (Some(600.0), Some(400.0), Some(8.0))
    );
    assert_eq!(c.x_axis_position, Some(XAxisPosition::Bottom));
    assert_eq!(c.y_axis_position, Some(YAxisPosition::Right));
    let (q, d) = parse("---\nconfig:\n  quadrantChart:\n    titleFontSize: 18\n    quadrantPadding: \"12\"\n---\nquadrantChart\n");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(q.meta.quadrant.title_font_size, Some(18.0));
    assert_eq!(q.meta.quadrant.quadrant_padding, Some(12.0));
    let (q, d) = parse(
        "%%{init: {\"quadrantChart\": {\"chartWidth\": 5, \"xAxisPosition\": \"middle\", \"useMaxWidth\": false, \"bogus\": 1}}}%%\nquadrantChart\n",
    );
    assert_eq!(codes(&d), ["W016", "W016", "W016", "W016"]);
    assert!(d[3].message.contains("quadrantChart.bogus"));
    assert_eq!(q.meta.quadrant.chart_width, None);
}

#[test]
fn quadrant_theme_variables_are_fixed_colours_and_others_stay_ignored() {
    let (q, d) = parse(
        "%%{init: {\"themeVariables\": {\"quadrant1Fill\": \"#b4dcff\", \"quadrantPointFill\": \"ff0000\"}}}%%\nquadrantChart\n",
    );
    assert_eq!(codes(&d), ["I030"]);
    assert_eq!(q.meta.quadrant.theme.fill[0], hex(0xb4, 0xdc, 0xff));
    assert_eq!(q.meta.quadrant.theme.point_fill, hex(0xff, 0, 0));
    let (_, d) = parse(
        "%%{init: {\"themeVariables\": {\"quadrant1Fill\": \"#b4dcff\", \"primaryColor\": \"#fff\"}}}%%\nquadrantChart\n",
    );
    assert_eq!(codes(&d), ["I011", "I030"]);
    assert!(d[0].message.contains("except its quadrant chart colours"));
    let (_, d) = parse(
        "%%{init: {\"themeVariables\": {\"quadrant1Fill\": \"not a colour\"}}}%%\nquadrantChart\n",
    );
    assert_eq!(codes(&d), ["W016"]);
    // A diagram of another type reads no quadrant colour, so the key is ignored.
    let (r, d) = try_parse("%%{init: {\"themeVariables\": {\"quadrant1Fill\": \"#b4dcff\"}}}%%\nflowchart LR\n  a[A]\n");
    assert!(r.is_ok());
    assert_eq!(codes(&d), ["I011"]);
    assert!(!d[0].message.contains("except"));
}

#[test]
fn limits_bound_points_and_labels() {
    let mut src = String::from("quadrantChart\n");
    for i in 0..3 {
        src.push_str(&format!("p{i}: [0.5, 0.5]\n"));
    }
    let limits = Limits {
        nodes: 2,
        ..Limits::default()
    };
    let (r, _) = run(&src, false, limits);
    assert!(matches!(
        r,
        Err(ParseError::TooLarge {
            what: "quadrant points"
        })
    ));
    let long = "x".repeat(5_000);
    let (q, d) = parse(&format!("quadrantChart\n  quadrant-1 {long}\n"));
    assert!(has(&d, "W012"));
    assert_eq!(q.quadrants[0].as_ref().unwrap().len(), 4_096);
}
