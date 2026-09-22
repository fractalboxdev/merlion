//! Sequence layout (specs/sequence.md#layout): participants are columns in source
//! order and items are rows, so none of the seven phases of specs/layout.md runs. The
//! module shares the text measurement, the `target_width` fit and the fuel counter.
//!
//! The pass order is fixed:
//!
//! 1. Measure every label once, at the current wrap width, and collect the width each
//!    message, note and self-message needs between columns ([`Meas`]).
//! 2. Widen the column gaps until every requirement is met ([`solve_gaps`]), spreading
//!    a deficit equally over the gaps it spans in a fixed order, so the result does not
//!    depend on iteration order.
//! 3. Walk the item tree into rows, tracking the open activation bars per column, and
//!    close each fragment box around the rows it encloses ([`Build`]).
//! 4. Fit the container: shrink the gaps toward [`COLUMN_GAP_MIN`], then narrow the
//!    wrap width in [`WRAP_STEP_PX`] steps down to [`MIN_WRAP_WIDTH`]. The widest wrap
//!    that reaches `target_width` wins; a diagram no wrap reaches keeps the full width.
//!
//! Coordinates are built with the first column's centre at the origin and the head
//! boxes at `y = 0`; the finished drawing is translated so its bounding box starts at
//! [`MARGIN`]. Height is never fitted — a sequence diagram scrolls.

use alloc::vec;
use alloc::vec::Vec;

use crate::diag::Diagnostics;
use crate::fuel::Fuel;
use crate::geometry::sequence::{
    ActivationGeom, BoxGeom, FragmentGeom, MessageGeom, NoteGeom, ParticipantGeom, Rect,
    SectionGeom, SequenceGeometry, ACTIVATION_NEST, ACTIVATION_W, COLUMN_GAP_MIN, FRAGMENT_TAB,
    NUMBER_R,
};
use crate::math::{clamp, max, min};
use crate::model::sequence::{
    Central, Fragment, Item, Message, Note, ParticipantKind, Placement, Sequence,
};
use crate::options::RenderOptions;
use crate::text::{self, LabelLayout, TextStyle, Weight};

use super::pipeline::{MARGIN, MIN_WRAP_WIDTH, WRAP_STEP_PX};
use super::LayoutError;

/// Padding inside a participant head box, per side (specs/sequence.md#constants).
pub const HEAD_PAD: (f64, f64) = (12.0, 8.0);
/// Smallest head box.
pub const HEAD_MIN: (f64, f64) = (80.0, 32.0);
/// The stick figure an `Actor` draws above its label.
pub const ACTOR_FIGURE: (f64, f64) = (24.0, 32.0);
/// Space above a message label and below its arrow.
pub const ROW_GAP: f64 = 12.0;
/// Padding of a message label over the line it sits on.
pub const LABEL_PAD: (f64, f64) = (6.0, 2.0);
/// Height of a self-message bracket.
pub const SELF_HEIGHT: f64 = 34.0;
/// How far a self-message reaches right of its own lifeline.
pub const SELF_WIDTH: f64 = 40.0;
/// Padding inside a note box, per side.
pub const NOTE_PAD: (f64, f64) = (10.0, 8.0);
/// Space between a fragment box and the content it encloses.
pub const FRAGMENT_PAD: f64 = 8.0;
/// Space between a participant box and the head boxes it encloses.
pub const BOX_PAD: f64 = 8.0;

/// Fragments deeper than this are not laid out. The parser bounds nesting at
/// `limits.nesting` (`E010`), so this only bounds a model built by hand.
const MAX_DEPTH: usize = 64;
/// Rounds of gap shrinking per wrap width; each halves the remaining interval between
/// the smallest and the widest gaps that satisfy the requirements.
const FIT_ROUNDS: usize = 10;
/// A message index this large never reaches the layout from the parser, which bounds
/// messages at `limits.edges`; the cap keeps a hand-built model from allocating.
const MAX_MESSAGES: usize = 1_000_000;

// ---------------------------------------------------------------------------
// Options

/// Render options with every number made finite and bounded, so hostile values cannot
/// produce NaN coordinates or unbounded work.
#[derive(Clone, Debug)]
struct Opts {
    target_width: f64,
    column_gap: f64,
    font_size: f64,
    wrap_width: f64,
    /// Label-box width factor of the font mode ([`text::width_tolerance`]).
    tolerance: f64,
}

fn finite_or(v: f64, default: f64) -> f64 {
    if v.is_nan() {
        default
    } else {
        v
    }
}

impl Opts {
    fn new(o: &RenderOptions) -> Self {
        Opts {
            // +∞ stays: "never fit". NaN falls back to the default.
            target_width: max(finite_or(o.target_width, 720.0), 0.0),
            column_gap: clamp(finite_or(o.node_spacing, 24.0), COLUMN_GAP_MIN, 1_000.0),
            font_size: clamp(finite_or(o.font_size, 14.0), 1.0, 1_000.0),
            wrap_width: clamp(finite_or(o.wrap_width, 200.0), 1.0, 100_000.0),
            tolerance: text::width_tolerance(o.font),
        }
    }

