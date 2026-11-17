<div align="center">

<img src="apps/desktop/assets/icon.svg" width="96" alt="">

# Better Matrix

**A Matrix client for Linux, macOS and Windows, built around several accounts side by side.**

[![CI](https://github.com/m43i/better-matrix/actions/workflows/ci.yml/badge.svg)](https://github.com/m43i/better-matrix/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/m43i/better-matrix?include_prereleases)](https://github.com/m43i/better-matrix/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

![Better Matrix: signing in, a chat and a video call](docs/desktop.png)

</div>

## What it does

- **Every account at once.** Sign in to as many homeservers as you like and switch with one click;
  notifications, unread counts and calls work for all of them.
- **Encrypted by default.** Cross-signing, key backup and device verification, with the keys kept in
  your system's keychain.
- **Calls.** Voice and video calls, screen sharing and group calls, built to work with Element Call.
- **A tidy sidebar.** Spaces on a rail, your own sections, drag and drop, latest conversations first.
- **Everything a chat needs.** Threads, replies, reactions, polls, voice messages, files, GIFs and
  link previews.
- **Fast and native.** Rust from the network to the pixels, rendered on the GPU with
  [Rune](https://github.com/anomalyco/rune). In English, German and French.

## Compared with Element

[Element](https://element.io) is the reference Matrix client and covers more of Matrix than Better
Matrix does today. Better Matrix trades some of that breadth for several accounts in one window and
a native app instead of a web page in Electron:

|                                  | Better Matrix                     | Element Desktop                 |
| -------------------------------- | --------------------------------- | ------------------------------- |
| Accounts in one window           | As many as you like, side by side | One (a second needs a profile)  |
| Built with                       | Rust, drawn on the GPU            | Electron (Chromium and Node.js) |
| Processes                        | 1                                 | 8                               |
| Memory (PSS, shared pages split) | 104 MiB                           | 280 MiB                         |
| CPU while idle                   | 0.03%                             | 0.17%                           |

Measured on one Linux machine (Wayland, Element Desktop 1.12.25 on Electron 43), each app open and
idle, summed over all its processes for 30 seconds. A snapshot, not a controlled benchmark: memory
grows with the rooms an app has loaded.

## Install

Copy the line for your system into a terminal. Each one fetches the latest release from
[Releases](https://github.com/m43i/better-matrix/releases/latest), where you can also download the
files by hand. Linux packages come for x86_64 and ARM (aarch64); the commands pick the right one.
They need glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36, openSUSE Tumbleweed or later
(RHEL, AlmaLinux and Rocky Linux from version 10).

### Linux

**Debian, Ubuntu, Linux Mint, Pop!\_OS**

```sh
curl -fLo /tmp/better-matrix.deb "https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-linux-$(uname -m).deb" && sudo apt install -y /tmp/better-matrix.deb
```

**Fedora, RHEL 10, AlmaLinux 10, Rocky Linux 10**

```sh
sudo dnf install -y "https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-linux-$(uname -m).rpm"
```

**openSUSE**

```sh
sudo zypper install -y --allow-unsigned-rpm "https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-linux-$(uname -m).rpm"
```

**Arch Linux, Manjaro, EndeavourOS**

```sh
curl -fLo /tmp/better-matrix.pkg.tar.xz "https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-linux-$(uname -m).pkg.tar.xz" && sudo pacman -U --noconfirm /tmp/better-matrix.pkg.tar.xz
```

Or build the package yourself from the release's `PKGBUILD`:

```sh
mkdir -p ~/.cache/better-matrix && cd ~/.cache/better-matrix && curl -fLO https://github.com/m43i/better-matrix/releases/latest/download/PKGBUILD && makepkg -si
```

**Any other distribution: AppImage**

```sh
mkdir -p ~/.local/bin && curl -fLo ~/.local/bin/better-matrix "https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-linux-$(uname -m).AppImage" && chmod +x ~/.local/bin/better-matrix && ~/.local/bin/better-matrix
```

AppImages need FUSE 2 (`libfuse2`, called `fuse2` or `fuse-libs` on some distributions). Without it,
start it with `--appimage-extract-and-run`.

### macOS (Apple silicon)

```sh
curl -fLo /tmp/better-matrix.dmg https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-macos-arm64.dmg && hdiutil attach -nobrowse -quiet -mountpoint /tmp/better-matrix-dmg /tmp/better-matrix.dmg && rm -rf "/Applications/Better Matrix.app" && cp -R "/tmp/better-matrix-dmg/Better Matrix.app" /Applications/ && hdiutil detach -quiet /tmp/better-matrix-dmg && xattr -dr com.apple.quarantine "/Applications/Better Matrix.app"
```

Quit Better Matrix before updating. The app isn't signed with an Apple Developer ID or notarized
yet; the last step lets it open without a warning. Because of that, macOS asks again for keychain,
camera and microphone access after each update. Installing from the `.dmg` by hand works too: drag
it to Applications, then right-click it and choose **Open** the first time.

### Windows (x64)

In PowerShell:

```powershell
$dir = "$env:LOCALAPPDATA\Programs\Better Matrix"; $zip = "$env:TEMP\better-matrix.zip"; Invoke-WebRequest https://github.com/m43i/better-matrix/releases/latest/download/better-matrix-windows-x86_64.zip -OutFile $zip; Expand-Archive $zip -DestinationPath $dir -Force; $link = (New-Object -ComObject WScript.Shell).CreateShortcut("$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Better Matrix.lnk"); $link.TargetPath = "$dir\better-matrix.exe"; $link.IconLocation = "$dir\better-matrix.ico"; $link.Save(); $key = "HKCU:\Software\Classes\matrix"; New-Item -Force "$key\shell\open\command" | Out-Null; Set-Item $key "URL:Matrix link"; New-ItemProperty $key "URL Protocol" -Value "" -Force | Out-Null; Set-Item "$key\shell\open\command" "`"$dir\better-matrix.exe`" `"%1`""; & "$dir\better-matrix.exe"
```

This installs for your user only (no administrator rights needed), adds Better Matrix to the Start
menu and opens `matrix:` links with it. Windows may show a SmartScreen warning for a new app: choose **More info**, then **Run anyway**.

### Updating and removing

Run the install line again to update; your accounts, history and keys stay where they are. To remove
the app, uninstall the package (`sudo apt remove better-matrix`, `sudo dnf remove better-matrix`,
`sudo pacman -R better-matrix`, or `sudo pacman -R better-matrix-bin` after a `PKGBUILD` install), or
delete the AppImage, the app in Applications, or the `Better Matrix` folder, Start menu entry and
`matrix:` link handler (`Remove-Item -Recurse HKCU:\Software\Classes\matrix`) on Windows.

## Develop

You need [Bun](https://bun.sh), [Node.js 26](https://nodejs.org) (Oxlint plugins and Jest), and
[rustup](https://rustup.rs); the pinned Rust toolchain installs
itself. On Linux, also clang 21 or newer (libwebrtc's C++ library needs it; set `CC` and `CXX` if
it isn't your default compiler) and the development packages for `libxkbcommon`, `wayland`, `alsa`,
`glib`, `pipewire` and `libclang`; on ARM also `lld` (libwebrtc links with it). On Debian or Ubuntu:

```sh
sudo apt install build-essential pkg-config libclang-dev libglib2.0-dev libasound2-dev libxkbcommon-dev libwayland-dev libpipewire-0.3-dev
wget -qO- https://apt.llvm.org/llvm.sh | sudo bash -s -- 21 && export CC=clang-21 CXX=clang++-21
```

```sh
bun install
bun run dev:desktop    # run the desktop app (dev is an alias)
bun run prebuild       # generate mobile native projects; --platform ios or android to select
bun run dev:ios        # build the Rust bridge and run iOS (Mac with Xcode)
bun run dev:android    # build the Rust bridge and run Android (SDK/NDK required)
bun run test           # all tests
bun run lint           # Oxlint + anti-slop, clippy, typechecks, formatting, third-party list
bun run lint:fix       # apply safe JavaScript/TypeScript lint fixes
bun run format         # Oxfmt for supported files, rustfmt for Rust
bun run build          # debug build; build:release for release
bun run install:linux  # install a release build for this user (Linux)
```

The first build downloads a prebuilt libwebrtc for calls. To see the engine's logs, set `RUST_LOG`
(`RUST_LOG=matrix_core=debug,livekit=info cargo run -p better-matrix-desktop`).

Architecture decisions are in [docs/adr](docs/adr/README.md), terms in
[docs/glossary.md](docs/glossary.md), and working conventions in [AGENTS.md](AGENTS.md).

```
apps/desktop          the desktop app (Rust, Rune UI)
packages/matrix-core  the shared Matrix engine (Rust, matrix-sdk)
packaging/            icons, desktop entry and the per-platform packaging
scripts/              Bun scripts
```

### Contributing

Issues and pull requests are welcome. Pick a template when you open an issue, or start blank. Pull
requests get a label for the release notes (`feature`, `enhancement`, `bug`, …); CI lints and tests
each one.

### Releasing

In **Actions → Release → Run workflow**, enter the version (like `0.2.0`, or `0.2.0-rc.1` for a
pre-release). The workflow sets it in `Cargo.toml` on `main`, tests the commit (unless it already
passed CI there), builds every package, and publishes them under the tag `v0.2.0`, with notes
grouped by the pull requests' labels. A failed run leaves no tag behind, so you can start it again
with the same version. Committing the version to `main` needs Actions to be allowed to push there:
if `main` is protected, set the version in a pull request first and run the workflow with it.

Pushing a tag works too, once `Cargo.toml` carries its version:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

## License

[MIT](LICENSE). The third-party libraries and assets the app ships, with their licenses, are listed
in the app under **Settings → Info**.
