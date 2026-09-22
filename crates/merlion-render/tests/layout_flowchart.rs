//! Layered layout of hand-built flowcharts (specs/layout.md).

#[path = "layout_support.rs"]
mod support;

use merlion_render::layout::metrics;
use merlion_render::model::{Shape, Stroke};
use merlion_render::options::{Direction, EdgeStyle, RenderOptions};
use support::*;

#[test]
fn single_node_gets_the_margin() {
    let mut b = B::new();
    b.node("a");
    let g = run(&b.c);
    check(&b.c, &g);
    let n = &g.nodes[0];
    assert_eq!(g.width, n.w + 16.0);
    assert_eq!(g.height, n.h + 16.0);
    assert_eq!(n.x, 8.0 + n.w / 2.0);
    assert_eq!(g.layers, vec![vec![0]]);
    assert_eq!(g.direction, Direction::TB);
}

#[test]
fn empty_chart_lays_out() {
    let b = B::new();
    let g = run(&b.c);
    assert!(g.nodes.is_empty());
    assert!(g.width >= 16.0 && g.height >= 16.0);
}

#[test]
fn chain_runs_top_to_bottom_with_rank_spacing() {
    let c = chain(4);
    let g = run(&c);
    check(&c, &g);
    for w in g.nodes.windows(2) {
        assert_eq!(w[0].x, w[1].x);
        let gap = (w[1].y - w[1].h / 2.0) - (w[0].y + w[0].h / 2.0);
        assert!(gap >= 48.0 - 1e-9, "gap {}", gap);
    }
    assert_eq!(g.layers, vec![vec![0], vec![1], vec![2], vec![3]]);
    assert_eq!(metrics::bends(&g), 0);
}

#[test]
fn direction_transforms() {
    for (dir, check_order) in [
        (
            Direction::TB,
            (|a: (f64, f64), b: (f64, f64)| b.1 > a.1 && a.0 == b.0)
                as fn((f64, f64), (f64, f64)) -> bool,
        ),
        (Direction::BT, |a, b| b.1 < a.1 && a.0 == b.0),
        (Direction::LR, |a, b| b.0 > a.0 && a.1 == b.1),
        (Direction::RL, |a, b| b.0 < a.0 && a.1 == b.1),
    ] {
        let mut c = chain(3);
        c.direction = dir;
        let g = run(&c);
        check(&c, &g);
        assert_eq!(g.direction, dir);
        for w in g.nodes.windows(2) {
            assert!(check_order((w[0].x, w[0].y), (w[1].x, w[1].y)), "{:?}", dir);
        }
    }
}

#[test]
fn loop_back_edge_is_marked_and_drawn_in_source_direction() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c", "d"]);
    b.edge(v[0], v[1]);
    b.edge(v[1], v[2]);
    let back = b.edge(v[2], v[1]);
    b.edge(v[2], v[3]);
    let g = run(&b.c);
    check(&b.c, &g);
    let flags: Vec<bool> = g.edges.iter().map(|e| e.back).collect();
    assert_eq!(flags, vec![false, false, true, false]);
    // The back-edge starts at c (below) and ends at b (above).
    let e = &g.edges[back];
    assert!(e.points[0].y > e.points[e.points.len() - 1].y);
}

#[test]
fn rank_is_dominator_depth() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c", "d", "e"]);
    b.edge(v[0], v[1]);
    b.edge(v[0], v[2]);
    b.edge(v[1], v[3]);
    b.edge(v[2], v[3]);
    b.edge(v[3], v[4]);
    let g = run(&b.c);
    let ranks: Vec<u8> = g.nodes.iter().map(|n| n.rank).collect();
    assert_eq!(ranks, vec![0, 1, 1, 1, 2]);
}

#[test]
fn rank_is_clamped_to_15() {
    let c = chain(20);
    let g = run(&c);
    assert_eq!(g.nodes[19].rank, 15);
    assert_eq!(g.nodes[3].rank, 3);
}

