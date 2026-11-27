//! Quadrant chart layout and SVG: placement, structure, escaping, theming and
//! precedence of point colours (specs/quadrant.md).

mod css_support;
mod svg_support;

use css_support::{same, Engine};
use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::layout::layout_quadrant;
use merlion_render::model::Diagram;
use merlion_render::parse::{parse, ParseOptions};
use merlion_render::stylesheet::{compile, StylesheetLimits};
use merlion_render::{render, RenderError, RenderOptions};
use std::collections::BTreeSet;
use svg_support::{all_attrs, assert_safe, assert_well_formed, style_text};

const CAMPAIGNS: &str = include_str!("fixtures/quadrant/campaigns.mmd");
const PRIORITIES: &str = include_str!("fixtures/quadrant/priorities.mmd");
const STYLED: &str = include_str!("fixtures/quadrant/styled.mmd");
const THEME: &str = include_str!("fixtures/quadrant/theme.mmd");

fn quadrant(src: &str) -> merlion_render::model::Quadrant {
    let mut d = Diagnostics::new(false);
    let opts = ParseOptions {
        strict: false,
        limits: Default::default(),
    };
    match parse(src, &opts, &mut d) {
        Ok(Diagram::Quadrant(q)) => q,
        other => panic!("{other:?}: {:?}", d.items),
    }
}

fn geometry(
    src: &str,
    opts: &RenderOptions,
) -> merlion_render::geometry::quadrant::QuadrantGeometry {
    let q = quadrant(src);
    let mut d = Diagnostics::new(false);
    layout_quadrant(&q, opts, &mut Fuel::new(opts.fuel), &mut d).unwrap()
}

fn svg(src: &str, opts: &RenderOptions) -> String {
    let out = render(src, opts);
    assert_eq!(out.error, None, "{:?}", out.diagnostics);
    out.svg.unwrap()
}

fn with_id(id: &str) -> RenderOptions {
    RenderOptions {
        id_prefix: Some(id.into()),
        ..RenderOptions::default()
    }
}

#[test]
fn layout_follows_mermaids_builder_with_measured_labels() {
    let g = geometry(CAMPAIGNS, &RenderOptions::default());
    assert_eq!((g.width, g.height), (500.0, 500.0));
    let p = g.plot;
    // Quadrant 1 top right, 2 top left, 3 bottom left, 4 bottom right.
    let r: Vec<_> = g.quadrants.iter().map(|q| q.rect).collect();
    assert_eq!((r[0].x, r[0].y), (p.x + p.w / 2.0, p.y));
    assert_eq!((r[1].x, r[1].y), (p.x, p.y));
    assert_eq!((r[2].x, r[2].y), (p.x, p.y + p.h / 2.0));
    assert_eq!((r[3].x, r[3].y), (p.x + p.w / 2.0, p.y + p.h / 2.0));
    // Points sit at their coordinates within the plot.
    let first = &g.points[0];
    assert!((first.cx - (p.x + 0.3 * p.w)).abs() < 1e-9);
    assert!((first.cy - (p.y + 0.4 * p.h)).abs() < 1e-9);
    // With points, the x-axis labels sit below the plot and the quadrant labels at the
    // top of their quadrants; the y-axis labels sit left of the plot.
    assert!(g.x_labels.iter().all(|l| l.text.y > p.y + p.h));
    assert!(g.y_labels.iter().all(|l| l.text.x < p.x));
    for q in &g.quadrants {
        let l = q.label.as_ref().unwrap();
        assert!(l.y - l.label.height / 2.0 < q.rect.y + 6.0);
    }
    // Two labels on an axis centre on their halves.
    let low = g.x_labels.iter().find(|l| !l.high).unwrap();
    assert!((low.text.x - (p.x + p.w / 4.0)).abs() < 1e-9);
    // A point's label hangs below the point.
    assert!(g.points.iter().all(|pt| pt.label.y > pt.cy));
}

#[test]
fn without_points_the_axes_follow_the_configuration_and_labels_centre() {
    let g = geometry(PRIORITIES, &RenderOptions::default());
    let p = g.plot;
    assert!(g.x_labels.iter().all(|l| l.text.y < p.y), "x-axis on top");
    assert!(
        g.y_labels.iter().all(|l| l.text.x > p.x + p.w),
        "y-axis on the right"
    );
    for q in &g.quadrants {
        let l = q.label.as_ref().unwrap();
        assert!((l.y - (q.rect.y + q.rect.h / 2.0)).abs() < 1e-9);
    }
    // A lone x label starts at the left of the plot; a lone y label at its bottom.
    let g = geometry(STYLED, &RenderOptions::default());
    let x = &g.x_labels[0];
    assert!((x.text.x - x.text.label.width / 2.0 - g.plot.x).abs() < 1e-9);
    let y = &g.y_labels[0];
    assert!((y.text.y + y.text.label.width / 2.0 - (g.plot.y + g.plot.h)).abs() < 1e-9);
}

