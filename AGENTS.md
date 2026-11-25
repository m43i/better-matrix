# Better Matrix

A Matrix client for Linux, macOS, Windows, Android and iOS built around several accounts signed
in side by side. Goal: every Matrix feature, calls included, in a Slack-style layout.

- **Glossary**: `docs/glossary.md`. Name things with its terms, in code and prose; add a term when
  you introduce a concept.
- **Decisions**: `docs/adr/`, one record per area (index in its `README.md`). Read the area's
  record before changing it, and update it in place when a decision changes; only a new area
  gets a new record.
- **Skills**: `.agents/skills/` is the shared root for every workspace. Load the skill for the
  area you touch: Rune, Rust best practices, Rust async, Turborepo, or Expo overview (which routes
  mobile work to its Expo/EAS skill). Keep skills here so desktop and mobile agents share them.

## Layout

- `apps/desktop` (Cargo `better-matrix-desktop`, binary `better-matrix`): the desktop app, Rune UI.
- `apps/mobile` (`better-matrix-mobile`): the Android and iOS app, Expo / React Native. The
  mobile-specific instructions below apply to work there.
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

Scripts live in `package.json`; Turborepo maps Rust tasks to Cargo. Root Oxlint (including the
vendored anti-slop rules) checks owned JavaScript and TypeScript; Oxfmt formats supported files
alongside rustfmt. Run `bun run lint:fix` for lint autofixes and `bun run format` for formatting.
Run `bun run lint` and `bun run test` before calling work done.

Start from the root with `bun run dev:desktop`, `bun run dev:ios` (Mac) or `bun run dev:android`.
The mobile commands build their Rust bridge before launching Expo. `bun run prebuild` generates
mobile native projects; pass `--platform ios` or `--platform android` to select one.

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
- Text: every user-facing string is a catalog key (`t!`, `tf!` on desktop; `t`, `tp` on mobile)
  present in `en`, `de` and `fr`; element keys use stable IDs, never translated labels.
- Tests: names read as sentences (`a_failed_sign_in_should_show_its_error`), one behaviour each.
  UI behaviour is tested headless with `rune::Runtime`. State tests live in sibling `tests.rs`.

## Dependencies

- Rune is pinned to one `dev` rev in `[workspace.dependencies]`; `rune-notify` and every
  `[patch.crates-io]` entry use the same rev, and the patch table mirrors Rune's own. When
  bumping: update every rev, compare with Rune's root `Cargo.toml`, run `bun run agent-tools`.
