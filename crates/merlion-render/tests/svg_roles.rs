//! Roles on nodes, edges and clusters (specs/svg-output.md#roles, #built-in-roles,
//! #source-styles-classdef-style-linkstyle), checked on computed values in the cascade
//! of `css_support`, on presentation attributes, and against the safety checker.

mod css_support;
mod svg_support;

use css_support::{rgba8, same, Engine};
use merlion_render::color::{oklab_mix, Rgba8};
use merlion_render::{render, Diagnostic, RenderOptions};
use svg_support::{assert_safe, assert_well_formed};

fn run(src: &str) -> (String, Vec<Diagnostic>) {
    let opts = RenderOptions {
        id_prefix: Some("m1".into()),
        ..RenderOptions::default()
    };
    let r = render(src, &opts);
    let svg = r.svg.unwrap_or_else(|| panic!("{:?}", r.diagnostics));
    assert_well_formed(&svg);
    assert_safe(&svg, "m1");
    (svg, r.diagnostics)
}

fn svg_of(src: &str) -> String {
    run(src).0
}

fn mix(a: &str, b: &str, p: f64) -> String {
    oklab_mix(Rgba8::from_hex(a).unwrap(), Rgba8::from_hex(b).unwrap(), p).to_hex()
}

/// The element with `class` under the node, edge or cluster whose attribute `key` is `v`.
fn part(e: &Engine, key: &str, v: &str, class: &str) -> usize {
    let g = (0..e.els.len())
        .find(|&i| e.els[i].attr(key) == Some(v))
        .unwrap_or_else(|| panic!("no {}={}", key, v));
    (0..e.els.len())
        .find(|&i| {
            e.els[i].has_class(class) && {
                let mut p = e.els[i].parent;
                while let Some(x) = p {
                    if x == g {
                        break;
                    }
                    p = e.els[x].parent;
                }
                p == Some(g)
            }
        })
        .unwrap_or_else(|| panic!("no .{} under {}={}", class, key, v))
}

fn node_part(e: &Engine, id: &str, class: &str) -> usize {
    part(e, "data-merlion-id", id, class)
}

fn edge_part(e: &Engine, to: &str, class: &str) -> usize {
    part(e, "data-merlion-to", to, class)
}

fn computed(e: &Engine, el: usize, p: &str) -> String {
    e.computed(el, p).unwrap_or_else(|m| panic!("{}", m))
}

/// Both the `@supports` path and the fallback path, plus the attributes alone.
fn engines(svg: &str, page: &[&str]) -> Vec<Engine> {
    vec![
        Engine::new(svg, &[], page, true, false),
        Engine::new(svg, &[], page, false, false),
    ]
}

const DANGER: &str = "#cf222e";

// ---------------------------------------------------------------------------------------
// Class plumbing
// ---------------------------------------------------------------------------------------

#[test]
fn edges_carry_their_roles_and_get_role_markers() {
    let svg = svg_of(
        "flowchart LR\na e1@--> b\nb e2@--> c\nc e3@--x d\nd --> a\nclass e1 failure\nclass e2 failure,hot\nclass e3 hot",
    );
    assert!(svg.contains("<g class=\"merlion-edge merlion-c-failure\" data-merlion-from=\"a\""));
    assert!(svg.contains(
        "<g class=\"merlion-edge merlion-c-failure merlion-c-hot\" data-merlion-from=\"b\""
    ));
    assert!(svg.contains("<marker id=\"m1-arrow\" "));
    assert!(svg.contains("<marker id=\"m1-arrow-c-failure\" class=\"merlion-c-failure\" "));
    assert!(svg.contains("<marker id=\"m1-cross-c-hot\" class=\"merlion-c-hot\" "));
    assert!(svg.contains("marker-end=\"url(#m1-arrow-c-failure)\""));
    assert!(svg.contains("marker-end=\"url(#m1-cross-c-hot)\""));
    // A role set gets one marker carrying every class of the set.
    assert!(svg.contains("class=\"merlion-c-failure merlion-c-hot\" viewBox"));
    assert_eq!(svg.matches("<marker ").count(), 4);
    // Each marker is defined once however many edges reference it.
    let svg = svg_of("flowchart LR\na e1@--> b\na e2@--> c\nclass e1,e2 failure");
    assert_eq!(svg.matches("id=\"m1-arrow-c-failure\"").count(), 1);
    assert_eq!(svg.matches("url(#m1-arrow-c-failure)").count(), 2);
    assert!(!svg.contains("id=\"m1-arrow\""));
}

