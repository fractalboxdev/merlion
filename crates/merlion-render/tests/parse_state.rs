//! The `stateDiagram` / `stateDiagram-v2` parser (specs/state.md#syntax, #diagnostics):
//! one case per statement form, `[*]` resolving to one start and one end per scope, the
//! diagnostics `W024`, `W025` and the shared codes, and a repair case per `R014`–`R018`
//! whose fix reparses clean.

mod parse_support;

use merlion_render::diag::{Diagnostic, Severity};
use merlion_render::model::state::{NotePlacement, State, StateKind, StateMachine};
use merlion_render::model::Diagram;
use merlion_render::options::{Direction, Limits};
use merlion_render::parse::ParseError;
use parse_support::{codes, count, errors, fixed, has, run, try_parse};

// ----------------------------------------------------------------- helpers

/// Parses `src` and returns the state machine with every diagnostic.
fn sm_d(src: &str) -> (StateMachine, Vec<Diagnostic>) {
    let (r, d) = try_parse(src);
    match r {
        Ok(Diagram::State(s)) => (s, d),
        Ok(other) => panic!("expected a state diagram, got a {}", other.type_name()),
        Err(e) => panic!("parse failed for {src:?}: {e:?}\n{d:#?}"),
    }
}

/// Parses `src` and asserts that it carries no diagnostic at all.
fn sm(src: &str) -> StateMachine {
    let (s, d) = sm_d(src);
    assert!(d.is_empty(), "{src:?}: unexpected diagnostics {d:#?}");
    s
}

/// `stateDiagram-v2` followed by `b`, indented as a hand-written diagram is.
fn body(b: &str) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    for line in b.lines() {
        s.push_str("    ");
        s.push_str(line);
        s.push('\n');
    }
    s
}

fn st(b: &str) -> StateMachine {
    sm(&body(b))
}

fn st_d(b: &str) -> (StateMachine, Vec<Diagnostic>) {
    sm_d(&body(b))
}

fn ids(s: &StateMachine) -> Vec<&str> {
    s.states.iter().map(|x| x.id.as_str()).collect()
}

fn labels(s: &StateMachine) -> Vec<&str> {
    s.states.iter().map(|x| x.label.as_str()).collect()
}

fn kinds(s: &StateMachine) -> Vec<StateKind> {
    s.states.iter().map(|x| x.kind).collect()
}

fn find<'a>(s: &'a StateMachine, id: &str) -> &'a State {
    s.states
        .iter()
        .find(|x| x.id == id)
        .unwrap_or_else(|| panic!("no state {id:?} in {:?}", ids(s)))
}

fn idx(s: &StateMachine, id: &str) -> usize {
    s.state_index(id)
        .unwrap_or_else(|| panic!("no state {id:?} in {:?}", ids(s)))
}

fn id_at(s: &StateMachine, i: usize) -> &str {
    s.states.get(i).map(|x| x.id.as_str()).unwrap_or("?")
}

/// Every transition as `(from id, to id, label)`.
fn edges(s: &StateMachine) -> Vec<(&str, &str, &str)> {
    s.transitions
        .iter()
        .map(|t| {
            (
                id_at(s, t.from),
                id_at(s, t.to),
                t.label.as_deref().unwrap_or(""),
            )
        })
        .collect()
}

/// Applies every fix, reparses, and asserts that none of `repair_codes` remains and that
/// the result carries no error.
fn assert_fix_round_trip(src: &str, repair_codes: &[&str]) -> (String, StateMachine) {
    let (_, d) = sm_d(src);
    for c in repair_codes {
        let r: Vec<_> = d.iter().filter(|x| x.code == *c).collect();
        assert!(!r.is_empty(), "{src:?}: expected {c}, got {:?}", codes(&d));
        for x in r {
            assert_eq!(x.severity, Severity::Repair, "{x:?}");
            assert!(x.fix.is_some(), "{c} without a fix: {x:?}");
        }
    }
    let out = fixed(src, &d);
    let (s, d2) = sm_d(&out);
    for c in repair_codes {
        assert!(
            !has(&d2, c),
            "{c} remains after fixing {src:?} into {out:?}: {d2:#?}"
        );
    }
    assert!(errors(&d2).is_empty(), "{out:?}: {d2:#?}");
    (out, s)
}

// ----------------------------------------------------------------- headers

#[test]
fn both_headers_parse_the_same_model() {
    let a = sm("stateDiagram-v2\n[*] --> Still\nStill --> [*]\n");
    let b = sm("stateDiagram\n[*] --> Still\nStill --> [*]\n");
    assert_eq!(ids(&a), ids(&b));
    assert_eq!(edges(&a), edges(&b));
}

#[test]
fn a_direction_on_the_header_line_sets_the_diagram_direction() {
    assert_eq!(sm("stateDiagram-v2 LR\nA --> B\n").direction, Direction::LR);
    assert_eq!(sm("stateDiagram-v2 TD\nA --> B\n").direction, Direction::TB);
}

#[test]
fn a_trailing_semicolon_ends_the_statement_rather_than_naming_a_state() {
    let s = st("A --> B;\nB --> C;\n");
    assert_eq!(ids(&s), ["A", "B", "C"]);
}

#[test]
fn several_statements_share_a_line_when_semicolons_separate_them() {
    let s = st("A --> B; B --> C\n");
    assert_eq!(edges(&s), [("A", "B", ""), ("B", "C", "")]);
}

#[test]
fn percent_comments_run_to_the_end_of_the_line() {
    let s = st("%% the whole line\nA --> B %% and the tail\nB --> C\n");
    assert_eq!(edges(&s), [("A", "B", ""), ("B", "C", "")]);
}

#[test]
fn hash_comments_run_to_the_end_of_the_line() {
    let s = st("# the whole line\nA --> B # and the tail\n");
    assert_eq!(edges(&s), [("A", "B", "")]);
}

#[test]
fn an_entity_code_is_not_a_hash_comment() {
    let s = st("A --> B : 100#37; done\n");
    assert_eq!(edges(&s), [("A", "B", "100% done")]);
}

#[test]
fn entity_codes_decode_in_every_label() {
    let s = st("state \"#quot;live#quot;\" as A\nA --> B : #hearts;\n");
    assert_eq!(find(&s, "A").label, "\"live\"");
    assert_eq!(edges(&s), [("A", "B", "♥")]);
}

