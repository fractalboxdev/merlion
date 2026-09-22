#!/usr/bin/env node
// README.md and the docs site agree (zero dependencies):
//   1. every fenced code block in README's "## Quick start" section appears verbatim in
//      docs/src/content/docs/getting-started.md;
//   2. every relative link in README.md names a file or directory that exists;
//   3. README and the docs index carry the same guarantees table.
// Exits 1 listing each mismatch.
//
//   node scripts/check-docs-alignment.mjs [README.md] [getting-started.md] [index.md]
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Fenced code blocks (``` or longer, any info string) with their bodies. */
export const codeBlocks = (md) => {
  const out = [];
  const lines = md.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const open = /^(`{3,})(.*)$/.exec(lines[i]);
    if (!open) continue;
    const fence = open[1];
    const body = [];
    let j = i + 1;
    while (j < lines.length && !(lines[j].startsWith(fence) && lines[j].slice(fence.length).trim() === "")) body.push(lines[j++]);
    out.push({ info: open[2].trim(), body: body.join("\n"), line: i + 1 });
    i = j;
  }
  return out;
};

/** The text of a `## heading` section, up to the next `## `. */
export const section = (md, heading) => {
  const lines = md.split("\n");
  const start = lines.findIndex((l) => l.trim() === `## ${heading}`);
  if (start < 0) return null;
  const end = lines.findIndex((l, i) => i > start && /^## /.test(l));
  return lines.slice(start, end < 0 ? undefined : end).join("\n");
};

/** Relative link targets of Markdown links, without fragments, outside code. */
export const relativeLinks = (md) => {
  const prose = md.replace(/^(`{3,})[^\n]*\n[\s\S]*?^\1[ \t]*$/gm, "").replace(/`[^`\n]*`/g, "");
  const links = [];
  for (const m of prose.matchAll(/\]\(([^)\s]+)\)/g)) {
    const target = m[1];
    if (/^(?:[a-z][a-z0-9+.-]*:|#|\/)/i.test(target)) continue;
    links.push(decodeURI(target.split("#")[0]));
  }
  return links;
};

/** The first Markdown table following a `## Guarantees` heading, as trimmed rows. */
const guarantees = (md) => (section(md, "Guarantees") ?? "").split("\n").filter((l) => l.startsWith("|")).map((l) => l.trim());

export const check = (readmePath, gettingStartedPath, indexPath) => {
  const problems = [];
  const readme = readFileSync(readmePath, "utf8");
  const gettingStarted = readFileSync(gettingStartedPath, "utf8");
  const quick = section(readme, "Quick start");
  if (quick === null) problems.push(`${readmePath}: no "## Quick start" section`);
  else {
    const docBodies = new Set(codeBlocks(gettingStarted).map((b) => b.body));
    const blocks = codeBlocks(quick);
    if (blocks.length === 0) problems.push(`${readmePath}: "## Quick start" has no code block`);
    for (const b of blocks) {
      if (!docBodies.has(b.body)) {
        problems.push(`${readmePath}: Quick start block "${b.body.split("\n")[0]}" (${b.info || "no language"}) is not in ${gettingStartedPath}`);
      }
    }
  }
  for (const link of relativeLinks(readme)) {
    if (!existsSync(resolve(dirname(readmePath), link))) problems.push(`${readmePath}: link to missing file ${link}`);
  }
  if (indexPath) {
    const a = guarantees(readme);
    const b = guarantees(readFileSync(indexPath, "utf8"));
    if (a.length === 0 || a.join("\n") !== b.join("\n")) problems.push(`${readmePath}: the Guarantees table differs from ${indexPath}`);
  }
  return problems;
};

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const [readme, gs, index] = [
    process.argv[2] ?? resolve(ROOT, "README.md"),
    process.argv[3] ?? resolve(ROOT, "docs/src/content/docs/getting-started.md"),
    process.argv[4] ?? resolve(ROOT, "docs/src/content/docs/index.md"),
  ];
  const problems = check(readme, gs, index);
  for (const p of problems) console.error(p);
  if (problems.length) process.exitCode = 1;
  else console.log("docs alignment: README quick start, links and guarantees match the docs site");
}
