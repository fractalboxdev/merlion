# Parser

A hand-written recursive-descent parser per diagram type, with no parser-generator runtime. It accepts Mermaid syntax as mermaid 12.0.0 documents it and recovers from the errors people and LLMs commonly make.

## Diagram types

| Type | Header | Milestone |
|---|---|---|
| Flowchart | `flowchart`, `graph` + `TB`/`TD`/`BT`/`LR`/`RL` | M1 |
| Sequence | `sequenceDiagram` | M4 |
| State | `stateDiagram`, `stateDiagram-v2` | M4 |
| Class | `classDiagram` | M4 |
| ER | `erDiagram` | M4 |
| Gantt, timeline, pie, XY, packet, kanban | `gantt`, `timeline`, `pie`, `xychart-beta`/`xychart`, `packet-beta`/`packet`, `kanban` | M3 |

Any other header returns `UnsupportedDiagram { header }`, never a partial render.

## Compatibility

Compatibility means rendering the same graph that mermaid 12.0.0 renders: the same nodes, edges, labels, clusters and directions. Pixel positions are not part of it. The benchmark reports the pass rate per diagram type against the mermaid repository's own demo and test diagrams ([benchmark.md](benchmark.md)).

mermaid 12's small symbol shapes (`@{ shape: … }` `sm-circ`, `f-circ`, `fr-circ`, `cross-circ`, `fork`, `hourglass`, `bolt` and their aliases) are drawn at a fixed size without their label, as mermaid draws them; the label stays in the model for the text alternative. The other expanded shapes (the document, stacked, cylinder, triangle, brace and process variants, `text` and `datastore`, with their aliases) are drawn with their own outline; `text` draws no outline and `datastore` only the lines above and below the label. `bang`, `cloud`, `folder`, `bucket`, `console`, `browser` and `person` become rectangles with `W015`, as does any other name.

Subgraph ids follow mermaid's resolution, which happens after the whole source is read:

- A node id that names a subgraph stands for the subgraph unless it is given a bracket shape, an `@{…}` `shape` or `label`, or a label by `R004`. So `id@{…}` with other keys (`view`, `algorithm`) configures the subgraph and never creates a node.
- Named inside another subgraph, it nests that subgraph there, even when the subgraph is declared later; a link that would close a nesting cycle is not made. Parents always precede their children in the model.
- As an edge endpoint it connects to the subgraph, which the model represents by the subgraph's first member node. An edge between a subgraph and one of its own members is dropped.

## Front matter and directives

