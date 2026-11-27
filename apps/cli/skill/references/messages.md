# Messages

Use a room ID for either a group room or direct chat. Reads and watches leave receipts to `rooms mark-read`. History requires the session’s room keys to decrypt encrypted messages. Quote event IDs because they can start with a dollar sign.

## `bm messages list`

Return one server page, newest first, with `messages` and `next_cursor`. Pass the cursor back with `--cursor` and keep the same room or `--thread` root. `--limit` is the page size (1–100, default 30). Filtered pages can be short.

```bash
bm messages list '!ops:example.org' --limit 20
```

## `bm messages show`

Read one message by event ID. The result includes its sender, time, text, and decryption status.

```bash
bm messages show '!ops:example.org' '$deploy-event'
```

## `bm messages search`

Fuzzy search the CLI session’s locally stored history, optionally in one room with `--room`. Returns ranked matches with room and event IDs.

```bash
bm messages search "deploy complete" --room '!ops:example.org'
```

## `bm messages watch`

Follow new messages in the room until Ctrl-C. Add `--thread` to restrict to one thread, or global `--json` for JSON Lines. The watch backfills limited sync batches and exits with an error if it cannot recover a gap.

```bash
bm messages watch '!ops:example.org' --json
```

## `bm messages send`

Send text supplied as an argument or with `--stdin`. Add `--markdown`, `--reply-to`, a `--thread` root, repeatable `--mention`, or `--mention-room`. Returns the acknowledged server event ID. Use `--txn-id` with automated sends. Exit 6 means delivery is unknown. Retry the same logical send with the same transaction ID.

```bash
bm messages send '!ops:example.org' "Deploy complete" --txn-id deploy-842
```

## `bm messages edit`

Replace the text of your original message. Supply text directly or with `--stdin`, and add `--markdown` if needed. Returns the edit’s event ID.

```bash
bm messages edit '!ops:example.org' '$deploy-event' "Deploy complete at 16:20"
```

## `bm messages redact`

Redact the given event subject to Matrix permissions. Confirm the prompt or pass `--yes`. Returns the redaction’s event ID.

```bash
bm messages redact '!ops:example.org' '$deploy-event'
```

## `bm messages forward`

Send the message’s current text or attachment content as a new message into another joined room. Returns the new room and event IDs.

```bash
bm messages forward '!ops:example.org' '$deploy-event' '!incidents:example.org'
```

## `bm messages react`

Add the emoji to a message, or return your existing matching reaction. Returns the reaction event ID.

```bash
bm messages react '!ops:example.org' '$deploy-event' "👍"
```

## `bm messages unreact`

Remove your matching reaction. Returns the target and any redaction event IDs.

```bash
bm messages unreact '!ops:example.org' '$deploy-event' "👍"
```

## `bm messages source`

Return the event’s JSON as read by this session and, when available, the encrypted event as it was sent.

```bash
bm messages source '!ops:example.org' '$deploy-event'
```