#[test]
fn a_markdown_fence_left_in_the_source_is_stripped() {
    let (s, d) = sm_d("```mermaid\nstateDiagram-v2\nA --> B\n```\n");
    assert_eq!(edges(&s), [("A", "B", "")]);
    assert_eq!(count(&d, "R006"), 2);
}

#[test]
fn an_init_directive_inside_the_body_is_read() {
    let s = sm("stateDiagram-v2\n%%{init: {\"merlion\": {\"autoTone\": false}}}%%\nA --> B\n");
    assert_eq!(s.meta.auto_tone, Some(false));
}

// ----------------------------------------------------------------- states

#[test]
fn a_bare_id_declares_a_simple_state_labelled_by_its_id() {
    let s = st("Still\n");
    assert_eq!(ids(&s), ["Still"]);
    assert_eq!(labels(&s), ["Still"]);
    assert_eq!(kinds(&s), [StateKind::Simple]);
    assert!(!find(&s, "Still").implicit);
}

#[test]
fn the_state_keyword_declares_a_simple_state() {
    let s = st("state Still\n");
    assert_eq!(ids(&s), ["Still"]);
    assert_eq!(kinds(&s), [StateKind::Simple]);
}

#[test]
fn a_quoted_description_names_the_label_and_as_names_the_id() {
    let s = st("state \"This is a state description\" as s2\n");
    assert_eq!(ids(&s), ["s2"]);
    assert_eq!(labels(&s), ["This is a state description"]);
}

#[test]
fn a_colon_description_sets_the_label() {
    let s = st("s2 : This is a state description\n");
    assert_eq!(ids(&s), ["s2"]);
    assert_eq!(labels(&s), ["This is a state description"]);
}

#[test]
fn a_description_without_spaces_around_the_colon_still_parses() {
    let s = st("s2:short\n");
    assert_eq!(labels(&s), ["short"]);
}

#[test]
fn a_later_description_adds_a_line() {
    let s = st("state \"first\" as A\nA : second\n");
    assert_eq!(labels(&s), ["first\nsecond"]);
}

#[test]
fn a_third_description_adds_a_third_line() {
    let s = st("A : one\nA : two\nA : three\n");
    assert_eq!(labels(&s), ["one\ntwo\nthree"]);
}

#[test]
fn the_alias_form_takes_a_description_after_the_colon() {
    let s = st("state \"Some long name\" as S1: The description\n");
    assert_eq!(ids(&s), ["S1"]);
    assert_eq!(labels(&s), ["Some long name\nThe description"]);
}

#[test]
fn a_bare_declaration_takes_a_description_after_the_colon() {
    let s = st("state S1 : only\n");
    assert_eq!(labels(&s), ["only"]);
}

#[test]
fn an_opening_brace_on_the_next_line_opens_the_composite() {
    let s = st("state Outer\n{\nA --> B\n}\nOuter --> C\n");
    assert_eq!(ids(&s), ["Outer", "A", "B", "C"]);
    assert_eq!(find(&s, "Outer").kind, StateKind::Composite);
    assert_eq!(
        find(&s, "A").parent.map(|p| s.states[p].id.as_str()),
        Some("Outer")
    );
}

#[test]
fn a_brace_on_the_next_line_of_the_alias_form_opens_the_composite() {
    let s = st("state \"Outside\" as Outer\n  {\n  A --> B\n  }\n");
    assert_eq!(find(&s, "Outer").kind, StateKind::Composite);
    assert_eq!(find(&s, "Outer").label, "Outside");
}

#[test]
fn a_role_shorthand_after_the_state_keyword_is_not_a_description() {
    let (s, _) = st_d("classDef hot fill:#f00\nstate A:::hot\n");
    assert_eq!(ids(&s), ["A"]);
    assert_eq!(labels(&s), ["A"]);
    assert_eq!(find(&s, "A").classes, ["hot"]);
}

#[test]
fn text_after_the_state_id_is_dropped_with_w024() {
    let (s, d) = st_d("state fork_state &lt;&lt;fork&gt;&gt;\nfork_state --> A\n");
    assert_eq!(ids(&s), ["fork_state", "A"]);
    assert_eq!(labels(&s), ["fork_state", "A"]);
    assert_eq!(kinds(&s), [StateKind::Simple, StateKind::Simple]);
    assert!(has(&d, "W024"), "{d:#?}");
}

#[test]
fn a_description_runs_to_the_end_of_the_statement_and_keeps_its_breaks() {
    let s = st("A : one<br/>two\n");
    assert_eq!(labels(&s), ["one\ntwo"]);
}

#[test]
fn a_description_may_hold_colons() {
    let s = st("A : ratio 1:2\n");
    assert_eq!(labels(&s), ["ratio 1:2"]);
}

#[test]
fn a_state_named_only_by_a_transition_is_implicit_and_carries_no_diagnostic() {
    let s = st("A --> B\n");
    assert!(find(&s, "A").implicit && find(&s, "B").implicit);
}

#[test]
fn declaring_a_state_after_a_transition_names_it_clears_implicit() {
    let s = st("A --> B\nstate B\n");
    assert!(!find(&s, "B").implicit);
    assert_eq!(ids(&s), ["A", "B"]);
}

#[test]
fn a_description_clears_implicit_too() {
    let s = st("A --> B\nB : done\n");
    assert!(!find(&s, "B").implicit);
    assert_eq!(labels(&s), ["A", "done"]);
}

#[test]
fn declaration_order_is_source_order() {
    let s = st("state C\nA --> B\nB --> C\n");
    assert_eq!(ids(&s), ["C", "A", "B"]);
}

#[test]
fn keywords_are_case_insensitive() {
    let s = st("STATE Alpha\nDirection LR\nAlpha --> Beta\nNOTE right of Beta : hi\n");
    assert_eq!(s.direction, Direction::LR);
    assert_eq!(ids(&s), ["Alpha", "Beta"]);
    assert_eq!(s.notes.len(), 1);
}

#[test]
fn an_id_may_hold_hyphens_underscores_and_dots() {
    let s = st("my-state_1.2 --> other\n");
    assert_eq!(ids(&s), ["my-state_1.2", "other"]);
}

#[test]
fn a_state_named_like_a_keyword_still_takes_a_transition() {
    let s = st("state --> note\nnote --> direction\n");
    assert_eq!(ids(&s), ["state", "note", "direction"]);
}

// ----------------------------------------------------------------- transitions

#[test]
fn a_transition_links_two_states() {
    let s = st("s1 --> s2\n");
    assert_eq!(edges(&s), [("s1", "s2", "")]);
    assert!(s.transitions[0].label.is_none());
}

