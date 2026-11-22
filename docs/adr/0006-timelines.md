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
    Opening a thread keeps that anchor through the room's narrower layout; it does not settle
    the whole window, including when the SDK sends initial or bootstrap thread snapshots.
    Thread snapshots use an instant commit, without repinning the room's sticky viewport.
  - A timeline's first content and its history land at once (`RoomTimeline::adds_history`: a
    message or room event above all the timeline holds, or its first one; dividers and markers
    count for neither). A window animates engine changes only in a frame with input or asked
    motion (the read marker's fade asks for it as it starts, as a room switch shows the
    marker), so a room opened with a few cached messages would otherwise build up row by row as
    its first page arrives. The change marks its timeline `Landing::Due`; the next poll makes it
    `Now`, and that frame's view lands the timeline's viewport and list
    (`El::instant_changes`; a short timeline's viewport easing taller would uncover the rows
    from the bottom), so the rest of the window keeps its motion; the poll after makes it
    `Idle`, and news at the end eases in again. The rule looks at the diffs, not at
    `HistoryLoaded`, which can arrive before the page's diffs. The "New" divider keeps its fade
    through a landing (`El::animated_changes`), and a fade due to start while a timeline is
    `Due` waits a frame: starting with a settled Reset would land it, its row left blank until
    it closes.
  - Gaps from time offline fill in place through the same pagination.
- **Start of history:** the server enforces history visibility; paging back returns only what
  the user may read.
  - At the start, the timeline shows where visible history begins. Rooms whose history
    visibility is _joined_ or _invited_ say earlier messages are hidden by the room's settings.
  - A thread starts at its root: once the root is in, `reached_start` is set. Its pagination
    may never report the start itself.
- **Failed pages:** a failure stops automatic loading, so a flaky server isn't asked in a loop.
  - `RoomTimeline.history_error` holds the error. The timeline shows "Couldn't load earlier
    messages" with Try again, the error in the button's accessible label.
  - A failed first page keeps the timeline open (`Event::HistoryFailed`). A timeline that fails
    to open leaves the open set; Try again opens it again. Phones do the same
    (`Timeline.openFailed`), and a page that fails with only the command's error (no
    `HistoryFailed`) shows it too. Phones ask for no page while the timeline opens
    (`Timeline.opening`); a linked message is looked for once it has opened, and a failed page
    pauses the search until Try again.
- **Opening fast:**
  - The engine subscribes and sends what is cached at once. The first page loads in a spawned
    task beside it, ending in `HistoryLoaded` or `HistoryFailed`; the desktop's `loading` flag
    waits for one.
  - A closed `Timeline` stays warm, the last 8 per account (`WARM_TIMELINES`). Reopening it only
    subscribes. Open and close races are in [0002](0002-engine-and-desktop-state.md).
  - A thread opens with its root copied from the room's timeline (`Accounts::seed_thread`); the
    engine's first update replaces the copy.
  - Cold threads also request their first replies directly through `Room::relations`, in
    parallel with SDK pagination. This removes the SDK's three-second wait for an initial
    pagination token from the time to first replies, including when nothing is cached.
    `timeline_bootstrap` merges that page with subscription changes by event ID, retains row
    keys and live replies, and hands back to ordinary diffs once the SDK has supplied every
    early event. Completing a network page is not a subscription barrier or evidence that
    unmatched replies were deleted: SDK pages also include reactions and edits. A reply already
    supplied by the SDK is never restored from its early copy if later removed, including
    additions/removals in one batch or before the direct response arrives. Both the direct
    page and its merge preserve server order, using shared event IDs rather than sender clocks.
    SDK pagination continues independently of the displayed early root until the subscription
    catches up or SDK history ends, since a recursive page may cover fewer thread replies.
    Initial loading and explicit retries use the same catch-up path.
    The SDK remains authoritative for edits, reactions, deletion and later history. Closing aborts the
    immediate fetch with the forwarding task. This costs one extra relations request on a cold
    thread (and a root request if it is absent from the event cache).
  - The room list reads push-rule settings once per list and encryption state from the cache,
    and `RoomsUpdated` goes out only when the list changed.
- **Sending:** `Command::Send { key, reply_to, .. }` goes through the open timeline, which sets
  reply and thread relations and shows a pending message at once. The room's send queue
  persists it, retries offline and resumes after a restart.
