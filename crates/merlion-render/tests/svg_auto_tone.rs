//! Automatic tones (specs/svg-output.md#automatic-tones): decisions, stores, terminals
//! and top-level clusters take a built-in role with no roles written; explicit styling
//! replaces the automatic tone per element; every opt-out draws the untoned output.

mod css_support;
mod svg_support;

use css_support::{rgba8, same, Engine};
use merlion_render::color::{oklab_mix, Rgba8};
use merlion_render::stylesheet::{compile, StylesheetLimits};
use merlion_render::svg::theme::{self, SERIES, STORE};
use merlion_render::{render, Diagnostic, RenderOptions};
use svg_support::{assert_safe, assert_well_formed};

fn opts(auto_tone: bool) -> RenderOptions {
    RenderOptions {
        id_prefix: Some("m1".into()),
        auto_tone,
        ..RenderOptions::default()
    }
}

fn run_with(src: &str, o: &RenderOptions) -> (String, Vec<Diagnostic>) {
    let r = render(src, o);
    let svg = r.svg.unwrap_or_else(|| panic!("{:?}", r.diagnostics));
    assert_well_formed(&svg);
    assert_safe(&svg, o.id_prefix.as_deref().unwrap_or("m"));
    (svg, r.diagnostics)
}

fn svg_of(src: &str) -> String {
    run_with(src, &opts(true)).0
}

fn mix(a: &str, b: &str, p: f64) -> String {
    oklab_mix(Rgba8::from_hex(a).unwrap(), Rgba8::from_hex(b).unwrap(), p).to_hex()
}

/// The `class` attribute of the node or cluster group with `data-merlion-id="{id}"`.
fn group_class<'a>(svg: &'a str, id: &str) -> &'a str {
    let at = svg
        .find(&format!("data-merlion-id=\"{}\"", id))
        .unwrap_or_else(|| panic!("no {}", id));
    let open = svg[..at].rfind("<g class=\"").unwrap() + "<g class=\"".len();
    &svg[open..open + svg[open..].find('"').unwrap()]
}

/// The element with `class` inside the group whose `data-merlion-id` is `id`.
fn part(e: &Engine, id: &str, class: &str) -> usize {
    let g = (0..e.els.len())
        .find(|&i| e.els[i].attr("data-merlion-id") == Some(id))
        .unwrap_or_else(|| panic!("no {}", id));
    (0..e.els.len())
        .find(|&i| e.els[i].has_class(class) && e.els[i].parent == Some(g))
        .unwrap_or_else(|| panic!("no .{} under {}", class, id))
}

fn computed(e: &Engine, el: usize, p: &str) -> String {
    e.computed(el, p).unwrap_or_else(|m| panic!("{}", m))
}

/// The `@supports` path and the fallback path.
fn engines(svg: &str, page: &[&str]) -> Vec<Engine> {
    vec![
        Engine::new(svg, &[], page, true, false),
        Engine::new(svg, &[], page, false, false),
    ]
}

const BORDER: &str = "#c8c9cb";
const NODE_BG: &str = "#f5f5f5";
const CLUSTER_BG: &str = "#fafafa";
const FG: &str = "#1f2328";

fn assert_node_tone(svg: &str, id: &str, tone: &str) {
    for e in engines(svg, &[]) {
        let shape = part(&e, id, "merlion-shape");
        assert_eq!(
            rgba8(&computed(&e, shape, "stroke")),
            rgba8(tone),
            "{} stroke",
            id
        );
        assert!(
            same(&computed(&e, shape, "fill"), &mix(tone, NODE_BG, 14.0), 1),
            "{} fill {}",
            id,
            computed(&e, shape, "fill")
        );
        let label = part(&e, id, "merlion-label");
        assert!(same(&computed(&e, label, "fill"), &mix(tone, FG, 75.0), 1));
    }
}

fn assert_node_untoned(svg: &str, id: &str) {
    for e in engines(svg, &[]) {
        let shape = part(&e, id, "merlion-shape");
        assert_eq!(
            rgba8(&computed(&e, shape, "stroke")),
            rgba8(BORDER),
            "{} stroke",
            id
        );
    }
}

// ---------------------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------------------

