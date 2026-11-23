# 0005. Navigation and settings

Status: Accepted
Date: 2026-11-22

## Context

- Slack's layout is the familiar standard for team chat. Matrix spaces map to its workspaces;
  several accounts side by side are this client's reason to exist.
- Notification modes and the sidebar arrangement must match on every session, so they live on
  the server.
- People differ in the interface size they like; scaling each token by hand would drift.

## Decision

- **Rail:** Home, then the active account's spaces, then "+" to create a space (see
  [0008](0008-people-and-rooms.md)). With many spaces, the plus stays above the user's avatar
  at the bottom. When a newer desktop release is available, its update icon sits directly
  above the avatar, with the plus above it when sticky: Plus → Update → Avatar. The icon's
  tooltip names the version and restart; clicking downloads and installs, disabling the action
  while busy and keeping it available after a failure. On Linux, clicking opens a panel beside
  it with the command that updates the install, to copy (ADR 0001). With no newer release the
  icon is absent.
  Spaces scroll behind them through a blurred transition. With short lists,
  the plus follows the last space. The avatar opens the account menu.
- **Account menu:** what changes often comes first and stays quick to reach: status message,
  then status. Then the accounts, with adding one at their end; then profile and settings.
  Signing out comes last, behind a divider and in the danger colour, since it ends the session.
  Everyday destinations don't belong here: saved messages sit beside the top bar's search, as a
  bookmark button lit while the list shows. The search keeps the bar's middle: a spacer as wide
  as the button balances it. Phones keep saved messages in their account sheet (ADR 0010).
- **Scope:** what the sidebar lists, filtered in one place (`AccountData::rooms_in`).
  - Home: every direct chat, and the rooms in no space.
  - A space: its rooms, subspaces included (cycle-safe), and direct chats with its members.
  - Each account remembers its own scope and open room.
  - The device remembers which account was last used and opens it at launch (ADR 0003).
- **Overview:** the main pane with no room open.
  - Home greets the user; a space shows its size. Then rooms that want attention (mentions
    first) and recent conversations.
  - The app opens on it; choosing a scope, the sidebar header's name, and leaving a room land
    on it. Rooms never open by themselves.
  - Home also lists the active account's invitations, including rooms outside spaces. A
    space's Overview suggests up to five unjoined public rooms, most members first; More opens
    the complete public-room list for that space, with Join on each room (see 0008).
  - When the selected space disappears from the joined list, navigation returns to Home's
    Overview. A removed room or deleted direct chat leaves its timeline for the current
    Overview. Side panels close when the displayed account or room changes.
    Each account may restore its previously opened thread when returning to that account,
    consistent with remembering its selected conversation; ordinary room changes clear it.
  - Invitations, directory joins and notifications share `open_room`: keep the current scope
    when it lists the room, otherwise choose Home or a joined space that lists it. A joined
    room opens with a corresponding sidebar entry, including invitations accepted from another
    space.
  - Changing screen, account, scope or room lands the destination at once. Navigation actions
    use `navigate` in the input transaction; the reducer also settles an event that navigates.
    A deferred poll is too late, because Rune polls before input. Threads and side panels keep
    their motion within the current room.
