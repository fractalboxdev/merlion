//! Front matter, `%%{init}%%` directives and accessibility metadata
//! (specs/parser.md#front-matter-and-directives).

mod parse_support;

use merlion_render::diag::Severity;
use merlion_render::parse::ParseError;
use parse_support::*;

#[test]
fn front_matter_sets_meta() {
    let src = "---\ntitle: Release flow\naccTitle: Release\naccDescr: \"How a release ships\"\nconfig:\n  layout: elk\n  flowchart:\n    curve: linear\n---\nflowchart LR\nA[a] --> B[b]";
    let (f, d) = parse_ok(src);
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(f.meta.title.as_deref(), Some("Release flow"));
    assert_eq!(f.meta.acc_title.as_deref(), Some("Release"));
    assert_eq!(f.meta.acc_descr.as_deref(), Some("How a release ships"));
    assert_eq!(f.meta.layout.as_deref(), Some("elk"));
    assert_eq!(f.meta.curve.as_deref(), Some("linear"));
    // Spans stay relative to the original source.
    assert_eq!(f.nodes[0].span.line, 11);
}

#[test]
fn front_matter_theme_and_unknown_keys() {
    let src = "---\nconfig:\n  theme: forest\n  look: handDrawn\n  layout: circo\n  flowchart:\n    curve: wiggly\n    htmlLabels: false\ndisplayMode: compact\n---\nflowchart LR\nA[a]";
    let (f, d) = parse_ok(src);
    assert_eq!(
        codes(&d),
        vec!["I011", "I011", "W016", "W016", "W016", "W016"]
    );
    let theme = &d[0];
    assert_eq!((theme.span.line, theme.span.column), (3, 3));
    assert_eq!(theme.severity, Severity::Info);
    assert_eq!(f.meta.layout, None);
    assert_eq!(f.meta.curve, None);
}

#[test]
fn front_matter_subset_violations_fail_with_e011() {
    for body in [
        "a: &x 1\nb: *x",
        "title: !!str x",
        "config:\n  - a",
        "title: a\ntitle: b",
        "title: |\n  multi",
    ] {
        let src = format!("---\n{body}\n---\nflowchart LR\nA[a]");
        let (r, d) = try_parse(&src);
        assert_eq!(r, Err(ParseError::Failed), "{body:?}");
        assert_eq!(
            codes(&errors(&d).into_iter().cloned().collect::<Vec<_>>()),
            vec!["E011"],
            "{body:?}: {d:#?}"
        );
        assert!(d.iter().find(|x| x.code == "E011").unwrap().span.line >= 2);
    }
}

#[test]
fn unclosed_front_matter_is_e011() {
    let (r, d) = try_parse("---\ntitle: x\nflowchart LR\nA-->B");
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E011"]);
    assert_eq!(d[0].span.line, 1);
}

#[test]
fn deep_front_matter_is_e011() {
    let mut body = String::new();
    for i in 0..80 {
        body.push_str(&" ".repeat(i));
        body.push_str("k:\n");
    }
    let (r, d) = try_parse(&format!("---\n{body}---\nflowchart LR\nA[a]"));
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E011"]);
}

#[test]
fn init_directive_sets_meta() {
    let src = "%%{init: {'flowchart': {'curve': 'basis'}, 'layout': 'dagre', 'theme': 'dark', 'themeVariables': {'primaryColor': '#fff'}}}%%\nflowchart TD\nA[a]";
    let (f, d) = parse_ok(src);
    assert_eq!(codes(&d), vec!["I011", "I011"]);
    assert_eq!(f.meta.curve.as_deref(), Some("basis"));
    assert_eq!(f.meta.layout.as_deref(), Some("dagre"));
}

#[test]
fn init_directive_anywhere_and_multiline() {
    let src = "flowchart TD\nA[a]\n%%{\n  init: {\n    \"flowchart\": {\"curve\": \"stepBefore\"}\n  }\n}%%\nA --> B[b]";
    let (f, d) = parse_ok(src);
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(f.meta.curve.as_deref(), Some("stepBefore"));
    assert_eq!(f.edges.len(), 1);
}

#[test]
fn directive_overrides_front_matter() {
    let src = "---\nconfig:\n  flowchart:\n    curve: linear\n---\n%%{init: {\"flowchart\": {\"curve\": \"natural\"}}}%%\nflowchart TD\nA[a]";
    let (f, _) = parse_ok(src);
    assert_eq!(f.meta.curve.as_deref(), Some("natural"));
}

#[test]
fn unknown_and_malformed_directives_warn() {
    for (src, n) in [
        ("%%{wrap}%%\nflowchart TD\nA[a]", 1),
        (
            "%%{init: {\"securityLevel\": \"loose\"}}%%\nflowchart TD\nA[a]",
            1,
        ),
        ("%%{init: {\"flowchart\": }}%%\nflowchart TD\nA[a]", 1),
        ("%%{init: [1, 2]}%%\nflowchart TD\nA[a]", 1),
        ("flowchart TD\n%%{init: {'a': 1\nA[a]", 1),
    ] {
        let (f, d) = parse_ok(src);
        assert_eq!(codes(&d), vec!["W016"; n], "{src:?}: {d:#?}");
        assert_eq!(f.nodes.len(), 1, "{src:?}");
    }
}

#[test]
fn oversized_directive_is_e012() {
    let deep = format!(
        "%%{{init: {}1{}}}%%\nflowchart TD\nA[a]",
        "[".repeat(70),
        "]".repeat(70)
    );
    let (r, d) = try_parse(&deep);
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E012"]);
    let long = format!(
        "%%{{init: {{\"theme\": \"{}\"}}}}%%\nflowchart TD\nA[a]",
        "x".repeat(5000)
    );
    let (r, d) = try_parse(&long);
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E012"]);
}

#[test]
fn acc_statements_override_front_matter() {
    let src = "---\naccTitle: From front matter\ntitle: Diagram title\n---\nflowchart TD\naccTitle: From statement\nA[a]";
    let (f, _) = parse_ok(src);
    assert_eq!(f.meta.acc_title.as_deref(), Some("From statement"));
    assert_eq!(f.meta.title.as_deref(), Some("Diagram title"));
}

#[test]
fn directive_inside_subgraph_and_after_statements() {
    let src = "flowchart TD\nsubgraph S\n  %%{init: {\"layout\": \"merlion\"}}%%\n  A[a]\nend";
    let (f, d) = parse_ok(src);
    assert!(d.is_empty(), "{d:#?}");
    assert_eq!(f.meta.layout.as_deref(), Some("merlion"));
    assert_eq!(node(&f, "A").subgraph, Some(0));
}
