//! Deterministic Gantt parsing and dependency resolution.

use super::{cursor::LineIndex, label, ParseOptions, Stop};
use crate::{
    diag::{Diagnostic, Diagnostics, Fix, Severity, Span},
    model::{
        gantt::{
            calendar::{self, DAY_MS},
            DisplayMode, Excludes, Gantt, Task, TaskFlags,
        },
        Meta,
    },
};
use alloc::{collections::BTreeMap, format, string::String, vec, vec::Vec};

#[derive(Clone)]
enum Start {
    Date(i64),
    After(Vec<String>),
    Previous,
}
#[derive(Clone)]
enum End {
    Date(i64),
    Duration(i64, crate::model::gantt::TimeUnit),
    Until(Vec<String>),
}
struct Raw {
    task: Task,
    start: Start,
    end: End,
    manual_end: bool,
}

fn fail(diags: &mut Diagnostics, span: Span, message: impl Into<String>) -> Stop {
    diags.emit(Severity::Error, "E002", span, message);
    Stop::Failed
}
fn bounded_text(value: &str, opts: &ParseOptions, span: Span, diags: &mut Diagnostics) -> String {
    let mut text = label::clean_label(value);
    if label::truncate(&mut text, opts.limits.label_bytes) {
        diags.emit(
            Severity::Warning,
            "W012",
            span,
            "Gantt label exceeds the label byte limit; truncated",
        );
    }
    text
}
fn dependencies(
    value: &str,
    keyword: &str,
    remaining: &mut usize,
) -> Result<Option<Vec<String>>, Stop> {
    let Some(rest) = value.strip_prefix(keyword) else {
        return Ok(None);
    };
    if !rest.starts_with(char::is_whitespace) {
        return Ok(None);
    }
    let mut ids = Vec::new();
    for id in rest.split_whitespace() {
        *remaining = remaining
            .checked_sub(1)
            .ok_or(Stop::TooLarge("task dependency references"))?;
        ids.push(String::from(id));
    }
    if ids.is_empty() {
        Ok(None)
    } else {
        Ok(Some(ids))
    }
}

