import { describe, expect, it } from "vitest";
import { canon, fidelity, multisetMatch } from "../src/authoring/compare.ts";
import { extractAnswer, promptFor } from "../src/authoring/prompt.ts";
import type { RefGraph, Task } from "../src/authoring/tasks.ts";
import { referenceMermaid } from "../src/authoring/tasks.ts";
import type { ExtractedEdge, ExtractedGraph, ExtractedNode } from "../src/svg/extract.ts";
import { extractGeneric } from "../src/svg/generic.ts";

const ref: RefGraph = {
  nodes: [
    { id: "a", label: "Push" },
    { id: "b", label: "Build" },
    { id: "c", label: "Notify author" },
  ],
  edges: [
    { from: "a", to: "b", label: "" },
    { from: "b", to: "c", label: "fail" },
  ],
};

const node = (id: string, label: string, x: number, y: number, w = 60, h = 30): ExtractedNode => ({
  id,
  label,
  box: { x, y, w, h },
});

const edge = (from: string | null, to: string | null, label = ""): ExtractedEdge => ({
  from,
  to,
  label,
  points: [{ x: 0, y: 0 }, { x: 10, y: 10 }],
  vertices: [{ x: 0, y: 0 }, { x: 10, y: 10 }],
  labelBox: null,
});

const graph = (nodes: ExtractedNode[], edges: ExtractedEdge[]): ExtractedGraph => ({
  flavor: "unknown",
  viewBox: { x: 0, y: 0, w: 200, h: 200 },
  nodes,
  edges,
  clusters: [],
});

describe("canon", () => {
  it("ignores case, repeated whitespace and trailing punctuation", () => {
    expect(canon("  Notify   Author. ")).toBe("notify author");
    expect(canon("Build")).toBe(canon("build"));
  });
});

describe("multisetMatch", () => {
  it("counts each repeat once", () => {
    expect(multisetMatch(["a", "a", "b"], ["a", "a", "a"])).toBe(2);
    expect(multisetMatch([], ["a"])).toBe(0);
  });
});

describe("fidelity", () => {
  it("scores a drawing of exactly the declared graph as perfect", () => {
    const got = graph(
      [node("n0", "Push", 0, 0), node("n1", "Build", 0, 50), node("n2", "Notify author", 0, 100)],
      [edge("n0", "n1"), edge("n1", "n2", "fail")],
    );
    const f = fidelity(ref, got);
    expect(f.nodeF1).toBe(1);
    expect(f.edgeF1).toBe(1);
    expect(f.edgeLabelsMatched).toBe(1);
    expect(f.labelledEdges).toBe(1);
  });

  it("matches labels case-insensitively", () => {
    const got = graph(
      [node("n0", "push", 0, 0), node("n1", "BUILD", 0, 50), node("n2", "Notify  author", 0, 100)],
      [edge("n0", "n1"), edge("n1", "n2", "Fail")],
    );
    expect(fidelity(ref, got).nodeF1).toBe(1);
    expect(fidelity(ref, got).edgeLabelsMatched).toBe(1);
  });

  it("penalises an invented node and a missing edge", () => {
    const got = graph(
      [node("n0", "Push", 0, 0), node("n1", "Build", 0, 50), node("n2", "Notify author", 0, 100), node("n3", "Deploy", 0, 150)],
      [edge("n0", "n1")],
    );
    const f = fidelity(ref, got);
    expect(f.nodesMatched).toBe(3);
    expect(f.nodeF1).toBeLessThan(1);
    expect(f.edgesMatched).toBe(1);
    expect(f.edgeLabelsMatched).toBe(0);
  });

  it("counts an edge that reaches no shape as dangling and not as an edge", () => {
    const got = graph(
      [node("n0", "Push", 0, 0), node("n1", "Build", 0, 50), node("n2", "Notify author", 0, 100)],
      [edge("n0", "n1"), edge("n1", null, "fail")],
    );
    const f = fidelity(ref, got);
    expect(f.danglingEdges).toBe(1);
    expect(f.gotEdges).toBe(1);
  });

  it("does not credit an edge label carried between the wrong two nodes", () => {
    const got = graph(
      [node("n0", "Push", 0, 0), node("n1", "Build", 0, 50), node("n2", "Notify author", 0, 100)],
      [edge("n0", "n1", "fail"), edge("n1", "n2")],
    );
    expect(fidelity(ref, got).edgeLabelsMatched).toBe(0);
  });
});

describe("referenceMermaid", () => {
  it("writes the declared graph as Mermaid, labels on the edges that have them", () => {
    expect(referenceMermaid(ref)).toBe(
      "flowchart TD\n  a[Push]\n  b[Build]\n  c[Notify author]\n  a --> b\n  b -->|fail| c\n",
    );
  });
});

describe("prompts", () => {
  const task: Task = { name: "t", size: "small", prompt: "Two boxes.", reference: ref };

  it("differ only in the format instruction", () => {
    const mermaid = promptFor(task, "mermaid");
    const svg = promptFor(task, "svg");
    const shared = "The drawing must be readable";
    expect(mermaid).toContain(shared);
    expect(svg).toContain(shared);
    expect(mermaid.slice(0, mermaid.indexOf("Answer with"))).toBe(svg.slice(0, svg.indexOf("Answer with")));
  });
});

