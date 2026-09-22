// @fractalbox/merlion-view/interact: click to highlight, collapse and hide, a detail popover
// and keyboard traversal for Merlion flowcharts in <merlion-view> (specs/interaction.md).
// The logic lives in interact-model.js; this layer reads the SVG and toggles classes. It never
// re-renders or touches the SVG's <style>, and clearing restores the SVG's markup exactly.
// The popover and the live region are the base element's (`tip`, `say`).
import { MerlionView } from "./merlion-view.js";
import { adjacency, focusSet, collapseSets, clickAction, lines, nameOf, prefix, outline } from "./interact-model.js";

const V = "[data-merlion-interactive] ";
// Classes only this module sets need no scope.
const ACCENT = "var(--merlion-highlight,var(--merlion-accent,#0969da))";
// Rules for the SVG, added to the base element's light-DOM sheet.
MerlionView.style(`${V}:is(.merlion-node,.merlion-edge,.merlion-cluster-title){cursor:pointer}
${V}.merlion-node:hover,.merlion-edge.merlion-lit,.merlion-primary{--merlion-stroke:var(--merlion-highlight-stroke,2.5px)}
[data-merlion-state]${V}:is(.merlion-node,.merlion-edge):not(.merlion-lit),.merlion-stub:not(.merlion-lit){opacity:var(--merlion-dim-opacity,.2)}
.merlion-primary{--merlion-node-border:${ACCENT}}
.merlion-hidden{display:none}
.merlion-collapsed{--merlion-dash:4 3}
.merlion-badge{fill-opacity:.6}
.merlion-active{outline:2px solid ${ACCENT};outline-offset:3px}
@media (prefers-reduced-motion:no-preference){${V}:is(.merlion-node,.merlion-edge){transition:opacity .15s}}`);

// Label text of a <text> element as lines (specs/interaction.md#text-reconstruction).
const textOf = (t) =>
  lines(
    [...(t?.childNodes ?? [])].map((c) => ({
      t: c.textContent,
      y: c.hasAttribute?.("y"),
      j: c.dataset?.merlionJoin,
      d: c.classList?.contains("merlion-detail"),
    })),
  );

// Enclosing cluster groups, outermost first.
const clustersOf = (g) => {
  const a = [];
  while ((g = g.parentElement?.closest(".merlion-cluster"))) a.unshift(g);
  return a;
};

const title = (c) => c.querySelector(":scope>.merlion-cluster-title");
const id = (g) => g.dataset.merlionId;
const all = (el, sel) => [...el.querySelectorAll(sel)];
const toggle = (el, c, on) => el.classList.toggle(c, on);
// Two targets ({ n } or { e }) name the same element.
const same = (a, b) => a && b && a.n === b.n && a.e === b.e && a.c === b.c;

