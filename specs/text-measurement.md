# Text measurement

Layout needs the size of every label before it can place anything. Merlion measures labels from committed font metric tables, never from a DOM, so output is identical on every target.

## Font tables

- **Default font: Inter** (SIL Open Font License 1.1, no Reserved Font Name). The tables cover the Regular and SemiBold weights.
- A table holds each glyph's advance width in font units (keyed by Unicode code point), the pair kerning values, `unitsPerEm`, ascender, descender and line gap.
- Coverage: Latin, Latin Extended-A/B, Greek, Cyrillic, general punctuation, arrows and box drawing. A code point outside the table is measured at 1 em if it falls in the CJK, Hangul or fullwidth ranges, and at the table's average advance width otherwise. Each fallback adds an `Info` diagnostic `I010 UnmeasuredGlyph` once per diagram.
- `tools/fontgen` generates the tables into `const` Rust arrays in the core. The generated source is committed, together with the input font's SHA-256 and the OFL text. The generator is the only place a font-parsing crate is used, and it is never published ([supply-chain.md](supply-chain.md)).

## Measuring

- Width = sum of advance widths + kerning adjustments, scaled to the font size (default 14 px).
- Height = (ascender − descender + line gap), scaled, per line. A label's height is the sum of its line heights and its width is its widest line.
- Title + detail node labels ([svg-output.md](svg-output.md#text)) measure the title line at the font size and every detail line with the same tables at 0.8 × the font size; a detail line's height and baseline offset scale by the same factor. The last title line is 2 px taller at 14 px (1/7 of the font size), which leaves a gap between the tiers. Every other label measures all lines at the font size.
- Labels wrap at a maximum width (default 200 px) on whitespace, with a hard break inside any word longer than the maximum. A `\n` in the label forces a break; the parser writes one for every `<br>` the source carries ([parser.md](parser.md#labels-and-entity-codes)), so a `<br>` still in the label is text the source escaped as `#lt;br#gt;`.
- Stylesheets never set font tokens ([svg-output.md](svg-output.md#stylesheet)), so measurement depends only on the source and the render options; font size is `RenderOptions.font_size`.
- Padding comes from the node shape (see [layout.md](layout.md)), never from the text measure.
- Measurement applies pair kerning and no other OpenType feature. The browser draws the same way because the SVG's embedded style disables contextual alternates and ligatures (Inter's `calt` substitutes glyphs in sequences such as `->`) and resets `letter-spacing`, `word-spacing`, `text-transform` and the font weight and stretch that the host page would otherwise pass down ([svg-output.md](svg-output.md#embedded-style)). `tools/fontgen` reads advances with the same features off.

## Serving the font

The SVG must be drawn in the font that was measured, or labels overflow their boxes. The renderer outputs one of these, chosen by the `font` option:

| `font` | Output | Use |
|---|---|---|
| `"link"` (default) | Nothing inline. `font-family: var(--merlion-font, Inter, …)`; the host page loads Inter | Sites that already serve Inter, or that include `merlion-font.css` from `@fractalbox/merlion-themes`: two `@font-face` rules for the committed subsets under the family `Merlion Inter` (so a site's own Inter is never replaced) and `--merlion-font` set to it on `:root`. `@fractalbox/merlion-astro` adds that stylesheet by default and passes `fontCss: true`; the rehype plugin warns once per build when `font: "link"` is used and its `fontCss` option is not set, because a page without Inter draws in the fallback font and labels overflow |
| `"embed"` | The committed WOFF2 subset (covering exactly the table's coverage ranges) as a `data:` URI inside the SVG's `<style>` | Standalone SVG files |
| `"system"` | Metrics for a system-font stack (`system-ui`) at a declared tolerance | Accepted drift of up to ±6% in label width; boxes get matching extra padding |

The core does not subset fonts at render time; that would need font parsing and a Brotli encoder in the runtime path. `tools/fontgen` produces the WOFF2 subset once, at development time, and commits it next to the metric tables. The file's size is unmeasured; TODO(owner): record it once generated, and add per-diagram subsetting only if the size proves to matter.

The OFL counts a subset as a Modified Version. That is permitted because Inter declares no Reserved Font Name, so the subset keeps the name "Inter". The OFL notice travels with the subset file and, in `"embed"` mode, inside the SVG as an XML comment ([licensing.md](licensing.md)).
