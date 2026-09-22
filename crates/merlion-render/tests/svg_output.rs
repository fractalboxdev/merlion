//! End-to-end tests of the SVG writer (specs/svg-output.md, specs/security.md#output).
//! Models and geometry are built by hand, so these tests depend on no other stage.

use merlion_render::diag::{Diagnostics, Severity, Span};
use merlion_render::geometry::{ClusterGeom, EdgeGeom, EdgeLabelGeom, Geometry, NodeGeom, Point};
use merlion_render::model::{
    Arrow, ClassDef, Color, Edge, Flowchart, FontWeight, Link, Meta, Node, Shape, Stroke, Style,
    Subgraph,
};
use merlion_render::options::{Direction, EdgeStyle, FontMode, RenderOptions};
use merlion_render::svg::EMBED_FONT_FAMILY;
use merlion_render::svg::{draw_flowchart, outline_flowchart, DrawOutput, ALL_SHAPES};
use merlion_render::text::{embedded_font_css, ofl_xml_comment, LabelLayout, Line, Run, Weight};

mod svg_support;
use svg_support::*;

// ---------------------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------------------

const FS: f64 = 14.0;

fn run(text: &str) -> Run {
    Run {
        text: text.to_string(),
        weight: Weight::Regular,
        italic: false,
        code: false,
        width: text.chars().count() as f64 * FS * 0.5,
    }
}

/// One line per `\n`, 7 px per char, 17 px line height, 13 px ascent.
fn lbl(text: &str) -> LabelLayout {
    let lines: Vec<Line> = text
        .split('\n')
        .map(|l| {
            let r = run(l);
            Line {
                width: r.width,
                runs: vec![r],
                size: FS,
                detail: false,
                height: 17.0,
                ascent: 13.0,
            }
        })
        .collect();
    LabelLayout {
        width: lines
            .iter()
            .fold(0.0, |a, l| if l.width > a { l.width } else { a }),
        height: 17.0 * lines.len() as f64,
        line_height: 17.0,
        ascent: 13.0,
        lines,
    }
}

fn node(id: &str, label: &str) -> Node {
    Node {
        id: id.to_string(),
        label: label.to_string(),
        shape: Shape::Rect,
        classes: vec![],
        style: Style::default(),
        link: None,
        subgraph: None,
        span: Span::default(),
    }
}

fn edge(from: usize, to: usize) -> Edge {
    Edge {
        from,
        to,
        label: None,
        stroke: Stroke::Normal,
        arrow_start: Arrow::None,
        arrow_end: Arrow::Arrow,
        min_len: 1,
        style: Style::default(),
        span: Span::default(),
        id: None,
        classes: vec![],
    }
}

fn subgraph(id: &str, title: &str, parent: Option<usize>, nodes: Vec<usize>) -> Subgraph {
    Subgraph {
        id: id.to_string(),
        title: title.to_string(),
        parent,
        nodes,
        direction: None,
        classes: Vec::new(),
        style: Style::default(),
        span: Span::default(),
    }
}

/// Geometry for `chart`: node i at (60 + 120·i, 40), 100 × 40; edges straight between
/// centres with one elbow; one layer per node; clusters around their members.
fn geom_for(chart: &Flowchart) -> Geometry {
    let nodes: Vec<NodeGeom> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let label = lbl(&n.label);
            NodeGeom {
                x: 60.0 + 120.0 * i as f64,
                y: 40.0,
                w: 100.0,
                h: 40.0,
                label,
                rank: i as u8,
            }
        })
        .collect();
    let edges = chart
        .edges
        .iter()
        .map(|e| {
            let a = &nodes[e.from];
            let b = &nodes[e.to];
            EdgeGeom {
                points: vec![
                    Point::new(a.x, a.y + 20.0),
                    Point::new(a.x, a.y + 40.0),
                    Point::new(b.x, a.y + 40.0),
                    Point::new(b.x, b.y + 20.0),
                ],
                label: e.label.as_ref().map(|l| EdgeLabelGeom {
                    x: (a.x + b.x) / 2.0,
                    y: a.y + 40.0,
                    label: lbl(l),
                }),
                back: e.from > e.to,
                wrap: false,
            }
        })
        .collect();
    let clusters = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(si, s)| {
            let xs: Vec<f64> = chart
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.subgraph == Some(si))
                .map(|(i, _)| nodes[i].x)
                .collect();
            let (lo, hi) = xs
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
            let (lo, hi) = if xs.is_empty() { (0.0, 0.0) } else { (lo, hi) };
            ClusterGeom {
                x: lo - 62.0,
                y: 0.0,
                w: hi - lo + 124.0,
                h: 80.0,
                label: lbl(&s.title),
                label_x: (lo + hi) / 2.0,
                label_y: 8.0,
            }
        })
        .collect();
    Geometry {
        width: 120.0 * chart.nodes.len() as f64,
        height: 100.0,
        direction: chart.direction,
        nodes,
        edges,
        clusters,
        layers: (0..chart.nodes.len()).map(|i| vec![i]).collect(),
        fuel_used: 0,
    }
}

fn opts() -> RenderOptions {
    RenderOptions::default()
}

