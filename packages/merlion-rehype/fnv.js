// FNV-1a 64 over UTF-8 bytes, and the ids and cache keys derived from it
// (specs/integrations.md#fractalboxdevmerlion-rehype, specs/svg-output.md#ids-and-data-attributes).

const OFFSET = 0xcbf29ce484222325n;
const PRIME = 0x100000001b3n;
const MASK = 0xffffffffffffffffn;
const utf8 = new TextEncoder();

/** FNV-1a 64 of the UTF-8 encoding of `str`, as 16 lowercase hex digits. */
export const fnv1a64 = (str) => {
  let h = OFFSET;
  for (const b of utf8.encode(str)) h = ((h ^ BigInt(b)) * PRIME) & MASK;
  return h.toString(16).padStart(16, "0");
};

// The (path, index) pair as one string. The index is decimal digits and comes
// last, so the pair is recoverable by splitting at the final `-`.
const pair = (relPath, n) => `${relPath}-${n}`;

/**
 * `id_prefix` for block `n` (1-based) of the file at `relPath` (relative to the
 * project root, `/`-separated): `m` + the first 8 hex digits of FNV-1a 64.
 */
export const idPrefix = (relPath, n) => `m${fnv1a64(pair(relPath, n)).slice(0, 8)}`;

/** Cache entry name for block `n` of `relPath`: all 16 hex digits, so no path can escape `cacheDir`. */
export const cacheKey = (relPath, n) => fnv1a64(pair(relPath, n));

/**
 * Hash of everything that decides a block's SVG apart from the hint: the source
 * and the render options the plugin passes. A change in either re-renders.
 */
export const contentHash = (source, { width, strict }) =>
  fnv1a64(JSON.stringify([1, source, width, strict]));
