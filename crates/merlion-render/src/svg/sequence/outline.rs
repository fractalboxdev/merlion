//! The plain-text outline of a sequence (specs/sequence.md#text-alternative).
//!
//! ```text
//! Sequence diagram. 4 participants, 7 messages.
//! Participants: Customer, Web app (Web), API gateway (API), Bank.
//! 1. Customer → Web app: Place order
//! loop Every minute:
//!   2. API gateway → Bank: Authorise payment
//! Note over Customer,Bank: One order, one transaction
//! ```
//!
//! Messages are numbered by [`Message::index`] + 1, or by the autonumber sequence when
//! the diagram sets one, and indented two spaces per enclosing fragment under a heading
//! per fragment and per section. `activate` and `deactivate` carry no text of their own
//! and are left out.

use alloc::string::String;
use core::fmt::Write;

use crate::model::sequence::{
    Fragment, FragmentKind, Head, Message, MessageLine, Note, Placement, Sequence,
};
use crate::numfmt::push_num;

use super::super::outline::plain_label;
use super::walk::{walk, Ev};

/// The keyword a fragment's section `k` opens with (specs/sequence.md#fragments).
fn section_word(kind: FragmentKind, k: usize) -> &'static str {
    if k == 0 {
        return kind.as_str();
    }
    match kind {
        FragmentKind::Alt => "else",
        FragmentKind::Critical => "option",
        _ => "and",
    }
}

/// `→`, `←`, `↔` or `—` from the heads, with a dotted line written `-->`
/// (specs/interaction.md#text-reconstruction).
fn glyph(m: &Message) -> &'static str {
    if m.line == MessageLine::Dotted {
        return "-->";
    }
    match (m.tail != Head::None, m.head != Head::None) {
        (false, true) => "→",
        (true, true) => "↔",
        (true, false) => "←",
        (false, false) => "—",
    }
}

fn placement_word(p: Placement) -> &'static str {
    match p {
        Placement::LeftOf => "left of",
        Placement::RightOf => "right of",
        Placement::Over => "over",
    }
}

/// A participant's display name: its label, with `(id)` appended when they differ.
fn participant_name(seq: &Sequence, i: usize) -> String {
    let Some(p) = seq.participants.get(i) else {
        return String::from("?");
    };
    let label = plain_label(&p.label);
    let id = plain_label(&p.id);
    let mut out = if label.is_empty() { id.clone() } else { label };
    if out != id && !id.is_empty() {
        out.push_str(" (");
        out.push_str(&id);
        out.push(')');
    }
    out
}

/// The name alone, for the `A → B` columns, where the id is noise.
fn short_name(seq: &Sequence, i: usize) -> String {
    match seq.participants.get(i) {
        Some(p) => {
            let l = plain_label(&p.label);
            if l.is_empty() {
                plain_label(&p.id)
            } else {
                l
            }
        }
        None => String::from("?"),
    }
}

/// The number printed beside a message: the autonumber value when the diagram numbers
/// its messages, else the 1-based source index.
fn push_number(out: &mut String, seq: &Sequence, m: &Message) {
    match seq.autonumber.filter(|a| a.visible) {
        Some(a) => push_num(out, a.value(m.index) as f64 / 100.0),
        None => {
            let _ = write!(out, "{}", m.index.saturating_add(1));
        }
    }
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth.min(super::walk::MAX_FRAMES) {
        out.push_str("  ");
    }
}

fn push_message(out: &mut String, seq: &Sequence, m: &Message, depth: usize) {
    out.push('\n');
    push_indent(out, depth);
    push_number(out, seq, m);
    out.push_str(". ");
    out.push_str(&short_name(seq, m.from));
    out.push(' ');
    out.push_str(glyph(m));
    out.push(' ');
    out.push_str(&short_name(seq, m.to));
    let text = plain_label(&m.label);
    if !text.is_empty() {
        out.push_str(": ");
        out.push_str(&text);
    }
}

