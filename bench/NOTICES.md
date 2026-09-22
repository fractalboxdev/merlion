# Third-party notices: benchmark corpora

## mermaid (`corpus/compat/`, `corpus/edits/`)

`corpus/compat/*.mmd` are diagrams extracted verbatim (dedented) from the
mermaid repository at tag `mermaid@12.0.0`, commit
`98a0945418c76238f15df2afaddbba4272656c3b`: the demo pages (`demos/*.html`),
the flowchart syntax documentation
(`packages/mermaid/src/docs/syntax/flowchart.md`) and the flowchart end-to-end
tests (`e2e/rendering/flowchart/*.spec.*`, `e2e/diagrams/flowchart/**/*.mmd`).
`corpus/compat/manifest.json` records each diagram's source path, the source
file's git blob, and the diagram's sha256. `corpus/edits/*.json` are derived
from those diagrams by fixed text edits (`src/corpus/edits.ts`).

https://github.com/mermaid-js/mermaid

```
The MIT License (MIT)

Copyright (c) 2014 - 2022 Knut Sveidqvist

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Baselines

mermaid 12.0.0 (MIT) and the ELK layout it bundles (elkjs, `EPL-2.0 OR
GPL-3.0-or-later`) are development dependencies installed from npm and run in
headless Chromium through Playwright (Apache-2.0). They are run, never
vendored or modified (`specs/licensing.md`, rule 3).
