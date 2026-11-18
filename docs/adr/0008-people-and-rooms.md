# 0008. People and rooms

Status: Accepted
Date: 2026-11-14

## Context

- An OAuth homeserver manages email, password and sessions on its own account page. Its
  homeserver then refuses contact-detail changes through the client-server API (Synapse with
  MAS: `m.3pid_changes` off, the endpoints gone); MAS's own API for them isn't a stable,
  client-facing one. Name and picture still change through the client-server API everywhere.
- Rooms can have thousands of members, and the user directory is a server request. Neither may
  be loaded or asked for all at once.
- People share rooms, people and messages as links: `https://matrix.to/#/…` (what most clients
  write) and `matrix:` URIs (MSC2312, in the spec since v1.2). A link names a room by its ID or
  an alias, often one the account hasn't joined, sometimes on a server the account's own
  doesn't know (`?via=`). The phones must route a link before the engine could answer.

## Decision

- **Profile:** Account menu → Profile opens Settings → Profile for every account.
  - It sets the picture and display name through the client API.
  - Contact details follow the server's `m.3pid_changes` capability (`Contacts::changeable`),
    not the sign-in method: where it allows changes, the add and remove form shows; where not,
    the linked details show read-only (kinds with none are left out), and an OAuth account gets
    a link to its account page (`Command::OpenAccountPage`, answered by `Event::OpenUrl`).
- **Status:** the account menu starts with the user's status for the account on screen: Online,
  Away, or Hidden (shown to others as offline); a dot on the account's avatar shows it. Do not
  disturb is left out: Matrix presence has only `online`, `unavailable` and `offline`, and
  MSC3026's `busy` is not in the spec.
  - Saved in the account data event `chat.bettermatrix.status`, not on the device: presence
    belongs to the user, so every session of this app announces the same one. Other clients
    still announce online while they sync.
  - Every sync sends it as `set_presence`, read from the store before each request. Until the
    first sync has caught up, every sync sends `offline`, which marks no one: the stored status
    may be stale, and one set elsewhere meanwhile could be Hidden. With none ever set, Online.
  - A sync as `offline` leaves the session's presence as it was, so hiding needs an explicit
    `PUT /presence` on every session: `Command::SetStatus` writes the account data and then
    announces it, and every session announces a status that a sync brings. A sync that raced a
    change brings that change back, so its announcement lands after the stale sync. Announcing
    keeps the status message another client set, and sends nothing when it can't read it.
    Changes to one account save and announce one at a time (`Account::status_writes`).
  - A missed announcement (a failed request, the app quitting between a sync and its
    announcement) is not retried: syncs carry Online and Away, and Synapse takes a session
    offline 30 s after its last online or away sync (`presence_sync_online_timeout`), so Hidden
    still holds, only later.
  - `Event::Status` reports it at start, after a change here, and when a sync brings one.
  - The apps show the choice at once and, if saving fails, show the status last saved with a
    notice, unless another was chosen or reported since.
  - Synapse turns Away (`unavailable`) back into online when the user sends a message, types or
    reads; the next sync or change sets it again. Hidden is not affected. Servers with presence
    switched off accept the status but show nobody's.
- **Contact details** step through `ContactStep` per kind:
  1. `Entering` the address.
  2. `Requesting`: `RequestContactCode` sends a link or code; `Event::ContactCodeSent` carries
     the opaque `PendingContact`.
  3. `Sent`: the user confirms the email link, or the phone code goes to `submit_url`
     (`ConfirmContactCode`).
  4. `AddContact` links it. It is a guarded action.
  - A failure stays on its step. Phone numbers without `+` read from the system's region.
- **Pictures:** profile, member, room and space pictures are 128 px thumbnails (`AVATAR_SIZE`)
  in the shared media cache, drawn by `media_avatar`/`account_avatar` with initials as fallback.
- **File dialog:** `Shell::pick_files(PickFor)` and `State::files_picked` serve attachments and
  the profile picture. A new use is a `PickFor` variant.
