---
name: better-matrix
description: Use the bm terminal client to find Matrix conversations, read or send messages, manage rooms and spaces, handle invitations, or work with a Better Matrix account.
---

# Better Matrix

Use `bm auth list` to find signed-in accounts and `bm --account @alice:example.org` to select one for a command. Room IDs, aliases and Matrix permalinks identify conversations. Pass stable room and event IDs from command results to follow-up commands. Quote Matrix IDs and aliases in shell commands.

Use `--json` to consume results in scripts. Watches emit one JSON object per line. Use `--no-input` when running unattended. Check `bm help <command>` for the installed version's exact flags.

Read the reference for the current task.

- Sign-in, verification, recovery, and room keys → [auth](references/auth.md)
- Room membership, moderation, read receipts, and settings → [rooms](references/rooms.md)
- Space membership and child rooms or subspaces → [spaces](references/spaces.md)
- Direct chats and note to self → [dms](references/dms.md)
- Pending invitations → [invites](references/invites.md)
- History, search, sending, replies, edits, and reactions → [messages](references/messages.md)
- Thread roots → [threads](references/threads.md)
- Shared room pins → [pins](references/pins.md)
- Privately saved messages → [saved](references/saved.md)
- Attachments → [files](references/files.md)
- Recent and unread mentions → [mentions](references/mentions.md)
- Notification modes and keywords → [notifications](references/notifications.md)
- Directory, profiles, and ignored users → [people](references/people.md)
- Your display name, avatar, and presence → [profile](references/profile.md)
- Reports to the homeserver → [report](references/report.md)
- Help, shell completions, diagnostics, and skill installation → [cli](references/cli.md)
