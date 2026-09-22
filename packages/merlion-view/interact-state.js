// State support for @fractalboxdev/merlion-view/interact (specs/state.md#interaction).
// `interact` imports this on demand, for an SVG carrying `merlion-state` only, so a page of
// flowcharts never fetches it (specs/viewer.md#constraints).
//
// States are nodes and transitions are edges, so the highlight set needs no rule of its own.
// What a state machine adds, all of it by filling in the model `interact` already keeps: a
// start, end, choice, fork or join state draws no label and reads as the outline names it; the
// walk follows declaration order, which is the order the outline lists the states, with each
// composite ahead of the first state it holds; a note belongs to the state it points at and
// travels with it; and a composite state lights what it holds. The functions below are pure;
// the default export is the only DOM binding.
import { prefix } from "./interact-model.js";

/**
 * Node indices in declaration order: the core numbers each node group `{id}-n{k}` by the order the
 * parser meets the state (specs/interaction.md#svg-additions). A group it did not number follows,
 * in document order.
 */
export const declOrder = (ids) =>
  ids
    .map((s, i) => [+(/-n(\d+)$/.exec(s)?.[1] ?? Infinity), i])
    .sort((a, b) => a[0] - b[0])
    .map(([, i]) => i);

// A state that draws no label reads as its kind, or as its id and its kind in parentheses,
// which is how the outline prints it (specs/state.md#text-alternative).
const MARK = { start: "start", end: "end" };

/** How the outline names a state: its label, else its id, else the mark its kind draws. */
export const display = (id, kind, name) =>
  MARK[kind] ?? (kind === "choice" || kind === "fork" || kind === "join" ? `${id} (${kind})` : name || id);

/**
 * The keyboard's walk over a state machine: the states in declaration order, each composite
 * ahead of the first state it holds, as the outline heads its members. `seq` holds the states in
 * declaration order with `cl`, their enclosing composites outermost first. A composite the SVG
 * leaves unnamed is no target: nothing can be said about it.
 */
export const walkOf = (seq) => {
  const out = [];
  const seen = new Set();
  for (const n of seq) {
    for (const c of n.cl) if (c && !seen.has(c)) seen.add(c), out.push({ c });
    out.push({ n: n.id });
  }
  return out;
};

// A line names a target when it is the target's outline prefix, or that prefix followed by an
// edge glyph, so a name never matches a longer name's line.
const names = (l, k) => l === k || (l.startsWith(`${k} `) && "→←↔—".includes(l[k.length + 1]));

/**
 * Each target's line in the outline, -1 where it has none. The scan runs forward, one pass over
 * the outline in walk order, because both are in declaration order: three concurrency regions
 * each print `Active: start`, and each takes the next such line rather than all taking the first.
 * A target the outline skips — a state with no transition at all — leaves the scan where it was.
 */
export const assign = (keys, desc) => {
  let p = 1; // Line 0 is the header.
  return keys.map((k) => {
    let i = p;
    while (i < desc.length && !names(desc[i], k)) i++;
    if (i >= desc.length) return -1;
    p = i + 1;
    return i;
  });
};

/** What a composite state lights: the states it holds and the transitions between them. */
export const held = (ids, edges) => {
  const has = new Set(ids);
  return { nodes: ids, edges: edges.flatMap((e, i) => (has.has(e.from) && has.has(e.to) ? i : [])) };
};

// "1 state", "4 states": the popover says what a composite holds.
const count = (n, w) => `${n} ${w}${n === 1 ? "" : "s"}`;

// A collapsed composite hides its concurrency regions: a dashed divider marks a boundary between
// members that are no longer drawn. A pinned composite carries the accent on its own box, the
// only mark a cluster can take, because clusters never dim.
// Both classes are the interaction module's own, so neither rule needs the host's scope.
const CSS = `.merlion-collapsed .merlion-region{display:none}
.merlion-primary>.merlion-cluster-box{stroke:var(--merlion-highlight,var(--merlion-accent,#0969da));stroke-width:var(--merlion-highlight-stroke,2.5px)}`;

let styled = false;

/**
 * Fills in one SVG's model: the name and outline line of every state, the walk in outline order,
 * each note joined to its state, and a lit set for every composite state. `text` reads an
 * element's label as the outline writes it and `style` is the base element's `MerlionView.style`,
 * both passed in so this module loads without a DOM and its rules reach the sheet once per page.
 */
export default ({ svg, nodes, edges, cls, desc, walk, groups, text, style }) => {
  if (!styled) (styled = true), style(CSS);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const byCluster = new Map(cls.map((c) => [c.id, c]));
  for (const c of cls) c.name = text(c.g.querySelector(":scope>.merlion-cluster-title"));

  const seq = declOrder(nodes.map((n) => n.g.id)).map((i) => nodes[i]);
  for (const n of seq) n.name = display(n.id, n.g.dataset.merlionKind, n.name);
  const targets = walkOf(seq);
  const of = (t) => (t.c ? byCluster.get(t.c) : byId.get(t.n));
  const keyOf = (t) => prefix(t.c ? { name: of(t).name, path: of(t).cl.map((k) => byCluster.get(k).name) } : of(t));
  const at = assign(targets.map(keyOf), desc);

  // A composite state lights what it holds and heads the popover with its own outline line. It
  // still collapses on a plain tap; Shift+tap pins the set (specs/interaction.md#gestures).
  targets.forEach((t, i) => {
    const c = of(t);
    c.line = desc[at[i]] ?? keyOf(t);
    if (!t.c) return;
    const s = held([...c.g.querySelectorAll(".merlion-node")].map((g) => g.dataset.merlionId), edges);
    groups.set(t.c, { ...s, ...c, collapse: true, sub: `${count(s.nodes.length, "state")}, ${count(s.edges.length, "transition")}` });
  });
  walk.splice(0, walk.length, ...targets);

  // A note is geometry on its state, not a target of its own: it hides with the state, lights
  // with it and, carrying neither the node nor the edge class, never dims.
  for (const g of svg.querySelectorAll(".merlion-note")) byId.get(g.dataset.merlionId)?.els.push(g);
};
