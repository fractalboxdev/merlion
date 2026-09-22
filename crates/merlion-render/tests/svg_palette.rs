//! Baking a stylesheet into a standalone SVG (specs/svg-output.md#palette,
//! specs/benchmark.md stylesheet parity). The reference is the plain SVG inlined with the
//! compiled page CSS linked and `data-theme` on the host; the baked SVG must compute the
//! same colours with no host CSS, with its `<style>` removed (presentation attributes
//! only), and without `color-mix` support, each to ±1 per 8-bit channel.

mod css_support;
mod svg_support;

use css_support::{same, Engine};
use merlion_render::stylesheet::{compile, Palette, StylesheetLimits};
use merlion_render::{render, Diagnostic, RenderOptions};
use svg_support::{assert_safe, assert_well_formed};

const THEMES: &str = include_str!("fixtures/stylesheets/themes-with-roles.css");

const SITE: &str = r#"
:root { --brand: #0f766e; --merlion-accent: var(--brand); --merlion-c-hot-fill: #ffe4e6;
        --merlion-stroke: 1.5px; }
[data-theme="dark"] { --merlion-c-hot-fill: #4c0519; --merlion-c-hot-color: #fecdd3;
                      --merlion-edge: #8b949e; }
.merlion-c-store { --merlion-tone: var(--brand); }
[data-theme="dark"] .merlion-c-store { --merlion-tone: #5eead4; --merlion-dash: 3 2; }
.merlion-cc-zone { --merlion-tone: #7c3aed; --merlion-dash: 8 3; }
.merlion-c-late { --merlion-tone: #b45309; }
.merlion-c-danger { --merlion-tone: #be123c; }
"#;

const DIAGRAM: &str = "flowchart LR
  subgraph z [Zone]
    a[\"**Store**<br/>detail\"]:::store --> b[B]:::danger
  end
  subgraph g [Group]
    c[C]:::ok
  end
  b e1@-->|fails| c
  c e2@-.-> d[D]:::hot
  d e3@--x e[E]:::late
  e --> f[F]:::muted
  a --> f
  class z zone
  class g group
  class e1 failure
  class e2 async
  class e3 late
  classDef hot fill:#ff0000,color:#ffffff
";

fn stylesheet() -> merlion_render::stylesheet::Stylesheet {
    let css = format!("{}\n{}", THEMES, SITE);
    let (s, d) = compile(&css, &StylesheetLimits::default());
    assert!(d.items.iter().all(|x| x.code == "I032"), "{:?}", d.items);
    s.expect("compiles")
}

fn render_with(src: &str, palette: Option<Palette>) -> (String, Vec<Diagnostic>) {
    let opts = RenderOptions {
        id_prefix: Some("m1".into()),
        palette,
        ..RenderOptions::default()
    };
    let r = render(src, &opts);
    let svg = r.svg.unwrap_or_else(|| panic!("{:?}", r.diagnostics));
    assert_well_formed(&svg);
    assert_safe(&svg, "m1");
    (svg, r.diagnostics)
}

/// The SVG with an empty `<style>`, so element indices stay aligned.
fn without_style(svg: &str) -> String {
    let a = svg.find("<style>").unwrap() + "<style>".len();
    let b = svg.find("</style>").unwrap();
    format!("{}{}", &svg[..a], &svg[b..])
}

const PARTS: [&str; 10] = [
    "merlion-shape",
    "merlion-label",
    "merlion-edge-text",
    "merlion-edge-label-bg",
    "merlion-cluster-title",
    "merlion-cluster-box",
    "merlion-edge-path",
    "merlion-marker-fill",
    "merlion-marker-stroke",
    "merlion-detail",
];

/// Compares every part's fill, stroke and dash array between two engines over the same
/// document structure (element indices match).
fn assert_parity(reference: &Engine, baked: &Engine, what: &str) -> usize {
    let mut n = 0;
    for class in PARTS {
        for e in reference.with_class(class) {
            for p in ["fill", "stroke", "stroke-dasharray"] {
                let want = reference.computed(e, p).unwrap_or_else(|m| panic!("{}", m));
                let got = baked.computed(e, p).unwrap_or_else(|m| panic!("{}", m));
                assert!(
                    same(&want, &got, 1),
                    "{}: .{} #{} {}: reference {:?}, baked {:?}",
                    what,
                    class,
                    e,
                    p,
                    want,
                    got
                );
                n += 1;
            }
        }
    }
    n
}

#[test]
fn baked_output_matches_the_page_css_in_every_theme() {
    let sheet = stylesheet();
    let page = sheet.to_css();
    let (plain, _) = render_with(DIAGRAM, None);
    let mut checked = 0;
    for theme in [
        None,
        Some("light"),
        Some("dark"),
        Some("harbour"),
        Some("lantern"),
    ] {
        let host: Vec<(&str, &str)> = theme.map(|t| ("data-theme", t)).into_iter().collect();
        let reference = Engine::new(&plain, &host, &[&page], true, false);
        let palette = sheet.palette(theme, None).unwrap();
        let (baked, _) = render_with(DIAGRAM, Some(palette));
        let what = format!("theme {:?}", theme);
        checked += assert_parity(
            &reference,
            &Engine::new(&baked, &[], &[], true, false),
            &format!("{} baked", what),
        );
        checked += assert_parity(
            &reference,
            &Engine::new(&baked, &[], &[], false, false),
            &format!("{} baked without color-mix", what),
        );
        checked += assert_parity(
            &reference,
            &Engine::new(&without_style(&baked), &[], &[], true, false),
            &format!("{} attributes", what),
        );
    }
    assert!(checked > 1000, "{}", checked);
    // `--merlion-stroke: 1.5px` reaches the attributes and the fallbacks.
    let (baked, _) = render_with(DIAGRAM, sheet.palette(None, None));
    assert!(baked.contains("stroke-width=\"1.5\""));
    assert!(baked.contains("var(--merlion-stroke, 1.5px)"));
    assert!(!baked.contains("stroke-width=\"1.25\""));
}

#[test]
fn auto_dark_follows_the_colour_scheme() {
    let sheet = stylesheet();
    let page = sheet.to_css();
    let (plain, _) = render_with(DIAGRAM, None);
    let palette = sheet.palette(None, Some("dark")).unwrap();
    let (baked, _) = render_with(DIAGRAM, Some(palette));
    assert!(baked
        .contains("@media (prefers-color-scheme: dark){#m1:not(:is([data-theme=\"light\"] *)) "));
    // Dark scheme: the reference page resolves `[data-theme="dark"]` on the host.
    let reference = Engine::new(&plain, &[("data-theme", "dark")], &[&page], true, true);
    assert_parity(
        &reference,
        &Engine::new(&baked, &[], &[], true, true),
        "auto dark",
    );
    // Light scheme: the base palette.
    let reference = Engine::new(&plain, &[], &[&page], true, false);
    assert_parity(
        &reference,
        &Engine::new(&baked, &[], &[], true, false),
        "auto light",
    );
    // A light ancestor keeps the light palette even in a dark scheme.
    let e = Engine::new(&baked, &[("data-theme", "light")], &[], true, true);
    assert_parity(&reference, &e, "light ancestor");
}

#[test]
fn a_host_theme_still_wins_over_a_baked_palette_when_inlined() {
    let sheet = stylesheet();
    let (baked, _) = render_with(DIAGRAM, sheet.palette(Some("harbour"), None));
    let host = ":root{--merlion-node-bg:#010203;}";
    let e = Engine::new(&baked, &[], &[host], true, false);
    let shape = e
        .with_class("merlion-shape")
        .into_iter()
        .find(|&i| {
            e.els[i]
                .parent
                .is_some_and(|p| e.els[p].attr("data-merlion-id") == Some("e"))
        })
        .unwrap();
    // `late` tones the node; its fill mixes the tone into the host's node-bg.
    let fill = e.computed(shape, "fill").unwrap();
    assert!(
        same(&fill, "color-mix(in oklab, #b45309 14%, #010203)", 1),
        "{}",
        fill
    );
}

#[test]
fn a_palette_never_changes_layout_or_ids_of_palette_free_renders() {
    let sheet = stylesheet();
    let plain = render(DIAGRAM, &RenderOptions::default()).svg.unwrap();
    let baked = render(
        DIAGRAM,
        &RenderOptions {
            palette: sheet.palette(Some("dark"), None),
            ..RenderOptions::default()
        },
    )
    .svg
    .unwrap();
    let attr = |s: &str, name: &str| -> Vec<String> {
        s.split(&format!(" {}=\"", name))
            .skip(1)
            .map(|x| x[..x.find('"').unwrap()].to_string())
            .collect()
    };
    for a in [
        "data-merlion-layout",
        "d",
        "x",
        "y",
        "width",
        "height",
        "viewBox",
    ] {
        assert_eq!(attr(&plain, a), attr(&baked, a), "{}", a);
    }
    let id = |s: &str| attr(s, "id")[0].clone();
    assert_ne!(
        id(&plain),
        id(&baked),
        "the palette digest joins the id hash"
    );
    // The same palette from a differently formatted stylesheet gives the same id.
    let again = compile(&sheet.to_css(), &StylesheetLimits::default())
        .0
        .unwrap();
    let baked2 = render(
        DIAGRAM,
        &RenderOptions {
            palette: again.palette(Some("dark"), None),
            ..RenderOptions::default()
        },
    )
    .svg
    .unwrap();
    assert_eq!(id(&baked), id(&baked2));
    // No palette: the id is what it always was.
    let none = render(DIAGRAM, &RenderOptions::default()).svg.unwrap();
    assert_eq!(id(&plain), id(&none));
}

#[test]
fn tone_masked_by_a_source_literal_is_reported() {
    let sheet = stylesheet();
    let p = sheet.palette(None, None);
    let (_, d) = render_with(
        "flowchart LR\na[A]:::store --> b\nstyle a stroke:#000000",
        p.clone(),
    );
    assert!(d.iter().any(|x| x.code == "I033"), "{:?}", d);
    // A classDef colour whose token the stylesheet sets does not mask.
    let (_, d) = render_with(
        "flowchart LR\na[A]:::store --> b\nclass a hot\nclassDef hot fill:#ff0000",
        p.clone(),
    );
    assert!(!d.iter().any(|x| x.code == "I033"), "{:?}", d);
    let (_, d) = render_with(
        "flowchart LR\na[A]:::store --> b\nclass a cold\nclassDef cold fill:#0000ff",
        p.clone(),
    );
    assert!(d.iter().any(|x| x.code == "I033"), "{:?}", d);
    // Built-in roles never emit I033, and nothing is reported without a palette tone.
    let (_, d) = render_with("flowchart LR\na[A]:::ok --> b\nstyle a stroke:#000000", p);
    assert!(!d.iter().any(|x| x.code == "I033"), "{:?}", d);
}

#[test]
fn palette_roles_are_embedded_only_when_used_and_capped() {
    let sheet = stylesheet();
    let (svg, _) = render_with("flowchart LR\na --> b", sheet.palette(None, None));
    assert!(!svg.contains("merlion-c-store") && !svg.contains("merlion-cc-zone"));
    // Many roles: the embedded style grows by at most 16 KiB, the rest get W017.
    let mut css = String::new();
    let mut src = String::from("flowchart LR\n");
    for i in 0..200 {
        css.push_str(&format!(
            ".merlion-c-r{} {{ --merlion-tone: #1{:05x}; }}\n",
            i, i
        ));
        src.push_str(&format!("n{}[N]:::r{}\n", i, i));
    }
    let (s, _) = compile(&css, &StylesheetLimits::default());
    let p = s.unwrap().palette(None, None);
    let (with, d) = render_with(&src, p);
    let (without, _) = render_with(&src, None);
    let style_len = |s: &str| s.find("</style>").unwrap() - s.find("<style>").unwrap();
    assert!(style_len(&with) - style_len(&without) <= 16 * 1024);
    assert!(
        d.iter().any(|x| x.code == "W017"),
        "{:?}",
        d.iter().map(|x| x.code).collect::<Vec<_>>()
    );
}

#[test]
fn a_full_palette_on_a_role_heavy_diagram_emits_used_tones_and_charges_fuel() {
    use merlion_render::stylesheet::Palette;
    let entries: String = (0..256).map(|i| format!("c-r{}:#123456/;", i)).collect();
    let palette = Palette::parse(&format!("palette-v1|{}", entries)).unwrap();
    let mut src = String::from("flowchart LR\n");
    for n in 0..200 {
        src.push_str(&format!("n{}\n", n));
    }
    for n in 0..200 {
        for k in 0..20 {
            src.push_str(&format!("class n{} r{}\n", n, (n * 20 + k) % 400));
        }
    }
    let plain = render(&src, &RenderOptions::default());
    let t = std::time::Instant::now();
    let r = render(
        &src,
        &RenderOptions {
            palette: Some(palette),
            ..RenderOptions::default()
        },
    );
    let elapsed = t.elapsed();
    let svg = r.svg.unwrap();
    assert_eq!(r.fuel_used, plain.fuel_used + 256);
    // Tones r0..r255 are used, within the 16 KiB budget; r256..r399 carry no tone.
    assert!(svg.contains(".merlion-c-r0>.merlion-shape{"));
    assert!(!svg.contains(".merlion-c-r256>"));
    assert!(r.diagnostics.iter().any(|d| d.code == "W017"));
    assert!(elapsed.as_secs() < 5, "{:?}", elapsed);
}
