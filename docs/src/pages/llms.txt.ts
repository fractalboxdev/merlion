// /llms.txt (https://llmstxt.org): the site index for language models, built from the
// docs content collection, in sidebar-section order.
import type { APIRoute } from "astro";
import { getCollection } from "astro:content";
import { sectionOf, SECTIONS } from "../lib/llms";

export const GET: APIRoute = async ({ site }) => {
  const entries = await getCollection("docs");
  const base = site ?? new URL("https://merlion.fractalbox.dev");
  const lines = [
    "# Merlion",
    "",
    "> Merlion renders Mermaid flowcharts to static, themeable, accessible SVG. The core is a no_std Rust library with zero dependencies, shipped as the `merlion` CLI and as a WebAssembly module with byte-identical output, plus a rehype plugin, an Astro integration and a pan-and-zoom web component.",
    "",
    `The full text of every page is at ${new URL("/llms-full.txt", base).href}.`,
    "",
  ];
  for (const section of SECTIONS) {
    const pages = entries.filter((e) => sectionOf(e.id) === section.key).sort((a, b) => Number(b.id === "index") - Number(a.id === "index") || a.id.localeCompare(b.id));
    if (pages.length === 0) continue;
    lines.push(`## ${section.title}`, "");
    for (const e of pages) {
      const url = new URL(e.id === "index" ? "/" : `/${e.id}/`, base).href;
      lines.push(`- [${e.data.title}](${url})${e.data.description ? `: ${e.data.description}` : ""}`);
    }
    lines.push("");
  }
  lines.push("## Optional", "", `- [Playground](${new URL("/playground/", base).href}): live editor over the WebAssembly build`, "");
  return new Response(lines.join("\n"), { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
