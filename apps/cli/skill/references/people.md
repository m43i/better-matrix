# People

Use full Matrix user IDs for commands that target a person.

## `bm people search`

Search the homeserver’s user directory. Returns user IDs and display names.

```bash
bm people search Alice
```

## `bm people show`

Fetch the user’s public Matrix profile.

```bash
bm people show '@alice:example.org'
```

## `bm people ignore`

Add a user to the account’s ignored list.

```bash
bm people ignore '@alice:example.org'
```

## `bm people unignore`

Remove a user from the account’s ignored list.

```bash
bm people unignore '@alice:example.org'
```

## `bm people ignored`

List ignored Matrix user IDs.

```bash
bm people ignored
```
