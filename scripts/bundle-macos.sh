#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
out="$root/target/Kelp.app"

cargo build --release -p kelp --manifest-path "$root/Cargo.toml"

rm -rf "${out:?}"
mkdir -p "$out/Contents/MacOS" "$out/Contents/Resources"
cp "$root/target/release/kelp" "$out/Contents/MacOS/kelp"
cp "$root/crates/kelp/assets/Kelp.icns" "$out/Contents/Resources/Kelp.icns"
cp "$root/crates/kelp/assets/Assets.car" "$out/Contents/Resources/Assets.car"
sed "s/__VERSION__/${version}/g" "$root/packaging/Info.plist" > "$out/Contents/Info.plist"
echo "Built $out ($(du -sh "$out" | cut -f1))"
