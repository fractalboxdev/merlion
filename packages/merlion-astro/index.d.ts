import type { AstroIntegration } from "astro";
import type { Options as RehypeOptions } from "@fractalbox/merlion-rehype";

export interface Options extends Omit<RehypeOptions, "fontCss"> {
  /**
   * Stylesheet imported on every page that loads Inter for font mode "link", or false
   * to skip it (the rehype plugin then warns). Default
   * "@fractalbox/merlion-themes/merlion-font.css"; true means the default.
   */
  fontCss?: string | boolean;
  /**
   * Stylesheet imported on every page for the theme variables and semantic-zoom rules,
   * or false to skip it. Default "@fractalbox/merlion-themes/merlion-themes.css".
   */
  themesCss?: string | false;
  /**
   * A Merlion stylesheet (specs/svg-output.md#stylesheet), relative to the project root.
   * Compiled once per build, written to `<cacheDir>/merlion/stylesheet.css` and imported
   * on every page after `themesCss`; the source file never reaches a page. Warnings are
   * logged; a refused path, a limit (E013) or a failed compile fails the build.
   */
  stylesheet?: string;
}

/**
 * Registers @fractalbox/merlion-rehype with the Markdown processor (first in
 * Sätteri's `hastPlugins` or unified's `rehypePlugins`; `markdown.rehypePlugins` on
 * Astro 5 and 6), excludes
 * mermaid from syntax highlighting, imports the theme and font stylesheets and the
 * compiled `stylesheet`, and loads
 * <merlion-view> on pages that contain a diagram (unless `viewer: false`).
 * `root` defaults to the Astro project root.
 */
export default function merlion(options?: Options): AstroIntegration;
