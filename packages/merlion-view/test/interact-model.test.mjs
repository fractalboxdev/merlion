// Pure interaction logic behind @fractalbox/merlion-view/interact (specs/interaction.md), no DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  adjacency,
  reach,
  focusSet,
  collapseSets,
  outOfView,
  clickAction,
  lines,
  nameOf,
  prefix,
  outline,
} from "../interact-model.js";

const E = (from, to, u = false) => ({ from, to, u });
const sorted = (s) => [...s].sort();

// a → b → c, a → d, e → a, c → c (self-loop), b → c twice (parallel), x — y undirected
const edges = [E("a", "b"), E("b", "c"), E("a", "d"), E("e", "a"), E("c", "c"), E("b", "c"), E("x", "y", true)];
const adj = adjacency(edges);

test("focus on a node: itself, incident edges, neighbours", () => {
  const f = focusSet(adj, edges, { n: "a" }, false);
  assert.deepEqual(sorted(f.nodes), ["a", "b", "d", "e"]);
  assert.deepEqual(sorted(f.edges), [0, 2, 3]);
});

test("a self-loop and parallel edges are incident", () => {
  const f = focusSet(adj, edges, { n: "c" }, false);
  assert.deepEqual(sorted(f.nodes), ["b", "c"]);
  assert.deepEqual(sorted(f.edges), [1, 4, 5]);
});

test("focus on an edge: the edge and both endpoints", () => {
  const f = focusSet(adj, edges, { e: 2 }, false);
  assert.deepEqual(sorted(f.nodes), ["a", "d"]);
  assert.deepEqual(sorted(f.edges), [2]);
});

test("path mode: transitive upstream and downstream, with traversed edges", () => {
  const f = focusSet(adj, edges, { n: "b" }, true);
  assert.deepEqual(sorted(f.nodes), ["a", "b", "c", "e"]);
  assert.deepEqual(sorted(f.edges), [0, 1, 3, 4, 5]);
});

test("path mode on an edge: upstream of from, downstream of to", () => {
  const f = focusSet(adj, edges, { e: 0 }, true);
  assert.deepEqual(sorted(f.nodes), ["a", "b", "c", "e"]);
  assert.deepEqual(sorted(f.edges), [0, 1, 3, 4, 5]);
});

test("undirected links are followed both ways in path mode", () => {
  assert.deepEqual(sorted(reach(adj, edges, "y", true).nodes), ["x", "y"]);
  assert.deepEqual(sorted(reach(adj, edges, "x", false).nodes), ["x", "y"]);
});

test("reach terminates on cycles and is linear", () => {
  const n = 20000;
  const big = Array.from({ length: n }, (_, i) => E(`n${i}`, `n${(i + 1) % n}`));
  const t = performance.now();
  const r = reach(adjacency(big), big, "n0", true);
  assert.equal(r.nodes.size, n);
  assert.equal(r.edges.size, n);
  assert.ok(performance.now() - t < 500);
});

// Clusters: A ⊃ B; nodes p, q in A; r in B; s outside.
const nodes = [
  { id: "p", cl: ["A"] },
  { id: "q", cl: ["A"] },
  { id: "r", cl: ["A", "B"] },
  { id: "s", cl: [] },
];
const clusters = [
  { id: "A", cl: [] },
  { id: "B", cl: ["A"] },
];
const cedges = [E("p", "q"), E("s", "p"), E("r", "s"), E("q", "r"), E("s", "s")];

test("collapsing a cluster hides its members, internal edges and nested clusters", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(["A"]), new Set());
  assert.deepEqual(sorted(c.nodes), ["p", "q", "r"]);
  assert.deepEqual(sorted(c.clusters), ["B"]);
  assert.equal(c.count.get("A"), 3);
  assert.equal(c.edges.get(0), "hidden");
  assert.equal(c.edges.get(3), "hidden");
  // edges from outside into hidden members stay, dimmed
  assert.equal(c.edges.get(1), "stub");
  assert.equal(c.edges.get(2), "stub");
  assert.equal(c.edges.get(4), undefined);
});

test("a cluster target is out of view when a collapsed cluster hides it", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(["A"]), new Set());
  // A nested cluster hidden under the collapsed one is no keyboard target and clears a pin.
  assert.equal(outOfView(c, { c: "B" }), true);
  // The collapsed cluster itself stays drawn, dashed, and stays a target.
  assert.equal(outOfView(c, { c: "A" }), false);
  assert.equal(outOfView(c, { n: "r" }), true);
  assert.equal(outOfView(c, { n: "s" }), false);
  // A stub edge is drawn, dimmed; only a hidden one is out of view.
  assert.equal(outOfView(c, { e: 0 }), true);
  assert.equal(outOfView(c, { e: 1 }), false);
});

