//! Node sizes from label size and shape, and the shape outlines used for edge ports.
//!
//! Padding comes from the shape, never from the text measure
//! (specs/text-measurement.md#measuring). Every shape is sized so that the label box,
//! centred on the node, lies inside the outline. The outline of each shape, with the
//! node centre at the origin, `a = w/2` and `b = h/2`, is the contract with the draw
//! stage:
//!
//! | Shape | Outline |
//! |---|---|
//! | `Rect`, `Round`, `Subroutine` | The `w × h` rectangle (corner rounding and the subroutine's inner bars lie inside it) |
//! | `Stadium` | Rectangle with semicircular ends of radius `b` |
//! | `Cylinder` | Rectangle whose top and bottom are elliptical arcs with radius `a` × [`cylinder_ry`]: the top cap's upper half and the bottom cap's lower half |
//! | `Circle`, `DoubleCircle` | Circle of diameter `w = h` (the outer ring for `DoubleCircle`, [`DOUBLE_CIRCLE_GAP`] outside the inner one) |
//! | `Rhombus` | Diamond with vertices at `(±a, 0)`, `(0, ±b)` |
//! | `Hexagon` | Points at `(±a, 0)`, flat top and bottom between `±(a − h/4)` |
//! | `Parallelogram` (`[/t/]`) | Top edge `[−a + s, a]`, bottom `[−a, a − s]`, `s = h ·` [`SLANT`] |
//! | `ParallelogramAlt` (`[\t\]`) | Top `[−a, a − s]`, bottom `[−a + s, a]` |
//! | `Trapezoid` (`[/t\]`) | Top `[−a + s, a − s]`, bottom `[−a, a]` |
//! | `TrapezoidAlt` (`[\t/]`) | Top `[−a, a]`, bottom `[−a + s, a − s]` |
//! | `Asymmetric` (`>t]`) | Rectangle whose left side has a notch reaching `h/4` inwards at mid-height |
//! | `SmallCircle`, `FilledCircle`, `FramedCircle`, `CrossedCircle` | Circle of diameter `w = h` ([`fixed_size`]) |
//! | `Fork`, `Hourglass`, `Bolt` | The `w × h` rectangle ([`fixed_size`]); the hourglass and bolt are drawn inside it |
//! | `Document`, `LinedDocument`, `TaggedDocument` | Rectangle whose bottom is the wave [`wave_bottom`] around `b − WAVE` |
//! | `StackedDocument` | Union of a document in `[−a, a − 2S] × [−b + 2S, b]` and two rectangles offset by `S = ` [`STACK`] up and right |
//! | `Delay` | Rectangle whose right end is a half ellipse of radii `min(a, b)` × `b` |
//! | `HorizontalCylinder` | Rectangle whose left and right ends are half ellipses of radii [`CYLINDER_RY`] × `b` |
//! | `LinedCylinder` | As `Cylinder` |
//! | `CurvedTrapezoid` | Left side pointed at `(−a, 0)`, `b/2` deep; right end a half ellipse of radii `b` × `b` |
//! | `Triangle` / `FlippedTriangle` | Apex at `(0, −b)` / `(0, b)`, base `[−a, a]` on the opposite side |
//! | `NotchedPentagon` | Rectangle with its top corners cut `h/4` deep |
//! | `SlopedRect` | Rectangle whose top edge rises [`SLOPE`] from left to right |
//! | `StackedRect` | Union of `[−a, a − 2S] × [−b + 2S, b]` and the same box moved `S` and `2S` up and right |
//! | `BowTieRect` | Left end a convex, right end a concave half ellipse of radii [`BOW`] × `b` |
//! | `Flag` | Rectangle whose top ([`wave_top`]) and bottom ([`wave_bottom`]) are waves around `−b + WAVE` and `b − WAVE` |
//! | `NotchedRect` | Rectangle with its top-left corner cut [`NOTCH`] deep |
//! | `DividedRect`, `WindowPane`, `TaggedRect`, `LinedRect`, `TextBlock`, `BraceLeft`, `BraceRight`, `Braces`, `DataStore` | The `w × h` rectangle (bars, folds and braces lie inside it; `TextBlock` and `DataStore` draw no outline around it) |

