#!/bin/sh
# Builds the docs in Workers Builds, whose image has Node and pnpm but no Rust.
# See README.md#docs-site for the build connection settings.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

if ! command -v rustup >/dev/null 2>&1; then
  installer=$(mktemp)
  trap 'rm -f "$installer"' EXIT HUP INT TERM
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o "$installer"
  sh "$installer" -y --no-modify-path --profile minimal --default-toolchain none
  # shellcheck disable=SC1091
  . "${CARGO_HOME:-$HOME/.cargo}/env"
fi

# Install the channel, components and wasm32 target from rust-toolchain.toml.
rustup toolchain install --profile minimal
sh packages/merlion-wasm/scripts/build-wasm.sh
pnpm install --frozen-lockfile --ignore-scripts
pnpm --filter docs run build