fn draw_with(chart: &Flowchart, o: &RenderOptions) -> (DrawOutput, Diagnostics) {
    let g = geom_for(chart);
    let mut d = Diagnostics::new(o.strict);
    let out = draw_flowchart(chart, &g, o, "m1", &mut d);
    (out, d)
}

fn draw(chart: &Flowchart) -> DrawOutput {
    draw_with(chart, &opts()).0
}

/// The outline example of specs/svg-output.md#text-alternative.
fn sample() -> Flowchart {
    let mut nodes = vec![
        node("src", "Source .md"),
        node("has", "Has mermaid?"),
        node("render", "Render SVG"),
        node("pass", "Pass through"),
        node("cache", "Cache"),
    ];
    nodes[0].subgraph = Some(0);
    let mut yes = edge(1, 2);
    yes.label = Some("yes".into());
    let mut no = edge(1, 3);
    no.label = Some("no".into());
    Flowchart {
        direction: Direction::LR,
        nodes,
        edges: vec![edge(0, 1), yes, no, edge(2, 4)],
        subgraphs: vec![subgraph("build", "Build", None, vec![0])],
        ..Flowchart::default()
    }
}

// ---------------------------------------------------------------------------------------
// Checkers
// ---------------------------------------------------------------------------------------

fn count(h: &str, n: &str) -> usize {
    h.matches(n).count()
}

// ---------------------------------------------------------------------------------------
// Root element and accessibility
// ---------------------------------------------------------------------------------------

#[test]
fn root_element_matches_the_spec() {
    let out = draw(&sample());
    let expected = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" id=\"m1\" viewBox=\"0 0 600 100\" width=\"100%\" \
         style=\"max-width:600px\" role=\"img\" aria-labelledby=\"m1-title m1-desc\" \
         class=\"merlion merlion-flowchart\" data-merlion-version=\"{}\" \
         data-merlion-layout=\"v1;LR;0:src;1:has;2:render;3:pass;4:cache\">\n\
         <title id=\"m1-title\">Flowchart diagram</title>\n<desc id=\"m1-desc\">",
        merlion_render::VERSION
    );
    assert!(out.svg.starts_with(&expected), "{}", &out.svg[..400]);
    assert!(out.svg.ends_with("</svg>\n"));
    assert!(!out.svg.contains(" height=\"") || !out.svg[..300].contains(" height="));
}

#[test]
fn output_is_well_formed_and_safe() {
    let out = draw(&sample());
    assert_well_formed(&out.svg);
    assert_safe(&out.svg, "m1");
}

#[test]
fn title_prefers_acc_title_then_front_matter() {
    let mut c = sample();
    c.meta = Meta {
        title: Some("Front".into()),
        acc_title: Some("Acc <title>".into()),
        ..Meta::default()
    };
    assert!(draw(&c)
        .svg
        .contains("<title id=\"m1-title\">Acc &lt;title&gt;</title>"));
    c.meta.acc_title = None;
    assert!(draw(&c)
        .svg
        .contains("<title id=\"m1-title\">Front</title>"));
    c.meta.title = Some("   ".into());
    assert!(draw(&c)
        .svg
        .contains("<title id=\"m1-title\">Flowchart diagram</title>"));
}

#[test]
fn acc_descr_replaces_desc_but_not_the_outline() {
    let mut c = sample();
    c.meta.acc_descr = Some("A pipeline & its cache".into());
    let out = draw(&c);
    assert!(out
        .svg
        .contains("<desc id=\"m1-desc\">A pipeline &amp; its cache</desc>"));
    assert!(out.outline.starts_with("Flowchart, left to right."));
}

#[test]
fn desc_holds_the_escaped_outline_by_default() {
    let out = draw(&sample());
    assert!(out
        .svg
        .contains("<desc id=\"m1-desc\">Flowchart, left to right. 5 nodes, 4 edges.\nBuild: Source .md → Has mermaid?"));
}

#[test]
fn background_rect_only_when_requested() {
    let out = draw(&sample());
    assert!(!out.svg.contains("merlion-bg\""));
    let o = RenderOptions {
        background: true,
        ..opts()
    };
    let (out, _) = draw_with(&sample(), &o);
    assert!(out
        .svg
        .contains("<rect class=\"merlion-bg\" x=\"0\" y=\"0\" width=\"600\" height=\"100\" fill=\"#ffffff\"/>"));
    assert!(style_text(&out.svg).contains("#m1 .merlion-bg{fill:var(--merlion-bg, #ffffff);}"));
}

#[test]
fn invalid_root_id_is_encoded_defensively() {
    let c = sample();
    let g = geom_for(&c);
    let mut d = Diagnostics::new(false);
    let out = draw_flowchart(&c, &g, &opts(), "x\"{}<y", &mut d);
    assert!(out.svg.contains("id=\"mx_22_7b_7d_3cy\""));
    assert_well_formed(&out.svg);
    assert_safe(&out.svg, "mx_22_7b_7d_3cy");
}

// ---------------------------------------------------------------------------------------
// Layout hint
// ---------------------------------------------------------------------------------------

