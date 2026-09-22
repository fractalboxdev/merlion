//! Role rules (specs/svg-output.md#roles, #built-in-roles).
//!
//! A role rule restyles the elements carrying one role class at use sites: it reads
//! `var(--merlion-tone, <tone>)` and `var(--merlion-dash, <dash>)`, so a page or
//! stylesheet rule that sets the per-element token on the role wins, and it never
//! declares a custom property. Outside `@supports` the mixed fills carry the literal mix
//! of the tone's literal; inside, `color-mix` of the tone expression.
//!
//! Selectors keep the specificity that the precedence of the spec needs: shapes, paths,
//! markers and boxes use `.merlion-c-{name}>.merlion-shape` and the like (1,2,0), the
//! same as the base rules and `classDef` rules, so source order decides. Labels use a
//! type selector, `.merlion-c-{name}>text` on nodes and `.merlion-c-{name}>*>text` on
//! edges, whose label sits inside the label group (both 1,1,1): each beats the base
//! label rule (1,1,0), ties the `classDef` and `style` text rules
//! (`.merlion-c-{name} text`), and loses to them because they come later. The child
//! combinators keep an edge role off a node label with the same class and back. No role rule uses `:where()`, which librsvg
//! drops together with the whole rule.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::color::{oklab_mix, Rgba8};

use super::style::RoleRule;
use super::theme::{Role, Table};

/// Mix ratios of the per-element tone (specs/svg-output.md#roles).
pub const TONE_FILL: u8 = 14;
pub const TONE_CLUSTER_FILL: u8 = 8;
pub const TONE_TEXT: u8 = 75;

/// The dash of the dashed built-in roles.
pub const ROLE_DASH: &str = "6 4";

/// The element kind a role applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Node,
    Edge,
    Cluster,
}

/// One built-in role.
pub struct BuiltIn {
    pub name: &'static str,
    pub kind: Kind,
    pub tone: Option<Role>,
    pub dash: Option<&'static str>,
}

/// The built-in roles, in the order their rules are emitted.
pub const BUILT_IN: [BuiltIn; 8] = [
    BuiltIn {
        name: "accent",
        kind: Kind::Node,
        tone: Some(Role::Accent),
        dash: None,
    },
    BuiltIn {
        name: "ok",
        kind: Kind::Node,
        tone: Some(Role::Ok),
        dash: None,
    },
    BuiltIn {
        name: "warn",
        kind: Kind::Node,
        tone: Some(Role::Warn),
        dash: None,
    },
    BuiltIn {
        name: "danger",
        kind: Kind::Node,
        tone: Some(Role::Danger),
        dash: None,
    },
    BuiltIn {
        name: "muted",
        kind: Kind::Node,
        tone: Some(Role::Muted),
        dash: None,
    },
    BuiltIn {
        name: "group",
        kind: Kind::Cluster,
        tone: None,
        dash: Some(ROLE_DASH),
    },
    BuiltIn {
        name: "failure",
        kind: Kind::Edge,
        tone: Some(Role::Danger),
        dash: Some(ROLE_DASH),
    },
    BuiltIn {
        name: "async",
        kind: Kind::Edge,
        tone: None,
        dash: Some(ROLE_DASH),
    },
];

/// A tone as CSS and as a literal.
pub struct Tone {
    /// Outside `@supports`: `var(--merlion-tone, …)` with a literal-only chain.
    pub plain: String,
    /// Inside `@supports`, where a mixed role's chain ends in `color-mix`.
    pub mixed: String,
    /// The literal drawn by presentation attributes and CSS-less renderers.
    pub lit: String,
}

impl Tone {
    /// The tone of a built-in role: `var(--merlion-tone, var(--merlion-{role}, …))`.
    pub fn of_role(t: &Table, role: Role) -> Tone {
        Tone {
            plain: format!("var(--merlion-tone, {})", t.var(role, false)),
            mixed: format!("var(--merlion-tone, {})", t.var(role, true)),
            lit: t.lit(role),
        }
    }

