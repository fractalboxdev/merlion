//! Calendar-aware Gantt layout with measured label columns and bounded ticks.

use super::LayoutError;
use crate::diag::{Diagnostics, Severity, Span};
use crate::fuel::Fuel;
use crate::geometry::gantt::{GanttGeometry, Rect, SectionGeom, TaskGeom, TextGeom, TickGeom};
use crate::math::{clamp, max, min};
use crate::model::gantt::{calendar, DisplayMode, Gantt, TickInterval, TimeUnit};
use crate::options::RenderOptions;
use crate::text::{self, LabelLayout, TextStyle, Weight};
use alloc::vec::Vec;

const PAD: f64 = 20.0;
const GAP: f64 = 12.0;
const MAX_TICKS: usize = 512;
const MAX_DAYS: i128 = 10_000;

fn fail(what: &'static str) -> LayoutError {
    LayoutError::TooLarge { what }
}
fn burn(fuel: &mut Fuel, n: u64) -> Result<(), LayoutError> {
    fuel.burn(n).map_err(|_| fail("fuel"))
}

fn measure(
    s: &str,
    size: f64,
    wrap: f64,
    bold: bool,
    tolerance: f64,
    diags: &mut Diagnostics,
) -> LabelLayout {
    let style = TextStyle {
        font_size: size,
        weight: if bold {
            Weight::SemiBold
        } else {
            Weight::Regular
        },
        italic: false,
    };
    let mut l = text::layout_label(s, &style, wrap / tolerance, diags);
    l.width *= tolerance;
    l
}

fn x_at(t: i64, start: i64, end: i64, plot: Rect) -> f64 {
    let ratio =
        (i128::from(t) - i128::from(start)) as f64 / (i128::from(end) - i128::from(start)) as f64;
    plot.x + clamp(ratio, 0.0, 1.0) * plot.w
}

#[derive(Clone, Copy)]
struct AxisInterval {
    count: i64,
    unit: TimeUnit,
}

impl From<TickInterval> for AxisInterval {
    fn from(value: TickInterval) -> Self {
        Self {
            count: i64::from(value.count),
            unit: value.unit,
        }
    }
}

fn fixed_scale(unit: TimeUnit) -> Option<i64> {
    use TimeUnit::*;
    match unit {
        Millisecond => Some(1),
        Second => Some(1_000),
        Minute => Some(60_000),
        Hour => Some(3_600_000),
        Day => Some(calendar::DAY_MS),
        Week => Some(7 * calendar::DAY_MS),
        _ => None,
    }
}

/// An arithmetic upper bound, independent of the number of authored ticks.
fn tick_bound(start: i64, end: i64, interval: AxisInterval) -> Option<i128> {
    if interval.count <= 0 {
        return None;
    }
    let span = if let Some(scale) = fixed_scale(interval.unit) {
        (i128::from(end) - i128::from(start)) / i128::from(scale)
    } else {
        let a = calendar::date_parts(start);
        let b = calendar::date_parts(end);
        match interval.unit {
            TimeUnit::Month => {
                i128::from(b.year - a.year) * 12 + i128::from(b.month) - i128::from(a.month)
            }
            TimeUnit::Year => i128::from(b.year - a.year),
            _ => return None,
        }
    };
    Some(span / i128::from(interval.count) + 1)
}

/// First aligned UTC boundary at or after `ms`. No synthetic unaligned start
/// replaces a boundary outside the supported calendar.
fn first_tick(ms: i64, interval: AxisInterval, week: u8) -> Option<i64> {
    use TimeUnit::*;
    let count = interval.count;
    if count <= 0 {
        return None;
    }
    if let Some(scale) = fixed_scale(interval.unit).filter(|_| interval.unit != Week) {
        let step = count.checked_mul(scale)?;
        let floor = ms.div_euclid(step).checked_mul(step)?;
        return if floor < ms {
            floor.checked_add(step)
        } else {
            Some(floor)
        };
    }
    let mut p = calendar::date_parts(ms);
    p.hour = 0;
    p.minute = 0;
    p.second = 0;
    p.millisecond = 0;
    match interval.unit {
        Week => {
            let day = calendar::from_parts(p)?;
            let back = (calendar::weekday(day) + 7 - week.min(6)) % 7;
            let floor = day.checked_sub(i64::from(back) * calendar::DAY_MS)?;
            if floor < ms {
                floor.checked_add(count.checked_mul(7 * calendar::DAY_MS)?)
            } else {
                Some(floor)
            }
        }
        Month => {
            let months = i64::from(p.year) * 12 + i64::from(p.month) - 1;
            let mut boundary = months.div_euclid(count) * count;
            if boundary < months || p.day != 1 || ms.rem_euclid(calendar::DAY_MS) != 0 {
                boundary = boundary.checked_add(count)?;
            }
            p.year = i32::try_from(boundary.div_euclid(12)).ok()?;
            p.month = (boundary.rem_euclid(12) + 1) as u8;
            p.day = 1;
            calendar::from_parts(p)
        }
        Year => {
            let year = i64::from(p.year);
            let mut boundary = year.div_euclid(count) * count;
            if boundary < year || p.month != 1 || p.day != 1 || ms.rem_euclid(calendar::DAY_MS) != 0
            {
                boundary = boundary.checked_add(count)?;
            }
            p.year = i32::try_from(boundary).ok()?;
            p.month = 1;
            p.day = 1;
            calendar::from_parts(p)
        }
        _ => None,
    }
}

