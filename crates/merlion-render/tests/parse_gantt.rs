//! Calendar and dependency semantics for deterministic Gantt schedules.
mod parse_support;

use merlion_render::{
    diag::Severity,
    model::{
        gantt::{
            calendar::{self, DAY_MS},
            DisplayMode, Gantt, TimeUnit,
        },
        Diagram,
    },
    options::Limits,
    parse::ParseError,
};
use parse_support::{run, try_parse};

fn parse(body: &str) -> Gantt {
    let source = format!("gantt\ntodayMarker off\n{body}");
    let (result, diags) = try_parse(&source);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:?}"
    );
    match result {
        Ok(Diagram::Gantt(g)) => g,
        other => panic!("{other:?}: {diags:?}"),
    }
}
fn date(value: &str) -> i64 {
    calendar::parse_date(value, "YYYY-MM-DD").unwrap()
}

#[test]
fn resolves_dates_status_and_implicit_predecessors_across_sections() {
    let g=parse("title Delivery\nsection Design\nDesign:done,a,2026-10-01,7d\nsection Build\nBuild:active,crit,b,after a,5d\nShip:milestone,0d");
    assert_eq!(g.meta.title.as_deref(), Some("Delivery"));
    assert_eq!(g.sections, ["Design", "Build"]);
    assert_eq!(
        (g.tasks[0].start, g.tasks[0].end),
        (date("2026-10-01"), date("2026-10-08"))
    );
    assert_eq!(g.tasks[1].end, date("2026-10-13"));
    assert_eq!(g.tasks[2].start, g.tasks[1].end);
    assert_eq!(g.tasks[2].id, "task1");
    assert!(
        g.tasks[0].flags.done
            && g.tasks[1].flags.active
            && g.tasks[1].flags.crit
            && g.tasks[2].flags.milestone
    );
    assert_eq!(g.tasks[1].dependencies, [0]);
}

#[test]
fn forward_multiple_after_and_until_resolve_extrema() {
    let g=parse("Middle:m,after a b,until z y\nA:a,2024-01-01,2d\nB:b,2024-01-01,4d\nZ:z,2024-01-10,1d\nY:y,2024-01-08,1d");
    assert_eq!(
        (g.tasks[0].start, g.tasks[0].end),
        (date("2024-01-05"), date("2024-01-08"))
    );
    assert_eq!(g.tasks[0].dependencies, [1, 2, 3, 4]);
}

#[test]
fn excludes_merge_and_explicit_includes_override() {
    let g=parse("excludes weekends\nexcludes 2024-01-08\nincludes 2024-01-06\nA:a,2024-01-05,2d\nB:b,2024-01-05,2024-01-07");
    assert_eq!(g.tasks[0].end, date("2024-01-09"));
    assert_eq!(
        g.tasks[1].end,
        date("2024-01-07"),
        "absolute ISO end bypasses exclusions"
    );
    let g = parse("weekend friday\nexcludes weekends\nA:a,2024-01-04,2d");
    assert_eq!(g.tasks[0].end, date("2024-01-08"));
}

#[test]
fn subday_exclusions_and_inclusive_end_dates() {
    let g = parse(
        "dateFormat YYYY-MM-DD HH:mm:ss\nexcludes saturday,sunday\nA:a,2024-01-05 12:00:00,48h",
    );
    assert_eq!(
        calendar::format_iso(g.tasks[0].end),
        "2024-01-09T12:00:00.000Z"
    );
    let g = parse("inclusiveEndDates\nA:a,2024-01-01,2024-01-02");
    assert_eq!(g.tasks[0].end, date("2024-01-03"));
    let g = parse("excludes weekends\nA:a,2024-01-06,1d");
    assert_eq!(
        g.tasks[0].end,
        date("2024-01-08"),
        "exclusions count start+1 day, not the start day"
    );
    let g = parse("dateFormat YYYY-MM-DD HH:mm:ss\nexcludes weekends\nA:a,2024-01-05 20:00:00,12h");
    assert_eq!(
        calendar::format_iso(g.tasks[0].end),
        "2024-01-06T08:00:00.000Z",
        "only complete daily samples are excluded"
    );
}

