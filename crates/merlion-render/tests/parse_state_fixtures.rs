//! The state corpus in `tests/fixtures/state/` parses, and applying every fix leaves
//! source that parses without repairs (specs/state.md#testing).

mod parse_support;

use std::path::PathBuf;

use merlion_render::diag::{Diagnostic, Severity};
use merlion_render::model::state::{StateKind, StateMachine};
use merlion_render::model::Diagram;
use merlion_render::options::Direction;
use parse_support::{codes, errors, fixed, has, try_parse};

fn fixtures() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/state");
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

fn parse_ok(src: &str) -> (StateMachine, Vec<Diagnostic>) {
    let (r, d) = try_parse(src);
    match r {
        Ok(Diagram::State(s)) => (s, d),
        Ok(other) => panic!("expected a state diagram, got a {}", other.type_name()),
        Err(e) => panic!("parse failed: {e:?}\n{d:#?}"),
    }
}

fn get(name: &str) -> String {
    fixtures()
        .into_iter()
        .find(|(k, _)| k == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
        .1
}

fn ids(s: &StateMachine) -> Vec<&str> {
    s.states.iter().map(|x| x.id.as_str()).collect()
}

fn find<'a>(s: &'a StateMachine, id: &str) -> &'a merlion_render::model::state::State {
    s.states
        .iter()
        .find(|x| x.id == id)
        .unwrap_or_else(|| panic!("no state {id:?} in {:?}", ids(s)))
}

#[test]
fn corpus_size() {
    let n = fixtures().len();
    assert!((15..=24).contains(&n), "{n} fixtures");
}

#[test]
fn every_fixture_parses() {
    for (name, src) in fixtures() {
        let (s, d) = parse_ok(&src);
        assert!(s.states.len() >= 3, "{name}: {} states", s.states.len());
        assert!(!s.transitions.is_empty(), "{name}");
        assert!(errors(&d).is_empty(), "{name}: {d:#?}");
    }
}

#[test]
fn every_state_index_a_transition_names_is_in_range() {
    for (name, src) in fixtures() {
        let (s, _) = parse_ok(&src);
        for t in &s.transitions {
            assert!(t.from < s.states.len() && t.to < s.states.len(), "{name}");
        }
        for n in &s.notes {
            assert!(n.state < s.states.len(), "{name}");
        }
        for r in &s.regions {
            assert!(r.parent < s.states.len(), "{name}");
            assert!(r.states.iter().all(|&i| i < s.states.len()), "{name}");
        }
    }
}

#[test]
fn every_id_is_unique_and_a_parent_precedes_its_children() {
    for (name, src) in fixtures() {
        let (s, _) = parse_ok(&src);
        let mut seen = std::collections::BTreeSet::new();
        for (i, x) in s.states.iter().enumerate() {
            assert!(!x.id.is_empty(), "{name}: state {i} has no id");
            assert!(seen.insert(x.id.clone()), "{name}: duplicate id {:?}", x.id);
            if let Some(p) = x.parent {
                assert!(p < i, "{name}: {:?} precedes its parent", x.id);
                assert!(
                    s.states[p].children.contains(&i),
                    "{name}: {:?} is missing from its parent",
                    x.id
                );
            }
        }
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
        if name.starts_with("llm-") {
            assert!(
                d.iter().any(|x| x.severity == Severity::Repair),
                "{name} should exercise repairs"
            );
        } else {
            assert!(d.is_empty(), "{name}: {:?}", codes(&d));
        }
    }
}

#[test]
fn the_repair_fixtures_cover_every_state_repair() {
    let mut seen = std::collections::BTreeSet::new();
    for (name, src) in fixtures() {
        if !name.starts_with("llm-") {
            continue;
        }
        let (_, d) = parse_ok(&src);
        for x in d.iter().filter(|x| x.severity == Severity::Repair) {
            seen.insert(x.code);
        }
    }
    for c in ["R014", "R015", "R016", "R017", "R018", "R019"] {
        assert!(seen.contains(c), "{c} is never exercised: {seen:?}");
    }
}

