import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { extractDiagrams, isQuadrant, selectSourcePaths, sourceSlug } from "../src/corpus/sources.ts";
import { quadrantCompat, quadrantGate, quadrantOutcome } from "../src/metrics/quadrant.ts";
import { COMPAT_QUADRANT_DIR } from "../src/paths.ts";
import { extractQuadrant, hexColour, type QuadrantDrawing } from "../src/svg/quadrant.ts";

/** A mermaid-shaped drawing: 400 × 400 plot at (50, 50), two points, axes below and left. */
const mermaidSvg = (over: { pointFill?: string; secondY?: number; xLabelY?: number; quadrantTextY?: number } = {}) => `<svg viewBox="0 0 500 500"><g class="main">
<g class="quadrants">
<g class="quadrant"><rect x="250" y="50" width="200" height="200" fill="#ECECFF"></rect><text font-size="16" dominant-baseline="hanging" text-anchor="middle" transform="translate(350, ${over.quadrantTextY ?? 55}) rotate(0)">Plan</text></g>
<g class="quadrant"><rect x="50" y="50" width="200" height="200" fill="#f1f1ff"></rect><text font-size="16" dominant-baseline="hanging" text-anchor="middle" transform="translate(150, 55) rotate(0)">Do</text></g>
<g class="quadrant"><rect x="50" y="250" width="200" height="200" fill="#f6f6ff"></rect><text font-size="16" dominant-baseline="hanging" text-anchor="middle" transform="translate(150, 255) rotate(0)">Delegate</text></g>
<g class="quadrant"><rect x="250" y="250" width="200" height="200" fill="#fbfbff"></rect><text font-size="16" dominant-baseline="hanging" text-anchor="middle" transform="translate(350, 255) rotate(0)">Delete</text></g>
</g>
<g class="data-points">
<g class="data-point"><circle cx="130" cy="210" r="5" fill="${over.pointFill ?? "hsl(240, 100%, NaN%)"}" stroke-width="0px"></circle><text font-size="12" dominant-baseline="hanging" text-anchor="middle" transform="translate(130, 215) rotate(0)">A &amp; B</text></g>
<g class="data-point"><circle cx="410" cy="${over.secondY ?? 370}" r="5" fill="#ff0000" stroke-width="0px"></circle><text font-size="12" dominant-baseline="hanging" text-anchor="middle" transform="translate(410, 375) rotate(0)">C</text></g>
</g>
<g class="labels">
<g class="label"><text font-size="16" dominant-baseline="hanging" text-anchor="middle" transform="translate(150, ${over.xLabelY ?? 460}) rotate(0)">Urgent</text></g>
<g class="label"><text font-size="16" dominant-baseline="hanging" text-anchor="middle" transform="translate(350, ${over.xLabelY ?? 460}) rotate(0)">Not urgent</text></g>
<g class="label"><text font-size="16" dominant-baseline="hanging" text-anchor="start" transform="translate(5, 450) rotate(-90)">Important ⟶</text></g>
</g>
<g class="title"><text font-size="20" dominant-baseline="hanging" text-anchor="middle" transform="translate(250, 10) rotate(0)">Matrix</text></g>
</g></svg>`;

