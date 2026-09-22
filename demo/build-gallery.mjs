#!/usr/bin/env node
// Builds the demo gallery with the local @fractalboxdev/merlion-wasm build:
//
//   1. the hand-written entries of demo/showcase.json (an entry without `source` is a
//      committed fixture and is listed as is);
//   2. every flowchart fixture in crates/merlion-render/tests/fixtures/flowcharts/;
//   3. twelve curated diagrams of the mermaid compat corpus (bench/corpus/compat/).
//
// Each rendered entry is written to demo/gallery/<name>.svg, with the previous file as the
// layout hint so a rebuild keeps the drawing stable, and the list to demo/gallery.json.
//
//   node demo/build-gallery.mjs
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEMO = fileURLToPath(new URL(".", import.meta.url));
const REPO = join(DEMO, "..");
const WASM_DIR = join(REPO, "packages", "merlion-wasm");
const FIXTURES = join(REPO, "crates", "merlion-render", "tests", "fixtures", "flowcharts");
const COMPAT = join(REPO, "bench", "corpus", "compat");

/** Compat diagrams chosen for range: size, subgraphs, classes and styles, edge labels. */
export const CURATED = [
  "demos-flowchart-01",
  "demos-flowchart-09",
  "demos-flowchart-13",
  "demos-flowchart-23",
  "demos-flowchart-31",
  "demos-flowchart-38",
  "demos-flowchart-62",
  "demos-flowchart-elk-01",
  "docs-flowchart-08",
  "e2e-dagre-7213-should-render-dagre-edges-with-right-angles-not-curves",
  "e2e-flowchart-redux-color-subgraphs-01",
  "e2e-v2-18-should-render-nested-subgraphs-with-edge-from-cluster-containing-extractable-subgraph",
];

/** `ci-pipeline` → `Ci pipeline`. */
const titleOf = (stem) => stem.charAt(0).toUpperCase() + stem.slice(1).replaceAll("-", " ");

/** Every gallery entry, in display order: `{ title, file, source?, origin }`. */
export const galleryEntries = () => {
  const showcase = JSON.parse(readFileSync(join(DEMO, "showcase.json"), "utf8")).map((e) => ({
    ...e,
    origin: "showcase",
  }));
  const fixtures = readdirSync(FIXTURES)
    .filter((n) => n.endsWith(".mmd"))
    .sort()
    .map((n) => {
      const stem = basename(n, ".mmd");
      return {
        title: titleOf(stem),
        file: `gallery/fixture-${stem}.svg`,
        source: readFileSync(join(FIXTURES, n), "utf8"),
        origin: "fixture",
      };
    });
  const compat = CURATED.map((stem) => ({
    title: `mermaid corpus: ${stem}`,
    file: `gallery/compat-${stem}.svg`,
    source: readFileSync(join(COMPAT, `${stem}.mmd`), "utf8"),
    origin: "compat",
  }));
  return [...showcase, ...fixtures, ...compat];
};

const loadWasm = async () => {
  const wasm = await import(pathToFileURL(join(WASM_DIR, "index.js")).href);
  const file = join(WASM_DIR, "merlion.wasm");
  if (!existsSync(file)) throw new Error("no merlion.wasm; run sh packages/merlion-wasm/scripts/build-wasm.sh");
  wasm.initSync(readFileSync(file));
  return wasm;
};

const main = async () => {
  let wasm;
  try {
    wasm = await loadWasm();
  } catch (err) {
    console.error(`packages/merlion-wasm is not available: ${err.message}`);
    return 1;
  }
  const entries = galleryEntries();
  mkdirSync(join(DEMO, "gallery"), { recursive: true });
  let failed = 0;
  for (const [i, e] of entries.entries()) {
    if (!e.source) continue;
    const out = join(DEMO, e.file);
    const hint = existsSync(out) ? readFileSync(out, "utf8") : undefined;
    const res = wasm.render(e.source, { idPrefix: `g${i + 1}`, ...(hint ? { hint } : {}) });
    for (const d of res.diagnostics) {
      if (d.severity === "info") continue;
      console.error(`${e.file}:${d.line}:${d.column}: ${d.severity} ${d.code} ${d.message}`);
    }
    if (res.svg === null) {
      failed++;
      continue;
    }
    writeFileSync(out, res.svg);
  }
  const listed = entries.map(({ title, file, source, origin }) => ({ title, file, ...(source ? { source } : {}), origin }));
  writeFileSync(join(DEMO, "gallery.json"), `${JSON.stringify(listed, null, 2)}\n`);
  console.log(`gallery: ${entries.length - failed} of ${entries.length} entries, ${failed} failed`);
  return failed ? 1 : 0;
};

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) process.exitCode = await main();
