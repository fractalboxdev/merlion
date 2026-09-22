// @ts-check
// Hand-written glue over merlion.wasm (specs/integrations.md#fractalboxdevmerlion-wasm).
//
// Strings cross the boundary as UTF-8 pointer-and-length pairs through the module's
// `alloc`/`dealloc` exports; `render`/`check` return a buffer holding a u32
// little-endian length followed by UTF-8 JSON, released with `result_free`. No
// wasm-bindgen, no dependencies.

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

/** The compiled module, kept so a trapped instance can be replaced. */
/** @type {WebAssembly.Module | null} */
let wasmModule = null;
/** @type {any} */
let wasm = null;
/** Pending asynchronous re-instantiation, when a synchronous one is not allowed. */
/** @type {Promise<void> | null} */
let replacing = null;
/** `render` is not re-entrant on one instance. */
let busy = false;

/** Thrown for invalid arguments; never caught as a trap. */
class ArgumentError extends TypeError {}

/**
 * @param {WebAssembly.Module} module
 * @param {WebAssembly.Instance} instance
 */
function install(module, instance) {
  wasmModule = module;
  wasm = instance.exports;
  replacing = null;
}

/**
 * Initialises the module in a browser (or any host with `fetch`). Without an argument
 * it fetches `merlion.wasm` next to this file.
 * @param {BufferSource | WebAssembly.Module | URL | string | Response | Promise<Response>} [source]
 */
export async function init(source) {
  let input = source ?? new URL("./merlion.wasm", import.meta.url);
  if (input instanceof URL || typeof input === "string") input = fetch(input);
  input = await input;
  if (input instanceof WebAssembly.Module) {
    install(input, await WebAssembly.instantiate(input, {}));
    return;
  }
  if (typeof Response !== "undefined" && input instanceof Response) {
    if (!input.ok) throw new Error(`merlion: fetching merlion.wasm failed with HTTP ${input.status}`);
    if (typeof WebAssembly.instantiateStreaming === "function") {
      try {
        const { module, instance } = await WebAssembly.instantiateStreaming(input.clone(), {});
        install(module, instance);
        return;
      } catch (e) {
        // A server that does not send `application/wasm` makes streaming fail with a
        // TypeError; compile the bytes instead.
        if (!(e instanceof TypeError)) throw e;
      }
    }
    input = await input.arrayBuffer();
  }
  const { module, instance } = await WebAssembly.instantiate(/** @type {BufferSource} */ (input), {});
  install(module, instance);
}

/**
 * Initialises the module synchronously (Node, build time).
 * @param {BufferSource | WebAssembly.Module} source
 */
export function initSync(source) {
  const module = source instanceof WebAssembly.Module ? source : new WebAssembly.Module(source);
  install(module, new WebAssembly.Instance(module, {}));
}

/**
 * Discards the current instance and creates a new one from the cached module: after a
 * trap the allocator state is undefined. Hosts that forbid synchronous instantiation of
 * large modules on the main thread get an asynchronous replacement instead; calls made
 * before it resolves return `E001`.
 */
function reset() {
  wasm = null;
  if (!wasmModule) return;
  const module = wasmModule;
  try {
    wasm = new WebAssembly.Instance(module, {}).exports;
  } catch {
    replacing = WebAssembly.instantiate(module, {}).then(
      (instance) => {
        if (wasmModule === module && wasm === null) wasm = instance.exports;
        replacing = null;
      },
      () => {
        replacing = null;
      },
    );
  }
}

/** Runs `body` against the instance; a trap or bad pointer yields `null`. */
function guarded(body) {
  if (busy) throw new Error("merlion: render is not re-entrant");
  if (!wasm) {
    if (replacing) return null;
    throw new Error("merlion: call init() or initSync() first");
  }
  busy = true;
  try {
    return body(wasm);
  } catch (e) {
    if (e instanceof ArgumentError) throw e;
    reset();
    return null;
  } finally {
    busy = false;
  }
}

/** A fresh view: `memory.grow` detaches earlier ones. */
function memory(ex) {
  return new Uint8Array(ex.memory.buffer);
}

function checkRange(ex, ptr, len) {
  if (ptr === 0 || ptr + len > ex.memory.buffer.byteLength) {
    throw new RangeError("merlion: pointer out of bounds");
  }
}