- **Search:**
  - The desktop opens search on a field press or the in-application `mod+k` shortcut (Ctrl+K,
    Command+K on macOS). Filters for Spaces, Rooms, People and Messages stay above the scrolling
    results, including before typing. No filter searches all kinds in separate sections; one
    selected filter limits both results and requests. Selecting it again or removing its field
    chip clears it. Rooms includes direct chats. Emptying the query keeps the filter row open.
  - `Command::SearchDirectory` → `Event::DirectoryFound` searches the active account's homeserver
    public directory by name, alias and topic. Rooms and spaces use separate type-filtered
    requests so popular rooms cannot crowd out spaces; each returns at most 20 destinations
    (at most six pages, with repeated-token protection). The UI omits joined destinations from
    the public sections. Selecting a result opens the existing room preview and Join flow for
    this account, using its homeserver as a `via` server, without joining on selection.
    Each source runs independently after the 300 ms pause, reports failures in its section,
    and ignores obsolete responses. Changing query, filter or account cancels old requests.
  - A typed link or address comes first (Typed links, below), then the top bar matches rooms,
    direct chats and spaces on this device with case-insensitive fuzzy matching, and people in
    the user directory (`Command::SearchPeople`). Each kind has its own section (up to six
    entries), followed by Messages. Names and excerpts highlight the matched characters, as in
    fzf; consecutive characters and word starts rank ahead of scattered matches when usage ties.
  - **Internal ranking:** within each section, frequently used rooms and people come first.
    `SearchUsage` belongs to one account on this device, loaded from the SDK's custom state
    store at startup and reported as `Event::SearchUsage`. An actual main-timeline open adds
    one to its room and direct-chat peers (`Command::VisitRoom`, sent when a main timeline is
    shown); background subscriptions such as Mark as read never count. An accepted send adds three. Explicit replies also
    count for their original sender, looked up only in the event cache. Repeated commands for
    an already shown room and thread opens do not count as another room visit.
    A per-account worker owns the counters, accepts interactions through a channel and
    serializes persistence; snapshot requests go through the same queue, so searches see all
    earlier interactions and await initialization without holding up the engine actor. No usage goes to the homeserver. Counts begin learning with
    this feature and survive app restarts. If saved usage cannot be read, that session keeps
    new counts in memory and skips writes, preserving the saved data for the next launch.
    Message preference is the sum of room and sender counts, before fuzzy quality and recency,
    applied before retaining the best 30 hits. Room and people preference is also applied
    before the six-entry UI limit (and before the directory's 20-entry limit). A typed full
    user ID stays first; frequency ties keep existing relevance/federation order.
  - `Command::SearchMessages` → `Event::MessagesFound` searches only the active account's joined
    rooms in the SDK's persistent event cache, including closed rooms and thread-only events.
    It never requests history or uses server search, and stores no extra plaintext index.
    Parsing and matching run on the blocking pool, outside the UI and engine actor. The cache
    lock is released per room before matching. Edits replace original text, redactions remove
    it, and undecrypted events are excluded. At most 30 hits, by usage then match quality and recency,
    carry a bounded excerpt and Unicode scalar offsets for both renderers.
  - Local message searches share the field's 300 ms pause but run independently of the user
    directory; a slow or failed directory doesn't delay local results. Changing text cancels
    the previous message request and clears its hits; an obsolete answer or an answer for an
    account no longer shown is ignored. Failures show in the search and typing again retries.
    Selecting a message opens its room or thread and scrolls to its event, loading older
    history if necessary.
  - `PeopleSearch` serves the top bar, new-chat search and invite dialog. It sends from `poll`
    once the text rests 300 ms (`SEARCH_PAUSE`) and keeps only the latest answer; changing the
    text drops the search in flight. Emptying the text clears results at once.
  - Before typing, the new-chat search suggests people the user shares rooms with
    (`Command::SuggestPeople`).
  - Federation: a homeserver's directory knows its own users and those it has seen in shared or
    public rooms, never everyone. Results put people on the account's homeserver first, keeping
    the directory's order within each group (`people::rank`); 50 are asked for and 20 shown, so
    other servers can't crowd them out. Suggestions likewise rank the homeserver first, then by
    rooms shared.
  - A full user ID typed from any server is offered first, with its profile fetched from that
    server, even when the directory doesn't know it or fails; the invite dialog, new-chat search
    and top bar all get it this way.
- **Side panel:** beside the open room, in the thread's place, one of `SidePanel::{Info,
  Members, Threads, Files, Pins}` (`State::side`; opening a thread closes it, opening it closes the
  thread). Each has a title and a close button, no back arrow.
  - Info (the header's info button): the room's picture, name and topic, rows leading to the
    members, pinned messages, threads and files, and leave room (or delete chat).
  - Threads: the server's thread list (`Command::ListThreads`, `Account::threads`); a press
    opens the thread. Files: messages with media, newest first (`Command::ListFiles`), the
    server filtering by URL in the clear, the device searching six pages in encrypted rooms.
    An open Threads or Files panel follows the room shown (`State::sync_side`, once per room);
    a list that fails to load shows as empty, with a notice.
- **Members:**
  - The room header shows the name, up to three faces and the member count (opening the member
    list), and an info button.
  - A complete loaded member list supplies the displayed count; a partial list is a lower
    bound on the sync summary's count. The header, member list and Info use the same rule.
    Sync membership/power-level changes and changed summary counts invalidate cached snapshots
    and refresh those already requested. Stale rows stay visible during refresh so search and
    scroll position survive; fresh nonzero sync counts take precedence while a list is stale.
    Responses from requests invalidated in the meantime are ignored, including when a join and
    leave leave the total count unchanged.
    Superseded requests are cancelled. Only a room currently shown (or in its invite dialog)
    refreshes immediately; background snapshots stay stale until the room is opened again.
    Restoring an account's conversation also refreshes missing or stale member data. Equivalent
    requests already in flight are reused, including the room-selection path.
    One-to-one direct chats omit the People link from Info; group conversations keep it.
  - Opening a room lists only members the store knows (`Command::ListMembers`, `all: false`).
    The member list and invite dialog fetch everyone unless `Members::complete`.
  - The member list (`ui::members`): a search, the count, then everyone (`RoomMember`) with a
    presence dot (from the store's presence events; none when the server shares none), a badge
    when the user verified them (looked up for rooms of up to 1000), and their role (Admin at
    power level 100 or a v12 creator, Moderator at 50, by the level itself, not scaled to the
    room's top level). Order: online, away, the rest; then by
    role; then by name. Virtualized; pictures load for its first 60 (`MEMBER_PICTURES`). The
    search is applied once per build (the rows look members up by position), and the engine
    reads presence and verification for 32 members at a time.
  - An invite button sits beside the search when the power level allows (`Members::can_invite`).
  - Pressing a member opens their menu: start chat, call and video call (through the direct
    chat with them alone, never a group chat they're in nor one a third person joined, where a
    call would ring them too (`AccountData::direct_chat_with`, `Account::direct_chat`),
    started first if there is none; offered unless that chat allows no
    calls), invite to another group room, and with the rights (`Members::may`: the room's
    power levels allow it and the user ranks above them) change role, remove (kick) and ban.
    Kick, ban and raising someone to the user's own level (which equals can't undo) ask first
    (`Confirm::Moderate`). Member rows take keyboard focus; all go through
    `Command::ModerateMember(Moderation)`, which waits (up to 10 s) for sync to bring the change
    into the store, then lists the members again.
- **Inviting:** a dialog searches people, a typed user ID among them (`Command::Invite`).
- **Received invitations:** `RoomList::invitations` carries invited rooms and spaces, their
  stripped name/topic/picture, inviter and receiving account. The fixed Invitations section
  and Home cards open a preview with Accept and Decline. Accept uses `Command::JoinRoom`;
  Decline leaves without forgetting the room. Buttons wait for completion and failures keep
  the invite available to retry. Joining refreshes the room feed before navigation, and late
  completion does not move another account or room.
  Phones show the same fixed Invitations section first in Home's room list and open a sheet
  with the same content and buttons (`app/invitation.tsx`, `state/actions/invitations.ts`).
  Accepting brings the invited account on screen and opens the room, or shows a space as its
  scope on Home (`openJoined`, as links do); an answer that ends after the sheet was left
  navigates nowhere (`afterAnswer`).
- **Public rooms in a space:** `Command::ListPublicRooms` reads all pages of the space hierarchy,
  including nested spaces, retains only public rooms and ranks by joined-member count. The
  space itself, subspaces and invite-only rooms are excluded; the UI also omits joined rooms.
  Results and errors stay with their account and space; failures offer retry. The full list is
  virtualized. Joins use the `via` servers advertised in space-child links for federation,
  preserving advertised order and dropping duplicates,
  falling back to the space's server, the room's server, or this account's server if neither ID
  names one. A repeated pagination token is reported as a server pagination error.
- **Direct chats:** `Command::OpenDirectChat` opens the existing chat or starts one.
- **Making rooms** (`Command::CreateRoom`, `CreateSpace`, `RenameRoom`):
  - Rooms are private and encrypted. One made in a space is linked both ways.
  - The sidebar header's "+" offers a new section, a room in a space where
    `SpaceSummary::can_add_rooms`, or a room from Home.
  - On desktop, the rail's "+" after the last space opens the new-space form from any scope,
    including when the account has no spaces yet. It requires an active signed-in account.
    Creating a top-level space has no existing space's power-level requirement, and Matrix's
    [capabilities API](https://spec.matrix.org/latest/client-server-api/#capabilities-negotiation)
    exposes no creation permission. Homeserver restrictions are enforced by `CreateSpace` and
    reported through the usual failure notice; `can_add_rooms` only governs linking rooms to
    an existing space.
  - Right-clicking a space on the rail offers rename (`can_rename`) and leave. Matrix can't
    delete a space for everyone.
- **Permalinks** (`matrix_core::Permalink`, `permalinks.rs`) read matrix.to links (encoded or
  not, `#/@user`, `#/#alias`, `#/!room`, `#/!room/$event`, `?via=`) and `matrix:` URIs
  (`u/`, `r/`, `roomid/`, `/e/`, `?via=`, `?action=join|chat`). IDs are checked loosely (a sigil,
  no spaces, a server where the kind needs one: v12 room IDs have none). The mobile app has the
  same parser in TypeScript (`apps/mobile/src/state/permalinks.ts`); both run the shared vectors
  in `packages/matrix-core/src/permalinks.json`, so a link reads the same in every app. A new
  case is a new vector.
