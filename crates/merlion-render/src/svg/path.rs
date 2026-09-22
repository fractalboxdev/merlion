//! Path `d` strings for edges (specs/layout.md#6-edge-routing). Every number goes
//! through `numfmt`, so the text is canonical on every target.

use alloc::string::String;

use crate::geometry::Point;
use crate::math;
use crate::numfmt::push_num;
use crate::options::EdgeStyle;

/// Corner radius of orthogonal routes, in px.
pub const CORNER_RADIUS: f64 = 6.0;

fn push_pt(out: &mut String, cmd: char, p: Point) {
    out.push(cmd);
    push_num(out, p.x);
    out.push(' ');
    push_num(out, p.y);
}

fn finite(p: &Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// Path for an edge polyline in the given style. Fewer than two usable points give "".
pub fn edge_d(points: &[Point], style: EdgeStyle) -> String {
    // Non-finite coordinates would print as 0 and draw a stray segment to the origin.
    let pts: alloc::vec::Vec<Point> = points.iter().copied().filter(finite).collect();
    if pts.len() < 2 {
        return String::new();
    }
    match style {
        EdgeStyle::Orthogonal => rounded(&pts, CORNER_RADIUS),
        // Sleeve routing (specs/layout.md#6-edge-routing) is not implemented; `spline`
        // routes and draws as `polyline`, its prescribed fallback.
        EdgeStyle::Polyline | EdgeStyle::Spline => straight(&pts),
    }
}

fn straight(pts: &[Point]) -> String {
    let mut d = String::new();
    for (i, p) in pts.iter().enumerate() {
        push_pt(&mut d, if i == 0 { 'M' } else { 'L' }, *p);
    }
    d
}

/// Straight segments with a quadratic corner of radius `r` at every interior point.
/// The radius is clamped to half of each adjacent segment, so two corners on one
/// segment never overlap; a corner on a straight run (no turn) stays a plain vertex.
fn rounded(pts: &[Point], r: f64) -> String {
    let mut d = String::new();
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
        return d;
    };
    push_pt(&mut d, 'M', *first);
    for w in pts.windows(3) {
        let (a, p, b) = (w[0], w[1], w[2]);
        let (ix, iy) = (p.x - a.x, p.y - a.y);
        let (ox, oy) = (b.x - p.x, b.y - p.y);
        let lin = math::hypot(ix, iy);
        let lout = math::hypot(ox, oy);
        let cross = ix * oy - iy * ox;
        let rr = min3(r, lin / 2.0, lout / 2.0);
        if rr.is_nan() || rr <= 0.0 || cross == 0.0 {
            push_pt(&mut d, 'L', p);
            continue;
        }
        let start = Point::new(p.x - ix / lin * rr, p.y - iy / lin * rr);
        let end = Point::new(p.x + ox / lout * rr, p.y + oy / lout * rr);
        push_pt(&mut d, 'L', start);
        push_pt(&mut d, 'Q', p);
        d.push(' ');
        push_num(&mut d, end.x);
        d.push(' ');
        push_num(&mut d, end.y);
    }
    push_pt(&mut d, 'L', *last);
    d
}

fn min3(a: f64, b: f64, c: f64) -> f64 {
    let m = if a < b { a } else { b };
    if m < c {
        m
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn polyline_is_straight() {
        let d = edge_d(
            &[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 20.5)],
            EdgeStyle::Polyline,
        );
        assert_eq!(d, "M0 0L10 0L10 20.5");
    }

    #[test]
    fn orthogonal_corner_has_radius_six() {
        let d = edge_d(
            &[p(0.0, 0.0), p(0.0, 50.0), p(40.0, 50.0)],
            EdgeStyle::Orthogonal,
        );
        assert_eq!(d, "M0 0L0 44Q0 50 6 50L40 50");
    }

    #[test]
    fn corner_radius_is_clamped_to_half_the_shorter_segment() {
        let d = edge_d(
            &[p(0.0, 0.0), p(0.0, 4.0), p(40.0, 4.0)],
            EdgeStyle::Orthogonal,
        );
        assert_eq!(d, "M0 0L0 2Q0 4 2 4L40 4");
        // Two corners on a 6 px segment get 3 px each.
        let d = edge_d(
            &[p(0.0, 0.0), p(0.0, 20.0), p(6.0, 20.0), p(6.0, 40.0)],
            EdgeStyle::Orthogonal,
        );
        assert_eq!(d, "M0 0L0 17Q0 20 3 20L3 20Q6 20 6 23L6 40");
    }

    #[test]
    fn collinear_interior_point_is_plain() {
        let d = edge_d(
            &[p(0.0, 0.0), p(0.0, 10.0), p(0.0, 20.0)],
            EdgeStyle::Orthogonal,
        );
        assert_eq!(d, "M0 0L0 10L0 20");
    }

    #[test]
    fn two_points_orthogonal_is_a_line() {
        assert_eq!(
            edge_d(&[p(1.0, 2.0), p(3.0, 4.0)], EdgeStyle::Orthogonal),
            "M1 2L3 4"
        );
    }

    #[test]
    fn degenerate_inputs_give_empty_path() {
        assert_eq!(edge_d(&[], EdgeStyle::Orthogonal), "");
        assert_eq!(edge_d(&[p(1.0, 1.0)], EdgeStyle::Polyline), "");
        assert_eq!(
            edge_d(&[p(f64::NAN, 1.0), p(1.0, 1.0)], EdgeStyle::Polyline),
            ""
        );
        // Repeated points: zero-length segments get no corner and no division by zero.
        let d = edge_d(
            &[p(1.0, 1.0), p(1.0, 1.0), p(1.0, 1.0)],
            EdgeStyle::Orthogonal,
        );
        assert_eq!(d, "M1 1L1 1L1 1");
    }

    #[test]
    fn spline_draws_the_routed_polyline() {
        // Sleeve routing is not implemented (specs/roadmap.md M5); `spline` falls back to
        // `polyline` for drawing as well as routing, so no curve overshoots its vertices.
        let pts = [p(0.0, 0.0), p(0.0, 60.0), p(60.0, 60.0), p(60.0, 0.0)];
        assert_eq!(
            edge_d(&pts, EdgeStyle::Spline),
            edge_d(&pts, EdgeStyle::Polyline)
        );
    }
}
