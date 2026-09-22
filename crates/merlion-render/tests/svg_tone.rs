//! Per-element tone and dash tokens (specs/svg-output.md#roles), checked on computed
//! values: an untoned diagram draws exactly the colours it drew before the tone rules
//! existed, a tone on a group recolours only that group, and a tone set on an ancestor
//! never reaches member elements.

mod css_support;

use css_support::{rgba8, same, Engine};
use merlion_render::{render, RenderOptions};

/// The embedded style of the flowchart below before the tone, dash and reset rules
/// (id `m1`, with the detail rule).
const BEFORE: &str = "#m1 text { font-family: var(--merlion-font, Inter, ui-sans-serif, system-ui, sans-serif); font-size: var(--merlion-font-size, 14px); font-weight: 400; font-style: normal; font-stretch: normal; font-kerning: normal; font-variant-ligatures: none; font-feature-settings: \"calt\" 0, \"liga\" 0; letter-spacing: 0; word-spacing: 0; text-transform: none; }#m1 .merlion-bg{fill:var(--merlion-bg, #ffffff);}#m1 .merlion-label,#m1 .merlion-edge-text,#m1 .merlion-cluster-title{text-anchor:start;white-space:pre;}#m1 .merlion-label{fill:var(--merlion-node-text, var(--merlion-fg, #1f2328));}#m1 .merlion-edge-text,#m1 .merlion-cluster-title{fill:var(--merlion-fg, #1f2328);}#m1 .merlion-b{font-weight:600;}#m1 .merlion-i{font-style:italic;}#m1 .merlion-code{font-family:ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;}#m1 .merlion-node>.merlion-shape{fill:var(--merlion-node-bg, var(--merlion-surface, #f5f5f5));stroke:var(--merlion-node-border, var(--merlion-border, #c8c9cb));stroke-width:var(--merlion-stroke, 1.25px);}#m1 .merlion-cluster>.merlion-cluster-box{fill:var(--merlion-cluster-bg, #fafafa);stroke:var(--merlion-cluster-border, var(--merlion-border, #c8c9cb));stroke-width:var(--merlion-stroke, 1.25px);}#m1 .merlion-edge>.merlion-edge-path{fill:none;stroke:var(--merlion-edge, var(--merlion-line, #919497));stroke-width:var(--merlion-stroke, 1.25px);}#m1 .merlion-edge>.merlion-thick{stroke-width:calc(var(--merlion-stroke, 1.25px) * 2);}#m1 .merlion-edge>.merlion-dotted{stroke-dasharray:3 3;}#m1 .merlion-marker-fill{fill:var(--merlion-edge, var(--merlion-line, #919497));stroke:none;}#m1 .merlion-marker-stroke{fill:none;stroke:var(--merlion-edge, var(--merlion-line, #919497));stroke-width:1.5px;}#m1 .merlion-edge-label>.merlion-edge-label-bg{fill:var(--merlion-edge-label-bg, var(--merlion-bg, #ffffff));}#m1 .merlion-detail{fill:var(--merlion-node-detail, var(--merlion-muted, #7b7d81));font-size:11.2px;}@supports (color: color-mix(in oklab, #000, #fff)){#m1 .merlion-node>.merlion-shape{fill:var(--merlion-node-bg, var(--merlion-surface, color-mix(in oklab, var(--merlion-fg, #1f2328) 4%, var(--merlion-bg, #ffffff))));stroke:var(--merlion-node-border, var(--merlion-border, color-mix(in oklab, var(--merlion-fg, #1f2328) 22%, var(--merlion-bg, #ffffff))));}#m1 .merlion-cluster>.merlion-cluster-box{fill:var(--merlion-cluster-bg, color-mix(in oklab, var(--merlion-fg, #1f2328) 2%, var(--merlion-bg, #ffffff)));stroke:var(--merlion-cluster-border, var(--merlion-border, color-mix(in oklab, var(--merlion-fg, #1f2328) 22%, var(--merlion-bg, #ffffff))));}#m1 .merlion-edge>.merlion-edge-path{stroke:var(--merlion-edge, var(--merlion-line, color-mix(in oklab, var(--merlion-fg, #1f2328) 45%, var(--merlion-bg, #ffffff))));}#m1 .merlion-marker-fill{fill:var(--merlion-edge, var(--merlion-line, color-mix(in oklab, var(--merlion-fg, #1f2328) 45%, var(--merlion-bg, #ffffff))));}#m1 .merlion-marker-stroke{stroke:var(--merlion-edge, var(--merlion-line, color-mix(in oklab, var(--merlion-fg, #1f2328) 45%, var(--merlion-bg, #ffffff))));}#m1 .merlion-detail{fill:var(--merlion-node-detail, var(--merlion-muted, color-mix(in oklab, var(--merlion-fg, #1f2328) 55%, var(--merlion-bg, #ffffff))));}}";