#[test]
fn hint_encodes_ids_and_keeps_layer_order() {
    let mut c = Flowchart {
        direction: Direction::TB,
        nodes: vec![node("a b", "x"), node("c_d", "y"), node("é;:,", "z")],
        ..Flowchart::default()
    };
    c.edges = vec![edge(0, 1)];
    let mut g = geom_for(&c);
    g.layers = vec![vec![1, 0], vec![], vec![2, 99]];
    let mut d = Diagnostics::new(false);
    let out = draw_flowchart(&c, &g, &opts(), "m1", &mut d);
    assert!(out
        .svg
        .contains("data-merlion-layout=\"v1;TB;0:c__d,a_20b;1:;2:_c3_a9_3b_3a_2c\""));
}

#[test]
fn hint_uses_the_drawn_direction() {
    let c = sample();
    let mut g = geom_for(&c);
    g.direction = Direction::TB;
    let mut d = Diagnostics::new(false);
    let out = draw_flowchart(&c, &g, &opts(), "m1", &mut d);
    assert!(out.svg.contains("data-merlion-layout=\"v1;TB;"));
    assert!(out.outline.starts_with("Flowchart, top to bottom."));
}

// ---------------------------------------------------------------------------------------
// Theming and embedded style
// ---------------------------------------------------------------------------------------

#[test]
fn every_style_rule_is_scoped() {
    let mut c = sample();
    c.class_defs = vec![ClassDef {
        name: "hot".into(),
        style: Style {
            fill: Some(Color::Named("red")),
            ..Style::default()
        },
    }];
    c.nodes[0].classes = vec!["hot".into()];
    let css = draw(&c).svg;
    for s in css_selectors(style_text(&css)) {
        assert!(
            s.starts_with("#m1 ") || s.starts_with(":where(#m1 "),
            "{}",
            s
        );
    }
}

#[test]
fn presentation_attributes_carry_the_defaults() {
    let svg = draw(&sample()).svg;
    assert!(svg.contains("class=\"merlion-shape\" d=\"M10 20H110V60H10Z\" fill=\"#f5f5f5\" stroke=\"#c8c9cb\" stroke-width=\"1.25\"/>"));
    assert!(svg.contains("class=\"merlion-edge-path\""));
    assert!(svg.contains("fill=\"none\" stroke=\"#919497\" stroke-width=\"1.25\""));
    assert!(svg.contains("<text class=\"merlion-label\" fill=\"#1f2328\">"));
    assert!(
        svg.contains("fill=\"#fafafa\" stroke=\"#c8c9cb\""),
        "cluster box defaults"
    );
    assert!(svg
        .contains("font-family=\"Inter, ui-sans-serif, system-ui, sans-serif\" font-size=\"14\""));
}

#[test]
fn css_rules_read_the_same_defaults() {
    let css_owner = draw(&sample()).svg;
    let css = style_text(&css_owner);
    assert!(css.contains("fill:var(--merlion-node-bg, var(--merlion-surface, #f5f5f5))"));
    assert!(css
        .contains("stroke:var(--merlion-tone, var(--merlion-edge, var(--merlion-line, #919497)))"));
    assert!(css.contains("fill:var(--merlion-node-text, var(--merlion-fg, #1f2328))"));
    assert!(css.contains("fill:var(--merlion-edge-label-bg, var(--merlion-bg, #ffffff))"));
    assert!(css.contains("fill:var(--merlion-cluster-bg, #fafafa)"));
    assert!(css.contains("stroke-width:var(--merlion-stroke, 1.25px)"));
}

#[test]
fn mixed_roles_are_defined_only_inside_supports() {
    let svg = draw(&sample()).svg;
    let css = style_text(&svg);
    let at = css
        .find("@supports (color: color-mix(in oklab, #000, #fff)){")
        .unwrap();
    assert!(!css[..at].contains("color-mix(in oklab, var"));
    assert!(css[at..].contains(
        "color-mix(in oklab, var(--merlion-fg, #1f2328) 45%, var(--merlion-bg, #ffffff))"
    ));
    assert!(css[at..]
        .contains("var(--merlion-cluster-bg, color-mix(in oklab, var(--merlion-fg, #1f2328) 2%"));
}

#[test]
fn text_reset_rule_is_present_verbatim() {
    let svg = draw(&sample()).svg;
    assert!(style_text(&svg).starts_with(
        "#m1 text { font-family: var(--merlion-font, Inter, ui-sans-serif, system-ui, sans-serif); \
         font-size: var(--merlion-font-size, 14px); font-weight: 400; font-style: normal; font-stretch: normal; \
         font-kerning: normal; font-variant-ligatures: none; font-feature-settings: \"calt\" 0, \"liga\" 0; \
         letter-spacing: 0; word-spacing: 0; text-transform: none; }"
    ));
}

#[test]
fn font_link_and_system_emit_no_inline_font() {
    for font in [FontMode::Link, FontMode::System] {
        let o = RenderOptions { font, ..opts() };
        let (out, _) = draw_with(&sample(), &o);
        assert!(!out.svg.contains("@font-face"));
        assert!(!out.svg.contains("data:"));
        assert!(!out.svg.contains("<!--"));
        assert!(!out.svg.contains(EMBED_FONT_FAMILY));
    }
}

