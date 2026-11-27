//! Chart geometry from a resolved spec (specs/chart.md#layout).
//!
//! The width is the caller's `target_width`. Margins come from measured labels: the
//! widest value tick or category label sets the left margin, and labels drawn beside
//! marks get gutters, so no text overlaps the plot edge it belongs to. Every position is
//! a pure function of the spec and the options.

use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

use super::resolve::{Item, MarkData, Pos, Resolved, Val};
use super::scale::{self, Step};
use super::spec::{MarkType, ScaleType, Spec, Tone};
use crate::diag::Diagnostics;
use crate::geometry::gantt::{Rect, TextGeom};
use crate::layout::LayoutError;
use crate::math::{clamp, max, min, round};
use crate::model::gantt::calendar;
use crate::options::RenderOptions;
use crate::text::{self, LabelLayout, TextStyle, Weight};

const PAD: f64 = 8.0;
const TICK_GAP: f64 = 6.0;
const LABEL_GAP: f64 = 5.0;
const MAX_TICKS: usize = 64;
const DOT_R: f64 = 4.5;
/// Share of a band step a bar fills; the rest separates neighbouring categories.
const BAR_SHARE: f64 = 0.72;
/// Widest bar on a vertical chart, so two categories never draw as slabs.
const MAX_BAR: f64 = 48.0;
const MIN_ROW: f64 = 28.0;
/// Surface gap between adjacent fills of one group (specs/chart.md#marks).
const FILL_GAP: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A bar whose data end is `end`; that end is rounded, the baseline end is square.
    Bar {
        rect: Rect,
        end: End,
    },
    /// A line (`fill: false`) or an area outline (`fill: true`) in SVG path syntax.
    Path {
        d: String,
        fill: bool,
        points: Vec<PointGeom>,
    },
    Dot {
        cx: f64,
        cy: f64,
        r: f64,
    },
    Segment {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        width: f64,
    },
    Shade {
        rect: Rect,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointGeom {
    pub row: Option<usize>,
    pub cx: f64,
    pub cy: f64,
    pub tip: Vec<Rc<str>>,
    pub data: Vec<(&'static str, Rc<str>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemGeom {
    pub row: Option<usize>,
    pub series: Option<usize>,
    pub shape: Shape,
    pub label: Option<TextGeom>,
    /// The item's native tooltip text.
    pub tip: Vec<Rc<str>>,
    /// `data-merlion-*` attributes: the exact values the item was drawn from.
    pub data: Vec<(&'static str, Rc<str>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkGeom {
    pub index: usize,
    pub kind: MarkType,
    pub tone: Tone,
    pub dash: bool,
    pub items: Vec<ItemGeom>,
    /// The mark's `text`, once per item for a rule or span.
    pub notes: Vec<TextGeom>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisTick {
    /// Pixel position along the axis.
    pub pos: f64,
    /// `data-merlion-value`: the tick's value as printed, or the category.
    pub value: String,
    pub text: TextGeom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendItem {
    pub series: usize,
    pub swatch: Rect,
    pub text: TextGeom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartGeometry {
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub small: f64,
    pub plot: Rect,
    pub x_ticks: Vec<AxisTick>,
    pub y_ticks: Vec<AxisTick>,
    /// Grid lines run across the plot from value-axis ticks; a band axis has none.
    pub x_grid: bool,
    pub y_grid: bool,
    pub x_title: Option<TextGeom>,
    pub y_title: Option<TextGeom>,
    /// The zero line of a linear value axis: (x1, y1, x2, y2).
    pub zero: Option<(f64, f64, f64, f64)>,
    pub legend: Vec<LegendItem>,
    pub marks: Vec<MarkGeom>,
}

struct Measure<'a> {
    size: f64,
    tolerance: f64,
    diags: &'a mut Diagnostics,
}

impl Measure<'_> {
    fn text(&mut self, s: &str, wrap: f64) -> LabelLayout {
        let style = TextStyle {
            font_size: self.size,
            weight: Weight::Regular,
            italic: false,
        };
        let mut l = text::layout_label(s, &style, wrap / self.tolerance, self.diags);
        l.width *= self.tolerance;
        l
    }
}

/// A continuous axis: its domain and, for a linear one, the tick step.
#[derive(Clone, Copy, Debug)]
struct Continuous {
    d0: f64,
    d1: f64,
    step: Option<Step>,
}

/// Whether some bar or area uses axis `x` (true) or `y` (false) as its value axis.
fn needs_zero(r: &Resolved, on_x: bool) -> bool {
    r.marks.iter().any(|m| match m.kind {
        MarkType::Bar => m.horizontal == on_x,
        MarkType::Area => !on_x && m.items.iter().any(|it| it.y2.is_none()),
        _ => false,
    })
}

fn values_on(r: &Resolved, on_x: bool) -> Vec<f64> {
    let mut out = Vec::new();
    for m in &r.marks {
        for it in &m.items {
            let vals = if on_x {
                [&it.x, &it.x2]
            } else {
                [&it.y, &it.y2]
            };
            for v in vals.into_iter().flatten() {
                if let Pos::Num(n) = v.pos {
                    out.push(n);
                }
            }
        }
    }
    out
}

fn continuous(
    spec_zero: Option<bool>,
    scale_type: ScaleType,
    authored: Option<(f64, f64)>,
    values: &[f64],
    zero: bool,
    length: f64,
    per_tick: f64,
) -> Continuous {
    let count = clamp(round(length / per_tick), 2.0, 10.0);
    if let Some((d0, d1)) = authored {
        let step = (scale_type == ScaleType::Linear).then(|| scale::domain_step(d0, d1, count));
        return Continuous { d0, d1, step };
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in values {
        lo = min(lo, v);
        hi = max(hi, v);
    }
    if !lo.is_finite() {
        lo = 0.0;
        hi = 1.0;
    }
    if scale_type == ScaleType::Time {
        if hi <= lo {
            lo -= calendar::DAY_MS as f64;
            hi += calendar::DAY_MS as f64;
        }
        return Continuous {
            d0: lo,
            d1: hi,
            step: None,
        };
    }
    if zero || spec_zero == Some(true) {
        lo = min(lo, 0.0);
        hi = max(hi, 0.0);
    }
    let (lo, hi) = scale::widen(lo, hi);
    let step = scale::domain_step(lo, hi, count);
    let (d0, d1) = scale::nice_domain(lo, hi, step);
    Continuous {
        d0,
        d1,
        step: Some(scale::domain_step(d0, d1, count)),
    }
}

/// Tick values and labels of a continuous axis.
fn continuous_ticks(
    c: &Continuous,
    scale_type: ScaleType,
    spec: &super::spec::Axis,
    length: f64,
) -> Vec<(f64, String)> {
    match (scale_type, c.step) {
        (ScaleType::Linear, Some(step)) => scale::ticks(c.d0, c.d1, step, MAX_TICKS)
            .into_iter()
            .map(|t| (t.value, scale::tick_label(t, step, &spec.format)))
            .collect(),
        (ScaleType::Time, _) => scale::time_ticks(
            c.d0 as i64,
            c.d1 as i64,
            length * 120.0 / 90.0,
            spec.time_format.as_deref(),
            MAX_TICKS,
        )
        .into_iter()
        .map(|t| (t.ms as f64, t.label))
        .collect(),
        _ => Vec::new(),
    }
}

struct Band {
    start: f64,
    step: f64,
    width: f64,
}

impl Band {
    fn centre(&self, i: usize) -> f64 {
        self.start + (i as f64 + 0.5) * self.step
    }
}

/// Lays out `spec`. Mark order is drawing order, except that spans draw first.
pub fn layout(
    spec: &Spec,
    r: &Resolved,
    opts: &RenderOptions,
    diags: &mut Diagnostics,
) -> Result<ChartGeometry, LayoutError> {
    let base = if opts.font_size.is_finite() && opts.font_size > 0.0 {
        opts.font_size
    } else {
        14.0
    };
    let small = base * 6.0 / 7.0;
    let width = max(opts.target_width, 240.0);
    let mut ms = Measure {
        size: small,
        tolerance: text::width_tolerance(opts.font),
        diags,
    };

    // Legend: every series a mark draws, in series order.
    let mut drawn: Vec<usize> = r
        .marks
        .iter()
        .flat_map(|m| m.series.iter().copied())
        .collect();
    drawn.sort_unstable();
    drawn.dedup();
    let show_legend = spec.legend.unwrap_or(drawn.len() >= 2) && !drawn.is_empty();
    let mut legend = Vec::new();
    let mut top = PAD;
    if show_legend {
        let mut x = PAD;
        let mut row_h = 0.0f64;
        for &k in &drawn {
            let label = ms.text(r.series.get(k).map_or("", AsRef::as_ref), 200.0);
            let w = 10.0 + 6.0 + label.width;
            if x > PAD && x + w > width - PAD {
                x = PAD;
                top += row_h + 4.0;
                row_h = 0.0;
            }
            let h = max(label.height, 10.0);
            legend.push(LegendItem {
                series: k,
                swatch: Rect {
                    x,
                    y: top + (h - 10.0) / 2.0,
                    w: 10.0,
                    h: 10.0,
                },
                text: TextGeom {
                    x: x + 16.0 + label.width / 2.0,
                    y: top + h / 2.0,
                    label,
                },
            });
            row_h = max(row_h, h);
            x += w + 16.0;
        }
        top += row_h + 10.0;
    }

    let y_title_label = spec
        .y
        .label
        .as_deref()
        .map(|l| ms.text(l, width - 2.0 * PAD));
    let y_title = y_title_label.map(|label| {
        let g = TextGeom {
            x: PAD + label.width / 2.0,
            y: top + label.height / 2.0,
            label,
        };
        top += g.label.height + 6.0;
        g
    });
    let line_h = ms.text("0", 1e9).height;

    let label_width = |ms: &mut Measure, m: &MarkData, positive: Option<bool>| -> f64 {
        let mut w = 0.0f64;
        for it in &m.items {
            if let Some(l) = &it.label {
                let sign_ok = match positive {
                    None => true,
                    Some(p) => value_sign(m, it) == p,
                };
                if sign_ok {
                    w = max(w, ms.text(l, 160.0).width);
                }
            }
        }
        w
    };

    let horizontal = r.y.scale == ScaleType::Band;
    let x_zero = needs_zero(r, true);
    let y_zero = needs_zero(r, false);
    let xs = values_on(r, true);
    let ys = values_on(r, false);
    let mut marks_geom: Vec<MarkGeom>;
    let plot: Rect;
    let height: f64;
    let mut x_ticks = Vec::new();
    let mut y_ticks = Vec::new();
    let mut x_cont = None;
    let mut y_cont = None;
    let mut x_band = None;
    let mut y_band = None;

    // Right gutter: labels drawn after the end of a mark, rule notes, line end labels.
    let mut right_gutter = 0.0f64;
    let mut left_gutter = 0.0f64;
    let mut top_gutter = 0.0f64;
    let mut bottom_gutter = 0.0f64;
    for m in &r.marks {
        if !m.has_labels {
            continue;
        }
        match (m.kind, horizontal || m.horizontal) {
            (MarkType::Bar, true) => {
                right_gutter = max(
                    right_gutter,
                    label_width(&mut ms, m, Some(true)) + LABEL_GAP,
                );
                left_gutter = max(
                    left_gutter,
                    label_width(&mut ms, m, Some(false)) + LABEL_GAP,
                );
            }
            (MarkType::Bar, false) => {
                if m.items
                    .iter()
                    .any(|it| it.label.is_some() && value_sign(m, it))
                {
                    top_gutter = max(top_gutter, line_h + LABEL_GAP);
                }
                if m.items
                    .iter()
                    .any(|it| it.label.is_some() && !value_sign(m, it))
                {
                    bottom_gutter = max(bottom_gutter, line_h + LABEL_GAP);
                }
            }
            (MarkType::Interval, false) => top_gutter = max(top_gutter, line_h + LABEL_GAP),
            (MarkType::Dot, true) => {
                let w = label_width(&mut ms, m, None) + DOT_R + LABEL_GAP;
                right_gutter = max(right_gutter, w);
                left_gutter = max(left_gutter, w);
            }
            _ => right_gutter = max(right_gutter, label_width(&mut ms, m, None) + 8.0),
        }
    }

    // Notes of rules across the x axis sit in a row above the plot, clear of every mark.
    let x_rule_notes = spec
        .marks
        .iter()
        .any(|m| m.kind == MarkType::Rule && m.x.is_some() && m.text.is_some());
    if x_rule_notes {
        top_gutter = max(top_gutter, line_h + 6.0);
    }

    let x_title_label = spec
        .x
        .label
        .as_deref()
        .map(|l| ms.text(l, width - 2.0 * PAD));
    let x_title_h = x_title_label.as_ref().map_or(0.0, |l| l.height + 4.0);

    if horizontal {
        // Categories down the left; value axis along the bottom.
        let wrap = min(180.0, width * 0.32);
        let cats: Vec<LabelLayout> = r.y.categories.iter().map(|c| ms.text(c, wrap)).collect();
        let cat_w = cats.iter().fold(0.0f64, |a, l| max(a, l.width));
        let cat_h = cats.iter().fold(0.0f64, |a, l| max(a, l.height));
        let groups = r
            .marks
            .iter()
            .filter(|m| m.kind == MarkType::Bar)
            .map(|m| m.series.len().max(1))
            .max()
            .unwrap_or(1);
        let step = max(max(MIN_ROW, cat_h + 10.0), groups as f64 * 12.0 / BAR_SHARE);
        let n = r.y.categories.len().max(1);
        let plot_y = top + max(top_gutter, small / 2.0);
        let plot_h = step * n as f64;
        let plot_x = PAD + cat_w + 10.0 + left_gutter;
        let plot_w = max(width - plot_x - max(right_gutter, 12.0) - PAD, 40.0);
        let bottom = TICK_GAP + line_h + x_title_h + PAD;
        height = plot_y + plot_h + bottom;
        plot = Rect {
            x: plot_x,
            y: plot_y,
            w: plot_w,
            h: plot_h,
        };
        let band = Band {
            start: plot.y,
            step,
            width: min(step * BAR_SHARE, 22.0 * groups as f64),
        };
        for (i, label) in cats.into_iter().enumerate() {
            y_ticks.push(AxisTick {
                pos: band.centre(i),
                value: r
                    .y
                    .categories
                    .get(i)
                    .map_or_else(String::new, |s| String::from(s.as_ref())),
                text: TextGeom {
                    x: plot.x - left_gutter - 10.0 - label.width / 2.0,
                    y: band.centre(i),
                    label,
                },
            });
        }
        y_band = Some(band);
        let c = continuous(
            spec.x.zero,
            r.x.scale,
            r.x_domain,
            &xs,
            x_zero,
            plot.w,
            90.0,
        );
        for (v, s) in continuous_ticks(&c, r.x.scale, &spec.x, plot.w) {
            let pos = scale::map(v, c.d0, c.d1, plot.x, plot.x + plot.w);
            let label = ms.text(&s, 200.0);
            let half = label.width / 2.0;
            x_ticks.push(AxisTick {
                pos,
                value: s,
                text: TextGeom {
                    x: clamp(pos, half, width - half),
                    y: plot.y + plot.h + TICK_GAP + label.height / 2.0,
                    label,
                },
            });
        }
        x_cont = Some(c);
    } else {
        let total = spec
            .height
            .unwrap_or_else(|| top + clamp(round(width * 0.45), 180.0, 380.0) + 3.0 * line_h);
        let plot_y = top + max(top_gutter, small / 2.0);
        // y ticks depend on the plot height, which depends on the x labels below it, whose
        // wrap width depends on the left margin set by the y ticks: two passes settle it.
        let mut bottom = TICK_GAP + line_h + x_title_h + PAD + bottom_gutter;
        let mut y_labels: Vec<(f64, LabelLayout, String)> = Vec::new();
        let mut c_y = None;
        let mut plot_x = PAD;
        let mut plot_w = width - 2.0 * PAD;
        let mut x_labels: Vec<LabelLayout> = Vec::new();
        for _pass in 0..2 {
            let plot_h = max(total - plot_y - bottom, 60.0);
            let c = continuous(
                spec.y.zero,
                r.y.scale,
                r.y_domain,
                &ys,
                y_zero,
                plot_h,
                56.0,
            );
            y_labels = continuous_ticks(&c, r.y.scale, &spec.y, plot_h)
                .into_iter()
                .map(|(v, s)| (v, ms.text(&s, 200.0), s))
                .collect();
            let tick_w = y_labels.iter().fold(0.0f64, |a, l| max(a, l.1.width));
            plot_x = PAD + tick_w + 8.0;
            plot_w = max(width - plot_x - max(right_gutter, 12.0) - PAD, 40.0);
            if r.x.scale == ScaleType::Band {
                let n = r.x.categories.len().max(1);
                let wrap = max(plot_w / n as f64 - 4.0, 24.0);
                x_labels = r.x.categories.iter().map(|s| ms.text(s, wrap)).collect();
                let h = x_labels.iter().fold(line_h, |a, l| max(a, l.height));
                bottom = TICK_GAP + h + x_title_h + PAD + bottom_gutter;
            }
            c_y = Some(c);
        }
        let c = c_y.unwrap_or(Continuous {
            d0: 0.0,
            d1: 1.0,
            step: None,
        });
        let plot_h = max(total - plot_y - bottom, 60.0);
        height = plot_y + plot_h + bottom;
        plot = Rect {
            x: plot_x,
            y: plot_y,
            w: plot_w,
            h: plot_h,
        };
        for (v, label, s) in y_labels {
            let pos = scale::map(v, c.d0, c.d1, plot.y + plot.h, plot.y);
            y_ticks.push(AxisTick {
                pos,
                value: s,
                text: TextGeom {
                    x: plot.x - 8.0 - label.width / 2.0,
                    y: pos,
                    label,
                },
            });
        }
        y_cont = Some(c);
        let label_top = plot.y + plot.h + bottom_gutter + TICK_GAP;
        if r.x.scale == ScaleType::Band {
            let n = r.x.categories.len().max(1);
            let groups = r
                .marks
                .iter()
                .filter(|m| m.kind == MarkType::Bar)
                .map(|m| m.series.len().max(1))
                .max()
                .unwrap_or(1);
            let step = plot.w / n as f64;
            let band = Band {
                start: plot.x,
                step,
                width: min(step * BAR_SHARE, MAX_BAR * groups as f64),
            };
            for (i, label) in x_labels.into_iter().enumerate() {
                x_ticks.push(AxisTick {
                    pos: band.centre(i),
                    value: r
                        .x
                        .categories
                        .get(i)
                        .map_or_else(String::new, |s| String::from(s.as_ref())),
                    text: TextGeom {
                        x: band.centre(i),
                        y: label_top + label.height / 2.0,
                        label,
                    },
                });
            }
            x_band = Some(band);
        } else {
            let c = continuous(
                spec.x.zero,
                r.x.scale,
                r.x_domain,
                &xs,
                x_zero,
                plot.w,
                90.0,
            );
            for (v, s) in continuous_ticks(&c, r.x.scale, &spec.x, plot.w) {
                let pos = scale::map(v, c.d0, c.d1, plot.x, plot.x + plot.w);
                let label = ms.text(&s, 200.0);
                let half = label.width / 2.0;
                x_ticks.push(AxisTick {
                    pos,
                    value: s,
                    text: TextGeom {
                        x: clamp(pos, half, width - half),
                        y: label_top + label.height / 2.0,
                        label,
                    },
                });
            }
            x_cont = Some(c);
        }
    }

    let x_title = x_title_label.map(|label| TextGeom {
        x: plot.x + plot.w - label.width / 2.0,
        y: height - PAD - label.height / 2.0,
        label,
    });

    let px = |v: &Val| -> f64 {
        match (v.pos, &x_band, &x_cont) {
            (Pos::Cat(i), Some(b), _) => b.centre(i),
            (Pos::Num(n), _, Some(c)) => scale::map(n, c.d0, c.d1, plot.x, plot.x + plot.w),
            _ => plot.x,
        }
    };
    let py = |v: &Val| -> f64 {
        match (v.pos, &y_band, &y_cont) {
            (Pos::Cat(i), Some(b), _) => b.centre(i),
            (Pos::Num(n), _, Some(c)) => scale::map(n, c.d0, c.d1, plot.y + plot.h, plot.y),
            _ => plot.y + plot.h,
        }
    };
    let zero_x = x_cont
        .filter(|c| r.x.scale == ScaleType::Linear && c.d0 <= 0.0 && c.d1 >= 0.0)
        .map(|c| scale::map(0.0, c.d0, c.d1, plot.x, plot.x + plot.w));
    let zero_y = y_cont
        .filter(|c| r.y.scale == ScaleType::Linear && c.d0 <= 0.0 && c.d1 >= 0.0)
        .map(|c| scale::map(0.0, c.d0, c.d1, plot.y + plot.h, plot.y));
    // The zero line belongs to the value axis of the chart's bars or areas, else to the
    // linear y axis, else the linear x axis.
    let zero = if horizontal || (x_zero && !y_zero) {
        zero_x.map(|x| (x, plot.y, x, plot.y + plot.h))
    } else {
        zero_y
            .map(|y| (plot.x, y, plot.x + plot.w, y))
            .or_else(|| zero_x.map(|x| (x, plot.y, x, plot.y + plot.h)))
    };

    let width_px = width;
    let mut note_boxes: Vec<(f64, f64)> = Vec::new();
    // On a chart with categories down the side, a labelled dot at the low end of its
    // row's dots and intervals labels to the left, so a dumbbell's labels sit outside.
    let mut row_extent: Vec<(f64, f64)> =
        alloc::vec![(f64::INFINITY, f64::NEG_INFINITY); r.y.categories.len()];
    if horizontal {
        for m in r
            .marks
            .iter()
            .filter(|m| matches!(m.kind, MarkType::Dot | MarkType::Interval))
        {
            for it in &m.items {
                let Some(Val {
                    pos: Pos::Cat(c), ..
                }) = &it.y
                else {
                    continue;
                };
                for v in [&it.x, &it.x2].into_iter().flatten() {
                    if let Some(e) = row_extent.get_mut(*c) {
                        e.0 = min(e.0, v.pos.num());
                        e.1 = max(e.1, v.pos.num());
                    }
                }
            }
        }
    }

    marks_geom = Vec::with_capacity(r.marks.len());
    for (index, m) in r.marks.iter().enumerate() {
        let mut g = MarkGeom {
            index,
            kind: m.kind,
            tone: m.tone,
            dash: m.dash,
            items: Vec::new(),
            notes: Vec::new(),
        };
        let name = |k: Option<usize>| k.and_then(|k| r.series.get(k)).cloned();
        let data_of = |it: &Item| {
            let mut d: Vec<(&'static str, Rc<str>)> = Vec::new();
            if let Some(v) = &it.x {
                d.push(("data-merlion-x", v.raw.clone()));
            }
            if let Some(v) = &it.y {
                d.push(("data-merlion-y", v.raw.clone()));
            }
            if let Some(v) = &it.x2 {
                d.push(("data-merlion-x2", v.raw.clone()));
            }
            if let Some(v) = &it.y2 {
                d.push(("data-merlion-y2", v.raw.clone()));
            }
            if let Some(s) = name(it.series) {
                d.push(("data-merlion-series", s));
            }
            d
        };
        let tip_of = |it: &Item| {
            let mut t = Vec::new();
            if matches!(m.kind, MarkType::Rule | MarkType::Span) {
                if let Some(note) = &m.text {
                    t.push(Rc::from(note.as_str()));
                    t.push(Rc::from(": "));
                }
            }
            if let Some(s) = name(it.series) {
                t.push(s);
                t.push(Rc::from(": "));
            }
            let parts: Vec<_> = [&it.x, &it.x2, &it.y, &it.y2]
                .into_iter()
                .flatten()
                .map(|v| v.raw.clone())
                .collect();
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    t.push(Rc::from(if i == 1 { ", " } else { " to " }));
                }
                t.push(p.clone());
            }
            t
        };
        match m.kind {
            MarkType::Bar => {
                let groups = m.series.len().max(1);
                for it in &m.items {
                    let (Some(xv), Some(yv)) = (&it.x, &it.y) else {
                        continue;
                    };
                    let slot = it
                        .series
                        .and_then(|s| m.series.iter().position(|&k| k == s))
                        .unwrap_or(0);
                    let (band, cat, value) = if m.horizontal {
                        (&y_band, yv, xv)
                    } else {
                        (&x_band, xv, yv)
                    };
                    let Some(band) = band else { continue };
                    let centre = if m.horizontal { py(cat) } else { px(cat) };
                    let sub = band.width / groups as f64;
                    let gap = if groups > 1 { FILL_GAP } else { 0.0 };
                    let a = centre - band.width / 2.0 + slot as f64 * sub + gap / 2.0;
                    let thick = max(sub - gap, 1.0);
                    let positive = value.pos.num() >= 0.0;
                    let (rect, end, label) = if m.horizontal {
                        let z = zero_x.unwrap_or(plot.x);
                        let e = px(value);
                        let rect = Rect {
                            x: min(z, e),
                            y: a,
                            w: max((e - z).abs(), 0.0),
                            h: thick,
                        };
                        let label = it.label.as_ref().map(|l| {
                            let label = ms.text(l, 160.0);
                            let x = if positive {
                                e + LABEL_GAP + label.width / 2.0
                            } else {
                                e - LABEL_GAP - label.width / 2.0
                            };
                            TextGeom {
                                x,
                                y: a + thick / 2.0,
                                label,
                            }
                        });
                        (rect, if positive { End::Right } else { End::Left }, label)
                    } else {
                        let z = zero_y.unwrap_or(plot.y + plot.h);
                        let e = py(value);
                        let rect = Rect {
                            x: a,
                            y: min(z, e),
                            w: thick,
                            h: max((e - z).abs(), 0.0),
                        };
                        let label = it.label.as_ref().map(|l| {
                            let label = ms.text(l, 160.0);
                            let y = if positive {
                                e - LABEL_GAP - label.height / 2.0
                            } else {
                                e + LABEL_GAP + label.height / 2.0
                            };
                            TextGeom {
                                x: a + thick / 2.0,
                                y,
                                label,
                            }
                        });
                        (rect, if positive { End::Top } else { End::Bottom }, label)
                    };
                    g.items.push(ItemGeom {
                        row: it.row,
                        series: it.series,
                        shape: Shape::Bar { rect, end },
                        label,
                        tip: tip_of(it),
                        data: data_of(it),
                    });
                }
            }
            MarkType::Line | MarkType::Area => {
                let mut keys: Vec<Option<usize>> = Vec::new();
                for it in &m.items {
                    if !keys.contains(&it.series) {
                        keys.push(it.series);
                    }
                }
                for key in keys {
                    let mut pts: Vec<&Item> =
                        m.items.iter().filter(|it| it.series == key).collect();
                    pts.sort_by(|a, b| {
                        let va = a.x.as_ref().map_or(0.0, |v| v.pos.num());
                        let vb = b.x.as_ref().map_or(0.0, |v| v.pos.num());
                        va.total_cmp(&vb)
                    });
                    let coords: Vec<(f64, f64, f64)> = pts
                        .iter()
                        .filter_map(|it| {
                            let (x, y) = (it.x.as_ref()?, it.y.as_ref()?);
                            let base = match (&it.y2, zero_y) {
                                (Some(y2), _) => py(y2),
                                (None, Some(z)) => z,
                                (None, None) => plot.y + plot.h,
                            };
                            Some((px(x), py(y), base))
                        })
                        .collect();
                    if coords.is_empty() {
                        continue;
                    }
                    let mut d = String::new();
                    for (i, (x, y, _)) in coords.iter().enumerate() {
                        d.push(if i == 0 { 'M' } else { 'L' });
                        crate::numfmt::push_num(&mut d, *x);
                        d.push(' ');
                        crate::numfmt::push_num(&mut d, *y);
                    }
                    let fill = m.kind == MarkType::Area;
                    if fill {
                        for (x, _, b) in coords.iter().rev() {
                            d.push('L');
                            crate::numfmt::push_num(&mut d, *x);
                            d.push(' ');
                            crate::numfmt::push_num(&mut d, *b);
                        }
                        d.push('Z');
                    }
                    let last = pts.last().copied();
                    let label = last.and_then(|it| {
                        let l = it.label.as_ref()?;
                        let (x, y, _) = *coords.last()?;
                        let label = ms.text(l, 160.0);
                        Some(TextGeom {
                            x: x + 8.0 + label.width / 2.0,
                            y,
                            label,
                        })
                    });
                    let mut data = Vec::new();
                    if let Some(s) = name(key) {
                        data.push(("data-merlion-series", s));
                    }
                    data.push((
                        "data-merlion-points",
                        Rc::from(alloc::format!("{}", coords.len())),
                    ));
                    let points = pts
                        .iter()
                        .zip(&coords)
                        .map(|(it, &(cx, cy, _))| PointGeom {
                            row: it.row,
                            cx,
                            cy,
                            tip: tip_of(it),
                            data: data_of(it),
                        })
                        .collect();
                    g.items.push(ItemGeom {
                        row: None,
                        series: key,
                        shape: Shape::Path { d, fill, points },
                        label,
                        tip: alloc::vec![name(key).unwrap_or_else(|| Rc::from(spec.title.as_str()))],
                        data,
                    });
                }
            }
            MarkType::Dot => {
                for it in &m.items {
                    let (Some(xv), Some(yv)) = (&it.x, &it.y) else {
                        continue;
                    };
                    let (cx, cy) = (px(xv), py(yv));
                    let low_end = match yv.pos {
                        Pos::Cat(c) => row_extent
                            .get(c)
                            .is_some_and(|e| e.0 < e.1 && xv.pos.num() <= e.0),
                        Pos::Num(_) => false,
                    };
                    let label = it.label.as_ref().map(|l| {
                        let label = ms.text(l, 160.0);
                        let off = DOT_R + LABEL_GAP + label.width / 2.0;
                        TextGeom {
                            x: if low_end { cx - off } else { cx + off },
                            y: cy,
                            label,
                        }
                    });
                    g.items.push(ItemGeom {
                        row: it.row,
                        series: it.series,
                        shape: Shape::Dot { cx, cy, r: DOT_R },
                        label,
                        tip: tip_of(it),
                        data: data_of(it),
                    });
                }
            }
            MarkType::Interval => {
                let band_w = if m.horizontal {
                    y_band.as_ref().map(|b| b.width)
                } else {
                    x_band.as_ref().map(|b| b.width)
                };
                let default = if m.tone == Tone::Neutral {
                    2.0
                } else {
                    min(band_w.unwrap_or(20.0) * 0.45, 12.0)
                };
                let width = m.thickness.unwrap_or(default);
                for it in &m.items {
                    let (Some(xv), Some(yv)) = (&it.x, &it.y) else {
                        continue;
                    };
                    let (x1, y1, x2, y2) = if m.horizontal {
                        let Some(x2v) = &it.x2 else { continue };
                        let y = py(yv);
                        (px(xv), y, px(x2v), y)
                    } else {
                        let Some(y2v) = &it.y2 else { continue };
                        let x = px(xv);
                        (x, py(yv), x, py(y2v))
                    };
                    let label = it.label.as_ref().map(|l| {
                        let label = ms.text(l, 160.0);
                        if m.horizontal {
                            TextGeom {
                                x: max(x1, x2) + width / 2.0 + LABEL_GAP + label.width / 2.0,
                                y: y1,
                                label,
                            }
                        } else {
                            TextGeom {
                                x: x1,
                                y: min(y1, y2) - LABEL_GAP - label.height / 2.0,
                                label,
                            }
                        }
                    });
                    g.items.push(ItemGeom {
                        row: it.row,
                        series: it.series,
                        shape: Shape::Segment {
                            x1,
                            y1,
                            x2,
                            y2,
                            width,
                        },
                        label,
                        tip: tip_of(it),
                        data: data_of(it),
                    });
                }
            }
            MarkType::Rule => {
                let width = m.thickness.unwrap_or(1.25);
                for it in &m.items {
                    let (shape, note) = if let Some(xv) = &it.x {
                        let x = px(xv);
                        let note = m.text.as_ref().map(|t| {
                            let label = ms.text(t, 200.0);
                            let w = label.width;
                            // Right of the line, else left of it, in the row above the
                            // plot; a third note at the same place drops inside the plot.
                            let right = (x + 4.0, x + 4.0 + w);
                            let left = (x - 4.0 - w, x - 4.0);
                            let free = |b: (f64, f64)| {
                                b.0 >= PAD
                                    && b.1 <= width_px - PAD
                                    && !note_boxes
                                        .iter()
                                        .any(|o: &(f64, f64)| b.0 < o.1 + 6.0 && o.0 < b.1 + 6.0)
                            };
                            let (b, inside) = if free(right) {
                                (right, false)
                            } else if free(left) {
                                (left, false)
                            } else {
                                (right, true)
                            };
                            if !inside {
                                note_boxes.push(b);
                            }
                            let y = if inside {
                                plot.y + 2.0 + label.height / 2.0
                            } else {
                                plot.y - 4.0 - label.height / 2.0
                            };
                            TextGeom {
                                x: (b.0 + b.1) / 2.0,
                                y,
                                label,
                            }
                        });
                        (
                            Shape::Segment {
                                x1: x,
                                y1: plot.y,
                                x2: x,
                                y2: plot.y + plot.h,
                                width,
                            },
                            note,
                        )
                    } else if let Some(yv) = &it.y {
                        let y = py(yv);
                        let note = m.text.as_ref().map(|t| {
                            let label = ms.text(t, 200.0);
                            TextGeom {
                                x: plot.x + plot.w - label.width / 2.0,
                                y: y - 3.0 - label.height / 2.0,
                                label,
                            }
                        });
                        (
                            Shape::Segment {
                                x1: plot.x,
                                y1: y,
                                x2: plot.x + plot.w,
                                y2: y,
                                width,
                            },
                            note,
                        )
                    } else {
                        continue;
                    };
                    if let Some(n) = note {
                        g.notes.push(n);
                    }
                    g.items.push(ItemGeom {
                        row: it.row,
                        series: None,
                        shape,
                        label: None,
                        tip: tip_of(it),
                        data: data_of(it),
                    });
                }
            }
            MarkType::Span => {
                for it in &m.items {
                    let rect = match (&it.x, &it.x2, &it.y, &it.y2) {
                        (Some(a), Some(b), _, _) => {
                            let (a, b) = (px(a), px(b));
                            Rect {
                                x: min(a, b),
                                y: plot.y,
                                w: (b - a).abs(),
                                h: plot.h,
                            }
                        }
                        (_, _, Some(a), Some(b)) => {
                            let (a, b) = (py(a), py(b));
                            Rect {
                                x: plot.x,
                                y: min(a, b),
                                w: plot.w,
                                h: (b - a).abs(),
                            }
                        }
                        _ => continue,
                    };
                    if let Some(t) = &m.text {
                        let label = ms.text(t, max(rect.w - 8.0, 40.0));
                        g.notes.push(TextGeom {
                            x: rect.x + 4.0 + label.width / 2.0,
                            y: rect.y + 4.0 + label.height / 2.0,
                            label,
                        });
                    }
                    g.items.push(ItemGeom {
                        row: it.row,
                        series: None,
                        shape: Shape::Shade { rect },
                        label: None,
                        tip: tip_of(it),
                        data: data_of(it),
                    });
                }
            }
        }
        marks_geom.push(g);
    }

    Ok(ChartGeometry {
        width,
        height,
        font_size: base,
        small,
        plot,
        x_ticks,
        y_ticks,
        x_grid: r.x.scale != ScaleType::Band,
        y_grid: r.y.scale != ScaleType::Band,
        x_title,
        y_title,
        zero,
        legend,
        marks: marks_geom,
    })
}

/// Whether the item's value end is at or above zero (bars label outward from zero).
fn value_sign(m: &MarkData, it: &Item) -> bool {
    let v = if m.horizontal { &it.x } else { &it.y };
    v.as_ref().is_none_or(|v| v.pos.num() >= 0.0)
}
