//! Flowchart syntax (specs/parser.md, mermaid 12 flowchart documentation).

mod parse_support;

use merlion_render::diag::Severity;
use merlion_render::model::{Arrow, Color, Shape, Stroke};
use merlion_render::options::{Direction, Limits};
use merlion_render::parse::ParseError;
use parse_support::*;

// ---------------------------------------------------------------- headers

#[test]
fn headers_and_directions() {
    let cases = [
        ("graph TD", Direction::TB),
        ("graph TB", Direction::TB),
        ("flowchart BT", Direction::BT),
        ("flowchart LR", Direction::LR),
        ("flowchart RL", Direction::RL),
        ("flowchart", Direction::TB),
        ("graph", Direction::TB),
        ("flowchart lr", Direction::LR),
        ("graph TD;", Direction::TB),
        ("  flowchart LR  %% trailing comment", Direction::LR),
        ("\n\n%% leading comment\nflowchart RL\n", Direction::RL),
        ("graph >", Direction::LR),
        ("graph <", Direction::RL),
        ("graph ^", Direction::BT),
        ("graph v", Direction::TB),
        ("flowchart-elk TD", Direction::TB),
        ("graph TD;A-->B;", Direction::TB),
    ];
    for (src, dir) in cases {
        assert_eq!(chart(src).direction, dir, "{src:?}");
    }
}

#[test]
fn unsupported_headers_name_the_header() {
    for (src, header) in [
        ("sequenceDiagram\nA->>B: hi", "sequenceDiagram"),
        ("classDiagram", "classDiagram"),
        ("stateDiagram-v2\n[*] --> A", "stateDiagram-v2"),
        ("erDiagram", "erDiagram"),
        ("gantt", "gantt"),
        ("pie title Pets", "pie"),
        ("Graph TD", "Graph"),
        ("hello world", "hello"),
        ("%%{init: {}}%%\nmindmap", "mindmap"),
    ] {
        let (r, d) = try_parse(src);
        assert_eq!(
            r,
            Err(ParseError::UnsupportedDiagram {
                header: header.into()
            }),
            "{src:?}"
        );
        assert!(errors(&d).is_empty(), "{d:?}");
    }
}

#[test]
fn empty_source_is_a_syntax_error() {
    for src in ["", "   \n\n", "%% only a comment\n"] {
        let (r, d) = try_parse(src);
        assert_eq!(r, Err(ParseError::Failed), "{src:?}");
        assert_eq!(codes(&d), vec!["E002"]);
        assert!(d[0].message.contains("expected"), "{}", d[0].message);
    }
}

#[test]
fn bad_direction_is_a_syntax_error() {
    let (r, d) = try_parse("flowchart XY\nA-->B");
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E002"]);
    assert_eq!((d[0].span.line, d[0].span.column), (1, 11));
    assert!(d[0].message.contains("expected a direction"));
}

// ---------------------------------------------------------------- nodes

#[test]
fn classic_node_shapes() {
    let cases = [
        ("A[text]", Shape::Rect, "text"),
        ("A(text)", Shape::Round, "text"),
        ("A([text])", Shape::Stadium, "text"),
        ("A[[text]]", Shape::Subroutine, "text"),
        ("A[(text)]", Shape::Cylinder, "text"),
        ("A((text))", Shape::Circle, "text"),
        ("A(((text)))", Shape::DoubleCircle, "text"),
        ("A>text]", Shape::Asymmetric, "text"),
        ("A{text}", Shape::Rhombus, "text"),
        ("A{{text}}", Shape::Hexagon, "text"),
        ("A[/text/]", Shape::Parallelogram, "text"),
        ("A[\\text\\]", Shape::ParallelogramAlt, "text"),
        ("A[/text\\]", Shape::Trapezoid, "text"),
        ("A[\\text/]", Shape::TrapezoidAlt, "text"),
        ("A", Shape::Rect, "A"),
        ("A[  spaced out  ]", Shape::Rect, "spaced out"),
        ("A[\"quoted\"]", Shape::Rect, "quoted"),
        ("A(\"quoted round\")", Shape::Round, "quoted round"),
        ("A{\"a (b) c\"}", Shape::Rhombus, "a (b) c"),
        ("A[/path/to]", Shape::Rect, "/path/to"),
        ("A[]", Shape::Rect, ""),
        ("A [spaced opener]", Shape::Rect, "spaced opener"),
        (
            "A[Hello, world! ~ 100% @ home]",
            Shape::Rect,
            "Hello, world! ~ 100% @ home",
        ),
        ("A[Ünïcödé 中文 🎉]", Shape::Rect, "Ünïcödé 中文 🎉"),
    ];
    for (stmt, shape, label) in cases {
        let f = chart(&format!("flowchart TD\n{stmt}"));
        assert_eq!(f.nodes.len(), 1, "{stmt}");
        let n = &f.nodes[0];
        assert_eq!(
            (n.id.as_str(), n.shape, n.label.as_str()),
            ("A", shape, label),
            "{stmt}"
        );
    }
}

