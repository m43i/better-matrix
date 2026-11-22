# 0011. Safety

Status: Accepted
Date: 2026-11-22

## Context

- Public rooms let anyone speak, so they bring spam, abuse and unwanted invitations. A user
  needs to stop hearing from someone, tell the homeserver's admins, and keep strangers'
  invitations and pictures out of sight without leaving the room.
- Matrix has the pieces: ignoring (`m.ignored_user_list`, the server then sends nothing from
  them), reporting (`/rooms/{id}/report/{event}`, `/rooms/{id}/report` and, since v1.14,
  `/users/{id}/report`), and MSC4278's `m.media_preview_config` (media previews `on`, `private`
  or `off`; invite avatars `on` or `off`), which Element already writes under
  `io.element.msc4278.media_preview_config`.
- Server-side invite filtering (`m.invite_permission_config`, MSC4380/4494) is too new for most
  homeservers, and it rejects invitations the user never sees.
- A user has several accounts; each has its own choices, and each follows the user to their
  other sessions.

## Decision

- **One vocabulary:** `Safety` (per account: `ignored`, `media: ShowMedia`, `invite_pictures`,
  `hide_stranger_invites`) is reported as `Event::Safety` once an account is ready, when a sync
  brings any of its account data events, and after `Command::SetSafety(SafetyChange)`, whose
  answer already holds the change (the store's copy changes only with the next sync). Reports go
  through `Command::Report { target: ReportTarget::{Message, Room, Person}, reason }`; the reason
  may be empty and is trimmed. The apps decide with `Safety::shows_media`,
  `Safety::shows_invitation` and `Safety::ignores` (mirrored in the phones' selectors).
- **Stored in account data**, never on the device:
  - ignored people through the SDK (`ignore_user`), whose event cache clears every room when the
    list changes, so earlier messages from them disappear too; nothing in the apps filters them;
  - media and invitation pictures in MSC4278's event under both names, the spec's read first,
    keeping fields other clients set, so Element and this app follow one choice;
  - hiding strangers' invitations in `chat.bettermatrix.invites` (`hide_strangers`), which no
    other client reads.
- **Ignoring:** offered from the member menu and the person panel (desktop) and the member sheet
  (phones), never for the user themselves; it asks first, as it hides history. Settings → Safety
  lists ignored people with Stop ignoring.
- **Reporting:** a dialog (a sheet on phones) with an optional reason and "Also ignore them" for a
  message or a person. Offered on others' messages (message menu, message actions), on rooms
  (room menu, room info), on people (member menu, person panel, member sheet) and on invitations.
  Success shows a notice; a failure keeps the dialog open with a notice.
- **Strangers' invitations:** `Invitation::stranger` is set when the inviter shares no joined
  room with the account as this device knows it: a direct chat with them, or a room where they
  are a joined member in the store. Lazy loading leaves quiet members of large rooms out, so the
  rule errs towards calling someone a stranger, which only hides. An unknown inviter is a
  stranger. With `hide_stranger_invites`, such invitations leave the Invitations section, Home's
  cards and counters, and pushes (`fetch` answers `PushContent::Nothing`); they are not declined
  and Settings → Safety (for the account on screen) lists them, each opening the usual
  invitation preview. Every invitation preview also offers Report (the room) and, when the
  inviter is known, Ignore: it asks, then ignores the inviter and declines.
- **Pictures in invitations:** with `invite_pictures` off, invitations show initials for the
  room and the inviter everywhere they appear, and their pictures are never fetched.
- **Media:** `ShowMedia::{Always, Private, Never}` (MSC4278's `on`, `private`, `off`). `Private`
  shows media in rooms that aren't public (`RoomSummary::public`, the public join rule). Images,
  video posters, linked GIFs and link cards' images that may not show draw a placeholder of the
  same size with Show; a press reveals that message's media until the app restarts. Nothing is
  fetched before. The user's own media, avatars and stickers always show.

## Consequences

- Ignoring takes effect on every session at its next sync and clears each room's cached
  timeline, so an open room reloads its history once.
- Reports reach the user's own homeserver's admins, not the room's moderators; a server older
  than v1.14 may refuse a report about a person, which shows as a failure.
- The stranger rule depends on what the store knows; joining a room with someone later makes
  their invitation show when the room list next reads it.
- A newer server-side invite filter can replace `chat.bettermatrix.invites` once homeservers
  support it; the setting would then move into `m.invite_permission_config`.
