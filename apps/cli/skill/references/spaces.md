# Spaces

A space groups rooms and subspaces. Room settings, membership and moderation are documented in [rooms](rooms.md).

## `bm spaces list`

List joined spaces and their IDs.

```bash
bm spaces list
```

## `bm spaces show`

Show a joined space and its ordered child IDs.

```bash
bm spaces show '!eng:example.org'
```

## `bm spaces discover`

Search the public directory for spaces.

```bash
bm spaces discover engineering
```

## `bm spaces create`

Create a private space and return its ID. Use `--topic`, `--alias`, `--parent`, `--join-rule` and `--publish` to configure it. A restricted join rule requires a parent.

```bash
bm spaces create "On call" --parent '!eng:example.org' --alias oncall
```

## `bm spaces join`

Join a space and return its ID. Repeat `--via` to add candidate servers.

```bash
bm spaces join '#engineering:example.org'
```

## `bm spaces leave`

Leave the space itself. Its rooms and subspaces stay joined.

```bash
bm spaces leave '!eng:example.org'
```

## `bm spaces children list`

Ask the server for visible direct children. Add `--recursive` to include descendants.

```bash
bm spaces children list '!eng:example.org' --recursive
```

## `bm spaces children add`

Add a joined room or subspace to the space.

```bash
bm spaces children add '!eng:example.org' '!ops:example.org'
```

## `bm spaces children remove`

Remove a child from the space’s hierarchy. Your membership in the child continues.

```bash
bm spaces children remove '!eng:example.org' '!ops:example.org'
```

## `bm spaces children order`

Supply every immediate child by room ID in the desired order.

```bash
bm spaces children order '!eng:example.org' '!ops:example.org' '!incidents:example.org'
```
