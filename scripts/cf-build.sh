#!/usr/bin/env sh
# Builds the docs site inside Cloudflare Workers Builds (the Worker `merlion-docs`).
#
# Workers Builds settings:
#   Root directory:          /            (repository root; the build needs the Rust core)
#   Build command:           sh scripts/cf-build.sh
#   Deploy command:          pnpm --filter docs exec wrangler deploy
#   Non-production branches: pnpm --filter docs exec wrangler versions upload
#
# The build image ships Node and pnpm but no Rust, so the script installs the pinned
# toolchain from rust-toolchain.toml (minimal profile plus the wasm32 target), compiles
# merlion.wasm, installs the workspace without install scripts, and builds docs/dist.
set -eu

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain none
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

# rust-toolchain.toml names the channel and the wasm32-unknown-unknown target.
rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install

sh packages/merlion-wasm/scripts/build-wasm.sh
pnpm install --frozen-lockfile --ignore-scripts
pnpm --filter docs run build
