import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readdirSync, readFileSync, realpathSync, writeFileSync as write } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import rehypeMerlion from "@fractalbox/merlion-rehype";
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
  assert.equal(integration.name, "@fractalbox/merlion-astro");
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
  assert.ok(css.includes('import "@fractalbox/merlion-themes/merlion-font.css";'), css);
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
  assert.ok(css.includes('import "@fractalbox/merlion-themes/merlion-themes.css";'), css);
  const page = scripts.find((s) => s.stage === "page");
  assert.match(page.content, /document\.querySelector\("merlion-view"\)/);
  assert.match(page.content, /import\("@fractalbox\/merlion-view"\)/);
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

// --- The `stylesheet` option: compiled once in the config hook, written under Astro's
// cache directory and imported as page CSS; the source stylesheet never reaches a page.

const SHEET = ':root { --merlion-accent: #0f766e; }\n.merlion-c-store { --merlion-tone: #b8408f; }\n';

const setupAsync = async (options, dir) => {
  const updates = [];
  const scripts = [];
  const logs = [];
  const integration = merlion(options);
  await integration.hooks["astro:config:setup"]({
    config: { root: pathToFileURL(`${dir}/`), cacheDir: pathToFileURL(`${dir}/node_modules/.astro/`), markdown: {} },
    updateConfig: (c) => updates.push(c),
    injectScript: (stage, content) => scripts.push({ stage, content }),
    logger: { warn: (m) => logs.push(["warn", m]), info: (m) => logs.push(["info", m]), error: (m) => logs.push(["error", m]) },
  });
  return { updates, scripts, logs };
};

const project = () => realpathSync(mkdtempSync(join(tmpdir(), "merlion-astro-")));

test("stylesheet: compiled once through the WASM module, written as an asset and imported on pages", async () => {
  const dir = project();
  write(join(dir, "diagram.css"), SHEET);
  const { updates, scripts } = await setupAsync({ stylesheet: "diagram.css" }, dir);
  const asset = join(dir, "node_modules/.astro/merlion/stylesheet.css");
  assert.equal(
    readFileSync(asset, "utf8"),
    ":root {\n  --merlion-accent: #0f766e;\n}\n.merlion .merlion-c-store {\n  --merlion-tone: #b8408f;\n}\n",
  );
  assert.deepEqual(readdirSync(join(dir, "node_modules/.astro/merlion")), ["stylesheet.css"]);
  const css = scripts.filter((s) => s.stage === "page-ssr").map((s) => s.content);
  // Theme tokens first, then the compiled stylesheet, so its rules win.
  assert.deepEqual(css.slice(-1), [`import ${JSON.stringify(asset)};`]);
  assert.ok(css.indexOf('import "@fractalbox/merlion-themes/merlion-themes.css";') < css.length - 1);
  // The plugin does not compile it again.
  const [, opts] = updates.flatMap((u) => u.markdown?.rehypePlugins ?? [])[0];
  assert.equal(opts.stylesheet, undefined);
});

test("stylesheet: warnings go to the logger; strict errors and refused paths fail the build", async () => {
  const dir = project();
  write(join(dir, "warn.css"), ":root { --merlion-bg: #fff; --merlion-font: Comic; }\n");
  const { logs } = await setupAsync({ stylesheet: "warn.css" }, dir);
  assert.ok(logs.some(([level, m]) => level === "warn" && /warn\.css:1:\d+: W018/.test(m)), JSON.stringify(logs));
  await assert.rejects(setupAsync({ stylesheet: "warn.css", strict: true }, dir), /W018/);
  await assert.rejects(setupAsync({ stylesheet: "../outside.css" }, dir), /outside the project root/);
  const fresh = project();
  write(join(fresh, "big.css"), `/*${"x".repeat(64 * 1024)}*/`);
  await assert.rejects(setupAsync({ stylesheet: "big.css" }, fresh), /E013/);
  assert.ok(!existsSync(join(fresh, "node_modules/.astro/merlion/stylesheet.css")));
});

// --- Astro 7 markdown processors. `markdown.rehypePlugins` is deprecated there and the
// default Sätteri processor never runs it, so the plugin goes into the processor's own
// plugin list, first, ahead of code-block transformers such as Expressive Code.

const withProcessor = async (processor, options = {}) => {
  const updates = [];
  await merlion({ fontCss: false, ...options }).hooks["astro:config:setup"]({
    config: { root: new URL("file:///site/"), markdown: { processor } },
    updateConfig: (c) => updates.push(c),
    injectScript: () => {},
    logger: { warn() {}, info() {}, error() {} },
  });
  return updates;
};

test("Sätteri processor: a hast plugin factory first in processor.options.hastPlugins, no legacy rehypePlugins", async () => {
  const codeBlocks = () => ({ name: "code-blocks" });
  const processor = { name: "satteri", options: { mdastPlugins: [], hastPlugins: [codeBlocks], features: {} } };
  const updates = await withProcessor(processor);
  assert.equal(processor.options.hastPlugins.length, 2);
  assert.equal(processor.options.hastPlugins[1], codeBlocks);
  const factory = processor.options.hastPlugins[0];
  assert.equal(typeof factory, "function");
  const plugin = factory({ fileURL: new URL("file:///site/src/content/docs/a.md"), sourceFormat: "markdown", source: "", data: {} });
  assert.equal(plugin.name, "@fractalbox/merlion-rehype");
  assert.deepEqual(plugin.element.filter, ["pre"]);
  assert.ok(!updates.some((u) => u.markdown?.rehypePlugins), "no deprecated markdown.rehypePlugins");
});

test("unified processor: rehypeMerlion first in processor.options.rehypePlugins", async () => {
  const other = () => {};
  const processor = { name: "unified", options: { remarkPlugins: [], rehypePlugins: [other], remarkRehype: {} } };
  const updates = await withProcessor(processor, { width: 640 });
  assert.equal(processor.options.rehypePlugins.length, 2);
  const [plugin, opts] = processor.options.rehypePlugins[0];
  assert.equal(plugin, rehypeMerlion);
  assert.deepEqual(opts, { width: 640, fontCss: false, root: "/site/" });
  assert.ok(!updates.some((u) => u.markdown?.rehypePlugins));
});

test("an unknown markdown processor fails the build instead of skipping every diagram", async () => {
  await assert.rejects(withProcessor({ name: "custom", options: {} }), /markdown\.processor "custom"/);
});