    fn style(&self) -> TextStyle {
        TextStyle {
            font_size: self.font_size,
            weight: Weight::Regular,
            italic: false,
        }
    }
}

/// A measured label, widened by the font mode's tolerance and cleaned of any
/// non-finite extent.
fn measured(o: &Opts, mut l: LabelLayout) -> LabelLayout {
    let fix = |v: f64| if v.is_finite() { max(v, 0.0) } else { 0.0 };
    l.width = fix(l.width) * o.tolerance;
    l.height = fix(l.height);
    l.line_height = fix(l.line_height);
    l
}

// ---------------------------------------------------------------------------
// Extents

/// A closed interval, empty when `lo > hi`.
#[derive(Clone, Copy, Debug)]
struct Extent {
    lo: f64,
    hi: f64,
}

impl Extent {
    const EMPTY: Extent = Extent {
        lo: f64::INFINITY,
        hi: f64::NEG_INFINITY,
    };

    fn span(&mut self, lo: f64, hi: f64) {
        if lo < self.lo {
            self.lo = lo;
        }
        if hi > self.hi {
            self.hi = hi;
        }
    }

    fn join(&mut self, other: Extent) {
        self.span(other.lo, other.hi);
    }

    fn is_empty(&self) -> bool {
        self.lo > self.hi
    }
}

/// The bounding box of the whole drawing, grown element by element.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    x: Extent,
    y: Extent,
}

impl Bounds {
    fn new() -> Self {
        Bounds {
            x: Extent::EMPTY,
            y: Extent::EMPTY,
        }
    }

    fn point(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.x.span(x, x);
            self.y.span(y, y);
        }
    }

    fn rect(&mut self, r: &Rect) {
        self.point(r.x, r.y);
        self.point(r.x + r.w, r.y + r.h);
    }
}

// ---------------------------------------------------------------------------
// Measurement

/// A width some element needs across the gaps `a..b`.
#[derive(Clone, Copy, Debug)]
struct Req {
    a: usize,
    b: usize,
    need: f64,
    /// Pre-order position, the tie-breaker that fixes the order deficits are spread in.
    at: u32,
}

/// Every label of the diagram, measured once at one wrap width.
struct Meas {
    /// Line height at the diagram's font size, used by a message row with no label.
    line_height: f64,
    participant: Vec<LabelLayout>,
    /// Outer `(w, h)` of every head box.
    head: Vec<(f64, f64)>,
    /// Indexed by `Message::index`; `None` for a message that draws no label.
    message: Vec<Option<LabelLayout>>,
    /// In the pre-order of the item tree.
    note: Vec<LabelLayout>,
    /// One entry per fragment in pre-order, one label per section.
    section: Vec<Vec<LabelLayout>>,
    /// Width of each fragment's kind tab.
    tab: Vec<f64>,
    box_label: Vec<LabelLayout>,
    reqs: Vec<Req>,
    /// Total item count, fragment sections included: the cost of one build pass.
    items: u64,
}

impl Meas {
    fn head_w(&self, p: usize) -> f64 {
        self.head.get(p).map_or(0.0, |h| h.0)
    }
}

/// Bytes of every label the diagram measures: the mandatory measurement cost and the
/// optional cost of one re-measurement (specs/sequence.md#fuel).
fn label_bytes(seq: &Sequence) -> u64 {
    fn items(list: &[Item], depth: usize, n: &mut u64) {
        if depth > MAX_DEPTH {
            return;
        }
        for item in list {
            match item {
                Item::Message(m) => *n = n.saturating_add(m.label.len() as u64),
                Item::Note(t) => *n = n.saturating_add(t.text.len() as u64),
                Item::Fragment(f) => {
                    for s in &f.sections {
                        *n = n.saturating_add(s.label.len() as u64);
                        items(&s.items, depth + 1, n);
                    }
                }
                _ => {}
            }
        }
    }
    let mut n = 0u64;
    for p in &seq.participants {
        n = n.saturating_add(p.label.len() as u64);
    }
    for b in &seq.boxes {
        n = n.saturating_add(b.label.len() as u64);
    }
    items(&seq.items, 0, &mut n);
    n
}

/// One mandatory unit per participant, per item and per fragment section.
fn structure_units(seq: &Sequence) -> u64 {
    fn items(list: &[Item], depth: usize, n: &mut u64) {
        if depth > MAX_DEPTH {
            return;
        }
        for item in list {
            *n = n.saturating_add(1);
            if let Item::Fragment(f) = item {
                for s in &f.sections {
                    *n = n.saturating_add(1);
                    items(&s.items, depth + 1, n);
                }
            }
        }
    }
    let mut n = seq.participants.len() as u64;
    items(&seq.items, 0, &mut n);
    n
}

struct MeasState<'a, 'd> {
    o: &'a Opts,
    wrap: f64,
    diags: &'d mut Diagnostics,
    m: Meas,
    at: u32,
}

impl MeasState<'_, '_> {
    fn label(&mut self, text: &str, wrap: f64) -> LabelLayout {
        let style = self.o.style();
        measured(self.o, text::layout_label(text, &style, wrap, self.diags))
    }