test("nothing is out of view when nothing is collapsed or hidden", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(), new Set());
  for (const t of [{ c: "A" }, { c: "B" }, { n: "p" }, { e: 0 }]) assert.equal(outOfView(c, t), false);
});

test("collapsing a nested cluster counts only its own members", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(["B"]), new Set());
  assert.deepEqual(sorted(c.nodes), ["r"]);
  assert.deepEqual(sorted(c.clusters), []);
  assert.equal(c.count.get("B"), 1);
  assert.equal(c.edges.get(3), "stub");
});

test("an outer collapse takes the count from a collapsed inner cluster", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(["A", "B"]), new Set());
  assert.equal(c.count.get("A"), 3);
  assert.equal(c.count.has("B"), false);
});

test("a hidden node hides its incident edges: nothing is left to point at", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(), new Set(["s"]));
  assert.deepEqual(sorted(c.nodes), ["s"]);
  assert.deepEqual(sorted(c.edges.keys()), [1, 2, 4]);
  assert.ok([...c.edges.values()].every((v) => v === "hidden"));
});

test("nothing collapsed or hidden: empty sets", () => {
  const c = collapseSets(nodes, clusters, cedges, new Set(), new Set());
  assert.equal(c.nodes.size + c.clusters.size + c.edges.size + c.count.size, 0);
});

const click = (o) => clickAction({ hit: "node", ...o });

test("click on a node pins; the same click on the pinned node clears", () => {
  assert.equal(click({}), "pin");
  assert.equal(click({ same: "pin" }), "clear");
  assert.equal(click({ shift: true }), "path");
  assert.equal(click({ shift: true, same: "path" }), "clear");
  // a plain click clears a path pin; shift+click widens a plain pin to path mode
  assert.equal(click({ same: "path" }), "clear");
  assert.equal(click({ shift: true, same: "pin" }), "path");
  assert.equal(click({ hit: "edge" }), "pin");
});

test("background clears, cluster title collapses, alt+click hides", () => {
  assert.equal(click({ hit: "bg" }), "clear");
  assert.equal(click({ hit: "title" }), "collapse");
  assert.equal(click({ alt: true }), "hide");
  assert.equal(click({ hit: "edge", alt: true }), "pin");
});

test("a link keeps its own click", () => {
  assert.equal(click({ hit: "link" }), "none");
});

test("label lines: tspans with y start lines; runs append; joins rebuild words", () => {
  const L = lines([
    { t: "Build  the", y: 1 },
    { t: " site", y: 0 },
    { t: "pipe", y: 1 },
    { t: "line", y: 1, j: 1 },
  ]);
  assert.deepEqual(
    L.map((l) => l.t),
    ["Build the site", "pipe", "line"],
  );
  assert.equal(nameOf(L), "Build the site pipeline");
  assert.equal(nameOf([]), "");
});

const nodesM = [
  { id: "UI", name: "Web app", path: ["Client"] },
  { id: "Cache", name: "Local cache", path: ["Client"] },
  { id: "API", name: "API gateway", path: ["Services"] },
  { id: "Auth", name: "Auth service", path: ["Services", "Core"] },
  { id: "X", name: "Loose", path: [] },
];

test("outline prefix: cluster path, then name", () => {
  assert.equal(prefix(nodesM[0]), "Client: Web app");
  assert.equal(prefix(nodesM[3]), "Services / Core: Auth service");
  assert.equal(prefix(nodesM[4]), "Loose");
});

test("outline: each node's <desc> line and the outline order, unmatched nodes last", () => {
  const desc = [
    "Flowchart, left to right. 5 nodes, 4 edges.",
    "Services: API gateway → Auth service",
    "Client: Web app → API gateway; → Local cache",
    "Client: Local cache",
    "Services / Core: Auth service → Web app [token]",
  ];
  const [at, order] = outline(nodesM, desc);
  assert.deepEqual(at, [2, 3, 1, 4, -1]);
  assert.deepEqual(order, [2, 0, 1, 3, 4]);
  // a name that prefixes another's does not take its line
  const m2 = [
    { id: "a", name: "A", path: [] },
    { id: "b", name: "A B", path: [] },
  ];
  assert.deepEqual(outline(m2, ["A B → X", "A"]), [[1, 0], [1, 0]]);
});
