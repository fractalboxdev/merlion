# Why a model writes Mermaid, not SVG

*Snapshot as of 2026-09-23.*

Merlion's premise is that a model asked for a diagram should emit a graph and let a renderer place it. This document states the argument, the evidence for it, and the evidence against — and points at [benchmark.md](../benchmark.md#authoring), which measures it rather than asserting it.

## The argument

Mermaid asks a model for the graph: which nodes exist, which edges connect them, what each is called. SVG asks for the graph **and the geometry** — every coordinate, every text width, every arrow route. Three consequences follow, and each one is measurable.

**Geometry needs a measurement the model cannot take, and it cannot check its own answer.** Text advance depends on the font the reader's browser resolves. A model generating SVG has to guess how wide `Assign a third reviewer as tie-breaker` renders at 14 px in Inter, and a label that overruns its box is unreadable whatever the reasoning that produced it. Merlion's core carries font metric tables for exactly this ([text-measurement.md](../text-measurement.md)); a model generating text has nothing equivalent. Worse, it cannot read the drawing back: asked to name the digit an SVG draws, GPT-4o scores 13.0% where chance is 10% (SGP-Bench, ICLR 2025). The guess is unchecked because the checking step is itself the thing models cannot do.

**Geometry is expensive.** A node costs one line of Mermaid and a shape, a label, coordinates and a routed path in SVG. The cost is paid in output tokens on every generation, and output tokens are the slow, billed half of a request.

