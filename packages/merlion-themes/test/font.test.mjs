import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const here = (p) => new URL(`../${p}`, import.meta.url);
const asset = (p) => new URL(`../../../crates/merlion-render/assets/${p}`, import.meta.url);

test("merlion-font.css loads the committed Inter subsets for weights 400 and 600", () => {
  const css = readFileSync(here("merlion-font.css"), "utf8");
  const faces = [...css.matchAll(/@font-face\s*{([^}]*)}/g)].map((m) => m[1]);
  assert.equal(faces.length, 2);
  const weights = faces.map((f) => /font-weight:\s*(\d+)/.exec(f)[1]).sort();
  assert.deepEqual(weights, ["400", "600"]);
  for (const f of faces) {
    const url = /url\("\.\/([^"]+)"\)/.exec(f)[1];
    // The shipped files are the core's subsets, so the page draws what the core measured.
    assert.deepEqual(readFileSync(here(url)), readFileSync(asset(url)), url);
  }
  // Diagrams pick the face up through --merlion-font.
  assert.match(css, /--merlion-font:\s*"Merlion Inter"/);
});

test("the package ships the stylesheet, both fonts and the OFL text", () => {
  const pkg = JSON.parse(readFileSync(here("package.json"), "utf8"));
  for (const f of ["merlion-font.css", "Inter-Regular.subset.woff2", "Inter-SemiBold.subset.woff2", "OFL.txt"]) {
    assert.ok(pkg.files.includes(f), f);
  }
  assert.equal(pkg.exports["./merlion-font.css"], "./merlion-font.css");
  assert.deepEqual(readFileSync(here("OFL.txt")), readFileSync(asset("OFL.txt")));
});
