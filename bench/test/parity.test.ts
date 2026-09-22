import { deflateSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import {
  compareElements,
  comparePixels,
  type ElementRecord,
  normaliseDash,
  over,
  type PixelSample,
  sameColour,
  themesOf,
  uniformAt,
} from "../src/parity/compare.ts";
import { decodePng } from "../src/parity/png.ts";

const crc32 = (bytes: Uint8Array): number => {
  let c = 0xffffffff;
  for (const b of bytes) {
    c ^= b;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  }
  return (c ^ 0xffffffff) >>> 0;
};

const chunk = (type: string, data: Uint8Array): Uint8Array => {
  const out = new Uint8Array(12 + data.length);
  const v = new DataView(out.buffer);
  v.setUint32(0, data.length);
  out.set(new TextEncoder().encode(type), 4);
  out.set(data, 8);
  v.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
};

/** A PNG with one scanline filter per row, so every filter type is decoded. */
const png = (width: number, rows: readonly (readonly number[])[], filters: readonly number[], colourType: 2 | 6): Uint8Array => {
  const bpp = colourType === 6 ? 4 : 3;
  const raw: number[] = [];
  const prev = new Array<number>(width * bpp).fill(0);
  rows.forEach((row, y) => {
    const f = filters[y] ?? 0;
    raw.push(f);
    row.forEach((x, i) => {
      const a = i >= bpp ? (row[i - bpp] ?? 0) : 0;
      const b = prev[i] ?? 0;
      const c = i >= bpp ? (prev[i - bpp] ?? 0) : 0;
      const p = a + b - c;
      const pa = Math.abs(p - a);
      const pb = Math.abs(p - b);
      const pc = Math.abs(p - c);
      const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      const pred = [0, a, b, (a + b) >> 1, paeth][f] ?? 0;
      raw.push((x - pred) & 0xff);
    });
    row.forEach((x, i) => (prev[i] = x));
  });
  const ihdr = new Uint8Array(13);
  const v = new DataView(ihdr.buffer);
  v.setUint32(0, width);
  v.setUint32(4, rows.length);
  ihdr.set([8, colourType, 0, 0, 0], 8);
  const parts = [
    new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(Uint8Array.from(raw))),
    chunk("IEND", new Uint8Array()),
  ];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
};

describe("decodePng", () => {
  const rgba = [
    [10, 20, 30, 255, 40, 50, 60, 128],
    [11, 21, 31, 255, 45, 55, 65, 0],
    [200, 100, 50, 255, 1, 2, 3, 4],
    [7, 8, 9, 10, 250, 251, 252, 253],
    [0, 0, 0, 0, 255, 255, 255, 255],
  ];

  it("decodes RGBA rows under every filter type", () => {
    const img = decodePng(png(2, rgba, [0, 1, 2, 3, 4], 6));
    expect(img.width).toBe(2);
    expect(img.height).toBe(5);
    expect(Array.from(img.rgba)).toEqual(rgba.flat());
  });

  it("expands RGB to opaque RGBA", () => {
    const rgb = [
      [1, 2, 3, 4, 5, 6],
      [7, 8, 9, 10, 11, 12],
    ];
    const img = decodePng(png(2, rgb, [4, 3], 2));
    expect(Array.from(img.rgba)).toEqual([1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]);
  });

  it("rejects a file that is not a PNG", () => {
    expect(() => decodePng(new Uint8Array([1, 2, 3]))).toThrow(/not a PNG/);
  });
});

describe("colours", () => {
  it("matches within the tolerance per channel", () => {
    expect(sameColour([10, 20, 30, 255], [11, 19, 30, 255], 1)).toBe(true);
    expect(sameColour([10, 20, 30, 255], [12, 20, 30, 255], 1)).toBe(false);
    expect(sameColour([10, 20, 30, 254], [10, 20, 30, 255], 1)).toBe(true);
  });

  it("treats every fully transparent colour as the same", () => {
    expect(sameColour([10, 20, 30, 0], [200, 0, 0, 0], 1)).toBe(true);
  });

  it("composites a translucent pixel over an opaque backdrop", () => {
    expect(over([255, 0, 0, 128], [255, 255, 255, 255])).toEqual([255, 127, 127, 255]);
    expect(over([1, 2, 3, 0], [9, 9, 9, 255])).toEqual([9, 9, 9, 255]);
  });

  it("normalises dash arrays to comma-separated numbers", () => {
    expect(normaliseDash("6px, 4px")).toBe("6,4");
    expect(normaliseDash("6 4")).toBe("6,4");
    expect(normaliseDash("none")).toBe("none");
    expect(normaliseDash("0px")).toBe("none");
  });
});

describe("themesOf", () => {
  it("lists the base theme and each named theme of compiled CSS once", () => {
    const css = ':root {\n}\n[data-theme="dark"] {\n}\n[data-theme="dark"] .merlion .merlion-c-a {\n}\n[data-theme="sea"] {\n}\n';
    expect(themesOf(css)).toEqual([null, "dark", "sea"]);
  });
});

const el = (index: number, props: Partial<ElementRecord["props"]> = {}): ElementRecord => ({
  index,
  tag: "path",
  kind: "node",
  label: `node#${index}`,
  props: { fill: [1, 2, 3, 255], stroke: [4, 5, 6, 255], dash: "none", opacity: 1, ...props },
});

describe("compareElements", () => {
  it("counts every compared property and reports each one outside the tolerance", () => {
    const reference = [el(0), el(1)];
    const target = [el(0), el(1, { stroke: [9, 5, 6, 255], dash: "6,4" })];
    const r = compareElements(reference, target, 1);
    expect(r.compared).toBe(8);
    expect(r.mismatches.map((m) => `${m.label} ${m.property}`)).toEqual(["node#1 stroke", "node#1 dash"]);
  });

  it("compares a missing paint (none) as a value of its own", () => {
    const r = compareElements([el(0, { fill: null })], [el(0, { fill: [0, 0, 0, 0] })], 1);
    expect(r.mismatches.map((m) => m.property)).toEqual(["fill"]);
  });

  it("reports a structure mismatch when the element lists differ", () => {
    const r = compareElements([el(0), el(1)], [el(0)], 1);
    expect(r.structure).toBe(false);
    expect(r.mismatches.map((m) => m.property)).toEqual(["structure"]);
  });
});

describe("pixels", () => {
  const img = { width: 3, height: 3, rgba: new Uint8Array(3 * 3 * 4) };
  for (let i = 0; i < 9; i++) img.rgba.set([50, 60, 70, 255], i * 4);

  it("detects a uniform neighbourhood and a disturbed one", () => {
    expect(uniformAt(img, 1, 1, 1)).toBe(true);
    const spotted = { ...img, rgba: Uint8Array.from(img.rgba) };
    spotted.rgba.set([90, 60, 70, 255], 0);
    expect(uniformAt(spotted, 1, 1, 1)).toBe(false);
    expect(uniformAt(img, 0, 0, 1)).toBe(false);
  });

  it("compares the reference pixel with the other raster over white", () => {
    const other = { width: 3, height: 3, rgba: new Uint8Array(3 * 3 * 4) };
    other.rgba.set([50, 60, 71, 255], (1 * 3 + 1) * 4);
    other.rgba.set([0, 0, 0, 0], 0);
    const samples: PixelSample[] = [
      { label: "a", x: 1, y: 1 },
      { label: "b", x: 0, y: 0 },
    ];
    const r = comparePixels(img, other, samples, 1);
    expect(r.compared).toBe(2);
    expect(r.mismatches.map((m) => m.label)).toEqual(["b"]);
  });
});
