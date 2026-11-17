# 0001. Stack and tooling

Status: Accepted
Date: 2026-11-14

## Context

- The desktop and mobile apps share Matrix code; the desktop runs on Linux, macOS
  and Windows and should share Rust with the Matrix layer without a JavaScript bridge.
- The goal is every Matrix feature, calls included; writing encryption, sync and storage from
  scratch is years of work and a security risk.
- Development happens mostly on Linux; Windows has no system SQLite.
- The app ships in English, German and French; a missing message key must never reach users.

## Decision

- **Monorepo:**
  - `apps/*` hold apps, `packages/*` shared modules.
  - Bun runs scripts and packages; Turborepo orchestrates tasks. Root `dev:desktop` starts the
    desktop (`dev` remains an alias); `prebuild` forwards to the mobile app's Expo prebuild.
    Root `dev:ios` and `dev:android` build their Rust bridge before running the mobile app on
    that platform. Additional flags go to the platform's Expo command.
  - Rust crates form one Cargo workspace, members listed explicitly so non-Rust packages fit in
    `packages/`.
  - Turborepo discovers it (`experimentalCargoWorkspaces`) as `better-matrix-cargo` and maps
    `build`, `dev`, `test`, `check`, `lint`, `format` to Cargo.
- **JavaScript/TypeScript quality:** root `.oxlintrc.json` enables Oxlint's correctness rules,
  React checks, and every generic anti-slop rule from `tools/oxlint/anti-slop` (including native
  `oxc/no-accumulating-spread`). Oxlint and `@oxlint/plugins` are pinned to the same version.
  The vendored plugin's `UPSTREAM.md` records its source commit and pristine hashes; its nested
  Stylistic license and provenance stay with it. Effect rules remain opt-in until a workspace
  directly uses Effect. `bun run lint:js` checks owned source; `bun run lint:fix` applies safe
  autofixes. Warnings fail the check.
- **Formatting:** root `.oxfmtrc.json` configures Oxfmt with single quotes and a 100-column
  width. `bun run format` and `bun run format:check` cover Oxfmt and rustfmt; the existing CI
  `bun run lint` invokes both linters, typechecks, the format check, and the third-party check.
  Both Oxc tools exclude generated bindings, native prebuild output, installed agent assets,
  and vendored lint rules. Agent instructions are consolidated in root `AGENTS.md`, and all
  workspace skills live under root `.agents/skills/`.