- **Editing and deleting:** the user's own text messages can be edited (`Command::Edit`, through the
  timeline's `edit`) and any of their messages deleted (`Command::Redact`), from the menu under the
  hover toolbar's ⋮ or right-click menu (`Popover::MessageMenu`), available on every sent message; deleting asks
  first (`Confirm::DeleteMessage`). Up in an empty composer edits the user's newest plain text
  message there (`Message::editable`: the user's own `m.text`; emotes, notices and files keep what
  they are). The composer then holds the message (`Composer::editing`: the account, timeline and
  event, so the edit goes where it began) under an "Editing your message" bar; the draft, its reply
  and attachments are set aside and come back when the edit is sent or dropped. Escape stops it, as
  does replying. While editing, attachments, voice memos and polls are off, and no edit starts while
  a voice memo records or on a message still sending. An emptied or unchanged edit isn't sent. An
  edit left by navigating away, or refused by the server (`Purpose::Edit`), is set aside
  (`State::set_aside`, one per message) and comes back into its composer, as the user left it, once
  its timeline is on screen and the composer is free (no other edit, no memo); sending an edit of
  the same message, from any timeline, drops the kept one, as does removing its account (an edit
  still on its way then doesn't come back either), and sending one ends an edit of the same message
  under way in the other composer. Kept edits come back newest first. Starting an edit brings the
  kept edit back when it was made in the same timeline, and keeps an edit of another message under
  way when it changed anything. Phones keep a refused edit in its composer
  (`ComposerState.kept`, `refusedEdit`): it takes the free field at once, setting aside what
  was typed since, or waits for the edit holding the field to end; a refusal of an edit older
  than the newest sent of the same message is ignored. A failed delete has its own notice.
- **Message utilities:** More and right-click use the same menu on desktop; long press opens
  the corresponding phone sheet. Conversation actions (reply, thread, forward, edit) come first,
  followed by copy link, pin/unpin, save/remove from saved, copy text and translate; deletion stays last and separate.
  Permissions, local echoes and content type determine availability. Attachment messages start
  with the shared file actions (copy image for images, download and open in the default app);
  they offer no copy-text action for their filename. Empty text has no copy action either.
  Phones use the same content rules in their message sheet, with the system share action too.
  The message menu establishes a modal input boundary, so rows behind it cannot hover or
  reveal their toolbars. Calls keep their own controls.
  `Command::Forward` reads the open timeline's latest event JSON (including edits), strips source
  relations, mentions and legacy reply fallbacks, and sends the text or attachment through the
  chosen room on the same account. The destination picker separates Rooms (`#` and name)
  from Direct messages (avatar and name), as the sidebar does.
  Message links use matrix.to with an account-server routing hint, including v12 room IDs.
  Translate opens Google Translate in the browser targeting the app language, after confirmation
  that its text will be shared with Google. It is offered for text only.
- **Pins:** `RoomSummary::{pinned, can_pin}` come from room state and power levels.
  `Command::SetPinned` uses SDK pin/unpin so existing pins are preserved; server sync updates
  both apps. Pinned rows show a pin label, accent border and subtle background. Room info leads
  to Pinned messages (`SidePanel::Pins` on desktop, a native stack screen on phones).
  `ListPinnedMessages` uses an SDK pinned timeline and waits for its initial background fetch
  (at most five seconds), independently of loaded history; unreadable events retain
  a placeholder with unpin available to authorized members. The list uses normal avatar/message
  rows with sender, excerpt and time; clicking the row opens it and the top-right icon unpins.
  Lists refresh when pin IDs change,
  preserve cached content on failure and offer retry. Opening a pin uses the shared message jump
  to its room or containing thread, loading history
  as needed (desktop `State::open_message`, phone route `eventId`).
- **Saved messages:** a private list of messages per user, offered for every readable sent
  message. Matrix has no ratified feature for it; MSC2437's tagged events are used, under the
  unstable type `org.matrix.msc2437.tagged_events` (tag `m.favourite`, with `tagged_at`), in each
  room's account data: private to the user, following them to every session, gone with a room
  they leave, and other tags and fields are kept. `Command::SetSaved` reads the server's copy and
  writes it back, one change per account at a time (`Account::saved_changes`), so quick changes
  in a row or another session's aren't lost before sync; summaries (`RoomSummary::saved`) follow
  through sync. Two sessions changing the same room's saved messages at the same moment can still
  lose one change (last write wins): room account data has no conditional write, as MSC2437
  itself notes, and a format of our own would give up the shared one. `ListSavedMessages` reads each saved event from the event cache, else fetches it,
  most recently saved first; unreadable ones keep a placeholder. Its text is the event's as sent:
  a later edit shows once the message is opened (fetching every message's relations would cost a
  request each). The account menu opens the list: a dialog on desktop (`Modal::Saved`), a pushed
  screen on phones (`app/saved-messages.tsx`). Rows show sender, room, excerpt and time; one no
  longer saved leaves at once, and one saved meanwhile reloads the open list (checked again when
  a list request finishes; phones run list requests one at a time). Opening a row uses the shared
  message jump; its bookmark button stops saving it. The desktop's rows act on the dialog's
  account, which need not be the one on screen.
- **Message layout:** a device setting (`Settings::message_layout`, `messageLayout` on phones),
  classic rows by default. Both layouts are built from the same plan and the same parts
  (`message_parts`: what a message says, then what hangs off it), so a feature added to a
  message shows in both.
  - Classic rows: the sender's picture, name and time head a run; the row tints on hover.
  - Chat bubbles: other people's messages sit at the left beside their picture (on the first of
    a run) with their name in their identity colour inside the bubble; the user's own sit at the
    right with neither (`Context::on_right`). Only the other person of a one-to-one direct chat
    goes unnamed (`names_sender`, `unnamedSenders`): once someone else is there, or for anyone
    who was and left, names show. The rule reads the summary's `dm_users` and joined count, not
    live membership, so a partner who left while someone else stayed goes unnamed too.
  - Words get a bubble (`bubble`, `bubble_own` palette roles), at most 80% of the content width
    and 600 points on desktop, 85% of the row on phones, hugging short text. A compact desktop
    timeline (content under 320 points) gives bubbles all of it, with 8 points of padding. At
    the smallest window and 175% zoom (about 37 points of content) a time can still spill out,
    as classic rows truncate there. Media and cards
    bring their own frames and stand without one, in the width a classic row gives them, which
    they are sized for, keeping to their message's side (phones: `MessageSideContext`). What a
    bubble holds sizes itself to the bubble (desktop: `message_parts`'s card width; phones:
    `ContentWidthContext`).
  - The time (with "edited" or "sending") closes the bubble in `bubble_meta`, which keeps 4.5:1
    on either bubble, as do muted words inside it (a quote, a notice; phones: `InBubble`); reactions and the thread hang below it, on its side. The readers' column,
    menu and press-to-open-thread are the classic row's; the hover toolbar moves to the row's
    left over the user's own bubbles, which take its usual place (in a timeline too narrow for
    both it may cover a long first line, as it does a classic row's).
