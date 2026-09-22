#!/usr/bin/env node
// Renders every gallery.json entry that has a `source` into its `file`, using
// the local @fractalboxdev/merlion-wasm build. Entries without a source
// (hand-drawn fixtures) are left alone.
//   node demo/build-gallery.mjs
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEMO = fileURLToPath(new URL(".", import.meta.url));
const WASM_DIR = join(DEMO, "..", "packages", "merlion-wasm");

let wasm;
try {
  wasm = await import(pathToFileURL(join(WASM_DIR, "index.js")).href);
  const file = readdirSync(WASM_DIR).find((n) => n.endsWith(".wasm"));
  if (!file) throw new Error("no .wasm file; build crates/merlion-wasm first");
  wasm.initSync(readFileSync(join(WASM_DIR, file)));
} catch (err) {
  console.error(`packages/merlion-wasm is not available: ${err.message}`);
  process.exit(1);
}

const entries = JSON.parse(readFileSync(join(DEMO, "gallery.json"), "utf8"));
let failed = 0;
for (const [i, e] of entries.entries()) {
  if (!e.source) continue;
  const out = join(DEMO, e.file);
  let hint;
  try {
    hint = readFileSync(out, "utf8");
  } catch {
    hint = undefined;
  }
  const res = wasm.render(e.source, { id_prefix: `g${i + 1}`, ...(hint ? { hint } : {}) });
  for (const d of res.diagnostics) {
    console.error(`${e.file}:${d.span?.line ?? 0}:${d.span?.column ?? 0}: ${d.severity} ${d.code} ${d.message}`);
  }
  if (res.svg === null) {
    failed++;
    continue;
  }
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, res.svg);
  console.log(`wrote ${e.file}`);
}
process.exit(failed ? 1 : 0);
