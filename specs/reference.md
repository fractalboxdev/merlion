# References

Projects the specs cite for context, comparison or future integration. Merlion copies, ports, vendors, runs and depends on none of them. Before any of them is used, it moves to [licensing.md](licensing.md) with its terms (rule 7). Licences were checked against each project's repository or registry entry on 2026-09-22.

| Project | Licence | Why it is cited | Constraint if used later |
|---|---|---|---|
| [Zed](https://github.com/zed-industries/zed) | GPL-3.0-or-later (editor crates) | Target for an upstream rendering integration ([integrations.md](integrations.md#zed)) | Zed may depend on MIT-licensed Merlion; Merlion never copies Zed code |
| [vscode-markdown-mermaid](https://github.com/mjbvz/vscode-markdown-mermaid) | MIT | The incumbent VS Code preview extension ([integrations.md](integrations.md#vs-code-and-cursor-merlion-vscode)) | Copy only with the notice kept |
| [rehype-mermaid](https://github.com/remcohaszing/rehype-mermaid) | MIT | Prior art for build-time rendering in unified ([ADR-0003](adr/0003-prerender-first.md), [research/landscape.md](research/landscape.md)) | Copy only with the notice kept |
| [ELK](https://github.com/eclipse-elk/elk) (Java) | EPL-2.0 | Its design paper is background reading ([research/papers.md](research/papers.md)) | Never copied or ported (licensing rule 3) |
| [D2](https://github.com/terrastruct/d2) | MPL-2.0 | Comparison in [research/landscape.md](research/landscape.md) | Run only (MPL is copyleft per file) |
| TALA (D2's layout engine) | Proprietary | Comparison in [research/landscape.md](research/landscape.md) | Not examined (licensing rule 4) |
| [@panzoom/panzoom](https://github.com/timmywil/panzoom), svg-pan-zoom | MIT, BSD-2-Clause | Existing pan-and-zoom libraries; `<merlion-view>` is written from scratch ([viewer.md](viewer.md)) | Copy only with the notice kept |
| [IBM/MermaidSeqBench-Eval](https://github.com/IBM/MermaidSeqBench-Eval) | Apache-2.0 | Evaluation code published with the `llm` corpus dataset | Copy only with the notice, `NOTICE` contents and a statement of changes |
| [ulab-uiuc/diagram-eval](https://github.com/ulab-uiuc/diagram-eval) (DiagramEval, EMNLP 2025) | MIT | Reference implementation of the round-trip metric, which Merlion implements from the paper ([benchmark.md](benchmark.md)) | Copy only with the notice kept |
| Named colour themes: [Nord](https://github.com/nordtheme/nord), [Catppuccin](https://github.com/catppuccin/catppuccin), [Dracula](https://github.com/dracula/dracula-theme), [Solarized](https://github.com/altercation/solarized), [Tokyo Night](https://github.com/enkia/tokyo-night-vscode-theme) | MIT (each repository, 2026-09-22) | Candidate named themes for `merlion-themes.css` ([svg-output.md](svg-output.md#theming)) | A palette ships with its copyright notice in `THIRD_PARTY_NOTICES.md`; take Tokyo Night from the MIT VS Code original, not the Apache-2.0 Neovim port. `merlion-themes.css` ships original palettes until a palette moves to [licensing.md](licensing.md) |
