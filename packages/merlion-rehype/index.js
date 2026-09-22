// rehype plugin: renders ```mermaid code blocks to inline SVG at build time
// (specs/integrations.md#fractalboxdevmerlion-rehype).
//
// The tree is walked by hand, iteratively, so the plugin has no
// unist-util-visit dependency and no recursion depth limit.
import { isAbsolute, relative, resolve, sep } from "node:path";
import { cacheKey, contentHash, idPrefix } from "./fnv.js";
import { CacheRefused, openCache, readEntry, writeEntry } from "./cache.js";

export { fnv1a64, idPrefix, cacheKey } from "./fnv.js";

const DEFAULTS = { width: 720, strict: false, source: "details", viewer: true, fontCss: false };

const text = (value) => ({ type: "text", value });
const el = (tagName, properties, children) => ({ type: "element", tagName, properties, children });

const classes = (node) => {
  const c = node.properties?.className;
  return Array.isArray(c) ? c : typeof c === "string" ? c.split(/\s+/) : [];
};

// `pre` whose element children are exactly one `code.language-mermaid`.
const mermaidCode = (node) => {
  if (node.type !== "element" || node.tagName !== "pre") return null;
  const kids = (node.children ?? []).filter((c) => c.type === "element");
  const code = kids.length === 1 ? kids[0] : null;
  return code?.tagName === "code" && classes(code).includes("language-mermaid") ? code : null;
};

// Every mermaid block in document order, with its parent and position in it.
// Pre-order walk with an explicit stack of (node, next child index) frames.
const findBlocks = (tree) => {
  const found = [];
  const stack = [[tree, 0]];
  while (stack.length) {
    const top = stack[stack.length - 1];
    const [node, i] = top;
    const kids = node.children;
    if (!Array.isArray(kids) || i >= kids.length) {
      stack.pop();
      continue;
    }
    top[1] = i + 1;
    const child = kids[i];
    const code = mermaidCode(child);
    if (code) found.push({ parent: node, index: i, pre: child, code });
    else if (Array.isArray(child?.children)) stack.push([child, 0]);
  }
  return found;
};

// Concatenated text of a node, iteratively.
const textOf = (node) => {
  let out = "";
  const stack = [node];
  while (stack.length) {
    const n = stack.pop();
    if (n.type === "text") out += n.value;
    else if (Array.isArray(n.children)) for (let i = n.children.length - 1; i >= 0; i--) stack.push(n.children[i]);
  }
  return out;
};