/** Copies bytes into a fresh module allocation. */
function put(ex, bytes) {
  if (bytes.length === 0) return 0;
  const ptr = ex.alloc(bytes.length) >>> 0;
  checkRange(ex, ptr, bytes.length);
  memory(ex).set(bytes, ptr);
  return ptr;
}

/** Calls `render` or `check` and returns the parsed JSON. */
function call(ex, fn, source, optionsJson) {
  const src = encoder.encode(source);
  const opts = encoder.encode(optionsJson);
  const sp = put(ex, src);
  const op = put(ex, opts);
  const rp = ex[fn](sp, src.length, op, opts.length) >>> 0;
  ex.dealloc(sp, src.length);
  ex.dealloc(op, opts.length);
  checkRange(ex, rp, 4);
  const len = new DataView(ex.memory.buffer).getUint32(rp, true);
  checkRange(ex, rp + 4, len);
  const text = decoder.decode(memory(ex).subarray(rp + 4, rp + 4 + len));
  ex.result_free(rp);
  return JSON.parse(text);
}

const OPTION_KEYS = ["width", "direction", "edgeStyle", "font", "strict", "idPrefix", "hint", "fuel"];

function optionsJson(options) {
  if (options == null) return "{}";
  if (typeof options !== "object") throw new ArgumentError("merlion: options must be an object");
  /** @type {Record<string, unknown>} */
  const picked = {};
  for (const k of OPTION_KEYS) if (options[k] !== undefined) picked[k] = options[k];
  return JSON.stringify(picked);
}

function checkSource(source) {
  if (typeof source !== "string") throw new ArgumentError("merlion: source must be a string");
}

function toDiagnostic(d) {
  return {
    severity: d.severity,
    code: d.code,
    line: d.line,
    column: d.column,
    byteStart: d.byte_start,
    byteEnd: d.byte_end,
    message: d.message,
    fix: d.fix ? { byteStart: d.fix.byte_start, byteEnd: d.fix.byte_end, replacement: d.fix.replacement } : null,
  };
}

function internalError() {
  return {
    severity: "error",
    code: "E001",
    line: 0,
    column: 0,
    byteStart: 0,
    byteEnd: 0,
    message: "InternalError: the WebAssembly instance trapped and was replaced",
    fix: null,
  };
}

function internalResult() {
  return { svg: null, outline: null, diagnostics: [internalError()], error: { kind: "internal" }, fuelUsed: 0 };
}

function rejectOptions(raw) {
  if (raw.error && raw.error.kind === "invalid_options") {
    throw new ArgumentError(`merlion: ${raw.error.message}`);
  }
}

/**
 * Renders one diagram. Synchronous; bounded by the fuel limit.
 * @param {string} source
 * @param {import("./index.d.ts").RenderOptions} [options]
 * @returns {import("./index.d.ts").RenderResult}
 */
export function render(source, options) {
  checkSource(source);
  const opts = optionsJson(options);
  const raw = guarded((ex) => call(ex, "render", source, opts));
  if (raw === null) return internalResult();
  rejectOptions(raw);
  return {
    svg: raw.svg,
    outline: raw.outline,
    diagnostics: raw.diagnostics.map(toDiagnostic),
    error: raw.error,
    fuelUsed: raw.fuel_used,
  };
}

/**
 * Parses without rendering.
 * @param {string} source
 * @param {{ strict?: boolean }} [options]
 * @returns {import("./index.d.ts").Diagnostic[]}
 */
export function check(source, options) {
  checkSource(source);
  const opts = optionsJson(options && { strict: options.strict });
  const raw = guarded((ex) => call(ex, "check", source, opts));
  if (raw === null) return [internalError()];
  rejectOptions(raw);
  return raw.diagnostics.map(toDiagnostic);
}

/**
 * Calls the `trap` export of a module built with the `test-trap` feature, through the
 * same recovery path as `render`. Not part of the public API.
 * @returns {import("./index.d.ts").RenderResult}
 */
export function __trapForTests() {
  const raw = guarded((ex) => {
    if (typeof ex.trap !== "function") throw new ArgumentError("merlion: module has no trap export");
    ex.trap();
    return {};
  });
  return raw === null ? internalResult() : { svg: null, outline: null, diagnostics: [], error: null, fuelUsed: 0 };
}
