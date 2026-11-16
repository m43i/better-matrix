# Glossary

The words this project uses, in code, docs and conversation. Use these names; when a new concept
needs one, add it here.

## Product

**Account**
: One signed-in Matrix user (`@alice:example.org`) on one homeserver. Several run side by side;
  each has its own client, store and sync loop. `AccountId` in code.

**Account menu**
: The popover opened from the signed-in user's avatar at the bottom of the rail: switch account,
  add an account, settings, sign out.

**Rail**
: The narrow column on the far left: Home and the active account's spaces in the user's order,
  then the account avatar at the bottom.

**Scope**
: What the sidebar lists: **Home** or one **space**. Each account remembers its own.

**Home**
: The scope with every direct chat and the rooms that belong to no space.

**Space**
: A Matrix room that groups other rooms and subspaces. Its scope lists its rooms (subspaces
  included) and the direct chats with its members.

**Room**
: A group conversation. Listed under "Rooms".

**Direct chat** (DM)
: A conversation with one or a few people, marked direct by the account. Listed under "Direct
  messages".

**Overview**
: The main pane when no room is open: the scope's greeting or size, its unread rooms and recent
  conversations.

**Invitations**
: The fixed, collapsible section at the top of the sidebar for rooms and spaces the account
  was invited to. Its counter stays while collapsed. Home also lists invitations; each opens
  a preview with Accept and Decline.

**Public rooms**
: Rooms anyone may join, discovered through a space's hierarchy. Its Overview suggests the
  unjoined ones with the most members; More shows the full list.

**Sidebar**
: The column listing the rooms and direct chats of the current scope.

**Timeline**
: A room's messages, oldest first, with date dividers, the "New" marker and the start of visible
  history. Followed only for the room on screen; changes arrive as diffs. A **timeline item** is
  one message with its formatting pre-parsed. See [ADR 0006](adr/0006-timelines.md).

**Timeline key**
: Which timeline: a room's main one, or one thread in it (`TimelineKey`).

**Thread**
: Replies to one message (its root), shown in the panel on the right with its own composer.

**Reply**
: A message answering another, shown with the answered message quoted above it (`Quote`).

**Reaction**
: An emoji added to a message; the same emoji from several people is counted on one chip.

**Hover toolbar**
: React, reply and reply-in-thread buttons at a message's top right, shown while it is hovered.

**Section**
: A group in the sidebar: the built-in Rooms and Direct messages, or one of the user's own,
  per Home or space. See [ADR 0005](adr/0005-navigation-and-settings.md).

**Voice memo**
: A recording sent as a Matrix voice message (`Outgoing::Voice`).

**Room event**
: Something that happened to a room rather than something written: a join, an invite, a new
  name (`RoomEvent`, `RoomChange`). Shown as a small line; joins and leaves in a row merge. See
  [ADR 0006](adr/0006-timelines.md).

**Attachment preview**
: An image or PDF shown full size over the window, with download and close (`State::preview`).
  See [ADR 0007](adr/0007-composer-and-media.md).

**Upload**
: A file to send: a path on disk or bytes in memory, such as a pasted screenshot (`Upload`).

**Warm timeline**
: A timeline closed lately and kept built in the engine, so opening it again is instant; an open
  that finishes after its close lands here too. See
  [ADR 0006](adr/0006-timelines.md) and [ADR 0002](adr/0002-engine-and-desktop-state.md).

**On screen**
: The timeline rows the view shows, plus ten either side (`State::rows_shown`). Only their
  pictures, previews and GIFs load; before the view reports, a timeline's newest 30 entries
  count. See [ADR 0006](adr/0006-timelines.md).

**Plan** (timeline plan)
: The list of rows a timeline shows and their keys, made in one pass over its entries and made
  again only when the timeline's revision changes.

**Typing line**
: The line under the room's composer showing who is typing: their faces stacked, three pulsing
  dots, and "Ada is typing…". The user's own typing is sent as typing notices.

**Linked GIF**
: A GIF behind a Giphy, Tenor or Imgur link in a message, played inline; a setting turns it off.

**Link card**
: The card below a message for the first link in it (not a GIF): the page's site, title,
  description and image, as the homeserver describes it; a setting turns it off.

**Call card**
: A call's message in the timeline: video or voice, and whether it ended, was declined, or is
  under way (the newest card only), with Join while it is.

**History page**
: The 30 messages loaded when a room opens with fewer, and each time the user scrolls near the
  top (`HISTORY_PAGE`).

**Gap**
: Messages missed while offline, between what was stored and what sync resumed with; filled in
  place when paging back.

**Pending message**
: A sent message the server hasn't confirmed yet, shown dimmed with "Sending…"; the send queue
  retries it while offline and after a restart.

