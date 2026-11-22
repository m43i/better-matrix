# 0003. Accounts and sign-in

Status: Accepted
Date: 2026-11-14

## Context

Several accounts run side by side, and users expect to stay signed in across restarts. Tokens
and store keys must not sit in plain files when the OS offers a keychain. Matrix is moving to
OAuth 2.0 (MSC3861), but many homeservers offer only passwords. OAuth servers rotate refresh
tokens, so a refresh that goes unsaved ends the session at the next launch. A server can also
end a session at any time, hard or softly.

## Decision

- **One store directory per login:**
  - Each login gets a random store directory under `<data>/accounts/`, its SQLite databases
    encrypted with a random 32-byte passphrase. A new sign-in is a new device, unless it resumes
    a suspended account.
  - `<data>/accounts.json` lists the accounts (`StoredAccount`: ID, homeserver URL, store
    directory, `signed_out`). No secrets. A store directory is read as `<data>/accounts/<name>`
    wherever the list says it was, so the data directory may move (an iOS container changes
    path, and the data moves into the App Group for push, ADR 0010).
  - Saving an account over one with a different store deletes the old store, after the new
    secrets and list are written. If the list can't be written, the previous secrets go back,
    so the store it still lists stays readable. Signing out deletes the account's
    secrets, entry and store.
  - The engine opens each of a store's databases with at most 8 connections
    (`STORE_CONNECTIONS`) instead of the SDK's four per physical core, each with the SDK's
    default page cache. A push opens them with the SDK's low-memory settings
    (`SqliteStoreConfig::with_low_memory_config`): it runs in a phone's memory-capped
    notification process.
- **Secrets:**
  - Tokens and the passphrase go to the app's `SecretStore`, one entry per account
    (`StoredSecrets`). Calls may block, so they run on Tokio's blocking pool.
  - The desktop uses the OS keychain (macOS Keychain, Windows Credential Manager, Secret Service),
    probed on first use, and falls back to `FileSecrets` under `<data>/secrets`, saying so in its
    log.
  - `Sessions` holds one lock across every read-modify-write of the list or a secret, so a
    sign-out and a token refresh can't undo each other. The list is written to a temp file and
    renamed.
  - A file secret is written to a private (`0600`) temp file, then renamed. Its file name keeps
    letters and digits and writes every other byte as `_` and two hex digits, so no two account
    IDs share a file. Names from earlier versions are still read.
- **Sign-in in two steps** (`SignInStep`):
  - **Homeserver:** a server name, URL or Matrix ID (which also fills in the username).
    `Command::Discover` tries `.well-known/matrix/client`, then the input as a URL, and reports a
    `Homeserver` with its `LoginMethods`.
  - **Sign in**, chosen by the server: OAuth opens the browser at once; password only shows the
    fields (`Command::Login`); both open the browser, with "Use a password instead".
  - The second step shows its server above the form. There is no step indicator.
  - Everything after discovery uses the resolved URL; restored sessions use the saved one.
- **Creating an account** (`registration`): discovery reports `LoginMethods::create_account`,
  from the OAuth metadata's `prompt_values_supported` (`create`) or, on a password server, from
  whether `POST /register` answers with interactive-auth flows.
  - An OAuth server creates accounts on its own page: `LoginOAuth { create: true }` asks for
    `prompt=create`, then signs in as usual.
  - A password server goes through `Command::Register`: the engine picks the flow with the
    fewest browser stages, completes `m.login.dummy` itself, asks the app for a registration
    token, agreement to the server's policies or an email address (`Event::Registration` with a
    `RegistrationStep`; the engine then asks the server to send a link and waits for the app to
    say it was opened, asking again while the server hasn't seen it), and sends every other
    stage (captchas) to the server's fallback page in the browser, continuing when the app
    answers. Synapse has no fallback page for email, hence the app's own step. Flows with a
    phone number stage aren't taken. Answers travel as
    `Command::Answer { request, answer }`, the general path for a running command's
    questions. The new account signs in like a password sign-in.