const unquote = (s) => (/^(["']).*\1$/.test(s) ? s.slice(1, -1) : s);

/**
 * The diagram's caption: `accTitle`, else the front-matter `title`, else null
 * (specs/svg-output.md#text-alternative uses the same precedence for `<title>`).
 */
export const diagramTitle = (source) => {
  const acc = /^[ \t]*accTitle[ \t]*:[ \t]*(.*?)[ \t]*$/m.exec(source);
  if (acc && acc[1]) return acc[1];
  const fm = /^﻿?---[ \t]*\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/.exec(source);
  const title = fm && /^title[ \t]*:[ \t]*(.*?)[ \t]*$/m.exec(fm[1]);
  return title && title[1] ? unquote(title[1]) : null;
};

// The file's path relative to the project root, `/`-separated; '' without a path.
const relPathOf = (file, root) => {
  if (!file.path) return "";
  const abs = isAbsolute(file.path) ? file.path : resolve(file.cwd ?? ".", file.path);
  return relative(root, abs).split(sep).join("/");
};

const normalise = (options) => {
  const o = { ...DEFAULTS, ...options };
  if (!(Number.isFinite(o.width) && o.width > 0)) throw new TypeError("merlion-rehype: `width` must be a positive number");
  if (o.source !== "details" && o.source !== "none") {
    throw new TypeError('merlion-rehype: `source` must be "details" or "none"');
  }
  if (o.render !== undefined && typeof o.render !== "function") throw new TypeError("merlion-rehype: `render` must be a function");
  o.strict = Boolean(o.strict);
  return o;
};

// Map a diagnostic (the @fractalboxdev/merlion-wasm `Diagnostic` shape) to a vfile
// message. The fence is on `pre`'s first line, so line k of the source is file line
// fence + k; a diagnostic without a location (line 0) points at the whole block.
const report = (file, d, pre, strict) => {
  const start = pre.position?.start;
  const place = start && d.line > 0 ? { line: start.line + d.line, column: d.column || 1 } : pre.position;
  const m = file.message(`${d.code ?? "E001"} ${d.message ?? "render failed"}`, {
    place,
    ruleId: d.code ?? "E001",
    source: "merlion",
  });
  if (d.severity === "error" && strict) m.fatal = true;
  return m;
};

// E001 for a renderer that throws, in the same shape the WASM glue returns after a trap.
const internalError = (err) => ({
  severity: "error",
  code: "E001",
  line: 0,
  column: 0,
  byteStart: 0,
  byteEnd: 0,
  message: String(err?.message ?? err),
  fix: null,
});

const reported = (d) => d && (d.severity === "error" || d.severity === "warning" || d.severity === "repair");

/**
 * @param {import("./index.js").Options} [options]
 */
export default function rehypeMerlion(options = {}) {
  const o = normalise(options);
  let fontWarned = false;
  let loading = null;
  const getRender = () => o.render ?? (loading ??= import("./wasm.js").then((m) => m.loadWasmRender()));

  return async (tree, file) => {
    const blocks = findBlocks(tree);
    if (blocks.length === 0) return;
    const render = await getRender();
    const root = o.root ?? file.cwd ?? ".";
    const relPath = relPathOf(file, root);

    let cache = null;
    if (o.cacheDir) {
      try {
        cache = openCache(root, o.cacheDir);
      } catch (err) {
        if (!(err instanceof CacheRefused) && err?.code === undefined) throw err;
        file.message(`merlion cache disabled: ${err.message}`, { ruleId: "cache-dir", source: "merlion" });
      }
    }

    let failed = 0;
    let rendered = 0;
    for (const [i, { parent, index, pre, code }] of blocks.entries()) {
      const n = i + 1;
      const source = textOf(code);
      const hash = contentHash(source, o);
      const key = cacheKey(relPath, n);
      const prev = cache ? readEntry(cache, key) : null;

      let svg = null;
      let outline = null;
      if (prev && prev.hash === hash) {
        // Unchanged source and options: reuse without rendering.
        ({ svg, outline } = prev);
      } else {
        const ropts = { width: o.width, strict: o.strict, idPrefix: idPrefix(relPath, n) };
        if (prev) ropts.hint = prev.svg;
        let res;
        try {
          res = await render(source, ropts);
        } catch (err) {
          res = { svg: null, outline: null, diagnostics: [internalError(err)] };
        }
        for (const d of Array.isArray(res?.diagnostics) ? res.diagnostics : []) {
          if (reported(d)) report(file, d, pre, o.strict);
        }
        svg = typeof res?.svg === "string" ? res.svg : null;
        outline = typeof res?.outline === "string" ? res.outline : null;
        if (svg && cache) {
          try {
            writeEntry(cache, key, { hash, svg, outline });
          } catch (err) {
            file.message(`merlion cache write failed: ${err.message}`, { ruleId: "cache-write", source: "merlion" });
          }
        }
      }

      if (svg === null) {
        // Keep the code block; the diagnostics are already on the file.
        failed++;
        continue;
      }
      rendered++;
      o.outline?.({ path: relPath, index: n, source, outline });

      // The SVG is the core's output only; caption and source are text nodes the serialiser escapes.
      const raw = { type: "raw", value: svg };
      const children = [o.viewer ? el("merlion-view", {}, [raw]) : raw];
      const title = diagramTitle(source);
      if (title) children.push(el("figcaption", {}, [text(title)]));
      if (o.source === "details") {
        children.push(
          el("details", {}, [
            el("summary", {}, [text("Diagram source")]),
            el("pre", {}, [el("code", { className: ["language-mermaid"] }, [text(source)])]),
          ]),
        );
      }
      parent.children[index] = {
        ...el("figure", { id: `diagram-${n}`, className: ["merlion-figure"] }, children),
        position: pre.position,
      };
    }

    // Font mode "link" expects the page to load merlion-font.css.
    if (rendered > 0 && !o.fontCss && !fontWarned) {
      fontWarned = true;
      file.message(
        "merlion: diagrams use font mode 'link'; load merlion-font.css on the page and set `fontCss: true` to silence this",
        { ruleId: "font-css", source: "merlion" },
      );
    }
    if (o.strict && failed > 0) {
      file.fail(`${failed} Mermaid diagram${failed === 1 ? "" : "s"} failed to render`, {
        ruleId: "merlion-strict",
        source: "merlion",
      });
    }
  };
}