#[test]
fn min_len_stretches_edges() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c"]);
    b.edge_full(v[0], v[1], None, 3, Stroke::Normal);
    b.edge(v[1], v[2]);
    let g = run(&b.c);
    check(&b.c, &g);
    assert_eq!(g.layers, vec![vec![0], vec![], vec![], vec![1], vec![2]]);
    assert!(g.nodes[1].y - g.nodes[0].y > 3.0 * 48.0);
}

#[test]
fn edge_labels_sit_on_their_edge_and_clear_of_nodes() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c"]);
    b.edge_l(v[0], v[1], "yes");
    b.edge_l(v[0], v[2], "no");
    b.edge(v[1], v[2]);
    let g = run(&b.c);
    check(&b.c, &g);
    for e in &g.edges[..2] {
        let l = e.label.as_ref().expect("label");
        let on = e.points.windows(2).any(|s| {
            let (a, c) = (s[0], s[1]);
            let minx = a.x.min(c.x) - 1e-6;
            let maxx = a.x.max(c.x) + 1e-6;
            let miny = a.y.min(c.y) - 1e-6;
            let maxy = a.y.max(c.y) + 1e-6;
            (a.x == c.x || a.y == c.y) && l.x >= minx && l.x <= maxx && l.y >= miny && l.y <= maxy
        });
        assert!(on, "label off its edge");
    }
    assert!(g.edges[2].label.is_none());
    assert_eq!(metrics::label_overlaps(&g), 0);
}

#[test]
fn orthogonal_routes_are_axis_aligned() {
    let c = random(5, 14, 22, 0);
    let g = run(&c);
    check(&c, &g);
    for e in &g.edges {
        for s in e.points.windows(2) {
            assert!(
                s[0].x == s[1].x || s[0].y == s[1].y,
                "diagonal segment {:?}",
                s
            );
        }
    }
}

#[test]
fn polyline_passes_through_dummies_and_spline_falls_back() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c", "d"]);
    b.edge(v[0], v[1]);
    b.edge(v[1], v[2]);
    b.edge(v[2], v[3]);
    let long = b.edge(v[0], v[3]);
    for style in [EdgeStyle::Polyline, EdgeStyle::Spline] {
        let opts = RenderOptions {
            edge_style: style,
            ..RenderOptions::default()
        };
        let g = run_with(&b.c, &opts).0.unwrap();
        check(&b.c, &g);
        // Two dummies between the endpoints.
        assert_eq!(g.edges[long].points.len(), 4);
    }
}

#[test]
fn self_loop_sits_beside_its_node() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b"]);
    b.edge(v[0], v[1]);
    let l = b.edge_l(v[0], v[0], "again");
    let g = run(&b.c);
    check(&b.c, &g);
    let e = &g.edges[l];
    let n = &g.nodes[0];
    assert!(e.points.len() >= 4);
    assert!(e.points.iter().any(|p| p.x > n.x + n.w / 2.0 + 4.0));
    let lab = e.label.as_ref().unwrap();
    assert!(lab.x - lab.label.width / 2.0 >= n.x + n.w / 2.0);
    assert!(!e.back);
}

#[test]
fn self_loop_in_lr_sits_below() {
    let mut b = B::new().dir(Direction::LR);
    let v = b.nodes(&["a"]);
    let l = b.edge(v[0], v[0]);
    let g = run(&b.c);
    check(&b.c, &g);
    let n = &g.nodes[0];
    assert!(g.edges[l]
        .points
        .iter()
        .any(|p| p.y > n.y + n.h / 2.0 + 4.0));
}

#[test]
fn invisible_edges_take_part_in_layout() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b"]);
    b.edge_full(v[0], v[1], None, 1, Stroke::Invisible);
    let g = run(&b.c);
    check(&b.c, &g);
    assert!(g.nodes[1].y > g.nodes[0].y);
    assert_eq!(g.edges[0].points.len(), 2);
}

