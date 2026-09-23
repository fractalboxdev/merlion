// Size budgets (specs/viewer.md#constraints, specs/interaction.md#performance-and-budgets):
// the base <merlion-view> ≤ 6 KB, the interact module ≤ 3.25 KB and its sequence and state
// modules ≤ 1.25 KB each,
// minified and gzipped. Each entry is bundled with esbuild on its own, gzipped at level 9, and
// fails above its budget. Every module loads on demand, so no bundle counts another.
import { build } from "esbuild";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

export const BUDGETS = [
  ["merlion-view", "../merlion-view.js", 6 * 1024, ["./interact.js"]],
  ["merlion-view/interact", "../interact.js", 3.25 * 1024, ["./merlion-view.js", "./interact-seq.js", "./interact-state.js"]],
  ["merlion-view/interact-seq", "../interact-seq.js", 1.25 * 1024, []],
  ["merlion-view/interact-state", "../interact-state.js", 1.25 * 1024, ["./interact-model.js"]],
];

let failed = false;
for (const [name, file, budget, external] of BUDGETS) {
  const out = await build({
    entryPoints: [fileURLToPath(new URL(file, import.meta.url))],
    bundle: true,
    minify: true,
    format: "esm",
    target: "es2022",
    external,
    write: false,
  });
  const min = out.outputFiles[0].contents;
  const gz = gzipSync(min, { level: 9 }).length;
  console.log(`${name}: ${min.length} B minified, ${gz} B min+gzip (budget ${budget} B)`);
  if (gz > budget) {
    console.error(`${name} exceeds its size budget by ${gz - budget} B`);
    failed = true;
  }
}
if (failed) process.exit(1);