fn automatic_interval(span: i128, width: f64) -> TickInterval {
    use TimeUnit::*;
    let need = span as f64 / max(width / 120.0, 1.0);
    let choices = [
        (1, Millisecond, 1.0),
        (10, Millisecond, 10.0),
        (100, Millisecond, 100.0),
        (1, Second, 1_000.0),
        (5, Second, 5_000.0),
        (15, Second, 15_000.0),
        (1, Minute, 60_000.0),
        (5, Minute, 300_000.0),
        (15, Minute, 900_000.0),
        (1, Hour, 3_600_000.0),
        (6, Hour, 21_600_000.0),
        (1, Day, 86_400_000.0),
        (2, Day, 172_800_000.0),
        (3, Day, 259_200_000.0),
        (1, Week, 604_800_000.0),
        (2, Week, 1_209_600_000.0),
        (1, Month, 2_678_400_000.0),
        (2, Month, 5_356_800_000.0),
        (3, Month, 8_035_200_000.0),
        (1, Year, 31_622_400_000.0),
    ];
    for (count, unit, duration) in choices {
        if duration >= need {
            return TickInterval { count, unit };
        }
    }
    let years = min(
        crate::math::ceil(need / 31_557_600_000.0),
        f64::from(u32::MAX),
    ) as u32;
    TickInterval {
        count: years.max(1),
        unit: Year,
    }
}

