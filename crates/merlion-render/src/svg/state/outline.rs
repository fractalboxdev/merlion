//! The plain-text outline of a state machine (specs/state.md#text-alternative).
//!
//! ```text
//! State diagram, top to bottom. 9 states, 8 transitions.
//! start → Draft
//! Draft → Submitted [submit]
//! Submitted → Review
//! Review → Published [approved]; → Draft [rejected]
//! Review: start → Screening
//! Review: Screening → Decision
//! Review: Decision
//! Published → end
//! Note right of Draft: Waits for the author
//! ```
//!
//! The outline reads the state machine, not the lowered graph: a transition naming a
//! composite state prints that state's name rather than its first member, and a
//! generated `[*]` state prints as `start` or `end` rather than as `root_start`.
//!
//! It describes the drawing, so it lists and counts only the transitions the lowering
//! keeps: one between a composite state and a state nested inside it is not drawn
//! ([`crate::layout::state::transition_is_dropped`]) and a closing line says how many
//! there were, rather than naming an edge no sighted reader can find.
//!
//! A state gets a line when it has outgoing transitions, has none at all, or sits inside
//! a composite state, the flowchart rule; a state inside one is prefixed with its
//! composite path as a heading, so every line stands on its own when read aloud. The
//! notes follow, one line each, in source order.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::model::state::{NotePlacement, StateKind, StateMachine};
use crate::options::Direction;

use super::super::outline::plain_label;

fn direction_phrase(d: Direction) -> &'static str {
    match d {
        Direction::TB => "top to bottom",
        Direction::BT => "bottom to top",
        Direction::LR => "left to right",
        Direction::RL => "right to left",
    }
}

fn count(out: &mut String, n: usize, one: &str, many: &str) {
    let _ = write!(out, "{} {}", n, if n == 1 { one } else { many });
}

/// How a state reads. `[*]` prints as `start` or `end`, and a state that draws no label
/// prints as its id followed by its kind, since the reader has nothing else to go on.
fn state_name(sm: &StateMachine, i: usize) -> String {
    let Some(s) = sm.states.get(i) else {
        return String::from("?");
    };
    match s.kind {
        StateKind::Start => String::from("start"),
        StateKind::End => String::from("end"),
        StateKind::Choice | StateKind::Fork | StateKind::Join => {
            let mut out = plain_label(&s.id);
            let _ = write!(out, " ({})", s.kind.as_str());
            out
        }
        StateKind::Simple | StateKind::Composite => {
            let l = plain_label(&s.label);
            if l.is_empty() {
                plain_label(&s.id)
            } else {
                l
            }
        }
    }
}

/// The composite states enclosing `i`, outermost first, joined by ` / `. The chain is
/// bounded by the number of states, so a parent link that cycles still terminates.
fn composite_path(sm: &StateMachine, i: usize) -> String {
    let mut names: Vec<String> = Vec::new();
    let mut cur = sm.states.get(i).and_then(|s| s.parent);
    for _ in 0..sm.states.len() {
        let Some(p) = cur.filter(|&p| p < sm.states.len() && p != i) else {
            break;
        };
        names.push(state_name(sm, p));
        cur = sm.states.get(p).and_then(|s| s.parent);
    }
    names.reverse();
    names.join(" / ")
}

fn placement_word(p: NotePlacement) -> &'static str {
    match p {
        NotePlacement::Before => "left of",
        NotePlacement::After => "right of",
    }
}

