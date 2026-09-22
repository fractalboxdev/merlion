//! Diagram model: the parser's output and the input to measure, layout and draw
//! (specs/architecture.md#pipeline). One variant per diagram type.

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;
use crate::options::Direction;

#[derive(Clone, Debug, PartialEq)]
pub enum Diagram {
    Flowchart(Flowchart),
}

impl Diagram {
    pub fn type_name(&self) -> &'static str {
        match self {
            Diagram::Flowchart(_) => "flowchart",
        }
    }
}

/// Front matter, `%%{init}%%` and `acc*` statements (specs/parser.md#front-matter-and-directives).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    /// `config.flowchart.curve`, one of Mermaid's curve names.
    pub curve: Option<String>,
    /// `config.layout`: `dagre`, `elk` or `merlion`; all use Merlion's engine.
    pub layout: Option<String>,
    /// `config.merlion.autoTone`: `Some(false)` turns the automatic tones off
    /// (specs/svg-output.md#automatic-tones).
    pub auto_tone: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Flowchart {
    pub meta: Meta,
    /// From the header; `TB` when absent (`TD` normalises to `TB`).
    pub direction: Direction,
    /// Declaration order: the order a node id first appears in the source.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Declaration order; a parent precedes its children.
    pub subgraphs: Vec<Subgraph>,
    pub class_defs: Vec<ClassDef>,
    /// Applied to every edge before its own `linkStyle <index>`.
    pub default_link_style: Style,
}

impl Default for Flowchart {
    fn default() -> Self {
        Flowchart {
            meta: Meta::default(),
            direction: Direction::TB,
            nodes: Vec::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            class_defs: Vec::new(),
            default_link_style: Style::default(),
        }
    }
}

impl Flowchart {
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `A[text]`
    Rect,
    /// `A(text)`
    Round,
    /// `A([text])`
    Stadium,
    /// `A[[text]]`
    Subroutine,
    /// `A[(text)]`
    Cylinder,
    /// `A((text))`
    Circle,
    /// `A(((text)))`
    DoubleCircle,
    /// `A>text]`
    Asymmetric,
    /// `A{text}`
    Rhombus,
    /// `A{{text}}`
    Hexagon,
    /// `A[/text/]`
    Parallelogram,
    /// `A[\text\]`
    ParallelogramAlt,
    /// `A[/text\]`
    Trapezoid,
    /// `A[\text/]`
    TrapezoidAlt,
    /// `@{ shape: sm-circ }` (`small-circle`, `start`); drawn without its label.
    SmallCircle,
    /// `@{ shape: f-circ }` (`filled-circle`, `junction`); drawn without its label.
    FilledCircle,
    /// `@{ shape: fr-circ }` (`framed-circle`, `stop`); drawn without its label.
    FramedCircle,
    /// `@{ shape: cross-circ }` (`crossed-circle`, `summary`); drawn without its label.
    CrossedCircle,
    /// `@{ shape: fork }` (`join`); drawn without its label.
    Fork,
    /// `@{ shape: hourglass }` (`collate`); drawn without its label.
    Hourglass,
    /// `@{ shape: bolt }` (`com-link`, `lightning-bolt`); drawn without its label.
    Bolt,
    /// `@{ shape: doc }` (`document`): wavy bottom edge.
    Document,
    /// `@{ shape: lin-doc }` (`lined-document`): a document with a bar inside its left side.
    LinedDocument,
    /// `@{ shape: tag-doc }` (`tagged-document`): a document with a folded corner.
    TaggedDocument,
    /// `@{ shape: docs }` (`documents`, `st-doc`, `stacked-document`).
    StackedDocument,
    /// `@{ shape: delay }` (`half-rounded-rectangle`): the right side is a half ellipse.
    Delay,
    /// `@{ shape: h-cyl }` (`das`, `horizontal-cylinder`).
    HorizontalCylinder,
    /// `@{ shape: lin-cyl }` (`disk`, `lined-cylinder`): a cylinder with a second rim.
    LinedCylinder,
    /// `@{ shape: curv-trap }` (`curved-trapezoid`, `display`).
    CurvedTrapezoid,
    /// `@{ shape: div-rect }` (`div-proc`, `divided-rectangle`, `divided-process`).
    DividedRect,
    /// `@{ shape: tri }` (`extract`, `triangle`): apex up, label near the base.
    Triangle,
    /// `@{ shape: flip-tri }` (`manual-file`, `flipped-triangle`): apex down.
    FlippedTriangle,
    /// `@{ shape: win-pane }` (`internal-storage`, `window-pane`).
    WindowPane,
    /// `@{ shape: notch-pent }` (`loop-limit`, `notched-pentagon`): top corners cut.
    NotchedPentagon,
    /// `@{ shape: sl-rect }` (`manual-input`, `sloped-rectangle`): top edge rises to the right.
    SlopedRect,
    /// `@{ shape: st-rect }` (`procs`, `processes`, `stacked-rectangle`).
    StackedRect,
    /// `@{ shape: bow-rect }` (`stored-data`, `bow-tie-rectangle`).
    BowTieRect,
    /// `@{ shape: tag-rect }` (`tag-proc`, `tagged-rectangle`, `tagged-process`).
    TaggedRect,
    /// `@{ shape: flag }` (`paper-tape`): wavy top and bottom edges.
    Flag,
    /// `@{ shape: lin-rect }` (`lin-proc`, `lined-rectangle`, `lined-process`, `shaded-process`).
    LinedRect,
    /// `@{ shape: notch-rect }` (`card`, `notched-rectangle`): top-left corner cut.
    NotchedRect,
    /// `@{ shape: text }`: the label alone, without an outline.
    TextBlock,
    /// `@{ shape: brace }` (`brace-l`, `comment`): a curly brace left of the label.
    BraceLeft,
    /// `@{ shape: brace-r }`: a curly brace right of the label.
    BraceRight,
    /// `@{ shape: braces }`: curly braces on both sides.
    Braces,
    /// `@{ shape: datastore }` (`data-store`): lines above and below the label.
    DataStore,
}

