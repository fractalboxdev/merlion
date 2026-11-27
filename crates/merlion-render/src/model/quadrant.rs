//! A quadrant chart (specs/quadrant.md): two labelled axes, four labelled quadrants and
//! points placed by coordinates in 0–1. Quadrant 1 is the top right, 2 the top left,
//! 3 the bottom left and 4 the bottom right, as in mermaid.

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;
use crate::model::{Color, Meta};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quadrant {
    pub meta: Meta,
    /// `x-axis <left> --> <right>`.
    pub x_axis: Axis,
    /// `y-axis <bottom> --> <top>`.
    pub y_axis: Axis,
    /// `quadrant-1` … `quadrant-4`, indexed from 0.
    pub quadrants: [Option<String>; 4],
    /// Declaration order.
    pub points: Vec<Point>,
    /// `classDef` statements in first-declaration order; a later statement of the same
    /// name replaces the earlier style.
    pub class_defs: Vec<PointClass>,
}

impl Quadrant {
    /// The point style of class `name`, when a `classDef` declares it.
    pub fn class_def(&self, name: &str) -> Option<&PointClass> {
        self.class_defs.iter().find(|c| c.name == name)
    }

    /// The style a point draws with: its class's style, then its own on top, the
    /// precedence mermaid gives them.
    pub fn effective_style(&self, p: &Point) -> PointStyle {
        let mut s = p
            .class
            .as_deref()
            .and_then(|c| self.class_def(c))
            .map(|c| c.style.clone())
            .unwrap_or_default();
        s.merge(&p.style);
        s
    }
}

/// One axis: the label at its low end (left or bottom) and at its high end (right or
/// top). The one-sided form `x-axis Low -->` keeps only `low`, ending in ` →`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Axis {
    pub low: Option<String>,
    pub high: Option<String>,
}

impl Axis {
    pub fn is_empty(&self) -> bool {
        self.low.is_none() && self.high.is_none()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    /// Label text, quotes removed and entity codes decoded.
    pub label: String,
    /// 0 (left) to 1 (right).
    pub x: f64,
    /// 0 (bottom) to 1 (top).
    pub y: f64,
    /// `Name:::class: [x, y]`; the name already validated.
    pub class: Option<String>,
    /// Styles written after the coordinates; they win over the class's.
    pub style: PointStyle,
    pub span: Span,
}

/// The point styles mermaid accepts (`radius`, `color`, `stroke-color`,
/// `stroke-width`); `None` means unset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointStyle {
    /// 0..=100 px.
    pub radius: Option<f64>,
    /// Fill of the point.
    pub color: Option<Color>,
    pub stroke_color: Option<Color>,
    /// 0..=20 px.
    pub stroke_width: Option<f64>,
}

impl PointStyle {
    pub fn is_empty(&self) -> bool {
        *self == PointStyle::default()
    }

    /// Fields set in `other` override fields in `self`.
    pub fn merge(&mut self, other: &PointStyle) {
        if other.radius.is_some() {
            self.radius = other.radius;
        }
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.stroke_color.is_some() {
            self.stroke_color = other.stroke_color;
        }
        if other.stroke_width.is_some() {
            self.stroke_width = other.stroke_width;
        }
    }
}