    fn require(&mut self, a: usize, b: usize, need: f64) {
        let at = self.at;
        self.at = self.at.saturating_add(1);
        if a < b && need > 0.0 && need.is_finite() && b < self.m.head.len() {
            self.m.reqs.push(Req { a, b, need, at });
        }
    }

    fn items(&mut self, list: &[Item], depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let n = self.m.head.len();
        for item in list {
            self.m.items = self.m.items.saturating_add(1);
            match item {
                Item::Message(msg) => {
                    let k = msg.index as usize;
                    let label = if msg.label.is_empty() {
                        None
                    } else {
                        // `:nowrap:` keeps the label on one line whatever its width.
                        let w = if msg.wrap == Some(false) {
                            f64::INFINITY
                        } else {
                            self.wrap
                        };
                        Some(self.label(&msg.label, w))
                    };
                    let lw = label.as_ref().map_or(0.0, |l| l.width);
                    if let Some(slot) = self.m.message.get_mut(k) {
                        *slot = label;
                    }
                    if msg.from == msg.to {
                        // The bracket and its label sit right of the lifeline.
                        let need = SELF_WIDTH + LABEL_PAD.0 + lw - self.m.head_w(msg.from) / 2.0;
                        self.require(msg.from, msg.from + 1, need);
                    } else {
                        let (a, b) = (umin(msg.from, msg.to), umax(msg.from, msg.to));
                        self.require(a, b, lw + 2.0 * LABEL_PAD.0);
                    }
                }
                Item::Note(note) => {
                    let l = self.label(&note.text, self.wrap);
                    let bw = l.width + 2.0 * NOTE_PAD.0;
                    self.m.note.push(l);
                    let (a, b) = (umin(note.from, note.to), umax(note.from, note.to));
                    match note.placement {
                        Placement::Over if a < b && b < n => {
                            self.require(a, b, bw - self.m.head_w(a) / 2.0 - self.m.head_w(b) / 2.0)
                        }
                        Placement::Over => {
                            let half = (bw - self.m.head_w(a)) / 2.0;
                            if a > 0 {
                                self.require(a - 1, a, half);
                            }
                            self.require(a, a + 1, half);
                        }
                        Placement::LeftOf => {
                            let need = NOTE_PAD.0 + bw - self.m.head_w(a) / 2.0;
                            if a > 0 {
                                self.require(a - 1, a, need);
                            }
                        }
                        Placement::RightOf => {
                            let need = NOTE_PAD.0 + bw - self.m.head_w(a) / 2.0;
                            self.require(a, a + 1, need);
                        }
                    }
                }
                Item::Fragment(f) => {
                    let at = self.m.section.len();
                    self.m.section.push(Vec::new());
                    self.m.tab.push(0.0);
                    let kind = self.label(f.kind.as_str(), f64::INFINITY);
                    if let Some(t) = self.m.tab.get_mut(at) {
                        *t = kind.width + 2.0 * FRAGMENT_PAD;
                    }
                    let mut labels = Vec::with_capacity(f.sections.len());
                    for s in &f.sections {
                        self.m.items = self.m.items.saturating_add(1);
                        labels.push(self.label(&s.label, self.wrap));
                    }
                    if let Some(slot) = self.m.section.get_mut(at) {
                        *slot = labels;
                    }
                    for s in &f.sections {
                        self.items(&s.items, depth + 1);
                    }
                }
                Item::Activate { .. } | Item::Deactivate { .. } => {}
            }
        }
    }
}

fn measure(seq: &Sequence, o: &Opts, wrap: f64, diags: &mut Diagnostics) -> Meas {
    let style = o.style();
    let probe = text::layout_label("", &style, f64::INFINITY, diags);
    let line_height = if probe.line_height.is_finite() && probe.line_height > 0.0 {
        probe.line_height
    } else {
        o.font_size
    };
    let n_msg = umin(seq.messages as usize, MAX_MESSAGES);
    let mut m = Meas {
        line_height,
        participant: Vec::with_capacity(seq.participants.len()),
        head: Vec::with_capacity(seq.participants.len()),
        message: vec![None; n_msg],
        note: Vec::new(),
        section: Vec::new(),
        tab: Vec::new(),
        box_label: Vec::with_capacity(seq.boxes.len()),
        reqs: Vec::new(),
        items: 0,
    };
    for p in &seq.participants {
        let l = measured(o, text::layout_label(&p.label, &style, wrap, diags));
        let mut w = max(l.width + 2.0 * HEAD_PAD.0, HEAD_MIN.0);
        let mut h = max(l.height + 2.0 * HEAD_PAD.1, HEAD_MIN.1);
        if p.kind == ParticipantKind::Actor {
            // The stick figure stands above the label, inside the same box.
            h += ACTOR_FIGURE.1 + 4.0;
            w = max(w, ACTOR_FIGURE.0 + 2.0 * HEAD_PAD.0);
        }
        m.head.push((w, h));
        m.participant.push(l);
    }
    for b in &seq.boxes {
        let l = measured(o, text::layout_label(&b.label, &style, wrap, diags));
        m.box_label.push(l);
    }
    let mut st = MeasState {
        o,
        wrap,
        diags,
        m,
        at: 0,
    };
    st.items(&seq.items, 0);
    let mut m = st.m;
    // Deficits are spread over the shortest spans first, then left to right, then in
    // source order, so the solved gaps are independent of collection order.
    m.reqs.sort_by_key(|x| (x.b - x.a, x.a, x.at));
    m
}

