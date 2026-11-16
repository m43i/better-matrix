# 0007. Composer and media

Status: Accepted
Date: 2026-11-14

## Context

Matrix carries formatting as HTML (`org.matrix.custom.html`) beside a plain body, and the
timeline must render what others send safely. Media can be encrypted, large and numerous, so
decoded pictures and PDF pages need memory limits. People expect Slack's composer, pasted
screenshots, voice memos, and images and PDFs that open large.

## Decision

- **Rich text through Rune's `TextEdit`:**
  - The composer edits a rich `TextEdit` limited to what Matrix HTML carries: bold, italic,
    strike, code, links, lists, quotes, code blocks.
  - Formatted text is sent as `to_html()` with the plain body; unformatted text goes plain.
  - Received HTML is parsed once on arrival with `TextEdit::from_html` (a sanitising parser)
    and shown as selectable read-only text. Every bare web address in a message, plain or
    formatted, becomes a link mark there too (`links::link_ranges`), except inside code and
    where the sender already linked the words; plain text without links stays plain selectable
    text. Pressing a link opens it in the browser (`on_link`, `State::open_link`).
  - A pasted lone web address goes into the draft as a link, and pasted over a selection links
    the selection (`Composer::paste_link`, one undo step); text typed after it is plain.
  - `Format` lists every toolbar action; the toolbar renders the list.
- **Composer layout:**
  - Room and thread editors grow up to eight lines, then scroll inside their own viewport,
    keeping the caret visible while typing or pasting. Vertical padding and the bottom bar stay outside
    that viewport so the controls remain visible; clicking its padding focuses the editor.
    The editor shrinks again when text is removed.
  - The wheel works over the text and its side padding. A Rune Kit scrollbar indicates the
    position and can be dragged without taking focus. Keyboard navigation and selection keep
    the caret visible. Pointer selections use a content-space caret marker to reveal their
    moving end too; only a changed selection or caret line requests a reveal, so scrolling by
    hand can leave the caret off screen.
  - The field comes first. A bottom bar holds attach, emoji, `Aa`, voice memo and poll, with send at
    the right. The formatting toolbar shows only while `Aa` is on.
  - Both bars `preserve_focus_on_press` and their buttons don't take focus, so formatting and
    emoji go where the caret is.
  - B with nothing selected arms bold for what is typed next. B on a selection marks exactly
    the selection: the composer remembers the mark and the selection's end (`fresh`), and a
    character typed right there switches the mark off first.
  - One emoji picker serves reactions and drafts; `Popover::Emoji(EmojiFor)` says where a
    picked emoji goes (`State::choose_emoji`). Its search (focused when it opens) matches every
    Unicode emoji by its English name and shortcodes (the `emojis` crate), up to 96; Enter picks
    the first. Without a search it shows the quick reactions and the curated groups.
- **Uploads:** `Outgoing::File` carries an `Upload`, a path on disk or bytes with a name.
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
  - While recording, a glow lies along the bottom of the pane it records in (the room or the
    thread panel), under the timeline and composer, and moves like water (`state::water`,
    `voice_water`). Loudness sloshes it from wall to wall, where it runs up; a sharp sound
    (peak well above loudness) throws sparks off the walls. It fades to nothing above its
    surface. Rune reports loudness, not pitch, so that is what it follows.
  - The water is a height field across the width, stepped on `poll`'s clock (virtual when
    headless) and drawn into a `Pixels` image at half resolution, so it never rebuilds the
    view. It rests while the window is unfocused and under reduced motion
    (`State::reduced_motion`, which the view sets for the state's animations).
  - A memo is sent as `Outgoing::Voice`, a Matrix voice message (`AttachmentInfo::Voice` with
    its duration).
  - Audio attachments and voice memos play on the picked speaker, from a copy saved once to the
    scratch folder (`SaveMedia`), decoded whole on a thread and resampled to the device's rate.
    A second press pauses. A file that can't be decoded, no speaker, or the speaker going while
    it plays (unplugged, the sound server gone) stops the clip with a notice
    (`State::audio_failed`).
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
  - An image whose download failed shows as failed while a retry loads, so its row keeps its
    height.
  - An image counts at the size fetched (a thumbnail, the sender's own included), not the
    original.
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
    Settings → General → Link previews turns them off (on by default).
- **Linked GIFs:** each holds at most 48 MiB of frames (beyond that, its first frame only), and
  at most 16 play.
  - One stopped to make room is marked, not forgotten, so it isn't refetched while shown.
  - GIFs of timelines no longer shown, or all with linked GIFs off, are let go; a download
    finishing after that is dropped, not played.
  - Only those on screen play, and only while the window is focused; the rest hold their
    frame. A player lets go of the file once decoded.
  - A download that isn't a GIF (`GIF8`) is marked broken: the link shows as text.
- **Retries:** a failed download (media or linked GIF) waits 5 s, doubling up to 5 minutes, and
  is fetched again only if still on screen (`state::retry::Retries`). `poll` wakes when the
  next wait ends; a success forgets the failures.
- **Attachment preview:** `State::open_attachment` asks `Opens`: an image or PDF opens the
  preview, any other file goes straight to the save dialog.
  - `State::preview` is its own slot, drawn over everything including dialogs, because it holds
    live resources. A black layer at 86% opacity, the name top left, Download and Close top
    right; Esc or a press beside the file closes it.
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
- **Download:** the save dialog (rfd) runs on a worker; `Command::SaveMedia` downloads the
  original and writes it in the engine.
- **Pointer cursors** mark whatever a press acts on: components with a button, menu, radio or
  tab role, message rows (they open the thread), attachments. Message text keeps the text
  cursor.

## Consequences

- What you type is what others see, through one model. Tags outside Rune's subset (tables,
  colours, spoilers) render as plain text.
- Only what the user looks at is downloaded, and a server hiccup heals without a restart.
- Two media with the same bytes share one decode in Rune but count twice here; an animation
  whose frame count can't be read counts as one frame.
- Memos are WAV rather than Opus: no pure-Rust Opus encoder exists. Video downloads rather
  than plays.
- hayro can't render password-protected PDFs; the preview says so and Download still works.
