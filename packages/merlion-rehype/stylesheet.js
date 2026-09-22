// Reads a Merlion stylesheet under the CLI's file-handling rules
// (specs/integrations.md#file-handling): the path resolves inside the project root, is
// not a symbolic link, and names a regular file of at most 64 KiB, checked from its
// metadata before any byte is read. The stylesheet is untrusted input
// (specs/security.md); pages only ever link its compiled output.
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve } from "node:path";

/** The stylesheet size limit (specs/svg-output.md#stylesheet). */
export const MAX_STYLESHEET_BYTES = 64 * 1024;

export class StylesheetRefused extends Error {
  /**
   * @param {string} message
   * @param {string} [code] `E013` for the size limit; otherwise `stylesheet`.
   */
  constructor(message, code = "stylesheet") {
    super(message);
    this.code = code;
  }
}

const inside = (base, p) => {
  const rel = relative(base, p);
  return rel !== "" && !rel.startsWith("..") && !isAbsolute(rel);
};

/**
 * Returns the stylesheet text at `path` (relative to `root`, or absolute inside it).
 * Throws `StylesheetRefused` naming why; the message never quotes the file's content.
 */
export const readStylesheet = (root, path) => {
  let base;
  try {
    base = realpathSync(root);
  } catch {
    throw new StylesheetRefused(`project root ${root} does not exist`);
  }
  const abs = resolve(base, path);
  if (!inside(base, abs)) throw new StylesheetRefused(`${path}: resolves outside the project root`);
  let st;
  try {
    st = lstatSync(abs);
  } catch (err) {
    throw new StylesheetRefused(`${path}: cannot read: ${err.code ?? err.message}`);
  }
  if (st.isSymbolicLink()) throw new StylesheetRefused(`${path}: is a symbolic link`);
  if (!st.isFile()) throw new StylesheetRefused(`${path}: not a regular file`);
  let dir;
  try {
    dir = realpathSync(dirname(abs));
  } catch (err) {
    throw new StylesheetRefused(`${path}: cannot read: ${err.code ?? err.message}`);
  }
  if (dir !== base && !inside(base, dir)) throw new StylesheetRefused(`${path}: resolves outside the project root`);
  if (st.size > MAX_STYLESHEET_BYTES) {
    throw new StylesheetRefused(`${path}: stylesheet exceeds its limit on size`, "E013");
  }
  const bytes = readFileSync(abs);
  if (bytes.length > MAX_STYLESHEET_BYTES) {
    throw new StylesheetRefused(`${path}: stylesheet exceeds its limit on size`, "E013");
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new StylesheetRefused(`${path}: not UTF-8`);
  }
};
