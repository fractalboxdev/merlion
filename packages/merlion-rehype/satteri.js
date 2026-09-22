// Sätteri adapter (specs/integrations.md#fractalboxmerlion-rehype): the rehype
// plugin's rendering as a Sätteri hast plugin, for Astro 7's default Markdown
// processor. Sätteri has no vfile, so diagnostics go to `onMessage` (default:
// console.warn) and a fatal one throws, which fails the document.
//
// No dependency on satteri: the adapter returns plain objects in its plugin shape.
import { fileURLToPath } from "node:url";
import { createSession } from "./session.js";

const NAME = "@fractalbox/merlion-rehype";

const isMermaid = (node) => {
  const kids = (node.children ?? []).filter((c) => c.type === "element");
  const code = kids.length === 1 ? kids[0] : null;
  if (code?.tagName !== "code") return null;
  const c = code.properties?.className;
  const list = Array.isArray(c) ? c : typeof c === "string" ? c.split(/\s+/) : [];
  return list.includes("language-mermaid") || code.data?.lang === "mermaid" ? code : null;
};

const where = (file, place) => {
  const at = place?.line ? place : place?.start;
  return at?.line ? `${file}:${at.line}:${at.column || 1}` : file || "<markdown>";
};

const defaultOnMessage = (m) => console.warn(`${where(m.file, m.place)}: ${m.reason}`);

/**
 * @param {import("./satteri.js").SatteriOptions} [options]
 * @returns {import("./satteri.js").SatteriPluginFactory}
 */
export default function merlionSatteri(options = {}) {
  const { onMessage = defaultOnMessage, ...rest } = options;
  if (typeof onMessage !== "function") throw new TypeError("merlion-rehype: `onMessage` must be a function");
  const session = createSession(rest);
  const { o } = session;

  // Sätteri calls the factory once per document.
  return ({ fileURL } = {}) => {
    const path = fileURL ? fileURLToPath(fileURL) : undefined;
    const doc = session.open({ path, cwd: process.cwd() });
    const emit = (m) => {
      const at = m.place?.line ? m.place : m.place?.start;
      const out = { ...m, file: path ?? "", line: at?.line ?? 0, column: at?.line ? at.column || 1 : 0 };
      onMessage(out);
      if (m.fatal) throw new Error(`${where(out.file, m.place)}: ${m.reason}`);
    };
    let n = 0;
    let sheet = null;

    return {
      name: NAME,
      options: { position: true },
      element: {
        filter: ["pre"],
        async visit(node, ctx) {
          const code = isMermaid(node);
          if (!code) return;
          // Numbered before the first await, so in document order.
          const index = ++n;
          const source = ctx.textContent(code);
          if (o.stylesheet !== undefined) {
            sheet ??= session.stylesheet(doc.root).then(({ css, messages }) => {
              for (const [ruleId, reason, fatal] of messages) emit({ reason, ruleId, place: undefined, fatal });
              if (css !== null) ctx.data.merlion = { ...ctx.data.merlion, css };
            });
            await sheet;
          }
          const { node: figure, messages } = await doc.render(source, index, node.position, code.data?.meta);
          for (const m of messages) emit({ ...m, fatal: false });
          if (figure) ctx.replaceNode(node, figure);
        },
      },
      after() {
        for (const m of doc.finish()) emit(m);
      },
    };
  };
}
