//! Plain-text outline (specs/svg-output.md#text-alternative).
//!
//! Format:
//!
//! ```text
//! Flowchart, left to right. 5 nodes, 4 edges.
//! Build: Source .md → Has mermaid?
//! Has mermaid? → Render SVG [yes]; → Pass through [no]
//! Render SVG → Cache
//! ```
//!
//! - The first line names the type, the direction and the counts. Invisible (`~~~`)
//!   links are layout-only and are neither counted nor listed.
//! - Then one line per node, in declaration order, for every node that has outgoing
//!   edges, has no edges at all, or sits inside a cluster. A line lists the node's
//!   outgoing edges grouped by source, separated by `; `, labels in brackets.
//! - A node inside a cluster is prefixed with its cluster path as a heading
//!   (`Outer / Inner: `), so every line stands on its own when read aloud or quoted.
//!   A cluster with no members and no nested clusters is listed as `Title:` alone.

use alloc::string::String;
use alloc::vec::Vec;

use crate::model::{Arrow, Flowchart, Stroke};
use crate::options::Direction;

use super::escape::is_dropped;
use super::tree::effective_parents;

fn direction_phrase(d: Direction) -> &'static str {
    match d {
        Direction::TB => "top to bottom",
        Direction::BT => "bottom to top",
        Direction::LR => "left to right",
        Direction::RL => "right to left",
    }
}

fn count(out: &mut String, n: usize, one: &str, many: &str) {
    use core::fmt::Write;
    let _ = write!(out, "{} {}", n, if n == 1 { one } else { many });
}

/// Label as plain text: a hard line break (`\n`) becomes a space, paired Markdown
/// delimiters (`**`, `*`, `` ` ``) are removed, whitespace collapses, dropped
/// characters vanish. A `<br>` still in the label came from `#lt;br#gt;` and is text.
pub fn plain_label(label: &str) -> String {
    // Pass 1: split into tokens so delimiters can be paired.
    enum Tok<'a> {
        Text(&'a str),
        Delim(&'static str),
        Space,
    }
    let mut toks: Vec<Tok> = Vec::new();
    let mut rest = label;
    while !rest.is_empty() {
        if rest.starts_with('\n') {
            toks.push(Tok::Space);
            rest = rest.get(1..).unwrap_or("");
        } else if rest.starts_with("**") {
            toks.push(Tok::Delim("**"));
            rest = rest.get(2..).unwrap_or("");
        } else if rest.starts_with('*') || rest.starts_with('`') {
            toks.push(Tok::Delim(if rest.starts_with('*') { "*" } else { "`" }));
            rest = rest.get(1..).unwrap_or("");
        } else {
            let end = rest
                .char_indices()
                .skip(1)
                .find(|(_, c)| *c == '*' || *c == '`' || *c == '\n')
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            toks.push(Tok::Text(rest.get(..end).unwrap_or("")));
            rest = rest.get(end..).unwrap_or("");
        }
    }
    // Pass 2: per delimiter kind, occurrences pair up in order (1st with 2nd, 3rd with
    // 4th, …) and are dropped; an odd last occurrence stays literal. Linear in length.
    let mut keep = alloc::vec![true; toks.len()];
    for kind in ["**", "*", "`"] {
        let pos: Vec<usize> = toks
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, Tok::Delim(d) if *d == kind))
            .map(|(i, _)| i)
            .collect();
        for pair in pos.chunks_exact(2) {
            for &i in pair {
                if let Some(k) = keep.get_mut(i) {
                    *k = false;
                }
            }
        }
    }
    let mut raw = String::new();
    for (t, k) in toks.iter().zip(keep.iter()) {
        match t {
            Tok::Text(s) => raw.push_str(s),
            Tok::Delim(d) if *k => raw.push_str(d),
            Tok::Delim(_) => {}
            Tok::Space => raw.push(' '),
        }
    }
    let mut out = String::with_capacity(raw.len());
    let mut space = false;
    for c in raw.chars().filter(|c| !is_dropped(*c)) {
        if c.is_whitespace() {
            space = !out.is_empty();
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    out
}

fn arrow_glyph(start: Arrow, end: Arrow) -> &'static str {
    match (start != Arrow::None, end != Arrow::None) {
        (false, true) => "→",
        (true, true) => "↔",
        (true, false) => "←",
        (false, false) => "—",
    }
}

fn node_name(chart: &Flowchart, i: usize) -> String {
    match chart.nodes.get(i) {
        Some(n) => {
            let l = plain_label(&n.label);
            if l.is_empty() {
                plain_label(&n.id)
            } else {
                l
            }
        }
        None => String::from("?"),
    }
}

