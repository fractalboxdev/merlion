# Integrations

## CLI

```
merlion render [<input>] [-o <output>] [--width <px>] [--direction auto]
               [--edge-style orthogonal|polyline|spline] [--font link|embed|system]
               [--hint <previous.svg>] [--strict] [--outline <file>]
               [--css <file>] [--theme <name>] [--auto-dark <name>]
merlion css    [<input.css>] [-o <output.css>] [--strict] [--follow-symlinks]
merlion check  [<input>...] [--strict] [--fix]
merlion outline [<input>] [--follow-symlinks]
merlion --version
```

- The input defaults to stdin and the output to stdout. A Markdown input (`.md`, `.mdx`) renders every ```` ```mermaid ```` block, and `-o` then names a directory: block `n` (1-based) of `<name>.md` is written to `<dir>/<name>-<n>.svg`. A block that fails leaves its previous output file untouched; the other blocks are still written.
- `--hint` defaults to the existing output file when one exists, so re-rendering in place is stable without any extra flag. `--no-hint` forces a fresh layout.
- `check` parses without rendering and prints diagnostics as `file:line:col: severity code message`. `--fix` applies every `Repair` fix to the file.
- `merlion css` compiles a stylesheet ([svg-output.md](svg-output.md#stylesheet)) from a file or stdin and writes the page CSS to `-o` or stdout; diagnostics use the `check` format under the stylesheet's name. `--strict` turns every `W017`–`W019` into an error, and an error writes nothing. `E013` exits `3`.
- `--css` bakes a stylesheet into the output ([svg-output.md](svg-output.md#palette)). `--theme <name>` picks the `[data-theme="<name>"]` block over `:root`; the default is `:root` alone. `--auto-dark <name>` adds the named block as the `prefers-color-scheme: dark` variant. A name the stylesheet does not define is a usage error (exit 2). Neither flag reads the `:root:not([data-theme])` media block. A Markdown input parses the stylesheet once for all its blocks. Without `--css`, built-in roles still render in their default tones ([svg-output.md](svg-output.md#built-in-roles)).
- Front matter and `%%{init}%%` never name a stylesheet.
- Exit codes: `0` every diagram rendered (warnings allowed); `1` at least one diagram failed to parse or render; `2` usage error; `3` at least one input exceeds limits (`TooLarge`) and none failed otherwise.

### File handling

The CLI runs in CI against repositories it doesn't control, so every write assumes the tree is hostile.

- Every write (`-o`, `--outline`, `--fix`) goes to a temporary file in the target's directory, is flushed, and is renamed over the target. The rename replaces a symbolic link rather than writing through it, and a crash never leaves a half-written file.
- Unless `--follow-symlinks` is given, the CLI refuses to read an input or a hint from, or write to, a path that is a symbolic link or whose resolved directory lies outside the current working directory. This covers `render`, `check`, `outline` and `css` inputs, `--css` stylesheets, Markdown files, and every `--batch` directory entry; a symbolic link in a `--batch` directory is skipped. A link planted in the tree therefore cannot echo a file from outside it (`/proc/self/environ`, a credentials file) into CI logs through a diagnostic. Standard input is always read.
- A hint file larger than the 1 MiB input limit is ignored with `I022 LayoutHintInvalid`.
- A stylesheet larger than 64 KiB, checked from file metadata before reading, fails with `E013 StylesheetTooLarge`.
- Arguments are parsed by hand with the standard library ([supply-chain.md](supply-chain.md)).
- Distributed as prebuilt binaries for macOS (arm64, x86_64), Linux (x86_64, arm64, musl static) and Windows (x86_64), and through `cargo install`.

## `@fractalboxdev/merlion-wasm`

```ts
export function init(wasm?: BufferSource | URL | Response): Promise<void>; // browser
export function initSync(wasm: BufferSource): void;                         // Node, at build time
export function render(source: string, options?: RenderOptions): RenderResult;
export function check(source: string, options?: { strict?: boolean }): Diagnostic[];
export function compileStylesheet(
  css: string,
  options?: { theme?: string; autoDark?: string; strict?: boolean },
): { css: string | null; palette: Palette | null; diagnostics: Diagnostic[] };

