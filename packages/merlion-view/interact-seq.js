// Sequence support for @fractalboxdev/merlion-view/interact (specs/sequence.md#interaction).
// `interact` imports this on demand, for an SVG carrying `merlion-sequence` only, so a page of
// flowcharts never fetches it (specs/viewer.md#constraints).
//
// Participants are nodes and messages are edges, so the highlight set needs no rule of its own.
// What a sequence adds, all of it by filling in the model `interact` already keeps: an activation
// bar is a group outside its participant's and joins the elements that participant lights; the
// keyboard walks the messages after the participants; and a message carries the outline line that
// numbers it, which the popover heads with and the live region reads. The functions below are
// pure; `sequence` is the only DOM binding.

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

// An activation bar dims with the rest and thickens with nothing: it is a filled shape, and the
// participant it belongs to already carries the highlight. A sequence's fragment and box titles
// name no cluster the viewer can collapse, so they take no pointer cursor.
const CSS = `[data-merlion-state][data-merlion-interactive] .merlion-activation:not(.merlion-lit){opacity:var(--merlion-dim-opacity,.2)}
[data-merlion-interactive] .merlion-sequence .merlion-cluster-title{cursor:default}
@media (prefers-reduced-motion:no-preference){[data-merlion-interactive] .merlion-activation{transition:opacity .15s}}`;

let styled = false;

/**
 * Fills in one SVG's model: the activation bars, each message's outline line, and the messages at
 * the end of the keyboard's walk. `style` is the base element's `MerlionView.style`, passed in so
 * this module loads without a DOM and its rules reach the sheet once per page.
 */
export const sequence = (svg, nodes, edges, desc, walk, style) => {
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
};
