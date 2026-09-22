//! Pre-order walk of the item tree (specs/sequence.md#model).
//!
//! The drawing and the outline both read the items in source order, with the fragments
//! and notes numbered as [`crate::geometry::sequence::SequenceGeometry`] lists them:
//! "the pre-order of the item tree, which is source order". The walk keeps its own
//! stack, so a model nested deeper than the parser allows costs memory rather than
//! recursion, and the stack is capped at [`MAX_FRAMES`].

use alloc::vec::Vec;

use crate::model::sequence::{Fragment, Item, Message, Note, Sequence};

/// Deepest nesting the walk descends into: four times the parser's `limits.nesting`
/// (64), so every model the parser builds walks whole and a hand-built one still stops.
pub const MAX_FRAMES: usize = 256;

/// One step of the walk.
#[derive(Clone, Copy)]
pub enum Ev<'a> {
    Message(&'a Message),
    /// The `n`-th note of the pre-order, which is `SequenceGeometry::notes[n]`.
    Note(&'a Note, usize),
    /// Opens section `k` of the `f`-th fragment of the pre-order, which is
    /// `SequenceGeometry::fragments[f]`. `k == 0` opens the fragment itself.
    Section {
        frag: &'a Fragment,
        f: usize,
        k: usize,
    },
}

/// One event and the number of fragments enclosing it.
#[derive(Clone, Copy)]
pub struct Step<'a> {
    pub depth: usize,
    pub ev: Ev<'a>,
}

struct Frame<'a> {
    /// The fragment whose sections this frame walks, and its pre-order index.
    frag: Option<(&'a Fragment, usize)>,
    section: usize,
    items: &'a [Item],
    at: usize,
}

/// Every item of `seq` in source order, fragments before their contents.
pub fn walk(seq: &Sequence) -> Vec<Step<'_>> {
    let mut out: Vec<Step> = Vec::new();
    let mut stack: Vec<Frame> = alloc::vec![Frame {
        frag: None,
        section: 0,
        items: &seq.items,
        at: 0,
    }];
    let (mut fragments, mut notes) = (0usize, 0usize);
    while let Some(top) = stack.len().checked_sub(1) {
        // Depth counts the enclosing fragments: frame 0 holds the top-level items.
        let depth = top;
        if stack[top].at >= stack[top].items.len() {
            let next = stack[top].section + 1;
            match stack[top].frag.filter(|(fr, _)| next < fr.sections.len()) {
                Some((fr, f)) => {
                    stack[top].section = next;
                    stack[top].items = fr.sections.get(next).map_or(&[][..], |s| &s.items);
                    stack[top].at = 0;
                    out.push(Step {
                        depth: depth.saturating_sub(1),
                        ev: Ev::Section {
                            frag: fr,
                            f,
                            k: next,
                        },
                    });
                }
                None => {
                    stack.pop();
                }
            }
            continue;
        }
        let item = &stack[top].items[stack[top].at];
        stack[top].at += 1;
        match item {
            Item::Message(m) => out.push(Step {
                depth,
                ev: Ev::Message(m),
            }),
            Item::Note(n) => {
                out.push(Step {
                    depth,
                    ev: Ev::Note(n, notes),
                });
                notes += 1;
            }
            Item::Fragment(fr) => {
                let f = fragments;
                fragments += 1;
                out.push(Step {
                    depth,
                    ev: Ev::Section { frag: fr, f, k: 0 },
                });
                if stack.len() < MAX_FRAMES {
                    stack.push(Frame {
                        frag: Some((fr, f)),
                        section: 0,
                        items: fr.sections.first().map_or(&[][..], |s| &s.items),
                        at: 0,
                    });
                }
            }
            Item::Activate { .. } | Item::Deactivate { .. } => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;
    use crate::model::sequence::{FragmentKind, Section};
    use alloc::string::String;

    fn msg(index: u32) -> Item {
        Item::Message(Message {
            index,
            ..Message::default()
        })
    }

    fn frag(kind: FragmentKind, sections: Vec<Section>) -> Item {
        Item::Fragment(Fragment {
            kind,
            sections,
            span: Span::default(),
        })
    }

    fn section(items: Vec<Item>) -> Section {
        Section {
            label: String::new(),
            items,
            span: Span::default(),
        }
    }

    #[test]
    fn messages_come_out_in_source_order_with_their_depth() {
        let seq = Sequence {
            items: alloc::vec![
                msg(0),
                frag(
                    FragmentKind::Alt,
                    alloc::vec![
                        section(alloc::vec![
                            msg(1),
                            frag(
                                FragmentKind::Loop,
                                alloc::vec![section(alloc::vec![msg(2)])]
                            ),
                        ]),
                        section(alloc::vec![msg(3)]),
                    ],
                ),
                msg(4),
            ],
            messages: 5,
            ..Sequence::default()
        };
        let steps = walk(&seq);
        let got: Vec<(usize, u32)> = steps
            .iter()
            .filter_map(|s| match s.ev {
                Ev::Message(m) => Some((s.depth, m.index)),
                _ => None,
            })
            .collect();
        assert_eq!(got, alloc::vec![(0, 0), (1, 1), (2, 2), (1, 3), (0, 4)]);
        // The `alt` is fragment 0 and opens twice; the nested `loop` is fragment 1.
        let sections: Vec<(usize, usize, usize)> = steps
            .iter()
            .filter_map(|s| match s.ev {
                Ev::Section { f, k, .. } => Some((s.depth, f, k)),
                _ => None,
            })
            .collect();
        assert_eq!(sections, alloc::vec![(0, 0, 0), (1, 1, 0), (0, 0, 1)]);
    }

    #[test]
    fn nesting_past_the_frame_cap_stops_descending() {
        let mut inner = frag(FragmentKind::Opt, alloc::vec![section(alloc::vec![msg(0)])]);
        for _ in 0..MAX_FRAMES + 8 {
            inner = frag(FragmentKind::Opt, alloc::vec![section(alloc::vec![inner])]);
        }
        let seq = Sequence {
            items: alloc::vec![inner],
            messages: 1,
            ..Sequence::default()
        };
        let steps = walk(&seq);
        assert!(steps.iter().all(|s| s.depth < MAX_FRAMES));
    }
}