use crate::math::{abs, hypot, max, min, sqrt};
use crate::model::{Flowchart, FontStyle, FontWeight, Node, Shape, Style};
use crate::text::{TextStyle, Weight};

/// Horizontal padding between the label box and a rectangular outline, per side.
pub const PAD_X: f64 = 16.0;
/// Vertical padding between the label box and a rectangular outline, per side.
pub const PAD_Y: f64 = 10.0;
/// Padding kept around the label inside round and pointed shapes, per side.
pub const PAD_INNER: f64 = 8.0;
/// Radius of a horizontal cylinder's elliptical ends.
pub const CYLINDER_RY: f64 = 6.0;

/// Vertical radius of the elliptical caps of a `w × h` cylinder: flatter for narrow
/// nodes, at most 10 px, and at most `h/4`. The drawing, the outline and the node size
/// all use it, so the front rim of the top cap (2·ry below the top) clears the label.
pub fn cylinder_ry(w: f64, h: f64) -> f64 {
    min(min(4.0 + 0.05 * max(w, 0.0), 10.0), max(h, 0.0) / 4.0)
}
/// Gap between the inner and outer ring of a double circle.
pub const DOUBLE_CIRCLE_GAP: f64 = 5.0;
/// Horizontal slant of parallelograms and trapezoids as a fraction of the height.
pub const SLANT: f64 = 1.0 / 3.0;
/// Subroutine inner bars sit this far inside the left and right sides.
pub const SUBROUTINE_INSET: f64 = 8.0;
/// Smallest node width and height.
pub const MIN_SIZE: f64 = 20.0;
/// Amplitude of the wavy edges of documents and flags.
pub const WAVE: f64 = 5.0;
/// Offset of each layer behind a stacked document or rectangle.
pub const STACK: f64 = 5.0;
/// Height of the band above the divider of `DividedRect`, and of the pane bars of
/// `WindowPane` (kept free below and right of the label as well, so it stays centred).
pub const BAND: f64 = 10.0;
/// Depth of the top-left notch of `NotchedRect`.
pub const NOTCH: f64 = 12.0;
/// Rise of the top edge of `SlopedRect`.
pub const SLOPE: f64 = 10.0;
/// Horizontal depth of the ends of `BowTieRect`.
pub const BOW: f64 = 8.0;
/// Size of the folded corner of `TaggedRect` and `TaggedDocument`.
pub const TAG: f64 = 10.0;
/// Width of a curly brace.
pub const BRACE: f64 = 10.0;

/// Control-point offset of the cubic wave: `2·√3` times the amplitude, so the curve
/// deviates exactly `amplitude` from its base line.
pub const WAVE_K: f64 = 3.464_101_615_137_754_6;

/// `y` of the wave through `(x1, y0)` and `(x0, y0)` (`x0 < x1`) at `x`: the cubic from
/// `(x1, y0)` with controls `(xm, y0 − K)`, `(xm, y0 + K)` to `(x0, y0)`, `xm` the
/// midpoint and `K = WAVE_K · amplitude`. It rises above `y0` on the right half and
/// dips below it on the left half, by at most `amplitude`.
pub fn wave_y(x: f64, x0: f64, x1: f64, y0: f64, amplitude: f64) -> f64 {
    if x1 <= x0 {
        return y0;
    }
    let xm = (x0 + x1) / 2.0;
    let k = WAVE_K * amplitude;
    let bx = |t: f64| {
        let u = 1.0 - t;
        x1 * u * u * u + xm * 3.0 * t * u * (u + t) + x0 * t * t * t
    };
    // x decreases from x1 to x0 as t runs from 0 to 1.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..50 {
        let mid = (lo + hi) / 2.0;
        if bx(mid) > x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let t = (lo + hi) / 2.0;
    y0 + 3.0 * k * t * (1.0 - t) * (2.0 * t - 1.0)
}