#[test]
fn clusters_carry_every_class_with_or_without_a_class_def() {
    let svg = svg_of(
        "flowchart LR\nsubgraph g [G]\na\nend\nsubgraph h [H]\nb\nend\nclass g group,other\nclass h group\nclassDef other stroke-width:2px",
    );
    assert!(svg.contains(
        "<g class=\"merlion-cluster merlion-cc-group merlion-cc-other\" data-merlion-id=\"g\""
    ));
    assert!(svg.contains("<g class=\"merlion-cluster merlion-cc-group\" data-merlion-id=\"h\""));
}

// ---------------------------------------------------------------------------------------
// Built-in roles
// ---------------------------------------------------------------------------------------

#[test]
fn built_in_node_roles_tone_shape_and_label() {
    let src = "flowchart LR\na[A]:::danger --> b[B]:::ok\nb --> c[C]:::warn\nc --> d[D]:::accent\nd --> e[E]:::muted\ne --> f[F]";
    let svg = svg_of(src);
    for (id, tone) in [
        ("a", DANGER),
        ("b", "#1a7f37"),
        ("c", "#9a6700"),
        ("d", "#0969da"),
        ("e", "#7b7d81"),
    ] {
        for e in engines(&svg, &[]) {
            let shape = node_part(&e, id, "merlion-shape");
            let label = node_part(&e, id, "merlion-label");
            assert_eq!(rgba8(&computed(&e, shape, "stroke")), rgba8(tone), "{}", id);
            assert!(
                same(&computed(&e, shape, "fill"), &mix(tone, "#f5f5f5", 14.0), 1),
                "{} {}",
                id,
                computed(&e, shape, "fill")
            );
            assert!(same(
                &computed(&e, label, "fill"),
                &mix(tone, "#1f2328", 75.0),
                1
            ));
        }
    }
    // Presentation attributes carry the mixed literals of the default tone.
    assert!(svg.contains(&format!(
        "fill=\"{}\" stroke=\"{}\"",
        mix(DANGER, "#f5f5f5", 14.0),
        DANGER
    )));
    assert!(svg.contains(&format!(
        "<text class=\"merlion-label\" fill=\"{}\">",
        mix(DANGER, "#1f2328", 75.0)
    )));
    // The untoned node keeps the defaults.
    let e = Engine::new(&svg, &[], &[], true, false);
    assert_eq!(
        rgba8(&computed(&e, node_part(&e, "f", "merlion-shape"), "stroke")),
        rgba8("#c8c9cb")
    );
}

#[test]
fn built_in_tones_follow_the_page_theme_and_role_rules() {
    let svg = svg_of("flowchart LR\na[A]:::danger --> b[B]\nb e1@--> c[C]\nclass e1 failure");
    // A theme that sets --merlion-danger retunes every danger and failure element.
    let page = ":root{--merlion-danger:#ff0000;}";
    let e = Engine::new(&svg, &[], &[page], true, false);
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "stroke"),
        "#ff0000"
    );
    assert_eq!(
        computed(&e, edge_part(&e, "c", "merlion-edge-path"), "stroke"),
        "#ff0000"
    );
    let path = edge_part(&e, "c", "merlion-edge-path");
    assert_eq!(
        computed(&e, e.marker_path(path).unwrap(), "fill"),
        "#ff0000"
    );
    // A page or stylesheet rule on the role replaces the built-in tone.
    let page = ".merlion .merlion-c-danger{--merlion-tone:#00aa00;}.merlion .merlion-c-failure{--merlion-tone:#0000aa;--merlion-dash:none;}";
    let e = Engine::new(&svg, &[], &[page], true, false);
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "stroke"),
        "#00aa00"
    );
    let path = edge_part(&e, "c", "merlion-edge-path");
    assert_eq!(computed(&e, path, "stroke"), "#0000aa");
    assert_eq!(computed(&e, path, "stroke-dasharray"), "none");
    assert_eq!(
        computed(&e, e.marker_path(path).unwrap(), "fill"),
        "#0000aa"
    );
    // A tone on :root still reaches nothing.
    let e = Engine::new(&svg, &[], &[":root{--merlion-tone:#00ff00;}"], true, false);
    assert_eq!(
        rgba8(&computed(&e, node_part(&e, "b", "merlion-shape"), "stroke")),
        rgba8("#c8c9cb")
    );
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "stroke"),
        DANGER
    );
}