#[test]
fn at_shape_form() {
    let cases = [
        ("rect", Shape::Rect),
        ("proc", Shape::Rect),
        ("rounded", Shape::Round),
        ("event", Shape::Round),
        ("stadium", Shape::Stadium),
        ("terminal", Shape::Stadium),
        ("subroutine", Shape::Subroutine),
        ("fr-rect", Shape::Subroutine),
        ("cyl", Shape::Cylinder),
        ("database", Shape::Cylinder),
        ("circle", Shape::Circle),
        ("circ", Shape::Circle),
        ("dbl-circ", Shape::DoubleCircle),
        ("odd", Shape::Asymmetric),
        ("diam", Shape::Rhombus),
        ("decision", Shape::Rhombus),
        ("hex", Shape::Hexagon),
        ("lean-r", Shape::Parallelogram),
        ("lean-l", Shape::ParallelogramAlt),
        ("trap-b", Shape::Trapezoid),
        ("trap-t", Shape::TrapezoidAlt),
    ];
    for (name, shape) in cases {
        let f = chart(&format!(
            "flowchart TD\nA@{{ shape: {name}, label: \"Label {name}\" }}"
        ));
        let n = &f.nodes[0];
        assert_eq!(n.shape, shape, "{name}");
        assert_eq!(n.label, format!("Label {name}"));
    }
}

#[test]
fn at_shape_form_details() {
    // Multi-line, quoted shape, missing label, class shorthand and an edge.
    let f = chart(
        "flowchart LR\nA@{\n  shape: 'diamond'\n}:::hot --> B@{shape: rect, label: \"B (x)\"}",
    );
    assert_eq!(node(&f, "A").shape, Shape::Rhombus);
    assert_eq!(node(&f, "A").label, "A");
    assert_eq!(node(&f, "A").classes, vec!["hot".to_string()]);
    assert_eq!(node(&f, "B").label, "B (x)");
    assert_eq!(edge_ids(&f), vec![("A".into(), "B".into())]);
}

#[test]
fn at_shape_unsupported_shapes_and_keys_warn() {
    let (f, d) =
        parse_ok("flowchart TD\nA@{ shape: bolt, label: Zap }\nB@{ icon: 'fa:user', shape: rect }");
    assert_eq!(node(&f, "A").shape, Shape::Rect);
    assert_eq!(node(&f, "A").label, "Zap");
    assert_eq!(count(&d, "W015"), 2, "{d:#?}");
    assert!(d.iter().all(|x| x.severity == Severity::Warning));
}

#[test]
fn markdown_strings() {
    let f = chart("flowchart TD\nA[\"`**Bold** and\n  *italic*`\"] --> B(\"`code: `x``\")");
    assert_eq!(node(&f, "A").label, "**Bold** and<br>*italic*");
    assert_eq!(node(&f, "B").label, "code: `x`");
}

#[test]
fn entity_codes() {
    let cases = [
        ("#quot;hi#quot;", "\"hi\""),
        ("#35; one", "# one"),
        ("a #amp; b", "a & b"),
        ("#lt;tag#gt;", "<tag>"),
        ("#9829;", "\u{2665}"),
        ("#x2665;", "\u{2665}"),
        ("#nbsp;", "\u{a0}"),
        ("#unknownname;", "#unknownname;"),
        ("#;", "#;"),
        ("#0;", "#0;"),
        ("#99999999;", "#99999999;"),
        ("#55296;", "#55296;"),
        ("50#37; off", "50% off"),
    ];
    for (raw, want) in cases {
        let f = chart(&format!("flowchart TD\nA[\"{raw}\"]"));
        assert_eq!(f.nodes[0].label, want, "{raw}");
    }
}

#[test]
fn line_breaks_normalise_to_br() {
    for raw in ["a<br>b", "a<br/>b", "a<br />b", "a<BR>b", "a<Br/>b"] {
        let f = chart(&format!("flowchart TD\nA[\"{raw}\"]"));
        assert_eq!(f.nodes[0].label, "a<br>b", "{raw}");
    }
    let f = chart("flowchart TD\nA[\"line one\n   line two\"]");
    assert_eq!(f.nodes[0].label, "line one<br>line two");
}

#[test]
fn node_ids() {
    let f = chart("flowchart LR\nmy-node --> node.2 --> 中文 --> _x1 --> 42");
    let ids: Vec<_> = f.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["my-node", "node.2", "中文", "_x1", "42"]);
}

#[test]
fn keyword_prefixed_ids_are_nodes() {
    let f = chart("flowchart LR\nclassA --> styleB --> endpoint --> subgraphs --> clicker --> End");
    let ids: Vec<_> = f.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "classA",
            "styleB",
            "endpoint",
            "subgraphs",
            "clicker",
            "End"
        ]
    );
}

#[test]
fn redeclaration_updates_label_and_shape() {
    let f = chart("flowchart TD\nA --> B\nA[Start]\nB{Choice} --> A");
    assert_eq!(node(&f, "A").label, "Start");
    assert_eq!(node(&f, "B").shape, Shape::Rhombus);
    // A later bare reference keeps the declared label.
    let f = chart("flowchart TD\nA[Start] --> B\nB --> A");
    assert_eq!(node(&f, "A").label, "Start");
}

#[test]
fn nodes_keep_first_appearance_order() {
    let f = chart("flowchart TD\nC --> A\nB --> C\nA --> D");
    let ids: Vec<_> = f.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["C", "A", "B", "D"]);
}

