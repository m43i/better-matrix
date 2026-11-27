# Glossary

The words this project uses, in code, docs and conversation. Use these names; when a new concept
needs one, add it here.

## Product

**Account**
: One signed-in Matrix user (`@alice:example.org`) on one homeserver. Several run side by side;
each has its own client, store and sync loop. `AccountId` in code.

**Account menu**
: The popover opened from the signed-in user's avatar at the bottom of the rail: the user's
status, switch or add an account, profile, settings, and sign out (set apart, in red). Saved
messages open from the top bar instead. On phones, the sheet the avatar at the header's right opens.

**Status**
: What others see of the user on one account, as the user sets it in the account menu: Online,
Away, or Hidden (shown as offline). Saved in the account's data and announced as Matrix presence
by every session of this app; a dot on the account's avatar shows it. `Status` in code.

**Status message**
: What the user says they are up to on one account ("🌴 On vacation"), set in the account menu
and shown beside their presence: Matrix presence's `status_msg`. An emoji starting it stands for
it where space is short. `status_message` in code.

**Status message expiry**
: The optional time when this device clears the account's status message. The account menu
offers durations and a custom date and time. The engine keeps the deadline across restarts and checks
it while the account is running.

**Presence**
: Whether someone is around (online, away, offline) as their homeserver says, with their status
message: a dot on their avatar. Unknown when the server shares none, and then nothing shows.
`Presence` and `PersonPresence` in code.

**Rail**
: The narrow column on the far left: Home and the active account's spaces in the user's order,
then "+" to create a space, with the account avatar at the bottom.

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

**Note to self**
: A room only the user is in, for notes, links and files they keep for themselves; one per
account. A normal private, encrypted room on the server, which the account data event
`chat.bettermatrix.note_to_self` names. Made only when the user picks it in the new-chat search
or the top bar's search. While nobody else has joined or been invited, this app lists it above
every section of Home (never in a space), with a note icon and no calls; once someone is, it is a
normal room and the next note to self is a new one. Unlike a saved message, it holds new messages
rather than marking existing ones.

**Self-update**
: A newer desktop release offered by an icon above the rail's account avatar. The app checks
at startup and hourly; only clicking the icon downloads, replaces the install and restarts it.
Available for installed macOS and Windows release builds.

**Close to tray**
: Closing the desktop window while Better Matrix keeps running. The system tray (menu bar on
macOS) offers Open and Quit. General settings can make closing quit, and closing quits whenever
the tray is unavailable. See [ADR 0005](adr/0005-navigation-and-settings.md).

