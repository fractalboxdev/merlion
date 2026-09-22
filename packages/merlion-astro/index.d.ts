import type { AstroIntegration } from "astro";
import type { Options as RehypeOptions } from "@fractalboxdev/merlion-rehype";

export interface Options extends Omit<RehypeOptions, "fontCss"> {
  /**
   * Stylesheet imported on every page that loads Inter for font mode "link", or false
   * to skip it (the rehype plugin then warns). Default
   * "@fractalboxdev/merlion-themes/merlion-font.css"; true means the default.
   */
  fontCss?: string | boolean;
  /**
   * Stylesheet imported on every page for the theme variables and semantic-zoom rules,
   * or false to skip it. Default "@fractalboxdev/merlion-themes/merlion-themes.css".
   */
  themesCss?: string | false;
}

/**
 * Registers @fractalboxdev/merlion-rehype in `markdown.rehypePlugins`, excludes
 * mermaid from syntax highlighting, imports the theme and font stylesheets, and loads
 * <merlion-view> on pages that contain a diagram (unless `viewer: false`).
 * `root` defaults to the Astro project root.
 */
export default function merlion(options?: Options): AstroIntegration;
