#!/usr/bin/env node
// Build inputs of the docs site that come from elsewhere in the repository. Runs
// before `astro dev` and `astro build`; everything it writes is gitignored.
//
//   1. public/merlion.wasm and public/merlion-wasm.js: the WASM build of
//      packages/merlion-wasm and its glue, served as static files for the playground,
//      which imports the glue at runtime (build the module first:
//      sh packages/merlion-wasm/scripts/build-wasm.sh).
//   2. src/content/docs/gallery/*.md: one Markdown page per source, generated from the
//      files themselves, so the fixtures stay the single source of every diagram:
//        - showcase: src/gallery/showcase.json
//        - roles: bench/fixtures/roles/two-tier-roles.mmd, inline and as standalone
//          light and dark renders with the site stylesheet baked in (public/gallery/)
//        - fixtures: every crates/merlion-render/tests/fixtures/flowcharts/*.mmd
//        - compat: twelve curated diagrams of bench/corpus/compat/
//      Each diagram is a ```mermaid block, rendered at build time by the Merlion
//      integration like any other page.
//
//   node scripts/prepare.mjs
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DOCS = fileURLToPath(new URL("..", import.meta.url));
const REPO = join(DOCS, "..");
const WASM_DIR = join(REPO, "packages", "merlion-wasm");
const THEMES_CSS = join(REPO, "packages", "merlion-themes", "merlion-themes.css");
const SITE_CSS = join(DOCS, "src", "styles", "diagrams.css");
const FIXTURES = join(REPO, "crates", "merlion-render", "tests", "fixtures", "flowcharts");
const COMPAT = join(REPO, "bench", "corpus", "compat");
const ROLES = join(REPO, "bench", "fixtures", "roles");
const SHOWCASE = join(DOCS, "src", "gallery", "showcase.json");
const GALLERY = join(DOCS, "src", "content", "docs", "gallery");
const PUBLIC = join(DOCS, "public");

/** The roles fixture shown in the roles section. */
export const ROLE_FIXTURES = ["two-tier-roles"];
/** The themes baked into its standalone renders. */
export const BAKED_THEMES = ["light", "dark"];

/** Compat diagrams chosen for range: size, subgraphs, classes and styles, edge labels. */
export const CURATED = [
  "demos-flowchart-01",
  "demos-flowchart-09",
  "demos-flowchart-13",
  "demos-flowchart-23",
  "demos-flowchart-31",
  "demos-flowchart-38",
  "demos-flowchart-62",
  "demos-flowchart-elk-01",
  "docs-flowchart-08",
  "e2e-dagre-7213-should-render-dagre-edges-with-right-angles-not-curves",
  "e2e-flowchart-redux-color-subgraphs-01",
  "e2e-v2-18-should-render-nested-subgraphs-with-edge-from-cluster-containing-extractable-subgraph",
];

/** `ci-pipeline` → `Ci pipeline`. */
const titleOf = (stem) => stem.charAt(0).toUpperCase() + stem.slice(1).replaceAll("-", " ");

const mmd = (dir) =>
  readdirSync(dir)
    .filter((n) => n.endsWith(".mmd"))
    .sort()
    .map((n) => ({ stem: basename(n, ".mmd"), source: readFileSync(join(dir, n), "utf8") }));

