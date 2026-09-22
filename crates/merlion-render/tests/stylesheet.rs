//! The stylesheet compiler (specs/svg-output.md#stylesheet, specs/adr/0009-stylesheet.md):
//! the CSS subset, its diagnostics and limits, re-serialisation to page CSS with fixed
//! selector shapes and literal values, and palette resolution.

mod stylesheet_support;

use merlion_render::color::{oklab_mix, Rgba8};
use merlion_render::diag::{Diagnostic, Severity};
use merlion_render::stylesheet::{compile, Palette, StylesheetLimits};
use stylesheet_support::assert_page_css_safe;

const THEMES: &str = include_str!("../../../packages/merlion-themes/merlion-themes.css");
const THEMES_WITH_ROLES: &str = include_str!("fixtures/stylesheets/themes-with-roles.css");

fn run(css: &str) -> (Option<String>, Vec<Diagnostic>) {
    let (s, d) = compile(css, &StylesheetLimits::default());
    (s.map(|s| s.to_css()), d.items)
}

fn codes(d: &[Diagnostic]) -> Vec<&'static str> {
    d.iter().map(|x| x.code).collect()
}

fn ok(css: &str) -> String {
    let (out, d) = run(css);
    assert!(d.iter().all(|x| x.severity == Severity::Info), "{:?}", d);
    out.expect("compiles")
}

fn hex(s: &str) -> Rgba8 {
    Rgba8::from_hex(s).unwrap()
}

fn palette(css: &str, theme: Option<&str>, dark: Option<&str>) -> Palette {
    let (s, _) = compile(css, &StylesheetLimits::default());
    s.expect("compiles")
        .palette(theme, dark)
        .expect("theme exists")
}

// ---------------------------------------------------------------------------------------
// The shipped themes
// ---------------------------------------------------------------------------------------

#[test]
fn shipped_themes_compile_with_no_warning() {
    for css in [THEMES, THEMES_WITH_ROLES] {
        let (s, d) = compile(css, &StylesheetLimits::default());
        let s = s.expect("compiles");
        // The two viewer rules declare no token: one I032 summary.
        assert_eq!(codes(&d.items), ["I032"], "{:?}", d.items);
        assert!(
            d.items[0].message.contains("2 rules"),
            "{}",
            d.items[0].message
        );
        let mut names = s.theme_names();
        names.sort();
        assert_eq!(names, ["dark", "harbour", "lantern", "light"]);
        for t in [
            None,
            Some("light"),
            Some("dark"),
            Some("harbour"),
            Some("lantern"),
        ] {
            let p = s.palette(t, Some("dark")).expect("palette");
            assert!(p.light.colour("bg").is_some());
        }
        assert_page_css_safe(&s.to_css());
    }
}

#[test]
fn shipped_themes_resolve_their_foundations() {
    let p = palette(THEMES, Some("harbour"), None);
    assert_eq!(p.light.colour("bg"), Some(hex("#f6f2ea")));
    assert_eq!(p.light.colour("fg"), Some(hex("#1d3440")));
    // Mixed roles the stylesheet leaves unset derive from the resolved foundations.
    assert_eq!(
        p.light.resolved("surface"),
        Some(oklab_mix(hex("#1d3440"), hex("#f6f2ea"), 4.0))
    );
    assert_eq!(
        p.light.resolved("node-bg"),
        Some(oklab_mix(hex("#1d3440"), hex("#f6f2ea"), 4.0))
    );
    assert_eq!(p.light.resolved("danger"), Some(hex("#cf222e")));
    let p = palette(THEMES_WITH_ROLES, Some("lantern"), None);
    assert!(p.light.colour("danger").is_some());
    assert_ne!(p.light.colour("danger"), Some(hex("#cf222e")));
    // No theme: `:root` alone.
    let p = palette(THEMES, None, None);
    assert_eq!(p.light.colour("bg"), Some(hex("#ffffff")));
    assert!(p.dark.is_none());
    let p = palette(THEMES, None, Some("dark"));
    assert_eq!(p.dark.as_ref().unwrap().colour("bg"), Some(hex("#16191d")));
}

#[test]
fn an_unknown_theme_has_no_palette() {
    let (s, _) = compile(THEMES, &StylesheetLimits::default());
    let s = s.unwrap();
    assert!(s.palette(Some("nope"), None).is_none());
    assert!(s.palette(None, Some("nope")).is_none());
}

// ---------------------------------------------------------------------------------------
// Output shape and fixed point
// ---------------------------------------------------------------------------------------

