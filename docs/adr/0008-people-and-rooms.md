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

## Decision

- **Profile:** Account menu → Profile opens Settings → Profile for every account.
  - It sets the picture and display name through the client API.
  - Contact details follow the server's `m.3pid_changes` capability (`Contacts::changeable`),
    not the sign-in method: where it allows changes, the add and remove form shows; where not,
    the linked details show read-only (kinds with none are left out), and an OAuth account gets
    a link to its account page (`Command::OpenAccountPage`, answered by `Event::OpenUrl`).
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
  - The top bar matches rooms, direct chats and spaces on this device when every typed word
    starts a word of the name (`state::matches`), and people in the user directory
    (`Command::SearchPeople`).
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
  Members, Threads, Files}` (`State::side`; opening a thread closes it, opening it closes the
  thread). Each has a title and a close button, no back arrow.
  - Info (the header's info button): the room's picture, name and topic, rows leading to the
    members, threads and files, and leave room (or delete chat).
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
    `SpaceSummary::can_add_rooms`, or a room or space from Home.
  - Right-clicking a space on the rail offers rename (`can_rename`) and leave. Matrix can't
    delete a space for everyone.
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
