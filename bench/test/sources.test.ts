import { describe, expect, it } from "vitest";
import {
  dedent,
  extractDiagrams,
  selectSourcePaths,
  extractHtmlPreBlocks,
  extractMarkdownFences,
  extractTemplateLiterals,
  isFlowchart,
  isSequence,
  isState,
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

describe("selectSourcePaths", () => {
  it("keeps demo pages, the flowchart syntax doc and flowchart e2e sources", () => {
    const tree = [
      "demos/flowchart.html",
      "demos/sub/x.html",
      "demos/readme.md",
      "docs/syntax/flowchart.md",
      "packages/mermaid/src/docs/syntax/flowchart.md",
      "packages/mermaid/src/docs/syntax/sequence.md",
      "e2e/rendering/flowchart/flowchart-v2.spec.js",
      "e2e/rendering/flowchart/flowchart-shape-alias.spec.ts",
      "e2e/rendering/sequence/sequence.spec.js",
      "e2e/diagrams/flowchart/1-a.mmd",
      "e2e/diagrams/flowchart/dagre/2-b.mmd",
      "e2e/diagrams/flowchart/handdrawn/3-c.mmd",
      "e2e/diagrams/class-diagram/1.mmd",
    ];
    expect(selectSourcePaths(tree)).toEqual([
      "demos/flowchart.html",
      "e2e/diagrams/flowchart/1-a.mmd",
      "e2e/diagrams/flowchart/dagre/2-b.mmd",
      "e2e/rendering/flowchart/flowchart-shape-alias.spec.ts",
      "e2e/rendering/flowchart/flowchart-v2.spec.js",
      "packages/mermaid/src/docs/syntax/flowchart.md",
    ]);
  });
});

describe("isSequence", () => {
  it("accepts a sequenceDiagram header, whatever its case", () => {
    expect(isSequence("sequenceDiagram\nA->>B: hi")).toBe(true);
    expect(isSequence("sequenceDiagram")).toBe(true);
    expect(isSequence("SequenceDiagram\nA->>B: hi")).toBe(true);
    expect(isSequence("sequenceDiagram;A->>B: hi")).toBe(true);
  });
  it("skips front matter, directives and comments before the header", () => {
    expect(isSequence("---\ntitle: x\n---\nsequenceDiagram\nA->>B: hi")).toBe(true);
    expect(isSequence("%%{init: {'theme':'dark'}}%%\n%% a comment\n\nsequenceDiagram")).toBe(true);
  });
  it("rejects other diagram types", () => {
    expect(isSequence("graph TD\nA-->B")).toBe(false);
    expect(isSequence("sequenceDiagrams\nA->>B: hi")).toBe(false);
    expect(isSequence("---\ntitle: never closed\nsequenceDiagram")).toBe(false);
  });
});

describe("selectSourcePaths for sequences", () => {
  it("keeps demo pages, the sequence syntax doc and sequence e2e sources", () => {
    const tree = [
      "demos/sequence.html",
      "demos/flowchart.html",
      "packages/mermaid/src/docs/syntax/flowchart.md",
      "packages/mermaid/src/docs/syntax/sequenceDiagram.md",
      "e2e/rendering/flowchart/flowchart-v2.spec.js",
      "e2e/rendering/sequence/sequencediagram.spec.js",
      "e2e/rendering/sequence/sequenceDiagram-redux-themes.spec.ts",
      "e2e/diagrams/sequence/should-render-a-simple-sequence-diagram.mmd",
      "e2e/diagrams/flowchart/1-a.mmd",
      "e2e/platform/dev-diagrams/diagrams/sequence-fixes/01-actor-vs-database.mmd",
    ];
    expect(selectSourcePaths(tree, "sequence")).toEqual([
      "demos/flowchart.html",
      "demos/sequence.html",
      "e2e/diagrams/sequence/should-render-a-simple-sequence-diagram.mmd",
      "e2e/rendering/sequence/sequenceDiagram-redux-themes.spec.ts",
      "e2e/rendering/sequence/sequencediagram.spec.js",
      "packages/mermaid/src/docs/syntax/sequenceDiagram.md",
    ]);
  });
});

describe("isState", () => {
  it("accepts both state headers, case-insensitively, after front matter and directives", () => {
    expect(isState("stateDiagram-v2\n[*] --> A")).toBe(true);
    expect(isState("stateDiagram\n[*] --> A")).toBe(true);
    expect(isState("StateDiagram-V2\n[*] --> A")).toBe(true);
    expect(isState("stateDiagram-v2;[*] --> A")).toBe(true);
    expect(isState("---\ntitle: x\n---\nstateDiagram-v2\n[*] --> A")).toBe(true);
    expect(isState("%%{init: {'theme':'dark'}}%%\n%% a comment\n\nstateDiagram-v2")).toBe(true);
  });

  it("rejects another diagram type and an unterminated front matter", () => {
    expect(isState("graph TD\nA-->B")).toBe(false);
    expect(isState("sequenceDiagram\nA->>B: hi")).toBe(false);
    expect(isState("stateDiagrams\n[*] --> A")).toBe(false);
    expect(isState("---\ntitle: never closed\nstateDiagram-v2")).toBe(false);
  });
});

describe("selectSourcePaths for state machines", () => {
  it("keeps demo pages, the state syntax doc and both state e2e directories", () => {
    const tree = [
      "demos/state.html",
      "demos/flowchart.html",
      "packages/mermaid/src/docs/syntax/flowchart.md",
      "packages/mermaid/src/docs/syntax/stateDiagram.md",
      "e2e/rendering/flowchart/flowchart-v2.spec.js",
      "e2e/rendering/state/stateDiagram.spec.js",
      "e2e/rendering/state/stateDiagram-v2.spec.js",
      "e2e/diagrams/state-diagram/should-render-composite-states.mmd",
      "e2e/diagrams/state-diagram-v2/v2-should-render-forks-and-joins.mmd",
      "e2e/diagrams/state-diagram-v2/elk/elk-notes-keep-their-layout.mmd",
      "e2e/diagrams/state-diagram-v2/handdrawn/hd-1.mmd",
      "e2e/diagrams/class-diagram/1.mmd",
      "e2e/platform/dev-diagrams/diagrams/state-diagram/1-simple-state-diagram.mmd",
    ];
    expect(selectSourcePaths(tree, "state")).toEqual([
      "demos/flowchart.html",
      "demos/state.html",
      "e2e/diagrams/state-diagram-v2/elk/elk-notes-keep-their-layout.mmd",
      "e2e/diagrams/state-diagram-v2/v2-should-render-forks-and-joins.mmd",
      "e2e/diagrams/state-diagram/should-render-composite-states.mmd",
      "e2e/rendering/state/stateDiagram-v2.spec.js",
      "e2e/rendering/state/stateDiagram.spec.js",
      "packages/mermaid/src/docs/syntax/stateDiagram.md",
    ]);
  });

  it("names a state source after its directory, dropping mermaid's v2 prefix", () => {
    expect(sourceSlug("e2e/diagrams/state-diagram-v2/v2-should-render-forks-and-joins.mmd")).toBe(
      "e2e-v2-should-render-forks-and-joins",
    );
    expect(sourceSlug("e2e/diagrams/state-diagram/should-render-composite-states.mmd")).toBe(
      "e2e-should-render-composite-states",
    );
    expect(sourceSlug("e2e/rendering/state/stateDiagram-v2.spec.js")).toBe("e2e-state-v2");
    expect(sourceSlug("packages/mermaid/src/docs/syntax/stateDiagram.md")).toBe("docs-statediagram");
  });
});

describe("extractDiagrams", () => {
  it("keeps sequence diagrams when asked for them", () => {
    const html = `<pre class="mermaid">sequenceDiagram\nA->>B: hi</pre><pre class="mermaid">graph TD\nA</pre>`;
    expect(extractDiagrams("a.html", html, "sequence")).toEqual(["sequenceDiagram\nA->>B: hi"]);
    expect(extractDiagrams("a.html", html, "flowchart")).toEqual(["graph TD\nA"]);
    expect(extractDiagrams("a.mmd", "\n  sequenceDiagram\n    A->>B: hi\n", "sequence")).toEqual([
      "sequenceDiagram\n  A->>B: hi",
    ]);
    // A bare header with no statements is not a diagram.
    expect(extractDiagrams("a.mmd", "sequenceDiagram", "sequence")).toEqual([]);
  });

  it("dispatches on the file type and keeps flowcharts only", () => {
    expect(extractDiagrams("a.html", `<pre class="mermaid">graph TD\nA</pre><pre class="mermaid">pie\n"a": 1</pre>`)).toEqual([
      "graph TD\nA",
    ]);
    expect(extractDiagrams("a.md", "```mermaid\nflowchart LR\nB\n```")).toEqual(["flowchart LR\nB"]);
    expect(extractDiagrams("a.spec.ts", "f(`graph TD\n  C`); g(`not a diagram`)")).toEqual(["graph TD\n  C"]);
    expect(extractDiagrams("a.mmd", "\n  graph TD\n    D\n")).toEqual(["graph TD\n  D"]);
    expect(extractDiagrams("a.mmd", "sequenceDiagram")).toEqual([]);
    // A header with no statements is not a diagram (e.g. a string later concatenated in a test).
    expect(extractDiagrams("a.spec.ts", "const base = `flowchart`;")).toEqual([]);
    expect(extractDiagrams("a.txt", "graph TD")).toEqual([]);
  });
});

describe("sourceSlug", () => {
  it("builds a short file-name-safe slug from a repository path", () => {
    expect(sourceSlug("demos/flowchart.html")).toBe("demos-flowchart");
    expect(sourceSlug("packages/mermaid/src/docs/syntax/flowchart.md")).toBe("docs-flowchart");
    expect(sourceSlug("e2e/rendering/flowchart/flowchart-v2.spec.js")).toBe("e2e-flowchart-v2");
    expect(sourceSlug("e2e/diagrams/flowchart/dagre/7-Some Name.mmd")).toBe("e2e-dagre-7-some-name");
  });
  it("shortens the sequence sources the same way", () => {
    expect(sourceSlug("demos/sequence.html")).toBe("demos-sequence");
    expect(sourceSlug("packages/mermaid/src/docs/syntax/sequenceDiagram.md")).toBe("docs-sequencediagram");
    expect(sourceSlug("e2e/rendering/sequence/sequencediagram-v2.spec.js")).toBe("e2e-sequence-v2");
    expect(sourceSlug("e2e/diagrams/sequence/should-render-a-simple-sequence-diagram.mmd")).toBe(
      "e2e-should-render-a-simple-sequence-diagram",
    );
  });
});
