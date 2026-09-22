import { test } from "node:test";
import assert from "node:assert/strict";
import { VFile } from "vfile";
import rehypeMerlion, { diagramTitle } from "../index.js";
import { idPrefix } from "../fnv.js";
import { el, text, root, mermaidBlock, fakeRender } from "./helpers.mjs";

const SRC = "flowchart LR\n  a --> b\n";

const run = async (tree, options = {}, file = new VFile({ path: "/site/docs/page.md", cwd: "/site" })) => {
  const transform = rehypeMerlion({ fontCss: true, ...options });
  await transform(tree, file);
  return { tree, file };
};

test("replaces pre>code.language-mermaid with the figure structure", async () => {
  const { render, calls } = fakeRender();
  const { tree } = await run(root(mermaidBlock(SRC)), { render });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].source, SRC);
  const [figure] = tree.children;
  assert.equal(figure.tagName, "figure");
  assert.deepEqual(figure.properties, { id: "diagram-1", className: ["merlion-figure"] });
  const [view, details] = figure.children;
  assert.equal(view.tagName, "merlion-view");
  assert.equal(view.children.length, 1);
  assert.equal(view.children[0].type, "raw");
  assert.match(view.children[0].value, /^<svg /);
  // No accTitle or title: no figcaption.
  assert.equal(details.tagName, "details");
  const [summary, pre] = details.children;
  assert.deepEqual(summary, el("summary", {}, [text("Diagram source")]));
  assert.equal(pre.tagName, "pre");
  const code = pre.children[0];
  assert.deepEqual(code.properties, { className: ["language-mermaid"] });
  assert.deepEqual(code.children, [text(SRC)]);
});

test("passes width, strict and a per-file id_prefix to the renderer", async () => {
  const { render, calls } = fakeRender();
  await run(root(mermaidBlock(SRC), el("p", {}, [text("x")]), mermaidBlock(SRC)), {
    render,
    width: 600,
    strict: true,
  });
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0].options, { target_width: 600, strict: true, id_prefix: idPrefix("docs/page.md", 1) });
  assert.equal(calls[1].options.id_prefix, idPrefix("docs/page.md", 2));
});

test("numbers figures per file and leaves other code blocks alone", async () => {
  const { render } = fakeRender();
  const js = el("pre", {}, [el("code", { className: ["language-js"] }, [text("1")])]);
  const nested = el("section", {}, [mermaidBlock(SRC)]);
  const { tree } = await run(root(mermaidBlock(SRC), js, nested), { render });
  assert.equal(tree.children[0].properties.id, "diagram-1");
  assert.equal(tree.children[1], js);
  assert.equal(tree.children[2].children[0].properties.id, "diagram-2");
});

test("accepts a className string and extra classes on the code element", async () => {
  const { render, calls } = fakeRender();
  const block = el("pre", {}, [el("code", { className: "hljs language-mermaid" }, [text(SRC)])]);
  const { tree } = await run(root(block), { render });
  assert.equal(calls.length, 1);
  assert.equal(tree.children[0].tagName, "figure");
});

test("the figcaption carries accTitle, else the front-matter title, as a text node", async () => {
  const { render } = fakeRender();
  const src = "flowchart LR\n  accTitle: Build <pipeline> & cache\n  a --> b\n";
  const { tree } = await run(root(mermaidBlock(src)), { render });
  const cap = tree.children[0].children[1];
  assert.deepEqual(cap, el("figcaption", {}, [text("Build <pipeline> & cache")]));

  assert.equal(diagramTitle('---\ntitle: "Release flow"\n---\nflowchart LR\n a-->b'), "Release flow");
  assert.equal(diagramTitle("---\ntitle: Plain\n---\nflowchart LR\n accTitle: Wins\n"), "Wins");
  assert.equal(diagramTitle("flowchart LR\n a-->b"), null);
  assert.equal(diagramTitle("flowchart LR\n title: not front matter"), null);
});

test("source: 'none' drops the <details>; viewer: false drops <merlion-view>", async () => {
  const { render } = fakeRender();
  const { tree } = await run(root(mermaidBlock(SRC)), { render, source: "none", viewer: false });
  const figure = tree.children[0];
  assert.equal(figure.children.length, 1);
  assert.equal(figure.children[0].type, "raw");
});