**Update command**
: On Linux, where the package manager owns the install, the update icon's panel instead shows
the terminal command that installs the newer release the way this copy was installed (apt, dnf,
zypper, pacman, the release's `PKGBUILD`, or the AppImage replaced in place), with a button to
copy it. An install the app can't place links the release's page instead.

**Overview**
: The main pane when no room is open: the scope's greeting or size, its unread rooms and recent
conversations. Home also offers shortcuts for a new chat and saved messages.

**Navigation**
: The screen, account, scope and room currently shown (`Nav`, with `Location` as a snapshot).
Changing destination lands it at once through `navigate`; threads and side panels belong to
the current room. See [ADR 0005](adr/0005-navigation-and-settings.md).

**Invitations**
: The fixed, collapsible section at the top of the sidebar for rooms and spaces the account
was invited to. Its counter stays while collapsed. Home also lists invitations; each opens
a preview with Accept and Decline.

**Public rooms**
: Rooms anyone may join, discovered through the homeserver’s public directory or a space’s
hierarchy. Its Overview suggests the unjoined ones with the most members; More shows the full list.

**Sidebar**
: The column listing the rooms and direct chats of the current scope.

**Resize handle**
: The draggable inner edge of the desktop's sidebar, side panel or thread. Its horizontal
resize cursor marks where to change the width, within limits that keep the panes readable.

**Space picker**
: On phones, the sheet Home's title opens: Home and the account's spaces, the desktop's rail.

**Search**
: The field in the desktop's top bar, and under each tab's title on phones: the account's rooms,
direct chats and spaces by name, people from the user directory, and a **typed link** to open.
The desktop also searches public rooms and spaces from the homeserver’s directory. Its filter
row narrows search to spaces, rooms (including direct chats), people or messages; no filter searches all.
Results have sections, with **message hits** from this device beneath the people and destinations.
Names and message excerpts highlight the characters matching the fuzzy query.

**Message hit**
: A locally stored, decrypted message matching search: its room, sender, date and an excerpt
with matching characters highlighted (`MessageHit`). Selecting it opens its message or thread.

**Search usage**
: This device's per-account frequency counts (`SearchUsage`): room opens and sends, direct-chat
peers and explicit reply recipients. They rank matching rooms, people and message hits within
sections, survive restarts, and never sync to the homeserver.

**Typed link**
: What a search reads as a link rather than a name: a permalink, or a bare room alias, room ID
or user ID (`#rust:matrix.org`), as people paste invitations (`Permalink::parse_typed`).

**Create menu**
: What can be made here: a room or section under the sidebar's "+" on desktop (spaces use the
rail's "+"); on phones, a chat, room, space or section under Android's floating button or the
"+" in iOS's tab bar.

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

**Read indicator**
: The small avatar stack at a message's lower right showing whose latest exposed read position
is there. Opens the reader list with receipt times (a sheet on mobile); selecting a reader opens
the member menu.

**Message actions**
: On phones, the sheet a long press on a message opens: quick reactions, reply, reply in thread,
forward, edit, copy link, pin/unpin, save/remove from saved, copy text, translate, end poll,
share/save and delete.
The desktop offers the same actions under More and in the right-click message menu.

**Pinned message**
: A message kept for everyone in a room through Matrix's `m.room.pinned_events` state. Its row
has a pin marker; room info, and on desktop the room header's pin button, lead to the full
pinned list, including older messages.

**Saved message**
: A message the user keeps for themselves, from its message actions. Private to the user and
following them to every session: MSC2437 tagged events (`m.favourite`) in the room's account data.
The bookmark beside the top bar's search lists them across the account's rooms (on phones, the
account menu). `SavedMessage` in code; pinned and saved
messages show as a `MessagePreview` outside their timeline. On desktop the list has a search and
saved filters.

**Saved filter**
: A quick filter over the desktop's saved messages, from the filter button beside their search:
when a message was sent (`SavedSince`: today, the last 7, 30 or 365 days) and where it was saved
(`SavedIn`: rooms, direct chats, or one conversation). Each one in effect shows as a chip that
removes it. `SavedFilter` in code.

**Forward**
: Send a copy of a text or attachment message into a chosen conversation on the same account,
keeping its current content and formatting and removing its reply, thread and edit relations.

**Hover toolbar**
: React, reply, reply-in-thread, edit on the user's own text and, behind a rule, More, shown while
a message is hovered. It floats on the top edge of what the message says: in a classic row at the
content's top right, the same on every row, on someone else's bubble at its top right, and beside
the user's own bubble on its left. A timeline too narrow for it keeps only More. Rising over the
message above, the toolbar the pointer is on wins over the row under it.

**Section**
: A group in the sidebar: the built-in Rooms, Direct messages, Favourites and Low priority, or one
of the user's own, per Home or space. See [ADR 0005](adr/0005-navigation-and-settings.md).

**Room priority**
: Whether the user put a room among their **Favourites** or their **Low priority** rooms, or
neither (`RoomPriority`): the room tags `m.favourite` and `m.lowpriority`, shared with every
client. Each has its built-in section.

**Marked unread**
: A room the user marked unread by hand (MSC2867, `RoomSummary::marked_unread`). It wants
attention until it is opened or marked read, on every client.

**Voice memo**
: A recording sent as a Matrix voice message (`Outgoing::Voice`), with its waveform. It shows as
a voice card, not a file.

**Waveform**
: An audio clip's loudness along its length, amplitudes from 0 to 1024 (MSC3246), drawn as bars
(`AudioDetails::waveform`).

**Voice card**
: How an audio clip shows in the timeline, on desktop and mobile: play, the waveform bars that
fill as it plays, a scrubber dot to drag, the time, and the playback speed button once it plays.

**Playback speed**
: How fast clips play, 1×, 1.5× or 2×, at their own pitch (`PlaybackSpeed`); one choice for every
clip, kept in settings.

**Voice glow**
: The blurred halo around the composer while a voice memo records, brighter as the voice gets
louder.

**Room event**
: Something that happened to a room rather than something written: a join, an invite, a new
name (`RoomEvent`, `RoomChange`). Shown as a small line; joins and leaves in a row merge. See
[ADR 0006](adr/0006-timelines.md).

**Room log**
: Every room event of a room's loaded history, newest first with its date and time: the
profile and room-setting changes timelines leave out by default (`RoomChange::log_only`), and
who came and went. Opened from the room's info (`SidePanel::Log`); "Load earlier" pages the
room's history back. See [ADR 0006](adr/0006-timelines.md).

**Room changes** (setting)
: Whether timelines also show the room events only the room log lists by default
(`show_room_changes`, a setting on this device).

**Hidden event**
: An event timelines leave out: room state the app draws no line for, an event that didn't
parse, a membership event that changed nothing (`EntryKind::Hidden`, `HiddenEvent`). With "Show
hidden events" (developer tools, a setting on this device) each shows as a small line naming its
sender and type, which opens its event source. See [ADR 0006](adr/0006-timelines.md).

**Event source**
: An event's JSON, for developer tools: as this device reads it (decrypted) and, for an encrypted
event, as it was sent (`EventSource`, `Command::ViewSource`). "View source" in a message's
menu. See [ADR 0006](adr/0006-timelines.md).

**Widget**
: A web app pinned to a room as state (`im.vector.modular.widgets`, `m.widget`): a board, a
dashboard, a meeting (`Widget`). Listed in the room's details and run in the app (the widget
window, a phone's web view) or the browser, with its URL's template variables filled in;
whoever may change the room's state adds and removes them. See
[ADR 0012](adr/0012-widgets-and-integrations.md).

**Meeting**
: A video conference pinned to a room as a widget, Jitsi or BigBlueButton (`Meeting`,
`Widget::meeting`), joined as itself rather than through Element's page; not a call, which the
app runs natively. The room offers Join meeting while one is pinned. See
[ADR 0012](adr/0012-widgets-and-integrations.md).

**Widget run**
: One widget open in the app: a long-running `Command::RunWidget` whose page the app shows
(`WidgetPage`) and whose messages it passes through the page's bridge (`WidgetBridge`) until the
app closes it. See [ADR 0012](adr/0012-widgets-and-integrations.md).

**Widget window**
: The desktop's window for a widget run: the same binary as a web-view helper
(`better-matrix --widget-window`), one process per run. See
[ADR 0012](adr/0012-widgets-and-integrations.md).

**Widget permission**
: Something a running widget asks to do through the widget API, granted or refused by the user
and remembered per room and widget (`WidgetPermission`, `Grants`). See
[ADR 0012](adr/0012-widgets-and-integrations.md).

**Integration manager**
: The page that adds bots, bridges and widgets to a room (Element's, Dimension), named by the
homeserver's well-known file and run like a widget (`Command::OpenIntegrations`). "Bots and
integrations" in the widgets panel. See [ADR 0012](adr/0012-widgets-and-integrations.md).

**Attachment preview**
: An image, PDF, video or audio clip shown full size over the window, with download, close and
right-click file actions (copy image, download, open in the default application;
`State::preview`). On phones, the full-screen viewer has the same file actions under More and a
long press. See [ADR 0007](adr/0007-composer-and-media.md).

**Attachment batch**
: The attachments a sender posted together: the run of their attachment messages the timeline
groups under one header, with nothing else between them. The attachment preview steps through
it with a strip of thumbnails, the arrow keys or a swipe (`preview::batch`, `state/gallery.ts`).

**Upload**
: A file to send: a path on disk or bytes in memory, such as a pasted screenshot (`Upload`).

**Upload thumbnail**
: A picture waiting in the composer, shown as a small square of itself with a remove button
instead of a file chip (`UploadThumbnails`). See [ADR 0007](adr/0007-composer-and-media.md).

**Warm timeline**
: A timeline closed lately and kept built in the engine, so opening it again is instant; an open
that finishes after its close lands here too. See
[ADR 0006](adr/0006-timelines.md) and [ADR 0002](adr/0002-engine-and-desktop-state.md).

**Cold timeline**
: A closed timeline past the engine's warm set, which the engine let go of. The mobile store keeps
it trimmed to its newest entries (`coldTimeline`), at least its newest message for the room list,
and forgets the room's members, threads, files and pins once no timeline of the room is open or
warm. See [ADR 0010](adr/0010-mobile.md).

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
GIFs sent from the **sticker and GIF picker** are linked GIFs.

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

**Draft**
: What the user wrote in a composer and hasn't sent. Each room and thread keeps its own: leaving
keeps it, coming back brings it back, and its text stays in this device's store across restarts.
`Draft` in code (the stored text and formatting; the desktop's `composer::Draft` also holds the
reply and attachments while the app runs).

**Format**
: One composer formatting action (bold, link, bulleted list, spoiler, formula, …).

**Pill**
: Something in a text that isn't plain words, held as one piece the caret steps over: a
mention, `@room`, a spoiler, a formula, a custom emoji. A field holds it as Rune's
`Mark::Pill` with an id (`user:@ada:x`, `room:#rust:x`, `everyone`, `spoiler`, `math`,
`emote:mxc://…`); `matrix_core::html` turns Matrix HTML into pills and back. Phones keep each as
a draft pill beside the draft's text: its id and where it starts (`DraftPill`).

**Mention**
: A message naming someone (or a room) on purpose: a link to them on matrix.to in its HTML,
and their ID in its `m.mentions`, so they are notified whatever the text says. `@room` mentions
everyone. A message mentioning the user is **highlighted** (`Message::highlighted`).

**Suggestion**
: What the composer offers for the word being typed: people after `@`, rooms after `#`, emoji
and custom emoji after `:` and two letters, slash commands at the start (`state::suggest`).

**Slash command**
: A message starting with `/name` that does something instead of being sent as it is: `/me`,
`/shrug`, `/invite`, `/join`, `/topic`, … (`SlashCommand`). `//` sends a leading slash.

**Emote**
: A message that says what its sender does ("* Ada waves"), sent with `/me`
(`MessageContent::Emote`, `Outgoing::Emote`).

**Code fence**
: A Markdown code block typed in the composer: a line of three backticks, the code, and another
line of backticks, the first naming the code's language (` ```rust `). Sending turns it into
a code block in that language (`composer::with_code_fences`), whose code shows in the
language's colours.

**Emoticon**
: A face typed as text (`:)`, `:D`, `<3`). The composer keeps it as typed; sending turns each
one that stands as a word of its own, outside code, links and pills, into its emoji
(`matrix_core::emoticons`), unless the desktop's Emoticons setting sends them as typed
(`Emoticons`).

**Spoiler**
: Words hidden until the reader presses them (`data-mx-spoiler`).

**Formula**
: LaTeX in a message (`data-mx-maths`), shown as Unicode text (`matrix_core::math`).

**Image pack**
: A set of images with short codes, the user's own or a room's (MSC2545), each usable as a
**custom emoji** in text, a **sticker** sent on its own, or both (`ImagePack`, `PackImage`). Where
it is kept is its **pack place** (`PackPlace`: the user's account data, or a room's state under a
state key); a room's pack may be **used everywhere** (in all the user's rooms).

**Sticker and GIF picker**
: The composer's one button for pictures sent on their own, with two tabs: **Stickers** (the
image packs' stickers, searchable, recently used first) and **GIFs** (found on GIPHY with the
user's **GIPHY key**, sent as their GIPHY link). `Popover::Stickers`, `State::picker`.

**GIPHY key**
: The user's own GIPHY API key, which GIF search needs; the app ships none. Kept in the system
keychain (the phone's secure store), never shown again once kept: only replaced or removed.

**Sticker manager**
: The dialog (a screen on phones) for managing image packs: adding pictures as stickers,
renaming, describing, choosing sticker or emoji use, the pack's picture, new room packs, and
using a room's pack everywhere (`Modal::Packs`, `Command::EditImagePack`).

**Location**
: A place a message shares (`m.location`), shown on a MapLibre map with the sender’s avatar pin.

**Live location**
: A time-limited room share whose avatar pin follows the sender’s device. Element-compatible beacon
updates refer to the state event that started the share.

**Scheduled message**
: A message the server holds to send later (MSC4140's delayed events), encrypted like any
other in an encrypted room (`ScheduledMessage`).

**Edit history**
: Every version an edited message went through, from its "edited" label (`EditVersion`).

**Message jump**
: Going to one message from a link, a reply's quote, a pin or a search hit: its timeline opens
(the room's, or its thread's) and history loads until it shows; it then **flashes** a moment.

**Attachment**
: A file sent with a message: an image, video, audio clip or other file.

**Preview**
: The inline image shown for an image attachment (the sender's thumbnail, or a server
thumbnail), or a video's poster (only the sender's thumbnail).

**Notification mode**
: When a room notifies: all messages, mentions only, or mute. Stored as push rules on the server.

**Keyword**
: A word that notifies the user like a mention wherever it is written: a content push rule on
the server, edited under each account in Settings, Notifications (`Event::Keywords`).

**Default notification mode**
: An account's notification mode for every room that doesn't set its own. A room set back to
"Use the default" follows it again.

**Settings**
: The modal for this device's preferences (language, appearance) and each account's default
notification mode, in sections (`SettingsSection`).

**Appearance** (settings section)
: The settings section for how the app looks: the theme (light, dark or the system's;
`Appearance`), the message layout and the interface size. On phones these sit under Preferences,
less the interface size.

**Message layout**
: How timelines lay out messages (`MessageLayout`, `messageLayout`): **classic rows**, the
default, every message under its sender's name and picture as in Slack, or **chat bubbles**,
other people's messages at the left beside their picture and the user's own at the right. A
bubble holds words (text, a deleted or undecryptable message) with the sender's name (left out
in a one-to-one direct chat) and the time; pictures, files, clips, polls, call cards and GIFs stand without one. Reactions and the
thread hang below. See [ADR 0006](adr/0006-timelines.md).

**Interface size** (zoom)
: How large the whole interface draws, as a browser's page zoom (`Zoom`). 100% is the design
size. See [ADR 0005](adr/0005-navigation-and-settings.md).

**Language**
: The app's display language: the system's, or English, German or French (`Language`).

**Wants attention**
: A room drawing the eye (bold name, rail and account dots): unread messages, or only mentions
when it notifies for mentions only or is muted; and any room marked unread.

**Taskbar badge**
: The count on the app's taskbar button (Windows) or Dock icon (macOS): the messages that want
attention in every account (at least one per room marked unread), the invitations shown and the
calls ringing. A
notification also flashes the taskbar button on Windows. See
[ADR 0005](adr/0005-navigation-and-settings.md).

**Jump list**
: What the Dock icon's menu offers on macOS: the rooms and the direct chats opened most, and
lately, on this device (**frequent rooms**, `Visits`), five of each, each opening through its
room link. Windows has none (ADR 0005). See [ADR 0005](adr/0005-navigation-and-settings.md).

**Popover**
: A floating panel over the window (account menu, room menu, emoji picker). One at a time; the
timelines under it take no hover while it is open.

**Modal**
: A dialog over the whole window with the rest dimmed (settings, confirmations). One at a time.

**Safety**
: What the user keeps away on one account: people they ignore, where pictures and videos show,
and invitations from strangers (`Safety`). Settings → Safety; kept in the account's data. See
[ADR 0011](adr/0011-safety.md).

**Ignored person**
: Someone the user ignores (Matrix's ignore list): the server sends nothing from them, earlier
messages disappear, and they aren't told. Ignore is in the member menu and person panel.

**Report**
: Telling the homeserver's admins about a message, a room or a person, with an optional reason
(`Command::Report`, `ReportTarget`).

**Stranger**
: Someone the account shares no joined room with, as this device knows (`Invitation::stranger`).
Their invitations can be hidden: out of the sidebar and pushes, listed in Settings → Safety.

**Hidden media**
: A picture, video, linked GIF or link card image the media choice (`ShowMedia`: always, in
private rooms, never) keeps back: a placeholder with Show until pressed.

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

**Permalink**
: A link naming a Matrix user, room or message rather than a web page: `https://matrix.to/#/…` or
a `matrix:` URI (`matrix_core::Permalink`). It opens in the app, never the browser. See
[ADR 0008](adr/0008-people-and-rooms.md).

**Room preview**
: A room as someone outside it sees it: name, alias, picture, topic, member count, who may join,
and whether the account joined or is invited (`RoomPreview`, `Command::PreviewRoom`). What a
permalink to an unjoined room shows, with Join.

**Link dialog** / **link screen**
: Where a permalink opens when its room isn't joined, or names a person: the room preview with
Join, or the person with Start chat, and the account it acts for. A dialog on the desktop
(`Modal::Link`), a sheet on phones (`app/link.tsx`).

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

**Push**
: A notification the homeserver sends a phone while the app may not be running: only an account,
a room and an event ID (`event_id_only`), which the phone fetches and decrypts itself
(`PushTarget`, `PushContent`). See [ADR 0010](adr/0010-mobile.md) and [push.md](push.md).

**Pusher**
: An account's registration of one device for pushes on its homeserver: the push gateway, the
device's push token, and what each push carries (`Pusher`, `Command::RegisterPusher`).

**Push gateway**
: The service a homeserver sends pushes to, which hands them to the phones: ours translates the
Matrix push gateway API into Expo's push service (`packages/push-gateway`).

**Push token**
: The device's address for pushes: an Expo push token (`ExponentPushToken[…]`), one per device,
shared by every account on it.

**Notification Service Extension**
: The small iOS process that gets each push before it shows and replaces its generic text with
the decrypted message (`BetterMatrixPush`, `bm_push_fetch`).

**Push task**
: Android's handler for a push, run in the app's JavaScript (headless when the app isn't
running): it asks the engine, or the account's store without it (`fetch_push`).

**Session**
: One signed-in device of an account (a Matrix device). This app's own session's tokens are saved
so it survives a restart. The Sessions page in settings lists every session of the account.

**Verified session**
: A session the account's cross-signing identity trusts, so it may receive room keys and read
encrypted history. See [ADR 0004](adr/0004-encryption.md).

**Verification**
: Confirming a new session from one already verified, or another person's identity, by
comparing the same emoji on both sides or scanning one side's QR code with the other
(`VerificationStep`). An incoming request comes from the user's other session or, in a direct
chat, from another person.

**Verified person**
: Someone whose identity the user verified (`Identity::verified`). When their identity changes
afterwards (`Identity::changed`), whoever holds it now may not be them.

**Offline device**
: A device the server keeps for the account (a dehydrated device, MSC3814) so a session signed
in later reads messages sent while no session was signed in. Turned on with the recovery key
(`OfflineDevice`). See [ADR 0004](adr/0004-encryption.md).

**Key export**
: The account's message keys written to a file, encrypted with a passphrase, for importing on
another session or client (`ExportRoomKeys`, `ImportRoomKeys`).

**QR sign-in** (link a device)
: Signing a new device in with a QR code exchanged with a device where the account is signed in
already, which approves it and hands over the secrets, so it starts verified (MSC4108,
`QrLoginStep`). See [ADR 0003](adr/0003-accounts-and-sign-in.md).

**Check code**
: The two digits a device shows after scanning a QR sign-in code, typed on the other device to
prove the two are talking to each other.

**Registration**
: Creating an account: on the account page for OAuth servers, in the app for password servers,
which may ask for a registration token, agreement to policies (`Policy`), or a step on the
server's page (`RegistrationStep`).

**Deactivation**
: Closing an account for good on its server; nobody can sign in to it again
(`DeactivateAccount`).

**Identity reset**
: Replacing the account's cryptographic identity when the recovery key and every verified session
are lost. This session becomes the first verified one, and earlier encrypted messages stay
unreadable. See [ADR 0004](adr/0004-encryption.md).

**Guarded action**
: An action the server may protect with the account password (`Guarded`): ending a session,
resetting the identity, adding a contact detail, changing the password, deactivating the account.
They share one password dialog and retry.

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
threads, its files, its pins, its widgets or its room log, or someone's person panel (`SidePanel`). See [ADR 0008](adr/0008-people-and-rooms.md).

**Person panel**
: Someone's profile in the side panel, from their name or picture on a message: picture,
name, user ID, presence and role, with Message, calls and the member menu
(`SidePanel::Person`).

**Member menu**
: What a press on someone in the member list offers: chat, call, invite elsewhere, and with the
rights, change role or power level, remove or ban; for someone banned, unban (`Moderation`).

**Join request** (knock)
: Someone asking to join a room whose join rule is "Ask to join". With the right to invite, the
member list shows them apart with Approve (`Moderation::Admit`) and Deny (`Moderation::Deny`);
`RoomSummary::knocks` counts them.

**Room settings**
: A room's or space's name, topic, picture, who may join, history, guest access, addresses,
directory listing, encryption, permissions and version (`RoomSettings`), changed one
`RoomSetting` at a time. Offered only to those who may change something (`RoomRights`).

**Join rule**
: Who may join a room or space: anyone (Public), members of its space (Space members), those
let in after asking (Ask to join), or the invited (Private). `JoinRule` in code.

**Permission**
: Something a room's power levels allow from a level on: sending messages, inviting, removing,
changing the name, and so on (`Permission`, `PermissionLevel`). The permissions editor sets the
level each needs.

**Upgrade**
: Replacing a room with a new one on a newer room version. The old room points to its
**successor** (`Successor`), which its banner offers to join.

**Space rooms**
: The rooms and subspaces a space lists, in the space's order; with the right, added, removed
and reordered (`SpaceChange`). Leaving a space may leave chosen ones too (`Command::LeaveSpace`).

**Poll**
: A question with two to twenty answers that members vote on, its counts shown as they vote or
once it ends. See [ADR 0006](adr/0006-timelines.md).

**Call**
: Voice or video with the people in a room: a MatrixRTC session through the homeserver's
LiveKit service, or a one-to-one call device to device in a direct chat. One runs at a time.
See [ADR 0009](adr/0009-calls.md).

**Sticky membership**
: A call membership as Element Call's "matrix_2_0" mode writes it: a sticky `m.rtc.member`
event (MSC4354) with a random member ID, current for at most an hour unless sent again, rather
than a state event (`calls::sticky`). Read alongside state memberships. See
[ADR 0009](adr/0009-calls.md).

**Call stage**
: What a room shows in place of its timeline during its call: faces (voice) or the video grid,
over the call glow, with the controls. It can share the room pane with chat; on phones it is a
full-screen modal over the app.

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

**Compact stage**
: The call stage made small above the room's chat, one video surface at a time with small
controls: above the chat in a narrow desktop room, and on phones the top of the call's room
while its chat is shown. Tapping it returns to the full stage. See
[ADR 0010](adr/0010-mobile.md).

**Call bar**
: The strip at the bottom of the sidebar while the call's stage isn't shown, leading back to it.
On phones, the strip at the top of the tab screens.

**Platform camera**
: A camera the app runs with the system's own API (Camera2, AVFoundation) for the engine, which
pushes its frames into the call (`calls::PlatformCamera`, `push_camera_frame`): phones, where
`nokhwa` has no backend. See [ADR 0009](adr/0009-calls.md).

**Video view**
: A native view on phones that shows one call video (`CallVideoView`, by `VideoId`); the engine
draws its frames straight into it, never through JavaScript. See
[ADR 0010](adr/0010-mobile.md).

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

**Room list sync**
: The SDK's `RoomListService`: rooms over sliding sync (MSC4186), newest first, a batch at a
time, on servers that offer it. Beside it a long-poll sync that asks for no rooms carries
to-device messages, keys, presence and account data. See
[ADR 0002](adr/0002-engine-and-desktop-state.md).

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
: `State::apply`, the one function that folds engine events into desktop state. The mobile app has its own, `apply` in
`apps/mobile/src/state/reducer.ts`.

**Bridge**
: The engine for the phones: `matrix-ffi`'s UniFFI object that takes commands and gives events
as JSON, with media handed over as files (`Bridge`, `BridgeEvent`). See
[ADR 0010](adr/0010-mobile.md).

**Event batch**
: Events the bridge delivers to JavaScript together in one call (`events::next_batch`), applied by
the store with one notification to the views (`Engine.subscribeBatches`). See
[ADR 0010](adr/0010-mobile.md).

**Launch handoff**
: The folder where a second desktop launch leaves its permalinks for the running app, which reads
them when Rune reopens it (`platform::launch`). See [ADR 0001](adr/0001-stack-and-tooling.md).

**Link inbox**
: On phones, where permalinks the system opened the app with wait until an account can open them
(`ui/links/inbox.ts`). See [ADR 0010](adr/0010-mobile.md).

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
On phones, `bettermatrix://demo/<scene>` opens one in a dev build (`apps/mobile/src/demo`).

**Headless**
: Running the UI without a window: `--snap` renders a PNG, `--tree` dumps the element tree, and
tests drive `rune::Runtime`.

**Agent tools**
: Reference material for coding agents in `.agent-tools/`, synced by `bun run agent-tools`.