#[test]
fn a_transition_takes_a_label_after_the_colon() {
    let s = st("s1 --> s2 : A transition\n");
    assert_eq!(edges(&s), [("s1", "s2", "A transition")]);
}

#[test]
fn a_transition_label_keeps_its_line_breaks() {
    let s = st("s1 --> s2 : one<br/>two\n");
    assert_eq!(edges(&s), [("s1", "s2", "one\ntwo")]);
}

#[test]
fn a_transition_label_may_hold_further_colons() {
    let s = st("s1 --> s2 : retry: at most 3\n");
    assert_eq!(edges(&s), [("s1", "s2", "retry: at most 3")]);
}

#[test]
fn an_empty_label_after_the_colon_is_no_label() {
    let s = st("s1 --> s2 :\n");
    assert_eq!(s.transitions[0].label.as_deref(), Some(""));
}

#[test]
fn a_self_transition_is_kept() {
    let s = st("A --> A : retry\n");
    assert_eq!(edges(&s), [("A", "A", "retry")]);
}

#[test]
fn transitions_keep_source_order() {
    let s = st("A --> B\nC --> D\nB --> C\n");
    assert_eq!(edges(&s), [("A", "B", ""), ("C", "D", ""), ("B", "C", "")]);
}

#[test]
fn a_transition_without_spaces_around_the_arrow_parses() {
    let s = st("A-->B\n");
    assert_eq!(edges(&s), [("A", "B", "")]);
}

#[test]
fn every_other_arrow_form_is_read_as_the_canonical_one() {
    for arrow in [
        "->", "->>", "-->>", "==>", "===>", "=>", "-.->", "-.-", "--->", "<-->", "<->",
    ] {
        let src = body(&alloc_line("A", arrow, "B"));
        let (s, d) = sm_d(&src);
        assert_eq!(edges(&s), [("A", "B", "")], "{arrow}");
        assert_eq!(count(&d, "R018"), 1, "{arrow}: {:?}", codes(&d));
    }
}

fn alloc_line(a: &str, arrow: &str, b: &str) -> String {
    format!("{a} {arrow} {b}\n")
}

#[test]
fn the_canonical_arrow_is_never_repaired() {
    let (_, d) = st_d("A --> B\n");
    assert!(!has(&d, "R018"), "{:?}", codes(&d));
}

#[test]
fn a_repaired_arrow_reparses_clean() {
    assert_fix_round_trip(&body("A ->> B : go\nB ==> C\n"), &["R018"]);
}

#[test]
fn transition_text_without_its_colon_is_repaired() {
    let (s, d) = st_d("s1 --> s2 done\n");
    assert_eq!(edges(&s), [("s1", "s2", "done")]);
    assert_eq!(count(&d, "R017"), 1, "{:?}", codes(&d));
}

#[test]
fn repaired_transition_text_reparses_clean() {
    assert_fix_round_trip(&body("s1 --> s2 done\ns2 --> s3 also done\n"), &["R017"]);
}