const SHAPES: &str = "flowchart LR
  r[Rect] --> d{Decide}
  d --> h{{Prepare}}
  h --> c[(Store)]
  c --> s([Start])
  s --> o((Circle))
  o --> w(((Stop)))
  w --> n(Round)
  n --> u[[Sub]]
  u --> hc@{ shape: h-cyl, label: \"Disk\" }
";

#[test]
fn shapes_map_to_built_in_roles() {
    let svg = svg_of(SHAPES);
    for (id, role) in [
        ("d", "warn"),
        ("h", "warn"),
        ("c", "store"),
        ("s", "ok"),
        ("o", "ok"),
        ("w", "ok"),
    ] {
        assert_eq!(
            group_class(&svg, id),
            format!("merlion-node merlion-c-{} merlion-auto", role),
            "{}",
            id
        );
    }
    for id in ["r", "n", "u", "hc"] {
        assert_eq!(group_class(&svg, id), "merlion-node", "{}", id);
    }
    assert_node_tone(&svg, "d", theme::WARN);
    assert_node_tone(&svg, "h", theme::WARN);
    assert_node_tone(&svg, "c", STORE);
    assert_node_tone(&svg, "s", theme::OK);
    assert_node_untoned(&svg, "r");
    // Presentation attributes carry the mixed literals for CSS-less renderers.
    assert!(svg.contains(&format!(
        "fill=\"{}\" stroke=\"{}\"",
        mix(STORE, NODE_BG, 14.0),
        STORE
    )));
}

#[test]
fn edges_get_no_automatic_tone() {
    let svg = svg_of(SHAPES);
    assert!(!svg.contains("<g class=\"merlion-edge merlion"));
    for e in engines(&svg, &[]) {
        let edge = (0..e.els.len())
            .find(|&i| e.els[i].attr("data-merlion-from") == Some("d"))
            .unwrap();
        let path = (0..e.els.len())
            .find(|&i| e.els[i].parent == Some(edge) && e.els[i].has_class("merlion-edge-path"))
            .unwrap();
        assert_eq!(rgba8(&computed(&e, path, "stroke")), rgba8("#919497"));
    }
}

#[test]
fn store_token_and_page_themes_retune_automatic_tones() {
    let svg = svg_of(SHAPES);
    let page = ":root{--merlion-warn:#ff0000;--merlion-store:#00aa00;}";
    let e = Engine::new(&svg, &[], &[page], true, false);
    assert_eq!(
        computed(&e, part(&e, "d", "merlion-shape"), "stroke"),
        "#ff0000"
    );
    assert_eq!(
        computed(&e, part(&e, "c", "merlion-shape"), "stroke"),
        "#00aa00"
    );
    // A page rule on the role restyles automatic and explicit elements alike; the
    // marker class lets it single out automatic ones.
    let page = ".merlion .merlion-c-warn.merlion-auto{--merlion-tone:#0000aa;}";
    let e = Engine::new(&svg, &[], &[page], true, false);
    assert_eq!(
        computed(&e, part(&e, "d", "merlion-shape"), "stroke"),
        "#0000aa"
    );
}

// ---------------------------------------------------------------------------------------
// Clusters
// ---------------------------------------------------------------------------------------

fn clusters(n: usize) -> String {
    let mut s = String::from("flowchart LR\n");
    for i in 0..n {
        s.push_str(&format!("subgraph g{i} [G{i}]\n  n{i}[N{i}]\nend\n"));
    }
    s
}

#[test]
fn top_level_clusters_cycle_through_the_series() {
    let svg = svg_of(&clusters(9));
    for i in 0..9 {
        let k = i % 8 + 1;
        assert_eq!(
            group_class(&svg, &format!("g{i}")),
            format!("merlion-cluster merlion-cc-series-{} merlion-auto", k),
            "g{}",
            i
        );
    }
    for e in engines(&svg, &[]) {
        for (i, tone) in [(0, SERIES[0]), (4, SERIES[4]), (8, SERIES[0])] {
            let id = format!("g{i}");
            let bx = part(&e, &id, "merlion-cluster-box");
            assert_eq!(rgba8(&computed(&e, bx, "stroke")), rgba8(tone));
            assert!(same(
                &computed(&e, bx, "fill"),
                &mix(tone, CLUSTER_BG, 8.0),
                1
            ));
            let title = part(&e, &id, "merlion-cluster-title");
            assert!(same(&computed(&e, title, "fill"), &mix(tone, FG, 75.0), 1));
            // Member nodes stay neutral.
            let shape = part(&e, &format!("n{i}"), "merlion-shape");
            assert_eq!(rgba8(&computed(&e, shape, "stroke")), rgba8(BORDER));
            assert_eq!(rgba8(&computed(&e, shape, "fill")), rgba8(NODE_BG));
        }
    }
    let page = ":root{--merlion-series-2:#ff00ff;}";
    let e = Engine::new(&svg, &[], &[page], true, false);
    assert_eq!(
        computed(&e, part(&e, "g1", "merlion-cluster-box"), "stroke"),
        "#ff00ff"
    );
}

#[test]
fn nested_clusters_get_no_automatic_tone() {
    let svg = svg_of(
        "flowchart LR\nsubgraph outer [Outer]\n  subgraph inner [Inner]\n    a{A}\n  end\n  b\nend\nsubgraph second [Second]\n  c\nend",
    );
    assert_eq!(
        group_class(&svg, "outer"),
        "merlion-cluster merlion-cc-series-1 merlion-auto"
    );
    assert_eq!(group_class(&svg, "inner"), "merlion-cluster");
    assert_eq!(
        group_class(&svg, "second"),
        "merlion-cluster merlion-cc-series-2 merlion-auto"
    );
    for e in engines(&svg, &[]) {
        let bx = part(&e, "inner", "merlion-cluster-box");
        assert_eq!(rgba8(&computed(&e, bx, "stroke")), rgba8(BORDER));
        assert_eq!(rgba8(&computed(&e, bx, "fill")), rgba8(CLUSTER_BG));
    }
    // The decision inside the nested cluster keeps its own tone.
    assert_node_tone(&svg, "a", theme::WARN);
}

// ---------------------------------------------------------------------------------------
// Explicit styling wins
// ---------------------------------------------------------------------------------------

#[test]
fn an_explicit_built_in_role_replaces_the_automatic_tone() {
    let svg = svg_of("flowchart LR\nd{D}:::danger --> e{E}");
    assert_eq!(group_class(&svg, "d"), "merlion-node merlion-c-danger");
    assert_node_tone(&svg, "d", theme::DANGER);
    assert_node_tone(&svg, "e", theme::WARN);
}

#[test]
fn a_custom_role_or_class_def_replaces_the_automatic_tone() {
    let svg = svg_of("flowchart LR\nd{D}:::hot --> e{E}:::cool\nclassDef cool fill:#e0f2fe");
    assert_eq!(group_class(&svg, "d"), "merlion-node merlion-c-hot");
    assert_eq!(group_class(&svg, "e"), "merlion-node merlion-c-cool");
    assert_node_untoned(&svg, "d");
    for e in engines(&svg, &[]) {
        let shape = part(&e, "e", "merlion-shape");
        assert_eq!(computed(&e, shape, "fill"), "#e0f2fe");
        assert_eq!(rgba8(&computed(&e, shape, "stroke")), rgba8(BORDER));
    }
}

#[test]
fn a_style_replaces_the_automatic_tone() {
    let svg = svg_of("flowchart LR\nd{D} --> c[(C)]\nstyle d fill:#ffffff\nstyle c stroke:#123456");
    assert!(!group_class(&svg, "d").contains("merlion-auto"));
    assert!(!group_class(&svg, "c").contains("merlion-auto"));
    for e in engines(&svg, &[]) {
        let d = part(&e, "d", "merlion-shape");
        assert_eq!(computed(&e, d, "fill"), "#ffffff");
        assert_eq!(rgba8(&computed(&e, d, "stroke")), rgba8(BORDER));
        let c = part(&e, "c", "merlion-shape");
        assert_eq!(computed(&e, c, "stroke"), "#123456");
        assert_eq!(rgba8(&computed(&e, c, "fill")), rgba8(NODE_BG));
    }
}

#[test]
fn a_class_def_named_after_a_built_in_role_turns_its_automatic_tone_off() {
    let svg = svg_of("flowchart LR\nd{D} --> s([S])\nclassDef warn fill:#fff7cc");
    assert_eq!(group_class(&svg, "d"), "merlion-node");
    assert_eq!(
        group_class(&svg, "s"),
        "merlion-node merlion-c-ok merlion-auto"
    );
    for e in engines(&svg, &[]) {
        let d = part(&e, "d", "merlion-shape");
        assert_eq!(rgba8(&computed(&e, d, "fill")), rgba8(NODE_BG));
    }
}

#[test]
fn cluster_class_or_style_replaces_the_series_tone_without_shifting_the_others() {
    let svg = svg_of(
        "flowchart LR\nsubgraph a [A]\n  x\nend\nsubgraph b [B]\n  y\nend\nsubgraph c [C]\n  z\nend\nsubgraph d [D]\n  w\nend\nclass a group\nstyle b fill:#eeeeee",
    );
    assert_eq!(group_class(&svg, "a"), "merlion-cluster merlion-cc-group");
    assert_eq!(group_class(&svg, "b"), "merlion-cluster merlion-ss-1");
    assert_eq!(
        group_class(&svg, "c"),
        "merlion-cluster merlion-cc-series-3 merlion-auto"
    );
    assert_eq!(
        group_class(&svg, "d"),
        "merlion-cluster merlion-cc-series-4 merlion-auto"
    );
    for e in engines(&svg, &[]) {
        let bx = part(&e, "a", "merlion-cluster-box");
        assert_eq!(rgba8(&computed(&e, bx, "stroke")), rgba8(BORDER));
        assert_eq!(computed(&e, bx, "stroke-dasharray"), "6 4");
        let bx = part(&e, "b", "merlion-cluster-box");
        assert_eq!(computed(&e, bx, "fill"), "#eeeeee");
        assert_eq!(rgba8(&computed(&e, bx, "stroke")), rgba8(BORDER));
    }
}

const SHEET: &str = "
:root { --merlion-warn: #b54708; --merlion-series-1: #7c3aed; }
[data-theme=\"dark\"] { --merlion-bg: #0d1117; --merlion-fg: #e6edf3; --merlion-warn: #f0b429;
  --merlion-store: #5eead4; --merlion-series-1: #c4a7f5; }
.merlion-c-hot { --merlion-tone: #be123c; }
.merlion-c-ok { --merlion-tone: #0e7490; }
";

#[test]
fn stylesheet_tones_win_and_bake_into_automatic_tones() {
    let (sheet, d) = compile(SHEET, &StylesheetLimits::default());
    assert!(d.items.is_empty(), "{:?}", d.items);
    let sheet = sheet.unwrap();
    let src =
        "flowchart LR\nsubgraph g [G]\n  d{D} --> h{H}:::hot\nend\nh --> s([S])\ns --> c[(C)]";
    let page = sheet.to_css();
    let (plain, _) = run_with(src, &opts(true));
    let baked = |p| {
        run_with(
            src,
            &RenderOptions {
                palette: p,
                ..opts(true)
            },
        )
        .0
    };
    for (theme_name, host) in [
        (None, &[][..]),
        (Some("dark"), &[("data-theme", "dark")][..]),
    ] {
        let reference = Engine::new(&plain, host, &[&page], true, false);
        let svg = baked(sheet.palette(theme_name, None));
        for e in [
            Engine::new(&svg, &[], &[], true, false),
            Engine::new(&svg, &[], &[], false, false),
        ] {
            for (id, class) in [
                ("d", "merlion-shape"),
                ("h", "merlion-shape"),
                ("s", "merlion-shape"),
                ("c", "merlion-shape"),
                ("g", "merlion-cluster-box"),
                ("g", "merlion-cluster-title"),
            ] {
                for prop in ["fill", "stroke"] {
                    let want = computed(&reference, part(&reference, id, class), prop);
                    let got = computed(&e, part(&e, id, class), prop);
                    assert!(
                        same(&got, &want, 1),
                        "{:?} {} {} {}: {} vs {}",
                        theme_name,
                        id,
                        class,
                        prop,
                        got,
                        want
                    );
                }
            }
        }
    }
    // The explicit stylesheet role wins over the automatic decision tone; the
    // stylesheet's tone for `ok` restyles the automatic terminal.
    let e = Engine::new(&plain, &[], &[&page], true, false);
    assert_eq!(
        computed(&e, part(&e, "h", "merlion-shape"), "stroke"),
        "#be123c"
    );
    assert_eq!(
        computed(&e, part(&e, "s", "merlion-shape"), "stroke"),
        "#0e7490"
    );
    assert_eq!(
        computed(&e, part(&e, "d", "merlion-shape"), "stroke"),
        "#b54708"
    );
    assert_eq!(
        computed(&e, part(&e, "g", "merlion-cluster-box"), "stroke"),
        "#7c3aed"
    );
}

// ---------------------------------------------------------------------------------------
// Opt-out
// ---------------------------------------------------------------------------------------

const TONED: &str = "flowchart LR\nsubgraph g [G]\n  d{D} --> c[(C)]\nend\nc --> s([S])";

#[test]
fn auto_tone_off_draws_no_automatic_tone() {
    let (svg, _) = run_with(TONED, &opts(false));
    assert!(!svg.contains("merlion-auto"));
    assert!(!svg.contains("merlion-c-"));
    assert!(!svg.contains("merlion-cc-"));
}

#[test]
fn source_opt_outs_match_the_render_option_byte_for_byte() {
    let (off, _) = run_with(TONED, &opts(false));
    for src in [
        format!(
            "%%{{init: {{\"merlion\": {{\"autoTone\": false}}}}}}%%\n{}",
            TONED
        ),
        format!(
            "%%{{init: {{'merlion': {{'autoTone': false}}}}}}%%\n{}",
            TONED
        ),
        format!(
            "---\nconfig:\n  merlion:\n    autoTone: false\n---\n{}",
            TONED
        ),
    ] {
        let (svg, diags) = run_with(&src, &opts(true));
        assert!(diags.is_empty(), "{:?}", diags);
        assert_eq!(svg, off, "{}", src);
    }
    // `autoTone: true` in the source keeps the default.
    let (on, _) = run_with(TONED, &opts(true));
    let (svg, _) = run_with(
        &format!(
            "%%{{init: {{\"merlion\": {{\"autoTone\": true}}}}}}%%\n{}",
            TONED
        ),
        &opts(true),
    );
    assert_eq!(svg, on);
}

#[test]
fn unknown_merlion_keys_and_bad_values_warn() {
    for src in [
        format!(
            "%%{{init: {{\"merlion\": {{\"autoTones\": false}}}}}}%%\n{}",
            TONED
        ),
        format!(
            "%%{{init: {{\"merlion\": {{\"autoTone\": \"no\"}}}}}}%%\n{}",
            TONED
        ),
        format!("%%{{init: {{\"merlion\": false}}}}%%\n{}", TONED),
    ] {
        let (svg, diags) = run_with(&src, &opts(true));
        assert_eq!(
            diags.iter().filter(|d| d.code == "W016").count(),
            1,
            "{:?}",
            diags
        );
        assert!(svg.contains("merlion-auto"), "{}", src);
    }
}

#[test]
fn tones_never_change_layout_or_outline() {
    let src = format!(
        "{}\n{}",
        SHAPES,
        clusters(3).trim_start_matches("flowchart LR\n")
    );
    let on = render(&src, &opts(true));
    let off = render(&src, &opts(false));
    assert_eq!(on.outline, off.outline);
    let (on, off) = (on.svg.unwrap(), off.svg.unwrap());
    let attrs = |s: &str, name: &str| -> Vec<String> {
        svg_support::all_attrs(s)
            .into_iter()
            .filter(|(_, n, _)| n == name)
            .map(|(_, _, v)| v)
            .collect()
    };
    for name in [
        "d",
        "x",
        "y",
        "width",
        "height",
        "viewBox",
        "data-merlion-layout",
    ] {
        assert_eq!(attrs(&on, name), attrs(&off, name), "{}", name);
    }
}

#[test]
fn renders_are_deterministic_and_the_flag_joins_the_default_id() {
    let base = RenderOptions::default();
    let off = RenderOptions {
        auto_tone: false,
        ..RenderOptions::default()
    };
    let a = render(TONED, &base).svg.unwrap();
    assert_eq!(a, render(TONED, &base).svg.unwrap());
    let b = render(TONED, &off).svg.unwrap();
    assert_eq!(b, render(TONED, &off).svg.unwrap());
    let id = |s: &str| {
        let i = s.find(" id=\"").unwrap() + 5;
        s[i..i + s[i..].find('"').unwrap()].to_string()
    };
    assert_ne!(id(&a), id(&b));
    assert_safe(&a, &id(&a));
    assert_safe(&b, &id(&b));
}

#[test]
fn light_series_defaults_match_the_themes_package() {
    let css = include_str!("../../../packages/merlion-themes/merlion-themes.css");
    for (i, c) in SERIES.iter().enumerate() {
        assert!(
            css.contains(&format!("--merlion-series-{}: {};", i + 1, c)),
            "series {}",
            i + 1
        );
    }
    assert!(css.contains(&format!("--merlion-store: {};", STORE)));
}
