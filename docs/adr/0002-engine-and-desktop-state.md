# 0002. Engine and desktop state

Status: Accepted
Date: 2026-11-14

## Context

- UIs must never block on the network, several accounts run at once, and a later mobile app
  must reuse all Matrix logic.
- Engine events arrive on background threads, user actions in UI handlers, and some results
  need the OS. All of it must be testable without a window.
- Answers can arrive late, after their timeline closed or the user moved on. They must not land
  on whatever is on screen then.

## Decision

- **The engine is an actor:**
  - `matrix-core` runs it on its own Tokio runtime and thread; one loop owns its state.
  - Apps send `Command`s through a cloneable `EngineHandle` and receive `Event`s through an
    `EventSink` they implement. No SDK type crosses; apps see `model` values only.
  - `EngineHandle::detached()` gives a handle with no engine, for headless UIs and tests.
- **Every command is a task:**
  - It gets a `RequestId`, ends with exactly one `Event::Finished`, and `Command::Cancel`
    stops it.
  - Tasks report events directly; changes to the engine's own state return to the loop as an
    `Outcome`.
  - Content grows by variant, not by command: `Outgoing` for what is sent, `MessageContent` for
    what arrives.
  - A feature is an operation on `Account`, a `Command` variant, the `Event`s it reports, and
    tests for each.
- **Account start runs beside the loop:**
  - The sign-in task reads `Account::cached_summary` (store only, no requests). The engine emits
    it as `AccountAdded` before answering, so the app can select the account it is adding.
  - The server profile follows as `AccountChanged` (`Account::refreshed_summary`), keeping the
    cached name or picture if the request fails.
  - The sync starts at once. The sidebar layout is read with retries (2 s, doubling to 60 s).
- **Opening a timeline is a two-step handshake:**
  - `open_timeline` builds (or takes a warm timeline) and subscribes, then hands it back unseen.
  - The loop shows it with `start_timeline` (reset, forwarding, first page) only if
    `Running::opening` still names that request.
  - An open that finishes after its close, or after a newer open, parks it in the warm set
    (`WARM_TIMELINES` = 8 per account).
- **Two syncs where the server has sliding sync** (MSC4186, `org.matrix.simplified_msc3575` in
  `/versions`, asked again after every failed sync):
  - Rooms come through the SDK's `RoomListService` (the room list sync): the newest rooms first,
    then the rest a batch at a time, each with its last 10 events (`ROOM_LIST_EVENTS`, as many
    as a long-poll sync's, so a burst such as a call invite and a message arrives whole for the
    call handlers and notifications) and the state the list needs, its position kept in the
    store. A big account shows its rooms at once instead of waiting for a whole long-poll
    initial sync. The rooms open timelines show are subscribed for their recent events and
    full state (`Account::follow_room`; the SDK replaces the whole set at once, so the account
    keeps it, and a new room list sync after a failure subscribes it again); a room is let go
    when its last timeline closes, the set changing at once so a later open follows it again
    (and an open that finishes after its close lets it go).
  - Beside it a long-poll sync that asks for no rooms (`SyncedRooms::None`) carries what sliding
    sync can't: to-device messages and device keys under the crypto store's lock, presence
    (MSC4186 has no presence; this is why it isn't the SDK's `SyncService`, whose encryption
    sync would leave none), and account data. Both announce the same presence
    (`Client::set_presence` without a request).
  - Both feed the same paths: the room updates the SDK broadcasts go through
    `report_room_updates` and the `RoomFeed`. Notifications come from the SDK's notification
    handler, only for events sent after syncing started and once each (`Notified`): the list
    reaching a room late, or a subscription, brings events again. An encrypted one whose key is
    still on its way through the other sync is decrypted again for a few seconds, beside the
    sync (which waits for each handler), its push rules worked out for what it says
    (`decrypted_soon`). Known gap: in a room set to mentions only, the SDK drops an unreadable
    message before the handler, so a mention whose key comes after it doesn't notify.
  - Known gap: a server that names sliding sync and then refuses it, after the room-less sync
    moved past a removal from a room, may keep that room listed until the app starts again.
  - A sliding sync that starts afresh (the first after an upgrade, or once the server forgot
    its position) lists only rooms joined now, never ones left meanwhile on another device:
    as it starts, rooms this device held as joined that the server's `/joined_rooms` no longer
    lists are marked left (`leave_rooms_left_elsewhere`; only rooms held as joined before asking
    and still when marked, and none a sync reported as joined, or whose membership changed,
    while asking, so a rejoin or an invitation back keeps it; asked again with backoff until
    the server answers). Known gap: a rejoin elsewhere applied in the instant between that check
    and the write is undone until the room's next update.
  - The first failure of either ends both; `sync_loop` waits and starts again. A server that
    names sliding sync but refuses the endpoint (`M_UNRECOGNIZED`, 404: a proxy in front, a
    rollout half done) gets the long-poll sync for every room until the app starts again; its
    first response brings every room's whole state (`full_state`), as the room-less sync before
    moved the token past changes sliding sync never delivered.
  - Subscription updates run one at a time, each reading the set when its turn comes, so the
    newest set is applied last; an open that fails lets its room go.
  - Elsewhere one long-poll sync brings everything, as before.
