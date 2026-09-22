// Interaction logic for @fractalboxdev/merlion-view/interact (specs/interaction.md).
// Pure functions, no DOM: the element layer reads the SVG's data-merlion-* attributes
// and text into plain arrays and applies the sets these functions return as classes.
//
// An edge is { from, to, u } (u: undirected, followed both ways); a focus target is
// { n: nodeId } or { e: edgeIndex }. Sets hold node ids and edge indices.

const push = (m, k, i) => (m.get(k) ?? m.set(k, []).get(k)).push(i);

/** Outgoing and incoming edge indices per node id. O(V + E). */
export const adjacency = (edges) => {
  const out = new Map();
  const inc = new Map();
  edges.forEach((e, i) => {
    push(out, e.from, i);
    push(inc, e.to, i);
    if (e.u) {
      push(out, e.to, i);
      push(inc, e.from, i);
    }
  });
  return { out, inc };
};

// The node at the other end of edge e from id (a self-loop returns id).
const other = (e, id) => (e.from === id ? e.to : e.from);

/** Every node reachable from `start` downstream (`down`) or upstream, with the edges walked. O(V + E). */
export const reach = (adj, edges, start, down) => {
  const nodes = new Set([start]);
  const seen = new Set();
  const queue = [start];
  const m = down ? adj.out : adj.inc;
  for (let q = 0; q < queue.length; q++) {
    for (const i of m.get(queue[q]) ?? []) {
      seen.add(i);
      const o = other(edges[i], queue[q]);
      if (!nodes.has(o)) nodes.add(o), queue.push(o);
    }
  }
  return { nodes, edges: seen };
};

/**
 * The lit set of a target (specs/interaction.md#highlight-set). A node: itself, its incident
 * edges and their other ends. An edge: itself and both endpoints. `path` makes it transitive:
 * upstream and downstream of a node, or upstream of an edge's source and downstream of its target.
 */
export const focusSet = (adj, edges, t, path) => {
  const e = edges[t.e];
  if (path) {
    const up = reach(adj, edges, e ? e.from : t.n, false);
    const dn = reach(adj, edges, e ? e.to : t.n, true);
    return { nodes: new Set([...up.nodes, ...dn.nodes]), edges: new Set([...up.edges, ...dn.edges, ...(e ? [t.e] : [])]) };
  }
  const ids = e ? [t.e] : [...(adj.out.get(t.n) ?? []), ...(adj.inc.get(t.n) ?? [])];
  return { nodes: new Set(e ? [e.from, e.to] : [t.n, ...ids.map((i) => other(edges[i], t.n))]), edges: new Set(ids) };
};

/**
 * What collapsed clusters and hidden nodes take out of view (specs/interaction.md#hide-and-collapse).
 * `nodes` and `clusters` carry `cl`, their enclosing cluster ids outermost first. Returns the hidden
 * node ids and nested cluster ids, each collapsed cluster's badge count (members hidden under it),
 * and a state per affected edge: "hidden" when it runs inside one collapsed cluster or touches a
 * node hidden on its own, "stub" (drawn dimmed) when it runs from outside into a collapsed cluster.
 */
export const collapseSets = (nodes, clusters, edges, collapsed, hidden) => {
  const anchor = new Map(); // hidden node → outermost collapsed cluster, or "" when hidden on its own
  const count = new Map();
  for (const n of nodes) {
    const c = n.cl.find((a) => collapsed.has(a));
    if (c !== undefined) anchor.set(n.id, c), count.set(c, (count.get(c) ?? 0) + 1);
    else if (hidden.has(n.id)) anchor.set(n.id, "");
  }
  const es = new Map();
  edges.forEach((e, i) => {
    const a = anchor.get(e.from);
    const b = anchor.get(e.to);
    if (a === "" || b === "" || (a !== undefined && a === b)) es.set(i, "hidden");
    else if (a !== undefined || b !== undefined) es.set(i, "stub");
  });
  return {
    nodes: new Set(anchor.keys()),
    clusters: new Set(clusters.filter((c) => c.cl.some((a) => collapsed.has(a))).map((c) => c.id)),
    count,
    edges: es,
  };
};

/**
 * What a tap does (specs/interaction.md#gestures); a click that is not a tap (a drag, a text
 * selection, a double-click) never reaches this. `hit`: "node", "edge", "title" (a cluster title),
 * "bg" or "link" (inside an <a>, left to the browser). `same` is the mode
 * ("pin" or "path") the tapped target is already pinned in: a plain tap on the pinned target
 * clears it, and Shift+tap turns a plain pin into path mode.
 * Returns "none", "clear", "pin", "path", "collapse" or "hide".
 */
export const clickAction = ({ hit, shift, alt, same }) => {
  if (hit === "link") return "none";
  if (hit === "title") return "collapse";
  if (hit === "bg") return "clear";
  if (alt && hit === "node") return "hide";
  return same && (!shift || same === "path") ? "clear" : shift ? "path" : "pin";
};

/**
 * A label's lines from its runs (specs/interaction.md#text-reconstruction): a run with `y` starts
 * a line, other runs append to it; `j` marks a line that continues a broken word. Whitespace
 * collapses to single spaces.
 */
export const lines = (runs) => {
  const L = [];
  for (const r of runs) {
    if (r.y || !L.length) L.push({ ...r });
    else L[L.length - 1].t += r.t;
  }
  for (const l of L) l.t = l.t.replace(/\s+/g, " ").trim();
  return L;
};

/** A node's name: its lines joined by one space, or none after a `j` line. */
export const nameOf = (L) => L.map((l, i) => (i && !l.j ? " " : "") + l.t).join("");

/** A node's outline prefix: its cluster path, then its name (specs/svg-output.md#text-alternative). */
export const prefix = (n) => (n.path.length ? `${n.path.join(" / ")}: ` : "") + n.name;

/**
 * Each node's line in the SVG's <desc> outline (-1 when it has none), and node indices in outline
 * order: by that line, then document order. A line matches a node when it is the node's prefix,
 * or the prefix followed by an edge glyph, so a name never matches a longer name's line.
 */
export const outline = (nodes, desc) => {
  const at = nodes.map((n) => {
    const k = prefix(n);
    return desc.findIndex((l) => l === k || (l.startsWith(`${k} `) && "→←↔—".includes(l[k.length + 1])));
  });
  const key = (i) => (at[i] < 0 ? Infinity : at[i]);
  // Array sort is stable, so nodes with equal keys keep document order.
  return [at, nodes.map((_, i) => i).sort((a, b) => key(a) - key(b))];
};
