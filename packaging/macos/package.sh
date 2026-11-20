#!/usr/bin/env bash
# Packages a macOS release build (target/release/better-matrix) as "Better Matrix.app" in a
# disk image to drag to Applications, dist/out/better-matrix-macos-arm64.dmg, and in
# dist/out/better-matrix-macos-arm64.zip for the self-update.
#
# Signing follows what the environment provides (ADR 0001):
# - MACOS_CERTIFICATE (a base64 .p12 with a Developer ID Application certificate and its key) and
#   MACOS_CERTIFICATE_PASSWORD: the app and the disk image are signed with it, the app with the
#   hardened runtime and packaging/macos/entitlements.plist.
# - APPLE_API_KEY (a base64 App Store Connect API .p8), APPLE_API_KEY_ID and APPLE_API_ISSUER_ID as
#   well: both are notarized and stapled, so they open without a warning.
# - Neither: the app is signed ad hoc; see the README for opening it the first time.
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

identity=-
notarizing=false
if [ -n "${MACOS_CERTIFICATE:-}" ]; then
  # A keychain of its own for the certificate, gone when the script ends.
  keychain="$PWD/dist/signing.keychain-db"
  keychain_password=$(uuidgen)
  searched=$(security list-keychains -d user | tr -d '"')
  security create-keychain -p "$keychain_password" "$keychain"
  # shellcheck disable=SC2086 # One keychain path per line, none with spaces.
  trap 'security list-keychains -d user -s $searched; security delete-keychain "$keychain"; rm -f dist/api-key.p8' EXIT
  security set-keychain-settings -lut 3600 "$keychain"
  security unlock-keychain -p "$keychain_password" "$keychain"
  printf '%s' "$MACOS_CERTIFICATE" | base64 --decode > dist/certificate.p12
  security import dist/certificate.p12 -k "$keychain" -P "${MACOS_CERTIFICATE_PASSWORD:-}" -T /usr/bin/codesign
  rm dist/certificate.p12
  # codesign may use the key without asking, and finds the certificate's chain in the search list.
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
  # shellcheck disable=SC2086
  security list-keychains -d user -s "$keychain" $searched
  identity=$(security find-identity -p codesigning "$keychain" | awk '$1 == "1)" { print $2; exit }')
  if [ -z "$identity" ]; then
    echo "MACOS_CERTIFICATE holds no code signing identity" >&2
    exit 1
  fi
  if [ -n "${APPLE_API_KEY:-}" ]; then
    printf '%s' "$APPLE_API_KEY" | base64 --decode > dist/api-key.p8
    notarizing=true
  else
    echo "::warning::No APPLE_API_KEY: the app is signed but not notarized, so macOS still warns."
  fi
fi

# Reads one field of notarytool's JSON answer.
field() { python3 -c 'import json, sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }

# Notarizes a .zip or .dmg, failing with Apple's log unless it is accepted.
notarize() {
  local result id
  result=$(xcrun notarytool submit "$1" --key dist/api-key.p8 --key-id "$APPLE_API_KEY_ID" \
    --issuer "$APPLE_API_ISSUER_ID" --wait --output-format json)
  if [ "$(field status <<<"$result")" != Accepted ]; then
    id=$(field id <<<"$result")
    xcrun notarytool log "$id" --key dist/api-key.p8 --key-id "$APPLE_API_KEY_ID" \
      --issuer "$APPLE_API_ISSUER_ID" >&2
    echo "Notarizing $1 failed: $result" >&2
    return 1
  fi
}

if [ "$identity" = - ]; then
  codesign --force --deep --sign - "$app"
else
  codesign --force --options runtime --timestamp --entitlements packaging/macos/entitlements.plist \
    --sign "$identity" "$app"
fi
if $notarizing; then
  ditto -c -k --keepParent "$app" dist/notarize.zip
  notarize dist/notarize.zip
  rm dist/notarize.zip
  xcrun stapler staple "$app"
fi
# Self-updates replace the complete signed bundle; ditto preserves its metadata.
ditto -c -k --sequesterRsrc --keepParent "$app" dist/out/better-matrix-macos-arm64.zip
# The disk image opens on a window to drag the app to Applications (dmg-settings.py), built by
# dmgbuild (Python 3.11+, macOS 14+) from a hash-pinned install in a virtual environment.
python3 -m venv dist/dmgbuild
dist/dmgbuild/bin/pip install --quiet --disable-pip-version-check --require-hashes \
  --only-binary=:all: -r packaging/macos/dmgbuild-requirements.txt
# Attaching or detaching fails now and then on hosted runners ("Resource busy"): a few tries.
dmg=dist/out/better-matrix-macos-arm64.dmg
for try in 1 2 3; do
  rm -f "$dmg"
  if dist/dmgbuild/bin/dmgbuild -s packaging/macos/dmg-settings.py -D "app=$app" \
    -D "icon=$app/Contents/Resources/better-matrix.icns" "Better Matrix" "$dmg"; then
    break
  fi
  echo "dmgbuild failed (try $try of 3)"
  if [ "$try" = 3 ]; then exit 1; fi
  sleep 5
done
if [ "$identity" != - ]; then
  codesign --timestamp --sign "$identity" "$dmg"
fi
if $notarizing; then
  notarize "$dmg"
  xcrun stapler staple "$dmg"
fi
