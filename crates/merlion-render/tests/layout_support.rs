//! Builders and invariant checks shared by the layout integration tests
//! (included with `#[path]`).
#![allow(dead_code)]

use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::geometry::Geometry;
use merlion_render::layout::measure::inside;
use merlion_render::layout::{layout_flowchart, LayoutError};
use merlion_render::model::{Arrow, Edge, Flowchart, Node, Shape, Stroke, Style, Subgraph};
use merlion_render::options::{Direction, RenderOptions};

pub struct B {
    pub c: Flowchart,
}

impl Default for B {
    fn default() -> Self {
        B::new()
    }
}

impl B {
    pub fn new() -> Self {
        B {
            c: Flowchart::default(),
        }
    }

    pub fn dir(mut self, d: Direction) -> Self {
        self.c.direction = d;
        self
    }

    pub fn shape(&mut self, id: &str, label: &str, shape: Shape) -> usize {
        self.c.nodes.push(Node {
            id: id.into(),
            label: label.into(),
            shape,
            classes: vec![],
            style: Style::default(),
            link: None,
            subgraph: None,
            span: Default::default(),
            reserve: Default::default(),
        });
        self.c.nodes.len() - 1
    }

    pub fn node(&mut self, id: &str) -> usize {
        self.shape(id, id, Shape::Rect)
    }

    pub fn nodes(&mut self, ids: &[&str]) -> Vec<usize> {
        ids.iter().map(|i| self.node(i)).collect()
    }

    pub fn edge_full(
        &mut self,
        from: usize,
        to: usize,
        label: Option<&str>,
        min_len: u32,
        stroke: Stroke,
    ) -> usize {
        self.c.edges.push(Edge {
            from,
            to,
            label: label.map(Into::into),
            stroke,
            arrow_start: Arrow::None,
            arrow_end: Arrow::Arrow,
            min_len,
            style: Style::default(),
            span: Default::default(),
            id: None,
            classes: Vec::new(),
        });
        self.c.edges.len() - 1
    }

    pub fn edge(&mut self, from: usize, to: usize) -> usize {
        self.edge_full(from, to, None, 1, Stroke::Normal)
    }

    pub fn edge_l(&mut self, from: usize, to: usize, label: &str) -> usize {
        self.edge_full(from, to, Some(label), 1, Stroke::Normal)
    }

    pub fn sub(&mut self, id: &str, title: &str, parent: Option<usize>, nodes: &[usize]) -> usize {
        let idx = self.c.subgraphs.len();
        self.c.subgraphs.push(Subgraph {
            id: id.into(),
            title: title.into(),
            parent,
            nodes: nodes.to_vec(),
            direction: None,
            classes: Vec::new(),
            style: Default::default(),
            span: Default::default(),
        });
        for &n in nodes {
            self.c.nodes[n].subgraph = Some(idx);
        }
        idx
    }
}

pub fn chain(n: usize) -> Flowchart {
    let mut b = B::new();
    let ids: Vec<String> = (0..n).map(|i| format!("n{}", i)).collect();
    for id in &ids {
        b.node(id);
    }
    for i in 1..n {
        b.edge(i - 1, i);
    }
    b.c
}

pub struct Lcg(pub u64);
impl Lcg {
    pub fn draw(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize
    }
}

/// A pseudo-random flowchart: `n` nodes, about `m` edges (self-loops, cycles, labels,
/// long edges and clusters included).
pub fn random(seed: u64, n: usize, m: usize, clusters: usize) -> Flowchart {
    let mut r = Lcg(seed);
    let mut b = B::new();
    let shapes = [
        Shape::Rect,
        Shape::Round,
        Shape::Rhombus,
        Shape::Circle,
        Shape::Hexagon,
        Shape::Stadium,
        Shape::Cylinder,
        Shape::Parallelogram,
        Shape::Trapezoid,
        Shape::Asymmetric,
        Shape::Subroutine,
        Shape::DoubleCircle,
    ];
    for i in 0..n {
        let label = "x".repeat(1 + r.draw() % 12);
        b.shape(&format!("n{}", i), &label, shapes[r.draw() % shapes.len()]);
    }
    for _ in 0..m {
        if n == 0 {
            break;
        }
        let (u, v) = (r.draw() % n, r.draw() % n);
        let label = if r.draw() % 4 == 0 { Some("yes") } else { None };
        let len = 1 + (r.draw() % 5 == 0) as u32;
        b.edge_full(u, v, label, len, Stroke::Normal);
    }
    for c in 0..clusters {
        let parent = if c > 0 && r.draw() % 2 == 0 {
            Some(r.draw() % c)
        } else {
            None
        };
        b.sub(&format!("s{}", c), &format!("Cluster {}", c), parent, &[]);
    }
    if clusters > 0 {
        for i in 0..n {
            if r.draw() % 3 == 0 {
                let s = r.draw() % clusters;
                b.c.nodes[i].subgraph = Some(s);
                b.c.subgraphs[s].nodes.push(i);
            }
        }
    }
    b.c
}

pub fn run_with(
    c: &Flowchart,
    opts: &RenderOptions,
) -> (Result<Geometry, LayoutError>, Diagnostics) {
    let mut fuel = Fuel::new(opts.fuel);
    let mut d = Diagnostics::new(false);
    let r = layout_flowchart(c, opts, &mut fuel, &mut d);
    (r, d)
}

pub fn run(c: &Flowchart) -> Geometry {
    run_with(c, &RenderOptions::default()).0.expect("layout")
}

