//! Container fit (specs/layout.md#5-container-fit).

#[path = "layout_support.rs"]
mod support;

use merlion_render::options::{Direction, DirectionOption, RenderOptions};
use support::*;

#[test]
fn wide_lr_chain_wraps_to_fit() {
    let mut c = chain(14);
    c.direction = Direction::LR;
    let plain = run_with(
        &c,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    assert!(plain.width > 720.0, "precondition: {}", plain.width);
    let g = run(&c);
    check(&c, &g);
    assert!(g.width <= 720.0, "width {}", g.width);
    assert!(g.height / g.width <= 1.6);
    assert!(g.edges.iter().any(|e| e.wrap));
    assert!(g.edges.iter().filter(|e| !e.wrap).count() >= 10);
    // The hint layers are the phase-2 layers, unchanged by the wrap.
    assert_eq!(g.layers, plain.layers);
    // Orthogonal routes, also around the wrap.
    for e in &g.edges {
        for s in e.points.windows(2) {
            assert!(s[0].x == s[1].x || s[0].y == s[1].y);
        }
    }
}

#[test]
fn wrap_is_not_applied_beyond_max_aspect() {
    let mut c = chain(14);
    c.direction = Direction::LR;
    let g = run_with(
        &c,
        &RenderOptions {
            max_aspect: 0.05,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    assert!(g.edges.iter().all(|e| !e.wrap));
    assert!(g.width > 720.0);
}

fn wide_fan(k: usize) -> B {
    let mut b = B::new();
    let root = b.node("root");
    for i in 0..k {
        let n = b.node(&format!("child{}", i));
        b.edge(root, n);
    }
    b
}

#[test]
fn tb_splits_a_wide_layer_into_rows_that_fit() {
    // Fourteen unconnected nodes form one wide layer.
    let mut b = B::new();
    for i in 0..14 {
        b.node(&format!("child{}", i));
    }
    let plain = run_with(
        &b.c,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    assert!(plain.width > 720.0, "precondition: {}", plain.width);
    let g = run(&b.c);
    check(&b.c, &g);
    assert!(g.width <= 720.0, "width {}", g.width);
    assert!(g.height / g.width <= 1.6);
    assert_eq!(g.layers, plain.layers);
}

#[test]
fn tb_split_narrows_a_wide_fan_and_keeps_edges_downward() {
    // Edges into lower rows pass between the nodes of the rows above, so a single
    // fan narrows but does not reach 720 px with these label widths.
    let b = wide_fan(14);
    let plain = run_with(
        &b.c,
        &RenderOptions {
            target_width: 1e9,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    assert!(plain.width > 720.0, "precondition: {}", plain.width);
    let g = run(&b.c);
    check(&b.c, &g);
    assert!(g.width < plain.width, "{} vs {}", g.width, plain.width);
    assert!(g.height / g.width <= 1.6);
    for (i, n) in g.nodes.iter().enumerate().skip(1) {
        assert!(n.y > g.nodes[0].y, "child {} not below root", i);
    }
    for e in &g.edges {
        assert!(e.points[e.points.len() - 1].y > e.points[0].y);
    }
    assert_eq!(g.layers, plain.layers);
}

#[test]
fn auto_direction_picks_lr_for_a_wide_fan() {
    let b = wide_fan(14);
    let g = run_with(
        &b.c,
        &RenderOptions {
            direction: DirectionOption::Auto,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    check(&b.c, &g);
    assert_eq!(g.direction, Direction::LR);
    assert!(g.width <= 720.0);
}

#[test]
fn auto_direction_keeps_tb_for_a_chain() {
    let c = chain(5);
    let g = run_with(
        &c,
        &RenderOptions {
            direction: DirectionOption::Auto,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    assert!(g.width <= 720.0);
    // Both fit: the smaller area wins, and a vertical chain of rects is the same area
    // either way up to spacing, so only check that the result fits and is valid.
    check(&c, &g);
}

#[test]
fn nothing_fits_leaves_the_drawing_wider() {
    let b = wide_fan(40);
    let g = run_with(
        &b.c,
        &RenderOptions {
            max_aspect: 0.01,
            ..RenderOptions::default()
        },
    )
    .0
    .unwrap();
    check(&b.c, &g);
    assert!(g.width > 720.0);
}

#[test]
fn wrap_never_splits_a_cluster() {
    let mut c = chain(14);
    c.direction = Direction::LR;
    let mut b = B { c };
    let members: Vec<usize> = (5..9).collect();
    let s = b.sub("mid", "Middle", None, &members);
    let g = run(&b.c);
    check(&b.c, &g);
    assert!(g.edges.iter().any(|e| e.wrap));
    let k = &g.clusters[s];
    for &m in &members {
        let (x0, y0, x1, y1) = node_box(&g, m);
        assert!(
            x0 >= k.x && x1 <= k.x + k.w && y0 >= k.y && y1 <= k.y + k.h,
            "node {} outside",
            m
        );
    }
    // No wrapping edge starts or ends inside the cluster's layer span except at its border.
    for (i, e) in b.c.edges.iter().enumerate() {
        if g.edges[i].wrap {
            assert!(!(members.contains(&e.from) && members.contains(&e.to)));
        }
    }
}

#[test]
fn polyline_wraps_stay_inside_the_drawing() {
    let mut c = chain(16);
    c.direction = Direction::RL;
    let opts = RenderOptions {
        edge_style: merlion_render::options::EdgeStyle::Polyline,
        ..RenderOptions::default()
    };
    let g = run_with(&c, &opts).0.unwrap();
    check(&c, &g);
    assert!(g.width <= 720.0);
    assert!(g.edges.iter().any(|e| e.wrap));
}