impl Shape {
    /// Every shape, for exhaustive tests.
    pub const ALL: [Shape; 46] = [
        Shape::Rect,
        Shape::Round,
        Shape::Stadium,
        Shape::Subroutine,
        Shape::Cylinder,
        Shape::Circle,
        Shape::DoubleCircle,
        Shape::Asymmetric,
        Shape::Rhombus,
        Shape::Hexagon,
        Shape::Parallelogram,
        Shape::ParallelogramAlt,
        Shape::Trapezoid,
        Shape::TrapezoidAlt,
        Shape::SmallCircle,
        Shape::FilledCircle,
        Shape::FramedCircle,
        Shape::CrossedCircle,
        Shape::Fork,
        Shape::Hourglass,
        Shape::Bolt,
        Shape::Document,
        Shape::LinedDocument,
        Shape::TaggedDocument,
        Shape::StackedDocument,
        Shape::Delay,
        Shape::HorizontalCylinder,
        Shape::LinedCylinder,
        Shape::CurvedTrapezoid,
        Shape::DividedRect,
        Shape::Triangle,
        Shape::FlippedTriangle,
        Shape::WindowPane,
        Shape::NotchedPentagon,
        Shape::SlopedRect,
        Shape::StackedRect,
        Shape::BowTieRect,
        Shape::TaggedRect,
        Shape::Flag,
        Shape::LinedRect,
        Shape::NotchedRect,
        Shape::TextBlock,
        Shape::BraceLeft,
        Shape::BraceRight,
        Shape::Braces,
        Shape::DataStore,
    ];

