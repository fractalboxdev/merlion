// Tests of the hand-written glue against the real module, built with the test-only
// `trap` export (scripts/build-wasm.sh --test-trap).
import { test, before } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { init, initSync, render, check, compileStylesheet, __trapForTests } from "../index.js";

const script = fileURLToPath(new URL("../scripts/build-wasm.sh", import.meta.url));
const wasmPath = fileURLToPath(new URL("./merlion-test.wasm", import.meta.url));
let bytes;

const VALID = "flowchart LR\nA-->B\n";
const INVALID = "this is not a diagram\n";

before(() => {
  if (!process.env.MERLION_SKIP_WASM_BUILD) {
    execFileSync("sh", [script, "--test-trap"], { stdio: "inherit" });
  }
  bytes = readFileSync(wasmPath);
});

// --- A minimal hand-assembled module with the same exports, whose `render` returns a
// pointer 2 bytes before the end of memory, to exercise the glue's bounds checks.
function uleb(n) {
  const out = [];
  do {
    let b = n & 0x7f;
    n >>>= 7;
    if (n !== 0) b |= 0x80;
    out.push(b);
  } while (n !== 0);
  return out;
}
const vec = (items) => [...uleb(items.length), ...items.flat()];
const section = (id, body) => [id, ...uleb(body.length), ...body];
const name = (s) => vec([...Buffer.from(s)].map((b) => [b]));
const body = (code) => [...uleb(code.length + 1), 0x00, ...code];
function outOfBoundsModule() {
  const i32 = 0x7f;
  const types = vec([
    [0x60, 1, i32, 1, i32], // (i32) -> i32
    [0x60, 2, i32, i32, 0], // (i32, i32) -> ()
    [0x60, 4, i32, i32, i32, i32, 1, i32], // (i32 x4) -> i32
    [0x60, 1, i32, 0], // (i32) -> ()
  ]);
  const funcs = vec([[0], [1], [2], [2], [3]]);
  const memory = vec([[0x00, 0x01]]); // 1 page, no maximum
  const exp = (n, kind, idx) => [...name(n), kind, idx];
  const exports = vec([
    exp("memory", 2, 0),
    exp("alloc", 0, 0),
    exp("dealloc", 0, 1),
    exp("render", 0, 2),
    exp("check", 0, 3),
    exp("result_free", 0, 4),
  ]);
  const ret16 = [0x41, 16, 0x0b]; // i32.const 16
  const retEnd = [0x41, 0xfe, 0xff, 0x03, 0x0b]; // i32.const 65534
  const code = vec([body(ret16), body([0x0b]), body(retEnd), body(retEnd), body([0x0b])]);
  return new Uint8Array([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    ...section(1, types),
    ...section(3, funcs),
    ...section(5, memory),
    ...section(7, exports),
    ...section(10, code),
  ]);
}

test("calls before initialisation throw", () => {
  assert.throws(() => render(VALID), /init/);
});

test("initSync then render returns the documented shape", () => {
  initSync(bytes);
  const r = render(INVALID);
  assert.equal(r.svg, null);
  assert.equal(r.outline, null);
  assert.ok(Array.isArray(r.diagnostics) && r.diagnostics.length > 0);
  const d = r.diagnostics.find((x) => x.severity === "error");
  assert.ok(d, JSON.stringify(r));
  assert.match(d.code, /^E0\d\d$/);
  for (const k of ["line", "column", "byteStart", "byteEnd"]) assert.equal(typeof d[k], "number");
  assert.equal(typeof d.message, "string");
  assert.notEqual(r.error, null);
  assert.equal(typeof r.fuelUsed, "number");
});

test("a valid diagram renders with the camelCase options", () => {
  const r = render(VALID, { width: 640, idPrefix: "t1", font: "embed", edgeStyle: "polyline" });
  assert.ok(r.svg?.startsWith('<svg xmlns="http://www.w3.org/2000/svg" id="t1"'), JSON.stringify(r.diagnostics));
  assert.ok(r.svg.includes("@font-face"), "font: embed reaches the core");
  assert.equal(r.error, null);
  assert.equal(typeof r.outline, "string");
  assert.deepEqual(Object.keys(r).sort(), ["diagnostics", "error", "fuelUsed", "outline", "svg"]);
});