- **Typed links:** search fields also read what is pasted or typed as a link
  (`Permalink::parse_typed`, `parseTyped` in TypeScript, vectors in `typed_links.json`): a
  permalink, a matrix.to link without its scheme, or a bare room alias, room ID or user ID. A
  name never reads as one (an alias or user ID needs its server, and nothing may hold a space).
  The desktop's top bar and the phones' search list it first ("Open #rust:matrix.org") and open
  it on Enter (or the search key) as any permalink; phones hand it to the link screen as a
  `matrix:` URI (`permalinkUri`).
- **Where a link goes:** a room ID an account has joined opens at once (the account the link was
  clicked in first, then the one on screen, then any); a space opens as that account's scope,
  as it does when one is joined from its preview. Anything else looks the room up for one
  account (`Command::PreviewRoom` → `Event::RoomPreview`, the SDK's `get_room_preview`: the
  summary API, then the directory, then world-readable state) and shows a **room preview**:
  picture, name, alias, members, topic, and Join when `RoomPreview::joinable` (invited, public,
  or restricted, which only the server can check). A preview that names a room some account
  joined opens it instead, unless the user picked the account. A room no server describes
  still offers Join. Knocking isn't offered (the preview says so), and `?action=join|chat` never
  acts by itself: a link can't make the user join or start a chat without a press.