- **Signing in with a QR code** (MSC4108, `qr_login`): `LoginMethods::qr` is an OAuth server
  with a device authorization endpoint.
  - One flow covers the four directions: a new device shows a code or scans a signed-in
    device's (`Command::SignInWithQr`), and a signed-in, verified session shows one or scans the
    new device's (`Command::LinkDevice`). Both report `QrLoginStep`s in `Event::QrLogin`:
    `Show(QrImage)`, `EnterCheckCode` (answered with `Answer::CheckCode`), `CheckCode(n)`,
    `Approve { url }` (also `Event::OpenUrl`), `Waiting { code }`, `Syncing`, `Done`.
  - The signed-in side confirms at once once the account page is open: the new device then
    waits for the approval, and the signed-in one for the new device (up to a minute after
    the new device signs in).
  - The new device reports `Done { account }` only once its session is saved, and the apps
    leave the flow when the command finishes; once saving started it finishes even when the
    sign-in is cancelled (the account then comes back at the next start).
  - A QR sign-in as an account this device already has, signed in or suspended, signs the new
    device out again (`Error::AlreadySignedIn`): saving it would replace that account's store
    (a running one's, or a suspended one's history and keys). A suspended account signs in
    again from its entry, which keeps its store.
- **Which account a sign-in added:** every sign-in command (`Login`, `LoginOAuth`, `Register`,
  `SignInWithQr`) answers with `Event::SignedIn { request, account }` after `AccountAdded`, so
  the apps select exactly that account even while saved accounts are still being restored.
  - A sign-in or registration that fails or is cancelled removes its new store directory
    (`NewStore`).
  - The new device registers as a native client with the device-code grant and starts
    verified, with the account's secrets. A code from the wrong side is refused
    (`Error::WrongSignInCode`).
  - The desktop shows codes; the phones also scan them (ADR 0004).
- **Where the new ways in sit:** the sign-in step offers "Sign in with a QR code" (where
  `LoginMethods::qr`) and "Create an account" (where `create_account`) as links under the
  form. On an OAuth server, where the browser has already opened, picking one cancels that
  browser sign-in. Signing in again as a suspended account offers neither: both make a new
  store, which would replace the suspended account's. Phones also offer scanning a code before any server is chosen, as the code
  names the server. Linking a new device is on the Sessions page of a verified OAuth session.
- **OAuth** (`Command::LoginOAuth`): the app registers dynamically as a native client, listens
  on a loopback redirect and asks the UI to open the URL (`Event::OpenUrl`). It waits up to ten
  minutes (`BROWSER_LOGIN_TIMEOUT`) and can be cancelled. It introduces itself with
  `ClientIdentity` (name and public `https` URI, `CLIENT_URI`).