#[test]
fn a_transition_with_no_source_fails() {
    let (r, d) = try_parse(&body("--> B\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

#[test]
fn a_transition_with_no_target_fails() {
    let (r, d) = try_parse(&body("A -->\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

#[test]
fn an_id_holding_a_space_fails() {
    let (r, d) = try_parse(&body("A B --> C\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

// ----------------------------------------------------------------- start and end

#[test]
fn a_star_source_is_the_scopes_start_state() {
    let s = st("[*] --> Still\n");
    assert_eq!(ids(&s), ["root_start", "Still"]);
    assert_eq!(kinds(&s), [StateKind::Start, StateKind::Simple]);
    assert_eq!(edges(&s), [("root_start", "Still", "")]);
}

#[test]
fn a_star_target_is_the_scopes_end_state() {
    let s = st("Still --> [*]\n");
    assert_eq!(ids(&s), ["Still", "root_end"]);
    assert_eq!(kinds(&s), [StateKind::Simple, StateKind::End]);
}

#[test]
fn every_star_in_one_scope_names_the_same_pair() {
    let s = st("[*] --> A\n[*] --> B\nA --> [*]\nB --> [*]\n");
    assert_eq!(ids(&s), ["root_start", "A", "B", "root_end"]);
    assert_eq!(s.states.len(), 4);
}

#[test]
fn the_pseudo_states_carry_no_label() {
    let s = st("[*] --> A\nA --> [*]\n");
    assert_eq!(find(&s, "root_start").label, "");
    assert_eq!(find(&s, "root_end").label, "");
    assert!(!StateKind::Start.draws_label());
}

#[test]
fn a_composite_scope_has_its_own_start_and_end() {
    let s = st("[*] --> First\nstate First {\n  [*] --> second\n  second --> [*]\n}\n");
    assert!(s.state_index("First_start").is_some(), "{:?}", ids(&s));
    assert!(s.state_index("First_end").is_some(), "{:?}", ids(&s));
    assert_eq!(find(&s, "First_start").parent, Some(idx(&s, "First")));
}

#[test]
fn a_generated_id_that_clashes_with_a_declared_one_takes_a_further_underscore() {
    let s = st("state root_start\n[*] --> A\n");
    assert_eq!(ids(&s), ["root_start", "root_start_", "A"]);
    assert_eq!(find(&s, "root_start_").kind, StateKind::Start);
}

#[test]
fn a_star_outside_a_transition_fails() {
    let (r, d) = try_parse(&body("[*]\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

#[test]
fn a_star_to_a_star_links_the_scopes_two_pseudo_states() {
    let s = st("[*] --> [*]\n");
    assert_eq!(ids(&s), ["root_start", "root_end"]);
    assert_eq!(edges(&s), [("root_start", "root_end", "")]);
}

// ----------------------------------------------------------------- composite states

#[test]
fn a_composite_state_owns_its_members() {
    let s = st("state First {\n  [*] --> second\n  second --> [*]\n}\n");
    let first = idx(&s, "First");
    assert_eq!(find(&s, "First").kind, StateKind::Composite);
    assert_eq!(find(&s, "second").parent, Some(first));
    assert_eq!(
        s.states[first].children,
        vec![
            idx(&s, "First_start"),
            idx(&s, "second"),
            idx(&s, "First_end")
        ]
    );
}

#[test]
fn a_parent_precedes_its_children() {
    let s = st("state Outer {\n  state Inner {\n    A --> B\n  }\n}\n");
    let (o, i, a) = (idx(&s, "Outer"), idx(&s, "Inner"), idx(&s, "A"));
    assert!(o < i && i < a, "{:?}", ids(&s));
    assert_eq!(find(&s, "Inner").parent, Some(o));
    assert_eq!(find(&s, "A").parent, Some(i));
}

#[test]
fn a_named_composite_takes_its_description() {
    let s = st("state \"Another Composite\" as NamedComposite {\n  A --> B\n}\n");
    assert_eq!(find(&s, "NamedComposite").label, "Another Composite");
    assert_eq!(find(&s, "NamedComposite").kind, StateKind::Composite);
}

#[test]
fn a_later_description_labels_a_composite() {
    let s = st("state NamedComposite {\n  A --> B\n}\nNamedComposite: Another Composite\n");
    assert_eq!(find(&s, "NamedComposite").label, "Another Composite");
    assert_eq!(find(&s, "NamedComposite").kind, StateKind::Composite);
}

#[test]
fn a_composite_with_no_members_keeps_its_kind_and_has_no_children() {
    let s = st("state Empty {\n}\nEmpty --> Done\n");
    assert_eq!(find(&s, "Empty").kind, StateKind::Composite);
    assert!(find(&s, "Empty").children.is_empty());
    assert_eq!(edges(&s), [("Empty", "Done", "")]);
}

#[test]
fn a_composite_opens_and_closes_on_one_line() {
    let s = st("state First { A --> B }\nFirst --> Done\n");
    assert_eq!(find(&s, "A").parent, Some(idx(&s, "First")));
    assert_eq!(find(&s, "Done").parent, None);
}

#[test]
fn a_transition_between_members_of_two_composites_is_kept() {
    let s = st("state L {\n  a --> b\n}\nstate R {\n  c --> d\n}\nb --> c\n");
    assert!(edges(&s).contains(&("b", "c", "")));
    assert_eq!(find(&s, "b").parent, Some(idx(&s, "L")));
    assert_eq!(find(&s, "c").parent, Some(idx(&s, "R")));
}

#[test]
fn a_state_declared_before_the_composite_keeps_its_first_parent() {
    let s = st("A --> B\nstate Box {\n  A --> C\n}\n");
    assert_eq!(find(&s, "A").parent, None);
    assert_eq!(find(&s, "C").parent, Some(idx(&s, "Box")));
}

#[test]
fn a_composite_upgrade_keeps_the_states_position() {
    let s = st("A --> Box\nstate Box {\n  x --> y\n}\n");
    assert_eq!(ids(&s), ["A", "Box", "x", "y"]);
    assert_eq!(find(&s, "Box").kind, StateKind::Composite);
}

#[test]
fn a_composite_that_never_closes_is_repaired() {
    let (s, d) = st_d("state First {\n  A --> B\n");
    assert_eq!(find(&s, "A").parent, Some(idx(&s, "First")));
    assert_eq!(count(&d, "R014"), 1, "{:?}", codes(&d));
}

#[test]
fn a_repaired_composite_reparses_clean() {
    assert_fix_round_trip(&body("state First {\n  A --> B\n"), &["R014"]);
}

#[test]
fn every_open_composite_is_closed_at_the_end_of_input() {
    let (_, d) = st_d("state A {\n  state B {\n    x --> y\n");
    assert_eq!(count(&d, "R014"), 2, "{:?}", codes(&d));
}

#[test]
fn a_closing_brace_with_nothing_open_is_dropped() {
    let (s, d) = st_d("A --> B\n}\nB --> C\n");
    assert_eq!(count(&d, "R015"), 1, "{:?}", codes(&d));
    assert_eq!(edges(&s), [("A", "B", ""), ("B", "C", "")]);
}

#[test]
fn a_dropped_closing_brace_reparses_clean() {
    assert_fix_round_trip(&body("A --> B\n}\nB --> C\n"), &["R015"]);
}

#[test]
fn composites_nested_past_the_limit_fail_with_e010() {
    let mut src = String::from("stateDiagram-v2\n");
    for i in 0..70 {
        src.push_str(&format!("state s{i} {{\n"));
    }
    src.push_str("A --> B\n");
    let (r, d) = run(&src, false, Limits::default());
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E010"), "{:?}", codes(&d));
}

#[test]
fn nesting_inside_the_limit_parses() {
    let mut src = String::from("stateDiagram-v2\n");
    for i in 0..60 {
        src.push_str(&format!("state s{i} {{\n"));
    }
    src.push_str("A --> B\n");
    for _ in 0..60 {
        src.push_str("}\n");
    }
    let (s, d) = sm_d(&src);
    assert!(errors(&d).is_empty(), "{:?}", codes(&d));
    assert_eq!(find(&s, "A").parent, Some(idx(&s, "s59")));
}

// ----------------------------------------------------------------- concurrency

#[test]
fn a_divider_splits_a_composite_into_regions() {
    let s = st("state Active {\n  [*] --> NumLockOff\n  --\n  [*] --> CapsLockOff\n}\n");
    assert_eq!(s.regions.len(), 2);
    assert_eq!(s.regions[0].parent, idx(&s, "Active"));
    assert_eq!(s.regions[0].index, 0);
    assert_eq!(s.regions[1].index, 1);
    assert_eq!(find(&s, "NumLockOff").region, Some(0));
    assert_eq!(find(&s, "CapsLockOff").region, Some(1));
}

#[test]
fn k_dividers_make_k_plus_one_regions() {
    let s = st("state A {\n  a1\n  --\n  b1\n  --\n  c1\n}\n");
    assert_eq!(s.regions.len(), 3);
    assert_eq!(
        s.regions.iter().map(|r| r.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn an_empty_region_is_dropped() {
    let s = st("state A {\n  a1\n  --\n  --\n  b1\n}\n");
    assert_eq!(s.regions.len(), 2);
    assert_eq!(find(&s, "a1").region, Some(0));
    assert_eq!(find(&s, "b1").region, Some(1));
}

#[test]
fn a_composite_left_with_one_region_keeps_its_members_directly() {
    let s = st("state A {\n  a1 --> a2\n  --\n}\n");
    assert!(s.regions.is_empty());
    assert_eq!(find(&s, "a1").region, None);
    assert_eq!(find(&s, "a1").parent, Some(idx(&s, "A")));
}

#[test]
fn each_region_resolves_its_own_start_and_end() {
    let s = st(
        "state Active {\n  [*] --> Num\n  Num --> [*]\n  --\n  [*] --> Caps\n  Caps --> [*]\n}\n",
    );
    assert!(s.state_index("Active_r0_start").is_some(), "{:?}", ids(&s));
    assert!(s.state_index("Active_r0_end").is_some(), "{:?}", ids(&s));
    assert!(s.state_index("Active_r1_start").is_some(), "{:?}", ids(&s));
    assert!(s.state_index("Active_r1_end").is_some(), "{:?}", ids(&s));
}

#[test]
fn a_regions_states_list_holds_its_direct_members() {
    let s = st("state A {\n  a1 --> a2\n  --\n  b1\n}\n");
    assert_eq!(s.regions[0].states, vec![idx(&s, "a1"), idx(&s, "a2")]);
    assert_eq!(s.regions[1].states, vec![idx(&s, "b1")]);
}

#[test]
fn a_composites_children_span_every_region_in_declaration_order() {
    let s = st("state A {\n  a1\n  --\n  b1\n}\n");
    assert_eq!(
        s.states[idx(&s, "A")].children,
        vec![idx(&s, "a1"), idx(&s, "b1")]
    );
}

#[test]
fn a_divider_outside_a_composite_is_dropped() {
    let (s, d) = st_d("A --> B\n--\nB --> C\n");
    assert_eq!(count(&d, "R016"), 1, "{:?}", codes(&d));
    assert!(s.regions.is_empty());
    assert_eq!(edges(&s), [("A", "B", ""), ("B", "C", "")]);
}

#[test]
fn a_dropped_divider_reparses_clean() {
    assert_fix_round_trip(&body("A --> B\n--\nB --> C\n"), &["R016"]);
}

// ----------------------------------------------------------------- choice, fork, join

#[test]
fn the_three_markers_set_the_kind() {
    for (marker, kind) in [
        ("<<choice>>", StateKind::Choice),
        ("<<fork>>", StateKind::Fork),
        ("<<join>>", StateKind::Join),
    ] {
        let s = st(&format!("state x {marker}\n"));
        assert_eq!(find(&s, "x").kind, kind, "{marker}");
        assert_eq!(find(&s, "x").label, "", "{marker} draws no label");
    }
}

#[test]
fn the_bracket_spelling_is_the_same_marker() {
    for (marker, kind) in [
        ("[[choice]]", StateKind::Choice),
        ("[[fork]]", StateKind::Fork),
        ("[[join]]", StateKind::Join),
    ] {
        let s = st(&format!("state x {marker}\n"));
        assert_eq!(find(&s, "x").kind, kind, "{marker}");
    }
}

#[test]
fn a_marker_is_case_insensitive() {
    let s = st("state x <<CHOICE>>\n");
    assert_eq!(find(&s, "x").kind, StateKind::Choice);
}

#[test]
fn a_marker_naming_anything_else_leaves_the_state_simple() {
    let (s, d) = st_d("state x <<history>>\n");
    assert_eq!(find(&s, "x").kind, StateKind::Simple);
    assert_eq!(count(&d, "W025"), 1, "{:?}", codes(&d));
}

#[test]
fn a_marker_without_an_id_fails() {
    let (r, d) = try_parse(&body("state <<fork>>\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

#[test]
fn a_marker_state_takes_transitions_like_any_other() {
    let s = st("state f <<fork>>\nA --> f\nf --> B\nf --> C\n");
    assert_eq!(edges(&s), [("A", "f", ""), ("f", "B", ""), ("f", "C", "")]);
}

#[test]
fn a_marker_without_a_space_before_it_still_parses() {
    let s = st("state x<<join>>\n");
    assert_eq!(find(&s, "x").kind, StateKind::Join);
}

// ----------------------------------------------------------------- notes

#[test]
fn a_right_note_is_placed_after_its_state() {
    let s = st("A --> B\nnote right of A : the first step\n");
    assert_eq!(s.notes.len(), 1);
    assert_eq!(s.notes[0].placement, NotePlacement::After);
    assert_eq!(s.notes[0].state, idx(&s, "A"));
    assert_eq!(s.notes[0].text, "the first step");
}

#[test]
fn a_left_note_is_placed_before_its_state() {
    let s = st("A --> B\nnote left of B : the last step\n");
    assert_eq!(s.notes[0].placement, NotePlacement::Before);
}

#[test]
fn a_block_note_keeps_its_line_breaks() {
    let s = st("A\nnote right of A\n  Important information! You can\n  write notes.\nend note\n");
    assert_eq!(
        s.notes[0].text,
        "Important information! You can\nwrite notes."
    );
}

#[test]
fn end_note_is_case_insensitive() {
    let s = st("A\nnote right of A\n  text\nEND NOTE\n");
    assert_eq!(s.notes[0].text, "text");
}

#[test]
fn a_block_note_left_open_closes_at_the_end_of_input() {
    let s = st("A\nnote right of A\n  text\n");
    assert_eq!(s.notes[0].text, "text");
}

#[test]
fn a_note_on_an_undeclared_state_declares_it() {
    let s = st("note right of Ghost : nobody linked me\n");
    assert_eq!(ids(&s), ["Ghost"]);
    assert!(find(&s, "Ghost").implicit);
}

#[test]
fn several_notes_on_one_state_keep_source_order() {
    let s = st("A\nnote right of A : one\nnote right of A : two\nnote left of A : three\n");
    assert_eq!(
        s.notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
        ["one", "two", "three"]
    );
}

#[test]
fn a_floating_note_is_dropped() {
    let (s, d) = st_d("A --> B\nnote \"free text\" as N1\n");
    assert!(s.notes.is_empty());
    assert_eq!(count(&d, "W024"), 1, "{:?}", codes(&d));
    assert!(s.state_index("N1").is_none(), "{:?}", ids(&s));
}

#[test]
fn a_note_without_left_or_right_fails() {
    let (r, d) = try_parse(&body("A\nnote over A : x\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

#[test]
fn a_note_is_not_a_graph_element() {
    let s = st("A --> B\nnote right of A : x\n");
    assert_eq!(s.states.len(), 2);
    assert_eq!(s.transitions.len(), 1);
}

// ----------------------------------------------------------------- direction

#[test]
fn a_top_level_direction_sets_the_diagram_direction() {
    for (tok, dir) in [
        ("TB", Direction::TB),
        ("TD", Direction::TB),
        ("BT", Direction::BT),
        ("LR", Direction::LR),
        ("RL", Direction::RL),
    ] {
        let s = st(&format!("direction {tok}\nA --> B\n"));
        assert_eq!(s.direction, dir, "{tok}");
    }
}

#[test]
fn a_direction_inside_a_composite_is_recorded_on_the_composite() {
    let s = st("direction LR\nstate Box {\n  direction TB\n  A --> B\n}\n");
    assert_eq!(s.direction, Direction::LR);
    assert_eq!(find(&s, "Box").direction, Some(Direction::TB));
}

#[test]
fn a_direction_outside_a_composite_leaves_every_state_unset() {
    let s = st("direction RL\nA --> B\n");
    assert!(s.states.iter().all(|x| x.direction.is_none()));
}

#[test]
fn an_unknown_direction_fails() {
    let (r, d) = try_parse(&body("direction sideways\nA --> B\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

// ----------------------------------------------------------------- styling

#[test]
fn class_def_collects_a_named_style() {
    let s = st("classDef risky stroke-width:3px\nA --> B\n");
    assert_eq!(s.class_defs.len(), 1);
    assert_eq!(s.class_defs[0].name, "risky");
    assert_eq!(s.class_defs[0].style.stroke_width, Some(3.0));
}

#[test]
fn a_second_class_def_with_the_same_name_merges() {
    let s = st("classDef a stroke-width:1px\nclassDef a font-weight:bold\nA\n");
    assert_eq!(s.class_defs.len(), 1);
    assert_eq!(s.class_defs[0].style.stroke_width, Some(1.0));
    assert!(s.class_defs[0].style.font_weight.is_some());
}

#[test]
fn class_def_names_a_list() {
    let s = st("classDef one,two stroke-width:1px\nA\n");
    assert_eq!(
        s.class_defs
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
}

#[test]
fn a_class_statement_applies_a_role_to_every_id_it_names() {
    let s = st("classDef t stroke-width:1px\nA --> B\nclass A,B t\n");
    assert_eq!(find(&s, "A").classes, vec![String::from("t")]);
    assert_eq!(find(&s, "B").classes, vec![String::from("t")]);
}

#[test]
fn the_triple_colon_shorthand_applies_a_role_where_the_id_stands() {
    let s = st("classDef t stroke-width:1px\nA:::t --> B:::t\n");
    assert_eq!(find(&s, "A").classes, vec![String::from("t")]);
    assert_eq!(find(&s, "B").classes, vec![String::from("t")]);
    assert_eq!(edges(&s), [("A", "B", "")]);
}

#[test]
fn the_triple_colon_shorthand_works_on_a_bare_declaration() {
    let s = st("classDef t stroke-width:1px\nA:::t\n");
    assert_eq!(ids(&s), ["A"]);
    assert_eq!(find(&s, "A").classes, vec![String::from("t")]);
}

#[test]
fn a_role_reaches_a_composite_state() {
    let s = st("classDef t stroke-width:1px\nstate Box {\n  A --> B\n}\nclass Box t\n");
    assert_eq!(find(&s, "Box").classes, vec![String::from("t")]);
}

#[test]
fn a_role_reaches_a_generated_start_state() {
    let s = st("classDef t stroke-width:1px\n[*] --> A\nclass root_start t\n");
    assert_eq!(find(&s, "root_start").classes, vec![String::from("t")]);
}

#[test]
fn a_style_statement_reaches_the_state_it_names() {
    let s = st("A --> B\nstyle A stroke-width:2px\n");
    assert_eq!(find(&s, "A").style.stroke_width, Some(2.0));
}

#[test]
fn a_style_statement_names_a_list() {
    let s = st("A --> B\nstyle A,B stroke-width:2px\n");
    assert_eq!(find(&s, "B").style.stroke_width, Some(2.0));
}

#[test]
fn a_style_target_that_is_not_a_state_is_rejected() {
    let (_, d) = st_d("A --> B\nstyle Ghost stroke-width:2px\n");
    assert_eq!(count(&d, "W010"), 1, "{:?}", codes(&d));
}

#[test]
fn a_class_name_outside_the_grammar_is_rejected() {
    let (_, d) = st_d("classDef 9bad stroke-width:1px\nA\n");
    assert_eq!(count(&d, "W011"), 1, "{:?}", codes(&d));
}

#[test]
fn a_class_statement_with_a_bad_name_is_rejected() {
    let (s, d) = st_d("A\nclass A 9bad\n");
    assert_eq!(count(&d, "W011"), 1, "{:?}", codes(&d));
    assert!(find(&s, "A").classes.is_empty());
}

#[test]
fn a_style_property_outside_the_set_is_rejected() {
    let (_, d) = st_d("A\nstyle A border-radius:4px\n");
    assert_eq!(count(&d, "W010"), 1, "{:?}", codes(&d));
}

#[test]
fn a_fixed_colour_is_reported_once() {
    let (_, d) = st_d("classDef a fill:#ff0000\nclassDef b fill:#00ff00\nA\n");
    assert_eq!(count(&d, "I030"), 1, "{:?}", codes(&d));
}

#[test]
fn an_element_keeps_at_most_thirty_two_classes() {
    let mut b = String::new();
    for i in 0..40 {
        b.push_str(&format!("classDef c{i} stroke-width:1px\n"));
    }
    b.push_str("A\n");
    for i in 0..40 {
        b.push_str(&format!("class A c{i}\n"));
    }
    let (s, d) = st_d(&b);
    assert_eq!(find(&s, "A").classes.len(), 32);
    assert_eq!(count(&d, "W020"), 1, "{:?}", codes(&d));
}

#[test]
fn a_class_naming_an_unknown_state_is_ignored() {
    let (s, d) = st_d("classDef t stroke-width:1px\nA\nclass Ghost t\n");
    assert!(!has(&d, "E002"), "{:?}", codes(&d));
    assert!(find(&s, "A").classes.is_empty());
}

#[test]
fn the_state_grammar_has_no_link_style() {
    let (r, d) = try_parse(&body("A --> B\nlinkStyle 0 stroke-width:2px\n"));
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "E002"), "{:?}", codes(&d));
}

// ----------------------------------------------------------------- links

#[test]
fn a_click_href_wraps_the_state_in_a_link() {
    let s = st("A --> B\nclick A href \"https://example.com/a\"\n");
    let link = find(&s, "A").link.as_ref().expect("link");
    assert_eq!(link.url, "https://example.com/a");
    assert!(!link.target_blank);
}

#[test]
fn a_click_target_of_blank_is_kept() {
    let s = st("A\nclick A href \"https://example.com/a\" _blank\n");
    assert!(find(&s, "A").link.as_ref().expect("link").target_blank);
}

#[test]
fn a_click_tooltip_is_dropped() {
    let (s, d) = st_d("A\nclick A \"https://example.com/a\" \"a tooltip\"\n");
    assert!(find(&s, "A").link.is_some());
    assert_eq!(count(&d, "W024"), 1, "{:?}", codes(&d));
}

#[test]
fn a_rejected_url_is_dropped() {
    let (s, d) = st_d("A\nclick A href \"javascript:alert(1)\"\n");
    assert!(find(&s, "A").link.is_none());
    assert_eq!(count(&d, "W013"), 1, "{:?}", codes(&d));
}

// ----------------------------------------------------------------- accessibility and dropped statements

#[test]
fn acc_title_and_acc_descr_reach_the_meta() {
    let s = st("accTitle: A title\naccDescr: A description\nA --> B\n");
    assert_eq!(s.meta.acc_title.as_deref(), Some("A title"));
    assert_eq!(s.meta.acc_descr.as_deref(), Some("A description"));
}

#[test]
fn an_acc_descr_block_joins_its_lines() {
    let s = sm("stateDiagram-v2\naccDescr {\n  one\n  two\n}\nA --> B\n");
    assert_eq!(s.meta.acc_descr.as_deref(), Some("one\ntwo"));
}

#[test]
fn front_matter_reaches_the_state_meta() {
    let s = sm("---\ntitle: Traffic light\n---\nstateDiagram-v2\n[*] --> Red\n");
    assert_eq!(s.meta.title.as_deref(), Some("Traffic light"));
}

#[test]
fn hide_empty_description_is_dropped() {
    let (s, d) = st_d("hide empty description\nA --> B\n");
    assert_eq!(count(&d, "W024"), 1, "{:?}", codes(&d));
    assert_eq!(edges(&s), [("A", "B", "")]);
}

#[test]
fn scale_width_is_dropped() {
    let (s, d) = st_d("scale 350 width\nA --> B\n");
    assert_eq!(count(&d, "W024"), 1, "{:?}", codes(&d));
    assert_eq!(ids(&s), ["A", "B"]);
}

#[test]
fn a_title_statement_sets_the_meta_title() {
    let s = st("title Traffic light\nA --> B\n");
    assert_eq!(s.meta.title.as_deref(), Some("Traffic light"));
}

// ----------------------------------------------------------------- limits

#[test]
fn more_states_than_the_limit_is_too_large() {
    let mut src = String::from("stateDiagram-v2\n");
    for i in 0..40 {
        src.push_str(&format!("s{i}\n"));
    }
    let limits = Limits {
        nodes: 10,
        ..Limits::default()
    };
    let (r, _) = run(&src, false, limits);
    assert_eq!(r, Err(ParseError::TooLarge { what: "states" }));
}

#[test]
fn more_transitions_than_the_limit_is_too_large() {
    let mut src = String::from("stateDiagram-v2\n");
    for i in 0..40 {
        src.push_str(&format!("a{i} --> b{i}\n"));
    }
    let limits = Limits {
        edges: 5,
        ..Limits::default()
    };
    let (r, _) = run(&src, false, limits);
    assert_eq!(
        r,
        Err(ParseError::TooLarge {
            what: "transitions"
        })
    );
}

#[test]
fn more_notes_than_the_limit_is_too_large() {
    let mut src = String::from("stateDiagram-v2\na\n");
    for i in 0..40 {
        src.push_str(&format!("note left of a : n{i}\n"));
    }
    let limits = Limits {
        notes: 10,
        ..Limits::default()
    };
    let (r, _) = run(&src, false, limits);
    assert_eq!(r, Err(ParseError::TooLarge { what: "notes" }));
}

/// A note draws a box the size of its text, so a source of nothing but notes reaches the
/// limit long before it reaches `input_bytes` and never grows an SVG without bound.
#[test]
fn a_source_of_nothing_but_notes_stops_at_the_limit() {
    let mut src = String::from("stateDiagram-v2\na\n");
    for _ in 0..(Limits::default().notes + 1) {
        src.push_str("note left of a : x\n");
    }
    let (r, _) = run(&src, false, Limits::default());
    assert_eq!(r, Err(ParseError::TooLarge { what: "notes" }));
}

#[test]
fn a_label_longer_than_the_limit_is_truncated() {
    let long = "x".repeat(200);
    let limits = Limits {
        label_bytes: 32,
        ..Limits::default()
    };
    let (r, d) = run(&body(&format!("A --> B : {long}\n")), false, limits);
    let s = match r {
        Ok(Diagram::State(s)) => s,
        other => panic!("{other:?}"),
    };
    assert_eq!(s.transitions[0].label.as_deref().map(str::len), Some(32));
    assert!(has(&d, "W012"), "{:?}", codes(&d));
}

#[test]
fn a_note_longer_than_the_limit_is_truncated() {
    let long = "y".repeat(200);
    let limits = Limits {
        label_bytes: 16,
        ..Limits::default()
    };
    let (r, d) = run(
        &body(&format!("A\nnote right of A : {long}\n")),
        false,
        limits,
    );
    let s = match r {
        Ok(Diagram::State(s)) => s,
        other => panic!("{other:?}"),
    };
    assert_eq!(s.notes[0].text.len(), 16);
    assert!(has(&d, "W012"), "{:?}", codes(&d));
}

// ----------------------------------------------------------------- strict mode

#[test]
fn strict_mode_turns_every_repair_into_an_error() {
    for src in [
        "A ->> B\n",
        "A --> B done\n",
        "state X {\n  A --> B\n",
        "}\n",
        "--\n",
    ] {
        let (r, d) = run(&body(src), true, Limits::default());
        assert!(r.is_err(), "{src:?}: {r:?}");
        assert!(!errors(&d).is_empty(), "{src:?}: {:?}", codes(&d));
    }
}

#[test]
fn strict_mode_turns_a_warning_into_an_error() {
    let (r, d) = run(
        &body("hide empty description\nA\n"),
        true,
        Limits::default(),
    );
    assert!(r.is_err(), "{r:?}");
    assert!(has(&d, "W024"), "{:?}", codes(&d));
    assert_eq!(errors(&d).len(), 1);
}

#[test]
fn a_clean_diagram_parses_in_strict_mode() {
    let (r, d) = run(
        &body("[*] --> A\nA --> B : go\nB --> [*]\n"),
        true,
        Limits::default(),
    );
    assert!(r.is_ok(), "{r:?}\n{d:#?}");
    assert!(d.is_empty(), "{d:#?}");
}

// ----------------------------------------------------------------- spans

#[test]
fn a_state_span_points_at_its_declaration() {
    let s = sm("stateDiagram-v2\nstate Alpha\n");
    let sp = find(&s, "Alpha").span;
    assert_eq!(sp.line, 2);
    assert_eq!(sp.column, 1);
}

#[test]
fn a_transition_span_covers_the_whole_statement() {
    let src = "stateDiagram-v2\nA --> B : go\n";
    let s = sm(src);
    let sp = s.transitions[0].span;
    assert_eq!(sp.line, 2);
    assert_eq!(
        &src[sp.byte_start as usize..sp.byte_end as usize],
        "A --> B : go"
    );
}

#[test]
fn a_note_span_covers_the_block() {
    let src = "stateDiagram-v2\nA\nnote right of A\n  text\nend note\n";
    let s = sm(src);
    let sp = s.notes[0].span;
    assert_eq!(sp.line, 3);
    assert!(
        src[sp.byte_start as usize..sp.byte_end as usize].ends_with("end note"),
        "{:?}",
        &src[sp.byte_start as usize..sp.byte_end as usize]
    );
}

#[test]
fn a_diagnostic_span_points_at_the_offending_text() {
    let src = "stateDiagram-v2\nA ->> B\n";
    let (_, d) = sm_d(src);
    let r = d.iter().find(|x| x.code == "R018").expect("R018");
    assert_eq!(
        &src[r.span.byte_start as usize..r.span.byte_end as usize],
        "->>"
    );
}

// ----------------------------------------------------------------- robustness

#[test]
fn adversarial_input_never_panics() {
    let cases = [
        "stateDiagram-v2",
        "stateDiagram-v2\n",
        "stateDiagram-v2\n[*]\n",
        "stateDiagram-v2\n[*] -->\n",
        "stateDiagram-v2\nstate\n",
        "stateDiagram-v2\nstate \"unterminated as X\n",
        "stateDiagram-v2\nstate <<>>\n",
        "stateDiagram-v2\nnote\n",
        "stateDiagram-v2\nnote right\n",
        "stateDiagram-v2\nnote right of\n",
        "stateDiagram-v2\nnote right of A\n",
        "stateDiagram-v2\n}\n}\n}\n",
        "stateDiagram-v2\n--\n--\n",
        "stateDiagram-v2\n{\n",
        "stateDiagram-v2\nA --> B : \u{202e}rtl\n",
        "stateDiagram-v2\nclass\n",
        "stateDiagram-v2\nclassDef\n",
        "stateDiagram-v2\nstyle\n",
        "stateDiagram-v2\nclick\n",
        "stateDiagram-v2\ndirection\n",
        "stateDiagram-v2\nA:::\n",
        "stateDiagram-v2\n:::x\n",
        "stateDiagram-v2\n-->\n",
        "stateDiagram-v2\nstate x <<fork\n",
        "stateDiagram-v2\né --> ü : 🙂\n",
    ];
    for src in cases {
        let (_, d) = try_parse(src);
        let _ = d.len();
    }
}

#[test]
fn a_one_line_diagram_stays_linear() {
    let mut src = String::from("stateDiagram-v2\n");
    for i in 0..500 {
        src.push_str(&format!("a{i} --> a{};", i + 1));
    }
    let (s, d) = sm_d(&src);
    assert_eq!(s.transitions.len(), 500);
    assert!(errors(&d).is_empty(), "{:?}", codes(&d));
}

/// A statement separated by `;` costs what the same statement separated by `\n` costs:
/// both end the line lookup in constant time, so neither shape is quadratic in the
/// length of its line (specs/state.md#syntax).
#[test]
fn a_one_line_diagram_costs_what_the_same_lines_cost() {
    const N: usize = 100_000;
    let one_line = format!("stateDiagram-v2\n{}", "a;".repeat(N));
    let many_lines = format!("stateDiagram-v2\n{}", "a\n".repeat(N));
    assert_eq!(one_line.len(), many_lines.len());

    let time = |src: &str| {
        let t = std::time::Instant::now();
        let (s, _) = sm_d(src);
        assert_eq!(s.states.len(), 1);
        t.elapsed()
    };
    // Parse each shape twice and keep the faster run, so a cold cache or a descheduled
    // first run cannot decide the comparison.
    let lines = time(&many_lines).min(time(&many_lines));
    let inline = time(&one_line).min(time(&one_line));
    // 16× leaves room for the constant factors a line lookup adds; the quadratic scan
    // this guards against costs `N / 2` times more, which is four orders of magnitude.
    // 4× leaves room for the constant factors a line lookup adds; a scan to the end of
    // the line instead costs 8× here and grows with `N`.
    assert!(
        inline <= lines * 4 + core::time::Duration::from_millis(50),
        "{N} `;`-separated statements took {inline:?}, the same statements on their own \
         lines took {lines:?}: the line lookup is not constant time"
    );
}

#[test]
fn a_composite_that_holds_every_statement_form_parses() {
    let s = st(
        "state Box {\n  direction LR\n  classDef inner stroke-width:1px\n  [*] --> a\n  a : an inner state\n  a --> b : go\n  note right of a : a note\n  class a inner\n  b --> [*]\n}\n",
    );
    assert_eq!(find(&s, "Box").direction, Some(Direction::LR));
    assert_eq!(find(&s, "a").label, "an inner state");
    assert_eq!(find(&s, "a").classes, vec![String::from("inner")]);
    assert_eq!(s.notes.len(), 1);
    assert_eq!(s.class_defs.len(), 1);
}

#[test]
fn a_blank_body_gives_an_empty_machine() {
    let s = sm("stateDiagram-v2\n\n\n");
    assert!(s.states.is_empty() && s.transitions.is_empty());
    assert_eq!(s.direction, Direction::TB);
}

#[test]
fn every_repair_carries_a_fix() {
    let src = body("A ->> B done\n}\n--\nstate X {\n  C --> D\n");
    let (_, d) = sm_d(&src);
    for x in d.iter().filter(|x| x.severity == Severity::Repair) {
        assert!(x.fix.is_some(), "{x:?}");
    }
    for c in ["R014", "R015", "R016", "R017", "R018"] {
        assert!(has(&d, c), "{c} missing from {:?}", codes(&d));
    }
}

#[test]
fn the_five_repairs_converge_together() {
    assert_fix_round_trip(
        &body("A ->> B done\n}\n--\nstate X {\n  C --> D\n"),
        &["R014", "R015", "R016", "R017", "R018"],
    );
}
