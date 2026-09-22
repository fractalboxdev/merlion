import { describe, expect, it } from "vitest";
import {
  dedent,
  extractHtmlPreBlocks,
  extractMarkdownFences,
  extractTemplateLiterals,
  isFlowchart,
  sourceSlug,
} from "../src/corpus/sources.ts";

describe("dedent", () => {
  it("removes common indentation and surrounding blank lines", () => {
    expect(dedent("\n    graph TD\n      A --> B\n\n    B --> C\n  ")).toBe("graph TD\n  A --> B\n\nB --> C");
    expect(dedent("\t\tgraph LR\n\t\tA")).toBe("graph LR\nA");
    expect(dedent("")).toBe("");
  });
});

describe("isFlowchart", () => {
  it("accepts graph, flowchart and flowchart-elk headers", () => {
    expect(isFlowchart("graph TD\nA-->B")).toBe(true);
    expect(isFlowchart("flowchart LR")).toBe(true);
    expect(isFlowchart("flowchart-elk TB\nA")).toBe(true);
    expect(isFlowchart("graph")).toBe(true);
  });
  it("skips front matter, directives and comments before the header", () => {
    expect(isFlowchart("---\ntitle: x\nconfig:\n  layout: elk\n---\nflowchart TD\nA")).toBe(true);
    expect(isFlowchart("%%{init: {'theme':'dark'}}%%\n%% a comment\n\ngraph LR")).toBe(true);
  });
  it("rejects other diagram types", () => {
    expect(isFlowchart("sequenceDiagram\nA->>B: hi")).toBe(false);
    expect(isFlowchart("gitGraph\ncommit")).toBe(false);
    expect(isFlowchart("graphic TD")).toBe(false);
    expect(isFlowchart("---\ntitle: never closed\ngraph TD")).toBe(false);
  });
});

describe("extractHtmlPreBlocks", () => {
  it("finds pre.mermaid blocks, decodes entities and dedents", () => {
    const html = `<h1>x</h1>
    <pre class="mermaid">
      graph TD
        A --&gt; B
    </pre>
    <pre class="other">graph TD</pre>
    <pre id="p" class="mermaid big" style="x">
      sequenceDiagram
    </pre>`;
    expect(extractHtmlPreBlocks(html)).toEqual(["graph TD\n  A --> B", "sequenceDiagram"]);
  });
});

describe("extractMarkdownFences", () => {
  it("finds mermaid and mermaid-example fences only", () => {
    const md = "text\n```mermaid-example\nflowchart LR\n  A\n```\n\n```js\nconsole.log(1)\n```\n  ```mermaid\n  graph TD\n  B\n  ```\n```mermaid\nunterminated";
    expect(extractMarkdownFences(md)).toEqual(["flowchart LR\n  A", "graph TD\nB"]);
  });
});

describe("extractTemplateLiterals", () => {
  it("returns static template literals, skipping interpolated ones", () => {
    const js =
      "renderGraph(page, t, `graph TD\n  A-->B`, {});\nimgSnapshotTest(`flowchart LR\n  ${x + `n`} --> Y`);\nconst s = `a \\` b`;\n";
    expect(extractTemplateLiterals(js)).toEqual(["graph TD\n  A-->B", "a ` b"]);
  });
  it("ignores backticks inside ordinary strings and comments", () => {
    const js = "const a = 'x`y'; const b = \"`\"; // don't `x`\n/* `y` it's */ f(`graph LR`)";
    expect(extractTemplateLiterals(js)).toEqual(["graph LR"]);
  });
});

describe("sourceSlug", () => {
  it("builds a short file-name-safe slug from a repository path", () => {
    expect(sourceSlug("demos/flowchart.html")).toBe("demos-flowchart");
    expect(sourceSlug("packages/mermaid/src/docs/syntax/flowchart.md")).toBe("docs-flowchart");
    expect(sourceSlug("e2e/rendering/flowchart/flowchart-v2.spec.js")).toBe("e2e-flowchart-v2");
    expect(sourceSlug("e2e/diagrams/flowchart/dagre/7-Some Name.mmd")).toBe("e2e-dagre-7-some-name");
  });
});