- **One room feed per account:**
  - Every re-read of rooms goes through the account's `RoomFeed`: the sync, the unread listener,
    the space-member fetch, and commands that change rooms (rename, leave, notification modes).
  - It holds its lock until the list is reported, so the list reported last is the newest.
  - It re-reads only what may have changed and reports only a changed list
    (`Account::update_room_list` returns `None` otherwise): a sync re-reads the rooms its update
    touched beyond typing notices (`summary_may_change`), and every room only for `m.direct`
    and `m.push_rules`, the account data summaries read. Names and heroes the SDK works out
    from a room's summary arrive through the unread listener's notable updates.
  - Every room is read once when the sync starts. The unread listener lives as long as the
    sync loop, through failed syncs and the waits between them, so a retry re-reads nothing.
- **Idle costs nothing on the desktop:** the engine's runtime has at most four workers
  (`WORKERS`), and nothing polls: both syncs long-poll, and a store shared with no other process
  takes no cross-process locks ([0010](0010-mobile.md)).
  - Spaces without loaded members are queued from the store at start, from every sync, and
    after a failed sync. A failed member fetch (a space that forbids it, a server error) is
    tried again when its space is next queued, but not before a wait: 30 s, doubling with each
    failure in a row to an hour, forgotten on success (`Backoff`).
- **One reducer on the desktop:**
  - `State::apply(Event, &mut Tx)` is the only way Matrix data enters the app. The sink calls
    `AppHandle::update`, so it runs on the UI thread.
  - User actions are methods on `State`; they send commands through `dispatch(Command, Purpose)`,
    and the `Purpose` routes the answer back to its owner.
  - `Accounts` is a pure client-side store; `Nav`, the composers, the sign-in form and the media
    cache are separate parts of `State`.
  - Modes are explicit state machines (`SignInStep`), not combinations of flags.
- **Answers carry their origin:**
  - `MediaLoaded` carries its request ID.
  - `Purpose::DirectChat` and `Purpose::Create` carry the account; a result for an account no
    longer shown doesn't navigate.
- **Closed timelines:** the desktop keeps the entries of the last 8 closed timelines per account
  (`KEPT_TIMELINES`, as many as the engine keeps warm). A room's member list goes with the last
  of its timelines kept. Answers for a timeline no longer kept (history, read boundary) change
  nothing (`Accounts::kept_timeline_mut`); only opening one creates it.
- **Derived data:** what views read on every commit is worked out once per change, not per
  commit: an account's room lookups and the rooms each scope lists (`RoomIndex`, rebuilt on
  first use after `set_room_list`), the filtered member list (by `members_revision`), the
  stored form of a draft (by a fingerprint of its field), and the call's watched videos (by
  `Call::videos_key`). Viewport bookkeeping (read position) doesn't commit the view.
- **Overlays:**
  - One `Popover` slot holds every floating panel, drawn over a click-outside scrim. A menu a
    right-click opens at the pointer (`components::at_point`) is anchored to a marker there
    (Rune's `El::anchored`): it opens down and to the right, up or to the left where the window
    has no room, and shifts to stay inside the window (left by the menu's width, which it is
    given), so no action is cut off at an edge.
  - One `Modal` slot holds every dialog over a dimmed backdrop; settings fill 90% of the window.
    A dialog (`components::dialog`) stays inside the window: its title takes two lines at most,
    and in a short window its content scrolls and its buttons stay in view. Long text a dialog
    is about (a poll's question) goes in the content, not the title.
  - `Modal::Confirm` guards anything that can't be undone and runs its action only through
    `State::confirm`.
  - Escape or a click outside closes either. A new kind is a new variant.
- **The OS goes through `platform::Shell`:**
  - It does notifications, the taskbar (`platform::Taskbar`: the badge, the
    flash and the jump list, passed to Rune only when they change) and saving `Settings` and
    the rooms opened (`rune::prefs`, on a worker thread, only the latest of each); opening a URL
    goes through `Tx`. File dialogs also use `Tx`, with options and response routing in
    `platform::files`. Headless runtimes can answer them without native UI. On Linux, a portal
    failure can fall back to rfd and Zenity on a worker.
  - `Shell::headless()` and `Shell::recording()` replace it in tests.
  - Played audio and voice memos go to a private scratch folder under the cache, emptied at
    launch and exit.
- **Headless runs** (`--snap`, `--tree`, scripts) use `State::detached`: no engine, no keychain,
  no saved settings, so a render never changes the user's data.
- **Views** compose shared components (`ui/components.rs`) and read semantic colours from
  `ui/theme.rs`.

## Consequences

- A slow sign-in never stalls a send on another account; one mechanism tracks every command.
- The boundary is a natural FFI seam for mobile (UniFFI or similar).
- Every behaviour is testable by feeding events and actions to `State`, or by driving
  `rune::Runtime` headless. Demo fixtures (`demo.rs`) go through `apply`.
- An open whose answer is discarded costs a build that stays warm, so switching back is free.
- Watch: new code that re-reads rooms must use `RoomFeed`, and a new result that navigates must
  carry its account in its `Purpose`.
