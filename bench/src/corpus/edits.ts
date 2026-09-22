/**
 * The `edits` corpus (specs/benchmark.md#corpora): pairs of a diagram and the
 * same diagram after a one-line edit. Edits are derived from the source text
 * with fixed rules, so the corpus is reproducible from `compat` alone.
 */
import { isFlowchart } from "./sources.ts";

export type EditKind = "add-node" | "add-edge" | "remove-edge" | "rename-label";

export interface EditPair {
  readonly name: string;
  readonly source: string;
  readonly kind: EditKind;
  readonly before: string;
  readonly after: string;
}

export interface SimpleEdge {
  readonly from: string;
  readonly to: string;
  /** Shape and label declared for the endpoint on this line (`[Label]`, `(Label)`, `{Label}`), if any. */
  readonly fromShape: string | null;
  readonly toShape: string | null;
}

const ID = "([A-Za-z][A-Za-z0-9_]*)";
const SHAPE = "(\\[[^\\[\\]]*\\]|\\([^()]*\\)|\\{[^{}]*\\})?";
const ARROW = "(?:-->|---|-\\.->|==>)";
const EDGE_LABEL = "(?:\\|[^|]*\\|\\s*)?";
const SIMPLE_EDGE = new RegExp(`^\\s*${ID}${SHAPE}\\s*${ARROW}\\s*${EDGE_LABEL}${ID}${SHAPE}\\s*;?\\s*$`);

/** Parses a line holding exactly one edge between two plain ids, else null. */
export const parseSimpleEdge = (line: string): SimpleEdge | null => {
  const m = SIMPLE_EDGE.exec(line);
  if (m === null || m[1] === undefined || m[3] === undefined) return null;
  return { from: m[1], to: m[3], fromShape: m[2] ?? null, toShape: m[4] ?? null };
};

/** Minimum number of simple edge lines for a diagram to be edited. */
const MIN_SIMPLE_EDGES = 3;
const NEW_NODE = "benchNewNode[New node]";

/**
 * Edits of one diagram, in the order add-node, add-edge, remove-edge,
 * rename-label; kinds whose rule finds no site are left out.
 */
export const makeEdits = (source: string, text: string): EditPair[] => {
  if (!isFlowchart(text)) return [];
  const lines = text.split("\n");
  const simple = lines.map((l, k) => ({ k, e: parseSimpleEdge(l) })).filter((x): x is { k: number; e: SimpleEdge } => x.e !== null);
  if (simple.length < MIN_SIMPLE_EDGES) return [];
  const lastLine = lines[simple[simple.length - 1]!.k]!;
  const indent = /^[ \t]*/.exec(lastLine)?.[0] ?? "";

  const out: EditPair[] = [];
  const push = (kind: EditKind, after: string): void => {
    if (after !== text) out.push({ name: `${source}--${kind}`, source, kind, before: text, after });
  };

  // add-node: an isolated node appended at the end.
  push("add-node", `${text}\n${indent}${NEW_NODE}`);

  // add-edge: the farthest-apart pair (in order of first appearance) that no simple line connects.
  const order: string[] = [];
  for (const { e } of simple) for (const id of [e.from, e.to]) if (!order.includes(id)) order.push(id);
  const connected = new Set(simple.flatMap(({ e }) => [`${e.from}\u0000${e.to}`, `${e.to}\u0000${e.from}`]));
  addEdge: for (let gap = order.length - 1; gap >= 2; gap--) {
    for (let i = 0; i + gap < order.length; i++) {
      const u = order[i]!;
      const v = order[i + gap]!;
      if (!connected.has(`${u}\u0000${v}`)) {
        push("add-edge", `${text}\n${indent}${u} --> ${v}`);
        break addEdge;
      }
    }
  }

  // remove-edge: the last simple line. Endpoints declared on that line, or
  // mentioned nowhere else, are re-declared in its place so every node survives.
  {
    const { k, e } = simple[simple.length - 1]!;
    const lineIndent = /^[ \t]*/.exec(lines[k]!)?.[0] ?? "";
    const others = lines.filter((_, j) => j !== k).join("\n");
    const mentioned = (id: string): boolean => new RegExp(`(^|[^A-Za-z0-9_])${id}([^A-Za-z0-9_]|$)`, "m").test(others);
    const keep: string[] = [];
    for (const [id, shape] of [
      [e.from, e.fromShape],
      [e.to, e.toShape],
    ] as const) {
      if (shape !== null || !mentioned(id)) keep.push(`${lineIndent}${id}${shape ?? ""}`);
    }
    push("remove-edge", [...lines.slice(0, k), ...keep, ...lines.slice(k + 1)].join("\n"));
  }

  // rename-label: the first `id[Label]` with a plain label.
  const rename = /([A-Za-z][A-Za-z0-9_]*)\[([^\[\]"()|{}]+)\]/;
  for (let k = 0; k < lines.length; k++) {
    const m = rename.exec(lines[k]!);
    if (m === null || /^\s*(subgraph|style|classDef|class|click|linkStyle)\b/.test(lines[k]!)) continue;
    const renamed = lines[k]!.replace(rename, (_, id: string, label: string) => `${id}[${label.trim()} renamed]`);
    push("rename-label", [...lines.slice(0, k), renamed, ...lines.slice(k + 1)].join("\n"));
    break;
  }
  return out;
};
