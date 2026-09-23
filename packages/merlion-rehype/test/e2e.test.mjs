// Markdown → HTML through unified: first with a fake renderer, then over the real
// @fractalbox/merlion-wasm module (the plugin's default renderer).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkRehype from "remark-rehype";
import rehypeStringify from "rehype-stringify";
import { VFile } from "vfile";
import rehypeMerlion from "../index.js";
import { idPrefix } from "../fnv.js";
import { fakeRender } from "./helpers.mjs";

const pipeline = (options) =>
  unified()
    .use(remarkParse)
    .use(remarkRehype)
    .use(rehypeMerlion, { fontCss: true, ...options })
    .use(rehypeStringify, { allowDangerousHtml: true });

const md = [
  "# Title",
  "",
  "```mermaid",
  "flowchart LR",
  "  accTitle: A <b> & c",
  "  a --> b",
  "```",
  "",
  "```js",
  "let x = 1;",
  "```",
  "",
  "```mermaid",
  "flowchart LR",
  "  a -->> BROKEN",
  "```",
  "",
].join("\n");

test("markdown with mermaid fences renders figures and escapes caption and source", async () => {
  const { render } = fakeRender();
  const file = await pipeline({ render }).process(new VFile({ value: md, path: "/site/docs/x.md", cwd: "/site" }));
  const html = String(file);
  const id = idPrefix("docs/x.md", 1);
  assert.ok(
    html.includes(
      `<figure id="diagram-1" class="merlion-figure"><merlion-view><svg xmlns="http://www.w3.org/2000/svg" id="${id}"`,
    ),
    html,
  );
  assert.ok(html.includes("<figcaption>A &#x3C;b> &#x26; c</figcaption>"), html);
  assert.ok(
    html.includes(
      '<details><summary>Diagram source</summary><pre><code class="language-mermaid">flowchart LR\n  accTitle: A &#x3C;b> &#x26; c\n  a --> b\n</code></pre></details></figure>',
    ),
    html,
  );
  assert.ok(html.includes('<code class="language-js">'), "other fences stay");
  // The broken block stays a code block and its diagnostic points into the file.
  assert.ok(html.includes('<pre><code class="language-mermaid">flowchart LR\n  a -->> BROKEN\n</code></pre>'), html);
  const [m] = file.messages;
  assert.equal(m.ruleId, "E002");
  assert.equal(m.line, 15);
});

test("renders a sequence diagram through the real WASM, with its outline", async () => {
  const doc = [
    "# Checkout",
    "",
    "```mermaid",
    "sequenceDiagram",
    "  accTitle: Placing an order",
    "  actor Customer",
    "  participant API as API gateway",
    "  Customer->>+API: POST /orders",
    "  loop Every minute",
    "    API-->>-Customer: 202 Accepted",
    "  end",
    "```",
    "",
  ].join("\n");
  const outlines = [];
  const file = await pipeline({ width: 480, outline: (info) => outlines.push(info) }).process(
    new VFile({ value: doc, path: "/site/docs/seq.md", cwd: "/site" }),
  );
  const html = String(file);
  const id = idPrefix("docs/seq.md", 1);
  assert.ok(html.includes(`<figure id="diagram-1" class="merlion-figure"><merlion-view><svg`), html.slice(0, 300));
  assert.ok(html.includes(`class="merlion merlion-sequence"`), "the root names the diagram type");
  assert.ok(html.includes(`<title id="${id}-title">Placing an order</title>`), "accTitle becomes the SVG title");
  assert.ok(html.includes('data-merlion-id="Customer"'), "participants carry their ids");
  assert.ok(html.includes('data-merlion-index="0"'), "messages carry their index");
  assert.ok(html.includes('class="merlion-activation"'), "the activation bar is drawn");
  assert.ok(html.includes("<figcaption>Placing an order</figcaption>"), html.slice(-400));
  // The source stays available below the figure, unrendered.
  assert.ok(html.includes('<details><summary>Diagram source</summary><pre><code class="language-mermaid">sequenceDiagram'), html.slice(-600));
  assert.deepEqual(file.messages.filter((m) => /^E\d{3}$/.test(String(m.ruleId))), []);
  // The outline hook is what a `.md` mirror and llms-full.txt publish (specs/integrations.md).
  assert.equal(outlines.length, 1);
  assert.equal(outlines[0].path, "docs/seq.md");
  assert.ok(outlines[0].outline.startsWith("Sequence diagram. 2 participants, 2 messages."), outlines[0].outline);
  assert.ok(outlines[0].outline.includes("loop Every minute:"), outlines[0].outline);
});

