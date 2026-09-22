//! Helpers shared by the `parse_*` integration tests.
#![allow(dead_code)]

use merlion_render::diag::{Diagnostic, Diagnostics, Severity};
use merlion_render::model::{Diagram, Edge, Flowchart};
use merlion_render::options::Limits;
use merlion_render::parse::{self, ParseError, ParseOptions};

pub fn run(
    src: &str,
    strict: bool,
    limits: Limits,
) -> (Result<Diagram, ParseError>, Vec<Diagnostic>) {
    let mut diags = Diagnostics::new(strict);
    let r = parse::parse(src, &ParseOptions { strict, limits }, &mut diags);
    (r, diags.items)
}

pub fn try_parse(src: &str) -> (Result<Diagram, ParseError>, Vec<Diagnostic>) {
    run(src, false, Limits::default())
}

/// Parses in default mode and fails the test on an error.
pub fn parse_ok(src: &str) -> (Flowchart, Vec<Diagnostic>) {
    let (r, d) = try_parse(src);
    match r {
        Ok(Diagram::Flowchart(f)) => (f, d),
        Ok(other) => panic!("expected a flowchart, got a {}", other.type_name()),
        Err(e) => panic!("parse failed for {src:?}: {e:?}\n{d:#?}"),
    }
}

/// Parses and asserts that no diagnostic other than the listed codes appears.
pub fn chart(src: &str) -> Flowchart {
    let (f, d) = parse_ok(src);
    let unexpected: Vec<_> = d.iter().filter(|x| x.code != "R005").collect();
    assert!(
        unexpected.is_empty(),
        "{src:?}: unexpected diagnostics {unexpected:#?}"
    );
    f
}

pub fn codes(d: &[Diagnostic]) -> Vec<&'static str> {
    d.iter().map(|x| x.code).collect()
}

pub fn has(d: &[Diagnostic], code: &str) -> bool {
    d.iter().any(|x| x.code == code)
}

pub fn count(d: &[Diagnostic], code: &str) -> usize {
    d.iter().filter(|x| x.code == code).count()
}

pub fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter().filter(|x| x.severity == Severity::Error).collect()
}

pub fn edge_ids(f: &Flowchart) -> Vec<(String, String)> {
    f.edges
        .iter()
        .map(|e| (id(f, e.from), id(f, e.to)))
        .collect()
}

pub fn id(f: &Flowchart, i: usize) -> String {
    f.nodes.get(i).map(|n| n.id.clone()).unwrap_or_default()
}

pub fn node<'a>(f: &'a Flowchart, id: &str) -> &'a merlion_render::model::Node {
    f.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| {
        panic!(
            "no node {id:?} in {:?}",
            f.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
        )
    })
}

pub fn only_edge(f: &Flowchart) -> &Edge {
    assert_eq!(f.edges.len(), 1, "{:?}", f.edges);
    &f.edges[0]
}

/// Applies every fix, as `merlion check --fix` does, and returns the new source.
pub fn fixed(src: &str, d: &[Diagnostic]) -> String {
    parse::repair::apply_fixes(src, d)
}

/// Applies the fixes, reparses, and asserts that none of `repair_codes` remains and
/// that the result carries no error.
pub fn assert_fix_round_trip(src: &str, repair_codes: &[&str]) -> (String, Flowchart) {
    let (_, d) = parse_ok(src);
    for c in repair_codes {
        let r: Vec<_> = d.iter().filter(|x| x.code == *c).collect();
        assert!(!r.is_empty(), "{src:?}: expected {c}, got {:?}", codes(&d));
        for x in r {
            assert_eq!(x.severity, Severity::Repair, "{x:?}");
            assert!(x.fix.is_some(), "{c} without a fix: {x:?}");
        }
    }
    let out = fixed(src, &d);
    let (f, d2) = parse_ok(&out);
    for c in repair_codes {
        assert!(
            !has(&d2, c),
            "{c} remains after fixing {src:?} into {out:?}: {d2:#?}"
        );
    }
    assert!(errors(&d2).is_empty(), "{out:?}: {d2:#?}");
    (out, f)
}
