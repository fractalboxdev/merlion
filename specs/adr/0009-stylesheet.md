# ADR-0009: One compiled stylesheet for web and standalone diagrams

- **Status:** Proposed
- **Date:** 2026-09-22

## Context

Mermaid source can name roles (`class q store`, `q:::store`), but the only way to colour a role today is a `classDef` literal, which stays fixed in every theme (`I030`). A page can already restyle a classed node by setting tokens on `.merlion-c-<name>`, because a custom property on the node group inherits into its shape and text. Nothing consumes a per-node tone, and standalone SVGs (GitHub image embeds, librsvg, files) can only show the built-in light defaults ([ADR-0005](0005-css-variable-theming.md)).

Constraints the design must meet, each measured in Chrome (Playwright 1.63) and rsvg-convert 2.62.2:

- **Stylesheet authors are untrusted.** A docs repository's `diagram.css` is edited by pull-request authors whose content CI renders; a WASM chat host may let a model write it. Raw CSS linked into a page restyles any host element, exfiltrates attribute values through attribute-prefix selectors plus `url()`, and fetches through `@import`, `image-set()`, `url()` paint servers (directly or through a custom property) and `font-family` naming a host `@font-face`.
- **An inline SVG `<style>` is foreign content, not RAWTEXT.** A `<` in its text ends the style and opens HTML (`</style><img onerror>` executed). The core's only protection is that no free text reaches the style, enforced by a test (`assert_safe` in `tests/svg_output.rs`), not by a runtime guard.
- **Embedded rules beat presentation attributes.** Every embedded rule has id specificity, so a value baked only into `fill="…"` never shows in a browser. librsvg draws the fallback of a single-level `var()` and drops a nested `var()` declaration, drawing the attribute instead; which value shows depends on nesting depth.
- **A declared token shadows the host.** A custom property declared on the SVG (even `:where(#id){…}`) beats one inherited from `[data-theme]` on an ancestor, so the diagram stops following the page theme. The module comment of `svg/theme.rs` forbids declarations for this reason.
- **Ordinary properties in page CSS do not reach Merlion's elements.** `.merlion-c-x{fill;stroke;stroke-width;font-weight}` and `.merlion-c-x text{…}` lose to `#id …` rules; only custom properties and inherited properties no embedded rule sets (`stroke-dasharray`) get through.
- **An unguarded `var(--merlion-tone)` inside `color-mix()` paints untoned nodes black**: the declaration is invalid at computed-value time and `fill` resets to its initial value. `--merlion-tone` inherits, so a tone set on a wrapper or cluster tints every untoned node beneath it.
- **Precedence is per property and per element**, decided by specificity then source order. A `classDef` literal fill on the shape and a stylesheet tone on the label combine into a pink box with teal text. `:root` and `[data-theme]` on one element tie at (0,1,0) and resolve by order.
- **The core computes no colour mixes.** Mixed-role defaults are constants in `svg/theme.rs`; oklab code exists only in `tests/svg_theme.rs` using `powf`/`cbrt`, which `no_std` lacks.
- **Clusters and edges have no role hook.** `class grp dashed` without a `classDef` emits no cluster class; the parser keeps edge ids only to recognise a later `e1@{…}` block (`skip_edge_id` in `parse/flowchart.rs`) and attaches none to the edge, so `class e1 failure` is dropped silently. mermaid 12 applies edge classes.
- **mermaid compatibility.** A role without a `classDef` renders in mermaid's default style; mermaid 12 rejects `var(` inside `classDef` with a parse error. A diagram that must look right on GitHub and in mermaid therefore keeps `classDef` literals in its source, and only Merlion's output can make them themeable.
- **No shared vocabulary.** Every site invents its own role names, so a diagram written with `class x danger` has no colour anywhere until someone writes a stylesheet for it.

## Decision

Authors write one stylesheet in a CSS subset. Merlion compiles it into a typed `Stylesheet`; browsers only ever see Merlion's re-serialisation of that model, and standalone renders bake it into the SVG's literals. The stylesheet sets tokens only; Merlion's own embedded rules turn tokens into properties.

### Tokens the stylesheet may set

A closed list, each with a typed grammar. Any other property, including any other `--merlion-*` or unprefixed custom property used as a value, is dropped with `W018`.

