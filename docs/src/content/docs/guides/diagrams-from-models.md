---
title: Diagrams from models
description: A model asked for a diagram should write Mermaid, not SVG. Mermaid asks it for the graph alone; SVG also asks it for geometry it cannot measure. Merlion renders the result.
---

Ask a model for a diagram and it can answer in two ways. In Mermaid it writes the graph: which nodes exist, which edges connect them, what each is called. In SVG it writes the graph *and the geometry* — every coordinate, every text width, every arrow route.

The second job is the one models are bad at, and not for want of capability. Text advance depends on the font the reader's browser resolves; nothing a model can compute at generation time tells it how wide `Assign a third reviewer as tie-breaker` will be at 14 px in Inter. A layout engine measures it. A model estimates it, and a label that overruns its box is unreadable no matter how good the prose that produced it.

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

**It is cheap.** A Mermaid flowchart is one line per node and one per edge. The same diagram in SVG carries a path, a rectangle, a text element and their coordinates for each of those, and every one of those is output tokens — paid for, and waited for, on every generation.

**It is checkable.** A Mermaid source *states* its graph, so a parser recovers exactly what the model declared and a program can compare it against what was asked for, before anything is drawn. An SVG states coordinates; its graph has to be inferred back out of geometry — a painted shape holding a label is probably a node, a stroke between two shapes is probably an edge — and "probably" is as good as that check gets. This is why a model can be held to a Mermaid diagram in a way it cannot be held to an SVG one: the failure is visible without rendering it and looking.

Merlion's [`authoring` benchmark](/reference/specs/benchmark/#authoring) measures both properties, along with fidelity to the requested graph and whether the labels fit, over 18 diagrams asked of the same model in each format.

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