/** The extension: attaches to one host and SVG and returns its cleanup (specs/interaction.md#loading). */
export const interact = (host, svg) => {
  const edgeEls = all(svg, ".merlion-edge");
  if (!svg.classList.contains("merlion") || edgeEls.some((g) => !g.dataset.merlionFrom)) return;

  // Model, from data-merlion-* attributes and label text.
  const cls = all(svg, ".merlion-cluster").map((g) => ({ g, id: id(g), cl: clustersOf(g).map(id) }));
  const nodes = all(svg, ".merlion-node").map((g) => {
    const L = textOf(g.querySelector(".merlion-label"));
    const cs = clustersOf(g);
    return { g, els: [g], id: id(g), L, name: nameOf(L), path: cs.map((c) => nameOf(textOf(title(c)))), cl: cs.map(id) };
  });
  const edges = edgeEls.map((el) => {
    const p = el.querySelector(".merlion-edge-path");
    const me = p?.hasAttribute("marker-end");
    const ms = p?.hasAttribute("marker-start");
    return {
      el,
      from: el.dataset.merlionFrom,
      to: el.dataset.merlionTo,
      u: !me && !ms,
      g: me ? (ms ? "↔" : "→") : ms ? "←" : "—",
      label: nameOf(textOf(el.querySelector(".merlion-edge-text"))),
    };
  });
  const adj = adjacency(edges);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const name = (k) => byId.get(k)?.name ?? k;
  const desc = svg.querySelector(":scope>desc")?.textContent.split("\n") ?? [];
  const [at, order] = outline(nodes, desc);
  nodes.forEach((n, i) => (n.line = desc[at[i]] ?? prefix(n)));

  // State, all in the viewer.
  let pin = null; // { n } or { e }, plus `path`
  let active = null; // the keyboard's current target
  let gone;
  const collapsed = new Set();
  const hidden = new Set();
  const badges = new Map();
  // Lit sets an extension supplies for a cluster the viewer cannot derive one for, by its
  // `data-merlion-id`: a sequence's fragments and boxes (specs/interaction.md#highlight-set).
  const groups = new Map();
  // The keyboard's walk: every node in outline order. A sequence appends its messages to it, gives
  // each message the outline line that numbers it and lights the activation bars of a lit
  // participant, all by filling in the model above (specs/sequence.md#interaction). That module
  // loads for an SVG carrying `merlion-sequence` only, so a page of flowcharts never fetches it and
  // this one stays inside its budget (specs/viewer.md#constraints).
  const walk = order.map((i) => ({ n: nodes[i].id }));
  if (svg.classList.contains("merlion-sequence"))
    import("./interact-seq.js").then(
      (m) => (
        m.sequence({ svg, nodes, edges, cls, desc, walk, groups, text: (el) => nameOf(textOf(el)), style: MerlionView.style }),
        paint()
      ),
    );

  // Shown: the pin, else the keyboard's target as a preview.
  const shown = () => pin ?? active;
  const elOf = (t) => (t.c ? groups.get(t.c).g : t.e >= 0 ? edges[t.e].el : byId.get(t.n).g);
  const away = (t) => (t.e >= 0 ? gone.edges.get(t.e) === "hidden" : gone.nodes.has(t.n));
  const visible = () => walk.filter((t) => !away(t));

  host.toggleAttribute("data-merlion-interactive", true);

  // Popover content, built with textContent only (specs/interaction.md#content): a bold heading,
  // muted detail lines and cluster path, then outgoing and incoming edges with their labels.
  const fill = (t) => (pop) => {
    const add = (text, css = "") =>
      text && (Object.assign(pop.appendChild(document.createElement("div")), { textContent: text }).style.cssText = css);
    const lbl = (e) => (e.label ? ` [${e.label}]` : "");
    const e = edges[t.e];
    const n = byId.get(t.n);
    // A supplied group heads with its own line and says what it holds; it has no edge list.
    const c = groups.get(t.c);
    if (c) return add(c.line, "font-weight:600"), add(c.sub, "opacity:.7");
    // A message's outline line is its heading and its whole story: number, arrow and text.
    const ln = e?.line;
    add(ln ?? (e ? `${name(e.from)} ${e.g} ${name(e.to)}` : nameOf(n.L.filter((l) => !l.d)) || n.id), "font-weight:600");
    if (e) return ln || add(e.label);
    for (const l of n.L) if (l.d) add(l.t, "opacity:.7");
    add(n.path.join(" / "), "opacity:.7");
    for (const e of edges) if (e.from === n.id) add(`${e.g} ${name(e.to)}${lbl(e)}`);
    for (const e of edges) if (e.to === n.id) add(`← ${name(e.from)}${lbl(e)}`);
  };

  const paint = () => {
    for (const el of all(svg, ".merlion-lit,.merlion-primary,.merlion-active"))
      el.classList.remove("merlion-lit", "merlion-primary", "merlion-active");
    if (active) elOf(active).classList.add("merlion-active");
    const t = shown();
    host.toggleAttribute("data-merlion-state", !!t);
    host.tip(t && elOf(t), t && fill(t));
    if (!t) return;
    const f = t.c ? groups.get(t.c) : focusSet(adj, edges, t, t.path);
    // An edge may end on a cluster (`A --> subgraph`): that end has no node group to light.
    for (const k of f.nodes) for (const el of byId.get(k)?.els ?? []) el.classList.add("merlion-lit");
    for (const i of f.edges) edges[i].el.classList.add("merlion-lit");
    elOf(t).classList.add("merlion-primary");
    // The polite live region reads the target's outline line.
    const ln = (edges[t.e] ?? byId.get(t.n) ?? groups.get(t.c))?.line;
    if (ln) host.say(ln);
  };

  // Hide and collapse (specs/interaction.md#hide-and-collapse): classes, plus a "+N" badge per collapsed cluster.
  const apply = () => {
    gone = collapseSets(nodes, cls, edges, collapsed, hidden);
    for (const n of nodes) toggle(n.g, "merlion-hidden", gone.nodes.has(n.id));
    edges.forEach((e, i) => {
      toggle(e.el, "merlion-hidden", gone.edges.get(i) === "hidden");
      toggle(e.el, "merlion-stub", gone.edges.get(i) === "stub");
    });
    for (const c of cls) {
      toggle(c.g, "merlion-hidden", gone.clusters.has(c.id));
      toggle(c.g, "merlion-collapsed", collapsed.has(c.id));
      const n = gone.count.get(c.id);
      // The badge is a run appended to the title, so it follows the title's text and colour.
      let b = badges.get(c);
      if (n && !b) {
        badges.set(c, (b = document.createElementNS(svg.namespaceURI, "tspan")));
        b.setAttribute("class", "merlion-badge");
        title(c.g).append(b);
      }
      if (n) b.textContent = ` +${n}`;
      else b?.remove(), badges.delete(c);
    }
    // The base shows its "Show all" control while this is set.
    host.toggleAttribute("data-merlion-hidden", !!(collapsed.size + hidden.size));
    if (pin && away(pin)) pin = null;
    if (active && away(active)) active = null;
    paint();
  };

  const showAll = () => {
    collapsed.clear();
    hidden.clear();
    apply();
  };

  // Gestures (specs/interaction.md#gestures): the base decides what is a tap, the model what it does.
  const onClick = (e) => {
    const t = e.target;
    if (!host.tap(e)) return;
    const g = t.closest?.(".merlion-node");
    const eg = !g && t.closest?.(".merlion-edge");
    const target = g ? { n: id(g) } : eg && { e: edgeEls.indexOf(eg) };
    const a = clickAction({
      hit: t.closest?.(".merlion-cluster-title")
          ? "title"
          : t.closest?.("a")
            ? "link"
            : g
              ? "node"
              : eg
                ? "edge"
                : "bg",
      same: same(pin, target) && (pin.path ? "path" : "pin"),
      shift: e.shiftKey,
      alt: e.altKey,
    });
    if (a === "none") return;
    active = null;
    if (a === "hide") hidden.add(target.n);
    else if (a === "collapse") {
      // A cluster the SVG names collapses; one an extension lit set covers pins instead; one with
      // neither does nothing, so a title that names no cluster never reaches the collapsed set.
      const k = id(t.closest(".merlion-cluster"));
      if (groups.has(k)) pin = same(pin, { c: k }) ? null : { c: k };
      else if (k) collapsed.delete(k) || collapsed.add(k);
    } else pin = a === "clear" ? null : { ...target, path: a === "path" };
    apply();
  };

  // Keyboard (specs/interaction.md#keyboard-and-screen-readers). Capture phase: the base viewer
  // skips a key this consumes (defaultPrevented), and Shift + arrows still pan.
  const onKey = (e) => {
    const k = e.key;
    const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[k];
    if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
    const vis = visible();
    if (step) {
      const i = vis.findIndex((t) => same(t, active));
      active = vis[i < 0 ? (step > 0 ? 0 : vis.length - 1) : (i + step + vis.length) % vis.length];
    } else if (k === "Enter" && active) pin = same(pin, active) ? null : active;
    else if (k === "Escape" && (pin || active)) pin = active = null;
    else return;
    e.preventDefault();
    paint();
  };

  const on = (add) => {
    host[add]("click", onClick);
    host[add]("keydown", onKey, true);
    host[add]("merlion-show-all", showAll);
  };
  on("addEventListener");
  apply();

  return () => {
    on("removeEventListener");
    pin = active = null;
    showAll();
    host.removeAttribute("data-merlion-interactive");
  };
};

MerlionView.extend(interact);
