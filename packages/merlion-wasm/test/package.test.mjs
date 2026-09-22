// Supply-chain properties of the published package (specs/supply-chain.md).
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const dir = fileURLToPath(new URL("..", import.meta.url));
const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

test("no runtime dependencies", () => {
  assert.deepEqual(pkg.dependencies, {});
  assert.equal(pkg.optionalDependencies, undefined);
  assert.equal(pkg.bundleDependencies, undefined);
});

test("no install scripts", () => {
  for (const hook of ["preinstall", "install", "postinstall", "prepare", "prepack", "prepublish"]) {
    assert.equal(pkg.scripts?.[hook], undefined, hook);
  }
});

test("files is an allow-list of the shipped entry points", () => {
  assert.deepEqual(pkg.files, ["index.js", "index.d.ts", "worker.js", "merlion.wasm"]);
  for (const f of ["index.js", "index.d.ts", "worker.js"]) {
    assert.ok(existsSync(`${dir}/${f}`), f);
  }
});

test("merlion.wasmSha256 matches merlion.wasm when it has been built", (t) => {
  const wasm = `${dir}/merlion.wasm`;
  if (!existsSync(wasm)) {
    t.skip("merlion.wasm not built; run scripts/build-wasm.sh");
    return;
  }
  const sha = createHash("sha256").update(readFileSync(wasm)).digest("hex");
  assert.equal(pkg.merlion?.wasmSha256, sha);
});