**Composer**
: Where messages are written: the formatting toolbar, the rich-text field, pending attachments,
  attach and send.

**Format**
: One composer formatting action (bold, link, bulleted list, …).

**Attachment**
: A file sent with a message: an image, video, audio clip or other file.

**Preview**
: The inline image shown for an image attachment: the sender's thumbnail, or a server thumbnail.

**Notification mode**
: When a room notifies: all messages, mentions only, or mute. Stored as push rules on the server.

**Default notification mode**
: An account's notification mode for every room that doesn't set its own. A room set back to
  "Use the default" follows it again.

**Settings**
: The modal for this device's preferences (language, appearance) and each account's default
  notification mode, in sections (`SettingsSection`).

**Interface size** (zoom)
: How large the whole interface draws, as a browser's page zoom (`Zoom`). 100% is the design
  size. See [ADR 0005](adr/0005-navigation-and-settings.md).

**Language**
: The app's display language: the system's, or English, German or French (`Language`).

**Wants attention**
: A room drawing the eye (bold name, rail and account dots): unread messages, or only mentions
  when it notifies for mentions only or is muted.

**Popover**
: A floating panel over the window (account menu, room menu). One at a time.

**Modal**
: A dialog over the whole window with the rest dimmed (settings, confirmations). One at a time.

**Room menu**
: A room's right-click menu: the Notifications submenu, then Leave room, or Delete chat for a
  direct chat.

**Confirmation**
: The dialog before an action that can't be undone (`Confirm`): leaving a room, deleting a chat.

**Delete chat**
: Leaving a direct chat and asking the server to forget it, so it leaves the room list.

## Matrix

**Homeserver**
: The server an account lives on. Entered as a server name (`matrix.org`) or URL.

**Discovery**
: Resolving what the user typed (server name, URL or Matrix ID; `SignInInput`) to a homeserver
  through `.well-known`, and learning its sign-in methods (`LoginMethods`). The result is a
  `Homeserver`.

**Sign-in steps**
: Step 1, homeserver; step 2, sign in in the browser (OAuth) or with a password, whichever the
  server uses (`SignInStep`).

**OAuth** (next-generation auth)
: Signing in through the homeserver's authorization server in the browser (Matrix's OAuth 2.0
  API, often served by the Matrix Authentication Service). See
  [ADR 0003](adr/0003-accounts-and-sign-in.md).

**History visibility**
: A room setting for who may read messages from before they joined: anyone, all members, members
  from their invite, or from their join (`HistoryVisibility`). The server enforces it.

**Push rules**
: Server-side rules that decide which events notify. The SDK evaluates them during sync.

**Session**
: One signed-in device of an account (a Matrix device). This app's own session's tokens are saved
  so it survives a restart. The Sessions page in settings lists every session of the account.

**Verified session**
: A session the account's cross-signing identity trusts, so it may receive room keys and read
  encrypted history. See [ADR 0004](adr/0004-encryption.md).

**Verification**
: Confirming a new session from one already verified by comparing the same emoji on both
  (`VerificationStep`). An incoming verification request comes from the user's other session.

**Identity reset**
: Replacing the account's cryptographic identity when the recovery key and every verified session
  are lost. This session becomes the first verified one, and earlier encrypted messages stay
  unreadable. See [ADR 0004](adr/0004-encryption.md).

**Guarded action**
: An action the server may protect with the account password (`Guarded`): ending a session,
  resetting the identity, adding a contact detail. They share one password dialog and retry.

**Profile**
: The account's display name and picture, plus its contact details. An OAuth account manages
  the details on its homeserver's **account page** when the server leaves them to it. See
  [ADR 0008](adr/0008-people-and-rooms.md).

**Contact detail**
: An email address or phone number linked to the account (Matrix's third-party identifier,
  3PID), confirmed by a link or a texted code before it is added.

**Sign-in method**
: How an account signed in, OAuth or password (`SignInMethod`). It decides where account details
  are managed.

**Key backup**
: The server-side copy of the account's room keys, encrypted so only verified sessions can read
  it. A session fetches keys from it for messages it can't decrypt. See
  [ADR 0004](adr/0004-encryption.md).

**Recovery key**
: The key that unlocks the server-side key backup, so a session can verify itself and read
  earlier messages without another session. It is shown once, when recovery is set up or reset.

**Ended session**
: A session the server no longer accepts (unknown token, rejected refresh token). The account is
  signed out here and the app offers to sign in again (`Event::SessionEnded`).

**Suspended account** (soft logout)
: An account the server signed out softly. It stops running but keeps its store directory and
  device until the user signs in as it again (which takes them back) or removes it
  (`Command::Suspend`, `StoredAccount::signed_out`).

**Side panel**
: The panel beside the open room, in the thread's place: the room's info, its members, its
  threads or its files (`SidePanel`). See [ADR 0008](adr/0008-people-and-rooms.md).

