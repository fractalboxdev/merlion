# fontgen

Generates the font data the core measures and embeds with (specs/text-measurement.md). Development tool only: `publish = false`, and `ttf-parser` never reaches a published crate (specs/supply-chain.md).

| Output | Made by | Committed at |
|---|---|---|
| Metric tables: advances, class-based pair kerning, vertical metrics | `cargo run -p fontgen --release` | `crates/merlion-render/src/text/tables.rs` |
| WOFF2 subsets for `font: "embed"` | `tools/fontgen/subset.sh` | `crates/merlion-render/assets/Inter-{Regular,SemiBold}.subset.woff2` |
| OFL text | `tools/fontgen/subset.sh` (copies `OFL.txt`) | `crates/merlion-render/assets/OFL.txt` |

## Inputs

Inter 4.1 static TTFs from the [Inter release](https://github.com/rsms/inter/releases/tag/v4.1), placed in `tools/fontgen/input/` (gitignored). The generator refuses a file whose SHA-256 differs:

| File | SHA-256 |
|---|---|
| `Inter-Regular.ttf` | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| `Inter-SemiBold.ttf` | `78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3` |

## Regenerate

From the repository root:

```sh
cargo run -p fontgen --release              # rewrite tables.rs
cargo run -p fontgen --release -- --check   # exit 1 if tables.rs is stale
python3 -m venv .tmp/venv && .tmp/venv/bin/pip install fonttools brotli
tools/fontgen/subset.sh                     # rewrite the WOFF2 subsets (PYFTSUBSET overrides the pyftsubset path)
```

## What the tables hold

- **Coverage** (`--unicodes` prints it): U+0020–007E, U+00A0–00FF, Latin Extended-A/B (U+0100–024F), Greek (U+0370–03FF), Cyrillic (U+0400–04FF), General Punctuation (U+2000–206F), Arrows (U+2190–21FF), Box Drawing (U+2500–257F). Only code points the font maps get an entry: 982 per weight. Inter 4.1 maps no Box Drawing code point and 28 of 112 Arrows. Bidi controls are excluded because the core strips them.
- **Advances** from `hmtx`, per code point.
- **Kerning** from the GPOS lookups the `kern` feature references (in any script): pair adjustment (LookupType 2, formats 1 and 2), first applicable subtable per lookup, summed over lookups; the legacy `kern` table only when the font has no GPOS `kern` feature. The pair matrix over covered glyphs is compressed to classes: glyphs with identical rows share a left class, identical columns a right class, and only non-zero class pairs are stored. Regular: 139 × 137 classes, 3,218 pairs; SemiBold: 143 × 144 classes, 3,552 pairs.
- **Vertical metrics** from OS/2 `sTypo*` because Inter sets `USE_TYPO_METRICS` (fsSelection bit 7), which every browser honours; otherwise `hhea`. Inter: unitsPerEm 2048, ascender 1984, descender −494, lineGap 0.
- **Average advance**: mean advance over covered glyphs with a non-zero advance, rounded (Regular 1287, SemiBold 1325 units).

Widths computed from the tables match HarfBuzz shaping of the input fonts with `kern` on and `calt`/`liga` off exactly, for every ordered pair of printable ASCII, basic Cyrillic and Greek capitals (33,489 pairs per weight).

## Subsets

`subset.sh` runs `pyftsubset` with the same coverage, `--layout-features=kern` (no `calt`, no `liga`), `--no-hinting` and `--flavor=woff2`. Sizes: Regular 37,368 bytes, SemiBold 38,300 bytes (about 100 KB of base64 together in an `"embed"` SVG). The OFL counts a subset as a Modified Version; Inter declares no Reserved Font Name, so the subset keeps the name.