#[test]
fn ports_spread_along_the_node_side() {
    let mut b = B::new();
    let v = b.nodes(&["root", "a", "b", "c"]);
    for i in 1..4 {
        b.edge(v[0], v[i]);
    }
    let g = run(&b.c);
    check(&b.c, &g);
    let mut xs: Vec<f64> = g.edges.iter().map(|e| e.points[0].x).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup();
    assert_eq!(xs.len(), 3);
    let n = &g.nodes[0];
    for e in &g.edges {
        assert!((e.points[0].y - (n.y + n.h / 2.0)).abs() < 1e-6);
    }
}

#[test]
fn every_shape_gets_boundary_ports() {
    let shapes = [
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
    for dir in [Direction::TB, Direction::BT, Direction::LR, Direction::RL] {
        let mut b = B::new().dir(dir);
        let hub = b.shape("hub", "decide", Shape::Rhombus);
        for (i, s) in shapes.iter().enumerate() {
            let n = b.shape(&format!("s{}", i), "shape label", *s);
            b.edge(hub, n);
            let m = b.node(&format!("t{}", i));
            b.edge(n, m);
            b.edge(n, m);
        }
        let g = run(&b.c);
        check(&b.c, &g);
    }
}

#[test]
fn clusters_enclose_members_and_exclude_others() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c", "d", "e", "f"]);
    b.edge(v[0], v[1]);
    b.edge(v[1], v[2]);
    b.edge(v[0], v[3]);
    b.edge(v[3], v[4]);
    b.edge(v[4], v[5]);
    b.edge(v[2], v[5]);
    let outer = b.sub("outer", "Outer cluster", None, &[v[1]]);
    let inner = b.sub("inner", "Inner", Some(outer), &[v[2]]);
    let other = b.sub("other", "Other", None, &[v[3], v[4]]);
    let g = run(&b.c);
    check(&b.c, &g);
    let inside = |c: usize, i: usize| {
        let k = &g.clusters[c];
        let (x0, y0, x1, y1) = node_box(&g, i);
        x0 >= k.x + 12.0 - 1e-6
            && x1 <= k.x + k.w - 12.0 + 1e-6
            && y0 >= k.y + 12.0 - 1e-6
            && y1 <= k.y + k.h - 12.0 + 1e-6
    };
    let clear = |c: usize, i: usize| {
        let k = &g.clusters[c];
        let (x0, y0, x1, y1) = node_box(&g, i);
        x1 <= k.x || x0 >= k.x + k.w || y1 <= k.y || y0 >= k.y + k.h
    };
    assert!(inside(outer, v[1]) && inside(outer, v[2]) && inside(inner, v[2]));
    assert!(inside(other, v[3]) && inside(other, v[4]));
    for i in [v[0], v[3], v[4], v[5]] {
        assert!(clear(outer, i), "node {} inside outer", i);
    }
    for i in [v[0], v[1], v[2], v[5]] {
        assert!(clear(other, i), "node {} inside other", i);
    }
    assert!(clear(inner, v[1]));
    // Nested box inside its parent; sibling boxes apart.
    let (o, n, t) = (&g.clusters[outer], &g.clusters[inner], &g.clusters[other]);
    assert!(n.x >= o.x && n.y >= o.y && n.x + n.w <= o.x + o.w && n.y + n.h <= o.y + o.h);
    assert!(o.x + o.w <= t.x || t.x + t.w <= o.x || o.y + o.h <= t.y || t.y + t.h <= o.y);
    // Title inside the top band.
    assert!(o.label_y > o.y && o.label_y < n.y);
    assert!((o.label_x - (o.x + o.w / 2.0)).abs() < 1e-6);
}

