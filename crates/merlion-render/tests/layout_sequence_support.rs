//! Builders and invariant checks shared by the sequence-layout integration tests
//! (included with `#[path]`). Models are hand-built, so the layout stage is tested
//! independently of the parser.
#![allow(dead_code)]

use merlion_render::diag::Diagnostics;
use merlion_render::fuel::Fuel;
use merlion_render::geometry::sequence::{Rect, SequenceGeometry};
use merlion_render::layout::{layout_sequence, LayoutError};
use merlion_render::model::sequence::{
    Autonumber, Central, Fragment, FragmentKind, Item, Message, Note, Participant, ParticipantBox,
    ParticipantKind, Placement, Section, Sequence,
};
use merlion_render::options::RenderOptions;

/// Coordinates compare to this tolerance; the layout is exact, so the slack only
/// absorbs the sum-versus-product forms of an accumulated row cursor.
pub const EPS: f64 = 1e-6;

/// Builds a `Sequence` by hand: participants first, then the item tree.
pub struct S {
    pub seq: Sequence,
    next: u32,
}

impl Default for S {
    fn default() -> Self {
        S::new()
    }
}

impl S {
    pub fn new() -> Self {
        S {
            seq: Sequence::default(),
            next: 0,
        }
    }

    pub fn kind(&mut self, id: &str, label: &str, kind: ParticipantKind) -> usize {
        self.seq.participants.push(Participant {
            id: id.into(),
            label: label.into(),
            kind,
            ..Participant::default()
        });
        self.seq.participants.len() - 1
    }

    pub fn p(&mut self, id: &str) -> usize {
        self.kind(id, id, ParticipantKind::Participant)
    }

    pub fn ps(&mut self, ids: &[&str]) -> Vec<usize> {
        ids.iter().map(|i| self.p(i)).collect()
    }

    pub fn labelled(&mut self, id: &str, label: &str) -> usize {
        self.kind(id, label, ParticipantKind::Participant)
    }

    pub fn actor(&mut self, id: &str) -> usize {
        self.kind(id, id, ParticipantKind::Actor)
    }

    /// Groups `members` behind one box.
    pub fn group(&mut self, label: &str, members: &[usize]) -> usize {
        let b = self.seq.boxes.len();
        self.seq.boxes.push(ParticipantBox {
            label: label.into(),
            color: None,
            participants: members.to_vec(),
            span: Default::default(),
        });
        for &m in members {
            if let Some(p) = self.seq.participants.get_mut(m) {
                p.group = Some(b);
            }
        }
        b
    }

    /// The next message, numbered in the order the builder is called.
    pub fn msg(&mut self, from: usize, to: usize, label: &str) -> Item {
        self.msg_act(from, to, label, false, false)
    }

    pub fn msg_act(
        &mut self,
        from: usize,
        to: usize,
        label: &str,
        activate: bool,
        deactivate: bool,
    ) -> Item {
        let index = self.next;
        self.next += 1;
        Item::Message(Message {
            index,
            from,
            to,
            label: label.into(),
            activate,
            deactivate,
            ..Message::default()
        })
    }

    /// The next message, with `central` and `wrap` set.
    pub fn msg_opt(
        &mut self,
        from: usize,
        to: usize,
        label: &str,
        central: Central,
        wrap: Option<bool>,
    ) -> Item {
        let index = self.next;
        self.next += 1;
        Item::Message(Message {
            index,
            from,
            to,
            label: label.into(),
            central,
            wrap,
            ..Message::default()
        })
    }

    /// Index of the message the next `msg` call returns.
    pub fn peek(&self) -> u32 {
        self.next
    }

    pub fn note(&mut self, placement: Placement, from: usize, to: usize, text: &str) -> Item {
        Item::Note(Note {
            placement,
            from,
            to,
            text: text.into(),
            span: Default::default(),
        })
    }