/// Measures and lays out a deterministic UTC schedule. Explicit intervals retain
/// every tick through 512 ticks. Denser axes draw a disclosed integer multiple of
/// the authored interval, with no iteration over the omitted ticks.
pub fn layout_gantt(
    gantt: &Gantt,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<GanttGeometry, LayoutError> {
    let spent = fuel.used();
    if gantt.tasks.len() > opts.limits.nodes || gantt.sections.len() > opts.limits.nodes {
        return Err(fail("Gantt tasks"));
    }
    if gantt.excludes.dates.len() > opts.limits.layered_nodes
        || gantt.excludes.included_dates.len() > opts.limits.layered_nodes
    {
        return Err(fail("Gantt excluded dates"));
    }
    if gantt.axis_format.len() > opts.limits.label_bytes {
        return Err(fail("Gantt axis format bytes"));
    }
    for s in gantt
        .tasks
        .iter()
        .map(|t| t.label.as_str())
        .chain(gantt.sections.iter().map(|s| s.as_str()))
        .chain(gantt.meta.title.as_deref())
    {
        if s.len() > opts.limits.label_bytes {
            return Err(fail("Gantt label bytes"));
        }
        burn(fuel, s.len() as u64 + 1)?;
    }
    burn(
        fuel,
        (gantt.excludes.dates.len() + gantt.tasks.len()) as u64,
    )?;
    let size = if opts.font_size.is_nan() {
        14.0
    } else {
        clamp(opts.font_size, 1.0, 1_000.0)
    };
    let target = if opts.target_width.is_finite() {
        max(opts.target_width, 0.0)
    } else {
        720.0
    };
    let wrap = if opts.wrap_width.is_nan() {
        200.0
    } else {
        clamp(opts.wrap_width, 1.0, 100_000.0)
    };
    let tolerance = text::width_tolerance(opts.font);
    let compact = gantt.display_mode == DisplayMode::Compact;
    let label_wrap = min(wrap, max(48.0, target * 0.30));
    let labels: Vec<_> = gantt
        .tasks
        .iter()
        .map(|t| measure(&t.label, size, label_wrap, false, tolerance, diags))
        .collect();
    let section_labels: Vec<_> = gantt
        .sections
        .iter()
        .map(|s| {
            measure(
                s,
                size,
                max(48.0, target - 2.0 * PAD),
                true,
                tolerance,
                diags,
            )
        })
        .collect();
    let label_w = labels
        .iter()
        .filter(|_| !compact)
        .fold(0.0, |w, l| max(w, l.width));
    let left = PAD + if compact { 0.0 } else { label_w + GAP };
    let mut plot = Rect {
        x: left + 8.0,
        y: 0.0,
        w: max(160.0, target - left - PAD - 16.0),
        h: 0.0,
    };
    let mut start = gantt.tasks.iter().map(|t| t.start).min().unwrap_or(0);
    let mut end = gantt
        .tasks
        .iter()
        .map(|t| t.end)
        .max()
        .unwrap_or(calendar::DAY_MS);
    if gantt.tasks.iter().any(|t| t.end < t.start) {
        return Err(fail("Gantt task time range"));
    }
    let min_time = calendar::days_from_civil(1, 1, 1).unwrap_or(0) * calendar::DAY_MS;
    let max_time = calendar::days_from_civil(9999, 12, 31).unwrap_or(0) * calendar::DAY_MS
        + calendar::DAY_MS
        - 1;
    if start < min_time || end > max_time {
        return Err(fail("Gantt calendar range"));
    }
    if end <= start {
        start = start.saturating_sub(calendar::DAY_MS / 2).max(min_time);
        end = end.saturating_add(calendar::DAY_MS / 2).min(max_time);
    }
    let authored = gantt
        .tick_interval
        .unwrap_or_else(|| automatic_interval(i128::from(end) - i128::from(start), plot.w));
    if authored.count == 0 {
        return Err(fail("Gantt tick interval"));
    }
    let mut interval = AxisInterval::from(authored);
    let bound = tick_bound(start, end, interval).ok_or_else(|| fail("Gantt tick interval"))?;
    if gantt.tick_interval.is_some() && bound > MAX_TICKS as i128 {
        burn(fuel, (gantt.axis_format.len() as u64 + 1).saturating_mul(2))?;
        let mut representative = 0.0;
        for time in [start, end] {
            let value = calendar::format_axis(time, &gantt.axis_format)
                .ok_or_else(|| fail("Gantt axis format"))?;
            let label = measure(&value, size, max(80.0, wrap), false, tolerance, diags);
            representative = max(representative, label.width);
        }
        let budget = clamp(
            crate::math::floor(max(plot.w - representative, 0.0) / (representative + GAP)),
            2.0,
            MAX_TICKS as f64,
        ) as i128;
        let multiple = bound
            .checked_add(budget - 1)
            .and_then(|n| n.checked_div(budget))
            .ok_or_else(|| fail("Gantt tick interval"))?;
        interval.count = i64::try_from(
            i128::from(interval.count)
                .checked_mul(multiple)
                .ok_or_else(|| fail("Gantt tick interval"))?,
        )
        .map_err(|_| fail("Gantt tick interval"))?;
        diags.emit(Severity::Info,"I039",gantt.tick_interval_span,alloc::format!("Gantt axis draws every {multiple} authored intervals to keep at most {budget} readable ticks; task dates are unchanged."));
    }
    let mut time = first_tick(start, interval, gantt.week_start)
        .ok_or_else(|| fail("Gantt tick interval calendar range"))?;
    let mut ticks = Vec::new();
    while time <= end {
        if ticks.len() >= MAX_TICKS {
            return Err(fail("Gantt axis ticks"));
        }
        burn(fuel, gantt.axis_format.len() as u64 + 1)?;
        let value = calendar::format_axis(time, &gantt.axis_format)
            .ok_or_else(|| fail("Gantt axis format"))?;
        let l = measure(&value, size, max(80.0, wrap), false, tolerance, diags);
        ticks.push((time, l));
        let Some(next) = calendar::add(time, interval.count, interval.unit) else {
            break;
        };
        if next <= time {
            return Err(fail("Gantt tick interval"));
        }
        time = next;
    }
    // Tick labels share a baseline. Their measured half-widths bound the minimum
    // scale, including custom formats and system-font tolerance.
    let mut minimum_plot_width = 160.0;
    if compact {
        for l in &labels {
            minimum_plot_width = max(minimum_plot_width, l.width + 16.0);
        }
    }
    for pair in ticks.windows(2) {
        let fraction = (i128::from(pair[1].0) - i128::from(pair[0].0)) as f64
            / (i128::from(end) - i128::from(start)) as f64;
        minimum_plot_width = max(
            minimum_plot_width,
            (pair[0].1.width / 2.0 + pair[1].1.width / 2.0 + GAP) / fraction,
        );
    }
    let tick_edge = ticks.iter().fold(0.0, |w, (_, l)| max(w, l.width / 2.0));
    plot.x = max(plot.x, PAD + tick_edge);
    plot.w = max(minimum_plot_width, target - plot.x - PAD - tick_edge);
    let mut width = plot.x + plot.w + PAD + tick_edge;
    for l in &section_labels {
        width = max(width, l.width + 2.0 * PAD);
    }
    for (t, l) in gantt.tasks.iter().zip(&labels) {
        if t.flags.vert {
            width = max(width, l.width + 2.0 * PAD);
        }
    }
    let title = gantt
        .meta
        .title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| measure(s, size, width - 2.0 * PAD, true, tolerance, diags));
    if let Some(title) = &title {
        width = max(width, title.width + 2.0 * PAD);
    }
    let mut y = PAD;
    let title = title.map(|label| {
        let h = label.height;
        let text = TextGeom {
            x: width / 2.0,
            y: y + h / 2.0,
            label,
        };
        y += h + GAP;
        text
    });
    let axis_h = ticks.iter().fold(size * 1.25, |h, (_, l)| max(h, l.height)) + GAP;
    let top_axis_y = y;
    if gantt.top_axis {
        y += axis_h;
    }
    // Vertical markers have measured header rows and never consume task rows.
    let mut tasks = Vec::with_capacity(gantt.tasks.len());
    for (i, t) in gantt.tasks.iter().enumerate().filter(|(_, t)| t.flags.vert) {
        let l = labels[i].clone();
        let x = x_at(t.start, start, end, plot);
        let lx = clamp(x, PAD + l.width / 2.0, width - PAD - l.width / 2.0);
        let text = TextGeom {
            x: lx,
            y: y + l.height / 2.0,
            label: l,
        };
        y += text.label.height + GAP;
        tasks.push(TaskGeom {
            index: i,
            bar: Rect {
                x,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            text,
        });
    }
    plot.y = y;
    let mut sections = Vec::new();
    let mut at = 0;
    while at < gantt.tasks.len() {
        if gantt.tasks[at].flags.vert {
            at += 1;
            continue;
        }
        let section = gantt.tasks[at].section;
        let mut until = at + 1;
        while until < gantt.tasks.len()
            && (gantt.tasks[until].flags.vert || gantt.tasks[until].section == section)
        {
            until += 1;
        }
        let band_start = y;
        let section_text = section
            .and_then(|i| section_labels.get(i))
            .cloned()
            .map(|label| {
                let h = label.height;
                let text = TextGeom {
                    x: PAD + label.width / 2.0,
                    y: y + h / 2.0 + GAP,
                    label,
                };
                y += h + 2.0 * GAP;
                text
            });
        let mut rows: Vec<(f64, f64, f64)> = Vec::new(); // right edge, top, height
        for (i, t) in gantt
            .tasks
            .iter()
            .enumerate()
            .take(until)
            .skip(at)
            .filter(|(_, t)| !t.flags.vert)
        {
            let label = labels[i].clone();
            let (x0, x1) = if t.flags.milestone {
                let x = (x_at(t.start, start, end, plot) + x_at(t.end, start, end, plot)) / 2.0;
                (x - 6.0, x + 6.0)
            } else {
                (
                    x_at(t.start, start, end, plot),
                    x_at(t.end, start, end, plot),
                )
            };
            let bar_w = max(x1 - x0, 2.0);
            let label_x = if compact {
                clamp(
                    x0 + bar_w / 2.0,
                    plot.x + label.width / 2.0,
                    plot.x + plot.w - label.width / 2.0,
                )
            } else {
                PAD + label_w - label.width / 2.0
            };
            let occupied_left = min(x0, label_x - label.width / 2.0);
            let occupied_right = max(x0 + bar_w, label_x + label.width / 2.0);
            let row_h = if compact {
                label.height + 30.0 + GAP
            } else {
                max(label.height, 20.0) + GAP
            };
            let reuse = if compact {
                rows.iter().position(|(right, _, height)| {
                    *right + GAP <= occupied_left && *height >= row_h
                })
            } else {
                None
            };
            burn(fuel, if compact { rows.len() as u64 + 1 } else { 1 })?;
            let top = if let Some(r) = reuse {
                rows[r].0 = occupied_right;
                rows[r].1
            } else {
                let top = y;
                rows.push((occupied_right, top, row_h));
                y += row_h;
                top
            };
            let cy = if compact {
                top + label.height + GAP + 10.0
            } else {
                top + (row_h - GAP) / 2.0
            };
            let text = TextGeom {
                x: label_x,
                y: if compact {
                    top + label.height / 2.0
                } else {
                    cy
                },
                label,
            };
            tasks.push(TaskGeom {
                index: i,
                bar: Rect {
                    x: x0,
                    y: cy - 10.0,
                    w: bar_w,
                    h: 20.0,
                },
                text,
            });
        }
        if let (Some(index), Some(text)) = (section, section_text) {
            sections.push(SectionGeom {
                index,
                band: Rect {
                    x: PAD,
                    y: band_start,
                    w: width - 2.0 * PAD,
                    h: y - band_start,
                },
                text,
            });
        }
        at = until;
    }
    plot.h = max(y - plot.y, 24.0);
    for t in &mut tasks {
        if gantt.tasks[t.index].flags.vert {
            t.bar.y = plot.y;
            t.bar.h = plot.h;
        }
    }
    tasks.sort_by_key(|t| t.index);
    let tick_geoms = ticks
        .into_iter()
        .map(|(time, label)| {
            let x = x_at(time, start, end, plot);
            TickGeom {
                time,
                x,
                bottom: TextGeom {
                    x,
                    y: plot.y + plot.h + GAP + label.height / 2.0,
                    label: label.clone(),
                },
                top: if gantt.top_axis {
                    Some(TextGeom {
                        x,
                        y: top_axis_y + label.height / 2.0,
                        label,
                    })
                } else {
                    None
                },
            }
        })
        .collect();
    let mut exclusions = Vec::new();
    let first_day = start.div_euclid(calendar::DAY_MS);
    let last_day = end.div_euclid(calendar::DAY_MS);
    let days = i128::from(last_day) - i128::from(first_day) + 1;
    if gantt.excludes.weekdays.iter().any(|x| *x) && days > MAX_DAYS {
        diags.emit_once(
            Severity::Info,
            "I040",
            Span::default(),
            "Excluded weekdays are too dense to mark individually at this time scale.",
        );
    } else if gantt.excludes.weekdays.iter().any(|x| *x) {
        burn(fuel, days as u64)?;
        for day in first_day..=last_day {
            let Some(ms) = day.checked_mul(calendar::DAY_MS) else {
                continue;
            };
            if gantt.excludes.weekdays[usize::from(calendar::weekday(ms))]
                && gantt.excludes.included_dates.binary_search(&ms).is_err()
            {
                excluded_rect(&mut exclusions, ms, start, end, plot);
            }
        }
    }
    for &ms in &gantt.excludes.dates {
        if gantt.excludes.included_dates.binary_search(&ms).is_err()
            && (days > MAX_DAYS || !gantt.excludes.weekdays[usize::from(calendar::weekday(ms))])
        {
            excluded_rect(&mut exclusions, ms, start, end, plot);
        }
    }
    let height = plot.y + plot.h + axis_h + PAD;
    if !width.is_finite() || !height.is_finite() || width > 1_000_000.0 || height > 10_000_000.0 {
        return Err(fail("Gantt geometry extent"));
    }
    Ok(GanttGeometry {
        width,
        height,
        font_size: size,
        plot,
        start,
        end,
        title,
        tasks,
        sections,
        ticks: tick_geoms,
        exclusions,
        fuel_used: fuel.used().saturating_sub(spent),
    })
}

fn excluded_rect(out: &mut Vec<Rect>, ms: i64, start: i64, end: i64, plot: Rect) {
    let next = ms.saturating_add(calendar::DAY_MS);
    if next <= start || ms >= end {
        return;
    }
    let x = x_at(ms, start, end, plot);
    let right = x_at(next, start, end, plot);
    out.push(Rect {
        x,
        y: plot.y,
        w: max(right - x, 0.01),
        h: plot.h,
    });
}