#[test]
fn configuration_and_directives_are_honored() {
    let source="---\ndisplayMode: compact\nconfig:\n  gantt:\n    topAxis: true\n    tickInterval: 2day\n    axisFormat: '%m-%d'\n---\ngantt\ntodayMarker off\nA:a,2024-01-01,4d";
    let (result, diags) = try_parse(source);
    assert!(diags.is_empty(), "{diags:?}");
    let Ok(Diagram::Gantt(g)) = result else {
        panic!("{result:?}")
    };
    assert_eq!(g.display_mode, DisplayMode::Compact);
    assert!(g.top_axis);
    assert_eq!(g.tick_interval.unwrap().count, 2);
    assert_eq!(g.axis_format, "%m-%d");
    let g = parse(
        "topAxis\ndisplayMode compact\nweekday monday\ntickInterval 1week\nA:a,2024-01-01,4d",
    );
    assert_eq!(g.week_start, 1);
    assert!(g.top_axis);
}

#[test]
fn omitted_calendar_components_are_explicitly_neutral() {
    let (result, diags) = try_parse("gantt\ndateFormat HH:mm\ntodayMarker off\nA:a,09:30,1.5h");
    let Ok(Diagram::Gantt(g)) = result else {
        panic!("{result:?}")
    };
    assert_eq!(g.tasks[0].start, 34_200_000);
    assert_eq!(g.tasks[0].end, 39_600_000);
    assert_eq!(g.axis_format, "%H:%M");
    assert!(diags.iter().any(|d| d.code == "I037"));
    let g = parse("dateFormat D\nA:a,2,1d");
    assert_eq!(g.tasks[0].start, DAY_MS);
}

#[test]
fn malformed_dates_cycles_missing_dependencies_and_unsupported_syntax_locate_errors() {
    for body in [
        "A:a,2023-02-29,1d",
        "A:a,2024-13-01,1d",
        "A:1d",
        "A:a,after missing,1d",
        "A:a,after b,1d\nB:b,after a,1d",
        "A:a,2024-01-03,2024-01-02",
        "dateFormat bogus",
        "axisFormat %?",
        "tickInterval 1decade",
        "todayMarker nonsense",
        "click a nonsense",
        "A:a,2024-01-01,5min",
        "A:a,2024-01-01,1d\ndateFormat DD/MM/YYYY",
        "A:a,2024-01-01,2024-01-02\ninclusiveEndDates",
    ] {
        let source = format!("gantt\ntodayMarker off\n  {body}");
        let (result, diags) = try_parse(&source);
        assert_eq!(result, Err(ParseError::Failed), "{body}: {result:?}");
        let error = diags
            .iter()
            .find(|d| d.severity == Severity::Error)
            .unwrap();
        assert!(error.span.line >= 3, "{body}: {error:?}");
        assert!(error.span.byte_end <= source.len() as u32);
    }
}

#[test]
fn limits_and_unusable_calendars_terminate() {
    let limits = Limits {
        nodes: 1,
        ..Limits::default()
    };
    let (result, _) = run("gantt\nA:a,2024-01-01,1d\nB:b,after a,1d", false, limits);
    assert_eq!(result, Err(ParseError::TooLarge { what: "tasks" }));
    let limits = Limits {
        edges: 0,
        ..Limits::default()
    };
    let (result, _) = run("gantt\nA:a,2024-01-01,1d\nB:b,after a,1d", false, limits);
    assert_eq!(
        result,
        Err(ParseError::TooLarge {
            what: "task dependency references"
        })
    );
    let (result,diags)=try_parse("gantt\nexcludes monday,tuesday,wednesday,thursday,friday,saturday,sunday\nA:a,2024-01-01,1d");
    assert_eq!(result, Err(ParseError::Failed));
    assert!(diags.iter().any(|d| d.message.contains("10,000")));
}

