//! Title + detail node labels end to end (specs/svg-output.md#text,
//! specs/text-measurement.md#measuring).

mod parse_support;

use std::path::PathBuf;

use merlion_render::diag::Diagnostics;
use merlion_render::text::{is_title_detail, layout_label, layout_node_label, TextStyle};
use merlion_render::{render, RenderOptions};
use parse_support::parse_ok;

fn svg(src: &str) -> String {
    let r = render(src, &RenderOptions::default());
    assert!(r.error.is_none(), "{:?}", r.diagnostics);
    r.svg.unwrap()
}

fn root_id(svg: &str) -> String {
    let at = svg.find(" id=\"").unwrap() + 5;
    svg[at..at + svg[at..].find('"').unwrap()].to_string()
}

fn style_text(svg: &str) -> &str {
    let a = svg.find("<style>").unwrap() + 7;
    &svg[a..a + svg[a..].find("</style>").unwrap()]
}

const OBSERVE: &str =
    "flowchart LR\n  q[\"**q-observe**<br/>250 push slots<br/>separate invocations\"] --> r[plain]\n";

#[test]
fn detail_lines_are_marked_sized_and_muted() {
    let s = svg(OBSERVE);
    assert!(
        s.contains(
            "class=\"merlion-detail\" font-size=\"11.2\" fill=\"#7b7d81\">250 push slots</tspan>"
        ),
        "{s}"
    );
    assert!(s.contains(
        "class=\"merlion-detail\" font-size=\"11.2\" fill=\"#7b7d81\">separate invocations</tspan>"
    ));
    assert!(s.contains("class=\"merlion-b\" font-weight=\"600\">q-observe</tspan>"));
    assert_eq!(s.matches("merlion-detail\"").count(), 2);
}

#[test]
fn detail_rule_is_scoped_and_themeable() {
    let s = svg(OBSERVE);
    let id = root_id(&s);
    let css = style_text(&s);
    assert!(
        css.contains(&format!(
            "#{id} .merlion-detail{{fill:var(--merlion-node-detail, var(--merlion-muted, \
             #7b7d81));font-size:11.2px;}}"
        )),
        "{css}"
    );
    // Every occurrence of the class in the style belongs to an id-scoped rule.
    assert_eq!(
        css.matches(".merlion-detail").count(),
        css.matches(&format!("#{id} .merlion-detail")).count()
    );
}

#[test]
fn diagrams_without_the_pattern_have_no_detail_rule() {
    for src in [
        "flowchart LR\n  a[\"**only title**\"] --> b[\"x<br/>y\"]\n",
        "flowchart LR\n  a[\"**a** b<br/>x\"] --> b[\"x<br/>**t**\"]\n",
        "flowchart LR\n  a --> |\"**edge**<br/>detail\"| b\n",
    ] {
        let s = svg(src);
        assert!(!s.contains("merlion-detail"), "{src}");
    }
}

#[test]
fn source_colour_styles_the_title_and_detail_stays_muted() {
    let src = format!("{OBSERVE}  classDef hot color:#ff0000\n  class q hot\n");
    let s = svg(&src);
    let id = root_id(&s);
    let css = style_text(&s);
    assert!(
        css.contains(&format!("#{id} .merlion-c-hot text{{")),
        "{css}"
    );
    assert!(s.contains("class=\"merlion-detail\" font-size=\"11.2\" fill=\"#7b7d81\""));
}

#[test]
fn outline_keeps_every_line_as_plain_text() {
    let r = render(OBSERVE, &RenderOptions::default());
    let outline = r.outline.unwrap();
    assert!(outline.contains("q-observe"), "{outline}");
    assert!(outline.contains("250 push slots"), "{outline}");
    assert!(outline.contains("separate invocations"), "{outline}");
    assert!(!outline.contains("**"), "{outline}");
}

#[test]
fn detail_size_follows_the_font_size_option() {
    let opts = RenderOptions {
        font_size: 20.0,
        ..RenderOptions::default()
    };
    let s = render(OBSERVE, &opts).svg.unwrap();
    assert!(s.contains("font-size=\"16\" fill=\"#7b7d81\">250 push slots"));
    assert!(style_text(&s).contains("font-size:16px;}"));
}

#[test]
fn render_is_deterministic() {
    let src = fixture("two-tier-labels.mmd");
    let a = svg(&src);
    let b = svg(&src);
    assert_eq!(a, b);
    assert!(a.contains("merlion-detail"));
}

fn fixtures() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flowcharts");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

fn fixture(name: &str) -> String {
    fixtures()
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, s)| s)
        .unwrap()
}

/// Only `two-tier-labels.mmd` uses the pattern. In every other fixture each node label
/// measures exactly as before (the node layout equals the plain one) and the SVG carries
/// no trace of the feature.
#[test]
fn fixtures_outside_the_pattern_are_unchanged() {
    let mut tiered = Vec::new();
    for (name, src) in fixtures() {
        let (chart, _) = parse_ok(&src);
        let mut any = false;
        for node in &chart.nodes {
            let mut d = Diagnostics::new(false);
            let style = TextStyle::default();
            let n = layout_node_label(&node.label, &style, 200.0, &mut d);
            if is_title_detail(&node.label) {
                any = true;
                assert!(n.lines.iter().any(|l| l.detail), "{name}: {}", node.id);
            } else {
                assert_eq!(
                    n,
                    layout_label(&node.label, &style, 200.0, &mut d),
                    "{name}"
                );
            }
        }
        let s = svg(&src);
        assert_eq!(s.contains("merlion-detail"), any, "{name}");
        if any {
            tiered.push(name);
        }
    }
    assert_eq!(tiered, ["two-tier-labels.mmd"]);
}
