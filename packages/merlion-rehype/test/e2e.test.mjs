// Markdown → HTML through unified: first with a fake renderer, then over the real
// @fractalboxdev/merlion-wasm module (the plugin's default renderer).
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