const SOURCE: &str = "flowchart LR
  subgraph s [S]
    a[\"**T**<br/>detail\"] -->|yes| b[B]
    a -.-> c[C]
  end
  b ==> d[D]
  c --x e[E]
  c --o f[F]
";

fn svg_of(src: &str) -> String {
    let opts = RenderOptions {
        id_prefix: Some("m1".into()),
        ..RenderOptions::default()
    };
    render(src, &opts).svg.expect("renders")
}

fn with_style(svg: &str, css: &str) -> String {
    let a = svg.find("<style>").unwrap() + 7;
    let b = svg.find("</style>").unwrap();
    format!("{}{}{}", &svg[..a], css, &svg[b..])
}

const ELEMENTS: [&str; 10] = [
    "merlion-shape",
    "merlion-label",
    "merlion-detail",
    "merlion-edge-text",
    "merlion-edge-label-bg",
    "merlion-cluster-title",
    "merlion-cluster-box",
    "merlion-edge-path",
    "merlion-marker-fill",
    "merlion-marker-stroke",
];
const PROPS: [&str; 4] = ["fill", "stroke", "stroke-width", "stroke-dasharray"];

/// Host themes as page CSS: none, a dark foundation pair, every role overridden, and a
/// host that sets the per-element tokens on `:root` (which must not reach anything).
fn hosts() -> Vec<&'static str> {
    vec![
        "",
        ":root{--merlion-bg:#16191d;--merlion-fg:#e3e6ea;}",
        ":root{--merlion-node-bg:#123456;--merlion-node-border:#654321;--merlion-node-text:#0f0f0f;\
         --merlion-edge:#abcdef;--merlion-cluster-bg:#eeeeee;--merlion-cluster-border:#111111;\
         --merlion-muted:#777777;--merlion-edge-label-bg:#fefefe;--merlion-fg:#202020;}",
        ":root{--merlion-tone:#ff0000;--merlion-dash:1 1;}",
        "[data-theme=\"x\"]{--merlion-tone:#00ff00;}.merlion{--merlion-tone:#0000ff;--merlion-dash:2 2;}",
    ]
}