test("snake_case option names are rejected with the camelCase name", () => {
  assert.throws(() => render(VALID, { target_width: 600 }), /`target_width`.*`width`/);
  assert.throws(() => render(VALID, { id_prefix: "x" }), /`id_prefix`.*`idPrefix`/);
  assert.throws(() => render(VALID, { edge_style: "spline" }), /`edge_style`.*`edgeStyle`/);
});

test("autoTone: false draws the untoned diagram; the flag is typed", () => {
  const src = "flowchart LR\nsubgraph g [G]\n  d{D} --> c[(C)]\nend\nc --> s([S])\n";
  const on = render(src, { idPrefix: "a1" }).svg;
  assert.ok(on.includes('class="merlion-node merlion-c-warn merlion-auto"'), on);
  assert.ok(on.includes('class="merlion-cluster merlion-cc-series-1 merlion-auto"'));
  const off = render(src, { idPrefix: "a1", autoTone: false }).svg;
  assert.ok(!off.includes("merlion-auto"));
  const directive = render(`%%{init: {"merlion": {"autoTone": false}}}%%\n${src}`, { idPrefix: "a1" }).svg;
  assert.equal(directive, off);
  assert.equal(render(src, { idPrefix: "a1", autoTone: true }).svg, on);
  assert.throws(() => render(src, { autoTone: "no" }), TypeError);
  assert.throws(() => render(src, { auto_tone: false }), /`auto_tone`.*`autoTone`/);
  assert.throws(() => render(src, { autoTones: false }), /unknown option `autoTones`/);
});

test("diagnostics have the flat documented shape", () => {
  const r = render("flowchart LR\nA -->\n");
  const d = r.diagnostics.find((x) => x.code === "E002");
  assert.ok(d, JSON.stringify(r.diagnostics));
  assert.deepEqual(Object.keys(d).sort(), ["byteEnd", "byteStart", "code", "column", "fix", "line", "message", "severity"]);
  assert.equal(d.line, 2);
});

test("check returns diagnostics", () => {
  const ds = check(INVALID, { strict: true });
  assert.ok(Array.isArray(ds));
  assert.ok(ds.some((d) => d.severity === "error"));
});

test("invalid arguments throw instead of reaching the module", () => {
  assert.throws(() => render(42), TypeError);
  assert.throws(() => render(VALID, { width: -1 }), TypeError);
  assert.throws(() => render(VALID, { direction: "up" }), TypeError);
  // Unknown option keys throw, so an older module never silently ignores one.
  assert.throws(() => render(VALID, { zoom: 3 }), /unknown option `zoom`/);
  assert.throws(() => check(VALID, { zoom: 3 }), /unknown option `zoom`/);
});

const SHEET = `
:root { --merlion-accent: #0f766e; --merlion-fg: #202830; --merlion-stroke: 1.5px; }
[data-theme="dark"] { --merlion-bg: #101418; --merlion-fg: #e6e6e6; }
.merlion-c-store { --merlion-tone: #b8408f; --merlion-dash: 4 2; }
.merlion-cc-zone { --merlion-tone: #1b98a6; }
.viewer { color: red; }
`;
const ROLES = "flowchart LR\nsubgraph z [Zone]\nA-->B\nend\nclass A store\nclass z zone\n";

