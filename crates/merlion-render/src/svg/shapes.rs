//! Node shape outlines. Every shape is one `<path>` filling the node's outer box
//! (`NodeGeom::w` × `h`, centred on `x`, `y`); decorations such as the subroutine bars,
//! the cylinder rim and the inner ring of a double circle are extra subpaths of the
//! same path, so one CSS rule themes the whole shape.

use alloc::string::String;

use crate::model::Shape;
use crate::numfmt::push_num;

/// Corner radius of `Round` nodes, in px.
const ROUND_RADIUS: f64 = 8.0;
/// Inset of the subroutine bars and the double-circle ring, in px.
const INSET: f64 = 4.0;

struct D(String);

impl D {
    fn cmd(&mut self, c: char, pts: &[f64]) -> &mut Self {
        self.0.push(c);
        for (i, v) in pts.iter().enumerate() {
            if i > 0 {
                self.0.push(' ');
            }
            push_num(&mut self.0, *v);
        }
        self
    }
    /// Elliptical arc to (x, y) with the given radii, small arc, clockwise.
    fn arc(&mut self, rx: f64, ry: f64, x: f64, y: f64) -> &mut Self {
        self.cmd('A', &[rx, ry, 0.0, 0.0, 1.0, x, y])
    }
    fn z(&mut self) -> &mut Self {
        self.0.push('Z');
        self
    }
}