- **Reactions, replies, threads:**
  - `Command::React` toggles the user's reaction (`toggle_reaction`). Messages carry each
    reaction's count and whether the user is among them. The picker offers quick reactions,
    then emoji by group.
  - The composer holds the message it answers (`ReplyTarget`); a reply shows its sender and
    excerpt above the body (`Quote`).
  - Clicking a message, or its thread button, opens its thread in the right panel with its own
    composer. A message with replies shows "3 replies · last from …". Clicking the sender's
    name or picture opens their person panel instead
    ([0008](0008-people-and-rooms.md)).
  - The hover toolbar fades in through the row's hover group, so hovering never rebuilds the
    view.
  - Room events and call cards are informational rows: no message toolbar, reactions, replies
    or thread actions. A call card keeps its call-specific controls.
  - Posted formatted messages use selectable read-only text, with inline links, rather than
    text fields. Links show the pointer; surrounding text keeps the text cursor.
    Plain and formatted text boxes fit their widest rendered line instead of filling the row:
    message text keeps selection, while row space outside that box shows the pointer and opens
    the thread. The box reaches 8 pt past the words on each side without moving them
    (padding with a matching negative margin, `message_text::reach`), so a click or drag that
    starts just beside the text selects it rather than opening the thread. 8 pt stays inside
    the gap to the sender's picture and the readers' column, which keep their own presses.
    Links and media keep their own actions. The pinned framework hit-tests the full text
    rectangle, so space following shorter lines within a multiline box stays selectable.
    Per-line hit regions require framework support and are not enabled.
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
  `CallCard`: video, voice or unsaid, one-to-one or not, and who declined): MatrixRTC's ring or
  notification (`m.rtc.notification`, and the unstable `org.matrix.msc4075.rtc.notification`
  Element Call sends, which ruma reads as the same event) and one-to-one invites (video when the
  offer sends video, read from the raw event). Whether a call is under way isn't the card's:
  only the newest card shows it, as matrix-sdk-ui has it: a MatrixRTC call by the room's
  members (`RoomSummary::call`, with the number in it), a one-to-one call by this app's own
  one-to-one call and rings ("Ringing…"). Deleting a call's card while it runs hands the call
  to an older card; that is accepted, as tying cards to sessions would need the session's start,
  which the membership events lose when its first member leaves.
  Join shows where the header would offer it: the room allows calls (`RoomCall::via`) and the
  user isn't in its call here (`State::call_here`); in it, the card says so. Older cards read
  as ended, or declined in a direct chat. Call cards take no reactions, and a reply quotes the
  call's kind (`Quote::call`; a one-to-one call is just "Call" there, its offer not embedded).
  Call memberships stay hidden.
