# Merlion

Merlion renders Mermaid flowcharts to static, themeable, accessible SVG. The core (`crates/merlion-render`) is a `no_std` Rust library with zero dependencies; the same code runs as the `merlion` CLI and as a WebAssembly module, and native and WASM output are byte-identical. Labels are `<text>`, colours are CSS custom properties (light, dark and custom themes switch with CSS alone), and every SVG carries `role="img"`, a `<title>` and a generated `<desc>`. The output is safe to inline without a sanitiser. The design lives in [specs/](specs/README.md).

## Status

Pre-1.0 ([roadmap](specs/roadmap.md)). Flowcharts only. 384 of the 390 flowcharts in the mermaid 12.0.0 corpus (`bench/corpus/compat`) render; the other 6 stop at an `E002` syntax error. Other diagram types return `E003`.

| Package | Path |
|---|---|
| `merlion-render` (core) | `crates/merlion-render` |
| `merlion` CLI | `crates/merlion-cli` |
| `@fractalboxdev/merlion-wasm` | `crates/merlion-wasm`, `packages/merlion-wasm` |
| `@fractalboxdev/merlion-rehype` | `packages/merlion-rehype` |
| `@fractalboxdev/merlion-astro` | `packages/merlion-astro` |
| `@fractalboxdev/merlion-view` (`<merlion-view>` pan and zoom) | `packages/merlion-view` |
| `@fractalboxdev/merlion-themes` | `packages/merlion-themes` |

## Quick start

### CLI

```sh
cargo build --release -p merlion-cli
echo 'flowchart LR
  a[Source] --> b{Valid?} -->|yes| c[Render]' | target/release/merlion render > diagram.svg
target/release/merlion render docs/page.md -o out/      # every ```mermaid block of a Markdown file
target/release/merlion check diagram.mmd --fix          # apply the parser's repairs to the file
target/release/merlion render diagram.mmd --json        # {svg, outline, diagnostics, fuel_used, error}
```

Re-rendering onto an existing output file uses it as the layout hint, so nodes keep their positions across edits. `--font embed` inlines an Inter subset for standalone files. Exit codes: `0` rendered, `1` failed, `2` usage error, `3` input over a limit.

### Roles and stylesheets

A class name given with `class` or `:::` is a role, with or without a `classDef`; `class e1 failure` gives edge `a e1@--> b` a role. Eight roles work with no stylesheet: node tones `accent`, `ok`, `warn`, `danger`, `muted`, the dashed cluster `group`, and the edges `failure` (danger tone, dashed) and `async` (dashed). `merlion-themes.css` defines their tones for every theme.

A stylesheet colours roles and themes in a CSS subset that sets `--merlion-*` tokens only ([specs/svg-output.md](specs/svg-output.md#stylesheet)):

```css
:root { --brand: #0f766e; --merlion-accent: var(--brand); }
[data-theme="dark"] { --merlion-bg: #101418; --merlion-fg: #e6e6e6; }
.merlion-c-store { --merlion-tone: #b8408f; }
.merlion-cc-zone { --merlion-dash: 4 2; }
```

```sh
target/release/merlion css site.css -o site.compiled.css  # page CSS: literal values, fixed selector shapes
target/release/merlion render d.mmd --css site.css --theme dark -o d.svg  # bake one theme into a standalone SVG
target/release/merlion render d.mmd --css site.css --auto-dark dark -o d.svg  # :root, plus dark under prefers-color-scheme
```

A page links only the compiled CSS, never the source stylesheet; inline SVGs render without `--css` and follow the page's cascade. Baked SVGs carry the theme's literals in attributes and fallbacks, so GitHub image embeds and librsvg draw them. `--theme` and `--auto-dark` name `[data-theme]` blocks; an undefined name exits `2`. A stylesheet over 64 KiB or its other limits exits `3` with `E013`; `--strict` turns `W017`–`W019` into errors.

### WebAssembly

```sh
sh packages/merlion-wasm/scripts/build-wasm.sh          # writes packages/merlion-wasm/merlion.wasm
```

```js
import { readFileSync } from "node:fs";
import { initSync, render } from "@fractalboxdev/merlion-wasm";

initSync(readFileSync(new URL("./node_modules/@fractalboxdev/merlion-wasm/merlion.wasm", import.meta.url)));
const { svg, diagnostics } = render("flowchart LR\n  a --> b\n", { width: 640, idPrefix: "d1" });
```

In a browser, `await init()` fetches `merlion.wasm` next to the module. Options and result shapes are in [`packages/merlion-wasm/index.d.ts`](packages/merlion-wasm/index.d.ts), the JavaScript contract every package follows.

### rehype

```js
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkRehype from "remark-rehype";
import rehypeStringify from "rehype-stringify";
import rehypeMerlion from "@fractalboxdev/merlion-rehype";

const html = await unified()
  .use(remarkParse)
  .use(remarkRehype)
  .use(rehypeMerlion, { width: 720, cacheDir: ".merlion", fontCss: true })
  .use(rehypeStringify, { allowDangerousHtml: true })
  .process("```mermaid\nflowchart LR\n  a --> b\n```\n");
```

Each ```` ```mermaid ```` block becomes a `<figure>` holding the inline SVG. A block that fails to parse stays a code block and its diagnostics land on the vfile. Astro sites add `@fractalboxdev/merlion-astro` to `integrations` instead.

## Tests

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

sh packages/merlion-wasm/scripts/build-wasm.sh
pnpm install --ignore-scripts
pnpm test                                               # every JS package, over the built merlion.wasm
```

## Benchmark and determinism

The harness in [bench/](bench/README.md) scores Merlion against mermaid (dagre and ELK) on crossings, bends, stress, fit, compatibility and stability ([specs/benchmark.md](specs/benchmark.md)).

```sh
cargo build --release -p merlion-cli
cd bench
pnpm install --ignore-workspace
pnpm bench run --renderers merlion,mermaid-dagre,mermaid-elk
pnpm bench report
pnpm bench determinism [--font link|embed|system]       # CLI --batch vs WASM, byte for byte
```

`pnpm bench determinism` renders every compat diagram with `merlion render --batch` and through the WASM module and fails on any difference. CI runs it for all three font modes.

## Demo

```sh
sh packages/merlion-wasm/scripts/build-wasm.sh
node demo/build-gallery.mjs                             # demo/gallery/*.svg and demo/gallery.json
node demo/serve.mjs                                     # http://localhost:4173/demo/ (PORT to change)
```

The demo holds a live editor with diagnostics and stable re-layout while typing, and a gallery of the test fixtures and twelve diagrams from the mermaid corpus.

## Licence

MIT. The embedded Inter subset is under the SIL Open Font License 1.1; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [specs/licensing.md](specs/licensing.md).