fn cluster_path(chart: &Flowchart, parents: &[Option<usize>], mut sg: usize) -> String {
    let mut names: Vec<String> = Vec::new();
    // `effective_parents` guarantees an acyclic chain; the bound keeps the loop total anyway.
    for _ in 0..=chart.subgraphs.len() {
        let Some(s) = chart.subgraphs.get(sg) else {
            break;
        };
        let t = plain_label(&s.title);
        names.push(if t.is_empty() { plain_label(&s.id) } else { t });
        match parents.get(sg).copied().flatten() {
            Some(p) => sg = p,
            None => break,
        }
    }
    names.reverse();
    names.join(" / ")
}

/// The outline of a flowchart drawn in direction `dir`.
pub fn outline(chart: &Flowchart, dir: Direction) -> String {
    let visible: Vec<usize> = (0..chart.edges.len())
        .filter(|&i| {
            chart
                .edges
                .get(i)
                .is_some_and(|e| e.stroke != Stroke::Invisible)
        })
        .collect();
    let mut out = String::from("Flowchart, ");
    out.push_str(direction_phrase(dir));
    out.push_str(". ");
    count(&mut out, chart.nodes.len(), "node", "nodes");
    out.push_str(", ");
    count(&mut out, visible.len(), "edge", "edges");
    out.push('.');

    let parents = effective_parents(chart);
    let n = chart.nodes.len();
    let mut outgoing: Vec<Vec<usize>> = alloc::vec![Vec::new(); n];
    let mut has_edge = alloc::vec![false; n];
    for &ei in &visible {
        let Some(e) = chart.edges.get(ei) else {
            continue;
        };
        if let Some(v) = outgoing.get_mut(e.from) {
            v.push(ei);
        }
        for end in [e.from, e.to] {
            if let Some(h) = has_edge.get_mut(end) {
                *h = true;
            }
        }
    }

    for (i, node) in chart.nodes.iter().enumerate() {
        let sg = node.subgraph.filter(|&s| s < chart.subgraphs.len());
        let outs = outgoing.get(i).map(Vec::as_slice).unwrap_or(&[]);
        let isolated = !has_edge.get(i).copied().unwrap_or(false);
        if outs.is_empty() && !isolated && sg.is_none() {
            continue;
        }
        out.push('\n');
        if let Some(s) = sg {
            out.push_str(&cluster_path(chart, &parents, s));
            out.push_str(": ");
        }
        out.push_str(&node_name(chart, i));
        for (k, &ei) in outs.iter().enumerate() {
            let Some(e) = chart.edges.get(ei) else {
                continue;
            };
            out.push_str(if k == 0 { " " } else { "; " });
            out.push_str(arrow_glyph(e.arrow_start, e.arrow_end));
            out.push(' ');
            out.push_str(&node_name(chart, e.to));
            if let Some(l) = e
                .label
                .as_deref()
                .map(plain_label)
                .filter(|l| !l.is_empty())
            {
                out.push_str(" [");
                out.push_str(&l);
                out.push(']');
            }
        }
    }

    // Clusters that would otherwise not appear at all.
    for (si, _) in chart.subgraphs.iter().enumerate() {
        let has_member = chart.nodes.iter().any(|nd| nd.subgraph == Some(si));
        let has_child = parents.contains(&Some(si));
        if !has_member && !has_child {
            out.push('\n');
            out.push_str(&cluster_path(chart, &parents, si));
            out.push(':');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_label_strips_markup() {
        assert_eq!(plain_label("a\nb\nc\nd"), "a b c d");
        // A `<br>` in the label is text the source escaped, not a break.
        assert_eq!(plain_label("a<br>b"), "a<br>b");
        assert_eq!(plain_label("**bold** and *it* `code`"), "bold and it code");
        assert_eq!(plain_label("2 * 3"), "2 * 3");
        assert_eq!(plain_label("  lots   of\tspace "), "lots of space");
        assert_eq!(plain_label("<b>html</b>"), "<b>html</b>");
        assert_eq!(plain_label("a\u{0}b"), "ab");
    }

    #[test]
    fn arrow_glyphs() {
        assert_eq!(arrow_glyph(Arrow::None, Arrow::Arrow), "→");
        assert_eq!(arrow_glyph(Arrow::Circle, Arrow::Cross), "↔");
        assert_eq!(arrow_glyph(Arrow::None, Arrow::None), "—");
        assert_eq!(arrow_glyph(Arrow::Arrow, Arrow::None), "←");
    }
}
