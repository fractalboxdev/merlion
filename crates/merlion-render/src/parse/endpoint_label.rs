//! `W026` LabelRepeatsEndpoint (specs/parser.md#endpoint-labels): a flowchart edge or
//! state transition label that names only the shapes it joins.
//!
//! Labels and names are compared lower case, with each run of whitespace, `-` and `_`
//! as one space. A label repeats its endpoints when it is, for some name `t` of the target
//! and some name `s` of the source:
//!
//! - `t`
//! - `to t`
//! - `w to t`, where `w` is one word
//! - `s to t`
//!
//! `→`, `->`, `-->` and `=>` in a label read as ` to `.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::{excerpt, Diagnostics, Severity, Span};
use crate::model::state::StateKind;
use crate::model::{Diagram, Flowchart, StateMachine};

/// Records `W026` for each edge or transition of `diagram` whose label only repeats the
/// names of its endpoints. Gantt and sequence diagrams have no such labels.
pub(crate) fn check(diagram: &Diagram, diags: &mut Diagnostics) {
    match diagram {
        Diagram::Flowchart(f) => flowchart(f, diags),
        Diagram::State(s) => state(s, diags),
        Diagram::Gantt(_) | Diagram::Sequence(_) => {}
    }
}

fn flowchart(f: &Flowchart, diags: &mut Diagnostics) {
    let names: Vec<Vec<String>> = f
        .nodes
        .iter()
        .map(|n| names_of(&[&n.id, &n.label]))
        .collect();
    for e in &f.edges {
        report(
            e.label.as_deref(),
            &names,
            e.from,
            e.to,
            e.span,
            "edge",
            "nodes",
            diags,
        );
    }
}

