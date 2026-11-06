// /robots.txt: every crawler, AI search and training crawlers included, may read the
// whole site; the sitemap comes from Starlight's @astrojs/sitemap.
import type { APIRoute } from "astro";

const AI_CRAWLERS = ["GPTBot", "ClaudeBot", "PerplexityBot", "Google-Extended", "Applebot-Extended", "CCBot"];

export const GET: APIRoute = ({ site }) => {
  const sitemap = new URL("/sitemap-index.xml", site ?? "https://merlion.fractalbox.dev").href;
  const body = [
    "User-agent: *",
    "Allow: /",
    "",
    ...AI_CRAWLERS.flatMap((ua) => [`User-agent: ${ua}`, "Allow: /", ""]),
    `Sitemap: ${sitemap}`,
    "",
  ].join("\n");
  return new Response(body, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