pub fn codes(d: &Diagnostics) -> Vec<&'static str> {
    d.items.iter().map(|i| i.code).collect()
}

/// Whether `(px, py)` lies on the outline of node `i`: inside when pulled 1% towards
/// the centre, outside when pushed 1% away (every shape is star-shaped from its centre).
pub fn on_boundary(g: &Geometry, c: &Flowchart, i: usize, px: f64, py: f64) -> bool {
    let n = &g.nodes[i];
    let (dx, dy) = (px - n.x, py - n.y);
    let shape = c.nodes[i].shape;
    inside(shape, n.w, n.h, dx * 0.99, dy * 0.99)
        && !inside(
            shape,
            n.w,
            n.h,
            dx * 1.01 + dx.signum() * 0.02,
            dy * 1.01 + dy.signum() * 0.02,
        )
}

fn boxes_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 < b.2 - 1e-6 && b.0 < a.2 - 1e-6 && a.1 < b.3 - 1e-6 && b.1 < a.3 - 1e-6
}

pub fn node_box(g: &Geometry, i: usize) -> (f64, f64, f64, f64) {
    let n = &g.nodes[i];
    (
        n.x - n.w / 2.0,
        n.y - n.h / 2.0,
        n.x + n.w / 2.0,
        n.y + n.h / 2.0,
    )
}

/// Invariants every layout must satisfy.
pub fn check(c: &Flowchart, g: &Geometry) {
    assert_eq!(g.nodes.len(), c.nodes.len());
    assert_eq!(g.edges.len(), c.edges.len());
    assert_eq!(g.clusters.len(), c.subgraphs.len());
    let within =
        |x: f64, y: f64| x >= -1e-6 && y >= -1e-6 && x <= g.width + 1e-6 && y <= g.height + 1e-6;
    for (i, n) in g.nodes.iter().enumerate() {
        let (x0, y0, x1, y1) = node_box(g, i);
        assert!(
            within(x0, y0) && within(x1, y1),
            "node {} outside the drawing",
            i
        );
        assert!(x0 >= 8.0 - 1e-6 && y0 >= 8.0 - 1e-6, "margin");
        assert!(n.rank <= 15);
        assert!(n.w > 0.0 && n.h > 0.0);
    }
    for i in 0..g.nodes.len() {
        for j in i + 1..g.nodes.len() {
            assert!(
                !boxes_overlap(node_box(g, i), node_box(g, j)),
                "nodes {} and {} overlap",
                i,
                j
            );
        }
    }
    for (e, edge) in c.edges.iter().zip(&g.edges) {
        if e.from >= c.nodes.len() || e.to >= c.nodes.len() {
            assert!(edge.points.is_empty());
            continue;
        }
        assert!(
            edge.points.len() >= 2,
            "edge {:?} has no route",
            (e.from, e.to)
        );
        for p in &edge.points {
            assert!(
                p.x.is_finite() && p.y.is_finite() && within(p.x, p.y),
                "edge point outside"
            );
        }
        let (s, t) = (edge.points[0], edge.points[edge.points.len() - 1]);
        assert!(
            on_boundary(g, c, e.from, s.x, s.y),
            "edge {}->{} does not start on the boundary",
            e.from,
            e.to
        );
        assert!(
            on_boundary(g, c, e.to, t.x, t.y),
            "edge {}->{} does not end on the boundary",
            e.from,
            e.to
        );
        if let Some(l) = &edge.label {
            let lb = (
                l.x - l.label.width / 2.0,
                l.y - l.label.height / 2.0,
                l.x + l.label.width / 2.0,
                l.y + l.label.height / 2.0,
            );
            assert!(within(lb.0, lb.1) && within(lb.2, lb.3), "label outside");
            for i in 0..g.nodes.len() {
                assert!(
                    !boxes_overlap(lb, node_box(g, i)),
                    "edge label overlaps node {}",
                    i
                );
            }
        }
    }
    for cl in &g.clusters {
        assert!(
            within(cl.x, cl.y) && within(cl.x + cl.w, cl.y + cl.h),
            "cluster outside"
        );
    }
    // With a valid cluster tree, members lie inside their clusters and no other node
    // overlaps a cluster box.
    let k = c.subgraphs.len();
    let chain_of = |s: Option<usize>| -> Option<Vec<usize>> {
        let mut out = Vec::new();
        let mut cur = s;
        while let Some(x) = cur {
            if x >= k || out.contains(&x) || out.len() > 64 {
                return None;
            }
            out.push(x);
            cur = c.subgraphs[x].parent;
        }
        Some(out)
    };
    let valid = (0..k).all(|s| chain_of(Some(s)).is_some())
        && c.nodes.iter().all(|n| n.subgraph.is_none_or(|s| s < k));
    if valid {
        for (i, n) in c.nodes.iter().enumerate() {
            let chain = chain_of(n.subgraph).unwrap_or_default();
            let b = node_box(g, i);
            for (s, cl) in g.clusters.iter().enumerate() {
                let bx = (cl.x, cl.y, cl.x + cl.w, cl.y + cl.h);
                if chain.contains(&s) {
                    assert!(
                        b.0 >= bx.0 - 1e-6
                            && b.2 <= bx.2 + 1e-6
                            && b.1 >= bx.1 - 1e-6
                            && b.3 <= bx.3 + 1e-6,
                        "node {} outside its cluster {}",
                        i,
                        s
                    );
                } else {
                    assert!(
                        !boxes_overlap(b, bx),
                        "node {} overlaps foreign cluster {}",
                        i,
                        s
                    );
                }
            }
        }
    }
}