    /// A palette tone: `var(--merlion-tone, <literal>)`.
    pub fn literal(hex: String) -> Tone {
        let plain = format!("var(--merlion-tone, {})", hex);
        Tone {
            mixed: plain.clone(),
            plain,
            lit: hex,
        }
    }
}

/// The literal of `color-mix(in oklab, tone pct%, base)`; `base` when either is not a
/// hex literal.
pub fn mix_lit(tone: &str, base: &str, pct: u8) -> String {
    match (Rgba8::from_hex(tone), Rgba8::from_hex(base)) {
        (Some(t), Some(b)) => oklab_mix(t, b, pct as f64).to_hex(),
        _ => String::from(base),
    }
}

fn dash_decl(out: &mut String, dash: Option<&str>) {
    if let Some(d) = dash {
        out.push_str("stroke-dasharray:var(--merlion-dash, ");
        out.push_str(d);
        out.push_str(");");
    }
}

/// `prop:{mixed};` when the tone's chain differs inside `@supports`.
fn mixed_stroke(out: &mut String, prop: &str, tone: &Tone) {
    if tone.mixed != tone.plain {
        out.push_str(&format!("{}:{};", prop, tone.mixed));
    }
}

fn mixed_fill(t: &Table, tone: &Tone, pct: u8, base: Role) -> String {
    format!(
        "fill:color-mix(in oklab, {} {}%, {});",
        tone.mixed,
        pct,
        t.var(base, true)
    )
}

fn rule(selector: String, plain: String, mixed: String) -> RoleRule {
    RoleRule {
        selector,
        plain,
        mixed,
    }
}

/// Rules for node role `name` (`merlion-c-{name}` on node groups).
pub fn node_rules(t: &Table, name: &str, tone: Option<&Tone>, dash: Option<&str>) -> Vec<RoleRule> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut mixed = String::new();
    if let Some(tn) = tone {
        plain.push_str(&format!(
            "fill:{};stroke:{};",
            mix_lit(&tn.lit, &t.lit(Role::NodeBg), TONE_FILL),
            tn.plain
        ));
        mixed.push_str(&mixed_fill(t, tn, TONE_FILL, Role::NodeBg));
        mixed_stroke(&mut mixed, "stroke", tn);
    }
    dash_decl(&mut plain, dash);
    out.push(rule(
        format!(".merlion-c-{}>.merlion-shape", name),
        plain,
        mixed,
    ));
    if let Some(tn) = tone {
        out.push(rule(
            format!(".merlion-c-{}>text", name),
            format!(
                "fill:{};",
                mix_lit(&tn.lit, &t.lit(Role::NodeText), TONE_TEXT)
            ),
            mixed_fill(t, tn, TONE_TEXT, Role::NodeText),
        ));
    }
    out.retain(|r| !r.plain.is_empty() || !r.mixed.is_empty());
    out
}

/// Rules for edge role `name` (`merlion-c-{name}` on edge groups and their markers).
pub fn edge_rules(t: &Table, name: &str, tone: Option<&Tone>, dash: Option<&str>) -> Vec<RoleRule> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut mixed = String::new();
    if let Some(tn) = tone {
        plain.push_str(&format!("stroke:{};", tn.plain));
        mixed_stroke(&mut mixed, "stroke", tn);
    }
    dash_decl(&mut plain, dash);
    out.push(rule(
        format!(".merlion-c-{}>.merlion-edge-path", name),
        plain,
        mixed,
    ));
    if let Some(tn) = tone {
        let mut m = String::new();
        mixed_stroke(&mut m, "fill", tn);
        out.push(rule(
            format!(".merlion-c-{}>.merlion-marker-fill", name),
            format!("fill:{};", tn.plain),
            m,
        ));
        let mut m = String::new();
        mixed_stroke(&mut m, "stroke", tn);
        out.push(rule(
            format!(".merlion-c-{}>.merlion-marker-stroke", name),
            format!("stroke:{};", tn.plain),
            m,
        ));
        out.push(rule(
            format!(".merlion-c-{}>*>text", name),
            format!("fill:{};", mix_lit(&tn.lit, &t.lit(Role::Fg), TONE_TEXT)),
            mixed_fill(t, tn, TONE_TEXT, Role::Fg),
        ));
    }
    out.retain(|r| !r.plain.is_empty() || !r.mixed.is_empty());
    out
}