// ---------------------------------------------------------------------------
// Columns

/// Space a `box` adds to the gap on each side of its participant run: [`BOX_PAD`]
/// outside the head boxes plus [`BOX_PAD`] clearance to the neighbouring column.
fn box_pads(seq: &Sequence, n: usize) -> Vec<f64> {
    let mut pad = vec![0.0; n.saturating_sub(1)];
    for b in &seq.boxes {
        let Some((lo, hi)) = member_range(b.participants.as_slice(), n) else {
            continue;
        };
        if lo > 0 {
            if let Some(g) = pad.get_mut(lo - 1) {
                *g += 2.0 * BOX_PAD;
            }
        }
        if let Some(g) = pad.get_mut(hi) {
            *g += 2.0 * BOX_PAD;
        }
    }
    pad
}

fn member_range(members: &[usize], n: usize) -> Option<(usize, usize)> {
    let mut lo = usize::MAX;
    let mut hi = 0usize;
    let mut any = false;
    for &p in members {
        if p < n {
            any = true;
            lo = umin(lo, p);
            hi = umax(hi, p);
        }
    }
    any.then_some((lo, hi))
}

fn umin(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}

fn umax(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

/// Grows `base` until every requirement is met, spreading each deficit equally over
/// the gaps it spans. `reqs` is already in the order the deficits are applied.
fn solve_gaps(base: &[f64], reqs: &[Req]) -> Vec<f64> {
    let mut g = base.to_vec();
    for r in reqs {
        let (Some(span), Some(slice)) = (r.b.checked_sub(r.a), g.get(r.a..r.b)) else {
            continue;
        };
        if span == 0 {
            continue;
        }
        let have = slice.iter().fold(0.0f64, |a, v| a + v);
        if r.need <= have {
            continue;
        }
        let add = (r.need - have) / span as f64;
        if let Some(slice) = g.get_mut(r.a..r.b) {
            for v in slice {
                *v += add;
            }
        }
    }
    g
}

/// Cost of one [`solve_gaps`] pass: one optional unit per requirement and spanned gap
/// (specs/sequence.md#fuel).
fn solve_cost(reqs: &[Req]) -> u64 {
    reqs.iter()
        .fold(0u64, |a, r| a.saturating_add((r.b - r.a) as u64))
}

/// Column centres from the head widths and the gaps between them.
fn centres(head: &[(f64, f64)], gaps: &[f64]) -> Vec<f64> {
    let mut cx = Vec::with_capacity(head.len());
    let mut left = 0.0;
    for (i, &(w, _)) in head.iter().enumerate() {
        if i > 0 {
            left += gaps.get(i - 1).copied().unwrap_or(0.0);
        }
        cx.push(left + w / 2.0);
        left += w;
    }
    cx
}

// ---------------------------------------------------------------------------
// Rows

/// One open activation bar: its nesting depth, the row it opened on and the slot it
/// reserved in the geometry, so bars are listed in the order they open.
#[derive(Clone, Copy, Debug)]
struct Open {
    depth: u32,
    start: f64,
    slot: usize,
}

struct Build<'a> {
    seq: &'a Sequence,
    m: &'a Meas,
    cx: Vec<f64>,
    y: f64,
    stacks: Vec<Vec<Open>>,
    created: Vec<Option<f64>>,
    destroyed: Vec<Option<f64>>,
    messages: Vec<MessageGeom>,
    notes: Vec<NoteGeom>,
    fragments: Vec<FragmentGeom>,
    activations: Vec<ActivationGeom>,
    note_at: usize,
    frag_at: usize,
    numbered: bool,
}

impl<'a> Build<'a> {
    fn new(seq: &'a Sequence, m: &'a Meas, gaps: &[f64]) -> Self {
        let n = seq.participants.len();
        let cx = centres(&m.head, gaps);
        // The first row clears the tallest head box that starts at the top.
        let top = m
            .head
            .iter()
            .zip(&seq.participants)
            .filter(|(_, p)| p.created_by.is_none())
            .map(|(h, _)| h.1)
            .fold(0.0f64, |a, h| if h > a { h } else { a });
        let top = if top > 0.0 { top } else { HEAD_MIN.1 };
        Build {
            seq,
            m,
            cx,
            y: top + ROW_GAP,
            stacks: vec![Vec::new(); n],
            created: vec![None; n],
            destroyed: vec![None; n],
            messages: vec![
                MessageGeom {
                    from: (0.0, 0.0),
                    to: (0.0, 0.0),
                    self_loop: false,
                    label: None,
                    number: None,
                };
                m.message.len()
            ],
            notes: Vec::with_capacity(m.note.len()),
            fragments: Vec::with_capacity(m.section.len()),
            activations: Vec::new(),
            note_at: 0,
            frag_at: 0,
            numbered: seq.autonumber.is_some_and(|a| a.visible),
        }
    }

