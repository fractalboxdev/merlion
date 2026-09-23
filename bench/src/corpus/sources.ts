/**
 * Pure extraction of Mermaid diagrams from the mermaid repository's demo pages,
 * documentation and end-to-end tests (the `compat` corpus, specs/benchmark.md#corpora).
 */
import { decodeEntities } from "../svg/xml.ts";

/**
 * Removes the indentation common to every non-blank line and the blank lines
 * around the block, as mermaid does before parsing an indented `<pre>` block.
 */
export const dedent = (s: string): string => {
  const lines = s.replace(/\r\n?/g, "\n").split("\n");
  while (lines.length > 0 && lines[0]!.trim() === "") lines.shift();
  while (lines.length > 0 && lines[lines.length - 1]!.trim() === "") lines.pop();
  let indent: string | null = null;
  for (const l of lines) {
    if (l.trim() === "") continue;
    const ws = /^[ \t]*/.exec(l)?.[0] ?? "";
    if (indent === null) {
      indent = ws;
    } else {
      let k = 0;
      while (k < indent.length && k < ws.length && indent[k] === ws[k]) k++;
      indent = indent.slice(0, k);
    }
  }
  const cut = indent?.length ?? 0;
  return lines.map((l) => (l.trim() === "" ? "" : l.slice(cut).trimEnd())).join("\n");
};

/** The corpora this module extracts: one per diagram type Merlion draws. */
export type DiagramKind = "flowchart" | "sequence" | "state";

/** The header keyword of each kind. Mermaid's lexer reads the keyword case-insensitively. */
const HEADER: Record<DiagramKind, RegExp> = {
  flowchart: /^(graph|flowchart|flowchart-elk)(\s|;|$)/,
  sequence: /^sequenceDiagram(\s|;|$)/i,
  state: /^stateDiagram(-v2)?(\s|;|$)/i,
};

/**
 * The diagram's header line, after optional YAML front matter, `%%{init}%%`
 * directives, `%%` comments and blank lines. `null` when the front matter never
 * closes or the source holds no statement line.
 */
const header = (src: string): string | null => {
  const lines = dedent(src).split("\n");
  let k = 0;
  if (lines[0]?.trim() === "---") {
    k = 1;
    while (k < lines.length && lines[k]!.trim() !== "---") k++;
    if (k >= lines.length) return null;
    k++;
  }
  for (; k < lines.length; k++) {
    const l = lines[k]!.trim();
    if (l === "" || l.startsWith("%%")) continue;
    return l;
  }
  return null;
};

/** True when the diagram's header is `graph`, `flowchart` or `flowchart-elk`. */
export const isFlowchart = (src: string): boolean => {
  const h = header(src);
  return h !== null && HEADER.flowchart.test(h);
};

/** True when the diagram's header is `sequenceDiagram`. */
export const isSequence = (src: string): boolean => {
  const h = header(src);
  return h !== null && HEADER.sequence.test(h);
};

/** True when the diagram's header is `stateDiagram` or `stateDiagram-v2`. */
export const isState = (src: string): boolean => {
  const h = header(src);
  return h !== null && HEADER.state.test(h);
};

const isKind: Record<DiagramKind, (src: string) => boolean> = {
  flowchart: isFlowchart,
  sequence: isSequence,
  state: isState,
};

/** Contents of every `<pre class="… mermaid …">` block, entity-decoded and dedented. */
export const extractHtmlPreBlocks = (html: string): string[] => {
  const out: string[] = [];
  const re = /<pre\b([^>]*)>([\s\S]*?)<\/pre>/gi;
  for (const m of html.matchAll(re)) {
    const attrs = m[1] ?? "";
    const cls = /\bclass\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(attrs);
    const classes = (cls?.[1] ?? cls?.[2] ?? "").split(/\s+/);
    if (!classes.includes("mermaid")) continue;
    out.push(dedent(decodeEntities(m[2] ?? "")));
  }
  return out;
};