**Geometry is not checkable by anyone else either.** A Mermaid source *states* its graph: a parser recovers exactly what the author declared, and a program compares that against the graph that was asked for, before anything is drawn. `merlion outline` is that surface, and it is exact. An SVG states coordinates; its graph has to be inferred back out of geometry — a painted shape holding a label is probably a node, a stroke between two shapes is probably an edge. The inference can be made good (`bench/src/svg/generic.ts` recovers every node and edge of Merlion's own drawings) but it is inference, and it only ever reads back what a reader could see, not what the author meant.

The direction of the failure matters as much as the rate. SVG is forgiving markup, so a model's SVG nearly always parses — and an SVG that parses can still be unreadable, with nothing to say so. Mermaid has a grammar, so a model's Mermaid sometimes breaks it, loudly, at parse time, where a diagnostic can be fed back. Mermaid fails early and visibly; SVG fails late and silently.

The third point is the one that compounds. A pipeline that generates diagrams needs to know when a diagram is wrong, and a format whose correctness is decidable by a parser can be corrected in a loop; a format whose correctness needs a renderer, a geometry heuristic or a vision model cannot be corrected with the same confidence. The same reason unit tests beat screenshots for checking generated code.

## What the evidence supports

Every row was read at the source; findings are quoted or taken from a numbered table.

| \ | Source | Venue | Finding | Supports |
|---|---|---|---|---|
| 1 | Z. Qiu, W. Liu, H. Feng, Z. Liu, T. Z. Xiao, K. M. Collins, J. B. Tenenbaum, A. Weller, M. J. Black, B. Schölkopf. *Can Large Language Models Understand Symbolic Graphics Programs?* [arXiv:2408.08313](https://arxiv.org/abs/2408.08313) | ICLR 2025 Spotlight | 1,085 SVG programs and 4,340 questions answered from program text alone. On SGP-MNIST — name the digit an SVG draws, ten categories, so chance is 10% — **GPT-4o scores 13.0%**, GPT-4-turbo 10.6, Qwen-2-70B 11.3, Llama3-70B 10.0: "even the powerful GPT-4o can only achieve an accuracy slightly higher than the chance-level" | A model cannot see what its own SVG draws. There is no feedback loop to catch a label outside its box, because reading the drawing back out of the source is the thing models cannot do |
| 2 | S. Li, Y. Cai, H. Chen, Y. Wang. *GeoSVG-RL: Geometry-Aware Reinforcement Learning for Layout-Constrained Text-to-SVG Diagram Generation.* [arXiv:2605.25447](https://arxiv.org/abs/2605.25447) | arXiv, 2026-05 | "Minor errors such as misaligned connector endpoints, text labels overlapping borders, or complex layouts drifting beyond the canvas boundaries render the resulting SVG files functionally unusable." Trained with GRPO against a browser-backed geometric verifier, their system reaches a Text-In-Box Rate of 83.0 and an Arrow Anchor Accuracy of 78.6 — the best of three compared (VFig 81.8 / 76.6, AutoFigure-Edit 81.6 / 77.4) | The cost of that missing feedback loop, priced. A system built and trained for this task still leaves about one label in six outside its box |
| 3 | S. Chen et al. *SVGenius: Benchmarking LLMs in SVG Understanding, Editing and Generation.* [arXiv:2506.03139](https://arxiv.org/abs/2506.03139) | ACM MM 2025 | 2,377 queries over 22 models, stratified by path count (Easy 2.14 paths, Hard 16.02). Perceptual accuracy from Easy to Hard: GPT-4o 82.72 → 42.22, Claude-3.7-Sonnet 80.25 → 33.33, Gemini-2.0-Flash 77.78 → 31.11. "All model families exhibit systematic performance degradation as SVG complexity increases, indicating fundamental limitations in current approaches" | Geometry is the hard part and it does not come out with scale. Degradation tracks geometric density, which is what a dense diagram is |
| 4 | X. Xing, J. Hu, G. Liang, J. Zhang, D. Xu, Q. Yu. *Empowering LLMs to Understand and Generate Complex Vector Graphics.* [arXiv:2412.11102](https://arxiv.org/abs/2412.11102) | CVPR 2025 | Names two mechanisms: "semantically ambiguous and tokenized representations within LLMs may result in hallucinations in vector primitive predictions", and "LLM training typically lacks modeling and understanding of the rendering sequence of vector paths, which can lead to occlusion between output vector primitives" | Why, not just that. A tokenizer splits coordinates into fragments carrying no geometric meaning, and SVG is painter's-algorithm ordered while a text model holds no representation of what ends up on top |
| 5 | J. A. Rodriguez et al. *StarVector: Generating Scalable Vector Graphics Code from Images and Text.* [arXiv:2312.11556](https://arxiv.org/abs/2312.11556) | arXiv, 2023-12 (rev. 2025-05) | A model purpose-built for SVG generation: "StarVector is constrained by its 16k token context, which is inadequate for complex SVGs", and its training data was filtered to "examples with up to 8,192 tokens". SVG-Stack averages 1,822 ± 1,808 tokens per drawing | Geometry is expensive. One diagram's SVG is a four-figure token count before any prose around it |
| 6 | C. Liang, J. You. *DiagramEval: Evaluating LLM-Generated Diagrams via Graphs.* [arXiv:2510.25761](https://arxiv.org/abs/2510.25761) | EMNLP 2025 | Scores a generated SVG by extracting a graph from it — text to nodes, connections to edges — and aligning that against the intended graph | Geometry is not checkable directly. Checking an SVG means first inferring the graph back out of it, which is what [`bench/src/svg/generic.ts`](../../bench/src/svg/generic.ts) does and what the reader ceiling prices |
| 7 | B. Shbita, F. Ahmed, C. DeLuca. *MermaidSeqBench: An Evaluation Benchmark for NL-to-Mermaid Sequence Diagram Generation.* [arXiv:2511.14967](https://arxiv.org/abs/2511.14967) | arXiv, 2025-11 (rev. 2026-08) | 132 human-verified cases scoring natural language to Mermaid on syntax correctness, activation handling and error handling; "significant capability gaps across models" | Diagram-generation research already treats Mermaid as the model's output format and measures it there. It also sets the size of the syntax problem Merlion's parser repairs ([parser.md](../parser.md)) |

Rows 1 and 2 are the argument in two halves. A model cannot read back what its own SVG draws, so it has no way to notice the drawing is wrong; and the failures that follow are the ones a reader trips over — labels outside their boxes, arrows landing nowhere, ink off the canvas.

GeoSVG-RL is also the closest external check on the `authoring` method, reached independently: its verifier renders in a browser, as [legibility](../benchmark.md#authoring) does, and four of its six reward dimensions are metrics this benchmark scores — render success, canvas fit, text containment, and edge connectivity F1 against the intended graph.

## What the evidence does not support

- **That models cannot draw SVG at all.** They draw simple figures well; SVGenius measures degradation *with complexity*, not failure at the floor, and GeoSVG-RL's render success rates sit above 90%. The claim here is about diagrams dense enough for layout to matter, and about what the failures cost.
- **That Mermaid is the more reliable format on syntax.** It is not, and this cuts against the argument. Raw SVG is forgiving markup with no grammar to violate, so a model's SVG output almost always parses; Mermaid has a grammar and a model's output sometimes breaks it, which is why MermaidSeqBench scores syntax correctness at all and why Merlion's parser repairs rather than rejects. What SVG buys with that parse rate is the wrong kind of success: an SVG that parses can still be unreadable, and nothing says so. Mermaid fails loudly and early, SVG fails silently and late, and a pipeline can act on the first.
- **A measured token ratio between the two formats for the same diagram.** No published source compares them directly. The ratios in the [authoring results](../../bench/results/) are this project's own measurement, over 18 diagrams and the models recorded in the corpus.
- **That constraining a model to a grammar is free.** Grammar-constrained decoding raises validity by construction, but its cost to the quality of what is generated is contested and was not resolved here. Merlion does not constrain decoding; it parses tolerantly after the fact, which sidesteps the question rather than answering it.
- **That Mermaid is the best graph DSL for this.** Graphviz, D2 and others make the same trade. Mermaid is chosen for reach — it is what models already emit and what Markdown renderers already accept — not because it was measured against the alternatives.

## Where SVG is the right answer

The argument is narrow, and stating its boundary is part of stating it. A model should write SVG when the picture is not a graph: an illustration, an icon, a chart with a particular visual design, anything where the drawing itself is the content rather than a view of a structure. Mermaid has nothing to say about those, and a model's direct control of geometry is the point there rather than the problem.

Mermaid's own costs are real too, and Merlion carries most of them:

- **Syntax errors.** Models make small ones. Merlion's parser repairs what it can and renders the rest rather than failing the diagram ([parser.md](../parser.md)); the `llm` corpus measures parse rate and repair rate.
- **Dialect drift.** Mermaid's grammar moves between releases, and a model's training data straddles several. The `compat` corpora pin mermaid 12.0.0 and measure what Merlion accepts against it.
- **Limited expressivity.** A diagram Mermaid cannot state is a diagram this argument does not cover.

## Applied in

| Spec | How |
|---|---|
| [benchmark.md](../benchmark.md#authoring) | The `authoring` corpus: 18 diagrams asked of a model in both formats, scored on cost, validity, fidelity and legibility, against a reader ceiling |
| [parser.md](../parser.md) | Error tolerance and repair, which is the cost of Mermaid being the input format |
| [text-measurement.md](../text-measurement.md) | The measurement a model writing SVG cannot take |
| [integrations.md](../integrations.md#cli) | `--json` and `outline`: diagnostics to feed back, and the graph to check against what was asked for |
