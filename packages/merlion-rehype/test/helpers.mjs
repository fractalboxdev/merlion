// Hand-built hast trees and a fake renderer for the plugin tests.

export const text = (value) => ({ type: "text", value });

export const el = (tagName, properties, children = []) => ({
  type: "element",
  tagName,
  properties,
  children,
});

/** `<pre><code class="language-mermaid">…</code></pre>` as remark-rehype emits it, fence on `line`. */
export const mermaidBlock = (source, line = 1) => ({
  ...el("pre", {}, [el("code", { className: ["language-mermaid"] }, [text(source)])]),
  position: { start: { line, column: 1 }, end: { line: line + 2, column: 4 } },
});

export const root = (...children) => ({ type: "root", children });

/**
 * A fake `render(source, options)` following the @fractalboxdev/merlion-wasm
 * contract (its index.d.ts). A source containing `BROKEN` fails to parse with E002 on line 2;
 * `WARN` succeeds with a W010 warning.
 */
export const fakeRender = () => {
  const calls = [];
  const render = (source, options) => {
    calls.push({ source, options });
    if (source.includes("BROKEN")) {
      return {
        svg: null,
        outline: null,
        diagnostics: [
          {
            severity: "error",
            code: "E002",
            line: 2,
            column: 5,
            byteStart: 0,
            byteEnd: 1,
            message: "unexpected token",
            fix: null,
          },
        ],
      };
    }
    const id = options.idPrefix ?? "m00000000";
    const diagnostics = source.includes("WARN")
      ? [
          {
            severity: "warning",
            code: "W010",
            line: 1,
            column: 1,
            byteStart: 0,
            byteEnd: 0,
            message: "style rejected",
            fix: null,
          },
        ]
      : [];
    return {
      svg: `<svg xmlns="http://www.w3.org/2000/svg" id="${id}" viewBox="0 0 10 10"><title id="${id}-title">Flowchart diagram</title></svg>`,
      outline: `outline of ${id}`,
      diagnostics,
    };
  };
  return { render, calls };
};

/** Collect every node of a tree in document order. */
export const all = (node, out = []) => {
  out.push(node);
  for (const c of node.children ?? []) all(c, out);
  return out;
};
