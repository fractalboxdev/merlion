// rehype plugin: renders ```mermaid code blocks to inline SVG at build time
// (specs/integrations.md#fractalboxmerlion-rehype).
//
// The tree is walked by hand, iteratively, so the plugin has no
// unist-util-visit dependency and no recursion depth limit. Rendering itself lives in
// session.js, shared with the Sätteri adapter (satteri.js).
import { createSession } from "./session.js";

export { fnv1a64, idPrefix, cacheKey } from "./fnv.js";
export { compileStylesheetFile, diagramTitle } from "./session.js";

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

// A session message on the vfile. A block's error under `strict` is marked fatal and
// fails the file after every block is reported; a whole-file fatal message fails it now.
const toVfile = (file, m, now) => {
  if (m.fatal && now) file.fail(m.reason, { place: m.place, ruleId: m.ruleId, source: "merlion" });
  const msg = file.message(m.reason, { place: m.place, ruleId: m.ruleId, source: "merlion" });
  if (m.fatal) msg.fatal = true;
  return msg;
};

/**
 * @param {import("./index.js").Options} [options]
 */
export default function rehypeMerlion(options = {}) {
  const session = createSession(options);
  const { o } = session;

  return async (tree, file) => {
    const blocks = findBlocks(tree);
    if (blocks.length === 0) return;
    await session.getRender();
    const doc = session.open({ path: file.path, cwd: file.cwd });

    if (o.stylesheet !== undefined) {
      const { css, messages } = await session.stylesheet(doc.root);
      for (const [ruleId, reason, fatal] of messages) toVfile(file, { reason, ruleId, place: undefined, fatal }, true);
      // Page CSS for the diagrams of this file; inline renders never receive a palette.
      if (css !== null) file.data.merlion = { ...file.data.merlion, css };
    }

    for (const [i, { parent, index, pre, code }] of blocks.entries()) {
      const { node, messages } = await doc.render(textOf(code), i + 1, pre.position, code.data?.meta);
      for (const m of messages) toVfile(file, m, false);
      if (node) parent.children[index] = { ...node, position: pre.position };
    }
    for (const m of doc.finish()) toVfile(file, m, m.ruleId === "merlion-strict");
  };
}