#[test]
fn the_composite_fixtures_nest_and_carry_their_members() {
    let (s, _) = parse_ok(&get("elevator.mmd"));
    let moving = s.state_index("Moving").expect("Moving");
    let cruising = s.state_index("Cruising").expect("Cruising");
    assert_eq!(find(&s, "Cruising").parent, Some(moving));
    assert_eq!(find(&s, "Level").parent, Some(cruising));
    assert_eq!(find(&s, "Moving").kind, StateKind::Composite);
    assert!(s.state_index("Cruising_start").is_some(), "{:?}", ids(&s));
}

#[test]
fn the_concurrency_fixture_has_three_regions_with_their_own_pseudo_states() {
    let (s, _) = parse_ok(&get("keyboard-concurrency.mmd"));
    assert_eq!(s.regions.len(), 3);
    assert!(s
        .regions
        .iter()
        .all(|r| r.parent == s.state_index("Active").unwrap()));
    for k in 0..3 {
        assert!(
            s.state_index(&format!("Active_r{k}_start")).is_some(),
            "{:?}",
            ids(&s)
        );
    }
    assert_eq!(find(&s, "NumLockOff").region, Some(0));
    assert_eq!(find(&s, "ScrollLockOff").region, Some(2));
}

#[test]
fn the_marker_fixture_carries_one_state_of_each_kind() {
    let (s, _) = parse_ok(&get("choice-fork-join.mmd"));
    assert_eq!(find(&s, "score_gate").kind, StateKind::Choice);
    assert_eq!(find(&s, "fan_out").kind, StateKind::Fork);
    assert_eq!(find(&s, "fan_in").kind, StateKind::Join);
    assert!(find(&s, "score_gate").label.is_empty());
}

#[test]
fn the_note_fixture_carries_a_block_note_and_an_inline_one() {
    let (s, _) = parse_ok(&get("notes-and-descriptions.mmd"));
    assert_eq!(s.notes.len(), 2);
    assert!(s.notes[0].text.contains('\n'), "{:?}", s.notes[0].text);
    assert_eq!(find(&s, "Draft").label, "Waiting for the author");
    assert_eq!(find(&s, "Review").label, "Two approvals are required");
}

#[test]
fn the_styled_fixture_carries_its_roles_and_styles() {
    let (s, _) = parse_ok(&get("styled-roles.mmd"));
    assert_eq!(s.class_defs.len(), 2);
    assert_eq!(find(&s, "Screening").classes, vec![String::from("risky")]);
    assert_eq!(find(&s, "Approved").classes, vec![String::from("terminal")]);
    assert!(find(&s, "Received").style.stroke_dasharray.is_some());
}

#[test]
fn the_accessible_fixture_carries_its_meta_and_link() {
    let (s, _) = parse_ok(&get("accessible-review.mmd"));
    assert_eq!(s.meta.title.as_deref(), Some("Access review"));
    assert_eq!(s.meta.acc_title.as_deref(), Some("Quarterly access review"));
    assert!(s.meta.acc_descr.is_some());
    assert!(find(&s, "UnderReview")
        .link
        .as_ref()
        .is_some_and(|l| l.target_blank));
}

#[test]
fn the_direction_fixture_sets_both_levels() {
    let (s, _) = parse_ok(&get("ingest-direction.mmd"));
    assert_eq!(s.direction, Direction::LR);
    assert_eq!(find(&s, "Ingest").direction, Some(Direction::TB));
}

#[test]
fn the_mixed_repair_fixture_reports_three_kinds_of_repair() {
    let (_, d) = parse_ok(&get("llm-mixed-mistakes.mmd"));
    for c in ["R014", "R017", "R018"] {
        assert!(has(&d, c), "{c} missing from {:?}", codes(&d));
    }
}
