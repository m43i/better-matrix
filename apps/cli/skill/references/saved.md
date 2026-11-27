# Saved messages

Saved messages are private to the account and sync to its other Better Matrix sessions.

## `bm saved list`

List messages the account saved, newest saved first. Add `--room` to filter.

```bash
bm saved list --room '!ops:example.org'
```

## `bm saved add`

Save a message privately.

```bash
bm saved add '!ops:example.org' '$deploy-event'
```

## `bm saved remove`

Remove a message from your saved list.

```bash
bm saved remove '!ops:example.org' '$deploy-event'
```
