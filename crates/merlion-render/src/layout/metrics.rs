//! Layout quality metrics over a finished [`Geometry`] (specs/benchmark.md): crossings,
//! bends, total edge length, area, and overlap counts. Shared by the tests and the
//! benchmark harness.

use crate::geometry::{Geometry, Point};
use crate::math::{abs, hypot};

const EPS: f64 = 1e-6;

fn orient(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Whether segments `ab` and `cd` cross at a single interior point of both (touching
/// endpoints and collinear overlaps do not count).
fn proper_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let d1 = orient(c, d, a);
    let d2 = orient(c, d, b);
    let d3 = orient(a, b, c);
    let d4 = orient(a, b, d);
    ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS))
        && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS))
}

/// Pairs of segments from different edges that cross properly.
pub fn crossings(g: &Geometry) -> usize {
    let mut count = 0;
    for (i, a) in g.edges.iter().enumerate() {
        for b in g.edges.iter().skip(i + 1) {
            for sa in a.points.windows(2) {
                for sb in b.points.windows(2) {
                    if proper_cross(sa[0], sa[1], sb[0], sb[1]) {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

/// Interior points of edge polylines where the direction changes.
pub fn bends(g: &Geometry) -> usize {
    let mut count = 0;
    for e in &g.edges {
        for w in e.points.windows(3) {
            if abs(orient(w[0], w[1], w[2])) > EPS {
                count += 1;
            }
        }
    }
    count
}

/// Sum of the lengths of all edge polylines.
pub fn total_edge_length(g: &Geometry) -> f64 {
    g.edges
        .iter()
        .flat_map(|e| e.points.windows(2))
        .map(|s| hypot(s[1].x - s[0].x, s[1].y - s[0].y))
        .sum()
}

/// Drawing area in px².
pub fn area(g: &Geometry) -> f64 {
    g.width * g.height
}

fn overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 < b.2 - EPS && b.0 < a.2 - EPS && a.1 < b.3 - EPS && b.1 < a.3 - EPS
}

fn node_box(g: &Geometry, i: usize) -> (f64, f64, f64, f64) {
    let n = &g.nodes[i];
    (
        n.x - n.w / 2.0,
        n.y - n.h / 2.0,
        n.x + n.w / 2.0,
        n.y + n.h / 2.0,
    )
}

fn label_boxes(g: &Geometry) -> impl Iterator<Item = (f64, f64, f64, f64)> + '_ {
    g.edges.iter().filter_map(|e| e.label.as_ref()).map(|l| {
        (
            l.x - l.label.width / 2.0,
            l.y - l.label.height / 2.0,
            l.x + l.label.width / 2.0,
            l.y + l.label.height / 2.0,
        )
    })
}

/// Pairs of node boxes that overlap.
pub fn node_overlaps(g: &Geometry) -> usize {
    let mut count = 0;
    for i in 0..g.nodes.len() {
        for j in i + 1..g.nodes.len() {
            if overlap(node_box(g, i), node_box(g, j)) {
                count += 1;
            }
        }
    }
    count
}

/// Edge-label boxes that overlap a node box or another edge-label box.
pub fn label_overlaps(g: &Geometry) -> usize {
    let labels: alloc::vec::Vec<_> = label_boxes(g).collect();
    let mut count = 0;
    for (i, &l) in labels.iter().enumerate() {
        count += (0..g.nodes.len())
            .filter(|&n| overlap(l, node_box(g, n)))
            .count();
        count += labels
            .iter()
            .skip(i + 1)
            .filter(|&&m| overlap(l, m))
            .count();
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{EdgeGeom, NodeGeom};
    use crate::options::Direction;
    use crate::text::LabelLayout;
    use alloc::vec;
    use alloc::vec::Vec;

    fn edge(pts: &[(f64, f64)]) -> EdgeGeom {
        EdgeGeom {
            points: pts.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            label: None,
            back: false,
            wrap: false,
        }
    }

    fn geom(edges: Vec<EdgeGeom>, nodes: Vec<NodeGeom>) -> Geometry {
        Geometry {
            width: 100.0,
            height: 50.0,
            direction: Direction::TB,
            nodes,
            edges,
            clusters: vec![],
            layers: vec![],
            fuel_used: 0,
        }
    }

    fn node(x: f64, y: f64, w: f64, h: f64) -> NodeGeom {
        NodeGeom {
            x,
            y,
            w,
            h,
            label: LabelLayout::default(),
            rank: 0,
        }
    }

    #[test]
    fn counts_proper_crossings_only() {
        let g = geom(
            vec![
                edge(&[(0.0, 0.0), (10.0, 10.0)]),
                edge(&[(0.0, 10.0), (10.0, 0.0)]),
                // Shares an endpoint with the first edge: not a crossing.
                edge(&[(10.0, 10.0), (20.0, 0.0)]),
            ],
            vec![],
        );
        assert_eq!(crossings(&g), 1);
    }

    #[test]
    fn bends_length_and_area() {
        let g = geom(
            vec![edge(&[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (20.0, 10.0)])],
            vec![],
        );
        assert_eq!(bends(&g), 1);
        assert_eq!(total_edge_length(&g), 30.0);
        assert_eq!(area(&g), 5000.0);
    }

    #[test]
    fn overlap_counts() {
        let g = geom(
            vec![],
            vec![
                node(0.0, 0.0, 10.0, 10.0),
                node(5.0, 5.0, 10.0, 10.0),
                node(10.0, 0.0, 10.0, 10.0),
            ],
        );
        // The first and third only touch.
        assert_eq!(node_overlaps(&g), 2);
        assert_eq!(label_overlaps(&g), 0);
    }
}
