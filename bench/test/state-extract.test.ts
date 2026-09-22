import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { COMPAT_STATE_DIR } from "../src/paths.ts";
import { extractState } from "../src/svg/state.ts";

const merlion = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300" class="merlion merlion-state">
<g class="merlion-diagram" font-size="14">
<g class="merlion-cluster merlion-composite" data-merlion-id="Review"><path class="merlion-cluster-box" d="M8 100H200V220H8Z"/><text class="merlion-cluster-title"><tspan x="12" y="112">Review</tspan></text>
<g class="merlion-region" data-merlion-id="Review-r1" data-merlion-index="1"><path class="merlion-region-divider" d="M8 160H200"/></g>
<g class="merlion-node merlion-state-node" data-merlion-id="Editing" data-merlion-kind="simple"><path class="merlion-shape" d="M20 120H120V150H20Z"/><text class="merlion-label"><tspan x="24" y="140">Editing</tspan></text></g>
</g>
<g class="merlion-edge merlion-transition" data-merlion-from="root_start" data-merlion-to="Draft" data-merlion-index="0"><path class="merlion-edge-path" d="M50 20L50 60"/></g>
<g class="merlion-edge merlion-transition" data-merlion-from="Draft" data-merlion-to="Editing" data-merlion-index="1"><path class="merlion-edge-path" d="M50 90L50 120"/><g class="merlion-edge-label"><rect class="merlion-edge-label-bg" x="52" y="96" width="50" height="20"/><text class="merlion-edge-text"><tspan x="56" y="110">submit</tspan></text></g></g>
<g class="merlion-node merlion-state-node merlion-state-start" data-merlion-id="root_start" data-merlion-kind="start"><path class="merlion-shape" d="M43 13H57V27H43Z"/></g>
<g class="merlion-node merlion-state-node" data-merlion-id="Draft" data-merlion-kind="simple"><path class="merlion-shape" d="M20 60H120V90H20Z"/><text class="merlion-label"><tspan x="24" y="80">Draft</tspan></text></g>
<g class="merlion-note" data-merlion-id="Draft" data-merlion-placement="after"><path class="merlion-note-link" d="M120 75L200 75"/><rect class="merlion-note-box" x="200" y="60" width="120" height="30"/><text class="merlion-label"><tspan x="204" y="80">edited freely</tspan></text></g>
</g></svg>`;

const mermaid = `<svg id="d1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300" class="statediagram" aria-roledescription="stateDiagram">
<g class="root">
<g class="clusters"><g class="note-cluster" id="d1-state-Draft----parent-3"><rect x="4" y="50" width="330" height="50" fill="none"/></g>
<g class="statediagram-state statediagram-cluster" id="d1-state-Review-4" data-id="Review"><rect class="outer" x="8" y="100" width="192" height="120"/><g class="cluster-label"><text><tspan x="0" y="12">Review</tspan></text></g></g></g>
<g class="edgePaths">
<path d="M50 20L50 60" class="transition" data-edge="true" data-id="edge0"/>
<path d="M50 90L50 120" class="transition" data-edge="true" data-id="edge1"/>
<path d="M120 75L200 75" class="transition note-edge" data-edge="true" data-id="Draft-Draft----note-3"/>
</g>
<g class="edgeLabels"><g class="edgeLabel"><g class="label" data-id="edge1"><rect class="background" x="52" y="96" width="50" height="20"/><text><tspan class="row" x="0" y="110">submit</tspan></text></g></g></g>
<g class="nodes">
<g class="node default" id="d1-state-root_start-0"><circle class="state-start" cx="50" cy="20" r="7"/></g>
<g class="node statediagram-state" id="d1-state-Draft-1"><rect class="basic label-container" x="20" y="60" width="100" height="30"/><g class="label"><text><tspan class="row" x="0" y="80">Draft</tspan></text></g></g>
<g class="node statediagram-state" id="d1-state-Editing-2"><rect class="basic label-container" x="20" y="120" width="100" height="30"/><g class="label"><text><tspan class="row" x="0" y="140">Editing</tspan></text></g></g>
<g class="node statediagram-note" id="d1-state-Draft----note-3"><rect x="200" y="60" width="120" height="30"/><g class="label noteLabel"><text><tspan class="row" x="0" y="80">edited freely</tspan></text></g></g>
<g class="node statediagram-state" id="d1-state-divider-id-1-5"><rect class="divider" x="8" y="158" width="192" height="4"/></g>
</g></g></svg>`;

describe("extractState", () => {
  it("reads a Merlion drawing through its data attributes", () => {
    const d = extractState(merlion);
    expect(d.flavor).toBe("merlion");
    expect(d.graph.viewBox).toEqual({ x: 0, y: 0, w: 400, h: 300 });
    expect(d.graph.nodes.map((n) => n.id)).toEqual(["Editing", "root_start", "Draft"]);
    expect(d.graph.nodes.map((n) => n.label)).toEqual(["Editing", "", "Draft"]);
    expect(d.graph.edges.map((e) => [e.from, e.to, e.label])).toEqual([
      ["root_start", "Draft", ""],
      ["Draft", "Editing", "submit"],
    ]);
    expect(d.notes.map((n) => n.text)).toEqual(["edited freely"]);
    expect(d.notes[0]!.box).toEqual({ x: 200, y: 60, w: 120, h: 30 });
  });

  it("counts a composite state as a cluster and a concurrency region as neither", () => {
    const d = extractState(merlion);
    expect(d.graph.clusters.map((c) => [c.id, c.label])).toEqual([["Review", "Review"]]);
  });

  it("reads a mermaid drawing through its class names", () => {
    const d = extractState(mermaid);
    expect(d.flavor).toBe("mermaid");
    expect(d.graph.nodes.map((n) => n.id)).toEqual(["root_start", "Draft", "Editing"]);
    expect(d.graph.nodes.map((n) => n.label)).toEqual(["", "Draft", "Editing"]);
    expect(d.graph.clusters.map((c) => c.id)).toEqual(["Review"]);
    expect(d.notes.map((n) => n.text)).toEqual(["edited freely"]);
  });

  it("leaves mermaid's note node, note cluster and note edge out of the graph", () => {
    const d = extractState(mermaid);
    expect(d.graph.nodes.some((n) => n.id.includes("note"))).toBe(false);
    expect(d.graph.clusters.some((c) => c.id.includes("parent"))).toBe(false);
    expect(d.graph.edges.map((e) => e.label)).toEqual(["", "submit"]);
  });

  it("leaves mermaid's divider node out of the graph: a region divider is not a state", () => {
    expect(extractState(mermaid).graph.nodes.some((n) => n.id.startsWith("divider-id"))).toBe(false);
  });

  it("returns an empty drawing for a failed render and for another diagram type", () => {
    for (const svg of ["", "not xml", '<svg xmlns="http://www.w3.org/2000/svg" class="merlion merlion-sequence"></svg>']) {
      const d = extractState(svg);
      expect(d.flavor).toBe("unknown");
      expect(d.graph.nodes).toEqual([]);
      expect(d.notes).toEqual([]);
    }
  });
});

describe("the compat-state corpus", () => {
  it("holds only state diagrams, each with a manifest entry", () => {
    const manifest = JSON.parse(readFileSync(join(COMPAT_STATE_DIR, "manifest.json"), "utf8")) as {
      corpus: string;
      tag: string;
      diagrams: Array<{ name: string; source: string; sha256: string }>;
    };
    expect(manifest.corpus).toBe("compat-state");
    expect(manifest.tag).toBe("mermaid@12.0.0");
    expect(manifest.diagrams.length).toBeGreaterThan(80);
    for (const e of manifest.diagrams.slice(0, 20)) {
      const src = readFileSync(join(COMPAT_STATE_DIR, `${e.name}.mmd`), "utf8");
      expect(src).toMatch(/stateDiagram(-v2)?\b/i);
      expect(e.sha256).toMatch(/^[0-9a-f]{64}$/);
    }
  });
});