- `rust-toolchain.toml` matches Rune's toolchain.
- Linux builds need `libxkbcommon`, `wayland`, `alsa`, `glib`, `pipewire`, `gstreamer` (with
  `gst-plugins-base`, for video) and `webkit2gtk-4.1` (with `gtk3`, `libsoup3`, `javascriptcoregtk`,
  for the widget window, ADR 0012) development packages
  and clang 21+ (libwebrtc's bundled libc++ requires it; `CC`/`CXX` pick it); the first build
  downloads a prebuilt libwebrtc for calls (ADR 0009).

## Mobile (`apps/mobile`)

Read [ADR 0010](docs/adr/0010-mobile.md) before changing the mobile app. Paths below are relative
to the repository root unless a command explicitly starts in `apps/mobile`.

### Expo changes every SDK

Don't trust memory for Expo or React Native APIs. Read the installed type definitions under
`node_modules/<package>/build/*.d.ts`, or the versioned docs
(`https://docs.expo.dev/versions/v58.0.0/`). Add dependencies with `bunx expo install <pkg>`
from `apps/mobile`, never by hand. `apps/mobile/android/` and `apps/mobile/ios/` are generated (`bunx expo prebuild`);
never edit them, configure `apps/mobile/app.json` or a config plugin.

### Layout

- `apps/mobile/src/app/`: routes (Expo Router). Every route and its presentation (push, sheet, modal) is
  declared in `apps/mobile/src/app/_layout.tsx`; build hrefs with `apps/mobile/src/routes.ts`, never by string.
- `apps/mobile/src/state/`: the store. `types.ts` (state), `reducer.ts` (`apply`, the only way engine
  events enter), `actions/*.ts` (user actions: functions taking the `Store`), `selectors.ts`,
  `composer.ts`, `react.tsx` (`useStore`, `useAppStore`, `shallow`).
- `apps/mobile/src/engine/`: starting the native engine (paths, secrets, OAuth identity), notifications and
  push (`push.ts`, decided in `pushRules.ts`; the iOS extension is `apps/mobile/plugins/push`, set up by
  `apps/mobile/plugins/withPush.js`). Push's release checklist: `docs/push.md`.
- `apps/mobile/src/ui/`: shared components (`components.tsx`, `Avatar`, `icons.tsx`, `media.ts`), and one
  folder per area for that area's components.
- `apps/mobile/src/theme/palette.ts`: the desktop's palette roles and type scale.
- `apps/mobile/src/i18n/`: `t`, `tp`; desktop catalogs plus `mobile/<area>.<locale>.json`.
- Engine types: `@better-matrix/matrix-native/types` (generated from Rust), the client:
  `@better-matrix/matrix-native/client`.

### Talking to the engine

- Fire and forget: `store.send(command)`; failures become a notice (`{ quiet: true }` for
  background work). Await an answer: `await store.run(command, onEvent)`; it rejects with
  `CommandFailed` (`.cancelled` when cancelled), and `onEvent` hears the request's own events
  (`HomeserverDiscovered`, `OpenUrl`, `PeopleFound`, `DirectChat`, `RoomCreated`,
  `PasswordNeeded`, `ContactCodeSent`, and a widget run's `WidgetPage`, `WidgetMessage`,
  `WidgetPermissions`, `IntegrationTerms`).
- Everything else the engine reports goes through `apply` into state; read it with
  `useStore(selector)` (pass `shallow` when the selector builds a new array or object).
- Media: `useMedia(account, media, size)` gives a `file://` URI once fetched. Never fetch in
  render loops by hand.
- Commands and events are JSON shaped as Rust serde makes them: data enums `{ type, data }`,
  unit enums plain strings, snake_case fields. Check `packages/matrix-native/src/generated/types`.
- The engine is `matrix-core`; read a command's Rust doc comment
  (`packages/matrix-core/src/engine.rs`) before using it. The desktop
  (`apps/desktop/src/state`, `apps/desktop/src/ui`) is the reference for behaviour and wording.

### Design

- Slack's mobile app is the model for layout: tabs at the bottom, a native stack into a
  conversation, sheets for actions, long-press for message actions.
- Use the platform's own components: the native stack and its header (title, header buttons,
  search bar), native tabs, form sheets, `Alert.alert` for confirmations, `Switch`, the system
  share sheet, pickers. Draw custom views only where the desktop's design needs them (message
  rows, avatars, the composer).
- Colours come from `usePalette()` roles, type sizes from `font`, icons from `<Icon name>` (add
  the SF Symbol and Material Symbol pair to `icons.tsx` first).

### Conventions

- Every user-facing string is a catalog key: reuse the desktop's key when the text is the same
  (`apps/desktop/i18n/app/en.json`), else add a `mobile.<area>.*` key to your area's
  `en`, `de` and `fr` files with real translations. Element `testID`s are stable IDs, never
  labels.
- State changes are pure functions with tests in a sibling `__tests__/`: reducer cases,
  selectors, actions (with `detachedEngine()` to see the commands sent). Test names read as
  sentences, one behaviour each.
- No `any`, no non-null `!` outside tests; strict TypeScript, `noUncheckedIndexedAccess`.
- Keep screens thin: derive data in selectors, act through actions.

### Commands

From `apps/mobile`: `bunx tsc --noEmit` (typecheck), `bunx jest` (tests). Both must pass
before work is done.

**Look at what you built** with a demo scene, the counterpart of the desktop's `--scene`: in a
dev build, `adb shell am start -d bettermatrix://demo/<scene> chat.bettermatrix.mobile` switches
the running app to that scene (no account needed), then `adb exec-out screencap -p > shot.png`.
Scenes are in `apps/mobile/src/demo/scenes.ts` (`home`, `room`, `thread`, `message-actions`, `room-info`,
`settings-account`, `call-voice`, `ringing`, `verify`, `de`, …); the fixtures are the desktop's
(`apps/mobile/src/demo/fixtures.ts`), fed through `apply`, and the demo engine (`apps/mobile/src/demo/engine.ts`)
answers commands like a homeserver would. A new screen gets a scene; a test boots every scene.
After a few dozen scene switches in one run the app may stop drawing icons: restart it.

Android: `bun run ubrn:android` in `packages/matrix-native` builds the engine after Rust
changes (slow). Then from `apps/mobile/android/`: `JAVA_HOME=<JDK 17> ./gradlew :app:assembleDebug
-PreactNativeArchitectures=x86_64`, install with `adb install -r`, start Metro with
`bunx expo start --dev-client`. iOS needs a Mac.
