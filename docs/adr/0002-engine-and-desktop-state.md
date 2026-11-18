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
  (`WORKERS`), and nothing polls: syncs long-poll, and a store shared with no other process
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
  - One `Popover` slot holds every floating panel, drawn over a click-outside scrim.
  - One `Modal` slot holds every dialog over a dimmed backdrop; settings fill 90% of the window.
    A dialog (`components::dialog`) stays inside the window: its title takes two lines at most,
    and in a short window its content scrolls and its buttons stay in view. Long text a dialog
    is about (a poll's question) goes in the content, not the title.
  - `Modal::Confirm` guards anything that can't be undone and runs its action only through
    `State::confirm`.
  - Escape or a click outside closes either. A new kind is a new variant.
- **The OS goes through `platform::Shell`:**
  - It does file picking, notifications and saving `Settings` (`rune::prefs`, on a worker
    thread); opening a URL goes through `Tx`.
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
