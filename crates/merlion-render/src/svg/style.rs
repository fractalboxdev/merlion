//! The embedded `<style>` (specs/svg-output.md#embedded-style, #theming).
//!
//! Every rule starts with `#{id} ` or `:where(#{id} `, so nothing matches outside the
//! SVG. Rule order:
//!
//! 1. the text reset, verbatim from the spec, then the embedded font (`font: "embed"`);
//! 2. the zero-specificity reset of the per-element `--merlion-tone` / `--merlion-dash`
//!    (specs/svg-output.md#roles), the only custom properties the style declares;
//! 3. base rules, each reading its token with a literal fallback; strokes read the tone
//!    first and dash arrays the dash;
//! 4. `@supports (color: color-mix(in oklab, #000, #fff))` repeating the declarations
//!    whose fallback chain contains a mixed role, now falling back to `color-mix`, and the
//!    fills and text that mix the tone into their role;
//! 5. source styles (`classDef`, then node `style`, then `linkStyle`), last so that at
//!    equal specificity they win over 3 and 4.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::numfmt::push_num;
use crate::options::FontMode;

use super::theme::{Role, FONT_MONO, STROKE};

/// One base declaration's value.
enum Value {
    /// A colour role: `var(--merlion-{role}, …)`.
    Role(Role),
    /// A literal.
    Lit(String),
    /// A role replaced by the per-element tone when one is set:
    /// `var(--merlion-tone, <role chain>)`.
    Tone(Role),
    /// A role mixed with the per-element tone at `pct` percent. Outside `@supports` the
    /// role alone; inside, `color-mix(in oklab, var(--merlion-tone, R) pct%, R)`, which
    /// with the tone unset mixes R with itself and draws exactly R.
    ToneMix(Role, u8),
    /// A dash pattern replaced by the per-element dash when one is set:
    /// `var(--merlion-dash, <default>)`.
    Dash(&'static str),
}

impl Value {
    /// The value outside `@supports`.
    fn plain(&self) -> String {
        match self {
            Value::Role(r) | Value::ToneMix(r, _) => r.var(false),
            Value::Lit(s) => s.clone(),
            Value::Tone(r) => format!("var(--merlion-tone, {})", r.var(false)),
            Value::Dash(d) => format!("var(--merlion-dash, {})", d),
        }
    }

