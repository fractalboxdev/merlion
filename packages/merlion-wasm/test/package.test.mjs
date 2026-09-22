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

test("merlion.wasmSha256 records a SHA-256, and the release build reproduces it", (t) => {
  // `scripts/build-wasm.sh` writes the hash through `scripts/record-sha256.mjs`, so the committed
  // value is empty until a build records one: the field holds a hash or nothing, never a malformed
  // value. Only the release build reproduces an attested hash — a local build runs an unpinned
  // toolchain without SOURCE_DATE_EPOCH — so the file comparison runs under MERLION_RELEASE=1,
  // which the release workflow sets, and skips with its reason everywhere else
  // (specs/supply-chain.md#releases).
  const sha = pkg.merlion?.wasmSha256 ?? "";
  const release = process.env.MERLION_RELEASE === "1";
  assert.match(sha, release ? /^[0-9a-f]{64}$/ : /^(|[0-9a-f]{64})$/);
  if (!release) {
    t.skip("MERLION_RELEASE is not 1: a local build reproduces no attested hash");
    return;
  }
  const wasm = `${dir}/merlion.wasm`;
  assert.ok(existsSync(wasm), "merlion.wasm not built; run scripts/build-wasm.sh");
  assert.equal(createHash("sha256").update(readFileSync(wasm)).digest("hex"), sha);
});