#[test]
fn configuration_sets_the_chart_size_and_a_narrow_container_scales_it() {
    let g = geometry(THEME, &RenderOptions::default());
    assert_eq!((g.width, g.height), (420.0, 360.0));
    assert!(g.points.iter().all(|p| p.radius == 6.0));
    let g = geometry(
        CAMPAIGNS,
        &RenderOptions {
            target_width: 300.0,
            ..RenderOptions::default()
        },
    );
    assert_eq!((g.width, g.height), (300.0, 300.0));
    // A label at the right edge stays inside the chart.
    let g = geometry(
        "quadrantChart\n  A rather long point label: [1, 1]\n",
        &RenderOptions::default(),
    );
    let l = &g.points[0].label;
    assert!(l.x + l.label.width / 2.0 <= g.width);
    // A large point keeps its label clear of the circle.
    let g = geometry(
        "quadrantChart\n  Big: [0.5, 0.5] radius: 30\n",
        &RenderOptions::default(),
    );
    let pt = &g.points[0];
    assert!(pt.label.y - pt.label.label.height / 2.0 >= pt.cy + 30.0);
}

#[test]
fn wrapped_quadrant_labels_fit_their_boxes_and_viewport() {
    let label = "This quadrant label contains a fairly long description that needs multiple lines of text to remain readable in a narrow document";
    for points in ["", "A: [0.2, 0.3]\n"] {
        let source = format!("quadrantChart\nquadrant-1 {label}\nquadrant-3 {label}\n{points}");
        let g = geometry(
            &source,
            &RenderOptions {
                target_width: 200.0,
                ..RenderOptions::default()
            },
        );
        assert!(g.height > 200.0);
        for q in &g.quadrants {
            if let Some(t) = &q.label {
                assert!(t.y - t.label.height / 2.0 >= q.rect.y);
                assert!(t.y + t.label.height / 2.0 <= q.rect.y + q.rect.h);
                assert!(t.y + t.label.height / 2.0 <= g.height);
            }
        }
    }
}

#[test]
fn styled_corner_points_fit_the_viewport_and_keep_their_coordinates() {
    let g = geometry("quadrantChart\nA: [0, 1] radius: 30, stroke-width: 10px\nB: [1, 1] radius: 40, stroke-width: 20px\nC: [0, 0] radius: 35, stroke-width: 10px\nD: [1, 0] radius: 30, stroke-width: 10px\n", &RenderOptions::default());
    for (p, (x, y)) in g
        .points
        .iter()
        .zip([(0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 0.0)])
    {
        let extent = p.radius + p.stroke_width / 2.0;
        assert!(p.cx - extent >= 0.0 && p.cy - extent >= 0.0);
        assert!(p.cx + extent <= g.width && p.cy + extent <= g.height);
        assert!((p.cx - g.plot.x - x * g.plot.w).abs() < 1e-9);
        assert!((p.cy - g.plot.y - (1.0 - y) * g.plot.h).abs() < 1e-9);
    }
}

#[test]
fn svg_structure_data_attributes_and_accessibility() {
    let s = svg(CAMPAIGNS, &with_id("q1"));
    assert_well_formed(&s);
    assert_safe(&s, "q1");
    assert!(s.contains("class=\"merlion merlion-quadrant\""));
    assert!(s.contains("<title id=\"q1-title\">Reach and engagement of campaigns</title>"));
    assert!(s.contains("<desc id=\"q1-desc\">Quadrant chart. 6 points."));
    assert_eq!(s.matches("class=\"merlion-quadrant-region").count(), 4);
    assert_eq!(s.matches("<line").count(), 6);
    assert_eq!(s.matches("class=\"merlion-quadrant-point").count(), 6);
    assert!(s.contains("data-merlion-id=\"Spring launch\" id=\"q1-point-0\" data-merlion-x=\"0.3\" data-merlion-y=\"0.6\""));
    assert!(s.contains("data-merlion-axis=\"y\" data-merlion-end=\"high\" transform=\"rotate(-90 "));
    let ids: Vec<String> = all_attrs(&s)
        .into_iter()
        .filter(|(_, n, _)| n == "id")
        .map(|(_, _, v)| v)
        .collect();
    assert_eq!(ids.len(), ids.iter().collect::<BTreeSet<_>>().len());
    let out = render(CAMPAIGNS, &RenderOptions::default());
    let outline = out.outline.unwrap();
    assert!(outline.contains("x-axis: Low reach → High reach"));
    assert!(outline.contains("Quadrant 1, top right: Expand"));
    assert!(outline.contains("Spring launch: x 0.3, y 0.6, quadrant 2 (Promote)"));
    // accDescr replaces the description, not the outline.
    let s = svg(STYLED, &with_id("q2"));
    assert!(s.contains("<title id=\"q2-title\">Supplier risk</title>"));
    assert!(s.contains("<desc id=\"q2-desc\">Suppliers placed by delivery risk"));
}