interface RenderOptions {
  width?: number;                              // container width in px; default 720
  direction?: "auto" | "source";
  edgeStyle?: "orthogonal" | "polyline" | "spline";
  font?: "link" | "embed" | "system";
  strict?: boolean;
  idPrefix?: string;                           // [a-z][a-z0-9-]{0,31}
  hint?: string;                               // the previous SVG, for stable layout
  fuel?: number;
  palette?: Palette;                           // from compileStylesheet; validated against the token grammars
}
type Palette = {
  roles: Record<string, string>;               // token name without `--merlion-` → value: `#` hex colours, `stroke` as a number string, `c-{name}-{fill|stroke}` may be `none`
  tones?: Record<string, { tone?: string; dash?: number[] }>;          // node and edge roles, in cascade order; `dash: []` is `none`
  clusterTones?: Record<string, { tone?: string; dash?: number[] }>;
  dark?: Record<string, string>;
  darkTones?: Record<string, { tone?: string; dash?: number[] }>;
  darkClusterTones?: Record<string, { tone?: string; dash?: number[] }>;
};
interface Diagnostic {
  severity: "error" | "warning" | "repair" | "info";
  code: string;
  line: number; column: number;                // 1-based; 0 without a location
  byteStart: number; byteEnd: number;
  message: string;
  fix: { byteStart: number; byteEnd: number; replacement: string } | null;
}
interface RenderResult {
  svg: string | null; outline: string | null; diagnostics: Diagnostic[];
  error: RenderError | null; fuelUsed: number;
}
```

- `packages/merlion-wasm/index.d.ts` is the authoritative JavaScript contract; the rehype plugin, the Astro integration and the demo use its names and shapes. Option names are camelCase. The core's own names (`target_width`, `id_prefix`, `edge_style`) and any other unknown key throw a `TypeError` naming the key (for the core's names, also the camelCase option), so an older module never silently ignores `palette`. A `css` string above 64 KiB returns `E013` without crossing the boundary.
- `compileStylesheet` throws `RangeError` for a `theme` or `autoDark` the stylesheet does not define. The glue turns `palette` into the canonical string of `Palette::canonical` (shape and separators checked in JavaScript); the core parses it with `Palette::parse`, which checks every value against its token's grammar, so a render from `compileStylesheet`'s palette and a CLI render with the same `--css`/`--theme` are byte-identical.
- The module returns the JSON of `merlion render --json` (`merlion_render::json`, shared by both surfaces); the glue converts it to the camelCase shape above.

- `render` is synchronous after initialisation. Its worst-case time is bounded by the fuel limit ([ADR-0008](adr/0008-deterministic-work-budget.md)), not by a clock. For source the page doesn't control, run it in a Web Worker so a heavy diagram never blocks the main thread; the package exports a `worker.js` entry that wraps `render` in a message handler.
- The glue is hand-written: strings cross the boundary as UTF-8 pointer-and-length pairs through exported `alloc`/`dealloc` functions. No `wasm-bindgen` is involved. The glue:
  - creates a fresh `Uint8Array` view over `memory.buffer` after every call that may allocate, because `memory.grow` detaches earlier views;
  - checks every returned pointer and length against `memory.buffer.byteLength` before reading, and decodes with `new TextDecoder("utf-8", { fatal: true })`;
  - on any trap (allocation failure, `unreachable`, stack exhaustion) discards the instance and instantiates a new one from the cached `WebAssembly.Module`, then returns `{ svg: null, outline: null, diagnostics: [<E001 InternalError>] }`, because a trapped instance's allocator state is undefined;
  - serialises calls: `render` is not re-entrant on one instance.
- The package's `dependencies` field is empty and it has no install scripts. `package.json` records the `.wasm` file's SHA-256 under `merlion.wasmSha256`. That value sits in the same tarball as the file, so it proves nothing by itself; it is the value to compare against the release's build attestation and published checksums ([supply-chain.md](supply-chain.md#releases)).

## `@fractalboxdev/merlion-rehype`

```ts
import rehypeMerlion from "@fractalboxdev/merlion-rehype";
unified().use(remarkParse).use(remarkRehype).use(rehypeMerlion, {
  width: 720,          // RenderOptions.width
  strict: false,
  source: "details",   // "details" | "none": keep the Mermaid source in a collapsed <details>
  cacheDir: ".merlion", // previous renders, used as layout hints
  viewer: true,        // wrap each SVG in <merlion-view>
  fontCss: true,       // the page loads @fractalboxdev/merlion-themes/merlion-font.css; silences the font warning
  stylesheet: "diagram.css", // compiled once per build; never passed to inline renders
});
```

- Replaces every `pre > code.language-mermaid` element with:

  ```html
  <figure id="diagram-{n}" class="merlion-figure">
    <merlion-view>{svg}</merlion-view>
    <figcaption>{accTitle or title, if any}</figcaption>
    <details><summary>Diagram source</summary><pre><code class="language-mermaid">…</code></pre></details>
  </figure>
  ```

- It walks the tree by hand, so it has no `unist-util-visit` dependency; `@types/hast` is a development dependency only.
- It passes `idPrefix` = `m` + the first 8 hex characters of FNV-1a 64 over the file's path relative to the project root + `-` + `n`, so diagrams from several files on one page keep unique ids ([svg-output.md](svg-output.md#ids-and-data-attributes)).
- The SVG enters the tree as a `raw` node containing only the core's output. `figcaption` and the `<details>` source are hast text nodes, so the serialiser escapes them.
- A parse error leaves the code block in place and reports the diagnostic through the unified `vfile` (`file.message`), which fails the build when `strict` is set.
- `cacheDir` holds one entry per (file path, block index). The entry's filename is the FNV-1a 64 hash of that pair (with the path relative to the project root) in hex, so no source path can name a location outside `cacheDir`. It stores the last SVG and the content hash of the source that produced it. Every build renders every block, with the stored SVG as the layout hint; the stored SVG is never inlined, because anyone who can write `cacheDir` (a pull-request author committing it, for example) controls its bytes and can compute the public FNV hash ([security.md](security.md)). An entry is rewritten only when the hash or the SVG changes. When a diagram is inserted above others the indices shift and a hint lands on a different diagram; the core then discards it as having too few surviving nodes.
- Writes to `cacheDir` follow the CLI's [file handling](#file-handling) rules.
- `stylesheet` is read under the CLI's [file handling](#file-handling) rules, compiled once per build through `compileStylesheet`, and exposed as `file.data.merlion.css`. Inline diagrams are rendered without a palette; the page's cascade themes them.

## `@fractalboxdev/merlion-astro`

Registers `@fractalboxdev/merlion-rehype` in `markdown.rehypePlugins` and adds `merlion-themes.css`, `merlion-font.css` (both from `@fractalboxdev/merlion-themes`), the compiled `stylesheet` when one is set (written as an asset), and the `<merlion-view>` script, only on pages that contain a diagram. Its options match the rehype plugin's, except that `fontCss` names the font stylesheet to import (default `@fractalboxdev/merlion-themes/merlion-font.css`, `false` to skip it) and the plugin receives `fontCss: true` whenever one is imported.

## Editors

The same core serves editors in two ways. **Rendering** puts SVG in the Markdown preview. **Language service** provides diagnostics, quick fixes and outlines while typing. In an editor, stable layout keeps the preview from jumping on every keystroke: the previous render of each block, held in memory, is the next render's hint.

### Language server: `merlion lsp`

A CLI subcommand speaking the Language Server Protocol over stdio, covering `.mmd`/`.mermaid` files and ```` ```mermaid ```` fences in Markdown.
- `textDocument/publishDiagnostics` from the parser's diagnostics ([parser.md](parser.md#diagnostics)).
- `textDocument/codeAction`: one quick fix per `Repair`, plus "apply all repairs".
- `textDocument/documentSymbol`: nodes and clusters.
- `textDocument/hover` on a node id: its label and edges, taken from the outline.
- JSON-RPC and JSON parsing are hand-written, in keeping with zero runtime dependencies ([supply-chain.md](supply-chain.md)). A `Content-Length` above 16 MiB or JSON nesting beyond 64 ends the session with an error before any allocation for the body.
- Positions: the server offers `positionEncoding: "utf-8"` (LSP 3.17) and falls back to UTF-16 code units, converting the parser's columns ([parser.md](parser.md#diagnostics)).

