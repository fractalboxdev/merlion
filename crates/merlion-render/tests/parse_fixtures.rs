//! The shared flowchart corpus in `tests/fixtures/flowcharts/` parses, and applying
//! every fix leaves source that parses without repairs.

mod parse_support;

use std::path::PathBuf;

use merlion_render::diag::Severity;
use parse_support::*;

fn fixtures() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flowcharts");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let body = std::fs::read_to_string(&p).unwrap();
            (name, body)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn corpus_size() {
    let n = fixtures().len();
    assert!((15..=25).contains(&n), "{n} fixtures");
}

#[test]
fn every_fixture_parses() {
    for (name, src) in fixtures() {
        let (f, d) = parse_ok(&src);
        assert!(f.nodes.len() >= 3, "{name}: {} nodes", f.nodes.len());
        assert!(!f.edges.is_empty(), "{name}");
        assert!(errors(&d).is_empty(), "{name}: {d:#?}");
    }
}

#[test]
fn fixes_converge_for_every_fixture() {
    for (name, src) in fixtures() {
        let (_, d) = parse_ok(&src);
        let out = fixed(&src, &d);
        let (_, d2) = parse_ok(&out);
        let left: Vec<_> = d2
            .iter()
            .filter(|x| x.severity == Severity::Repair)
            .collect();
        assert!(left.is_empty(), "{name}: {out}\n{left:#?}");
    }
}

#[test]
fn llm_fixtures_need_repairs_and_clean_ones_do_not() {
    for (name, src) in fixtures() {
        let (_, d) = parse_ok(&src);
        let repairs: Vec<_> = d
            .iter()
            .filter(|x| x.severity == Severity::Repair && x.code != "R005")
            .collect();
        if name.starts_with("llm-") {
            assert!(!repairs.is_empty(), "{name} should exercise repairs");
        } else {
            assert!(repairs.is_empty(), "{name}: {repairs:#?}");
        }
    }
}

#[test]
fn fixture_details() {
    let all = fixtures();
    let get = |n: &str| {
        all.iter()
            .find(|(k, _)| k == n)
            .map(|(_, v)| v.clone())
            .unwrap()
    };
    let (f, _) = parse_ok(&get("nested-architecture.mmd"));
    assert_eq!(f.subgraphs.len(), 5);
    assert_eq!(f.subgraphs[4].id, "cluster");
    assert_eq!(f.subgraphs[4].parent, Some(3));
    let (f, _) = parse_ok(&get("release-train.mmd"));
    assert_eq!(f.meta.title.as_deref(), Some("Release train"));
    assert_eq!(f.meta.curve.as_deref(), Some("stepAfter"));
    let (f, _) = parse_ok(&get("login-flow.mmd"));
    assert_eq!(f.meta.acc_title.as_deref(), Some("Login flow"));
    assert!(node(&f, "home")
        .link
        .as_ref()
        .is_some_and(|l| l.target_blank));
    let (f, _) = parse_ok(&get("edge-gallery.mmd"));
    assert_eq!(f.edges.len(), 14);
    let (f, _) = parse_ok(&get("styled-classes.mmd"));
    assert_eq!(f.class_defs.len(), 3);
    assert_eq!(node(&f, "d").classes, vec!["ok".to_string()]);
}
