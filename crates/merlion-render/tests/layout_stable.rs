//! Stable layout from a previous render's hint (specs/layout.md#stable-layout).

#[path = "layout_support.rs"]
mod support;

use merlion_render::geometry::Geometry;
use merlion_render::layout::hint;
use merlion_render::model::Flowchart;
use merlion_render::options::{Direction, DirectionOption, RenderOptions};
use support::*;

fn hint_of(c: &Flowchart, g: &Geometry) -> String {
    let ids: Vec<&str> = c.nodes.iter().map(|n| n.id.as_str()).collect();
    hint::format(g.direction, &g.layers, &ids)
}

fn with_hint(h: &str) -> RenderOptions {
    RenderOptions {
        hint: Some(h.into()),
        ..RenderOptions::default()
    }
}

/// A two-level fan whose second layer the sweeps are free to reorder.
fn fan() -> B {
    let mut b = B::new();
    let v = b.nodes(&["root", "p", "q", "r", "s", "t"]);
    for i in 1..6 {
        b.edge(v[0], v[i]);
    }
    let w = b.nodes(&["x", "y"]);
    b.edge(v[1], w[1]);
    b.edge(v[5], w[0]);
    b
}

#[test]
fn hint_round_trip_reproduces_the_layout() {
    let b = fan();
    let g1 = run(&b.c);
    let h = hint_of(&b.c, &g1);
    let (g2, d) = run_with(&b.c, &with_hint(&h));
    let g2 = g2.unwrap();
    assert!(codes(&d).is_empty(), "{:?}", codes(&d));
    assert_eq!(g1.layers, g2.layers);
}

#[test]
fn hinted_order_wins_over_the_fresh_order() {
    let b = fan();
    // Reverse the fresh order of the second layer in the hint.
    let g1 = run(&b.c);
    let mut layers = g1.layers.clone();
    layers[1].reverse();
    let ids: Vec<&str> = b.c.nodes.iter().map(|n| n.id.as_str()).collect();
    let h = hint::format(Direction::TB, &layers, &ids);
    let g2 = run_with(&b.c, &with_hint(&h)).0.unwrap();
    assert_eq!(g2.layers[1], layers[1]);
}

#[test]
fn adding_a_node_keeps_survivor_order_within_stability() {
    let b = fan();
    let g1 = run(&b.c);
    let h = hint_of(&b.c, &g1);
    let mut b2 = fan();
    let n = b2.node("new");
    b2.edge(0, n);
    let (g2, d) = run_with(&b2.c, &with_hint(&h));
    let g2 = g2.unwrap();
    check(&b2.c, &g2);
    assert_eq!(codes(&d), vec!["I021"]);
    for (l, old) in g1.layers.iter().enumerate() {
        let new = &g2.layers[l];
        let surv: Vec<usize> = new.iter().copied().filter(|v| old.contains(v)).collect();
        let old_surv: Vec<usize> = old.iter().copied().filter(|v| new.contains(v)).collect();
        assert_eq!(surv, old_surv, "layer {} order changed", l);
        for (rank, s) in surv.iter().enumerate() {
            let idx = new.iter().position(|v| v == s).unwrap();
            assert!(idx <= rank + 2, "survivor moved {} positions", idx - rank);
        }
    }
}

#[test]
fn whole_svg_is_accepted_as_hint() {
    let b = fan();
    let g1 = run(&b.c);
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" data-merlion-layout=\"{}\"><title>t</title></svg>",
        hint_of(&b.c, &g1)
    );
    let (g2, d) = run_with(&b.c, &with_hint(&svg));
    assert!(codes(&d).is_empty());
    assert_eq!(g2.unwrap().layers, g1.layers);
}

#[test]
fn malformed_hint_is_discarded_with_i022() {
    let b = fan();
    for bad in [
        "garbage",
        "v9;TB;0:root",
        "v1;TB;0:root,root",
        "v1;QQ;0:root",
    ] {
        let (g, d) = run_with(&b.c, &with_hint(bad));
        assert!(g.is_ok());
        assert_eq!(codes(&d), vec!["I022"], "{}", bad);
    }
}