### VS Code and Cursor: `merlion-vscode`

Cursor runs VS Code extensions (installed from Open VSX), so one extension serves both. It is published to the Visual Studio Marketplace and to Open VSX. TODO(owner): the Marketplace publisher id.
- **Preview:** contributes `markdown.markdownItPlugins` and extends the built-in preview's markdown-it. Mermaid fences render through `@fractalboxdev/merlion-wasm` (`initSync`) in the extension host, synchronously, so the preview webview receives finished SVG and runs no renderer.
- **Theme:** maps VS Code theme colours (`--vscode-editor-background`, `--vscode-editor-foreground`, …) onto `--merlion-*` in a preview stylesheet. The diagram follows the editor theme with no re-render.
- **Zoom:** `@fractalboxdev/merlion-view` is loaded as a preview script.
- **Language features:** starts `merlion lsp` from the bundled binary for the current platform. The extension build takes each binary from the signed release and checks its SHA-256 against the release attestation; it never rebuilds them.
- **Rendering cost:** the extension host is shared by every extension, so the preview renders with a lower `fuel` limit than the CLI and shows the diagnostic in place of a diagram that exceeds it.
- **Standalone files:** a custom editor for `.mmd`/`.mermaid` shows source and preview side by side.

The incumbent, `bierner.markdown-mermaid` (MIT; 777,735 Open VSX downloads as of 2026-09-22), renders with mermaid inside the preview webview. Merlion's differences: no renderer in the webview, stable layout while typing, theme-following colours, and quick fixes.

