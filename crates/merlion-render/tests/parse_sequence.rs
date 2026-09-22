//! The `sequenceDiagram` parser (specs/sequence.md#syntax, #diagnostics): one case per
//! statement form, the diagnostics `W021`–`W023` and `E002`–`E012`, and a repair case
//! per `R009`–`R013` whose fix reparses clean.

mod parse_support;

use merlion_render::diag::{Diagnostic, Severity};
use merlion_render::model::sequence::{
    Central, FragmentKind, Head, Item, Message, MessageLine, Note, Participant, ParticipantKind,
    Placement, Sequence,
};
use merlion_render::model::{Color, Diagram};
use merlion_render::options::Limits;
use merlion_render::parse::ParseError;
use parse_support::{codes, count, errors, fixed, has, run, try_parse};

// ----------------------------------------------------------------- helpers

/// Parses `src` and returns the sequence with every diagnostic.
fn seq_d(src: &str) -> (Sequence, Vec<Diagnostic>) {
    let (r, d) = try_parse(src);
    match r {
        Ok(Diagram::Sequence(s)) => (s, d),
        Ok(other) => panic!("expected a sequence, got a {}", other.type_name()),
        Err(e) => panic!("parse failed for {src:?}: {e:?}\n{d:#?}"),
    }
}

/// Parses `src` and asserts that it carries no diagnostic at all.
fn seq(src: &str) -> Sequence {
    let (s, d) = seq_d(src);
    assert!(d.is_empty(), "{src:?}: unexpected diagnostics {d:#?}");
    s
}

/// `sequenceDiagram` followed by `body`, indented as a hand-written diagram is.
fn body(body: &str) -> String {
    let mut s = String::from("sequenceDiagram\n");
    for line in body.lines() {
        s.push_str("    ");
        s.push_str(line);
        s.push('\n');
    }
    s
}

fn sq(b: &str) -> Sequence {
    seq(&body(b))
}

fn sq_d(b: &str) -> (Sequence, Vec<Diagnostic>) {
    seq_d(&body(b))
}

fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Item>) {
    for it in items {
        out.push(it);
        if let Item::Fragment(f) = it {
            for s in &f.sections {
                walk(&s.items, out);
            }
        }
    }
}

/// Every item in pre-order, which is source order.
fn items(s: &Sequence) -> Vec<&Item> {
    let mut out = Vec::new();
    walk(&s.items, &mut out);
    out
}