fn state(s: &StateMachine, diags: &mut Diagnostics) {
    let names: Vec<Vec<String>> = s
        .states
        .iter()
        .map(|st| match st.kind {
            // `[*]` has no name a reader sees.
            StateKind::Start | StateKind::End => Vec::new(),
            StateKind::Choice | StateKind::Fork | StateKind::Join => names_of(&[&st.id]),
            StateKind::Simple | StateKind::Composite => names_of(&[&st.id, &st.label]),
        })
        .collect();
    for t in &s.transitions {
        report(
            t.label.as_deref(),
            &names,
            t.from,
            t.to,
            t.span,
            "transition",
            "states",
            diags,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    label: Option<&str>,
    names: &[Vec<String>],
    from: usize,
    to: usize,
    span: Span,
    what: &str,
    shapes: &str,
    diags: &mut Diagnostics,
) {
    let Some(label) = label else { return };
    let (Some(source), Some(target)) = (names.get(from), names.get(to)) else {
        return;
    };
    if repeats_endpoints(&normalize_label(label), source, target) {
        diags.emit(
            Severity::Warning,
            "W026",
            span,
            format!(
                "{what} label `{}` only repeats the {shapes} it joins; name the event or condition, or remove the label",
                excerpt(label)
            ),
        );
    }
}

/// The normalised, non-empty, distinct names among `raw`.
fn names_of(raw: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in raw {
        let n = normalize(r);
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Lower case, with each run of whitespace, `-` and `_` as one space, trimmed.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        if c.is_whitespace() || c == '-' || c == '_' {
            gap = !out.is_empty();
        } else {
            if gap {
                out.push(' ');
                gap = false;
            }
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// [`normalize`], after reading each arrow in `label` as ` to `.
fn normalize_label(label: &str) -> String {
    let mut s = String::with_capacity(label.len());
    let mut rest = label;
    'outer: while let Some(c) = rest.chars().next() {
        for arrow in ["-->", "->", "=>", "→"] {
            if let Some(after) = rest.strip_prefix(arrow) {
                s.push_str(" to ");
                rest = after;
                continue 'outer;
            }
        }
        s.push(c);
        rest = &rest[c.len_utf8()..];
    }
    normalize(&s)
}

fn repeats_endpoints(label: &str, source: &[String], target: &[String]) -> bool {
    target.iter().any(|t| {
        if label == t {
            return true;
        }
        let Some(head) = label
            .strip_suffix(t.as_str())
            .and_then(|h| h.strip_suffix(' '))
        else {
            return false;
        };
        if head == "to" {
            return true;
        }
        let Some(head) = head.strip_suffix(" to") else {
            return false;
        };
        !head.contains(' ') || source.iter().any(|s| s == head)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Limits;
    use crate::parse::{parse, ParseOptions};

    fn opts() -> ParseOptions {
        ParseOptions {
            strict: false,
            limits: Limits::default(),
        }
    }

    fn codes(src: &str) -> Vec<String> {
        let mut diags = Diagnostics::new(false);
        parse(src, &opts(), &mut diags).expect("parses");
        diags
            .items
            .iter()
            .filter(|d| d.code == "W026")
            .map(|d| d.message.clone())
            .collect()
    }

    fn fires(src: &str) -> bool {
        !codes(src).is_empty()
    }

    #[test]
    fn normalizes_case_and_separators() {
        assert_eq!(normalize("  On-Hold__now  "), "on hold now");
        assert_eq!(normalize_label("idle→on_hold"), "idle to on hold");
        assert_eq!(normalize_label("idle -> busy"), "idle to busy");
        assert_eq!(normalize_label("a-->b"), "a to b");
    }

    #[test]
    fn state_label_naming_only_endpoints_fires() {
        for label in [
            "on-hold",
            "On Hold",
            "to on-hold",
            "pause to on-hold",
            "idle to on_hold",
            "idle -> on-hold",
            "idle → on-hold",
        ] {
            let src = format!(
                "stateDiagram-v2\n  idle --> on_hold: {label}\n  state \"on-hold\" as on_hold\n"
            );
            assert!(fires(&src), "{label}");
        }
    }

    #[test]
    fn state_label_with_information_is_quiet() {
        for label in [
            "queue drains",
            "on-hold after three failures",
            "operator sets on-hold for the night",
            "to on-hold now",
            "x to on-holder",
        ] {
            let src = format!("stateDiagram-v2\n  idle --> on_hold: {label}\n");
            assert!(!fires(&src), "{label}");
        }
    }

    #[test]
    fn multi_word_head_must_be_the_source() {
        let src = "stateDiagram-v2\n  state \"on hold\" as oh\n  oh --> idle: on hold to idle\n  oh --> b: please go to b\n";
        assert_eq!(codes(src).len(), 1);
    }

    #[test]
    fn pseudo_states_and_unlabelled_transitions_are_quiet() {
        assert!(!fires(
            "stateDiagram-v2\n  [*] --> a\n  a --> [*]: done\n  a --> b\n"
        ));
    }

    #[test]
    fn flowchart_edge_label_fires_on_node_id_or_label() {
        assert!(fires("flowchart LR\n  a[Build] -->|to build| a2[Build]\n"));
        assert!(fires("flowchart LR\n  a --> |b| b\n"));
        assert!(fires(
            "flowchart LR\n  a[Start] -- go to Deploy --> d[Deploy]\n"
        ));
        assert!(!fires("flowchart LR\n  a -->|tests pass| b[Deploy]\n"));
    }

    #[test]
    fn every_transition_of_a_restated_machine_fires() {
        let src = "stateDiagram-v2\n  [*] --> idle\n  idle --> on_hold: set to on-hold\n  idle --> stopped: set to stopped\n  on_hold --> stopped: set to stopped\n  stopped --> on_hold: set to on-hold\n  on_hold --> idle: set to idle\n  stopped --> idle: set to idle\n  state \"on-hold\" as on_hold\n";
        let found = codes(src);
        assert_eq!(found.len(), 6);
        assert_eq!(
            found[0],
            "transition label `set to on-hold` only repeats the states it joins; name the event or condition, or remove the label"
        );
    }

    #[test]
    fn strict_makes_it_an_error() {
        let mut diags = Diagnostics::new(true);
        let r = parse("flowchart LR\n  a -->|b| b\n", &opts(), &mut diags);
        assert!(r.is_err());
        assert!(diags
            .items
            .iter()
            .any(|d| d.code == "W026" && d.severity == Severity::Error));
    }
}
