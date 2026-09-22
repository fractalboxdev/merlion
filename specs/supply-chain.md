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

- **Tags.** Only maintainers can create `v*` tags (a repository tag ruleset). The release workflow first verifies the tag's signature against `.github/allowed_signers`, committed in the repository, and stops if it doesn't verify.
- **Release workflow.** One workflow, triggered only by a `v*` tag push, holds `id-token: write`. It runs in a protected `release` environment with required reviewers, uses no caches shared with other workflows, and pins every action by commit SHA. No workflow triggered by `pull_request_target` exists, and no workflow that runs third-party input (the benchmark, which renders external corpora in headless Chromium) has publish permissions.
- **npm** packages are published with `--provenance` (OIDC from the release workflow, no long-lived npm token).
- **crates.io** publishing uses trusted publishing (OIDC), with no stored token.
- **Reproducible artifacts.** Binaries and the `.wasm` file are built reproducibly (pinned toolchain via `rust-toolchain.toml`, `--remap-path-prefix`, `SOURCE_DATE_EPOCH`), and their SHA-256 values and build attestations are published with the release. A second job rebuilds from the tag on a different runner image and operating system family, and fails the release if any hash differs, so a single compromised image can't produce matching bad artifacts. The build recipe is documented so that anyone can reproduce the hashes.
- **Derived artifacts.** The npm `.wasm` and the binaries bundled in `merlion-vscode` are the attested release artifacts, checked by hash, never rebuilt ([integrations.md](integrations.md)).