    fn centre(&self, p: usize) -> f64 {
        self.cx.get(p).copied().unwrap_or(0.0)
    }

    fn head_w(&self, p: usize) -> f64 {
        self.m.head.get(p).map_or(0.0, |h| h.0)
    }

    /// The x a message touches at column `p`: the innermost open activation bar's
    /// right or left edge, else the lifeline.
    fn edge(&self, p: usize, right: bool) -> f64 {
        let c = self.centre(p);
        match self.stacks.get(p).and_then(|s| s.last()) {
            Some(open) => {
                let x = c - ACTIVATION_W / 2.0 + f64::from(open.depth) * ACTIVATION_NEST;
                if right {
                    x + ACTIVATION_W
                } else {
                    x
                }
            }
            None => c,
        }
    }

    fn open(&mut self, p: usize, y: f64) {
        let Some(stack) = self.stacks.get(p) else {
            return;
        };
        let depth = u32::try_from(stack.len()).unwrap_or(u32::MAX);
        let slot = self.activations.len();
        self.activations.push(ActivationGeom {
            participant: p,
            depth,
            bar: Rect {
                x: self.centre(p) - ACTIVATION_W / 2.0 + f64::from(depth) * ACTIVATION_NEST,
                y,
                w: ACTIVATION_W,
                h: 0.0,
            },
        });
        if let Some(stack) = self.stacks.get_mut(p) {
            stack.push(Open {
                depth,
                start: y,
                slot,
            });
        }
    }

    /// Closes the innermost bar on `p`. A `deactivate` with nothing open is dropped
    /// (`W023` is the parser's; the layout draws nothing).
    fn close(&mut self, p: usize, y: f64) {
        let Some(open) = self.stacks.get_mut(p).and_then(Vec::pop) else {
            return;
        };
        if let Some(bar) = self.activations.get_mut(open.slot) {
            bar.bar.h = max(y - open.start, 0.0);
        }
    }

    fn column_extent(&self, p: usize) -> Extent {
        let c = self.centre(p);
        let w = self.head_w(p) / 2.0;
        Extent {
            lo: c - w,
            hi: c + w,
        }
    }

    fn walk(&mut self, list: &[Item], depth: usize) -> Extent {
        let mut ext = Extent::EMPTY;
        if depth > MAX_DEPTH {
            return ext;
        }
        let n = self.seq.participants.len();
        for item in list {
            match item {
                Item::Message(msg) => {
                    if msg.from >= n || msg.to >= n {
                        continue;
                    }
                    ext.join(self.column_extent(msg.from));
                    ext.join(self.column_extent(msg.to));
                    ext.join(self.message(msg));
                }
                Item::Note(note) => {
                    let at = self.note_at;
                    self.note_at += 1;
                    let Some(label) = self.m.note.get(at).cloned() else {
                        continue;
                    };
                    if note.from < n {
                        ext.join(self.column_extent(note.from));
                    }
                    if note.to < n {
                        ext.join(self.column_extent(note.to));
                    }
                    ext.join(self.note(note, label));
                }
                Item::Fragment(f) => ext.join(self.fragment(f, depth)),
                Item::Activate { participant, .. } => {
                    if *participant < n {
                        ext.join(self.column_extent(*participant));
                        self.open(*participant, self.y);
                    }
                }
                Item::Deactivate { participant, .. } => {
                    if *participant < n {
                        ext.join(self.column_extent(*participant));
                        self.close(*participant, self.y + ROW_GAP / 2.0);
                    }
                }
            }
        }
        ext
    }

    fn message(&mut self, msg: &Message) -> Extent {
        let k = msg.index as usize;
        let label = self.m.message.get(k).cloned().flatten();
        let (lw, lh) = label.as_ref().map_or((0.0, 0.0), |l| (l.width, l.height));
        let mut ext = Extent::EMPTY;
        let geom = if msg.from == msg.to {
            let h = max(SELF_HEIGHT, lh) + 2.0 * ROW_GAP;
            let top = self.y + ROW_GAP;
            let bottom = top + SELF_HEIGHT;
            if msg.activate {
                self.open(msg.to, bottom);
            }
            let x = self.edge(msg.from, true);
            if msg.deactivate {
                self.close(msg.from, bottom + ROW_GAP / 2.0);
            }
            let c = self.centre(msg.from);
            let label_x = c + SELF_WIDTH + LABEL_PAD.0 + lw / 2.0;
            ext.span(x, c + SELF_WIDTH + LABEL_PAD.0 + lw);
            self.y += h;
            self.mark(msg, bottom);
            MessageGeom {
                from: (x, top),
                to: (x, bottom),
                self_loop: true,
                label: label.map(|l| (label_x, (top + bottom) / 2.0, l)),
                number: self.numbered.then_some((x + NUMBER_R, top)),
            }
        } else {
            let h = max(lh + 2.0 * ROW_GAP, 2.0 * ROW_GAP + self.m.line_height);
            let arrow = self.y + h;
            if msg.activate {
                self.open(msg.to, arrow);
            }
            let rightwards = self.centre(msg.to) >= self.centre(msg.from);
            let sx = match msg.central {
                Central::Source | Central::Both => self.centre(msg.from),
                _ => self.edge(msg.from, rightwards),
            };
            let tx = match msg.central {
                Central::Target | Central::Both => self.centre(msg.to),
                _ => self.edge(msg.to, !rightwards),
            };
            if msg.deactivate {
                self.close(msg.from, arrow + ROW_GAP / 2.0);
            }
            let mid = (sx + tx) / 2.0;
            ext.span(min(sx, tx), max(sx, tx));
            ext.span(mid - lw / 2.0, mid + lw / 2.0);
            let step = if rightwards { NUMBER_R } else { -NUMBER_R };
            ext.span(sx + step - NUMBER_R, sx + step + NUMBER_R);
            self.y += h;
            self.mark(msg, arrow);
            MessageGeom {
                from: (sx, arrow),
                to: (tx, arrow),
                self_loop: false,
                label: label.map(|l| (mid, arrow - ROW_GAP - lh / 2.0, l)),
                number: self.numbered.then_some((sx + step, arrow)),
            }
        };
        if let Some(slot) = self.messages.get_mut(k) {
            *slot = geom;
        }
        ext
    }