/** Contents of every ```` ```mermaid ```` and ```` ```mermaid-example ```` fence (closed fences only). */
export const extractMarkdownFences = (md: string): string[] => {
  const out: string[] = [];
  const lines = md.replace(/\r\n?/g, "\n").split("\n");
  for (let k = 0; k < lines.length; k++) {
    const open = /^([ \t]*)(`{3,}|~{3,})\s*(mermaid|mermaid-example)\s*$/.exec(lines[k]!);
    if (open === null) continue;
    const fence = open[2]!;
    const body: string[] = [];
    let j = k + 1;
    let closed = false;
    for (; j < lines.length; j++) {
      const t = lines[j]!.trim();
      if (t.startsWith(fence[0]!.repeat(fence.length)) && t.replace(/[`~]/g, "") === "") {
        closed = true;
        break;
      }
      body.push(lines[j]!);
    }
    if (!closed) break;
    out.push(dedent(body.join("\n")));
    k = j;
  }
  return out;
};

/**
 * Static template literals in JavaScript/TypeScript source, dedented. Skips
 * literals with `${…}` substitutions (their text is computed at run time),
 * and backticks inside quoted strings and comments.
 */
export const extractTemplateLiterals = (js: string): string[] => {
  const out: string[] = [];
  const n = js.length;
  let i = 0;

  /** Scans a template literal starting after its opening backtick; returns [end index, text | null]. */
  const scanTemplate = (start: number, depth: number): [number, string | null] => {
    let k = start;
    let text = "";
    let dynamic = false;
    while (k < n) {
      const ch = js[k]!;
      if (ch === "\\") {
        const esc = js[k + 1] ?? "";
        text += esc === "n" ? "\n" : esc === "t" ? "\t" : esc === "\n" ? "" : esc;
        k += 2;
        continue;
      }
      if (ch === "`") return [k + 1, dynamic ? null : text];
      if (ch === "$" && js[k + 1] === "{") {
        dynamic = true;
        k = depth > 32 ? n : scanExpression(k + 2, depth + 1);
        continue;
      }
      text += ch;
      k++;
    }
    return [n, null];
  };

  /** Skips a `${…}` expression, including nested strings and templates; returns the index after `}`. */
  const scanExpression = (start: number, depth: number): number => {
    let k = start;
    let braces = 1;
    while (k < n) {
      const ch = js[k]!;
      if (ch === "'" || ch === '"') {
        k = skipString(k);
        continue;
      }
      if (ch === "`") {
        k = scanTemplate(k + 1, depth)[0];
        continue;
      }
      if (ch === "{") braces++;
      if (ch === "}") {
        braces--;
        if (braces === 0) return k + 1;
      }
      k++;
    }
    return n;
  };

  const skipString = (start: number): number => {
    const q = js[start];
    let k = start + 1;
    while (k < n && js[k] !== q && js[k] !== "\n") k += js[k] === "\\" ? 2 : 1;
    return k + 1;
  };

  while (i < n) {
    const ch = js[i]!;
    if (ch === "/" && js[i + 1] === "/") {
      const nl = js.indexOf("\n", i);
      i = nl < 0 ? n : nl + 1;
    } else if (ch === "/" && js[i + 1] === "*") {
      const end = js.indexOf("*/", i + 2);
      i = end < 0 ? n : end + 2;
    } else if (ch === "'" || ch === '"') {
      i = skipString(i);
    } else if (ch === "`") {
      const [end, text] = scanTemplate(i + 1, 0);
      if (text !== null) out.push(dedent(text));
      i = end;
    } else {
      i++;
    }
  }
  return out;
};

