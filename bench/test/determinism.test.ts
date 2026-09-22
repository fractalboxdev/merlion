import { readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { CORPORA, compareRenders, summaryLine } from "../src/determinism.ts";
import { REPO_DIR } from "../src/paths.ts";

describe("compareRenders", () => {
  it("counts identical, jointly failed and differing diagrams", () => {
    const names = ["a", "b", "c", "d", "e"];
    const native = new Map<string, string | null>([
      ["a", "<svg>a</svg>"],
      ["b", "<svg>b</svg>"],
      ["c", null],
      ["d", "<svg>d</svg>"],
      ["e", null],
    ]);
    const wasm = new Map<string, string | null>([
      ["a", "<svg>a</svg>"],
      ["b", "<svg>b </svg>"],
      ["c", null],
      ["d", null],
      ["e", "<svg>e</svg>"],
    ]);
    expect(compareRenders(names, native, wasm)).toEqual({
      compared: 5,
      identical: 1,
      bothFailed: 1,
      differing: ["b", "d", "e"],
    });
  });

  it("treats a diagram missing from either side as differing", () => {
    const r = compareRenders(["x"], new Map([["x", "<svg/>"]]), new Map());
    expect(r.differing).toEqual(["x"]);
    expect(r.identical).toBe(0);
  });

  it("summarises the outcome in one line, naming the corpus and the font", () => {
    expect(summaryLine({ compared: 3, identical: 2, bothFailed: 1, differing: [] }, "sequence", "embed")).toBe(
      "determinism (sequence, font embed): 2/3 byte-identical, 1 failed on both targets, 0 differing",
    );
  });
});

describe("CORPORA", () => {
  it("names one directory of .mmd sources per corpus", () => {
    expect(Object.keys(CORPORA)).toEqual(["compat", "sequence"]);
    expect(CORPORA.sequence).toBe(join(REPO_DIR, "crates", "merlion-render", "tests", "fixtures", "sequence"));
    for (const dir of Object.values(CORPORA)) {
      expect(readdirSync(dir).filter((f) => f.endsWith(".mmd")).length).toBeGreaterThan(0);
    }
  });

  it("the sequence corpus holds sequence diagrams", () => {
    const names = readdirSync(CORPORA.sequence).filter((f) => f.endsWith(".mmd"));
    expect(names).toContain("checkout-order.mmd");
  });
});
