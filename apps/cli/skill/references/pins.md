# Pinned messages

Room permissions control who may change pins, which are shared with everyone in the room.

## `bm pins list`

List the room’s pinned messages in pin order. Unavailable events retain their event IDs.

```bash
bm pins list '!ops:example.org'
```

## `bm pins add`

Pin a message for everyone in the room.

```bash
bm pins add '!ops:example.org' '$deploy-event'
```

## `bm pins remove`

Remove a message from the room’s pins.

```bash
bm pins remove '!ops:example.org' '$deploy-event'
```