/// Rules for cluster role `name` (`merlion-cc-{name}` on cluster groups).
pub fn cluster_rules(
    t: &Table,
    name: &str,
    tone: Option<&Tone>,
    dash: Option<&str>,
) -> Vec<RoleRule> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut mixed = String::new();
    if let Some(tn) = tone {
        plain.push_str(&format!(
            "fill:{};stroke:{};",
            mix_lit(&tn.lit, &t.lit(Role::ClusterBg), TONE_CLUSTER_FILL),
            tn.plain
        ));
        mixed.push_str(&mixed_fill(t, tn, TONE_CLUSTER_FILL, Role::ClusterBg));
        mixed_stroke(&mut mixed, "stroke", tn);
    }
    dash_decl(&mut plain, dash);
    out.push(rule(
        format!(".merlion-cc-{}>.merlion-cluster-box", name),
        plain,
        mixed,
    ));
    if let Some(tn) = tone {
        out.push(rule(
            format!(".merlion-cc-{}>text", name),
            format!("fill:{};", mix_lit(&tn.lit, &t.lit(Role::Fg), TONE_TEXT)),
            mixed_fill(t, tn, TONE_TEXT, Role::Fg),
        ));
    }
    out.retain(|r| !r.plain.is_empty() || !r.mixed.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_name_is_a_valid_class_name_and_unique_per_kind() {
        for (i, b) in BUILT_IN.iter().enumerate() {
            assert!(super::super::color::is_valid_class_name(b.name));
            assert!(BUILT_IN[..i]
                .iter()
                .all(|o| !(o.name == b.name && o.kind == b.kind)));
        }
        let has = |n: &str, k: Kind| BUILT_IN.iter().any(|b| b.name == n && b.kind == k);
        assert!(has("danger", Kind::Node) && !has("danger", Kind::Edge));
        assert!(has("failure", Kind::Edge) && has("group", Kind::Cluster));
    }

    #[test]
    fn danger_node_rules() {
        let tb = Table::default();
        let t = Tone::of_role(&tb, Role::Danger);
        let r = node_rules(&tb, "danger", Some(&t), None);
        assert_eq!(r[0].selector, ".merlion-c-danger>.merlion-shape");
        assert_eq!(
            r[0].plain,
            format!(
                "fill:{};stroke:var(--merlion-tone, var(--merlion-danger, #cf222e));",
                mix_lit("#cf222e", "#f5f5f5", 14)
            )
        );
        assert!(r[0].mixed.starts_with(
            "fill:color-mix(in oklab, var(--merlion-tone, var(--merlion-danger, #cf222e)) 14%, var(--merlion-node-bg"
        ));
        assert!(!r[0].mixed.contains("stroke:"));
        assert_eq!(r[1].selector, ".merlion-c-danger>text");
    }

    #[test]
    fn muted_tone_differs_inside_supports() {
        let tb = Table::default();
        let t = Tone::of_role(&tb, Role::Muted);
        assert_ne!(t.plain, t.mixed);
        let r = node_rules(&tb, "muted", Some(&t), None);
        assert!(r[0]
            .mixed
            .contains("stroke:var(--merlion-tone, var(--merlion-muted, color-mix("));
    }

    #[test]
    fn dash_only_roles_have_no_colour() {
        let tb = Table::default();
        let r = edge_rules(&tb, "async", None, Some(ROLE_DASH));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].plain, "stroke-dasharray:var(--merlion-dash, 6 4);");
        assert!(r[0].mixed.is_empty());
        let r = cluster_rules(&tb, "group", None, Some(ROLE_DASH));
        assert_eq!(r[0].selector, ".merlion-cc-group>.merlion-cluster-box");
        assert_eq!(r.len(), 1);
    }
}
