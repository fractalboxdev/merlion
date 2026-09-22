#!/usr/bin/env bash
# Builds the committed WOFF2 subsets for `font: "embed"` (specs/text-measurement.md#serving-the-font).
# Needs fontTools with Brotli: python3 -m venv .tmp/venv && .tmp/venv/bin/pip install fonttools brotli
# Run from the repository root: tools/fontgen/subset.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
pyftsubset="${PYFTSUBSET:-$root/.tmp/venv/bin/pyftsubset}"
unicodes="$(cargo run --quiet -p fontgen --release -- --unicodes)"
out="$root/crates/merlion-render/assets"
mkdir -p "$out"

for weight in Regular SemiBold; do
  # `kern` is the only layout feature kept: measurement applies pair kerning and
  # nothing else, so `calt`/`liga` must not exist in the drawn font either.
  "$pyftsubset" "$root/tools/fontgen/input/Inter-$weight.ttf" \
    --unicodes="$unicodes" \
    --layout-features=kern \
    --no-hinting \
    --flavor=woff2 \
    --output-file="$out/Inter-$weight.subset.woff2"
done

cp "$root/tools/fontgen/OFL.txt" "$out/OFL.txt"
ls -l "$out"
