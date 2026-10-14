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

cp "$root/assets/icons/macos/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"

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
