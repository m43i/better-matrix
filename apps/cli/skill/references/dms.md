# Direct chats

Use the room ID returned by `dms open` or `dms note-to-self` with [messages](messages.md) or [files](files.md).

## `bm dms list`

List joined direct chats and their room IDs. Add `--unread` to filter by unread messages, mentions, or a manual unread mark.

```bash
bm dms list --unread
```

## `bm dms open`

Find the existing direct chat with the user or create one. Returns its room ID.

```bash
bm dms open '@alice:example.org'
```

## `bm dms leave`

Leave a joined direct chat by room ID.

```bash
bm dms leave '!ops:example.org'
```

## `bm dms note-to-self`

Find or create the account’s private, encrypted note to self and return its room ID.

```bash
bm dms note-to-self
```
