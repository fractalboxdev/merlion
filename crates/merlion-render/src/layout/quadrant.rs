//! Quadrant chart layout (specs/quadrant.md#layout): mermaid 12's quadrant builder with
//! every label measured from the font tables instead of assumed one font size tall.
//!
//! The chart keeps its configured width and height (500 × 500 px by default, narrowed
//! to the container width). The title, the x-axis labels and the y-axis labels take
//! bands at the edges; the four quadrants share what is left. Labels wrap to the room
//! they have, and their measured heights size the bands.

use super::LayoutError;
use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::quadrant::{
    AxisLabel, PointGeom, QuadrantGeom, QuadrantGeometry, Rect, TextGeom, TextSizes,
};
use crate::math::{clamp, max, min};
use crate::model::quadrant::{Quadrant, XAxisPosition, YAxisPosition};
use crate::options::RenderOptions;
use crate::text::{self, LabelLayout, TextStyle, Weight};
use alloc::vec::Vec;

/// mermaid 12's `quadrantChart` defaults.
const CHART: f64 = 500.0;
const TITLE_PADDING: f64 = 10.0;
const QUADRANT_PADDING: f64 = 5.0;
const AXIS_LABEL_PADDING: f64 = 5.0;
const TEXT_TOP_PADDING: f64 = 5.0;
const POINT_TEXT_PADDING: f64 = 5.0;
const POINT_RADIUS: f64 = 5.0;
const EXTERNAL_BORDER: f64 = 2.0;
const INTERNAL_BORDER: f64 = 1.0;
/// Narrowest chart a container width narrows the chart to.
const MIN_CHART: f64 = 200.0;
/// Least width and height of the four quadrants together.
const MIN_PLOT: f64 = 40.0;
/// Least width a label wraps to.
const MIN_WRAP: f64 = 24.0;

fn fail(what: &'static str) -> LayoutError {
    LayoutError::TooLarge { what }
}

fn burn(fuel: &mut Fuel, n: u64) -> Result<(), LayoutError> {
    fuel.burn(n).map_err(|_| fail("fuel"))
}

/// Measures `s` at `size` px, wrapping at `wrap`, widened by the font mode's tolerance.
fn measure(
    s: &str,
    size: f64,
    wrap: f64,
    weight: Weight,
    tolerance: f64,
    diags: &mut Diagnostics,
) -> LabelLayout {
    let style = TextStyle {
        font_size: size,
        weight,
        italic: false,
    };
    let mut l = text::layout_label(s, &style, max(wrap, MIN_WRAP) / tolerance, diags);
    l.width *= tolerance;
    l
}

fn at(label: LabelLayout, x: f64, y: f64) -> TextGeom {
    TextGeom { x, y, label }
}

/// A label centred on `x` with its top at `top`.
fn top_at(label: LabelLayout, x: f64, top: f64) -> TextGeom {
    let y = top + label.height / 2.0;
    at(label, x, y)
}

fn non_empty(s: &Option<alloc::string::String>) -> Option<&str> {
    s.as_deref().filter(|t| !t.trim().is_empty())
}

