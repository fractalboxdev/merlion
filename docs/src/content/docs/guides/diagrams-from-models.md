---
title: Diagrams from models
description: A model asked for a diagram should write Mermaid, not SVG. Mermaid asks it for the graph alone; SVG also asks for geometry it cannot measure — which is why a small model needs this most. Merlion renders the result.
---

Ask a model for a diagram and it can answer in two ways. In Mermaid it writes the graph: which nodes exist, which edges connect them, what each is called. In SVG it writes the graph *and the geometry* — every coordinate, every text width, every arrow route.

The second job is the one models are bad at, and not for want of capability. Text advance depends on the font the reader's browser resolves; nothing a model can compute at generation time tells it how wide `Assign a third reviewer as tie-breaker` will be at 14 px in Inter. A layout engine measures it. A model estimates it, and a label that overruns its box is unreadable no matter how good the prose that produced it.

Nor can the model check its own estimate. Asked to name the digit an SVG draws, from the source alone, GPT-4o is right 13% of the time — chance is 10% ([SGP-Bench, ICLR 2025](https://arxiv.org/abs/2408.08313)). Writing SVG means placing geometry blind and having no way to look.

It shows. A code model prompted for a diagram in SVG keeps 44.6% of its labels inside their boxes; training it on more correct SVG moves that number nowhere, because the constraint being broken appears nowhere in the text it is imitating ([GeoSVG-RL](https://arxiv.org/abs/2605.25447)). Only rendering the drawing and measuring it finds the problem — which is what a layout engine does before it draws anything.

Merlion takes the Mermaid and does the rest: layout, text measurement, routing, theming, accessibility metadata.

```mermaid
flowchart LR
  accTitle: What each output format asks a model for
  prompt[Description in prose] --> choice{Output format}
  choice -->|Mermaid| graph["Graph only:<br/>nodes, edges, labels"]
  choice -->|SVG| both["Graph **and** geometry:<br/>coordinates, text widths, routes"]
  graph --> merlion[Merlion]
  merlion --> svg1[Readable SVG]
  both --> svg2[SVG the model laid out]
  class prompt input
  class choice warn
  class merlion accent
  class svg1 ok
  class svg2 warn
```

## Two properties Mermaid has and SVG does not

**It is cheap.** A Mermaid flowchart is one line per node and one per edge. The same diagram in SVG carries a path, a rectangle, a text element and their coordinates for each of those — about nine times the source for both models measured, and thirteen times the output tokens where the provider counts a single completion. That is paid for, and waited for, on every generation.

**It is checkable.** A Mermaid source *states* its graph, so a parser recovers exactly what the model declared and a program can compare it against what was asked for, before anything is drawn. An SVG states coordinates; its graph has to be inferred back out of geometry — a painted shape holding a label is probably a node, a stroke between two shapes is probably an edge — and "probably" is as good as that check gets.

Mermaid is the stricter format, and that is the point rather than the cost. A model's SVG nearly always parses, because SVG has no grammar to violate — and an SVG that parses can still be unreadable, with nothing to say so. A model's Mermaid sometimes breaks the grammar, loudly, at parse time, where a diagnostic can go back to the model.

That difference decides whether a repair loop works. [VisPlotBench](https://arxiv.org/abs/2510.23642) (ICLR 2026) runs the same tasks in both formats and feeds the renderer's output back to the model: GPT-4.1's Mermaid goes from 68.7% to 93.9%, its SVG from 95.4% to 96.9%. SVG starts higher and stays there — not because it is better, but because running it reveals almost nothing about what is wrong with it.

Merlion's [`authoring` benchmark](/reference/specs/benchmark/#authoring) measures both properties, along with fidelity to the requested graph and whether the labels fit, over 18 diagrams asked of the same model in each format.

## Small models need this most

The two jobs have different difficulty curves. Stating a graph gets longer as the diagram grows and never gets qualitatively harder — naming six boxes and six arrows is reading comprehension, and a small model does it. Placing geometry has a floor: size text you cannot measure, keep hundreds of coordinates consistent, route arrows to the right boundaries, hold the order that decides what covers what. Below that floor the output is not a worse drawing, it is not a drawing.

| Writing SVG | Labels inside their box | Renders at all | Arrows anchored |
|---|---|---|---|
| A 7B code model, prompted ([GeoSVG-RL](https://arxiv.org/abs/2605.25447)) | 44.6% | 72.4% | 31.7% |
| Gemma 4 31B, on a laptop ([this benchmark](/reference/specs/benchmark/#authoring)) | 90.5% | 100% | 98.5% |
| A frontier model | 99.6% | 100% | 100% |

Over the same 18 diagrams, dropping from the frontier model to the local one costs 0.088 of edge fidelity in Mermaid and 0.130 in SVG, and takes label overflow from 0.4% to 9.5% while leaving it at zero in Mermaid. The two failures differ in kind: the Mermaid losses are syntax, which the parser names and repairs, and the SVG losses are geometry, which nothing reports. The format is worth **more** the less capable the model: a frontier model writing SVG pays in size, and a small one pays in usability.

## Repairing what a model writes

Models make small syntax mistakes. Merlion's parser is [error-tolerant by design](/guides/diagnostics/): it reports a diagnostic, repairs what it can, and renders the rest rather than failing the whole diagram.

```sh
merlion check diagram.mmd            # what is wrong, and what the parser would repair
merlion check diagram.mmd --fix      # apply the repairs to the file
merlion render diagram.mmd --json    # {svg, outline, diagnostics, fuel_used, error}
```

`--json` is the shape to use in a pipeline: `diagnostics` tells the calling program what the model got wrong, so it can feed the message back and ask for a correction, and `outline` is the graph Merlion read. That outline is the verification surface — compare it against the graph you asked for and the check is exact, with nothing rendered and nobody looking:

```sh
$ merlion outline diagram.mmd
Flowchart, top to bottom. 3 nodes, 2 edges.
Push → Build
Build → Notify author [fail]
```

## When SVG is the right answer

A model should write SVG when the picture is not a graph: an illustration, an icon, a chart with a specific visual design, anything where the point is the drawing itself rather than the structure behind it. Mermaid has nothing to say about those. The argument here is narrower — for a flowchart, a sequence diagram or a state machine, the model should write the graph and let a renderer place it.
