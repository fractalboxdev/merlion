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

/**
 * True when the diagram's header is `graph`, `flowchart` or `flowchart-elk`,
 * after optional YAML front matter, `%%{init}%%` directives, `%%` comments and
 * blank lines.
 */
export const isFlowchart = (src: string): boolean => {
  const lines = dedent(src).split("\n");
  let k = 0;
  if (lines[0]?.trim() === "---") {
    k = 1;
    while (k < lines.length && lines[k]!.trim() !== "---") k++;
    if (k >= lines.length) return false;
    k++;
  }
  for (; k < lines.length; k++) {
    const l = lines[k]!.trim();
    if (l === "" || l.startsWith("%%")) continue;
    return /^(graph|flowchart|flowchart-elk)(\s|;|$)/.test(l);
  }
  return false;
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
