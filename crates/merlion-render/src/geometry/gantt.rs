//! Measured static Gantt geometry. Task indices match the resolved model.

pub use super::sequence::Rect;
use crate::text::LabelLayout;
use alloc::vec::Vec;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextGeom {
    pub x: f64,
    pub y: f64,
    pub label: LabelLayout,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskGeom {
    pub index: usize,
    pub bar: Rect,
    pub text: TextGeom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SectionGeom {
    pub index: usize,
    pub band: Rect,
    pub text: TextGeom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TickGeom {
    pub time: i64,
    pub x: f64,
    pub bottom: TextGeom,
    pub top: Option<TextGeom>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GanttGeometry {
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub plot: Rect,
    pub start: i64,
    pub end: i64,
    pub title: Option<TextGeom>,
    pub tasks: Vec<TaskGeom>,
    pub sections: Vec<SectionGeom>,
    pub ticks: Vec<TickGeom>,
    pub exclusions: Vec<Rect>,
    pub fuel_used: u64,
}