### Zed

Zed's Markdown preview already renders Mermaid, natively, through the `merman` crate (`crates/mermaid_render`). Zed extensions cannot draw custom UI or images as of 2026-09-22: the visual extension API (discussion #53403) and file-preview API (#59598) are proposals. Two routes follow:
- **Now:** a Zed extension that registers `merlion lsp` for Markdown and Mermaid files, giving diagnostics and quick fixes next to Zed's own rendering.
- **Rendering:** propose `merlion-render` to Zed as an alternative behind `mermaid_render`'s interface. Merlion is a `no_std` crate with no dependencies, and its MIT licence is compatible with Zed's GPL-3.0-or-later. This needs Zed maintainers to agree, and the benchmark results against `merman` are the argument. Zed's contribution terms (checked 2026-09-22) require a signed [Contributor License Agreement](https://zed.dev/cla) before merge and ask that a larger feature start as a GitHub discussion following its feature process, not as a pull request; the proposal starts there.

### JetBrains and others

Out of scope before 1.0. Any editor with an LSP client gets the language server; JetBrains preview rendering would need a plugin built on the CLI.

## Markdown and LLM outputs

For every rendered page, `merlion outline` and the rehype plugin's `outline` hook produce the plain-text outline ([svg-output.md](svg-output.md#text-alternative)) alongside the Mermaid source. This is what sites put in `.md` mirrors and `llms-full.txt`.
