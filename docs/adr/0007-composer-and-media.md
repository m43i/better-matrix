# 0007. Composer and media

Status: Accepted
Date: 2026-11-14

## Context

Matrix carries formatting as HTML (`org.matrix.custom.html`) beside a plain body, and the
timeline must render what others send safely. Media can be encrypted, large and numerous, so
decoded pictures and PDF pages need memory limits. People expect Slack's composer, pasted
screenshots, voice memos, and images, PDFs, videos and recordings that open large.

## Decision

- **Rich text through Rune's `TextEdit`:**
  - The composer edits a rich `TextEdit` limited to what Matrix HTML carries: bold, italic,
    strike, code, links, lists, quotes, code blocks.
  - Formatted text is sent as `to_html()` with the plain body; unformatted text goes plain.
  - What one send carries goes in order: the words, then each attachment. Each is its own
    `Command::Send`, so the engine runs a room's sends one after another in the order they came
    (`Running::sends`). The SDK sends an attachment straight to the server, past the room's send
    queue (the queue would show it pending, but a file the server refuses, too large say,
    would wedge the queue and hold every later message in the room), so an attachment first
    waits for that queue to send the words before it (`queue_sent`): not when the queue is
    stopped or held by a message it couldn't send, which no wait would get out.
  - Markdown code fences typed in the field (a line of three or more backticks, optionally a
    language, and a closing line of as many) go as the code block they stand for
    (`composer::with_code_fences`, on the copy that is sent; the field keeps what was typed): the
    lines between become code-block lines without their marks, the fence lines go, from the body
    too, as for a code block made with the toolbar. The opening fence's language goes as the
    block's `language-…` class, each fence its own `<pre>` even when fences follow each other
    (`html::with_code_blocks`; Rune's code blocks carry no language and join, so
    `composer::Sent` keeps each block's language and lines beside the field). Editing a message
    puts each block with a language, and each right beside another block, back into the field
    as the fence it came from, longer than any line of backticks in its code, so an edit keeps the
    blocks and their languages. Blank lines at a fence's edges go: HTML readers drop a code
    block's first line break, which would shift the blocks' lines. A fence
    still open, around blank lines or around a pill goes as typed, and
    `/plain` sends fences as typed. Between an opening fence and its closing one Enter starts a
    new line instead of sending, nothing is suggested, and a paste goes as plain text (a copied
    code block would carry on past the closing fence), so a block is typed as in other chat
    apps; the caret stays in view as the composer adds lines. An edit counts as unchanged when its
    field is as it opened (text and formatting), so opening a message holding literal fences and
    sending it as it was sends no edit. A slash command's name isn't part of its line, so
    `/plain ```` opens a fence for Enter like any other. Other Markdown in the field goes as
    typed: the field formats with the toolbar and shortcuts.
  - Received HTML is parsed once on arrival with `TextEdit::from_html` (a sanitising parser)
    and shown as selectable read-only text. Every bare web address in a message, plain or
    formatted, becomes a link mark there too (`links::link_ranges`), except inside code and
    where the sender already linked the words; plain text without links stays plain selectable
    text. Pressing a link opens it in the browser (`on_link`, `State::open_link`).
  - A pasted lone web address goes into the draft as a link, and pasted over a selection links
    the selection (`Composer::paste_link`, one undo step); text typed after it is plain.
  - `Format` lists every toolbar action; the toolbar renders the list. Spoiler and Formula
    turn the selection into a pill (below).
- **Pills:** what Matrix HTML carries that isn't formatting is one pill in a field (Rune's
  `Mark::Pill`, `<span data-pill="id">` in its HTML): mentions (`user:@…`, `room:#…` or a room
  ID), `@room` (`everyone`), spoilers (`spoiler`), formulas (`math`) and custom emoji
  (`emote:mxc://…`). `matrix_core::html` is the one translation, both ways: `to_pills` reads a
  message's HTML for showing it (`Pills::Display`: formulas as Unicode) or editing it
  (`Pills::Compose`: formulas as their LaTeX), `from_pills` writes a field's pills as Matrix
  HTML (a matrix.to link, `data-mx-spoiler`, `data-mx-maths` with the LaTeX as its `<code>`
  fallback, `<img data-mx-emoticon>` 32 px high). A field with a pill counts as formatted, so it
  goes with HTML. Drafts keep Matrix HTML and come back as pills, as edits do. A spoiler's words
  stay out of the plain body (`[Spoiler]`), so clients that show only the body don't show them.
- **Mentions:** every text, emote and edit carries `m.mentions` (`Outgoing::Text::mentions`,
  `Command::Edit::mentions`), empty when it mentions nobody: a message mentions only the people
  its pills name, never someone whose name its words happen to hold (the spec's intentional
  mentions). A reply adds the replied-to sender, as the SDK does. Received mention links show as
  chips in the accent colour; one naming the reader is filled, and the row of a message whose
  push rules highlight it (`Message::highlighted`) gets a warm tint and bar, as in Slack.
- **Suggestions** (`state::suggest`): the word before the caret asks for them when it starts
  with `@` (room members, then `@room`), `#` (joined rooms, not direct chats), `:` and two
  letters (the packs' custom emoji, then emoji whose short code starts with it, then whose name
  holds it) or `/` at the start of the message (commands). Never inside code, a pill or a
  selection. The list is worked out from the field each frame, eight at most; a composer keeps
  only the pick and a dismissal for the word they were made for (`Suggest`), so typing on keeps
  them and a new word starts over. Up and Down move round, Tab and Enter take, Escape hides the
  list for that word; a press takes one. Taking one puts in a pill (mentions by display name,
  as Element does; rooms as `#name`) or the emoji, then a space. Members load when `@` is first
  typed in a room whose list isn't here (`load_for_suggestions`, from `poll`). The list sits on
  the composer's frame at the window's root and takes no focus.