/// The outline of `sm`, in the source direction.
pub fn outline(sm: &StateMachine) -> String {
    let n = sm.states.len();
    let mut outgoing: Vec<Vec<usize>> = alloc::vec![Vec::new(); n];
    let mut has_edge = alloc::vec![false; n];
    let mut drawn = 0usize;
    let mut left_out = 0usize;
    for (ti, t) in sm.transitions.iter().enumerate() {
        if crate::layout::state::transition_is_dropped(sm, ti) {
            left_out += 1;
            continue;
        }
        drawn += 1;
        if let Some(v) = outgoing.get_mut(t.from) {
            v.push(ti);
        }
        for end in [t.from, t.to] {
            if let Some(h) = has_edge.get_mut(end) {
                *h = true;
            }
        }
    }

    let mut out = String::from("State diagram, ");
    out.push_str(direction_phrase(sm.direction));
    out.push_str(". ");
    count(&mut out, sm.states.len(), "state", "states");
    out.push_str(", ");
    count(&mut out, drawn, "transition", "transitions");
    out.push('.');

    for (i, s) in sm.states.iter().enumerate() {
        let outs = outgoing.get(i).map(Vec::as_slice).unwrap_or(&[]);
        let isolated = !has_edge.get(i).copied().unwrap_or(false);
        let inside = s.parent.is_some_and(|p| p < n && p != i);
        if outs.is_empty() && !isolated && !inside {
            continue;
        }
        out.push('\n');
        if inside {
            let path = composite_path(sm, i);
            if !path.is_empty() {
                out.push_str(&path);
                out.push_str(": ");
            }
        }
        out.push_str(&state_name(sm, i));
        for (k, &ti) in outs.iter().enumerate() {
            let Some(t) = sm.transitions.get(ti) else {
                continue;
            };
            out.push_str(if k == 0 { " → " } else { "; → " });
            out.push_str(&state_name(sm, t.to));
            if let Some(l) = t
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

    for note in &sm.notes {
        out.push('\n');
        out.push_str("Note ");
        out.push_str(placement_word(note.placement));
        out.push(' ');
        out.push_str(&state_name(sm, note.state));
        out.push_str(": ");
        out.push_str(&plain_label(&note.text));
    }

    if left_out > 0 {
        out.push('\n');
        count(&mut out, left_out, "transition", "transitions");
        let _ = write!(
            out,
            " of the {} in the source {} not drawn.",
            sm.transitions.len(),
            if left_out == 1 { "is" } else { "are" }
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;
    use crate::model::state::{Note, State, Transition};
    use alloc::string::ToString;

    fn st(id: &str, kind: StateKind, parent: Option<usize>) -> State {
        State {
            id: id.to_string(),
            label: if kind.draws_label() {
                id.to_string()
            } else {
                String::new()
            },
            kind,
            parent,
            ..State::default()
        }
    }

    #[test]
    fn a_lone_state_is_listed_and_counted_in_the_singular() {
        let sm = StateMachine {
            states: alloc::vec![st("Idle", StateKind::Simple, None)],
            ..StateMachine::default()
        };
        assert_eq!(
            outline(&sm),
            "State diagram, top to bottom. 1 state, 0 transitions.\nIdle"
        );
    }

    #[test]
    fn a_cyclic_parent_chain_terminates() {
        let sm = StateMachine {
            states: alloc::vec![
                st("A", StateKind::Composite, Some(1)),
                st("B", StateKind::Composite, Some(0)),
            ],
            ..StateMachine::default()
        };
        assert!(outline(&sm).contains("\nB: A"), "{}", outline(&sm));
    }

    #[test]
    fn a_note_on_a_missing_state_still_prints_one_line() {
        let sm = StateMachine {
            states: alloc::vec![st("A", StateKind::Simple, None)],
            transitions: alloc::vec![Transition {
                from: 0,
                to: 9,
                label: None,
                span: Span::default(),
            }],
            notes: alloc::vec![Note {
                state: 9,
                placement: NotePlacement::Before,
                text: "hi\nthere".to_string(),
                span: Span::default(),
            }],
            ..StateMachine::default()
        };
        // The transition names a state the model does not hold, so it is not drawn and
        // the outline neither lists nor counts it.
        assert_eq!(
            outline(&sm),
            "State diagram, top to bottom. 1 state, 0 transitions.\nA\nNote left of ?: hi there\n\
             1 transition of the 1 in the source is not drawn."
        );
    }
}