/** A fenced block whose fence is longer than any backtick run in the source. */
export const fence = (source, meta = "") => {
  const longest = Math.max(2, ...(source.match(/`+/g) ?? []).map((r) => r.length));
  const f = "`".repeat(longest + 1);
  const body = source.endsWith("\n") ? source : `${source}\n`;
  return `${f}mermaid${meta ? ` ${meta}` : ""}\n${body}${f}\n`;
};

/** Heading text safe in Markdown: no leading `#`, no inline markup characters. */
const heading = (s) => s.replace(/[`*_[\]<>]/g, "");

/** The gallery's sections, in sidebar order: `{ slug, title, description, intro, entries }`. */
export const gallerySections = () => {
  const showcase = JSON.parse(readFileSync(SHOWCASE, "utf8"));
  const roles = mmd(ROLES).filter((f) => ROLE_FIXTURES.includes(f.stem));
  return [
    {
      slug: "showcase",
      from: "docs/src/gallery/showcase.json",
      title: "Showcase",
      description: "Hand-written Merlion diagrams: a wide pipeline for the viewer, subgraphs, shapes and a titled request lifecycle.",
      intro:
        "Hand-written diagrams from `docs/src/gallery/showcase.json`. The wide pipeline renders at 1600 px (`width=1600` in its fence), wider than the page, so `<merlion-view>` shows its zoom controls.",
      entries: showcase.map((e) => ({
        title: e.title,
        source: e.source,
        meta: e.options?.width ? `width=${e.options.width}` : "",
      })),
    },
    {
      slug: "roles",
      from: "bench/fixtures/roles/",
      title: "Roles and stylesheet",
      description: "Built-in roles and site stylesheet roles on nodes, edges and clusters, inline and as standalone SVG with light and dark themes baked in.",
      intro: [
        "Classes are roles. `accent`, `ok`, `warn`, `danger`, `muted`, the dashed cluster `group` and the edges `failure` and `async` take their tones from the theme; `queue` and `senders` come from the site stylesheet `docs/src/styles/diagrams.css`, which pages link only as compiled CSS. The inline diagram follows the theme switch in the header; the two images are standalone SVGs with the light and dark themes baked in, so they keep their colours in any host.",
        "Source: `bench/fixtures/roles/`, shared with the stylesheet parity gate.",
      ].join("\n\n"),
      entries: roles.map((f) => ({
        title: titleOf(f.stem),
        source: f.source,
        baked: BAKED_THEMES.map((theme) => ({ theme, file: `/gallery/roles-${f.stem}-${theme}.svg` })),
      })),
    },
    {
      slug: "fixtures",
      from: "crates/merlion-render/tests/fixtures/flowcharts/",
      title: "Test fixtures",
      description: "Every flowchart fixture of the Merlion core's test suite, rendered at build time: shapes, edges, labels, subgraphs, styles and LLM-style repairs.",
      intro:
        "Every file in `crates/merlion-render/tests/fixtures/flowcharts/`, the inputs of the core's snapshot tests. Fixtures with deliberate mistakes (`llm-repairs`, `llm-typographic`, `llm-unquoted-labels`) render through the parser's repairs.",
      entries: mmd(FIXTURES).map((f) => ({ title: titleOf(f.stem), source: f.source })),
    },
    {
      slug: "mermaid-corpus",
      from: "bench/corpus/compat/",
      title: "Mermaid corpus",
      description: "Twelve diagrams from the mermaid 12.0.0 compatibility corpus, rendered by Merlion: large graphs, nested subgraphs, classes and edge labels.",
      intro:
        "Twelve diagrams from `bench/corpus/compat/`, the benchmark's compatibility corpus drawn from the mermaid repository at tag `mermaid@12.0.0` (MIT; see `THIRD_PARTY_NOTICES.md`). They are chosen for range: size, nested subgraphs, classes and styles, edge labels.",
      entries: CURATED.map((stem) => ({ title: stem, source: readFileSync(join(COMPAT, `${stem}.mmd`), "utf8") })),
    },
  ];
};

/** One gallery page as Markdown. */
export const galleryPage = (section, order) => {
  const parts = [
    "---",
    `title: ${JSON.stringify(section.title)}`,
    `description: ${JSON.stringify(section.description)}`,
    "editUrl: false",
    `sidebar: { order: ${order} }`,
    "---",
    "",
    "<!-- Generated by docs/scripts/prepare.mjs from the repository's fixture files. -->",
    "",
    section.intro,
    "",
  ];
  for (const e of section.entries) {
    parts.push(`## ${heading(e.title)}`, "", fence(e.source, e.meta));
    if (e.baked) {
      for (const b of e.baked) {
        parts.push(`![${e.title}, standalone with the ${b.theme} theme baked in](${b.file})`, "");
      }
    }
  }
  return parts.join("\n");
};

const galleryIndex = (sections) =>
  [
    "---",
    'title: "Gallery"',
    'description: "Every diagram of the Merlion gallery, rendered at build time by the Merlion Astro integration from the repository\'s fixture files."',
    "editUrl: false",
    "sidebar: { order: 0 }",
    "---",
    "",
    "<!-- Generated by docs/scripts/prepare.mjs. -->",
    "",
    "Every diagram in the gallery is a ```` ```mermaid ```` block that the Merlion Astro integration renders to inline SVG when the site builds; no Mermaid runtime reaches the page. Switch the theme in the header and the diagrams restyle through CSS alone. Ctrl/⌘ + wheel or pinch zooms a diagram, drag pans, double-click resets.",
    "",
    "| Section | Diagrams | Source |",
    "|---|---|---|",
    ...sections.map((s) => `| [${s.title}](/gallery/${s.slug}/) | ${s.entries.length} | \`${s.from}\` |`),
    "",
  ].join("\n");

const loadWasm = async () => {
  const file = join(WASM_DIR, "merlion.wasm");
  if (!existsSync(file)) throw new Error("no packages/merlion-wasm/merlion.wasm; run sh packages/merlion-wasm/scripts/build-wasm.sh");
  const wasm = await import(pathToFileURL(join(WASM_DIR, "index.js")).href);
  wasm.initSync(readFileSync(file));
  return { wasm, file };
};

/** The stylesheet the standalone renders bake: theme tokens, then the site's roles. */
export const bakeStylesheet = () => `${readFileSync(THEMES_CSS, "utf8")}\n${readFileSync(SITE_CSS, "utf8")}`;

const main = async () => {
  let loaded;
  try {
    loaded = await loadWasm();
  } catch (err) {
    console.error(`prepare: ${err.message}`);
    return 1;
  }
  const { wasm, file } = loaded;
  mkdirSync(PUBLIC, { recursive: true });
  copyFileSync(file, join(PUBLIC, "merlion.wasm"));
  copyFileSync(join(WASM_DIR, "index.js"), join(PUBLIC, "merlion-wasm.js"));

  const sections = gallerySections();
  rmSync(GALLERY, { recursive: true, force: true });
  mkdirSync(GALLERY, { recursive: true });
  writeFileSync(join(GALLERY, "index.md"), galleryIndex(sections));
  sections.forEach((s, i) => writeFileSync(join(GALLERY, `${s.slug}.md`), galleryPage(s, i + 1)));

  // Standalone renders: the theme's literals baked in, Inter embedded.
  let failed = 0;
  mkdirSync(join(PUBLIC, "gallery"), { recursive: true });
  for (const theme of BAKED_THEMES) {
    const c = wasm.compileStylesheet(bakeStylesheet(), { theme, strict: true });
    if (!c.palette) {
      console.error(`prepare: theme ${theme} did not compile: ${c.diagnostics.map((d) => d.code).join(", ")}`);
      return 1;
    }
    for (const e of sections.find((s) => s.slug === "roles").entries) {
      for (const b of e.baked.filter((x) => x.theme === theme)) {
        const res = wasm.render(e.source, { idPrefix: `baked-${theme}`, palette: c.palette, font: "embed" });
        if (res.svg === null) {
          failed++;
          console.error(`prepare: ${b.file}: ${res.diagnostics.map((d) => `${d.code} ${d.message}`).join("; ")}`);
          continue;
        }
        writeFileSync(join(PUBLIC, b.file), res.svg);
      }
    }
  }
  const total = sections.reduce((n, s) => n + s.entries.length, 0);
  console.log(`prepare: public/merlion.wasm, public/merlion-wasm.js; gallery: ${sections.length} pages, ${total} diagrams`);
  return failed ? 1 : 0;
};

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) process.exitCode = await main();