    /// Records the rows a `create` starts a lifeline on and a `destroy` ends one on.
    fn mark(&mut self, msg: &Message, y: f64) {
        for p in [msg.from, msg.to] {
            let Some(part) = self.seq.participants.get(p) else {
                continue;
            };
            if part.created_by == Some(msg.index) {
                if let Some(slot) = self.created.get_mut(p) {
                    *slot = Some(y);
                }
            }
            if part.destroyed_by == Some(msg.index) {
                if let Some(slot) = self.destroyed.get_mut(p) {
                    *slot = Some(y);
                }
            }
        }
    }

    fn note(&mut self, note: &Note, label: LabelLayout) -> Extent {
        let n = self.seq.participants.len();
        let w = label.width + 2.0 * NOTE_PAD.0;
        let h = label.height + 2.0 * NOTE_PAD.1;
        let (a, b) = (umin(note.from, note.to), umax(note.from, note.to));
        let ca = self.centre(umin(a, n.saturating_sub(1)));
        let (x, w) = match note.placement {
            Placement::Over if a < b && b < n => {
                let cb = self.centre(b);
                let w = max(w, cb - ca);
                ((ca + cb) / 2.0 - w / 2.0, w)
            }
            Placement::Over => (ca - w / 2.0, w),
            Placement::LeftOf => (ca - NOTE_PAD.0 - w, w),
            Placement::RightOf => (ca + NOTE_PAD.0, w),
        };
        let box_ = Rect { x, y: self.y, w, h };
        self.y += h + ROW_GAP;
        let at = self.notes.len();
        self.notes.push(NoteGeom {
            index: at,
            placement: note.placement,
            box_,
            label,
        });
        Extent { lo: x, hi: x + w }
    }

    fn fragment(&mut self, f: &Fragment, depth: usize) -> Extent {
        let at = self.frag_at;
        self.frag_at += 1;
        let labels = self.m.section.get(at).cloned().unwrap_or_default();
        let tab_w = self.m.tab.get(at).copied().unwrap_or(0.0);
        let header = labels.first().cloned().unwrap_or_default();
        let slot = self.fragments.len();
        self.fragments.push(FragmentGeom {
            kind: f.kind,
            box_: Rect::default(),
            depth: u32::try_from(depth).unwrap_or(u32::MAX),
            tab: Rect::default(),
            label: header.clone(),
            label_x: 0.0,
            label_y: 0.0,
            sections: Vec::new(),
        });

        let top = self.y;
        self.y += FRAGMENT_TAB + header.height + FRAGMENT_PAD;
        let mut ext = Extent::EMPTY;
        let mut dividers: Vec<(f64, LabelLayout)> = Vec::new();
        for (i, s) in f.sections.iter().enumerate() {
            if i > 0 {
                let label = labels.get(i).cloned().unwrap_or_default();
                self.y += ROW_GAP / 2.0;
                let y = self.y;
                self.y += label.height + ROW_GAP / 2.0;
                dividers.push((y, label));
            }
            ext.join(self.walk(&s.items, depth + 1));
        }
        self.y += FRAGMENT_PAD;
        let bottom = self.y;

        if ext.is_empty() {
            // A fragment with no items spans the whole diagram.
            let n = self.seq.participants.len();
            if n == 0 {
                ext.span(0.0, HEAD_MIN.0);
            } else {
                ext.join(self.column_extent(0));
                ext.join(self.column_extent(n - 1));
            }
        }
        let x0 = ext.lo - FRAGMENT_PAD;
        let x1 = max(
            ext.hi + FRAGMENT_PAD,
            x0 + tab_w + header.width + 2.0 * FRAGMENT_PAD,
        );
        let box_ = Rect {
            x: x0,
            y: top,
            w: x1 - x0,
            h: bottom - top,
        };
        let sections = dividers
            .into_iter()
            .map(|(y, label)| SectionGeom {
                label_x: x0 + FRAGMENT_PAD + label.width / 2.0,
                label_y: y + label.height / 2.0,
                y,
                label,
            })
            .collect();
        if let Some(g) = self.fragments.get_mut(slot) {
            g.box_ = box_;
            g.tab = Rect {
                x: x0,
                y: top,
                w: tab_w,
                h: FRAGMENT_TAB,
            };
            g.label_x = x0 + tab_w + FRAGMENT_PAD + header.width / 2.0;
            g.label_y = top + (FRAGMENT_TAB + header.height) / 2.0;
            g.sections = sections;
        }
        Extent { lo: x0, hi: x1 }
    }
}

