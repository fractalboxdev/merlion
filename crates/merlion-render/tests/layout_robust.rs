//! Limits, fuel and adversarial input (specs/architecture.md#boundaries,
//! specs/adr/0008-deterministic-work-budget.md).

#[path = "layout_support.rs"]
mod support;

use merlion_render::layout::{metrics, LayoutError};
use merlion_render::options::{DirectionOption, EdgeStyle, Limits, RenderOptions};
use support::*;

#[test]
fn random_graphs_keep_every_invariant() {
    for seed in 0..40u64 {
        let mut r = Lcg(seed);
        let n = 1 + r.next() % 25;
        let m = r.next() % 45;
        let k = r.next() % 4;
        let c = random(seed, n, m, k);
        let g = run(&c);
        check(&c, &g);
        assert_eq!(metrics::node_overlaps(&g), 0);
    }
}

#[test]
fn random_graphs_in_every_direction_and_style() {
    let dirs = [
        merlion_render::options::Direction::TB,
        merlion_render::options::Direction::BT,
        merlion_render::options::Direction::LR,
        merlion_render::options::Direction::RL,
    ];
    for seed in 100..124u64 {
        let mut c = random(seed, 16, 26, 2);
        c.direction = dirs[(seed % 4) as usize];
        let style = if seed % 3 == 0 { EdgeStyle::Polyline } else { EdgeStyle::Orthogonal };
        let opts = RenderOptions { edge_style: style, ..RenderOptions::default() };
        let g = run_with(&c, &opts).0.unwrap();
        check(&c, &g);
    }
}

#[test]
fn large_random_graph_stays_within_limits() {
    let c = random(7, 300, 600, 8);
    let (g, _) = run_with(&c, &RenderOptions::default());
    match g {
        Ok(g) => {
            check(&c, &g);
            assert!(g.fuel_used <= RenderOptions::default().fuel);
        }
        Err(LayoutError::TooLarge { .. }) => {}
    }
}

#[test]
fn tiny_fuel_is_too_large() {
    let c = random(1, 40, 80, 2);
    let opts = RenderOptions { fuel: 50, ..RenderOptions::default() };
    let (g, _) = run_with(&c, &opts);
    assert!(matches!(g, Err(LayoutError::TooLarge { .. })));
}

#[test]
fn layered_node_limit_is_too_large() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b"]);
    b.edge_full(v[0], v[1], None, 400, merlion_render::model::Stroke::Normal);
    let opts = RenderOptions { limits: Limits { layered_nodes: 100, ..Limits::default() }, ..RenderOptions::default() };
    assert!(matches!(run_with(&b.c, &opts).0, Err(LayoutError::TooLarge { .. })));
}

#[test]
fn layer_limit_is_too_large() {
    let c = chain(30);
    let opts = RenderOptions { limits: Limits { layers: 20, ..Limits::default() }, ..RenderOptions::default() };
    assert!(matches!(run_with(&c, &opts).0, Err(LayoutError::TooLarge { .. })));
}

#[test]
fn node_and_edge_limits_are_too_large() {
    let c = chain(30);
    let opts = RenderOptions { limits: Limits { nodes: 10, ..Limits::default() }, ..RenderOptions::default() };
    assert!(matches!(run_with(&c, &opts).0, Err(LayoutError::TooLarge { .. })));
    let opts = RenderOptions { limits: Limits { edges: 10, ..Limits::default() }, ..RenderOptions::default() };
    assert!(matches!(run_with(&c, &opts).0, Err(LayoutError::TooLarge { .. })));
}

#[test]
fn hostile_options_do_not_panic() {
    let c = random(9, 12, 20, 2);
    for opts in [
        RenderOptions { node_spacing: f64::NAN, rank_spacing: -5.0, ..RenderOptions::default() },
        RenderOptions { wrap_width: -1.0, target_width: 0.0, max_aspect: f64::NAN, ..RenderOptions::default() },
        RenderOptions { target_width: f64::INFINITY, font_size: 0.0, ..RenderOptions::default() },
        RenderOptions { direction: DirectionOption::Auto, target_width: 10.0, ..RenderOptions::default() },
        RenderOptions { stability: u32::MAX, hint: Some("v1;TB;0:n0".into()), ..RenderOptions::default() },
        RenderOptions { node_spacing: 1e300, ..RenderOptions::default() },
    ] {
        let g = run_with(&c, &opts).0.unwrap();
        for n in &g.nodes {
            assert!(n.x.is_finite() && n.y.is_finite());
        }
    }
}

#[test]
fn broken_cluster_tree_does_not_panic() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c"]);
    b.edge(v[0], v[1]);
    b.edge(v[1], v[2]);
    let s0 = b.sub("s0", "zero", None, &[v[0]]);
    let s1 = b.sub("s1", "one", Some(s0), &[v[1]]);
    b.c.subgraphs[s0].parent = Some(s1); // cycle
    b.sub("s2", "two", Some(77), &[]);
    b.c.nodes[2].subgraph = Some(99);
    let g = run(&b.c);
    check(&b.c, &g);
}

#[test]
fn deeply_nested_clusters_do_not_panic() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b"]);
    b.edge(v[0], v[1]);
    let mut parent = None;
    for i in 0..100 {
        parent = Some(b.sub(&format!("s{}", i), "t", parent, &[]));
    }
    b.c.nodes[1].subgraph = parent;
    let g = run(&b.c);
    check(&b.c, &g);
}

#[test]
fn dense_cycles_and_multi_edges() {
    let mut b = B::new();
    let v = b.nodes(&["a", "b", "c", "d"]);
    for &x in &v {
        for &y in &v {
            b.edge(x, y);
            b.edge(x, y);
        }
    }
    let g = run(&b.c);
    check(&b.c, &g);
    assert!(g.edges.iter().any(|e| e.back));
}
