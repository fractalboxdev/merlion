//! Render options (specs/layout.md#options, specs/integrations.md).

use alloc::string::String;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    TB,
    BT,
    LR,
    RL,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::TB => "TB",
            Direction::BT => "BT",
            Direction::LR => "LR",
            Direction::RL => "RL",
        }
    }
    pub fn is_horizontal(self) -> bool {
        matches!(self, Direction::LR | Direction::RL)
    }
}

/// `Auto` lets the engine choose TB or LR to fit `target_width`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionOption {
    FromSource,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeStyle {
    Orthogonal,
    Polyline,
    Spline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontMode {
    Link,
    Embed,
    System,
}

/// Size limits (specs/architecture.md#boundaries). Every limit is configurable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub input_bytes: usize,
    pub nodes: usize,
    pub edges: usize,
    /// Notes a state diagram carries. A note is a drawn box that no other limit counts:
    /// it is neither a node nor an edge, so without this a source of nothing but notes
    /// grows the SVG far past what `nodes` and `edges` allow
    /// (specs/state.md#diagnostics).
    pub notes: usize,
    pub layered_nodes: usize,
    pub layers: usize,
    pub nesting: usize,
    pub label_bytes: usize,
    /// Diagnostics kept. A repair costs a few source bytes and about a hundred bytes of
    /// message, so an unbounded list is the cheapest way to grow a render past its
    /// input. Past this many, [`crate::diag::Diagnostics`] counts instead of keeping,
    /// and the last slot holds `I034` naming how many were dropped. The default matches
    /// `edges`, so a diagram inside every other limit keeps every diagnostic it earns.
    pub diagnostics: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            input_bytes: 1 << 20,
            nodes: 2_000,
            edges: 4_000,
            notes: 2_000,
            layered_nodes: 20_000,
            layers: 500,
            nesting: 64,
            label_bytes: 4_096,
            diagnostics: 4_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderOptions {
    pub target_width: f64,
    pub max_aspect: f64,
    pub direction: DirectionOption,
    pub edge_style: EdgeStyle,
    pub node_spacing: f64,
    pub rank_spacing: f64,
    /// Previous SVG's `data-merlion-layout` value (or the whole previous SVG; the layout stage extracts it).
    pub hint: Option<String>,
    pub stability: u32,
    pub fuel: u64,
    pub font: FontMode,
    pub font_size: f64,
    /// Label wrap width in px.
    pub wrap_width: f64,
    pub strict: bool,
    pub id_prefix: Option<String>,
    pub background: bool,
    pub limits: Limits,
    /// Literals resolved from a stylesheet for one theme (specs/svg-output.md#palette).
    /// `None` draws the built-in defaults. It never affects measurement or layout.
    pub palette: Option<crate::stylesheet::Palette>,
    /// Automatic tones for decisions, stores, terminals and top-level clusters
    /// (specs/svg-output.md#automatic-tones). Presentation only: it never affects
    /// measurement or layout.
    pub auto_tone: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            target_width: 720.0,
            max_aspect: 1.6,
            direction: DirectionOption::FromSource,
            edge_style: EdgeStyle::Orthogonal,
            node_spacing: 24.0,
            rank_spacing: 48.0,
            hint: None,
            stability: 2,
            fuel: 20_000_000,
            font: FontMode::Link,
            font_size: 14.0,
            wrap_width: 200.0,
            strict: false,
            id_prefix: None,
            background: false,
            limits: Limits::default(),
            palette: None,
            auto_tone: true,
        }
    }
}
