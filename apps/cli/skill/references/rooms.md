# Rooms

Room arguments accept a Matrix room ID, alias, or permalink. Membership and settings commands also accept spaces where Matrix permissions allow them.

## `bm rooms list`

List joined group rooms. Use `--unread` for rooms with unread messages, mentions, or a manual unread mark. Use `--space` for rooms under a joined space.

```bash
bm rooms list --space '!eng:example.org' --unread
```

## `bm rooms show`

Show a joined room’s summary or, if it is outside the room list, ask the server for a preview.

```bash
bm rooms show '!ops:example.org'
```

## `bm rooms discover`

Search the public room directory by name, alias or topic.

```bash
bm rooms discover "systems engineering"
```

## `bm rooms create`

Create a private, encrypted room and return its room ID. Set `--topic`, `--alias` (local part), `--space`, `--join-rule private|public|knock|restricted`, `--publish`, or `--unencrypted` as needed. Restricted rooms require a parent space.

```bash
bm rooms create "Operations" --alias ops --space '!eng:example.org'
```

## `bm rooms join`

Join a room by ID, alias, or permalink. Repeat `--via` to add servers to try. Returns the joined room ID.

```bash
bm rooms join '#ops:example.org' --via example.org
```

## `bm rooms knock`

Ask to join a room that allows join requests. Optionally share a reason with moderators. Returns the room ID.

```bash
bm rooms knock '#ops:example.org' --reason 'I am on call this week'
```

## `bm rooms leave`

Leave a conversation.

```bash
bm rooms leave '!ops:example.org'
```

## `bm rooms forget`

Ask the server to forget a room that you have already left. Confirm the prompt or pass `--yes`.

```bash
bm rooms forget '!ops:example.org'
```

## `bm rooms members`

List member IDs, names and power levels. Use `--membership joined|invited|banned|knocking` to choose the membership state. Defaults to joined.

```bash
bm rooms members '!ops:example.org' --membership invited
```

## `bm rooms invite`

Invite a Matrix user to the room.

```bash
bm rooms invite '!ops:example.org' '@alice:example.org'
```

## `bm rooms kick`

Remove a member from the room.

```bash
bm rooms kick '!ops:example.org' '@alice:example.org'
```

## `bm rooms ban`

Ban a Matrix user from the room.

```bash
bm rooms ban '!ops:example.org' '@alice:example.org'
```

## `bm rooms unban`

Lift the room ban for a Matrix user.

```bash
bm rooms unban '!ops:example.org' '@alice:example.org'
```

## `bm rooms approve`

Approve a pending request to join. The user receives an invitation.

```bash
bm rooms approve '!ops:example.org' '@alice:example.org'
```

## `bm rooms deny`

Decline a pending request to join.

```bash
bm rooms deny '!ops:example.org' '@alice:example.org'
```

## `bm rooms power-level`

Assign an integer Matrix power level to a room member, subject to room permissions.

```bash
bm rooms power-level '!ops:example.org' '@alice:example.org' 50
```

## `bm rooms mark-read`

Send an explicit receipt and clear the room’s manual unread mark. With `--through`, set the receipt at the specified event.

```bash
bm rooms mark-read '!ops:example.org' --through '$deploy-event'
```

## `bm rooms mark-unread`

Set the account’s manual unread mark. Add `--clear` to remove the mark.

```bash
bm rooms mark-unread '!ops:example.org'
```

## `bm rooms priority`

Set the room tag to `favourite`, `low`, or `normal`.

```bash
bm rooms priority '!ops:example.org' favourite
```

## `bm rooms settings show`

Return room settings, addresses, permissions, encryption state, and version.

```bash
bm rooms settings show '!ops:example.org'
```

## `bm rooms settings set`

Change one setting. Use `name`, `topic`, `avatar` (file path or `none`), `join-rule` (same values as create), `history` (`world-readable|shared|invited|joined`), `guest-access` or `directory` (both `true|false`). An empty topic clears it.

```bash
bm rooms settings set '!ops:example.org' history joined
```

## `bm rooms settings alias add`

Publish a room alias by local part or full alias.

```bash
bm rooms settings alias add '!ops:example.org' '#oncall:example.org'
```

## `bm rooms settings alias remove`

Unpublish a room alias. The alias is deleted when it belongs to your server.

```bash
bm rooms settings alias remove '!ops:example.org' '#old-ops:example.org'
```

## `bm rooms settings alias primary`

Select an already published alias as the main one.

```bash
bm rooms settings alias primary '!ops:example.org' '#ops:example.org'
```

## `bm rooms settings permission`

Set the level a permission requires. Use kebab-case permission names from `rooms settings show`, for example `send-messages` or `mention-room`.

```bash
bm rooms settings permission '!ops:example.org' mention-room 50
```

## `bm rooms settings encrypt`

Permanently enable end-to-end encryption. Confirm the prompt or pass `--yes`.

```bash
bm rooms settings encrypt '!ops:example.org'
```

## `bm rooms upgrade`

Replace a room with a successor on the specified room version. Check `rooms settings show` for the offered version. Returns the successor ID after confirmation.

```bash
bm rooms upgrade '!ops:example.org' 12
```