/** The same chart as Merlion draws it. */
const merlionSvg = `<svg class="merlion merlion-quadrant" viewBox="0 0 500 500"><g class="merlion-diagram" font-size="14">
<g class="merlion-quadrant-region merlion-quadrant-1"><rect class="merlion-quadrant-box" x="250" y="50" width="200" height="200" fill="#e3eefc"/><text class="merlion-label merlion-quadrant-text merlion-quadrant-label" fill="#1f2328"><tspan x="330" y="72">Plan</tspan></text></g>
<g class="merlion-quadrant-region merlion-quadrant-2"><rect class="merlion-quadrant-box" x="50" y="50" width="200" height="200" fill="#eaf2fd"/><text class="merlion-label merlion-quadrant-text merlion-quadrant-label" fill="#1f2328"><tspan x="140" y="72">Do</tspan></text></g>
<g class="merlion-quadrant-region merlion-quadrant-3"><rect class="merlion-quadrant-box" x="50" y="250" width="200" height="200" fill="#f1f6fe"/><text class="merlion-label merlion-quadrant-text merlion-quadrant-label" fill="#1f2328"><tspan x="120" y="272">Delegate</tspan></text></g>
<g class="merlion-quadrant-region merlion-quadrant-4"><rect class="merlion-quadrant-box" x="250" y="250" width="200" height="200" fill="#f8fbfe"/><text class="merlion-label merlion-quadrant-text merlion-quadrant-label" fill="#1f2328"><tspan x="330" y="272">Delete</tspan></text></g>
<g class="merlion-quadrant-point" data-merlion-id="A &amp; B"><circle class="merlion-quadrant-dot" cx="130" cy="210" r="5" fill="#0969da" stroke-width="0"/><text class="merlion-label merlion-quadrant-text merlion-quadrant-point-label" font-size="12"><tspan x="115" y="228">A &amp; B</tspan></text></g>
<g class="merlion-quadrant-point merlion-ps-1" data-merlion-id="C"><circle class="merlion-quadrant-dot" cx="410" cy="370" r="5" fill="#ff0000" stroke-width="0"/><text class="merlion-label merlion-quadrant-text merlion-quadrant-point-label" font-size="12"><tspan x="406" y="388">C</tspan></text></g>
<g class="merlion-quadrant-axis" data-merlion-axis="x" data-merlion-end="low"><text class="merlion-label merlion-quadrant-text merlion-quadrant-axis-x"><tspan x="125" y="475">Urgent</tspan></text></g>
<g class="merlion-quadrant-axis" data-merlion-axis="x" data-merlion-end="high"><text class="merlion-label merlion-quadrant-text merlion-quadrant-axis-x"><tspan x="315" y="475">Not urgent</tspan></text></g>
<g class="merlion-quadrant-axis" data-merlion-axis="y" data-merlion-end="low" transform="rotate(-90 15 400)"><text class="merlion-label merlion-quadrant-text merlion-quadrant-axis-y"><tspan x="-35" y="420">Important →</tspan></text></g>
<text class="merlion-label merlion-quadrant-text merlion-quadrant-title" font-size="16"><tspan x="220" y="25">Matrix</tspan></text>
</g></svg>`;

const source = "quadrantChart\n  title Matrix\n  x-axis Urgent --> Not urgent\n  y-axis Important -->\n";

describe("quadrant corpus extraction", () => {
  it("recognises the header as mermaid's detector does", () => {
    expect(isQuadrant("---\ntitle: x\n---\n%% c\nquadrantChart\n  a: [0, 1]")).toBe(true);
    expect(isQuadrant("QuadrantChart\n  a: [0, 1]")).toBe(false);
    expect(isQuadrant("quadrantChartX\n  a: [0, 1]")).toBe(false);
  });

  it("selects demos, documentation, examples, unit tests and fixtures", () => {
    const paths = [
      "demos/quadrantchart.html", "packages/mermaid/src/docs/syntax/quadrantChart.md",
      "packages/examples/src/examples/quadrant-chart.ts",
      "packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison.spec.ts",
      "packages/mermaid/src/diagrams/quadrant-chart/quadrantBuilder.ts",
      "e2e/diagrams/quadrant-chart/should-render-without-points.mmd", "e2e/diagrams/gantt/a.mmd",
    ];
    expect(selectSourcePaths(paths, "quadrant")).toEqual([
      "demos/quadrantchart.html", "e2e/diagrams/quadrant-chart/should-render-without-points.mmd",
      "packages/examples/src/examples/quadrant-chart.ts",
      "packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison.spec.ts",
      "packages/mermaid/src/docs/syntax/quadrantChart.md",
    ]);
    expect(sourceSlug("e2e/diagrams/quadrant-chart/should-render-without-points.mmd")).toBe("e2e-should-render-without-points");
    expect(sourceSlug("packages/examples/src/examples/quadrant-chart.ts")).toBe("examples-quadrant-chart");
    expect(sourceSlug("packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison.spec.ts")).toBe("unit-quadrant-jison");
    expect(extractDiagrams("x.spec.ts", "parse(`quadrantChart\n  a: [0.1, 0.2]`)", "quadrant")).toHaveLength(1);
  });

  it("pins every retained source by content hash", () => {
    const manifest = JSON.parse(readFileSync(join(COMPAT_QUADRANT_DIR, "manifest.json"), "utf8")) as {
      corpus: string; commit: string; diagrams: Array<{ name: string; sourceBlob: string; sha256: string }>;
    };
    expect(manifest.corpus).toBe("compat-quadrant");
    expect(manifest.commit).toBe("98a0945418c76238f15df2afaddbba4272656c3b");
    expect(manifest.diagrams).toHaveLength(24);
    for (const d of manifest.diagrams) {
      const content = readFileSync(join(COMPAT_QUADRANT_DIR, `${d.name}.mmd`), "utf8");
      expect(createHash("sha256").update(content.slice(0, -1)).digest("hex"), d.name).toBe(d.sha256);
      expect(d.sourceBlob).toMatch(/^[a-f0-9]{40}$/);
    }
  });
});

