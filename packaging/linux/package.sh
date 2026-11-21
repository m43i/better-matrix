#!/usr/bin/env bash
# Packages a Linux release build (target/release/better-matrix) as a tarball, .deb, .rpm, an
# Arch package (.pkg.tar.xz) and an AppImage, all into dist/out with names that don't carry the
# version, so "latest" download links stay stable. Needs nfpm, zstd, xz, patchelf and the
# GStreamer plugins the AppImage carries; fetches linuxdeploy and its GStreamer plugin.
#
#   packaging/linux/package.sh <x86_64|aarch64> <version>
set -euo pipefail

arch="$1"
version="$2"
case "$arch" in
  x86_64) pkgarch=amd64 ;;
  aarch64) pkgarch=arm64 ;;
  *) echo "unknown architecture: $arch" >&2; exit 1 ;;
esac

# Absolute: the AppImage tools run unpacked from elsewhere and resolve relative paths there.
out="$PWD/dist/out"
name="better-matrix-linux-$arch"
mkdir -p "$out"
install -Dm755 target/release/better-matrix dist/better-matrix

# The tarball the PKGBUILD installs from: the binary and what a desktop needs to list it.
stage=dist/tar/better-matrix
mkdir -p "$stage"
cp dist/better-matrix packaging/linux/chat.bettermatrix.desktop.desktop LICENSE "$stage/"
cp apps/desktop/assets/icon.svg "$stage/better-matrix.svg"
cp packaging/icons/better-matrix-256.png packaging/icons/better-matrix-512.png "$stage/"
tar -C dist/tar -czf "$out/$name.tar.gz" better-matrix

# Distribution packages, from one description. ARCH and VERSION go to nfpm only: the AppImage
# tools below read the same names and expect other values.
pack() { ARCH="$pkgarch" VERSION="$version" nfpm package -f packaging/nfpm.yaml -p "$1" -t "$2"; }
pack deb "$out/$name.deb"
pack rpm "$out/$name.rpm"
pack archlinux dist/arch.pkg.tar.zst
# pacman takes xz as well as zstd; xz is what was asked for.
zstd -dc dist/arch.pkg.tar.zst | xz -T0 -9 > "$out/$name.pkg.tar.xz"

# AppImage, for distributions without one of the packages above. linuxdeploy bundles the
# libraries a desktop may lack and leaves out the ones every system has. Its GStreamer plugin
# bundles GStreamer whole, the core the binary links with the plugins installed here (playbin,
# the codecs) and a hook that points GStreamer at them alone, so the AppImage plays video on a
# system without GStreamer, and never mixes this core with a system's plugins built for another.
# linuxdeploy finds its plugins on PATH.
curl -fsSL -o dist/linuxdeploy "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$arch.AppImage"
curl -fsSL -o dist/linuxdeploy-plugin-gstreamer.sh \
  "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gstreamer/432af639725bc8b99df12351e86bb2e8a6b3585f/linuxdeploy-plugin-gstreamer.sh"
chmod +x dist/linuxdeploy dist/linuxdeploy-plugin-gstreamer.sh
cp packaging/icons/better-matrix-256.png dist/better-matrix.png
PATH="$PWD/dist:$PATH" APPIMAGE_EXTRACT_AND_RUN=1 LDAI_OUTPUT="$out/$name.AppImage" LINUXDEPLOY_OUTPUT_VERSION="$version" dist/linuxdeploy \
  --appdir "$PWD/dist/AppDir" \
  --executable "$PWD/dist/better-matrix" \
  --desktop-file "$PWD/packaging/linux/chat.bettermatrix.desktop.desktop" \
  --icon-file "$PWD/dist/better-matrix.png" \
  --plugin gstreamer \
  --output appimage

ls -la "$out"
