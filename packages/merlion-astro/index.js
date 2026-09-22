// Astro integration for Merlion (specs/integrations.md#fractalboxmerlion-astro).
// Registers @fractalbox/merlion-rehype and adds the theme and font stylesheets,
// the compiled `stylesheet` and the <merlion-view> script.
import { closeSync, fsyncSync, mkdirSync, openSync, renameSync, unlinkSync, writeSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import rehypeMerlion, { compileStylesheetFile } from "@fractalbox/merlion-rehype";
import merlionSatteri from "@fractalbox/merlion-rehype/satteri";

const NAME = "@fractalbox/merlion-astro";
const THEMES = "@fractalbox/merlion-themes/merlion-themes.css";
const FONT = "@fractalbox/merlion-themes/merlion-font.css";

// The viewer module loads only on pages that contain a diagram: this loader is
// a few bytes on every page, and the bundler splits the element into its own chunk.
const VIEWER_LOADER = 'if (document.querySelector("merlion-view")) import("@fractalbox/merlion-view");';

// Astro highlights code blocks before user rehype plugins run, which removes the
// `language-mermaid` class; exclude mermaid from highlighting (Astro ≥ 5.5).
const withoutMermaid = (hl) => {
  if (hl === false) return null;
  const base = typeof hl === "string" ? { type: hl } : { type: "shiki", ...hl };
  const langs = base.excludeLangs ?? ["math"];
  return langs.includes("mermaid") ? { ...base, excludeLangs: langs } : { ...base, excludeLangs: [...langs, "mermaid"] };
};

let seq = 0;

/** Writes `text` to `target` through a temporary file in its directory and a rename. */
const atomicWrite = (target, text) => {
  mkdirSync(dirname(target), { recursive: true });
  const tmp = join(dirname(target), `.stylesheet.${process.pid}.${seq++}.tmp`);
  // `wx` = O_CREAT | O_EXCL: never opens a planted file or link.
  const fd = openSync(tmp, "wx", 0o644);
  try {
    writeSync(fd, text);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  try {
    renameSync(tmp, target);
  } catch (err) {
    try {
      unlinkSync(tmp);
    } catch {
      // Already gone.
    }
    throw err;
  }
};

/**
 * Compiles `stylesheet` once per build and writes the page CSS under Astro's cache
 * directory; returns the asset's path. Warnings go to the logger; a refused path, a
 * limit (E013) or a failed compile fails the build.
 */
const buildStylesheet = async (options, root, cacheDir, logger) => {
  const { css, messages } = await compileStylesheetFile(options, root);
  for (const [, reason, fatal] of messages) {
    if (fatal) throw new Error(`${NAME}: ${reason}`);
    logger?.warn(reason);
  }
  if (css === null) {
    throw new Error(`${NAME}: ${messages.map((m) => m[1]).join("; ") || "stylesheet failed to compile"}`);
  }
  const target = fileURLToPath(new URL("merlion/stylesheet.css", cacheDir));
  atomicWrite(target, css);
  return target;
};

/**
 * Puts the plugin where the configured Markdown processor runs it. Astro 7 has a
 * `markdown.processor`: Sätteri (the default) takes hast plugins and never runs the
 * deprecated `markdown.rehypePlugins`; unified takes rehype plugins. The plugin goes
 * first in either list so it claims mermaid blocks before a code-block transformer
 * (Expressive Code, in Starlight) rewrites them. Astro 5 and 6 have no processor and
 * read `markdown.rehypePlugins`.
 */
const registerPlugin = (processor, pluginOptions, updateConfig, logger) => {
  if (!processor) {
    updateConfig({ markdown: { rehypePlugins: [[rehypeMerlion, pluginOptions]] } });
  } else if (processor.name === "satteri" && Array.isArray(processor.options?.hastPlugins)) {
    const onMessage = (m) => {
      const at = m.line > 0 ? `${m.file}:${m.line}:${m.column}` : m.file;
      (logger ?? console).warn(`${at}: ${m.reason}`);
    };
    processor.options.hastPlugins.unshift(merlionSatteri({ ...pluginOptions, onMessage }));
  } else if (processor.name === "unified" && Array.isArray(processor.options?.rehypePlugins)) {
    processor.options.rehypePlugins.unshift([rehypeMerlion, pluginOptions]);
  } else {
    throw new Error(`${NAME}: markdown.processor "${processor.name}" is not supported; use satteri() or unified()`);
  }
};

/**
 * @param {import("./index.js").Options} [options]
 * @returns {import("astro").AstroIntegration}
 */
export default function merlion(options = {}) {
  const { themesCss = THEMES, fontCss = FONT, stylesheet, ...rehypeOptions } = options;
  // `true` means the default stylesheet; a string names another one that loads Inter.
  const fontSheet = fontCss === true ? FONT : fontCss;
  return {
    name: NAME,
    hooks: {
      "astro:config:setup": async ({ config, updateConfig, injectScript, logger }) => {
        const pluginOptions = { ...rehypeOptions, fontCss: Boolean(fontSheet) };
        if (pluginOptions.root === undefined && config.root) pluginOptions.root = fileURLToPath(config.root);
        // Compiled here, once; the plugin gets no `stylesheet`, so it never compiles it again.
        const compiled =
          stylesheet === undefined
            ? null
            : await buildStylesheet(
                { ...rehypeOptions, stylesheet },
                pluginOptions.root ?? ".",
                config.cacheDir ?? new URL("node_modules/.astro/", config.root),
                logger,
              );
        registerPlugin(config.markdown?.processor, pluginOptions, updateConfig, logger);

        const hl = withoutMermaid(config.markdown?.syntaxHighlight ?? "shiki");
        if (hl) updateConfig({ markdown: { syntaxHighlight: hl } });

        if (themesCss) injectScript("page-ssr", `import ${JSON.stringify(themesCss)};`);
        if (fontSheet) injectScript("page-ssr", `import ${JSON.stringify(fontSheet)};`);
        // After the theme tokens, so the compiled rules win at equal specificity.
        if (compiled) injectScript("page-ssr", `import ${JSON.stringify(compiled)};`);
        if (rehypeOptions.viewer !== false) injectScript("page", VIEWER_LOADER);
      },
    },
  };
}
