# 0013. Command line

Status: Accepted
Date: 2026-11-27

## Context

Servers and scripts need the same Matrix accounts, encryption, conversations and actions as the
desktop and phones. A terminal process may run one operation and exit, or watch for new messages.

## Decision

- `apps/cli` is the Rust app and `bm` is its executable. Clap supplies nested help and completion
  scripts. Linux releases package the executable separately for x86_64 and aarch64.
- The CLI drives the same engine commands and observes their completion. Terminal operations in the
  engine supply pagination, server-acknowledged sends, mention watches, and additional lookups.
  They use the SDK inside `matrix-core`. The terminal engine waits for the first full-room sync
  and drains to-device events before reporting readiness. A watch attaches before live sync
  resumes, backfills limited timelines to the last observed event, and follows the event cache’s
  decryption reports. An unrecoverable gap ends the watch with an error. Terminal sync leaves
  presence alone.
- Explicit sign-in checks the returned user and any resumed device before opening the persistent
  store. The terminal engine persists soft logout so the next invocation can reuse its device.
- The store defaults to `$XDG_DATA_HOME/better-matrix-cli` (with `~/.local/share` as the default
  base). `--data-dir` selects another store. A process holds an exclusive lock while the store is
  open. A foreground watcher can use its own store and session alongside other CLI invocations.
  `FileSecrets` keeps session tokens and database passphrases in private files. Passwords and
  recovery keys enter through a hidden terminal prompt or explicit stdin options.
- CLI catalog keys use their English source messages for Clap help and prompts. Options, JSON
  fields, and exit codes are stable across locales.
- One-shot actions produce a single result and a documented exit code. In JSON mode, watches emit
  newline-delimited objects. Network timeouts are errors. Message sends wait for an event ID and
  accept a caller's transaction ID for retrying the same send. Read receipts are explicit.
- `native-media` enables calls and desktop maps. Desktop and mobile enable it, and the CLI
  release builds it out by compiling the CLI package by itself. Parser help and skill
  installation work without an account or a network connection.
- `bm skill` installs one bundled skill per supported destination at the chosen scope. A short
  `SKILL.md` links to the reference for each command group. The references include examples
  verified against the same argument parser that supplies terminal help.

## Consequences

- The CLI can be installed on a Linux server without desktop media libraries, and watchers can be
  supervised as foreground processes.
- The newest messages come through sync. History pages come from the homeserver. Message search
  reads the CLI session's local decrypted cache. Mention history has a bounded scan and reports
  how many events lacked room keys.
