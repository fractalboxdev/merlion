// Default renderer: @fractalboxdev/merlion-wasm, initialised synchronously from
// the `.wasm` file next to its entry point (specs/integrations.md).
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const PKG = "@fractalboxdev/merlion-wasm";

/** Import the WASM package, run `initSync`, and return its `render`. */
export const loadWasmRender = async () => {
  let mod;
  let entry;
  try {
    mod = await import(PKG);
    entry = fileURLToPath(import.meta.resolve(PKG));
  } catch (cause) {
    throw new Error(`${PKG} is not installed; install it or pass the \`render\` option`, { cause });
  }
  const dir = dirname(entry);
  const wasm =
    ["merlion.wasm", "merlion_wasm.wasm"].map((n) => join(dir, n)).find((p) => existsSync(p)) ??
    readdirSync(dir)
      .filter((n) => n.endsWith(".wasm"))
      .sort()
      .map((n) => join(dir, n))[0];
  if (!wasm) throw new Error(`${PKG}: no .wasm file next to ${entry}`);
  mod.initSync(readFileSync(wasm));
  return (source, options) => mod.render(source, options);
};
