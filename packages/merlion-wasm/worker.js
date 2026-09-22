// Web Worker entry: runs `render`/`check`/`compileStylesheet` off the main thread
// (specs/integrations.md#fractalboxdevmerlion-wasm).
//
// Request:  { id, type: "render" | "check" | "compileStylesheet", source, options?, wasm? }
//           `source` is the stylesheet text for "compileStylesheet".
//           `wasm` (bytes, Module, URL or Response) is used by the first message only;
//           without it the worker fetches merlion.wasm next to this file.
// Response: { id, result } or { id, error: string }
import { init, render, check, compileStylesheet } from "./index.js";

/** @type {Promise<void> | null} */
let ready = null;

self.addEventListener("message", async (event) => {
  const { id, type, source, options, wasm } = event.data ?? {};
  try {
    if (!ready) {
      ready = init(wasm).catch((e) => {
        // Let a later message retry initialisation.
        ready = null;
        throw e;
      });
    }
    await ready;
    const result =
      type === "check"
        ? check(source, options)
        : type === "compileStylesheet"
          ? compileStylesheet(source, options)
          : render(source, options);
    self.postMessage({ id, result });
  } catch (err) {
    self.postMessage({ id, error: err instanceof Error ? err.message : String(err) });
  }
});
