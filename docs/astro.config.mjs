// @ts-check
import { defineConfig } from "astro/config";
import { satteri } from "@astrojs/markdown-satteri";
import starlight from "@astrojs/starlight";
import merlion from "@fractalboxdev/merlion-astro";
import starlightLinksValidator from "starlight-links-validator";
import { specLinks } from "./src/lib/specs.mjs";

export default defineConfig({
  site: "https://merlion-docs.debuggingfuturecors.workers.dev",
  trailingSlash: "always",
  markdown: {
    // Relative links inside specs/ point at sibling files, as on GitHub; on the site they
    // point at spec pages (src/lib/specs.mjs).
    processor: satteri({ hastPlugins: [specLinks] }),
  },
  integrations: [
    starlight({
      title: "Merlion",
      description:
        "Merlion renders Mermaid flowcharts to static, themeable, accessible SVG from a no_std Rust core shipped as a CLI and as WebAssembly.",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/fractalboxdev/merlion" }],
      editLink: { baseUrl: "https://github.com/fractalboxdev/merlion/edit/main/docs/" },
      lastUpdated: false,
      customCss: ["./src/styles/starlight.css"],
      head: [
        { tag: "link", attrs: { rel: "alternate", type: "text/plain", href: "/llms.txt", title: "llms.txt" } },
      ],
      plugins: [
        starlightLinksValidator({
          errorOnLocalLinks: true,
          exclude: ({ link, slug }) =>
            // Pages outside the docs collection.
            ["/playground/", "/llms.txt", "/llms-full.txt"].includes(link) ||
            // The validator keys pages by file name; the two spec indexes are README.md
            // files served at their directory (src/lib/specs.mjs).
            /^\/reference\/specs\/(adr\/)?(#.*)?$/.test(link) ||
            // `click` links inside gallery diagrams belong to the diagram's own source.
            (slug.startsWith("gallery/") && !link.startsWith("/") && !link.startsWith("#")),
        }),
      ],
      sidebar: [
        { label: "Overview", link: "/" },
        { label: "Getting started", slug: "getting-started" },
        {
          label: "Guides",
          items: [
            "guides/theming",
            "guides/roles-and-stylesheets",
            "guides/labels",
            "guides/stable-layout",
            "guides/viewer",
            "guides/diagnostics",
          ],
        },
        {
          label: "How it works",
          items: [
            "how-it-works/architecture",
            "how-it-works/render-pipeline",
            {
              label: "Layout",
              items: [
                "how-it-works/layout/cycle-removal",
                "how-it-works/layout/layer-assignment",
                "how-it-works/layout/crossing-minimisation",
                "how-it-works/layout/coordinate-assignment",
                "how-it-works/layout/container-fit",
                "how-it-works/layout/edge-routing",
                "how-it-works/layout/stable-layout",
              ],
            },
            "how-it-works/stylesheet",
          ],
        },
        { label: "Playground", link: "/playground/" },
        { label: "Gallery", items: [{ autogenerate: { directory: "gallery" } }] },
        { label: "Specs", collapsed: true, items: [{ autogenerate: { directory: "reference/specs" } }] },
      ],
    }),
    // Every ```mermaid block renders to inline SVG at build time through the WASM build.
    merlion({ stylesheet: "src/styles/diagrams.css", width: 720 }),
  ],
  vite: {
    server: { allowedHosts: [".ts.net"] },
  },
});
