import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../merlion-themes.css", import.meta.url), "utf8");

// Semantic zoom hides labels only (specs/viewer.md#semantic-zoom): shapes, edges and
// cluster members stay drawn at every zoom level.
test("zoomed-out rules only ever target text", () => {
  const rules = css.replace(/\/\*[\s\S]*?\*\//g, "").split("}");
  for (const rule of rules) {
    const [selector, body = ""] = rule.split("{");
    if (!/zoomed-out|rank-limit/.test(selector ?? "")) continue;
    for (const sel of selector.split(",")) {
      assert.match(sel.trim(), /\btext$/, `semantic-zoom selector hides more than a label: ${sel.trim()}`);
    }
    assert.doesNotMatch(body, /display\s*:\s*none/);
  }
});