    /// Whether the shape shows its label. mermaid 12 draws its small symbol shapes
    /// (start, stop, junction, summary, fork, collate, communication link) without one;
    /// the label stays in the model for the text alternative.
    pub fn draws_label(self) -> bool {
        !matches!(
            self,
            Shape::SmallCircle
                | Shape::FilledCircle
                | Shape::FramedCircle
                | Shape::CrossedCircle
                | Shape::Fork
                | Shape::Hourglass
                | Shape::Bolt
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Source id, after any `R004` rename.
    pub id: String,
    /// Label text with quotes removed and Mermaid entity codes (`#quot;`, `#35;`) decoded.
    /// May contain `<br>` and Markdown (`**bold**`, `*italic*`, `` `code` ``); the text stage interprets them.
    pub label: String,
    pub shape: Shape,
    /// `class` statements and `:::name` shorthand, in order; names already validated.
    pub classes: Vec<String>,
    /// `style <id> …`
    pub style: Style,
    pub link: Option<Link>,
    /// Innermost subgraph containing this node.
    pub subgraph: Option<usize>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    /// `--`
    Normal,
    /// `==`
    Thick,
    /// `-.`
    Dotted,
    /// `~~~`
    Invisible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    None,
    /// `>` / `<`
    Arrow,
    /// `o`
    Circle,
    /// `x`
    Cross,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub stroke: Stroke,
    pub arrow_start: Arrow,
    pub arrow_end: Arrow,
    /// Minimum layer span: 1 for `-->`, +1 per extra dash / dot / `=` (`--->` is 2).
    pub min_len: u32,
    /// `linkStyle <index> …`, merged over `linkStyle default`.
    pub style: Style,
    pub span: Span,
    /// The edge id (`a e1@--> b`), when the source gives one.
    pub id: Option<String>,
    /// Roles given through the edge id (`class e1 failure`), in order; names already validated.
    pub classes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Subgraph {
    /// Source id (`subgraph id [title]`); generated as `subGraph{n}` when the source gives only a title.
    pub id: String,
    pub title: String,
    pub parent: Option<usize>,
    /// Direct member nodes in declaration order (not nodes of nested subgraphs).
    pub nodes: Vec<usize>,
    pub direction: Option<Direction>,
    /// `class <subgraph id> …`, in order; names already validated.
    pub classes: Vec<String>,
    /// `style <subgraph id> …`; it styles the cluster box and title only.
    pub style: Style,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassDef {
    /// Matches `[A-Za-z_][A-Za-z0-9_-]{0,63}`; emitted as `merlion-c-{name}`.
    pub name: String,
    pub style: Style,
}

/// `click <node> href "<url>" [_self|_blank]`, URL already checked against the link rules.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub url: String,
    pub target_blank: bool,
}

/// A colour from the accepted grammar (specs/svg-output.md#source-styles-classdef-style-linkstyle).
/// The renderer re-serialises it; source text never reaches the output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`; alpha 0..=255.
    Rgba {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
    /// `hsl()` / `hsla()`: hue in degrees [0, 360), saturation and lightness in percent, alpha 0..=1.
    Hsla {
        h: f64,
        s: f64,
        l: f64,
        a: f64,
    },
    /// A CSS named colour, lower-cased; the `&'static str` comes from the core's table.
    Named(&'static str),
    Transparent,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontWeight {
    /// `normal`, `400`
    Regular,
    /// `bold`, `600`, `700`: drawn and measured as 600.
    SemiBold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
}

/// Typed style values; every field is optional and `None` means unset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub color: Option<Color>,
    /// 0..=20 px.
    pub stroke_width: Option<f64>,
    /// Up to 8 numbers in 0..=100.
    pub stroke_dasharray: Option<Vec<f64>>,
    pub opacity: Option<f64>,
    pub fill_opacity: Option<f64>,
    pub stroke_opacity: Option<f64>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
}

impl Style {
    pub fn is_empty(&self) -> bool {
        *self == Style::default()
    }

    /// Fields set in `other` override fields in `self`.
    pub fn merge(&mut self, other: &Style) {
        macro_rules! m {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f = other.$f.clone(); } )* };
        }
        m!(
            fill,
            stroke,
            color,
            stroke_width,
            stroke_dasharray,
            opacity,
            fill_opacity,
            stroke_opacity,
            font_weight,
            font_style
        );
    }
}