test("renders a state diagram through the real WASM, with its outline", async () => {
  const doc = [
    "# Lifecycle",
    "",
    "```mermaid",
    "stateDiagram-v2",
    "  accTitle: Order lifecycle",
    "  [*] --> Draft",
    "  Draft --> Review : submit",
    "  state Review {",
    "    [*] --> Editing",
    "    Editing --> Approved",
    "  }",
    "  Review --> Published : approve",
    "  note right of Published : The feed picks it up within a minute.",
    "  Published --> [*]",
    "```",
    "",
  ].join("\n");
  const outlines = [];
  const file = await pipeline({ width: 480, outline: (info) => outlines.push(info) }).process(
    new VFile({ value: doc, path: "/site/docs/state.md", cwd: "/site" }),
  );
  const html = String(file);
  const id = idPrefix("docs/state.md", 1);
  assert.ok(html.includes(`<figure id=\"diagram-1\" class=\"merlion-figure\"><merlion-view><svg`), html.slice(0, 300));
  assert.ok(html.includes(`class=\"merlion merlion-state\"`), "the root names the diagram type");
  assert.ok(html.includes(`<title id=\"${id}-title\">Order lifecycle</title>`), "accTitle becomes the SVG title");
  assert.ok(html.includes('data-merlion-id=\"Draft\"'), "states carry their ids");
  assert.ok(html.includes('data-merlion-kind=\"start\"'), "a pseudo-state names its kind");
  assert.ok(html.includes('class=\"merlion-cluster merlion-composite'), "the composite state is a cluster");
  assert.ok(html.includes('class=\"merlion-note\"'), "the note is drawn");
  assert.ok(html.includes("<figcaption>Order lifecycle</figcaption>"), html.slice(-400));
  assert.ok(html.includes('<details><summary>Diagram source</summary><pre><code class=\"language-mermaid\">stateDiagram-v2'), html.slice(-600));
  assert.deepEqual(file.messages.filter((m) => /^E\d{3}$/.test(String(m.ruleId))), []);
  assert.equal(outlines.length, 1);
  assert.equal(outlines[0].path, "docs/state.md");
  assert.ok(outlines[0].outline.startsWith("State diagram, top to bottom."), outlines[0].outline);
  assert.ok(outlines[0].outline.includes("Draft → Review [submit]"), outlines[0].outline);
});

test("strict fails the process on a parse error", async () => {
  const { render } = fakeRender();
  await assert.rejects(pipeline({ render, strict: true }).process(md), /1 Mermaid diagram failed/);
});

test("published package has no runtime dependencies (specs/supply-chain.md)", () => {
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.deepEqual(pkg.dependencies, {});
  for (const f of pkg.files) assert.doesNotThrow(() => readFileSync(new URL(`../${f}`, import.meta.url)), f);
});

test("renders a markdown document through the real WASM module", async () => {
  const doc = [
    "# Pipeline",
    "",
    "```mermaid",
    "flowchart LR",
    "  accTitle: Build pipeline",
    "  src[Source] --> build[Build] --> ship[Ship]",
    "```",
    "",
    "```mermaid",
    "flowchart LR",
    "  a -->",
    "```",
    "",
  ].join("\n");
  const file = await pipeline({ width: 480 }).process(
    new VFile({ value: doc, path: "/site/docs/real.md", cwd: "/site" }),
  );
  const html = String(file);
  const id = idPrefix("docs/real.md", 1);
  assert.ok(
    html.includes(`<figure id="diagram-1" class="merlion-figure"><merlion-view><svg xmlns="http://www.w3.org/2000/svg" id="${id}"`),
    html.slice(0, 400),
  );
  assert.ok(html.includes(`<title id="${id}-title">Build pipeline</title>`), "accTitle becomes the SVG title");
  assert.ok(html.includes('data-merlion-id="build"'), "nodes carry their ids");
  assert.ok(/max-width:\d+(\.\d+)?px/.test(html));
  assert.ok(html.includes("<figcaption>Build pipeline</figcaption>"));
  // The second block fails to parse: it stays a code block and E002 points into the file.
  assert.ok(html.includes('<pre><code class="language-mermaid">flowchart LR\n  a -->\n</code></pre>'), html);
  const errors = file.messages.filter((m) => /^E\d{3}$/.test(String(m.ruleId)));
  assert.equal(errors.length, 1, file.messages.map(String).join("\n"));
  assert.equal(errors[0].ruleId, "E002");
  assert.equal(errors[0].line, 11);
});
