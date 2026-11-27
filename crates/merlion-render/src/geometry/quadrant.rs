//! Quadrant chart geometry (specs/quadrant.md#layout). Point indices match the model.

pub use super::gantt::{Rect, TextGeom};
use alloc::vec::Vec;

/// One quadrant: its box and its label, when the source gives one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuadrantGeom {
    pub rect: Rect,
    pub label: Option<TextGeom>,
}

/// An axis label. A y-axis label is laid out horizontally around its centre and drawn
/// rotated −90° about it.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisLabel {
    /// The high end (right or top) rather than the low end (left or bottom).
    pub high: bool,
    pub text: TextGeom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointGeom {
    pub index: usize,
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub stroke_width: f64,
    pub label: TextGeom,
}

/// Font sizes of the five kinds of text, in px; each is measured at its size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextSizes {
    pub title: f64,
    pub quadrant: f64,
    pub x_axis: f64,
    pub y_axis: f64,
    pub point: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuadrantGeometry {
    pub width: f64,
    pub height: f64,
    /// The base font size of the diagram.
    pub font_size: f64,
    pub sizes: TextSizes,
    /// The four quadrants together.
    pub plot: Rect,
    /// Quadrants 1 to 4: top right, top left, bottom left, bottom right.
    pub quadrants: [QuadrantGeom; 4],
    pub title: Option<TextGeom>,
    pub x_labels: Vec<AxisLabel>,
    pub y_labels: Vec<AxisLabel>,
    pub points: Vec<PointGeom>,
    pub external_border: f64,
    pub internal_border: f64,
    pub fuel_used: u64,
}
