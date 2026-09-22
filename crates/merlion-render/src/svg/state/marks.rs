//! The marks a state diagram adds to the flowchart's: the ink of the pseudo-states, the
//! note box and its connector, and the concurrency divider
//! (specs/state.md#theme-tokens).
//!
//! Each one reuses an existing theme token, so the rules join the embedded style as
//! ordinary role rules ([`RoleRule`]) and no token, palette entry or stylesheet key is
//! introduced. They are emitted after the built-in and palette role rules and before the
//! source styles, which puts an automatic tone below the ink of a start, end, fork or
//! join mark and a written `classDef` above it.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::numfmt::push_num;

use super::super::style::RoleRule;
use super::super::theme::{Role, Table};

/// The dash of the note connector and the region divider.
pub const DASH: &str = "4 4";

/// One declaration's value.
enum V {
    /// A colour role: `var(--merlion-{role}, …)`, with the `color-mix` chain inside
    /// `@supports` when the role's fallback chain mixes.
    Role(Role),
    Lit(&'static str),
    /// `var(--merlion-stroke, {n}px)`, the theme's stroke width.
    Width,
}

impl V {
    fn plain(&self, t: &Table) -> String {
        match self {
            V::Role(r) => t.var(*r, false),
            V::Lit(s) => String::from(*s),
            V::Width => stroke_var(t),
        }
    }

    /// The value inside `@supports`, when it differs from [`V::plain`].
    fn mixed(&self, t: &Table) -> Option<String> {
        match self {
            V::Role(r) if t.is_mixed(*r) => Some(t.var(*r, true)),
            _ => None,
        }
    }
}

fn stroke_var(t: &Table) -> String {
    let mut s = String::from("var(--merlion-stroke, ");
    push_num(&mut s, t.stroke_px());
    s.push_str("px)");
    s
}

fn rule(t: &Table, selector: &str, decls: &[(&str, V)]) -> RoleRule {
    let mut plain = String::new();
    let mut mixed = String::new();
    for (prop, v) in decls {
        let _ = write!(plain, "{}:{};", prop, v.plain(t));
        if let Some(m) = v.mixed(t) {
            let _ = write!(mixed, "{}:{};", prop, m);
        }
    }
    RoleRule {
        selector: String::from(selector),
        plain,
        mixed,
    }
}

/// The rules of a state diagram's own marks. `#{id} ` inside a selector list is replaced
/// by the scoping prefix when the style is written, exactly as for a base rule.
pub fn state_rules(t: &Table) -> Vec<RoleRule> {
    alloc::vec![
        // The pseudo-states draw in ink: `--merlion-fg` through the element class, never
        // through a role, so no tone tints them and they stay solid under every theme.
        rule(
            t,
            ".merlion-state-start>.merlion-shape,\
             #{id} .merlion-state-end>.merlion-shape,\
             #{id} .merlion-state-bar>.merlion-shape",
            &[
                ("fill", V::Role(Role::Fg)),
                ("stroke", V::Role(Role::NodeBorder)),
                ("stroke-width", V::Width),
            ],
        ),
        rule(
            t,
            ".merlion-note>.merlion-note-box",
            &[
                ("fill", V::Role(Role::Surface)),
                ("stroke", V::Role(Role::Border)),
                ("stroke-width", V::Width),
            ],
        ),
        rule(
            t,
            ".merlion-note>.merlion-note-link",
            &[
                ("fill", V::Lit("none")),
                ("stroke", V::Role(Role::Line)),
                ("stroke-width", V::Width),
                ("stroke-dasharray", V::Lit(DASH)),
            ],
        ),
        rule(
            t,
            ".merlion-region>.merlion-region-divider",
            &[
                ("fill", V::Lit("none")),
                ("stroke", V::Role(Role::ClusterBorder)),
                ("stroke-width", V::Width),
                ("stroke-dasharray", V::Lit(DASH)),
            ],
        ),
    ]
}

fn finite(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// `M x1 y1L x2 y2`: the straight segment a divider and a note connector draw.
pub fn line_d(a: (f64, f64), b: (f64, f64)) -> String {
    let mut d = String::from("M");
    push_num(&mut d, finite(a.0));
    d.push(' ');
    push_num(&mut d, finite(a.1));
    d.push('L');
    push_num(&mut d, finite(b.0));
    d.push(' ');
    push_num(&mut d, finite(b.1));
    d
}

/// The connector from the anchor on a state's rect to the nearest point of the note box
/// `(x, y, w, h)`: a straight dashed segment, so it crosses nothing between them.
pub fn link_d(anchor: (f64, f64), x: f64, y: f64, w: f64, h: f64) -> String {
    use crate::math::{clamp, max};
    let (ax, ay) = (finite(anchor.0), finite(anchor.1));
    let (x, y) = (finite(x), finite(y));
    let (w, h) = (max(finite(w), 0.0), max(finite(h), 0.0));
    line_d((ax, ay), (clamp(ax, x, x + w), clamp(ay, y, y + h)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn every_rule_is_scoped_and_declares_no_custom_property() {
        let t = Table::default();
        for r in state_rules(&t) {
            assert!(r.selector.starts_with(".merlion-"), "{}", r.selector);
            for part in r.selector.split(',').skip(1) {
                assert!(part.starts_with("#{id} "), "{part}");
            }
            assert!(!r.plain.is_empty());
            assert!(!r.plain.contains("{--"), "{}", r.plain);
        }
    }

    #[test]
    fn the_ink_marks_read_the_foreground_token() {
        let t = Table::default();
        let first = state_rules(&t).remove(0);
        assert!(
            first.plain.starts_with("fill:var(--merlion-fg,"),
            "{}",
            first.plain
        );
    }

    #[test]
    fn a_connector_meets_the_nearest_edge_of_the_box() {
        // The anchor sits left of the box, so the segment is horizontal.
        assert_eq!(
            link_d((100.0, 90.0), 160.0, 70.0, 140.0, 40.0),
            "M100 90L160 90"
        );
        // Above it, the segment is vertical.
        assert_eq!(
            link_d((200.0, 10.0), 160.0, 70.0, 140.0, 40.0),
            "M200 10L200 70"
        );
        // A non-finite anchor still writes a valid path.
        assert_eq!(
            link_d((f64::NAN, f64::INFINITY), 0.0, 0.0, -1.0, -1.0).to_string(),
            "M0 0L0 0"
        );
    }
}
