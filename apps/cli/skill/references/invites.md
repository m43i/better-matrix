# Invitations

Invitation targets are rooms or spaces.

## `bm invites list`

List pending invitations with room IDs and, when known, their inviters.

```bash
bm invites list
```

## `bm invites show`

Show the pending invitation for a room.

```bash
bm invites show '!ops:example.org'
```

## `bm invites accept`

Accept the invitation and return the joined room ID.

```bash
bm invites accept '!ops:example.org'
```

## `bm invites decline`

Decline the pending invitation.

```bash
bm invites decline '!ops:example.org'
```
