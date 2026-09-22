//! The sequence-diagram model (specs/sequence.md#model): the sequence parser's output
//! and the input to [`crate::layout::layout_sequence`].
//!
//! Items form a tree rather than mermaid's flat event stream: a fragment owns its
//! sections and a section owns its items, so a section boundary cannot be unbalanced
//! downstream and nesting depth is bounded once, by the parser.

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;
use crate::model::{Color, Meta};

/// One `sequenceDiagram` (specs/sequence.md#model).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sequence {
    pub meta: Meta,
    /// Declaration order, which is drawing order (specs/sequence.md#columns).
    pub participants: Vec<Participant>,
    /// `box … end` groups, in declaration order.
    pub boxes: Vec<ParticipantBox>,
    /// Top-level items in source order; fragments hold their own.
    pub items: Vec<Item>,
    /// `autonumber`; `None` leaves messages unnumbered.
    pub autonumber: Option<Autonumber>,
    /// Number of messages in the whole diagram, fragments included: one more than the
    /// largest [`Message::index`].
    pub messages: u32,
}

impl Sequence {
    pub fn participant_index(&self, id: &str) -> Option<usize> {
        self.participants.iter().position(|p| p.id == id)
    }
}

/// A column of the diagram (specs/sequence.md#participants).
#[derive(Clone, Debug, PartialEq)]
pub struct Participant {
    /// Source id, after entity decoding.
    pub id: String,
    /// Display text: the `as` alias, else the inline `alias`, else the id.
    pub label: String,
    pub kind: ParticipantKind,
    /// The `box` holding this participant, as an index into [`Sequence::boxes`].
    pub group: Option<usize>,
    /// Declared by naming it in a message rather than by a `participant` statement.
    /// Idiomatic, so it carries no diagnostic (specs/sequence.md#diagnostics).
    pub implicit: bool,
    /// `create`: the index of the message that starts this lifeline.
    pub created_by: Option<u32>,
    /// `destroy`: the index of the message that ends this lifeline.
    pub destroyed_by: Option<u32>,
    /// `:wrap:` / `:nowrap:` opening the `as` alias, as on a message.
    pub wrap: Option<bool>,
    pub span: Span,
}

impl Default for Participant {
    fn default() -> Self {
        Participant {
            id: String::new(),
            label: String::new(),
            kind: ParticipantKind::Participant,
            group: None,
            implicit: false,
            created_by: None,
            destroyed_by: None,
            wrap: None,
            span: Span::default(),
        }
    }
}

/// The symbol a participant draws with: the `participant` / `actor` keyword, overridden
/// by `@{ "type": … }` when both are given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantKind {
    /// `participant A`: a labelled box.
    Participant,
    /// `actor A`, `@{ "type": "actor" }`: a stick figure above the label.
    Actor,
    /// `@{ "type": "boundary" }`
    Boundary,
    /// `@{ "type": "control" }`
    Control,
    /// `@{ "type": "entity" }`
    Entity,
    /// `@{ "type": "database" }`
    Database,
    /// `@{ "type": "collections" }`
    Collections,
    /// `@{ "type": "queue" }`
    Queue,
}

impl ParticipantKind {
    /// Every kind, for exhaustive tests.
    pub const ALL: [ParticipantKind; 8] = [
        ParticipantKind::Participant,
        ParticipantKind::Actor,
        ParticipantKind::Boundary,
        ParticipantKind::Control,
        ParticipantKind::Entity,
        ParticipantKind::Database,
        ParticipantKind::Collections,
        ParticipantKind::Queue,
    ];

    /// The `type` value that names this kind, and the `data-merlion-kind` value the SVG
    /// carries (specs/sequence.md#groups-and-data-attributes).
    pub fn as_str(self) -> &'static str {
        match self {
            ParticipantKind::Participant => "participant",
            ParticipantKind::Actor => "actor",
            ParticipantKind::Boundary => "boundary",
            ParticipantKind::Control => "control",
            ParticipantKind::Entity => "entity",
            ParticipantKind::Database => "database",
            ParticipantKind::Collections => "collections",
            ParticipantKind::Queue => "queue",
        }
    }

    /// The kind a `@{ "type": … }` value names, lower-cased by the caller.
    pub fn from_type_name(s: &str) -> Option<Self> {
        ParticipantKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// `box <colour?> <label> … end` (specs/sequence.md#boxes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticipantBox {
    pub label: String,
    /// The leading colour word, typed; `None` draws untinted. A source literal, so it
    /// emits `I030 FixedColour`.
    pub color: Option<Color>,
    /// Member participants, in declaration order.
    pub participants: Vec<usize>,
    pub span: Span,
}

/// One statement of the diagram body, in source order.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Message(Message),
    Note(Note),
    Fragment(Fragment),
    /// `activate X`
    Activate {
        participant: usize,
        span: Span,
    },
    /// `deactivate X`
    Deactivate {
        participant: usize,
        span: Span,
    },
}

/// `A->>B: text` (specs/sequence.md#messages).
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// 0-based source order over the whole diagram, fragments included. The autonumber
    /// badge, `data-merlion-index` and the outline all print it.
    pub index: u32,
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub line: MessageLine,
    /// The head at the target end.
    pub head: Head,
    /// The head at the source end: `<<->>` and the reverse half arrows set it.
    pub tail: Head,
    /// `+`: opens an activation on `to`.
    pub activate: bool,
    /// `-`: closes the innermost activation on `from`.
    pub deactivate: bool,
    pub central: Central,
    /// `:wrap:` / `:nowrap:`; `None` wraps at `wrap_width` like any other label.
    pub wrap: Option<bool>,
    pub span: Span,
}

