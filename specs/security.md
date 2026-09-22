# Security

Merlion renders untrusted Mermaid source — LLM output, chat messages, pull-request content — into SVG that hosts inline into their pages without a sanitiser. This spec states what each input can reach and which rule, specified elsewhere, stops it.

## Threat model

| Input | Controlled by | Reaches | Rules |
|---|---|---|---|
| Mermaid source | Page authors at build time; anyone in the browser path and in CI renders of pull requests | The host page's DOM and CSS through the inlined SVG; CPU and memory of the build or the browser tab | [Output](#output), [Resource bounds](#resource-bounds) |
| Previous SVG and `cacheDir` entries (layout hints) | Whoever can write the output file or cache, including a pull-request author | The hint parser; layout CPU | A hint is untrusted: malformed, unknown-version or oversized hints are discarded with `I022` ([layout.md](layout.md#stable-layout)) |
| File paths and symbolic links in the working tree | Repository contents | Files the CLI and rehype plugin write | [integrations.md](integrations.md#file-handling) |
| `%%{init}%%` JSON, YAML front matter | Source author | Hand-written parsers | Accepted subsets and limits in [parser.md](parser.md#front-matter-and-directives) |
| LSP messages | The editor and the workspace it opens | The language server's memory | Message and nesting limits in [integrations.md](integrations.md#language-server-merlion-lsp) |
| Stylesheet (`--css`, `compileStylesheet`, the rehype `stylesheet` option) | Repository contributors, site authors, possibly a model; untrusted | Page CSS through the compiled output; the embedded style and literals of baked SVGs; parser CPU | [Stylesheet](svg-output.md#stylesheet): typed tokens only, fixed selector shapes, limits; read under [file handling](integrations.md#file-handling) |
| Caller hooks | The integrating application (trusted code) | The SVG | Hooks return geometry, which the core validates ([svg-output.md](svg-output.md#caller-hooks)) |
| Release pipeline | Maintainers; CI configuration | npm and crates.io publish rights | [supply-chain.md](supply-chain.md#releases) |

Out of scope: a host page that sets hostile `--merlion-*` values on its own diagrams outside Merlion's compiled output (the host already controls its page), and denial of service by a caller that raises the limits.

## Output

The guarantee that inline SVG needs no sanitiser rests on four rules, each specified in [svg-output.md](svg-output.md):

1. **No active content.** A fixed list of elements and attributes never appears, and links are limited to `https`, `http`, `mailto` and relative URLs checked after the same whitespace stripping browsers apply.
2. **No source or stylesheet text in CSS.** Style statements and stylesheets are parsed into typed values and re-serialised. An inline SVG's `<style>` applies to the whole document and is parsed as foreign content, so passing text through would let a diagram restyle the host page, request `url()` resources, or close the style and inject markup. `style::build` drops any rule containing `<`, `&` or `\` at run time. Class names and role names reach the style only inside class selectors and `--merlion-c-{name}-*` token names, after matching `[A-Za-z_][A-Za-z0-9_-]{0,63}`.
3. **Scoped selectors.** Every emitted rule starts with `#{id} `, `:where(#{id} ` or `#{id}:not(:is([data-theme="light"] *)) `. Compiled page CSS uses only the selector shapes in [svg-output.md](svg-output.md#stylesheet): it has no attribute selector other than `[data-theme="<ident>"]` and `:not([data-theme])`, so it cannot probe host attributes.
4. **Escaped, filtered text.** Entities for the five XML specials; control characters, non-characters and bidirectional formatting characters removed.

A page never links an uncompiled stylesheet. Linked raw, a stylesheet is full CSS: it restyles host elements, exfiltrates attribute values through attribute-prefix selectors, and fetches through `@import`, `image-set()`, `url()` paint servers (including through a custom property) and font families naming a host `@font-face`. Compiled output carries typed colours, numbers and dash lists only, and no font token.

Role classes, built-in or not, carry presentation only, never provenance or trust: any diagram can put `ok` or `danger` on any element.

Ids: the default `{id}` is a 32-bit FNV prefix, and a colliding source can be found offline in minutes. On a page mixing diagrams from different authors, a colliding diagram could make another diagram's markers and accessible name resolve to its own definitions. Hosts that show more than one diagram pass `id_prefix`, which removes the dependency on the hash.

## Resource bounds

Limits on input size, declared and layered graph size, layer count, nesting depth and label length are in [architecture.md](architecture.md#boundaries). Every phase draws from one fuel counter ([ADR-0008](adr/0008-deterministic-work-budget.md)), so the worst-case render time is a function of the input and options alone. The layout hint is parsed once per render, after one optional fuel unit per byte is drawn; a hint the remaining fuel cannot cover is dropped with `I022`. Label measurement draws one unit per byte of label text: mandatory for the first measurement, optional for each container-fit re-measurement, which lays out again only the labels wider than the new wrap width and stops container fit when the fuel runs out. The parsers track recursion depth explicitly, so deep input returns `E010` instead of exhausting the stack; in WASM a stack overflow would trap and leave the instance unusable. The WASM glue recovers from any trap by re-instantiating ([integrations.md](integrations.md#fractalboxmerlion-wasm)).

## Fuzzing

Each hand-written parser has its own `cargo-fuzz` target, plus one for the whole pipeline. CI runs every target for 5 minutes per pull request that touches its code and for 1 hour nightly, from a committed seed corpus that includes the `compat` diagrams.

| Target | Property checked |
|---|---|
| `parse_flowchart` and one per diagram type | No panic; every `Repair` fix applied to the source parses without that diagnostic |
| `front_matter` | No panic; subset violations return `E011` |
| `init_directive` | No panic; limits return `E012` |
| `layout_hint` | No panic; any rejected hint returns `I022` |
| `render` | Takes (source, stylesheet) pairs. No panic; output is well-formed XML; output contains none of the forbidden elements, attributes or selectors; the embedded style contains no `<`, `&` or `\`, every rule starts with an allowed prefix, and it declares no custom property other than `--merlion-tone` and `--merlion-dash`; every `url(#…)` names an id defined in the same SVG |
| `stylesheet` | No panic; limits return `E013`; `compile(to_css(compile(x))) == compile(x)`; every emitted selector matches an allowed shape and every value the token's grammar |
| `lsp_message` | No panic; oversized or deeply nested messages are rejected before allocating for the body |
| `markdown_fences` | No panic; block boundaries round-trip |

## Reporting

TODO(owner): the security contact and disclosure policy, published as `SECURITY.md` at the repository root before the first release.
