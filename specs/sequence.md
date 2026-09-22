# Sequence diagrams

`sequenceDiagram` renders as a fixed-geometry diagram ([layout.md](layout.md#fixed-geometry-diagram-types)): participants are columns in source order and messages are rows, so there is no graph layout, no layer assignment and no crossing minimisation. This spec states the accepted syntax, the geometry, the SVG contract and the diagnostics; it extends [parser.md](parser.md), [layout.md](layout.md), [svg-output.md](svg-output.md) and [interaction.md](interaction.md) rather than restating them.

Compatibility target: mermaid 12.0.0's `sequenceDiagram` (`packages/mermaid/src/diagrams/sequence/parser/sequenceDiagram.jison` at tag `mermaid@12.0.0`), measured by the `compat` pass rate ([benchmark.md](benchmark.md)).

## Model

`model::Diagram::Sequence(model::sequence::Sequence)`. The item list is a tree, not mermaid's flat event stream: a fragment owns its sections and each section owns its items, so a section boundary cannot be unbalanced downstream and the nesting depth is bounded once, at parse time.

| Type | Holds |
|---|---|
| `Sequence` | `meta`, `participants`, `boxes`, `items`, `autonumber`, `messages` (the count assigned by the parser) |
| `Participant` | `id`, `label` (the alias when given, else the id), `kind`, `group` (index into `boxes`), `implicit`, `created_by` / `destroyed_by` (message indices), `span` |
| `ParticipantKind` | `Participant`, `Actor`, `Boundary`, `Control`, `Entity`, `Database`, `Collections`, `Queue` |
| `ParticipantBox` | `label`, `color: Option<Color>`, `participants`, `span` |
| `Item` | `Message`, `Note`, `Fragment`, `Activate { participant }`, `Deactivate { participant }` |
| `Message` | `index` (0-based, source order over the whole diagram), `from`, `to`, `label`, `line`, `head`, `tail`, `activate`, `deactivate`, `central`, `wrap`, `span` |
| `MessageLine` | `Solid`, `Dotted` |
| `Head` | `None`, `Filled`, `Open`, `Cross`, `HalfTop`, `HalfBottom`, `StickTop`, `StickBottom` |
| `Central` | `None`, `Target`, `Source`, `Both` |
| `Note` | `placement` (`LeftOf`, `RightOf`, `Over`), `from`, `to` (equal unless the note spans two participants), `text`, `span` |
| `Fragment` | `kind`, `sections`, `span` |
| `FragmentKind` | `Loop`, `Alt`, `Opt`, `Par`, `ParOver`, `Critical`, `Break`, `Rect(Color)` |
| `Section` | `label`, `items`, `span` |
| `Autonumber` | `start`, `step` (both in hundredths, `i64`), `visible` |

`Participant::id` is the source id after entity decoding; `label` is the display text. `Message::index` numbers every message in source order including those inside fragments, and is the number the autonumber badge, `data-merlion-index` and the outline print.

Autonumber values are stored in hundredths of a unit, not as floats: `autonumber 1.05 0.1` is `start: 105, step: 10`. Formatting an integer needs no float rounding, so the badge text is identical on every target ([architecture.md](architecture.md#determinism)).

## Syntax

Keywords are case-insensitive, as in mermaid's lexer. A statement ends at a newline or `;`; `#59;` writes a literal semicolon. Entity codes (`#9829;`, `#35;`) decode in every label, as in flowcharts.

### Participants

```
participant Alice
actor Bob
participant A as Alice
participant API@{ "type": "boundary" }
participant API@{ "type": "boundary", "alias": "Public API" }
actor DB@{ "type": "database" } as User Database
```

- Declaration order is drawing order. A participant a message names before any declaration is declared implicitly, at that point, with its id as its label — mermaid's behaviour and idiomatic in hand-written diagrams, so it carries **no** diagnostic. `Participant::implicit` records it for the model's readers.
- `as <text>` sets the label; the text runs to the end of the line and may contain `<br/>`.
- `@{ … }` is a JSON object parsed by the directive parser ([parser.md](parser.md#front-matter-and-directives)), with the same nesting and string limits. `type` names a participant kind; `alias` sets the label. An external `as` alias wins over an inline `alias`. Any other key, a non-string value, or a `type` outside the eight kinds leaves the participant a plain `Participant` with `W022`.
- `actor X` and `@{ "type": "actor" }` both give `ParticipantKind::Actor`; when both a keyword and a `type` are present, the `type` wins.
- `create participant B` / `create actor D as Donald` declares a participant whose lifeline starts at the next message that names it, recorded as `created_by`. `destroy X` ends `X`'s lifeline at the next message that names it, recorded as `destroyed_by`; mermaid allows destroying either end of that message and creating only its target, and Merlion follows it.

### Messages

`[participant][arrow][participant]: text`, with an optional `+` / `-` activation suffix on the arrow and an optional `()` central-connection marker at either end.

| Written | `line` | `tail` | `head` |
|---|---|---|---|
| `->` / `-->` | Solid / Dotted | None | None |
| `->>` / `-->>` | Solid / Dotted | None | Filled |
| `<<->>` / `<<-->>` | Solid / Dotted | Filled | Filled |
| `-x` / `--x` | Solid / Dotted | None | Cross |
| `-)` / `--)` | Solid / Dotted | None | Open |
| `-\|\` / `--\|\` | Solid / Dotted | None | HalfTop |
| `-\|/` / `--\|/` | Solid / Dotted | None | HalfBottom |
| `-\\` / `--\\` | Solid / Dotted | None | StickTop |
| `-//` / `--//` | Solid / Dotted | None | StickBottom |
| `/\|-` / `/\|--` | Solid / Dotted | HalfTop | None |
| `\\|-` / `\\|--` | Solid / Dotted | HalfBottom | None |
| `//-` / `//--` | Solid / Dotted | StickTop | None |
| `\\\\-` / `\\\\--` | Solid / Dotted | StickBottom | None |

- A dotted line draws `stroke-dasharray: 3 3`; `Open` is mermaid's async arrowhead (two open strokes), `Cross` an ✕ at the end.
- `A->>B: text` with `text` empty draws no label and no chip.
- `:wrap:` and `:nowrap:` directly after the colon set `Message::wrap` — and, in the same position, `Note::wrap` and, opening an `as` alias, `Participant::wrap` — to `Some(true)` / `Some(false)`: `wrap` wraps at `wrap_width` even when the label is short enough, `nowrap` keeps the label on one line whatever its width. Unset, a label wraps at `wrap_width` like every other label.
- `()` marks a central connection: `A->>()B` is `Central::Target`, `A()->>B` is `Central::Source`, `A()->>()B` is `Central::Both`. The message draws as usual and the marked end terminates in a 4 px filled dot on the lifeline (`.merlion-central`) instead of on the activation bar's edge.
- A self-message (`from == to`) draws as a bracket to the right of its own lifeline ([Rows](#rows)).

### Activation

`A->>+B: text` opens an activation on `B` at that row; `A-->>-B: text` closes the innermost open activation on the **sender**, matching mermaid's grammar. `activate X` / `deactivate X` do the same as standalone statements. Activations stack: a participant may hold several at once, each drawn as a bar nested inside the previous one. A `deactivate` with nothing open, and an activation still open at the end of the diagram, both give `W023`; the former is dropped, the latter closes at the last row.

### Notes

```
Note right of John: Text in note
Note left of John: Text
Note over Alice,John: A typical interaction
```

`over` takes one participant or a pair. Text may contain `<br/>`, and the same `:wrap:` / `:nowrap:` annotations a message takes sit directly after the colon and set `Note::wrap`; a space before the keyword leaves it as note text.

### Fragments

```
loop <label> … end
alt <label> … else <label> … end
opt <label> … end
par <label> … and <label> … end
par_over <label> … and <label> … end
critical <label> … option <label> … end
break <label> … end
rect rgb(191, 223, 255) … end
rect … end
```

- Every fragment nests. `else`, `and` and `option` open a new `Section` of the innermost matching fragment; the first section's label is the header label.
- `rect` takes an optional colour in the `rgb()`, `rgba()`, `hsl()` or `hsla()` form, or a CSS named colour, parsed into a typed `Color` by the source-style colour grammar ([svg-output.md](svg-output.md#source-styles-classdef-style-linkstyle)); raw text never reaches the output. Hex colours are unavailable because `#` opens a comment. A named colour is a fixed literal that ignores the theme, so it emits `I030 FixedColour`, and that `rect` takes no automatic tone. `transparent` names a colour like any other and emits `I030`.
- `rect` with no colour draws the theme's cluster tint and emits no `I030`, matching mermaid 12. Whatever follows the keyword and is not a colour is the header label, so `rect the retry window` tints with the theme and labels the fragment.
- `par_over` draws the same box as `par` and is recorded as its own kind; its sections overlap in mermaid's renderer, which Merlion draws as stacked sections in one box. TODO(owner): decide whether `par_over` sections share their rows once the `compat` corpus shows how often it appears.
- Fragments nest at most `limits.nesting` (64) deep; deeper input is `E010 NestingTooDeep`.

### Boxes

```
box Aqua Group Description
  participant A
  participant J
end
box transparent Aqua … end
```

A `box` groups consecutive participant declarations behind one tinted rectangle. The first word is a colour when it parses as a CSS named colour or an `rgb()` / `rgba()` / `hsl()` / `hsla()` function; the rest of the line is the label. `transparent` as the colour lets a label that is itself a colour name through. A box holds participant declarations only; any other statement inside closes nothing and is parsed where it stands. Box colours are source literals, so they emit `I030`.

### Autonumber

`autonumber`, `autonumber <start>`, `autonumber <start> <increment>`, `autonumber off`. `start` and `increment` accept up to two decimals. A second `autonumber` statement replaces the first; `off` clears it. With autonumber on, every message draws a badge with its number, `start + index × step`, and the outline prints the same numbers.

### Accessibility, title and comments

`accTitle: …`, `accDescr: …`, `accDescr { … }`, `title <text>` and the legacy `title: <text>` behave exactly as in flowcharts ([parser.md](parser.md#front-matter-and-directives)); front matter and `%%{init}%%` are read by the shared preamble parser. `%%` starts a comment on its own line; `#` starts one in the remainder of a `participant`, alias or fragment-header line, as mermaid's lexer does.

### Statements dropped

`link <actor>: <label> @ <url>`, `links <actor>: <json>`, `properties <actor>: <json>` and `details <actor>: <json>` are parsed and dropped with `W021`. They exist to drive a popup menu built by page JavaScript, which the SVG never carries ([security.md](security.md#output)); the URLs would otherwise reach the output outside the `click … href` rules.

## Diagnostics

Codes shared with flowcharts keep their meaning: `E002`, `E004`, `E010`, `E011`, `E012`, `W012`, `W014`, `W016`, `I010`, `I011`, `I030`, `I033`. The codes below extend the table in [parser.md](parser.md#codes).

| Code | Severity | Meaning |
|---|---|---|
| `W021` SequenceStatementIgnored | Warning | `link`, `links`, `properties` or `details` dropped |
| `W022` ParticipantTypeRejected | Warning | `@{…}` key unknown, value not a string, or `type` outside the eight kinds; the participant draws plain |
| `W023` ActivationUnbalanced | Warning | `deactivate` or `-` with nothing open (dropped), or an activation still open at the end (closed at the last row) |
| `R009` FragmentNotClosed | Repair | `loop`, `alt`, `opt`, `par`, `critical`, `break`, `rect` or `box` without `end`; closed at end of input |
| `R010` SectionOutsideFragment | Repair | `else`, `and` or `option` with no open fragment; the keyword is dropped and its items stay in the enclosing scope |
| `R011` UnmatchedEnd | Repair | `end` with no open fragment or box; dropped |
| `R012` DestroyWithoutMessage | Repair | `destroy X` with no later message naming `X`; the statement is dropped and `X`'s lifeline runs to the last row |
| `R013` MessageTextUnmarked | Repair | A message line with no `:` before its text (`Alice->>Bob Hello`); the `:` is inserted |

Every repair carries a `fix` that edits the source, as in flowcharts. An implicit participant is **not** repaired: `R005`'s flowchart reasoning (an undeclared edge endpoint often hides a typo) does not hold here, because declaring participants only to name them is not idiomatic in sequence diagrams.

Limits reuse the fields of `options::Limits`: `nodes` bounds participants, `edges` bounds messages, `nesting` bounds fragment depth. Exceeding `nodes` or `edges` is `E004 TooLarge`; exceeding `nesting` is `E010`.

## Layout

`layout::sequence::layout_sequence` computes `geometry::sequence::SequenceGeometry` from the model. It measures labels through `text::layout_label`, draws from the same `Fuel` counter and honours `target_width`, so the shared contracts of [layout.md](layout.md) hold; it runs none of that spec's seven phases.

### Constants

| Constant | Value | Meaning |
|---|---|---|
| `MARGIN` | 8 px | Outer margin, shared with the flowchart pipeline |
| `HEAD_PAD` | (12, 8) px | Padding inside a participant head box |
| `HEAD_MIN` | (80, 32) px | Smallest head box |
| `ACTOR_FIGURE` | 24 × 32 px | Stick figure drawn above the label of an `Actor` |
| `COLUMN_GAP` | 24 px | Smallest gap between two head boxes, from `node_spacing` |
| `COLUMN_GAP_MIN` | 8 px | Smallest gap container fit may shrink to |
| `ROW_GAP` | 12 px | Space above a message label and below its arrow |
| `LABEL_PAD` | (6, 2) px | Padding of a message label over the line it sits on |
| `SELF_HEIGHT` | 34 px | Height of a self-message bracket |
| `SELF_WIDTH` | 40 px | How far a self-message reaches right of its lifeline |
| `ACTIVATION_W` | 10 px | Width of an activation bar |
| `ACTIVATION_NEST` | 5 px | Horizontal offset of each nested activation bar |
| `NOTE_PAD` | (10, 8) px | Padding inside a note box |
| `FRAGMENT_PAD` | 8 px | Space between a fragment box and the content it encloses |
| `FRAGMENT_TAB` | 18 px | Height of the kind tab at a fragment's top-left corner |
| `BOX_PAD` | 8 px | Space between a participant box and the heads it encloses |
| `NUMBER_R` | 9 px | Radius of an autonumber badge |

### Columns

1. Measure every participant label at `wrap_width`; the head box is the label box plus `HEAD_PAD`, at least `HEAD_MIN`, plus `ACTOR_FIGURE.1 + 4` of height for an `Actor`.
2. Lay the columns out left to right in source order with `COLUMN_GAP` between adjacent head boxes. Participant order is the source's, always; there is no reordering step and therefore no crossing minimisation.
3. Widen gaps for content that must fit between columns. Each message between columns `a < b` requires `label_width + 2 × LABEL_PAD.0` across the gaps `a..b`, each note `over A,B` requires its box width the same way, and a self-message requires `SELF_WIDTH + label_width` to the right of its own column. A requirement short of the span is discarded; a deficit is spread equally over the gaps it spans, in increasing order of `(b − a, a, message index)`, so the result is independent of iteration order.
4. A box adds `BOX_PAD` outside the head boxes at each end of its participant run and `BOX_PAD` between its outermost members and the neighbouring column.

### Rows

A row cursor starts below the tallest head box plus `ROW_GAP` and advances per item, in item order:

| Item | Height |
|---|---|
| Message between two columns | `label_height + 2 × ROW_GAP`, at least `2 × ROW_GAP + line_height` |
| Self-message | `max(SELF_HEIGHT, label_height) + 2 × ROW_GAP` |
| Note | `note_box_height + ROW_GAP` |
| Fragment | `FRAGMENT_TAB + header_label_height + FRAGMENT_PAD` before its first section, `FRAGMENT_PAD` after its last, and `section_label_height + ROW_GAP` for each divider between sections |
| `Activate` / `Deactivate` | 0; the bar's end sits at the current cursor |

The arrow of a message row sits at the row's bottom, its label centred above it. A message's endpoints are the activation-bar edges of its participants when a bar is open there, else the lifelines. A message that `create`s its target ends at the near edge of the head box it opens, since that box sits on this very row and the lifeline starts below it.

### Activations, lifelines, create and destroy

- An activation bar runs from the row of the message or `activate` that opened it to the row of the message or `deactivate` that closed it, plus `ROW_GAP / 2`. Nesting depth `d` shifts the bar right by `d × ACTIVATION_NEST` and the message endpoints with it.
- A lifeline runs from the bottom of the head box to the top of the foot box, dashed, under everything else.
- `created_by` starts the head box at the creating message's row instead of at the top, and the lifeline below it. `destroyed_by` ends the lifeline at the destroying message's row and draws a ✕ there instead of a foot box.
- Foot boxes mirror the head boxes at the bottom for every participant that is not destroyed, as mermaid draws them with its default `mirrorActors: true`.

### Fragments and container fit

A fragment box spans from the leftmost to the rightmost column its items touch, padded by `FRAGMENT_PAD` and never narrower than its header label plus the tab. A nested fragment insets `FRAGMENT_PAD` inside its parent. Sections are separated by a dashed divider carrying the section label at its left.

Container fit follows [layout.md](layout.md#5-container-fit) in spirit and in this order:

1. Shrink every column gap proportionally toward `COLUMN_GAP_MIN`, keeping the requirements of step 3 above satisfied.
2. Reduce the label wrap width in steps of 20 px down to 120 px and lay out again, charging optional fuel for each re-measurement. The widest wrap width that brings the drawing to `target_width` wins.
3. A diagram that no wrap width brings to `target_width` keeps the layout at the full wrap width: wrapping a label buys a fit the diagram never reaches, and costs a line of text and the height it adds. The result stays wider than `target_width`; `<merlion-view>` zooms it ([viewer.md](viewer.md)).

Height is never fitted: a sequence diagram scrolls.

### Fuel

Mandatory: one unit per participant, per item, per fragment section and per byte of label text measured. Optional: one unit per (message, spanned gap) pair in step 3 of [Columns](#columns) and one per byte re-measured during container fit. Exhaustion in a mandatory phase returns `TooLarge`; an optional pass stops and keeps the previous result.

### Layout hint

Participant order is the source's, so a hint cannot change a sequence's geometry. The render writes `data-merlion-layout="v1;SEQ;0:{participants}"` — the encoded participant ids in source order — so the attribute is present, the id hash stays defined over the drawn layout, and a re-render hinted with its own previous SVG reproduces it byte for byte. An input hint is never read, and no `I020`, `I021` or `I022` is emitted for a sequence.

## SVG output

Root `class="merlion merlion-sequence"`; everything else in [svg-output.md](svg-output.md) — root attributes, theming tokens, the embedded style's prefixes, escaping, the `{id}-…` id rule and the four output guarantees — holds unchanged.

### Groups and data attributes

Participants carry the node classes and messages the edge classes, so the token reset, the role and `classDef` rules, the markers, the CSS hover layer and the viewer's `interact` module all apply to sequences with no change of their own ([Interaction](#interaction)).

| Element | Group | Contents |
|---|---|---|
| Participant | `<g class="merlion-node merlion-participant" data-merlion-id="{id}" data-merlion-kind="{kind}" data-merlion-rank="0" id="{id}-n{k}">` | `.merlion-lifeline` line, `.merlion-shape` head box or actor figure, `<text>`, mirrored `.merlion-shape.merlion-participant-foot` |
| Message | `<g class="merlion-edge merlion-message" data-merlion-from="{id}" data-merlion-to="{id}" data-merlion-index="{n}" id="{id}-e{k}">` | `.merlion-edge-path`, optional `.merlion-edge-text` with its `.merlion-edge-label-bg` chip, optional `.merlion-message-number` badge, optional `.merlion-central` dot |
| Note | `<g class="merlion-note" data-merlion-from="{id}" data-merlion-to="{id}" data-merlion-placement="over\|left\|right">` | `.merlion-note-box`, `<text>` |
| Fragment | `<g class="merlion-cluster merlion-fragment" data-merlion-kind="{kind}" data-merlion-index="{n}">` | `.merlion-cluster-box`, `.merlion-fragment-tab`, `.merlion-cluster-title` (the kind word), `.merlion-fragment-label`, one `.merlion-fragment-divider` and `.merlion-fragment-section` text per later section |
| Activation | `<g class="merlion-activation" data-merlion-id="{id}" data-merlion-depth="{d}">` | `.merlion-shape` bar |
| Box | `<g class="merlion-cluster merlion-box" data-merlion-index="{n}">` | `.merlion-cluster-box`, `.merlion-cluster-title` |

- `k` in `{id}-n{k}` is the participant's 0-based declaration index and in `{id}-e{k}` the message's index, matching [interaction.md](interaction.md#svg-additions).
- Draw order: boxes, fragments, columns, activations, notes, messages. Later elements paint over earlier ones, so a message's label chip covers the fragment box behind it. A fragment box is filled, so it precedes the columns it encloses: a lifeline stays visible inside a fragment, and the head box of a participant `create`d inside one is not painted over.
- `data-merlion-back` and `data-merlion-wrap` never appear: sequences reverse nothing and wrap nothing.

### Theme tokens

Sequences introduce no token. Each element reuses an existing one, so `merlion-themes.css`, every palette and every stylesheet theme colours sequences without a change:

| Element | Token |
|---|---|
| Head box, foot box, activation bar | `--merlion-node-bg`, `--merlion-node-border`, `--merlion-node-text` |
| Lifeline | `--merlion-line`, dashed `4 4` |
| Message line and its markers | `--merlion-edge` |
| Message label, autonumber badge text | `--merlion-fg` over `--merlion-edge-label-bg` |
| Note box | `--merlion-surface`, `--merlion-border` |
| Fragment box, box behind a participant group | `--merlion-cluster-bg`, `--merlion-cluster-border` |
| Fragment tab and section labels | `--merlion-muted` |

### Roles and automatic tones

Written roles reach a sequence through the built-in role names alone: mermaid's sequence grammar has no `class`, `classDef` or `:::`, so a sequence carries no source styles and emits no `classDef` rules. Participants take node roles (`merlion-c-{name}`) and fragments cluster roles (`merlion-cc-{name}`). The built-in tone roles `accent`, `ok`, `warn`, `danger`, `muted` and `store` therefore apply to clusters as well as nodes, at the cluster mix (8%), which extends the table in [svg-output.md](svg-output.md#built-in-roles) and changes nothing for flowcharts, where no cluster carries those names automatically.

With `auto_tone` on (the default), elements take these roles, each as the role class plus `merlion-auto`:

| Element | Role |
|---|---|
| Participant of kind `Actor` | `accent` |
| Participant of kind `Database`, `Collections` or `Queue` | `store` |
| Participant of kind `Participant`, `Boundary`, `Control` or `Entity` | none |
| `loop` | `series-1` |
| `par`, `par_over` | `series-2` |
| `alt` | `warn` |
| `opt` | `muted` |
| `critical`, `break` | `danger` |
| `rect`, `box`, notes, messages, activations | none |

`--no-auto-tone`, `autoTone: false` and `%%{init: {"merlion": {"autoTone": false}}}%%` turn them off exactly as for flowcharts, and the bytes then match a render with `auto_tone: false`. A `rect` or `box` colour is a source literal and masks nothing, so it emits `I030` and never `I033`.

### Text alternative

`<desc>` and the plain-text return carry the outline: a header, the participant list, then the messages in order, numbered by `Message::index + 1` (or by the autonumber sequence when one is set), indented two spaces per fragment level under a heading per fragment and per section.

```
Sequence diagram. 4 participants, 7 messages.
Participants: Customer, Web app (Web), API gateway (API), Bank.
1. Customer → Web app: Place order
2. Web app → API gateway: POST /orders
loop Every minute:
  3. API gateway → Bank: Authorise payment
  4. Bank --> API gateway: Approved
alt is sick:
  5. Bank --> Web app: Declined
else is well:
  6. Web app --> Customer: Order confirmed
Note over Customer,Bank: One order, one transaction
```

- A participant prints as its label, with `(id)` appended when the label differs from the id.
- The arrow glyph follows [interaction.md](interaction.md#text-reconstruction): `→`, `←`, `↔`, `—`, with `-->` written as `-->` where the line is dotted. Self-messages print `A → A`.
- `accDescr` replaces the outline in `<desc>` and not in the plain-text return, as in flowcharts.

## Interaction

The highlight sets of [interaction.md](interaction.md#highlight-set) carry over with no new rule: a participant's incident "edges" are the messages naming it, and a message's endpoints are its two participants. Clicking a participant therefore lights its lifeline (inside its group), its head and foot boxes and every message it sends or receives; clicking a message lights the message and both participants.

- The viewer's `interact` module activates on `.merlion-edge` groups carrying `data-merlion-from` and `data-merlion-to`, which every message carries, so `@fractalboxdev/merlion-view/interact` needs no sequence-specific code.
- Path mode follows messages in source direction, which for a sequence walks the call graph the diagram describes.
- The CSS hover layer emits the same rules over participant and message ids, and `N = 128` counts participants plus messages.
- Notes, fragments and boxes are never dimmed and are not targets, matching the cluster rule.

TODO(owner): decide whether a click on a fragment collapses its rows, as a cluster title collapses a cluster, once the viewer's collapse path is exercised on sequences.

## Testing

- Parser: one fixture per statement form, each asserting the model, plus a repair fixture per `R009`–`R013` whose fix, applied to the source, re-parses without that diagnostic.
- Layout: column and row geometry over the `compat` sequence diagrams, asserting that no label overlaps another element and that every message stays inside its fragment box.
- SVG: `assert_safe` and `assert_well_formed` over every sequence fixture, the same checks flowcharts pass, extended with the sequence class names and the draw order above.
- Determinism: the sequence fixtures render byte-identically native and through the WASM module in Node, in each font mode — `pnpm bench determinism --corpus sequence` ([benchmark.md](benchmark.md)), since the `compat` corpus holds no sequence diagram — and flowchart output stays byte-identical to its recorded digests.