**Member menu**
: What a press on someone in the member list offers: chat, call, invite elsewhere, and with the
  rights, change role, remove or ban (`Moderation`).

**Poll**
: A question with two to twenty answers that members vote on, its counts shown as they vote or
  once it ends. See [ADR 0006](adr/0006-timelines.md).

**Call**
: Voice or video with the people in a room: a MatrixRTC session through the homeserver's
  LiveKit service, or a one-to-one call device to device in a direct chat. One runs at a time.
  See [ADR 0009](adr/0009-calls.md).

**Call stage**
: What a room shows in place of its timeline during its call: faces (voice) or the video grid,
  over the call glow, with the controls. It can share the room pane with chat.

**Call layout**
: How the call stage arranges its surfaces: Auto chooses a gallery, one-to-one view or shared
  screen; Grid gives surfaces equal space; Focus follows the speaker or a locally pinned surface.
  See [ADR 0009](adr/0009-calls.md).

**Call plan**
: The visible call tiles and their rectangles, computed from the available space, layout and
  page. Both the view and the video watches follow this plan.

**Call tile**
: One participant's camera (or avatar) or shared screen on the call stage. Its identity follows
  the participant and surface kind, so a camera restarting keeps its pin.

**Participant strip**
: The smaller call tiles beside or below a focused surface, with a resizable divider and pages.

**Self-view**
: The user's own camera preview. Hiding it changes only the local call layout, not what the
  camera publishes to others.

**Call bar**
: The strip at the bottom of the sidebar while the call's stage isn't shown, leading back to it.

**Ring**
: A call ringing for the user (`Event::IncomingCall`), shown as a card until answered, declined
  or lapsed.

**Third-party list**
: Every library and bundled file the app is built from, with version and license, shown under
  Settings → Info (`third_party::all`, written by `bun run third-party`). See
  [ADR 0005](adr/0005-navigation-and-settings.md).

**Backdrop**
: The animated background of the sign-in and restoring screens: a glow drawn in ASCII
  characters that scramble under the pointer (`state::backdrop`, `ui::backdrop`). See
  [ADR 0003](adr/0003-accounts-and-sign-in.md).

**Restoring**
: Signing saved accounts back in at launch. The app shows a launch screen meanwhile
  (`Screen::Restoring`). See [ADR 0003](adr/0003-accounts-and-sign-in.md).

**Store directory**
: An account's local SDK databases (state, crypto, event cache, media), one per login.

## Architecture

**Engine**
: `matrix-core`'s actor: owns every account, runs their syncs and carries out commands on a
  background runtime. See [ADR 0002](adr/0002-engine-and-desktop-state.md).

**Command**
: A request from an app to the engine (`Command`). Gets a **request ID** (`RequestId`) and ends
  with exactly one `Event::Finished`.

**Event**
: Something the engine reports (`Event`): command results, account changes, room lists, messages,
  media, notifications.

**Event sink**
: What an app gives the engine to receive events (`EventSink`).

**Engine handle**
: The cloneable sender of commands (`EngineHandle`). A **detached** handle has no engine behind
  it, for headless runs and tests.

**Outcome**
: What a finished engine task hands back to the engine loop when it changes engine state (an
  account signed in or out).

**Model**
: The plain data types that cross the engine boundary (`matrix_core::model`). No SDK types.

**Media ref**
: An opaque handle to a piece of Matrix media (`MediaRef`), used to fetch and to key caches.

**Secret store**
: Where tokens and database passphrases are kept (`SecretStore`): the OS keychain on desktop.

**Client identity**
: How the app introduces itself to OAuth servers: name and public URI (`ClientIdentity`).

**Reducer**
: `State::apply`, the one function that folds engine events into desktop state.

**Scratch folder**
: A private folder under the app's cache for files made for a moment (audio to play, voice
  memos), emptied at launch and exit (`Shell::scratch`).

**Purpose**
: Why the desktop sent a command, so its answer reaches the right place (`Purpose`).

**Shell**
: The desktop's door to the OS: file picker and notifications (`platform::Shell`).

**Palette**
: The desktop's semantic colours (`ui::theme::Palette`). Views use roles, never raw colours.

**Component**
: A shared UI building block in `ui/components.rs`.

**Catalog**
: A language's messages, `apps/desktop/i18n/app/<locale>.json`, looked up with `t!` and `tf!`.

## Tooling

**Scene**
: A named starting state for a headless render (`--scene demo-space`), built from the demo
  fixtures.

**Headless**
: Running the UI without a window: `--snap` renders a PNG, `--tree` dumps the element tree, and
  tests drive `rune::Runtime`.

**Agent tools**
: Reference material for coding agents in `.agent-tools/`, synced by `bun run agent-tools`.