fn task_date(value: &str, fmt: &str, idx: &LineIndex, diags: &mut Diagnostics) -> Option<i64> {
    if let Some(date) = calendar::parse_date(value, fmt) {
        return Some(date);
    }
    if fmt != "YYYY-MM-DD" {
        return None;
    }
    let mut parts = value.split('-');
    let (year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some()
        || !(1..=4).contains(&year.len())
        || !(1..=2).contains(&month.len())
        || !(1..=2).contains(&day.len())
        || !year
            .bytes()
            .chain(month.bytes())
            .chain(day.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let padded = format!(
        "{:04}-{:02}-{:02}",
        year.parse::<u16>().ok()?,
        month.parse::<u8>().ok()?,
        day.parse::<u8>().ok()?
    );
    let date = calendar::parse_date(&padded, fmt)?;
    let start = (value.as_ptr() as usize).checked_sub(idx.src().as_ptr() as usize)?;
    if idx.src().get(start..start.checked_add(value.len())?) != Some(value) {
        return None;
    }
    let span = idx.span(start, start + value.len());
    diags.push(Diagnostic {
        severity: Severity::Repair,
        code: "R020",
        span,
        message: String::from(
            "padded the Gantt date to match YYYY-MM-DD; the numeric year is unchanged",
        ),
        fix: Some(Fix {
            span,
            replacement: padded,
        }),
    });
    Some(date)
}

pub(crate) fn parse_gantt(
    idx: &LineIndex,
    mut pos: usize,
    meta: Meta,
    opts: &ParseOptions,
    diags: &mut Diagnostics,
) -> Result<Gantt, Stop> {
    let mut gantt = Gantt {
        meta,
        ..Gantt::default()
    };
    let cfg = &gantt.meta.gantt;
    gantt.display_mode = cfg.display_mode.unwrap_or_default();
    gantt.top_axis = cfg.top_axis.unwrap_or(false);
    gantt.tick_interval = cfg.tick_interval;
    gantt.tick_interval_span = cfg.tick_interval_span;
    if let Some(v) = &cfg.axis_format {
        if v.len() > opts.limits.label_bytes {
            return Err(Stop::TooLarge("axis format"));
        }
        gantt.axis_format = v.clone();
        gantt.axis_format_span = cfg.axis_format_span;
    }
    let mut raws: Vec<Raw> = Vec::new();
    let mut ids = BTreeMap::<String, usize>::new();
    let mut section = None;
    let mut section_ids = BTreeMap::new();
    let mut automatic = 0usize;
    let mut dependency_refs = opts.limits.edges;
    let mut filters: Vec<(bool, String, Span)> = Vec::new();
    let mut weekend = 6u8;
    let mut inclusive = false;
    let mut today_off = false;
    let mut description: Option<(usize, String)> = None;
    while pos < idx.src().len() {
        let next = idx.next_line_start(pos);
        let rawline = idx
            .src()
            .get(pos..next)
            .unwrap_or("")
            .trim_end_matches(['\n', '\r']);
        let leading = rawline.len() - rawline.trim_start().len();
        let line = rawline.trim();
        let span = idx.span(pos + leading, pos + rawline.len());
        pos = next;
        if let Some((start, body)) = &mut description {
            if let Some((before, after)) = line.split_once('}') {
                body.push_str(before);
                if !after.trim().is_empty() {
                    return Err(fail(diags, span, "unexpected text after accDescr block"));
                }
                gantt.meta.acc_descr = Some(bounded_text(body, opts, idx.span(*start, pos), diags));
                description = None;
            } else {
                if body.len().saturating_add(line.len()) > opts.limits.label_bytes {
                    return Err(Stop::TooLarge("accessibility description"));
                }
                body.push_str(line);
                body.push('\n');
            }
            continue;
        }
        let line = line
            .split_once("%%")
            .map_or(line, |(a, _)| a)
            .trim_end()
            .trim_end_matches(';')
            .trim_end();
        if line.is_empty() {
            continue;
        }
        if super::repair::is_fence_line(line) {
            super::strip_fence_line(idx, span.byte_start as usize, diags);
            continue;
        }
        let (key, value) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(a, b)| (a, b.trim()));
        match key {
            "title" => gantt.meta.title = Some(bounded_text(value, opts, span, diags)),
            "dateFormat" => {
                if !raws.is_empty() && value != gantt.date_format {
                    return Err(fail(
                        diags,
                        span,
                        "dateFormat must be set before task declarations",
                    ));
                }
                if value.len() > opts.limits.label_bytes {
                    return Err(Stop::TooLarge("date format"));
                }
                if !calendar::valid_date_format(value) {
                    return Err(fail(
                        diags,
                        span,
                        "dateFormat contains an unsupported or malformed date token",
                    ));
                }
                gantt.date_format = String::from(value);
                gantt.date_format_span = span;
            }
            "axisFormat" => {
                if value.len() > opts.limits.label_bytes {
                    return Err(Stop::TooLarge("axis format"));
                }
                gantt.axis_format = String::from(value);
                gantt.axis_format_span = span;
            }
            "tickInterval" => {
                gantt.tick_interval = Some(calendar::tick_interval(value).ok_or_else(|| {
                    fail(
                        diags,
                        span,
                        "tickInterval requires a positive integer and a singular time unit",
                    )
                })?);
                gantt.tick_interval_span = span;
            }
            "topAxis" if value.is_empty() => gantt.top_axis = true,
            "inclusiveEndDates" if value.is_empty() => {
                if !raws.is_empty() && !inclusive {
                    return Err(fail(
                        diags,
                        span,
                        "inclusiveEndDates must be set before task declarations",
                    ));
                }
                inclusive = true;
            }
            "displayMode" => {
                gantt.display_mode = match value {
                    "compact" => DisplayMode::Compact,
                    "default" => DisplayMode::Default,
                    _ => return Err(fail(diags, span, "displayMode must be compact or default")),
                }
            }
            "weekday" => {
                gantt.week_start = calendar::weekday_name(value)
                    .ok_or_else(|| fail(diags, span, "weekday requires an English weekday name"))?
            }
            "weekend" => {
                weekend = match value {
                    "friday" => 5,
                    "saturday" => 6,
                    _ => {
                        return Err(fail(
                            diags,
                            span,
                            "weekend must start on friday or saturday",
                        ))
                    }
                }
            }
            "excludes" | "includes" => {
                for v in value.split([',', ' ']).filter(|s| !s.is_empty()) {
                    if filters.len() >= opts.limits.nodes {
                        return Err(Stop::TooLarge("calendar exclusions"));
                    }
                    filters.push((key == "includes", String::from(v), span));
                }
            }
            "todayMarker" => {
                if value == "off" {
                    today_off = true;
                } else if value.split(',').all(|part| {
                    part.split_once(':').is_some_and(|(key, val)| {
                        !key.is_empty()
                            && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                            && !val.trim().is_empty()
                    })
                }) {
                    diags.emit(
                        Severity::Info,
                        "I038",
                        span,
                        "the styled live today marker is omitted for deterministic rendering",
                    );
                    today_off = true;
                } else {
                    return Err(fail(
                        diags,
                        span,
                        "todayMarker requires off or comma-separated CSS properties",
                    ));
                }
            }
            "section" => {
                if gantt.sections.len() >= opts.limits.nodes {
                    return Err(Stop::TooLarge("sections"));
                }
                let text = bounded_text(value, opts, span, diags);
                section = Some(if let Some(&i) = section_ids.get(&text) {
                    i
                } else {
                    section_ids.insert(text.clone(), gantt.sections.len());
                    gantt.sections.push(text);
                    gantt.sections.len() - 1
                });
            }
            "accTitle:" => gantt.meta.acc_title = Some(bounded_text(value, opts, span, diags)),
            "accDescr:" => gantt.meta.acc_descr = Some(bounded_text(value, opts, span, diags)),
            "accDescr" if value.starts_with('{') => {
                let body = value.trim_start_matches('{');
                if let Some((text, after)) = body.split_once('}') {
                    if !after.trim().is_empty() {
                        return Err(fail(diags, span, "unexpected text after accDescr block"));
                    }
                    gantt.meta.acc_descr = Some(bounded_text(text, opts, span, diags));
                } else {
                    description = Some((span.byte_start as usize, String::from(body)));
                }
            }
            "click" => {
                let valid = value
                    .split_once(char::is_whitespace)
                    .is_some_and(|(id, action)| {
                        !id.is_empty()
                            && (action.trim_start().starts_with("call ")
                                || action.trim_start().starts_with("href \""))
                    });
                if !valid {
                    return Err(fail(
                        diags,
                        span,
                        "click requires a task identifier and call or href action",
                    ));
                }
                diags.emit(
                    Severity::Warning,
                    "W016",
                    span,
                    "Gantt click actions are omitted from static output",
                );
            }
            _ => {
                let (name, data) = line.split_once(':').ok_or_else(|| {
                    fail(
                        diags,
                        span,
                        "expected a Gantt directive or task label followed by ':'",
                    )
                })?;
                if raws.len() >= opts.limits.nodes {
                    return Err(Stop::TooLarge("tasks"));
                }
                let mut fields: Vec<&str> = data.split(',').map(str::trim).take(9).collect();
                if fields.len() > 8 {
                    return Err(fail(diags, span, "too many Gantt task metadata fields"));
                }
                let mut flags = TaskFlags::default();
                while let Some(flag) = fields.first().copied() {
                    match flag {
                        "done" => flags.done = true,
                        "active" => flags.active = true,
                        "crit" => flags.crit = true,
                        "milestone" => flags.milestone = true,
                        "vert" => flags.vert = true,
                        _ => break,
                    }
                    fields.remove(0);
                }
                if flags.vert && fields.len() >= 2 && fields.last() == Some(&"") {
                    if let Some(last) = fields.last_mut() {
                        *last = "0d";
                    }
                }
                if fields.iter().any(|s| s.is_empty()) || !(1..=3).contains(&fields.len()) {
                    return Err(fail(
                        diags,
                        span,
                        "a task requires [id,] [start,] duration or end date",
                    ));
                }
                let explicit = fields.len() == 3;
                let id = if explicit {
                    String::from(fields.remove(0))
                } else {
                    automatic += 1;
                    format!("task{automatic}")
                };
                if id.len() > opts.limits.label_bytes {
                    return Err(Stop::TooLarge("task identifier"));
                }
                if ids.contains_key(&id) {
                    diags.emit(
                        Severity::Warning,
                        "W016",
                        span,
                        format!("duplicate Gantt task identifier `{id}`; dependencies use the last declaration"),
                    );
                }
                let start = if fields.len() == 1 {
                    Start::Previous
                } else {
                    let value = fields.remove(0);
                    if let Some(deps) = dependencies(value, "after", &mut dependency_refs)? {
                        Start::After(deps)
                    } else {
                        Start::Date(task_date(value, &gantt.date_format, idx, diags).ok_or_else(
                            || {
                                fail(
                                    diags,
                                    span,
                                    format!(
                                        "invalid start date `{value}` for dateFormat `{}`",
                                        gantt.date_format
                                    ),
                                )
                            },
                        )?)
                    }
                };
                let value = fields[0];
                let end = if let Some(deps) = dependencies(value, "until", &mut dependency_refs)? {
                    End::Until(deps)
                } else if let Some(v) = task_date(value, &gantt.date_format, idx, diags) {
                    End::Date(if inclusive {
                        calendar::add(v, 1, crate::model::gantt::TimeUnit::Day).ok_or_else(
                            || fail(diags, span, "inclusive end date exceeds calendar range"),
                        )?
                    } else {
                        v
                    })
                } else if let Some((v, u)) = calendar::duration(value) {
                    End::Duration(v, u)
                } else if value == "0" {
                    End::Duration(0, crate::model::gantt::TimeUnit::Millisecond)
                } else {
                    return Err(fail(
                        diags,
                        span,
                        format!("invalid task duration or end date `{value}`"),
                    ));
                };
                let manual_end = calendar::parse_date(value, "YYYY-MM-DD").is_some()
                    || (gantt.date_format == "YYYY-MM-DD" && matches!(end, End::Date(_)));
                ids.insert(id.clone(), raws.len());
                raws.push(Raw {
                    task: Task {
                        id,
                        label: bounded_text(name, opts, span, diags),
                        section,
                        start: 0,
                        end: 0,
                        flags,
                        dependencies: Vec::new(),
                        span,
                    },
                    start,
                    end,
                    manual_end,
                });
            }
        }
    }
    if let Some((start, _)) = description {
        return Err(fail(
            diags,
            idx.span(start, pos),
            "unterminated accDescr block",
        ));
    }
    if calendar::format_axis(0, &gantt.axis_format).is_none() {
        return Err(fail(
            diags,
            gantt.axis_format_span,
            "unsupported axisFormat conversion",
        ));
    }
    for (include, text, span) in filters {
        if text.eq_ignore_ascii_case("weekdays") {
            diags.emit(
                Severity::Warning,
                "W016",
                span,
                "weekdays is not a Mermaid calendar exclusion and is ignored",
            );
            continue;
        }
        if !include && text == "weekends" {
            gantt.excludes.weekdays[usize::from(weekend)] = true;
            gantt.excludes.weekdays[usize::from((weekend + 1) % 7)] = true;
        } else if !include && calendar::weekday_name(&text).is_some() {
            gantt.excludes.weekdays[usize::from(calendar::weekday_name(&text).unwrap_or(0))] = true;
        } else {
            let date = calendar::parse_date(&text, &gantt.date_format)
                .or_else(|| calendar::parse_date(&text, "YYYY-MM-DD"))
                .ok_or_else(|| {
                    fail(
                        diags,
                        span,
                        format!(
                            "invalid calendar {} `{text}`",
                            if include { "include" } else { "exclusion" }
                        ),
                    )
                })?;
            let day = date.div_euclid(DAY_MS) * DAY_MS;
            if date != day {
                return Err(fail(diags,span,"calendar includes and excludes require whole dates; time-specific exclusions are unsupported"));
            }
            if include {
                gantt.excludes.included_dates.push(day);
            } else {
                gantt.excludes.dates.push(day);
            }
        }
    }
    gantt.excludes.dates.sort_unstable();
    gantt.excludes.dates.dedup();
    gantt.excludes.included_dates.sort_unstable();
    gantt.excludes.included_dates.dedup();
    let calendar = SampleCalendar::new(&gantt.excludes);
    let mut edges = 0usize;
    for (i, raw) in raws.iter_mut().enumerate() {
        let mut deps = Vec::new();
        match &raw.start {
            Start::After(names) => resolve_ids(names, &ids, &mut deps, raw.task.span, diags)?,
            Start::Previous if i > 0 => deps.push(i - 1),
            Start::Previous => {
                return Err(fail(
                    diags,
                    raw.task.span,
                    "the first task requires an explicit start date; no clock is available",
                ))
            }
            Start::Date(_) => {}
        }
        if let End::Until(names) = &raw.end {
            resolve_ids(names, &ids, &mut deps, raw.task.span, diags)?;
        }
        deps.sort_unstable();
        deps.dedup();
        edges = edges.saturating_add(deps.len());
        if edges > opts.limits.edges {
            return Err(Stop::TooLarge("task dependencies"));
        }
        raw.task.dependencies = deps;
    }
    // Kahn's algorithm resolves forward references without recursion or quadratic scans.
    let mut pending: Vec<_> = raws.iter().map(|r| r.task.dependencies.len()).collect();
    let mut followers = vec![Vec::new(); raws.len()];
    for (i, raw) in raws.iter().enumerate() {
        for &dep in &raw.task.dependencies {
            followers[dep].push(i);
        }
    }
    let mut ready: Vec<_> = pending
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    let mut cursor = 0;
    while cursor < ready.len() {
        let i = ready[cursor];
        cursor += 1;
        let start = match &raws[i].start {
            Start::Date(v) => *v,
            Start::Previous => raws[i - 1].task.end,
            Start::After(names) => names
                .iter()
                .filter_map(|n| ids.get(n))
                .map(|&d| raws[d].task.end)
                .max()
                .ok_or_else(|| fail(diags, raws[i].task.span, "after requires a dependency"))?,
        };
        let mut end = match &raws[i].end {
            End::Date(v) => *v,
            End::Duration(n, u) => calendar::add(start, *n, *u).ok_or_else(|| {
                fail(
                    diags,
                    raws[i].task.span,
                    "task duration exceeds the supported calendar range",
                )
            })?,
            End::Until(names) => names
                .iter()
                .filter_map(|n| ids.get(n))
                .map(|&d| raws[d].task.start)
                .min()
                .ok_or_else(|| fail(diags, raws[i].task.span, "until requires a dependency"))?,
        };
        if end < start {
            return Err(fail(
                diags,
                raws[i].task.span,
                "task end precedes its start",
            ));
        }
        if !raws[i].manual_end {
            end = extend_exclusions(start, end, &calendar).ok_or_else(|| {
                fail(
                    diags,
                    raws[i].task.span,
                    "calendar exclusions cannot produce an end within 10,000 added days",
                )
            })?;
        }
        raws[i].task.start = start;
        raws[i].task.end = end;
        for &follower in &followers[i] {
            pending[follower] -= 1;
            if pending[follower] == 0 {
                ready.push(follower);
            }
        }
    }
    if ready.len() != raws.len() {
        let i = pending.iter().position(|&p| p > 0).unwrap_or(0);
        return Err(fail(
            diags,
            raws[i].task.span,
            "cyclic Gantt task dependency",
        ));
    }
    if calendar::uses_epoch_defaults(&gantt.date_format) && !raws.is_empty() {
        if gantt.axis_format_span == Span::default() {
            gantt.axis_format = String::from(if calendar::time_only_format(&gantt.date_format) {
                "%H:%M"
            } else {
                "%m-%d"
            });
        }
        diags.emit(Severity::Info,"I037",gantt.date_format_span,"omitted Gantt calendar components use the neutral 1970-01-01 epoch rather than the current date");
    }
    if !today_off {
        diags.emit(Severity::Info,"I038",idx.span(0,0),"the live today marker is omitted for deterministic rendering; use todayMarker off to make this explicit");
    }
    gantt.tasks = raws.into_iter().map(|r| r.task).collect();
    Ok(gantt)
}

fn resolve_ids(
    names: &[String],
    ids: &BTreeMap<String, usize>,
    deps: &mut Vec<usize>,
    span: Span,
    diags: &mut Diagnostics,
) -> Result<(), Stop> {
    for name in names {
        deps.push(*ids.get(name).ok_or_else(|| {
            fail(
                diags,
                span,
                format!("unknown Gantt task dependency `{name}`"),
            )
        })?);
    }
    Ok(())
}
struct SampleCalendar {
    weekdays: [bool; 7],
    days: Vec<i64>,
    prefix: Vec<i64>,
}
impl SampleCalendar {
    fn new(excludes: &Excludes) -> Self {
        let mut entries = Vec::new();
        for &date in &excludes.dates {
            if !excludes.weekdays[usize::from(calendar::weekday(date))]
                && excludes.included_dates.binary_search(&date).is_err()
            {
                entries.push((date.div_euclid(DAY_MS), 1));
            }
        }
        for &date in &excludes.included_dates {
            if excludes.weekdays[usize::from(calendar::weekday(date))] {
                entries.push((date.div_euclid(DAY_MS), -1));
            }
        }
        entries.sort_unstable_by_key(|entry| entry.0);
        let mut days = Vec::new();
        let mut prefix = vec![0];
        let mut sum = 0;
        for (day, delta) in entries {
            days.push(day);
            sum += delta;
            prefix.push(sum);
        }
        Self {
            weekdays: excludes.weekdays,
            days,
            prefix,
        }
    }
}
fn excluded_count(start_day: i64, days: i64, excludes: &SampleCalendar) -> i64 {
    let mut count = 0;
    for weekday in 0..7 {
        if excludes.weekdays[weekday] {
            let first = (weekday as i64 - (start_day + 5).rem_euclid(7)).rem_euclid(7);
            if first < days {
                count += 1 + (days - 1 - first) / 7;
            }
        }
    }
    let end_day = start_day + days;
    let from = excludes.days.partition_point(|&day| day <= start_day);
    let to = excludes.days.partition_point(|&day| day <= end_day);
    count + excludes.prefix[to] - excludes.prefix[from]
}
fn extend_exclusions(start: i64, end: i64, excludes: &SampleCalendar) -> Option<i64> {
    let days = end.checked_sub(start)?.div_euclid(DAY_MS);
    if days == 0 || (!excludes.weekdays.iter().any(|&b| b) && excludes.days.is_empty()) {
        return Some(end);
    }
    let start_day = start.div_euclid(DAY_MS);
    let (mut lo, mut hi) = (days, days.checked_add(10_000)?);
    if hi - excluded_count(start_day, hi, excludes) < days {
        return None;
    }
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if mid - excluded_count(start_day, mid, excludes) >= days {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let result = end.checked_add((lo - days).checked_mul(DAY_MS)?)?;
    (calendar::date_parts(result).year <= 9999).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_exclusions_match_daily_reference_samples() {
        let mut excludes = Excludes::default();
        excludes.weekdays[0] = true;
        excludes.weekdays[6] = true;
        excludes.dates = vec![4 * DAY_MS, 11 * DAY_MS];
        excludes.included_dates = vec![3 * DAY_MS, 4 * DAY_MS, 10 * DAY_MS];
        let calendar = SampleCalendar::new(&excludes);
        for start_day in -5..20 {
            for hours in [0, 12, 24, 36, 48, 72, 168, 500] {
                let start = start_day * DAY_MS + 13 * 3_600_000;
                let mut expected = start + hours * 3_600_000;
                let mut sample = start + DAY_MS;
                while sample <= expected {
                    let day = sample.div_euclid(DAY_MS) * DAY_MS;
                    let invalid = !excludes.included_dates.contains(&day)
                        && (excludes.weekdays[usize::from(calendar::weekday(sample))]
                            || excludes.dates.contains(&day));
                    if invalid {
                        expected += DAY_MS;
                    }
                    sample += DAY_MS;
                }
                assert_eq!(
                    extend_exclusions(start, start + hours * 3_600_000, &calendar),
                    Some(expected),
                    "day{start_day},hours{hours}"
                );
            }
        }
    }
}