- **Joining** (`Command::JoinRoom`, by the previewed room ID or the link's alias, through the
  link's `via` servers) waits for the account's room feed to list the room before
  `Event::RoomJoined`, so the room the app then opens is in its list.
- **People:** a link to a person shows their profile (`Command::SearchPeople` with the full ID,
  which fetches it from their server) and Start chat (`OpenDirectChat`).
- **Several accounts:** the preview acts for one account; the user can switch it ("Open as"),
  which looks the room or person up again.
- **Permalinks on the desktop:** `State::open_permalink` handles every matrix.to link and
  `matrix:` URI, from a message (`open_link`), from the OS (`App::on_open_url`, the launch
  handoff of [0001](0001-stack-and-tooling.md)) or from launch. A room ID one of the accounts
  has joined opens at once, preferring the active account. The desktop has no jump to a
  message, so a link to one opens its room. Anything else opens `Modal::Link`, stepping through
  `LinkStep`: `Previewing`, then `Preview` (or `NoPreview`, which still offers Join), `Joining`,
  or `Person`; a failed join returns to the preview with a notice. A link that comes before any
  account is ready waits for the first room list.
- **Permalinks on phones** are routed before the router ([0010](0010-mobile.md)) and open the
  link screen (`app/link.tsx`), the same steps as the desktop's dialog in a sheet; a joined
  room opens at the linked message. The account can't change while a join or chat is under
  way, and one that ends after the sheet was left navigates nowhere (`useShown`).
- **Shared layout:** every pane header is `HEADER_HEIGHT` (56) tall so dividers line up; a
  short timeline sits by the composer; a thread summary shows the last replier, "N replies" and
  "Last reply today at …".

## Consequences

- A server that checks phone codes itself (no `submit_url`) can't finish adding a number; it
  fails with `Error::NoCodeSubmission`.
- Password change and deactivation aren't in the app: the account page on OAuth, future guarded
  actions on password accounts.
- Opening a room makes no member request. A large room's full list is fetched once, on demand.
- New pictures should go through `media_avatar` and `request_avatars` and stay on-screen only.
- A room only an alias names can't be checked against the room lists (summaries carry no
  aliases), so it always costs a preview request before it opens, even when joined.
- A private room's link shows no preview: its members see it, nobody else (by design).
- Watch: a change to how links read goes into both parsers and the shared vectors together.