/// specs/text-measurement.md#serving-the-font: the WOFF2 subset as a `data:` URI under a
/// Merlion-only family name, the OFL notice as an XML comment, and the family first in
/// the stack.
#[test]
fn font_embed_inlines_the_subset_and_the_licence() {
    let o = RenderOptions {
        font: FontMode::Embed,
        ..opts()
    };
    let (out, _) = draw_with(&sample(), &o);
    let css = style_text(&out.svg);
    let face = embedded_font_css(EMBED_FONT_FAMILY);
    assert_eq!(out.svg.matches("@font-face").count(), 2);
    assert!(css.contains(&face));
    assert_eq!(out.svg.matches(ofl_xml_comment().as_str()).count(), 1);
    let stack = format!(
        "{}, Inter, ui-sans-serif, system-ui, sans-serif",
        EMBED_FONT_FAMILY
    );
    assert!(css.starts_with(&format!(
        "#m1 text {{ font-family: var(--merlion-font, {});",
        stack
    )));
    assert!(out.svg.contains(&format!("font-family=\"{}\"", stack)));
    assert_well_formed(&out.svg);
    assert_safe(&out.svg, "m1");
}

#[test]
#[should_panic(expected = "@font-face")]
fn safety_check_rejects_a_second_font_face() {
    let o = RenderOptions {
        font: FontMode::Embed,
        ..opts()
    };
    let (out, _) = draw_with(&sample(), &o);
    let svg = out.svg.replacen(
        "</style>",
        "@font-face{font-family:\"x\";src:local(x)}</style>",
        1,
    );
    assert_safe(&svg, "m1");
}

#[test]
#[should_panic(expected = "external url")]
fn safety_check_rejects_other_data_urls() {
    let (out, _) = draw_with(&sample(), &opts());
    let svg = out.svg.replacen(
        "</style>",
        "#m1 rect{fill:url(data:image/png;base64,AAAA)}</style>",
        1,
    );
    assert_safe(&svg, "m1");
}

// ---------------------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------------------

#[test]
fn node_groups_carry_id_and_rank() {
    let svg = draw(&sample()).svg;
    assert!(
        svg.contains("<g class=\"merlion-node\" data-merlion-id=\"src\" data-merlion-rank=\"0\">")
    );
    assert!(svg
        .contains("<g class=\"merlion-node\" data-merlion-id=\"cache\" data-merlion-rank=\"4\">"));
    assert_eq!(count(&svg, "class=\"merlion-node"), 5);
}

#[test]
fn rank_is_clamped_to_fifteen() {
    let c = sample();
    let mut g = geom_for(&c);
    g.nodes[2].rank = 200;
    let mut d = Diagnostics::new(false);
    let svg = draw_flowchart(&c, &g, &opts(), "m1", &mut d).svg;
    assert!(svg.contains("data-merlion-id=\"render\" data-merlion-rank=\"15\""));
}

#[test]
fn every_shape_emits_a_path() {
    let nodes: Vec<Node> = ALL_SHAPES
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut n = node(&format!("n{}", i), "x");
            n.shape = *s;
            n
        })
        .collect();
    let c = Flowchart {
        nodes,
        ..Flowchart::default()
    };
    let svg = draw(&c).svg;
    assert_eq!(
        count(&svg, "<path class=\"merlion-shape\" d=\"M"),
        ALL_SHAPES.len()
    );
    assert_well_formed(&svg);
}

#[test]
fn label_text_is_positioned_from_the_layout() {
    let svg = draw(&sample()).svg;
    // Node 0: centre (60, 40); "Source .md" is 10 chars = 70 px; one 17 px line.
    assert!(
        svg.contains("<tspan x=\"25\" y=\"44.5\">Source .md</tspan>"),
        "{}",
        svg
    );
}

#[test]
fn markdown_runs_become_formatted_tspans() {
    let c = Flowchart {
        nodes: vec![node("a", "x")],
        ..Flowchart::default()
    };
    let mut g = geom_for(&c);
    let mut b = run("bold");
    b.weight = Weight::SemiBold;
    let mut it = run("it");
    it.italic = true;
    let mut code = run("f()");
    code.code = true;
    g.nodes[0].label = LabelLayout {
        lines: vec![Line {
            width: 63.0,
            runs: vec![b, it, code],
            size: FS,
            detail: false,
            height: 17.0,
            ascent: 13.0,
        }],
        width: 63.0,
        height: 17.0,
        line_height: 17.0,
        ascent: 13.0,
    };
    let mut d = Diagnostics::new(false);
    let svg = draw_flowchart(&c, &g, &opts(), "m1", &mut d).svg;
    assert!(svg.contains("<tspan class=\"merlion-b\" font-weight=\"600\">bold</tspan>"));
    assert!(svg.contains("<tspan x=\"56.5\" class=\"merlion-i\" font-style=\"italic\">it</tspan>"));
    assert!(svg.contains("<tspan x=\"70.5\" class=\"merlion-code\" font-family=\"ui-monospace"));
    let css = style_text(&svg).to_string();
    assert!(css.contains("#m1 .merlion-b{font-weight:600;}"));
}

