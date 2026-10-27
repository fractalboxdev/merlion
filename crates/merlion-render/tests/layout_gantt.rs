//! Layout assertions use resolved models, independently of the parser.

use merlion_render::diag::{Diagnostics, Span};
use merlion_render::fuel::Fuel;
use merlion_render::geometry::gantt::GanttGeometry;
use merlion_render::layout::layout_gantt;
use merlion_render::model::gantt::{
    calendar, DisplayMode, Gantt, Task, TaskFlags, TickInterval, TimeUnit,
};
use merlion_render::RenderOptions;

const DAY: i64 = calendar::DAY_MS;
fn task(id: &str, start: i64, end: i64) -> Task {
    Task {
        id: id.into(),
        label: id.into(),
        section: None,
        start,
        end,
        flags: TaskFlags::default(),
        dependencies: vec![],
        span: Span::default(),
    }
}
fn layout(g: &Gantt, o: &RenderOptions) -> GanttGeometry {
    layout_gantt(g, o, &mut Fuel::new(o.fuel), &mut Diagnostics::new(false)).expect("layout")
}
fn schedule() -> Gantt {
    Gantt {
        tasks: vec![task("Design", 0, DAY * 2), task("Build", DAY * 2, DAY * 5)],
        ..Gantt::default()
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.00001, "{a} != {b}");
}

#[test]
fn measured_labels_have_a_dedicated_column_and_nonoverlapping_rows() {
    let mut g = schedule();
    g.tasks[0].label =
        "A long label with **important words** repeated many times and <br> two lines".repeat(4);
    g.sections = vec!["A long section title that wraps across the narrow viewport".into()];
    for t in &mut g.tasks {
        t.section = Some(0);
    }
    let o = RenderOptions {
        target_width: 320.0,
        font: merlion_render::FontMode::System,
        ..RenderOptions::default()
    };
    let l = layout(&g, &o);
    for t in &l.tasks {
        assert!(t.text.x + t.text.label.width / 2.0 + 12.0 <= l.plot.x);
        assert!(t.text.x - t.text.label.width / 2.0 >= 19.99);
        assert!(
            t.text.y - t.text.label.height / 2.0
                >= l.sections[0].text.y + l.sections[0].text.label.height / 2.0 + 11.99
        );
    }
    let a = &l.tasks[0];
    let b = &l.tasks[1];
    assert!(a.text.y + a.text.label.height / 2.0 + 11.99 <= b.text.y - b.text.label.height / 2.0);
    assert!(l.width >= 320.0 && l.height > 200.0);
}

#[test]
fn a_week_long_schedule_has_several_readable_automatic_date_ticks() {
    let g = Gantt {
        tasks: vec![task("Work", 0, DAY * 8)],
        ..Gantt::default()
    };
    let l = layout(&g, &RenderOptions::default());
    assert!(
        l.ticks.len() >= 3,
        "{} ticks leave the schedule's dates unclear",
        l.ticks.len()
    );
    assert!(l.width <= 720.0);
    for pair in l.ticks.windows(2) {
        assert!(
            pair[0].bottom.x + pair[0].bottom.label.width / 2.0 + 11.99
                <= pair[1].bottom.x - pair[1].bottom.label.width / 2.0
        );
    }
}

#[test]
fn bars_share_one_linear_utc_scale_and_milestones_use_duration_midpoint() {
    let mut g = schedule();
    g.tasks.push(Task {
        flags: TaskFlags {
            milestone: true,
            ..TaskFlags::default()
        },
        ..task("Release", DAY, DAY * 3)
    });
    let l = layout(&g, &RenderOptions::default());
    close(l.tasks[0].bar.x, l.plot.x);
    close(l.tasks[1].bar.x, l.plot.x + l.plot.w * 2.0 / 5.0);
    close(
        l.tasks[2].bar.x + l.tasks[2].bar.w / 2.0,
        l.plot.x + l.plot.w * 2.0 / 5.0,
    );
    close(l.tasks[1].bar.x + l.tasks[1].bar.w, l.plot.x + l.plot.w);
}

#[test]
fn odd_millisecond_milestones_retain_fractional_midpoints() {
    let mut g = Gantt {
        tasks: vec![task("domain", -10, 0), task("instant", -3, -2)],
        axis_format: "%L".into(),
        ..Gantt::default()
    };
    g.tasks[1].flags.milestone = true;
    let l = layout(&g, &RenderOptions::default());
    close(l.tasks[1].bar.x + 6.0, l.plot.x + l.plot.w * 0.75);
    for (start, end) in [(0, 1), (-5, 4)] {
        let g = Gantt {
            tasks: vec![Task {
                flags: TaskFlags {
                    milestone: true,
                    ..Default::default()
                },
                ..task("Half millisecond", start, end)
            }],
            axis_format: "%L".into(),
            ..Gantt::default()
        };
        let l = layout(&g, &RenderOptions::default());
        close(
            l.tasks[0].bar.x + l.tasks[0].bar.w / 2.0,
            l.plot.x + l.plot.w / 2.0,
        );
    }
}

