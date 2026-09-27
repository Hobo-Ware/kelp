#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
out="$root/target/Kelp.app"
work="$root/target/bundle-work"

cargo build --release -p kelp --manifest-path "$root/Cargo.toml"

rm -rf "${out:?}" "${work:?}"
mkdir -p "$out/Contents/MacOS" "$out/Contents/Resources" "$work/Kelp.iconset"

inkscape "$root/crates/kelp/assets/icon.svg" --export-type=png --export-width=1024 \
  --export-filename="$work/icon-1024.png" >/dev/null 2>&1
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$work/icon-1024.png" --out "$work/Kelp.iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$work/icon-1024.png" --out "$work/Kelp.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$work/Kelp.iconset" -o "$out/Contents/Resources/Kelp.icns"

cp "$root/target/release/kelp" "$out/Contents/MacOS/kelp"
cat > "$out/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Kelp</string>
  <key>CFBundleDisplayName</key><string>Kelp</string>
  <key>CFBundleIdentifier</key><string>com.hoboware.kelp</string>
  <key>CFBundleExecutable</key><string>kelp</string>
  <key>CFBundleIconFile</key><string>Kelp</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

rm -rf "${work:?}"
echo "Built $out ($(du -sh "$out" | cut -f1))"