#[test]
fn calendar_leap_centuries_offsets_ordinal_month_clamp_and_fractional_durations() {
    for value in [
        "0001-01-01",
        "1600-02-29",
        "1900-03-01",
        "1969-12-31",
        "2000-02-29",
        "9999-12-31",
    ] {
        assert_eq!(calendar::format_iso(date(value)), value);
    }
    assert!(calendar::parse_date("1900-02-29", "YYYY-MM-DD").is_none());
    assert_eq!(
        calendar::parse_date("2024-060", "YYYY-DDD"),
        Some(date("2024-02-29"))
    );
    assert_eq!(
        calendar::parse_date("060-2024", "DDD-YYYY"),
        Some(date("2024-02-29"))
    );
    assert_eq!(
        calendar::parse_date("2024-01-02T01:30+01:30", "YYYY-MM-DDTHH:mmZ"),
        Some(date("2024-01-02"))
    );
    assert_eq!(
        calendar::format_iso(calendar::add(date("2024-01-31"), 1, TimeUnit::Month).unwrap()),
        "2024-02-29"
    );
    assert_eq!(
        calendar::format_iso(calendar::add(date("2024-02-29"), 1, TimeUnit::Year).unwrap()),
        "2025-02-28"
    );
    assert_eq!(
        calendar::duration("1.5h"),
        Some((5_400_000, TimeUnit::Millisecond))
    );
    assert_eq!(calendar::duration("1.5d"), Some((2, TimeUnit::Day)));
    assert_eq!(calendar::duration("0.5w"), Some((4, TimeUnit::Day)));
    assert_eq!(calendar::duration("1.9M"), Some((1, TimeUnit::Month)));
    assert_eq!(calendar::parse_date("1234", "X"), Some(1_234_000));
    assert_eq!(calendar::parse_date("1234", "x"), Some(1234));
}

#[test]
fn boundary_arithmetic_and_hostile_input_do_not_panic() {
    for value in [i64::MIN, i64::MAX] {
        let _ = calendar::date_parts(value);
        let _ = calendar::format_iso(value);
        let _ = calendar::format_axis(value, "%Y-%m-%d");
        assert!(calendar::add(value, 1, TimeUnit::Year).is_none());
        assert!(calendar::parse_date(&value.to_string(), "x").is_none());
    }
    assert!(calendar::add(date("9999-12-31"), 1, TimeUnit::Day).is_none());
    assert!(calendar::add(date("0001-01-01"), -1, TimeUnit::Day).is_none());
    assert!(calendar::duration("9223372036854775807y").is_some());
    assert!(calendar::add(0, i64::MAX, TimeUnit::Year).is_none());
    assert!(calendar::duration("999999999999999999999999h").is_none());
    for value in ["💥", "éééé-01-01", "2024-01-💥", "[", "YYYY💥", ""] {
        let _ = calendar::parse_date(value, "YYYY-MM-DD");
        let _ = try_parse(&format!("gantt\n{value}"));
    }
}

#[test]
fn unpadded_month_and_day_repair_exact_fields_and_converge() {
    let source = "gantt\ntodayMarker off\n2019-09-1 label: a,2019-9-1,2019-9-2";
    let (result, diags) = try_parse(source);
    assert!(result.is_ok(), "{diags:?}");
    assert_eq!(diags.iter().filter(|d| d.code == "R020").count(), 2);
    let fixed = merlion_render::parse::repair::apply_fixes(source, &diags);
    assert_eq!(
        fixed,
        "gantt\ntodayMarker off\n2019-09-1 label: a,2019-09-01,2019-09-02"
    );
    let (result, diags) = try_parse(&fixed);
    assert!(result.is_ok());
    assert!(diags.is_empty(), "{diags:?}");
    let (result, diags) = run(source, true, Limits::default());
    assert_eq!(result, Err(ParseError::Failed));
    assert!(diags
        .iter()
        .any(|d| d.code == "R020" && d.severity == Severity::Error));
    assert!(calendar::uses_epoch_defaults("YYYY-MM"));
    assert!(calendar::time_only_format("HH[YYYY]"));
    assert!(!calendar::uses_epoch_defaults("YYYY-DDD"));
    assert_eq!(
        calendar::duration("0.5ms"),
        Some((0, TimeUnit::Millisecond))
    );
    assert_eq!(
        calendar::duration("1.0005s"),
        Some((1000, TimeUnit::Millisecond))
    );
}

