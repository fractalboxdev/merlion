#!/usr/bin/env node
// Zero-dependency static server for the demo: serves the repository root.
//   node demo/serve.mjs            → http://0.0.0.0:4173/demo/
//   PORT=8080 node demo/serve.mjs
import { createServer } from "node:http";
import { createReadStream, realpathSync, statSync } from "node:fs";
import { extname, join, relative, resolve, sep, isAbsolute } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = realpathSync(resolve(fileURLToPath(new URL("..", import.meta.url))));

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".wasm": "application/wasm",
  ".md": "text/markdown; charset=utf-8",
  ".mmd": "text/plain; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
  ".ts": "text/plain; charset=utf-8",
  ".map": "application/json; charset=utf-8",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

export const contentType = (path) => TYPES[extname(path).toLowerCase()] ?? "application/octet-stream";

// Resolve a URL path to a file inside ROOT, or null. Symbolic links that
// resolve outside the repository are refused.
const locate = (pathname) => {
  let p = join(ROOT, pathname);
  let st;
  try {
    p = realpathSync(p);
    const rel = relative(ROOT, p);
    if (rel.startsWith("..") || isAbsolute(rel)) return null;
    st = statSync(p);
    if (st.isDirectory()) {
      p = join(p, "index.html");
      st = statSync(p);
    }
  } catch {
    return null;
  }
  return st.isFile() ? { path: p, size: st.size } : null;
};

const send = (res, status, body, headers = {}) => {
  res.writeHead(status, { "content-type": "text/plain; charset=utf-8", ...headers });
  res.end(body);
};

export const createDemoServer = () =>
  createServer((req, res) => {
    if (req.method !== "GET" && req.method !== "HEAD") return send(res, 405, "method not allowed", { allow: "GET, HEAD" });
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    } catch {
      return send(res, 400, "bad request");
    }
    if (pathname.includes("\0")) return send(res, 400, "bad request");
    // The server binds 0.0.0.0: never expose dotfiles (.git, .env*, .claude).
    if (pathname.split("/").some((s) => s.startsWith("."))) return send(res, 404, "not found");
    if (pathname === "/") return send(res, 302, "", { location: "/demo/" });
    // A directory URL without its trailing slash would break relative links.
    const file = locate(pathname.split("/").join(sep));
    if (!file) return send(res, 404, "not found");
    if (!pathname.endsWith("/") && file.path.endsWith(`${sep}index.html`) && !pathname.endsWith("index.html")) {
      return send(res, 301, "", { location: `${pathname}/` });
    }
    res.writeHead(200, {
      "content-type": contentType(file.path),
      "content-length": file.size,
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
    });
    if (req.method === "HEAD") return res.end();
    createReadStream(file.path)
      .on("error", () => res.destroy())
      .pipe(res);
  });

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const port = Number(process.env.PORT) || 4173;
  createDemoServer().listen(port, "0.0.0.0", () => {
    console.log(`merlion demo: http://localhost:${port}/demo/ (serving ${ROOT}, bound to 0.0.0.0)`);
  });
}