test("compileStylesheet returns page CSS, the palette and diagnostics", () => {
  const r = compileStylesheet(SHEET);
  assert.deepEqual(Object.keys(r).sort(), ["css", "diagnostics", "palette"]);
  assert.ok(r.css.startsWith(":root {\n  --merlion-fg: #202830;\n"), r.css);
  assert.ok(r.css.includes(".merlion .merlion-c-store {"), r.css);
  assert.ok(!r.css.includes("viewer"));
  assert.deepEqual(r.palette, {
    roles: { fg: "#202830", accent: "#0f766e", stroke: "1.5" },
    tones: { store: { tone: "#b8408f", dash: [4, 2] } },
    clusterTones: { zone: { tone: "#1b98a6" } },
  });
  assert.deepEqual(
    r.diagnostics.map((d) => [d.severity, d.code]),
    [["info", "I032"]],
  );
  // Compiling the output again is a fixed point.
  assert.equal(compileStylesheet(r.css).css, r.css);
});

test("compileStylesheet resolves a theme and an automatic dark variant", () => {
  const { palette } = compileStylesheet(SHEET, { theme: "dark", autoDark: "dark" });
  assert.equal(palette.roles.bg, "#101418");
  assert.equal(palette.dark.fg, "#e6e6e6");
  assert.deepEqual(palette.darkTones, { store: { tone: "#b8408f", dash: [4, 2] } });
  assert.throws(() => compileStylesheet(SHEET, { theme: "nope" }), RangeError);
  assert.throws(() => compileStylesheet(SHEET, { them: "dark" }), /unknown option `them`/);
  assert.throws(() => compileStylesheet(42), TypeError);
});

test("compileStylesheet strict mode and limits", () => {
  const bad = ":root { --merlion-bg: #fff; --merlion-font: Comic; }";
  const lax = compileStylesheet(bad);
  assert.ok(lax.css !== null && lax.diagnostics.some((d) => d.code === "W018" && d.severity === "warning"));
  const strict = compileStylesheet(bad, { strict: true });
  assert.equal(strict.css, null);
  assert.equal(strict.palette, null);
  assert.ok(strict.diagnostics.some((d) => d.code === "W018" && d.severity === "error"));
  // Over 64 KiB of UTF-8: E013 without crossing the boundary.
  const big = compileStylesheet(`:root{--merlion-bg:#fff}/*${"é".repeat(33 * 1024)}*/`);
  assert.deepEqual(big.diagnostics.map((d) => d.code), ["E013"]);
  assert.equal(big.css, null);
});

test("render bakes a palette from compileStylesheet", () => {
  const plain = render(ROLES, { idPrefix: "p1" }).svg;
  const { palette } = compileStylesheet(SHEET, { theme: "dark" });
  const baked = render(ROLES, { idPrefix: "p1", palette }).svg;
  assert.ok(baked.includes('stroke="#b8408f"'), "node tone in the attribute");
  assert.ok(baked.includes("#101418"), "dark background");
  const layout = (s) => /data-merlion-layout="([^"]*)"/.exec(s)[1];
  assert.equal(layout(baked), layout(plain));
  // Without an id prefix the palette joins the id: another palette, another id.
  const a = render(ROLES, { palette }).svg;
  const b = render(ROLES, { palette: compileStylesheet(SHEET).palette }).svg;
  const id = (s) => /^<svg [^>]*id="([^"]+)"/.exec(s)[1];
  assert.notEqual(id(a), id(b));
  assert.equal(render(ROLES, { palette }).svg, a);
  const dark = render(ROLES, { palette: compileStylesheet(SHEET, { autoDark: "dark" }).palette }).svg;
  assert.ok(dark.includes("@media (prefers-color-scheme: dark)"));
});

