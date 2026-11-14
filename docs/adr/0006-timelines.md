# 0006. Timelines

Status: Accepted
Date: 2026-11-14

## Context

A timeline shows a room's messages and room events, or one thread's. People react, reply and
see who is typing there.
- After time away, sync resumes with the newest messages and a gap before them, which appending
  sync events can't fill in the right place.
- Messages written offline must not be lost.
- Opening a room or thread must feel instant. A thread's first page can wait seconds in
  matrix-sdk for a pagination token sync never sends.
- Only what the user sees should cost requests, memory and redraws.

## Decision

- **Source:** matrix-sdk-ui's `Timeline`, on the SDK's event cache (history on disk, gaps
  tracked).
  - The engine follows only open timelines (`Command::OpenTimeline` / `CloseTimeline`) and
    forwards their changes as `Event::TimelineChanged` diffs (`TimelineDiff`, by index).
  - The desktop applies them to a mirror (`RoomTimeline::apply`). Entries it doesn't draw stay
    as `Other`, so indices line up.
  - Sync reports each room's newest message (`Event::LatestMessage`) for the room list and the
    overview, which open no timeline. Across restarts the room list's order comes from the
    store's latest event (`RoomSummary::activity_ms`; the room list re-reads a room when the SDK
    says its latest event changed).
- **Timeline keys:** every timeline is a `TimelineKey { room_id, thread }`: the room's main
  timeline (thread replies hidden) or one thread, focused on its root.
  - Opening, closing, paging, sending and reacting all take a key, so the thread panel is a
    second timeline on the same paths.
- **History pages:** `HISTORY_PAGE` = 30 messages, about a screen and a half.
  - Opening loads a page if the timeline holds fewer. Scrolling within a screen of the oldest
    loaded message loads the page before it (`Command::LoadOlder`, `Event::HistoryLoaded`).
  - The viewport holds the reading position as older messages arrive above it.
  - Gaps from time offline fill in place through the same pagination.
- **Start of history:** the server enforces history visibility; paging back returns only what
  the user may read.
  - At the start, the timeline shows where visible history begins. Rooms whose history
    visibility is *joined* or *invited* say earlier messages are hidden by the room's settings.
  - A thread starts at its root: once the root is in, `reached_start` is set. Its pagination
    may never report the start itself.
- **Failed pages:** a failure stops automatic loading, so a flaky server isn't asked in a loop.
  - `RoomTimeline.history_error` holds the error. The timeline shows "Couldn't load earlier
    messages" with Try again, the error in the button's accessible label.
  - A failed first page keeps the timeline open (`Event::HistoryFailed`). A timeline that fails
    to open leaves the open set; Try again opens it again.
- **Opening fast:**
  - The engine subscribes and sends what is cached at once. The first page loads in a spawned
    task beside it, ending in `HistoryLoaded` or `HistoryFailed`; the desktop's `loading` flag
    waits for one.
  - A closed `Timeline` stays warm, the last 8 per account (`WARM_TIMELINES`). Reopening it only
    subscribes. Open and close races are in [0002](0002-engine-and-desktop-state.md).
  - A thread opens with its root copied from the room's timeline (`Accounts::seed_thread`); the
    engine's first update replaces the copy.
  - The room list reads push-rule settings once per list and encryption state from the cache,
    and `RoomsUpdated` goes out only when the list changed.
- **Sending:** `Command::Send { key, reply_to, .. }` goes through the open timeline, which sets
  reply and thread relations and shows a pending message at once. The room's send queue
  persists it, retries offline and resumes after a restart.
- **Reactions, replies, threads:**
  - `Command::React` toggles the user's reaction (`toggle_reaction`). Messages carry each
    reaction's count and whether the user is among them. The picker offers quick reactions,
    then emoji by group.
  - The composer holds the message it answers (`ReplyTarget`); a reply shows its sender and
    excerpt above the body (`Quote`).
  - Clicking a message, or its thread button, opens its thread in the right panel with its own
    composer. A message with replies shows "3 replies · last from …".
  - The hover toolbar fades in through the row's hover group, so hovering never rebuilds the
    view.
- **Polls** (MSC3381, unstable events as clients send them): `MessageContent::Poll` from the
  SDK's poll results, with each answer's votes, the user's picks, the voters and whether it
  ended. A card shows the question and answers; a press votes (`Command::VotePoll`; in a
  single-choice poll pressing one's own answer takes the vote back, `state::poll::next_vote`).
  Counts show as people vote when the poll is disclosed, else once it ends. The sender can end it
  (`Command::EndPoll`), after confirming (`Confirm::EndPoll`): it can't be reopened. Polls are
  written in a dialog from the composer's poll button (`PollForm`: a question, two to twenty
  answers, disclosed or not) and sent as `Outgoing::Poll` with a plain-text fallback listing the
  answers.