    pub fn activate(&self, participant: usize) -> Item {
        Item::Activate {
            participant,
            span: Default::default(),
        }
    }

    pub fn deactivate(&self, participant: usize) -> Item {
        Item::Deactivate {
            participant,
            span: Default::default(),
        }
    }

    pub fn autonumber(&mut self) -> &mut Self {
        self.seq.autonumber = Some(Autonumber::default());
        self
    }

    /// Finishes the diagram with `items` at the top level.
    pub fn done(mut self, items: Vec<Item>) -> Sequence {
        self.seq.items = items;
        self.seq.messages = self.next;
        self.seq
    }
}

pub fn section(label: &str, items: Vec<Item>) -> Section {
    Section {
        label: label.into(),
        items,
        span: Default::default(),
    }
}

pub fn frag(kind: FragmentKind, sections: Vec<Section>) -> Item {
    Item::Fragment(Fragment {
        kind,
        sections,
        span: Default::default(),
    })
}

pub fn run(seq: &Sequence) -> SequenceGeometry {
    run_with(seq, &RenderOptions::default()).expect("layout")
}

pub fn run_with(seq: &Sequence, opts: &RenderOptions) -> Result<SequenceGeometry, LayoutError> {
    let mut fuel = Fuel::new(opts.fuel);
    let mut diags = Diagnostics::new(false);
    layout_sequence(seq, opts, &mut fuel, &mut diags)
}

pub fn run_fuel(seq: &Sequence, fuel: u64) -> Result<SequenceGeometry, LayoutError> {
    run_with(
        seq,
        &RenderOptions {
            fuel,
            ..RenderOptions::default()
        },
    )
}

// ---------------------------------------------------------------------------
// Rectangles

pub fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

pub fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x + a.w > b.x + EPS && b.x + b.w > a.x + EPS && a.y + a.h > b.y + EPS && b.y + b.h > a.y + EPS
}

pub fn contains(outer: &Rect, inner: &Rect) -> bool {
    inner.x >= outer.x - EPS
        && inner.y >= outer.y - EPS
        && inner.x + inner.w <= outer.x + outer.w + EPS
        && inner.y + inner.h <= outer.y + outer.h + EPS
}

pub fn finite(rc: &Rect) -> bool {
    rc.x.is_finite() && rc.y.is_finite() && rc.w.is_finite() && rc.h.is_finite()
}

/// The box a message label occupies, from its centre and measured size.
pub fn label_rect(g: &SequenceGeometry, k: usize) -> Option<Rect> {
    let m = g.messages.get(k)?;
    let (x, y, l) = m.label.as_ref()?;
    Some(r(x - l.width / 2.0, y - l.height / 2.0, l.width, l.height))
}

// ---------------------------------------------------------------------------
// The item tree, flattened the way the geometry lists it

#[derive(Default)]
pub struct Tree<'a> {
    pub notes: Vec<&'a Note>,
    pub frags: Vec<&'a Fragment>,
    /// Per fragment: the indices of the messages anywhere inside it.
    pub frag_msgs: Vec<Vec<u32>>,
    /// Per fragment: the positions in `notes` of the notes anywhere inside it.
    pub frag_notes: Vec<Vec<usize>>,
    /// Per fragment: the positions in `frags` of its direct children.
    pub frag_kids: Vec<Vec<usize>>,
    /// Per fragment: the columns its items touch.
    pub frag_cols: Vec<Vec<usize>>,
}

impl<'a> Tree<'a> {
    pub fn of(seq: &'a Sequence) -> Self {
        let mut t = Tree::default();
        t.walk(&seq.items, None);
        t
    }

