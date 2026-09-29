// The core's SVG as hast elements (specs/integrations.md#fractalboxmerlion-rehype).
//
// A `raw` node works for Markdown serialised straight to HTML, but MDX compiles hast
// to JSX and rejects `raw`, so the figure carries elements instead. The input is only
// ever the core's own output, a closed grammar (specs/svg-output.md): elements with
// double-quoted attributes, self-closing empty elements, text escaped with `&amp;`
// `&lt;` `&gt;` `&quot;` and numeric references, and no comments, CDATA or
// processing instructions. Anything outside that grammar returns null and the caller
// keeps the `raw` node.

// Attribute name -> hast property name (the `property-information` SVG schema) for the
// attributes the core emits whose property name differs from the attribute. `data-*`
// follows the hast rule below; every other attribute keeps its name.
const PROPERTY = {
  class: "className",
  "aria-labelledby": "ariaLabelledBy",
  "font-family": "fontFamily",
  "font-size": "fontSize",
  "font-style": "fontStyle",
  "font-weight": "fontWeight",
  "marker-end": "markerEnd",
  "marker-start": "markerStart",
  "stroke-dasharray": "strokeDashArray",
  "stroke-width": "strokeWidth",
  "text-anchor": "textAnchor",
};

const NAMED = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };

/** Decodes entities; null for an unknown or malformed reference. */
const decode = (s) => {
  if (!s.includes("&")) return s;
  let bad = false;
  const out = s.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-z]+);|&/g, (m, ref) => {
    if (ref === undefined) return ((bad = true), m);
    if (ref[0] !== "#") return NAMED[ref] ?? ((bad = true), m);
    const cp = ref[1] === "x" ? parseInt(ref.slice(2), 16) : parseInt(ref.slice(1), 10);
    if (!(cp > 0 && cp <= 0x10ffff) || (cp >= 0xd800 && cp <= 0xdfff)) return ((bad = true), m);
    return String.fromCodePoint(cp);
  });
  return bad ? null : out;
};

/** `data-merlion-rank` -> `dataMerlionRank`, the hast rule for data attributes. */
const dataProperty = (name) => "data" + name.slice(4).replace(/-([a-z])/g, (_, c) => c.toUpperCase());

const property = (name) => (name.startsWith("data-") ? dataProperty(name) : (PROPERTY[name] ?? name));

const TAG = /<(\/?)([a-zA-Z][\w:-]*)((?:\s+[a-zA-Z_:][\w:.-]*="[^"<]*")*)\s*(\/?)>/y;
const ATTR = /\s+([a-zA-Z_:][\w:.-]*)="([^"<]*)"/g;

/**
 * Parses one SVG document of the core's output into a hast `svg` element.
 * @param {string} svg
 * @returns {import("hast").Element | null} null when the input is outside the grammar.
 */
export const svgToHast = (svg) => {
  const root = { type: "root", children: [] };
  const stack = [root];
  let i = 0;
  while (i < svg.length) {
    const lt = svg.indexOf("<", i);
    const end = lt === -1 ? svg.length : lt;
    if (end > i) {
      const value = decode(svg.slice(i, end));
      if (value === null) return null;
      const parent = stack[stack.length - 1];
      // HTML serialisers write <style> text verbatim, so a decoded `<` there could close
      // the element. The core never emits one; if it did, the escaped raw string is safe.
      if (parent.tagName === "style" && value.includes("<")) return null;
      parent.children.push({ type: "text", value });
    }
    if (lt === -1) break;
    TAG.lastIndex = lt;
    const m = TAG.exec(svg);
    if (!m) return null;
    const [whole, closing, tagName, attrs, selfClosing] = m;
    i = lt + whole.length;
    if (closing) {
      if (attrs || selfClosing) return null;
      const open = stack.pop();
      if (open === root || open.tagName !== tagName) return null;
      continue;
    }
    const properties = {};
    for (const [, name, raw] of attrs.matchAll(ATTR)) {
      const value = decode(raw);
      if (value === null) return null;
      const key = property(name);
      if (key in properties) return null;
      properties[key] = key === "className" ? value.split(/[ \t\n\f\r]+/).filter(Boolean) : value;
    }
    const node = { type: "element", tagName, properties, children: [] };
    stack[stack.length - 1].children.push(node);
    if (!selfClosing) stack.push(node);
  }
  if (stack.length !== 1) return null;
  // Whitespace around the root element carries nothing.
  const [top, ...rest] = root.children.filter((c) => c.type !== "text" || c.value.trim() !== "");
  return top?.type === "element" && top.tagName === "svg" && rest.length === 0 ? top : null;
};