- YAML front matter (`---` … `---`) accepts `title`, `config.flowchart.curve`, `config.layout`, `config.merlion.autoTone`, `accTitle`, `accDescr`.
- The front matter parser accepts a YAML subset: block mappings, plain and quoted scalars, and flow sequences. Anchors, aliases, tags, multi-document streams and duplicate keys are rejected with an `Error` (`E011 FrontMatterUnsupported`), which rules out alias-expansion attacks.
- `%%{init: …}%%` accepts the same keys as JSON. The JSON parser rejects nesting deeper than 64 and strings longer than 4,096 bytes (`E012 DirectiveTooLarge`).
- Every accepted key takes an enumerated or numeric value (`curve` is one of Mermaid's curve names, `layout` is `dagre`, `elk` or `merlion`, all laid out by Merlion's engine); a value outside its set is ignored with `W016`.
- `merlion` holds Merlion's own keys. `merlion.autoTone` is `true` or `false` (a JSON boolean, or the YAML scalar `true` / `false`); `false` turns the automatic tones off for the diagram ([svg-output.md](svg-output.md#automatic-tones)), and `true` leaves the render option in charge. Any other `merlion.*` key, a non-boolean `autoTone` or a `merlion` value that is not a mapping gives `W016`.
- `theme`, `themeVariables` and `look` are accepted and ignored with `I011`, because themes are CSS ([svg-output.md](svg-output.md)).
- Unknown keys and values outside a key's set produce `W016` and are otherwise ignored.
- Neither front matter nor `%%{init}%%` names a stylesheet; only the caller passes one ([integrations.md](integrations.md#cli)).
- `accTitle:` and `accDescr:` statements override the generated `<title>` and `<desc>`.

## Error tolerance

In the default mode, the parser applies each repair below, records it as a `Repair` diagnostic, and continues. With `strict: true`, any repair becomes an `Error` and rendering fails.

| Code | Input | Repair |
|---|---|---|
| `R001` | Unquoted label containing `()`, `[]`, `{}` or `:` inside a node shape | Quote the label |
| `R002` | `subgraph` without a matching `end` | Close it at end of input |
| `R003` | Typographic quotes (`“ ” ‘ ’`) used as delimiters | Replace with ASCII quotes |
| `R004` | A reserved word as a node id (`end`, `graph`, `subgraph`) | Rename to `end_` etc., appending further `_` until the id is unused, and keep the original text as the label |
| `R005` | An edge to a node id that is never declared | Declare the node with its id as the label (Mermaid behaviour; recorded because it often hides a typo) |
| `R006` | Markdown code fence left inside the source | Strip it |
| `R007` | Tabs mixed with spaces in indentation-sensitive types (mindmap, kanban) | Treat each tab as 4 spaces |
| `R008` | An edge id (`e1@-->`) already given to an earlier edge | Drop the id from the later edge; the id keeps naming the first edge |

A syntax error that no rule repairs stops parsing and returns an `Error` diagnostic with its location and the tokens expected at that point.

Style statements (`classDef`, `style`, `linkStyle`) and `click` statements are parsed into typed values and validated as specified in [svg-output.md](svg-output.md#source-styles-classdef-style-linkstyle); the parser never passes their text through. Edge ids (`a e1@--> b`) are kept in the model: `class e1 <name>` gives the edge a role ([svg-output.md](svg-output.md#roles)), and `e1@{…}` still configures it. An edge id names exactly one edge: on a fan-out (`a & b e1@--> c & d`) it names the edge from the last source to the first target, as mermaid does, and a later link reusing it loses it with `R008`. An element keeps at most 32 classes from `class` and `:::`; further classes are dropped with one `W020` per element. TODO(owner): decide what `class` does with an id that names both a node and an edge. Subgraphs nest at most 64 deep (`E010 NestingTooDeep`); the parser tracks depth explicitly, so deep input fails with a diagnostic instead of exhausting the stack.

## Diagnostics

```
Diagnostic {
  severity: Error | Warning | Repair | Info,
  code: &'static str,      // "E0xx", "W0xx", "R0xx", "I0xx"
  span: { line, column, byte_start, byte_end },   // 1-based line; 1-based column in Unicode scalar values; UTF-8 byte offsets
  message: String,
  fix: Option<{ span, replacement }>,             // present for every Repair
}
```

A message is one printable line: every source excerpt it quotes (a token, an id, a style value, the `E003` header) is cut to 64 characters followed by `…`, and C0/C1 controls, tab, newline, bidi controls and non-characters are written as `\u{…}` escapes, so untrusted source never drives a terminal or a CI log. The whole message is capped at 512 characters. The CLI escapes the file name it prints before each message the same way.

The `fix` field lets an editor or an LLM loop apply the repair to the source text. `merlion check --fix` writes every fix back to the file ([integrations.md](integrations.md)). `merlion lsp` converts columns to the position encoding the client negotiates ([integrations.md](integrations.md#language-server-merlion-lsp)).

### Codes

| Code | Severity | Meaning |
|---|---|---|
| `E001` InternalError | Error | The WASM instance trapped and was replaced ([integrations.md](integrations.md#fractalboxdevmerlion-wasm)) |
| `E002` SyntaxError | Error | Syntax error no repair covers; the message names the expected tokens |
| `E003` UnsupportedDiagram | Error | The header names a diagram type Merlion does not render, or there is no header |
| `E004` TooLarge | Error | Input over its size limit, a structural limit exceeded, or mandatory-phase fuel exhausted |
| `E010` NestingTooDeep | Error | Subgraphs nested beyond 64 |
| `E011` FrontMatterUnsupported | Error | YAML outside the accepted subset, or nested beyond 64 |
| `E012` DirectiveTooLarge | Error | `%%{init}%%` JSON nested beyond 64 or with a string over 4,096 bytes |
| `E013` StylesheetTooLarge | Error | Stylesheet over 64 KiB, 512 rules, 32 declarations per rule, 8 selectors per token rule, 16 theme names, 256 role selectors or block depth 2, or compiled output over 64 KiB ([svg-output.md](svg-output.md#stylesheet)) |
| `W010` StyleRejected | Warning | Style property or value outside the accepted set |
| `W011` ClassNameRejected | Warning | `classDef` name outside `[A-Za-z_][A-Za-z0-9_-]{0,63}` |
| `W012` LabelTruncated | Warning | Label longer than 4,096 bytes |
| `W013` LinkRejected | Warning | `click … href` URL outside the accepted schemes |
| `W014` BidiControlStripped | Warning | Bidirectional formatting characters removed from a label |
| `W015` ShapeUnsupported | Warning | `@{ shape: … }` names a shape Merlion draws as a rectangle ([Compatibility](#compatibility)) |
| `W016` ConfigRejected | Warning | Front-matter or `%%{init}%%` key unknown, or its value outside the key's set |
| `W017` StylesheetRuleRejected | Warning | A rule declaring a token under a selector or at-rule outside the subset; or a role left out of the embedded style by the 16 KiB cap |
| `W018` StylesheetDeclarationRejected | Warning | A property outside the token list, a font token, or a value outside the token's grammar |
| `W019` StylesheetReferenceInvalid | Warning | `var()` naming an undefined token, forming a cycle, or nested deeper than 8 |
| `W020` ClassesTruncated | Warning | An element given more than 32 classes; the rest are dropped |
| `I010` UnmeasuredGlyph | Info | Code point outside the font table ([text-measurement.md](text-measurement.md)) |
| `I011` ThemeConfigIgnored | Info | `theme`, `themeVariables` or `look` in front matter or `%%{init}%%` |
| `I020` LayoutHintDiscarded | Info | Fewer than 50% of nodes survive ([layout.md](layout.md#stable-layout)) |
| `I021` LayoutHintPartial | Info | Some nodes treated as new |
| `I022` LayoutHintInvalid | Info | Hint malformed, of unknown version, or too large |
| `I030` FixedColour | Info | Source sets a colour that ignores the theme: a `classDef` colour is fixed unless a stylesheet or page sets its `--merlion-c-{name}-*` token; a `style` or `linkStyle` colour is fixed |
| `I031` ClickCallbackIgnored | Info | `click` callback or `call` dropped |
| `I032` StylesheetRulesIgnored | Info | Count of stylesheet rules declaring no `--merlion-*` token |
| `I033` ToneMasked | Info | A source `style` colour, or a `classDef` colour whose token the stylesheet leaves unset, overrides a stylesheet tone on the same element |
| `R001`–`R008` | Repair | See [Error tolerance](#error-tolerance) |

Under `strict: true`, every `Warning` and `Repair` becomes an `Error`.

A failed render always carries at least one `Error` diagnostic. When no stage recorded one, the core adds `E002`, `E003` or `E004` for its `RenderError`, so the CLI, the WASM module and every other caller report the same codes.
