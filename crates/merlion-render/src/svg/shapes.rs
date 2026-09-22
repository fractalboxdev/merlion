//! Node shape outlines. Every shape is one `<path>` filling the node's outer box
//! (`NodeGeom::w` × `h`, centred on `x`, `y`); decorations such as the subroutine bars,
//! the cylinder rim and the inner ring of a double circle are extra subpaths of the
//! same path, so one CSS rule themes the whole shape.

use alloc::string::String;

use crate::layout::measure::{
    cylinder_ry, wave_y, BAND, BOW, BRACE, CYLINDER_RY, NOTCH, SLOPE, STACK, SUBROUTINE_INSET, TAG,
    WAVE, WAVE_K,
};
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
    /// Elliptical arc to (x, y) with the given radii, small arc, counter-clockwise.
    fn arc_ccw(&mut self, rx: f64, ry: f64, x: f64, y: f64) -> &mut Self {
        self.cmd('A', &[rx, ry, 0.0, 0.0, 0.0, x, y])
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

/// Rectangle whose bottom edge is the wave of [`wave_y`] around `yb`, drawn clockwise.
fn document(d: &mut D, l: f64, t: f64, r: f64, yb: f64) {
    let xm = (l + r) / 2.0;
    let k = WAVE_K * WAVE;
    d.cmd('M', &[l, t])
        .cmd('H', &[r])
        .cmd('V', &[yb])
        .cmd('C', &[xm, yb - k, xm, yb + k, l, yb])
        .z();
}

/// The visible top and right edges of a layer stacked `o` up and right of the front
/// box `[l, fr] × [ft, fb]`, as an open subpath.
fn stacked_layer(d: &mut D, l: f64, ft: f64, fr: f64, fb: f64, o: f64) {
    d.cmd('M', &[l + o, ft - o + STACK])
        .cmd('V', &[ft - o])
        .cmd('H', &[fr + o])
        .cmd('V', &[fb - o])
        .cmd('H', &[fr + o - STACK]);
}

/// A curly brace of width `bw` with its tip at `(tip, cy)`, opening towards `sgn`
/// (+1 right, −1 left), traced there and back so it encloses no area to fill.
fn brace(d: &mut D, tip: f64, sgn: f64, t: f64, b: f64, cy: f64, bw: f64) {
    let (inner, mid, q) = (tip + sgn * bw, tip + sgn * bw / 2.0, bw / 2.0);
    d.cmd('M', &[inner, t])
        .cmd('Q', &[mid, t, mid, t + q])
        .cmd('V', &[cy - q])
        .cmd('Q', &[mid, cy, tip, cy])
        .cmd('Q', &[mid, cy, mid, cy + q])
        .cmd('V', &[b - q])
        .cmd('Q', &[mid, b, inner, b])
        .cmd('Q', &[mid, b, mid, b - q])
        .cmd('V', &[cy + q])
        .cmd('Q', &[mid, cy, tip, cy])
        .cmd('Q', &[mid, cy, mid, cy - q])
        .cmd('V', &[t + q])
        .cmd('Q', &[mid, t, inner, t])
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
            let ry = cylinder_ry(w, h);
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
        Shape::SmallCircle | Shape::FilledCircle => ellipse(&mut d, cx, cy, w / 2.0, h / 2.0),
        Shape::FramedCircle => {
            ellipse(&mut d, cx, cy, w / 2.0, h / 2.0);
            let (irx, iry) = (nonneg(w / 2.0 - INSET), nonneg(h / 2.0 - INSET));
            ellipse(&mut d, cx, cy, irx, iry);
        }
        Shape::CrossedCircle => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            ellipse(&mut d, cx, cy, rx, ry);
            // The diagonals at 45° meet the circle at 1/√2 of each radius.
            let (dx, dy) = (
                rx * core::f64::consts::FRAC_1_SQRT_2,
                ry * core::f64::consts::FRAC_1_SQRT_2,
            );
            d.cmd('M', &[cx - dx, cy - dy])
                .cmd('L', &[cx + dx, cy + dy]);
            d.cmd('M', &[cx + dx, cy - dy])
                .cmd('L', &[cx - dx, cy + dy]);
        }
        Shape::Fork => round_rect(&mut d, l, t, w, h, 0.0),
        Shape::Hourglass => polygon(&mut d, &[(l, t), (r, t), (l, b), (r, b)]),
        Shape::Bolt => polygon(
            &mut d,
            &[
                (l + 0.6 * w, t),
                (l, cy + 0.1 * h),
                (cx, cy + 0.1 * h),
                (l + 0.4 * w, b),
                (r, cy - 0.1 * h),
                (cx, cy - 0.1 * h),
            ],
        ),
        Shape::Document => document(&mut d, l, t, r, b - WAVE),
        Shape::LinedDocument => {
            document(&mut d, l, t, r, b - WAVE);
            let x = l + min(SUBROUTINE_INSET, w / 4.0);
            d.cmd('M', &[x, t])
                .cmd('V', &[wave_y(x, l, r, b - WAVE, WAVE)]);
        }
        Shape::TaggedDocument => {
            document(&mut d, l, t, r, b - WAVE);
            let g = min(TAG, min(w, h) / 2.0);
            let yb = b - WAVE;
            polygon(
                &mut d,
                &[(r - g, wave_y(r - g, l, r, yb, WAVE)), (r, yb - g), (r, yb)],
            );
        }
        Shape::StackedDocument => {
            let (ft, fr) = (t + 2.0 * STACK, r - 2.0 * STACK);
            let yb = b - WAVE;
            stacked_layer(&mut d, l, ft, fr, yb, 2.0 * STACK);
            stacked_layer(&mut d, l, ft, fr, yb, STACK);
            document(&mut d, l, ft, fr, yb);
        }
        Shape::StackedRect => {
            let (ft, fr) = (t + 2.0 * STACK, r - 2.0 * STACK);
            stacked_layer(&mut d, l, ft, fr, b, 2.0 * STACK);
            stacked_layer(&mut d, l, ft, fr, b, STACK);
            polygon(&mut d, &[(l, ft), (fr, ft), (fr, b), (l, b)]);
        }
        Shape::Delay => {
            let rr = min(h / 2.0, w / 2.0);
            d.cmd('M', &[l, t])
                .cmd('H', &[r - rr])
                .arc(rr, h / 2.0, r - rr, b)
                .cmd('H', &[l])
                .z();
        }
        Shape::HorizontalCylinder => {
            let rx = min(CYLINDER_RY, w / 2.0);
            d.cmd('M', &[l + rx, t])
                .cmd('H', &[r - rx])
                .arc(rx, h / 2.0, r - rx, b)
                .cmd('H', &[l + rx])
                .arc(rx, h / 2.0, l + rx, t)
                .z();
            // Rim of the right end, drawn clockwise like the body.
            d.cmd('M', &[r - rx, b]).arc(rx, h / 2.0, r - rx, t);
        }
        Shape::LinedCylinder => {
            let rx = w / 2.0;
            let ry = cylinder_ry(w, h);
            d.cmd('M', &[l, t + ry])
                .arc(rx, ry, r, t + ry)
                .cmd('V', &[b - ry])
                .arc(rx, ry, l, b - ry)
                .z();
            d.cmd('M', &[r, t + ry]).arc(rx, ry, l, t + ry);
            let g = min(STACK, h / 4.0);
            d.cmd('M', &[r, t + ry + g]).arc(rx, ry, l, t + ry + g);
        }
        Shape::CurvedTrapezoid => {
            let s = h / 4.0;
            let rr = min(h / 2.0, w / 2.0);
            d.cmd('M', &[l + s, t])
                .cmd('H', &[r - rr])
                .arc(rr, h / 2.0, r - rr, b)
                .cmd('H', &[l + s])
                .cmd('L', &[l, cy])
                .z();
        }
        Shape::DividedRect => {
            round_rect(&mut d, l, t, w, h, 0.0);
            d.cmd('M', &[l, t + min(BAND, h / 2.0)]).cmd('H', &[r]);
        }
        Shape::Triangle => polygon(&mut d, &[(cx, t), (r, b), (l, b)]),
        Shape::FlippedTriangle => polygon(&mut d, &[(l, t), (r, t), (cx, b)]),
        Shape::WindowPane => {
            round_rect(&mut d, l, t, w, h, 0.0);
            let p = min(BAND, min(w, h) / 2.0);
            d.cmd('M', &[l, t + p]).cmd('H', &[r]);
            d.cmd('M', &[l + p, t]).cmd('V', &[b]);
        }
        Shape::NotchedPentagon => {
            let c = min(h / 4.0, w / 4.0);
            polygon(
                &mut d,
                &[
                    (l + c, t),
                    (r - c, t),
                    (r, t + c),
                    (r, b),
                    (l, b),
                    (l, t + c),
                ],
            );
        }
        Shape::SlopedRect => {
            polygon(&mut d, &[(l, t + min(SLOPE, h)), (r, t), (r, b), (l, b)]);
        }
        Shape::BowTieRect => {
            let rx = min(BOW, w / 4.0);
            d.cmd('M', &[l + rx, t])
                .cmd('H', &[r])
                .arc_ccw(rx, h / 2.0, r, b)
                .cmd('H', &[l + rx])
                .arc(rx, h / 2.0, l + rx, t)
                .z();
        }
        Shape::TaggedRect => {
            round_rect(&mut d, l, t, w, h, 0.0);
            let g = min(TAG, min(w, h) / 2.0);
            polygon(&mut d, &[(r - g, b), (r, b - g), (r, b)]);
        }
        Shape::Flag => {
            let (yt, yb) = (t + WAVE, b - WAVE);
            let k = WAVE_K * WAVE;
            d.cmd('M', &[l, yt])
                .cmd('C', &[cx, yt + k, cx, yt - k, r, yt])
                .cmd('V', &[yb])
                .cmd('C', &[cx, yb - k, cx, yb + k, l, yb])
                .z();
        }
        Shape::LinedRect => {
            round_rect(&mut d, l, t, w, h, 0.0);
            d.cmd('M', &[l + min(SUBROUTINE_INSET, w / 4.0), t])
                .cmd('V', &[b]);
        }
        Shape::NotchedRect => {
            let n = min(NOTCH, min(w, h) / 2.0);
            polygon(&mut d, &[(l + n, t), (r, t), (r, b), (l, b), (l, t + n)]);
        }
        // The label alone: two bare moves draw nothing but keep the path's extent the
        // node box, like the extra move after a single brace.
        Shape::TextBlock => {
            d.cmd('M', &[l, t]).cmd('M', &[r, b]);
        }
        Shape::BraceLeft => {
            brace(&mut d, l, 1.0, t, b, cy, min(BRACE, min(w, h) / 2.0));
            d.cmd('M', &[r, b]);
        }
        Shape::BraceRight => {
            brace(&mut d, r, -1.0, t, b, cy, min(BRACE, min(w, h) / 2.0));
            d.cmd('M', &[l, t]);
        }
        Shape::Braces => {
            let bw = min(BRACE, min(w / 4.0, h / 2.0));
            brace(&mut d, l, 1.0, t, b, cy, bw);
            brace(&mut d, r, -1.0, t, b, cy, bw);
        }
        // Lines above and below the label; open subpaths enclose nothing to fill.
        Shape::DataStore => {
            d.cmd('M', &[l, t]).cmd('H', &[r]);
            d.cmd('M', &[r, b]).cmd('H', &[l]);
        }
    }
    d.0
}