describe("quadrant comparison", () => {
  const reference = extractQuadrant(mermaidSvg());
  const candidate = extractQuadrant(merlionSvg);

  it("reads both renderers into one shape", () => {
    for (const d of [reference, candidate]) {
      expect(d.title).toBe("Matrix");
      expect(d.plot).toEqual({ x: 50, y: 50, w: 400, h: 400 });
      expect(d.quadrants.map((q) => [q.n, q.label, q.placement])).toEqual([[1, "Plan", "top"], [2, "Do", "top"], [3, "Delegate", "top"], [4, "Delete", "top"]]);
      expect(d.axes.map((a) => [a.axis, a.end, a.side, a.align])).toEqual([["x", "low", "bottom", "half"], ["x", "high", "bottom", "half"], ["y", "low", "left", "start"]]);
      const a = d.points.find((p) => p.label === "A & B")!;
      expect([a.x, a.y, a.labelBelow]).toEqual([0.2, 0.6, true]);
    }
    expect(quadrantCompat(reference, candidate, source)).toEqual({ pass: true, differences: [] });
  });

  it("finds moved points, moved labels, lost text and changed explicit colours", () => {
    const fails = (svg: string) => quadrantCompat(extractQuadrant(svg), candidate, source);
    expect(fails(mermaidSvg({ secondY: 380 })).differences[0]).toMatch(/point "C"/);
    expect(fails(mermaidSvg({ xLabelY: 20 })).differences.join(";")).toMatch(/x-low label on the bottom, expected top/);
    expect(fails(mermaidSvg({ quadrantTextY: 140 })).differences).toEqual(["quadrant 1 label at top, expected centre"]);
    expect(fails(mermaidSvg({ pointFill: "#00ff00" })).differences).toEqual([`point "A & B" fill #0969da, expected #00ff00`]);
    expect(fails(mermaidSvg().replace(">Matrix<", ">Other<")).pass).toBe(false);
    expect(quadrantCompat(reference, extractQuadrant(merlionSvg.replace(/<g class="merlion-quadrant-point merlion-ps-1"[\s\S]*?<\/g>/, "")), source).pass).toBe(false);
    expect(extractQuadrant("not svg").validSvg).toBe(false);
  });

  it("compares quadrant fills only where the source sets them", () => {
    const fill: QuadrantDrawing = { ...candidate, quadrants: candidate.quadrants.map((q) => ({ ...q, fill: "#123456" })) };
    expect(quadrantCompat(reference, fill, source).pass).toBe(true);
    expect(quadrantCompat(reference, fill, `${source}%% quadrant1Fill`).pass).toBe(false);
    expect(hexColour("ABC")).toBe("#aabbcc");
    expect(hexColour("hsl(240, 100%, NaN%)")).toBeNull();
  });

  it("rejects missing and duplicated quadrant positions even with four rectangles", () => {
    const overlapping = merlionSvg.replace(/<rect\b[^>]*\/>/g, '<rect x="10" y="10" width="100" height="100"/>');
    const result = quadrantCompat(candidate, extractQuadrant(overlapping), source);
    expect(result.pass).toBe(false);
    expect(result.differences.join(";")).toMatch(/quadrant 1 has 0 boxes/);
    expect(result.differences.join(";")).toMatch(/quadrant 3 has 4 boxes/);
  });

  it("detects a lost paint for every supported theme variable", () => {
    const keys = [
      ...[1, 2, 3, 4].flatMap((n) => [`quadrant${n}Fill`, `quadrant${n}TextFill`]),
      "quadrantPointFill", "quadrantPointTextFill", "quadrantXAxisTextFill", "quadrantYAxisTextFill",
      "quadrantInternalBorderStrokeFill", "quadrantExternalBorderStrokeFill", "quadrantTitleFill",
    ];
    const colours = Object.fromEntries(keys.map((k, i) => [k, `#${(0x123450 + i).toString(16)}`]));
    const attr = (tag: string, name: string, value: string) => tag.includes(`${name}="`)
      ? tag.replace(new RegExp(`${name}="[^"]*"`), `${name}="${value}"`)
      : tag.replace(/\/?$/, (end) => ` ${name}="${value}"${end}`);
    let themed = merlionSvg.replace(/(<g class="merlion-quadrant-region merlion-quadrant-(\d)">)([\s\S]*?)(<\/g>)/g,
      (_, open, n, inner: string, close) => open + inner
        .replace(/<rect\b[^>]*(?=>)/, (tag) => attr(tag, "fill", colours[`quadrant${n}Fill`]!))
        .replace(/<text\b[^>]*(?=>)/, (tag) => attr(tag, "fill", colours[`quadrant${n}TextFill`]!)) + close);
    themed = themed.replace(/<(?:circle|text)\b[^>]*(?=>)/g, (tag) => {
      const key = tag.includes("merlion-quadrant-dot") ? "quadrantPointFill"
        : tag.includes("merlion-quadrant-point-label") ? "quadrantPointTextFill"
        : tag.includes("merlion-quadrant-axis-x") ? "quadrantXAxisTextFill"
        : tag.includes("merlion-quadrant-axis-y") ? "quadrantYAxisTextFill"
        : tag.includes("merlion-quadrant-title") ? "quadrantTitleFill" : null;
      return key === null ? tag : attr(tag, "fill", colours[key]!);
    });
    themed = themed.replace("</svg>", `${[...Array(6)].map((_, i) => `<line class="merlion-quadrant-${i < 4 ? "border" : "divider"}" stroke="${colours[i < 4 ? "quadrantExternalBorderStrokeFill" : "quadrantInternalBorderStrokeFill"]}"/>`).join("")}</svg>`);
    const original = extractQuadrant(themed);
    const themeSource = keys.join(" ");
    expect(quadrantCompat(original, original, themeSource).pass).toBe(true);
    for (const key of keys) {
      const changed = extractQuadrant(themed.replaceAll(colours[key]!, "#111111"));
      const result = quadrantCompat(original, changed, themeSource);
      expect(result.pass, key).toBe(false);
      expect(result.differences.join(";"), key).toContain(`${key} paint differs`);
    }
  });

  it("gates on the complete corpus", () => {
    expect(quadrantOutcome("bad", "bad", null)).toBe("both rejected");
    expect(quadrantOutcome("bad", null, null)).toBe("reference rejected");
    expect(quadrantOutcome(null, null, { pass: true, differences: [] })).toBe("matched");
    expect(quadrantGate(24, 24, 23, 95)).toBeNull();
    expect(quadrantGate(24, 24, 22, 95)).toMatch(/below 95%/);
    expect(quadrantGate(24, 10, 10, 95)).toMatch(/complete/);
    expect(quadrantGate(24, 10, 0, 0)).toBeNull();
  });
});
