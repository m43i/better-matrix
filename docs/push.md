# Push notifications: release checklist

How push reaches the phones, what the owner provides before release, and how to test it end to
end. The design is in [ADR 0010](adr/0010-mobile.md) (Push); the words are in the
[glossary](glossary.md) (Pusher, Push gateway, Notification Service Extension, Push task).

```
homeserver ──(event_id_only)──▶ push gateway ──▶ Expo push service ──▶ FCM / APNs ──▶ phone
                                packages/push-gateway                     │
                                                                          ▼
                                    Android: push task (JS, headless) ─┐  iOS: Notification
                                    iOS:  Notification Service Extension ┘  decrypts with the
                                                                            account's own store
```

Nothing of a message leaves the homeserver: pushers ask for `event_id_only`, so the gateway, Expo,
Google and Apple see the account, room ID, event ID, unread count and timing. The phone fetches and
decrypts the event itself.

## Without any of it

The app builds and runs as before: no gateway URL or no EAS project means push is off (a warning
in development, and Settings → Notifications says it isn't set up); notifications still come from
the running engine. Android builds without `google-services.json`; iOS builds need the App Group
and keychain group on the Apple team (below), or the plugin's `notificationServiceExtension: false`.

## What goes where

| What                                  | Where                                                                                            | Notes                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| EAS project ID                        | `apps/mobile/app.json` → `expo.extra.eas.projectId`                                              | Written by `eas init` (run in `apps/mobile`). The app reads it; never hard-code it.                                                                                                                                                                                                                                                          |
| Push gateway URL                      | `EXPO_PUBLIC_PUSH_GATEWAY`                                                                       | E.g. `https://push.example.org` (the notify path is added). Put it in `apps/mobile/.env.local` for local builds and in the EAS environment (`eas env:create --name EXPO_PUBLIC_PUSH_GATEWAY --visibility plaintext`) for EAS builds. Inlined at build time.                                                                                  |
| Firebase Android config               | `apps/mobile/google-services.json` (gitignored), or the EAS file variable `GOOGLE_SERVICES_JSON` | From the Firebase project's Android app `chat.bettermatrix.mobile`. Expo's push service delivers through FCM, so Android needs it even though the app uses Expo tokens.                                                                                                                                                                      |
| FCM V1 service account key            | EAS: `eas credentials` → Android → _Google Service Account_ → _FCM V1_                           | The JSON key of a service account with the _Firebase Cloud Messaging API Admin_ role. Never in the repo.                                                                                                                                                                                                                                     |
| APNs key                              | EAS: `eas credentials` → iOS → _Push Notifications_                                              | A `.p8` key from the Apple developer account (Keys → Apple Push Notifications service). Never in the repo.                                                                                                                                                                                                                                   |
| App Group                             | Apple developer account: `group.chat.bettermatrix.mobile`                                        | On both App IDs: `chat.bettermatrix.mobile` and `chat.bettermatrix.mobile.BetterMatrixPush`. EAS creates the extension's App ID and profile from `extra.eas.build.experimental.ios.appExtensions` (the plugin writes it); check the group is enabled on both. Another ID: change `appGroup` in the `./plugins/withPush` entry of `app.json`. |
| Keychain sharing                      | nothing to do                                                                                    | The plugin adds `$(AppIdentifierPrefix)chat.bettermatrix.mobile` to both targets' `keychain-access-groups`; it is the app's default group already, so existing sign-ins stay readable.                                                                                                                                                       |
| `aps-environment`                     | `app.json` → `["expo-notifications", { "mode": "production" }]` for store builds                 | The default is `development` (sandbox APNs). Expo's push service picks the APNs environment from the token.                                                                                                                                                                                                                                  |
| Gateway: Expo access token (optional) | Gateway environment `EXPO_ACCESS_TOKEN`                                                          | Only if _Enhanced security for push notifications_ is on in the Expo project.                                                                                                                                                                                                                                                                |

## Deploy the gateway

`packages/push-gateway` is a dependency-free TypeScript handler: `POST /_matrix/push/v1/notify`
in, Expo's push API out, `rejected` answered for tokens Expo reports as `DeviceNotRegistered` (the
homeserver then drops that pusher). `GET /health` answers `{"ok":true}`. It keeps nothing and logs
no payload.

