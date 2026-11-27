# Notifications

Notification settings are stored as Matrix push rules on the account.

## `bm notifications show`

Show the account-wide default or a room’s effective mode and whether it overrides the default.

```bash
bm notifications show '!ops:example.org'
```

## `bm notifications default`

Set the account-wide mode to `all`, `mentions`, or `mute`.

```bash
bm notifications default mentions
```

## `bm notifications room`

Set a room to `all`, `mentions`, or `mute`. Use `default` to clear its override.

```bash
bm notifications room '!ops:example.org' mute
```

## `bm notifications keywords list`

List keywords that notify the account.

```bash
bm notifications keywords list
```

## `bm notifications keywords add`

Add a keyword notification rule.

```bash
bm notifications keywords add "on call"
```

## `bm notifications keywords remove`

Remove a keyword notification rule.

```bash
bm notifications keywords remove "on call"
```
