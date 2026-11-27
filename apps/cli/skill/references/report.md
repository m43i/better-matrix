# Reports

Reports go to the selected account’s homeserver for review by its administrators. Supply context using `--reason`.

## `bm report message`

Report a message identified by room and event ID.

```bash
bm report message '!ops:example.org' '$deploy-event' --reason "Spam"
```

## `bm report room`

Report a room by room ID, alias, or permalink.

```bash
bm report room '!ops:example.org' --reason "Spam"
```

## `bm report user`

Report a Matrix user by their full ID.

```bash
bm report user '@alice:example.org' --reason "Spam"
```
