import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import rehypeMerlion from "@fractalboxdev/merlion-rehype";
import merlion from "../index.js";

// Runs the integration's config hook against a recording stand-in for Astro.
const setup = (options, markdown = {}) => {
  const updates = [];
  const scripts = [];
  const integration = merlion(options);
  integration.hooks["astro:config:setup"]({
    config: { root: new URL("file:///site/"), markdown },
    updateConfig: (c) => updates.push(c),
    injectScript: (stage, content) => scripts.push({ stage, content }),
  });
  return { integration, updates, scripts };
};

test("registers the rehype plugin with the project root and the given options", () => {
  const { integration, updates } = setup({ width: 640, cacheDir: ".merlion", fontCss: true });
  assert.equal(integration.name, "@fractalboxdev/merlion-astro");
  const plugins = updates.flatMap((u) => u.markdown?.rehypePlugins ?? []);
  assert.equal(plugins.length, 1);
  const [plugin, opts] = plugins[0];
  assert.equal(plugin, rehypeMerlion);
  assert.deepEqual(opts, { width: 640, cacheDir: ".merlion", fontCss: true, root: "/site/" });
});

test("an explicit root wins over the Astro root; integration-only options stay out of the plugin", () => {
  const { updates } = setup({ root: "/elsewhere", themesCss: false });
  const [, opts] = updates.flatMap((u) => u.markdown?.rehypePlugins ?? [])[0];
  assert.deepEqual(opts, { root: "/elsewhere", fontCss: true });
});

test("loads merlion-font.css by default and tells the plugin the page has the font", () => {
  const { updates, scripts } = setup({});
  const [, opts] = updates.flatMap((u) => u.markdown?.rehypePlugins ?? [])[0];
  assert.equal(opts.fontCss, true);
  const css = scripts.filter((s) => s.stage === "page-ssr").map((s) => s.content);
  assert.ok(css.includes('import "@fractalboxdev/merlion-themes/merlion-font.css";'), css);
  const off = setup({ fontCss: false });
  assert.equal(off.updates.flatMap((u) => u.markdown?.rehypePlugins ?? [])[0][1].fontCss, false);
  assert.ok(!off.scripts.some((s) => s.content.includes("merlion-font")));
  const custom = setup({ fontCss: "./src/fonts.css" });
  assert.equal(custom.updates.flatMap((u) => u.markdown?.rehypePlugins ?? [])[0][1].fontCss, true);
  assert.ok(custom.scripts.some((s) => s.content === 'import "./src/fonts.css";'));
});

test("excludes mermaid from syntax highlighting so the plugin sees the code block", () => {
  const hl = (markdown) => setup({}, markdown).updates.find((u) => u.markdown?.syntaxHighlight)?.markdown.syntaxHighlight;
  assert.deepEqual(hl({}), { type: "shiki", excludeLangs: ["math", "mermaid"] });
  assert.deepEqual(hl({ syntaxHighlight: "prism" }), { type: "prism", excludeLangs: ["math", "mermaid"] });
  assert.deepEqual(hl({ syntaxHighlight: { type: "shiki", excludeLangs: ["math"] } }), {
    type: "shiki",
    excludeLangs: ["math", "mermaid"],
  });
  assert.deepEqual(hl({ syntaxHighlight: { type: "shiki", excludeLangs: ["mermaid"] } }), {
    type: "shiki",
    excludeLangs: ["mermaid"],
  });
  assert.equal(hl({ syntaxHighlight: false }), undefined);
});

test("injects the theme stylesheet and a viewer loader that imports only when a diagram is on the page", () => {
  const { scripts } = setup({});
  const css = scripts.filter((s) => s.stage === "page-ssr").map((s) => s.content);
  assert.ok(css.includes('import "@fractalboxdev/merlion-themes/merlion-themes.css";'), css);
  const page = scripts.find((s) => s.stage === "page");
  assert.match(page.content, /document\.querySelector\("merlion-view"\)/);
  assert.match(page.content, /import\("@fractalboxdev\/merlion-view"\)/);
});

test("themesCss and viewer: false skip the injections", () => {
  assert.deepEqual(setup({ themesCss: false, fontCss: false, viewer: false }).scripts, []);
  const custom = setup({ themesCss: "./src/diagram-theme.css", fontCss: false }).scripts.find((s) => s.stage === "page-ssr");
  assert.equal(custom.content, 'import "./src/diagram-theme.css";');
});

test("published package has no runtime dependencies; astro is a peer (specs/supply-chain.md)", () => {
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  assert.deepEqual(pkg.dependencies, {});
  assert.ok(pkg.peerDependencies.astro);
});
