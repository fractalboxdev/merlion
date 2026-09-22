import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { CURATED, fence, galleryPage, gallerySections } from "../scripts/prepare.mjs";
import { check } from "../../scripts/check-docs-alignment.mjs";
import { headingAndLead, specHref, specId, SPECS_BASE } from "../src/lib/specs.mjs";

const fixtures = readdirSync(new URL("../../crates/merlion-render/tests/fixtures/flowcharts/", import.meta.url)).filter((n) =>
  n.endsWith(".mmd"),
);

test("the gallery lists the showcase, the roles fixture, every fixture and twelve compat diagrams", () => {
  const sections = gallerySections();
  const by = (slug) => sections.find((s) => s.slug === slug).entries;
  assert.deepEqual(
    sections.map((s) => s.slug),
    ["showcase", "roles", "fixtures", "mermaid-corpus"],
  );
  assert.equal(by("fixtures").length, fixtures.length);
  assert.equal(CURATED.length, 12);
  assert.equal(by("mermaid-corpus").length, 12);
  for (const s of sections) for (const e of s.entries) assert.ok(e.source.length > 0, e.title);
  const wide = by("showcase").find((e) => /wide pipeline/i.test(e.title));
  assert.equal(wide.meta, "width=1600");
  const [roles] = by("roles");
  for (const line of ["class cron accent", "class e2 failure", "class e1 async", "class wf group", "class q queue", "class consumers senders"]) {
    assert.ok(roles.source.includes(line), line);
  }
  assert.deepEqual(
    roles.baked.map((b) => b.theme),
    ["light", "dark"],
  );
});

test("a fence is longer than any backtick run in the source, so a fixture holding ``` stays one block", () => {
  assert.equal(fence("flowchart LR\n  a --> b\n"), "```mermaid\nflowchart LR\n  a --> b\n```\n");
  const inner = "```mermaid\nflowchart TD\n  A --> B\n```\n";
  assert.ok(fence(inner).startsWith("````mermaid\n"));
  assert.match(fence("x", "width=1600"), /^```mermaid width=1600\n/);
  const page = galleryPage({ title: "T", description: "D", intro: "I", entries: [{ title: "a `b`", source: "x" }] }, 1);
  assert.match(page, /^## a b$/m);
});

test("specs: README is its directory's index, titles and descriptions come from the file", () => {
  assert.equal(specId({ entry: "reference/specs/README.md", data: {} }), "reference/specs");
  assert.equal(specId({ entry: "reference/specs/adr/0009-stylesheet.md", data: {} }), "reference/specs/adr/0009-stylesheet");
  assert.equal(specId({ entry: "index.md", data: {} }), "index");
  const { title, lead } = headingAndLead("# `<merlion-view>`\n\nA custom element that adds pan.\nSecond line.\n\n## Next\n");
  assert.equal(title, "<merlion-view>");
  assert.equal(lead, "A custom element that adds pan. Second line.");
});

test("specs: relative links become site URLs, or GitHub URLs outside specs/", () => {
  const specs = new URL("../../specs/", import.meta.url).pathname;
  const from = join(specs, "layout.md");
  assert.equal(specHref("architecture.md#boundaries", from), `${SPECS_BASE}architecture/#boundaries`);
  assert.equal(specHref("adr/0004-layout-implementation.md", from), `${SPECS_BASE}adr/0004-layout-implementation/`);
  assert.equal(specHref("adr/", from), `${SPECS_BASE}adr/`);
  assert.equal(specHref("README.md", from), SPECS_BASE);
  assert.equal(specHref("research/", from), "https://github.com/fractalboxdev/merlion/tree/main/specs/research");
  assert.equal(specHref("../bench/README.md#stylesheet-parity", from), "https://github.com/fractalboxdev/merlion/blob/main/bench/README.md#stylesheet-parity");
  assert.equal(specHref("https://example.com/", from), null);
  assert.equal(specHref("#options", from), null);
});

test("the alignment check passes on the repository and fails on a drifted README", () => {
  const root = new URL("../../", import.meta.url).pathname;
  const gs = join(root, "docs/src/content/docs/getting-started.md");
  const index = join(root, "docs/src/content/docs/index.md");
  assert.deepEqual(check(join(root, "README.md"), gs, index), []);

  const dir = mkdtempSync(join(tmpdir(), "merlion-align-"));
  const readme = join(dir, "README.md");
  const drifted = readFileSync(join(root, "README.md"), "utf8")
    .replace("cargo build --release -p merlion-cli\necho", "cargo build -p merlion-cli\necho")
    .replace("(specs/roadmap.md)", "(specs/missing.md)")
    .replace("| Fit | Layout takes", "| Fit | The layout takes");
  writeFileSync(readme, drifted);
  const problems = check(readme, gs, index);
  assert.ok(problems.some((p) => /Quick start block "cargo build -p merlion-cli"/.test(p)), problems.join("\n"));
  assert.ok(problems.some((p) => /missing file specs\/missing\.md/.test(p)), problems.join("\n"));
  assert.ok(problems.some((p) => /Guarantees table differs/.test(p)), problems.join("\n"));
  assert.ok(!existsSync(join(dir, "specs")));
});