// ---------------------------------------------------------------------------
// Assembly

/// Lays the rows out over one set of column gaps and returns the finished, translated
/// geometry.
fn build(seq: &Sequence, m: &Meas, gaps: &[f64]) -> SequenceGeometry {
    let mut b = Build::new(seq, m, gaps);
    b.walk(&seq.items, 0);
    let body = b.y;
    // An activation still open at the end closes at the last row.
    for p in 0..b.stacks.len() {
        for _ in 0..b.stacks.get(p).map_or(0, Vec::len) {
            b.close(p, body);
        }
    }

    let foot_y = body + ROW_GAP;
    let mut participants = Vec::with_capacity(seq.participants.len());
    for i in 0..seq.participants.len() {
        let (w, h) = m.head.get(i).copied().unwrap_or(HEAD_MIN);
        let created = b.created.get(i).copied().flatten();
        let top = created.map_or(0.0, |y| y - h / 2.0);
        let x = b.centre(i) - w / 2.0;
        let head = Rect { x, y: top, w, h };
        let lifeline_top = top + h;
        let (foot, lifeline_bottom) = match b.destroyed.get(i).copied().flatten() {
            Some(y) => (None, max(y, lifeline_top)),
            None => (
                Some(Rect {
                    x,
                    y: max(foot_y, lifeline_top),
                    w,
                    h,
                }),
                max(foot_y, lifeline_top),
            ),
        };
        participants.push(ParticipantGeom {
            x: b.centre(i),
            head,
            foot,
            lifeline: (lifeline_top, lifeline_bottom),
            label: m.participant.get(i).cloned().unwrap_or_default(),
        });
    }

    let n = seq.participants.len();
    let mut boxes = Vec::with_capacity(seq.boxes.len());
    for (i, bx) in seq.boxes.iter().enumerate() {
        let Some((lo, hi)) = member_range(bx.participants.as_slice(), n) else {
            continue;
        };
        let label = m.box_label.get(i).cloned().unwrap_or_default();
        let x0 = b.centre(lo) - b.head_w(lo) / 2.0 - BOX_PAD;
        let x1 = b.centre(hi) + b.head_w(hi) / 2.0 + BOX_PAD;
        let y0 = -(label.height + 2.0 * BOX_PAD);
        let y1 = bx
            .participants
            .iter()
            .filter_map(|&p| m.head.get(p))
            .fold(0.0f64, |a, h| if h.1 > a { h.1 } else { a })
            + BOX_PAD;
        boxes.push(BoxGeom {
            box_: Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            },
            label_x: (x0 + x1) / 2.0,
            label_y: y0 + BOX_PAD + label.height / 2.0,
            label,
        });
    }

    let mut geom = SequenceGeometry {
        width: 0.0,
        height: 0.0,
        participants,
        messages: b.messages,
        notes: b.notes,
        fragments: b.fragments,
        activations: b.activations,
        boxes,
        fuel_used: 0,
    };
    fit_page(&mut geom);
    geom
}

/// Grows the bounding box over everything drawn, then translates the drawing so it
/// starts at [`MARGIN`] on both axes.
fn fit_page(g: &mut SequenceGeometry) {
    let mut bb = Bounds::new();
    for p in &g.participants {
        bb.rect(&p.head);
        if let Some(f) = p.foot {
            bb.rect(&f);
        }
        bb.point(p.x, p.lifeline.0);
        bb.point(p.x, p.lifeline.1);
    }
    for m in &g.messages {
        bb.point(m.from.0, m.from.1);
        bb.point(m.to.0, m.to.1);
        if let Some((x, y, l)) = &m.label {
            bb.point(x - l.width / 2.0, y - l.height / 2.0);
            bb.point(x + l.width / 2.0, y + l.height / 2.0);
        }
        if let Some((x, y)) = m.number {
            bb.point(x - NUMBER_R, y - NUMBER_R);
            bb.point(x + NUMBER_R, y + NUMBER_R);
        }
    }
    for n in &g.notes {
        bb.rect(&n.box_);
    }
    for f in &g.fragments {
        bb.rect(&f.box_);
        bb.rect(&f.tab);
    }
    for a in &g.activations {
        bb.rect(&a.bar);
    }
    for b in &g.boxes {
        bb.rect(&b.box_);
    }
    let (dx, dy, w, h) = if bb.x.is_empty() || bb.y.is_empty() {
        (MARGIN, MARGIN, HEAD_MIN.0, HEAD_MIN.1)
    } else {
        (
            MARGIN - bb.x.lo,
            MARGIN - bb.y.lo,
            bb.x.hi - bb.x.lo,
            bb.y.hi - bb.y.lo,
        )
    };
    translate(g, dx, dy);
    g.width = w + 2.0 * MARGIN;
    g.height = h + 2.0 * MARGIN;
}