/// Lays out a quadrant chart. Work is one fuel unit per label byte and per point.
pub fn layout_quadrant(
    q: &Quadrant,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<QuadrantGeometry, LayoutError> {
    let spent = fuel.used();
    if q.points.len() > opts.limits.nodes {
        return Err(fail("quadrant points"));
    }
    let labels = q
        .points
        .iter()
        .map(|p| p.label.as_str())
        .chain(q.quadrants.iter().filter_map(|s| s.as_deref()))
        .chain(
            [&q.x_axis.low, &q.x_axis.high, &q.y_axis.low, &q.y_axis.high]
                .into_iter()
                .filter_map(|s| s.as_deref()),
        )
        .chain(q.meta.title.as_deref());
    for s in labels {
        if s.len() > opts.limits.label_bytes {
            return Err(fail("quadrant label bytes"));
        }
        burn(fuel, s.len() as u64 + 1)?;
    }
    burn(fuel, q.points.len() as u64 + q.class_defs.len() as u64)?;
    burn(fuel, q.class_lookup_units())?;

    let cfg = &q.meta.quadrant;
    let base = if opts.font_size.is_finite() && opts.font_size > 0.0 {
        clamp(opts.font_size, 1.0, 1_000.0)
    } else {
        14.0
    };
    let sizes = TextSizes {
        title: cfg.title_font_size.unwrap_or(base * 8.0 / 7.0),
        quadrant: cfg.quadrant_label_font_size.unwrap_or(base),
        x_axis: cfg.x_axis_label_font_size.unwrap_or(base),
        y_axis: cfg.y_axis_label_font_size.unwrap_or(base),
        point: cfg.point_label_font_size.unwrap_or(base * 6.0 / 7.0),
    };
    let tolerance = text::width_tolerance(opts.font);
    let title_pad = cfg.title_padding.unwrap_or(TITLE_PADDING);
    let quad_pad = cfg.quadrant_padding.unwrap_or(QUADRANT_PADDING);
    let x_pad = cfg.x_axis_label_padding.unwrap_or(AXIS_LABEL_PADDING);
    let y_pad = cfg.y_axis_label_padding.unwrap_or(AXIS_LABEL_PADDING);
    let text_top = cfg.quadrant_text_top_padding.unwrap_or(TEXT_TOP_PADDING);
    let point_pad = cfg.point_text_padding.unwrap_or(POINT_TEXT_PADDING);
    let wrap = if opts.wrap_width.is_finite() {
        clamp(opts.wrap_width, 1.0, 100_000.0)
    } else {
        200.0
    };

    let mut width = cfg.chart_width.unwrap_or(CHART);
    let mut height = cfg.chart_height.unwrap_or(CHART);
    let fit = if opts.target_width.is_finite() {
        max(opts.target_width, MIN_CHART)
    } else {
        f64::INFINITY
    };
    if width > fit {
        height *= fit / width;
        width = fit;
    }

    let has_points = !q.points.is_empty();
    // mermaid moves the x-axis labels to the bottom whenever the chart has points.
    let x_position = if has_points {
        XAxisPosition::Bottom
    } else {
        cfg.x_axis_position.unwrap_or_default()
    };
    let y_position = cfg.y_axis_position.unwrap_or_default();

    let title = non_empty(&q.meta.title).map(|t| {
        measure(
            t,
            sizes.title,
            width - 2.0 * quad_pad,
            Weight::SemiBold,
            tolerance,
            diags,
        )
    });
    let title_space = title.as_ref().map_or(0.0, |t| t.height + 2.0 * title_pad);

    // The y-axis band is first sized for one line, which bounds the x labels' wrap.
    let x_low = non_empty(&q.x_axis.low);
    let x_high = non_empty(&q.x_axis.high);
    let y_low = non_empty(&q.y_axis.low);
    let y_high = non_empty(&q.y_axis.high);
    let show_y = y_low.is_some() || y_high.is_some();
    let y_line = measure("Hg", sizes.y_axis, wrap, Weight::Regular, 1.0, diags).height;
    let y_space_guess = if show_y { 2.0 * y_pad + y_line } else { 0.0 };
    let plot_w_guess = max(width - 2.0 * quad_pad - y_space_guess, MIN_PLOT);
    let x_wrap = if x_high.is_some() {
        plot_w_guess / 2.0 - 2.0 * x_pad
    } else {
        plot_w_guess
    };
    let x_measured: Vec<(bool, LabelLayout)> = [(false, x_low), (true, x_high)]
        .into_iter()
        .filter_map(|(high, t)| t.map(|t| (high, t)))
        .map(|(high, t)| {
            let l = measure(t, sizes.x_axis, x_wrap, Weight::Regular, tolerance, diags);
            (high, l)
        })
        .collect();
    let x_h = x_measured.iter().fold(0.0, |h, (_, l)| max(h, l.height));
    let x_space = if x_measured.is_empty() {
        0.0
    } else {
        2.0 * x_pad + x_h
    };
    let plot_h_guess = max(height - 2.0 * quad_pad - x_space - title_space, MIN_PLOT);
    let y_wrap = if y_high.is_some() {
        plot_h_guess / 2.0 - 2.0 * y_pad
    } else {
        plot_h_guess
    };
    let y_measured: Vec<(bool, LabelLayout)> = [(false, y_low), (true, y_high)]
        .into_iter()
        .filter_map(|(high, t)| t.map(|t| (high, t)))
        .map(|(high, t)| {
            let l = measure(t, sizes.y_axis, y_wrap, Weight::Regular, tolerance, diags);
            (high, l)
        })
        .collect();
    let y_h = y_measured.iter().fold(0.0, |h, (_, l)| max(h, l.height));
    let y_space = if y_measured.is_empty() {
        0.0
    } else {
        2.0 * y_pad + y_h
    };

    let left = quad_pad
        + if y_position == YAxisPosition::Left {
            y_space
        } else {
            0.0
        };
    let top = quad_pad
        + title_space
        + if x_position == XAxisPosition::Top {
            x_space
        } else {
            0.0
        };
    let mut plot_w = width - 2.0 * quad_pad - y_space;
    if plot_w < MIN_PLOT {
        width += MIN_PLOT - plot_w;
        plot_w = MIN_PLOT;
    }
    let mut plot_h = height - 2.0 * quad_pad - x_space - title_space;
    if plot_h < MIN_PLOT {
        height += MIN_PLOT - plot_h;
        plot_h = MIN_PLOT;
    }
    let measured_quadrants: [Option<LabelLayout>; 4] = core::array::from_fn(|i| {
        non_empty(&q.quadrants[i]).map(|t| {
            measure(
                t,
                sizes.quadrant,
                plot_w / 2.0 - 2.0 * quad_pad,
                Weight::Regular,
                tolerance,
                diags,
            )
        })
    });
    let label_height = measured_quadrants
        .iter()
        .flatten()
        .fold(0.0, |h, l| max(h, l.height));
    let required_half = label_height
        + if has_points {
            text_top + quad_pad
        } else {
            2.0 * quad_pad
        };
    if plot_h < 2.0 * required_half {
        height += 2.0 * required_half - plot_h;
        plot_h = 2.0 * required_half;
    }
    let mut plot = Rect {
        x: left,
        y: top,
        w: plot_w,
        h: plot_h,
    };
    let (half_w, half_h) = (plot_w / 2.0, plot_h / 2.0);

    // Quadrant 1 top right, 2 top left, 3 bottom left, 4 bottom right.
    let corners = [
        (left + half_w, top),
        (left, top),
        (left, top + half_h),
        (left + half_w, top + half_h),
    ];
    let mut quadrants: [QuadrantGeom; 4] = Default::default();
    for (i, ((x, y), measured)) in corners.into_iter().zip(measured_quadrants).enumerate() {
        let rect = Rect {
            x,
            y,
            w: half_w,
            h: half_h,
        };
        let label = measured.map(|l| {
            let cx = x + half_w / 2.0;
            // Labels sit at the top of their quadrant when points might cover the
            // centre, and in the centre otherwise, as mermaid places them.
            if has_points {
                top_at(l, cx, y + text_top)
            } else {
                at(l, cx, y + half_h / 2.0)
            }
        });
        quadrants[i] = QuadrantGeom { rect, label };
    }

    // Two labels centre on their halves; a lone label starts at the low end.
    let two_x = x_high.is_some();
    let x_top = match x_position {
        XAxisPosition::Top => x_pad + title_space,
        XAxisPosition::Bottom => top + plot_h + quad_pad + x_pad,
    };
    let mut x_labels: Vec<AxisLabel> = x_measured
        .into_iter()
        .map(|(high, l)| {
            let cx = if two_x {
                left + if high { half_w * 1.5 } else { half_w / 2.0 }
            } else {
                left + l.width / 2.0
            };
            AxisLabel {
                high,
                text: top_at(l, cx, x_top),
            }
        })
        .collect();
    let two_y = y_high.is_some();
    let y_edge = match y_position {
        YAxisPosition::Left => y_pad,
        YAxisPosition::Right => left + plot_w + quad_pad + y_pad,
    };
    let mut y_labels: Vec<AxisLabel> = y_measured
        .into_iter()
        .map(|(high, l)| {
            // Drawn rotated −90° about its centre: its width runs up the axis and its
            // height across the band.
            let cy = if two_y {
                top + if high { half_h / 2.0 } else { half_h * 1.5 }
            } else {
                top + plot_h - l.width / 2.0
            };
            let cx = y_edge + l.height / 2.0;
            AxisLabel {
                high,
                text: at(l, cx, cy),
            }
        })
        .collect();

    let default_radius = cfg.point_radius.unwrap_or(POINT_RADIUS);
    let point_wrap = min(wrap, max(half_w, 48.0));
    let mut points = Vec::with_capacity(q.points.len());
    let mut bottom: f64 = height;
    for (index, p) in q.points.iter().enumerate() {
        let style = q.effective_style(p);
        let radius = style.radius.unwrap_or(default_radius);
        let stroke_width = style.stroke_width.unwrap_or(0.0);
        let cx = left + p.x * plot_w;
        let cy = top + (1.0 - p.y) * plot_h;
        let l = measure(
            &p.label,
            sizes.point,
            point_wrap,
            Weight::Regular,
            tolerance,
            diags,
        );
        // The label hangs below the point, clear of a circle larger than its padding,
        // and stays inside the chart's width.
        let label_top = cy + max(point_pad, radius + stroke_width / 2.0);
        let half = l.width / 2.0;
        let lx = if l.width + 2.0 * quad_pad >= width {
            width / 2.0
        } else {
            clamp(cx, quad_pad + half, width - quad_pad - half)
        };
        bottom = max(bottom, label_top + l.height + quad_pad);
        points.push(PointGeom {
            index,
            cx,
            cy,
            radius,
            stroke_width,
            label: top_at(l, lx, label_top),
        });
    }
    height = max(height, bottom);
    let mut title = title.map(|l| top_at(l, width / 2.0, title_pad));

    // Painted circles and border strokes contribute to the viewport. Translating the
    // whole drawing keeps every point at its source coordinate within the plot.
    let border_extent = cfg.external_border_width.unwrap_or(EXTERNAL_BORDER) / 2.0;
    let mut min_x = min(0.0, plot.x - border_extent);
    let mut min_y = min(0.0, plot.y - border_extent);
    let mut right = max(width, plot.x + plot.w + border_extent);
    let mut bottom = max(height, plot.y + plot.h + border_extent);
    for p in &points {
        let extent = p.radius + p.stroke_width / 2.0;
        min_x = min(min_x, p.cx - extent);
        min_y = min(min_y, p.cy - extent);
        right = max(right, p.cx + extent);
        bottom = max(bottom, p.cy + extent);
    }
    let (dx, dy) = (-min_x, -min_y);
    width = right + dx;
    height = bottom + dy;
    plot.x += dx;
    plot.y += dy;
    let shift = |t: &mut TextGeom| {
        t.x += dx;
        t.y += dy;
    };
    for q in &mut quadrants {
        q.rect.x += dx;
        q.rect.y += dy;
        if let Some(t) = &mut q.label {
            shift(t);
        }
    }
    for axis in x_labels.iter_mut().chain(y_labels.iter_mut()) {
        shift(&mut axis.text);
    }
    if let Some(t) = &mut title {
        shift(t);
    }
    for p in &mut points {
        p.cx += dx;
        p.cy += dy;
        shift(&mut p.label);
    }

    if !width.is_finite() || !height.is_finite() || width > 1_000_000.0 || height > 1_000_000.0 {
        return Err(fail("quadrant chart extent"));
    }
    Ok(QuadrantGeometry {
        width,
        height,
        font_size: base,
        sizes,
        plot,
        quadrants,
        title,
        x_labels,
        y_labels,
        points,
        external_border: cfg.external_border_width.unwrap_or(EXTERNAL_BORDER),
        internal_border: cfg.internal_border_width.unwrap_or(INTERNAL_BORDER),
        fuel_used: fuel.used().saturating_sub(spent),
    })
}