#[test]
fn empty_cluster_gets_a_box() {
    let mut b = B::new();
    let v = b.nodes(&["a"]);
    let s = b.sub("empty", "Nothing here", None, &[]);
    let g = run(&b.c);
    check(&b.c, &g);
    let k = &g.clusters[s];
    assert!(k.w >= k.label.width && k.h > k.label.height);
    let (x0, y0, x1, y1) = node_box(&g, v[0]);
    assert!(x1 <= k.x || x0 >= k.x + k.w || y1 <= k.y || y0 >= k.y + k.h);
}

#[test]
fn tree_has_no_crossings() {
    let mut b = B::new();
    let v = b.nodes(&["r", "a", "b", "a1", "a2", "b1", "b2"]);
    b.edge(v[0], v[1]);
    b.edge(v[0], v[2]);
    b.edge(v[2], v[5]);
    b.edge(v[1], v[3]);
    b.edge(v[2], v[6]);
    b.edge(v[1], v[4]);
    let g = run(&b.c);
    check(&b.c, &g);
    assert_eq!(metrics::crossings(&g), 0);
    assert_eq!(metrics::node_overlaps(&g), 0);
    assert!(metrics::total_edge_length(&g) > 0.0);
    assert!(metrics::area(&g) > 0.0);
}

#[test]
fn layout_is_deterministic() {
    let c = random(42, 30, 50, 3);
    let a = run(&c);
    let b = run(&c);
    assert_eq!(a, b);
}

#[test]
fn out_of_range_edges_get_no_route() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b"]);
    b.edge(v[0], v[1]);
    b.edge(v[0], 7);
    b.edge(9, 9);
    let g = run(&b.c);
    check(&b.c, &g);
    assert!(g.edges[1].points.is_empty() && g.edges[2].points.is_empty());
}

#[test]
fn fuel_used_is_reported() {
    let c = random(3, 20, 30, 1);
    let opts = RenderOptions::default();
    let mut fuel = merlion_render::fuel::Fuel::new(opts.fuel);
    let mut d = merlion_render::diag::Diagnostics::new(false);
    let g = merlion_render::layout::layout_flowchart(&c, &opts, &mut fuel, &mut d).unwrap();
    assert!(g.fuel_used > 0);
    assert_eq!(g.fuel_used, fuel.used());
}

#[test]
fn clusters_enclose_members_in_every_direction() {
    for dir in [Direction::TB, Direction::BT, Direction::LR, Direction::RL] {
        let mut b = B::new().dir(dir);
        let v = b.nodes(&["a", "b", "c", "d"]);
        b.edge(v[0], v[1]);
        b.edge(v[1], v[2]);
        b.edge(v[0], v[3]);
        let s = b.sub("box", "A rather long cluster title", None, &[v[1], v[2]]);
        let g = run(&b.c);
        check(&b.c, &g);
        let k = &g.clusters[s];
        for &m in &[v[1], v[2]] {
            let (x0, y0, x1, y1) = node_box(&g, m);
            assert!(
                x0 >= k.x + 12.0 - 1e-6 && x1 <= k.x + k.w - 12.0 + 1e-6,
                "{:?}",
                dir
            );
            assert!(
                y0 >= k.y + 12.0 - 1e-6 && y1 <= k.y + k.h - 12.0 + 1e-6,
                "{:?}",
                dir
            );
            // The title band sits above the members.
            assert!(y0 >= k.y + 12.0 + k.label.height - 1e-6, "{:?}", dir);
        }
        assert!(
            k.w >= k.label.width + 24.0 - 1e-6,
            "{:?} title wider than box",
            dir
        );
        for &m in &[v[0], v[3]] {
            let (x0, y0, x1, y1) = node_box(&g, m);
            assert!(
                x1 <= k.x || x0 >= k.x + k.w || y1 <= k.y || y0 >= k.y + k.h,
                "{:?}",
                dir
            );
        }
    }
}

