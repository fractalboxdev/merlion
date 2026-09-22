//! Property test of the stylesheet compiler (specs/security.md#fuzzing, `stylesheet`
//! and `render` targets): arbitrary and mutated stylesheets never panic; limits return
//! `E013`; compiled output passes the page-CSS shape check and compiles to itself with no
//! diagnostic; every palette bakes into an SVG that passes the output safety checker.
//! The generator is a fixed xorshift, so every run checks the same cases.

mod stylesheet_support;
mod svg_support;

use merlion_render::stylesheet::{compile, Palette, StylesheetLimits};
use merlion_render::{render, RenderOptions};
use stylesheet_support::assert_page_css_safe;
use svg_support::{assert_safe, assert_well_formed};

const THEMES: &str = include_str!("../../../packages/merlion-themes/merlion-themes.css");
const THEMES_WITH_ROLES: &str = include_str!("fixtures/stylesheets/themes-with-roles.css");

const SEEDS: [&str; 5] = [
    THEMES,
    THEMES_WITH_ROLES,
    r#":root { --brand: #0f766e; --merlion-accent: var(--brand); --merlion-stroke: 2px;
  --merlion-c-store-fill: oklch(0.7 0.1 180); --merlion-c-store-color: hsl(10 50% 20%); }
[data-theme="dark"] { --merlion-bg: #000; --merlion-fg: rgb(250 250 250 / 90%); }
@media (prefers-color-scheme: dark) { :root:not([data-theme]) { --merlion-bg: #111; }
  :root:not([data-theme]) .merlion-c-store { --merlion-tone: var(--brand); } }
.merlion-c-store, .merlion-cc-zone { --merlion-tone: var(--brand); --merlion-dash: 4 2; }
[data-theme="dark"] .merlion-c-danger { --merlion-tone: #ff0000; --merlion-dash: none; }"#,
    "</style><script>x</script>:root{--merlion-bg:url(javascript:x)}@import 'a';\
     [data-theme=\"a\\\"]{}\"]{--merlion-fg:#fff}.merlion-c-a\\62{--merlion-tone:red}",
    ":root{--a:var(--b);--b:var(--a);--merlion-bg:var(--a);--merlion-fg:var(--c,#fff)}",
];

/// Fragments spliced into seeds: CSS punctuation, at-rules, functions, tokens and bytes
/// that must never reach the output.
const DICT: [&str; 48] = [
    "{",
    "}",
    ";",
    ":",
    ",",
    "(",
    ")",
    "[",
    "]",
    "\"",
    "'",
    "\\",
    "/*",
    "*/",
    "<",
    "&",
    "@",
    "@media (prefers-color-scheme: dark)",
    "@import",
    "@font-face",
    "@supports",
    "url(",
    "var(",
    "var(--merlion-bg)",
    "color-mix(in oklab, red, blue)",
    "!important",
    "calc(",
    ":root",
    "[data-theme=\"x\"]",
    ":root:not([data-theme])",
    ".merlion-c-a",
    ".merlion-cc-b",
    ".merlion ",
    "--merlion-bg: ",
    "--merlion-tone: ",
    "--merlion-dash: ",
    "--merlion-c-a-fill: ",
    "--merlion-stroke: ",
    "--x: ",
    "#fff",
    "oklch(0.5 0.1 90)",
    "rgb(1 2 3)",
    "none",
    "6 4",
    "\u{0}",
    "\u{202e}",
    "é",
    "\n",
];

fn rng(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

fn char_floor(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn mutate(seed: &str, st: &mut u64) -> String {
    let mut s = String::from(seed);
    let n = 1 + rng(st) % 6;
    for _ in 0..n {
        let len = s.len().max(1);
        let at = char_floor(&s, (rng(st) as usize) % len);
        match rng(st) % 5 {
            0 => {
                let w = DICT[(rng(st) as usize) % DICT.len()];
                s.insert_str(at, w);
            }
            1 => {
                let end = char_floor(&s, at + (rng(st) as usize) % 64);
                s.replace_range(at..end, "");
            }
            2 => {
                let end = char_floor(&s, at + (rng(st) as usize) % 256);
                let dup = s[at..end].to_string();
                s.insert_str(end, &dup);
            }
            3 => {
                let c = (32 + rng(st) % 95) as u8 as char;
                s.insert(at, c);
            }
            _ => s.truncate(at),
        }
    }
    s
}

fn random_text(st: &mut u64) -> String {
    let n = (rng(st) % 400) as usize;
    (0..n)
        .map(|_| DICT[(rng(st) as usize) % DICT.len()])
        .collect()
}

const DIAGRAM: &str = "flowchart LR
  subgraph z [Z]
    a[A]:::store --> b[B]:::danger
  end
  b e1@--> c[C]:::a
  c e2@--x d[D]:::x
  class z zone
  class e1 failure
  class e2 a
  classDef store fill:#ff0000,stroke:#00ff00,color:#0000ff
";

fn check(css: &str, render_it: bool) {
    let limits = StylesheetLimits::default();
    let (sheet, d) = compile(css, &limits);
    assert!(
        d.items.len() <= limits.diagnostics + 1,
        "{} diagnostics",
        d.items.len()
    );
    let Some(sheet) = sheet else {
        assert!(
            d.items.iter().any(|x| x.code == "E013"),
            "None without E013"
        );
        return;
    };
    let out = sheet.to_css();
    assert_page_css_safe(&out);
    let (again, d2) = compile(&out, &limits);
    assert_eq!(
        again.as_ref(),
        Some(&sheet),
        "compile(to_css(compile(x))) == compile(x)"
    );
    assert_eq!(again.unwrap().to_css(), out, "fixed point");
    assert!(
        d2.items.is_empty(),
        "compiled output compiles clean: {:?}",
        d2.items
    );
    if !render_it {
        return;
    }
    let names = sheet.theme_names();
    let dark = names.first().copied();
    for theme in core::iter::once(None)
        .chain(names.iter().copied().map(Some))
        .take(4)
    {
        let palette = sheet.palette(theme, dark).expect("named themes exist");
        assert_eq!(
            Palette::parse(&palette.canonical()).as_ref(),
            Ok(&palette),
            "parse(canonical(p)) == p"
        );
        let r = render(
            DIAGRAM,
            &RenderOptions {
                id_prefix: Some("m1".into()),
                palette: Some(palette),
                ..RenderOptions::default()
            },
        );
        let svg = r.svg.expect("renders");
        assert_well_formed(&svg);
        assert_safe(&svg, "m1");
    }
}

/// Mutated canonical palettes (the WASM `palette` wire form) never panic, and whatever
/// parses serialises back to a string that parses to the same palette.
#[test]
fn mutated_palettes_never_panic() {
    let limits = StylesheetLimits::default();
    let mut st = 0x9e37_79b9_7f4a_7c15;
    let mut canon = Vec::new();
    for s in SEEDS {
        if let (Some(sheet), _) = compile(s, &limits) {
            let names = sheet.theme_names();
            if let Some(p) = sheet.palette(names.first().copied(), names.last().copied()) {
                canon.push(p.canonical());
            }
        }
    }
    assert!(!canon.is_empty());
    for _ in 0..3000 {
        let seed = &canon[(rng(&mut st) as usize) % canon.len()];
        let m = mutate(seed, &mut st);
        if let Ok(p) = Palette::parse(&m) {
            assert_eq!(Palette::parse(&p.canonical()), Ok(p.clone()), "{m}");
            let r = render(
                DIAGRAM,
                &RenderOptions {
                    id_prefix: Some("m1".into()),
                    palette: Some(p),
                    ..RenderOptions::default()
                },
            );
            let svg = r.svg.expect("renders");
            assert_safe(&svg, "m1");
        }
    }
}

#[test]
fn seeds_hold_the_properties() {
    for s in SEEDS {
        check(s, true);
    }
}

#[test]
fn mutated_stylesheets_hold_the_properties() {
    let mut st = 0x5eed_1234_abcd_ef01u64;
    for i in 0..3000 {
        let seed = SEEDS[(rng(&mut st) as usize) % SEEDS.len()];
        let css = mutate(seed, &mut st);
        check(&css, i % 20 == 0);
    }
}

#[test]
fn random_stylesheets_hold_the_properties() {
    let mut st = 0x0bad_cafe_f00d_d00du64;
    for i in 0..2000 {
        let css = random_text(&mut st);
        check(&css, i % 50 == 0);
    }
    // Raw bytes, lossily decoded.
    for _ in 0..500 {
        let n = (rng(&mut st) % 300) as usize;
        let bytes: Vec<u8> = (0..n).map(|_| rng(&mut st) as u8).collect();
        check(&String::from_utf8_lossy(&bytes), false);
    }
}

#[test]
fn inputs_at_the_limits_never_panic() {
    let l = StylesheetLimits::default();
    for css in [
        "{".repeat(l.bytes),
        "}".repeat(l.bytes),
        "@media (prefers-color-scheme: dark){".repeat(1000),
        "/*".repeat(1000),
        "\"".repeat(1000),
        ":root{--merlion-bg:var(--merlion-bg)}".to_string(),
        format!(":root{{{}}}", "--a:var(--a);".repeat(30)),
        format!(
            "{}{{--merlion-c-a-fill:#fff}}",
            ["[data-theme=\"a\"]"; 8].join(",")
        )
        .repeat(300),
    ] {
        check(&css, false);
    }
}
