import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createDemoServer, contentType } from "./serve.mjs";

let server;
let base;

before(async () => {
  server = createDemoServer();
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  base = `http://127.0.0.1:${server.address().port}`;
});
after(() => server.close());

test("content types, including application/wasm", () => {
  assert.equal(contentType("a.wasm"), "application/wasm");
  assert.equal(contentType("a.js"), "text/javascript; charset=utf-8");
  assert.equal(contentType("a.mjs"), "text/javascript; charset=utf-8");
  assert.equal(contentType("a.svg"), "image/svg+xml");
  assert.equal(contentType("a.css"), "text/css; charset=utf-8");
  assert.equal(contentType("a.json"), "application/json; charset=utf-8");
  assert.equal(contentType("a.html"), "text/html; charset=utf-8");
  assert.equal(contentType("a.unknown"), "application/octet-stream");
});

test("serves repo files with their type; / redirects to the demo", async () => {
  const res = await fetch(`${base}/demo/index.html`);
  assert.equal(res.status, 200);
  assert.equal(res.headers.get("content-type"), "text/html; charset=utf-8");
  const js = await fetch(`${base}/packages/merlion-view/merlion-view.js`);
  assert.equal(js.headers.get("content-type"), "text/javascript; charset=utf-8");
  const root = await fetch(`${base}/`, { redirect: "manual" });
  assert.equal(root.status, 302);
  assert.equal(root.headers.get("location"), "/demo/");
  const dir = await fetch(`${base}/demo/`);
  assert.equal(dir.status, 200);
});

test("missing files are 404; traversal outside the repo is refused", async () => {
  assert.equal((await fetch(`${base}/demo/nope.svg`)).status, 404);
  assert.equal((await fetch(`${base}/%2e%2e/%2e%2e/etc/passwd`)).status, 404);
  assert.equal((await fetch(`${base}/demo/%E0%A4%A`)).status, 400);
  assert.equal((await fetch(`${base}/.gitignore`)).status, 404);
  assert.equal((await fetch(`${base}/.git/HEAD`)).status, 404);
  assert.equal((await fetch(`${base}/demo/index.html`, { method: "POST" })).status, 405);
});
