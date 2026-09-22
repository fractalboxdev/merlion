//! Error tolerance: repairs R001–R006 and their fixes (specs/parser.md#error-tolerance).

mod parse_support;

use merlion_render::diag::Severity;
use merlion_render::model::Shape;
use merlion_render::options::Limits;
use merlion_render::parse::ParseError;
use parse_support::*;

// ---------------------------------------------------------------- R001

#[test]
fn r001_quotes_labels_with_special_characters() {
    let cases = [
        ("A[Process (main)]", Shape::Rect, "Process (main)"),
        ("A[Step 1: init]", Shape::Rect, "Step 1: init"),
        ("A{Is x > y: ok?}", Shape::Rhombus, "Is x > y: ok?"),
        ("A(call f[x])", Shape::Round, "call f[x]"),
        ("A[a ( b]", Shape::Rect, "a ( b"),
        ("A[map {k}]", Shape::Rect, "map {k}"),
        ("A([Start (here)])", Shape::Stadium, "Start (here)"),
        ("A[[Sub: routine]]", Shape::Subroutine, "Sub: routine"),
        ("A[(DB (primary))]", Shape::Cylinder, "DB (primary)"),
        ("A>Flag: up]", Shape::Asymmetric, "Flag: up"),
        ("A{{Hex (6)}}", Shape::Hexagon, "Hex (6)"),
        ("A[/In: data/]", Shape::Parallelogram, "In: data"),
        ("A[say \"hi\" (loud)]", Shape::Rect, "say \"hi\" (loud)"),
    ];
    for (stmt, shape, label) in cases {
        let src = format!("flowchart TD\n{stmt} --> B[b]");
        let (f, d) = parse_ok(&src);
        assert_eq!(codes(&d), vec!["R001"], "{stmt}");
        assert_eq!(
            (node(&f, "A").shape, node(&f, "A").label.as_str()),
            (shape, label),
            "{stmt}"
        );
        let r = &d[0];
        assert_eq!(r.span.line, 2);
        let (out, f2) = assert_fix_round_trip(&src, &["R001"]);
        assert_eq!(node(&f2, "A").label, label, "{stmt} -> {out}");
        assert_eq!(node(&f2, "A").shape, shape, "{stmt} -> {out}");
    }
}

#[test]
fn r001_applies_to_subgraph_titles() {
    let src = "flowchart TD\nsubgraph S[Backend (v2)]\n  A[a]\nend";
    let (f, d) = parse_ok(src);
    assert_eq!(codes(&d), vec!["R001"]);
    assert_eq!(f.subgraphs[0].title, "Backend (v2)");
    let (_, f2) = assert_fix_round_trip(src, &["R001"]);
    assert_eq!(f2.subgraphs[0].title, "Backend (v2)");
}

#[test]
fn quoted_labels_need_no_repair() {
    let (_, d) = parse_ok("flowchart TD\nA[\"Process (main): x\"] --> B[\"{k}\"]");
    assert!(d.is_empty(), "{d:#?}");
}

// ---------------------------------------------------------------- R002

#[test]
fn r002_closes_open_subgraphs() {
    for (src, open) in [
        ("flowchart TD\nsubgraph S\n  A[a]", 1),
        ("flowchart TD\nsubgraph S\n  A[a]\n", 1),
        ("flowchart TD\nsubgraph S\nsubgraph T\n  A[a] --> B[b]", 2),
    ] {
        let (f, d) = parse_ok(src);
        assert_eq!(count(&d, "R002"), open, "{src:?}");
        assert_eq!(f.subgraphs.len(), open);
        assert_eq!(node(&f, "A").subgraph, Some(open - 1));
        let (out, f2) = assert_fix_round_trip(src, &["R002"]);
        assert_eq!(f2.subgraphs.len(), open, "{out:?}");
        assert_eq!(node(&f2, "A").subgraph, Some(open - 1));
    }
}

// ---------------------------------------------------------------- R003