test("a parse error keeps the code block and reports through file.message", async () => {
  const { render } = fakeRender();
  const block = mermaidBlock("flowchart LR\n  a -->> BROKEN\n", 10);
  const { tree, file } = await run(root(block), { render });
  assert.equal(tree.children[0], block);
  assert.equal(file.messages.length, 1);
  const [m] = file.messages;
  assert.equal(m.ruleId, "E002");
  assert.equal(m.source, "merlion");
  assert.equal(m.fatal, false);
  // Fence on line 10; diagnostic line 2 of the block is file line 12.
  assert.equal(m.line, 12);
  assert.equal(m.column, 5);
  assert.match(m.reason, /unexpected token/);
});

test("with strict, a parse error fails the file after every block is reported", async () => {
  const { render, calls } = fakeRender();
  const tree = root(mermaidBlock("BROKEN"), mermaidBlock(SRC), mermaidBlock("BROKEN"));
  const file = new VFile({ path: "/site/a.md", cwd: "/site" });
  await assert.rejects(run(tree, { render, strict: true }, file), /2 Mermaid diagrams failed/);
  assert.equal(calls.length, 3);
  assert.equal(file.messages.filter((m) => m.ruleId === "E002").length, 2);
});

test("warnings from a successful render are reported without failing", async () => {
  const { render } = fakeRender();
  const { tree, file } = await run(root(mermaidBlock("WARN")), { render });
  assert.equal(tree.children[0].tagName, "figure");
  assert.equal(file.messages.length, 1);
  assert.equal(file.messages[0].ruleId, "W010");
  assert.equal(file.messages[0].fatal, false);
});

test("a renderer that throws is reported like a parse error", async () => {
  const render = () => {
    throw new Error("boom");
  };
  const block = mermaidBlock(SRC);
  const { tree, file } = await run(root(block), { render });
  assert.equal(tree.children[0], block);
  assert.match(file.messages[0].reason, /boom/);
});

test("warns once when fontCss is unset (font mode 'link')", async () => {
  const { render } = fakeRender();
  const transform = rehypeMerlion({ render });
  const f1 = new VFile({ path: "/site/a.md", cwd: "/site" });
  const f2 = new VFile({ path: "/site/b.md", cwd: "/site" });
  await transform(root(mermaidBlock(SRC)), f1);
  await transform(root(mermaidBlock(SRC)), f2);
  const warn = [...f1.messages, ...f2.messages].filter((m) => m.ruleId === "font-css");
  assert.equal(warn.length, 1);
  assert.match(warn[0].reason, /merlion-font\.css/);
});

test("no diagram: no font warning and the tree is untouched", async () => {
  const { render, calls } = fakeRender();
  const tree = root(el("p", {}, [text("hi")]));
  const before = structuredClone(tree);
  const file = new VFile({ path: "/site/a.md", cwd: "/site" });
  await rehypeMerlion({ render })(tree, file);
  assert.deepEqual(tree, before);
  assert.equal(calls.length, 0);
  assert.equal(file.messages.length, 0);
});

test("the outline hook receives each diagram's plain-text outline", async () => {
  const { render } = fakeRender();
  const seen = [];
  await run(root(mermaidBlock(SRC)), { render, outline: (o) => seen.push(o) });
  assert.equal(seen.length, 1);
  assert.equal(seen[0].index, 1);
  assert.equal(seen[0].path, "docs/page.md");
  assert.equal(seen[0].source, SRC);
  assert.match(seen[0].outline, /^outline of m/);
});

test("paths are relative to the root option, with / separators; a file without a path uses ''", async () => {
  const { render, calls } = fakeRender();
  await run(root(mermaidBlock(SRC)), { render, root: "/site/docs" });
  assert.equal(calls[0].options.id_prefix, idPrefix("page.md", 1));
  await run(root(mermaidBlock(SRC)), { render }, new VFile());
  assert.equal(calls[1].options.id_prefix, idPrefix("", 1));
});

test("deep trees are walked without recursion", async () => {
  const { render, calls } = fakeRender();
  let node = mermaidBlock(SRC);
  for (let i = 0; i < 20000; i++) node = el("div", {}, [node]);
  await run(root(node), { render });
  assert.equal(calls.length, 1);
});

test("without `render`, a missing @fractalboxdev/merlion-wasm fails with an install hint", async () => {
  const transform = rehypeMerlion({ fontCss: true });
  await assert.rejects(transform(root(mermaidBlock(SRC)), new VFile()), /merlion-wasm is not installed/);
});

test("rejects invalid options", () => {
  assert.throws(() => rehypeMerlion({ source: "inline" }), /source/);
  assert.throws(() => rehypeMerlion({ width: -1 }), /width/);
});