test("render validates the palette in the glue and the core", () => {
  for (const palette of [
    "palette-v1|",
    [],
    { roles: { bg: 1 } },
    { roles: { bg: "red" } },
    { roles: { "bg;fg": "#000" } },
    { roles: { bg: "#000;fg=#fff" } },
    { roles: {}, extra: {} },
    { roles: {}, tones: { a: { tone: "#000", dash: ["4"] } } },
    { roles: {}, tones: { a: { tone: "#000", dash: [Infinity] } } },
    { roles: {}, tones: { a: { hue: "#000" } } },
    { roles: {}, tones: { "a:b": {} } },
    { roles: {}, darkTones: {} },
    { roles: { stroke: "30" } },
    // More tones than a compiled stylesheet can produce (256 per table).
    { roles: {}, tones: Object.fromEntries(Array.from({ length: 257 }, (_, i) => [`r${i}`, { tone: "#123456" }])) },
  ]) {
    assert.throws(() => render(VALID, { palette }), TypeError, JSON.stringify(palette).slice(0, 80));
  }
  assert.doesNotThrow(() => render(VALID, { palette: { roles: {} } }));
  const full = Object.fromEntries(Array.from({ length: 256 }, (_, i) => [`r${i}`, { tone: "#123456" }]));
  assert.doesNotThrow(() => render(VALID, { palette: { roles: {}, tones: full } }));
});

test("non-ASCII and lone surrogates cross the boundary", () => {
  const r = render("flowchart LR\nA[é\u{1F600}\uD800]-->B\n");
  assert.ok(Array.isArray(r.diagnostics));
});

test("oversized input is reported as E004 / too_large", () => {
  const r = render("a".repeat((1 << 20) + 1));
  assert.equal(r.error?.kind, "too_large");
  assert.ok(r.diagnostics.some((d) => d.code === "E004"));
});

test("memory growth between calls does not break later calls", () => {
  for (let i = 0; i < 3; i++) {
    render("x".repeat(900_000 + i));
  }
  assert.ok(render(INVALID).diagnostics.length > 0);
});

test("a trap returns E001 and the next call runs on a fresh instance", () => {
  const r = __trapForTests();
  assert.equal(r.svg, null);
  assert.deepEqual(
    r.diagnostics.map((d) => d.code),
    ["E001"],
  );
  const next = render(INVALID);
  assert.ok(!next.diagnostics.some((d) => d.code === "E001"), JSON.stringify(next));
  assert.ok(check(INVALID).length > 0);
});

test("an out-of-bounds result pointer is an internal error, not a bad read", () => {
  initSync(outOfBoundsModule());
  const r = render(VALID);
  assert.deepEqual(
    r.diagnostics.map((d) => d.code),
    ["E001"],
  );
  assert.deepEqual(
    check(VALID).map((d) => d.code),
    ["E001"],
  );
  initSync(bytes);
});

test("init accepts bytes, a Module and a Response", async () => {
  await init(bytes);
  assert.ok(render(INVALID).diagnostics.length > 0);
  await init(new WebAssembly.Module(bytes));
  assert.ok(render(INVALID).diagnostics.length > 0);
  await init(new Response(bytes, { headers: { "content-type": "application/wasm" } }));
  assert.ok(render(INVALID).diagnostics.length > 0);
  // A Response with the wrong MIME type falls back to compiling the bytes.
  await init(new Response(bytes, { headers: { "content-type": "application/octet-stream" } }));
  assert.ok(render(INVALID).diagnostics.length > 0);
  await assert.rejects(init(new Response("nope", { status: 404 })), /404/);
});

test("worker.js answers render and check messages", async () => {
  const posted = [];
  let handler;
  globalThis.self = {
    addEventListener: (type, fn) => {
      if (type === "message") handler = fn;
    },
    postMessage: (m) => posted.push(m),
  };
  await import("../worker.js");
  assert.equal(typeof handler, "function");
  await handler({ data: { id: 1, type: "render", source: INVALID, wasm: bytes } });
  await handler({ data: { id: 2, type: "check", source: INVALID } });
  await handler({ data: { id: 3, type: "render", source: 7 } });
  await handler({ data: { id: 4, type: "compileStylesheet", source: SHEET, options: { theme: "dark" } } });
  assert.equal(posted.length, 4);
  assert.equal(posted[3].result.palette.roles.bg, "#101418");
  assert.equal(posted[0].id, 1);
  assert.equal(posted[0].result.svg, null);
  assert.ok(Array.isArray(posted[1].result));
  assert.equal(posted[2].id, 3);
  assert.match(posted[2].error, /string/);
  delete globalThis.self;
});