#[test]
fn r003_typographic_quotes() {
    let cases = [
        ("A[“Label”] --> B[b]", "A", "Label"),
        ("A[‘Label’] --> B[b]", "A", "Label"),
        ("A[”Label”] --> B[b]", "A", "Label"),
        ("A[“it’s (ok)”] --> B[b]", "A", "it’s (ok)"),
    ];
    for (stmt, id, label) in cases {
        let src = format!("flowchart TD\n{stmt}");
        let (f, d) = parse_ok(&src);
        assert_eq!(count(&d, "R003"), 2, "{stmt}: {d:#?}");
        assert_eq!(node(&f, id).label, label, "{stmt}");
        let (_, f2) = assert_fix_round_trip(&src, &["R003"]);
        assert_eq!(node(&f2, id).label, label);
    }
}

#[test]
fn r003_in_edge_labels_titles_and_links() {
    let src = "flowchart TD\nsubgraph “Title”\n  A[a] -->|“yes”| B[b]\n  A -- “no” --> C[c]\nend\nclick A “https://example.com”";
    let (f, d) = parse_ok(src);
    assert_eq!(count(&d, "R003"), 8, "{d:#?}");
    assert_eq!(f.subgraphs[0].title, "Title");
    assert_eq!(f.edges[0].label.as_deref(), Some("yes"));
    assert_eq!(f.edges[1].label.as_deref(), Some("no"));
    assert_eq!(
        node(&f, "A").link.as_ref().map(|l| l.url.as_str()),
        Some("https://example.com")
    );
    assert_fix_round_trip(src, &["R003"]);
}

#[test]
fn typographic_quotes_inside_text_are_content() {
    let (f, d) = parse_ok("flowchart TD\nA[He said “hi”] --> B[b]");
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(node(&f, "A").label, "He said “hi”");
}

// ---------------------------------------------------------------- R004

#[test]
fn r004_renames_reserved_ids() {
    let cases = [
        ("A[a] --> end", "end_", "end"),
        ("end --> A[a]", "end_", "end"),
        ("A[a] --> graph", "graph_", "graph"),
        ("A[a] --> subgraph", "subgraph_", "subgraph"),
        ("A[a] --> end[Finish]", "end_", "Finish"),
        ("A[a] --> end\nend_x[x] --> end_[e]", "end__", "end"),
    ];
    for (stmt, renamed, label) in cases {
        let src = format!("flowchart TD\n{stmt}");
        let (f, d) = parse_ok(&src);
        assert!(has(&d, "R004"), "{stmt}: {d:#?}");
        assert!(!has(&d, "R005"), "{stmt}: {d:#?}");
        assert_eq!(node(&f, renamed).label, label, "{stmt}");
        let (out, f2) = assert_fix_round_trip(&src, &["R004"]);
        assert_eq!(node(&f2, renamed).label, label, "{stmt} -> {out}");
        assert_eq!(f2.nodes.len(), f.nodes.len(), "{out}");
    }
}

#[test]
fn r004_is_consistent_across_references() {
    let src = "flowchart TD\nA[a] --> end\nend --> B[b]\nstyle end fill:red\nclass end hot\nsubgraph S\n  C[c]\nend";
    let (f, d) = parse_ok(src);
    assert_eq!(count(&d, "R004"), 4, "{d:#?}");
    assert_eq!(f.nodes.len(), 4);
    let e = node(&f, "end_");
    assert_eq!(e.label, "end");
    assert!(e.style.fill.is_some());
    assert_eq!(e.classes, vec!["hot".to_string()]);
    assert_eq!(f.subgraphs.len(), 1);
    let (out, f2) = assert_fix_round_trip(src, &["R004"]);
    assert!(out.contains("end_[end]"), "{out}");
    assert!(node(&f2, "end_").style.fill.is_some());
}

// ---------------------------------------------------------------- R005

#[test]
fn r005_declares_undeclared_edge_targets() {
    let src = "flowchart TD\nA[Start] --> B\nA --> Typo:::k";
    let (f, d) = parse_ok(src);
    let r: Vec<_> = d.iter().filter(|x| x.code == "R005").collect();
    assert_eq!(r.len(), 2, "{d:#?}");
    assert!(r[0].message.contains('B'));
    assert_eq!((r[0].span.line, r[0].span.column), (2, 14));
    assert_eq!(node(&f, "B").label, "B");
    let (out, f2) = assert_fix_round_trip(src, &["R005"]);
    assert!(
        out.contains("B[B]") && out.contains("Typo[Typo]:::k"),
        "{out}"
    );
    assert_eq!(node(&f2, "Typo").classes, vec!["k".to_string()]);
}

