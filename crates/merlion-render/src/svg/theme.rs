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
use alloc::vec::Vec;

use crate::color::{oklab_mix, Rgba8};

pub const BG: &str = "#ffffff";
pub const FG: &str = "#1f2328";
pub const ACCENT: &str = "#0969da";
/// Tones of the built-in roles `ok`, `warn` and `danger` / `failure` (specs/svg-output.md#built-in-roles).
pub const OK: &str = "#1a7f37";
pub const WARN: &str = "#9a6700";
pub const DANGER: &str = "#cf222e";
/// Tone of the built-in role `store`, which cylinders take automatically
/// (specs/svg-output.md#automatic-tones).
pub const STORE: &str = "#127a84";
/// `--merlion-series-1` … `-8` defaults: the categorical palette, and the tones of the
/// built-in cluster roles `series-1` … `series-8`.
pub const SERIES: [&str; 8] = [
    "#0969da", "#d4762c", "#2e8b57", "#b8408f", "#6f5bd6", "#1b98a6", "#b59a16", "#c4453d",
];
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    Store,
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
    /// Every role, in token-table order.
    pub const ALL: [Role; 19] = [
        Role::Bg,
        Role::Fg,
        Role::Muted,
        Role::Line,
        Role::Surface,
        Role::Border,
        Role::Accent,
        Role::Ok,
        Role::Warn,
        Role::Danger,
        Role::Store,
        Role::NodeBg,
        Role::NodeBorder,
        Role::NodeText,
        Role::NodeDetail,
        Role::Edge,
        Role::EdgeLabelBg,
        Role::ClusterBg,
        Role::ClusterBorder,
    ];

    /// The role whose token is `--merlion-{name}`.
    pub fn from_name(name: &str) -> Option<Role> {
        Role::ALL.iter().copied().find(|r| r.name() == name)
    }

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
            Role::Store => "store",
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
            Role::Store => Def::Literal(STORE),
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
        Table::default().is_mixed(self)
    }

    /// `var(--merlion-{role}, …)` with the fallback chain of the built-in table. With
    /// `mix`, mixed roles fall back to their `color-mix` expression; without, to their
    /// literal default.
    pub fn var(self, mix: bool) -> String {
        Table::default().var(self, mix)
    }
}

/// The literal table a render draws with (specs/svg-output.md#palette): the built-in
/// defaults, or a palette's values. A role the palette sets is a literal; an unset mixed
/// role is the `oklab_mix` of the table's foundations, and an alias follows its target.
/// With no palette every literal is the stored constant, so the output is unchanged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    set: Vec<(Role, Rgba8)>,
    /// `--merlion-series-{n}` values the palette sets, by `n`.
    series: Vec<(u8, Rgba8)>,
    /// `--merlion-stroke` in px, when the palette sets it.
    pub stroke: Option<f64>,
}

impl Table {
    /// The built-in defaults.
    pub const fn builtin() -> Self {
        Table {
            set: Vec::new(),
            series: Vec::new(),
            stroke: None,
        }
    }

    /// A table with `set` roles fixed to their colours.
    pub fn new(set: Vec<(Role, Rgba8)>, stroke: Option<f64>) -> Self {
        let stroke = stroke
            .filter(|s| s.is_finite())
            .map(|s| crate::math::clamp(s, 0.0, 20.0));
        Table {
            set,
            series: Vec::new(),
            stroke,
        }
    }

    /// The table with `series` entries (`n`, colour) replacing the series defaults.
    pub fn with_series(mut self, series: Vec<(u8, Rgba8)>) -> Self {
        self.series = series;
        self
    }

    /// The literal of `--merlion-series-{n}`, `n` in 1..=8.
    pub fn series_lit(&self, n: u8) -> String {
        if let Some((_, c)) = self.series.iter().rev().find(|(k, _)| *k == n) {
            return c.to_hex();
        }
        let i = usize::from(n.clamp(1, 8) - 1);
        String::from(SERIES.get(i).copied().unwrap_or(ACCENT))
    }

    /// `var(--merlion-series-{n}, <literal>)`.
    pub fn series_var(&self, n: u8) -> String {
        format!("var(--merlion-series-{}, {})", n, self.series_lit(n))
    }

    fn explicit(&self, r: Role) -> Option<Rgba8> {
        self.set
            .iter()
            .rev()
            .find(|(x, _)| *x == r)
            .map(|(_, c)| *c)
    }

    /// The stroke width in px.
    pub fn stroke_px(&self) -> f64 {
        self.stroke.unwrap_or(STROKE)
    }

    /// The colour `r` draws with.
    pub fn rgba(&self, r: Role) -> Rgba8 {
        self.rgba_depth(r, 0)
    }

    fn rgba_depth(&self, r: Role, depth: u8) -> Rgba8 {
        if let Some(c) = self.explicit(r) {
            return c;
        }
        let lit = |v: &str| Rgba8::from_hex(v).unwrap_or(Rgba8::new(0, 0, 0, 255));
        match r.def() {
            Def::Literal(v) => lit(v),
            Def::Mix(pct, v) => {
                if self.explicit(Role::Fg).is_some() || self.explicit(Role::Bg).is_some() {
                    oklab_mix(self.rgba(Role::Fg), self.rgba(Role::Bg), pct as f64)
                } else {
                    lit(v)
                }
            }
            Def::Alias(next) if depth < 4 => self.rgba_depth(next, depth + 1),
            Def::Alias(_) => lit(FG),
        }
    }

    /// The literal of `r`: the presentation attribute and the innermost fallback.
    pub fn lit(&self, r: Role) -> String {
        self.rgba(r).to_hex()
    }

    /// Whether `r` falls back to `color-mix` inside `@supports`.
    pub fn is_mixed(&self, r: Role) -> bool {
        if self.explicit(r).is_some() {
            return false;
        }
        match r.def() {
            Def::Mix(..) => true,
            Def::Alias(next) => self.explicit(next).is_none() && matches!(next.def(), Def::Mix(..)),
            Def::Literal(_) => false,
        }
    }

    /// `var(--merlion-{role}, …)`. A role the table sets falls back to its literal alone.
    pub fn var(&self, r: Role, mix: bool) -> String {
        let fallback = if self.explicit(r).is_some() {
            self.lit(r)
        } else {
            match r.def() {
                Def::Literal(_) => self.lit(r),
                Def::Mix(pct, _) => {
                    if mix {
                        format!(
                            "color-mix(in oklab, {} {}%, {})",
                            self.var(Role::Fg, false),
                            pct,
                            self.var(Role::Bg, false)
                        )
                    } else {
                        self.lit(r)
                    }
                }
                Def::Alias(next) => match next.def() {
                    Def::Alias(_) => format!("var(--merlion-{}, {})", next.name(), self.lit(next)),
                    _ => self.var(next, mix),
                },
            }
        };
        format!("var(--merlion-{}, {})", r.name(), fallback)
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