- **Sections:** separate for Home and for each space.
  - The note to self (ADR 0008) stands above everything in Home, invitations included, with a
    note icon instead of a face, once the user has made one. It is in no section, isn't dragged,
    and its tags don't move it. Spaces never list it, even one the user added it to: it belongs
    to no community.
  - Invitations is a fixed built-in section before all reorderable sections. It appears only
    when invitations are pending, collapses through the shared sidebar layout, and keeps its
    counter while collapsed. Room invites use the same `#` prefix as other rooms.
    All scopes show the account's invitations, including those outside spaces; collapse state
    is remembered separately in Home and each space, like the other built-in sections.
  - Built-in Rooms and Direct messages collapse but can't be removed; latest first. "Latest" is the
    newest of the room's latest event as the store keeps it across restarts
    (`RoomSummary::activity_ms`, from the SDK's latest events, which the engine asks for every
    joined room) and the newest message seen this session (`AccountData::activity`), so a quiet chat
    keeps its place after a restart instead of sinking until it's opened. A time more than five
    minutes ahead of the device's clock doesn't count, so a misdated event (or a server whose clock
    runs fast) can't hold a room at the top: a message heard this session counts from when it was
    heard, and a stored time counts once it has passed (until then the room sorts as unknown). The
    SDK works the latest event out from events it has in memory, so a room with nothing new since it
    was first registered, and never opened, sorts by name at the end until either happens. The
    engine hears the SDK's room updates before reading the list at start, so one worked out
    meanwhile isn't missed; rooms left are dropped from the SDK's tracking.
  - Favourites and Low priority are built in too: the rooms tagged `m.favourite` and
    `m.lowpriority` (`RoomSummary::priority`, `RoomPriority`), latest first, shown once they hold
    something. Tags are room account data every client shares, so these two are what other
    clients see of the arrangement; the user's own sections stay ours. Favourites comes first
    and Low priority last unless the user moved them (an order saved before they existed keeps
    Favourites first). A tagged room shows there even when one of the user's sections holds
    it: the tag may come from another client, and is the newer choice. Dropping a room into
    either sets its tag (`Command::SetRoomPriority`, the SDK takes the other tag away);
    dropping it anywhere else takes the tag away, so it shows where it was dropped. Dropping into
    Favourites or Low priority leaves the user's sections as they are, so a room one of them held
    goes back there when its tag goes. A room
    tagged both counts as a favourite. Tag order is not used.
  - The user's own hold the rooms dropped into them, in drop order. Rename and delete are in
    the section's right-click menu; deleting returns its rooms to the built-in ones.
  - Drag and drop uses `on_drag`; rows report rects via `on_layout_tx` with `live_only`. A press
    that moves no more than 4 points stays a press. Sections reorder but never nest.
- **Sidebar header:** the scope's name, a new direct chat button, and "+" (see
  [0008](0008-people-and-rooms.md)).
- **Desktop pane widths:** the sidebar's right edge and the side panel's or thread's left
  edge are six-point drag handles with the column-resize cursor. Arrow keys adjust a focused
  handle by ten points (Right/Up widens, Left/Down narrows); Home and End choose its limits.
  Escape or losing window focus cancels a drag and restores the previous width.
  - `PaneWidths` keeps preferred widths for the window's session, across rooms and accounts:
    sidebar 260, side panel 320, thread 420 points. The rail stays fixed at 68.
  - The sidebar stays between 220 and 420 points; right-hand panels between 300 and 640.
    `PaneLayout` caps them further to reserve 320 points for the conversation. Opening a
    right-hand panel in a window too narrow for all three temporarily hides the sidebar;
    closing the panel or widening the window restores it. A call can still hide it explicitly.
    The sidebar's chosen width takes priority over the right-hand panel's preferred width,
    so opening a thread leaves the room list steady whenever the minimum widths fit.
    A drag or keyboard adjustment blocked by a limit preserves the remembered width.
  - Layout uses zoomed points from `Cx::layout_size`. Under extreme zoom, panel readability
    takes precedence over the conversation's reserve; no pane exceeds the available width.
    Window resizing never overwrites preferred widths, so widening restores them.
- **Sidebar layout:** a `SidebarLayout` in the account data event `chat.bettermatrix.sidebar`.
  - Per scope: collapsed sections, the user's sections and their rooms, and section order.
  - For the account: the spaces' order on the rail (`SidebarLayout::spaces`). Home stays first;
    spaces dragged on the rail land in place (`Dragged::Space`, `sections::move_space`), and
    spaces the order doesn't name follow by name. It is ours, not MSC3230's per-space
    `space_order`, so other clients keep their own order.
  - The selected rail tile has an accent line over a soft glow that breathes over 5 s while
    the window has focus (`rune::Cycle` on the glow layer only).
  - Read at account start with retries (2 s, doubling to 60 s) and on sync; both arrive as
    `Event::SidebarLayout`. Written with `Command::SetSidebarLayout`.
  - Nothing is saved before the saved layout was read once (`sidebar_loaded`); a change shows a
    notice instead.
  - `changed_at` (ms since the epoch) is always later than the layout it changes.
  - One save per account at a time; changes made meanwhile are saved when it ends.
  - A layout from sync replaces the local one only when no older and no save is pending.
