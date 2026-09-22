import { test, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, readdirSync, readFileSync, writeFileSync, symlinkSync, mkdirSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { VFile } from "vfile";
import rehypeMerlion from "../index.js";
import { cacheKey } from "../fnv.js";
import { root, mermaidBlock, fakeRender } from "./helpers.mjs";

const SRC = "flowchart LR\n  a --> b\n";
let dir;

beforeEach(() => {
  dir = realpathSync(mkdtempSync(join(tmpdir(), "merlion-rehype-")));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const build = async (src, render, extra = {}) => {
  const file = new VFile({ path: join(dir, "docs/page.md"), cwd: dir });
  const tree = root(mermaidBlock(src));
  await rehypeMerlion({ render, fontCss: true, cacheDir: ".merlion", root: dir, ...extra })(tree, file);
  return { tree, file };
};

const entryPath = () => join(dir, ".merlion", `${cacheKey("docs/page.md", 1)}.json`);

test("first build renders and writes one entry named by the (path, index) hash", async () => {
  const { render, calls } = fakeRender();
  await build(SRC, render);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].options.hint, undefined);
  assert.deepEqual(readdirSync(join(dir, ".merlion")), [`${cacheKey("docs/page.md", 1)}.json`]);
  const entry = JSON.parse(readFileSync(entryPath(), "utf8"));
  assert.equal(entry.v, 1);
  assert.match(entry.hash, /^[0-9a-f]{16}$/);
  assert.match(entry.svg, /^<svg /);
});

test("an unchanged source renders again with the stored SVG as the layout hint", async () => {
  const first = fakeRender();
  const a = await build(SRC, first.render);
  const before = JSON.parse(readFileSync(entryPath(), "utf8"));
  const second = fakeRender();
  const b = await build(SRC, second.render);
  assert.equal(second.calls.length, 1);
  assert.equal(second.calls[0].options.hint, before.svg);
  assert.equal(b.tree.children[0].children[0].children[0].value, a.tree.children[0].children[0].children[0].value);
});

test("a planted entry whose hash matches never reaches the page", async () => {
  const first = fakeRender();
  await build(SRC, first.render);
  const entry = JSON.parse(readFileSync(entryPath(), "utf8"));
  const planted = '<svg><foreignObject><img src=x onerror="alert(1)"></foreignObject></svg><script>alert(1)</script>';
  writeFileSync(entryPath(), JSON.stringify({ ...entry, svg: planted }));
  const second = fakeRender();
  const { tree } = await build(SRC, second.render);
  assert.equal(second.calls.length, 1);
  assert.equal(second.calls[0].options.hint, planted);
  const inlined = tree.children[0].children[0].children[0].value;
  assert.doesNotMatch(inlined, /script|onerror|foreignObject/);
});

test("a changed source renders with the stored SVG as the layout hint and updates the entry", async () => {
  const first = fakeRender();
  await build(SRC, first.render);
  const before = JSON.parse(readFileSync(entryPath(), "utf8"));
  const second = fakeRender();
  await build(SRC + "  b --> c\n", second.render);
  assert.equal(second.calls.length, 1);
  assert.equal(second.calls[0].options.hint, before.svg);
  const after = JSON.parse(readFileSync(entryPath(), "utf8"));
  assert.notEqual(after.hash, before.hash);
});

test("a failed render leaves the previous entry in place", async () => {
  const first = fakeRender();
  await build(SRC, first.render);
  const before = readFileSync(entryPath(), "utf8");
  await build("BROKEN", fakeRender().render);
  assert.equal(readFileSync(entryPath(), "utf8"), before);
});

test("writes are atomic: no temporary files remain", async () => {
  await build(SRC, fakeRender().render);
  await build(SRC + "x", fakeRender().render);
  assert.deepEqual(
    readdirSync(join(dir, ".merlion")).filter((f) => !f.endsWith(".json")),
    [],
  );
});

test("a corrupt or oversized entry is ignored and replaced", async () => {
  mkdirSync(join(dir, ".merlion"));
  writeFileSync(entryPath(), "{not json");
  const a = fakeRender();
  await build(SRC, a.render);
  assert.equal(a.calls.length, 1);
  assert.equal(a.calls[0].options.hint, undefined);
  assert.equal(JSON.parse(readFileSync(entryPath(), "utf8")).v, 1);

  writeFileSync(entryPath(), JSON.stringify({ v: 1, hash: "0", svg: "x".repeat((1 << 20) + 1) }));
  const b = fakeRender();
  await build(SRC, b.render);
  assert.equal(b.calls[0].options.hint, undefined);
});

test("an entry that is a symbolic link is neither read nor written through", async () => {
  mkdirSync(join(dir, ".merlion"));
  const target = join(dir, "outside.json");
  writeFileSync(target, JSON.stringify({ v: 1, hash: "0", svg: "<svg>planted</svg>" }));
  symlinkSync(target, entryPath());
  const r = fakeRender();
  await build(SRC, r.render);
  assert.equal(r.calls[0].options.hint, undefined);
  // The rename replaced the link; the link target is untouched.
  assert.match(readFileSync(target, "utf8"), /planted/);
  assert.equal(JSON.parse(readFileSync(entryPath(), "utf8")).v, 1);
});

test("a cacheDir that is a symbolic link or lies outside the root is refused with a warning", async () => {
  const outside = realpathSync(mkdtempSync(join(tmpdir(), "merlion-outside-")));
  try {
    symlinkSync(outside, join(dir, ".merlion"));
    const r = fakeRender();
    const { tree, file } = await build(SRC, r.render);
    assert.equal(tree.children[0].tagName, "figure");
    assert.deepEqual(readdirSync(outside), []);
    assert.equal(file.messages.filter((m) => m.ruleId === "cache-dir").length, 1);

    const r2 = fakeRender();
    const { file: f2 } = await build(SRC, r2.render, { cacheDir: outside });
    assert.deepEqual(readdirSync(outside), []);
    assert.equal(f2.messages.filter((m) => m.ruleId === "cache-dir").length, 1);
  } finally {
    rmSync(outside, { recursive: true, force: true });
  }
});

test("entries for different blocks and files never collide", async () => {
  const { render } = fakeRender();
  const file = new VFile({ path: join(dir, "a.md"), cwd: dir });
  await rehypeMerlion({ render, fontCss: true, cacheDir: ".merlion", root: dir })(
    root(mermaidBlock(SRC), mermaidBlock(SRC + " ")),
    file,
  );
  const names = readdirSync(join(dir, ".merlion")).sort();
  assert.deepEqual(names, [`${cacheKey("a.md", 1)}.json`, `${cacheKey("a.md", 2)}.json`].sort());
});