#[test]
fn bold_class_widens_the_node() {
    use merlion_render::model::{ClassDef, FontWeight, Style};
    let mut b = B::new();
    let v = b.nodes(&["plain", "bold"]);
    b.c.nodes[v[1]].label = "plain".into();
    b.c.class_defs.push(ClassDef {
        name: "hot".into(),
        style: Style {
            font_weight: Some(FontWeight::SemiBold),
            ..Style::default()
        },
    });
    b.c.nodes[v[1]].classes.push("hot".into());
    let g = run(&b.c);
    assert_eq!(
        g.nodes[v[1]].label.lines[0].runs[0].weight,
        merlion_render::text::Weight::SemiBold
    );
    assert_eq!(
        g.nodes[v[0]].label.lines[0].runs[0].weight,
        merlion_render::text::Weight::Regular
    );
}

#[test]
fn edges_between_layers_have_no_avoidable_crossings() {
    // Two independent chains declared interleaved: the layout keeps them apart.
    let mut b = B::new();
    let v = b.nodes(&["a1", "b1", "a2", "b2", "a3", "b3"]);
    b.edge(v[0], v[2]);
    b.edge(v[1], v[3]);
    b.edge(v[2], v[4]);
    b.edge(v[3], v[5]);
    b.edge(v[0], v[5]);
    let g = run(&b.c);
    check(&b.c, &g);
    assert_eq!(metrics::crossings(&g), 0);
}

#[test]
fn rhombus_and_circle_labels_fit_inside() {
    let mut b = B::new();
    let d = b.shape("d", "Is it a long question?", Shape::Rhombus);
    let c = b.shape("c", "round thing", Shape::Circle);
    let h = b.shape("h", "hex label", Shape::Hexagon);
    b.edge(d, c);
    b.edge(d, h);
    let g = run(&b.c);
    check(&b.c, &g);
    for (i, n) in g.nodes.iter().enumerate() {
        let shape = b.c.nodes[i].shape;
        let (lw, lh) = (n.label.width / 2.0, n.label.height / 2.0);
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            assert!(
                merlion_render::layout::measure::inside(shape, n.w, n.h, sx * lw, sy * lh),
                "{:?}",
                shape
            );
        }
    }
}

/// specs/text-measurement.md#serving-the-font: `font: "system"` draws in a stack measured
/// with Inter's tables at ±6%, so every label box is 6% wider; the text itself is not.
#[test]
fn system_font_widens_label_boxes_by_the_tolerance() {
    use merlion_render::options::FontMode;
    use merlion_render::text::width_tolerance;
    let mut b = B::new();
    let a = b.shape("a", "a fairly long label", Shape::Rect);
    let z = b.node("z");
    b.edge_full(a, z, Some("edge label"), 1, Stroke::Normal);
    let link = run(&b.c);
    let sys = run_with(
        &b.c,
        &RenderOptions {
            font: FontMode::System,
            ..RenderOptions::default()
        },
    )
    .0
    .expect("layout");
    let tol = width_tolerance(FontMode::System);
    assert!(tol > 1.0);
    let (ln, sn) = (&link.nodes[0], &sys.nodes[0]);
    assert_eq!(sn.label.lines, ln.label.lines);
    assert!((sn.label.width - ln.label.width * tol).abs() < 1e-9);
    assert!((sn.w - (ln.w + ln.label.width * (tol - 1.0))).abs() < 1e-9);
    assert_eq!(sn.h, ln.h);
    let (le, se) = (
        link.edges[0].label.as_ref().expect("label"),
        sys.edges[0].label.as_ref().expect("label"),
    );
    assert!((se.label.width - le.label.width * tol).abs() < 1e-9);
    for font in [FontMode::Link, FontMode::Embed] {
        let g = run_with(
            &b.c,
            &RenderOptions {
                font,
                ..RenderOptions::default()
            },
        )
        .0
        .expect("layout");
        assert_eq!(g.nodes[0].w, ln.w);
    }
}