#[test]
fn hostile_label_and_id_are_escaped() {
    let mut c = Flowchart {
        nodes: vec![
            node("x\" onload=\"alert(1)", "</text><script>alert(1)</script>"),
            node("y", "\" onload=\"alert(2)"),
        ],
        ..Flowchart::default()
    };
    c.edges = vec![edge(0, 1)];
    c.edges[0].label = Some("<img src=x onerror=alert(3)>".into());
    let out = draw(&c);
    assert_well_formed(&out.svg);
    assert_safe(&out.svg, "m1");
    assert!(out
        .svg
        .contains("data-merlion-id=\"x&quot; onload=&quot;alert(1)\""));
    assert!(out
        .svg
        .contains("&lt;/text&gt;&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(out
        .svg
        .contains("data-merlion-from=\"x&quot; onload=&quot;alert(1)\""));
}

#[test]
fn control_characters_never_reach_the_output() {
    let c = Flowchart {
        nodes: vec![node("a\u{1}", "a\u{0}b\u{FFFF}c\u{202E}d")],
        ..Flowchart::default()
    };
    let out = draw(&c);
    assert_well_formed(&out.svg);
    assert!(out.svg.contains(">abcd</tspan>"));
    assert!(out.svg.contains("data-merlion-id=\"a\""));
}

// ---------------------------------------------------------------------------------------
// Source styles
// ---------------------------------------------------------------------------------------

fn styled_chart() -> Flowchart {
    let mut c = sample();
    c.class_defs = vec![
        ClassDef {
            name: "hot".into(),
            style: Style {
                fill: Some(Color::Rgba {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
                stroke: Some(Color::Hsla {
                    h: 120.0,
                    s: 50.0,
                    l: 25.0,
                    a: 0.5,
                }),
                color: Some(Color::Named("white")),
                font_weight: Some(FontWeight::SemiBold),
                stroke_width: Some(3.0),
                ..Style::default()
            },
        },
        ClassDef {
            name: "bad name\"{}".into(),
            style: Style {
                fill: Some(Color::Named("blue")),
                ..Style::default()
            },
        },
    ];
    c.nodes[1].classes = vec!["hot".into(), "hot".into(), "x\"><script>".into()];
    c.nodes[2].style = Style {
        fill: Some(Color::Transparent),
        opacity: Some(0.5),
        ..Style::default()
    };
    c.edges[1].style = Style {
        stroke: Some(Color::Rgba {
            r: 0,
            g: 0,
            b: 255,
            a: 128,
        }),
        stroke_dasharray: Some(vec![4.0, 2.0]),
        fill: Some(Color::Named("red")),
        ..Style::default()
    };
    c
}

#[test]
fn class_def_becomes_scoped_rules() {
    let out = draw(&styled_chart());
    let css = style_text(&out.svg);
    assert!(css.contains(
        "#m1 .merlion-c-hot>.merlion-shape{fill:var(--merlion-c-hot-fill, #ff0000);\
         stroke:var(--merlion-c-hot-stroke, hsla(120, 50%, 25%, 0.5));stroke-width:3px;}"
    ));
    assert!(css.contains(
        "#m1 .merlion-c-hot text{fill:var(--merlion-c-hot-color, white);font-weight:600;}"
    ));
    assert!(!css.contains("bad name"));
    assert!(out
        .svg
        .contains("<g class=\"merlion-node merlion-c-hot\" data-merlion-id=\"has\""));
    assert!(!out.svg.contains("script"));
    assert_safe(&out.svg, "m1");
}

#[test]
fn node_and_link_styles_use_generated_classes() {
    let out = draw(&styled_chart());
    let css = style_text(&out.svg);
    assert!(css.contains("#m1 .merlion-ns-2{opacity:0.5;}"));
    assert!(css.contains("#m1 .merlion-ns-2>.merlion-shape{fill:transparent;}"));
    assert!(css
        .contains("#m1 .merlion-es-1>.merlion-edge-path{stroke:#0000ff80;stroke-dasharray:4 2;}"));
    assert!(out.svg.contains("class=\"merlion-node merlion-ns-2\""));
    assert!(out.svg.contains("<g class=\"merlion-edge merlion-es-1\""));
    assert!(!out.svg.contains("merlion-ns-0"));
}

#[test]
fn source_rules_follow_base_and_supports_rules() {
    let out = draw(&styled_chart());
    let css = style_text(&out.svg);
    let supports = css.find("@supports").unwrap();
    let class_rule = css.find(".merlion-c-hot").unwrap();
    let node_rule = css.find(".merlion-ns-2").unwrap();
    let edge_rule = css.find(".merlion-es-1").unwrap();
    assert!(supports < class_rule && class_rule < node_rule && node_rule < edge_rule);
}

#[test]
fn fixed_colour_emits_i030_once() {
    let (_, d) = draw_with(&styled_chart(), &opts());
    let i030: Vec<_> = d.items.iter().filter(|x| x.code == "I030").collect();
    assert_eq!(i030.len(), 1);
    assert_eq!(i030[0].severity, Severity::Info);
    let (_, d) = draw_with(&sample(), &opts());
    assert!(d.items.is_empty());
}

#[test]
fn hostile_named_colour_is_dropped() {
    let mut c = sample();
    c.class_defs = vec![ClassDef {
        name: "x".into(),
        style: Style {
            fill: Some(Color::Named("red}body{background:url(//evil)")),
            ..Style::default()
        },
    }];
    let out = draw(&c);
    assert!(!out.svg.contains("evil"));
    assert_safe(&out.svg, "m1");
}

// ---------------------------------------------------------------------------------------
// Edges
// ---------------------------------------------------------------------------------------

#[test]
fn edge_group_attributes() {
    let mut c = sample();
    c.edges.push(edge(4, 1));
    let mut g = geom_for(&c);
    g.edges[4].wrap = true;
    let mut d = Diagnostics::new(false);
    let svg = draw_flowchart(&c, &g, &opts(), "m1", &mut d).svg;
    assert!(svg.contains(
        "<g class=\"merlion-edge\" data-merlion-from=\"has\" data-merlion-to=\"render\">"
    ));
    assert!(svg.contains(
        "<g class=\"merlion-edge\" data-merlion-from=\"cache\" data-merlion-to=\"has\" data-merlion-back=\"true\" data-merlion-wrap=\"true\">"
    ));
    assert_eq!(count(&svg, "data-merlion-back"), 1);
}

#[test]
fn orthogonal_edges_have_rounded_corners() {
    let svg = draw(&sample()).svg;
    // src (60,40) → has (180,40): down to 80, across, up to 60.
    assert!(
        svg.contains("d=\"M60 60L60 74Q60 80 66 80L174 80Q180 80 180 74L180 60\""),
        "{}",
        svg
    );
}

#[test]
fn polyline_edges_are_straight() {
    let o = RenderOptions {
        edge_style: EdgeStyle::Polyline,
        ..opts()
    };
    let (out, _) = draw_with(&sample(), &o);
    assert!(out.svg.contains("d=\"M60 60L60 80L180 80L180 60\""));
}

#[test]
fn markers_are_defined_once_and_referenced_by_id() {
    let mut c = sample();
    c.edges[0].arrow_start = Arrow::Arrow;
    c.edges[1].arrow_end = Arrow::Circle;
    c.edges[2].arrow_end = Arrow::Cross;
    let svg = draw(&c).svg;
    assert_eq!(count(&svg, "<marker id=\"m1-arrow\""), 1);
    assert_eq!(count(&svg, "<marker id=\"m1-circle\""), 1);
    assert_eq!(count(&svg, "<marker id=\"m1-cross\""), 1);
    assert!(svg.contains("orient=\"auto-start-reverse\""));
    assert!(svg.contains("marker-start=\"url(#m1-arrow)\" marker-end=\"url(#m1-arrow)\""));
    assert!(svg.contains("marker-end=\"url(#m1-circle)\""));
    assert!(svg.contains("marker-end=\"url(#m1-cross)\""));
    assert_safe(&svg, "m1");
}

#[test]
fn no_defs_without_markers() {
    let mut c = sample();
    for e in &mut c.edges {
        e.arrow_end = Arrow::None;
    }
    assert!(!draw(&c).svg.contains("<defs>"));
}

#[test]
fn stroke_kinds() {
    let mut c = sample();
    c.edges[0].stroke = Stroke::Thick;
    c.edges[1].stroke = Stroke::Dotted;
    c.edges[3].stroke = Stroke::Invisible;
    let out = draw(&c);
    assert!(out
        .svg
        .contains("class=\"merlion-edge-path merlion-thick\""));
    assert!(out.svg.contains("stroke-width=\"2.5\""));
    assert!(out
        .svg
        .contains("class=\"merlion-edge-path merlion-dotted\""));
    assert!(out.svg.contains("stroke-dasharray=\"3 3\""));
    // Invisible links are layout-only: not drawn and not in the outline.
    assert!(!out.svg.contains("data-merlion-to=\"cache\""));
    assert!(out.outline.contains("5 nodes, 3 edges."));
    let css = style_text(&out.svg);
    assert!(css.contains(
        "#m1 .merlion-edge>.merlion-thick{stroke-width:calc(var(--merlion-stroke, 1.25px) * 2);}"
    ));
}

#[test]
fn edge_label_sits_on_a_chip() {
    let svg = draw(&sample()).svg;
    // "yes" = 21 px wide, 17 px tall, centred on (240, 80); chip pads 4 × 2.
    assert!(svg.contains(
        "<g class=\"merlion-edge-label\"><rect class=\"merlion-edge-label-bg\" x=\"225.5\" y=\"69.5\" width=\"29\" height=\"21\" rx=\"3\" fill=\"#ffffff\"/>\
         <text class=\"merlion-edge-text\" fill=\"#1f2328\"><tspan x=\"229.5\" y=\"84.5\">yes</tspan></text></g>"
    ), "{}", svg);
}

// ---------------------------------------------------------------------------------------
// Clusters and z-order
// ---------------------------------------------------------------------------------------

#[test]
fn cluster_nests_its_members() {
    let svg = draw(&sample()).svg;
    let open = svg
        .find("<g class=\"merlion-cluster\" data-merlion-id=\"build\">")
        .unwrap();
    let rect = svg.find("<rect class=\"merlion-cluster-box\"").unwrap();
    let title = svg.find("<text class=\"merlion-cluster-title\"").unwrap();
    let member = svg.find("data-merlion-id=\"src\"").unwrap();
    let close = open + svg[open..].find("</g>\n<g class=\"merlion-edge\"").unwrap();
    assert!(open < rect && rect < title && title < member && member < close);
}

#[test]
fn z_order_is_clusters_then_edges_then_nodes() {
    let svg = draw(&sample()).svg;
    let last_cluster = svg.rfind("merlion-cluster-box").unwrap();
    let first_root_edge = svg.find("data-merlion-from=\"src\"").unwrap();
    let first_root_node = svg.find("data-merlion-id=\"has\"").unwrap();
    assert!(last_cluster < first_root_edge && first_root_edge < first_root_node);
}

#[test]
fn nested_clusters_and_internal_edges() {
    let mut c = Flowchart {
        nodes: vec![node("a", "A"), node("b", "B"), node("c", "C")],
        subgraphs: vec![
            subgraph("outer", "Outer", None, vec![2]),
            subgraph("inner", "Inner", Some(0), vec![0, 1]),
        ],
        ..Flowchart::default()
    };
    c.nodes[0].subgraph = Some(1);
    c.nodes[1].subgraph = Some(1);
    c.nodes[2].subgraph = Some(0);
    c.edges = vec![edge(0, 1), edge(1, 2)];
    let out = draw(&c);
    assert_well_formed(&out.svg);
    let svg = &out.svg;
    let outer = svg.find("data-merlion-id=\"outer\"").unwrap();
    let inner = svg.find("data-merlion-id=\"inner\"").unwrap();
    let e_ab = svg
        .find("data-merlion-from=\"a\" data-merlion-to=\"b\"")
        .unwrap();
    let node_a = svg.find("data-merlion-id=\"a\"").unwrap();
    let e_bc = svg
        .find("data-merlion-from=\"b\" data-merlion-to=\"c\"")
        .unwrap();
    let node_c = svg.find("data-merlion-id=\"c\"").unwrap();
    assert!(outer < inner && inner < e_ab && e_ab < node_a && node_a < e_bc && e_bc < node_c);
    assert_eq!(
        out.outline,
        "Flowchart, top to bottom. 3 nodes, 2 edges.\nOuter / Inner: A → B\nOuter / Inner: B → C\nOuter: C"
    );
}

#[test]
fn cyclic_cluster_parents_do_not_hang() {
    let mut c = sample();
    c.subgraphs = vec![
        subgraph("p", "P", Some(1), vec![0]),
        subgraph("q", "Q", Some(0), vec![]),
    ];
    let out = draw(&c);
    assert_well_formed(&out.svg);
}

// ---------------------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------------------

#[test]
fn links_wrap_the_node_group() {
    let mut c = sample();
    c.nodes[1].link = Some(Link {
        url: "https://example.com/?a=1&b=2".into(),
        target_blank: true,
    });
    c.nodes[2].link = Some(Link {
        url: "/docs".into(),
        target_blank: false,
    });
    let svg = draw(&c).svg;
    assert!(svg.contains(
        "<a href=\"https://example.com/?a=1&amp;b=2\" target=\"_blank\" rel=\"noopener noreferrer\"><g class=\"merlion-node\" data-merlion-id=\"has\""
    ));
    assert!(svg.contains("<a href=\"/docs\"><g class=\"merlion-node\" data-merlion-id=\"render\""));
    assert_well_formed(&svg);
}

#[test]
fn unsafe_link_is_dropped_with_w013() {
    let mut c = sample();
    c.nodes[1].link = Some(Link {
        url: " java\tscript:alert(1)".into(),
        target_blank: false,
    });
    let (out, d) = draw_with(&c, &opts());
    assert!(!out.svg.contains("<a "));
    assert!(d
        .items
        .iter()
        .any(|x| x.code == "W013" && x.severity == Severity::Warning));
    let strict = RenderOptions {
        strict: true,
        ..opts()
    };
    let (_, d) = draw_with(&c, &strict);
    assert!(d.has_errors());
}

// ---------------------------------------------------------------------------------------
// Outline
// ---------------------------------------------------------------------------------------

#[test]
fn outline_matches_the_spec_example() {
    let out = draw(&sample());
    assert_eq!(
        out.outline,
        "Flowchart, left to right. 5 nodes, 4 edges.\n\
         Build: Source .md → Has mermaid?\n\
         Has mermaid? → Render SVG [yes]; → Pass through [no]\n\
         Render SVG → Cache"
    );
    assert_eq!(outline_flowchart(&sample()), out.outline);
}

#[test]
fn outline_singulars_isolated_nodes_and_empty_clusters() {
    let c = Flowchart {
        nodes: vec![node("solo", "**Alone**<br>here")],
        subgraphs: vec![subgraph("e", "", None, vec![])],
        ..Flowchart::default()
    };
    assert_eq!(
        outline_flowchart(&c),
        "Flowchart, top to bottom. 1 node, 0 edges.\nAlone here\ne:"
    );
}

#[test]
fn outline_arrow_kinds_and_empty_labels() {
    let mut c = Flowchart {
        direction: Direction::RL,
        nodes: vec![node("a", ""), node("b", "B")],
        ..Flowchart::default()
    };
    let mut both = edge(0, 1);
    both.arrow_start = Arrow::Arrow;
    let mut open = edge(0, 1);
    open.arrow_end = Arrow::None;
    open.label = Some("  ".into());
    c.edges = vec![both, open];
    assert_eq!(
        outline_flowchart(&c),
        "Flowchart, right to left. 2 nodes, 2 edges.\na ↔ B; — B"
    );
}

// ---------------------------------------------------------------------------------------
// Determinism and robustness
// ---------------------------------------------------------------------------------------

#[test]
fn output_is_byte_identical_across_runs() {
    let c = styled_chart();
    let a = draw(&c);
    let b = draw(&c);
    assert_eq!(a.svg, b.svg);
    assert_eq!(a.outline, b.outline);
}

#[test]
fn coordinates_are_canonical() {
    let c = sample();
    let mut g = geom_for(&c);
    g.nodes[0].x = 60.004999;
    g.nodes[0].y = -0.001;
    g.width = 600.1000001;
    let mut d = Diagnostics::new(false);
    let svg = draw_flowchart(&c, &g, &opts(), "m1", &mut d).svg;
    assert!(svg.contains("viewBox=\"0 0 600.1 100\""));
    assert!(!svg.contains("-0\"") && !svg.contains("NaN") && !svg.contains("inf"));
    // No exponent form: every number in a d attribute is plain digits.
    for (_, name, v) in all_attrs(&svg) {
        if name == "d" {
            assert!(!v.contains('e'), "{}", v);
        }
    }
}

#[test]
fn mismatched_geometry_does_not_panic() {
    let c = sample();
    let mut g = geom_for(&c);
    g.nodes.truncate(2);
    g.edges.truncate(1);
    g.clusters.clear();
    g.layers = vec![vec![0, 1000]];
    g.width = f64::NAN;
    let mut d = Diagnostics::new(false);
    let out = draw_flowchart(&c, &g, &opts(), "m1", &mut d);
    assert_well_formed(&out.svg);
    assert!(out.svg.contains("viewBox=\"0 0 0 100\""));
    // Edges pointing at missing nodes are tolerated too.
    let mut c2 = sample();
    c2.edges.push(edge(0, 77));
    let g2 = geom_for(&sample());
    let out = draw_flowchart(&c2, &g2, &opts(), "m1", &mut d);
    assert_well_formed(&out.svg);
}

#[test]
fn empty_chart_draws_a_valid_svg() {
    let c = Flowchart::default();
    let out = draw(&c);
    assert_well_formed(&out.svg);
    assert_safe(&out.svg, "m1");
    assert_eq!(out.outline, "Flowchart, top to bottom. 0 nodes, 0 edges.");
}

#[test]
fn large_chart_is_well_formed() {
    let n = 300;
    let mut c = Flowchart {
        nodes: (0..n)
            .map(|i| node(&format!("n{}", i), &format!("Node {}", i)))
            .collect(),
        ..Flowchart::default()
    };
    c.edges = (1..n).map(|i| edge(i - 1, i)).collect();
    for (i, nd) in c.nodes.iter_mut().enumerate() {
        nd.shape = ALL_SHAPES[i % ALL_SHAPES.len()];
    }
    let out = draw(&c);
    assert_well_formed(&out.svg);
    assert_safe(&out.svg, "m1");
    assert_eq!(count(&out.svg, "class=\"merlion-edge\""), n - 1);
}

#[test]
fn subgraph_style_and_class_reach_only_the_box_and_title() {
    let mut c = styled_chart();
    c.subgraphs[0].style = Style {
        fill: Some(Color::Named("red")),
        color: Some(Color::Named("blue")),
        ..Style::default()
    };
    c.subgraphs[0].classes = vec!["hot".into(), "x\"><script>".into()];
    let out = draw(&c);
    let css = style_text(&out.svg);
    assert!(out.svg.contains(
        "<g class=\"merlion-cluster merlion-cc-hot merlion-ss-0\" data-merlion-id=\"build\""
    ));
    assert!(css.contains("#m1 .merlion-ss-0>.merlion-cluster-box{fill:red;}"));
    assert!(css.contains("#m1 .merlion-ss-0>.merlion-cluster-title{fill:blue;}"));
    assert!(css.contains(
        "#m1 .merlion-cc-hot>.merlion-cluster-box{fill:var(--merlion-c-hot-fill, #ff0000);\
         stroke:var(--merlion-c-hot-stroke, hsla(120, 50%, 25%, 0.5));stroke-width:3px;}"
    ));
    assert!(css.contains(
        "#m1 .merlion-cc-hot>.merlion-cluster-title{fill:var(--merlion-c-hot-color, white);font-weight:600;}"
    ));
    // Child combinators only: member nodes keep their own colours.
    assert!(!css.contains(".merlion-ss-0 text") && !css.contains(".merlion-cc-hot text"));
    assert_safe(&out.svg, "m1");
}
