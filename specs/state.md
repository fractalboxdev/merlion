# State diagrams

`stateDiagram` and `stateDiagram-v2` are graph-shaped ([architecture.md](architecture.md#pipeline)): the model lowers to a flowchart-shaped graph and runs the seven phases of [layout.md](layout.md), so a state machine gets dominator layering, crossing minimisation, container fit, orthogonal routing, clusters and stable layout without a layout module of its own. This spec states the accepted syntax, the model, the lowering, the SVG contract and the diagnostics; it extends [parser.md](parser.md), [layout.md](layout.md), [svg-output.md](svg-output.md) and [interaction.md](interaction.md) rather than restating them.

Both headers parse to the same model and draw the same SVG. mermaid keeps two renderers and points `stateDiagram` at the older one; the difference is appearance, not meaning, and Merlion has one appearance.

Compatibility target: mermaid 12.0.0's state grammar (`packages/mermaid/src/diagrams/state/parser/stateDiagram.jison` at tag `mermaid@12.0.0`), measured by the `compat` pass rate ([benchmark.md](benchmark.md)).

## Model

`model::Diagram::State(model::state::StateMachine)`. States form a tree by `parent`: a composite state owns its children, and a composite split by `--` owns concurrency regions that own theirs. Transitions and notes are flat lists indexed by the SVG and the outline.

| Type | Holds |
|---|---|
| `StateMachine` | `meta`, `direction`, `states`, `transitions`, `notes`, `regions`, `class_defs` |
| `State` | `id`, `label`, `kind`, `parent`, `region`, `children`, `direction`, `classes`, `style`, `link`, `implicit`, `span` |
| `StateKind` | `Simple`, `Composite`, `Choice`, `Fork`, `Join`, `Start`, `End` |
| `Transition` | `from`, `to` (state indices), `label`, `span` |
| `Note` | `state`, `placement` (`Before`, `After`), `text`, `span` |
| `Region` | `parent` (the composite), `index` (0-based within it), `states`, `span` |

- `State::id` is the source id after entity decoding, and the id the SVG, the layout hint and `class` statements use. `label` is the description when the source gives one, else the id; `Start`, `End`, `Choice`, `Fork` and `Join` carry an empty label and draw none.
- `State::parent` is the innermost composite state, `None` at the top level. `State::region` is the concurrency region inside that parent, `None` when the parent has none. A state's `children` are its direct members in declaration order, across every region.
- `State::implicit` records a state first named by a transition rather than declared. Declaring a state only to name it is not idiomatic, so this carries **no** diagnostic, as with sequence participants ([sequence.md](sequence.md#diagnostics)).
- `Transition::from` and `to` are indices into `states`; `[*]` resolves to the scope's `Start` or `End` state before the transition is recorded ([Start and end](#start-and-end)).
- `Note::placement` is `Before` for `note left of` and `After` for `note right of`. The names are axis-relative because the note sits on the order axis whatever the direction ([Notes](#notes)).

## Syntax

Keywords are case-insensitive, as in mermaid's lexer. A statement ends at a newline or `;`. mermaid has no `;` rule and reads a trailing one into the id, so `A --> B;` names a state `B;` there; Merlion ends the statement, because a trailing `;` is an editing artefact and never a state name. Entity codes (`#9829;`, `#35;`) decode in every label, as in flowcharts.

### States

```
Still
state "This is a state description" as s2
s2 : This is a state description
state "Some long name" as s3 : The description
```

- A bare id declares a simple state whose label is the id.
- `state "<description>" as <id>`, `state <id> : <description>` and `<id> : <description>` all describe a state. The first description replaces the id the state is named by and every later one adds a line, so the three forms above in that order label `s3` `Some long name` over `The description`, as mermaid's description list does. A description runs to the end of the statement and may contain `<br/>`.
- Text after the state id that the grammar has no place for — `state s2 &lt;&lt;fork&gt;&gt;`, the entity spelling an HTML source carries — is dropped with `W024`, together with the rest of its line, as mermaid's lexer drops it.
- A state named by a transition before any declaration is declared at that point, with its id as its label and `implicit` set.
- `<id>:::<class>` applies a role where the id stands, on either side of a transition ([Styling](#styling)).

### Transitions

```
s1 --> s2
s1 --> s2 : A transition
```

`-->` is the only arrow the grammar defines. Any other dashed or arrow form (`->`, `->>`, `-->>`, `==>`, `-.->`) is read as `-->` with `R018`: they come from flowchart and sequence habits and mean one thing here.

Text after the target without a `:` is `R017`: `s1 --> s2 done` inserts the colon and reads `done` as the label.

### Start and end

`[*]` is the start state when it is a transition's source and the end state when it is its target. Every `[*]` in one scope names the same pair: the top level has one start and one end, and so does each composite state, as mermaid's `docTranslator` resolves them. Their ids are `{scope}_start` and `{scope}_end`, the top level's scope being `root`; a collision with a declared id appends `_` until the id is unused, the `R004` rule.

### Composite states

```
state First {
  [*] --> second
  second --> [*]
}
state "Another Composite" as NamedComposite { … }
NamedComposite: Another Composite
```

A composite state holds any statement a top level holds, nested to `limits.nesting` (64); deeper input is `E010 NestingTooDeep`. `state <id> { … }` and `state "<description>" as <id> { … }` both open one, and a later `<id>: <text>` labels it. The `{` opens the body where it stands or on a later line, so `state <id>` and a `{` on the next line are one statement. A composite with no members lowers to a simple state, so a transition naming it still lands somewhere ([Lowering](#lowering)).

mermaid refuses a transition between members of two different composite states. Merlion draws it: the layout routes an edge across a cluster boundary through a port on it ([layout.md](layout.md#7-clusters-subgraphs)), so the graph the source describes is the graph the reader sees.

### Choice, fork and join

```
state if_state <<choice>>
state fork_state <<fork>>
state join_state <<join>>
```

`<<choice>>`, `<<fork>>` and `<<join>>` set the state's kind; `[[choice]]`, `[[fork]]` and `[[join]]` are the same markers in mermaid's alternate spelling. The id before the marker is required, and the state draws without a label. `<<…>>` naming anything else leaves the state `Simple` with `W025`.

### Concurrency

```
state Active {
  [*] --> NumLockOff
  NumLockOff --> NumLockOn : EvNumLockPressed
  --
  [*] --> CapsLockOff
}
```

`--` inside a composite state splits its body into regions. `k` dividers make `k + 1` regions; an empty one is dropped, and a composite left with fewer than two regions has none and keeps its members directly. Each region has its own start and end states, resolved in the region's own scope. `--` outside a composite state is `R016` and is dropped.

A composite carrying regions lowers to a cluster holding one cluster per region, so it costs two cluster levels rather than one. The layout follows twice the parser's nesting depth for that reason ([architecture.md](architecture.md#boundaries)), and a machine nested to the parser's limit of 64 draws every composite inside the one above it.

### Notes

```
note right of State1
  Important information! You can
  write notes.
end note
note left of State2 : This is the note to the left.
```

`note left of <id>` and `note right of <id>` take either `: <text>` on the same line or lines up to an `end note` line, which mermaid joins into one text with its line breaks kept. A floating note (`note "<text>" as <id>`) draws nothing in mermaid's renderer either, and is dropped with `W024`.

A state takes any number of notes; they stack on the side their placement names, in source order.

### Direction

`direction TB`, `BT`, `RL` and `LR` set `StateMachine::direction` at the top level and `State::direction` inside a composite state. The layout engine takes the diagram's direction and does not yet honour a per-cluster one, exactly as for flowchart subgraphs ([layout.md](layout.md#7-clusters-subgraphs)); the field is recorded for the reader of the model. TODO(owner): decide whether a per-cluster direction is worth a layout phase once the `compat` corpus shows how often a composite sets one.

### Styling

`classDef <name> <declarations>`, `class <id>[,<id>…] <name>`, `<id>:::<name>` and `style <id>[,<id>…] <declarations>` behave exactly as in flowcharts ([svg-output.md](svg-output.md#source-styles-classdef-style-linkstyle)), including the `W010`, `W011`, `W020` and `I030` diagnostics, and are the only source styles a state diagram carries: the grammar has no `linkStyle`, so no transition takes a source style. `classDef default …` defines a class named `default` and styles nothing by itself, as in Merlion's flowcharts; mermaid applies it to every unclassed node.

mermaid documents two limitations — a `classDef` reaches neither a start or end state nor a composite state. Merlion applies a role wherever the source writes one: the classes reach the lowered node or cluster, and the cluster case is the `class <subgraph id>` rule flowcharts already follow.

### Links

`click <id> href "<url>"` and `click <id> "<url>" "<tooltip>"` wrap the state's group in `<a href="…">` under the URL rules of [svg-output.md](svg-output.md#links), with `W013` for a rejected URL. The tooltip string is dropped with `W024`: a tooltip is page JavaScript's, and the SVG carries none.

### Accessibility, title and comments

`accTitle: …`, `accDescr: …`, `accDescr { … }`, front matter and `%%{init}%%` are read by the shared preamble parser ([parser.md](parser.md#front-matter-and-directives)). `%%` and `#` each start a comment that runs to the end of the line, on its own line or after a statement, as mermaid's lexer does.

`hide empty description` and `scale <n> width` are parsed and dropped with `W024`. The first toggles a divider mermaid draws inside a description-less state, which Merlion never draws; the second sets a fixed pixel width, which [Container fit](layout.md#5-container-fit) owns.

## Diagnostics

Codes shared with flowcharts keep their meaning: `E002`, `E004`, `E010`, `E011`, `E012`, `W010`, `W011`, `W012`, `W013`, `W014`, `W016`, `W020`, `I010`, `I011`, `I020`, `I021`, `I022`, `I030`, `I033`, `R003`, `R004`, `R006`. The codes below extend the table in [parser.md](parser.md#codes).

| Code | Severity | Meaning |
|---|---|---|
| `W024` StateStatementIgnored | Warning | `hide empty description`, `scale … width`, a floating note (`note "…" as <id>`), a `click` tooltip or text after a state id dropped |
| `W025` StateKindRejected | Warning | `<<…>>` or `[[…]]` naming something other than `choice`, `fork` or `join`; the state stays `Simple` |
| `R014` StateNotClosed | Repair | `state <id> {` without `}`; closed at end of input |
| `R015` UnmatchedStateEnd | Repair | `}` with no open composite state; dropped |
| `R016` ConcurrencyOutsideState | Repair | `--` outside a composite state; dropped |
| `R017` TransitionTextUnmarked | Repair | A transition with text and no `:` (`s1 --> s2 done`); the `:` is inserted |
| `R018` TransitionArrowRepaired | Repair | An arrow other than `-->`; read as `-->` |

Every repair carries a `fix` that edits the source, as in flowcharts. A transition to an undeclared state is **not** repaired: declaring every state before naming it is not idiomatic, so `R005`'s flowchart reasoning does not hold and the state is declared silently, as mermaid does.

Limits are fields of `options::Limits`: `nodes` bounds states, `edges` bounds transitions, `notes` bounds notes (2,000), `nesting` bounds composite nesting. Exceeding `nodes`, `edges` or `notes` is `E004 TooLarge`; exceeding `nesting` is `E010`.

`notes` is the one limit a state diagram adds. A note is no graph element, so neither `nodes` nor `edges` counts it, and a note box is as large as its text: without its own limit a source of nothing but notes stays one state wide, spends almost no fuel and still grows the SVG past what 2,000 nodes and 4,000 edges can.

## Lowering

`layout::state::lower` turns a `StateMachine` into a `model::Flowchart` plus the maps back to the model, and `layout::state::layout_state` runs `layout::layout_flowchart` over it. The lowered graph is the input format the layered engine already takes; nothing in [layout.md](layout.md) changes for it.

| Model | Lowered as |
|---|---|
| `Simple` state | Node, `Shape::Rect`, label the description |
| `Composite` state with members | Cluster (`Subgraph`), title the description; members lowered inside it |
| `Composite` state with no members | Node, `Shape::Rect`, so a transition naming it lands |
| `Choice` | Node, `Shape::Rhombus`, empty label: a diamond at the minimum node size |
| `Fork`, `Join` | Node, `Shape::Fork`: the 70 × 10 bar, drawn without a label |
| `Start` | Node, `Shape::FilledCircle`: a 14 px disc |
| `End` | Node, `Shape::FramedCircle`: a 20 px ring around a disc |
| `Region` | Cluster nested in its composite's cluster, empty title |
| `Transition` | Edge, `Stroke::Normal`, `arrow_end: Arrow::Arrow`, label the transition text |
| `Note` | Not a graph element: reserved inside its state's extent and placed after layout ([Note placement](#note-placement)) |

### What the lowering guarantees

1. **Ids are injective and stable.** A declared state keeps its source id; `[*]` becomes `{scope}_start` / `{scope}_end`; a clash with a declared id appends `_` until the id is unused. The same source always gives the same ids, so the layout hint matches across renders and `data-merlion-id` names what the source named.
2. **Order is the source's.** `nodes` follow state declaration order and `edges` transition order, so phase 1's entry choice and phase 3's tie-breaks are fixed by the source and an edit moves only what it must ([layout.md](layout.md#stable-layout)).
3. **A parent precedes its children.** `subgraphs` lists a composite before its regions and a region before the composites inside it, which `Clusters::from_chart` requires.
4. **Every node names its innermost cluster.** A state inside a region carries the region, never the composite above it, so the cluster boxes nest as the source nests.
5. **Every edge has two node endpoints.** A transition naming a composite state connects to the cluster, which the graph represents by the cluster's first member — the flowchart rule for a subgraph endpoint ([parser.md](parser.md#compatibility)) — and a transition between a composite and one of its own members is dropped, as it is there.
6. **A note never enters the graph.** It changes one node's measured extent and nothing else, so the layered graph's node count, layer count and crossing count are the same with and without notes.

### Phases and options

Every option of [layout.md](layout.md#options) applies unchanged: `target_width`, `max_aspect`, `direction` (including `auto`), `edge_style`, `node_spacing`, `rank_spacing`, `hint`, `stability` and `fuel`. So does every phase:

| Phase | What it does here |
|---|---|
| 1. Cycle removal | A state machine is a control-flow graph, so it takes the flowchart path: the dominator tree from the virtual root, back-edges reversed, a greedy feedback arc set for what is left ([layout.md](layout.md#1-cycle-removal)). `A --> A` is a self-loop and is routed, not layered. |
| 2. Layer assignment | Longest-path layering from the virtual root, `min_len` 1 on every transition. The start state has no predecessor and sinks to one layer above its successors, so it sits next to the state it enters. |
| 3. Crossing minimisation | All three passes, unchanged. |
| 4. Coordinate assignment | Brandes–Köpf, unchanged. |
| 5. Container fit | All five steps, wraps and layer splits included; a transition crossing a wrap carries `data-merlion-wrap="true"`. |
| 6. Edge routing | `orthogonal` by default; transition labels take the edge-label chip and its placement rules. |
| 7. Clusters | Composite states and concurrency regions are clusters: members occupy a contiguous span per layer, the box pads them by 12 px plus the title height, and a transition across the boundary enters through a port. |
| Stable layout | The hint is read and written, keyed by lowered node id, so `I020`, `I021` and `I022` all reach state diagrams. This is the visible difference from sequences, which write a hint and never read one. |

### Note placement

A note is geometry, not a graph node, so no layout phase changes for it. The lowering inflates its state's measured extent on the order axis — left for `Before`, right for `After` in `TB` / `BT`, above and below in `LR` / `RL` — by `NOTE_GAP + note_w`, and the engine reserves that space like any other node width. After layout, `layout::state::place_notes` splits the laid-out box: the state's own rect keeps its measured size at the far end, and the note box takes `note_w` at the near end, centred across the state's rect.

| Constant | Value | Meaning |
|---|---|---|
| `NOTE_PAD` | (10, 8) px | Padding inside a note box |
| `NOTE_GAP` | 16 px | Distance from the state's rect to the note box |
| `NOTE_WRAP` | 180 px | Wrap width of note text |
| `NOTE_STACK` | 8 px | Gap between two notes on the same state |

Because the space comes out of the node's own extent, a note box overlaps no node, no cluster title and no routed edge: the layout already treats that rectangle as occupied. Placing the note on the order axis, never the layer axis, keeps it clear of the ports a transition attaches to, which sit on the layer-axis sides of the node in every direction.

### Fuel

Mandatory: one unit per state, transition, note and region charged by the lowering before it allocates, then the flowchart pipeline's own charges. Draw-time role work is `svg::role_units` over the lowered graph. Exhaustion in a mandatory phase returns `TooLarge`; an optional pass stops and keeps the previous result.

## SVG output

Root `class="merlion merlion-state"`; everything else in [svg-output.md](svg-output.md) — root attributes, theming tokens, the embedded style's prefixes, escaping, the `{id}-…` id rule and the four output guarantees — holds unchanged. `data-merlion-layout` is the flowchart format, `v1;{dir};{layer}:{ids}`, over the lowered node ids.

### Groups and data attributes

States carry the node classes and transitions the edge classes, so the token reset, the role and `classDef` rules, the markers, the CSS hover layer and the viewer's `interact` module apply with no rule of their own.

| Element | Group | Contents |
|---|---|---|
| State | `<g class="merlion-node merlion-state-node" data-merlion-id="{id}" data-merlion-kind="{kind}" data-merlion-rank="{0..15}" id="{id}-n{k}">` | `.merlion-shape`, `<text>` |
| Start / end | the state group plus `merlion-state-start` / `merlion-state-end`, `data-merlion-kind="start"` / `"end"` | `.merlion-shape`, no text |
| Fork / join | the state group plus `merlion-state-bar`, `data-merlion-kind="fork"` / `"join"` | `.merlion-shape`, no text |
| Choice | the state group plus `merlion-state-choice`, `data-merlion-kind="choice"` | `.merlion-shape`, no text |
| Transition | `<g class="merlion-edge merlion-transition" data-merlion-from="{id}" data-merlion-to="{id}" data-merlion-index="{n}" id="{id}-e{k}">` | `.merlion-edge-path`, optional `.merlion-edge-text` with its `.merlion-edge-label-bg` chip |
| Composite state | `<g class="merlion-cluster merlion-composite" data-merlion-id="{id}">` | `.merlion-cluster-box`, `.merlion-cluster-title`, its members nested inside |
| Concurrency region | `<g class="merlion-region" data-merlion-id="{parent id}-r{n}" data-merlion-index="{n}">` | its members, and `.merlion-region-divider` on every region after the first |
| Note | `<g class="merlion-note" data-merlion-id="{state id}" data-merlion-placement="before\|after">` | `.merlion-note-link`, `.merlion-note-box`, `<text>` |

- `k` in `{id}-n{k}` is the lowered node index and in `{id}-e{k}` the transition index, matching [interaction.md](interaction.md#svg-additions).
- `data-merlion-kind` is `simple`, `composite`, `choice`, `fork`, `join`, `start` or `end`, the lower-case name of `StateKind`.
- A region group carries no `.merlion-cluster-box` and no title: UML draws the dashed separator alone, so the divider is the whole mark. One divider is drawn per region after the first, across the gap its region box leaves with its predecessor's, spanning the composite's inner extent on the other axis. The axis is the one the two boxes are apart on, not the one the direction names: a region is an ordinary sibling cluster of its composite, and the layered engine places sibling clusters beside one another on the order axis, which runs across the direction. Two region boxes that overlap on both axes have no boundary, and no divider is drawn.
- Draw order is the flowchart's — clusters, edges, nodes — with the notes last, so a note box covers the cluster tint behind it and nothing covers a note.
- A region group is `merlion-region`, not `merlion-cluster`: it has no title to collapse and no box to mark, so the viewer's cluster gestures never target it.

### Theme tokens

State diagrams introduce one pair of tokens, `--merlion-note-bg` and `--merlion-note-border`, because a note is an annotation and must not read as a state: sharing `--merlion-node-bg` made the two boxes byte-identical in fill and stroke. Every other element reuses an existing token, so `merlion-themes.css`, every palette and every stylesheet theme colour a state machine without a change:

| Element | Token |
|---|---|
| State box, choice diamond | `--merlion-node-bg`, `--merlion-node-border`, `--merlion-node-text` |
| Start and end discs, fork and join bars | `--merlion-fg` fill, `--merlion-node-border` stroke |
| Transition line and its marker | `--merlion-edge` |
| Transition label | `--merlion-fg` over `--merlion-edge-label-bg` |
| Note box | `--merlion-note-bg`, `--merlion-note-border` (bg / warn) |
| Note connector | `--merlion-line`, dashed `4 4` |
| Composite box and title | `--merlion-cluster-bg`, `--merlion-cluster-border` |
| Region divider | `--merlion-cluster-border`, dashed `4 4` |

The marks that draw in ink — start, end, fork and join — take `--merlion-fg` through the element classes above, not through a role, so they stay solid under every theme and no tone tints them.

### Roles and automatic tones

Written roles reach a state diagram through `classDef`, `class` and `:::`: states take node roles (`merlion-c-{name}`) and composite states cluster roles (`merlion-cc-{name}`), exactly as flowchart nodes and subgraphs do. Transitions take none, because the grammar gives a transition no id to name.

State diagrams add no automatic-tone rule. The table in [svg-output.md](svg-output.md#automatic-tones), applied to the shapes the lowering produces, already gives:

| Element | Role |
|---|---|
| `Choice` (`Shape::Rhombus`) | `warn` |
| Top-level composite state number `n` (0-based) | `series-{n mod 8 + 1}` |
| Nested composite state, concurrency region | none |
| `Simple`, `Start`, `End`, `Fork`, `Join`, notes, transitions | none |

So a decision tones like a flowchart decision and a composite state tones like a subgraph, and `--no-auto-tone`, `autoTone: false` and `%%{init: {"merlion": {"autoTone": false}}}%%` turn them off with the same bytes as `auto_tone: false`.

### Text alternative

`<desc>` and the plain-text return carry the outline in the flowchart's format ([svg-output.md](svg-output.md#text-alternative)): a header naming the type, the direction and the counts, then one line per state in declaration order, with its outgoing transitions grouped and separated by `; `, labels in brackets, and the composite path as a heading.

```
State diagram, top to bottom. 9 states, 8 transitions.
start → Draft
Draft → Submitted [submit]
Submitted → Review
Review → Published [approved]; → Draft [rejected]
Review: start → Screening
Review: Screening → Decision
Review: Decision
Published → end
Note right of Draft: Waits for the author
```

- The outline reads the state machine, not the lowered graph: a transition naming a composite state prints that state's name rather than its first member, and a generated `[*]` state prints as `start` or `end` rather than as `root_start`.
- It describes what is drawn, so it lists and counts only the transitions the lowering keeps: a transition between a composite state and a state nested inside it has no two endpoints and is dropped ([Lowering](#what-the-lowering-guarantees)). A final line, `N transition(s) of the M in the source is/are not drawn.`, names how many, so a reader of the text alternative is never sent looking for an edge no sighted reader can find.
- A choice, fork or join prints as its id followed by its kind in parentheses — `if_state (choice)` — since it draws no label and the reader has nothing else to go on.
- A note prints one line per note after the last line of the state it belongs to.
- `accDescr` replaces the outline in `<desc>` and not in the plain-text return, as in flowcharts.

## Interaction

The highlight sets of [interaction.md](interaction.md#highlight-set) carry over with no new rule: a state's incident edges are its transitions, and a transition's endpoints are its two states. Clicking a state lights the state, every transition touching it and the states at their other ends, and dims the rest; clicking a transition lights it and both its states.

- Composite states collapse, like every flowchart cluster: clicking the title hides the members, badges the box `+N` and leaves the box and title ([interaction.md](interaction.md#hide-and-collapse)). A collapsed composite also hides its concurrency regions, because a region's whole mark is the divider between members that are no longer drawn.
- A composite state also **pins what it holds**: Alt + tap on its title, or `Enter` on it in the keyboard walk, lights every state inside it and every transition between two of them, marks its own box with the accent and heads the popover with its outline line and the counts. Alt is the key because a title is text and Shift + click extends the selection, which is never a tap ([interaction.md](interaction.md#gestures)). A composite is the one cluster that both collapses and pins, so a plain tap keeps the collapse gesture the reader already knows.
- Concurrency regions are neither collapsed nor pinned: they carry no title and no box.
- Notes are never dimmed and are not targets, matching the cluster rule and the sequence one ([sequence.md](sequence.md#interaction)). A note belongs to the state it points at: it lights with that state and hides with it, so a hidden state leaves no orphaned note behind.
- Path mode follows transitions in source direction, which walks the reachable states of the machine.
- The keyboard walks the states in declaration order, each composite state ahead of the first state it holds, which is the order the outline lists them. A state that draws no label is announced as the outline names it — `start`, `end`, `if_state (choice)` — never as the generated id `root_start`.
- The CSS hover layer emits the same rules over state and transition ids, and `N = 128` counts states plus transitions.

`interact-state.js` carries all of it and nothing else; `interact.js` imports it for an SVG carrying `merlion-state` and for no other, so a page of flowcharts never fetches it ([interaction.md](interaction.md#loading)). It supplies no highlight rule, no gesture and no popover of its own: it fills in the model the interaction module has already read from the node, edge and cluster attributes the lowered graph writes — each state's name and outline line, the walk, the notes, and a lit set per composite state.

## Testing

- Parser: one fixture per statement form, each asserting the model, plus a repair fixture per `R014`–`R018` whose fix, applied to the source, re-parses without that diagnostic; `[*]` resolving to one start and one end per scope, including inside a composite state and inside a concurrency region.
- Lowering: the six guarantees above, each as its own test — id injectivity over a source that declares `root_start`, declaration order, parents before children, innermost cluster, endpoints of a transition naming a composite, and a graph whose node, layer and crossing counts are equal with and without notes.
- Layout: over the `compat` state diagrams, no label overlaps another element, every member sits inside its composite's box, and every note box sits inside the extent the lowering reserved.
- SVG: `assert_safe` and `assert_well_formed` over every state fixture, the same checks flowcharts pass, extended with the state class names and the draw order above.
- Viewer: the pure model of `interact-state.js` under `node --test` — declaration order read from the core's node ids, the name each kind reads as, the walk with a composite ahead of the first state it holds, the forward scan that gives three concurrency regions three different `Active: start` lines, and the lit set of a composite.
- Determinism: the state fixtures render byte-identically native and through the WASM module in Node, in each font mode — `pnpm bench determinism --corpus state` ([benchmark.md](benchmark.md)) — and flowchart and sequence output stay byte-identical to their recorded digests.
