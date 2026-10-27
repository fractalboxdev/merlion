import { describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { extractDiagrams, isGantt, selectSourcePaths, sourceSlug } from "../src/corpus/sources.ts";
import { COMPAT_GANTT_DIR } from "../src/paths.ts";

describe("Gantt corpus extraction", () => {
  it("recognises Gantt after front matter, directives and comments without accepting a prefix", () => {
    expect(isGantt("---\ntitle: Schedule\n---\n%% a comment\ngantt\nA :2026-01-01, 1d")).toBe(true);
    expect(isGantt("%%{init: {}}%%\nGantt\nA :2026-01-01, 1d")).toBe(true);
    expect(isGantt("ganttish\nA :2026-01-01, 1d")).toBe(false);
    expect(isGantt("flowchart LR\nA --> B")).toBe(false);
  });

  it("selects every canonical demo, syntax document, e2e spec and fixture independently of supported features", () => {
    const paths = [
      "demos/gantt.html", "demos/flowchart.html", "demos/dev/example.html",
      "packages/mermaid/src/docs/syntax/gantt.md", "docs/syntax/gantt.md",
      "e2e/rendering/gantt/gantt.spec.js",
      "e2e/diagrams/gantt/invalid-date.mmd", "e2e/diagrams/gantt/compact.mmd",
      "e2e/diagrams/gantt/should-default-to-showing-today-marker.mmd",
      "e2e/diagrams/sequence/a.mmd",
    ];
    expect(selectSourcePaths(paths, "gantt")).toEqual([
      "demos/flowchart.html", "demos/gantt.html", "e2e/diagrams/gantt/compact.mmd",
      "e2e/diagrams/gantt/invalid-date.mmd", "e2e/diagrams/gantt/should-default-to-showing-today-marker.mmd",
      "e2e/rendering/gantt/gantt.spec.js", "packages/mermaid/src/docs/syntax/gantt.md",
    ]);
  });

  it("keeps invalid and clock-dependent inputs instead of selecting only supported schedules", () => {
    const invalid = "gantt\n  Task :a, invalid-date, 1d";
    expect(extractDiagrams("e2e/diagrams/gantt/invalid.mmd", invalid, "gantt")).toEqual([invalid]);
    expect(extractDiagrams("e2e/diagrams/gantt/clock.mmd", "gantt\n  Task :after missing, 1d", "gantt")).toHaveLength(1);
    expect(extractDiagrams("e2e/diagrams/gantt/empty.mmd", "gantt\n%% only a comment", "gantt")).toEqual([]);
  });

  it("extracts all source forms and decodes the HTML fixture boundary", () => {
    const source = "gantt\n  Task &amp; review :a, 2026-01-01, 1d";
    expect(extractDiagrams("demos/gantt.html", `<pre class="mermaid">${source}</pre>`, "gantt")[0]).toContain("Task & review");
    expect(extractDiagrams("e2e/diagrams/gantt/a.mmd", source, "gantt")[0]).toContain("Task & review");
    expect(extractDiagrams("docs/gantt.md", "```mermaid\n" + source + "\n```", "gantt")).toHaveLength(1);
    expect(extractDiagrams("e2e/rendering/gantt/gantt.spec.js", "render(`" + source + "`)", "gantt")).toHaveLength(1);
    expect(sourceSlug("e2e/diagrams/gantt/invalid-date.mmd")).toBe("e2e-invalid-date");
  });

  it("retains all pinned fixture sources with reproducible content hashes", () => {
    const manifest = JSON.parse(readFileSync(join(COMPAT_GANTT_DIR, "manifest.json"), "utf8")) as {
      commit: string; diagrams: Array<{ name: string; source: string; sourceBlob: string; sha256: string }>;
    };
    expect(manifest.commit).toBe("98a0945418c76238f15df2afaddbba4272656c3b");
    expect(manifest.diagrams).toHaveLength(64);
    expect(manifest.diagrams.filter((d) => d.source.startsWith("e2e/diagrams/gantt/"))).toHaveLength(43);
    for (const d of manifest.diagrams) {
      const content = readFileSync(join(COMPAT_GANTT_DIR, `${d.name}.mmd`), "utf8");
      // The manifest hashes the extracted diagram; storage adds one final newline.
      expect(content.endsWith("\n"), d.name).toBe(true);
      expect(createHash("sha256").update(content.slice(0, -1)).digest("hex"), d.name).toBe(d.sha256);
      expect(d.sourceBlob).toMatch(/^[a-f0-9]{40}$/);
    }
  });
});