#[test]
fn built_in_edge_and_cluster_roles() {
    let svg = svg_of(
        "flowchart LR\nsubgraph g [G]\na --> b\nend\nb e1@--> c\nc e2@-.-> d\nd e3@--> e\nclass e1 failure\nclass e2 async\nclass g group",
    );
    for e in engines(&svg, &[]) {
        let failure = edge_part(&e, "c", "merlion-edge-path");
        assert_eq!(computed(&e, failure, "stroke"), DANGER);
        assert_eq!(computed(&e, failure, "stroke-dasharray"), "6 4");
        assert_eq!(
            computed(&e, e.marker_path(failure).unwrap(), "fill"),
            DANGER
        );
        let asynch = edge_part(&e, "d", "merlion-edge-path");
        assert_eq!(computed(&e, asynch, "stroke-dasharray"), "6 4");
        assert_eq!(rgba8(&computed(&e, asynch, "stroke")), rgba8("#919497"));
        let plain = edge_part(&e, "e", "merlion-edge-path");
        assert_eq!(computed(&e, plain, "stroke-dasharray"), "none");
        let bx = part(&e, "data-merlion-id", "g", "merlion-cluster-box");
        assert_eq!(computed(&e, bx, "stroke-dasharray"), "6 4");
        // Members of a `group` cluster keep solid borders.
        assert_eq!(
            computed(&e, node_part(&e, "a", "merlion-shape"), "stroke-dasharray"),
            "none"
        );
    }
    assert!(svg.contains(&format!(
        "fill=\"none\" stroke=\"{}\" stroke-width=\"1.25\" stroke-dasharray=\"6 4\"",
        DANGER
    )));
    assert!(svg.contains("stroke-dasharray=\"6 4\" marker-end=\"url(#m1-arrow-c-async)\""));
}

#[test]
fn built_in_rules_appear_only_for_roles_the_diagram_uses_on_their_element_kind() {
    let plain = svg_of("flowchart LR\na --> b");
    for r in [
        "danger", "failure", "async", "group", "accent", "ok", "warn", "muted",
    ] {
        assert!(!plain.contains(&format!("merlion-c-{}", r)));
        assert!(!plain.contains(&format!("merlion-cc-{}", r)));
    }
    // A node role on an edge or a cluster, and an edge role on a node, have no style.
    let svg = svg_of(
        "flowchart LR\nsubgraph g [G]\na:::failure\nend\na e1@--> b\nclass e1 danger\nclass g danger",
    );
    let style = &svg[svg.find("<style>").unwrap()..svg.find("</style>").unwrap()];
    assert!(!style.contains("merlion-c-danger"), "{}", style);
    assert!(!style.contains("merlion-c-failure"));
    assert!(!style.contains("merlion-cc-danger"));
    let e = Engine::new(&svg, &[], &[], true, false);
    assert_eq!(
        rgba8(&computed(
            &e,
            edge_part(&e, "b", "merlion-edge-path"),
            "stroke"
        )),
        rgba8("#919497")
    );
    assert_eq!(
        rgba8(&computed(&e, node_part(&e, "a", "merlion-shape"), "stroke")),
        rgba8("#c8c9cb")
    );
}

// ---------------------------------------------------------------------------------------
// classDef colours
// ---------------------------------------------------------------------------------------

#[test]
fn class_def_colours_read_their_token_with_the_literal_fallback() {
    let (svg, d) = run(
        "flowchart LR\na[A]:::hot --> b[B]\nclassDef hot fill:#ff0000,stroke:#00ff00,color:#0000ff,stroke-width:2px",
    );
    assert!(svg.contains(
        "#m1 .merlion-c-hot>.merlion-shape{fill:var(--merlion-c-hot-fill, #ff0000);\
         stroke:var(--merlion-c-hot-stroke, #00ff00);stroke-width:2px;}"
    ));
    assert!(svg.contains("#m1 .merlion-c-hot text{fill:var(--merlion-c-hot-color, #0000ff);}"));
    // Attributes carry the literal for CSS-less renderers.
    assert!(svg.contains("fill=\"#ff0000\" stroke=\"#00ff00\""));
    assert!(svg.contains("<text class=\"merlion-label\" fill=\"#0000ff\">"));
    let e = Engine::new(&svg, &[], &[], true, false);
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "fill"),
        "#ff0000"
    );
    // A page (or compiled stylesheet) re-themes the class through its tokens.
    let page = "[data-theme=\"dark\"]{--merlion-c-hot-fill:#112233;--merlion-c-hot-color:#eeeeee;}";
    let e = Engine::new(&svg, &[("data-theme", "dark")], &[page], true, false);
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "fill"),
        "#112233"
    );
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "stroke"),
        "#00ff00"
    );
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-label"), "fill"),
        "#eeeeee"
    );
    let i030: Vec<_> = d.iter().filter(|x| x.code == "I030").collect();
    assert_eq!(i030.len(), 1);
    assert!(
        i030[0].message.contains("unless a stylesheet or page sets"),
        "{}",
        i030[0].message
    );
}

