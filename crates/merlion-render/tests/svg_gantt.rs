//! Structural, semantic, escaping, and computed palette checks for Gantt SVG.

mod css_support;
mod svg_support;

use css_support::{same, Engine};
use merlion_render::diag::{Diagnostics, Span};
use merlion_render::fuel::Fuel;
use merlion_render::layout::layout_gantt;
use merlion_render::model::gantt::{calendar, Gantt, Task, TaskFlags};
use merlion_render::stylesheet::{compile, StylesheetLimits};
use merlion_render::svg::{draw_gantt, outline_gantt};
use merlion_render::RenderOptions;
use std::collections::BTreeSet;
use svg_support::{all_attrs, assert_safe, assert_well_formed, style_text};

fn model() -> Gantt {
    let flags = [
        TaskFlags::default(),
        TaskFlags {
            active: true,
            ..Default::default()
        },
        TaskFlags {
            done: true,
            ..Default::default()
        },
        TaskFlags {
            crit: true,
            active: true,
            done: true,
            ..Default::default()
        },
        TaskFlags {
            milestone: true,
            ..Default::default()
        },
        TaskFlags {
            vert: true,
            ..Default::default()
        },
    ];
    Gantt {
        sections: vec!["Build & release".into()],
        tasks: flags
            .into_iter()
            .enumerate()
            .map(|(i, flags)| Task {
                id: format!("t{i}"),
                label: format!("Task {i}"),
                section: Some(0),
                start: i as i64 * calendar::DAY_MS,
                end: (i as i64 + 1) * calendar::DAY_MS,
                flags,
                dependencies: if i > 0 { vec![i - 1] } else { vec![] },
                span: Span::default(),
            })
            .collect(),
        ..Gantt::default()
    }
}
fn draw(g: &Gantt, o: &RenderOptions) -> String {
    let mut d = Diagnostics::new(false);
    let l = layout_gantt(g, o, &mut Fuel::new(o.fuel), &mut d).unwrap();
    draw_gantt(g, &l, o, "mgantt", &mut d).svg
}

#[test]
fn all_statuses_have_real_shapes_labels_and_exact_timestamp_metadata() {
    let g = model();
    let svg = draw(&g, &RenderOptions::default());
    assert_well_formed(&svg);
    assert_safe(&svg, "mgantt");
    let e = Engine::new(&svg, &[], &[], true, false);
    let groups = e.with_class("merlion-gantt-task");
    assert_eq!(groups.len(), 6);
    assert_eq!(
        e.with_class("merlion-node").len(),
        0,
        "Gantt tasks have no flowchart interaction"
    );
    assert_eq!(e.with_class("merlion-gantt-task-text").len(), 6);
    assert_eq!(e.with_class("merlion-shape").len(), 6);
    assert!(svg.contains("data-merlion-flags=\"active done crit\""));
    for t in &g.tasks {
        let marker = format!("data-merlion-id=\"{}\"", t.id);
        let at = svg.find(&marker).unwrap();
        let group = &svg[at..svg[at..].find("</g>").unwrap() + at];
        assert!(group.contains(&format!("data-merlion-start=\"{}\"", t.start)));
        assert!(group.contains(&format!("data-merlion-end=\"{}\"", t.end)));
        assert!(group.contains("data-merlion-section=\"Build &amp; release\""));
        assert!(group.contains("merlion-shape") && group.contains(&t.label));
        if t.flags.milestone {
            assert!(group.contains("<path"));
        }
        if t.flags.vert {
            assert!(group.contains("<line"));
        }
    }
    let ids = all_attrs(&svg)
        .into_iter()
        .filter(|(_, n, _)| n == "id")
        .map(|(_, _, v)| v)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), ids.iter().collect::<BTreeSet<_>>().len());
}

#[test]
fn repeated_source_ids_keep_distinct_dom_ids_and_visible_markers() {
    let mut g = model();
    for t in &mut g.tasks {
        t.id = "repeated".into();
        t.flags.vert = true;
    }
    let svg = draw(&g, &RenderOptions::default());
    assert_well_formed(&svg);
    let attrs = all_attrs(&svg);
    let ids = attrs
        .iter()
        .filter(|(_, name, _)| name == "id")
        .map(|(_, _, value)| value)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), ids.iter().collect::<BTreeSet<_>>().len());
    assert_eq!(svg.matches("data-merlion-id=\"repeated\"").count(), 6);
    let e = Engine::new(&svg, &[], &[], true, false);
    assert_eq!(e.with_class("merlion-shape").len(), 6);
    assert_eq!(e.with_class("merlion-gantt-task-text").len(), 6);
}

