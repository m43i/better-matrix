# 0004. Encryption

Status: Accepted
Date: 2026-11-14

## Context

A new session can't read earlier encrypted messages until it has their room keys, from
verification by another session or from the key backup unlocked with the recovery key. People
also manage their recovery key and sessions, and need an identity reset when the recovery key
and every verified session are lost. They verify other people too, by emoji or QR code, move
message keys between clients as a file, and expect a session signed in later to read what
arrived while none was. The server guards some steps: OAuth accounts approve on the
account page, password accounts enter the password. matrix-sdk's default backup strategy never
fetches keys, so a newly verified session would stay unable to read history.

## Decision

- **matrix-sdk does the cryptography:**
  - `matrix_core::security` wraps its recovery, verification and device APIs as `Account`
    methods, reached only through commands (`Recover`, `SetUpRecovery`, `ResetRecoveryKey`,
    `VerifyThisSession`, `VerifyUser`, `AcceptVerification`, `CompareEmoji`,
    `ScanVerificationCode`, `ConfirmVerification`, `CancelVerification`, `LoadIdentity`,
    `ExportRoomKeys`, `ImportRoomKeys`, `SetOfflineDevice`, `ListSessions`, `RenameSession`,
    `EndSession`, `ResetIdentity`, `FetchRoomKeys`).
  - matrix-sdk's `qrcode` feature is on: QR verification and signing in with a QR code
    (ADR 0003).
  - Every client is built with `auto_enable_cross_signing`, `auto_enable_backups` and
    `BackupDownloadStrategy::AfterDecryptionFailure`, the settings Element X uses.
- **Security state is pushed:** the engine watches the SDK's recovery and verification state and
  the offline device, and sends `Event::SecurityChanged` with
  `Security { session_verified, recovery, offline_device }` at start and on every change. The
  banner and the Account page read it; nothing polls.
- **Verification is one flow model:**
  - Outgoing and incoming requests are the same flow, keyed by `flow_id` in the engine's
    `Flows`, whether with the user's other session (to-device) or with another person (in the
    direct chat both have joined). matrix-sdk drops an in-room request from devices the
    receiving client doesn't know yet, as in a chat the request itself would start, and offers
    no way to take it again, so `VerifyUser` refuses without a joined direct chat
    (`Error::NoDirectChat`). A room
    event handler picks up requests addressed to the user in rooms. Handlers only hand a
    request to `Flows::follow` and return (a handler that waited would hold up the sync bringing
    the flow's next steps); the follower tasks and the handlers end with the account.
  - `VerifyThisSession` and `VerifyUser` answer with `Event::VerificationRequested { flow_id }`
    before any step of that flow, and the apps follow exactly that flow: another person's
    request arriving meanwhile can't take over the dialog.
  - Each flow's task follows the request and its current method together: showing a QR code
    already moves the request on to the QR method, and choosing emoji or scanning the other
    side's code replaces the method.
  - The engine reports `VerificationStep`s (`Incoming` with the person when it isn't the user,
    `Waiting`, `Ready`, `Scanned`, `Compare(emoji)`, `Confirmed`, `Done`, `Cancelled`), shown
    in the Verification modal on the desktop and the verify sheet on phones.
  - Methods: emoji (SAS), showing a QR code and confirming the other side scanned it, and
    scanning the other side's code where the app has a camera (`EngineConfig::scans_qr`: the
    phones, not the desktop). Once both sides are ready the engine draws this side's code
    (`Ready { qr, scan }`) unless emoji are all both share, in which case the side that asked
    starts them as before. `CompareEmoji` switches to emoji; `ScanVerificationCode` takes the
    scanned bytes; `ConfirmVerification` answers both the emoji and "they scanned it".
  - Once a verification with someone else ends, `Event::Identity` reports their identity
    (`Identity { verified, changed }`), which the profile and member views also ask for with
    `LoadIdentity`.
- **QR codes:** `qr::QrImage` is the code as modules for the apps to draw (the desktop as a
  pixel image, the phones as views). Matrix's codes carry binary data, which barcode scanners
  (expo-camera on the phones) hand over as lossy text, so the phones send a camera picture and
  `Command::ReadQrCode` reads it in Rust (`rqrr`), answering `Event::QrCodeRead` with the raw
  bytes.
- **Key export and import:** `ExportRoomKeys` writes every message key this session holds to a
  file the user chose, encrypted with a passphrase (the standard Megolm export format, which
  other clients read); `ImportRoomKeys` reads one and answers `Event::KeysImported` with how many
  were new. The SDK decrypts affected messages again by itself.
