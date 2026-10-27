//! A resolved Gantt schedule. Times are integer UTC milliseconds; task ends are
//! exclusive and never depend on the clock or host time zone.

pub mod calendar;

use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;
use crate::model::Meta;

#[derive(Clone, Debug, PartialEq)]
pub struct Gantt {
    pub meta: Meta,
    pub sections: Vec<String>,
    pub tasks: Vec<Task>,
    pub date_format: String,
    pub axis_format: String,
    pub tick_interval: Option<TickInterval>,
    pub top_axis: bool,
    pub display_mode: DisplayMode,
    /// Sunday = 0, Saturday = 6.
    pub week_start: u8,
    pub excludes: Excludes,
    pub date_format_span: Span,
    pub axis_format_span: Span,
    pub tick_interval_span: Span,
}

impl Default for Gantt {
    fn default() -> Self {
        Self {
            meta: Meta::default(),
            sections: Vec::new(),
            tasks: Vec::new(),
            date_format: String::from("YYYY-MM-DD"),
            axis_format: String::from("%Y-%m-%d"),
            tick_interval: None,
            top_axis: false,
            display_mode: DisplayMode::Default,
            week_start: 0,
            excludes: Excludes::default(),
            date_format_span: Span::default(),
            axis_format_span: Span::default(),
            tick_interval_span: Span::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: String,
    pub label: String,
    pub section: Option<usize>,
    pub start: i64,
    pub end: i64,
    pub flags: TaskFlags,
    /// Declaration indices of tasks used by `after` and `until`.
    pub dependencies: Vec<usize>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskFlags {
    pub done: bool,
    pub active: bool,
    pub crit: bool,
    pub milestone: bool,
    pub vert: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    #[default]
    Default,
    Compact,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GanttConfig {
    pub display_mode: Option<DisplayMode>,
    pub top_axis: Option<bool>,
    pub tick_interval: Option<TickInterval>,
    pub axis_format: Option<String>,
    pub axis_format_span: Span,
    pub tick_interval_span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickInterval {
    pub count: u32,
    pub unit: TimeUnit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Excludes {
    /// Days excluded by weekday, Sunday first.
    pub weekdays: [bool; 7],
    /// UTC midnight timestamps of excluded dates, sorted and unique.
    pub dates: Vec<i64>,
    /// Explicit includes override weekday and individual-date exclusions.
    pub included_dates: Vec<i64>,
}