- **Call cards:** a call started in the room is a message with `MessageContent::Call` (a
  `CallCard`: video, voice or unsaid, and who declined): MatrixRTC's ring as the SDK parses it
  (`m.rtc.notification`), the unstable ring Element Call sends (which the timeline's event filter
  lets through beside the SDK's own, its intent read from the raw event), and one-to-one invites
  (video when the offer sends video). Whether a call is under way is the room's
  (`RoomSummary::call`), not the card's: the newest card in the timeline shows it, with the
  number in the call and Join (or that the user is in it); older cards read as ended or
  declined. Call memberships stay hidden.
- **Stickers** show as images (`convert::sticker`).
- **Room events:** `EntryKind::RoomEvent` holds the sender (a `Person`) and a `RoomChange`,
  converted from matrix-sdk-ui's membership, profile and state items.
  - Kinds: created; joined, left; invited, invite withdrawn or declined; removed, banned,
    unbanned; asked to join; display name, profile picture; room name, topic, picture;
    encryption, history visibility, join rules, permissions, pinned messages; upgraded,
    addresses, guest access, server access, a space's rooms, a room's spaces, an invite by
    email or phone, moderation rules (policy lists); live location shared.
  - The model carries what changed, never words; the desktop words it. Events that changed
    nothing worth saying stay hidden.
  - They read as small muted lines with round 18 pt pictures.
  - Runs of a kind that `groups()` (joined, left) merge into one line, "Grace, Alan, Kai, Mira
    and 2 more joined", with up to four stacked pictures; hovering "and 2 more" shows everyone.
    Merging is in the view, so entries stay one per event and diff indices stay true.
- **Pictures:** messages carry the sender's picture (`sender_avatar`), loaded as 128 px
  thumbnails and drawn by `media_avatar`.
  - Fallback initials: first letters of the display name's first two words, one for a single
    word, or the user ID's local part's first letter, never the server. Below 24 pt, one letter.
- **Linked GIFs:** Giphy, Tenor (share pages, via `og:image`), Imgur and direct `.gif` links
  resolve in `matrix_core::links`. The engine fetches them with a size limit (1 MiB page, 15
  MiB GIF).
  - A message with a GIF link shows its other text, then the GIF like an attachment, link
    hidden; a bare link shows the GIF alone. Prepared once on arrival (`TimelineItem::gif`).
  - Frames decode on a worker and stream into a `rune::Pixels`, so animation redraws only the
    picture.
  - Fetching contacts those sites, so General → GIFs from links turns it off. Memory bounds and
    retries are in [0007](0007-composer-and-media.md).
- **On screen:** only on-screen rows load media.
  - The virtual list reports its visible rows (`on_visible_rows`); the view maps them, plus 10
    either side (`SHOWN_MARGIN`), to a range of entries at the timeline's revision, and their IDs
    (`accounts::Shown`, `State::rows_shown`). While the revision holds, the range is read
    directly; after a change, the IDs find the entries until the view reports again.
  - Only those entries' previews, pictures and GIFs are fetched (`request_timeline_media`,
    `request_gifs`).
  - Until the view reports, the newest 30 entries count (`SHOWN_ON_OPEN`), where it opens.
  - GIFs scrolled past stay loaded, paused, while their timeline is open; only those on screen
    play ([0007](0007-composer-and-media.md)). Letting go of GIFs no timeline shows (a scan of
    every open timeline) runs when what is open changes, not on scroll.
- **Plan cache:** every `timeline_mut` gives the timeline a revision unique in the process.
  - The view keeps each timeline view's last plan and replans only when the revision changes.
  - The revision is the list's `virtual_rows_version`, so Rune skips asking for row keys on a
    commit that left the timeline alone.
- **Read receipts:** the desktop sends `Command::MarkRead` when an open timeline changes while
  the window is focused, and for each open timeline when focus returns.
  - The engine re-reads rooms on the SDK's room-info updates (receipts, unread markers), so
    badges follow the counts the SDK works out after sync.
- **Typing, received:** the task that forwards an open room timeline's changes also watches its
  typing, so closing stops both. It sends `Event::Typing` as `Person`s from the store.
  - Threads share their room's typing; Matrix has none per thread.
  - The desktop keeps the list while the room is open and forgets it on close.
  - The typing line sits under the room's composer at a fixed height: up to three faces, three
    dots pulsing on Rune's clock (no rebuilds), and "Ada is typing…", "Ada and Grace are
    typing…" or "Ada, Grace and 2 others are typing…".
- **Typing, sent:** `poll` runs `state::typing::Typing`, which compares the composers' drafts
  and yields `Command::Typing`.
  - A changed, non-empty draft says "typing" at most every 3 s (`REPEAT`); the SDK repeats it
    only near the 4 s expiry.
  - An emptied draft or leaving the room says "stopped"; an idle draft lets the notice lapse.
  - Opening a room with an old draft isn't typing.
- **Times:** times and date dividers use the local time zone at that moment, so summer time
  applies to summer dates (`local_ms`).

## Consequences

- One source of truth per timeline. A new message action is a toolbar button and a command; a
  new timeline kind is a key; a new room event is a `RoomChange` variant, a conversion arm and a
  catalog key.
- Warm timelines hold items in memory; eight per account is small next to the event cache.
- A failed history page needs a click; reconnects don't retry it.
- A changed room summary still rebuilds the whole room list. Hundreds of rooms at most make that
  acceptable.
- Receipts go out on every change in a focused timeline; the SDK skips those already sent.
- Any `timeline_mut` moves the revision: a no-op costs a replan, never a stale plan.
- Typing notices have no off switch; adding one is one check in `send_typing`.
- No GIF picker: Giphy and Tenor search needs an API key issued to the project.
- Emoji render through the system's colour emoji font.