#[test]
fn vertical_markers_use_start_while_milestones_use_midpoint() {
    let g = Gantt {
        tasks: vec![
            task("domain", 0, DAY * 10),
            Task {
                flags: TaskFlags {
                    vert: true,
                    ..Default::default()
                },
                ..task("Boundary", DAY * 2, DAY * 4)
            },
        ],
        ..Gantt::default()
    };
    let l = layout(&g, &RenderOptions::default());
    close(l.tasks[1].bar.x, l.plot.x + l.plot.w * 0.2);
}

#[test]
fn ordinary_diagrams_fit_the_requested_width_when_labels_allow_it() {
    let l = layout(&schedule(), &RenderOptions::default());
    assert!(l.width <= 720.001, "{}", l.width);
}

#[test]
fn compact_rows_pack_only_when_the_labels_and_shapes_clear() {
    let mut g = Gantt {
        tasks: vec![
            task("A", 0, DAY),
            task("B", DAY * 4, DAY * 5),
            task("C", DAY / 2, DAY * 2),
        ],
        display_mode: DisplayMode::Compact,
        ..Gantt::default()
    };
    let l = layout(&g, &RenderOptions::default());
    close(l.tasks[0].bar.y, l.tasks[1].bar.y);
    assert!(l.tasks[2].bar.y > l.tasks[0].bar.y);
    g.tasks[1].label = "A label that is deliberately too long to share the first row".repeat(3);
    let l = layout(&g, &RenderOptions::default());
    for a in &l.tasks {
        for b in &l.tasks {
            if a.index < b.index && a.bar.y == b.bar.y {
                let a_right = (a.text.x + a.text.label.width / 2.0).max(a.bar.x + a.bar.w);
                let b_left = (b.text.x - b.text.label.width / 2.0).min(b.bar.x);
                assert!(a_right + 11.99 <= b_left);
            }
        }
    }
}

#[test]
fn weekly_ticks_align_to_the_configured_weekday_and_top_axis_has_its_own_space() {
    let start = calendar::parse_date("2024-02-01", "YYYY-MM-DD").unwrap();
    let mut g = Gantt {
        tasks: vec![task("Work", start, start + DAY * 28)],
        tick_interval: Some(TickInterval {
            count: 1,
            unit: TimeUnit::Week,
        }),
        week_start: 1,
        top_axis: true,
        ..Gantt::default()
    };
    g.tasks.push(Task {
        flags: TaskFlags {
            vert: true,
            ..TaskFlags::default()
        },
        ..task("Review", start + DAY, start + DAY)
    });
    let l = layout(&g, &RenderOptions::default());
    assert_eq!(l.ticks.len(), 4);
    for tick in &l.ticks {
        assert_eq!(calendar::weekday(tick.time), 1);
        let top = tick.top.as_ref().unwrap();
        assert!(
            top.y + top.label.height / 2.0 + 11.99
                <= l.tasks[1].text.y - l.tasks[1].text.label.height / 2.0
        );
    }
    close(l.tasks[1].bar.h, l.plot.h);
    close(l.tasks[1].bar.y, l.plot.y);
    let without = layout(
        &Gantt {
            tasks: g.tasks[..1].to_vec(),
            ..g.clone()
        },
        &RenderOptions::default(),
    );
    close(l.plot.h, without.plot.h);
}

#[test]
fn custom_tick_labels_expand_width_without_hiding_explicit_ticks() {
    let g = Gantt {
        tasks: vec![task("Work", 0, DAY * 8)],
        tick_interval: Some(TickInterval {
            count: 1,
            unit: TimeUnit::Day,
        }),
        axis_format: "%A %B %d %Y".into(),
        ..Gantt::default()
    };
    let l = layout(
        &g,
        &RenderOptions {
            target_width: 240.0,
            ..RenderOptions::default()
        },
    );
    assert_eq!(l.ticks.len(), 9);
    assert!(l.width > 240.0);
    for pair in l.ticks.windows(2) {
        assert!(
            pair[0].bottom.x + pair[0].bottom.label.width / 2.0 + 11.99
                <= pair[1].bottom.x - pair[1].bottom.label.width / 2.0
        );
    }
    assert!(l.ticks[0].x - l.ticks[0].bottom.label.width / 2.0 >= 19.99);
    assert!(
        l.ticks.last().unwrap().x + l.ticks.last().unwrap().bottom.label.width / 2.0
            <= l.width - 19.99
    );
}