/** A short, file-name-safe slug for a repository path. */
export const sourceSlug = (path: string): string => {
  const known: Array<[RegExp, string]> = [
    [/^packages\/mermaid\/src\/docs\/syntax\//, "docs-"],
    [/^demos\//, "demos-"],
    [/^e2e\/rendering\/flowchart\/(flowchart-)?/, "e2e-flowchart-"],
    [/^e2e\/diagrams\/flowchart\//, "e2e-"],
    [/^e2e\/rendering\/sequence\/sequence[dD]iagram-?/, "e2e-sequence-"],
    [/^e2e\/rendering\/sequence\//, "e2e-sequence-"],
    [/^e2e\/diagrams\/sequence\//, "e2e-"],
    [/^e2e\/rendering\/state\/state[dD]iagram-?/, "e2e-state-"],
    [/^e2e\/rendering\/state\//, "e2e-state-"],
    [/^e2e\/diagrams\/state-diagram-v2\/(elk\/)?(v2-)?/, "e2e-v2-"],
    [/^e2e\/diagrams\/state-diagram\//, "e2e-"],
  ];
  let s = path;
  for (const [re, prefix] of known) {
    if (re.test(s)) {
      s = prefix + s.replace(re, "");
      break;
    }
  }
  return s
    .replace(/\.(spec\.)?(html|md|mmd|js|ts)$/, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
};

/**
 * Repository paths a corpus is drawn from: every demo page, the kind's syntax
 * documentation, and the kind's end-to-end tests (their spec files and their
 * `.mmd` fixtures). The `handdrawn/` fixtures repeat other fixtures with a
 * different `look` and are left out. Sorted.
 *
 * A state diagram is drawn under two directories, `state-diagram/` for the
 * `stateDiagram` header and `state-diagram-v2/` for `stateDiagram-v2`; both are
 * taken, and the identical sources among them collapse on content.
 */
const SOURCES: Record<DiagramKind, (p: string) => boolean> = {
  flowchart: (p) =>
    p === "packages/mermaid/src/docs/syntax/flowchart.md" ||
    /^e2e\/rendering\/flowchart\/[^/]+\.spec\.(js|ts)$/.test(p) ||
    /^e2e\/diagrams\/flowchart\/.+\.mmd$/.test(p),
  sequence: (p) =>
    p === "packages/mermaid/src/docs/syntax/sequenceDiagram.md" ||
    /^e2e\/rendering\/sequence\/[^/]+\.spec\.(js|ts)$/.test(p) ||
    /^e2e\/diagrams\/sequence\/.+\.mmd$/.test(p),
  state: (p) =>
    p === "packages/mermaid/src/docs/syntax/stateDiagram.md" ||
    /^e2e\/rendering\/state\/[^/]+\.spec\.(js|ts)$/.test(p) ||
    /^e2e\/diagrams\/state-diagram(-v2)?\/.+\.mmd$/.test(p),
};

export const selectSourcePaths = (paths: readonly string[], kind: DiagramKind = "flowchart"): string[] =>
  paths
    .filter((p) => !p.includes("/handdrawn/") && (/^demos\/[^/]+\.html$/.test(p) || SOURCES[kind](p)))
    .sort();

/** Diagrams of one kind in one source file, in document order. */
export const extractDiagrams = (path: string, content: string, kind: DiagramKind = "flowchart"): string[] => {
  let blocks: string[];
  if (path.endsWith(".html")) blocks = extractHtmlPreBlocks(content);
  else if (path.endsWith(".md")) blocks = extractMarkdownFences(content);
  else if (/\.(js|ts)$/.test(path)) blocks = extractTemplateLiterals(content);
  // A `.mmd` under `e2e/diagrams/` is inserted into an HTML page by mermaid's own
  // harness, so the browser decodes its entities before the parser sees them: the four
  // fork and join fixtures spell the markers `&lt;&lt;fork&gt;&gt;`, which mermaid draws
  // as bars and Merlion would drop with `W024`.
  else if (path.endsWith(".mmd")) blocks = [dedent(decodeEntities(content))];
  else blocks = [];
  return blocks.filter((b) => isKind[kind](b) && hasStatements(b));
};

/** True when a line other than front matter, directives, comments and the header remains. */
const hasStatements = (src: string): boolean => {
  const lines = src.split("\n");
  let k = 0;
  if (lines[0]?.trim() === "---") {
    k = 1;
    while (k < lines.length && lines[k]!.trim() !== "---") k++;
    k++;
  }
  let header = false;
  for (; k < lines.length; k++) {
    const l = lines[k]!.trim();
    if (l === "" || l.startsWith("%%")) continue;
    if (!header) {
      header = true;
      // Statements may follow the header on the same line after `;`.
      if (/;\s*\S/.test(l)) return true;
      continue;
    }
    return true;
  }
  return false;
};