impl Default for Message {
    fn default() -> Self {
        Message {
            index: 0,
            from: 0,
            to: 0,
            label: String::new(),
            line: MessageLine::Solid,
            head: Head::Filled,
            tail: Head::None,
            activate: false,
            deactivate: false,
            central: Central::None,
            wrap: None,
            span: Span::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLine {
    /// `->`, `->>`, `-x`, `-)`
    Solid,
    /// `-->`, `-->>`, `--x`, `--)`; drawn dashed.
    Dotted,
}

/// What an end of a message line draws (specs/sequence.md#messages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    /// `->`, `-->`: an open line end.
    None,
    /// `->>`, `-->>`, `<<->>`: a filled arrowhead.
    Filled,
    /// `-)`, `--)`: mermaid's async arrowhead.
    Open,
    /// `-x`, `--x`.
    Cross,
    /// `-|\`, `--|\`, `/|-`, `/|--`.
    HalfTop,
    /// `-|/`, `--|/`, `\|-`, `\|--`.
    HalfBottom,
    /// `-\\`, `--\\`, `//-`, `//--`.
    StickTop,
    /// `-//`, `--//`, `\\-`, `\\--`.
    StickBottom,
}

/// `()` on either end of an arrow: the message terminates on a central dot rather than
/// on the participant's lifeline or activation bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Central {
    None,
    /// `A->>()B`
    Target,
    /// `A()->>B`
    Source,
    /// `A()->>()B`
    Both,
}

/// `Note left of A: …`, `Note over A,B: …` (specs/sequence.md#notes).
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub placement: Placement,
    pub from: usize,
    /// Equal to `from` unless the note spans a pair.
    pub to: usize,
    pub text: String,
    /// `:wrap:` / `:nowrap:` straight after the colon, as on a message.
    pub wrap: Option<bool>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    LeftOf,
    RightOf,
    Over,
}

/// `loop`, `alt`, `opt`, `par`, `critical`, `break`, `rect` … `end`
/// (specs/sequence.md#fragments).
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    pub kind: FragmentKind,
    /// At least one; `else`, `and` and `option` open the later ones.
    pub sections: Vec<Section>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FragmentKind {
    Loop,
    /// Sections come from `else`.
    Alt,
    Opt,
    /// Sections come from `and`.
    Par,
    /// `par_over`: mermaid overlaps the sections; Merlion stacks them in one box.
    ParOver,
    /// Sections come from `option`.
    Critical,
    Break,
    /// `rect`: a tinted background. A named colour is a source literal, so it emits
    /// `I030 FixedColour` and takes no automatic tone; `rect` alone carries `None` and
    /// takes the theme's cluster tint, as mermaid 12 does.
    Rect(Option<Color>),
}

impl FragmentKind {
    /// The keyword, and the `data-merlion-kind` value the SVG carries.
    pub fn as_str(self) -> &'static str {
        match self {
            FragmentKind::Loop => "loop",
            FragmentKind::Alt => "alt",
            FragmentKind::Opt => "opt",
            FragmentKind::Par => "par",
            FragmentKind::ParOver => "par_over",
            FragmentKind::Critical => "critical",
            FragmentKind::Break => "break",
            FragmentKind::Rect(_) => "rect",
        }
    }

    /// The automatic tone of a fragment of this kind
    /// (specs/sequence.md#roles-and-automatic-tones).
    pub fn auto_role(self) -> Option<&'static str> {
        match self {
            FragmentKind::Loop => Some("series-1"),
            FragmentKind::Par | FragmentKind::ParOver => Some("series-2"),
            FragmentKind::Alt => Some("warn"),
            FragmentKind::Opt => Some("muted"),
            FragmentKind::Critical | FragmentKind::Break => Some("danger"),
            FragmentKind::Rect(_) => None,
        }
    }
}

/// One section of a fragment: its label and the items between its own keyword and the
/// next section or `end`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub label: String,
    pub items: Vec<Item>,
    pub span: Span,
}

/// `autonumber [start [increment]]` (specs/sequence.md#autonumber). Both values are in
/// hundredths of a unit, so formatting a badge needs no float rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Autonumber {
    pub start: i64,
    pub step: i64,
    /// `autonumber off` leaves the statement in the model and hides the badges.
    pub visible: bool,
}

impl Default for Autonumber {
    fn default() -> Self {
        Autonumber {
            start: 100,
            step: 100,
            visible: true,
        }
    }
}

impl Autonumber {
    /// The value printed beside the message at `index`, in hundredths.
    pub fn value(&self, index: u32) -> i64 {
        self.start
            .saturating_add(self.step.saturating_mul(i64::from(index)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_round_trip() {
        for k in ParticipantKind::ALL {
            assert_eq!(ParticipantKind::from_type_name(k.as_str()), Some(k));
        }
    }

    #[test]
    fn autonumber_counts_in_hundredths() {
        let a = Autonumber {
            start: 105,
            step: 10,
            visible: true,
        };
        assert_eq!(a.value(0), 105);
        assert_eq!(a.value(3), 135);
    }
}
