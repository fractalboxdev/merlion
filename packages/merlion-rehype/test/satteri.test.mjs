// The Sätteri adapter (Astro 7's default Markdown processor): the same figures, ids,
// diagnostics and strict behaviour as the rehype plugin, driven through Sätteri's
// hast visitor API.
import { test } from "node:test";
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";
import { markdownToHtml } from "satteri";
import merlionSatteri from "../satteri.js";
import { idPrefix } from "../fnv.js";
import { fakeRender } from "./helpers.mjs";

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
  "```mermaid",
  "flowchart LR",
  "  c --> WARN",
  "```",
  "",
].join("\n");

const FILE = pathToFileURL("/site/docs/page.md");

const run = (options, extra = [], source = md) =>
  markdownToHtml(source, { hastPlugins: [merlionSatteri({ fontCss: true, root: "/site", ...options }), ...extra], fileURL: FILE });

test("renders each mermaid block to a figure with the rehype plugin's ids and structure", async () => {
  const { render, calls } = fakeRender();
  const messages = [];
  const { html } = await run({ render, onMessage: (m) => messages.push(m) });
  assert.equal(calls.length, 3);
  assert.deepEqual(
    calls.map((c) => c.options.idPrefix),
    [1, 2, 3].map((n) => idPrefix("docs/page.md", n)),
  );
  assert.match(html, /<figure id="diagram-1" class="merlion-figure"><merlion-view><svg /);
  assert.match(html, /<figcaption>A &lt;b&gt; &amp; c<\/figcaption>/);
  assert.match(html, /<details><summary>Diagram source<\/summary><pre><code class="language-mermaid">flowchart LR/);
  assert.match(html, /<figure id="diagram-3" class="merlion-figure">/);
  // The failed block stays a code block; the other code block is untouched.
  assert.ok(!html.includes('id="diagram-2"'));
  assert.match(html, /<pre><code class="language-mermaid">flowchart LR\n {2}a --&gt;&gt; BROKEN/);
  assert.match(html, /<code class="language-js">let x = 1;/);
  // Diagnostics carry the file and the line in the Markdown file.
  assert.deepEqual(
    messages.map((m) => [m.ruleId, m.line, m.fatal]),
    [
      ["E002", 15, false],
      ["W010", 19, false],
    ],
  );
  assert.ok(messages.every((m) => m.file === "/site/docs/page.md"));
});

test("runs before a later code-block plugin, which then sees only the <details> source", async () => {
  const { render } = fakeRender();
  const seen = [];
  const codeBlocks = {
    name: "code-blocks",
    element: {
      filter: ["pre"],
      visit(node, ctx) {
        seen.push(ctx.parent(node).tagName ?? "root");
      },
    },
  };
  await run({ render, onMessage() {} }, [codeBlocks]);
  // Two rendered blocks leave their source in <details>; BROKEN and js stay at the root.
  assert.deepEqual(seen.sort(), ["details", "details", "root", "root"]);
});

test("strict: a failed block throws after reporting", async () => {
  const { render } = fakeRender();
  const seen = [];
  await assert.rejects(run({ render, strict: true, onMessage: (m) => seen.push(m.ruleId) }), /page\.md: 1 Mermaid diagram failed to render/);
  assert.deepEqual(seen, ["E002", "W010", "merlion-strict"]);
});

test("without onMessage, warnings go to console.warn with file:line:col", async (t) => {
  const { render } = fakeRender();
  const warn = t.mock.method(console, "warn", () => {});
  await run({ render });
  const lines = warn.mock.calls.map((c) => c.arguments.join(" "));
  assert.ok(lines.some((l) => l.includes("/site/docs/page.md:15:5") && l.includes("E002")), lines.join("\n"));
});

test("width=<px> in the fence meta sets that block's width", async () => {
  const { render, calls } = fakeRender();
  await run({ render }, [], "```mermaid width=1600\nflowchart LR\n  a --> b\n```\n\n```mermaid\nflowchart LR\n  c --> d\n```\n");
  assert.deepEqual(
    calls.map((c) => c.options.width),
    [1600, 720],
  );
});

test("a page with no diagram is left alone and the renderer never loads", async () => {
  const { render, calls } = fakeRender();
  const { html } = await run({ render }, [], "# Only text\n\n```js\n1\n```\n");
  assert.equal(calls.length, 0);
  assert.match(html, /<code class="language-js">1/);
});
