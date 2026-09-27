#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
svg="$root/crates/kelp/assets/icon.svg"
out="$root/crates/kelp/assets/Kelp.icns"
work="$(mktemp -d)"
trap 'rm -rf "${work:?}"' EXIT

mkdir -p "$work/Kelp.iconset"
for size in 16 32 128 256 512; do
  for scale in 1 2; do
    px=$((size * scale))
    name="icon_${size}x${size}"
    [ "$scale" = 2 ] && name="${name}@2x"
    png="$work/Kelp.iconset/${name}.png"
    inkscape "$svg" --export-type=png --export-width="$px" --export-filename="$png" >/dev/null 2>&1
    pngquant --force --speed 1 --strip 64 --output "$png" "$png"
    oxipng -q -o 6 --strip safe "$png"
  done
done
iconutil -c icns "$work/Kelp.iconset" -o "$out"
echo "Wrote $out ($(du -h "$out" | cut -f1))"
