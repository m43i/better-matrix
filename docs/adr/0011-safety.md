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
  answer is read back from the server (the store's copy changes only with the next sync).
  Changes run one at a time per account (`Account::safety_changes`), each read-modify-writing the
  server's latest copy, so two made before a sync both hold. Should reading the answer back fail
  after a save, the choices the server last confirmed stand in, with the change applied. A sync
  changes only the parts whose events it carried, and reports nothing while a change is being
  saved (that change's answer does), so a sync answered before a save can't undo it. Reports go
  through `Command::Report { target: ReportTarget::{Message, Room, Person}, reason }`; the reason
  may be empty and is trimmed. The apps decide with `Safety::shows_media`,
  `Safety::shows_invitation` and `Safety::ignores` (mirrored in the phones' selectors).
- **Stored in account data**, never on the device:
  - ignored people in `m.ignored_user_list`; the SDK's event cache clears every room when a sync
    brings a new list, so earlier messages from them disappear too. Until then, and for what
    the apps keep themselves (a room's latest message, saved and pinned messages, the Files
    list), the apps leave out what an ignored person sent: dropped once the server confirms the
    ignore, filtered where shown while it is saving, so a failed ignore brings it back;
  - media and invitation pictures in MSC4278's event, keeping fields other clients set: read
    under the spec's name first, then the unstable one (as the SDK does); written under the
    unstable name, as the SDK and Element write it, and under the spec's name only where one
    exists, so this app never makes a copy that shadows Element's later choices; only the changed
    field is written, so each copy keeps its other fields, values this app doesn't know included. Where both exist, the spec's copy wins. An absent field shows; any other
    value this app doesn't know, `null` included, hides, as MSC4278 asks. Room-level overrides
    (the same event in a room's account data) aren't read;
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
  stranger. Every invitation is judged again whenever the account's joined rooms change. With `hide_stranger_invites`, such invitations leave the Invitations section, Home's
  cards and counters, and pushes (`fetch` answers `PushContent::Nothing`, reading the choice
  from the server, as the device's copy may be old (for at most 3 s, then the device's copy),
  and judging an invitation whose event can't
  be fetched by its room, an unknown inviter as a stranger); they are not declined
  and Settings → Safety (for the account on screen) lists them, each opening the usual
  invitation preview. Every invitation preview also offers Report (the room) and, when the
  inviter is known, Ignore: it asks, ignores the inviter, and declines once that is saved (a
  failed save keeps the invitation).
- **Pictures in invitations:** with `invite_pictures` off, invitations show initials for the
  room and the inviter everywhere they appear, and their pictures are never fetched.
- **Media:** `ShowMedia::{Always, Private, Never}` (MSC4278's `on`, `private`, `off`). `Private`
  shows media in private rooms only, as MSC4278 lists them: invite, knock, restricted and
  knock-restricted join rules; `RoomSummary::public` is every other rule, unknown ones and a
  rule not loaded yet included, so media waits rather than shows. Images, stickers,
  video posters, linked GIFs and link cards' images that may not show draw a placeholder of the
  same size with Show; a press reveals that message's media until the app restarts. Nothing is
  fetched before; the attachment preview applies the same rule wherever it opens from (a file
  whose sender isn't known counts as someone else's), as do every file action (download, copy,
  open, share) and an open preview when the choice changes. The user's own media and avatars
  always show.

## Consequences

- iOS can't drop a push without Apple's notification-filtering entitlement: a hidden
  invitation's push shows the push's own generic text there, as any push that turns out to show
  nothing does. Android shows nothing.
- Ignoring takes effect on every session at its next sync and clears each room's cached
  timeline, so an open room reloads its history once.
- Reports reach the user's own homeserver's admins, not the room's moderators; a server older
  than v1.14 may refuse a report about a person, which shows as a failure.
- The stranger rule depends on what the store knows, and is judged again only when the
  account's joined rooms change: joining a room with someone makes their invitation show, but an
  inviter who leaves the last shared room stays known until then. A push for an invitation to a
  room this device has never heard of, on a server without sliding sync, can't be judged and
  shows generically.
- A newer server-side invite filter can replace `chat.bettermatrix.invites` once homeservers
  support it; the setting would then move into `m.invite_permission_config`.
