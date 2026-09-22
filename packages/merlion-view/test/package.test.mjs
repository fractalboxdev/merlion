import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

test("published package has no runtime dependencies (specs/supply-chain.md)", () => {
  assert.deepEqual(pkg.dependencies, {});
  assert.equal(pkg.peerDependencies, undefined);
  assert.ok(Array.isArray(pkg.files) && pkg.files.length > 0, "files is an allow-list");
});

test("the element module loads outside a browser and registers nothing", async () => {
  const mod = await import("../merlion-view.js");
  assert.equal(typeof mod.MerlionView, "function");
  assert.equal(globalThis.customElements, undefined);
});