- **Stickers** show as images (`convert::sticker`); sending them is in
  [0007](0007-composer-and-media.md).
- **Emotes** are their own content (`MessageContent::Emote`), drawn in italics after the
  sender's name ("* Ada waves"), formatting and pills kept.
- **Locations** (`MessageContent::Location`): a card with the place's map, name and
  coordinates ([0007](0007-composer-and-media.md)).
- **Edit history:** an edited message's "edited" is a button opening every version, newest
  first (`Modal::EditHistory`, `Command::ListEdits`): the original event and its `m.replace`
  relations (decrypted by the SDK, up to 200), counting only the sender's own edits as the spec
  says. Phones open the same list in a sheet.
- **Message jump:** a link to a message (from a message, the OS or a typed link), a reply's
  quote, a pin, a saved message or a search hit goes to the message. One the timeline on screen
  holds is scrolled to; else the engine first finds where it shows
  (`Command::LocateMessage`: the room's timeline, or the thread its relation names; a link to
  an edit goes to the message it edits, which shows its latest version, as the spec has it), then the
  shared jump opens it there and loads history until it shows (`State::open_message`), unless
  the user has gone to another room or started another jump meanwhile (only the latest
  `LocateMessage` answer counts). The
  message then flashes in the accent colour for 2.4 s (`State::flash`, `FLASH`). Opening at the
  message without the history before it (an event-focused timeline) is not done: the timeline
  model is the room's or a thread's, and paging back keeps one history per timeline.
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
- **Hidden events** (developer tools): every timeline item the app draws nothing for (state it
  has no `RoomChange` for, events that didn't parse, membership events that changed nothing)
  is `EntryKind::Hidden` with its sender, type and event ID, so diff indices stay true. Whether
  they show is a setting on this device ("Show hidden events", off by default; the desktop's
  General settings, the phone's Preferences), not an engine mode: turning it on plans the rows
  again (`Accounts::replan_timelines` on the desktop) and each shows as a small line, "Grace
  sent m.room.server_acl", whose source button (a press on phones) opens its source. Hidden
  entries that don't show don't break a run of joins; ones that show do. Events the SDK's
  timeline can't represent at all (unknown message types, edits and reactions it folds into
  their message) have no entry, so they don't show either.
- **Event source** (developer tools): "View source" in a message's menu (any sent message) and
  on a hidden event's line asks `Command::ViewSource`, answered by `Event::EventSource`: the
  event as this device reads it (from the event cache, else the server; decrypted when it came
  encrypted) and, for an encrypted event, as the server holds it (`get_room_event`, unprocessed),
  each pretty-printed. The desktop shows it in a dialog (`Modal::Source`, selectable, Copy JSON);
  a source that arrives after its dialog closed opens nothing, one that fails closes it with a
  notice. Phones show it in a sheet (`app/event-source.tsx`).
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
- **Read indicators:** `Message::read_receipts` carries the SDK's read positions (user ID and
  optional receipt time), excluding the signed-in account, newest receipts first with stable ID
  ties. SDK tracking uses message-like events, so a receipt on a later state event still marks the
  preceding message. Only supplied positions are shown: no room permission or read is inferred from
  an absent receipt. The desktop shows up to four overlapping avatars (two in a timeline narrower
  than 640 points) and an overflow count, which stops at "99+", at the lower right of a sent
  message. Every room message keeps a column of that width free at its right, filled or not and as
  tall as the indicator, itself no taller than a line of text, so a receipt arriving never rewraps
  or grows its message; the column costs each message some width, as in Element. Pictures and GIFs
  fit the width left (the timeline passes it to its rows), and the hover toolbar sits left of the
  column. A desktop timeline that can't keep 200 points of message beside the column (a thread open
  at a high zoom) drops it, and each indicator takes a line under its message instead, as it did
  before the column. Clicking opens a virtualized reader list, with available receipt dates/times
  and room profiles (falling back to timeline senders or user IDs). The list loads the room members
  as the member panel does; selecting a reader opens that same member menu beside the list. A reader
  outside the joined-member list keeps chat, call and invite actions, without invented moderation
  rights. Only visible reader pictures load. Subscription diffs move or remove indicators and update
  an open list. Thread indicators stay hidden because the pinned SDK's exposed receipt map does not
  distinguish threads, so only room timelines track receipts and the read marker
  (`Account::timeline`): in a thread every receipt in the room would update its items for nothing.
  Thread-specific receipts are also excluded from main-timeline conversion. Readers are looked up by
  index, as the view asks for every reader on screen in each frame it builds: members by
  `AccountData::member` (indexed when a list is set), senders by `RoomTimeline::sender` (indexed
  once per revision). Mobile shows the indicator as a narrow desktop window does, two slightly
  smaller avatars in a column every room message keeps, dropped like the desktop's on a screen that
  can't keep 200dp of message beside it (`ui/timeline/readersLayout.ts`,
  `ui/timeline/ReadIndicator.tsx`), and the reader list as a sheet (`app/readers.tsx`): members load
  with it, a member opens the member sheet, and readers resolve as on the desktop
  (`state/readers.ts`, indexes per immutable list).
