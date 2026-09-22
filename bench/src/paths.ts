/** Fixed locations inside the repository, resolved from this module's location. */
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const BENCH_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const REPO_DIR = resolve(BENCH_DIR, "..");
export const COMPAT_DIR = join(BENCH_DIR, "corpus", "compat");
export const EDITS_DIR = join(BENCH_DIR, "corpus", "edits");
export const RESULTS_DIR = join(BENCH_DIR, "results");
export const MERLION_BIN = join(REPO_DIR, "target", "release", "merlion");
export const MERLION_WASM_DIR = join(REPO_DIR, "packages", "merlion-wasm");
export const MERMAID_DIST = join(BENCH_DIR, "node_modules", "mermaid", "dist", "mermaid.min.js");
