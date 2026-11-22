# 0004. Encryption

Status: Accepted
Date: 2026-11-14

## Context

A new session can't read earlier encrypted messages until it has their room keys, from
verification by another session or from the key backup unlocked with the recovery key. People
also manage their recovery key and sessions, and need an identity reset when the recovery key
and every verified session are lost. The server guards some steps: OAuth accounts approve on the
account page, password accounts enter the password. matrix-sdk's default backup strategy never
fetches keys, so a newly verified session would stay unable to read history.

## Decision

- **matrix-sdk does the cryptography:**
  - `matrix_core::security` wraps its recovery, verification and device APIs as `Account`
    methods, reached only through commands (`Recover`, `SetUpRecovery`, `ResetRecoveryKey`,
    `VerifyThisSession`, `AcceptVerification`, `ConfirmVerification`, `CancelVerification`,
    `ListSessions`, `RenameSession`, `EndSession`, `ResetIdentity`, `FetchRoomKeys`).
  - Every client is built with `auto_enable_cross_signing`, `auto_enable_backups` and
    `BackupDownloadStrategy::AfterDecryptionFailure`, the settings Element X uses.
  - One exception encrypts by hand: scheduled messages (MSC4140) must be encrypted before the
    server holds them, and the SDK encrypts only what it sends. Each client is built on a base
    client the engine makes itself (`account::Crypto`, the SDK builder's `base_client`, same
    stores and defaults), and `matrix_core::scheduled` follows the SDK's own send path with its
    `OlmMachine` ([0007](0007-composer-and-media.md)).
- **Security state is pushed:** the engine watches the SDK's recovery and verification state and
  sends `Event::SecurityChanged` with `Security { session_verified, recovery }` at start and on
  every change. The banner and the Account page read it; nothing polls.
- **Verification is one flow model:**
  - Outgoing and incoming requests (from the user's other session) are the same flow, keyed by
    `flow_id` in the engine's `Flows`.
  - The engine reports `VerificationStep`s (`Incoming`, `Waiting`, `Compare(emoji)`,
    `Confirmed`, `Done`, `Cancelled`), shown in the Verification modal.
  - Only emoji (SAS) comparison is offered, not QR codes.
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
- **Settings is where this lives:** the Account page shows verification and recovery actions;
  the Sessions page lists every session, this one first, to rename or sign out; an unverified
  session also gets a banner above the timeline.

## Consequences

- A new security action is a command plus an `Account` method in `security`.
- Verifying other users can reuse `Flows` and the Verification modal, given a user identity on
  the request.
- The SDK deletes backup and recovery before the server asks for approval or the password.
  Cancelling then leaves no recovery, no worse than before; running the reset again finishes it.
- Messages encrypted before a reset, or never backed up, stay unreadable unless another session
  holds their keys. The reset warning says so.
- A key fetch costs one request per room per run.
- None of this is checked headless against a live server. Scenes cover the screens, tests the
  reducer.
