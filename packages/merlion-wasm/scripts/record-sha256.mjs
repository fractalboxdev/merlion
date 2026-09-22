// Records the SHA-256 of <pkg>/merlion.wasm in <pkg>/package.json under
// merlion.wasmSha256. The value is what a consumer compares against the release's build
// attestation and published checksums (specs/integrations.md#fractalboxdevmerlion-wasm).
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
if (!dir) {
  console.error("usage: record-sha256.mjs <package dir>");
  process.exit(2);
}
const sha = createHash("sha256")
  .update(readFileSync(join(dir, "merlion.wasm")))
  .digest("hex");
const path = join(dir, "package.json");
const pkg = JSON.parse(readFileSync(path, "utf8"));
pkg.merlion = { ...pkg.merlion, wasmSha256: sha };
writeFileSync(path, `${JSON.stringify(pkg, null, 2)}\n`);
console.log(`merlion.wasm sha256 ${sha}`);