#[test]
fn short_year_repairs_preserve_the_numeric_year() {
    let source = "gantt\ntodayMarker off\nTask:a,202-12-01,7d";
    let (result, diags) = try_parse(source);
    let Ok(Diagram::Gantt(g)) = result else {
        panic!("{result:?}: {diags:?}")
    };
    assert_eq!(calendar::format_iso(g.tasks[0].start), "0202-12-01");
    assert_eq!(calendar::format_iso(g.tasks[0].end), "0202-12-08");
    assert_eq!(
        merlion_render::parse::repair::apply_fixes(source, &diags),
        "gantt\ntodayMarker off\nTask:a,0202-12-01,7d"
    );
    let (result, _) = run(source, true, Limits::default());
    assert_eq!(result, Err(ParseError::Failed));
    for invalid in ["0000-12-01", "20240-12-01", "202-02-29"] {
        assert_eq!(
            try_parse(&format!("gantt\nTask:a,{invalid},1d")).0,
            Err(ParseError::Failed)
        );
    }
}

#[test]
fn inert_unsupported_tokens_diagnose_and_strict_mode_rejects() {
    let source =
        "gantt\ntodayMarker off\nexcludes weekdays\nA:a,2024-01-01,1d\nclick a call callback()";
    let (result, diags) = try_parse(source);
    assert!(result.is_ok());
    assert_eq!(diags.iter().filter(|d| d.code == "W016").count(), 2);
    let (result, _) = run(source, true, Limits::default());
    assert_eq!(result, Err(ParseError::Failed));
    let g = parse("dateFormat ss\nMarker:vert,01,\nPoint:milestone,p,02,0");
    assert_eq!(g.tasks[0].start, g.tasks[0].end);
    assert_eq!(g.tasks[1].start, g.tasks[1].end);
}

#[test]
fn repeated_ids_keep_each_task_and_dependencies_use_the_last_declaration() {
    let source = "gantt\ntodayMarker off\nFirst:a,2024-01-01,1d\nSecond:a,2024-01-05,2d\nFollower:b,after a,1d";
    let (result, diags) = try_parse(source);
    let Ok(Diagram::Gantt(g)) = result else {
        panic!("{result:?}: {diags:?}")
    };
    assert_eq!(g.tasks.len(), 3);
    assert_eq!(g.tasks[0].id, g.tasks[1].id);
    assert_eq!(g.tasks[0].start, date("2024-01-01"));
    assert_eq!(g.tasks[1].start, date("2024-01-05"));
    assert_eq!(g.tasks[2].start, date("2024-01-07"));
    assert_eq!(g.tasks[2].dependencies, [1]);
    let warning = diags
        .iter()
        .find(|d| d.code == "W016")
        .expect("duplicate ID warning");
    assert_eq!(warning.span.line, 4);
    let (result, _) = run(source, true, Limits::default());
    assert_eq!(result, Err(ParseError::Failed));
}

#[test]
fn raw_reference_format_budgets_and_repair_semantics_are_bounded() {
    let limits = Limits {
        edges: 2,
        ..Limits::default()
    };
    let (result, _) = run(
        "gantt\nA:a,2024-01-01,1d\nB:b,after a a a,1d",
        false,
        limits,
    );
    assert_eq!(
        result,
        Err(ParseError::TooLarge {
            what: "task dependency references"
        })
    );
    let limits = Limits {
        label_bytes: 8,
        ..Limits::default()
    };
    let (result, _) = run("gantt\ndateFormat YYYY-MM-DD", false, limits);
    assert_eq!(
        result,
        Err(ParseError::TooLarge {
            what: "date format"
        })
    );
    let limits = Limits {
        label_bytes: 8,
        ..Limits::default()
    };
    let (result, _) = run(
        "---\nconfig:\n  gantt:\n    axisFormat: '%Y-%m-%d %H:%M'\n---\ngantt",
        false,
        limits,
    );
    assert_eq!(
        result,
        Err(ParseError::TooLarge {
            what: "axis format"
        })
    );
    let source = "gantt\ntodayMarker off\nexcludes weekends\nA:a,2024-1-5,2024-1-7";
    let (result, diags) = try_parse(source);
    let fixed = merlion_render::parse::repair::apply_fixes(source, &diags);
    let (fixed_result, _) = try_parse(&fixed);
    let (Ok(Diagram::Gantt(before)), Ok(Diagram::Gantt(after))) = (result, fixed_result) else {
        panic!("padding repair must parse")
    };
    assert_eq!(
        (before.tasks[0].start, before.tasks[0].end),
        (after.tasks[0].start, after.tasks[0].end),
        "repair preserves explicit ISO end semantics with exclusions"
    );
}