/// `x` where the wave of [`wave_y`] peaks (rises furthest above `y0`); it dips
/// furthest at `x0 + x1 − peak`.
fn wave_peak_x(x0: f64, x1: f64) -> f64 {
    // t = 1/2 − 1/(2√3) on the cubic.
    const T: f64 = 0.211_324_865_405_187_1;
    let (u, xm) = (1.0 - T, (x0 + x1) / 2.0);
    x1 * u * u * u + xm * 3.0 * T * u * (u + T) + x0 * T * T * T
}

/// Bottom edge of a wavy outline: the wave, held at its peak from the peak to the
/// right end, so every ray from the centre crosses the outline once (ports and
/// clipping assume star-shaped outlines).
pub fn wave_bottom(x: f64, x0: f64, x1: f64, y0: f64, amplitude: f64) -> f64 {
    if x >= wave_peak_x(x0, x1) {
        y0 - amplitude
    } else {
        wave_y(x, x0, x1, y0, amplitude)
    }
}

/// Top edge of a wavy outline (the same wave around `y0`), held at its dip from the
/// left end to the dip.
pub fn wave_top(x: f64, x0: f64, x1: f64, y0: f64, amplitude: f64) -> f64 {
    if x <= x0 + x1 - wave_peak_x(x0, x1) {
        y0 + amplitude
    } else {
        wave_y(x, x0, x1, y0, amplitude)
    }
}

/// Vertical offset of the label centre from the node centre: triangles hold their label
/// in the wide half, and cylinders below the top cap's front rim, which reaches 2·ry
/// down where the bottom cap rises only ry.
pub fn label_offset(shape: Shape, w: f64, h: f64, lh: f64) -> f64 {
    let d = max(h / 2.0 - lh / 2.0 - PAD_INNER, 0.0);
    match shape {
        Shape::Triangle => d,
        Shape::FlippedTriangle => -d,
        Shape::Cylinder => cylinder_ry(w, h) / 2.0,
        Shape::LinedCylinder => (cylinder_ry(w, h) + min(STACK, max(h, 0.0) / 4.0)) / 2.0,
        _ => 0.0,
    }
}

/// Outer size of the label-less symbol shapes ([`Shape::draws_label`]), which do not
/// grow with the label.
pub fn fixed_size(shape: Shape) -> Option<(f64, f64)> {
    match shape {
        Shape::SmallCircle | Shape::FilledCircle => Some((14.0, 14.0)),
        Shape::FramedCircle => Some((20.0, 20.0)),
        Shape::CrossedCircle | Shape::Hourglass => Some((30.0, 30.0)),
        Shape::Fork => Some((70.0, 10.0)),
        Shape::Bolt => Some((24.0, 36.0)),
        _ => None,
    }
}