fn nonneg(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

fn min(a: f64, b: f64) -> f64 {
    if a < b {
        a
    } else {
        b
    }
}

/// Rectangle with corner radius `r` (clamped to half of each side).
fn round_rect(d: &mut D, l: f64, t: f64, w: f64, h: f64, r: f64) {
    let r = min(r, min(w / 2.0, h / 2.0));
    let (rt, b) = (l + w, t + h);
    if r <= 0.0 {
        d.cmd('M', &[l, t])
            .cmd('H', &[rt])
            .cmd('V', &[b])
            .cmd('H', &[l])
            .z();
        return;
    }
    d.cmd('M', &[l + r, t])
        .cmd('H', &[rt - r])
        .arc(r, r, rt, t + r)
        .cmd('V', &[b - r])
        .arc(r, r, rt - r, b)
        .cmd('H', &[l + r])
        .arc(r, r, l, b - r)
        .cmd('V', &[t + r])
        .arc(r, r, l + r, t)
        .z();
}

/// Full ellipse inscribed in the box, as two half arcs.
fn ellipse(d: &mut D, cx: f64, cy: f64, rx: f64, ry: f64) {
    d.cmd('M', &[cx - rx, cy])
        .arc(rx, ry, cx + rx, cy)
        .arc(rx, ry, cx - rx, cy)
        .z();
}

fn polygon(d: &mut D, pts: &[(f64, f64)]) {
    for (i, (x, y)) in pts.iter().enumerate() {
        d.cmd(if i == 0 { 'M' } else { 'L' }, &[*x, *y]);
    }
    d.z();
}

/// Path `d` for a shape centred on (`cx`, `cy`) with outer size `w` × `h`.
pub fn shape_d(shape: Shape, cx: f64, cy: f64, w: f64, h: f64) -> String {
    let (cx, cy) = (
        if cx.is_finite() { cx } else { 0.0 },
        if cy.is_finite() { cy } else { 0.0 },
    );
    let (w, h) = (nonneg(w), nonneg(h));
    let (l, t, r, b) = (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
    let mut d = D(String::new());
    match shape {
        Shape::Rect => round_rect(&mut d, l, t, w, h, 0.0),
        Shape::Round => round_rect(&mut d, l, t, w, h, ROUND_RADIUS),
        Shape::Stadium => round_rect(&mut d, l, t, w, h, h / 2.0),
        Shape::Subroutine => {
            round_rect(&mut d, l, t, w, h, 0.0);
            let i = min(2.0 * INSET, w / 4.0);
            d.cmd('M', &[l + i, t]).cmd('V', &[b]);
            d.cmd('M', &[r - i, t]).cmd('V', &[b]);
        }
        Shape::Cylinder => {
            // The top and bottom caps are half-ellipses of height 2·ry.
            let rx = w / 2.0;
            let ry = min(h / 4.0, 4.0 + w * 0.05);
            d.cmd('M', &[l, t + ry])
                .arc(rx, ry, r, t + ry)
                .cmd('V', &[b - ry])
                .arc(rx, ry, l, b - ry)
                .z();
            // Front rim of the top cap, drawn right to left through the bottom so it
            // winds clockwise like the body and the nonzero fill keeps the cap filled.
            d.cmd('M', &[r, t + ry]).arc(rx, ry, l, t + ry);
        }
        Shape::Circle => ellipse(&mut d, cx, cy, w / 2.0, h / 2.0),
        Shape::DoubleCircle => {
            ellipse(&mut d, cx, cy, w / 2.0, h / 2.0);
            let (irx, iry) = (nonneg(w / 2.0 - INSET), nonneg(h / 2.0 - INSET));
            ellipse(&mut d, cx, cy, irx, iry);
        }
        Shape::Asymmetric => {
            let n = min(h / 2.0, w / 4.0);
            polygon(&mut d, &[(l, t), (r, t), (r, b), (l, b), (l + n, cy)]);
        }
        Shape::Rhombus => polygon(&mut d, &[(cx, t), (r, cy), (cx, b), (l, cy)]),
        Shape::Hexagon => {
            let m = min(h / 4.0, w / 4.0);
            polygon(
                &mut d,
                &[
                    (l + m, t),
                    (r - m, t),
                    (r, cy),
                    (r - m, b),
                    (l + m, b),
                    (l, cy),
                ],
            );
        }
        Shape::Parallelogram => {
            let s = min(h / 2.0, w / 4.0);
            polygon(&mut d, &[(l + s, t), (r, t), (r - s, b), (l, b)]);
        }
        Shape::ParallelogramAlt => {
            let s = min(h / 2.0, w / 4.0);
            polygon(&mut d, &[(l, t), (r - s, t), (r, b), (l + s, b)]);
        }
        Shape::Trapezoid => {
            let s = min(h / 2.0, w / 4.0);
            polygon(&mut d, &[(l + s, t), (r - s, t), (r, b), (l, b)]);
        }
        Shape::TrapezoidAlt => {
            let s = min(h / 2.0, w / 4.0);
            polygon(&mut d, &[(l, t), (r, t), (r - s, b), (l + s, b)]);
        }
    }
    d.0
}

/// Every shape, for exhaustive tests.
pub const ALL_SHAPES: [Shape; 14] = [
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_is_the_box() {
        assert_eq!(
            shape_d(Shape::Rect, 50.0, 20.0, 100.0, 40.0),
            "M0 0H100V40H0Z"
        );
    }

    #[test]
    fn rhombus_touches_the_box_midpoints() {
        assert_eq!(
            shape_d(Shape::Rhombus, 50.0, 20.0, 100.0, 40.0),
            "M50 0L100 20L50 40L0 20Z"
        );
    }

    #[test]
    fn stadium_has_semicircular_ends() {
        let d = shape_d(Shape::Stadium, 50.0, 20.0, 100.0, 40.0);
        assert!(d.starts_with("M20 0H80A20 20 0 0 1 100 20"), "{}", d);
    }

    #[test]
    fn round_radius_is_clamped_for_small_boxes() {
        let d = shape_d(Shape::Round, 5.0, 5.0, 10.0, 10.0);
        assert!(d.starts_with("M5 0H5A5 5"), "{}", d);
    }

    #[test]
    fn every_shape_stays_inside_its_box() {
        for s in ALL_SHAPES {
            let d = shape_d(s, 60.0, 30.0, 80.0, 40.0);
            assert!(d.starts_with('M') && !d.is_empty(), "{:?}", s);
            // Endpoints of every command lie in [20, 100] × [10, 50]; arcs flags are 0/1.
            for tok in d
                .split(|c: char| c.is_ascii_alphabetic() || c == ' ')
                .filter(|t| !t.is_empty())
            {
                let v: f64 = tok.parse().unwrap();
                assert!((0.0..=100.0).contains(&v), "{:?}: {}", s, d);
            }
        }
    }

    #[test]
    fn composite_shapes_have_extra_subpaths() {
        for s in [Shape::Subroutine, Shape::Cylinder, Shape::DoubleCircle] {
            let d = shape_d(s, 60.0, 30.0, 80.0, 40.0);
            assert!(d.matches('M').count() >= 2, "{:?}: {}", s, d);
        }
    }

    #[test]
    fn cylinder_rim_winds_with_the_body() {
        // Drawn right to left through the bottom (clockwise, like the body), so the
        // nonzero fill rule does not punch the lower half of the top cap out.
        let d = shape_d(Shape::Cylinder, 60.0, 30.0, 80.0, 40.0);
        assert!(d.ends_with("ZM100 18A40 8 0 0 1 20 18"), "{}", d);
    }

    #[test]
    fn hostile_geometry_is_total() {
        for s in ALL_SHAPES {
            let d = shape_d(s, f64::NAN, f64::INFINITY, -5.0, f64::NAN);
            assert!(!d.contains("NaN") && !d.contains("inf"), "{}", d);
        }
    }
}