    /// The value inside `@supports (color: color-mix(…))`, when it differs from [`Value::plain`].
    fn mixed(&self) -> Option<String> {
        match self {
            Value::Role(r) if r.is_mixed() => Some(r.var(true)),
            Value::Tone(r) if r.is_mixed() => Some(format!("var(--merlion-tone, {})", r.var(true))),
            Value::ToneMix(r, pct) => {
                let chain = r.var(true);
                Some(format!(
                    "color-mix(in oklab, var(--merlion-tone, {}) {}%, {})",
                    chain, pct, chain
                ))
            }
            _ => None,
        }
    }
}

/// Mix ratios of the per-element tone (specs/svg-output.md#roles).
const TONE_FILL: u8 = 14;
const TONE_CLUSTER_FILL: u8 = 8;
const TONE_TEXT: u8 = 75;

/// The zero-specificity reset of the per-element tokens (specs/svg-output.md#roles): a
/// tone or dash set on an ancestor, on `:root` or on a cluster never reaches a member
/// element, while a role rule on the group itself wins over it. Markers are reset too:
/// they inherit from `<defs>`, not from the edge that references them.
fn push_token_reset(out: &mut String, id: &str) {
    out.push_str(&format!(
        ":where(#{0} .merlion-node, #{0} .merlion-edge, #{0} .merlion-cluster, #{0} marker)\
         {{--merlion-tone:initial;--merlion-dash:initial;}}",
        id
    ));
}

struct Rule {
    selector: &'static str,
    decls: Vec<(&'static str, Value)>,
}

fn stroke_var() -> String {
    let mut s = String::from("var(--merlion-stroke, ");
    push_num(&mut s, STROKE);
    s.push_str("px)");
    s
}

fn base_rules() -> Vec<Rule> {
    use Value::{Dash, Lit, Role as R, Tone, ToneMix};
    let lit = |s: &str| Lit(String::from(s));
    alloc::vec![
        Rule {
            selector: ".merlion-bg",
            decls: alloc::vec![("fill", R(Role::Bg))],
        },
        Rule {
            selector: ".merlion-label,#{id} .merlion-edge-text,#{id} .merlion-cluster-title",
            decls: alloc::vec![("text-anchor", lit("start")), ("white-space", lit("pre"))],
        },
        Rule {
            selector: ".merlion-label",
            decls: alloc::vec![("fill", ToneMix(Role::NodeText, TONE_TEXT))],
        },
        Rule {
            selector: ".merlion-edge-text,#{id} .merlion-cluster-title",
            decls: alloc::vec![("fill", ToneMix(Role::Fg, TONE_TEXT))],
        },
        Rule {
            selector: ".merlion-b",
            decls: alloc::vec![("font-weight", lit("600"))],
        },
        Rule {
            selector: ".merlion-i",
            decls: alloc::vec![("font-style", lit("italic"))],
        },
        Rule {
            selector: ".merlion-code",
            decls: alloc::vec![("font-family", lit(FONT_MONO))],
        },
        Rule {
            selector: ".merlion-node>.merlion-shape",
            decls: alloc::vec![
                ("fill", ToneMix(Role::NodeBg, TONE_FILL)),
                ("stroke", Tone(Role::NodeBorder)),
                ("stroke-width", Lit(stroke_var())),
                ("stroke-dasharray", Dash("none")),
            ],
        },
        Rule {
            selector: ".merlion-cluster>.merlion-cluster-box",
            decls: alloc::vec![
                ("fill", ToneMix(Role::ClusterBg, TONE_CLUSTER_FILL)),
                ("stroke", Tone(Role::ClusterBorder)),
                ("stroke-width", Lit(stroke_var())),
                ("stroke-dasharray", Dash("none")),
            ],
        },
        Rule {
            selector: ".merlion-edge>.merlion-edge-path",
            decls: alloc::vec![
                ("fill", lit("none")),
                ("stroke", Tone(Role::Edge)),
                ("stroke-width", Lit(stroke_var())),
                ("stroke-dasharray", Dash("none")),
            ],
        },
        Rule {
            selector: ".merlion-edge>.merlion-thick",
            decls: alloc::vec![("stroke-width", Lit(format!("calc({} * 2)", stroke_var())))],
        },
        Rule {
            selector: ".merlion-edge>.merlion-dotted",
            decls: alloc::vec![("stroke-dasharray", Dash("3 3"))],
        },
        Rule {
            selector: ".merlion-marker-fill",
            decls: alloc::vec![("fill", Tone(Role::Edge)), ("stroke", lit("none"))],
        },
        Rule {
            selector: ".merlion-marker-stroke",
            decls: alloc::vec![
                ("fill", lit("none")),
                ("stroke", Tone(Role::Edge)),
                ("stroke-width", lit("1.5px")),
            ],
        },
        Rule {
            selector: ".merlion-edge-label>.merlion-edge-label-bg",
            decls: alloc::vec![("fill", R(Role::EdgeLabelBg))],
        },
    ]
}

fn push_rule(out: &mut String, id: &str, selector: &str, body: &str) {
    out.push('#');
    out.push_str(id);
    out.push(' ');
    out.push_str(&selector.replace("{id}", id));
    out.push('{');
    out.push_str(body);
    out.push('}');
}

/// The text reset of specs/svg-output.md#embedded-style, verbatim apart from the
/// font stack (`system` mode) and the size (the measured `font_size`).
fn push_reset(out: &mut String, id: &str, font: FontMode, font_size: f64) {
    let family = super::theme::font_stack(font);
    let mut size = String::new();
    push_num(&mut size, font_size);
    out.push('#');
    out.push_str(id);
    out.push_str(&format!(
        " text {{ font-family: var(--merlion-font, {}); font-size: var(--merlion-font-size, {}px); \
         font-weight: 400; font-style: normal; font-stretch: normal; \
         font-kerning: normal; font-variant-ligatures: none; \
         font-feature-settings: \"calt\" 0, \"liga\" 0; \
         letter-spacing: 0; word-spacing: 0; text-transform: none; }}",
        family, size
    ));
}

/// Source-style rules, already serialised: `(selector without the id prefix, body)`.
pub struct SourceRule {
    pub selector: String,
    pub body: String,
}

/// The `merlion-detail` rule for detail lines drawn at `size` px: the fill reads
/// `--merlion-node-detail`; the size stays literal because it is the measured one.
fn detail_rule(size: f64) -> Rule {
    let mut px = String::new();
    push_num(&mut px, size);
    px.push_str("px");
    Rule {
        selector: ".merlion-detail",
        decls: alloc::vec![
            ("fill", Value::Role(Role::NodeDetail)),
            ("font-size", Value::Lit(px)),
        ],
    }
}

/// The full `<style>` text (unescaped: it contains no `<` or `&` by construction).
/// `detail_size` is the measured size of detail lines; the `merlion-detail` rule is
/// written only when some label has them.
pub fn build(
    id: &str,
    font: FontMode,
    font_size: f64,
    detail_size: Option<f64>,
    font_css: Option<&str>,
    source: &[SourceRule],
) -> String {
    let mut out = String::new();
    push_reset(&mut out, id, font, font_size);
    if let Some(css) = font_css {
        out.push_str(css);
    }
    push_token_reset(&mut out, id);
    let mut rules = base_rules();
    if let Some(size) = detail_size.filter(|s| s.is_finite() && *s > 0.0) {
        rules.push(detail_rule(size));
    }
    for r in &rules {
        let mut body = String::new();
        for (prop, v) in &r.decls {
            body.push_str(prop);
            body.push(':');
            body.push_str(&v.plain());
            body.push(';');
        }
        push_rule(&mut out, id, r.selector, &body);
    }
    out.push_str("@supports (color: color-mix(in oklab, #000, #fff)){");
    for r in &rules {
        let mut body = String::new();
        for (prop, v) in &r.decls {
            if let Some(m) = v.mixed() {
                body.push_str(prop);
                body.push(':');
                body.push_str(&m);
                body.push(';');
            }
        }
        if !body.is_empty() {
            push_rule(&mut out, id, r.selector, &body);
        }
    }
    out.push('}');
    for r in source {
        push_rule(&mut out, id, &r.selector, &r.body);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_rule_matches_the_spec() {
        let s = build("m1", FontMode::Link, 14.0, None, None, &[]);
        assert!(s.starts_with(
            "#m1 text { font-family: var(--merlion-font, Inter, ui-sans-serif, system-ui, sans-serif); \
             font-size: var(--merlion-font-size, 14px); font-weight: 400; font-style: normal; \
             font-stretch: normal; font-kerning: normal; font-variant-ligatures: none; \
             font-feature-settings: \"calt\" 0, \"liga\" 0; letter-spacing: 0; word-spacing: 0; \
             text-transform: none; }:where(#m1 "
        ), "{}", s);
    }

    #[test]
    fn node_rule_reads_tokens_with_literal_fallbacks() {
        let s = build("m1", FontMode::Link, 14.0, None, None, &[]);
        assert!(s.contains(
            "#m1 .merlion-node>.merlion-shape{fill:var(--merlion-node-bg, var(--merlion-surface, #f5f5f5));\
             stroke:var(--merlion-tone, var(--merlion-node-border, var(--merlion-border, #c8c9cb)));\
             stroke-width:var(--merlion-stroke, 1.25px);stroke-dasharray:var(--merlion-dash, none);}"
        ), "{}", s);
    }

    #[test]
    fn color_mix_appears_only_inside_supports() {
        let s = build("m1", FontMode::Link, 14.0, None, None, &[]);
        let at = s.find("@supports").unwrap();
        assert!(!s[..at].contains("color-mix"));
        let bg = "var(--merlion-node-bg, var(--merlion-surface, \
             color-mix(in oklab, var(--merlion-fg, #1f2328) 4%, var(--merlion-bg, #ffffff))))";
        assert!(s[at..].contains(&format!(
            "#m1 .merlion-node>.merlion-shape{{fill:color-mix(in oklab, var(--merlion-tone, {bg}) 14%, {bg});"
        )), "{}", s);
    }

    #[test]
    fn only_the_per_element_tokens_are_declared() {
        let s = build("m1", FontMode::Link, 14.0, None, None, &[]);
        assert_eq!(s.matches("{--").count() + s.matches(";--").count(), 2);
        assert!(s.contains(
            ":where(#m1 .merlion-node, #m1 .merlion-edge, #m1 .merlion-cluster, #m1 marker)\
             {--merlion-tone:initial;--merlion-dash:initial;}"
        ));
    }

    #[test]
    fn detail_rule_only_when_detail_lines_exist() {
        let none = build("m1", FontMode::Link, 14.0, None, None, &[]);
        assert!(!none.contains("merlion-detail"));
        let s = build("m1", FontMode::Link, 14.0, Some(11.2), None, &[]);
        assert!(
            s.contains(
                "#m1 .merlion-detail{fill:var(--merlion-node-detail, var(--merlion-muted, \
                 #7b7d81));font-size:11.2px;}"
            ),
            "{}",
            s
        );
        let at = s.find("@supports").unwrap();
        assert!(s[at..].contains(
            "#m1 .merlion-detail{fill:var(--merlion-node-detail, var(--merlion-muted, \
             color-mix(in oklab, var(--merlion-fg, #1f2328) 55%, var(--merlion-bg, #ffffff))));}"
        ));
        assert!(!s.contains("}.merlion-detail") && !s.contains("{.merlion-detail"));
    }

    #[test]
    fn system_font_mode_uses_the_system_stack() {
        let s = build("m1", FontMode::System, 13.5, None, None, &[]);
        assert!(s.contains("var(--merlion-font, system-ui, sans-serif)"));
        assert!(s.contains("var(--merlion-font-size, 13.5px)"));
    }

    #[test]
    fn source_rules_come_last_and_are_scoped() {
        let src = [SourceRule {
            selector: String::from(".merlion-c-hot>.merlion-shape"),
            body: String::from("fill:red;"),
        }];
        let s = build("m1", FontMode::Link, 14.0, None, None, &src);
        assert!(
            s.ends_with("}#m1 .merlion-c-hot>.merlion-shape{fill:red;}"),
            "{}",
            s
        );
    }
}