#[test]
fn dates_labels_accessibility_and_ids_escape_hostile_text() {
    let mut g = model();
    g.meta.title = Some("<Planning & \"release\">".into());
    g.meta.acc_title = Some("Schedule <title>".into());
    g.meta.acc_descr = Some("Description & details".into());
    g.tasks[0].id = "\" onload=\"bad<>".into();
    g.tasks[0].label = "<b>Unsafe & text</b> **bold**".into();
    g.sections[0] = "<Section> & source".into();
    let svg = draw(&g, &RenderOptions::default());
    assert_well_formed(&svg);
    assert_safe(&svg, "mgantt");
    assert!(svg.contains("<title id=\"mgantt-title\">Schedule &lt;title&gt;</title>"));
    assert!(svg.contains("<desc id=\"mgantt-desc\">Description &amp; details</desc>"));
    assert!(svg.contains("&lt;b&gt;Unsafe &amp; text&lt;/b&gt;"));
    assert!(outline_gantt(&g).contains("<b>Unsafe & text</b> bold"));
    assert!(!svg.contains("<b>"));
    let again = draw(&g, &RenderOptions::default());
    assert_eq!(svg, again);
}

#[test]
fn outline_names_sections_dependencies_statuses_and_time_only_without_a_fake_date() {
    let mut g = model();
    g.date_format = "HH:mm".into();
    g.tasks[0].start = 3_600_000;
    g.tasks[0].end = 7_200_000;
    let outline = outline_gantt(&g);
    assert!(outline.contains("Section: Build & release"));
    assert!(outline.contains("Task 0 (t0): 01:00:00.000 to 02:00:00.000"));
    assert!(outline.contains("[active done crit]; depends on t2"));
    assert!(!outline.contains("1970"));
    g.meta.acc_descr = None;
    let svg = draw(&g, &RenderOptions::default());
    assert!(svg.contains("Task 0 (t0): 01:00:00.000 to 02:00:00.000"));
}

#[test]
fn palette_tokens_control_task_statuses_axes_sections_and_literal_fallbacks() {
    let css=":root {--merlion-accent:#116688;--merlion-ok:#227744;--merlion-danger:#993355;--merlion-muted:#556677;--merlion-surface:#ddeeff;} [data-theme=\"dark\"] {--merlion-accent:#66bbdd;--merlion-ok:#77cc99;--merlion-danger:#ee99bb;--merlion-muted:#aabbcc;--merlion-surface:#112233;}";
    let (sheet, diags) = compile(css, &StylesheetLimits::default());
    assert!(diags.items.is_empty(), "{:?}", diags.items);
    let sheet = sheet.unwrap();
    let palette = sheet.palette(None, Some("dark")).unwrap();
    let g = model();
    let plain = draw(&g, &RenderOptions::default());
    let baked = draw(
        &g,
        &RenderOptions {
            palette: Some(palette),
            ..RenderOptions::default()
        },
    );
    assert_safe(&baked, "mgantt");
    assert_well_formed(&baked);
    assert!(baked.contains("stroke=\"#116688\""));
    assert!(baked.contains("stroke=\"#227744\""));
    assert!(baked.contains("stroke=\"#993355\""));
    let without_style = {
        let a = baked.find("<style>").unwrap();
        let b = baked.find("</style>").unwrap() + 8;
        format!("{}{}", &baked[..a], &baked[b..])
    };
    let fallback = Engine::new(&without_style, &[], &[], false, false);
    let fallback_strokes = fallback
        .with_class("merlion-shape")
        .into_iter()
        .map(|i| fallback.computed(i, "stroke").unwrap())
        .collect::<Vec<_>>();
    assert!(fallback_strokes.iter().any(|c| same(c, "#116688", 1)));
    for (host, dark) in [(vec![], false), (vec![("data-theme", "dark")], true)] {
        let reference = Engine::new(&plain, &host, &[&sheet.to_css()], true, dark);
        let actual = Engine::new(&baked, &host, &[], true, dark);
        for class in [
            "merlion-shape",
            "merlion-gantt-task-text",
            "merlion-gantt-axis",
            "merlion-gantt-section",
        ] {
            let a = reference.with_class(class);
            let b = actual.with_class(class);
            assert_eq!(a.len(), b.len());
            for (a, b) in a.into_iter().zip(b) {
                for prop in ["fill", "stroke"] {
                    let expected = reference.computed(a, prop).unwrap();
                    let got = actual.computed(b, prop).unwrap();
                    assert!(
                        same(&expected, &got, 1),
                        "{class} {prop}: {expected} != {got}"
                    );
                }
            }
        }
    }
    assert!(style_text(&baked).contains("prefers-color-scheme: dark"));
}