- **Rune for the desktop UI:**
  - [Rune](https://github.com/anomalyco/rune), a GPU-rendered Rust UI framework; views are
    plain functions of state.
  - One `dev` rev is pinned for `rune`, `rune-i18n`, `rune-kit` and `rune-notify`; `[patch.crates-io]`
    mirrors Rune's own patch table at that rev.
  - Agents read Rune's docs synced at that rev (`bun run agent-tools`), never memory.
  - What Rune lacks goes behind the app's **shell** ([0002](0002-engine-and-desktop-state.md)).
- **matrix-rust-sdk:**
  - `matrix-sdk` (behind Element X) provides the SQLite stores, rustls TLS, OAuth and
    notification settings; `matrix-sdk-ui` provides timelines ([0006](0006-timelines.md)).
  - SDK types stay inside `matrix-core` ([0002](0002-engine-and-desktop-state.md)).
  - Its futures nest deeply, so `matrix-core` sets `recursion_limit = "256"`.
- **Builds:**
  - `bundled-sqlite` on every platform: all builds link the same SQLite.
  - Release builds use thin LTO and one codegen unit: a faster engine and UI for the
    desktop and the phones, for slower release builds.
  - The desktop's `frame-profile` feature turns on Rune's frame log (`RUNE_FRAME_LOG`): each
    drawn frame, its cost and what asked for it.
  - `bun run build:windows` builds natively on Windows and cross-compiles elsewhere with
    `cargo-xwin` (clang-cl, lld-link).
  - Windows release builds use the GUI subsystem: no console window.
- **CI and releases** (`.github/`), kept to as little compute as gives confidence:
  - Hosted runners are pinned to current stable images: Ubuntu 26.04 for CI and release
    orchestration, macOS 26 arm64 and Windows Server 2025 for their release builds. Linux
    release builds keep Ubuntu 22.04 deliberately, preserving the glibc 2.35 baseline below.
  - Actions use current stable major versions: checkout v7, upload-artifact v7,
    download-artifact v8, rust-cache v2, setup-bun v2 and setup-node v6. Their own manifests
    select Node 24; the Node 26 CI installs doesn't change that action runtime. Repository
    scripts run with the Bun version in `package.json`; Oxlint's TypeScript plugins and Jest
    (the mobile app's and matrix-native's tests, under `jest-expo`) run on Node 26 in CI.
  - CI (`ci.yml`) lints and tests in one Linux job on every pull request and every commit to
    `main`, the mobile app's TypeScript (typecheck and Jest, [0010](0010-mobile.md)) included;
    releases build the desktop app only, until the phones ship. A newer push cancels a pull request's older run. Only `main` writes the Rust cache;
    pull requests read it. Linux builds use clang 21 from LLVM's packages (libwebrtc's libc++
    needs it), passed to Cargo through Turbo (`CC`, `CXX`, `CARGO_*` in `turbo.json`).
  - A release (`release.yml`) starts by hand with a version (checked as Cargo reads versions,
    set in `Cargo.toml` and `Cargo.lock` on `main`) or from a pushed `v*` tag. Preparation reads
    every explicitly listed workspace member's manifest and updates each inherited version in
    the lockfile; members with their own versions and registry dependencies keep theirs. This
    uses Python's TOML reader so preparation needs no Rust toolchain or dependency downloads.
    When the version changes, the release job uses `GITHUB_TOKEN` with `contents: write` and `pull-requests: write`
    to push a temporary branch, open a version-only PR labeled `skip-changelog`, and squash-merge
    it with the expected head SHA. This obeys the PR rule on `main` without a bypass token or
    manual version bump. Repository Actions settings must allow creating pull requests
    (`can_approve_pull_request_reviews: true`); default token permissions stay read-only. The
    job stops if reviews or required checks block the merge. It releases the confirmed merge
    commit. Token-created PR workflows need manual approval and token-created merges do not
    trigger CI, so the release verifies the commit with its own test job as needed. If
    `main` advanced and changed the prepared tree, earlier CI no longer counts. Preparation
    tests use a local remote that rejects direct pushes to `main`, run in CI before Cargo.
    The tag is made only when the release is published, so a failed run leaves none behind. It tests the commit
    unless it already passed CI on `main` (a version-only change counts as the same commit),
    then builds macOS arm64 (`.dmg`, signed ad hoc), Windows x86_64 (`.zip`) and Linux x86_64
    and aarch64 on Ubuntu 22.04 (glibc 2.35, which the packages require): `.deb`, `.rpm` and an
    Arch `.pkg.tar.xz` from one nfpm description, an AppImage (linuxdeploy) and a tarball the
    release's `PKGBUILD` installs. Asset names carry no version, so `releases/latest/download/…`
    links stay stable. Release builds keep no Rust cache: a tag's cache can't be restored by any
    other run.
  - libwebrtc's link settings (`.cargo/config.toml`, as LiveKit's own): `-ObjC` on macOS (or it
    aborts at startup), the static C runtime on Windows; on ARM Linux, lld (with clang driving the
    link in CI). Cargo doesn't pass a dependency's link arguments on, so the app sets them.
  - Release notes come from the merged pull requests, grouped by label (`.github/release.yml`;
    the labels are in `.github/labels.yml`). The packaging lives in `packaging/`, runnable
    locally.
