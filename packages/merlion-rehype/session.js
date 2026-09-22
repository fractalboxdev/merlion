// Rendering shared by the rehype plugin (index.js) and the Sätteri adapter
// (satteri.js): option checks, the renderer, the stylesheet, the hint cache and the
// figure built for each block. Adapters walk their own trees and turn the returned
// messages into their host's diagnostics.
import { isAbsolute, relative, resolve, sep } from "node:path";
import { cacheKey, contentHash, idPrefix } from "./fnv.js";
import { CacheRefused, openCache, readEntry, writeEntry } from "./cache.js";
import { readStylesheet, StylesheetRefused } from "./stylesheet.js";

const DEFAULTS = { width: 720, strict: false, source: "details", viewer: true, fontCss: false };

const text = (value) => ({ type: "text", value });
const el = (tagName, properties, children) => ({ type: "element", tagName, properties, children });

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

/** A path relative to the project root, `/`-separated; '' without a path. */
export const relPathOf = (path, cwd, root) => {
  if (!path) return "";
  const abs = isAbsolute(path) ? path : resolve(cwd ?? ".", path);
  return relative(root, abs).split(sep).join("/");
};

export const normalise = (options) => {
  const o = { ...DEFAULTS, ...options };
  if (!(Number.isFinite(o.width) && o.width > 0)) throw new TypeError("merlion-rehype: `width` must be a positive number");
  if (o.source !== "details" && o.source !== "none") {
    throw new TypeError('merlion-rehype: `source` must be "details" or "none"');
  }
  if (o.render !== undefined && typeof o.render !== "function") throw new TypeError("merlion-rehype: `render` must be a function");
  if (o.stylesheet !== undefined && typeof o.stylesheet !== "string") {
    throw new TypeError("merlion-rehype: `stylesheet` must be a path");
  }
  if (o.compileStylesheet !== undefined && typeof o.compileStylesheet !== "function") {
    throw new TypeError("merlion-rehype: `compileStylesheet` must be a function");
  }
  o.strict = Boolean(o.strict);
  return o;
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

/**
 * Per-block options from a fence's info string after the language, `key=value` words
 * separated by spaces: `width=<px>` (a positive number) sets the block's container
 * width. Other keys are ignored.
 * @param {string | undefined} meta
 * @param {number} width The plugin's width.
 * @returns {{ width: number, error: string | null }}
 */
export const fenceMeta = (meta, width) => {
  for (const word of String(meta ?? "").split(/\s+/)) {
    const m = /^width=(.*)$/.exec(word);
    if (!m) continue;
    const px = /^\d+(?:\.\d+)?$/.test(m[1]) ? Number(m[1]) : NaN;
    if (px > 0 && Number.isFinite(px)) return { width: px, error: null };
    return { width, error: `fence meta ${word}: width must be a positive number of px; the block renders at ${width}` };
  }
  return { width, error: null };
};

const reported = (d) => d && (d.severity === "error" || d.severity === "warning" || d.severity === "repair");

/**
 * Reads the stylesheet `o.stylesheet` under `root` with the file-handling rules and
 * compiles it: `{ css, messages }`, where `css` is the page CSS (null when refused or
 * failed) and `messages` are `[ruleId, reason, fatal]`, `fatal` only under `o.strict`.
 * Shared with @fractalbox/merlion-astro.
 * @param {{ stylesheet: string, strict?: boolean, compileStylesheet?: Function }} o
 * @param {string} root
 */
export const compileStylesheetFile = async (o, root) => {
  const messages = [];
  let text;
  try {
    text = readStylesheet(root, o.stylesheet);
  } catch (err) {
    if (!(err instanceof StylesheetRefused)) throw err;
    const code = err.code === "E013" ? "E013 " : "";
    messages.push([err.code, `merlion stylesheet refused: ${code}${err.message}`, o.strict]);
    return { css: null, messages };
  }
  const compile = o.compileStylesheet ?? (await import("./wasm.js").then((m) => m.loadWasmCompile()));
  let res;
  try {
    res = await compile(text, { strict: Boolean(o.strict) });
  } catch (err) {
    res = { css: null, diagnostics: [internalError(err)] };
  }
  for (const d of Array.isArray(res?.diagnostics) ? res.diagnostics : []) {
    if (!reported(d)) continue;
    const at = d.line > 0 ? `${o.stylesheet}:${d.line}:${d.column || 1}` : o.stylesheet;
    messages.push([d.code ?? "E001", `${at}: ${d.code ?? "E001"} ${d.message ?? "compile failed"}`, d.severity === "error" && o.strict]);
  }
  const css = typeof res?.css === "string" ? res.css : null;
  if (css === null && !messages.some((m) => m[2])) {
    messages.push(["stylesheet", `merlion stylesheet ${o.stylesheet} failed to compile; diagrams use the page's other styles`, o.strict]);
  }
  return { css, messages };
};

/**
 * @typedef {object} Message
 * @property {string} reason
 * @property {string} ruleId
 * @property {{ line: number, column: number } | { start: object, end: object } | undefined} place
 *   A point in the Markdown file, or the block's whole position.
 * @property {boolean} fatal An error under `strict`.
 */

/**
 * One plugin instance, i.e. one build: the renderer is loaded, the stylesheet compiled
 * and the font warning given once, however many documents it sees.
 */
export const createSession = (options) => {
  const o = normalise(options);
  let loading = null;
  let sheet = null;
  let sheetReported = false;
  let fontWarned = false;

  const session = {
    o,
    getRender: () => o.render ?? (loading ??= import("./wasm.js").then((m) => m.loadWasmRender())),

    /**
     * The compiled stylesheet for a document with a diagram, compiled once; its messages
     * come back only the first time.
     * @returns {Promise<{ css: string | null, messages: [string, string, boolean][] }>}
     */
    stylesheet: async (root) => {
      const { css, messages } = await (sheet ??= compileStylesheetFile(o, root));
      if (sheetReported) return { css, messages: [] };
      sheetReported = true;
      return { css, messages };
    },

    /**
     * The per-document state: `open` a document, `render` each block, then `finish` for
     * the messages that depend on the whole document.
     * @param {{ path?: string, cwd?: string }} file
     */
    open: (file) => {
      const root = o.root ?? file.cwd ?? ".";
      const relPath = relPathOf(file.path, file.cwd, root);
      let cache;
      let cacheMessage = null;
      const getCache = () => {
        if (cache !== undefined) return cache;
        cache = null;
        if (!o.cacheDir) return cache;
        try {
          cache = openCache(root, o.cacheDir);
        } catch (err) {
          if (!(err instanceof CacheRefused) && err?.code === undefined) throw err;
          cacheMessage = { reason: `merlion cache disabled: ${err.message}`, ruleId: "cache-dir", place: undefined, fatal: false };
        }
        return cache;
      };
      let failed = 0;
      let rendered = 0;

      return {
        root,
        relPath,

        /**
         * Renders block `n` (1-based, document order). `position` is the fenced block's
         * position in the Markdown file. Returns the figure, or null to keep the code
         * block, and the block's messages.
         * @param {string} source
         * @param {number} n
         * @param {{ start?: { line: number, column: number }, end?: object } | undefined} position
         * @param {string} [meta] The fence info after the language; `width=<px>` sets
         *   this block's container width.
         * @returns {Promise<{ node: object | null, messages: Message[] }>}
         */
        render: async (source, n, position, meta) => {
          const render = await session.getRender();
          const messages = [];
          const block = fenceMeta(meta, o.width);
          if (block.error) messages.push({ reason: block.error, ruleId: "fence-meta", place: position, fatal: false });
          const width = block.width;
          const c = getCache();
          if (cacheMessage) {
            messages.push(cacheMessage);
            cacheMessage = null;
          }
          const hash = contentHash(source, { width, strict: o.strict });
          const key = cacheKey(relPath, n);
          const prev = c ? readEntry(c, key) : null;

          // Cache entries are untrusted (specs/security.md): the stored SVG is only ever a
          // layout hint, never inlined, so every block renders on every build.
          const ropts = { width, strict: o.strict, idPrefix: idPrefix(relPath, n) };
          if (prev) ropts.hint = prev.svg;
          let res;
          try {
            res = await render(source, ropts);
          } catch (err) {
            res = { svg: null, outline: null, diagnostics: [internalError(err)] };
          }
          // The fence is on the block's first line, so line k of the source is file line
          // fence + k; a diagnostic without a location points at the whole block.
          const start = position?.start;
          for (const d of Array.isArray(res?.diagnostics) ? res.diagnostics : []) {
            if (!reported(d)) continue;
            messages.push({
              reason: `${d.code ?? "E001"} ${d.message ?? "render failed"}`,
              ruleId: d.code ?? "E001",
              place: start && d.line > 0 ? { line: start.line + d.line, column: d.column || 1 } : position,
              fatal: d.severity === "error" && o.strict,
            });
          }
          const svg = typeof res?.svg === "string" ? res.svg : null;
          const outline = typeof res?.outline === "string" ? res.outline : null;
          if (svg && c && !(prev && prev.hash === hash && prev.svg === svg)) {
            try {
              writeEntry(c, key, { hash, svg, outline });
            } catch (err) {
              messages.push({ reason: `merlion cache write failed: ${err.message}`, ruleId: "cache-write", place: undefined, fatal: false });
            }
          }

          if (svg === null) {
            // Keep the code block; the diagnostics are in `messages`.
            failed++;
            return { node: null, messages };
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
          return { node: el("figure", { id: `diagram-${n}`, className: ["merlion-figure"] }, children), messages };
        },

        /**
         * Messages for the whole document: the font warning (once per session) and, under
         * `strict`, the failure count. A message with `fatal` fails the document.
         * @returns {Message[]}
         */
        finish: () => {
          const out = [];
          // Font mode "link" expects the page to load merlion-font.css.
          if (rendered > 0 && !o.fontCss && !fontWarned) {
            fontWarned = true;
            out.push({
              reason:
                "merlion: diagrams use font mode 'link'; load @fractalbox/merlion-themes/merlion-font.css on the page and set `fontCss: true` to silence this",
              ruleId: "font-css",
              place: undefined,
              fatal: false,
            });
          }
          if (o.strict && failed > 0) {
            out.push({
              reason: `${failed} Mermaid diagram${failed === 1 ? "" : "s"} failed to render`,
              ruleId: "merlion-strict",
              place: undefined,
              fatal: true,
            });
          }
          return out;
        },
      };
    },
  };
  return session;
};
