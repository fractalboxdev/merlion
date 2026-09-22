// Layout-hint cache for the rehype plugin (specs/integrations.md#fractalboxdevmerlion-rehype).
//
// Entries are untrusted input (specs/security.md): anyone who can write the
// cache directory controls them. Reads skip anything that is not a regular
// file or does not parse; writes follow the CLI's file-handling rules
// (specs/integrations.md#file-handling): temp file in the target directory,
// flushed, renamed over the target, never written through a symbolic link.
import {
  closeSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  realpathSync,
  renameSync,
  unlinkSync,
  writeSync,
} from "node:fs";
import { isAbsolute, join, relative, resolve, sep } from "node:path";

/** Largest stored SVG reused as a hint: the core's 1 MiB input limit (specs/architecture.md#boundaries). */
export const MAX_SVG = 1 << 20;
const MAX_FILE = 4 * MAX_SVG;

export class CacheRefused extends Error {}

/**
 * Resolve `cacheDir` under `root` and create it if missing. Refuses a directory
 * outside the root, the root itself, and any path component that is a
 * symbolic link or not a directory.
 */
export const openCache = (root, cacheDir) => {
  let base;
  try {
    base = realpathSync(root);
  } catch {
    throw new CacheRefused(`project root ${root} does not exist`);
  }
  const dir = resolve(base, cacheDir);
  const rel = relative(base, dir);
  if (rel === "" || rel.startsWith("..") || isAbsolute(rel)) {
    throw new CacheRefused(`cacheDir ${cacheDir} is not inside the project root`);
  }
  let cur = base;
  for (const part of rel.split(sep)) {
    cur = join(cur, part);
    let st = null;
    try {
      st = lstatSync(cur);
    } catch {
      mkdirSync(cur);
      continue;
    }
    if (st.isSymbolicLink()) throw new CacheRefused(`cacheDir component ${cur} is a symbolic link`);
    if (!st.isDirectory()) throw new CacheRefused(`cacheDir component ${cur} is not a directory`);
  }
  return dir;
};

const entryPath = (dir, key) => join(dir, `${key}.json`);

/** Read entry `key`, or null when it is missing, a link, oversized or malformed. */
export const readEntry = (dir, key) => {
  const p = entryPath(dir, key);
  try {
    const st = lstatSync(p);
    if (!st.isFile() || st.size > MAX_FILE) return null;
    const e = JSON.parse(readFileSync(p, "utf8"));
    if (e?.v !== 1 || typeof e.hash !== "string" || typeof e.svg !== "string") return null;
    if (e.svg.length > MAX_SVG) return null;
    return { hash: e.hash, svg: e.svg, outline: typeof e.outline === "string" ? e.outline : null };
  } catch {
    return null;
  }
};

let seq = 0;

/** Write entry `key` atomically. Throws on I/O failure; the previous entry then stays intact. */
export const writeEntry = (dir, key, { hash, svg, outline }) => {
  const target = entryPath(dir, key);
  const tmp = join(dir, `.${key}.${process.pid}.${seq++}.tmp`);
  const data = JSON.stringify({ v: 1, hash, svg, outline: outline ?? null });
  // `wx` = O_CREAT | O_EXCL: fails rather than follow a planted file or link.
  const fd = openSync(tmp, "wx", 0o644);
  try {
    writeSync(fd, data);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  try {
    // rename(2) replaces a symbolic link at `target`; it never writes through it.
    renameSync(tmp, target);
  } catch (err) {
    try {
      unlinkSync(tmp);
    } catch {
      // The temp file is already gone.
    }
    throw err;
  }
};