/// Every shape, for exhaustive tests.
pub const ALL_SHAPES: [Shape; 46] = Shape::ALL;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

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

    /// End points of every command of a path (control points and arc radii skipped).
    fn end_points(d: &str) -> Vec<(char, Vec<f64>)> {
        let mut out = Vec::new();
        let mut cur: Option<(char, String)> = None;
        for ch in d.chars().chain(core::iter::once('Z')) {
            if ch.is_ascii_alphabetic() {
                if let Some((c, body)) = cur.take() {
                    let nums: Vec<f64> = body
                        .split(' ')
                        .filter(|t| !t.is_empty())
                        .map(|t| t.parse().unwrap())
                        .collect();
                    let keep = match c {
                        'C' => 4,
                        'Q' => 2,
                        'A' => 5,
                        _ => 0,
                    };
                    out.push((c, nums.get(keep..).unwrap_or(&[]).to_vec()));
                }
                cur = Some((ch, String::new()));
            } else if let Some((_, body)) = cur.as_mut() {
                body.push(ch);
            }
        }
        out
    }

    #[test]
    fn every_shape_stays_inside_its_box() {
        for s in ALL_SHAPES {
            let d = shape_d(s, 60.0, 30.0, 80.0, 40.0);
            assert!(d.starts_with('M') && !d.is_empty(), "{:?}", s);
            // End points of every command lie in [20, 100] × [10, 50].
            for (c, nums) in end_points(&d) {
                let ok = match c {
                    'H' => nums.iter().all(|&x| (20.0..=100.0).contains(&x)),
                    'V' => nums.iter().all(|&y| (10.0..=50.0).contains(&y)),
                    _ => nums.chunks(2).all(|p| {
                        (20.0..=100.0).contains(&p[0])
                            && p.get(1).is_none_or(|y| (10.0..=50.0).contains(y))
                    }),
                };
                assert!(ok, "{:?}: {}", s, d);
            }
        }
    }

    #[test]
    fn waves_stay_inside_the_box() {
        // The cubic wave deviates at most WAVE from its base line.
        for i in 0..=80 {
            let x = 20.0 + i as f64;
            let y = wave_y(x, 20.0, 100.0, 50.0 - WAVE, WAVE);
            assert!(
                (50.0 - 2.0 * WAVE - 1e-9..=50.0 + 1e-9).contains(&y),
                "{} {}",
                x,
                y
            );
        }
    }

    #[test]
    fn composite_shapes_have_extra_subpaths() {
        for s in [
            Shape::Subroutine,
            Shape::Cylinder,
            Shape::DoubleCircle,
            Shape::FramedCircle,
            Shape::CrossedCircle,
        ] {
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
    fn cylinder_rim_clears_the_label() {
        use crate::layout::measure::node_size;
        for s in [Shape::Cylinder, Shape::LinedCylinder] {
            for lw in [0.0, 40.0, 200.0, 400.0] {
                let lh = 17.0;
                let (w, h) = node_size(s, lw, lh);
                let d = shape_d(s, 0.0, 0.0, w, h);
                // The last arc is the lowest rim; its vertical radius is the cap's.
                let (_, last) = d.rsplit_once('A').unwrap();
                let nums: Vec<f64> = last.split(' ').map(|t| t.parse().unwrap()).collect();
                let (ry, rim_y) = (nums[1], nums[6]);
                let top = crate::layout::measure::label_offset(s, w, h, lh) - lh / 2.0;
                assert!(
                    rim_y + ry <= top + 1e-9,
                    "{:?} {}: rim reaches {} below the label top {}",
                    s,
                    lw,
                    rim_y + ry,
                    top
                );
            }
        }
    }

    #[test]
    fn outline_free_shapes_still_span_their_box() {
        // The path's extent is the node's box for tools that read it, even where the
        // drawing leaves part of the box empty.
        for s in [
            Shape::TextBlock,
            Shape::BraceLeft,
            Shape::BraceRight,
            Shape::DataStore,
        ] {
            let d = shape_d(s, 60.0, 30.0, 80.0, 40.0);
            let pts: Vec<(f64, f64)> = end_points(&d)
                .into_iter()
                .filter(|(c, _)| !matches!(c, 'H' | 'V' | 'Z'))
                .flat_map(|(_, n)| n.chunks(2).map(|p| (p[0], p[1])).collect::<Vec<_>>())
                .collect();
            let xs = pts.iter().map(|p| p.0);
            let ys = pts.iter().map(|p| p.1);
            let (x0, x1) = (
                xs.clone().fold(f64::MAX, f64::min),
                xs.fold(f64::MIN, f64::max),
            );
            let (y0, y1) = (
                ys.clone().fold(f64::MAX, f64::min),
                ys.fold(f64::MIN, f64::max),
            );
            assert_eq!(
                (x0, y0, x1, y1),
                (20.0, 10.0, 100.0, 50.0),
                "{:?}: {}",
                s,
                d
            );
        }
    }

    #[test]
    fn hostile_geometry_is_total() {
        for s in ALL_SHAPES {
            let d = shape_d(s, f64::NAN, f64::INFINITY, -5.0, f64::NAN);
            assert!(!d.contains("NaN") && !d.contains("inf"), "{}", d);
        }
    }
}