- **No refresh goes unsaved:**
  - `Account` subscribes to `SessionChange` as soon as its client exists: in `restore` before
    `restore_session`, in `signed_in` right after sign-in.
  - `watch_session` gets that subscription before the first request and saves tokens on each
    refresh. A lagged channel counts as a refresh.
  - Every client also saves a refresh synchronously, inside it (the SDK's session callbacks),
    and OAuth refreshes take a cross-process lock: a push handled in another process (ADR 0010)
    may refresh the same session, and a client that finds the tokens refreshed elsewhere reads
    them back from the secret store instead of spending a used refresh token.
- **Restoring at launch:**
  - The engine sends `Event::Restoring`, then restores every saved account that isn't
    suspended. Restored accounts get ready like fresh ones (`Account::ready`).
  - The desktop shows "Signing you back in…" (`Screen::Restoring`) until each account arrives
    (`AccountAdded`) or fails (`AccountError`).
  - Device settings remember the last used account. The desktop waits for that account to
    restore, regardless of completion order; if it is missing or cannot restore, another
    restored account opens. Choosing an account explicitly takes precedence over restoration.
  - Dev builds optimise dependencies (`opt-level = 2`), since unoptimised key derivation made
    restoring take seconds.
  - Data lives in the app's data folder and the keychain, which no build or install touches, so
    dev runs, release builds and the installed app (`bun run install:linux`) share accounts.
- **Adding another account:** the desktop keeps the current conversation behind a sign-in
  modal taking 90% of the window in each direction, like Settings. The existing animated
  backdrop fills the modal behind the sign-in card. Closing cancels discovery
  or sign-in still under way. On phones, sign-in opens a full-screen page with an X at the
  top left to cancel and return. Successful sign-in selects the added account and closes the
  modal or page. The first account still uses the full sign-in screen.
- **Ended session:**
  - `account::session_ended` recognises an unknown access token, or a refresh token missing or
    rejected with `invalid_grant`; the sync loop then stops. Only `watch_session` sends
    `Event::SessionEnded`, from the SDK's `UnknownToken` broadcast, which every ending sends
    and which says whether it was soft: a rejected refresh hides that from the sync's error.
  - Other sync failures retry after 2 s, doubling to a minute. A sync that ran over a minute
    starts the waits over.
  - The desktop sends `Command::Logout`, which forgets the account even when the server refuses
    the logout. A dialog offers Later or Sign in again.
  - Sign in again starts discovery with the user ID, so an OAuth account goes straight to the
    browser.
- **Soft logout keeps the data:**
  - `SessionEnded` carries `kept` for `UnknownToken { soft_logout: true }`. The desktop then
    sends `Command::Suspend`: the engine stops the account and marks it `signed_out`, keeping its
    store and secrets.
  - A suspended account isn't restored at launch; the engine reports it as ended and kept.
  - The desktop lists suspended accounts (`State::suspended`) in the account menu and on the
    sign-in card, with Sign in again and Remove, so "Later" in the dialog loses nothing.
  - Signing in as a suspended account's user ID, from the list, the dialog or typed, passes
    `resume` (`LoginRequest::resume`, `LoginOAuth::resume`), reusing the store, passphrase and
    device ID, so history, keys and verification carry on.
  - Remove (or "Remove from this device") sends `Logout`, which only forgets the stopped
    account.
- **Backdrop** on the sign-in and restoring screens (`state::backdrop`, `ui::backdrop`):
  - An ASCII glow over a horizon after React Bits' Gradient ASCII, rising in as an intro.
  - Built from Rune's parts. A custom shader needs `Compositor`, which takes the whole frame and
    a GPU in every headless run.
  - The light is sampled once per character cell per tick. The glow is a `Pixels` image, one
    pixel per cell, upscaled smoothly. The characters (` =+xX0S8#@`) are one monospace text
    element per row, coloured by spans, and scramble under the pointer.
  - 12 ticks a second (`TICK`) on `poll`'s clock, virtual when headless, only while the screen
    shows in a focused window, including inside the add-account modal. Rows are memoized per tick.
  - A screen shown again rises in anew. Under reduced motion it holds still once risen.
  - Dark uses deep and pale blues with near white; light runs the same blues as ink.

## Consequences

- Passwords for OAuth servers never pass through the app.
- Registration stages the app can't take run on the server's fallback pages, so a server
  without them (rare) can't register in the app. Email and phone stages there work too.
- Signing in with a QR code needs both a rendezvous service and the device authorization grant
  on the server (MAS); elsewhere the option doesn't show.
- The file fallback is weaker than a keychain: anything running as the user can read it.
- `CLIENT_URI` is a placeholder; set the real site before release, since servers show it.
- Each browser sign-in registers a new client, as the loopback port changes.
- `resume` applies only when the sign-in is for that account: the same homeserver, and on a
  password sign-in the same user. Otherwise the sign-in gets a new store, and the suspended
  one stays as it was. Changing server clears `resume`.
- A password sign-in as a suspended account resumes it however it started: the engine looks for
  a suspended account on that server with that username, and the username decides over
  `resume`. A browser sign-in started from the
  server's name can't: the user is known only when the browser returns, so it gets a new store
  and replaces the suspended one. Signing in from the list, or by the user ID, resumes it.
- The backdrop rebuilds the view and lays out about 40 rows 12 times a second while it shows.
  Its look depends on the system's monospace font.