fn translate(g: &mut SequenceGeometry, dx: f64, dy: f64) {
    let shift = |r: &mut Rect| {
        r.x += dx;
        r.y += dy;
    };
    for p in &mut g.participants {
        p.x += dx;
        shift(&mut p.head);
        if let Some(f) = &mut p.foot {
            shift(f);
        }
        p.lifeline.0 += dy;
        p.lifeline.1 += dy;
    }
    for m in &mut g.messages {
        m.from.0 += dx;
        m.from.1 += dy;
        m.to.0 += dx;
        m.to.1 += dy;
        if let Some((x, y, _)) = &mut m.label {
            *x += dx;
            *y += dy;
        }
        if let Some((x, y)) = &mut m.number {
            *x += dx;
            *y += dy;
        }
    }
    for n in &mut g.notes {
        shift(&mut n.box_);
    }
    for f in &mut g.fragments {
        shift(&mut f.box_);
        shift(&mut f.tab);
        f.label_x += dx;
        f.label_y += dy;
        for s in &mut f.sections {
            s.y += dy;
            s.label_x += dx;
            s.label_y += dy;
        }
    }
    for a in &mut g.activations {
        shift(&mut a.bar);
    }
    for b in &mut g.boxes {
        shift(&mut b.box_);
        b.label_x += dx;
        b.label_y += dy;
    }
}

// ---------------------------------------------------------------------------
// Container fit

/// Lays the diagram out at one wrap width, shrinking the column gaps toward
/// [`COLUMN_GAP_MIN`] until the drawing fits `target_width` or cannot narrow further
/// (specs/sequence.md#fragments-and-container-fit).
fn fit_gaps(seq: &Sequence, o: &Opts, m: &Meas, fuel: &mut Fuel) -> SequenceGeometry {
    let pads = box_pads(seq, seq.participants.len());
    let base = |t: f64| -> Vec<f64> {
        pads.iter()
            .map(|p| p + COLUMN_GAP_MIN + t * (o.column_gap - COLUMN_GAP_MIN))
            .collect()
    };
    let pass = solve_cost(&m.reqs).saturating_add(m.items);
    let wide = build(seq, m, &solve_gaps(&base(1.0), &m.reqs));
    if wide.width <= o.target_width || pads.is_empty() {
        return wide;
    }
    if fuel.burn_optional(pass).is_err() {
        return wide;
    }
    let narrow = build(seq, m, &solve_gaps(&base(0.0), &m.reqs));
    if narrow.width >= o.target_width {
        return narrow;
    }
    // The width grows with `t`, so the widest drawing that still fits is one
    // bisection away; a fixed round count keeps the result deterministic.
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    let mut best = narrow;
    for _ in 0..FIT_ROUNDS {
        if fuel.burn_optional(pass).is_err() {
            break;
        }
        let t = (lo + hi) / 2.0;
        let cand = build(seq, m, &solve_gaps(&base(t), &m.reqs));
        if cand.width <= o.target_width {
            best = cand;
            lo = t;
        } else {
            hi = t;
        }
    }
    best
}

/// Measures, lays out and fits a sequence diagram. Fuel exhaustion in a mandatory phase
/// is `TooLarge`; an optional pass stops and keeps the previous result.
pub fn layout_sequence(
    seq: &Sequence,
    opts: &RenderOptions,
    fuel: &mut Fuel,
    diags: &mut Diagnostics,
) -> Result<SequenceGeometry, LayoutError> {
    let spent = fuel.used();
    let o = Opts::new(opts);
    // Mandatory: one unit per participant, per item and per fragment section, plus one
    // per byte of label text measured (specs/sequence.md#fuel).
    let bytes = label_bytes(seq);
    if fuel
        .burn(structure_units(seq).saturating_add(bytes))
        .is_err()
    {
        return Err(LayoutError::TooLarge { what: "fuel" });
    }

    let mut wrap = o.wrap_width;
    // The widest wrap that fits wins; when no wrap fits, the least-wrapped layout does.
    // Narrowing a label buys a fit the diagram never reaches, and costs legibility and
    // height (specs/sequence.md#fragments-and-container-fit).
    let mut best: Option<SequenceGeometry> = None;
    loop {
        let m = measure(seq, &o, wrap, diags);
        let g = fit_gaps(seq, &o, &m, fuel);
        let fits = g.width <= o.target_width;
        if fits {
            best = Some(g);
            break;
        }
        best.get_or_insert(g);
        if wrap <= MIN_WRAP_WIDTH {
            break;
        }
        // Each re-measurement is optional work, charged per byte.
        if fuel.burn_optional(bytes).is_err() {
            break;
        }
        wrap = max(wrap - WRAP_STEP_PX, MIN_WRAP_WIDTH);
    }

    let mut g = best.unwrap_or_default();
    if g.width <= 0.0 || g.height <= 0.0 {
        g.width = 2.0 * MARGIN + HEAD_MIN.0;
        g.height = 2.0 * MARGIN + HEAD_MIN.1;
    }
    g.fuel_used = fuel.used().saturating_sub(spent);
    Ok(g)
}
