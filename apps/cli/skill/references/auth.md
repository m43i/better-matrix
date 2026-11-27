# Accounts and encryption

Use a separate CLI session for each foreground watcher that must run alongside other commands. Interactive secrets use hidden prompts. Supply secrets from a protected stdin source for unattended runs.

## `bm auth discover`

Resolve a server name, homeserver URL, or Matrix user ID. Returns the Client API URL and available sign-in methods.

```bash
bm auth discover '@alice:example.org'
```

## `bm auth login`

Sign in a full Matrix user ID and return its account and device. Use `--server` to override discovery and `--method password|oauth` to choose a method. Password sign-in also accepts `--password-stdin`. OAuth prints a browser URL on stderr. When running over SSH, forward the loopback port in its redirect URI to the CLI host.

```bash
bm auth login '@alice:example.org' --method password
```

## `bm auth list`

List accounts in the CLI store, including their default selection and whether they need to sign in again.

```bash
bm auth list
```

## `bm auth use`

Select a signed-in Matrix user ID as the default for subsequent commands.

```bash
bm auth use '@alice:example.org'
```

## `bm auth whoami`

Return the selected account, device ID, and local verification and recovery status.

```bash
bm auth whoami
```

## `bm auth logout`

Sign out the selected CLI session and delete its saved local state. Confirm the prompt or pass `--yes`. Sign in again to make a new session.

```bash
bm auth logout
```

## `bm auth sessions list`

List the account’s sessions with device IDs, names, last-seen information and verification status.

```bash
bm auth sessions list
```

## `bm auth sessions rename`

Set the display name of a session identified by device ID.

```bash
bm auth sessions rename CLI123 "Automation server"
```

## `bm auth sessions end`

End the chosen device’s session after confirmation. Pass `--yes` to confirm in scripts. Password accounts may ask for the password. OAuth accounts can return an account-management URL for the browser.

```bash
bm auth sessions end OLD456
```

## `bm auth verify`

Start emoji verification for this device. Pass a Matrix user ID to verify that person in your joined direct chat. Compare the displayed emoji before confirming.

```bash
bm auth verify
```

## `bm auth recovery status`

Show whether this session is verified and whether account recovery is available.

```bash
bm auth recovery status
```

## `bm auth recovery restore`

Use the account’s recovery key to verify this device and unlock its key backup. Use `--key-stdin` when reading the key from stdin.

```bash
bm auth recovery restore
```

## `bm auth keys import`

Import room keys from an encrypted export. Returns the number imported. `--passphrase-stdin` reads the file’s passphrase from stdin.

```bash
bm auth keys import ./room-keys.txt
```

## `bm auth keys export`

Export the session’s room keys to an encrypted file at a new path. `--passphrase-stdin` reads the file’s passphrase from stdin.

```bash
bm auth keys export ./room-keys.txt
```