    fn walk(&mut self, items: &'a [Item], parent: Option<usize>) {
        for item in items {
            match item {
                Item::Message(m) => self.claim(parent, Some(m.index), None, &[m.from, m.to]),
                Item::Note(n) => {
                    let at = self.notes.len();
                    self.notes.push(n);
                    self.claim(parent, None, Some(at), &[n.from, n.to]);
                }
                Item::Activate { participant, .. } | Item::Deactivate { participant, .. } => {
                    self.claim(parent, None, None, &[*participant]);
                }
                Item::Fragment(f) => {
                    let at = self.frags.len();
                    self.frags.push(f);
                    self.frag_msgs.push(Vec::new());
                    self.frag_notes.push(Vec::new());
                    self.frag_kids.push(Vec::new());
                    self.frag_cols.push(Vec::new());
                    if let Some(p) = parent {
                        if let Some(k) = self.frag_kids.get_mut(p) {
                            k.push(at);
                        }
                    }
                    for s in &f.sections {
                        self.walk(&s.items, Some(at));
                    }
                }
            }
        }
    }

    /// Records a leaf against every fragment on the path to the root.
    fn claim(
        &mut self,
        parent: Option<usize>,
        msg: Option<u32>,
        note: Option<usize>,
        cols: &[usize],
    ) {
        let mut at = parent;
        // Every ancestor of `parent` has a smaller pre-order index, so a bounded
        // walk up the chain reaches the root.
        for _ in 0..self.frags.len() + 1 {
            let Some(i) = at else { break };
            if let Some(m) = msg {
                if let Some(v) = self.frag_msgs.get_mut(i) {
                    v.push(m);
                }
            }
            if let Some(n) = note {
                if let Some(v) = self.frag_notes.get_mut(i) {
                    v.push(n);
                }
            }
            if let Some(v) = self.frag_cols.get_mut(i) {
                for &c in cols {
                    if !v.contains(&c) {
                        v.push(c);
                    }
                }
            }
            at = self.parent_of(i);
        }
    }

    fn parent_of(&self, child: usize) -> Option<usize> {
        self.frag_kids.iter().position(|kids| kids.contains(&child))
    }
}

// ---------------------------------------------------------------------------
// Invariants

/// Every invariant of specs/sequence.md#layout that holds for any model.
pub fn check(seq: &Sequence, g: &SequenceGeometry) {
    check_finite(g);
    check_columns(seq, g);
    check_notes(g);
    check_labels(g);
    check_endpoints(seq, g);
    check_fragments(seq, g);
    check_inside(g);
}

pub fn check_finite(g: &SequenceGeometry) {
    assert!(g.width.is_finite() && g.width > 0.0, "width {}", g.width);
    assert!(
        g.height.is_finite() && g.height > 0.0,
        "height {}",
        g.height
    );
    for (i, p) in g.participants.iter().enumerate() {
        assert!(p.x.is_finite() && finite(&p.head), "participant {i}");
        assert!(
            p.lifeline.0.is_finite() && p.lifeline.1.is_finite(),
            "lifeline {i}"
        );
        assert!(
            p.lifeline.1 >= p.lifeline.0 - EPS,
            "lifeline {i} runs backwards: {:?}",
            p.lifeline
        );
        if let Some(f) = p.foot {
            assert!(finite(&f), "foot {i}");
        }
    }
    for (k, m) in g.messages.iter().enumerate() {
        assert!(
            m.from.0.is_finite()
                && m.from.1.is_finite()
                && m.to.0.is_finite()
                && m.to.1.is_finite(),
            "message {k}: {m:?}"
        );
    }
}