fn push_note(out: &mut String, seq: &Sequence, n: &Note, depth: usize) {
    out.push('\n');
    push_indent(out, depth);
    out.push_str("Note ");
    out.push_str(placement_word(n.placement));
    out.push(' ');
    out.push_str(&short_name(seq, n.from));
    if n.to != n.from {
        out.push(',');
        out.push_str(&short_name(seq, n.to));
    }
    let text = plain_label(&n.text);
    if !text.is_empty() {
        out.push_str(": ");
        out.push_str(&text);
    }
}

fn push_section(out: &mut String, frag: &Fragment, k: usize, depth: usize) {
    out.push('\n');
    push_indent(out, depth);
    out.push_str(section_word(frag.kind, k));
    if let Some(label) = frag
        .sections
        .get(k)
        .map(|s| plain_label(&s.label))
        .filter(|l| !l.is_empty())
    {
        out.push(' ');
        out.push_str(&label);
    }
    out.push(':');
}

/// The outline of `seq`.
pub fn outline(seq: &Sequence) -> String {
    let mut out = String::from("Sequence diagram. ");
    let n = seq.participants.len();
    let _ = write!(
        out,
        "{} participant{}, {} message{}.",
        n,
        if n == 1 { "" } else { "s" },
        seq.messages,
        if seq.messages == 1 { "" } else { "s" }
    );
    if n > 0 {
        out.push_str("\nParticipants: ");
        for i in 0..n {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&participant_name(seq, i));
        }
        out.push('.');
    }
    for step in walk(seq) {
        match step.ev {
            Ev::Message(m) => push_message(&mut out, seq, m, step.depth),
            Ev::Note(nt, _) => push_note(&mut out, seq, nt, step.depth),
            Ev::Section { frag, k, .. } => push_section(&mut out, frag, k, step.depth),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::sequence::{Autonumber, Participant};

    fn two() -> Sequence {
        Sequence {
            participants: alloc::vec![
                Participant {
                    id: String::from("A"),
                    label: String::from("A"),
                    ..Participant::default()
                },
                Participant {
                    id: String::from("b"),
                    label: String::from("Bob"),
                    ..Participant::default()
                },
            ],
            ..Sequence::default()
        }
    }

    #[test]
    fn an_alias_prints_its_id_in_brackets() {
        let seq = two();
        assert_eq!(participant_name(&seq, 0), "A");
        assert_eq!(participant_name(&seq, 1), "Bob (b)");
        assert_eq!(short_name(&seq, 1), "Bob");
    }

    #[test]
    fn a_dotted_line_prints_its_arrow() {
        let dotted = Message {
            line: MessageLine::Dotted,
            ..Message::default()
        };
        assert_eq!(glyph(&dotted), "-->");
        assert_eq!(glyph(&Message::default()), "→");
    }

    #[test]
    fn section_keywords_follow_the_kind() {
        assert_eq!(section_word(FragmentKind::Alt, 0), "alt");
        assert_eq!(section_word(FragmentKind::Alt, 1), "else");
        assert_eq!(section_word(FragmentKind::Critical, 1), "option");
        assert_eq!(section_word(FragmentKind::Par, 1), "and");
        assert_eq!(section_word(FragmentKind::ParOver, 0), "par_over");
    }

    #[test]
    fn autonumber_values_print_without_trailing_zeros() {
        let mut seq = two();
        seq.autonumber = Some(Autonumber {
            start: 105,
            step: 10,
            visible: true,
        });
        let mut s = String::new();
        push_number(&mut s, &seq, &Message::default());
        assert_eq!(s, "1.05");
        let mut s = String::new();
        push_number(
            &mut s,
            &seq,
            &Message {
                index: 5,
                ..Message::default()
            },
        );
        assert_eq!(s, "1.55");
    }

    #[test]
    fn autonumber_off_falls_back_to_the_source_index() {
        let mut seq = two();
        seq.autonumber = Some(Autonumber {
            visible: false,
            ..Autonumber::default()
        });
        let mut s = String::new();
        push_number(
            &mut s,
            &seq,
            &Message {
                index: 2,
                ..Message::default()
            },
        );
        assert_eq!(s, "3");
    }
}
