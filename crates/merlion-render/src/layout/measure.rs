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
//! | `Cylinder` | Rectangle whose top and bottom are elliptical arcs with radius `a` × [`CYLINDER_RY`]: the top cap's upper half and the bottom cap's lower half |
//! | `Circle`, `DoubleCircle` | Circle of diameter `w = h` (the outer ring for `DoubleCircle`, [`DOUBLE_CIRCLE_GAP`] outside the inner one) |
//! | `Rhombus` | Diamond with vertices at `(±a, 0)`, `(0, ±b)` |
//! | `Hexagon` | Points at `(±a, 0)`, flat top and bottom between `±(a − h/4)` |
//! | `Parallelogram` (`[/t/]`) | Top edge `[−a + s, a]`, bottom `[−a, a − s]`, `s = h ·` [`SLANT`] |
//! | `ParallelogramAlt` (`[\t\]`) | Top `[−a, a − s]`, bottom `[−a + s, a]` |
//! | `Trapezoid` (`[/t\]`) | Top `[−a + s, a − s]`, bottom `[−a, a]` |
//! | `TrapezoidAlt` (`[\t/]`) | Top `[−a, a]`, bottom `[−a + s, a − s]` |
//! | `Asymmetric` (`>t]`) | Rectangle whose left side has a notch reaching `h/4` inwards at mid-height |

use crate::math::{abs, hypot, max, min, sqrt};
use crate::model::{Flowchart, FontStyle, FontWeight, Node, Shape, Style};
use crate::text::{TextStyle, Weight};

/// Horizontal padding between the label box and a rectangular outline, per side.
pub const PAD_X: f64 = 16.0;
/// Vertical padding between the label box and a rectangular outline, per side.
pub const PAD_Y: f64 = 10.0;
/// Padding kept around the label inside round and pointed shapes, per side.
pub const PAD_INNER: f64 = 8.0;
/// Vertical radius of a cylinder's elliptical caps.
pub const CYLINDER_RY: f64 = 6.0;
/// Gap between the inner and outer ring of a double circle.
pub const DOUBLE_CIRCLE_GAP: f64 = 5.0;
/// Horizontal slant of parallelograms and trapezoids as a fraction of the height.
pub const SLANT: f64 = 1.0 / 3.0;
/// Subroutine inner bars sit this far inside the left and right sides.
pub const SUBROUTINE_INSET: f64 = 8.0;
/// Smallest node width and height.
pub const MIN_SIZE: f64 = 20.0;

/// Outer size `(w, h)` of a node whose label box measures `lw × lh`.
pub fn node_size(shape: Shape, lw: f64, lh: f64) -> (f64, f64) {
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
        Shape::Cylinder => (tw, th + 4.0 * CYLINDER_RY),
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
        Shape::Stadium => {
            let r = b;
            let cx = max(a - r, 0.0);
            let dx = abs(x) - cx;
            dx <= 0.0 || dx * dx + y * y <= r * r + EPS
        }
        Shape::Cylinder => {
            let ry = min(CYLINDER_RY, b);
            if a <= 0.0 {
                return false;
            }
            let u = x / a;
            let k = sqrt(max(1.0 - u * u, 0.0));
            y <= b - ry + ry * k + EPS && y >= -b + ry - ry * k - EPS
        }
        Shape::Circle | Shape::DoubleCircle => {
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

    const ALL: [Shape; 14] = [
        Shape::Rect,
        Shape::Round,
        Shape::Stadium,
        Shape::Subroutine,
        Shape::Cylinder,
        Shape::Circle,
        Shape::DoubleCircle,
        Shape::Asymmetric,
        Shape::Rhombus,
        Shape::Hexagon,
        Shape::Parallelogram,
        Shape::ParallelogramAlt,
        Shape::Trapezoid,
        Shape::TrapezoidAlt,
    ];

    #[test]
    fn rect_is_label_plus_padding() {
        let (w, h) = node_size(Shape::Rect, 100.0, 17.0);
        assert_eq!((w, h), (100.0 + 2.0 * PAD_X, 17.0 + 2.0 * PAD_Y));
    }

    #[test]
    fn label_box_fits_inside_every_shape() {
        for shape in ALL {
            for (lw, lh) in [(0.0, 17.0), (40.0, 17.0), (180.0, 51.0), (8.0, 34.0)] {
                let (w, h) = node_size(shape, lw, lh);
                assert!(w > 0.0 && h > 0.0, "{:?}", shape);
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    assert!(
                        inside(shape, w, h, sx * lw / 2.0, sy * lh / 2.0),
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
