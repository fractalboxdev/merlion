# Licensing

This spec defines what Merlion takes from each project, dataset and paper it uses, and under which terms. Licences below were checked against each project's repository or registry entry on 2026-09-22. Projects that are cited but not used are in [reference.md](reference.md).

## Merlion's own licence

MIT (`LICENSE` at the repository root; SPDX `MIT` in every `Cargo.toml` and `package.json`). Everything Merlion incorporates is MIT or OFL-1.1, and the benchmark fetches Apache-2.0 data at run time. Each incorporated work keeps its own notice in `THIRD_PARTY_NOTICES.md`.

## Rules

1. **Ideas and algorithms are free to use; code and text carry their licence.** Merlion implements algorithms from papers and from documentation freely. Copying, translating or closely paraphrasing source code creates a derivative work under the original licence, and that includes line-by-line porting to Rust.
2. **Code from permissive licences (MIT, BSD, Apache-2.0) may be ported or copied** only with its copyright notice and licence text kept: in a header comment on each derived file, and in `THIRD_PARTY_NOTICES.md` at the repository root. Apache-2.0 sources also carry their `NOTICE` file contents, if any, and a statement of changes.
3. **Code under copyleft licences (EPL, MPL, GPL) is never copied, ported or used as a reference while writing code.** Their programs may be *run* as benchmark baselines, because running software is not distribution.
4. **Proprietary software is not examined.** Merlion does no reverse engineering and never inspects its output to derive algorithms.
5. **Datasets keep their licence.** Benchmark corpora are downloaded at benchmark time and pinned by hash, not committed, unless their licence allows redistribution and the notice is kept.
6. **Names:** "Mermaid" appears only to describe compatibility ("renders Mermaid syntax"). Merlion's packages, logo and docs don't use Mermaid's branding.
7. **A new use gets a row here first.** Before Merlion copies, ports, vendors, runs or depends on a project listed in [reference.md](reference.md), the project moves to the table below with its terms.

## Projects used

| Project | Licence | Use | Terms |
|---|---|---|---|
| [mermaid](https://github.com/mermaid-js/mermaid) 12.0.0 | MIT (© Knut Sveidqvist) | Syntax implemented from the documentation; demo and test diagrams vendored as the `compat` corpus; benchmark baseline | Vendored diagrams keep the notice in `THIRD_PARTY_NOTICES.md` |
| [MSAGL.js](https://github.com/microsoft/msagljs) (`@msagl/core` 1.1.24) | MIT (© Microsoft Corporation) | Source of the layered layout port ([ADR-0004](adr/0004-layout-implementation.md), Proposed); layout-only benchmark baseline | Each ported file carries "Portions © Microsoft Corporation, MIT"; the notice goes in `THIRD_PARTY_NOTICES.md` |
| [Inter](https://github.com/rsms/inter) | OFL-1.1 (© The Inter Project Authors), no Reserved Font Name | Metric tables and a WOFF2 subset ([text-measurement.md](text-measurement.md)), shipped in the core and in `@fractalbox/merlion-themes` for `merlion-font.css` | Each shipped with the OFL text; not sold on its own |
| [beautiful-mermaid](https://github.com/lukilabs/beautiful-mermaid) 1.1.3 | MIT (© 2026 Craft Docs) | Benchmark baseline. The idea of deriving colours from `bg`/`fg` with `color-mix` (an idea, not code) | Run only; no code copied |
| [mermaid-rs-renderer (mmdr)](https://github.com/1jehuang/mermaid-rs-renderer) | MIT (© 2026 mermaid-rs-renderer contributors) | Benchmark baseline | Run only |
| [merman](https://github.com/Latias94/merman) 0.8.0-alpha.6 | MIT OR Apache-2.0; its optional ELK feature is EPL-2.0 | Benchmark baseline, built with default features | Run only; the ELK feature is never enabled |
| [elkjs](https://github.com/kieler/elkjs) | `EPL-2.0 OR GPL-3.0-or-later` | Runs inside the mermaid-ELK baseline | Run only (rule 3). Contributors don't read ELK source while implementing layout; the algorithms come from the papers ELK itself cites |
| [dagre](https://github.com/dagrejs/dagre) | MIT (© Chris Pettitt) | Runs inside the mermaid-dagre baseline (`layout: dagre`) | Run only |
| [librsvg](https://gitlab.gnome.org/GNOME/librsvg) | LGPL-2.1-or-later | Runs as `rsvg-convert` in the stylesheet parity gate ([benchmark.md](benchmark.md)) | Run only (rule 3); never linked, vendored or read |
| [@types/hast](https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/hast) | MIT | Development dependency of `@fractalbox/merlion-rehype` | Never a runtime dependency ([supply-chain.md](supply-chain.md)) |

## Datasets

| Source | Licence | Use |
|---|---|---|
| [MermaidSeqBench](https://huggingface.co/datasets/ibm-research/MermaidSeqBench) (dataset, arXiv:2511.14967) | Apache-2.0 | Downloaded at benchmark time, pinned by hash; licence and attribution kept in the benchmark report |

## Development tools

| Tool | Licence | Scope |
|---|---|---|
| `ttf-parser` 0.25.1 | MIT OR Apache-2.0 (crates.io, 2026-09-22) | `tools/fontgen` only; never published |
| `cargo-deny`, `cargo-vet`, `cargo-fuzz`, `wasm-opt` (Binaryen, Apache-2.0) | Permissive | CI only |
| Playwright | Apache-2.0 | Runs the mermaid baselines in headless Chromium ([benchmark.md](benchmark.md)) |

## Papers

Algorithms from the papers cited in [layout.md](layout.md) and [benchmark.md](benchmark.md) are implemented from their descriptions and cited in code comments at the implementation site. The DiagramEval round-trip metric is implemented from its paper. No paper text, figures or code are copied into the repository unless the artifact's own licence permits it, in which case the notice is kept as described above.