- **Offline device** (dehydrated device, MSC3814; `offline_device`):
  - The server keeps a device for the account that senders encrypt for, so a session signed in
    later reads what arrived while none was. Its key lives in secret storage, so turning it on
    (`SetOfflineDevice`) takes the recovery key.
  - A device on the server is only replaced after reading what it caught (`rehydrate`, whose
    failure stops the replacement), so a failed read never loses messages. The engine uses
    matrix-sdk's `rehydrate` and `create` itself rather than its `start`, which goes on after a
    failed read and prefers a key cached locally over the one in secret storage.
  - Whether the account wants one is the account data event `chat.bettermatrix.offline_device`
    (`{ enabled }`), so turning it off on one session keeps another's recovery from bringing it
    back. A session that recovers with the recovery key takes it over by itself unless the
    account turned it off.
  - Secret storage can't be opened again without the recovery key, so this session keeps the
    key in the app's secret store (`offline-device:<account>`, beside the session; the SQLite
    state store doesn't encrypt values), and when it last made the device in its state store
    (same name as the account data), and replaces the device every week from there, across
    restarts. Signing out forgets the key. Turning it off deletes the
    device and that copy. Turning it on or off, taking it over and replacing it run one at a
    time (`Account::offline_changes`).
  - Setting up or resetting recovery starts secret storage over with the account's own secrets
    only, so the session puts the offline device's key back in under the new recovery key,
    after checking it still opens the device on the server (by replacing that device with it);
    it reports off until the key is stored.
  - Taking it over and each replacement first ask the server (not this device's copy) whether
    the account still wants it; a session finding it turned off deletes it (again) and stops
    running it. A replacement made while another session turned it off is taken back. A session
    that runs none waits until it is turned on, here or by recovering, then keeps it fresh.
  - Every step runs in an order a failure can be retried from: turning off deletes the device
    before saying so for the account; turning on makes a fresh device before putting its key in
    secret storage, and keeps the key here before each upload (an upload the server took
    whose answer was lost is still read on the next try); it reports on only once the key is in
    secret storage, so a failed write leaves "Turn on" to finish it. A key that doesn't open the
    device on the server (`StaleOfflineKey`: it was
    replaced under a recovery key reset elsewhere) is dropped: a session running it stops,
    taking over finds nothing, and turning it on makes a fresh one.
  - `Security::offline_device` is `Unsupported` where the server has no endpoint for it, else
    `On` when the account has one (this session runs it, or the account setting says another
    does and finished setting it up: `{ enabled, ready }`, `ready` once a device is up and its
    key in secret storage), so any session can turn it off. What every session shows is that
    setting, followed as syncs bring it; turned off elsewhere, it wakes the session running the
    device, which deletes it. Only a session that just stored the key under the current
    recovery key marks it ready (turning it on, or carrying it over after a recovery key reset;
    setting up or resetting recovery marks it not ready before the key changes); `ready` changes never touch `enabled`. Not ready, "Turn
    on" finishes it: from the session holding the key it stores that key, elsewhere it makes a
    fresh device. Only a session holding the key replaces it; another
    takes it over by turning it on, or by recovering.
- **A recovery key is shown once:** setting up or resetting recovery returns it in
  `Event::RecoveryKey`, shown with Copy and never stored. Resetting it needs a verified session.
- **Ending a session follows the sign-in method:** on OAuth accounts `EndSession` returns the
  account page's device-delete URL for the shell to open; on password accounts the engine
  deletes the device with interactive auth.
- **Guarded actions share one password path:**
  - Any command can answer `Event::PasswordNeeded { request }`, then fail with
    `Error::PasswordRequired`.
  - The desktop names the action as a `Guarded` variant (`EndSession`, `ResetIdentity`,
    `AddContact`), opens the shared `Modal::Password`, and drops the pending purpose so the
    failure isn't shown. Confirming resends the action with the password.
  - A new guarded action is one variant with its command and dialog wording.
- **Identity reset:**
  - `Command::ResetIdentity` wraps the SDK's `recovery().reset_identity()`: it deletes the key
    backup and recovery, makes new cross-signing keys with this session as the first verified
    one, and turns key backup back on.
  - OAuth accounts get `Event::OpenUrl` with the approval page; the engine waits up to the SDK's
    two minutes, then fails with `Error::ApprovalTimedOut`. Password accounts go through the
    guarded-action path.
  - One modal runs `ResetStep`: `Warning` (what stays, what is lost; nothing done yet),
    `Resetting` (approval page, Open again, Cancel), `Done` (offers Set up recovery).
  - A failure returns to `Warning` with its error. Closing the dialog while waiting cancels the
    command.
  - It is offered only as the last way out: on the Account page while unverified, and as
    "Forgot your recovery key?" in the recovery-key dialog.
- **Room keys come from the key backup:**
  - Opening a room's main timeline fetches its keys in the background
    (`Account::fetch_room_keys`), once per room per run (`Running::keys_fetched`). It does
    nothing while the session can't read the backup.
  - When security changes while the session is verified, the desktop sends
    `Command::FetchRoomKeys`, which clears `keys_fetched` and fetches keys for every room with an
    open timeline.
  - The SDK's event cache decrypts again once keys are stored, so no fetch touches the timeline.
- **Settings is where this lives:** the Account page shows verification and recovery actions,
  the offline device, and key export and import; the Sessions page lists every session, this
  one first, to rename or sign out, and links a new device with a QR code; an unverified
  session also gets a banner above the timeline. Verifying someone else starts from their
  profile.

## Consequences

- A new security action is a command plus an `Account` method in `security`.
- The desktop can't scan codes: it shows its own and offers emoji. Two desktops verify by emoji.
- A running session replaces the offline device weekly; one never started again lets it age.
- matrix-sdk's `rehydrate` deletes the device it read before the fresh one is uploaded, and
  offers no read without deleting: when that upload fails, the account has no offline device
  until the hourly retry succeeds.
- The account setting has no compare-and-set: two sessions writing it within the same moment
  can leave the later write's view. Turning it on or off again settles it.
- Resetting the recovery key on a session that never ran the offline device can't carry its key
  over (only the old recovery key opens it): sessions recovering with the new key then don't
  take it over, and turning it on again makes a fresh one; what the old one caught is lost. Asking for the old key would defeat
  a reset meant for a lost one.
  Another client's session holding the key replaces it too, which is harmless: only the newest
  is on the server.
- The SDK deletes backup and recovery before the server asks for approval or the password.
  Cancelling then leaves no recovery, no worse than before; running the reset again finishes it.
- Messages encrypted before a reset, or never backed up, stay unreadable unless another session
  holds their keys. The reset warning says so.
- A key fetch costs one request per room per run.
- None of this is checked headless against a live server. Scenes cover the screens, tests the
  reducer.
