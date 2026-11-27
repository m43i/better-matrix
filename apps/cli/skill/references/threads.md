# Threads

Use the returned root event ID with `messages list --thread`, `messages watch --thread`, or `messages send --thread`.

## `bm threads list`

List a room’s threads, most recently active first. Results include root IDs and reply counts.

```bash
bm threads list '!ops:example.org'
```
