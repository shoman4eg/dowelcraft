#!/usr/bin/env bash
# Builds "Dowel Craft.app" (the program, Info.plist, icon) in DIR. macOS only: needs sips, iconutil, codesign.
# Usage: scripts/macos-app.sh path/to/dowel-craft VERSION DIR
set -euo pipefail
bin=$1 version=$2 dir=$3
app="$dir/Dowel Craft.app"
icon="$(dirname "$0")/../apps/gui/assets/icon.png"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/dowel-craft"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
set_dir=$tmp/AppIcon.iconset
mkdir "$set_dir"
for s in 16 32 128 256 512; do
  sips -z $s $s "$icon" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$icon" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$app/Contents/Resources/AppIcon.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Dowel Craft</string>
  <key>CFBundleDisplayName</key><string>Dowel Craft</string>
  <key>CFBundleIdentifier</key><string>io.github.shoman4eg.dowelcraft</string>
  <key>CFBundleExecutable</key><string>dowel-craft</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature: not a Developer ID, but Apple Silicon refuses to start a bundle without any signature.
codesign --force --sign - "$app"
