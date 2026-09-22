// The `stylesheet` option (specs/integrations.md#fractalboxdevmerlion-rehype): read under
// the CLI's file-handling rules, compiled once per build, exposed as file.data.merlion.css
// and never passed to inline renders.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, realpathSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { VFile } from "vfile";
import rehypeMerlion from "../index.js";
import { readStylesheet, StylesheetRefused } from "../stylesheet.js";
import { root, mermaidBlock, fakeRender } from "./helpers.mjs";

const SRC = "flowchart LR\n  a --> b\n";
const SHEET = ':root { --merlion-accent: #0f766e; }\n.merlion-c-store { --merlion-tone: #b8408f; }\n.viewer { color: red; }\n';

const site = () => {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), "merlion-rehype-css-")));
  mkdirSync(join(dir, "docs"));
  return dir;
};

/** A fake `compileStylesheet` that records its calls. */
const fakeCompile = () => {
  const calls = [];
  const compileStylesheet = (css, options) => {
    calls.push({ css, options });
    const warn = css.includes("--merlion-font")
      ? [{ severity: options?.strict ? "error" : "warning", code: "W018", line: 1, column: 8, byteStart: 7, byteEnd: 20, message: "font tokens are not allowed", fix: null }]
      : [];
    const failed = warn.some((d) => d.severity === "error");
    return { css: failed ? null : `/* compiled */\n${css.length}\n`, palette: null, diagnostics: warn };
  };
  return { compileStylesheet, calls };
};

const file = (dir, name = "docs/page.md") => new VFile({ path: join(dir, name), cwd: dir });

test("compiles the stylesheet once per build and exposes it on files with a diagram", async () => {
  const dir = site();
  writeFileSync(join(dir, "diagram.css"), SHEET);
  const { render, calls: renders } = fakeRender();
  const { compileStylesheet, calls } = fakeCompile();
  const transform = rehypeMerlion({ fontCss: true, render, compileStylesheet, root: dir, stylesheet: "diagram.css" });
  const a = file(dir, "docs/a.md");
  const b = file(dir, "docs/b.md");
  const none = file(dir, "docs/none.md");
  await transform(root(mermaidBlock(SRC)), a);
  await transform(root(mermaidBlock(SRC)), b);
  await transform(root(), none);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].css, SHEET);
  assert.deepEqual(calls[0].options, { strict: false });
  assert.equal(a.data.merlion.css, `/* compiled */\n${SHEET.length}\n`);
  assert.equal(b.data.merlion.css, a.data.merlion.css);
  assert.equal(none.data.merlion, undefined);
  // Inline renders never receive a palette.
  for (const c of renders) assert.ok(!("palette" in c.options), JSON.stringify(c.options));
});

test("stylesheet warnings are reported once; strict fails the file", async () => {
  const dir = site();
  writeFileSync(join(dir, "diagram.css"), ":root { --merlion-font: Comic; }\n");
  const { render } = fakeRender();
  const lax = rehypeMerlion({ fontCss: true, render, compileStylesheet: fakeCompile().compileStylesheet, root: dir, stylesheet: "diagram.css" });
  const a = file(dir, "docs/a.md");
  const b = file(dir, "docs/b.md");
  await lax(root(mermaidBlock(SRC)), a);
  await lax(root(mermaidBlock(SRC)), b);
  const w = a.messages.filter((m) => m.ruleId === "W018");
  assert.equal(w.length, 1);
  assert.match(String(w[0].reason), /diagram\.css:1:8/);
  assert.equal(b.messages.filter((m) => m.ruleId === "W018").length, 0);
  assert.ok(typeof a.data.merlion.css === "string");
  const strict = rehypeMerlion({ fontCss: true, render, compileStylesheet: fakeCompile().compileStylesheet, root: dir, stylesheet: "diagram.css", strict: true });
  await assert.rejects(strict(root(mermaidBlock(SRC)), file(dir)), /W018/);
});

test("the stylesheet path follows the file-handling rules", async () => {
  const dir = site();
  const outside = realpathSync(mkdtempSync(join(tmpdir(), "merlion-rehype-outside-")));
  writeFileSync(join(outside, "secret.css"), ":root{--merlion-bg:#fff}\nGITHUB_TOKEN=ghs_secretvalue123\n");
  symlinkSync(join(outside, "secret.css"), join(dir, "link.css"));
  writeFileSync(join(dir, "big.css"), `:root{--merlion-bg:#fff}/*${"x".repeat(64 * 1024)}*/`);
  writeFileSync(join(dir, "bin.css"), Buffer.from([0xff, 0xfe]));
  mkdirSync(join(dir, "dir.css"));
  for (const [name, why] of [
    ["link.css", /symbolic link/],
    [join(outside, "secret.css"), /outside the project root/],
    ["../x.css", /outside the project root/],
    ["missing.css", /cannot read/],
    ["dir.css", /not a regular file/],
    ["bin.css", /not UTF-8/],
  ]) {
    assert.throws(() => readStylesheet(dir, name), (e) => e instanceof StylesheetRefused && why.test(e.message), name);
  }
  assert.throws(() => readStylesheet(dir, "big.css"), (e) => e instanceof StylesheetRefused && e.code === "E013");
  // Through the plugin: reported on the file, never echoed, fatal under strict.
  const { render } = fakeRender();
  const { compileStylesheet, calls } = fakeCompile();
  const f = file(dir);
  await rehypeMerlion({ fontCss: true, render, compileStylesheet, root: dir, stylesheet: "link.css" })(root(mermaidBlock(SRC)), f);
  assert.equal(calls.length, 0);
  assert.equal(f.data.merlion, undefined);
  assert.ok(f.messages.some((m) => m.ruleId === "stylesheet" && /symbolic link/.test(m.reason)));
  assert.ok(!f.messages.some((m) => String(m.reason).includes("ghs_")));
  const g = file(dir);
  await rehypeMerlion({ fontCss: true, render, compileStylesheet, root: dir, stylesheet: "big.css" })(root(mermaidBlock(SRC)), g);
  assert.ok(g.messages.some((m) => m.ruleId === "E013"));
  await assert.rejects(
    rehypeMerlion({ fontCss: true, render, compileStylesheet, root: dir, stylesheet: "big.css", strict: true })(root(mermaidBlock(SRC)), file(dir)),
    /E013/,
  );
});

test("rejects a stylesheet option that is not a string", () => {
  assert.throws(() => rehypeMerlion({ stylesheet: 3 }), /stylesheet/);
  assert.throws(() => rehypeMerlion({ compileStylesheet: 3 }), /compileStylesheet/);
});

test("compiles through the real WASM module", async () => {
  const dir = site();
  writeFileSync(join(dir, "diagram.css"), SHEET);
  const f = file(dir);
  await rehypeMerlion({ fontCss: true, root: dir, stylesheet: "diagram.css" })(root(mermaidBlock(SRC)), f);
  assert.equal(
    f.data.merlion.css,
    ":root {\n  --merlion-accent: #0f766e;\n}\n.merlion .merlion-c-store {\n  --merlion-tone: #b8408f;\n}\n",
  );
  // The token-free rule is an info diagnostic: not reported.
  assert.ok(!f.messages.some((m) => m.ruleId === "I032"));
});
