# Profile

These commands target the selected account.

## `bm profile show`

Fetch your display name and avatar from the homeserver.

```bash
bm profile show
```

## `bm profile name`

Set your display name. Pass an empty string to clear it.

```bash
bm profile name "Alice on call"
```

## `bm profile avatar`

Upload an image as your profile picture.

```bash
bm profile avatar ./avatar.png
```

## `bm profile status`

Set your Better Matrix account presence to `online`, `away`, or `hidden`.

```bash
bm profile status away
```

## `bm profile status-message`

Set the text that accompanies your presence. Use `--clear` to remove it.

```bash
bm profile status-message "On call until 18:00"
```