- **Notification modes:** all messages, mentions only, mute; push rules set through the SDK.
  - Each account's default (Settings, Notifications) updates the server's defaults for every
    kind of room.
  - A room sets its own or follows the default (`RoomSummary::custom_notifications`).
  - Keywords: words that notify like a mention wherever they are written (content push rules,
    the SDK's keyword rules), listed under each account's default with add and remove. The
    engine reports them (`Event::Keywords`) at start, when a sync carries `m.push_rules`, and
    after each `Command::SetKeyword` (as the change left them; after a failure, as last synced).
    Each change reads the server's rules first (`GET /pushrules/global/`) and works from them, not
    this device's copy, which a sync begun before an earlier change may have left behind: adding
    creates the rule (named after the word when that is a plain, unused name, else
    `chat.bettermatrix.keyword.<random>`) or enables a disabled one; removing
    deletes every rule of the user's matching the word exactly. Additions are trimmed; removal
    takes the word as listed, since another client may keep spaces around it. New keywords
    notify with the default sound and highlight, so a match counts like a mention (the room's
    mention count, and attention in mentions-only rooms); being content rules, they also notify
    in rooms set to mentions only (in encrypted ones the server can't match them, so phones
    notify only while the app runs, and the hint says so). Words are Matrix glob patterns, as in
    other clients: `*` and `?` match too.
- **Room menu:** Mark as unread (or Mark as read when it is marked or has anything unread, muted
  or not), Add to or
  Remove from favourites, Move to or Remove from low priority; then the Notifications submenu
  (default last), then Leave room, or Delete chat for a direct chat. Both ask for confirmation.
- **Marked unread:** MSC2867's flag (`m.marked_unread` room account data, the SDK's unread
  flag; `RoomSummary::marked_unread`), so every client shows it. `Command::SetUnread` only sets
  or clears the flag; opening a marked room clears it at once, and its read receipt follows as
  the timeline shows messages (so a permalink to older history reads nothing it doesn't show).
  `Command::MarkRoomRead` reads a room from the menu without opening it: a read receipt for the
  latest event the SDK shows (it skips messages it can't decrypt, which are read once the room
  is opened), with the fully-read marker, then the flag cleared; it fails (`Error::NothingToRead`) when the
  room has unread messages but no such event yet. Phones read through their timeline as before.
  The SDK also clears the flag whenever it sends an unthreaded read receipt.
- **Organising writes:** tags, unread flags and keywords go through one ordered queue per
  account (`Account::organising`), queued as commands arrive, and each write sets or removes
  what it asks for whatever this device last synced, so a quick change back lands last. The apps
  show a change at once and, if the server refuses it, take back only that change (desktop
  `RoomUndo`) with a notice.
- **Desktop notifications:**
  - `Event::Notification` only when push rules say so, never for the first sync after start.
  - Skipped when the window is focused on that room.
  - Rune's notifier on macOS and Windows, freedesktop on Linux. A click opens the room,
    switching account and scope.
  - On Linux one session-bus connection (zbus), opened by the first notification, shows them
    all and one thread hears every click and close; the rooms of the last 256 shown are kept
    by notification id. A thread and connection per notification, waiting for its click, piled
    up with the notification history. A failed show closes the connection (ending its
    listener) and the next notification opens a new one; the listener skips messages it can't
    read and ends only with its connection. The action's label is a catalog key.
- **Wants attention:** one rule (`wants_attention`) for sidebar, rail and account menu.
  - Unread messages or mentions, or only mentions for mentions-only and muted rooms; and any
    room marked unread, whatever its mode.
  - The engine re-reads rooms on the SDK's room-info updates, so badges follow its counts.
  - In the sidebar a room that wants attention gets a tinted background (`sidebar_unread`) and
    a bold name; rows carry no counts. A section's header turns bold when any of its rooms
    wants attention and counts their mentions, so both show while it is collapsed.
- **Settings:** a modal in sections (`SettingsSection`). Device settings go to a prefs file off
  the UI thread; account settings live on the homeserver.
  - Every desktop dialog uses the shared modal layer's Rune focus trap and Escape dismissal.
    Attachment previews and selects above a dialog have their own traps; Escape dismisses
    the topmost overlay first. Popover fields (search, new chat and emoji) intercept Escape
    before the text editor consumes it to blur; the window handles Escape for other popovers.
  - The section content uses `instant_changes`: switching sections replaces it immediately,
    without morphing unrelated controls. Each section's content has its own stable key, so
    Rune does not reuse an unrelated control's colour tracks at the same position in the tree:
    `instant_changes` lands layout and presence but deliberately keeps colour animation.
    The modal itself keeps its entrance and exit.
  - Choices with more than a couple of options (language, each account's notifications,
    the ring, devices) are selects: the current choice in a button that opens its list
    (`select_button`, `select_option`, `select_menu`; `Popover::Select(SelectFor)`). The list
    draws above the settings modal. The theme, the message layout, emoticons, link previews
    and GIFs stay as radio rows.
  - General holds the language, link previews and GIFs; Appearance, the theme, the message
    layout ([0006](0006-timelines.md)), emoticons (sent as emoji or as typed,
    [0007](0007-composer-and-media.md)) and the interface size. Phones list the theme and the
    message layout under Preferences.
  - Voice & video picks the microphone, speaker and camera by name, for calls and voice
    messages alike (`Settings::devices`, `State::choose_device`); the system's default when
    none is picked or the picked one is gone. The devices list when the section opens
    (`Command::ListCallDevices`). Saved picks reach the engine at launch.
  - Notifications starts with the ring: aloud (a soft chime on the picked speaker until
    answered, declined or lapsed) or silent (`Settings::ring_sound`), above each account's
    defaults.
  - The section list scrolls when a short window at a large interface size can't show it all.
- **Info:** the last settings section, set apart at the bottom of the list, shows the app's version
  and the third-party list: every crate the desktop app ships (normal and build dependencies on
  every platform, ours left out) and bundled files such as the Inter font, each with version and
  license.
  - `bun run third-party` writes it to `apps/desktop/assets/third-party.tsv` from
    `cargo metadata`; the binary embeds it (`third_party::all`). Bundled files without a crate
    are listed by hand in the script (`BUNDLED`).
  - `bun run lint` runs `third-party:check`, so a dependency change that isn't listed fails.
  - The list stays behind a button and is virtualized when shown: it runs to hundreds of rows.
- **Interface size:** `Cx::zoom(settings.zoom.factor())`, Rune's page zoom, on every run.
  - `Zoom` steps through 67, 75, 80, 90, 100, 110, 125, 150, 175%; 100% is the default.
  - Settings, Appearance has − / + and Reset; no shortcuts.
- **Dev runs:** `turbo.json` passes the desktop session through (`globalPassThroughEnv`:
  display, `XDG_*`, D-Bus, locale, cursor), so the app runs as a native Wayland client rather
  than being scaled twice under XWayland.

## Consequences

- Filtering, attention and layout rules are pure desktop state, tested without a window.
- No client-side notification rules to keep in sync.
- Very large spaces make member lists heavy.
- macOS notifications need a bundled app.
- Concurrent edits resolve by `changed_at`: the later layout wins whole.
- A session variable dev runs need must join `globalPassThroughEnv`.
- `Cx::layout_size` is in zoomed points, so window shares hold at every zoom.
