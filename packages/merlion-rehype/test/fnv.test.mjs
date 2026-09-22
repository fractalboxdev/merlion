import { test } from "node:test";
import assert from "node:assert/strict";
import { fnv1a64, idPrefix, cacheKey, contentHash } from "../fnv.js";

test("FNV-1a 64 reference vectors", () => {
  assert.equal(fnv1a64(""), "cbf29ce484222325");
  assert.equal(fnv1a64("a"), "af63dc4c8601ec8c");
  assert.equal(fnv1a64("foobar"), "85944171f73967e8");
});

test("FNV-1a 64 hashes the UTF-8 bytes and always yields 16 hex digits", () => {
  assert.equal(fnv1a64("é"), fnv1a64(new TextDecoder().decode(new Uint8Array([0xc3, 0xa9]))));
  for (const s of ["", "x", "日本語", "a".repeat(1000)]) assert.match(fnv1a64(s), /^[0-9a-f]{16}$/);
});

test("idPrefix is m + the first 8 hex of FNV-1a 64 over path + '-' + n", () => {
  assert.equal(idPrefix("docs/intro.md", 1), "m" + fnv1a64("docs/intro.md-1").slice(0, 8));
  assert.equal(idPrefix("docs/intro.md", 1), idPrefix("docs/intro.md", 1));
  assert.notEqual(idPrefix("docs/intro.md", 1), idPrefix("docs/intro.md", 2));
  assert.notEqual(idPrefix("docs/a.md", 1), idPrefix("docs/b.md", 1));
  // Satisfies the core's id_prefix grammar [a-z][a-z0-9-]{0,31} (specs/svg-output.md).
  assert.match(idPrefix("x", 3), /^[a-z][a-z0-9-]{0,31}$/);
});

test("cacheKey is the full 16-hex FNV-1a 64 of the (path, index) pair", () => {
  assert.equal(cacheKey("docs/intro.md", 2), fnv1a64("docs/intro.md-2"));
  assert.match(cacheKey("../../etc/passwd", 1), /^[0-9a-f]{16}$/);
});

test("contentHash changes with the source and the options that shape the output", () => {
  const a = contentHash("graph TD; a-->b", { width: 720, strict: false });
  assert.equal(a, contentHash("graph TD; a-->b", { width: 720, strict: false }));
  assert.notEqual(a, contentHash("graph TD; a-->c", { width: 720, strict: false }));
  assert.notEqual(a, contentHash("graph TD; a-->b", { width: 600, strict: false }));
  assert.notEqual(a, contentHash("graph TD; a-->b", { width: 720, strict: true }));
});
