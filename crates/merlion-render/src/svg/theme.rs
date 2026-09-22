//! Theming tokens (specs/svg-output.md#theming, specs/adr/0005-css-variable-theming.md).
//!
//! Two foundation tokens (`--merlion-bg`, `--merlion-fg`) drive the mixed roles through
//! `color-mix(in oklab, …)`. The literal default of every mixed role is the oklab mix of
//! the two foundation defaults, stored below as a constant; `tests/svg_theme.rs`
//! recomputes each mix and asserts the constants.
//!
//! No custom property is ever *declared* in the embedded style: a declaration on the
//! SVG root would shadow a value the host page sets on an ancestor. Each use site instead
//! reads the role through a fallback chain, e.g.
//! `var(--merlion-node-bg, var(--merlion-surface, #f5f5f5))`, and inside
//! `@supports (color: color-mix(…))` the innermost literal becomes the `color-mix` expression.

use alloc::format;
use alloc::string::String;

pub const BG: &str = "#ffffff";
pub const FG: &str = "#1f2328";
pub const ACCENT: &str = "#0969da";
/// Tones of the built-in roles `ok`, `warn` and `danger` / `failure` (specs/svg-output.md#built-in-roles).
pub const OK: &str = "#1a7f37";
pub const WARN: &str = "#9a6700";
pub const DANGER: &str = "#cf222e";
/// `color-mix(in oklab, fg 55%, bg)` of the defaults.
pub const MUTED: &str = "#7b7d81";
/// `color-mix(in oklab, fg 45%, bg)` of the defaults.
pub const LINE: &str = "#919497";
/// `color-mix(in oklab, fg 4%, bg)` of the defaults.
pub const SURFACE: &str = "#f5f5f5";
/// `color-mix(in oklab, fg 22%, bg)` of the defaults.
pub const BORDER: &str = "#c8c9cb";
/// `color-mix(in oklab, fg 2%, bg)` of the defaults.
pub const CLUSTER_BG: &str = "#fafafa";

pub const FONT: &str = "Inter, ui-sans-serif, system-ui, sans-serif";
/// Family name of the embedded subset in `font: "embed"` mode. It is Merlion-specific
/// because an inline SVG's `@font-face` is visible to the whole host document, and a
/// plain `Inter` face would replace the page's own Inter.
pub const EMBED_FONT_FAMILY: &str = "Merlion Inter";
/// Font stack for `font: "embed"`: the embedded family first, then the `link` stack.
pub const FONT_EMBED: &str = "Merlion Inter, Inter, ui-sans-serif, system-ui, sans-serif";
/// Font stack for `font: "system"` (specs/text-measurement.md#serving-the-font).
pub const FONT_SYSTEM: &str = "system-ui, sans-serif";
/// Family for `` `code` `` runs. Not a token: the token set is a public API (ADR-0005).
pub const FONT_MONO: &str = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
/// The font stack drawn for `mode`.
pub fn font_stack(mode: crate::options::FontMode) -> &'static str {
    match mode {
        crate::options::FontMode::Link => FONT,
        crate::options::FontMode::Embed => FONT_EMBED,
        crate::options::FontMode::System => FONT_SYSTEM,
    }
}
/// `--merlion-stroke` default, in px.
pub const STROKE: f64 = 1.25;

