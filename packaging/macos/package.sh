#!/usr/bin/env bash
# Packages a macOS release build (target/release/better-matrix) as "Better Matrix.app" in a
# disk image to drag to Applications, dist/out/better-matrix-macos-arm64.dmg, and in
# dist/out/better-matrix-macos-arm64.zip for the self-update. The app is signed ad hoc only (no
# Developer ID): see the README for opening it the first time.
#
#   packaging/macos/package.sh <version>
set -euo pipefail

version="$1"
app="dist/app/Better Matrix.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" dist/out

install -m755 target/release/better-matrix "$app/Contents/MacOS/better-matrix"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

iconset=dist/better-matrix.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" packaging/icons/better-matrix-1024.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" packaging/icons/better-matrix-1024.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/better-matrix.icns"

codesign --force --deep --sign - "$app"
# Self-updates replace the complete signed bundle; ditto preserves its metadata.
ditto -c -k --sequesterRsrc --keepParent "$app" dist/out/better-matrix-macos-arm64.zip
# The disk image opens on a window to drag the app to Applications (dmg-settings.py), built by
# dmgbuild (Python 3.11+) from a hash-pinned install in a virtual environment.
python3 -m venv dist/dmgbuild
dist/dmgbuild/bin/pip install --quiet --disable-pip-version-check --require-hashes \
  --only-binary=:all: -r packaging/macos/dmgbuild-requirements.txt
# Attaching or detaching fails now and then on hosted runners ("Resource busy"): a few tries.
for try in 1 2 3; do
  rm -f dist/out/better-matrix-macos-arm64.dmg
  if dist/dmgbuild/bin/dmgbuild -s packaging/macos/dmg-settings.py -D "app=$app" \
    -D "icon=$app/Contents/Resources/better-matrix.icns" \
    "Better Matrix" dist/out/better-matrix-macos-arm64.dmg; then
    exit 0
  fi
  echo "dmgbuild failed (try $try of 3)"
  sleep 5
done
exit 1