- **Desktop self-updates** (macOS arm64 and Windows x86_64 release builds):
  - A shell worker checks GitHub's latest stable release at startup and hourly. Offline checks
    are quiet; prereleases, equal/older versions and releases without a matching archive and
    SHA-256 digest are ignored. SemVer precedence decides what is newer.
  - Discovery only offers an update. Clicking the rail's update icon downloads the immutable
    asset ID over HTTPS and verifies GitHub's SHA-256 digest before extracting it. This trusts
    the repository's release permissions and GitHub HTTPS; it is not a separate signing key.
  - Stage beside the install so permissions and disk failures are reported before quitting.
    macOS publishes a ditto ZIP of the complete ad-hoc-signed bundle as well as the DMG, and
    verifies its code signature. Windows extracts only `better-matrix.exe` from the existing ZIP.
  - A native script helper acknowledges startup, waits for process exit (the engine shuts
    down and releases the single-instance lock normally), moves the previous install aside,
    replaces it and relaunches. It restores the backup if replacement/relaunch fails and
    reports the error on the next launch. Paths are arguments, never generated script source.
  - Development/headless builds and Linux do not self-update. Linux remains package-managed.
    The install directory must be writable; there is no elevation flow. macOS must run from an
    installed `.app`, not a standalone executable or read-only disk image. Ad-hoc signing's
    existing macOS permission prompts still apply.
- **`matrix:` links and one process** (desktop):
  - The installs claim the `matrix` scheme: the Linux desktop entry (`packaging/linux`, and
    `bun run install:linux`) has `MimeType=x-scheme-handler/matrix;` and `Exec=… %u`; the macOS
    bundle's `Info.plist` has `CFBundleURLTypes`; the Windows install line in the README writes
    `HKCU\Software\Classes\matrix` (`URL Protocol`, `shell\open\command` → `"…exe" "%1"`).
    `https://matrix.to` isn't claimed: no desktop OS hands a web domain to an app without the
    domain's consent, so those links open the browser, and matrix.to's page offers clients.
  - The app runs as one process per user (`rune::single_instance`, before the engine starts, so
    two never share the stores; `rune::release_instance` once it stops). Rune hands a later
    launch's `scheme://` arguments to `App::on_open_url` (and macOS every link), but not an
    opaque `matrix:r/…` URI, after which it only reopens the app. So `platform::launch` writes
    the permalinks Rune won't pass into a private handoff folder (`<cache>/links`, one file per
    link, written whole then renamed) before asking for the instance; the running process takes
    them in `App::on_reopen` (on a worker) and lets Rune bring its window forward, and a launch
    that becomes the running process takes them itself. Links older than a minute are dropped.
    Every link reaches `State::open_permalink`, which waits for the first room list when no
    account is ready.
- **Translations:**
  - Each **catalog** is flat JSON, `apps/desktop/i18n/app/<locale>.json`, listed in
    `i18n/locales.json`. English is the source and the fallback.
  - `build.rs` compiles catalogs in (`rune_i18n::codegen`); `t!`, `tf!` and `tp!` resolve keys
    at compile time, so a typo fails the build.
  - Tests fail when German or French lacks an English key.
  - `Language` is `System` (negotiated from the OS) or a catalog locale, kept in `rune::prefs`:
    read before the first frame, saved off the UI thread.
  - Element keys use stable IDs (`Format::id`, `mode_id`), never translated labels.
  - Numbers go through the locale (`I18N.decimal`).
  - **Headless** runs and tests use English (`Settings::fixed_english`).
- **Fonts:** `families` are `("Inter", "ui-monospace")`, so `mono()` text (code, recovery keys,
  the **backdrop**) uses the system's monospace font; Rune's default put Inter in both slots.

## Consequences

- One command set for every language, cached by Turborepo; its Cargo support is experimental,
  so an upgrade can change task mapping.
- One language end to end; views test **headless** (`rune::Runtime`, `--snap`, `--tree`).
- Rune is pre-release: a bump updates every rev, the patch table and `bun run agent-tools`.
- Encryption and storage come from an audited codebase; an SDK upgrade touches one crate.
- Windows builds are checked anywhere; running them needs Windows.
- A new language is one JSON file and one `locales.json` line; new UI text is a key in all
  three catalogs.
- SDK error messages show in English inside translated sentences.
- Monospace text, the backdrop included, depends on the system's font.
- A second desktop launch no longer runs beside the first; links are verified only by the unit
  tests of the handoff folder and the state: the scheme registration on each OS and Rune's
  instance handoff were not exercised here (Linux needs `update-desktop-database`; the AppImage
  installs no desktop entry, so it claims nothing).