- **Bun:** `cd packages/push-gateway && PORT=5000 bun src/server.ts`, behind an HTTPS reverse
  proxy (Caddy, nginx). `APP_IDS` defaults to `chat.bettermatrix.mobile`.
- **Cloudflare Worker:** `bunx wrangler deploy src/worker.ts --name better-matrix-push
  --compatibility-date 2026-11-01`; set `EXPO_ACCESS_TOKEN` with `wrangler secret put` if needed.

The URL must be `https` and reachable from the homeservers (they post to it; the phones never
do). Test it: `curl -X POST https://push.example.org/_matrix/push/v1/notify -d
'{"notification":{"event_id":"$e","room_id":"!r:example.org","counts":{"unread":1},"devices":[{"app_id":"chat.bettermatrix.mobile","pushkey":"ExponentPushToken[…]","data":{"default_payload":{"account":"@you:example.org"}}}]}}'`
should answer `{"rejected":[]}` and the phone should show a notification for the event (or the
generic text if the IDs are made up).

## Build

From `apps/mobile`:

1. `eas init` (once; writes the project ID), `eas credentials` for Android (FCM V1) and iOS (APNs
   key; let EAS make the extension's profile).
2. iOS only: build the engine for iOS first on a Mac, `bun run ubrn:ios` (the extension links
   `packages/matrix-native/build/MatrixNativeFramework.xcframework`).
3. `eas build --profile development --platform android` (and `ios`), install, sign in. The
   engine's native libraries (`packages/matrix-native/android/src/main/jniLibs`, `…/build`) are
   built locally and gitignored, which EAS's upload follows: build with `eas build --local`, or
   add an `.easignore` that keeps them.

## Test end to end

1. Settings → Notifications: _Notifications on this device_ is on (off: push isn't configured in
   this build). The account's sessions page on another client lists a pusher named after this
   device (`GET /_matrix/client/v3/pushers` shows `app_id` `chat.bettermatrix.mobile`, `format`
   `event_id_only`, `default_payload.account`).
2. From another account, send a message in an encrypted room:
   - app in front: one notification (the engine's), none from the push;
   - app in the background: one notification with the sender and text;
   - app swiped away / phone rebooted (after first unlock): the same, decrypted.
3. Tap it: the room opens at the message.
4. Two accounts signed in, one turned off under _On this device_: only the other notifies.
5. Sign out: the pusher is gone from the server's list.
6. Uninstall the app and send again: the gateway answers `rejected` once Expo reports the token
   gone, and the homeserver removes the pusher.

## Verify on a Mac (written without one)

- `bun run ubrn:ios`, then `expo prebuild --platform ios` and open the workspace: the
  `BetterMatrixPush` target exists, is embedded in the app ("Copy Files", destination _Plug-ins_)
  and builds. Check the xcframework's slice folders are `ios-arm64` and `ios-arm64-simulator`
  (the target's `LIBRARY_SEARCH_PATHS`), and that the extension links (missing frameworks show
  as undefined symbols: add them to `OTHER_LDFLAGS` in `plugins/withPush.js`).
- The extension's memory stays under iOS's ~24 MB limit while it opens a store and decrypts
  (Xcode → Debug → Attach to Process → BetterMatrixPush, send a push). The engine library links
  calls too; dead stripping should drop them.
- Signing with automatic signing: both targets get the App Group and keychain group.
- The app's data moved into the App Group container on first launch (`matrix/` there, none left in
  Documents) and every account restored.
- A push while the app is suspended is decrypted; one while it is in front shows once (the
  engine's); `serviceExtensionTimeWillExpire` shows the generic text when the server is slow.
- An OAuth account's tokens refreshed by the extension are still valid in the app afterwards (the
  cross-process refresh lock; ADR 0010).
- The app doesn't get killed in the background for holding the store's SQLite files in the shared
  container (`0xdead10cc` in the crash log).
