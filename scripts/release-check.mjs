#!/usr/bin/env node
// Checks that a release tag matches the npm packages, then prints their directories in
// publish order, one per line (specs/supply-chain.md#releases). Zero dependencies.
//
//   node scripts/release-check.mjs v0.2.0
//   node scripts/release-check.mjs          the tag the packages' current version implies
//
// The packages version in lockstep: every packages/*/package.json carries the tag's
// version, and every peer range on another @fractalbox/merlion-* package names exactly
// that version. Peers publish before the packages that name them, so an install never
// resolves a peer range against a version the registry doesn't have yet.
// Exits 1 listing each mismatch.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Package directories, each after every workspace package it names as a peer. */
export const PUBLISH_ORDER = [
  "packages/merlion-themes",
  "packages/merlion-wasm",
  "packages/merlion-view",
  "packages/merlion-rehype",
  "packages/merlion-astro",
];

export const check = (tag, root = ROOT) => {
  const problems = [];
  const m = /^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tag ?? "");
  if (!m) return [`tag ${JSON.stringify(tag)} is not v<semver>`];
  const version = m[1];

  const dirs = readdirSync(join(root, "packages"), { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => `packages/${d.name}`)
    .sort();
  const listed = [...PUBLISH_ORDER].sort();
  if (JSON.stringify(dirs) !== JSON.stringify(listed)) {
    problems.push(`PUBLISH_ORDER lists ${listed.join(", ")}; packages/ holds ${dirs.join(", ")}`);
  }

  const names = new Map();
  for (const dir of PUBLISH_ORDER) {
    const pkg = JSON.parse(readFileSync(join(root, dir, "package.json"), "utf8"));
    names.set(pkg.name, dir);
    if (pkg.private) problems.push(`${dir}: "private": true`);
    if (pkg.version !== version) problems.push(`${dir}: version ${pkg.version}, tag ${tag}`);
    for (const [peer, range] of Object.entries(pkg.peerDependencies ?? {})) {
      if (peer.startsWith("@fractalbox/merlion-") && range !== version) {
        problems.push(`${dir}: peer ${peer}@${range}, tag ${tag}`);
      }
    }
  }
  for (const dir of PUBLISH_ORDER) {
    const pkg = JSON.parse(readFileSync(join(root, dir, "package.json"), "utf8"));
    for (const peer of Object.keys(pkg.peerDependencies ?? {})) {
      const peerDir = names.get(peer);
      if (peerDir && PUBLISH_ORDER.indexOf(peerDir) > PUBLISH_ORDER.indexOf(dir)) {
        problems.push(`${dir} publishes before its peer ${peerDir}`);
      }
    }
  }
  return problems;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const current = JSON.parse(readFileSync(join(ROOT, PUBLISH_ORDER[0], "package.json"), "utf8")).version;
  const problems = check(process.argv[2] ?? `v${current}`);
  if (problems.length) {
    for (const p of problems) console.error(p);
    process.exit(1);
  }
  console.log(PUBLISH_ORDER.join("\n"));
}