describe("extractAnswer", () => {
  it("takes the fenced block of the requested language", () => {
    const text = "Here you go:\n\n```mermaid\nflowchart LR\n  a --> b\n```\n\nHope that helps.";
    expect(extractAnswer(text, "mermaid")).toBe("flowchart LR\n  a --> b");
  });

  it("falls back to a block that looks like the format", () => {
    expect(extractAnswer("```\nflowchart TD\n  a --> b\n```", "mermaid")).toBe("flowchart TD\n  a --> b");
  });

  it("keeps only the root element of an SVG answer", () => {
    const text = '```svg\n<?xml version="1.0"?>\n<svg xmlns="http://www.w3.org/2000/svg"></svg>\nthanks\n```';
    expect(extractAnswer(text, "svg")).toBe('<svg xmlns="http://www.w3.org/2000/svg"></svg>');
  });

  it("accepts an unfenced answer", () => {
    expect(extractAnswer("flowchart LR\n  a --> b", "mermaid")).toBe("flowchart LR\n  a --> b");
  });

  it("is null when the answer holds no diagram", () => {
    expect(extractAnswer("I cannot draw that.", "mermaid")).toBeNull();
    expect(extractAnswer("```js\nconst a = 1;\n```", "svg")).toBeNull();
  });
});

describe("extractGeneric", () => {
  const box = (x: number, y: number, label: string) =>
    `<rect x="${x}" y="${y}" width="80" height="40" fill="#eee" stroke="#333"/>` +
    `<text x="${x + 40}" y="${y + 25}" text-anchor="middle">${label}</text>`;

  it("reads boxes as nodes and strokes between them as edges", () => {
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      box(20, 20, "Push") +
      box(20, 120, "Build") +
      `<path d="M60 60L60 120" fill="none" stroke="#333"/>` +
      `</svg>`;
    const g = extractGeneric(svg);
    expect(g.nodes.map((n) => n.label).sort()).toEqual(["Build", "Push"]);
    expect(g.edges).toHaveLength(1);
    expect(g.edges[0]?.from).toBe(g.nodes.find((n) => n.label === "Push")?.id);
    expect(g.edges[0]?.to).toBe(g.nodes.find((n) => n.label === "Build")?.id);
  });

  it("reads a text sitting on a stroke as that edge's label, and its chip as no node", () => {
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      box(20, 20, "Push") +
      box(20, 220, "Build") +
      `<path d="M60 60L60 220" fill="none" stroke="#333"/>` +
      `<rect x="48" y="130" width="24" height="18" fill="#fff"/>` +
      `<text x="60" y="143" text-anchor="middle">fail</text>` +
      `</svg>`;
    const g = extractGeneric(svg);
    expect(g.nodes.map((n) => n.label).sort()).toEqual(["Build", "Push"]);
    expect(g.edges[0]?.label).toBe("fail");
  });

  it("does not take a node's own label as an edge label", () => {
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      box(20, 20, "Push") +
      box(20, 120, "Build") +
      `<path d="M60 60L60 120" fill="none" stroke="#333"/>` +
      `</svg>`;
    expect(extractGeneric(svg).edges[0]?.label).toBe("");
  });

  it("reads a label drawn without a box as a node", () => {
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      box(20, 20, "Push") +
      `<text x="60" y="245" text-anchor="middle">Build</text>` +
      `<path d="M60 60L60 230" fill="none" stroke="#333"/>` +
      `</svg>`;
    expect(extractGeneric(svg).nodes.map((n) => n.label).sort()).toEqual(["Build", "Push"]);
  });

  it("ignores the background plate and anything in defs", () => {
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      `<rect x="0" y="0" width="200" height="300" fill="#fff"/>` +
      `<defs><marker id="a"><path d="M0 0L10 5L0 10Z" fill="#333"/></marker></defs>` +
      box(20, 20, "Push") +
      `</svg>`;
    const g = extractGeneric(svg);
    expect(g.nodes).toHaveLength(1);
    expect(g.edges).toHaveLength(0);
  });

  it("recovers when an end tag is missing its bracket, as a renderer does", () => {
    // `</defs` instead of `</defs>`: browsers end the tag at the next `<` and
    // draw the rest, so everything after it must not be lost inside <defs>.
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      `<defs><marker id="a"><polygon points="0,0 10,5 0,10"/></marker></defs` +
      box(20, 20, "Push") +
      box(20, 120, "Build") +
      `<path d="M60 60L60 120" fill="none" stroke="#333"/>` +
      `</svg>`;
    const g = extractGeneric(svg);
    expect(g.nodes.map((n) => n.label).sort()).toEqual(["Build", "Push"]);
    expect(g.edges).toHaveLength(1);
  });

  it("reads an arrow whose head closes with Z as an edge", () => {
    // Shaft and filled head in one `d`: the Z belongs to the head, not the shaft.
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 300">` +
      box(20, 20, "Push") +
      box(20, 120, "Build") +
      `<path d="M60 60 L60 120 M55 115 L60 120 L65 115 Z" fill="none" stroke="#333"/>` +
      `</svg>`;
    const g = extractGeneric(svg);
    expect(g.edges).toHaveLength(1);
    expect(g.edges[0]?.from).toBe(g.nodes.find((n) => n.label === "Push")?.id);
    expect(g.edges[0]?.to).toBe(g.nodes.find((n) => n.label === "Build")?.id);
  });

  it("reads two subpaths of one path as two edges, not one crossing between them", () => {
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300">` +
      box(20, 20, "A") +
      box(20, 160, "B") +
      box(260, 20, "C") +
      box(260, 160, "D") +
      `<path d="M60 60 L60 160 M300 60 L300 160" fill="none" stroke="#333"/>` +
      `</svg>`;
    const g = extractGeneric(svg);
    const id = (label: string) => g.nodes.find((n) => n.label === label)?.id;
    expect(g.edges).toHaveLength(2);
    expect(g.edges.map((e) => [e.from, e.to])).toEqual([[id("A"), id("B")], [id("C"), id("D")]]);
  });

  it("is empty for input that is not an SVG", () => {
    expect(extractGeneric("not markup").nodes).toHaveLength(0);
  });
});