#[test]
fn hostile_text_stays_inert() {
    let src = "quadrantChart\n  title <script>alert(1)</script> & co\n  quadrant-1 \"<img src=x onerror=alert(1)>\"\n  \"onload=x <b>bold</b>\": [0.2, 0.2]\n";
    let s = svg(src, &with_id("q3"));
    assert_well_formed(&s);
    assert_safe(&s, "q3");
    assert!(!s.contains("<script") && !s.contains("<img"));
    assert!(s.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; co"));
    assert!(s.contains("data-merlion-id=\"onload=x &lt;b&gt;bold&lt;/b&gt;\""));
}

#[test]
fn point_colours_follow_mermaids_precedence_through_css() {
    let s = svg(STYLED, &with_id("q4"));
    let e = Engine::new(&s, &[], &[], true, false);
    // Points draw in reverse declaration order, so an earlier point stays on top.
    let mut dots = e.with_class("merlion-quadrant-dot");
    dots.reverse();
    let fill = |i: usize| e.computed(dots[i], "fill").unwrap();
    // Northwind:::danger takes the danger tone; Contoso:::warn the warn tone.
    assert!(same(&fill(0), "#cf222e", 1), "{}", fill(0));
    assert!(same(&fill(1), "#9a6700", 1), "{}", fill(1));
    // Fabrikam has no role: the accent.
    assert!(same(&fill(2), "#0969da", 1), "{}", fill(2));
    // Litware:::key reads the classDef token with its literal fallback.
    assert!(same(&fill(3), "#6f5bd6", 1), "{}", fill(3));
    assert!(style_text(&s).contains("var(--merlion-c-key-fill, #6f5bd6)"));
    // Tailspin's own style.
    assert!(same(&fill(4), "#127a84", 1), "{}", fill(4));
    assert!(same(&e.computed(dots[4], "stroke").unwrap(), "#0b4f55", 1));
    // A page that sets the class token re-themes the class.
    let page = ":root { --merlion-c-key-fill: #112233; }";
    let e = Engine::new(&s, &[], &[page], true, false);
    assert!(same(&e.computed(dots[3], "fill").unwrap(), "#112233", 1));
    // Without CSS, the presentation attributes draw the same colours.
    let bare = {
        let a = s.find("<style>").unwrap();
        let b = s.find("</style>").unwrap() + 8;
        format!("{}{}", &s[..a], &s[b..])
    };
    let f = Engine::new(&bare, &[], &[], false, false);
    let mut bare_dots = f.with_class("merlion-quadrant-dot");
    bare_dots.reverse();
    for (i, want) in [
        (0, "#cf222e"),
        (2, "#0969da"),
        (3, "#6f5bd6"),
        (4, "#127a84"),
    ] {
        assert!(
            same(&f.computed(bare_dots[i], "fill").unwrap(), want, 1),
            "{i}"
        );
    }
}

#[test]
fn theme_variables_are_literals_and_the_rest_follows_tokens() {
    let s = svg(THEME, &with_id("q5"));
    let e = Engine::new(&s, &[], &[], true, false);
    let boxes = e.with_class("merlion-quadrant-box");
    assert!(same(&e.computed(boxes[0], "fill").unwrap(), "#e6f4ea", 1));
    let dots = e.with_class("merlion-quadrant-dot");
    assert!(same(&e.computed(dots[0], "fill").unwrap(), "#1a7f37", 1));
    // A page theme moves every quadrant the source leaves to the theme.
    let page = ":root { --merlion-accent: #ff0000; --merlion-bg: #000000; }";
    let themed = Engine::new(&s, &[], &[page], true, false);
    assert!(same(
        &themed.computed(boxes[0], "fill").unwrap(),
        "#e6f4ea",
        1
    ));
    assert!(!same(
        &themed.computed(boxes[1], "fill").unwrap(),
        &e.computed(boxes[1], "fill").unwrap(),
        4
    ));
}

#[test]
fn baked_palettes_match_the_page_css_in_light_and_dark() {
    let css = ":root {--merlion-accent:#116688;--merlion-bg:#fdfcf8;--merlion-fg:#1b1b1b;--merlion-danger:#993355;} [data-theme=\"dark\"] {--merlion-accent:#66bbdd;--merlion-bg:#101418;--merlion-fg:#e6e6e6;--merlion-danger:#ee99bb;} .merlion-c-key {--merlion-tone:#aa5500;}";
    let (sheet, diags) = compile(css, &StylesheetLimits::default());
    assert!(diags.items.is_empty(), "{:?}", diags.items);
    let sheet = sheet.unwrap();
    let palette = sheet.palette(None, Some("dark")).unwrap();
    let src = STYLED.replace("  classDef key color: #6f5bd6, radius: 9\n", "");
    let plain = svg(&src, &with_id("q6"));
    let baked = svg(
        &src,
        &RenderOptions {
            palette: Some(palette),
            ..with_id("q6")
        },
    );
    assert_safe(&baked, "q6");
    assert!(style_text(&baked).contains("prefers-color-scheme: dark"));
    for (host, dark) in [(vec![], false), (vec![("data-theme", "dark")], true)] {
        let reference = Engine::new(&plain, &host, &[&sheet.to_css()], true, dark);
        let actual = Engine::new(&baked, &host, &[], true, dark);
        for class in [
            "merlion-quadrant-box",
            "merlion-quadrant-dot",
            "merlion-quadrant-text",
            "merlion-quadrant-border",
            "merlion-quadrant-divider",
        ] {
            let a = reference.with_class(class);
            let b = actual.with_class(class);
            assert_eq!(a.len(), b.len(), "{class}");
            for (a, b) in a.into_iter().zip(b) {
                for prop in ["fill", "stroke"] {
                    let expected = reference.computed(a, prop).unwrap();
                    let got = actual.computed(b, prop).unwrap();
                    assert!(
                        same(&expected, &got, 1),
                        "{class} {prop}: {expected} != {got} (dark {dark})"
                    );
                }
            }
        }
    }
}

#[test]
fn class_scans_and_palette_tables_are_fuel_bounded() {
    let mut source = String::from("quadrantChart\n");
    for i in 0..100 {
        source.push_str(&format!("classDef c{i} radius: 1\n"));
    }
    for _ in 0..100 {
        source.push_str("A:::c99: [0.5, 0.5]\n");
    }
    let out = render(
        &source,
        &RenderOptions {
            fuel: 400,
            ..RenderOptions::default()
        },
    );
    assert!(matches!(
        out.error,
        Some(RenderError::TooLarge { what: "fuel" })
    ));
    assert!(out.svg.is_none());
    assert!(render(
        &source,
        &RenderOptions {
            fuel: 100_000,
            ..RenderOptions::default()
        }
    )
    .svg
    .is_some());

    use merlion_render::stylesheet::{Palette, PaletteTable, PaletteTone};
    let table = |count| PaletteTable {
        tones: (0..count)
            .map(|i| PaletteTone {
                name: format!("role{i}"),
                cluster: false,
                tone: None,
                dash: None,
            })
            .collect(),
        ..PaletteTable::default()
    };
    let palette = Palette {
        light: table(2),
        dark: Some(table(3)),
    };
    let options = RenderOptions {
        palette: Some(palette),
        fuel: 4,
        ..RenderOptions::default()
    };
    let out = render("quadrantChart\n", &options);
    assert!(matches!(
        out.error,
        Some(RenderError::TooLarge { what: "fuel" })
    ));
    assert!(out.svg.is_none());
    let out = render("quadrantChart\n", &RenderOptions { fuel: 5, ..options });
    assert!(out.svg.is_some());
    assert_eq!(out.fuel_used, 5);
    let out = render(
        "quadrantChart\nA:::accent: [0, 1]\n",
        &RenderOptions {
            fuel: 3,
            ..RenderOptions::default()
        },
    );
    assert!(matches!(
        out.error,
        Some(RenderError::TooLarge { what: "fuel" })
    ));
    assert!(out.svg.is_none());
}

#[test]
fn bytes_are_deterministic_and_fuel_bounds_the_work() {
    let a = svg(CAMPAIGNS, &RenderOptions::default());
    assert_eq!(a, svg(CAMPAIGNS, &RenderOptions::default()));
    let hinted = RenderOptions {
        hint: Some(a.clone()),
        ..RenderOptions::default()
    };
    assert_eq!(a, svg(CAMPAIGNS, &hinted));
    let out = render(
        CAMPAIGNS,
        &RenderOptions {
            fuel: 0,
            ..RenderOptions::default()
        },
    );
    assert!(matches!(
        out.error,
        Some(RenderError::TooLarge { what: "fuel" })
    ));
    assert!(out.svg.is_none());
}
