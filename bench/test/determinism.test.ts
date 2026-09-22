import { describe, expect, it } from "vitest";
import { compareRenders, summaryLine } from "../src/determinism.ts";

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

  it("summarises the outcome in one line", () => {
    expect(summaryLine({ compared: 3, identical: 2, bothFailed: 1, differing: [] })).toBe(
      "determinism: 2/3 byte-identical, 1 failed on both targets, 0 differing",
    );
  });
});