const SAMPLE: &str = r#"
/* brand */
:root { --brand: #0F766E; --merlion-accent: var(--brand); --merlion-bg: white;
        --merlion-stroke: 1.5px; --merlion-c-store-fill: rgb(10 20 30); }
[data-theme='dark'] { --merlion-bg: hsl(210, 20%, 10%); --merlion-fg: #eee;
                      --merlion-accent: var(--brand, #000); }
@media (prefers-color-scheme: dark) {
  :root:not([data-theme]) { --merlion-bg: #101010; }
}
.merlion-c-store, .merlion-cc-zone { --merlion-tone: oklch(0.6 0.1 180); --merlion-dash: 4, 2; }
[data-theme="dark"] .merlion-c-store { --merlion-tone: var(--merlion-accent); }
.merlion-c-quiet { --merlion-dash: none; }
.not-merlion { color: red; }
"#;

#[test]
fn compiles_to_fixed_selector_shapes_and_literal_values() {
    let out = ok(SAMPLE);
    let expected_prefix = ":root {\n  --merlion-bg: #ffffff;\n  --merlion-accent: #0f766e;\n  --merlion-stroke: 1.5px;\n  --merlion-c-store-fill: #0a141e;\n}\n[data-theme=\"dark\"] {\n  --merlion-bg: #141a1f;\n  --merlion-fg: #eeeeee;\n  --merlion-accent: #0f766e;\n}\n@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]) {\n    --merlion-bg: #101010;\n  }\n}\n.merlion .merlion-c-store {\n  --merlion-tone: #";
    assert!(out.starts_with(expected_prefix), "{}", out);
    assert!(out.contains(
        ";\n  --merlion-dash: 4 2;\n}\n.merlion .merlion-cc-zone {\n  --merlion-tone: #"
    ));
    assert!(out.contains(
        "[data-theme=\"dark\"] .merlion .merlion-c-store {\n  --merlion-tone: #0f766e;\n}\n"
    ));
    assert!(
        out.ends_with(".merlion .merlion-c-quiet {\n  --merlion-dash: none;\n}\n"),
        "{}",
        out
    );
    // Private tokens are resolved and never emitted.
    assert!(!out.contains("--brand"));
    assert_page_css_safe(&out);
}

#[test]
fn compiling_compiled_output_is_a_fixed_point() {
    for css in [SAMPLE, THEMES, THEMES_WITH_ROLES] {
        let once = ok(css);
        let (twice, d) = run(&once);
        assert_eq!(twice.as_deref(), Some(once.as_str()));
        assert!(d.is_empty(), "compiled output compiles clean: {:?}", d);
    }
}

#[test]
fn formatting_does_not_change_the_model() {
    let a = compile(":root{--merlion-bg:#FFF}", &StylesheetLimits::default()).0;
    let b = compile(
        "/* x */\n:root {\n  --merlion-bg : #ffffff ;\n}\n",
        &StylesheetLimits::default(),
    )
    .0;
    assert_eq!(a, b);
    let pa = a.unwrap().palette(None, None).unwrap();
    let pb = b.unwrap().palette(None, None).unwrap();
    assert_eq!(pa.digest(), pb.digest());
}

// ---------------------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------------------

fn one(css: &str) -> (String, Vec<&'static str>) {
    let (out, d) = run(css);
    (out.unwrap_or_default(), codes(&d))
}

#[test]
fn declarations_outside_the_token_list_are_dropped_with_w018() {
    for decl in [
        "--merlion-font: Evil",
        "--merlion-font-size: 20px",
        "fill: red",
        "--merlion-bg: url(//x)",
        "--merlion-bg: color-mix(in oklab, red, blue)",
        "--merlion-bg: calc(1 + 1)",
        "--merlion-bg: env(x)",
        "--merlion-bg: \"red\"",
        "--merlion-bg: red !important",
        "--merlion-bg: \\72 ed",
        "--merlion-bg: oklch(0.7 0.4 150)",
        "--merlion-bg: image-set(\"x\" 1x)",
        "--merlion-stroke: 30",
        "--merlion-stroke: red",
        "--merlion-tone: #fff",
        "--merlion-dash: 1 2",
        "--merlion-nope: #fff",
        "--merlion-c-1bad-fill: #fff",
        "--merlion-c-x-opacity: 1",
        "--merlion-c-x-color: none",
        "--merlion-bg: var(--a, var(--b))",
    ] {
        let (out, c) = one(&format!(":root {{ --merlion-fg: #000; {}; }}", decl));
        assert_eq!(c, ["W018"], "{}", decl);
        assert_eq!(out, ":root {\n  --merlion-fg: #000000;\n}\n", "{}", decl);
    }
    for decl in [
        "--merlion-bg: #fff",
        "--merlion-stroke: 2",
        "--merlion-c-x-fill: red",
        "--brand: #fff",
    ] {
        let (_, c) = one(&format!(
            ".merlion-c-x {{ --merlion-tone: #000; {}; }}",
            decl
        ));
        assert_eq!(c, ["W018"], "{}", decl);
    }
}

#[test]
fn references_that_fail_are_dropped_with_w019() {
    let (out, c) = one(":root { --merlion-fg: #000; --merlion-bg: var(--missing); }");
    assert_eq!(c, ["W019"]);
    assert!(!out.contains("--merlion-bg"));
    let (_, c) = one(":root { --a: var(--b); --b: var(--a); --merlion-bg: var(--a); }");
    assert!(c.iter().all(|x| *x == "W019") && !c.is_empty(), "{:?}", c);
    // Depth: a chain of 8 references resolves, 9 does not.
    let chain = |n: usize| {
        let mut s = String::from(":root { --t0: #123456;");
        for i in 1..=n {
            s.push_str(&format!(" --t{}: var(--t{});", i, i - 1));
        }
        s.push_str(&format!(" --merlion-bg: var(--t{}); }}", n));
        s
    };
    let (out, c) = one(&chain(7));
    assert!(c.is_empty(), "{:?}", c);
    assert!(out.contains("--merlion-bg: #123456;"));
    let (out, c) = one(&chain(8));
    assert!(c.contains(&"W019"), "{:?}", c);
    assert!(!out.contains("--merlion-bg"));
    // An undefined name with a fallback uses the fallback.
    let (out, c) = one(":root { --merlion-bg: var(--missing, #abcdef); }");
    assert!(c.is_empty());
    assert!(out.contains("--merlion-bg: #abcdef;"));
}

#[test]
fn rules_outside_the_subset_are_dropped_with_w017() {
    for css in [
        "@import \"x.css\";",
        "@import url(x.css);",
        "@font-face { font-family: X; src: url(x.woff2); }",
        "@supports (x: y) { :root { --merlion-bg: #000; } }",
        "@layer a { :root { --merlion-bg: #000; } }",
        "@media print { :root { --merlion-bg: #000; } }",
        "@container (min-width: 1px) { :root { --merlion-bg: #000; } }",
        "@namespace svg url(http://www.w3.org/2000/svg);",
        "body { --merlion-bg: #000; }",
        "[data-x^=\"a\"] { --merlion-bg: #000; }",
        ".merlion-c-x .merlion-c-y { --merlion-tone: #000; }",
        "[data-theme=\"Dark\"] { --merlion-bg: #000; }",
        "@media (prefers-color-scheme: dark) { :root { --merlion-bg: #000; } }",
        "@media (prefers-color-scheme: dark) { .merlion-c-x { --merlion-tone: #000; } }",
        ":root:not([data-theme]) { --merlion-bg: #000; }",
        ":root { .x { --merlion-bg: #000; } --merlion-fg: #000; }",
        "@media (prefers-color-scheme: dark) { @media (prefers-color-scheme: dark) { } }",
    ] {
        let (out, c) = one(css);
        assert!(c.contains(&"W017"), "{}: {:?}", css, c);
        assert!(!c.contains(&"E013"), "{}", css);
        assert!(!out.contains("#000000"), "{}: {}", css, out);
    }
    // Only the offending selector of a list is dropped.
    let (out, c) = one(":root, body { --merlion-bg: #000; }");
    assert_eq!(c, ["W017"]);
    assert_eq!(out, ":root {\n  --merlion-bg: #000000;\n}\n");
}

#[test]
fn rules_without_tokens_count_once_as_i032() {
    let (out, d) = run(".a { color: red; } .b { --other: 1; } :root { color-scheme: dark; }");
    assert_eq!(codes(&d), ["I032"]);
    assert!(d[0].message.contains("3 rules"), "{}", d[0].message);
    assert_eq!(out.as_deref(), Some(""));
}

#[test]
fn limits_fail_the_whole_stylesheet_with_e013() {
    let l = StylesheetLimits::default();
    let big = format!(":root {{ --merlion-bg: #000; }}/*{}*/", "x".repeat(l.bytes));
    let rules = ":root { --merlion-bg: #000; }\n".repeat(l.rules + 1);
    let decls = format!(
        ":root {{ {} }}",
        "--merlion-bg: #000;".repeat(l.declarations + 1)
    );
    let themes: String = (0..=l.themes)
        .map(|i| format!("[data-theme=\"t{}\"] {{ --merlion-bg: #000; }}", i))
        .collect();
    let roles: String = (0..=l.role_selectors)
        .map(|i| format!(".merlion-c-r{} {{ --merlion-tone: #000; }}", i))
        .collect();
    let deep = "@media (prefers-color-scheme: dark) { :root:not([data-theme]) { x { } } }";
    // A small input whose selector lists expand past 64 KiB of compiled output.
    let themes8 = (0..8)
        .map(|i| format!("[data-theme=\"t{}\"]", i))
        .collect::<Vec<_>>()
        .join(",");
    let expands: String = (0..50)
        .map(|r| {
            let decls: String = (0..32)
                .map(|k| format!("--merlion-c-n{}x{}-fill:#fff;", r, k))
                .collect();
            format!("{}{{{}}}", themes8, decls)
        })
        .collect();
    assert!(expands.len() < l.bytes);
    let list = format!(
        "{} {{ --merlion-bg: #000; }}",
        vec![":root"; l.selectors + 1].join(", ")
    );
    for (what, css) in [
        ("bytes", big),
        ("rules", rules),
        ("declarations", decls),
        ("themes", themes),
        ("roles", roles),
        ("depth", deep.to_string()),
        ("selectors", list),
        ("compiled size", expands),
    ] {
        let (s, d) = compile(&css, &l);
        assert!(s.is_none(), "{}", what);
        assert!(
            d.items
                .iter()
                .any(|x| x.code == "E013" && x.severity == Severity::Error),
            "{}: {:?}",
            what,
            codes(&d.items)
        );
    }
}

#[test]
fn diagnostics_stop_at_one_hundred_plus_a_summary() {
    let css: String = (0..300).map(|_| ":root { --merlion-font: x; }").collect();
    let (_, d) = run(&css);
    assert_eq!(d.len(), 101);
    assert!(d[100].message.contains("200 more"), "{}", d[100].message);
}

#[test]
fn diagnostics_point_at_the_source() {
    let (_, d) = run(":root {\n  --merlion-fg: #000;\n  --merlion-font: x;\n}");
    assert_eq!(d[0].span.line, 3);
    assert_eq!(d[0].span.column, 3);
}

// ---------------------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------------------

#[test]
fn colour_grammar_converts_to_hex() {
    for (v, want) in [
        ("#ABC", "#aabbcc"),
        ("#11223380", "#11223380"),
        ("rebeccapurple", "#663399"),
        ("RED", "#ff0000"),
        ("transparent", "#00000000"),
        ("rgb(255, 0, 0)", "#ff0000"),
        ("rgb(100% 0% 0% / 50%)", "#ff000080"),
        ("hsl(120, 100%, 25%)", "#008000"),
        ("hsla(0, 100%, 50%, 0.5)", "#ff000080"),
        ("oklab(0.62796 0.22486 0.12585)", "#ff0000"),
        ("oklch(62.796% 0.25768 29.2339)", "#ff0000"),
        ("oklch(0.62796 0.25768 29.2339 / 0.5)", "#ff000080"),
    ] {
        let out = ok(&format!(":root {{ --merlion-bg: {}; }}", v));
        assert_eq!(
            out,
            format!(":root {{\n  --merlion-bg: {};\n}}\n", want),
            "{}",
            v
        );
    }
}

#[test]
fn stroke_and_dash_grammars() {
    let out = ok(":root { --merlion-stroke: 2; } .merlion-c-a { --merlion-dash: 1,2 3; }");
    assert!(out.contains("--merlion-stroke: 2px;"));
    assert!(out.contains("--merlion-dash: 1 2 3;"));
    for bad in ["1 2 3 4 5 6 7 8 9", "-1", "101", "a"] {
        let (_, c) = one(&format!(".merlion-c-a {{ --merlion-dash: {}; }}", bad));
        assert_eq!(c, ["W018"], "{}", bad);
    }
}

#[test]
fn themed_role_rules_win_over_unthemed_ones_in_the_palette() {
    let css = "[data-theme=\"dark\"] .merlion-c-x { --merlion-tone: #111111; }\n\
               .merlion-c-x { --merlion-tone: #222222; --merlion-dash: 2 2; }\n\
               [data-theme=\"dark\"] { --merlion-bg: #000; }\n\
               .merlion-cc-x { --merlion-tone: #333333; }";
    let p = palette(css, Some("dark"), None);
    let t = p.light.tone("x", false).unwrap();
    assert_eq!(t.tone, Some(hex("#111111")));
    assert_eq!(t.dash.as_deref(), Some(&[2.0, 2.0][..]));
    assert_eq!(p.light.tone("x", true).unwrap().tone, Some(hex("#333333")));
    let p = palette(css, None, None);
    assert_eq!(p.light.tone("x", false).unwrap().tone, Some(hex("#222222")));
}

#[test]
fn hostile_input_never_reaches_the_output() {
    let css = "</style><script>alert(1)</script> :root { --merlion-bg: #000; }\n\
               :root { --merlion-fg: #fff<script>; }\n\
               [data-theme=\"x\\\"]{}\"] { --merlion-bg: #fff; }\n\
               .merlion-c-a\\62 { --merlion-tone: #fff; }";
    let (out, _) = run(css);
    let out = out.unwrap_or_default();
    assert!(
        !out.contains('<') && !out.contains('\\') && !out.contains("script"),
        "{}",
        out
    );
    assert_page_css_safe(&out);
}

// ---------------------------------------------------------------------------------------
// The canonical palette as a wire form (the WASM `palette` option)
// ---------------------------------------------------------------------------------------

#[test]
fn canonical_palettes_parse_back_to_the_same_palette() {
    for css in [THEMES, THEMES_WITH_ROLES, SAMPLE] {
        let (s, _) = compile(css, &StylesheetLimits::default());
        let s = s.unwrap();
        let names = s.theme_names();
        for t in core::iter::once(None).chain(names.iter().copied().map(Some)) {
            for dark in [None, names.first().copied()] {
                let p = s.palette(t, dark).unwrap();
                assert_eq!(
                    Palette::parse(&p.canonical()),
                    Ok(p.clone()),
                    "{t:?} {dark:?}"
                );
            }
        }
    }
    assert_eq!(Palette::parse("palette-v1|"), Ok(Palette::default()));
}

#[test]
fn canonical_palettes_keep_tone_order_and_none() {
    let src = "palette-v1|bg=#000000;stroke=1.5;c-store-fill=none;c-store-color=#ff0000;\
               c-b:#00ff00/;c-a:/none;cc-a:#0000ff/4 2;";
    let p = Palette::parse(src).unwrap();
    assert_eq!(p.canonical(), src);
    // Cluster tones follow node tones whatever the input order.
    let mixed = Palette::parse("palette-v1|cc-a:#0000ff/;c-b:#00ff00/;c-a:/;").unwrap();
    assert_eq!(
        mixed.canonical(),
        "palette-v1|c-b:#00ff00/;c-a:/;cc-a:#0000ff/;"
    );
    let (s, _) = compile(
        ".merlion-cc-z { --merlion-tone: #111111; } .merlion-c-y { --merlion-tone: #222222; }",
        &StylesheetLimits::default(),
    );
    let tones = s.unwrap().palette(None, None).unwrap().light.tones;
    assert_eq!((tones[0].cluster, tones[1].cluster), (false, true));
    assert_eq!(p.light.tones[0].name, "b");
    assert_eq!(p.light.tones[1].dash.as_deref(), Some(&[][..]));
    assert_eq!(p.light.stroke, Some(1.5));
    assert_eq!(
        p.light
            .class_colour("store", merlion_render::stylesheet::ClassProp::Fill),
        Some(None)
    );
    let dark = Palette::parse("palette-v1|bg=#ffffff;|dark|bg=#000000;").unwrap();
    assert_eq!(dark.dark.unwrap().colour("bg"), Some(hex("#000000")));
}

#[test]
fn canonical_palettes_reject_values_outside_the_token_grammars() {
    for bad in [
        "",
        "palette-v2|",
        "palette-v1|bg=#000000",
        "palette-v1|bg=red;",
        "palette-v1|bg=#00000;",
        "palette-v1|bg=url(x);",
        "palette-v1|font=#000000;",
        "palette-v1|tone=#000000;",
        "palette-v1|series-9=#000000;",
        "palette-v1|stroke=21;",
        "palette-v1|stroke=1px;",
        "palette-v1|c-1bad-fill=#000000;",
        "palette-v1|c-a-fill=#00000g;",
        "palette-v1|c-a-color=none;",
        "palette-v1|c-a:#000000;",
        "palette-v1|c-a:red/;",
        "palette-v1|c-a:/1 2 3 4 5 6 7 8 9;",
        "palette-v1|c-a:/101;",
        "palette-v1|c-a:#000000/;c-a:#111111/;",
        "palette-v1|c-a<b:#000000/;",
        "palette-v1|bg=#000000;|dark|x;",
        "palette-v1||dark||dark|",
    ] {
        assert!(Palette::parse(bad).is_err(), "{bad}");
    }
}
