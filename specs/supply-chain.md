# Supply chain

Merlion ships no third-party code to users. Every published artifact has zero runtime dependencies, and every release can be traced to a tagged commit.

## Dependency policy

| Artifact | Runtime dependencies | Enforcement |
|---|---|---|
| `merlion-render` | 0 | `[dependencies]` must be empty; CI fails otherwise |
| `merlion-cli` | 0 besides the core | Same check; arguments are parsed with `std::env::args` |
| `merlion-wasm` | 0 besides the core | No `wasm-bindgen`, `js-sys` or `web-sys` |
| `@fractalbox/merlion-wasm`, `@fractalbox/merlion-rehype`, `@fractalbox/merlion-astro`, `@fractalbox/merlion-view` | 0 | `"dependencies": {}` is checked in CI; `peerDependencies` only where the host framework is already present (`astro` for `@fractalbox/merlion-astro`) |

Development dependencies (the test runner, fuzzer, benchmark harness, `wasm-opt`, and the release tooling) are allowed. They are pinned by lockfile, and none reach a published artifact.

**Adding any runtime dependency needs an ADR.** It must name the dependency, its licence, its full transitive tree, and why writing the code is worse.

## Controls

- **Rust:** `cargo-deny` checks licences (allow-list: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib), advisories, duplicate versions and sources (crates.io only, no git dependencies). `cargo-vet` records an audit for every development dependency.
- **npm:** the lockfile is committed and CI installs with `--ignore-scripts`. Every published package's `files` field is an allow-list.
- **No network in builds:** `build.rs` scripts don't exist in the published crates, and generated sources (the font tables) are committed. A build needs only `rustc` and the crate sources.
- **Font inputs:** the Inter source files are pinned by SHA-256 in `tools/fontgen`. The generator refuses a font whose hash differs.

## Releases

- **Tags.** Only maintainers can create `v*` tags (a repository tag ruleset). The release workflow (`.github/workflows/release.yml`) first verifies the tag's SSH signature against `.github/allowed_signers` as it stands on `main`, never the tagged commit's copy, and requires the tagged commit to be reachable from `main`; it stops if either check fails.
- **Release workflow.** One workflow, triggered only by a `v*` tag push, holds `id-token: write`. It runs in a protected `release` environment with required reviewers, uses no caches shared with other workflows, and pins every action by commit SHA. No workflow triggered by `pull_request_target` exists, and no workflow that runs third-party input (the benchmark, which renders external corpora in headless Chromium) has publish permissions.
- **Versions.** The five npm packages version in lockstep: every `packages/*/package.json` carries the tag's version, and every peer range on another `@fractalbox/merlion-*` package names exactly that version. `scripts/release-check.mjs` enforces both, against the tag in the release workflow and against the current version in `pnpm test`, and prints the publish order, which puts each package after the peers it names.
- **npm** packages are packed with `pnpm pack`, which rewrites `workspace:` ranges, and published with `npm publish --provenance` through trusted publishing (OIDC from the release workflow, no long-lived npm token). A version with a prerelease suffix publishes under the `next` dist-tag. A package already on the registry at that version is skipped, so a release that failed partway re-runs cleanly.
- **First publish.** npm configures a trusted publisher only on a package that exists, so a package's first release authenticates with `NPM_TOKEN`, a granular token scoped to `@fractalbox` with at most a 7-day expiry, stored in the `release` environment. Once every package has a trusted publisher (repository `fractalboxdev/merlion`, workflow `release.yml`, environment `release`), the secret is deleted and the workflow falls back to OIDC.
- **Cutting a release.** Bump every `packages/*/package.json` version and the internal peer ranges, merge to `main`, then from `main`: `git tag -s v<version> -m v<version>` and `git push origin v<version>`. The `release` environment's reviewers approve the publish job.
- **crates.io** publishing uses trusted publishing (OIDC), with no stored token.
- **Reproducible artifacts.** Binaries and the `.wasm` file are built reproducibly (pinned toolchain via `rust-toolchain.toml`, `--remap-path-prefix`, `SOURCE_DATE_EPOCH`), and their SHA-256 values (`SHA256SUMS`, attached to the GitHub release with `merlion.wasm`) and build attestations (`gh attestation verify merlion.wasm --repo fractalboxdev/merlion`) are published with the release. A second job rebuilds from the tag on a different runner image and operating system family, and fails the release if any hash differs, so a single compromised image can't produce matching bad artifacts. The build recipe is documented so that anyone can reproduce the hashes.
- **Derived artifacts.** The npm `.wasm` and the binaries bundled in `merlion-vscode` are the attested release artifacts, checked by hash, never rebuilt ([integrations.md](integrations.md)).
- **`merlion.wasmSha256`.** `packages/merlion-wasm/scripts/build-wasm.sh` records the `.wasm` file's SHA-256 in `package.json`, so the committed value is empty until a build writes one. Its test suite holds the field to a hash or nothing, never a malformed value, and requires a hash under `MERLION_RELEASE=1`, which the release workflow sets. The value is meaningful only against the release attestation, so the comparison against the file on disk runs under that flag alone: a local build runs an unpinned toolchain without `SOURCE_DATE_EPOCH` and reproduces no attested hash, so the test skips with that reason instead of failing every working copy.