/// How a role gets its default value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Def {
    /// A literal colour.
    Literal(&'static str),
    /// `color-mix(in oklab, var(--merlion-fg) {pct}%, var(--merlion-bg))`, with its precomputed literal.
    Mix(u8, &'static str),
    /// Defaults to another role.
    Alias(Role),
}

/// The colour roles of the token table in specs/svg-output.md#theming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Bg,
    Fg,
    Muted,
    Line,
    Surface,
    Border,
    Accent,
    Ok,
    Warn,
    Danger,
    NodeBg,
    NodeBorder,
    NodeText,
    /// Detail lines of title + detail node labels.
    NodeDetail,
    Edge,
    EdgeLabelBg,
    ClusterBg,
    ClusterBorder,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Bg => "bg",
            Role::Fg => "fg",
            Role::Muted => "muted",
            Role::Line => "line",
            Role::Surface => "surface",
            Role::Border => "border",
            Role::Accent => "accent",
            Role::Ok => "ok",
            Role::Warn => "warn",
            Role::Danger => "danger",
            Role::NodeBg => "node-bg",
            Role::NodeBorder => "node-border",
            Role::NodeText => "node-text",
            Role::NodeDetail => "node-detail",
            Role::Edge => "edge",
            Role::EdgeLabelBg => "edge-label-bg",
            Role::ClusterBg => "cluster-bg",
            Role::ClusterBorder => "cluster-border",
        }
    }

    fn def(self) -> Def {
        match self {
            Role::Bg => Def::Literal(BG),
            Role::Fg => Def::Literal(FG),
            Role::Accent => Def::Literal(ACCENT),
            Role::Ok => Def::Literal(OK),
            Role::Warn => Def::Literal(WARN),
            Role::Danger => Def::Literal(DANGER),
            Role::Muted => Def::Mix(55, MUTED),
            Role::Line => Def::Mix(45, LINE),
            Role::Surface => Def::Mix(4, SURFACE),
            Role::Border => Def::Mix(22, BORDER),
            Role::ClusterBg => Def::Mix(2, CLUSTER_BG),
            Role::NodeBg => Def::Alias(Role::Surface),
            Role::NodeBorder => Def::Alias(Role::Border),
            Role::NodeText => Def::Alias(Role::Fg),
            Role::NodeDetail => Def::Alias(Role::Muted),
            Role::Edge => Def::Alias(Role::Line),
            Role::EdgeLabelBg => Def::Alias(Role::Bg),
            Role::ClusterBorder => Def::Alias(Role::Border),
        }
    }

    /// The literal default: the value of presentation attributes and the last fallback.
    pub fn default_value(self) -> &'static str {
        // Alias chains are at most one step long; the loop bound keeps it total anyway.
        let mut r = self;
        for _ in 0..4 {
            match r.def() {
                Def::Literal(v) | Def::Mix(_, v) => return v,
                Def::Alias(next) => r = next,
            }
        }
        FG
    }

    /// Whether the fallback chain contains a mixed role, i.e. differs inside `@supports`.
    pub fn is_mixed(self) -> bool {
        match self.def() {
            Def::Mix(..) => true,
            Def::Alias(next) => matches!(next.def(), Def::Mix(..)),
            Def::Literal(_) => false,
        }
    }

    /// `var(--merlion-{role}, …)` with the fallback chain. With `mix`, mixed roles fall
    /// back to their `color-mix` expression; without, to their literal default.
    pub fn var(self, mix: bool) -> String {
        let fallback = match self.def() {
            Def::Literal(v) => String::from(v),
            Def::Mix(pct, v) => {
                if mix {
                    format!(
                        "color-mix(in oklab, {} {}%, {})",
                        Role::Fg.var(false),
                        pct,
                        Role::Bg.var(false)
                    )
                } else {
                    String::from(v)
                }
            }
            Def::Alias(next) => next.var_shallow(mix),
        };
        format!("var(--merlion-{}, {})", self.name(), fallback)
    }

    /// Like [`Role::var`] for a role reached through an alias; aliases never chain further.
    fn var_shallow(self, mix: bool) -> String {
        match self.def() {
            Def::Alias(_) => format!("var(--merlion-{}, {})", self.name(), self.default_value()),
            _ => self.var(mix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foundation_roles_read_their_literal() {
        assert_eq!(Role::Bg.var(false), "var(--merlion-bg, #ffffff)");
        assert_eq!(Role::Fg.var(true), "var(--merlion-fg, #1f2328)");
    }

    #[test]
    fn aliased_role_chains_to_target() {
        assert_eq!(
            Role::NodeBg.var(false),
            "var(--merlion-node-bg, var(--merlion-surface, #f5f5f5))"
        );
        assert_eq!(
            Role::NodeText.var(true),
            "var(--merlion-node-text, var(--merlion-fg, #1f2328))"
        );
    }

    #[test]
    fn node_detail_defaults_to_muted() {
        assert_eq!(Role::NodeDetail.default_value(), MUTED);
        assert_eq!(
            Role::NodeDetail.var(false),
            "var(--merlion-node-detail, var(--merlion-muted, #7b7d81))"
        );
        assert!(Role::NodeDetail.is_mixed());
        assert!(Role::NodeDetail.var(true).contains("color-mix"));
    }

    #[test]
    fn mixed_role_uses_color_mix_only_when_asked() {
        assert_eq!(
            Role::Border.var(true),
            "var(--merlion-border, color-mix(in oklab, var(--merlion-fg, #1f2328) 22%, var(--merlion-bg, #ffffff)))"
        );
        assert!(!Role::Border.var(false).contains("color-mix"));
    }

    #[test]
    fn defaults_resolve_through_aliases() {
        assert_eq!(Role::NodeBg.default_value(), SURFACE);
        assert_eq!(Role::Edge.default_value(), LINE);
        assert_eq!(Role::EdgeLabelBg.default_value(), BG);
        assert_eq!(Role::ClusterBorder.default_value(), BORDER);
    }

    #[test]
    fn mixed_flags() {
        assert!(Role::NodeBg.is_mixed());
        assert!(Role::ClusterBg.is_mixed());
        assert!(!Role::NodeText.is_mixed());
        assert!(!Role::EdgeLabelBg.is_mixed());
        assert!(!Role::Accent.is_mixed());
    }
}
