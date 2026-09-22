// Default renderer: @fractalboxdev/merlion-wasm, initialised synchronously from
// the `.wasm` file next to its entry point (specs/integrations.md).
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const PKG = "@fractalboxdev/merlion-wasm";

/**
 * Import the WASM package, run `initSync`, and return its `render`. `pkg` names the
 * package to load; only tests pass another name.
 */
export const loadWasmRender = async (pkg = PKG) => (await loadWasm(pkg)).render;

/** The initialised package's `compileStylesheet`. */
export const loadWasmCompile = async (pkg = PKG) => (await loadWasm(pkg)).compileStylesheet;

/** One initialisation per package name, shared by `render` and `compileStylesheet`. */
const loaded = new Map();
const loadWasm = (pkg) => {
  if (!loaded.has(pkg)) {
    const p = initWasm(pkg);
    p.catch(() => loaded.delete(pkg));
    loaded.set(pkg, p);
  }
  return loaded.get(pkg);
};

const initWasm = async (pkg) => {
  let mod;
  let entry;
  try {
    mod = await import(pkg);
    entry = fileURLToPath(import.meta.resolve(pkg));
  } catch (cause) {
    throw new Error(`${pkg} is not installed; install it or pass the \`render\` option`, { cause });
  }
  const dir = dirname(entry);
  const wasm =
    ["merlion.wasm", "merlion_wasm.wasm"].map((n) => join(dir, n)).find((p) => existsSync(p)) ??
    readdirSync(dir)
      .filter((n) => n.endsWith(".wasm"))
      .sort()
      .map((n) => join(dir, n))[0];
  if (!wasm) throw new Error(`${pkg}: no .wasm file next to ${entry}`);
  mod.initSync(readFileSync(wasm));
  return {
    render: (source, options) => mod.render(source, options),
    compileStylesheet: (css, options) => mod.compileStylesheet(css, options),
  };
};