- **Slash commands** (`state::slash`): `/me`, `/shrug`, `/tableflip`, `/unflip`, `/lenny`,
  `/plain`, `/spoiler`, `/invite`, `/join`, `/leave`, `/topic`, `/nick`. Each becomes something
  that exists: an emote or text through the usual send (with the draft's reply), an invite, the
  link dialog for a join (`open_typed_link`), the leave confirmation, `Command::ChangeRoom` (`RoomSetting::Topic`, the room settings' own path), the
  global display name. A name the app doesn't know stops the send with a notice and keeps the
  text: a typo never goes out. `//` sends a leading slash; `/usr/bin` is a message.
- **Emoticons** (`matrix_core::emoticons`): the field keeps `:)`, `:D`, `<3` and the like as
  typed; sending a message or an edit sends each as its emoji (desktop `composer::with_emoji`,
  the field itself unchanged). One counts only as a word of its own: whitespace or the start
  before it, whitespace, the end or `.`, `,`, `!`, `?` after it, and not between backticks; `<3`
  beside a number stays a comparison. The desktop also leaves code, code blocks, links and pills
  (mentions, maths, spoilers) as typed. Inline code sends one literally. A device setting
  under Messages & media (`Settings::emoticons`, `Emoticons::AsEmoji` by default, or `AsTyped`)
  sends them all as typed instead, so `:D` can go as it is; the composer takes the choice
  when it builds what to send (`take_outgoing`, `take_edit`). Phones don't replace them yet.
- **Spoilers and formulas:** received spoilers draw as a block of their own colour, carrying an
  app link (`SPOILER_LINK`, naming the message, the place and a fingerprint of the words) a press
  reveals for the run (`State::revealed`): an edit hiding other words there hides them again.
  Until revealed, the words never reach the text Rune holds (blanks of the same length stand in),
  so neither a screen reader nor a copied selection has them. A spoiler is its
  plain words (a custom emoji by its description): nothing inside it becomes a pill or link that
  could show or act through it, and a link around a spoiler is no link, its words being hidden.
  A spoiler wins over everything else an element says (maths too), and a code block holding one
  shows as ordinary text, as fields keep no pills in code blocks; for the same reason the
  composer makes no spoiler or formula inside a code block, nor a code block over a pill, and a
  key or paste that would join a paragraph holding a pill onto a code block (Backspace or Delete
  at the join, or typing, Enter, cutting or pasting over a selection across it) does nothing.
  Any other change that strips a pill into a code block (the field's own cut, word deletion,
  Tab, input methods) is taken back after its frame, and again before a message is sent or
  scheduled (`Composer::keep_pills`), so a spoiler never leaves as plain words.
  A message's plain body may spell out what its spoiler hides, so text taken from a message
  (previews, notifications, quotes, search hits, copying, translating) is what it shows:
  `MessageContent::plain` and `html::shown_text`, each spoiler as `[Spoiler]`.
  `/spoiler` writes its spoiler itself (`html::spoiler`), line breaks and all, whatever the field
  held its words as. Formulas
  (`data-mx-maths`) show as one line of Unicode on a chip, in italics (`matrix_core::math`:
  Greek and symbols by name, scripts in Unicode's super- and subscripts or `^(…)`, `\frac` as
  `a/b`, unknown commands kept); there is no typesetting engine for Rune, and Unicode keeps
  selection and copy working. Formulas come from anyone in a room, so reading one is bounded:
  accents stack on letters rather than doubling the text, and past 2000 bytes, 32 nested levels
  or 8000 bytes of output a formula shows as its LaTeX. The mobile port runs the same vectors
  (`math.json`).
- **Custom emoji and stickers** (MSC2545 image packs, `matrix_core::packs`): the user's pack
  (`m.image_pack`), the room's (`m.room.image_pack`, one per state key) and those enabled
  everywhere (`m.image_pack.rooms`), each also under the `im.ponies.*` name clients used before
  (a room's pack under both names once), read from the store (under sliding sync, whose fixed
  required state leaves packs out, also from the room's state on the server, once a run per
  room, what sync brought since winning for the same pack), listed per room once a run
  (`Command::ListImagePacks`; the sticker picker lists again as it opens). The user's own pack
  is always listed, empty or not, so it can be added to; the listing says whether the account
  may add a pack to the room (`may_add`), and each pack where it is kept (`PackPlace`), whether
  the account may change it and whether the user uses it everywhere. The emoji picker
  leads with the room's custom emoji. The composer's sticker button opens the sticker and GIF
  picker: its Stickers tab filters by short code or description as one types, leads with the
  16 stickers sent last on this device (`RecentStickers`, saved in the prefs), then each pack's;
  a press sends `Outgoing::Sticker` (`m.sticker`), as a reply and into a thread like any message
  (the SDK relates only room messages itself, so the engine writes the relation for stickers
  and polls).
- **Pack images load** as their file when they are stickers (pickers, the manager, suggestions),
  as the timeline shows a sent sticker: the image cache keeps one rendition per image, and a
  server thumbnail loaded first would leave the sent sticker still. Emoji-only images load as
  128-pixel thumbnails.
- **Managing packs** (`Command::EditImagePack` with a `PackEdit`, answered by
  `Event::ImagePackEdited`): rename, set the pack's picture (one of its images), add images,
  change an image's short code, description and use, remove one, make or delete a room pack,
  use a room pack everywhere. Image packs are in the spec since Matrix 1.19: a room's packs and
  the rooms used everywhere are written under its names (`m.room.image_pack`,
  `m.image_pack.rooms`), the user's own pack, which the spec doesn't name, under
  `im.ponies.user_emotes`, which Cinny, FluffyChat and Nheko read; a pack already kept under the
  other name is changed there too. Pictures are uploaded first; then an account's pack changes
  go one at a time (`Account::pack_edits`), each reading every copy of the pack as the server
  holds it now (account data fetched, a room pack's state read by key), so two changes, here or
  on another device, never write over each other; each copy is changed as it stands, keeping
  what the app doesn't know, and written back under its own name. A new image takes a short code
  free in every copy; changing or removing an image touches another copy only where the same
  code names the same picture as the copy the apps show (the first), which is written last, so a
  change failing part way still shows as not done and doing it again reaches every copy. An edit
  acts on the pack as the server holds it when it runs (last writer wins, as for any edit made
  from a view another admin changed since). A change one copy refuses
  (a short code taken there) is refused for all; a room pack deleted meanwhile is not made again
  (only `Create` makes one). Using a pack everywhere names it in the list matching each name it is
  kept under (`m.image_pack.rooms` for `m.room.image_pack`, `im.ponies.emote_rooms` for the
  older), so apps reading either find it; stopping takes it off both. A write failing part way
  still has the room's pack state read again. After a write a room's pack
  state is read again from the server (`Account::pack_state`, its own times, kept even without
  sliding sync), and the user's own pack waits up to five seconds for sync, so listing right
  after shows the change. A new room pack (`PackEdit::Create`) takes a state key made from its
  name with a random part, so no key is used twice: two people making packs at once, or a
  deleted pack's key that people still use everywhere. Added pictures are uploaded
  unencrypted (packs aren't encrypted, as the spec warns); a picture from an encrypted message
  is downloaded and uploaded again, which both apps ask about first. `info` gives the size the
  image shows at, its side at most 256 pixels, as sticker tools do: other apps draw stickers at
  that size. Short codes follow the spec's grammar (1 to 100 ASCII letters, digits, `-`, `_`):
  made from file names (the rest `_`), numbered when taken; images already named otherwise still
  show. Per-image use (`usage` on an image) is kept from MSC2545: the spec has use only per pack,
  so other apps may offer such an image both ways; a pack naming its use is widened to every use
  its images name (a sticker added to an emoji pack makes it a sticker pack too), so the spec's
  apps offer it. The desktop's sticker manager
  (`Modal::Packs`, from the picker's gear) lists the user's pack, the room's (with "New pack"
  when `may_add`, the right to send `m.room.image_pack`) and those used everywhere; added
  pictures are stickers until changed. After a change the packs shown are listed again; other
  rooms' lists are dropped and listed when next needed.
- **GIF search** (`Command::SearchGifs`, `matrix_core::gif_search`): GIPHY, with the user's
  own API key (Settings → Messages & media → GIF search, or the picker's GIF tab, which asks for it with
  a link to GIPHY's dashboard); the app ships no key, so no one pays for everyone's searches.
  The key goes into the system keychain alone (`system_keychain`: unlike sessions, never into
  files when no keychain is reachable; the key then lasts the run), read and written off the
  UI thread (`Shell::load_gif_key`,
  `Shell::save_gif_key`, the latest of quick changes winning) and held in memory, never in the
  settings file. Once kept it is never shown again: Settings and the GIF tab offer Replace (an
  empty field, the old key kept until a new one is saved) and Remove (`picker::GifKey`: loading,
  missing, kept). A keychain that refuses a new key says so (it lasts the run); one that can't
  remove the key says so and shows it as kept again; Settings then tells a key kept until quitting
  (`GifKey::Session`) from one saved.
  Blank is what's trending, shown as the tab opens; typed text is searched once typing pauses
  500 ms (free keys allow 100 searches an hour) or on Enter, 24 to a page, "More GIFs" for the
  next, two pages at most (48: what the picker's players hold; a narrower search finds the
  rest); only the
  latest search's answer counts, and none is kept between visits: the GIF tab searches afresh
  each time it shows, and its previews' players are let go once it doesn't. Rated PG; the app's language goes along. GIPHY's
  rules apply: its media URLs are used exactly as given (linked `.gif` files are now fetched with
  their query, `links::gif_source`), its order kept, its "Powered by GIPHY" mark (from GIPHY's attribution marks,
  `assets/giphy-powered-by.png`) shows under the grid (its words until decoded), a failed search
  offers Try again (and is tried again as the picker opens) beside Replace key,
  and a sent GIF is reported to its `onsent` address with a random ID new each run. Previews
  (200 pixels wide) load through players of their own (below) while the tab shows them, whatever
  the setting for GIFs in messages says, still until hovered. A press sends `Outgoing::Gif`: the GIF's giphy.com
  page as plain text, which this app plays as a linked GIF and other apps show as a link. A received image from `mxc://` is a custom
  emoji, `data-mx-emoticon` or not: the SDK's sanitiser drops that attribute, which the spec's
  list of allowed attributes lacks. Received custom emoji draw as their picture
  inline in place of their short code (`El::inline_image`, 21 pt, within a body line so it
  never grows it), which selection and copying still see, so copying gives `:code:`;
  pictures load like other media on screen (`TimelineItem::emotes`, 128 px), and like it wait
  while the account keeps the message's media back (ADR 0011), reading as their short codes
  until it is shown.
- **Locations** (`matrix_core::geo`): `Outgoing::Location` sends `m.location` with a
  `geo:` URI, a pin asset (MSC3488) and the place's name as its body (else its coordinates).
  The picker uses a permitted device fix or a labelled approximate position from GeoJS, which
  receives the public IP when the picker needs an estimate. Search and typed coordinates remain
  available. Sending a place takes an explicit action. Live location is a separate room share
  for 1 hour, 1 day or until stopped (`LiveDuration`), using Element’s MSC3672 beacon info and
  related beacon events. "Until stopped" still names an end on each beacon, 24 days on: JavaScript timers (Element
  Web's among them) overflow past 2^31 − 1 ms, so a longer timeout would end the share at once
  there. A minute before it, the share carries on in a fresh beacon from the latest fix (a new
  card; `LiveLocationStarted` again, so Stop stops the newest), but only while the beacon is
  still the room's: renewal checks the server before writing, and after, that the fresh beacon
  replaced the old (`unsigned.replaces_state`, trusted when the server omits it) and is still
  current, else it stops the fresh beacon if still its own, so a stop from another device stands. An orderly quit stops the share. Only a device fix
  can start one in a room where the sender may publish both events. Room permissions let an
  admin set their level together. The picker can stop the current share, and each viewer can
  stop its own beacon. On Android the foreground service starts when sharing is requested and
  holds the latest fix until the first Matrix send succeeds.
  Desktop renders MapLibre Native snapshots off the UI thread, cached by coordinates, style and
  zoom. Both clients use the app’s bundled light and dark street styles with OpenFreeMap vector tiles. The sender’s avatar
  marks the place. A location card opens the **map viewer**: one map, near the window's size,
  of every recent place in that conversation (each sender's newest: a live share while it runs,
  a place shared within a day), and the opened card's own place in its sender's stead, however
  old, unless it is a live share that ended while they have a newer place (a renewed share's).
  The map centres on that person at street level, and on whoever has the newest place once
  theirs leaves; choosing someone on the map moves the focus without keeping their place past
  its time. A list over the map
  names who is there; choosing someone (a row or a pin) turns the map to them. The desktop drags
  the map and zooms with the wheel (toward the pointer) or buttons, from MapLibre snapshots of the
  map's size (`geo::framed_map`), placing pins by Web Mercator (`geo::project`); while a moved
  map's snapshot renders, the last one stays, moved and scaled into place. The phones use MapLibre's
  own interactive map. The viewer's bar opens the focused place in Google Maps.
  Turning off maps suppresses the renderer and tile requests. Nominatim still searches only on
  submit, with cached results and a one-second interval between requests.
- **Scheduled messages** (MSC4140 delayed events, `matrix_core::scheduled`): the button beside
  send (or right-clicking send) offers in 30 minutes, in an hour, tomorrow 09:00 and next Monday
  09:00, local time (on that day's clock, summer time and all); the server sends the message
  then, whether or not the app runs. The engine schedules any message (`Command::Schedule`
  takes an `Outgoing`: text, emotes, stickers, places, polls); the apps schedule the
  composer's message (text and emotes, and the commands that make one), while the sticker
  picker, the location dialog and the poll form send at once. Scheduled messages go
  with a reply (mentioning whom it answers) or into a thread (relations written as the SDK
  would); uploads can't (they go when sent), nor commands that do something rather than send
  (`/topic` waits, with a notice). The composer empties as sending does and, if the server
  refuses, what it held comes back with its reply (`Purpose::Scheduling`), at once if the composer
  is still empty, else as soon as it is free again (`State::refused`), as a refused edit does. Only a server whose
  `/versions` names `org.matrix.msc4140` is asked (one that doesn't is refused with a notice: it
  would ignore a delay on `/send` and post at once). Messages go only through MSC4140's own
  endpoint, in the proposal's current shape (`/delayed_event/…` with `delay_ms`; ruma 0.17 has an
  older shape, so `scheduled::api` defines these requests): Synapse first took a delay on
  `/send`, but a server ignoring it posts at once and nothing tells the two apart beforehand, so
  a server with only that is refused with a notice. Sending now and cancelling put the action in
  the path, else in the body as Synapse first had it; lists are read in either shape (`delay_ms`/`delayed_since_ts`/`finalised`, or
  `delay`/`running_since`/`finalised_ts`). In an encrypted room
  the message is encrypted before the server holds it: the SDK encrypts only what it sends
  itself, so each account's client is built on a base client of the engine's own
  (`account::Crypto`, through the builder's `base_client`, the same SQLite stores and defaults
  the builder would make) whose `OlmMachine` the engine drives as the SDK's send path does:
  members synced, unknown or outdated devices queried, sessions claimed, the room key shared
  (`BaseClient::share_room_key`), then `encrypt_room_event_raw`, under the store's cross-process
  lock and one at a time per account; a failed share discards the room key, as the SDK does. A bar above the room's composer counts the room's scheduled messages and lists them with
  send now and cancel (`Command::UpdateScheduled`); each account's list is asked for once a run,
  as the list opens and when one of its messages comes due, then every 30 s while the server
  still lists it (sent ones aren't listed), and comes back after every change. Each account's
  lists are read and reported one at a time (`Account::listing_scheduled`), so an older answer
  never covers a newer. A server that holds delayed events but can't list them (the bulk list is
  optional) shows the messages this account scheduled there, still to come, kept in its store
  across restarts, so they can still be sent now or cancelled. One this device can't decrypt shows as "Encrypted message".
  A message can still fail when its time comes (its sender lost the right to post, the room
  went): the server finalises it with an error and lists it no more. So this device keeps what
  it scheduled past its time until the server says what became of it (from the list, else
  `GET …/delayed_events/{delay_id}`, the one read MSC4140 keeps since the bulk list moved to
  MSC4486): a sent one is let go; one the server no longer knows (Synapse forgets them once it
  sends, or fails to) is looked for in the room: from its time on (the event the server finds
  at that time, then forward), else back from now, an event from this account between a minute
  before and two after its time that the server made of it (`unsigned`'s
  `org.matrix.msc4140.delay_id`, Synapse 1.150 on) or that says the same: there, it went;
  absent once two minutes have passed, it counts as failed (`NOT_POSTED`, a code the apps put
  in words; an event naming another delay never stands for it, and a few events past the
  window, their clocks being their servers', don't end the search). One sent early from another
  device ("send now" there) or delivered long after its time falls outside the window and reads
  as failed: rare, and the message is in the room to see; a failed one stays
  (`ScheduledMessage::failed`, its error), with a notice once, shown first in the bar and in the
  list with why, until dismissed (cancelling it asks nothing of the server). Unanswered ones go
  after eight days (servers keep finalised ones for about a week), failed ones after 30.
  Failures of messages scheduled from another device aren't followed: nothing on this device
  could dismiss them. When the server can't list them at all for now, the messages this device
  scheduled stand in (`Account::scheduled_or_known`), so one just scheduled never drops out of
  sight.
- **Composer layout:**
  - Room and thread editors grow up to eight lines, then scroll inside their own viewport,
    keeping the caret visible while typing or pasting. Vertical padding and the bottom bar stay outside
    that viewport so the controls remain visible; clicking its padding focuses the editor.
    The editor shrinks again when text is removed.
  - The wheel works over the text and its side padding. A Rune Kit scrollbar indicates the
    position and can be dragged without taking focus. Keyboard navigation and selection keep
    the caret visible, and a selection dragged past the editor's edge scrolls it while held
    (both Rune's). The caret in a code fence, whose Enter the composer handles, uses a
    content-space caret marker to reveal itself; only a changed selection or caret line
    requests a reveal, so scrolling by hand can leave the caret off screen.
  - The field comes first. A bottom bar holds attach, emoji, `Aa`, voice memo and poll, with send at
    the right. The formatting toolbar shows only while `Aa` is on.
    Formatting and recording controls wrap inside a narrow pane so resizing keeps them reachable.
  - Both bars `preserve_focus_on_press` and their buttons don't take focus, so formatting and
    emoji go where the caret is.
  - B with nothing selected arms bold for what is typed next. B on a selection marks exactly
    the selection: the composer remembers the mark and the selection's end (`fresh`), and a
    character typed right there switches the mark off first.
  - One emoji picker serves reactions and drafts; `Popover::Emoji(EmojiFor)` says where a
    picked emoji goes (`State::choose_emoji`). Its search (focused when it opens) matches every
    Unicode emoji by its English name and shortcodes (the `emojis` crate), up to 96; Enter picks
    the first. Without a search it shows the quick reactions, a tab per group, then (after a
    composer's room emoji) every emoji (skin tones left out) under its Unicode group, the
    crate's order. Rune builds only the rows in and near view (`virtual_rows`); a row's key
    names what it holds (its first emoji; a room emoji by image and short code), not its
    place. Room emoji arriving while the picker is open reopen the grid at its top (its key
    changes), so they show instead of landing above the rows Rune holds in view. A tab puts
    its group's title at the grid's top (`scroll_key_to`). Emoji draw in the system's
    colour emoji font, as everywhere else.
- **Drafts:** every room and thread keeps its own draft. The desktop's two composers each hold
  the draft of the timeline they write into (`Composer::owner`); when the timeline on screen
  changes, `State::follow_drafts` (from `sync_timeline`) sets the draft aside, with its reply and
  attachments (`composer::Draft`, in `State::drafts`), and brings back the new timeline's. Phones
  keep a composer per timeline already (`ComposerState`). The text and its formatting also go to
  the device's store through the SDK's composer drafts (`Command::SaveDraft` / `LoadDraft`,
  `NewMessage` drafts by room and thread root), so a draft survives a restart; replies and
  attachments don't. The engine runs an account's draft reads and writes one at a time in the
  order asked (`DraftQueue`), so a write never lands after a later clear. A draft being typed is
  written at most every 500 ms; an emptied one (sent or cleared), one being left, and the drafts
  on screen as the window loses focus (phones: as the conversation is left) at once, so a sent
  message never comes back. Quitting within half a second of the last keystroke can lose it.
  (`state/drafts.rs`, `state/actions/drafts.ts`.) A timeline asks the store once per run, on its
  first visit; its text goes in only if the composer still writes there, the draft's text is blank
  (a reply or files picked meanwhile stay; under an edit started meanwhile it becomes the draft set
  aside), no draft was written there since (reads and writes run in order, so the store holds that
  write), and the user hasn't changed it this run (`Kept::touched`, `ComposerState.touched`; typing in an edit or sending
  counts as changing it on phones only for the draft, not the edit), without a typing notice; if
  they had, what they left is written over it, even after leaving the timeline. Phones take the
  answer only for the read that asked (`DraftLoaded.request`, and a token per follow, so a read
  answered after sign-out is dropped), and put it under an edit started meanwhile as the draft
  it sets aside. Phones keep their own composer source (markers
  and all) as the body, so it comes back exactly, and, when it holds pills, the same source as
  the HTML with each pill as the desktop's field writes one (`<span data-pill="id">label</span>`,
  the rest escaped text), so mentions, `@room`, custom emoji and spoilers come back as the pills
  they were; phones read that HTML as their source only when it spells the body exactly, else
  as a message's. The desktop keeps text and HTML.
- **Uploads:** `Outgoing::File` carries an `Upload`, a path on disk or bytes with a name.
  - An upload whose name says it is a picture shows in the composer as a 64-point thumbnail,
    the picture alone cropped square with rounded corners and its remove button over its corner
    (no chip around it); other files, and pictures that can't be decoded, keep the chip with
    their name. On the desktop `State::upload_thumbnails` keeps one per image upload of the two
    composers, synced in `poll`: photos are read, decoded and cut to a 128-pixel square one at a
    time on a worker of their own, which wakes the window after each, so only that square is
    kept, never the whole photo, a batch never decodes at once, and no file is read on the UI
    thread. The square comes from `matrix_core::upload_thumbnail`: turned upright by its EXIF
    orientation, decoding bounded as the engine's thumbnails are (8192 pixels a side, 256 MiB).
    The middle square is scaled from the decoded picture in place and turned upright after, so
    the picture is never copied whole. Only regular files up to 64 MiB are read, looked at before
    they are opened (a pipe would block the worker); a job whose upload went before its turn is
    skipped, and one finished for an upload removed and attached again since is dropped (jobs
    have IDs). An SVG is read by the worker too and rasterized at the thumbnail's size by Rune. A thumbnail goes once its upload is sent, removed or set aside with
    its draft, and is made again when the draft comes back. Headless runs without a decoder show
    chips.
  - Pastes reach the field through `on_input_paste`. A pasted image joins the draft as
    `image.png`, `image-2.png` and so on; pasted files are attached; text pastes as text.
- **Voice and audio:**
  - Sound the app plays and records itself goes through `platform::sound`, on every desktop:
    cpal for the device (its PulseAudio host on Linux, which PipeWire serves, so device names
    match the ones calls see), pure-Rust decoders (symphonia for Ogg, WebM, MP4, MP3, FLAC and
    WAV; opuscule for Opus). Devices are the user's picks by name (Settings → Voice & video),
    else the system's default. Headless runs (`Shell` without a window) use Rune's `Player` and
    `Recorder` instead, whose devices are fake, so tests never touch a real one.
  - Voice memos record from the picked microphone into 16-bit mono WAV at 24 kHz (`.wav`, about 3 MB
    a minute), written on a worker once the microphone has closed (`State::voice_memo_recorded`
    sends it); a microphone that goes while recording fails the memo with a notice as soon as it's
    noticed, rather than sending what came before; the meter is smoothed like Rune's (peak falls 20
    dB/s, RMS 30 ms up, 300 ms down). While recording, one row replaces the field and the bottom
    bar: time, input level, discard and send. The bar's tools are for writing and would offer a
    second send. A memo belongs to the timeline it started in; once that leaves the screen (another
    room, the overview, the thread closed), it is dropped (`State::sync_timeline`). Its red dot
    breathes like a studio recording light (`recording_light`): the lamp dims a touch while a soft
    halo swells and fades over 1.6 s, on Rune's clock; still under reduced motion.
  - While recording, a voice glow surrounds the composer's frame, drawn under it (`voice_glow`): a
    faint blurred halo, and over it a brighter one whose opacity is bound live to the recorder's
    level (`Live::smooth`), so it swells with the voice without rebuilding the view. Under reduced
    motion the brighter halo holds still, half lit. The water that once lay along the bottom of
    the pane was dropped: it was too busy for a chat, and it stepped a simulation every frame.
  - Streams ask cpal for 20 ms blocks (`BufferSize::Fixed`) where the device takes a size.
    PulseAudio's default buffer is about 2 s: a clip would start that long after the press, and
    the recording level would lag as long.
  - A player opens its speaker only while its clip plays: play wakes its thread, which opens the
    stream; half a second after a pause or the clip's end (the last block drains, a quick
    pause and play reuses it) the stream closes, so a paused clip wakes nothing and lets the
    sound server suspend. After a minute paused, it also lets its decoded samples go, keeping
    its length, place and waveform, and decodes the saved copy again to play.
  - A memo is sent as `Outgoing::Voice`, a Matrix voice message (`AttachmentInfo::Voice` with
    its duration and a 100-point waveform: each stretch's RMS on a decibel scale from -50 dBFS,
    so quiet speech still shows). Received audio carries `AudioDetails` (whether it is a voice
    memo, its length and its waveform, from MSC3245's voice block and MSC1767's audio block).
  - Audio attachments and voice memos play on the picked speaker, from a copy saved once to the
    scratch folder (`SaveMedia`), decoded whole on a thread and resampled to the device's rate.
    Each clip downloads once a run (`State::audio_files`): played again, it opens the saved copy,
    so no download ever writes over a file a player still reads. Each pick of a clip has its own
    id, and a failure from a player picked before is ignored.
    A second press pauses. A file that can't be decoded, no speaker, or the speaker going while
    it plays (unplugged, the sound server gone) stops the clip with a notice
    (`State::audio_failed`).
  - A clip shows as a voice card, as in WhatsApp: play level with the waveform, as bars scaled
    to its loudest bar (up to 36, as many as the message's width fits: at least 8 in a roomy card, else at
    least 4 in a compact one with tighter padding; only where not even those fit beside the play
    button, as at 175% in the smallest window, do the bars go under it, across the card), a scrubber dot,
    and the time under the bars. A voice memo shows no file name; other audio keeps its name
    above the bars. Without a sender's waveform the bars are flat until the file is decoded
    here, which reads one; what a played clip's file said (waveform, length) is kept for its
    card after another clip plays (`State::decoded_audio`, the newest 64). Pressing or dragging along the bars (the slider reaches half a dot past them at
    each end, so the whole dot is the scrubber's, and a press focuses it for the keys) seeks; on a clip not yet playing,
    it starts from there (`State::seek_audio`). Focused, the bars are a slider: the arrow keys
    move a twentieth, Home and End go to the ends. A drag pauses the clip while the pointer is
    down and goes on where it is let go; a clip whose file comes while the pointer is down waits
    to be let go, as does a saved clip a press picks. A cancelled drag (Escape, focus lost)
    puts the clip back where it was, playing if it was, and unpicks a clip the press picked
    (`Scrub::Cancel`). Whether a clip is to play is kept apart from where it is (`AudioClip::play`):
    picking or playing sets it, pausing clears it, and a file that comes later honours it;
    while the file comes, the button shows pause for a queued play and a press takes it back. Any
    move to the very end (a drag, End) leaves the clip paused there until
    play, which starts it over (`State::scrub_audio`, `State::seek_audio`). A clip that plays to its end rewinds. In a
    message too narrow for the speed button beside the time, it takes a line under it. While a
    clip plays the view
    rebuilds as its progress passes each of 120 steps (`PROGRESS_STEPS`), not every frame.
  - Playback speed (1×, 1.5×, 2×) is one setting for every clip, stepped by the button beside a
    playing clip's time. Speeds other than 1× keep the pitch: the decoded clip is time-stretched
    by WSOLA (30 ms windows, a 10 ms search at about 8 kHz) on a worker, in segments of 4 s,
    each alone, starting with the one playing (after a seek, the one sought to); a segment not
    stretched yet plays at 1× (as does a clip of only a few windows), so a new speed or a seek
    waits for one segment (tens of ms), not the whole file. Segments join on the file's own
    samples (reading past a segment's last frame holds it, rather than dipping to silence);
    where what plays changes within a segment (its stretched copy arriving, another speed),
    the two crossfade over 10 ms, ending early at a join (`Playback::render`). Each stretch is a
    job: its worker stops once another speed starts a newer one or the player is dropped. The windows' places
    are spread evenly over the copy's due length (hops of at most half a window), each searched
    against the one before played on by its actual hop. The last window is the file's end,
    placed (within half a hop of the copy's due length) where its sound best
    continues the window before, so a segment ends without a dip and the copy is the file's
    length divided by the speed give or take a few ms (playback maps by the actual length).
    Each sample is divided by the window weight laid on it, so overlaps neither swell nor dip. The first window starts and the last ends at full strength, so neither end fades. The
    copy plays at its own sample rate, so the pitch holds; a place in the file is a place in the
    copy scaled by their lengths (`Playback::render`).
- **Inline previews:**
  - Attachments carry opaque `MediaRef`s (the file, and the sender's thumbnail if any) and the
    sender's dimensions.
  - A preview is the sender's thumbnail, else a server thumbnail (`MediaSize::PREVIEW`,
    800×600). It is sized from the sender's dimensions before loading, so rows don't jump;
    without them it gets a default box.
  - Only on-screen rows fetch media (rows themselves: [0006-timelines.md](0006-timelines.md)).
    `State::rows_shown` takes the visible rows plus 10 either side; only their previews, sender
    pictures and linked GIFs are fetched (`request_timeline_media`, `request_gifs`). Before the
    view reports, a timeline's newest 30 entries count.
- **Image cache:** `MediaCache` keeps decoded images within 64 MiB, evicting the least recently
  used; what shows counts as used each time the rows on screen are fetched for
  (`MediaCache::touch`). The SDK caches downloads on disk.
  - The pictures lists show (accounts, and the active account's rooms, spaces and invitations)
    are asked for only when those lists change (`Accounts::revision`), not on every event. A
    change that shows the same pictures (a fingerprint of them), all of them here, asks for
    none; one not here (a failed one whose wait is over) is asked for again.
  - Eviction leaves those pictures, and the budget doesn't count them (`MediaCache::keep`):
    they are bounded by the lists (a 128×128 thumbnail each, about 87 KB decoded with mips),
    and letting them go when they
    alone outgrew the budget would fetch and evict them again on every list change. The budget
    bounds the rest. Pictures a list no longer shows count again and may go.
  - An image whose download failed shows as failed while a retry loads, so its row keeps its
    height. The 512 failures to be tried again last are remembered (`FAILURES_KEPT`).
  - An image counts at the size fetched (a thumbnail, the sender's own included), not the
    original.
  - A thumbnail fits the box it was asked for: a server answers with the nearest rendition it
    made (Synapse answers 128×128 with 320×240), and the SDK answers one for an encrypted file
    with the original, so the engine scales a still PNG or JPEG larger than the box down to
    fit it (upright, premultiplied, keeping its colour profile), two at a time on blocking
    threads, before `MediaLoaded` (`thumbnail::fit_off_thread`). Decoding is bounded like
    Rune's: 8192 pixels a side, 256 MiB decoded, 64 MiB for a header's own buffers. A larger
    image, another format (animated PNGs among them) or a failure passes through unchanged. An
    encrypted original is scaled again on each fetch: the SDK caches the original.
  - It counts at an estimate until Rune decodes it, then `MediaCache::measure` (from `poll`)
    counts what Rune holds: RGBA at every mip level.
  - Animated frames are counted from the file without decoding (GIF image blocks, APNG `acTL`,
    WebP `ANMF`). All frames count unless Rune keeps only the first: when frames times its own
    estimate of a frame (`width × height × 16 / 3`) pass 256 MiB, the rule Rune applies.
  - A file Rune can't decode is marked and not fetched again.
- **Link cards:** the first link in a message that isn't a GIF gets a card from its page's
  Open Graph tags: site, title, description (cut at 300 characters) and image
  (`links::LinkPreview`). The homeserver fetches and caches the page (`preview_url`,
  authenticated, else the older endpoint; `Command::PreviewLink`), so the site never sees the
  user, and the image is homeserver media like any other. In encrypted rooms this tells the
  homeserver which links the user sees, which the setting's hint says.
  - Asked for only for rows on screen, once per link and account (each account's homeserver
    answers for itself), each request bounded (15 s, not retried);
    the older endpoint only where the server doesn't know the newer one. A page the server
    refuses to describe gets no card: any 4xx but "too many requests" (however worded: a proxy
    blocking previews answers with its own page), or a Matrix error from the homeserver itself
    (it couldn't fetch the page). So does one with no title or description. No answer, "too many
    requests" or a proxy's 5xx page is asked again after a wait (`LinkPreviews`, which keeps 500
    cards and 500 failed links).
  - Links are read without the punctuation around them, keeping a closing parenthesis the link
    opened (`…/Rust_(programming_language)`). While linked GIFs don't play, a GIF link gets a
    card like any other (`TimelineItem::card_link`).
  - The card sits below the message text; pressing it opens the link. Its image loads like the
    other media on screen, as a 160-pixel thumbnail; one that fails to load is left out.
    Settings → Messages & media → Link previews turns them off (on by default).
- **Linked GIFs:** each holds at most 48 MiB of frames, and at most 16 play. A GIF whose frames
  outgrow that keeps every other frame, each shown as long as the pair it stands for, until they
  fit (a single frame too big shows still), so a long GIF plays at a lower frame rate rather than
  freezing.
  - Past 128 MiB of frames across them, those off screen are forgotten, the one shown longest
    ago first, and fetched again if they come back; those on screen keep theirs. Each frame is
    still copied into the picture as it shows: Rune's `Pixels::set` takes an owned buffer.
  - One stopped to make room is marked, not forgotten, so it isn't refetched while shown.
  - GIFs of timelines no longer shown, or all with linked GIFs off, are let go; a download
    finishing after that is dropped, not played.
  - Only those on screen play, and only while the window is focused; the rest hold their
    frame.
  - The GIF picker's previews have players of their own (`Picker::players`, at most 48), so a
    picker never pauses, stops or shares a timeline's GIF. They are still: each holds its first
    frame and plays only while the pointer is over it (`GifCache::keep_still`), so a grid of
    them stays calm; a player starts held when it wouldn't play, and a held player decodes only
    its first frame until it first plays, so a grid of still previews holds one frame each. A player lets go of the file once decoded.
  - A download that isn't a GIF (`GIF8`) is marked broken: the link shows as text.
- **Retries:** a failed download (media or linked GIF) waits 5 s, doubling up to 5 minutes, and
  is fetched again only if still on screen (`state::retry::Retries`). `poll` wakes when the
  next wait ends; a success forgets the failures.
- **Attachment preview:** `State::open_attachment` asks `Opens`: an image, PDF, video or audio
  clip opens the preview, any other file goes straight to the save dialog. It opens from a
  message or the room's Files panel.
  - `State::preview` is its own slot, drawn over everything including dialogs, because it holds
    live resources. A black layer at 86% opacity, the name top left, Download and Close top
    right; Esc or a press beside the file closes it.
  - _Attachment batch:_ Matrix carries one attachment per message, so files dropped in together
    arrive as a run of messages. The batch of the opened one (`preview::batch`) is the run
    around it of attachment messages the timeline groups under one header
    (`continues_group`: one sender, each within 5 minutes of the one before, none a reply), with
    nothing drawn between them (read markers, undrawn entries and room changes the timeline
    leaves to the room log don't break it; any other message, a room event it draws or a date
    divider does), the previewable ones kept. Two or more get a
    strip of thumbnails along the bottom (the batch's previews, fetched like the timeline's
    rows on screen; else the kind's icon; it scrolls sideways when they don't fit, and whatever moves the
    preview, opening included, scrolls the current one into view on the next `poll`), arrows at the sides, and "2 of 4" beside the name. Left and Right (when nothing
    focused takes them, as a scrubber does), the side arrows or a thumbnail step through it,
    round past either end (`State::step_preview`, `State::show_preview_item`). Leaving an item
    lets go of it: a PDF render stops, a video's player is dropped, a clip pauses; closing does
    the same. Phones use the same rule ([0010-mobile.md](0010-mobile.md)).
  - Space plays or pauses the video or clip shown (`State::toggle_preview_playback`). The video
    scrubber is a Rune Kit slider with continuous seeking, keyboard steps, and an accessible
    playback time.
  - A press on an audio card beside its controls opens the clip in the preview. Audio shows
    the timeline's voice card at 420 points under a large audio glyph
    (`timeline::audio_player`): it plays through `State::audio` like any card, so speed,
    scrubbing and the saved copy are the same. It waits for play; moving past it stays quiet.
  - Each showing of an item is numbered (`Preview::visit`) and its file's fetch carries the
    number (`Purpose::Preview`): what an earlier showing of the same file asked for, failed or
    not, is dropped. A PDF render's pages carry the render's number
    (`DocumentStatus::Rendering`), so pages an earlier render still sends are dropped too.
  - Linux packages require GStreamer's base and good plugins (playbin, MP4, WebM, VP8/VP9) and
    recommend libav (H.264, HEVC; required on Arch). The RPM names GStreamer's libraries by
    soname and its plugins by either distribution's name (`(gstreamer1-plugins-good or
    gstreamer-plugins-good)`), so Fedora and openSUSE both install it whole, and recommends the
    H.264 and AAC decoders by the elements they provide (`gstreamer1(element-openh264dec)`).
  - A clip and a video that share one upload share its scratch download: saved or failed, both
    hear it. The AppImage bundles GStreamer whole
    (linuxdeploy's GStreamer plugin): its core with the build machine's plugins, and nothing else.
  - Video saves the original to the scratch folder once a run (`SaveMedia`, the scratch file
    map clips use), then plays from there, at once (`platform::video::Player`). Rune's player
    covers macOS (AVFoundation) and Windows (Media Foundation), on the system's default
    speaker; Rune has none for Linux at any version, so there GStreamer's `playbin` decodes
    with whatever codecs the system has (hardware decoding where its plugins offer it) into
    RGBA frames handed to `rune::Pixels` from its streaming thread, with sound on the picked
    speaker (the GStreamer device whose name matches the settings', else the default). A bus
    sync handler notes loading, a finished seek, the end and errors, and wakes the view (a seek
    after the end pauses the pipeline first, which GStreamer leaves playing); a `videoflip` turns
    pictures recorded sideways upright by their orientation tag; time and length are
    queried on the UI thread when drawn; play at the end (played or moved there) starts over,
    with either player.
    GStreamer starts on a worker at launch (`video::prepare`): its first start scans the
    plugins, seconds with no registry cached. Its players open on a worker too
    (`Shell::open_video`, `VideoStatus::Opening`), handed back paused with the showing's number
    (`State::video_opened`), so a video opened during that scan never waits on the UI thread;
    it plays only if that showing is still on screen, after the clip playing pauses. Stopping a pipeline happens on a thread of its own. A video starting pauses the clip playing.
    Headless runs get a stand-in that keeps what it is asked and plays nothing
    (`video::Detached`). The timeline's thumbnail is the poster until the first frame; under
    the picture, play/pause (replay once ended), time and length, a scrubber (drag, press, the
    arrow keys and Home/End when focused) and mute. A playing video rebuilds the view as its
    progress passes each of 120 steps, like a clip. A video whose sender uploaded a thumbnail
    shows it in the timeline with a play button (`Attachment::preview`; servers don't make
    video thumbnails); without one it keeps its file card.
  - Right-click opens the same file actions used by attachment messages: Copy image for
    images, Download and Open in the default application. The preview and its context menu
    each trap input above the background; Escape closes the menu before the preview.
  - Images fetch the original (`MediaSize::Original`) and decode outside `MediaCache`, since a
    thumbnail may share the original's `MediaRef`.
    The cached timeline thumbnail shows immediately while the original downloads and decodes.
    Downloading, decoding, ready and failed are explicit states; `poll` promotes a finished
    decode once so the decoder's wake rebuilds the viewer without waiting for another UI action.
    A failed decode shows an error and keeps the thumbnail available.
  - PDFs render on a worker with hayro, a pure-Rust rasteriser, so no platform needs a system
    library. Pages arrive one by one as `rune::Pixels` at 2× point size, up to 60.
  - A PDF render stops at 40 million pixels (160 MiB), draws pages above 8 million pixels
    smaller, and stops when the preview closes.
- **Attachment actions:** `AttachmentAction` flows through `State::act_on_attachment`. Copy
  image uses Rune's clipboard worker to encode and write the preview's ready original. When
  the original isn't decoded in the preview, it fetches and converts the original on a worker.
  Open saves the original to a unique path in the private scratch folder, then asks the
  OS default application on a worker; the file lives until the app exits. Open is available
  only for recognized document, media and archive extensions, checked in both the menu and
  the state action; executables, scripts and unknown types stay available through Download.
  Edits made to an opened scratch copy are temporary too: Download is the path for keeping a
  file and edits to it. Download uses the save dialog. Fetch, conversion and open failures
  become notices.
- **Download:** Rune's save dialog returns the chosen path. `Command::SaveMedia` downloads the
  original and writes it in the engine.
- **Pointer cursors** mark whatever a press acts on: components with a button, menu, radio or
  tab role, message rows (they open the thread), attachments. Message text keeps the text
  cursor.

## Consequences

- What you type is what others see, through one model. Tags outside Rune's subset and the pills
  (tables, colours) render as plain text.
- Formulas read as Unicode, not typeset: fractions and roots stay on one line.
- Scheduled messages depend on the server enabling MSC4140 (Synapse's
  `max_event_delay_duration`, which also caps how far ahead); a server without it refuses with a
  notice. Their encryption follows the SDK's send path by hand: an SDK upgrade must be checked
  against `Room::preshare_room_key` and `ensure_room_encryption_ready`. The SDK's own locks
  around claiming sessions and sharing a room's key are private, so a scheduled message and a
  message sent at the same moment can claim sessions side by side (the crypto store's
  cross-process lock still keeps their writes apart). A message scheduled in
  an encrypted room may go out with a room key the room rotated since; members who were given
  that key still decrypt it, a member who joined in between can't.
- A scheduled message in an encrypted room is encrypted, and its room key shared, when it is
  scheduled: whoever is a member then can read it once it is sent, even after leaving the room
  meanwhile (where history visibility lets them see it). Encrypting at the moment of sending
  would need the user's device then, which is what scheduling on the server avoids.
- Refused scheduled drafts that wait for a busy composer live for the run, as refused edits do;
  quitting before the composer is free again drops them. The message itself was never sent.
- OpenFreeMap receives the device IP and map area when maps are enabled. Nominatim receives
  submitted place searches. GeoJS receives the device IP for an approximate starting position.
- On the desktop, `@room` in a draft kept across a restart comes back as text: drafts keep
  Matrix HTML, where it is plain words (phones keep it as a pill).
- Only what the user looks at is downloaded, and a server hiccup heals without a restart.
- Two media with the same bytes share one decode in Rune but count twice here; an animation
  whose frame count can't be read counts as one frame.
- Memos are WAV rather than Opus: no pure-Rust Opus encoder exists. Video downloads rather
  than plays.
- hayro can't render password-protected PDFs; the preview says so and Download still works.
