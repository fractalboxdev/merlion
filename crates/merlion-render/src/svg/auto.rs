//! Automatic tones (specs/svg-output.md#automatic-tones).
//!
//! With `auto_tone` on, an element with no `class` and no `style` takes a built-in role:
//! decisions `warn`, stores `store`, terminals `ok`, and each top-level cluster the
//! series role of its position among the top-level clusters. The role is an ordinary
//! built-in role class next to the marker class `merlion-auto`, so themes, stylesheets
//! and host pages restyle it through the same tokens and role rules as a written one.
//! A role whose built-in is replaced by a `classDef` of the same name is never applied
//! automatically: it would draw the `classDef`.

use alloc::vec::Vec;

use crate::model::{Flowchart, Shape};

use super::roles::{series_name, BuiltIn, Kind};

/// The marker class of an element whose role is automatic.
pub const MARKER: &str = "merlion-auto";

/// The number of series tones top-level clusters cycle through.
pub const SERIES: usize = 8;

/// The built-in role a node shape takes automatically.
pub fn node_role(shape: Shape) -> Option<&'static str> {
    match shape {
        Shape::Rhombus | Shape::Hexagon => Some("warn"),
        Shape::Cylinder => Some("store"),
        Shape::Stadium | Shape::Circle | Shape::DoubleCircle => Some("ok"),
        _ => None,
    }
}

/// Whether the automatic tones are on for `chart`: the render option and the source's
/// `merlion.autoTone` must both allow them.
pub fn enabled(chart: &Flowchart, option: bool) -> bool {
    option && chart.meta.auto_tone != Some(false)
}

fn active(builtins: &[&BuiltIn], kind: Kind, name: &str) -> bool {
    builtins.iter().any(|b| b.kind == kind && b.name == name)
}

/// The automatic role of every node: `None` for a node with a class or a style, a shape
/// outside the mapping, or a role a `classDef` replaces.
pub fn node_roles(chart: &Flowchart, builtins: &[&BuiltIn]) -> Vec<Option<&'static str>> {
    chart
        .nodes
        .iter()
        .map(|n| {
            if !n.classes.is_empty() || !n.style.is_empty() {
                return None;
            }
            node_role(n.shape).filter(|r| active(builtins, Kind::Node, r))
        })
        .collect()
}

/// The automatic role of every cluster. Top-level clusters take `series-{k}`, `k` their
/// 1-based position among the top-level clusters modulo 8, whether or not an earlier one
/// is styled, so styling one cluster never recolours the others. Nested clusters take
/// none.
pub fn cluster_roles(
    chart: &Flowchart,
    parents: &[Option<usize>],
    builtins: &[&BuiltIn],
) -> Vec<Option<&'static str>> {
    let mut top = 0usize;
    chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if parents.get(i).copied().flatten().is_some() {
                return None;
            }
            let k = top % SERIES + 1;
            top += 1;
            if !s.classes.is_empty() || !s.style.is_empty() {
                return None;
            }
            let name = series_name(k as u8);
            active(builtins, Kind::Cluster, name).then_some(name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::roles::BUILT_IN;

    #[test]
    fn mapping_covers_decisions_stores_and_terminals_only() {
        assert_eq!(node_role(Shape::Rhombus), Some("warn"));
        assert_eq!(node_role(Shape::Hexagon), Some("warn"));
        assert_eq!(node_role(Shape::Cylinder), Some("store"));
        for s in [Shape::Stadium, Shape::Circle, Shape::DoubleCircle] {
            assert_eq!(node_role(s), Some("ok"));
        }
        for s in [
            Shape::Rect,
            Shape::Round,
            Shape::Subroutine,
            Shape::HorizontalCylinder,
            Shape::SmallCircle,
        ] {
            assert_eq!(node_role(s), None);
        }
        // Every mapped role is a built-in node role.
        for r in ["warn", "store", "ok"] {
            assert!(BUILT_IN.iter().any(|b| b.name == r && b.kind == Kind::Node));
        }
    }

    #[test]
    fn series_names_are_built_in_cluster_roles() {
        for k in 1..=8u8 {
            let n = series_name(k);
            assert_eq!(n, alloc::format!("series-{}", k));
            assert!(BUILT_IN
                .iter()
                .any(|b| b.name == n && b.kind == Kind::Cluster));
        }
    }
}
