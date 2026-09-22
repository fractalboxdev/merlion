import { describe, expect, it } from "vitest";
import { extractSvg, normalizeLabel } from "../src/svg/extract.ts";

const MERLION = `<svg xmlns="http://www.w3.org/2000/svg" id="m1" viewBox="0 0 300 200" class="merlion merlion-flowchart" data-merlion-version="0.1.0">
  <title id="m1-title">Flowchart diagram</title>
  <g class="merlion-cluster" data-merlion-id="grp">
    <rect x="0" y="0" width="300" height="120"/>
    <text x="10" y="14">Group</text>
    <g class="merlion-node" data-merlion-id="a" data-merlion-rank="0">
      <rect x="10" y="20" width="80" height="40"/>
      <text x="50" y="45"><tspan x="50" y="45">Start &amp; go</tspan></text>
    </g>
  </g>
  <g class="merlion-node" data-merlion-id="b" data-merlion-rank="1" transform="translate(200,140)">
    <polygon points="0,0 40,20 0,40 -40,20"/>
    <text x="0" y="25">Decide</text>
  </g>
  <g class="merlion-edge" data-merlion-from="a" data-merlion-to="b">
    <path d="M50,60 L50,100 L200,100 L200,140"/>
    <rect x="100" y="90" width="40" height="20"/>
    <text x="120" y="104">yes</text>
  </g>
</svg>`;

const MERMAID = `<svg id="d1" width="100%" xmlns="http://www.w3.org/2000/svg" class="flowchart" viewBox="-8 -8 316 216">
<style>#d1 .node > rect { fill: red }</style>
<g><g class="root">
 <g class="clusters"><g class="cluster" id="d1-S1"><rect x="0" y="0" width="200" height="100"/>
   <g class="cluster-label" transform="translate(80, 0)"><g><rect class="background"/><text y="-10.1"><tspan class="text-outer-tspan row" x="0" dy="1.1em"><tspan class="text-inner-tspan">Sub</tspan></tspan></text></g></g></g></g>
 <g class="edgePaths">
  <path d="M50,40 C50,60 150,60 150,80" id="d1-L_A_B_C_0" class="flowchart-link" data-edge="true" data-id="L_A_B_C_0"/>
  <path d="M150,120 L150,160" id="d1-L_B_C_D_0" class="flowchart-link" data-edge="true" data-id="L_B_C_D_0"/>
 </g>
 <g class="edgeLabels">
  <g class="edgeLabel" transform="translate(100, 60)"><g class="label" data-id="L_A_B_C_0" transform="translate(0, -9)"><g><rect class="background" x="-15" y="-1" width="30" height="20"/><text><tspan class="text-outer-tspan row"><tspan class="text-inner-tspan">Get</tspan><tspan class="text-inner-tspan"> money</tspan></tspan></text></g></g></g>
  <g class="edgeLabel"><g class="label" data-id="L_B_C_D_0" transform="translate(0, 0)"><text><tspan class="text-outer-tspan row"></tspan></text></g></g>
 </g>
 <g class="nodes">
  <g class="node default" id="d1-flowchart-A_B-0" transform="translate(50, 20)"><rect class="basic label-container" x="-40" y="-20" width="80" height="40"/><g class="label" transform="translate(0,-8)"><rect/><g><rect class="background"/><text><tspan class="text-outer-tspan row"><tspan class="text-inner-tspan">Christmas</tspan></tspan><tspan class="text-outer-tspan row"><tspan class="text-inner-tspan">tree</tspan></tspan></text></g></g></g>
  <g class="node default" id="d1-flowchart-C-1" transform="translate(150, 100)"><polygon class="label-container" points="20,0 40,-20 20,-40 0,-20" transform="translate(-20, 20)"/><g class="label"><text>fa:fa-car Car</text></g></g>
  <g class="node default" id="d1-flowchart-D-2" transform="translate(150, 180)"><rect x="-30" y="-20" width="60" height="40"/><g class="label"><text>D</text></g></g>
 </g>
</g></g></svg>`;

describe("normalizeLabel", () => {
  it("collapses whitespace and drops icon tokens", () => {
    expect(normalizeLabel("  Go \n  shopping ")).toBe("Go shopping");
    expect(normalizeLabel("fa:fa-car Car")).toBe("Car");
    expect(normalizeLabel(" x ")).toBe("x");
  });
});

describe("extractSvg: merlion", () => {
  const g = extractSvg(MERLION);
  it("detects the renderer and viewBox", () => {
    expect(g.flavor).toBe("merlion");
    expect(g.viewBox).toEqual({ x: 0, y: 0, w: 300, h: 200 });
  });
  it("reads nodes with exact ids, boxes and labels", () => {
    expect(g.nodes.map((n) => [n.id, n.label])).toEqual([
      ["a", "Start & go"],
      ["b", "Decide"],
    ]);
    expect(g.nodes[0]!.box).toEqual({ x: 10, y: 20, w: 80, h: 40 });
    expect(g.nodes[1]!.box).toEqual({ x: 160, y: 140, w: 80, h: 40 });
  });
  it("reads clusters without their members' text", () => {
    expect(g.clusters).toEqual([{ id: "grp", label: "Group", box: { x: 0, y: 0, w: 300, h: 120 } }]);
  });
  it("reads edges with endpoints, polyline, label and label chip", () => {
    expect(g.edges).toHaveLength(1);
    const e = g.edges[0]!;
    expect([e.from, e.to, e.label]).toEqual(["a", "b", "yes"]);
    expect(e.points).toHaveLength(4);
    expect(e.vertices).toHaveLength(4);
    expect(e.labelBox).toEqual({ x: 100, y: 90, w: 40, h: 20 });
  });
});