#[test]
fn r005_is_not_emitted_for_declared_nodes() {
    for src in [
        "flowchart TD\nA[a] --> B\nB",
        "flowchart TD\nA[a] --> B\nB(b)",
        "flowchart TD\nB{b}\nA[a] --> B",
        "flowchart TD\nA[a] --> B\nB@{ shape: rect }",
        "flowchart TD\nsubgraph S\n  A[a]\nend\nA --> S",
    ] {
        let (_, d) = parse_ok(src);
        assert!(!has(&d, "R005"), "{src:?}: {d:#?}");
    }
}

// ---------------------------------------------------------------- R006

#[test]
fn r006_strips_markdown_fences() {
    for src in [
        "```mermaid\nflowchart TD\nA[a] --> B[b]\n```",
        "```mermaid\nflowchart TD\nA[a] --> B[b]\n```\n",
        "  ```\nflowchart TD\nA[a] --> B[b]",
        "flowchart TD\nA[a] --> B[b]\n```",
        "~~~mermaid\ngraph LR\nA[a] --> B[b]\n~~~",
        "```mermaid\n---\ntitle: T\n---\nflowchart TD\nA[a]\n```",
    ] {
        let (f, d) = parse_ok(src);
        assert!(has(&d, "R006"), "{src:?}");
        assert!(d.iter().all(|x| x.code == "R006"), "{src:?}: {d:#?}");
        assert!(!f.nodes.is_empty());
        let (out, _) = assert_fix_round_trip(src, &["R006"]);
        assert!(!out.contains("```") && !out.contains("~~~"), "{out:?}");
    }
}

// ---------------------------------------------------------------- strict and combined

#[test]
fn strict_mode_turns_each_repair_into_an_error() {
    for (src, code) in [
        ("flowchart TD\nA[f(x)]", "R001"),
        ("flowchart TD\nsubgraph S\nA[a]", "R002"),
        ("flowchart TD\nA[“a”]", "R003"),
        ("flowchart TD\nA[a] --> end", "R004"),
        ("flowchart TD\nA[a] --> B", "R005"),
        ("```\nflowchart TD\nA[a]", "R006"),
    ] {
        let (r, d) = run(src, true, Limits::default());
        assert_eq!(r, Err(ParseError::Failed), "{src:?}");
        let x = d
            .iter()
            .find(|x| x.code == code)
            .unwrap_or_else(|| panic!("{src:?}: {d:#?}"));
        assert_eq!(x.severity, Severity::Error);
        assert!(x.fix.is_some(), "the fix survives promotion: {x:?}");
    }
}

#[test]
fn llm_style_output_repairs_in_one_pass() {
    let src = "```mermaid\nflowchart TD\n    A[“User Request”] --> B{Valid input: yes?}\n    B -->|Yes| C[Process (async)]\n    B -->|No| D[Return error]\n    C --> end\n    subgraph Workers\n        C --> E[(Queue)]\n        E --> F\n```\n";
    let (f, d) = parse_ok(src);
    for code in ["R001", "R002", "R003", "R004", "R005", "R006"] {
        assert!(has(&d, code), "missing {code}: {d:#?}");
    }
    assert!(errors(&d).is_empty());
    assert_eq!(node(&f, "C").label, "Process (async)");
    let (out, f2) = assert_fix_round_trip(src, &["R001", "R002", "R003", "R004", "R005", "R006"]);
    let (_, d2) = parse_ok(&out);
    assert!(
        d2.iter().all(|x| x.severity != Severity::Repair),
        "{out}\n{d2:#?}"
    );
    assert_eq!(f2.nodes.len(), f.nodes.len());
    assert_eq!(f2.edges.len(), f.edges.len());
    assert_eq!(f2.subgraphs.len(), f.subgraphs.len());
}

#[test]
fn every_repair_carries_a_fix() {
    let src = "```\nflowchart TD\nA[“x”] --> B[f(x)] --> end --> C\nsubgraph S\nD[d]";
    let (_, d) = parse_ok(src);
    let repairs: Vec<_> = d
        .iter()
        .filter(|x| x.severity == Severity::Repair)
        .collect();
    assert!(repairs.len() >= 6, "{d:#?}");
    assert!(repairs.iter().all(|x| x.fix.is_some()));
}
