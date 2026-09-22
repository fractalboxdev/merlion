import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync } from "node:fs";
import { CURATED, galleryEntries } from "./build-gallery.mjs";

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