- **Read receipts:** the desktop sends `Command::MarkRead` when an open timeline changes while
  the window is focused, and for each open timeline when focus returns.
  - Room reads send both the public `m.read` receipt and the private, cross-device
    `m.fully_read` marker in one request. While a "New" boundary remains unseen, only the
    receipt advances; reaching the boundary advances the fully-read marker too. Thread reads
    use their threaded receipt and never advance the room-wide fully-read marker.
    Once reported, receipts target the actual viewport's last message, excluding the media
    prefetch margin, rather than new arrivals below a user reading older history.
    Scrolling receipts are coalesced at 500 ms per open timeline, with the first fully-read
    acknowledgement sent as soon as the preceding request finishes. Only one receipt request
    per timeline is in flight, including across quick close/reopen; failures retry the latest
    position after two seconds. Warm rows wait for the SDK subscription's first snapshot before
    sending. Viewport progress and visible-marker metadata do not change the row-plan revision;
    dismissing the marker does.
    `ReadBoundary` reports the existing fully-read event (or receipt fallback) independently of
    the rendered marker. Until its event enters applied subscription history or the user reaches
    its marker, an omitted marker remains unresolved and cannot authorize a fully-read update.
    Phones hold the fully-read marker the same way until the "New" row has been in view
    (`markerAcknowledged`), and mark again once it has. Their room list's "Mark as read"
    opens the room's timeline for the request (the engine marks through open timelines) and
    closes it after.
  - The engine re-reads rooms on the SDK's room-info updates (receipts, unread markers), so
    badges follow the counts the SDK works out after sync.
  - The "New" marker is dismissed when its own row enters the viewport, including on open,
    without the media prefetch margin. It fades over a 2.5-second opacity spring, keeping its
    row's full height until the fade ends, then removing the row. The fading row retains its
    acknowledged neighbours independently of the SDK marker, so advancing receipts can move or
    remove that marker without moving the visible divider. A small fade state machine
    uses Rune's logical clock and a scheduled wake (150 ms under reduced motion). Removing the
    row at the start of an exit fade would let the virtual list close its space after 150 ms,
    while the divider still draws over the messages. The desktop
    remembers the first unread entry at that boundary across diffs and warm reopens, while
    keeping the engine's diff indices intact. A new boundary can show a new marker.
    Phones keep the same rule (`apps/mobile/src/state/newMarker.ts`): the divider stays before
    the entry it first stood before, fades over 2.5 s (150 ms under reduced motion) keeping its
    height when receipts move or drop the SDK's marker, and no later divider shows until the
    timeline leaves the screen.
    As in [Element Web](https://github.com/element-hq/element-web/blob/75eaab6ca5ac7ab4bac91a6097c8e77bb469455d/apps/web/src/components/structures/TimelinePanel.tsx),
    the visible indicator is separate from the advancing receipt: once reached it stays hidden
    while reading, including when receipts advance during an upscroll. New boundaries while
    already caught up, focused and at the bottom are acknowledged before rendering. New messages
    received while unfocused can restore the indicator. An unfocused viewport does not dismiss
    it; returning focus acknowledges a marker actually visible there. The app keeps its slow
    fade and viewport-triggered dismissal rather than Element's configurable activity timers.
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
  - Opening a room or thread with an old draft isn't typing, nor is a draft coming back
    ([0007](0007-composer-and-media.md)). Typing belongs to the composer typed in: it stops when
    that composer empties, closes or moves to another thread, whatever the other one holds.
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
