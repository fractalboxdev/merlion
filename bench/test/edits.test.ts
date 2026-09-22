import { describe, expect, it } from "vitest";
import { makeEdits, parseSimpleEdge } from "../src/corpus/edits.ts";

const SRC = `flowchart TD
  A[Christmas] -->|Get money| B(Go shopping)
  B --> C{Let me think}
  C -->|One| D[Laptop]
  C --> E
  D --> E`;

describe("parseSimpleEdge", () => {
  it("reads ids, shapes and labels of one-edge lines", () => {
    expect(parseSimpleEdge("  A[Christmas] -->|Get money| B(Go shopping)")).toEqual({
      from: "A",
      to: "B",
      fromShape: "[Christmas]",
      toShape: "(Go shopping)",
    });
    expect(parseSimpleEdge("D --> E;")).toEqual({ from: "D", to: "E", fromShape: null, toShape: null });
    expect(parseSimpleEdge("A --> B & C")).toBeNull();
    expect(parseSimpleEdge("subgraph one")).toBeNull();
  });
});

describe("makeEdits", () => {
  const edits = makeEdits("demo", SRC);
  const byKind = new Map(edits.map((e) => [e.kind, e]));

  it("produces the four edit kinds deterministically", () => {
    expect(edits.map((e) => e.kind)).toEqual(["add-node", "add-edge", "remove-edge", "rename-label"]);
    expect(makeEdits("demo", SRC)).toEqual(edits);
    for (const e of edits) {
      expect(e.before).toBe(SRC);
      expect(e.name).toBe(`demo--${e.kind}`);
      expect(e.source).toBe("demo");
    }
  });

  it("adds an isolated node", () => {
    expect(byKind.get("add-node")!.after).toBe(`${SRC}\n  benchNewNode[New node]`);
  });

  it("adds an edge between two unconnected existing nodes", () => {
    expect(byKind.get("add-edge")!.after).toBe(`${SRC}\n  A --> E`);
  });

  it("removes an unshaped edge whose endpoints survive", () => {
    expect(byKind.get("remove-edge")!.after).toBe(SRC.replace("\n  D --> E", ""));
  });

  it("keeps the endpoints of a removed edge whose line declares them", () => {
    const src = "graph LR\n  A --> B\n  B --> C\n  C -->|x| D[Done]";
    const e = makeEdits("s", src).find((x) => x.kind === "remove-edge")!;
    expect(e.after).toBe("graph LR\n  A --> B\n  B --> C\n  D[Done]");
  });

  it("renames the first square-bracket label", () => {
    expect(byKind.get("rename-label")!.after).toBe(SRC.replace("A[Christmas]", "A[Christmas renamed]"));
  });

  it("returns nothing for diagrams without enough simple edges", () => {
    expect(makeEdits("x", "flowchart TD\n  A --> B")).toEqual([]);
    expect(makeEdits("x", "sequenceDiagram\nA->>B: x\nB->>C: y\nC->>D: z")).toEqual([]);
  });
});
