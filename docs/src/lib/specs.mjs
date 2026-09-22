// @ts-check
// specs/ on the site: the docs collection reaches the repository's specs/ through the
// relative symbolic link src/content/docs/reference/specs. Specs are plain Markdown for
// GitHub, so this module supplies what Starlight needs and GitHub does not: ids,
// titles, descriptions and site URLs for their relative links.
import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

/** Repository root and its specs/ directory, both real paths. */
export const REPO = realpathSync(fileURLToPath(new URL("../../../", import.meta.url)));
export const SPECS = resolve(REPO, "specs");
/** Where relative links that leave specs/ point: the repository on GitHub. */
export const REPO_URL = "https://github.com/fractalboxdev/merlion";
/** Site path of specs/. */
export const SPECS_BASE = "/reference/specs/";

const slugify = (/** @type {string} */ segment) =>
  segment
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9_-]/g, "");

/**
 * Collection id of a docs entry: the path without extension, slugified per segment;
 * `README` and `index` name their directory.
 * @param {{ entry: string, data: Record<string, unknown> }} options
 */
export const specId = ({ entry, data }) => {
  if (typeof data.slug === "string") return data.slug;
  const parts = entry.replace(/\.(md|mdx|markdown)$/i, "").split("/");
  if (/^(readme|index)$/i.test(parts[parts.length - 1] ?? "")) parts.pop();
  return parts.map(slugify).join("/") || "index";
};

/** Markdown inline syntax to plain text, for titles and descriptions. */
const plain = (/** @type {string} */ s) =>
  s
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_]/g, "")
    .replace(/\s+/g, " ")
    .trim();

/**
 * The first `# heading` and the first paragraph of a Markdown file.
 * @param {string} text
 */
export const headingAndLead = (text) => {
  const lines = text.replace(/^---\n[\s\S]*?\n---\n/, "").split("\n");
  const h = lines.findIndex((l) => /^# /.test(l));
  const title = h >= 0 ? plain(lines[h].slice(2)) : undefined;
  let lead;
  for (let i = h + 1, para = []; i <= lines.length; i++) {
    const l = lines[i] ?? "";
    if (l.trim() === "" || /^(#|```|\||- |\d+\. |>)/.test(l)) {
      if (para.length) {
        lead = plain(para.join(" "));
        break;
      }
      continue;
    }
    para.push(l);
  }
  return { title, lead };
};

/** Cuts a description to at most 160 characters at a word boundary. */
const clip = (/** @type {string} */ s) => (s.length <= 160 ? s : `${s.slice(0, 157).replace(/\s+\S*$/, "")}…`);

/**
 * Front matter for an entry: a missing `title` comes from the first `# heading`, a
 * missing `description` from the first paragraph. Entries with their own front matter
 * pass through unchanged.
 * @template {Record<string, unknown>} T
 * @param {T} data
 * @param {string | undefined} filePath
 * @returns {T}
 */
export const specFrontmatter = (data, filePath) => {
  if (data.title && data.description) return data;
  if (!filePath) return data;
  const { title, lead } = headingAndLead(readFileSync(filePath, "utf8"));
  return {
    ...data,
    ...(data.title ? {} : { title: title ?? filePath.split("/").pop() }),
    ...(data.description || !lead ? {} : { description: clip(lead) }),
  };
};

const inside = (/** @type {string} */ base, /** @type {string} */ p) => p === base || p.startsWith(base + sep);

/**
 * Site URL of a relative link written in a spec at `realFile`: another spec → its page;
 * a directory with a README → that page; anything else in the repository → GitHub.
 * Returns null for links this module leaves alone (absolute, external, anchors).
 * @param {string} href
 * @param {string} realFile
 */
export const specHref = (href, realFile) => {
  if (!href || /^(?:[a-z][a-z0-9+.-]*:|\/|#)/i.test(href)) return null;
  const [path = "", hash = ""] = href.split(/(?=#)/);
  const target = resolve(dirname(realFile), decodeURI(path));
  if (!inside(REPO, target)) return null;
  const isDir = existsSync(target) && statSync(target).isDirectory();
  if (inside(SPECS, target)) {
    const rel = relative(SPECS, target).split(sep).join("/");
    if (!isDir && /\.md$/i.test(rel)) {
      const id = specId({ entry: rel, data: {} });
      return `${SPECS_BASE}${id === "index" ? "" : `${id}/`}${hash}`;
    }
    if (isDir && existsSync(resolve(target, "README.md"))) return `${SPECS_BASE}${rel ? `${rel}/` : ""}${hash}`;
  }
  const repoRel = relative(REPO, target).split(sep).join("/");
  return `${REPO_URL}/${isDir ? "tree" : "blob"}/main/${repoRel}${hash}`;
};

/** Real path of a document, or null when it is not under specs/. */
const specFile = (/** @type {URL | undefined} */ fileURL) => {
  if (!fileURL) return null;
  try {
    const real = realpathSync(fileURLToPath(fileURL));
    return inside(SPECS, real) ? real : null;
  } catch {
    return null;
  }
};

/**
 * Sätteri hast plugin factory: rewrites the relative links of specs to site URLs, so
 * links written for GitHub work on the site. The first `# heading` is dropped, because
 * Starlight renders the title from front matter.
 * @param {{ fileURL?: URL }} ctx
 */
export const specLinks = ({ fileURL } = {}) => {
  const real = specFile(fileURL);
  if (!real) return null;
  let droppedTitle = false;
  return {
    name: "merlion-docs-spec-links",
    element: [
      {
        filter: ["a"],
        /** @param {any} node @param {any} ctx */
        visit(node, ctx) {
          const href = specHref(String(node.properties?.href ?? ""), real);
          if (href) ctx.setProperty(node, "href", href);
        },
      },
      {
        filter: ["h1"],
        /** @param {any} node @param {any} ctx */
        visit(node, ctx) {
          if (droppedTitle) return;
          droppedTitle = true;
          ctx.removeNode(node);
        },
      },
    ],
  };
};
