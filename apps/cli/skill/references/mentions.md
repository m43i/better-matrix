# Mentions

Highlights follow the account’s Matrix rules, including keywords. Use `notifications` to control those rules.

## `bm mentions`

Scan recent history for highlights, with `--room`, `--limit` (1–100, default 50), and `--since` (RFC 3339, default the past day). The output reports scan limits and events missing decryption keys.

```bash
bm mentions --room '!ops:example.org' --since 2026-11-25T08:00:00Z
```

## `bm mentions count`

Return synced unread mention totals by room, optionally restricted with `--room`.

```bash
bm mentions count --json
```

## `bm mentions watch`

Stream newly synced highlights until Ctrl-C. Use `--room` for one room and `--json` for JSON Lines. The watch backfills limited sync batches and exits with an error if it cannot recover a gap.

```bash
bm mentions watch --room '!ops:example.org' --json
```
