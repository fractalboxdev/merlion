// Size budget for <merlion-view>: ≤ 6 KB minified and gzipped (specs/viewer.md#constraints).
// Bundles the element with esbuild, gzips at level 9 and fails above the budget.
import { build } from "esbuild";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

export const BUDGET = 6 * 1024;

const entry = fileURLToPath(new URL("../merlion-view.js", import.meta.url));
const out = await build({
  entryPoints: [entry],
  bundle: true,
  minify: true,
  format: "esm",
  target: "es2022",
  write: false,
});
const min = out.outputFiles[0].contents;
const gz = gzipSync(min, { level: 9 }).length;
console.log(`merlion-view: ${min.length} B minified, ${gz} B min+gzip (budget ${BUDGET} B)`);
if (gz > BUDGET) {
  console.error(`merlion-view exceeds its size budget by ${gz - BUDGET} B`);
  process.exit(1);
}
