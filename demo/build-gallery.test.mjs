import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { CURATED, bakeStylesheet, galleryEntries } from "./build-gallery.mjs";

const fixtures = readdirSync(new URL("../crates/merlion-render/tests/fixtures/flowcharts/", import.meta.url)).filter(
  (n) => n.endsWith(".mmd"),
);

test("the gallery lists the showcase, every fixture and twelve compat diagrams", () => {
  const entries = galleryEntries();
  const by = (origin) => entries.filter((e) => e.origin === origin);
  assert.equal(entries[0].origin, "showcase");
  assert.equal(by("fixture").length, fixtures.length);
  assert.equal(CURATED.length, 12);
  assert.equal(by("compat").length, 12);
  for (const e of [...by("fixture"), ...by("compat")]) {
    assert.match(e.file, /^gallery\/[a-z0-9-]+\.svg$/);
    assert.ok(e.source.length > 0, e.file);
  }
  assert.equal(new Set(entries.map((e) => e.file)).size, entries.length, "files are unique");
});

test("every showcase entry is rendered from source; the wide pipeline is a real render of 11 steps", () => {
  const showcase = galleryEntries().filter((e) => e.origin === "showcase");
  for (const e of showcase) assert.ok(e.source, `${e.title} has a source`);
  const wide = showcase.find((e) => /wide pipeline/i.test(e.title));
  assert.ok(wide, "the wide pipeline stays in the showcase");
  assert.equal(wide.file, "gallery/wide-pipeline.svg");
  // Wide enough that the chain stays on one row, as the viewer fixture needs.
  assert.deepEqual(wide.options, { width: 1600 });
  const steps = wide.source.split("\n").find((l) => l.includes("-->")).split("-->").map((s) => s.trim());
  assert.equal(steps.length, 11, steps.join(" | "));
  assert.ok(!existsSync(new URL("./fixtures/wide-pipeline.svg", import.meta.url)), "the hand-drawn fixture is gone");
});

test("the roles section uses built-in and stylesheet roles on nodes, edges and clusters", () => {
  const roles = galleryEntries().filter((e) => e.origin === "roles");
  assert.equal(roles.length, 1);
  const [e] = roles;
  for (const line of ["class cron accent", "class dlq danger", "class e2 failure", "class e1 async", "class wf group", "class q queue", "class consumers senders"]) {
    assert.ok(e.source.includes(line), line);
  }
  assert.deepEqual(
    e.baked.map((b) => b.theme),
    ["light", "dark"],
  );
  for (const b of e.baked) assert.match(b.file, /^gallery\/[a-z0-9-]+\.svg$/);
});

const wasmFile = new URL("../packages/merlion-wasm/merlion.wasm", import.meta.url);

test("demo/stylesheet.compiled.css is the compiled demo/stylesheet.css and the page links it", async (t) => {
  if (!existsSync(wasmFile)) return t.skip("merlion.wasm not built");
  const wasm = await import("../packages/merlion-wasm/index.js");
  wasm.initSync(readFileSync(wasmFile));
  const src = readFileSync(new URL("./stylesheet.css", import.meta.url), "utf8");
  const out = wasm.compileStylesheet(src, { strict: true });
  assert.deepEqual(out.diagnostics, []);
  assert.equal(readFileSync(new URL("./stylesheet.compiled.css", import.meta.url), "utf8"), out.css);
  const html = readFileSync(new URL("./index.html", import.meta.url), "utf8");
  const themes = html.indexOf('href="../packages/merlion-themes/merlion-themes.css"');
  const compiled = html.indexOf('href="stylesheet.compiled.css"');
  assert.ok(themes > 0 && compiled > themes, "compiled CSS linked after the theme tokens");
  assert.ok(!html.includes('href="stylesheet.css"'), "the source stylesheet is never linked");
  // The baked themes come from merlion-themes.css plus the demo stylesheet, with no warning.
  const both = bakeStylesheet();
  for (const theme of ["light", "dark"]) {
    const r = wasm.compileStylesheet(both, { theme, strict: true });
    assert.ok(r.palette && r.diagnostics.every((d) => d.severity === "info"), theme);
  }
});