#[test]
fn node_spans_use_scalar_columns() {
    let src = "flowchart TD\n  héllo[x] --> B[y]";
    let f = chart(src);
    let h = node(&f, "héllo");
    assert_eq!((h.span.line, h.span.column), (2, 3));
    assert_eq!((h.span.byte_start, h.span.byte_end), (15, 24));
    let b = node(&f, "B");
    assert_eq!((b.span.line, b.span.column), (2, 16));
    assert_eq!(
        &src[b.span.byte_start as usize..b.span.byte_end as usize],
        "B[y]"
    );
    let e = only_edge(&f);
    assert_eq!(e.span.line, 2);
    assert_eq!(
        &src[e.span.byte_start as usize..e.span.byte_end as usize],
        "héllo[x] --> B[y]"
    );
}

// ---------------------------------------------------------------- edges

#[test]
fn edge_tokens() {
    use Arrow::{Arrow as Ar, Circle as Ci, Cross as Cr, None as No};
    let cases = [
        ("-->", Stroke::Normal, No, Ar, 1),
        ("--->", Stroke::Normal, No, Ar, 2),
        ("---->", Stroke::Normal, No, Ar, 3),
        ("---", Stroke::Normal, No, No, 1),
        ("----", Stroke::Normal, No, No, 2),
        ("==>", Stroke::Thick, No, Ar, 1),
        ("===>", Stroke::Thick, No, Ar, 2),
        ("===", Stroke::Thick, No, No, 1),
        ("====", Stroke::Thick, No, No, 2),
        ("-.->", Stroke::Dotted, No, Ar, 1),
        ("-..->", Stroke::Dotted, No, Ar, 2),
        ("-.-", Stroke::Dotted, No, No, 1),
        ("-..-", Stroke::Dotted, No, No, 2),
        ("~~~", Stroke::Invisible, No, No, 1),
        ("~~~~", Stroke::Invisible, No, No, 2),
        ("--o", Stroke::Normal, No, Ci, 1),
        ("--x", Stroke::Normal, No, Cr, 1),
        ("<-->", Stroke::Normal, Ar, Ar, 1),
        ("o--o", Stroke::Normal, Ci, Ci, 1),
        ("x--x", Stroke::Normal, Cr, Cr, 1),
        ("<==>", Stroke::Thick, Ar, Ar, 1),
        ("<-.->", Stroke::Dotted, Ar, Ar, 1),
        ("-.-x", Stroke::Dotted, No, Cr, 1),
        ("==o", Stroke::Thick, No, Ci, 1),
        ("o==o", Stroke::Thick, Ci, Ci, 1),
    ];
    for (tok, stroke, start, end, len) in cases {
        for src in [
            format!("flowchart LR\nA {tok} B"),
            format!("flowchart LR\nA{tok}B"),
        ] {
            // `A o--o B` needs the space before `o`; skip the unspaced variant for it.
            if src.contains("Ao") || src.contains("Ax") {
                continue;
            }
            let f = chart(&src);
            let e = only_edge(&f);
            assert_eq!(
                (e.stroke, e.arrow_start, e.arrow_end, e.min_len),
                (stroke, start, end, len),
                "{src:?}"
            );
            assert_eq!(edge_ids(&f), vec![("A".into(), "B".into())], "{src:?}");
            assert_eq!(e.label, None);
        }
    }
}

