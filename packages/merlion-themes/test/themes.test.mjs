import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gzipSync } from "node:zlib";

const css = readFileSync(new URL("../merlion-themes.css", import.meta.url), "utf8");

/** Declarations of every rule whose selector list contains `selector`. */
const tokensOf = (selector) => {
  const out = {};
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const sels = m[1].split(",").map((s) => s.replace(/\/\*[\s\S]*?\*\//g, "").trim());
    if (!sels.includes(selector)) continue;
    for (const d of m[2].matchAll(/(--merlion-[a-z0-9-]+)\s*:\s*([^;]+);/g)) out[d[1]] = d[2].trim();
  }
  return out;
};

const THEMES = {
  light: '[data-theme="light"]',
  "automatic dark": ":root:not([data-theme])",
  dark: '[data-theme="dark"]',
  harbour: '[data-theme="harbour"]',
  lantern: '[data-theme="lantern"]',
};

test("every theme defines the tones of the built-in roles", () => {
  for (const [name, sel] of Object.entries(THEMES)) {
    const t = tokensOf(sel);
    for (const tok of ["--merlion-bg", "--merlion-fg", "--merlion-accent", "--merlion-ok", "--merlion-warn", "--merlion-danger"]) {
      assert.match(t[tok] ?? "", /^#[0-9a-f]{6}$/, `${name} ${tok}`);
    }
    const tones = ["--merlion-accent", "--merlion-ok", "--merlion-warn", "--merlion-danger"].map((k) => t[k]);
    assert.equal(new Set(tones).size, 4, `${name}: the four tones are distinct`);
  }
  // The automatic dark theme repeats [data-theme="dark"].
  assert.deepEqual(tokensOf(":root:not([data-theme])"), tokensOf('[data-theme="dark"]'));
});

test("merlion-themes.css stays within its 8 KB gzip budget", () => {
  const gz = gzipSync(Buffer.from(css), { level: 9 }).length;
  assert.ok(gz <= 8192, `${gz} B gzipped`);
});