| Token | Grammar |
|---|---|
| Colour tokens in the [svg-output.md](../svg-output.md#theming) table (`--merlion-bg` … `--merlion-series-8`) | The source-style colour grammar, plus `oklab()` and `oklch()` whose result lies inside sRGB; out-of-gamut values get `W018` |
| `--merlion-stroke` | A number from 0 to 20, optionally followed by `px` |
| `--merlion-tone` | A colour, as above |
| `--merlion-c-<name>-fill`, `--merlion-c-<name>-stroke`, `--merlion-c-<name>-color` | A colour, as above; `<name>` follows the `classDef` grammar ([Overridable `classDef` colours](#overridable-classdef-colours)) |
| `--merlion-dash` | The `stroke-dasharray` grammar, or `none` |
| `--<ident>` (private) | A colour; allowed only in theme blocks; used only through `var()` in another declaration; resolved at compile time and never emitted |

`--merlion-font` and `--merlion-font-size` are excluded: they are measurement inputs ([text-measurement.md](../text-measurement.md)), and a free family name can trigger a font fetch. The stylesheet never affects measurement, layout or the layout hint.

A value is a literal or `var(--<name>)` / `var(--<name>, <literal>)` naming a token or private token declared in the same file. The compiler resolves references memoised, to a depth of 8, rejecting cycles and undefined names with `W019`. `color-mix()`, `calc()`, `env()`, `url()`, quoted strings, `!important` and CSS escapes are rejected with `W018`.

### Selectors

| Selector | Meaning |
|---|---|
| `:root` | Base theme |
| `[data-theme="<t>"]`, `<t>` matching `[a-z][a-z0-9-]{0,31}` | Named theme `<t>` |
| `:root:not([data-theme])` inside `@media (prefers-color-scheme: dark)` | The automatic dark theme |
| `.merlion-c-<name>` | Nodes and edges carrying role `<name>` |
| `.merlion-cc-<name>` | Clusters carrying role `<name>` |
| `[data-theme="<t>"] .merlion-c-<name>`, `[data-theme="<t>"] .merlion-cc-<name>` | Role under theme `<t>` |

`<name>` follows the `classDef` grammar `[A-Za-z_][A-Za-z0-9_-]{0,63}`. Selector lists of these forms are accepted. Selectors are ASCII. The only at-rule is `@media (prefers-color-scheme: dark)`; nesting, `@import`, `@font-face`, `@layer`, `@container`, `@namespace`, `@supports` and every other at-rule are dropped with `W017`. A rule outside the subset that declares a `--merlion-*` token gets `W017`; a rule declaring no such token (the viewer rules in `merlion-themes.css`) is dropped with one `I032` summary count per stylesheet.

### Limits

| Limit | Value | Exceeded |
|---|---|---|
| Stylesheet size | 64 KiB, checked from file metadata before reading | `E013` |
| Rules / declarations per rule / selectors per list | 512 / 32 / 8 | `E013` |
| Theme names / role selectors | 16 / 256 | `E013` |
| Block depth | 2 (explicit counter) | `E013` |
| `var()` resolution depth | 8 | `W019` |
| Diagnostics | 100, then one summary count | — |
| Embedded style added per diagram | 16 KiB; only roles the diagram uses are embedded | `W017` for the roles left out |

Parsing and resolution are linear in the input and run once per invocation, outside any render's fuel counter; the 64 KiB cap bounds their cost.

### Precedence

Per property, per element:

1. Mermaid `style` on the node, for the properties it sets; its colours are literals.
2. `classDef` colours, for the properties they set. Each reads its `--merlion-c-<name>-*` token, so a stylesheet or page that sets the token replaces the literal; unset, the literal shows. `I033` reports a source literal (a `style` colour, or a `classDef` colour whose token the stylesheet leaves unset) masking a stylesheet tone on the same element.
3. Stylesheet role rules, by the CSS cascade: specificity, then source order. Every subset selector has a statically known specificity, so the compiler resolves exactly what a browser resolves.
4. Built-in roles ([Built-in roles](#built-in-roles)).
5. Tokens not set by a role rule come from the active theme block, then `:root`, then the built-in defaults.

### Embedded rules (every render)

Independent of any stylesheet, every SVG consumes the per-element tokens:

- Inside the existing `@supports (color: color-mix(in oklab, #000, #fff))` block, the node shape reads `fill: color-mix(in oklab, var(--merlion-tone, <node-bg chain>) 14%, <node-bg chain>)`, `stroke: var(--merlion-tone, <node-border chain>)`, and the label `fill: color-mix(in oklab, var(--merlion-tone, <node-text chain>) 75%, <node-text chain>)`. An untoned node mixes a colour with itself and draws exactly the default. Edges read the tone for path stroke, label text and marker; clusters for box stroke, a tinted box fill and title.
- `stroke-dasharray: var(--merlion-dash, <the element's default>)` on node shapes, edge paths and cluster boxes.
- `:where(#{id} .merlion-node, #{id} .merlion-edge, #{id} .merlion-cluster){--merlion-tone:initial;--merlion-dash:initial}` stops ancestors, `:root` and clusters from tinting members. A role rule (0,1,0) beats the zero-specificity reset on the group itself, and its shape and text inherit from the group. These two per-element tokens are the only custom properties the embedded style declares; theme tokens are never declared.
- Clusters carry `merlion-cc-<name>` for every valid name in `class`/`:::`, with or without a `classDef`. Edge ids (`e1@-->`) are kept; `class e1 failure` adds `merlion-c-failure` to the edge group. Each edge role used gets its own marker in `<defs>`, carrying the role class, because markers don't inherit from the referencing edge.

### Overridable `classDef` colours

A `classDef` colour is emitted through a per-class token instead of a bare literal: `fill: var(--merlion-c-<name>-fill, <literal>)`, and likewise `-stroke` for `stroke` and `-color` for `color` (the label fill). The presentation attribute and the fallback carry the source literal, so an unthemed render and a CSS-less renderer show the author's colour, as mermaid and GitHub do from the same source. A stylesheet re-themes the class by setting the token in a theme block or on the role (`[data-theme="dark"] { --merlion-c-store-fill: #1d3a2f; }`); a page may set it directly. Baking resolves the token into the literal. The embedded style reads these tokens and never declares them. `style` and `linkStyle` colours have no stable name to key a token on and stay literal. `I030` reports a source colour as fixed unless a stylesheet or page sets it.

### Built-in roles

Eight roles work with no stylesheet. Their rules are emitted only for roles the diagram uses, before `classDef` rules, and each tone reads a theme token with a literal fallback, so a toned role follows the page theme when inlined and shows its light default standalone:

| Role | Applies to | Tone | Dash |
|---|---|---|---|
| `accent` | Nodes | `--merlion-accent` | — |
| `ok` | Nodes | `--merlion-ok` | — |
| `warn` | Nodes | `--merlion-warn` | — |
| `danger` | Nodes | `--merlion-danger` | — |
| `muted` | Nodes | `--merlion-muted` | — |
| `group` | Clusters (`merlion-cc-group`) | — | `6 4` on the box |
| `failure` | Edges | `--merlion-danger` | `6 4` |
| `async` | Edges | — | `6 4` |

A built-in role's use-site rules read `var(--merlion-tone, var(--merlion-<tone>, <literal>))` and `var(--merlion-dash, 6 4)`. A stylesheet or page rule on the role (`.merlion-c-danger { --merlion-tone: … }`) beats the zero-specificity reset and overrides the built-in tone; setting `--merlion-danger` on a theme retunes every `danger` and `failure` element. `merlion-themes.css` defines `--merlion-ok`, `--merlion-warn` and `--merlion-danger` in every theme. A role name the table does not list has no built-in style.

### Web path

`compileStylesheet` re-serialises the typed model to page CSS: literal values only, selectors rewritten to the fixed shapes `:root`, `[data-theme="<t>"]`, `:root:not([data-theme])` inside the one media query, `.merlion .merlion-c-<name>`, `.merlion .merlion-cc-<name>`, `[data-theme="<t>"] .merlion .merlion-c-<name>` and `[data-theme="<t>"] .merlion .merlion-cc-<name>`. The `.merlion ` prefix adds (0,1,0) to every role selector, so specificity order between rules is the source's. Compilation is a fixed point: compiling its own output yields the same bytes. A page never links an uncompiled Merlion stylesheet; `merlion-themes.css` is Merlion's own file and is linked as shipped. Inline SVGs on the web are rendered with no palette, so the page's cascade themes them and theme switching never re-renders.

### Standalone path

`Stylesheet::palette(theme, auto_dark)` resolves one `Palette`: a literal per role (mixed roles recomputed from the resolved foundations), per-role tone and dash literals, and optionally a dark table. `RenderOptions.palette: Option<Palette>` replaces the literal table that `Role::default_value` returns: the same literal fills the presentation attribute, the innermost `var()` fallback and the fallback outside `@supports`. Browsers with no host tokens, librsvg on either nesting path, and CSS-less renderers all draw the baked value; host tokens set on an ancestor still win when the SVG is inlined. Baking never declares a custom property.

With `auto_dark`, the embedded style adds one `@media (prefers-color-scheme: dark)` block repeating the use-site rules under `#{id}:not(:is([data-theme="light"] *))` with the dark literals as fallbacks. In Chrome the query inside an `<img>`-embedded SVG follows the embedding page's used `color-scheme`. Auto-dark is for `<img>` and file embeds; inline hosts never request it.

The core adds `color::oklab_mix` in software: sRGB→linear through a 256-entry `const` table, linear→sRGB by binary search over the same table, `cbrt` by Newton iteration from a fixed seed using `+ − × ÷` only. `oklch()` input uses the core's software `sin`/`cos`.

### API surface

| Surface | Addition |
|---|---|
| Core | `stylesheet::compile(&str, &StylesheetLimits) -> (Option<Stylesheet>, Diagnostics)`; `Stylesheet::to_css() -> String`; `Stylesheet::palette(theme: Option<&str>, auto_dark: Option<&str>) -> (Palette, Diagnostics)`; `RenderOptions.palette: Option<Palette>` |
| CLI | `merlion css <in.css> [-o <out.css>] [--strict]` compiles; `merlion render --css <file> [--theme <t>] [--auto-dark <t>]` bakes. `--theme` defaults to `:root`; an unknown name is a usage error (exit 2). `--theme` never reads the media block |
| WASM | `compileStylesheet(css, { theme?, autoDark?, strict? }) -> { css, palette, diagnostics }`; `render(source, { palette })`, with `palette` validated by the glue and the core against the same grammars; an unknown option key throws `TypeError` |
| rehype / Astro | Option `stylesheet: "<path>"`: read under the CLI's file-handling rules, compiled once per build, exposed as `file.data.merlion.css`; the Astro integration writes it as an asset and links it on pages with a diagram. Inline renders never receive a palette |

A stylesheet path comes only from the caller: front matter and `%%{init}%%` never name one.

### Ids and determinism

When `palette` is `Some`, a digest of its canonical serialisation joins `fnv1a64_parts` in `diagram_id`; `None` hashes nothing, so every existing id is unchanged. The layout hint excludes the palette. The layout hint and every coordinate are byte-identical with and without a palette.

### Security rules

- `style::build` drops, at run time, any rule whose serialised text contains `<`, `&`, `\` or `@` outside the fixed at-rules (`@supports`, `@media (prefers-color-scheme: dark)`, the embedded `@font-face`).
- The embedded-style scoping check accepts exactly three prefixes: `#{id} `, `:where(#{id} ` and `#{id}:not(:is([data-theme="light"] *)) `.
- Role classes carry presentation only, never provenance or trust.

### Parity gate

For each corpus diagram, each theme of the gate's stylesheet (compiled with `--strict`, so no dropped declaration passes) and each target, the benchmark compares per element:

1. the plain SVG inlined in Chrome with the compiled CSS linked and `data-theme=<t>` (the reference);
2. the baked SVG in Chrome with no host CSS (fallback literals);
3. the baked SVG in Chrome with its `<style>` removed (presentation attributes);
4. the baked SVG through rsvg-convert, sampled as pixels inside each shape.

Chrome's computed colours are converted to 8-bit sRGB through a canvas `fillStyle` round trip; the gate fails above ±1 per channel. Pixel sampling catches group opacity that computed style on a `<path>` misses. `merlion-themes.css` bakes every named theme with no warning.

## Options considered

| Option | Buys | Costs |
|---|---|---|
| **CSS subset compiled to a typed model; page links the compiled CSS; standalone bakes a palette into literals** | One familiar file; browsers never see untrusted CSS; web and standalone are two emitters of one model, so parity holds by construction; reads `merlion-themes.css` | A CSS tokenizer and resolver in the core with its own fuzz target; a compile step in every web build; authors cannot use CSS outside the subset |
| Raw stylesheet linked on the web; CLI parses a subset and bakes attributes | No build step for the web | Untrusted CSS reaches the page unrestricted; baked attributes never show in browsers or reliably in librsvg; ordinary properties diverge between paths; the subset cannot read `merlion-themes.css` |
| Token file (TOML/JSON) that generates the page CSS and the palette | No selector parsing or browser-parser differential; reuses the bounded JSON parser | Not a CSS file: sites keep their colours in two formats; `merlion-themes.css` must be regenerated from it |
| Built-in themes plus `--tone store=#…` flags, no parser | No parser, no fuzz target | Custom brand themes need flags or a JSON palette; the page's own CSS cannot drive standalone output |
| Role palettes in front matter | No new input channel or file I/O | The palette repeats in every diagram; front matter grows a colour grammar |
| CSS parser in `merlion-cli` only; core takes a typed palette | Smaller core, fuzz surface where the file is read | rehype, Astro and WASM file-writing tools need a second parser to compile or bake |

Within the chosen design:

| Option | Buys | Costs |
|---|---|---|
| **`classDef` colours read `--merlion-c-<name>-*` with the literal as fallback** | A diagram keeps literals for mermaid and GitHub and still follows Merlion themes; the stylesheet re-themes it without touching the source | Every diagram with a `classDef` colour changes bytes once; one more token family in the stylesheet grammar and the fuzz checks |
| `classDef` literals stay fixed | Source colour means the same in every theme; `I030` unchanged | Diagrams shared with mermaid and GitHub can never follow a Merlion theme, so authors choose between compatibility and theming |
| **Built-in roles in the core and `merlion-themes.css`** | `class x danger` is coloured with zero configuration, in every theme and standalone; one vocabulary across sites | Eight role names, three tone tokens and one dash pattern become public API |
| Roles only through a stylesheet | Nothing to name or maintain in the core | A role without a stylesheet renders in the default style, so zero-config sites get no role colours |

## Criteria

Host safety with an untrusted stylesheet, web/standalone parity, theme switching without re-render ([ADR-0005](0005-css-variable-theming.md)), authoring cost, and core size. Safety and parity rule out the original design: raw linked CSS fails safety, and two independent consumers of one file fail parity. Among the compiled designs, authoring cost decided it: the CSS subset lets a site keep one stylesheet and parses the shipped `merlion-themes.css`; the token file loses only on that criterion. Authoring cost also decides the two choices within the design: overridable `classDef` colours let one source serve mermaid, GitHub and themed Merlion output, and built-in roles make a role useful before anyone writes a stylesheet. The parser lives in the core because the rehype and Astro integrations compile through WASM.

## Consequences

- Easier: roles colour per theme on the web and in standalone SVG from one file; untoned output draws exactly as before; GitHub image embeds and librsvg show a chosen theme; edges and clusters take roles; the eight built-in roles colour a diagram with no stylesheet; a diagram that keeps `classDef` literals for mermaid and GitHub follows Merlion themes once a stylesheet sets its class tokens; a hostile stylesheet can at worst set typed colours on Merlion diagrams.
- Harder: every existing snapshot changes once when the tone, dash and reset rules land, which ships ahead of the stylesheet work. The core carries `oklab_mix`, a CSS tokenizer and a resolver, each under fuzzing. Authors must run the compile step; CSS outside the subset is dropped, with a warning when it declares a token. A `classDef` colour still masks a stylesheet *tone* on its element (`I033`); re-theming it takes its own `--merlion-c-<name>-*` token. Every diagram with a `classDef` colour changes bytes once, when the literal moves inside `var()`. A diagram whose source already uses a built-in role name without a `classDef` (`class x muted`) changes colour. `merlion-themes.css` gains the three tone tokens in every theme.
- Expensive to reverse: the token names `--merlion-tone`, `--merlion-dash`, `--merlion-ok`, `--merlion-warn`, `--merlion-danger` and `--merlion-c-<name>-{fill,stroke,color}`; the built-in role names and their `6 4` dash; the fill (14%) and text (75%) mix ratios; the role class names for clusters and edges; and the compiled CSS shape once sites link it.
- Uncertainty: `prefers-color-scheme` inside `<img>` follows the page's `color-scheme` in Chrome only; Firefox and Safari are unmeasured. `oklch()` conversion is tested to ±1 per channel against Chrome inside sRGB only. The Chrome and librsvg behaviour above is measured on Playwright 1.63 and rsvg-convert 2.62.2.