describe("extractSvg: mermaid", () => {
  const g = extractSvg(MERMAID);
  it("detects the renderer", () => {
    expect(g.flavor).toBe("mermaid");
    expect(g.viewBox).toEqual({ x: -8, y: -8, w: 316, h: 216 });
  });
  it("reads node ids from element ids and boxes through transforms", () => {
    expect(g.nodes.map((n) => [n.id, n.label])).toEqual([
      ["A_B", "Christmas tree"],
      ["C", "Car"],
      ["D", "D"],
    ]);
    expect(g.nodes[0]!.box).toEqual({ x: 10, y: 0, w: 80, h: 40 });
    expect(g.nodes[1]!.box).toEqual({ x: 130, y: 80, w: 40, h: 40 });
  });
  it("resolves edge endpoints from data-id against known node ids", () => {
    expect(g.edges.map((e) => [e.from, e.to, e.label])).toEqual([
      ["A_B", "C", "Get money"],
      ["C", "D", ""],
    ]);
    expect(g.edges[0]!.labelBox).toEqual({ x: 85, y: 50, w: 30, h: 20 });
    expect(g.edges[1]!.labelBox).toBeNull();
    expect(g.edges[0]!.vertices).toHaveLength(2);
    expect(g.edges[0]!.points.length).toBeGreaterThan(2);
  });
  it("reads clusters", () => {
    expect(g.clusters).toEqual([{ id: "S1", label: "Sub", box: { x: 0, y: 0, w: 200, h: 100 } }]);
  });
});

describe("extractSvg: mermaid variants", () => {
  it("resolves edges to subgraphs against cluster ids", () => {
    const svg = `<svg id="d2" viewBox="0 0 300 100"><g class="clusters"><g class="cluster" id="d2-TOP"><rect x="100" y="0" width="100" height="100"/></g></g>
      <g class="edgePaths"><path class="flowchart-link" data-id="L_A_TOP_0" d="M40,50 L100,50"/></g>
      <g class="nodes"><g class="node" id="d2-flowchart-A-0" transform="translate(20,50)"><rect x="-20" y="-10" width="40" height="20"/></g></g></svg>`;
    expect(extractSvg(svg).edges.map((e) => [e.from, e.to])).toEqual([["A", "TOP"]]);
  });

  it("reads icon and image nodes by their flowchart element id", () => {
    const svg = `<svg id="d4" viewBox="0 0 100 100"><g class="nodes"><g class="icon-shape default" id="d4-flowchart-A-0" transform="translate(50,50)"><g><path d="M-10 -10 L10 -10 L10 10 L-10 10 Z"/></g><g class="label"><text>User Icon</text></g></g></g></svg>`;
    expect(extractSvg(svg).nodes).toEqual([{ id: "A", label: "User Icon", box: { x: 40, y: 40, w: 20, h: 20 } }]);
  });

  it("reads hand-drawn nodes and uses data-points for multi-stroke edges", () => {
    const pts = Buffer.from(JSON.stringify([{ x: 40, y: 50 }, { x: 70, y: 60 }, { x: 100, y: 50 }])).toString("base64");
    const svg = `<svg id="d3" viewBox="0 0 200 100"><g class="edgePaths"><path class="flowchart-link" data-id="L_A_B_0" data-points="${pts}" d="M40 50 C50 50, 60 50, 70 50 M41 51 C50 51, 60 51, 70 51"/></g>
      <g class="nodes"><g class="rough-node default" id="d3-flowchart-A-0" transform="translate(20,50)"><g class="basic label-container"><path d="M-20 -10 L20 -10 L20 10 L-20 10 Z"/></g></g>
      <g class="rough-node default" id="d3-flowchart-B-1" transform="translate(120,50)"><path d="M-20 -10 L20 -10 L20 10 L-20 10 Z"/></g></g></svg>`;
    const g = extractSvg(svg);
    expect(g.nodes.map((n) => n.id)).toEqual(["A", "B"]);
    expect(g.edges[0]!.points).toEqual([{ x: 40, y: 50 }, { x: 70, y: 60 }, { x: 100, y: 50 }]);
    expect(g.edges[0]!.vertices).toHaveLength(3);
  });
});

describe("extractSvg: fallbacks", () => {
  it("returns an empty graph for non-SVG input", () => {
    const g = extractSvg("<html>nope</html>");
    expect(g.nodes).toEqual([]);
    expect(g.edges).toEqual([]);
    expect(g.viewBox).toBeNull();
  });
  it("falls back to geometric endpoint matching", () => {
    const svg = `<svg viewBox="0 0 100 100"><g class="edgePaths"><path class="flowchart-link" d="M10,12 L10,58"/></g>
      <g class="nodes"><g class="node" id="flowchart-x-0"><rect x="0" y="0" width="20" height="10"/></g>
      <g class="node" id="flowchart-y-1"><rect x="0" y="60" width="20" height="10"/></g></g></svg>`;
    const g = extractSvg(svg);
    expect(g.edges.map((e) => [e.from, e.to])).toEqual([["x", "y"]]);
  });
});
