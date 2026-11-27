# CLI utilities

Global options include `--account <user-id>`, `--data-dir <path>`, `--json`, `--no-input`, `--yes`, and `--timeout <seconds>`. The selected data directory has one running CLI process at a time. Use a different data directory and session for a long-running watcher. Exit codes are 0 for success, 1 for a general failure, 2 for usage or input, 3 for authentication, 4 for permissions, 5 for rate limits, and 6 for timeout.

## `bm sync`

Wait for a successful sync of the selected account and return its ID and sync status.

```bash
bm sync
```

## `bm doctor`

Show the CLI version, data directory and selected session information. It works before sign-in.

```bash
bm doctor
```

## `bm completions`

Print a completion script for `bash`, `zsh`, or `fish` to stdout.

```bash
bm completions bash
```

## `bm help`

Print help at any depth. The `-h` and `--help` flags are available on each command.

```bash
bm help rooms settings set
```

## `bm --version`

Print the installed CLI version. The short flag is `-V`.

```bash
bm --version
```

## `bm skill`

Prompt for installation in the current directory or globally for your user. Installs the bundled skill files and prints their paths.

```bash
bm skill
```

## `bm skill install`

Install the offline skill. Use `--scope local|global` to skip the scope prompt. Claude Code receives a copy when detected. Existing files that differ from the bundle require confirmation.

```bash
bm skill install --scope local
```