/// Outer size `(w, h)` of a node whose label box measures `lw × lh`.
pub fn node_size(shape: Shape, lw: f64, lh: f64) -> (f64, f64) {
    if let Some(size) = fixed_size(shape) {
        return size;
    }
    let lw = max(lw, 0.0);
    let lh = max(lh, 0.0);
    let tw = lw + 2.0 * PAD_X;
    let th = lh + 2.0 * PAD_Y;
    let (w, h) = match shape {
        Shape::Rect | Shape::Round => (tw, th),
        Shape::Subroutine => (tw + 2.0 * SUBROUTINE_INSET, th),
        // Straight part holds the label plus inner padding; the ends are semicircles.
        Shape::Stadium => (lw + 2.0 * PAD_INNER + th, th),
        // The top cap occupies 2·ry below the top and the bottom arc ry above the
        // bottom; 2·ry on each side keeps the label centred and clear of both.
        // 2·ry above the label (the front rim) and ry below it; the label sits ry/2
        // below the centre ([`label_offset`]).
        Shape::Cylinder => (tw, th + 3.0 * cylinder_ry(tw, f64::MAX)),
        // Label box inscribed in the circle: diameter = diagonal of the padded box.
        Shape::Circle => {
            let d = hypot(lw + 2.0 * PAD_INNER, lh + 2.0 * PAD_INNER);
            (d, d)
        }
        Shape::DoubleCircle => {
            let d = hypot(lw + 2.0 * PAD_INNER, lh + 2.0 * PAD_INNER) + 2.0 * DOUBLE_CIRCLE_GAP;
            (d, d)
        }
        // Padded box (p, q) inside the diamond: its corner (p/2, q/2) lies on the edge
        // x/(W/2) + y/(H/2) = 1 when p/W + q/H = 1. W = p + 2q, H = q + p/2 satisfies it
        // and keeps the diamond twice as wide as tall, flatter than a square for long labels.
        Shape::Rhombus => {
            let p = lw + 2.0 * PAD_INNER;
            let q = lh + 2.0 * PAD_INNER;
            (p + 2.0 * q, q + p / 2.0)
        }
        // Sides slant inwards by h/4 at the top and bottom; the full-height box needs
        // W − 2·(h/4) of width, measured at the top and bottom.
        Shape::Hexagon => (lw + 2.0 * PAD_INNER + th / 2.0, th),
        // A full-height box fits between the slants: W − 2s with s = h·SLANT.
        Shape::Parallelogram | Shape::ParallelogramAlt | Shape::Trapezoid | Shape::TrapezoidAlt => {
            (lw + 2.0 * PAD_INNER + 2.0 * th * SLANT, th)
        }
        // The notch reaches h/4 into the left side.
        Shape::Asymmetric => (tw + th / 4.0, th),
        // The wave dips to the bottom and rises 2·WAVE above it; PAD_Y ≥ WAVE keeps the
        // label clear of it.
        Shape::Document | Shape::TaggedDocument => (tw, th + 2.0 * WAVE),
        Shape::LinedDocument => (tw + 2.0 * SUBROUTINE_INSET, th + 2.0 * WAVE),
        Shape::Flag => (tw, th + 2.0 * WAVE),
        // The front layer holds the label box; the two layers behind add 2·STACK up and
        // right, kept on both sides so the label stays centred.
        Shape::StackedDocument => (tw + 4.0 * STACK, th + 2.0 * WAVE + 4.0 * STACK),
        Shape::StackedRect => (tw + 4.0 * STACK, th + 4.0 * STACK),
        Shape::Delay | Shape::CurvedTrapezoid => (lw + 2.0 * PAD_INNER + th, th),
        // The inner rim reaches 2·ry in from the right end.
        Shape::HorizontalCylinder => (tw + 4.0 * CYLINDER_RY, th),
        Shape::LinedCylinder => (tw, th + 3.0 * cylinder_ry(tw, f64::MAX) + STACK),
        Shape::DividedRect => (tw, th + 2.0 * BAND),
        Shape::WindowPane => (tw + 2.0 * BAND, th + 2.0 * BAND),
        Shape::SlopedRect => (tw, th + SLOPE),
        Shape::BowTieRect => (tw + 2.0 * BOW, th),
        Shape::NotchedPentagon => (lw + 2.0 * PAD_INNER + th / 2.0, th),
        // The label sits in the wide half: the padded box (p, q) fits a triangle of
        // width 2p and height 2q with its label centre q/2 from the base.
        Shape::Triangle | Shape::FlippedTriangle => {
            let p = lw + 2.0 * PAD_INNER;
            let q = lh + 2.0 * PAD_INNER;
            (2.0 * p, 2.0 * q)
        }
        Shape::TaggedRect
        | Shape::LinedRect
        | Shape::NotchedRect
        | Shape::TextBlock
        | Shape::BraceLeft
        | Shape::BraceRight
        | Shape::Braces
        | Shape::DataStore => match shape {
            Shape::LinedRect => (tw + 2.0 * SUBROUTINE_INSET, th),
            _ => (tw, th),
        },
        // Fixed sizes, returned above.
        Shape::SmallCircle
        | Shape::FilledCircle
        | Shape::FramedCircle
        | Shape::CrossedCircle
        | Shape::Fork
        | Shape::Hourglass
        | Shape::Bolt => (tw, th),
    };
    (max(w, MIN_SIZE), max(h, MIN_SIZE))
}