#[test]
fn edge_labels() {
    let cases = [
        (
            "A-- text -->B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A -- text --> B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A-- text --->B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            2,
        ),
        (
            "A-- text ---B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::None,
            1,
        ),
        (
            "A-- text --xB",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::Cross,
            1,
        ),
        (
            "A== text ==>B",
            "text",
            Stroke::Thick,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A== text ===B",
            "text",
            Stroke::Thick,
            Arrow::None,
            Arrow::None,
            1,
        ),
        (
            "A-. text .->B",
            "text",
            Stroke::Dotted,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A-. text ..->B",
            "text",
            Stroke::Dotted,
            Arrow::None,
            Arrow::Arrow,
            2,
        ),
        (
            "A-. text .-B",
            "text",
            Stroke::Dotted,
            Arrow::None,
            Arrow::None,
            1,
        ),
        (
            "A-->|text|B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A --> |text| B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A-->| padded |B",
            "padded",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A---|text|B",
            "text",
            Stroke::Normal,
            Arrow::None,
            Arrow::None,
            1,
        ),
        (
            "A-.->|text|B",
            "text",
            Stroke::Dotted,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A==>|text|B",
            "text",
            Stroke::Thick,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A~~~|text|B",
            "text",
            Stroke::Invisible,
            Arrow::None,
            Arrow::None,
            1,
        ),
        (
            "A -->|\"quoted (x)\"| B",
            "quoted (x)",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A-- \"q -> x\" -->B",
            "q -> x",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A <-- both --> B",
            "both",
            Stroke::Normal,
            Arrow::Arrow,
            Arrow::Arrow,
            1,
        ),
        (
            "A-- a -- b -->B",
            "a -- b",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A-->|a #quot;b#quot;|B",
            "a \"b\"",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
        (
            "A-->|\"`**md**`\"|B",
            "**md**",
            Stroke::Normal,
            Arrow::None,
            Arrow::Arrow,
            1,
        ),
    ];
    for (stmt, label, stroke, start, end, len) in cases {
        let f = chart(&format!("flowchart LR\n{stmt}"));
        let e = only_edge(&f);
        assert_eq!(e.label.as_deref(), Some(label), "{stmt}");
        assert_eq!(
            (e.stroke, e.arrow_start, e.arrow_end, e.min_len),
            (stroke, start, end, len),
            "{stmt}"
        );
        assert_eq!(edge_ids(&f), vec![("A".into(), "B".into())], "{stmt}");
    }
}

#[test]
fn empty_pipe_label_is_no_label() {
    let f = chart("flowchart LR\nA-->||B");
    assert_eq!(only_edge(&f).label, None);
}

#[test]
fn chains() {
    let f = chart("flowchart LR\nA --> B --> C -.-> D");
    assert_eq!(
        edge_ids(&f),
        vec![
            ("A".into(), "B".into()),
            ("B".into(), "C".into()),
            ("C".into(), "D".into())
        ]
    );
    assert_eq!(f.edges[2].stroke, Stroke::Dotted);
    let f = chart("flowchart LR\nA -- one --> B -->|two| C");
    let labels: Vec<_> = f.edges.iter().map(|e| e.label.clone()).collect();
    assert_eq!(labels, vec![Some("one".into()), Some("two".into())]);
}

#[test]
fn ampersand_groups() {
    let f = chart("flowchart TD\nA & B --> C & D");
    assert_eq!(
        edge_ids(&f),
        vec![
            ("A".into(), "C".into()),
            ("A".into(), "D".into()),
            ("B".into(), "C".into()),
            ("B".into(), "D".into())
        ]
    );
    let f = chart("flowchart TD\nA --> B & C[See] --> D");
    assert_eq!(
        edge_ids(&f),
        vec![
            ("A".into(), "B".into()),
            ("A".into(), "C".into()),
            ("B".into(), "D".into()),
            ("C".into(), "D".into())
        ]
    );
    assert_eq!(node(&f, "C").label, "See");
    // A standalone group declares its nodes.
    let (f, d) = parse_ok("flowchart TD\nA & B");
    assert_eq!(f.nodes.len(), 2);
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn edge_ids_are_accepted_and_ignored() {
    let (f, d) = parse_ok("flowchart LR\nA[a] e1@--> B[b]\ne1@{ animate: true }\nA e2@==> B");
    assert_eq!(f.nodes.len(), 2, "{:?}", f.nodes);
    assert_eq!(f.edges.len(), 2);
    assert_eq!(f.edges[1].stroke, Stroke::Thick);
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn circle_and_cross_heads_are_greedy() {
    // Mermaid documents this pitfall: `dev---ops` is a circle edge to `ps`.
    let f = chart("flowchart LR\ndev---ops");
    assert_eq!(edge_ids(&f), vec![("dev".into(), "ps".into())]);
    assert_eq!(only_edge(&f).arrow_end, Arrow::Circle);
    let f = chart("flowchart LR\ndev--- ops");
    assert_eq!(edge_ids(&f), vec![("dev".into(), "ops".into())]);
    // An id ending in `o` before a link keeps its `o`.
    let f = chart("flowchart LR\ninfo-->B");
    assert_eq!(edge_ids(&f), vec![("info".into(), "B".into())]);
}

#[test]
fn separators_and_comments() {
    let f = chart("flowchart LR;A-->B;B-->C;\n%% a comment\nC-->D %% trailing\n  ;;\nD-->E");
    assert_eq!(f.edges.len(), 4);
    assert_eq!(f.nodes.len(), 5);
}

#[test]
fn crlf_line_endings() {
    let f = chart("flowchart TD\r\nA[a] --> B[b]\r\nsubgraph S\r\nC[c]\r\nend\r\n");
    assert_eq!(f.nodes.len(), 3);
    assert_eq!(f.subgraphs.len(), 1);
    assert_eq!(node(&f, "B").label, "b");
}

// ---------------------------------------------------------------- subgraphs

#[test]
fn subgraph_headers() {
    let f = chart(
        "flowchart TB\nsubgraph one [First One]\n a1\nend\nsubgraph \"Quoted Title\"\n a2\nend\n\
         subgraph three\n a3\nend\nsubgraph Many words here\n a4\nend\nsubgraph five[\"Q (x)\"]\n a5\nend",
    );
    let got: Vec<_> = f
        .subgraphs
        .iter()
        .map(|s| (s.id.as_str(), s.title.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("one", "First One"),
            ("subGraph1", "Quoted Title"),
            ("three", "three"),
            ("subGraph3", "Many words here"),
            ("five", "Q (x)"),
        ]
    );
}

#[test]
fn nested_subgraphs_and_membership() {
    let f = chart(
        "flowchart TB\nc1 --> a2\nsubgraph outer\n  a1 --> a2\n  subgraph inner\n    direction LR\n    b1 --> b2\n  end\n  a1 --> b1\nend\nc2",
    );
    assert_eq!(f.subgraphs.len(), 2);
    let outer = &f.subgraphs[0];
    let inner = &f.subgraphs[1];
    assert_eq!(outer.parent, None);
    assert_eq!(inner.parent, Some(0));
    assert_eq!(inner.direction, Some(Direction::LR));
    assert_eq!(outer.direction, None);
    let members = |s: &merlion_render::model::Subgraph| -> Vec<String> {
        s.nodes.iter().map(|&i| id(&f, i)).collect()
    };
    // `a2` first appears outside, then inside `outer`: it moves into `outer`.
    // `b1` is mentioned in `outer` after `inner` closed and stays in `inner`.
    assert_eq!(members(outer), vec!["a2", "a1"]);
    assert_eq!(members(inner), vec!["b1", "b2"]);
    assert_eq!(node(&f, "c1").subgraph, None);
    assert_eq!(node(&f, "c2").subgraph, None);
    assert_eq!(node(&f, "b2").subgraph, Some(1));
}

#[test]
fn inner_subgraph_claims_nodes_from_open_parent() {
    let f = chart("flowchart TB\nsubgraph P\n  x\n  subgraph C\n    x\n  end\nend");
    assert_eq!(node(&f, "x").subgraph, Some(1));
    assert!(f.subgraphs[0].nodes.is_empty());
}

#[test]
fn first_closed_sibling_keeps_its_nodes() {
    let f = chart("flowchart TB\nsubgraph X\n  a\nend\nsubgraph Y\n  a --> b\nend");
    assert_eq!(node(&f, "a").subgraph, Some(0));
    assert_eq!(node(&f, "b").subgraph, Some(1));
}

#[test]
fn edges_to_subgraph_ids_map_to_the_first_member() {
    let f = chart("flowchart LR\nsubgraph S1\n  a --> b\nend\nsubgraph S2\n  subgraph S3\n    c\n  end\nend\nstart --> S1\nS1 --> S2\nS2 --> fin");
    let ids: Vec<_> = f.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(!ids.contains(&"S1") && !ids.contains(&"S2"), "{ids:?}");
    assert_eq!(
        edge_ids(&f),
        vec![
            ("a".into(), "b".into()),
            ("start".into(), "a".into()),
            ("a".into(), "c".into()),
            ("c".into(), "fin".into())
        ]
    );
}

#[test]
fn subgraph_endpoints_never_get_r005() {
    // Referenced before the subgraph is declared: still maps to its first member.
    let (f, d) = parse_ok("flowchart TD\nx[X] --> S\nsubgraph S\n  b\nend");
    assert!(!has(&d, "R005"), "{d:?}");
    assert_eq!(edge_ids(&f), vec![("x".into(), "b".into())]);
    // An empty subgraph has no member to stand in for it; the endpoint stays a node.
    let (f, d) = parse_ok("flowchart TD\nsubgraph E\nend\nx[X] --> E");
    assert!(!has(&d, "R005"), "{d:?}");
    assert_eq!(edge_ids(&f), vec![("x".into(), "E".into())]);
}

#[test]
fn top_level_direction_statement() {
    let f = chart("flowchart TB\ndirection RL\nA-->B");
    assert_eq!(f.direction, Direction::RL);
}

#[test]
fn stray_end_is_a_syntax_error() {
    let (r, d) = try_parse("flowchart TD\nA-->B\nend");
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E002"]);
    assert_eq!(d[0].span.line, 3);
}

#[test]
fn nesting_limit() {
    let deep = |n: usize| {
        let mut s = String::from("flowchart TD\n");
        for i in 0..n {
            s.push_str(&format!("subgraph s{i}\n"));
        }
        s.push_str("x\n");
        for _ in 0..n {
            s.push_str("end\n");
        }
        s
    };
    let (f, _) = parse_ok(&deep(64));
    assert_eq!(f.subgraphs.len(), 64);
    let (r, d) = try_parse(&deep(65));
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E010"]);
    assert_eq!(d[0].span.line, 66);
    // Far deeper input fails the same way without exhausting the stack.
    let (r, d) = try_parse(&deep(40_000));
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E010"]);
}

// ---------------------------------------------------------------- styles

#[test]
fn class_defs_and_classes() {
    let (f, d) = parse_ok(
        "flowchart TD\nA[a]:::hot --> B[b]\nclassDef hot fill:#f96,stroke-width:2px\nclassDef cold,calm stroke:blue;\nclass A,B cold\nclass B calm",
    );
    assert!(d.iter().all(|x| x.code == "I030"), "{d:#?}");
    let names: Vec<_> = f.class_defs.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["hot", "cold", "calm"]);
    assert_eq!(
        f.class_defs[0].style.fill,
        Some(Color::Rgba {
            r: 0xff,
            g: 0x99,
            b: 0x66,
            a: 255
        })
    );
    assert_eq!(f.class_defs[0].style.stroke_width, Some(2.0));
    assert_eq!(f.class_defs[1].style.stroke, Some(Color::Named("blue")));
    assert_eq!(
        node(&f, "A").classes,
        vec!["hot".to_string(), "cold".to_string()]
    );
    assert_eq!(
        node(&f, "B").classes,
        vec!["cold".to_string(), "calm".to_string()]
    );
}

#[test]
fn class_def_default_and_merging() {
    let f = chart(
        "flowchart TD\nA\nclassDef default font-weight:bold\nclassDef default font-style:italic",
    );
    assert_eq!(f.class_defs.len(), 1);
    assert_eq!(f.class_defs[0].name, "default");
    assert!(f.class_defs[0].style.font_weight.is_some());
    assert!(f.class_defs[0].style.font_style.is_some());
}

#[test]
fn invalid_class_names_are_rejected() {
    let long = "a".repeat(65);
    let src = format!("flowchart TD\nA:::ok\nclassDef 1bad fill:red\nclassDef {long} fill:red\nclass A b@d\nB:::-x");
    let (f, d) = parse_ok(&src);
    assert_eq!(count(&d, "W011"), 4, "{d:#?}");
    assert!(f.class_defs.is_empty());
    assert_eq!(node(&f, "A").classes, vec!["ok".to_string()]);
    assert!(node(&f, "B").classes.is_empty());
}

#[test]
fn style_statements() {
    let (f, d) = parse_ok("flowchart TD\nA[a]\nstyle A fill:#f9f,stroke:#333,stroke-width:4px,font-size:30px\nstyle A color:red");
    let s = &node(&f, "A").style;
    assert!(s.fill.is_some() && s.stroke.is_some() && s.color.is_some());
    assert_eq!(s.stroke_width, Some(4.0));
    assert_eq!(count(&d, "W010"), 1, "{d:#?}");
    // I030 once per diagram.
    assert_eq!(count(&d, "I030"), 1);
    let w = d.iter().find(|x| x.code == "W010").unwrap();
    assert_eq!(w.span.line, 3);
    assert!(w.message.contains("font-size"));
}

#[test]
fn style_targets() {
    let (f, d) = parse_ok("flowchart TD\nsubgraph S\n  A[a]\nend\nstyle S fill:none\nstyle Missing fill:none\nclass Missing x\nclass S x");
    assert_eq!(count(&d, "W010"), 1, "{d:#?}");
    assert!(d
        .iter()
        .find(|x| x.code == "W010")
        .unwrap()
        .message
        .contains("Missing"));
    assert_eq!(f.nodes.len(), 1);
}

#[test]
fn link_styles() {
    let (f, d) = parse_ok(
        "flowchart LR\nA[a] --> B[b] --> C[c] --> D[d]\nlinkStyle default stroke:gray\nlinkStyle 0,2 stroke:red,stroke-width:3px\nlinkStyle 1 interpolate basis stroke-dasharray:4 2\nlinkStyle 7 stroke:blue",
    );
    assert_eq!(f.default_link_style.stroke, Some(Color::Named("gray")));
    assert_eq!(f.edges[0].style.stroke, Some(Color::Named("red")));
    assert_eq!(f.edges[0].style.stroke_width, Some(3.0));
    assert_eq!(f.edges[1].style.stroke, Some(Color::Named("gray")));
    assert_eq!(f.edges[1].style.stroke_dasharray, Some(vec![4.0, 2.0]));
    assert_eq!(f.edges[2].style.stroke, Some(Color::Named("red")));
    let w: Vec<_> = d.iter().filter(|x| x.code == "W010").collect();
    assert_eq!(w.len(), 1, "{d:#?}");
    assert!(w[0].message.contains('7'));
}

#[test]
fn link_style_syntax_errors() {
    for src in [
        "flowchart LR\nA-->B\nlinkStyle x stroke:red",
        "flowchart LR\nA-->B\nlinkStyle",
    ] {
        let (r, d) = try_parse(src);
        assert_eq!(r, Err(ParseError::Failed), "{src:?}");
        assert_eq!(codes(&d), vec!["E002"], "{src:?}");
    }
}

// ---------------------------------------------------------------- click

#[test]
fn click_links() {
    let cases = [
        (
            "click A \"https://example.com\"",
            "https://example.com",
            false,
        ),
        (
            "click A href \"https://example.com\"",
            "https://example.com",
            false,
        ),
        (
            "click A href \"https://example.com\" _blank",
            "https://example.com",
            true,
        ),
        (
            "click A href \"https://example.com\" \"Tooltip\" _blank",
            "https://example.com",
            true,
        ),
        ("click A \"/docs\" \"Tip\" _self", "/docs", false),
        (
            "click A href https://example.com/bare _blank",
            "https://example.com/bare",
            true,
        ),
        (
            "click A \"mailto:a@example.com\" _top",
            "mailto:a@example.com",
            false,
        ),
    ];
    for (stmt, url, blank) in cases {
        let (f, d) = parse_ok(&format!("flowchart TD\nA[a]\n{stmt}"));
        assert!(d.is_empty(), "{stmt}: {d:#?}");
        let link = node(&f, "A")
            .link
            .clone()
            .unwrap_or_else(|| panic!("{stmt}: no link"));
        assert_eq!(
            (link.url.as_str(), link.target_blank),
            (url, blank),
            "{stmt}"
        );
    }
}

#[test]
fn click_rejected_urls() {
    for url in [
        "javascript:alert(1)",
        "data:text/html,x",
        " JaVaScRiPt:x",
        "vbscript:x",
    ] {
        let (f, d) = parse_ok(&format!("flowchart TD\nA[a]\nclick A href \"{url}\""));
        assert_eq!(codes(&d), vec!["W013"], "{url}");
        assert_eq!(d[0].span.line, 3);
        assert!(node(&f, "A").link.is_none());
    }
}

#[test]
fn click_callbacks_are_ignored() {
    for stmt in [
        "click A callback",
        "click A callback \"Tooltip\"",
        "click A call callback()",
        "click A call callback(\"arg\", 2)",
    ] {
        let (f, d) = parse_ok(&format!("flowchart TD\nA[a]\n{stmt}"));
        assert_eq!(codes(&d), vec!["I031"], "{stmt}");
        assert_eq!(d[0].severity, Severity::Info);
        assert!(node(&f, "A").link.is_none());
    }
}

// ---------------------------------------------------------------- accessibility

#[test]
fn acc_title_and_descr() {
    let f = chart("flowchart TD\naccTitle: My title; with semicolon\naccDescr: One line\nA[a]");
    assert_eq!(
        f.meta.acc_title.as_deref(),
        Some("My title; with semicolon")
    );
    assert_eq!(f.meta.acc_descr.as_deref(), Some("One line"));
    let f = chart("flowchart TD\naccDescr {\n  First line\n  second line\n}\nA[a]");
    assert_eq!(f.meta.acc_descr.as_deref(), Some("First line\nsecond line"));
    let f = chart("flowchart TD\naccDescr{ inline block }\nA[a]");
    assert_eq!(f.meta.acc_descr.as_deref(), Some("inline block"));
}

#[test]
fn unterminated_acc_descr_block() {
    let (r, d) = try_parse("flowchart TD\naccDescr {\n  never closed\nA[a]");
    assert_eq!(r, Err(ParseError::Failed));
    assert_eq!(codes(&d), vec!["E002"]);
}

// ---------------------------------------------------------------- errors

#[test]
fn syntax_errors_report_location_and_expectation() {
    let cases = [
        ("flowchart TD\nA-->", 2, "expected a node id"),
        ("flowchart TD\nA--B", 2, "expected the end of the link"),
        ("flowchart TD\nA[unclosed --> B", 2, "expected `]`"),
        (
            "flowchart TD\nA(\"unterminated) --> B",
            2,
            "unterminated string",
        ),
        ("flowchart TD\nA --> B C", 2, "expected"),
        ("flowchart TD\nA -->|no close B", 2, "expected `|`"),
        ("flowchart TD\n\n  --> B", 3, "expected a node id"),
        ("flowchart TD\nA & --> B", 2, "expected a node id"),
        ("flowchart TD\nA[\"q\" tail]", 2, "expected `]`"),
        (
            "flowchart TD\nsubgraph\nend",
            2,
            "expected a subgraph id or title",
        ),
        ("flowchart TD\ndirection UP", 2, "expected a direction"),
        ("flowchart TD\nA@{ shape: rect", 2, "expected `}`"),
        ("flowchart TD\nclick", 2, "expected a node id"),
        ("flowchart TD\nstyle", 2, "expected a node id"),
        ("flowchart TD\nclassDef", 2, "expected a class name"),
        ("flowchart TD\nclass A", 2, "expected a class name"),
    ];
    for (src, line, needle) in cases {
        let (r, d) = try_parse(src);
        assert_eq!(r, Err(ParseError::Failed), "{src:?}");
        let e = errors(&d);
        assert_eq!(e.len(), 1, "{src:?}: {d:#?}");
        assert_eq!(e[0].code, "E002", "{src:?}");
        assert_eq!(e[0].span.line, line, "{src:?}: {:?}", e[0]);
        assert!(e[0].message.contains(needle), "{src:?}: {}", e[0].message);
    }
}

// ---------------------------------------------------------------- limits

#[test]
fn node_limit() {
    let limits = Limits {
        nodes: 3,
        ..Limits::default()
    };
    let (r, _) = run("flowchart TD\nA-->B-->C", false, limits);
    assert!(r.is_ok());
    let (r, _) = run("flowchart TD\nA-->B-->C-->D", false, limits);
    assert_eq!(r, Err(ParseError::TooLarge { what: "nodes" }));
}

#[test]
fn edge_limit_bounds_group_products() {
    let limits = Limits {
        edges: 5,
        ..Limits::default()
    };
    let (r, _) = run("flowchart TD\nA & B --> C & D", false, limits);
    assert!(r.is_ok());
    let (r, _) = run("flowchart TD\nA & B & C --> D & E", false, limits);
    assert_eq!(r, Err(ParseError::TooLarge { what: "edges" }));
    // A quadratic `&` product hits the default limit instead of allocating millions of edges.
    let group: Vec<String> = (0..300).map(|i| format!("n{i}")).collect();
    let g = group.join(" & ");
    let (r, _) = try_parse(&format!("flowchart TD\n{g} --> {g}"));
    assert_eq!(r, Err(ParseError::TooLarge { what: "edges" }));
}

#[test]
fn input_limit() {
    let limits = Limits {
        input_bytes: 16,
        ..Limits::default()
    };
    let (r, _) = run("flowchart TD\nA-->B-->C", false, limits);
    assert_eq!(r, Err(ParseError::TooLarge { what: "input" }));
}

#[test]
fn label_truncation_at_char_boundary() {
    let limits = Limits {
        label_bytes: 5,
        ..Limits::default()
    };
    let (r, d) = run(
        "flowchart TD\nA[abcdé] -->|long label| B[ok]",
        false,
        limits,
    );
    let f = match r {
        Ok(merlion_render::model::Diagram::Flowchart(f)) => f,
        other => panic!("{other:?}"),
    };
    // `é` straddles byte 5, so the cut falls before it.
    assert_eq!(node(&f, "A").label, "abcd");
    assert_eq!(f.edges[0].label.as_deref(), Some("long "));
    assert_eq!(node(&f, "B").label, "ok");
    assert_eq!(count(&d, "W012"), 2, "{d:#?}");
    assert_eq!(d.iter().find(|x| x.code == "W012").unwrap().span.line, 2);
}

// ---------------------------------------------------------------- strict

#[test]
fn strict_mode_fails_on_warnings_and_repairs() {
    for src in [
        "flowchart TD\nA[a]\nstyle A font-size:3px",
        "flowchart TD\nA[a] --> B",
        "flowchart TD\nA[Call (x)]",
        "flowchart TD\nA[a]\nclick A href \"javascript:x\"",
    ] {
        let (r, d) = run(src, true, Limits::default());
        assert_eq!(r, Err(ParseError::Failed), "{src:?}");
        assert!(!errors(&d).is_empty());
        let (r, _) = run(src, false, Limits::default());
        assert!(r.is_ok(), "{src:?}");
    }
    // Infos stay infos under strict.
    let (r, d) = run("flowchart TD\nA[a]\nclick A cb", true, Limits::default());
    assert!(r.is_ok());
    assert_eq!(d[0].severity, Severity::Info);
}

// ---------------------------------------------------------------- robustness

/// A deterministic byte soup: parsing must never panic, whatever the input.
#[test]
fn never_panics_on_mutated_input() {
    let seeds = [
        "flowchart LR\nA[\"a\"] -- t --> B{b} -.->|x| C((c))\nsubgraph S [T]\n direction TB\n C --> D>d]\nend\nclassDef k fill:#fff\nclass A k\nstyle B stroke:red\nlinkStyle 0 stroke:blue\nclick A href \"https://x\" _blank\n",
        "---\ntitle: t\nconfig:\n  flowchart:\n    curve: basis\n---\n%%{init: {'theme':'dark'}}%%\ngraph TD\nA@{shape: diam, label: \"x\"} o--o B\naccDescr {\n x\n}\n",
        "```mermaid\nflowchart TD\nA[“q”] --> end\nsubgraph X\nB[f(x)]\n```",
    ];
    let alphabet: Vec<char> = "[](){}<>|\"'`-=.~ox&:;@#%\n\t\\/ aZ9é“”".chars().collect();
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for seed in seeds {
        // Every prefix, on character boundaries.
        for (i, _) in seed.char_indices() {
            let _ = try_parse(&seed[..i]);
        }
        // Random single-character mutations.
        let chars: Vec<char> = seed.chars().collect();
        for _ in 0..3000 {
            let mut c = chars.clone();
            for _ in 0..1 + (next() % 4) as usize {
                let pos = (next() as usize) % c.len();
                let ch = alphabet[(next() as usize) % alphabet.len()];
                match next() % 3 {
                    0 => c[pos] = ch,
                    1 => c.insert(pos, ch),
                    _ => {
                        c.remove(pos);
                    }
                }
            }
            let s: String = c.into_iter().collect();
            let (_, _) = try_parse(&s);
            let (_, _) = run(&s, true, Limits::default());
        }
    }
}

/// Inputs near the 1 MiB limit on a single line: every per-statement scan must stay
/// bounded by what it consumes, or these take minutes instead of milliseconds.
#[test]
fn megabyte_single_lines_parse_in_linear_time() {
    let target = 1_000_000;
    let fill = |unit: &str, head: &str| {
        let mut s = String::from(head);
        while s.len() + unit.len() < target {
            s.push_str(unit);
        }
        s
    };
    let cases = [
        fill("style A fill:red;", "flowchart LR;A[a];"),
        fill("A;", "flowchart LR;"),
        fill("classDef c stroke:blue;", "flowchart LR;"),
        fill("%%{init: {}}%% ", ""),
        fill("linkStyle default stroke:red;", "flowchart LR;A-->B;"),
    ];
    for (i, src) in cases.iter().enumerate() {
        let src = if i == 3 {
            format!("{src}\nflowchart LR\nA[a]")
        } else {
            src.clone()
        };
        let (r, _) = try_parse(&src);
        assert!(r.is_ok(), "case {i}: {r:?}");
    }
    // 1,999 labelled nodes and edges on one line.
    let label = "x".repeat(450);
    let mut s = String::from("flowchart LR;");
    for i in 0..1_999 {
        s.push_str(&format!("n{i}[{label}] --> "));
    }
    s.push_str("last[x]");
    assert!(s.len() < target);
    let (f, _) = parse_ok(&s);
    assert_eq!(f.edges.len(), 1_999);
    // Long text-edge labels and pipe labels.
    let mut s = String::from("flowchart LR;");
    for i in 0..450 {
        s.push_str(&format!("a{i} -- {label} --> b{i};c{i} -->|{label}| d{i};"));
    }
    let (f, _) = parse_ok(&s);
    assert_eq!(f.edges.len(), 900);
}

#[test]
fn long_single_line_input_stays_fast() {
    // 2,000 nodes on one line with unbalanced brackets exercise every fallback scan.
    let mut s = String::from("flowchart LR\n");
    for i in 0..1_999 {
        s.push_str(&format!("n{i}[a ( b] --> "));
    }
    s.push_str("last[x]");
    let (f, d) = parse_ok(&s);
    assert_eq!(f.nodes.len(), 2_000);
    assert_eq!(count(&d, "R001"), 1_999);
}
