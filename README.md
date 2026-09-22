# Merlion

Merlion renders Mermaid flowcharts to static, themeable, accessible SVG. The core (`crates/merlion-render`) is a `no_std` Rust library with zero dependencies; the same code runs as the `merlion` CLI and as a WebAssembly module, and native and WASM output are byte-identical. Labels are `<text>`, colours are CSS custom properties, so light, dark and custom themes switch with CSS alone, and every SVG carries `role="img"`, a `<title>` and a generated `<desc>`. The output is safe to inline without a sanitiser.

Documentation: <https://merlion-docs.debuggingfuturecors.workers.dev/> (source in [docs/](docs/)). Design and contracts: [specs/](specs/README.md).

## Guarantees

| Property | Contract |
|---|---|
| Output | One self-contained SVG per diagram: labels are `<text>`, colours are CSS custom properties over presentation-attribute defaults, with `role="img"`, `<title>` and a generated `<desc>` |
| Safety | The SVG can be inlined without a sanitiser even when the source or the stylesheet is untrusted: no scripts, no source or stylesheet CSS text, no selectors that reach the host page |
| Theming | Light, dark and custom themes switch by CSS alone; there is no re-render. One stylesheet themes roles and `classDef` colours on the page and, baked, in standalone SVG |
| Runtime | Renders without a DOM or a browser; server and browser output are byte-identical for the same source, options and layout hint |
| Dependencies | 0 runtime crates and 0 runtime npm packages |
| Stability | Given the previous render as a layout hint, nodes that keep their layer across an edit keep their order and move at most `stability` (default 2) positions; the hint is dropped when fewer than half the nodes survive |
| Fit | Layout takes the container width as an input and fits it |

## Status

Pre-1.0 ([roadmap](specs/roadmap.md)). Flowcharts only; other diagram types return `E003`. The npm packages are workspace packages, not yet published. Over the 390 flowcharts of the mermaid 12.0.0 corpus, Merlion renders 384, fits 98.2% of them in 720 px (mermaid-dagre 81.0%, mermaid-elk 82.3%) and moves surviving nodes less after a one-line edit than either (mean 0.054, p95 0.253), per [bench/results/2026-09-22-round2.md](bench/results/2026-09-22-round2.md) at commit `5392e69`.

| Package | Path |
|---|---|
| `merlion-render` (core) | `crates/merlion-render` |
| `merlion` CLI | `crates/merlion-cli` |
| `@fractalbox/merlion-wasm` | `crates/merlion-wasm`, `packages/merlion-wasm` |
| `@fractalbox/merlion-rehype` | `packages/merlion-rehype` |
| `@fractalbox/merlion-astro` | `packages/merlion-astro` |
| `@fractalbox/merlion-view` (`<merlion-view>` pan and zoom) | `packages/merlion-view` |
| `@fractalbox/merlion-themes` | `packages/merlion-themes` |

## Quick start

The same commands, with what each produces, are in [Getting started](docs/src/content/docs/getting-started.md).

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

Diagrams are coloured by default: decisions take `warn`, stores `store`, terminals `ok`, and each top-level subgraph the next series tint; `--no-auto-tone` draws everything neutral. Roles given with `class` or `:::` and one compiled stylesheet colour the rest ([roles and stylesheets](https://merlion-docs.debuggingfuturecors.workers.dev/guides/roles-and-stylesheets/)).

### Stylesheet

```sh
target/release/merlion css site.css -o site.compiled.css  # page CSS: literal values, fixed selector shapes
target/release/merlion render d.mmd --css site.css --theme dark -o d.svg  # bake one theme into a standalone SVG
```

### WebAssembly

```sh
sh packages/merlion-wasm/scripts/build-wasm.sh          # writes packages/merlion-wasm/merlion.wasm
```

```js
import { readFileSync } from "node:fs";
import { initSync, render } from "@fractalbox/merlion-wasm";

initSync(readFileSync(new URL("./node_modules/@fractalbox/merlion-wasm/merlion.wasm", import.meta.url)));
const { svg, diagnostics } = render("flowchart LR\n  a --> b\n", { width: 640, idPrefix: "d1" });
```

### rehype

```js
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkRehype from "remark-rehype";
import rehypeStringify from "rehype-stringify";
import rehypeMerlion from "@fractalbox/merlion-rehype";

const html = await unified()
  .use(remarkParse)
  .use(remarkRehype)
  .use(rehypeMerlion, { width: 720, cacheDir: ".merlion", fontCss: true })
  .use(rehypeStringify, { allowDangerousHtml: true })
  .process("```mermaid\nflowchart LR\n  a --> b\n```\n");
```

### Astro

```js
// astro.config.mjs
import { defineConfig } from "astro/config";
import merlion from "@fractalbox/merlion-astro";

export default defineConfig({
  integrations: [merlion({ stylesheet: "src/styles/diagrams.css" })],
});
```

The docs site in [docs/](docs/) is an Astro Starlight site built this way: every diagram on it, including the ones in [specs/](specs/README.md), is a ```` ```mermaid ```` block rendered by Merlion at build time.

## Tests

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

sh packages/merlion-wasm/scripts/build-wasm.sh
pnpm install --ignore-scripts
pnpm test                                               # every JS package over the built merlion.wasm, and README/docs alignment
```

## Docs site

```sh
sh packages/merlion-wasm/scripts/build-wasm.sh
pnpm --filter docs dev                                  # http://localhost:4321/
pnpm --filter docs build                                # docs/dist/
pnpm --filter docs run deploy                           # build, then wrangler deploy to Cloudflare Workers
```

`docs/scripts/prepare.mjs` runs before every dev server and build: it copies `merlion.wasm` and its glue into `docs/public/` for the playground and generates the gallery pages from the test fixtures and the benchmark corpus. Deploying needs `CLOUDFLARE_API_TOKEN` (Account → Workers Scripts:Edit) and `CLOUDFLARE_ACCOUNT_ID`, or `wrangler login`.

## Benchmark and determinism

The harness in [bench/](bench/README.md) scores Merlion against mermaid (dagre and ELK) on crossings, bends, stress, fit, compatibility and stability ([specs/benchmark.md](specs/benchmark.md)).

```sh
cargo build --release -p merlion-cli
cd bench
pnpm install --ignore-workspace
pnpm bench run --renderers merlion,mermaid-dagre,mermaid-elk
pnpm bench report
pnpm bench determinism [--corpus compat|sequence] [--font link|embed|system]   # CLI --batch vs WASM, byte for byte
```

`pnpm bench determinism` renders a corpus with `merlion render --batch` and through the WASM module and fails on any difference. Two corpora: `compat`, the mermaid flowcharts, and `sequence`, the core's sequence fixtures. CI runs every corpus in all three font modes.

## Licence

MIT. The embedded Inter subset is under the SIL Open Font License 1.1; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and [specs/licensing.md](specs/licensing.md).
