#!/bin/sh
# Builds merlion.wasm from crates/merlion-wasm (specs/integrations.md#fractalboxmerlion-wasm).
#
#   scripts/build-wasm.sh              release build: writes merlion.wasm and records its
#                                      SHA-256 in package.json under merlion.wasmSha256
#   scripts/build-wasm.sh --test-trap  test build with the `trap` export: writes
#                                      test/merlion-test.wasm; package.json is untouched
#
# The build is reproducible: pinned toolchain (rust-toolchain.toml), locked lockfile, and
# the checkout path remapped so no absolute path lands in the binary
# (specs/supply-chain.md#releases).
set -eu

pkg_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
root=$(CDPATH='' cd -- "$pkg_dir/../.." && pwd)

mode="release"
case "${1:-}" in
  "") ;;
  --test-trap) mode="test-trap" ;;
  *)
    echo "usage: $0 [--test-trap]" >&2
    exit 2
    ;;
esac

cd "$root"
# One flag per variable entry, so a checkout path with spaces stays one argument.
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$root=."
export CARGO_ENCODED_RUSTFLAGS

if [ "$mode" = "release" ]; then
  cargo build --locked --profile release-wasm --target wasm32-unknown-unknown -p merlion-wasm
  cp target/wasm32-unknown-unknown/release-wasm/merlion_wasm.wasm "$pkg_dir/merlion.wasm"
  node "$pkg_dir/scripts/record-sha256.mjs" "$pkg_dir"
else
  # A separate target dir keeps the feature build from invalidating the release one.
  CARGO_TARGET_DIR="$root/target/wasm-test-trap"
  export CARGO_TARGET_DIR
  cargo build --locked --profile release-wasm --target wasm32-unknown-unknown \
    -p merlion-wasm --features test-trap
  cp "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release-wasm/merlion_wasm.wasm" \
    "$pkg_dir/test/merlion-test.wasm"
fi