#[test]
fn disabling_automatic_tones_preserves_status_and_geometry_with_neutral_paint() {
    let g = model();
    let off = draw(
        &g,
        &RenderOptions {
            auto_tone: false,
            ..Default::default()
        },
    );
    assert!(
        !off.contains("merlion-c-danger")
            && !off.contains("merlion-c-accent")
            && !off.contains("merlion-c-ok")
    );
    assert!(off.contains("data-merlion-flags=\"active done crit\""));
    let engine = Engine::new(&off, &[], &[], true, false);
    let shapes = engine.with_class("merlion-shape");
    let neutral = engine.computed(shapes[0], "stroke").unwrap();
    for i in shapes {
        assert!(same(&neutral, &engine.computed(i, "stroke").unwrap(), 0));
    }
    let mut source_off = g.clone();
    source_off.meta.auto_tone = Some(false);
    assert_eq!(off, draw(&source_off, &RenderOptions::default()));
}

#[test]
fn task_tone_overrides_win_over_ancestor_tones_and_match_baked_palette() {
    let css=":root {--merlion-tone:#ff0000;--merlion-dash:9 9;} .merlion-c-accent {--merlion-tone:#075985;--merlion-dash:3 2;}";
    let (sheet, _) = compile(css, &StylesheetLimits::default());
    let sheet = sheet.unwrap();
    let g = model();
    let plain = draw(&g, &RenderOptions::default());
    let page = sheet.to_css();
    let reference = Engine::new(&plain, &[], &[&page], true, false);
    let baked = draw(
        &g,
        &RenderOptions {
            palette: Some(sheet.palette(None, None).unwrap()),
            ..Default::default()
        },
    );
    let actual = Engine::new(&baked, &[], &[], true, false);
    for (a, b) in reference
        .with_class("merlion-shape")
        .into_iter()
        .zip(actual.with_class("merlion-shape"))
    {
        for property in ["fill", "stroke", "stroke-dasharray"] {
            let expected = reference.computed(a, property).unwrap();
            let got = actual.computed(b, property).unwrap();
            assert!(same(&expected, &got, 1), "{property}: {expected} != {got}");
        }
    }
    let neutral = reference.with_class("merlion-shape")[0];
    assert!(
        !same(
            &reference.computed(neutral, "stroke").unwrap(),
            "#ff0000",
            0
        ),
        "ancestor tone must not reach an untoned task"
    );
    assert_safe(&baked, "mgantt");
}

#[test]
fn neutral_dark_bars_milestones_and_vertical_markers_have_visible_line_outlines() {
    let source = include_str!("../../../packages/merlion-themes/merlion-themes.css");
    let (sheet, _) = compile(source, &StylesheetLimits::default());
    let sheet = sheet.unwrap();
    let g = model();
    let plain = draw(&g, &RenderOptions::default());
    let baked = draw(
        &g,
        &RenderOptions {
            palette: Some(sheet.palette(Some("dark"), None).unwrap()),
            ..Default::default()
        },
    );
    let reference = Engine::new(
        &plain,
        &[("data-theme", "dark")],
        &[&sheet.to_css()],
        true,
        false,
    );
    let actual = Engine::new(&baked, &[], &[], true, false);
    let without_style = {
        let a = baked.find("<style>").unwrap();
        let b = baked.find("</style>").unwrap() + 8;
        format!("{}{}", &baked[..a], &baked[b..])
    };
    let fallback = Engine::new(&without_style, &[], &[], false, false);
    fn luminance(color: &str) -> f64 {
        let rgba = css_support::rgba8(color).unwrap();
        let channel = |v: u8| {
            let s = f64::from(v) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(rgba[0]) * 0.2126 + channel(rgba[1]) * 0.7152 + channel(rgba[2]) * 0.0722
    }
    let background = actual
        .computed(actual.with_class("merlion-gantt-section")[0], "fill")
        .unwrap();
    for index in [0, 4, 5] {
        let a = reference.with_class("merlion-shape")[index];
        let b = actual.with_class("merlion-shape")[index];
        let f = fallback.with_class("merlion-shape")[index];
        let stroke = actual.computed(b, "stroke").unwrap();
        assert!(same(&stroke, &reference.computed(a, "stroke").unwrap(), 1));
        assert!(same(&stroke, &fallback.computed(f, "stroke").unwrap(), 1));
        let ratio = (luminance(&stroke).max(luminance(&background)) + 0.05)
            / (luminance(&stroke).min(luminance(&background)) + 0.05);
        assert!(
            ratio >= 3.0,
            "neutral mark {index}: stroke {stroke} on {background}, contrast {ratio}"
        );
    }
    let danger = actual
        .computed(actual.with_class("merlion-shape")[3], "stroke")
        .unwrap();
    assert!(
        same(&danger, "#f47067", 1),
        "status tone survives neutral outline rules: {danger}"
    );
    assert_well_formed(&baked);
    assert_safe(&baked, "mgantt");
}