/// Whether `(x, y)`, relative to the centre, lies inside or on the outline.
pub fn inside(shape: Shape, w: f64, h: f64, x: f64, y: f64) -> bool {
    let a = w / 2.0;
    let b = h / 2.0;
    if !(abs(x) <= a && abs(y) <= b) {
        return false;
    }
    let s = h * SLANT;
    // Tiny tolerance so points computed on the outline count as inside.
    const EPS: f64 = 1e-9;
    match shape {
        Shape::Rect | Shape::Round | Shape::Subroutine => true,
        Shape::Fork | Shape::Hourglass | Shape::Bolt => true,
        Shape::DividedRect
        | Shape::WindowPane
        | Shape::TaggedRect
        | Shape::LinedRect
        | Shape::TextBlock
        | Shape::BraceLeft
        | Shape::BraceRight
        | Shape::Braces
        | Shape::DataStore => true,
        Shape::Document | Shape::LinedDocument | Shape::TaggedDocument => {
            y <= wave_bottom(x, -a, a, b - WAVE, WAVE) + EPS
        }
        Shape::Flag => {
            y <= wave_bottom(x, -a, a, b - WAVE, WAVE) + EPS
                && y >= wave_top(x, -a, a, -b + WAVE, WAVE) - EPS
        }
        Shape::StackedDocument => {
            let front = x <= a - 2.0 * STACK + EPS
                && y >= -b + 2.0 * STACK - EPS
                && y <= wave_bottom(x, -a, a - 2.0 * STACK, b - WAVE, WAVE) + EPS;
            let base = b - WAVE;
            front
                || (1..=2).any(|k| {
                    let o = k as f64 * STACK;
                    x >= -a + o - EPS
                        && x <= a - 2.0 * STACK + o + EPS
                        && y >= -b + 2.0 * STACK - o - EPS
                        && y <= base - o + EPS
                })
        }
        Shape::StackedRect => (0..=2).any(|k| {
            let o = k as f64 * STACK;
            x >= -a + o - EPS
                && x <= a - 2.0 * STACK + o + EPS
                && y >= -b + 2.0 * STACK - o - EPS
                && y <= b - o + EPS
        }),
        Shape::Delay => {
            let r = min(a, b);
            let cx = a - r;
            if x <= cx || r <= 0.0 || b <= 0.0 {
                return true;
            }
            let u = (x - cx) / r;
            u * u + (y / b) * (y / b) <= 1.0 + EPS
        }
        Shape::HorizontalCylinder => {
            let rx = min(CYLINDER_RY, a);
            if b <= 0.0 {
                return false;
            }
            let v = y / b;
            let k = sqrt(max(1.0 - v * v, 0.0));
            x >= -a + rx - rx * k - EPS && x <= a - rx + rx * k + EPS
        }
        Shape::CurvedTrapezoid => {
            if b <= 0.0 {
                return false;
            }
            let s = b / 2.0;
            let r = min(b, a);
            let cx = a - r;
            let right = x <= cx || {
                let u = (x - cx) / r;
                u * u + (y / b) * (y / b) <= 1.0 + EPS
            };
            right && x >= -a + s * abs(y) / b - EPS
        }
        Shape::Triangle | Shape::FlippedTriangle => {
            if b <= 0.0 {
                return false;
            }
            let from_apex = if shape == Shape::Triangle {
                y + b
            } else {
                b - y
            };
            abs(x) <= a * from_apex / (2.0 * b) + EPS
        }
        Shape::NotchedPentagon => {
            let c = h / 4.0;
            y >= -b + c || abs(x) <= a - c + (y + b) + EPS
        }
        Shape::SlopedRect => {
            if a <= 0.0 {
                return false;
            }
            y >= -b + min(SLOPE, h) * (a - x) / (2.0 * a) - EPS
        }
        Shape::BowTieRect => {
            let rx = min(BOW, a / 2.0);
            if b <= 0.0 {
                return false;
            }
            let v = y / b;
            let k = sqrt(max(1.0 - v * v, 0.0));
            x >= -a + rx - rx * k - EPS && x <= a - rx * k + EPS
        }
        Shape::NotchedRect => (x + a) + (y + b) >= min(NOTCH, min(a, b)) - EPS,
        Shape::LinedCylinder => inside(Shape::Cylinder, w, h, x, y),
        Shape::Stadium => {
            let r = b;
            let cx = max(a - r, 0.0);
            let dx = abs(x) - cx;
            dx <= 0.0 || dx * dx + y * y <= r * r + EPS
        }
        Shape::Cylinder => {
            let ry = cylinder_ry(w, h);
            if a <= 0.0 {
                return false;
            }
            let u = x / a;
            let k = sqrt(max(1.0 - u * u, 0.0));
            y <= b - ry + ry * k + EPS && y >= -b + ry - ry * k - EPS
        }
        Shape::Circle
        | Shape::DoubleCircle
        | Shape::SmallCircle
        | Shape::FilledCircle
        | Shape::FramedCircle
        | Shape::CrossedCircle => {
            if a <= 0.0 || b <= 0.0 {
                return false;
            }
            (x / a) * (x / a) + (y / b) * (y / b) <= 1.0 + EPS
        }
        Shape::Rhombus => {
            if a <= 0.0 || b <= 0.0 {
                return false;
            }
            abs(x) / a + abs(y) / b <= 1.0 + EPS
        }
        Shape::Hexagon => {
            let inset = h / 4.0;
            if b <= 0.0 {
                return false;
            }
            abs(x) <= a - inset * (1.0 - abs(y) / b) + EPS
        }
        Shape::Parallelogram | Shape::ParallelogramAlt | Shape::Trapezoid | Shape::TrapezoidAlt => {
            if b <= 0.0 {
                return false;
            }
            // t = 0 at the top, 1 at the bottom.
            let t = (y + b) / (2.0 * b);
            let (left, right) = match shape {
                Shape::Parallelogram => (-a + s * (1.0 - t), a - s * t),
                Shape::ParallelogramAlt => (-a + s * t, a - s * (1.0 - t)),
                Shape::Trapezoid => (-a + s * (1.0 - t), a - s * (1.0 - t)),
                _ => (-a + s * t, a - s * t),
            };
            x >= left - EPS && x <= right + EPS
        }
        Shape::Asymmetric => {
            let d = h / 4.0;
            if b <= 0.0 {
                return false;
            }
            x >= -a + d * (1.0 - abs(y) / b) - EPS
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    /// Outward normal and tangent (in screen coordinates, y down).
    fn frame(self) -> (f64, f64, f64, f64) {
        match self {
            Side::Top => (0.0, -1.0, 1.0, 0.0),
            Side::Bottom => (0.0, 1.0, 1.0, 0.0),
            Side::Left => (-1.0, 0.0, 0.0, 1.0),
            Side::Right => (1.0, 0.0, 0.0, 1.0),
        }
    }
}

/// Distance from the centre to the outline along `side`'s outward normal, at tangential
/// offset `t` (x for top/bottom, y for left/right). Bisection to 2^-50 of the half size,
/// which is exact to far below the 1/100 px output precision; 0 when `t` misses the shape.
pub fn side_offset(shape: Shape, w: f64, h: f64, side: Side, t: f64) -> f64 {
    let (nx, ny, tx, ty) = side.frame();
    let at = |d: f64| inside(shape, w, h, tx * t + nx * d, ty * t + ny * d);
    if !at(0.0) {
        return 0.0;
    }
    let mut lo = 0.0;
    let mut hi = if nx != 0.0 { w / 2.0 } else { h / 2.0 };
    if at(hi) {
        return hi;
    }
    for _ in 0..50 {
        let mid = (lo + hi) / 2.0;
        if at(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Largest tangential offset from the centre at which ports may sit on `side`: at most
/// 80% of the half extent, and only where the outline stays at least 60% as far out as
/// at the centre (so ports on a diamond or circle stay near its tip).
pub fn port_span(shape: Shape, w: f64, h: f64, side: Side) -> f64 {
    let half_t = match side {
        Side::Top | Side::Bottom => w / 2.0,
        Side::Left | Side::Right => h / 2.0,
    };
    let limit = 0.8 * half_t;
    let centre = side_offset(shape, w, h, side, 0.0);
    let ok = |t: f64| {
        side_offset(shape, w, h, side, t) >= 0.6 * centre
            && side_offset(shape, w, h, side, -t) >= 0.6 * centre
    };
    if ok(limit) {
        return limit;
    }
    let (mut lo, mut hi) = (0.0, limit);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The point of the outline on the ray from the centre in direction `(dx, dy)`,
/// relative to the centre.
pub fn boundary_toward(shape: Shape, w: f64, h: f64, dx: f64, dy: f64) -> (f64, f64) {
    let len = hypot(dx, dy);
    if len.is_nan() || len <= 0.0 || !len.is_finite() {
        return (0.0, h / 2.0);
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut lo = 0.0;
    let mut hi = hypot(w, h);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if inside(shape, w, h, ux * mid, uy * mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (ux * lo, uy * lo)
}

/// The node's own style over its classes' styles, in `class` order (a later class wins).
pub fn effective_style(chart: &Flowchart, node: &Node) -> Style {
    let mut style = Style::default();
    for class in &node.classes {
        for def in chart.class_defs.iter().filter(|d| &d.name == class) {
            style.merge(&def.style);
        }
    }
    style.merge(&node.style);
    style
}

/// Text style of a node label: SemiBold when the node style or one of its classes sets a
/// bold weight, italic likewise.
pub fn text_style(chart: &Flowchart, node: &Node, font_size: f64) -> TextStyle {
    style_to_text(&effective_style(chart, node), font_size)
}

pub fn style_to_text(style: &Style, font_size: f64) -> TextStyle {
    TextStyle {
        font_size,
        weight: match style.font_weight {
            Some(FontWeight::SemiBold) => Weight::SemiBold,
            _ => Weight::Regular,
        },
        italic: style.font_style == Some(FontStyle::Italic),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClassDef, FontWeight, Style};
    use crate::text::Weight;
    use alloc::string::String;
    use alloc::vec;

    const ALL: [Shape; 46] = Shape::ALL;

    #[test]
    fn rect_is_label_plus_padding() {
        let (w, h) = node_size(Shape::Rect, 100.0, 17.0);
        assert_eq!((w, h), (100.0 + 2.0 * PAD_X, 17.0 + 2.0 * PAD_Y));
    }

    #[test]
    fn label_box_fits_inside_every_shape() {
        for shape in ALL.into_iter().filter(|s| s.draws_label()) {
            for (lw, lh) in [(0.0, 17.0), (40.0, 17.0), (180.0, 51.0), (8.0, 34.0)] {
                let (w, h) = node_size(shape, lw, lh);
                assert!(w > 0.0 && h > 0.0, "{:?}", shape);
                let dy = label_offset(shape, w, h, lh);
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    assert!(
                        inside(shape, w, h, sx * lw / 2.0, dy + sy * lh / 2.0),
                        "{:?} {}x{} label corner outside {}x{}",
                        shape,
                        lw,
                        lh,
                        w,
                        h
                    );
                }
            }
        }
    }

    #[test]
    fn shapes_stay_inside_their_box() {
        for shape in ALL {
            let (w, h) = node_size(shape, 60.0, 17.0);
            assert!(!inside(shape, w, h, w / 2.0 + 0.1, 0.0));
            assert!(!inside(shape, w, h, 0.0, h / 2.0 + 0.1));
            assert!(inside(shape, w, h, 0.0, 0.0));
        }
    }

    #[test]
    fn side_offset_lands_on_the_boundary() {
        for shape in ALL {
            let (w, h) = node_size(shape, 60.0, 17.0);
            for side in [Side::Top, Side::Bottom, Side::Left, Side::Right] {
                let span = port_span(shape, w, h, side);
                assert!(span >= 0.0);
                for t in [-span, 0.0, span * 0.5, span] {
                    let d = side_offset(shape, w, h, side, t);
                    let (nx, ny, tx, ty) = match side {
                        Side::Top => (0.0, -1.0, 1.0, 0.0),
                        Side::Bottom => (0.0, 1.0, 1.0, 0.0),
                        Side::Left => (-1.0, 0.0, 0.0, 1.0),
                        Side::Right => (1.0, 0.0, 0.0, 1.0),
                    };
                    let px = tx * t + nx * d;
                    let py = ty * t + ny * d;
                    assert!(
                        inside(shape, w, h, px - nx * 0.01, py - ny * 0.01),
                        "{:?} {:?}",
                        shape,
                        side
                    );
                    assert!(
                        !inside(shape, w, h, px + nx * 0.01, py + ny * 0.01),
                        "{:?} {:?}",
                        shape,
                        side
                    );
                }
            }
        }
    }

    #[test]
    fn rect_offsets_are_exact() {
        assert!((side_offset(Shape::Rect, 100.0, 40.0, Side::Bottom, 10.0) - 20.0).abs() < 1e-6);
        assert!((side_offset(Shape::Rect, 100.0, 40.0, Side::Left, 5.0) - 50.0).abs() < 1e-6);
        assert!((side_offset(Shape::Rhombus, 100.0, 40.0, Side::Bottom, 25.0) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn boundary_toward_hits_the_outline() {
        for shape in ALL {
            let (w, h) = node_size(shape, 50.0, 17.0);
            for (dx, dy) in [(1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (-3.0, 1.0), (0.2, -1.0)] {
                let (x, y) = boundary_toward(shape, w, h, dx, dy);
                assert!(inside(shape, w, h, x * 0.999, y * 0.999), "{:?}", shape);
                assert!(
                    !inside(shape, w, h, x * 1.001 + dx * 0.001, y * 1.001 + dy * 0.001),
                    "{:?}",
                    shape
                );
            }
        }
    }

    #[test]
    fn classdef_and_style_font_weight_select_semibold() {
        let mut chart = Flowchart::default();
        chart.class_defs.push(ClassDef {
            name: String::from("hot"),
            style: Style {
                font_weight: Some(FontWeight::SemiBold),
                ..Style::default()
            },
        });
        let mut node = Node {
            id: String::from("a"),
            label: String::from("a"),
            shape: Shape::Rect,
            classes: vec![],
            style: Style::default(),
            link: None,
            subgraph: None,
            span: Default::default(),
        };
        assert_eq!(text_style(&chart, &node, 14.0).weight, Weight::Regular);
        node.classes.push(String::from("hot"));
        assert_eq!(text_style(&chart, &node, 14.0).weight, Weight::SemiBold);
        node.style.font_weight = Some(FontWeight::Regular);
        assert_eq!(text_style(&chart, &node, 14.0).weight, Weight::Regular);
        assert_eq!(text_style(&chart, &node, 16.0).font_size, 16.0);
    }
}
