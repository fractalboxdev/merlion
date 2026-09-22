// Sequence support for @fractalbox/merlion-view/interact (specs/sequence.md#interaction).
// `interact` imports this on demand, for an SVG carrying `merlion-sequence` only, so a page of
// flowcharts never fetches it (specs/viewer.md#constraints).
//
// Participants are nodes and messages are edges, so the highlight set needs no rule of its own.
// What a sequence adds, all of it by filling in the model `interact` already keeps: an activation
// bar is a group outside its participant's and joins the elements that participant lights; the
// keyboard walks the messages after the participants; a message carries the outline line that
// numbers it, which the popover heads with and the live region reads; and a fragment or box lights
// the rows or columns its rect draws around. The functions below are pure; `sequence` is the only
// DOM binding.

// A message's outline line opens with its number and a dot: `1.`, or `1.05.` under an autonumber
// with decimals (specs/sequence.md#text-alternative).
const NUMBERED = /^\s*\d+(?:\.\d+)?\. /;

/**
 * The outline's message lines, in source order, indexed by `data-merlion-index`. An outline that
 * numbers a different count of lines than the diagram draws messages yields none: `accDescr`
 * replaces the outline with prose, and announcing the wrong message is worse than announcing the
 * reconstructed one.
 */
export const messageLines = (desc, count) => {
  const L = desc.filter((l) => NUMBERED.test(l)).map((l) => l.trim());
  return L.length === count ? L : [];
};

/** Each message's outline line in edge order; `idx` holds the edges' `data-merlion-index` values. */
export const lineOf = (L, idx) => idx.map((k) => L[k]);

/**
 * The keyboard targets a sequence appends to the walk (specs/interaction.md#keyboard-and-screen-readers):
 * its messages, in the order the outline numbers them, after the participants the walk already
 * holds in declaration order. Draw order never decides the walk.
 */
export const messageTargets = (idx) =>
  idx
    .map((k, e) => [k, e])
    .sort((a, b) => a[0] - b[0])
    .map(([, e]) => ({ e }));

/**
 * The element list each activation bar joins: its participant's, so a bar lights and dims with the
 * participant it belongs to. A bar naming no participant joins nothing.
 */
export const barOwners = (ids, byId) => ids.map((id) => byId.get(id)?.els);

/**
 * What a fragment or a box lights: exactly what its rect encloses (specs/sequence.md#interaction).
 * `data-merlion-span` gives the first and last row a fragment spans, as message indices, and the
 * first and last column a box spans, as participant positions. A fragment lights the messages in
 * its rows and the participants they name; a box lights its columns and every message with both
 * ends among them. `idx` holds the edges' `data-merlion-index` values, in edge order.
 */
export const spanSet = (box, lo, hi, nodes, edges, idx) => {
  if (box) {
    const ids = nodes.slice(lo, hi + 1).map((n) => n.id);
    const held = new Set(ids);
    return { nodes: ids, edges: edges.flatMap((e, i) => (held.has(e.from) && held.has(e.to) ? i : [])) };
  }
  const es = edges.flatMap((e, i) => (idx[i] >= lo && idx[i] <= hi ? i : []));
  return { nodes: [...new Set(es.flatMap((i) => [edges[i].from, edges[i].to]))], edges: es };
};

// "1 message", "4 messages": the popover says what a group holds.
const count = (n, w) => `${n} ${w}${n === 1 ? "" : "s"}`;

// An activation bar dims with the rest and thickens with nothing: it is a filled shape, and the
// participant it belongs to already carries the highlight. A pinned fragment or box carries the
// accent on its own rect, the only mark a cluster can take: clusters never dim.
const CSS = `[data-merlion-state][data-merlion-interactive] .merlion-activation:not(.merlion-lit){opacity:var(--merlion-dim-opacity,.2)}
[data-merlion-interactive] .merlion-cluster.merlion-primary>.merlion-cluster-box{stroke:var(--merlion-highlight,var(--merlion-accent,#0969da));stroke-width:var(--merlion-highlight-stroke,2.5px)}
@media (prefers-reduced-motion:no-preference){[data-merlion-interactive] .merlion-activation{transition:opacity .15s}}`;

let styled = false;

/**
 * Fills in one SVG's model: the activation bars, each message's outline line, the messages at the
 * end of the keyboard's walk, and a lit set for every fragment and box. `text` reads an element's
 * label as the outline writes it and `style` is the base element's `MerlionView.style`, both passed
 * in so this module loads without a DOM and its rules reach the sheet once per page.
 */
export const sequence = ({ svg, nodes, edges, cls, desc, walk, groups, text, style }) => {
  if (!styled) (styled = true), style(CSS);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const acts = [...svg.querySelectorAll(".merlion-activation")];
  barOwners(
    acts.map((g) => g.dataset.merlionId),
    byId,
  ).forEach((els, i) => els?.push(acts[i]));
  const idx = edges.map((e) => +e.el.dataset.merlionIndex);
  lineOf(messageLines(desc, edges.length), idx).forEach((l, i) => (edges[i].line = l));
  walk.push(...messageTargets(idx));
  // A fragment holding no message spans no row and stays out: clicking its title lights nothing.
  for (const c of cls) {
    const [lo, hi] = (c.g.dataset.merlionSpan ?? "").split(" ").map(Number);
    if (!c.id || !(hi >= lo)) continue;
    const s = spanSet(c.g.classList.contains("merlion-box"), lo, hi, nodes, edges, idx);
    groups.set(c.id, {
      ...s,
      g: c.g,
      // The kind word and the label beside it, which is how the outline heads the fragment's rows.
      line: [".merlion-cluster-title", ".merlion-fragment-label"]
        .map((q) => text(c.g.querySelector(q)))
        .filter(Boolean)
        .join(" "),
      sub: `${count(s.nodes.length, "participant")}, ${count(s.edges.length, "message")}`,
    });
  }
};
