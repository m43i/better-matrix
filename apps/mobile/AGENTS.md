# Better Matrix mobile

Android and iOS app: Expo (SDK 58, React Native 0.88, New Architecture) over the shared Rust
engine. Read [ADR 0010](../../docs/adr/0010-mobile.md) first; the root `AGENTS.md` applies too.

## Expo changes every SDK

Don't trust memory for Expo or React Native APIs. Read the installed type definitions under
`../../node_modules/<package>/build/*.d.ts`, or the versioned docs
(`https://docs.expo.dev/versions/v58.0.0/`). Add dependencies with `bunx expo install <pkg>`
from this directory, never by hand. `android/` and `ios/` are generated (`bunx expo prebuild`);
never edit them, configure `app.json` or a config plugin.

## Layout

- `src/app/`: routes (Expo Router). Every route and its presentation (push, sheet, modal) is
  declared in `src/app/_layout.tsx`; build hrefs with `src/routes.ts`, never by string.
- `src/state/`: the store. `types.ts` (state), `reducer.ts` (`apply`, the only way engine
  events enter), `actions/*.ts` (user actions: functions taking the `Store`), `selectors.ts`,
  `composer.ts`, `react.tsx` (`useStore`, `useAppStore`, `shallow`).
- `src/engine/`: starting the native engine (paths, secrets, OAuth identity), notifications and
  push (`push.ts`, decided in `pushRules.ts`; the iOS extension is `plugins/push`, set up by
  `plugins/withPush.js`). Push's release checklist: `docs/push.md`.
- `src/ui/`: shared components (`components.tsx`, `Avatar`, `icons.tsx`, `media.ts`), and one
  folder per area for that area's components.
- `src/theme/palette.ts`: the desktop's palette roles and type scale.
- `src/i18n/`: `t`, `tp`; desktop catalogs plus `mobile/<area>.<locale>.json`.
- Engine types: `@better-matrix/matrix-native/types` (generated from Rust), the client:
  `@better-matrix/matrix-native/client`.

## Talking to the engine

- Fire and forget: `store.send(command)`; failures become a notice (`{ quiet: true }` for
  background work). Await an answer: `await store.run(command, onEvent)`; it rejects with
  `CommandFailed` (`.cancelled` when cancelled), and `onEvent` hears the request's own events
  (`HomeserverDiscovered`, `OpenUrl`, `PeopleFound`, `DirectChat`, `RoomCreated`,
  `PasswordNeeded`, `ContactCodeSent`).
- Everything else the engine reports goes through `apply` into state; read it with
  `useStore(selector)` (pass `shallow` when the selector builds a new array or object).
- Media: `useMedia(account, media, size)` gives a `file://` URI once fetched. Never fetch in
  render loops by hand.
- Commands and events are JSON shaped as Rust serde makes them: data enums `{ type, data }`,
  unit enums plain strings, snake_case fields. Check `packages/matrix-native/src/generated/types`.
- The engine is `matrix-core`; read a command's Rust doc comment
  (`packages/matrix-core/src/engine.rs`) before using it. The desktop
  (`apps/desktop/src/state`, `apps/desktop/src/ui`) is the reference for behaviour and wording.

## Design

- Slack's mobile app is the model for layout: tabs at the bottom, a native stack into a
  conversation, sheets for actions, long-press for message actions.
- Use the platform's own components: the native stack and its header (title, header buttons,
  search bar), native tabs, form sheets, `Alert.alert` for confirmations, `Switch`, the system
  share sheet, pickers. Draw custom views only where the desktop's design needs them (message
  rows, avatars, the composer).
- Colours come from `usePalette()` roles, type sizes from `font`, icons from `<Icon name>` (add
  the SF Symbol and Material Symbol pair to `icons.tsx` first).

## Conventions

- Every user-facing string is a catalog key: reuse the desktop's key when the text is the same
  (`apps/desktop/i18n/app/en.json`), else add a `mobile.<area>.*` key to your area's
  `en`, `de` and `fr` files with real translations. Element `testID`s are stable IDs, never
  labels.
- State changes are pure functions with tests in a sibling `__tests__/`: reducer cases,
  selectors, actions (with `detachedEngine()` to see the commands sent). Test names read as
  sentences, one behaviour each.
- No `any`, no non-null `!` outside tests; strict TypeScript, `noUncheckedIndexedAccess`.
- Keep screens thin: derive data in selectors, act through actions.

## Commands

From this directory: `bunx tsc --noEmit` (typecheck), `bunx jest` (tests). Both must pass
before work is done.

**Look at what you built** with a demo scene, the counterpart of the desktop's `--scene`: in a
dev build, `adb shell am start -d bettermatrix://demo/<scene> chat.bettermatrix.mobile` switches
the running app to that scene (no account needed), then `adb exec-out screencap -p > shot.png`.
Scenes are in `src/demo/scenes.ts` (`home`, `room`, `thread`, `message-actions`, `room-info`,
`settings-account`, `call-voice`, `ringing`, `verify`, `de`, …); the fixtures are the desktop's
(`src/demo/fixtures.ts`), fed through `apply`, and the demo engine (`src/demo/engine.ts`)
answers commands like a homeserver would. A new screen gets a scene; a test boots every scene.
After a few dozen scene switches in one run the app may stop drawing icons: restart it.

Android: `bun run ubrn:android` in `packages/matrix-native` builds the engine after Rust
changes (slow). Then from `android/`: `JAVA_HOME=<JDK 17> ./gradlew :app:assembleDebug
-PreactNativeArchitectures=x86_64`, install with `adb install -r`, start Metro with
`bunx expo start --dev-client`. iOS needs a Mac.