/// Columns run left to right in source order and no two head or foot boxes touch.
pub fn check_columns(seq: &Sequence, g: &SequenceGeometry) {
    assert_eq!(
        g.participants.len(),
        seq.participants.len(),
        "one column per participant"
    );
    for w in g.participants.windows(2) {
        assert!(
            w[1].x > w[0].x + EPS,
            "columns out of order: {} then {}",
            w[0].x,
            w[1].x
        );
    }
    for (i, a) in g.participants.iter().enumerate() {
        for b in g.participants.iter().skip(i + 1) {
            assert!(
                !overlaps(&a.head, &b.head),
                "head boxes overlap: {:?} {:?}",
                a.head,
                b.head
            );
            if let (Some(x), Some(y)) = (a.foot, b.foot) {
                assert!(!overlaps(&x, &y), "foot boxes overlap: {x:?} {y:?}");
            }
        }
        // The lifeline hangs below the head and reaches the foot.
        assert!(
            a.lifeline.0 >= a.head.y + a.head.h - EPS,
            "lifeline {i} starts above its head"
        );
        if let Some(f) = a.foot {
            assert!(a.lifeline.1 <= f.y + EPS, "lifeline {i} runs past its foot");
        }
    }
}

/// Note boxes never overlap each other or a head or foot box.
pub fn check_notes(g: &SequenceGeometry) {
    for (i, a) in g.notes.iter().enumerate() {
        assert!(finite(&a.box_), "note {i}");
        for b in g.notes.iter().skip(i + 1) {
            assert!(
                !overlaps(&a.box_, &b.box_),
                "notes overlap: {:?} {:?}",
                a.box_,
                b.box_
            );
        }
        for p in &g.participants {
            assert!(
                !overlaps(&a.box_, &p.head),
                "note {:?} overlaps a head box {:?}",
                a.box_,
                p.head
            );
            if let Some(f) = p.foot {
                assert!(
                    !overlaps(&a.box_, &f),
                    "note {:?} overlaps a foot box {f:?}",
                    a.box_
                );
            }
        }
    }
}

/// Message labels never overlap each other or a note.
pub fn check_labels(g: &SequenceGeometry) {
    let boxes: Vec<(usize, Rect)> = (0..g.messages.len())
        .filter_map(|k| label_rect(g, k).map(|rc| (k, rc)))
        .collect();
    for (i, (ka, a)) in boxes.iter().enumerate() {
        assert!(finite(a), "label {ka}");
        for (kb, b) in boxes.iter().skip(i + 1) {
            assert!(!overlaps(a, b), "labels {ka} and {kb} overlap: {a:?} {b:?}");
        }
        for n in &g.notes {
            assert!(
                !overlaps(a, &n.box_),
                "label {ka} overlaps a note: {a:?} {:?}",
                n.box_
            );
        }
    }
}

/// Every drawn end sits on a lifeline or on an activation-bar edge of its own column.
pub fn check_endpoints(seq: &Sequence, g: &SequenceGeometry) {
    let mut anchors: Vec<Vec<f64>> = g.participants.iter().map(|p| vec![p.x]).collect();
    for a in &g.activations {
        if let Some(v) = anchors.get_mut(a.participant) {
            v.push(a.bar.x);
            v.push(a.bar.x + a.bar.w);
        }
    }
    let on = |v: &[f64], x: f64| v.iter().any(|a| (a - x).abs() <= EPS);
    for (from, to, k) in messages(seq) {
        let Some(m) = g.messages.get(k as usize) else {
            panic!("message {k} has no geometry");
        };
        assert_eq!(m.self_loop, from == to, "message {k} self-loop flag");
        if let Some(v) = anchors.get(from) {
            assert!(
                on(v, m.from.0),
                "message {k} starts at {} off column {from} {v:?}",
                m.from.0
            );
        }
        if let Some(v) = anchors.get(to) {
            assert!(
                on(v, m.to.0),
                "message {k} ends at {} off column {to} {v:?}",
                m.to.0
            );
        }
    }
}