#[test]
fn excluded_days_respect_includes_and_are_not_painted_twice() {
    let mut g = Gantt {
        tasks: vec![task("Work", 0, DAY * 7)],
        ..Gantt::default()
    };
    // Epoch starts Thursday. Friday and Saturday are excluded; Saturday is included.
    g.excludes.weekdays[5] = true;
    g.excludes.weekdays[6] = true;
    g.excludes.dates = vec![DAY, DAY * 3];
    g.excludes.included_dates = vec![DAY * 2];
    let l = layout(&g, &RenderOptions::default());
    assert_eq!(l.exclusions.len(), 2);
    close(l.exclusions[0].x, l.plot.x + l.plot.w / 7.0);
    close(l.exclusions[1].x, l.plot.x + l.plot.w * 3.0 / 7.0);
}

#[test]
fn single_milestone_at_calendar_limits_and_empty_diagrams_have_finite_geometry() {
    for time in [
        calendar::parse_date("0001-01-01", "YYYY-MM-DD").unwrap(),
        calendar::parse_date("9999-12-31", "YYYY-MM-DD").unwrap(),
        0,
    ] {
        let g = Gantt {
            tasks: vec![Task {
                flags: TaskFlags {
                    milestone: true,
                    ..TaskFlags::default()
                },
                ..task("Instant", time, time)
            }],
            ..Gantt::default()
        };
        let l = layout(&g, &RenderOptions::default());
        assert!(l.width.is_finite() && l.height.is_finite());
    }
    let l = layout(&Gantt::default(), &RenderOptions::default());
    assert!(l.width > 0.0 && l.height > 0.0);
}

#[test]
fn hostile_options_and_huge_glyphs_keep_text_inside_the_svg() {
    for compact in [DisplayMode::Default, DisplayMode::Compact] {
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let g = Gantt {
                display_mode: compact,
                tasks: vec![task("WWWW", 0, DAY)],
                ..Gantt::default()
            };
            let l = layout(
                &g,
                &RenderOptions {
                    target_width: width,
                    font_size: 1_000.0,
                    wrap_width: 1.0,
                    ..RenderOptions::default()
                },
            );
            for t in &l.tasks {
                assert!(t.text.x - t.text.label.width / 2.0 >= 0.0);
                assert!(t.text.x + t.text.label.width / 2.0 <= l.width);
            }
            assert!(l.width.is_finite() && l.height.is_finite());
        }
    }
}