#[test]
fn untoned_output_computes_the_same_values_as_before() {
    let now = svg_of(SOURCE);
    let before = with_style(&now, BEFORE);
    let mut checked = 0;
    for host in hosts() {
        for supports in [true, false] {
            let attrs: &[(&str, &str)] = &[("data-theme", "x")];
            let new = Engine::new(&now, attrs, &[host], supports, false);
            let old = Engine::new(&before, attrs, &[host], supports, false);
            for class in ELEMENTS {
                let els = new.with_class(class);
                assert!(!els.is_empty(), "no {}", class);
                for e in els {
                    for p in PROPS {
                        let (a, b) = (old.computed(e, p), new.computed(e, p));
                        let (a, b) = (a.unwrap(), b.unwrap_or_else(|m| panic!("{}: {}", class, m)));
                        assert!(
                            same(&a, &b, 0),
                            "{} {} host {:?} supports {}: before {:?}, now {:?}",
                            class,
                            p,
                            host,
                            supports,
                            a,
                            b
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 500, "{}", checked);
}

#[test]
fn reset_rule_has_zero_specificity_and_declares_only_the_two_tokens() {
    let svg = svg_of(SOURCE);
    assert!(svg.contains(
        ":where(#m1 .merlion-node, #m1 .merlion-edge, #m1 .merlion-cluster, #m1 marker)\
         {--merlion-tone:initial;--merlion-dash:initial;}"
    ));
    let style = &svg[svg.find("<style>").unwrap()..svg.find("</style>").unwrap()];
    assert_eq!(
        style.matches("{--").count() + style.matches(";--").count(),
        2
    );
}

fn tone_page(sel: &str) -> String {
    format!("{}{{--merlion-tone:#cf222e;--merlion-dash:5 1;}}", sel)
}

#[test]
fn a_tone_on_a_node_group_recolours_its_shape_and_label() {
    let svg = svg_of(SOURCE);
    let page = tone_page(".merlion .merlion-node[data-merlion-id=\"b\"]");
    let e = Engine::new(&svg, &[], &[&page], true, false);
    let b = e
        .with_class("merlion-node")
        .into_iter()
        .find(|&i| e.els[i].attr("data-merlion-id") == Some("b"))
        .unwrap();
    let kid = |class: &str| {
        (0..e.els.len())
            .find(|&i| e.els[i].parent == Some(b) && e.els[i].has_class(class))
            .unwrap()
    };
    let shape = kid("merlion-shape");
    let label = kid("merlion-label");
    assert_eq!(e.computed(shape, "stroke").unwrap(), "#cf222e");
    assert_eq!(e.computed(shape, "stroke-dasharray").unwrap(), "5 1");
    let fill = e.computed(shape, "fill").unwrap();
    assert!(
        same(&fill, "color-mix(in oklab, #cf222e 14%, #f5f5f5)", 1),
        "{}",
        fill
    );
    let text = e.computed(label, "fill").unwrap();
    assert!(
        same(&text, "color-mix(in oklab, #cf222e 75%, #1f2328)", 0),
        "{}",
        text
    );
    // Other nodes stay untoned.
    for s in e.with_class("merlion-shape") {
        if s != shape {
            assert_eq!(rgba8(&e.computed(s, "stroke").unwrap()), rgba8("#c8c9cb"));
        }
    }
}

#[test]
fn a_tone_on_a_cluster_reaches_its_box_and_title_but_not_its_members() {
    let svg = svg_of(SOURCE);
    let page = tone_page(".merlion .merlion-cluster");
    let e = Engine::new(&svg, &[], &[&page], true, false);
    let bx = e.with_class("merlion-cluster-box")[0];
    assert_eq!(e.computed(bx, "stroke").unwrap(), "#cf222e");
    assert_eq!(e.computed(bx, "stroke-dasharray").unwrap(), "5 1");
    assert!(same(
        &e.computed(bx, "fill").unwrap(),
        "color-mix(in oklab, #cf222e 8%, #fafafa)",
        1
    ));
    let title = e.with_class("merlion-cluster-title")[0];
    assert!(same(
        &e.computed(title, "fill").unwrap(),
        "color-mix(in oklab, #cf222e 75%, #1f2328)",
        0
    ));
    for s in e.with_class("merlion-shape") {
        assert_eq!(rgba8(&e.computed(s, "stroke").unwrap()), rgba8("#c8c9cb"));
        assert_eq!(e.computed(s, "stroke-dasharray").unwrap(), "none");
    }
    for p in e.with_class("merlion-edge-path") {
        assert_eq!(rgba8(&e.computed(p, "stroke").unwrap()), rgba8("#919497"));
    }
}

#[test]
fn a_tone_on_an_edge_group_recolours_path_and_label() {
    let svg = svg_of(SOURCE);
    let page = tone_page(".merlion .merlion-edge[data-merlion-to=\"b\"]");
    let e = Engine::new(&svg, &[], &[&page], true, false);
    let edge = e
        .with_class("merlion-edge")
        .into_iter()
        .find(|&i| e.els[i].attr("data-merlion-to") == Some("b"))
        .unwrap();
    let path = (0..e.els.len())
        .find(|&i| e.els[i].parent == Some(edge) && e.els[i].has_class("merlion-edge-path"))
        .unwrap();
    assert_eq!(e.computed(path, "stroke").unwrap(), "#cf222e");
    assert_eq!(e.computed(path, "stroke-dasharray").unwrap(), "5 1");
    let text = (0..e.els.len())
        .find(|&i| e.els[i].has_class("merlion-edge-text"))
        .unwrap();
    assert!(same(
        &e.computed(text, "fill").unwrap(),
        "color-mix(in oklab, #cf222e 75%, #1f2328)",
        0
    ));
}

#[test]
fn without_color_mix_a_toned_node_keeps_its_untoned_fill() {
    let svg = svg_of(SOURCE);
    let page = tone_page(".merlion .merlion-node");
    let e = Engine::new(&svg, &[], &[&page], false, false);
    for s in e.with_class("merlion-shape") {
        assert_eq!(e.computed(s, "fill").unwrap(), "#f5f5f5");
        assert_eq!(e.computed(s, "stroke").unwrap(), "#cf222e");
    }
}