/// A fragment box encloses every row inside it, and its parent encloses it.
pub fn check_fragments(seq: &Sequence, g: &SequenceGeometry) {
    let t = Tree::of(seq);
    assert_eq!(g.fragments.len(), t.frags.len(), "one box per fragment");
    for (i, fg) in g.fragments.iter().enumerate() {
        assert!(finite(&fg.box_), "fragment {i}");
        assert!(
            fg.box_.w > 0.0 && fg.box_.h > 0.0,
            "fragment {i} is empty: {:?}",
            fg.box_
        );
        assert!(
            contains(&fg.box_, &fg.tab),
            "fragment {i} tab escapes its box"
        );
        for &k in t.frag_msgs.get(i).into_iter().flatten() {
            let Some(m) = g.messages.get(k as usize) else {
                continue;
            };
            let lo = r(
                m.from.0.min(m.to.0),
                m.from.1.min(m.to.1),
                (m.to.0 - m.from.0).abs(),
                (m.to.1 - m.from.1).abs(),
            );
            assert!(
                contains(&fg.box_, &lo),
                "fragment {i} {:?} does not enclose message {k} {lo:?}",
                fg.box_
            );
            if let Some(lb) = label_rect(g, k as usize) {
                assert!(
                    contains(&fg.box_, &lb),
                    "fragment {i} does not enclose the label of message {k}"
                );
            }
        }
        for &n in t.frag_notes.get(i).into_iter().flatten() {
            if let Some(ng) = g.notes.get(n) {
                assert!(
                    contains(&fg.box_, &ng.box_),
                    "fragment {i} does not enclose note {n}"
                );
            }
        }
        for &kid in t.frag_kids.get(i).into_iter().flatten() {
            if let Some(kg) = g.fragments.get(kid) {
                assert!(
                    contains(&fg.box_, &kg.box_),
                    "fragment {i} does not enclose its child {kid}"
                );
                assert!(
                    kg.depth > fg.depth,
                    "child {kid} sits at the parent's depth"
                );
            }
        }
        for s in &fg.sections {
            assert!(
                s.y >= fg.box_.y - EPS && s.y <= fg.box_.y + fg.box_.h + EPS,
                "fragment {i} divider outside the box"
            );
        }
    }
}

/// Nothing is drawn outside the viewBox.
pub fn check_inside(g: &SequenceGeometry) {
    let page = r(0.0, 0.0, g.width, g.height);
    for (i, p) in g.participants.iter().enumerate() {
        assert!(contains(&page, &p.head), "head {i} outside {page:?}");
        if let Some(f) = p.foot {
            assert!(contains(&page, &f), "foot {i} outside {page:?}");
        }
        assert!(
            p.lifeline.0 >= -EPS && p.lifeline.1 <= g.height + EPS,
            "lifeline {i} outside"
        );
    }
    for (i, n) in g.notes.iter().enumerate() {
        assert!(contains(&page, &n.box_), "note {i} outside {page:?}");
    }
    for (i, f) in g.fragments.iter().enumerate() {
        assert!(contains(&page, &f.box_), "fragment {i} outside {page:?}");
    }
    for (i, a) in g.activations.iter().enumerate() {
        assert!(contains(&page, &a.bar), "activation {i} outside {page:?}");
    }
    for (i, b) in g.boxes.iter().enumerate() {
        assert!(contains(&page, &b.box_), "box {i} outside {page:?}");
    }
    for k in 0..g.messages.len() {
        if let Some(rc) = label_rect(g, k) {
            assert!(contains(&page, &rc), "label {k} outside {page:?}");
        }
    }
}

/// `(from, to, index)` of every message in the diagram, fragments included.
pub fn messages(seq: &Sequence) -> Vec<(usize, usize, u32)> {
    let mut out = Vec::new();
    collect_messages(&seq.items, &mut out, 0);
    out
}

fn collect_messages(items: &[Item], out: &mut Vec<(usize, usize, u32)>, depth: u32) {
    if depth > 64 {
        return;
    }
    for item in items {
        match item {
            Item::Message(m) => out.push((m.from, m.to, m.index)),
            Item::Fragment(f) => {
                for s in &f.sections {
                    collect_messages(&s.items, out, depth + 1);
                }
            }
            _ => {}
        }
    }
}