#[test]
fn oversized_hint_is_discarded_with_i022() {
    let b = fan();
    let ids: Vec<String> = (0..3000).map(|i| format!("n{}", i)).collect();
    let h = format!("v1;TB;0:{}", ids.join(","));
    let (_, d) = run_with(&b.c, &with_hint(&h));
    assert_eq!(codes(&d), vec!["I022"]);
}

#[test]
fn hint_with_few_survivors_is_discarded_with_i020() {
    let b = fan();
    let (g, d) = run_with(&b.c, &with_hint("v1;TB;0:root;1:zz,yy,xx"));
    assert!(g.is_ok());
    assert_eq!(codes(&d), vec!["I020"]);
}

#[test]
fn node_whose_layer_changed_is_treated_as_new() {
    let b = fan();
    let g1 = run(&b.c);
    let mut layers = g1.layers.clone();
    // Move "x" (index 6) from layer 2 to layer 1 in the hint.
    layers[2].retain(|&v| v != 6);
    layers[1].push(6);
    let ids: Vec<&str> = b.c.nodes.iter().map(|n| n.id.as_str()).collect();
    let h = hint::format(Direction::TB, &layers, &ids);
    let (_, d) = run_with(&b.c, &with_hint(&h));
    assert_eq!(codes(&d), vec!["I021"]);
    assert!(d.items[0].message.contains('1'));
}

#[test]
fn auto_direction_keeps_the_hint_direction_when_it_fits() {
    let c = chain(3);
    let opts = RenderOptions {
        direction: DirectionOption::Auto,
        hint: Some("v1;LR;0:n0;1:n1;2:n2".into()),
        ..RenderOptions::default()
    };
    let g = run_with(&c, &opts).0.unwrap();
    assert_eq!(g.direction, Direction::LR);
}

#[test]
fn source_direction_wins_over_the_hint_direction() {
    let c = chain(3);
    let g = run_with(&c, &with_hint("v1;LR;0:n0;1:n1;2:n2")).0.unwrap();
    assert_eq!(g.direction, Direction::TB);
}

#[test]
fn random_hints_never_panic() {
    let b = fan();
    let mut r = Lcg(77);
    let alphabet: Vec<char> = "v1;:,TBLR_0123456789abcxyz\"<>= -".chars().collect();
    let mut hints: Vec<String> = vec![
        String::new(),
        "v1".into(),
        "v1;TB".into(),
        "v1;TB;".into(),
        "v1;TB;0:".into(),
        "v1;TB;x:root".into(),
        "v1;TB;0:root;0:p".into(),
        "v1;TB;99999999999999:root".into(),
        "v1;TB;0:_zz".into(),
        "<svg data-merlion-layout=\"v1;TB;0:root".into(),
        "<svg data-merlion-layout=\"\">".into(),
    ];
    for _ in 0..300 {
        let len = r.draw() % 40;
        hints.push(
            (0..len)
                .map(|_| alphabet[r.draw() % alphabet.len()])
                .collect(),
        );
    }
    for h in &hints {
        let (g, d) = run_with(&b.c, &with_hint(h));
        let g = g.unwrap();
        check(&b.c, &g);
        for code in codes(&d) {
            assert!(
                ["I020", "I021", "I022"].contains(&code),
                "{:?}: {}",
                h,
                code
            );
        }
    }
}

#[test]
fn stable_layout_is_deterministic() {
    let b = fan();
    let g1 = run(&b.c);
    let h = hint_of(&b.c, &g1);
    let mut b2 = fan();
    let n = b2.node("late");
    b2.edge(3, n);
    let a = run_with(&b2.c, &with_hint(&h)).0.unwrap();
    let c = run_with(&b2.c, &with_hint(&h)).0.unwrap();
    assert_eq!(a, c);
}