#[test]
fn dense_axes_coarsen_while_model_limits_and_low_fuel_still_fail() {
    let mut g = schedule();
    g.tick_interval = Some(TickInterval {
        count: 1,
        unit: TimeUnit::Millisecond,
    });
    let mut diagnostics = Diagnostics::new(false);
    let dense = layout_gantt(
        &g,
        &RenderOptions::default(),
        &mut Fuel::new(100_000),
        &mut diagnostics,
    )
    .unwrap();
    assert!(dense.ticks.len() <= 512);
    assert!(diagnostics.items.iter().any(|d| d.code == "I039"));
    g.tick_interval = None;
    assert!(layout_gantt(
        &g,
        &RenderOptions::default(),
        &mut Fuel::new(1),
        &mut Diagnostics::new(false)
    )
    .is_err());
    let o = RenderOptions {
        limits: merlion_render::options::Limits {
            nodes: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(layout_gantt(
        &g,
        &o,
        &mut Fuel::new(100_000),
        &mut Diagnostics::new(false)
    )
    .is_err());
    g.tasks[0].start = i64::MIN;
    g.tasks[1].end = i64::MAX;
    assert!(layout_gantt(
        &g,
        &RenderOptions::default(),
        &mut Fuel::new(100_000),
        &mut Diagnostics::new(false)
    )
    .is_err());
}

#[test]
fn multiyear_millisecond_axis_is_bounded_aligned_and_source_located() {
    let start = calendar::parse_date("2024-01-01", "YYYY-MM-DD").unwrap();
    let end = calendar::parse_date("2030-01-01", "YYYY-MM-DD").unwrap();
    let span = Span {
        line: 4,
        column: 3,
        byte_start: 40,
        byte_end: 65,
    };
    for count in [1, 7] {
        let g = Gantt {
            tasks: vec![task("Work", start, end)],
            tick_interval: Some(TickInterval {
                count,
                unit: TimeUnit::Millisecond,
            }),
            tick_interval_span: span,
            ..Gantt::default()
        };
        let mut diagnostics = Diagnostics::new(true);
        let l = layout_gantt(
            &g,
            &RenderOptions::default(),
            &mut Fuel::new(10_000),
            &mut diagnostics,
        )
        .unwrap();
        assert!(
            l.ticks.len() >= 2 && l.ticks.len() <= 12,
            "{}",
            l.ticks.len()
        );
        assert!(l.width <= 720.001, "{}", l.width);
        assert!(l.fuel_used < 10_000);
        let info = diagnostics.items.iter().find(|d| d.code == "I039").unwrap();
        assert_eq!(info.span, span);
        assert_eq!(info.severity, merlion_render::diag::Severity::Info);
        assert_eq!((g.tasks[0].start, g.tasks[0].end), (start, end));
        let stride = l.ticks[1].time - l.ticks[0].time;
        assert_eq!(stride % i64::from(count), 0);
        for t in &l.ticks {
            assert_eq!(t.time % i64::from(count), 0);
        }
        for pair in l.ticks.windows(2) {
            assert_eq!(pair[1].time - pair[0].time, stride);
        }
        let mut again = Diagnostics::new(true);
        assert_eq!(
            l,
            layout_gantt(
                &g,
                &RenderOptions::default(),
                &mut Fuel::new(10_000),
                &mut again
            )
            .unwrap()
        );
        assert_eq!(diagnostics.items, again.items);
    }
}

#[test]
fn dense_calendar_and_week_axes_preserve_integer_interval_alignment() {
    let start = calendar::parse_date("0001-01-01", "YYYY-MM-DD").unwrap();
    let end = calendar::parse_date("9999-12-31", "YYYY-MM-DD").unwrap();
    for (unit, count) in [
        (TimeUnit::Month, 3),
        (TimeUnit::Year, 5),
        (TimeUnit::Week, 2),
    ] {
        let g = Gantt {
            tasks: vec![task("Work", start, end)],
            tick_interval: Some(TickInterval { count, unit }),
            week_start: 1,
            ..Gantt::default()
        };
        let mut diagnostics = Diagnostics::new(false);
        let l = layout_gantt(
            &g,
            &RenderOptions::default(),
            &mut Fuel::new(10_000),
            &mut diagnostics,
        )
        .unwrap();
        assert!(
            l.ticks.len() <= 12 && l.ticks.len() >= 2,
            "{}",
            l.ticks.len()
        );
        assert!(diagnostics.items.iter().any(|d| d.code == "I039"));
        for t in &l.ticks {
            let p = calendar::date_parts(t.time);
            match unit {
                TimeUnit::Month => {
                    assert_eq!((p.year * 12 + i32::from(p.month) - 1) % count as i32, 0)
                }
                TimeUnit::Year => assert_eq!(p.year % count as i32, 0),
                TimeUnit::Week => assert_eq!(calendar::weekday(t.time), 1),
                _ => unreachable!(),
            }
        }
        if unit == TimeUnit::Week {
            for pair in l.ticks.windows(2) {
                assert_eq!(
                    (pair[1].time - pair[0].time) % (i64::from(count) * 7 * DAY),
                    0
                );
            }
        }
    }
}

#[test]
fn ordinary_intervals_remain_exact_and_unrepresentable_intervals_fail_clearly() {
    let mut g = Gantt {
        tasks: vec![task("Work", 0, 511)],
        axis_format: "%L".into(),
        tick_interval: Some(TickInterval {
            count: 1,
            unit: TimeUnit::Millisecond,
        }),
        ..Gantt::default()
    };
    let mut diagnostics = Diagnostics::new(false);
    let l = layout_gantt(
        &g,
        &RenderOptions::default(),
        &mut Fuel::new(100_000),
        &mut diagnostics,
    )
    .unwrap();
    assert_eq!(l.ticks.len(), 512);
    assert!(!diagnostics.items.iter().any(|d| d.code == "I039"));
    for (i, t) in l.ticks.iter().enumerate() {
        assert_eq!(t.time, i as i64);
    }
    g.tick_interval = Some(TickInterval {
        count: u32::MAX,
        unit: TimeUnit::Year,
    });
    let error = layout_gantt(
        &g,
        &RenderOptions::default(),
        &mut Fuel::new(100_000),
        &mut Diagnostics::new(false),
    )
    .unwrap_err();
    assert_eq!(
        error,
        merlion_render::layout::LayoutError::TooLarge {
            what: "Gantt tick interval calendar range"
        }
    );
    g.tick_interval = Some(TickInterval {
        count: 0,
        unit: TimeUnit::Day,
    });
    assert!(layout_gantt(
        &g,
        &RenderOptions::default(),
        &mut Fuel::new(100_000),
        &mut Diagnostics::new(false)
    )
    .is_err());
}
