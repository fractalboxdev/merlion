---
title: Diagnostics and repairs
description: Every Merlion message has a severity and a stable code. Common syntax mistakes are repaired with a fix you can apply; --strict turns them into errors.
---

Every render and check returns a list of diagnostics. Each has a severity, a stable code, a 1-based line and column (in Unicode scalar values), UTF-8 byte offsets, a one-line message and, for a repair, the fix that performs it. Messages quote at most 64 characters of source and escape control and bidi characters, so untrusted source never drives a terminal or a CI log.

| Severity | Codes | Effect |
|---|---|---|
| Error | `E0xx` | Nothing renders for this diagram |
| Warning | `W0xx` | Renders; something in the source was dropped or changed |
| Repair | `R0xx` | Renders; the parser fixed a common mistake and records the fix |
| Info | `I0xx` | Renders; a fact worth knowing (a colour that ignores the theme, a discarded hint) |

With `strict`, every warning and repair becomes an error.

## Repairs

The parser recovers from the mistakes people and LLMs make most, applies the repair, and continues:

| Code | Input | Repair |
|---|---|---|
| `R001` | Unquoted label containing `()`, `[]`, `{}` or `:` inside a node shape | Quote the label |
| `R002` | `subgraph` without a matching `end` | Close it at the end of input |
| `R003` | Typographic quotes (`“ ” ‘ ’`) used as delimiters | Replace them with ASCII quotes |
| `R004` | A reserved word as a node id (`end`, `graph`, `subgraph`) | Rename it to `end_` and keep the original text as the label |
| `R005` | An edge to a node id that is never declared | Declare the node with its id as the label |
| `R006` | A Markdown code fence left inside the source | Strip it |

This diagram deliberately carries two mistakes. It renders anyway, with `R001` for the parenthesised label and `R004` for the node named `end`:

```mermaid
flowchart LR
  accTitle: A diagram repaired while parsing
  start[Parse (fast path)] --> check{Valid?}
  check -->|yes| end
  check -->|no| fix[Apply repairs]
  class start input
  class check warn
  class fix accent
```

`merlion check` prints each diagnostic as `file:line:col: severity code message`, and `--fix` writes every repair back to the file (Markdown files included, per ```` ```mermaid ```` block):

```sh
target/release/merlion check diagram.mmd --fix          # apply the parser's repairs to the file
target/release/merlion check docs/*.md --strict         # fail CI on any warning or repair
```

Through WASM, `check(source)` returns the same list, and each repair's `fix` is `{ byteStart, byteEnd, replacement }` for an editor or an LLM loop to apply.

## Codes

| Code | Name | Meaning |
|---|---|---|
| `E001` | InternalError | The WASM instance trapped and was replaced; the call returns this instead of throwing |
| `E002` | SyntaxError | A syntax error no repair covers; the message names the expected tokens |
| `E003` | UnsupportedDiagram | The header names a diagram type Merlion does not render, or there is no header |
| `E004` | TooLarge | Input over its size limit, a structural limit exceeded, or fuel exhausted in a mandatory layout phase |
| `E010` | NestingTooDeep | Subgraphs nested beyond 64 |
| `E011` | FrontMatterUnsupported | YAML outside the accepted subset (anchors, aliases, tags, duplicate keys), or nested beyond 64 |
| `E012` | DirectiveTooLarge | `%%{init}%%` JSON nested beyond 64 or with a string over 4,096 bytes |
| `E013` | StylesheetTooLarge | A stylesheet over one of its [limits](/guides/roles-and-stylesheets/#what-the-subset-accepts) |
| `W010` | StyleRejected | A `style`/`classDef` property or value outside the accepted set |
| `W011` | ClassNameRejected | A class or role name outside `[A-Za-z_][A-Za-z0-9_-]{0,63}` |
| `W012` | LabelTruncated | A label over 4,096 bytes |
| `W013` | LinkRejected | A `click … href` URL outside relative, `https`, `http` and `mailto` |
| `W014` | BidiControlStripped | Bidirectional formatting characters removed from a label |
| `W015` | ShapeUnsupported | An `@{ shape: … }` Merlion draws as a rectangle |
| `W016` | ConfigRejected | A front-matter or `%%{init}%%` key that is unknown, or a value outside its set |
| `W017` | StylesheetRuleRejected | A stylesheet rule under a selector or at-rule outside the subset, or a role left out by the 16 KiB embedded-style cap |
| `W018` | StylesheetDeclarationRejected | A property outside the token list, a font token, or a value outside its grammar |
| `W019` | StylesheetReferenceInvalid | A `var()` naming an undefined token, forming a cycle, or nested deeper than 8 |
| `I010` | UnmeasuredGlyph | A code point outside the font's metric table |
| `I011` | ThemeConfigIgnored | `theme`, `themeVariables` or `look` in the source; themes are CSS ([Theming](/guides/theming/)) |
| `I020` | LayoutHintDiscarded | Fewer than half the nodes survive ([Stable layout](/guides/stable-layout/)) |
| `I021` | LayoutHintPartial | Some nodes are treated as new |
| `I022` | LayoutHintInvalid | The hint is malformed, of an unknown version, or too large |
| `I030` | FixedColour | A source colour that ignores the theme |
| `I031` | ClickCallbackIgnored | A `click` callback or `call` dropped; the output never calls page JavaScript |
| `I032` | StylesheetRulesIgnored | Count of stylesheet rules that declare no `--merlion-*` token |
| `I033` | ToneMasked | A source colour overrides a stylesheet tone on the same element |

A failed render always carries at least one error, so the CLI, the WASM module and the integrations report the same codes. The full table is in [parser.md](/reference/specs/parser/#codes); the limits behind `E004` are in [architecture.md](/reference/specs/architecture/#boundaries).

## Exit codes

| Code | CLI outcome |
|---|---|
| `0` | Every diagram rendered; warnings, repairs and infos allowed |
| `1` | At least one diagram failed to parse or render |
| `2` | Usage error, including a `--theme` or `--auto-dark` name the stylesheet does not define |
| `3` | At least one input exceeds a limit (`E004`, `E013`) and nothing else failed |

## In a build

- **rehype**: each diagnostic becomes a `vfile` message with `ruleId` set to the code, `source: "merlion"`, and the line and column in the Markdown file (fence line plus the diagnostic's line). A block that fails keeps its code block. With `strict`, the file fails after every block is reported.
- **Astro**: the same messages are logged with `file:line:col` during `astro build`; with `strict`, the build fails.
- **WASM**: `render` returns `{ svg, diagnostics, error }` and throws only a `TypeError` for invalid arguments. Try it in the [playground](/playground/): the diagnostics list under the editor jumps to each location.