fn messages(s: &Sequence) -> Vec<&Message> {
    items(s)
        .into_iter()
        .filter_map(|i| match i {
            Item::Message(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn notes(s: &Sequence) -> Vec<&Note> {
    items(s)
        .into_iter()
        .filter_map(|i| match i {
            Item::Note(n) => Some(n),
            _ => None,
        })
        .collect()
}

fn fragments(s: &Sequence) -> Vec<&merlion_render::model::sequence::Fragment> {
    items(s)
        .into_iter()
        .filter_map(|i| match i {
            Item::Fragment(f) => Some(f),
            _ => None,
        })
        .collect()
}

fn only_message(s: &Sequence) -> &Message {
    let m = messages(s);
    assert_eq!(m.len(), 1, "{:#?}", s.items);
    m[0]
}

fn id(s: &Sequence, i: usize) -> &str {
    s.participants.get(i).map(|p| p.id.as_str()).unwrap_or("")
}

fn ids(s: &Sequence) -> Vec<&str> {
    s.participants.iter().map(|p| p.id.as_str()).collect()
}

fn labels(s: &Sequence) -> Vec<&str> {
    s.participants.iter().map(|p| p.label.as_str()).collect()
}

fn pair<'a>(s: &'a Sequence, m: &Message) -> (&'a str, &'a str) {
    (id(s, m.from), id(s, m.to))
}

fn find<'a>(s: &'a Sequence, id: &str) -> &'a Participant {
    s.participants
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no participant {id:?} in {:?}", ids(s)))
}

/// Applies every fix, reparses, and asserts that `codes` are gone and nothing errors.
fn assert_fix_round_trip(src: &str, repair_codes: &[&str]) -> String {
    let (_, d) = seq_d(src);
    for c in repair_codes {
        let found: Vec<_> = d.iter().filter(|x| x.code == *c).collect();
        assert!(
            !found.is_empty(),
            "{src:?}: expected {c}, got {:?}",
            codes(&d)
        );
        for x in found {
            assert_eq!(x.severity, Severity::Repair, "{x:?}");
            assert!(x.fix.is_some(), "{c} without a fix: {x:?}");
        }
    }
    let out = fixed(src, &d);
    let (_, d2) = seq_d(&out);
    for c in repair_codes {
        assert!(
            !has(&d2, c),
            "{c} remains after fixing into {out:?}: {d2:#?}"
        );
    }
    assert!(errors(&d2).is_empty(), "{out:?}: {d2:#?}");
    out
}

// ----------------------------------------------------------------- header and preamble

#[test]
fn the_header_alone_parses_an_empty_diagram() {
    let s = seq("sequenceDiagram\n");
    assert!(s.participants.is_empty() && s.items.is_empty());
    assert_eq!(s.messages, 0);
}

#[test]
fn front_matter_reaches_the_model() {
    let s = seq("---\ntitle: Ordering\n---\nsequenceDiagram\n    A->>B: hi\n");
    assert_eq!(s.meta.title.as_deref(), Some("Ordering"));
    assert_eq!(s.messages, 1);
}

#[test]
fn an_init_directive_turns_the_automatic_tones_off() {
    let s =
        seq("%%{init: {\"merlion\": {\"autoTone\": false}}}%%\nsequenceDiagram\n    A->>B: hi\n");
    assert_eq!(s.meta.auto_tone, Some(false));
}

#[test]
fn an_init_directive_inside_the_body_applies() {
    let s = sq("A->>B: hi\n%%{init: {\"merlion\": {\"autoTone\": false}}}%%\nB-->>A: ok");
    assert_eq!(s.meta.auto_tone, Some(false));
    assert_eq!(s.messages, 2);
}

#[test]
fn percent_comments_are_skipped() {
    let s = sq("%% the happy path\nA->>B: hi\n%% and back\nB-->>A: ok");
    assert_eq!(s.messages, 2);
}

#[test]
fn a_hash_comment_line_is_skipped() {
    let s = sq("# the happy path\nA->>B: hi");
    assert_eq!(s.messages, 1);
}

#[test]
fn a_code_fence_is_stripped_with_r006() {
    let (s, d) = seq_d("```mermaid\nsequenceDiagram\n    A->>B: hi\n```\n");
    assert_eq!(s.messages, 1);
    assert_eq!(count(&d, "R006"), 2, "{d:#?}");
}

#[test]
fn semicolons_separate_statements() {
    let s = sq("participant A; participant B; A->>B: hi");
    assert_eq!(ids(&s), ["A", "B"]);
    assert_eq!(s.messages, 1);
}

#[test]
fn carriage_returns_parse() {
    let s = seq("sequenceDiagram\r\n    participant A\r\n    A->>B: hi\r\n");
    assert_eq!(ids(&s), ["A", "B"]);
    assert_eq!(only_message(&s).label, "hi");
}

#[test]
fn a_bare_header_with_trailing_space_parses() {
    let s = seq("sequenceDiagram   \n");
    assert!(s.items.is_empty());
}

// ----------------------------------------------------------------- participants

#[test]
fn a_participant_declares_a_column() {
    let s = sq("participant Alice");
    assert_eq!(ids(&s), ["Alice"]);
    assert_eq!(labels(&s), ["Alice"]);
    assert_eq!(find(&s, "Alice").kind, ParticipantKind::Participant);
    assert!(!find(&s, "Alice").implicit);
}

#[test]
fn an_actor_declares_an_actor() {
    let s = sq("actor Bob");
    assert_eq!(find(&s, "Bob").kind, ParticipantKind::Actor);
}

#[test]
fn keywords_are_case_insensitive() {
    let s = sq("PARTICIPANT A\nActor B\nNOTE over A: hi");
    assert_eq!(ids(&s), ["A", "B"]);
    assert_eq!(find(&s, "B").kind, ParticipantKind::Actor);
    assert_eq!(notes(&s).len(), 1);
}

#[test]
fn an_as_alias_sets_the_label() {
    let s = sq("participant A as Alice");
    assert_eq!(ids(&s), ["A"]);
    assert_eq!(labels(&s), ["Alice"]);
}

#[test]
fn an_alias_runs_to_the_end_of_the_line() {
    let s = sq("participant API as The public API gateway");
    assert_eq!(labels(&s), ["The public API gateway"]);
}

#[test]
fn an_alias_keeps_line_breaks() {
    let s = sq("participant API as Public<br/>API");
    assert_eq!(labels(&s), ["Public\nAPI"]);
}

#[test]
fn declaration_order_is_drawing_order() {
    let s = sq("participant C\nparticipant A\nparticipant B\nA->>C: hi");
    assert_eq!(ids(&s), ["C", "A", "B"]);
}

#[test]
fn an_implicit_participant_carries_no_diagnostic() {
    let (s, d) = sq_d("Alice->>John: hi");
    assert_eq!(ids(&s), ["Alice", "John"]);
    assert!(find(&s, "Alice").implicit && find(&s, "John").implicit);
    assert!(d.is_empty(), "{d:#?}");
}

#[test]
fn an_implicit_participant_labels_itself_with_its_id() {
    let s = sq("Alice->>John: hi");
    assert_eq!(labels(&s), ["Alice", "John"]);
}

#[test]
fn an_implicit_participant_keeps_its_first_position() {
    let s = sq("A->>B: hi\nparticipant B as Bob\nC->>A: back");
    assert_eq!(ids(&s), ["A", "B", "C"]);
    assert_eq!(labels(&s), ["A", "Bob", "C"]);
    assert!(!find(&s, "B").implicit);
}

#[test]
fn a_redeclared_participant_keeps_one_column() {
    let s = sq("participant A\nparticipant A\nA->>B: hi");
    assert_eq!(ids(&s), ["A", "B"]);
}

#[test]
fn entity_codes_decode_in_ids_and_labels() {
    let s = sq("participant A#35;1 as Team #hearts;\nA#35;1->>B: #35;1");
    assert_eq!(ids(&s), ["A#1", "B"]);
    assert_eq!(labels(&s), ["Team ♥", "B"]);
    assert_eq!(only_message(&s).label, "#1");
}

#[test]
fn a_hash_comment_ends_a_declaration_line() {
    let s = sq("participant A as Alice  # the buyer\nA->>B: hi");
    assert_eq!(labels(&s), ["Alice", "B"]);
}

#[test]
fn the_at_block_sets_every_kind() {
    for k in ParticipantKind::ALL {
        let src = format!("participant A@{{ \"type\": \"{}\" }}", k.as_str());
        let s = sq(&src);
        assert_eq!(find(&s, "A").kind, k, "{src}");
        assert_eq!(labels(&s), ["A"], "{src}");
    }
}

#[test]
fn the_at_block_alias_sets_the_label() {
    let s = sq("participant API@{ \"type\": \"boundary\", \"alias\": \"Public API\" }");
    assert_eq!(ids(&s), ["API"]);
    assert_eq!(labels(&s), ["Public API"]);
    assert_eq!(find(&s, "API").kind, ParticipantKind::Boundary);
}

#[test]
fn an_external_as_alias_wins_over_an_inline_one() {
    let s = sq("actor DB@{ \"type\": \"database\", \"alias\": \"Inline\" } as User Database");
    assert_eq!(labels(&s), ["User Database"]);
    assert_eq!(find(&s, "DB").kind, ParticipantKind::Database);
}

#[test]
fn an_at_block_type_beats_the_keyword() {
    let s = sq("actor A@{ \"type\": \"queue\" }");
    assert_eq!(find(&s, "A").kind, ParticipantKind::Queue);
    let s = sq("participant A@{ \"type\": \"actor\" }");
    assert_eq!(find(&s, "A").kind, ParticipantKind::Actor);
}

#[test]
fn an_unknown_at_key_warns_w022() {
    let (s, d) = sq_d("participant A@{ \"shape\": \"circle\" }");
    assert_eq!(find(&s, "A").kind, ParticipantKind::Participant);
    assert_eq!(count(&d, "W022"), 1, "{d:#?}");
}

#[test]
fn a_non_string_at_value_warns_w022() {
    let (s, d) = sq_d("participant A@{ \"type\": 3 }");
    assert_eq!(find(&s, "A").kind, ParticipantKind::Participant);
    assert_eq!(count(&d, "W022"), 1, "{d:#?}");
}

#[test]
fn an_unknown_at_type_warns_w022() {
    let (s, d) = sq_d("participant A@{ \"type\": \"robot\" }");
    assert_eq!(find(&s, "A").kind, ParticipantKind::Participant);
    assert_eq!(count(&d, "W022"), 1, "{d:#?}");
}

#[test]
fn a_malformed_at_block_warns_w022() {
    let (s, d) = sq_d("participant A@{ type }");
    assert_eq!(ids(&s), ["A"]);
    assert_eq!(count(&d, "W022"), 1, "{d:#?}");
}

#[test]
fn an_unclosed_at_block_warns_w022() {
    let (s, d) = sq_d("participant A@{ \"type\": \"actor\"\nA->>B: hi");
    assert_eq!(ids(&s), ["A", "B"]);
    assert_eq!(count(&d, "W022"), 1, "{d:#?}");
}

#[test]
fn a_participant_id_may_hold_punctuation() {
    let s = sq("participant auth-service\nauth-service->>db.primary: SELECT 1");
    assert_eq!(ids(&s), ["auth-service", "db.primary"]);
}

#[test]
fn the_participant_limit_is_e004() {
    let mut src = String::from("sequenceDiagram\n");
    for i in 0..40 {
        src.push_str(&format!("    participant P{i}\n"));
    }
    let limits = Limits {
        nodes: 8,
        ..Limits::default()
    };
    let (r, _) = run(&src, false, limits);
    assert!(
        matches!(r, Err(ParseError::TooLarge { what }) if what == "participants"),
        "{r:?}"
    );
}

// ----------------------------------------------------------------- messages and arrows

#[test]
fn every_arrow_token_parses() {
    use Head::*;
    use MessageLine::*;
    let cases: [(&str, MessageLine, Head, Head); 22] = [
        ("->", Solid, Head::None, Head::None),
        ("-->", Dotted, Head::None, Head::None),
        ("->>", Solid, Head::None, Filled),
        ("-->>", Dotted, Head::None, Filled),
        ("<<->>", Solid, Filled, Filled),
        ("<<-->>", Dotted, Filled, Filled),
        ("-x", Solid, Head::None, Cross),
        ("--x", Dotted, Head::None, Cross),
        ("-)", Solid, Head::None, Open),
        ("--)", Dotted, Head::None, Open),
        ("-|\\", Solid, Head::None, HalfTop),
        ("--|\\", Dotted, Head::None, HalfTop),
        ("-|/", Solid, Head::None, HalfBottom),
        ("--|/", Dotted, Head::None, HalfBottom),
        ("-\\\\", Solid, Head::None, StickTop),
        ("--\\\\", Dotted, Head::None, StickTop),
        ("-//", Solid, Head::None, StickBottom),
        ("--//", Dotted, Head::None, StickBottom),
        ("/|-", Solid, HalfTop, Head::None),
        ("/|--", Dotted, HalfTop, Head::None),
        ("//-", Solid, StickTop, Head::None),
        ("\\\\--", Dotted, StickBottom, Head::None),
    ];
    for (tok, line, tail, head) in cases {
        let src = format!("A{tok}B: hi");
        let s = sq(&src);
        let m = only_message(&s);
        assert_eq!((m.line, m.tail, m.head), (line, tail, head), "{src:?}");
        assert_eq!(pair(&s, m), ("A", "B"), "{src:?}");
        assert_eq!(m.label, "hi", "{src:?}");
    }
}

#[test]
fn the_reverse_half_arrows_parse() {
    use Head::*;
    for (tok, line, tail) in [
        ("\\|-", MessageLine::Solid, HalfBottom),
        ("\\|--", MessageLine::Dotted, HalfBottom),
        ("//--", MessageLine::Dotted, StickTop),
        ("\\\\-", MessageLine::Solid, StickBottom),
    ] {
        let src = format!("A{tok}B: hi");
        let s = sq(&src);
        let m = only_message(&s);
        assert_eq!(
            (m.line, m.tail, m.head),
            (line, tail, Head::None),
            "{src:?}"
        );
    }
}

#[test]
fn spaces_around_the_arrow_are_trimmed() {
    let s = sq("Alice ->> John : hi");
    let m = only_message(&s);
    assert_eq!(pair(&s, m), ("Alice", "John"));
    assert_eq!(m.label, "hi");
}

#[test]
fn a_message_without_text_has_an_empty_label() {
    let s = sq("A->>B:");
    assert_eq!(only_message(&s).label, "");
}

#[test]
fn a_message_without_a_colon_and_without_text_parses() {
    let (s, d) = sq_d("A->>B");
    assert_eq!(only_message(&s).label, "");
    assert!(d.is_empty(), "{d:#?}");
}

#[test]
fn the_label_keeps_punctuation() {
    let s = sq("A->>B: GET /v1/x?q=1&r=2 (200 OK)");
    assert_eq!(only_message(&s).label, "GET /v1/x?q=1&r=2 (200 OK)");
}

#[test]
fn an_arrow_inside_the_label_is_text() {
    let s = sq("A->>B: a --> b -> c");
    assert_eq!(only_message(&s).label, "a --> b -> c");
    assert_eq!(s.messages, 1);
}

#[test]
fn a_semicolon_ends_the_message_text() {
    let s = sq("A->>B: hi; B-->>A: ok");
    assert_eq!(s.messages, 2);
    assert_eq!(messages(&s)[0].label, "hi");
    assert_eq!(messages(&s)[1].label, "ok");
}

#[test]
fn an_entity_code_writes_a_literal_semicolon() {
    let s = sq("A->>B: BEGIN#59; COMMIT");
    assert_eq!(only_message(&s).label, "BEGIN; COMMIT");
    assert_eq!(s.messages, 1);
}

#[test]
fn message_indices_count_source_order_across_fragments() {
    let s = sq(
        "A->>B: one\nloop twice\n  B->>A: two\n  alt x\n    A->>B: three\n  end\nend\nB->>A: four",
    );
    let m = messages(&s);
    assert_eq!(
        m.iter()
            .map(|x| (x.index, x.label.as_str()))
            .collect::<Vec<_>>(),
        [(0, "one"), (1, "two"), (2, "three"), (3, "four")]
    );
    assert_eq!(s.messages, 4);
}

#[test]
fn a_self_message_names_one_participant() {
    let s = sq("A->>A: retry");
    let m = only_message(&s);
    assert_eq!(m.from, m.to);
    assert_eq!(ids(&s), ["A"]);
}

#[test]
fn the_activation_suffix_opens_an_activation() {
    let s = sq("A->>+B: call\nB-->>-A: return");
    let m = messages(&s);
    assert!(m[0].activate && !m[0].deactivate);
    assert!(m[1].deactivate && !m[1].activate);
    assert_eq!(pair(&s, m[0]), ("A", "B"));
}

#[test]
fn the_activation_suffix_is_not_part_of_the_target_id() {
    let s = sq("A->>+B: call\nB-->>-A: return");
    assert_eq!(ids(&s), ["A", "B"]);
}

#[test]
fn a_central_target_marker_parses() {
    let s = sq("A->>()B: hi");
    let m = only_message(&s);
    assert_eq!(m.central, Central::Target);
    assert_eq!(pair(&s, m), ("A", "B"));
}

#[test]
fn a_central_source_marker_parses() {
    let s = sq("A()->>B: hi");
    let m = only_message(&s);
    assert_eq!(m.central, Central::Source);
    assert_eq!(pair(&s, m), ("A", "B"));
}

#[test]
fn central_markers_at_both_ends_parse() {
    let s = sq("A()->>()B: hi");
    assert_eq!(only_message(&s).central, Central::Both);
    assert_eq!(ids(&s), ["A", "B"]);
}

#[test]
fn a_plain_message_has_no_central_connection() {
    assert_eq!(sq("A->>B: hi").items.len(), 1);
    assert_eq!(only_message(&sq("A->>B: hi")).central, Central::None);
}

#[test]
fn wrap_and_nowrap_set_the_wrap_flag() {
    assert_eq!(only_message(&sq("A->>B:wrap: hi")).wrap, Some(true));
    assert_eq!(only_message(&sq("A->>B:nowrap: hi")).wrap, Some(false));
    assert_eq!(only_message(&sq("A->>B: hi")).wrap, None);
}

#[test]
fn wrap_is_stripped_from_the_label() {
    assert_eq!(only_message(&sq("A->>B:wrap: hi there")).label, "hi there");
    assert_eq!(only_message(&sq("A->>B:nowrap:hi there")).label, "hi there");
}

#[test]
fn an_alias_takes_the_same_wrap_annotations() {
    let s = sq("participant A as wrap:Extremely long");
    assert_eq!(find(&s, "A").wrap, Some(true));
    assert_eq!(labels(&s), ["Extremely long"]);
    let s = sq("participant A as nowrap: Extremely long");
    assert_eq!(find(&s, "A").wrap, Some(false));
    assert_eq!(labels(&s), ["Extremely long"]);
    let s = sq("participant A as Alice");
    assert_eq!(find(&s, "A").wrap, None);
}

#[test]
fn a_note_takes_the_same_wrap_annotations() {
    let s = sq("Note over A:wrap: hi there");
    assert_eq!(notes(&s)[0].wrap, Some(true));
    assert_eq!(notes(&s)[0].text, "hi there");
    let s = sq("Note right of A:nowrap:hi there");
    assert_eq!(notes(&s)[0].wrap, Some(false));
    assert_eq!(notes(&s)[0].text, "hi there");
    let s = sq("Note over A: hi there");
    assert_eq!(notes(&s)[0].wrap, None);
    assert_eq!(notes(&s)[0].text, "hi there");
    // A space before the keyword leaves it as note text, as it does for a message.
    let s = sq("Note over A: wrap: hi");
    assert_eq!(notes(&s)[0].wrap, None);
    assert_eq!(notes(&s)[0].text, "wrap: hi");
}

#[test]
fn a_label_that_starts_with_the_word_wrap_is_text() {
    let m = sq("A->>B: wrap: the box");
    assert_eq!(only_message(&m).label, "wrap: the box");
    assert_eq!(only_message(&m).wrap, None);
}

#[test]
fn a_long_label_is_truncated_with_w012() {
    let long = "x".repeat(200);
    let limits = Limits {
        label_bytes: 32,
        ..Limits::default()
    };
    let (r, d) = run(
        &format!("sequenceDiagram\n  A->>B: {long}\n"),
        false,
        limits,
    );
    let Ok(Diagram::Sequence(s)) = r else {
        panic!("{r:?}\n{d:#?}")
    };
    assert_eq!(only_message(&s).label.len(), 32);
    assert_eq!(count(&d, "W012"), 1, "{d:#?}");
}

#[test]
fn a_line_without_an_arrow_is_e002() {
    let (r, d) = try_parse("sequenceDiagram\n    Alice sends a message\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert_eq!(codes(&d), ["E002"], "{d:#?}");
}

#[test]
fn a_message_without_a_sender_is_e002() {
    let (r, d) = try_parse("sequenceDiagram\n    ->>B: hi\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert!(has(&d, "E002"), "{d:#?}");
}

#[test]
fn a_message_without_a_target_is_e002() {
    let (r, d) = try_parse("sequenceDiagram\n    A->>: hi\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert!(has(&d, "E002"), "{d:#?}");
}

#[test]
fn the_message_limit_is_e004() {
    let mut src = String::from("sequenceDiagram\n");
    for _ in 0..40 {
        src.push_str("    A->>B: hi\n");
    }
    let limits = Limits {
        edges: 8,
        ..Limits::default()
    };
    let (r, _) = run(&src, false, limits);
    assert!(
        matches!(r, Err(ParseError::TooLarge { what }) if what == "messages"),
        "{r:?}"
    );
}

#[test]
fn a_message_span_covers_its_statement() {
    let src = "sequenceDiagram\n    Alice->>Bob: hi\n";
    let s = seq(src);
    let m = only_message(&s);
    assert_eq!((m.span.line, m.span.column), (2, 5));
    let text = &src[m.span.byte_start as usize..m.span.byte_end as usize];
    assert_eq!(text, "Alice->>Bob: hi");
}

#[test]
fn a_participant_span_covers_its_statement() {
    let src = "sequenceDiagram\n    participant A as Alice\n";
    let s = seq(src);
    let p = find(&s, "A");
    assert_eq!((p.span.line, p.span.column), (2, 5));
    assert_eq!(
        &src[p.span.byte_start as usize..p.span.byte_end as usize],
        "participant A as Alice"
    );
}

#[test]
fn an_implicit_participant_span_points_at_its_first_use() {
    let src = "sequenceDiagram\n    A->>Bob: hi\n";
    let s = seq(src);
    let p = find(&s, "Bob");
    assert_eq!(
        &src[p.span.byte_start as usize..p.span.byte_end as usize],
        "Bob"
    );
}

// ----------------------------------------------------------------- notes

#[test]
fn a_note_right_of_one_participant_parses() {
    let s = sq("participant John\nNote right of John: Text in note");
    let n = notes(&s)[0];
    assert_eq!(n.placement, Placement::RightOf);
    assert_eq!((n.from, n.to), (0, 0));
    assert_eq!(n.text, "Text in note");
}

#[test]
fn a_note_left_of_one_participant_parses() {
    let s = sq("participant John\nNote left of John: Text");
    assert_eq!(notes(&s)[0].placement, Placement::LeftOf);
}

#[test]
fn a_note_over_a_pair_parses() {
    let s = sq("participant Alice\nparticipant John\nNote over Alice,John: A typical interaction");
    let n = notes(&s)[0];
    assert_eq!(n.placement, Placement::Over);
    assert_eq!((n.from, n.to), (0, 1));
    assert_eq!(n.text, "A typical interaction");
}

#[test]
fn a_note_over_one_participant_spans_itself() {
    let s = sq("participant A\nNote over A: alone");
    let n = notes(&s)[0];
    assert_eq!((n.from, n.to), (0, 0));
}

#[test]
fn a_note_declares_a_participant_implicitly() {
    let s = sq("Note over Alice,John: hi");
    assert_eq!(ids(&s), ["Alice", "John"]);
    assert!(find(&s, "Alice").implicit);
}

#[test]
fn note_text_keeps_line_breaks_and_entities() {
    let s = sq("Note over A: first<br/>second #hearts;");
    assert_eq!(notes(&s)[0].text, "first\nsecond ♥");
}

#[test]
fn a_note_inside_a_fragment_stays_there() {
    let s = sq("loop retry\n  Note over A: inside\nend");
    assert!(s.items.len() == 1 && notes(&s).len() == 1);
    let f = fragments(&s)[0];
    assert_eq!(f.sections[0].items.len(), 1);
}

#[test]
fn a_note_without_a_placement_is_e002() {
    let (r, d) = try_parse("sequenceDiagram\n    Note beside A: hi\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert!(has(&d, "E002"), "{d:#?}");
}

#[test]
fn a_note_without_text_is_e002() {
    let (r, d) = try_parse("sequenceDiagram\n    Note over A\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert!(has(&d, "E002"), "{d:#?}");
}

// ----------------------------------------------------------------- activation

#[test]
fn activate_and_deactivate_are_items() {
    let s = sq("participant A\nactivate A\nA->>A: work\ndeactivate A");
    let its = items(&s);
    assert!(matches!(its[0], Item::Activate { participant: 0, .. }));
    assert!(matches!(its[2], Item::Deactivate { participant: 0, .. }));
}

#[test]
fn activate_declares_a_participant_implicitly() {
    let s = sq("activate A\ndeactivate A");
    assert_eq!(ids(&s), ["A"]);
}

#[test]
fn activations_stack() {
    let (s, d) = sq_d("activate A\nactivate A\ndeactivate A\ndeactivate A");
    assert_eq!(items(&s).len(), 4);
    assert!(d.is_empty(), "{d:#?}");
}

#[test]
fn a_deactivate_with_nothing_open_warns_w023_and_is_dropped() {
    let (s, d) = sq_d("participant A\ndeactivate A");
    assert!(items(&s).is_empty(), "{:#?}", s.items);
    assert_eq!(count(&d, "W023"), 1, "{d:#?}");
}

#[test]
fn a_minus_suffix_with_nothing_open_warns_w023() {
    let (s, d) = sq_d("A-->>-B: return");
    assert!(!only_message(&s).deactivate);
    assert_eq!(count(&d, "W023"), 1, "{d:#?}");
}

#[test]
fn an_activation_still_open_at_the_end_warns_w023() {
    let (s, d) = sq_d("A->>+B: call");
    assert!(only_message(&s).activate);
    assert_eq!(count(&d, "W023"), 1, "{d:#?}");
}

#[test]
fn a_balanced_activation_warns_nothing() {
    let (_, d) = sq_d("A->>+B: call\nB-->>-A: return");
    assert!(d.is_empty(), "{d:#?}");
}

#[test]
fn the_minus_suffix_closes_the_senders_activation() {
    let (s, d) = sq_d("A->>+B: call\nB-->>-A: return");
    let m = messages(&s);
    assert_eq!(pair(&s, m[1]), ("B", "A"));
    assert!(m[1].deactivate);
    assert!(d.is_empty(), "{d:#?}");
}

// ----------------------------------------------------------------- fragments

#[test]
fn every_fragment_keyword_parses() {
    for (src, kind) in [
        ("loop every minute", FragmentKind::Loop),
        ("alt is sick", FragmentKind::Alt),
        ("opt extras", FragmentKind::Opt),
        ("par first", FragmentKind::Par),
        ("par_over first", FragmentKind::ParOver),
        ("critical set up", FragmentKind::Critical),
        ("break out of stock", FragmentKind::Break),
    ] {
        let s = sq(&format!("{src}\n  A->>B: hi\nend"));
        let f = fragments(&s);
        assert_eq!(f.len(), 1, "{src}");
        assert_eq!(f[0].kind, kind, "{src}");
        assert_eq!(f[0].sections.len(), 1, "{src}");
        assert_eq!(f[0].sections[0].items.len(), 1, "{src}");
    }
}

#[test]
fn a_fragment_header_label_is_the_first_sections_label() {
    let s = sq("loop every minute\n  A->>B: poll\nend");
    assert_eq!(fragments(&s)[0].sections[0].label, "every minute");
}

#[test]
fn a_fragment_may_have_no_label() {
    let s = sq("loop\n  A->>B: poll\nend");
    assert_eq!(fragments(&s)[0].sections[0].label, "");
}

#[test]
fn else_opens_a_section_of_an_alt() {
    let s = sq("alt is sick\n  A->>B: one\nelse is well\n  B->>A: two\nend");
    let f = fragments(&s)[0];
    assert_eq!(f.sections.len(), 2);
    assert_eq!(f.sections[0].label, "is sick");
    assert_eq!(f.sections[1].label, "is well");
    assert_eq!(f.sections[1].items.len(), 1);
}

#[test]
fn and_opens_a_section_of_a_par() {
    let s = sq("par one\n  A->>B: a\nand two\n  A->>B: b\nand three\n  A->>B: c\nend");
    assert_eq!(fragments(&s)[0].sections.len(), 3);
}

#[test]
fn option_opens_a_section_of_a_critical() {
    let s = sq("critical connect\n  A->>B: a\noption timeout\n  A->>B: b\nend");
    let f = fragments(&s)[0];
    assert_eq!(f.kind, FragmentKind::Critical);
    assert_eq!(f.sections.len(), 2);
    assert_eq!(f.sections[1].label, "timeout");
}

#[test]
fn fragments_nest() {
    let s = sq("loop outer\n  alt inner\n    A->>B: a\n  else other\n    A->>B: b\n  end\nend");
    let f = fragments(&s);
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].kind, FragmentKind::Loop);
    assert_eq!(f[1].kind, FragmentKind::Alt);
    assert_eq!(s.items.len(), 1);
}

#[test]
fn a_nested_fragment_belongs_to_the_open_section() {
    let s = sq("alt a\n  A->>B: one\nelse b\n  loop twice\n    A->>B: two\n  end\nend");
    let outer = fragments(&s)[0];
    assert_eq!(outer.sections[1].items.len(), 1);
    assert!(matches!(outer.sections[1].items[0], Item::Fragment(_)));
}

#[test]
fn rect_carries_a_typed_colour() {
    let (s, d) = sq_d("rect rgb(191, 223, 255)\n  A->>B: hi\nend");
    let f = fragments(&s)[0];
    assert_eq!(
        f.kind,
        FragmentKind::Rect(Some(Color::Rgba {
            r: 191,
            g: 223,
            b: 255,
            a: 255
        }))
    );
    assert_eq!(count(&d, "I030"), 1, "{d:#?}");
}

#[test]
fn rect_accepts_rgba_and_hsl() {
    for src in [
        "rect rgba(0, 0, 255, 0.1)",
        "rect hsl(210, 100%, 50%)",
        "rect hsla(210, 100%, 50%, 0.2)",
    ] {
        let (s, _) = sq_d(&format!("{src}\n  A->>B: hi\nend"));
        assert!(
            matches!(fragments(&s)[0].kind, FragmentKind::Rect(_)),
            "{src}"
        );
    }
}

#[test]
fn rect_without_a_colour_takes_the_theme_tint() {
    let (s, d) = sq_d("rect\n  A->>B: hi\nend");
    assert_eq!(fragments(&s)[0].kind, FragmentKind::Rect(None));
    // No colour is named, so nothing is fixed and `I030` has nothing to report.
    assert_eq!(count(&d, "I030"), 0, "{d:#?}");
    assert!(!has(&d, "E002"), "{d:#?}");
}

#[test]
fn rect_keeps_a_label_after_the_missing_colour() {
    let (s, _) = sq_d("rect the retry window\n  A->>B: hi\nend");
    let f = fragments(&s)[0];
    assert_eq!(f.kind, FragmentKind::Rect(None));
    assert_eq!(f.sections[0].label, "the retry window");
}

#[test]
fn rect_transparent_is_a_colour_the_source_names() {
    let (s, d) = sq_d("rect transparent\n  A->>B: hi\nend");
    assert_eq!(
        fragments(&s)[0].kind,
        FragmentKind::Rect(Some(Color::Transparent))
    );
    assert_eq!(count(&d, "I030"), 1, "{d:#?}");
}

#[test]
fn the_fixed_colour_info_is_emitted_once() {
    let (_, d) = sq_d("rect rgb(1,2,3)\n A->>B: a\nend\nrect rgb(4,5,6)\n A->>B: b\nend");
    assert_eq!(count(&d, "I030"), 1, "{d:#?}");
}

#[test]
fn a_fragment_span_covers_the_whole_fragment() {
    let src = "sequenceDiagram\n    loop twice\n      A->>B: hi\n    end\n";
    let s = seq(src);
    let f = fragments(&s)[0];
    let text = &src[f.span.byte_start as usize..f.span.byte_end as usize];
    assert!(text.starts_with("loop twice"), "{text:?}");
    assert!(text.ends_with("end"), "{text:?}");
}

#[test]
fn fragments_deeper_than_the_limit_are_e010() {
    let mut src = String::from("sequenceDiagram\n");
    for _ in 0..70 {
        src.push_str("    loop x\n");
    }
    src.push_str("    A->>B: hi\n");
    let (r, d) = try_parse(&src);
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert!(has(&d, "E010"), "{d:#?}");
}

#[test]
fn a_fragment_keyword_may_name_a_participant_on_a_message_line() {
    let s = sq("participant loop\nloop->>B: hi");
    assert_eq!(ids(&s), ["loop", "B"]);
    assert_eq!(s.messages, 1);
    assert!(fragments(&s).is_empty());
}

#[test]
fn a_hash_comment_ends_a_fragment_header() {
    let s = sq("loop twice  # the retry budget\n  A->>B: hi\nend");
    assert_eq!(fragments(&s)[0].sections[0].label, "twice");
}

// ----------------------------------------------------------------- boxes

#[test]
fn a_box_groups_its_participants() {
    let (s, d) =
        sq_d("box Aqua Group Description\n  participant A\n  participant J\nend\nA->>J: hi");
    assert_eq!(s.boxes.len(), 1);
    assert_eq!(s.boxes[0].label, "Group Description");
    assert_eq!(s.boxes[0].participants, [0, 1]);
    assert_eq!(find(&s, "A").group, Some(0));
    assert_eq!(find(&s, "J").group, Some(0));
    assert_eq!(count(&d, "I030"), 1, "{d:#?}");
}

#[test]
fn a_box_colour_is_typed() {
    let (s, _) = sq_d("box Aqua Group\n  participant A\nend");
    assert_eq!(s.boxes[0].color, Some(Color::Named("aqua")));
}

#[test]
fn a_transparent_box_keeps_a_colour_name_as_its_label() {
    let s = sq("box transparent Aqua\n  participant A\nend");
    assert_eq!(s.boxes[0].label, "Aqua");
    assert_eq!(s.boxes[0].color, None);
}

#[test]
fn a_box_without_a_colour_keeps_the_whole_label() {
    let s = sq("box Mobile clients\n  participant A\nend");
    assert_eq!(s.boxes[0].label, "Mobile clients");
    assert_eq!(s.boxes[0].color, None);
}

#[test]
fn a_box_colour_may_be_a_function() {
    let (s, _) = sq_d("box rgb(200, 220, 255) Ingest\n  participant A\nend");
    assert_eq!(s.boxes[0].label, "Ingest");
    assert!(matches!(s.boxes[0].color, Some(Color::Rgba { .. })));
}

#[test]
fn two_boxes_number_their_members() {
    let s =
        sq("box transparent One\n participant A\nend\nbox transparent Two\n participant B\nend");
    assert_eq!(s.boxes.len(), 2);
    assert_eq!(find(&s, "A").group, Some(0));
    assert_eq!(find(&s, "B").group, Some(1));
}

#[test]
fn a_participant_outside_a_box_has_no_group() {
    let s = sq("box transparent One\n participant A\nend\nparticipant B");
    assert_eq!(find(&s, "B").group, None);
}

#[test]
fn a_message_inside_a_box_is_parsed_where_it_stands() {
    let s = sq("box transparent One\n participant A\n A->>B: hi\nend");
    assert_eq!(s.messages, 1);
    assert_eq!(find(&s, "B").group, None);
    assert_eq!(s.items.len(), 1);
}

#[test]
fn an_implicit_participant_never_joins_a_box() {
    let s = sq("box transparent One\n A->>B: hi\nend");
    assert_eq!(s.boxes[0].participants, [] as [usize; 0]);
}

// ----------------------------------------------------------------- autonumber

#[test]
fn autonumber_alone_starts_at_one() {
    let s = sq("autonumber\nA->>B: hi");
    let a = s.autonumber.expect("autonumber");
    assert_eq!((a.start, a.step, a.visible), (100, 100, true));
}

#[test]
fn autonumber_takes_a_start() {
    let a = sq("autonumber 10\nA->>B: hi").autonumber.expect("a");
    assert_eq!((a.start, a.step), (1000, 100));
}

#[test]
fn autonumber_takes_a_start_and_a_step() {
    let a = sq("autonumber 10 5\nA->>B: hi").autonumber.expect("a");
    assert_eq!((a.start, a.step), (1000, 500));
}

#[test]
fn autonumber_counts_in_hundredths() {
    let a = sq("autonumber 1.05 0.1\nA->>B: hi").autonumber.expect("a");
    assert_eq!((a.start, a.step), (105, 10));
    assert_eq!(a.value(2), 125);
}

#[test]
fn autonumber_off_hides_the_badges() {
    let a = sq("autonumber\nA->>B: hi\nautonumber off")
        .autonumber
        .expect("a");
    assert!(!a.visible);
}

#[test]
fn a_second_autonumber_replaces_the_first() {
    let a = sq("autonumber 1\nA->>B: hi\nautonumber 100 10")
        .autonumber
        .expect("a");
    assert_eq!((a.start, a.step), (10000, 1000));
}

#[test]
fn no_autonumber_leaves_none() {
    assert!(sq("A->>B: hi").autonumber.is_none());
}

#[test]
fn a_non_numeric_autonumber_is_e002() {
    let (r, d) = try_parse("sequenceDiagram\n    autonumber start\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert!(has(&d, "E002"), "{d:#?}");
}

#[test]
fn autonumber_rejects_three_decimals() {
    let (r, _) = try_parse("sequenceDiagram\n    autonumber 1.005\n");
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
}

// ----------------------------------------------------------------- create and destroy

#[test]
fn create_records_the_creating_message() {
    let s = sq("A->>B: hi\ncreate participant C\nB->>C: spawn");
    let c = find(&s, "C");
    assert_eq!(c.created_by, Some(1));
    assert!(!c.implicit);
}

#[test]
fn create_actor_with_an_alias_parses() {
    let s = sq("create actor D as Donald\nA->>D: hi");
    assert_eq!(find(&s, "D").kind, ParticipantKind::Actor);
    assert_eq!(find(&s, "D").label, "Donald");
    assert_eq!(find(&s, "D").created_by, Some(0));
}

#[test]
fn create_matches_only_the_target() {
    let s = sq("create participant C\nC->>A: from the new one\nA->>C: to the new one");
    assert_eq!(find(&s, "C").created_by, Some(1));
}

#[test]
fn destroy_records_the_destroying_message() {
    let s = sq("A->>B: hi\ndestroy B\nA->>B: bye");
    assert_eq!(find(&s, "B").destroyed_by, Some(1));
}

#[test]
fn destroy_matches_either_end_of_the_message() {
    let s = sq("A->>B: hi\ndestroy B\nB-->>A: bye");
    assert_eq!(find(&s, "B").destroyed_by, Some(1));
}

#[test]
fn destroy_without_a_later_message_is_r012() {
    let (s, d) = sq_d("A->>B: hi\ndestroy B");
    assert_eq!(find(&s, "B").destroyed_by, None);
    assert_eq!(count(&d, "R012"), 1, "{d:#?}");
}

#[test]
fn a_created_participant_without_a_message_keeps_its_column() {
    let (s, d) = sq_d("A->>B: hi\ncreate participant C");
    assert_eq!(ids(&s), ["A", "B", "C"]);
    assert_eq!(find(&s, "C").created_by, None);
    assert!(d.is_empty(), "{d:#?}");
}

// ----------------------------------------------------------------- title and accessibility

#[test]
fn a_title_statement_sets_the_title() {
    let s = sq("title Release train\nA->>B: hi");
    assert_eq!(s.meta.title.as_deref(), Some("Release train"));
}

#[test]
fn the_legacy_title_statement_sets_the_title() {
    let s = sq("title: Release train\nA->>B: hi");
    assert_eq!(s.meta.title.as_deref(), Some("Release train"));
}

#[test]
fn acc_title_and_acc_descr_reach_the_meta() {
    let s = sq("accTitle: Login flow\naccDescr: How a user signs in\nA->>B: hi");
    assert_eq!(s.meta.acc_title.as_deref(), Some("Login flow"));
    assert_eq!(s.meta.acc_descr.as_deref(), Some("How a user signs in"));
}

#[test]
fn an_acc_descr_block_joins_its_lines() {
    let s = sq("accDescr {\n  first line\n  second line\n}\nA->>B: hi");
    assert_eq!(s.meta.acc_descr.as_deref(), Some("first line\nsecond line"));
}

#[test]
fn front_matter_and_a_title_statement_agree_on_the_last_one() {
    let s = seq("---\ntitle: From front matter\n---\nsequenceDiagram\n    title From the body\n    A->>B: hi\n");
    assert_eq!(s.meta.title.as_deref(), Some("From the body"));
}

// ----------------------------------------------------------------- dropped statements

#[test]
fn link_statements_are_dropped_with_w021() {
    let (s, d) = sq_d("participant A\nlink A: Dashboard @ https://example.com/a\nA->>B: hi");
    assert_eq!(s.messages, 1);
    assert_eq!(count(&d, "W021"), 1, "{d:#?}");
}

#[test]
fn every_dropped_statement_warns_w021() {
    for stmt in [
        "link A: Dashboard @ https://example.com/a",
        "links A: {\"Repo\": \"https://example.com\"}",
        "properties A: {\"class\": \"internal\"}",
        "details A: {\"region\": \"eu\"}",
    ] {
        let (s, d) = sq_d(&format!("participant A\n{stmt}\nA->>B: hi"));
        assert_eq!(count(&d, "W021"), 1, "{stmt}: {d:#?}");
        assert_eq!(s.messages, 1, "{stmt}");
    }
}

#[test]
fn a_dropped_statement_leaves_no_item() {
    let (s, _) = sq_d("participant A\nlink A: Dashboard @ https://example.com/a");
    assert!(s.items.is_empty());
}

#[test]
fn a_dropped_statement_url_never_reaches_the_model() {
    let (s, _) = sq_d("participant A\nlink A: Dashboard @ javascript:alert(1)");
    assert!(!format!("{:?}", s).contains("javascript"), "{s:?}");
}

// ----------------------------------------------------------------- repairs

#[test]
fn an_unclosed_fragment_is_r009() {
    let (s, d) = sq_d("loop forever\n  A->>B: hi");
    assert_eq!(fragments(&s).len(), 1);
    assert_eq!(fragments(&s)[0].sections[0].items.len(), 1);
    assert_eq!(count(&d, "R009"), 1, "{d:#?}");
}

#[test]
fn every_unclosed_fragment_gets_its_own_r009() {
    let (_, d) = sq_d("loop forever\n  alt a\n    A->>B: hi");
    assert_eq!(count(&d, "R009"), 2, "{d:#?}");
}

#[test]
fn an_unclosed_box_is_r009() {
    let (s, d) = sq_d("box transparent Group\n  participant A");
    assert_eq!(s.boxes.len(), 1);
    assert_eq!(count(&d, "R009"), 1, "{d:#?}");
}

#[test]
fn the_r009_fix_reparses_clean() {
    assert_fix_round_trip(
        "sequenceDiagram\n    loop forever\n      A->>B: hi\n",
        &["R009"],
    );
    assert_fix_round_trip(
        "sequenceDiagram\n    loop forever\n      alt a\n        A->>B: hi\n",
        &["R009"],
    );
}

#[test]
fn a_section_outside_a_fragment_is_r010() {
    let (s, d) = sq_d("A->>B: one\nelse other\nB->>A: two");
    assert_eq!(s.messages, 2);
    assert!(fragments(&s).is_empty());
    assert_eq!(s.items.len(), 2, "{:#?}", s.items);
    assert_eq!(count(&d, "R010"), 1, "{d:#?}");
}

#[test]
fn every_section_keyword_outside_a_fragment_is_r010() {
    for kw in ["else", "and", "option"] {
        let (_, d) = sq_d(&format!("A->>B: one\n{kw} other\nB->>A: two"));
        assert_eq!(count(&d, "R010"), 1, "{kw}: {d:#?}");
    }
}

#[test]
fn the_r010_fix_reparses_clean() {
    assert_fix_round_trip(
        "sequenceDiagram\n    A->>B: one\n    else other\n    B->>A: two\n",
        &["R010"],
    );
}

#[test]
fn an_unmatched_end_is_r011() {
    let (s, d) = sq_d("A->>B: one\nend\nB->>A: two");
    assert_eq!(s.messages, 2);
    assert_eq!(count(&d, "R011"), 1, "{d:#?}");
}

#[test]
fn the_r011_fix_reparses_clean() {
    assert_fix_round_trip(
        "sequenceDiagram\n    A->>B: one\n    end\n    B->>A: two\n",
        &["R011"],
    );
}

#[test]
fn the_r012_fix_reparses_clean() {
    assert_fix_round_trip("sequenceDiagram\n    A->>B: hi\n    destroy B\n", &["R012"]);
}

#[test]
fn a_message_without_a_colon_before_its_text_is_r013() {
    let (s, d) = sq_d("Alice->>Bob Hello");
    let m = only_message(&s);
    assert_eq!(pair(&s, m), ("Alice", "Bob"));
    assert_eq!(m.label, "Hello");
    assert_eq!(count(&d, "R013"), 1, "{d:#?}");
}

#[test]
fn the_r013_fix_inserts_the_colon() {
    let out = assert_fix_round_trip("sequenceDiagram\n    Alice->>Bob Hello\n", &["R013"]);
    assert_eq!(out, "sequenceDiagram\n    Alice->>Bob: Hello\n");
}

#[test]
fn several_repairs_in_one_diagram_all_reparse_clean() {
    assert_fix_round_trip(
        "sequenceDiagram\n    loop forever\n      Alice->>Bob Hello\n    end\n    else other\n    end\n    opt leftover\n      A->>B: x\n",
        &["R009", "R010", "R011", "R013"],
    );
}

#[test]
fn a_repair_carries_a_fix() {
    let (_, d) = sq_d("Alice->>Bob Hello");
    let r = d.iter().find(|x| x.code == "R013").expect("R013");
    assert!(r.fix.is_some(), "{r:?}");
    assert_eq!(r.severity, Severity::Repair);
}

// ----------------------------------------------------------------- strict mode

#[test]
fn strict_mode_fails_on_a_repair() {
    let (r, d) = run(
        "sequenceDiagram\n    Alice->>Bob Hello\n",
        true,
        Limits::default(),
    );
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(d[0].code, "R013");
}

#[test]
fn strict_mode_fails_on_a_warning() {
    let (r, d) = run(
        "sequenceDiagram\n    participant A@{ \"type\": \"robot\" }\n",
        true,
        Limits::default(),
    );
    assert!(matches!(r, Err(ParseError::Failed)), "{r:?}");
    assert_eq!(d[0].code, "W022");
    assert_eq!(d[0].severity, Severity::Error);
}

#[test]
fn strict_mode_accepts_a_clean_diagram() {
    let (r, d) = run(
        "sequenceDiagram\n    participant A\n    A->>B: hi\n",
        true,
        Limits::default(),
    );
    assert!(matches!(r, Ok(Diagram::Sequence(_))), "{r:?}");
    assert!(d.is_empty(), "{d:#?}");
}

#[test]
fn strict_mode_keeps_an_info_an_info() {
    let (r, d) = run(
        "sequenceDiagram\n    rect rgb(1,2,3)\n      A->>B: hi\n    end\n",
        true,
        Limits::default(),
    );
    assert!(matches!(r, Ok(Diagram::Sequence(_))), "{r:?}");
    assert_eq!(codes(&d), ["I030"], "{d:#?}");
}

// ----------------------------------------------------------------- robustness

#[test]
fn every_prefix_of_every_fixture_parses_without_panicking() {
    for (_, src) in fixtures() {
        for end in 0..src.len() {
            if src.is_char_boundary(end) {
                let _ = try_parse(&src[..end]);
            }
        }
    }
}

#[test]
fn a_mutated_fixture_never_panics() {
    // One byte of syntax removed or replaced is the shape a hand-edited diagram takes.
    for (_, src) in fixtures() {
        for at in (0..src.len()).step_by(3) {
            if !src.is_char_boundary(at) || !src.is_char_boundary(at + 1) {
                continue;
            }
            let mut cut = String::from(&src[..at]);
            cut.push_str(&src[at + 1..]);
            let _ = try_parse(&cut);
            for c in [':', ';', '#', '-', '>', '\n'] {
                let mut swapped = String::from(&src[..at]);
                swapped.push(c);
                swapped.push_str(&src[at + 1..]);
                let _ = try_parse(&swapped);
            }
        }
    }
}

#[test]
fn truncated_statements_never_panic() {
    for src in [
        "sequenceDiagram\n    participant",
        "sequenceDiagram\n    participant A as",
        "sequenceDiagram\n    participant A@{",
        "sequenceDiagram\n    A->>",
        "sequenceDiagram\n    A->>B:",
        "sequenceDiagram\n    Note",
        "sequenceDiagram\n    Note over",
        "sequenceDiagram\n    Note over A,",
        "sequenceDiagram\n    box",
        "sequenceDiagram\n    rect",
        "sequenceDiagram\n    autonumber",
        "sequenceDiagram\n    activate",
        "sequenceDiagram\n    create",
        "sequenceDiagram\n    destroy",
        "sequenceDiagram\n    loop",
        "sequenceDiagram\n    end",
        "sequenceDiagram\n    link",
        "sequenceDiagram\n    accDescr {",
        "sequenceDiagram\n    <<-->>",
        "sequenceDiagram\n    ()->>()",
    ] {
        let _ = try_parse(src);
    }
}

#[test]
fn unicode_ids_and_labels_parse() {
    let s = sq("participant Café as Café ☕\nCafé->>Bäcker: Zwei Brötchen, bitte");
    assert_eq!(ids(&s), ["Café", "Bäcker"]);
    assert_eq!(labels(&s)[0], "Café ☕");
    assert_eq!(only_message(&s).label, "Zwei Brötchen, bitte");
}

#[test]
fn a_long_single_line_diagram_parses() {
    let mut src = String::from("sequenceDiagram\n");
    for i in 0..500 {
        src.push_str(&format!("A->>B: message {i};"));
    }
    let (s, _) = seq_d(&src);
    assert_eq!(s.messages, 500);
}

#[test]
fn blank_lines_and_indentation_do_not_matter() {
    let s = seq("sequenceDiagram\n\n\n\t\tparticipant A\n\n   A->>B: hi\n\n");
    assert_eq!(ids(&s), ["A", "B"]);
}

// ----------------------------------------------------------------- fixtures

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sequence")
        .join(name)
}

fn fixtures() -> Vec<(String, String)> {
    let dir = fixture_path("");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&p).unwrap();
            (name, text)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn the_corpus_has_enough_diagrams() {
    let n = fixtures().len();
    assert!((15..=25).contains(&n), "{n} fixtures");
}

#[test]
fn every_fixture_parses() {
    for (name, src) in fixtures() {
        let (s, d) = seq_d(&src);
        assert!(s.participants.len() >= 2, "{name}: {:?}", ids(&s));
        assert!(s.messages >= 3, "{name}: {} messages", s.messages);
        assert!(errors(&d).is_empty(), "{name}: {d:#?}");
    }
}

#[test]
fn only_the_llm_fixtures_need_repairs() {
    for (name, src) in fixtures() {
        let (_, d) = seq_d(&src);
        let repairs: Vec<_> = d
            .iter()
            .filter(|x| x.severity == Severity::Repair)
            .collect();
        if name.starts_with("llm-") {
            assert!(!repairs.is_empty(), "{name} exercises no repair");
        } else {
            assert!(repairs.is_empty(), "{name}: {repairs:#?}");
        }
    }
}

#[test]
fn every_fixture_fix_converges() {
    for (name, src) in fixtures() {
        let (_, d) = seq_d(&src);
        let out = fixed(&src, &d);
        let (_, d2) = seq_d(&out);
        let left: Vec<_> = d2
            .iter()
            .filter(|x| x.severity == Severity::Repair)
            .collect();
        assert!(left.is_empty(), "{name}: {out}\n{left:#?}");
        assert!(errors(&d2).is_empty(), "{name}: {out}\n{d2:#?}");
    }
}

#[test]
fn the_corpus_covers_every_repair_code() {
    let mut seen: Vec<&str> = Vec::new();
    for (_, src) in fixtures() {
        let (_, d) = seq_d(&src);
        for x in d {
            if x.severity == Severity::Repair && !seen.contains(&x.code) {
                seen.push(x.code);
            }
        }
    }
    for code in ["R009", "R010", "R011", "R012", "R013"] {
        assert!(seen.contains(&code), "{code} is not exercised: {seen:?}");
    }
}

#[test]
fn the_corpus_covers_every_participant_kind() {
    let mut seen: Vec<ParticipantKind> = Vec::new();
    for (_, src) in fixtures() {
        let (s, _) = seq_d(&src);
        for p in &s.participants {
            if !seen.contains(&p.kind) {
                seen.push(p.kind);
            }
        }
    }
    for k in ParticipantKind::ALL {
        assert!(seen.contains(&k), "{} is not exercised", k.as_str());
    }
}

#[test]
fn the_corpus_covers_every_fragment_kind() {
    let mut seen: Vec<&'static str> = Vec::new();
    for (_, src) in fixtures() {
        let (s, _) = seq_d(&src);
        for f in fragments(&s) {
            if !seen.contains(&f.kind.as_str()) {
                seen.push(f.kind.as_str());
            }
        }
    }
    for k in [
        "loop", "alt", "opt", "par", "par_over", "critical", "break", "rect",
    ] {
        assert!(seen.contains(&k), "{k} is not exercised: {seen:?}");
    }
}

#[test]
fn the_arrow_gallery_covers_every_head() {
    let src = std::fs::read_to_string(fixture_path("arrow-gallery.mmd")).expect("fixture");
    let (s, _) = seq_d(&src);
    let mut seen: Vec<(MessageLine, Head, Head)> = Vec::new();
    for m in messages(&s) {
        let k = (m.line, m.tail, m.head);
        if !seen.contains(&k) {
            seen.push(k);
        }
    }
    assert_eq!(seen.len(), messages(&s).len(), "duplicate arrow forms");
    assert!(seen.len() >= 18, "{seen:?}");
}

// ------------------------------------------------------------- bounded scans

/// Every scan a statement runs is bounded by the statement, so the cost of a line is
/// the same whether its statements are separated by `;` or by newlines
/// (specs/sequence.md#syntax).
#[test]
fn statements_on_one_line_cost_the_same_as_one_per_line() {
    let n = 40_000;
    let mut one_line = String::from("sequenceDiagram\n");
    let mut one_each = String::from("sequenceDiagram\n");
    for _ in 0..n {
        one_line.push_str("participant P;");
        one_each.push_str("participant P\n");
    }
    let a = seq(&one_line);
    let b = seq(&one_each);
    assert_eq!(a.participants.len(), 1);
    assert_eq!(b.participants.len(), a.participants.len());
}

/// An `@{` that no `}` closes costs the statement, not the rest of the source: the
/// scan stops and the statement takes `W022`.
#[test]
fn an_unclosed_at_block_scans_no_further_than_its_statement() {
    let n = 20_000;
    let mut src = String::from("sequenceDiagram\n");
    for _ in 0..n {
        src.push_str("participant P@{\n");
    }
    let (s, d) = seq_d(&src);
    assert_eq!(s.participants.len(), 1);
    assert!(count(&d, "W022") > 0);
}

/// A block longer than the scan window is not closed: it takes `W022` and the
/// participant declares plain, as an unclosed `@{` does.
#[test]
fn an_at_block_past_the_scan_window_is_ignored() {
    let filler = "x".repeat(16_000);
    let src = format!("sequenceDiagram\n    participant A@{{ \"alias\": \"{filler}\" }}\n");
    let (s, d) = seq_d(&src);
    assert!(has(&d, "W022"), "{:?}", codes(&d));
    assert_eq!(s.participants.len(), 1);
    assert_eq!(s.participants[0].id, "A");
}
