#!/bin/sh
# Wrap the macOS release binary in HydraMerge.app.
# Usage: package-macos-app.sh <binary> [version]
set -eu

binary=$1
version=${2:-0.1.0}
root=$(CDPATH= cd "$(dirname "$0")/.." && pwd)
app=HydraMerge.app

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/hydra-merge"
chmod +x "$app/Contents/MacOS/hydra-merge"

iconset=$(mktemp -d)/AppIcon.iconset
mkdir -p "$iconset"
for spec in "16 icon_16x16" "32 icon_16x16@2x" "32 icon_32x32" "64 icon_32x32@2x" "128 icon_128x128" "256 icon_128x128@2x" "256 icon_256x256" "512 icon_256x256@2x" "512 icon_512x512" "1024 icon_512x512@2x"; do
  size=${spec%% *}
  name=${spec#* }
  sips -z "$size" "$size" "$root/assets/logo.png" --out "$iconset/${name}.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>HydraMerge</string>
  <key>CFBundleDisplayName</key>
  <string>HydraMerge</string>
  <key>CFBundleIdentifier</key>
  <string>me.hydramerge.HydraMerge</string>
  <key>CFBundleExecutable</key>
  <string>hydra-merge</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF
