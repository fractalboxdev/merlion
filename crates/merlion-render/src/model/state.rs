//! The state-diagram model (specs/state.md#model): the state parser's output and the
//! input to [`crate::layout::state::lower`].
//!
//! States form a tree by [`State::parent`]: a composite state owns its children, and a
//! composite split by `--` owns [`Region`]s that own theirs. Transitions and notes are
//! flat lists, indexed by the SVG's `data-merlion-index` and by the outline.

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;
use crate::model::{ClassDef, Link, Meta, Style};
use crate::options::Direction;

/// One `stateDiagram` or `stateDiagram-v2` (specs/state.md#model).
#[derive(Clone, Debug, PartialEq)]
pub struct StateMachine {
    pub meta: Meta,
    /// From the top-level `direction` statement; `TB` when absent.
    pub direction: Direction,
    /// Declaration order; a parent precedes its children
    /// (specs/state.md#what-the-lowering-guarantees).
    pub states: Vec<State>,
    /// Source order, which is the order the outline and `data-merlion-index` use.
    pub transitions: Vec<Transition>,
    pub notes: Vec<Note>,
    /// Concurrency regions, parent before child, in declaration order.
    pub regions: Vec<Region>,
    pub class_defs: Vec<ClassDef>,
}

impl Default for StateMachine {
    fn default() -> Self {
        StateMachine {
            meta: Meta::default(),
            direction: Direction::TB,
            states: Vec::new(),
            transitions: Vec::new(),
            notes: Vec::new(),
            regions: Vec::new(),
            class_defs: Vec::new(),
        }
    }
}

impl StateMachine {
    pub fn state_index(&self, id: &str) -> Option<usize> {
        self.states.iter().position(|s| s.id == id)
    }
}

/// One state (specs/state.md#states). `Start` and `End` are the states `[*]` resolves
/// to, one pair per scope (specs/state.md#start-and-end).
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// Source id after entity decoding, or `{scope}_start` / `{scope}_end` for `[*]`.
    /// Unique within the diagram: a generated id that clashes with a declared one takes
    /// a further `_`.
    pub id: String,
    /// The description when the source gives one, else the id. Empty for every kind
    /// that draws no label ([`StateKind::draws_label`]).
    pub label: String,
    pub kind: StateKind,
    /// Innermost composite state; `None` at the top level.
    pub parent: Option<usize>,
    /// Concurrency region inside `parent`, as an index into [`StateMachine::regions`];
    /// `None` when the parent has no regions.
    pub region: Option<usize>,
    /// Direct members in declaration order, across every region.
    pub children: Vec<usize>,
    /// `direction` inside a composite state. Recorded for the model's readers; the
    /// layout engine takes the diagram's direction (specs/state.md#direction).
    pub direction: Option<Direction>,
    /// `class` statements and `:::name` shorthand, in order; names already validated.
    pub classes: Vec<String>,
    /// `style <id> …`
    pub style: Style,
    pub link: Option<Link>,
    /// First named by a transition rather than declared. Idiomatic, so it carries no
    /// diagnostic (specs/state.md#diagnostics).
    pub implicit: bool,
    pub span: Span,
}

impl Default for State {
    fn default() -> Self {
        State {
            id: String::new(),
            label: String::new(),
            kind: StateKind::Simple,
            parent: None,
            region: None,
            children: Vec::new(),
            direction: None,
            classes: Vec::new(),
            style: Style::default(),
            link: None,
            implicit: false,
            span: Span::default(),
        }
    }
}

/// What a state is, which fixes the shape it lowers to (specs/state.md#lowering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateKind {
    /// A plain state: a labelled rectangle.
    Simple,
    /// `state X { … }`: a cluster holding its children.
    Composite,
    /// `state X <<choice>>`: a diamond.
    Choice,
    /// `state X <<fork>>`: a bar.
    Fork,
    /// `state X <<join>>`: a bar.
    Join,
    /// `[*]` as a transition's source: the scope's entry point.
    Start,
    /// `[*]` as a transition's target: the scope's exit point.
    End,
}

impl StateKind {
    /// Every kind, for exhaustive tests.
    pub const ALL: [StateKind; 7] = [
        StateKind::Simple,
        StateKind::Composite,
        StateKind::Choice,
        StateKind::Fork,
        StateKind::Join,
        StateKind::Start,
        StateKind::End,
    ];

    /// The `data-merlion-kind` value the SVG carries
    /// (specs/state.md#groups-and-data-attributes).
    pub fn as_str(self) -> &'static str {
        match self {
            StateKind::Simple => "simple",
            StateKind::Composite => "composite",
            StateKind::Choice => "choice",
            StateKind::Fork => "fork",
            StateKind::Join => "join",
            StateKind::Start => "start",
            StateKind::End => "end",
        }
    }

    /// Whether the state shows its label. The pseudo-states draw a bare symbol; their
    /// id stays in the model for the outline (specs/state.md#text-alternative).
    pub fn draws_label(self) -> bool {
        matches!(self, StateKind::Simple | StateKind::Composite)
    }
}

/// `A --> B` or `A --> B : text` (specs/state.md#transitions).
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// Index into [`StateMachine::states`].
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub span: Span,
}

/// `note left of X` / `note right of X` (specs/state.md#notes).
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// The state the note sits beside, as an index into [`StateMachine::states`].
    pub state: usize,
    pub placement: NotePlacement,
    /// Text with hard line breaks kept, as the source wrote them.
    pub text: String,
    pub span: Span,
}

/// Which side of the order axis a note takes. The names are axis-relative because a
/// note never sits on the layer axis, whatever the direction (specs/state.md#notes-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotePlacement {
    /// `note left of`: left in `TB` / `BT`, above in `LR` / `RL`.
    Before,
    /// `note right of`: right in `TB` / `BT`, below in `LR` / `RL`.
    After,
}

impl NotePlacement {
    /// The `data-merlion-placement` value the SVG carries.
    pub fn as_str(self) -> &'static str {
        match self {
            NotePlacement::Before => "before",
            NotePlacement::After => "after",
        }
    }
}

/// One `--`-separated region of a composite state (specs/state.md#concurrency).
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// The composite state this region belongs to.
    pub parent: usize,
    /// 0-based position within that composite.
    pub index: usize,
    /// Direct member states in declaration order.
    pub states: Vec<usize>,
    pub span: Span,
}