/// `classDef <name> <styles>` of a quadrant chart.
#[derive(Clone, Debug, PartialEq)]
pub struct PointClass {
    /// Matches `[A-Za-z_][A-Za-z0-9_-]{0,63}`; emitted as `merlion-c-{name}`.
    pub name: String,
    pub style: PointStyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum XAxisPosition {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YAxisPosition {
    #[default]
    Left,
    Right,
}

/// `config.quadrantChart` (specs/quadrant.md#configuration); `None` keeps the default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuadrantConfig {
    pub chart_width: Option<f64>,
    pub chart_height: Option<f64>,
    pub title_padding: Option<f64>,
    pub title_font_size: Option<f64>,
    pub quadrant_padding: Option<f64>,
    pub quadrant_text_top_padding: Option<f64>,
    pub quadrant_label_font_size: Option<f64>,
    pub internal_border_width: Option<f64>,
    pub external_border_width: Option<f64>,
    pub x_axis_label_padding: Option<f64>,
    pub x_axis_label_font_size: Option<f64>,
    pub x_axis_position: Option<XAxisPosition>,
    pub y_axis_label_padding: Option<f64>,
    pub y_axis_label_font_size: Option<f64>,
    pub y_axis_position: Option<YAxisPosition>,
    pub point_text_padding: Option<f64>,
    pub point_label_font_size: Option<f64>,
    pub point_radius: Option<f64>,
    /// The quadrant keys of `themeVariables`.
    pub theme: QuadrantTheme,
}

/// A numeric `config.quadrantChart` key: its slot and accepted range.
pub struct NumericKey {
    pub name: &'static str,
    pub min: f64,
    pub max: f64,
}

/// Every numeric `config.quadrantChart` key mermaid 12.0.0 reads, with its range.
pub const NUMERIC_KEYS: [NumericKey; 16] = [
    key("chartWidth", 100.0, 4_000.0),
    key("chartHeight", 100.0, 4_000.0),
    key("titlePadding", 0.0, 200.0),
    key("titleFontSize", 1.0, 100.0),
    key("quadrantPadding", 0.0, 200.0),
    key("quadrantTextTopPadding", 0.0, 200.0),
    key("quadrantLabelFontSize", 1.0, 100.0),
    key("quadrantInternalBorderStrokeWidth", 0.0, 20.0),
    key("quadrantExternalBorderStrokeWidth", 0.0, 20.0),
    key("xAxisLabelPadding", 0.0, 200.0),
    key("xAxisLabelFontSize", 1.0, 100.0),
    key("yAxisLabelPadding", 0.0, 200.0),
    key("yAxisLabelFontSize", 1.0, 100.0),
    key("pointTextPadding", 0.0, 200.0),
    key("pointLabelFontSize", 1.0, 100.0),
    key("pointRadius", 0.0, 100.0),
];

const fn key(name: &'static str, min: f64, max: f64) -> NumericKey {
    NumericKey { name, min, max }
}

impl QuadrantConfig {
    /// The slot of numeric key `name`, one of [`NUMERIC_KEYS`].
    pub fn numeric(&mut self, name: &str) -> Option<&mut Option<f64>> {
        Some(match name {
            "chartWidth" => &mut self.chart_width,
            "chartHeight" => &mut self.chart_height,
            "titlePadding" => &mut self.title_padding,
            "titleFontSize" => &mut self.title_font_size,
            "quadrantPadding" => &mut self.quadrant_padding,
            "quadrantTextTopPadding" => &mut self.quadrant_text_top_padding,
            "quadrantLabelFontSize" => &mut self.quadrant_label_font_size,
            "quadrantInternalBorderStrokeWidth" => &mut self.internal_border_width,
            "quadrantExternalBorderStrokeWidth" => &mut self.external_border_width,
            "xAxisLabelPadding" => &mut self.x_axis_label_padding,
            "xAxisLabelFontSize" => &mut self.x_axis_label_font_size,
            "yAxisLabelPadding" => &mut self.y_axis_label_padding,
            "yAxisLabelFontSize" => &mut self.y_axis_label_font_size,
            "pointTextPadding" => &mut self.point_text_padding,
            "pointLabelFontSize" => &mut self.point_label_font_size,
            "pointRadius" => &mut self.point_radius,
            _ => return None,
        })
    }
}

/// The quadrant colours of `themeVariables`. Each one replaces a themed colour with a
/// fixed one (`I030 FixedColour`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuadrantTheme {
    /// `quadrant1Fill` … `quadrant4Fill`.
    pub fill: [Option<Color>; 4],
    /// `quadrant1TextFill` … `quadrant4TextFill`.
    pub text: [Option<Color>; 4],
    pub point_fill: Option<Color>,
    pub point_text: Option<Color>,
    pub x_axis_text: Option<Color>,
    pub y_axis_text: Option<Color>,
    pub internal_border: Option<Color>,
    pub external_border: Option<Color>,
    pub title: Option<Color>,
    /// The first quadrant key read, where `I030` points.
    pub span: Option<Span>,
    /// The `themeVariables` key held only quadrant colours, so its `I011` waits until
    /// the header shows whether the diagram is a quadrant chart.
    pub pending_ignored: Option<Span>,
}

/// Every quadrant colour key of `themeVariables`.
pub const THEME_KEYS: [&str; 15] = [
    "quadrant1Fill",
    "quadrant2Fill",
    "quadrant3Fill",
    "quadrant4Fill",
    "quadrant1TextFill",
    "quadrant2TextFill",
    "quadrant3TextFill",
    "quadrant4TextFill",
    "quadrantPointFill",
    "quadrantPointTextFill",
    "quadrantXAxisTextFill",
    "quadrantYAxisTextFill",
    "quadrantInternalBorderStrokeFill",
    "quadrantExternalBorderStrokeFill",
    "quadrantTitleFill",
];

impl QuadrantTheme {
    /// The slot of colour key `name`, one of [`THEME_KEYS`].
    pub fn slot(&mut self, name: &str) -> Option<&mut Option<Color>> {
        Some(match name {
            "quadrant1Fill" => &mut self.fill[0],
            "quadrant2Fill" => &mut self.fill[1],
            "quadrant3Fill" => &mut self.fill[2],
            "quadrant4Fill" => &mut self.fill[3],
            "quadrant1TextFill" => &mut self.text[0],
            "quadrant2TextFill" => &mut self.text[1],
            "quadrant3TextFill" => &mut self.text[2],
            "quadrant4TextFill" => &mut self.text[3],
            "quadrantPointFill" => &mut self.point_fill,
            "quadrantPointTextFill" => &mut self.point_text,
            "quadrantXAxisTextFill" => &mut self.x_axis_text,
            "quadrantYAxisTextFill" => &mut self.y_axis_text,
            "quadrantInternalBorderStrokeFill" => &mut self.internal_border,
            "quadrantExternalBorderStrokeFill" => &mut self.external_border,
            "quadrantTitleFill" => &mut self.title,
            _ => return None,
        })
    }

    /// Whether any colour is set.
    pub fn any(&self) -> bool {
        self.fill
            .iter()
            .chain(self.text.iter())
            .any(Option::is_some)
            || [
                self.point_fill,
                self.point_text,
                self.x_axis_text,
                self.y_axis_text,
                self.internal_border,
                self.external_border,
                self.title,
            ]
            .iter()
            .any(Option::is_some)
    }
}
