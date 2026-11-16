# Better Matrix

A Matrix client for Linux, macOS and Windows (mobile later) built around several accounts signed
in side by side. Goal: every Matrix feature, calls included, in a Slack-style layout.

- **Glossary**: `docs/glossary.md`. Name things with its terms, in code and prose; add a term when
  you introduce a concept.
- **Decisions**: `docs/adr/`, one record per area (index in its `README.md`). Read the area's
  record before changing it, and update it in place when a decision changes; only a new area
  gets a new record.
- **Skills**: `.agents/skills/` (Rune, Rust best practices, Rust async, Turborepo). Load the one for
  the area you touch.

## Layout

- `apps/desktop` (Cargo `better-matrix-desktop`, binary `better-matrix`): the desktop app, Rune UI.
- `apps/mobile` (`better-matrix-mobile`): the Android and iOS app, Expo / React Native. Read its
  own `AGENTS.md` before working there.
- `packages/matrix-core` (`matrix-core`): the shared Matrix engine. No UI code.
- `packages/matrix-ffi` (`matrix-ffi`): the engine behind UniFFI for the phones, JSON in and out.
- `packages/matrix-native` (`@better-matrix/matrix-native`): the React Native turbo module
  generated from `matrix-ffi`, the typed client and the generated engine types (ADR 0010).
- `docs/`: glossary, ADRs, README images. `scripts/`: Bun scripts.
- `packaging/` and `.github/`: icons, desktop entry and per-platform packaging scripts; CI and
  the release workflow (ADR 0001).
- `.agent-tools/` (gitignored): Rune's docs and examples at the pinned rev; `bun run agent-tools`
  refreshes it.

## Commands

Scripts live in `package.json`; Turborepo maps them to Cargo. Run `bun run lint` and
`bun run test` before calling work done.

- Headless UI check: `cargo run -p better-matrix-desktop -- --snap shot.png --scene <scene>`, then
  look at the PNG. Scenes are listed in `apps/desktop/src/main.rs`; no scene shows sign-in.
  `--tree` dumps the element tree.
- Live network tests are `#[ignore]`d: `cargo test -p matrix-core -- --ignored`.

## Build general

Build general internal APIs and put features on top of them. A feature arrives as a variant of
an existing vocabulary (`Command`, `Event`, `Outgoing`, `MessageContent`, `Format`, `Popover`,
`Purpose`) flowing through the existing paths (engine task, `State::apply`, `dispatch`, shared
components), with tests at each layer. When a feature needs a path that doesn't exist yet, build
the path generally and record it in its area's ADR.

## Conventions

- Rust: `thiserror` in libraries; `unwrap`/`expect` only in tests (clippy enforces it);
  `#[expect(lint, reason)]` over `#[allow]`.
- Async: channels over shared state; blocking calls (secret stores, file I/O) go to
  `spawn_blocking`.
- Rune: stable keys on interactive elements; modes as explicit state machines; no file I/O on the
  UI thread.
- Text: every user-facing string is a catalog key (`t!`, `tf!`) present in `en`, `de` and `fr`;
  element keys use stable IDs, never translated labels.
- Tests: names read as sentences (`a_failed_sign_in_should_show_its_error`), one behaviour each.
  UI behaviour is tested headless with `rune::Runtime`. State tests live in sibling `tests.rs`.

## Dependencies

- Rune is pinned to one `dev` rev in `[workspace.dependencies]`; `rune-notify` and every
  `[patch.crates-io]` entry use the same rev, and the patch table mirrors Rune's own. When
  bumping: update every rev, compare with Rune's root `Cargo.toml`, run `bun run agent-tools`.
- `rust-toolchain.toml` matches Rune's toolchain.
- Linux builds need `libxkbcommon`, `wayland`, `alsa`, `glib` and `pipewire` development packages
  and clang 21+ (libwebrtc's bundled libc++ requires it; `CC`/`CXX` pick it); the first build
  downloads a prebuilt libwebrtc for calls (ADR 0009).