#[test]
fn style_colours_stay_literal_and_win_over_class_defs() {
    let svg = svg_of(
        "flowchart LR\na[A]:::hot --> b[B]\nclassDef hot fill:#ff0000\nstyle a fill:#00ff00\nlinkStyle 0 stroke:#0000ff",
    );
    assert!(svg.contains("{fill:#00ff00;}"));
    assert!(svg.contains("stroke:#0000ff;"));
    let e = Engine::new(
        &svg,
        &[],
        &[":root{--merlion-c-hot-fill:#123456;}"],
        true,
        false,
    );
    assert_eq!(
        computed(&e, node_part(&e, "a", "merlion-shape"), "fill"),
        "#00ff00"
    );
    assert!(
        svg.contains("fill=\"#00ff00\""),
        "attribute carries the style literal"
    );
}

#[test]
fn class_defs_win_over_built_in_roles_property_by_property() {
    let svg = svg_of("flowchart LR\na[A]:::danger --> b\nclass a hot\nclassDef hot fill:#ffeeaa");
    for e in engines(&svg, &[]) {
        let shape = node_part(&e, "a", "merlion-shape");
        assert_eq!(computed(&e, shape, "fill"), "#ffeeaa");
        assert_eq!(computed(&e, shape, "stroke"), DANGER);
    }
    assert!(svg.contains(&format!("fill=\"#ffeeaa\" stroke=\"{}\"", DANGER)));
}

#[test]
fn class_defs_reach_edges_that_carry_the_class() {
    let svg = svg_of("flowchart LR\na e1@--> b\na --> c\nclass e1 hot\nclassDef hot stroke:#ff00ff,color:#00ffff");
    let e = Engine::new(&svg, &[], &[], true, false);
    assert_eq!(
        computed(&e, edge_part(&e, "b", "merlion-edge-path"), "stroke"),
        "#ff00ff"
    );
    assert_eq!(
        rgba8(&computed(
            &e,
            edge_part(&e, "c", "merlion-edge-path"),
            "stroke"
        )),
        rgba8("#919497")
    );
    assert!(svg.contains("stroke=\"#ff00ff\""));
}

#[test]
fn class_defs_used_only_by_nodes_add_no_edge_rules() {
    let svg = svg_of("flowchart LR\na:::hot --> b\nclassDef hot stroke:#ff00ff");
    assert!(!svg.contains("merlion-c-hot>.merlion-edge-path"));
}

#[test]
fn hostile_role_names_never_reach_the_output() {
    let (svg, _) = run("flowchart LR\na e1@--> b\nclass e1 x\"><script>\nclass a y}body{x\nsubgraph g\nc\nend\nclass g z<>");
    assert!(!svg.contains("script") && !svg.contains("body{"));
}

// ---------------------------------------------------------------------------------------
// Cost bounds
// ---------------------------------------------------------------------------------------

fn render_plain(src: &str) -> merlion_render::RenderResult {
    let opts = RenderOptions {
        id_prefix: Some("m1".into()),
        ..RenderOptions::default()
    };
    render(src, &opts)
}

#[test]
fn a_shared_edge_id_cannot_fan_roles_out_across_edges() {
    let mut base = String::from("flowchart LR\n");
    for _ in 0..400 {
        base.push_str("a zq@--> b\n");
    }
    let mut src = base.clone();
    for i in 0..2000 {
        src.push_str(&format!("class zq c{}\n", i));
    }
    let b = render_plain(&base);
    let r = render_plain(&src);
    let (bs, rs) = (b.svg.unwrap(), r.svg.unwrap());
    // The id stays with the first edge, which keeps at most 32 roles.
    assert_eq!(rs.matches("<g class=\"merlion-edge merlion-c-").count(), 1);
    assert!(
        rs.len() < bs.len() + 16 * 1024,
        "{} bytes vs {} without roles",
        rs.len(),
        bs.len()
    );
    assert!(r.fuel_used > b.fuel_used, "roles are charged to fuel");
}

#[test]
fn applied_roles_are_charged_to_fuel() {
    let plain = render_plain("flowchart LR\na --> b");
    let roled = render_plain("flowchart LR\na e1@--> b\nclass a,e1 r1,r2,r3");
    assert_eq!(roled.fuel_used, plain.fuel_used + 6);
}
