// Astro integration for Merlion (specs/integrations.md#fractalboxdevmerlion-astro).
// Registers @fractalboxdev/merlion-rehype and adds the theme stylesheet and the
// <merlion-view> script.
import { fileURLToPath } from "node:url";
import rehypeMerlion from "@fractalboxdev/merlion-rehype";

const NAME = "@fractalboxdev/merlion-astro";
const THEMES = "@fractalboxdev/merlion-themes/merlion-themes.css";

// The viewer module loads only on pages that contain a diagram: this loader is
// a few bytes on every page, and the bundler splits the element into its own chunk.
const VIEWER_LOADER = 'if (document.querySelector("merlion-view")) import("@fractalboxdev/merlion-view");';

// Astro highlights code blocks before user rehype plugins run, which removes the
// `language-mermaid` class; exclude mermaid from highlighting (Astro ≥ 5.5).
const withoutMermaid = (hl) => {
  if (hl === false) return null;
  const base = typeof hl === "string" ? { type: hl } : { type: "shiki", ...hl };
  const langs = base.excludeLangs ?? ["math"];
  return langs.includes("mermaid") ? { ...base, excludeLangs: langs } : { ...base, excludeLangs: [...langs, "mermaid"] };
};

/**
 * @param {import("./index.js").Options} [options]
 * @returns {import("astro").AstroIntegration}
 */
export default function merlion(options = {}) {
  const { themesCss = THEMES, ...rehypeOptions } = options;
  return {
    name: NAME,
    hooks: {
      "astro:config:setup": ({ config, updateConfig, injectScript }) => {
        const pluginOptions = { ...rehypeOptions };
        if (pluginOptions.root === undefined && config.root) pluginOptions.root = fileURLToPath(config.root);
        updateConfig({ markdown: { rehypePlugins: [[rehypeMerlion, pluginOptions]] } });

        const hl = withoutMermaid(config.markdown?.syntaxHighlight ?? "shiki");
        if (hl) updateConfig({ markdown: { syntaxHighlight: hl } });

        if (themesCss) injectScript("page-ssr", `import ${JSON.stringify(themesCss)};`);
        if (rehypeOptions.viewer !== false) injectScript("page", VIEWER_LOADER);
      },
    },
  };
}
