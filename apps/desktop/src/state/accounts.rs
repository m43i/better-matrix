//! Everything the engine has told us about each account: the client-side store the views read.
//! Pure data and pure updates, so it is tested without a window or a server.

use std::collections::{HashMap, HashSet};

use matrix_core::{
    AccountId, AccountSummary, Attachment, EntryKind, Message, MessageContent, NotificationMode, RoomChange, RoomKind,
    RoomList, RoomSummary, SpaceSummary, TimelineDiff, TimelineEntry, TimelineKey,
};
use rune::{TextEdit, Tx};

/// Seconds reserved for the "New" divider's fade before its row closes.
pub const READ_MARKER_FADE: f32 = 2.5;

/// Whether an attachment is visual media the user's media choice may hold back: a picture
/// (stickers included) or a video, poster or not. Audio and other files never wait.
pub fn visual(attachment: &Attachment) -> bool {
    matches!(attachment.kind, matrix_core::AttachmentKind::Image | matrix_core::AttachmentKind::Video)
}

/// Whether a room should draw attention: any unread message, or only mentions when the room
/// notifies for mentions only or is muted.
pub fn wants_attention(room: &RoomSummary) -> bool {
    // Marked unread by the user, whatever the room's notifications.
    room.marked_unread
        || match room.notifications {
            NotificationMode::AllMessages => room.unread > 0 || room.mentions > 0,
            NotificationMode::MentionsOnly | NotificationMode::Mute => room.mentions > 0,
        }
}

/// What a room showed before a change sent to the server, to show again if it is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomUndo {
    /// Where it was among the favourites and low priority rooms.
    Priority(matrix_core::RoomPriority),
    /// Whether it was marked unread.
    Mark(bool),
    /// Its unread mark and counts before it was read.
    Read { marked: bool, unread: u64, mentions: u64 },
}

/// Which slice of an account's rooms the sidebar shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// Every room and direct chat.
    #[default]
    Home,
    /// The rooms in one space and its subspaces.
    Space(String),
}

/// A message ready to render: formatted bodies are parsed once, when they arrive.
#[derive(Debug)]
pub struct TimelineItem {
    pub message: Message,
    /// The sender's formatting, parsed from Matrix HTML into Rune's rich-text model.
    pub rich: Option<RichText>,
    /// A GIF linked from the text, shown below it like an attachment.
    pub gif: Option<GifLink>,
    /// The first other link in the text, which gets a preview card.
    pub link: Option<String>,
    /// The custom emoji its text shows, to fetch while it is on screen.
    pub emotes: Vec<matrix_core::MediaRef>,
}

/// One entry of a room's timeline, ready to render.
#[derive(Debug)]
pub struct Entry {
    /// Stable across updates; the element key.
    pub id: String,
    pub kind: EntryView,
}

/// Messages from one sender this close together share one header.
const GROUP_WINDOW_MS: u64 = 5 * 60_000;

/// Whether `next` joins `previous`'s group, under its header, when nothing comes between them:
/// the same sender, soon after, and not a reply (a reply shows what it answers above it).
pub fn continues_group(previous: &Message, next: &Message) -> bool {
    previous.sender_id == next.sender_id
        && next.timestamp_ms.saturating_sub(previous.timestamp_ms) < GROUP_WINDOW_MS
        && next.reply_to.is_none()
}

#[derive(Debug)]
pub enum EntryView {
    Message(Box<TimelineItem>),
    RoomEvent(Box<matrix_core::RoomEvent>),
    DateDivider(u64),
    ReadMarker,
    Start,
    /// Drawn only while hidden events show (`Settings::show_hidden_events`); otherwise it
    /// only keeps diff indices aligned with the engine's.
    Hidden(Box<matrix_core::HiddenEvent>),
}

impl From<TimelineEntry> for Entry {
    fn from(entry: TimelineEntry) -> Self {
        let kind = match entry.kind {
            EntryKind::Message(message) => EntryView::Message(Box::new(TimelineItem::new(*message))),
            EntryKind::RoomEvent(event) => EntryView::RoomEvent(event),
            EntryKind::DateDivider(at) => EntryView::DateDivider(at),
            EntryKind::ReadMarker => EntryView::ReadMarker,
            EntryKind::Start => EntryView::Start,
            EntryKind::Hidden(event) => EntryView::Hidden(event),
        };
        Self { id: entry.id, kind }
    }
}

impl Entry {
    pub fn message(&self) -> Option<&Message> {
        match &self.kind {
            EntryView::Message(item) => Some(&item.message),
            _ => None,
        }
    }

    /// The pictures it shows of people: a message's sender, a room event's sender and the
    /// person it was done to.
    pub fn avatars(&self) -> impl Iterator<Item = &matrix_core::MediaRef> {
        let (first, second) = match &self.kind {
            EntryView::Message(item) => (item.message.sender_avatar.as_ref(), None),
            EntryView::RoomEvent(event) => {
                let target = match &event.change {
                    RoomChange::Invited(person)
                    | RoomChange::InviteRevoked(person)
                    | RoomChange::Removed(person)
                    | RoomChange::Banned(person)
                    | RoomChange::Unbanned(person) => person.avatar.as_ref(),
                    _ => None,
                };
                (event.sender.avatar.as_ref(), target)
            }
            _ => (None, None),
        };
        first.into_iter().chain(second)
    }

    /// The custom emoji its text shows.
    pub fn emotes(&self) -> &[matrix_core::MediaRef] {
        match &self.kind {
            EntryView::Message(item) => &item.emotes,
            _ => &[],
        }
    }

    /// The map of the place it shares, if it shares one.
    pub fn location_map(&self) -> Option<&matrix_core::MediaRef> {
        match &self.message()?.content {
            MessageContent::Location(location) => location.map.as_ref(),
            _ => None,
        }
    }

    /// The image attachment it shows a preview of, if any.
    pub fn image(&self) -> Option<&Attachment> {
        match &self.message()?.content {
            MessageContent::Attachment(attachment) if attachment.preview().is_some() => Some(attachment),
            _ => None,
        }
    }

    /// The link in its text that gets a preview card, if any (see [`TimelineItem::card_link`]).
    pub fn preview_link(&self, gifs_shown: bool) -> Option<&str> {
        match &self.kind {
            EntryView::Message(item) => item.card_link(gifs_shown),
            _ => None,
        }
    }

    /// The GIF linked from its text, if any.
    pub fn gif_link(&self) -> Option<&str> {
        match &self.kind {
            EntryView::Message(item) => item.gif.as_ref().map(|gif| gif.url.as_str()),
            _ => None,
        }
    }
}

/// A room's timeline as its subscription built it, and where loading older history stands.
#[derive(Debug, Default, PartialEq, Eq)]
enum ReadBoundary {
    #[default]
    Unknown,
    Absent,
    Unresolved(String),
    Loaded(String),
}

/// The divider keeps its row until its opacity has finished fading.
#[derive(Debug)]
enum ReadMarkerFade {
    Pending(ReadMarker),
    Running { marker: ReadMarker, until: f64 },
}

impl ReadMarkerFade {
    fn marker(&self) -> &ReadMarker {
        match self {
            Self::Pending(marker) | Self::Running { marker, .. } => marker,
        }
    }
}

/// A visible divider's boundary, independent of the SDK's advancing read marker.
#[derive(Debug)]
struct ReadMarker {
    id: String,
    previous: Option<String>,
    next: Option<String>,
}

#[derive(Debug, Default)]
pub struct RoomTimeline {
    pub entries: Vec<Entry>,
    read_boundary: ReadBoundary,
    /// The first entry after a "New" marker the user has already reached. Kept across diffs
    /// and reopening a warm timeline; a marker at a new boundary is shown again.
    seen_read_marker: Option<String>,
    /// Once caught up, advancing receipts stays invisible until a later inactive period.
    read_marker_hidden: bool,
    read_marker_fade: Option<ReadMarkerFade>,
    /// The actual viewport's marker, without the media prefetch margin.
    pub visible_read_marker: Option<String>,
    /// The last actual viewport message, excluding the media prefetch margin.
    pub read_up_to: Option<String>,
    /// A page of older history is on its way.
    pub loading: bool,
    /// Nothing older exists, or this user may not see it.
    pub reached_start: bool,
    /// Loading older history failed; nothing loads by itself until the user retries.
    pub history_error: Option<String>,
    /// Changes whenever the timeline may have, and is never the same for two timelines: the
    /// view replans its rows only when it does. 0 for one never changed.
    pub revision: u64,
    /// Whether the view lands this frame's rows whole rather than easing them in: history and
    /// a timeline's first content do ([`RoomTimeline::adds_history`]).
    pub landing: Landing,
    /// Each sender's latest name and picture, for read indicators, at the revision it was
    /// built for.
    senders: std::sync::Mutex<Option<(u64, SenderIndex)>>,
}

/// Where a timeline's whole-landing change stands, frame by frame: it lands in the view of the
/// frame after it arrives, and only that one, so news after it eases in again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Landing {
    #[default]
    Idle,
    /// Arrived since the last frame began.
    Due,
    /// Landing in this frame's view.
    Now,
}

/// Senders by user ID, as [`RoomTimeline::sender`] keeps them.
type SenderIndex = std::sync::Arc<HashMap<String, matrix_core::Person>>;

/// Which entries of a timeline its view shows (plus a margin), as it last reported: their
/// range at the timeline's `revision` then, and their IDs, which outlive changes to the range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub revision: u64,
    pub range: std::ops::Range<usize>,
    pub ids: HashSet<String>,
}

/// The next [`RoomTimeline::revision`], unique in the process.
fn next_revision() -> u64 {
    static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl RoomTimeline {
    /// The fading divider and its insertion point in the current entries.
    pub fn fading_read_marker(&self) -> Option<(&str, usize)> {
        let marker = self.read_marker_fade.as_ref()?.marker();
        let index = marker
            .next
            .as_ref()
            .and_then(|id| self.entries.iter().position(|entry| &entry.id == id))
            .or_else(|| {
                marker
                    .previous
                    .as_ref()
                    .and_then(|id| self.entries.iter().position(|entry| &entry.id == id))
                    .map(|at| at + 1)
            })
            .unwrap_or(self.entries.len());
        Some((&marker.id, index))
    }

    pub fn read_marker_opacity(&self, id: &str) -> f32 {
        if matches!(&self.read_marker_fade, Some(ReadMarkerFade::Running { marker, .. }) if marker.id == id) {
            0.0
        } else {
            1.0
        }
    }

    /// Starts or ends the "New" divider's fade. `hold` keeps a fade from starting in a frame
    /// whose change lands at once, which would land the fade too: it starts the next frame.
    fn poll_read_marker(&mut self, tx: &mut Tx, reduced_motion: bool, hold: bool) -> bool {
        let Some(fade) = self.read_marker_fade.take() else { return false };
        let mut started = false;
        let (marker, until) = match fade {
            ReadMarkerFade::Pending(marker) if hold => {
                self.read_marker_fade = Some(ReadMarkerFade::Pending(marker));
                tx.wake_at(tx.clock());
                return false;
            }
            ReadMarkerFade::Pending(marker) => {
                // Viewport reports are not input. Explicitly animate this acknowledgement,
                // including when the marker was visible on the room's first frame.
                tx.animated();
                self.revision = next_revision();
                started = true;
                let duration = if reduced_motion { 0.15 } else { f64::from(READ_MARKER_FADE) };
                (marker, tx.clock() + duration)
            }
            ReadMarkerFade::Running { marker, until } => (marker, until),
        };
        if tx.clock() >= until {
            self.revision = next_revision();
            return true;
        }
        self.read_marker_fade = Some(ReadMarkerFade::Running { marker, until });
        tx.wake_at(until);
        started
    }

    pub fn set_read_boundary(&mut self, event_id: Option<String>, loaded: bool) {
        let previous = match &self.read_boundary {
            ReadBoundary::Unresolved(id) | ReadBoundary::Loaded(id) => Some(id.as_str()),
            _ => None,
        };
        if previous != event_id.as_deref() {
            self.seen_read_marker = None;
        }
        self.read_boundary = match event_id {
            None => ReadBoundary::Absent,
            Some(id) if loaded => ReadBoundary::Loaded(id),
            Some(id) => ReadBoundary::Unresolved(id),
        };
    }

    /// An omitted SDK marker can mean its event is still outside loaded history.
    pub fn may_mark_fully_read(&self) -> bool {
        !self.has_unseen_read_marker()
            && (self.seen_read_marker.is_some()
                || match &self.read_boundary {
                    ReadBoundary::Unknown | ReadBoundary::Unresolved(_) => false,
                    ReadBoundary::Absent | ReadBoundary::Loaded(_) => true,
                })
    }

    fn read_marker_boundary(&self, id: &str) -> Option<&str> {
        let at = self.entries.iter().position(|entry| entry.id == id && matches!(entry.kind, EntryView::ReadMarker))?;
        Some(
            self.entries[at + 1..]
                .iter()
                .find(|entry| matches!(entry.kind, EntryView::Message(_) | EntryView::RoomEvent(_)))
                .unwrap_or(&self.entries[at])
                .id
                .as_str(),
        )
    }

    pub fn read_marker_seen(&self, id: &str) -> bool {
        self.read_marker_hidden
            || self.read_marker_boundary(id).is_some_and(|boundary| self.seen_read_marker.as_deref() == Some(boundary))
    }

    pub fn has_unseen_read_marker(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry.kind, EntryView::ReadMarker) && !self.read_marker_seen(&entry.id))
    }

    /// Dismisses only the boundary actually reported by the view, retaining the engine's
    /// entries so subsequent diffs keep their indices.
    pub fn see_read_marker(&mut self, id: &str) -> bool {
        let Some(boundary) = self.read_marker_boundary(id).map(ToOwned::to_owned) else { return false };
        self.seen_read_marker = Some(boundary);
        self.read_marker_hidden = true;
        true
    }

    pub fn show_read_marker(&mut self) {
        self.read_marker_fade = None;
        self.read_marker_hidden = false;
        self.seen_read_marker = None;
    }

    /// New boundaries need no indicator while the focused user is already following live.
    pub fn see_current_read_marker(&mut self) {
        let id =
            self.entries.iter().find(|entry| matches!(entry.kind, EntryView::ReadMarker)).map(|entry| entry.id.clone());
        if let Some(id) = id {
            self.see_read_marker(&id);
        }
    }

    /// Whether `diffs` bring history rather than news: a message or room event above all the
    /// timeline holds (a page of older messages), or its first one. A timeline fills with these
    /// as it opens; they land as they are rather than easing in one by one. Dividers and
    /// markers count for neither.
    pub fn adds_history(&self, diffs: &[TimelineDiff]) -> bool {
        let content = |kind: &EntryKind| matches!(kind, EntryKind::Message(_) | EntryKind::RoomEvent(_));
        // News at the end of a timeline holding content, the usual change, needs no more look.
        let appends = diffs.iter().all(|diff| matches!(diff, TimelineDiff::PushBack(_) | TimelineDiff::Append(_)));
        if appends
            && self.entries.iter().any(|entry| matches!(entry.kind, EntryView::Message(_) | EntryView::RoomEvent(_)))
        {
            return false;
        }
        if !diffs.iter().any(|diff| match diff {
            TimelineDiff::PushFront(entry) | TimelineDiff::PushBack(entry) | TimelineDiff::Insert { entry, .. } => {
                content(&entry.kind)
            }
            TimelineDiff::Append(entries) => entries.iter().any(|entry| content(&entry.kind)),
            _ => false,
        }) {
            return false;
        }
        // Which entries are content, followed through the diffs as `apply` takes them.
        let mut shown: Vec<bool> = self
            .entries
            .iter()
            .map(|entry| matches!(entry.kind, EntryView::Message(_) | EntryView::RoomEvent(_)))
            .collect();
        let above_all = |shown: &[bool], index: usize| !shown[..index].contains(&true);
        for diff in diffs {
            match diff {
                TimelineDiff::PushFront(entry) if content(&entry.kind) => return true,
                TimelineDiff::PushFront(_) => shown.insert(0, false),
                TimelineDiff::Insert { index, entry } if *index <= shown.len() => {
                    if content(&entry.kind) && above_all(&shown, *index) {
                        return true;
                    }
                    shown.insert(*index, content(&entry.kind));
                }
                TimelineDiff::PushBack(entry) => {
                    if content(&entry.kind) && above_all(&shown, shown.len()) {
                        return true;
                    }
                    shown.push(content(&entry.kind));
                }
                TimelineDiff::Append(entries) => {
                    if entries.iter().any(|entry| content(&entry.kind)) && above_all(&shown, shown.len()) {
                        return true;
                    }
                    shown.extend(entries.iter().map(|entry| content(&entry.kind)));
                }
                TimelineDiff::Set { index, entry } if *index < shown.len() => shown[*index] = content(&entry.kind),
                TimelineDiff::Remove { index } if *index < shown.len() => {
                    shown.remove(*index);
                }
                TimelineDiff::PopFront if !shown.is_empty() => {
                    shown.remove(0);
                }
                TimelineDiff::PopBack => {
                    shown.pop();
                }
                TimelineDiff::Truncate { length } => shown.truncate(*length),
                TimelineDiff::Clear => shown.clear(),
                TimelineDiff::Reset(entries) => shown = entries.iter().map(|entry| content(&entry.kind)).collect(),
                _ => {}
            }
        }
        false
    }

    /// Applies the engine's diffs, in order. Indices outside the list are ignored rather than
    /// trusted: a stray diff can't crash the app.
    pub fn apply(&mut self, diffs: Vec<TimelineDiff>) {
        let entries = &mut self.entries;
        for diff in diffs {
            match diff {
                TimelineDiff::Reset(all) => {
                    self.reached_start = false;
                    *entries = all.into_iter().map(Entry::from).collect();
                }
                TimelineDiff::Append(more) => entries.extend(more.into_iter().map(Entry::from)),
                TimelineDiff::Clear => entries.clear(),
                TimelineDiff::PushFront(entry) => entries.insert(0, entry.into()),
                TimelineDiff::PushBack(entry) => entries.push(entry.into()),
                TimelineDiff::PopFront => {
                    if !entries.is_empty() {
                        entries.remove(0);
                    }
                }
                TimelineDiff::PopBack => {
                    entries.pop();
                }
                TimelineDiff::Insert { index, entry } if index <= entries.len() => entries.insert(index, entry.into()),
                TimelineDiff::Set { index, entry } if index < entries.len() => entries[index] = entry.into(),
                TimelineDiff::Remove { index } if index < entries.len() => {
                    entries.remove(index);
                }
                TimelineDiff::Truncate { length } => entries.truncate(length),
                TimelineDiff::Insert { .. } | TimelineDiff::Set { .. } | TimelineDiff::Remove { .. } => {}
            }
        }
        if entries.iter().any(|entry| matches!(entry.kind, EntryView::Start)) {
            self.reached_start = true;
        }
    }

    /// A thread starts at its root: once the root is in, there is nothing older to load.
    pub fn note_thread_root(&mut self, root: &str) {
        if self.messages().any(|message| message.event_id == root) {
            self.reached_start = true;
        }
    }

    /// Records that loading older history failed.
    pub fn history_failed(&mut self, error: String) {
        self.loading = false;
        self.history_error = Some(error);
    }

    /// The entries on screen, as the view last reported them (`shown`): the reported range
    /// while the timeline is as it was then, else the entries with a reported ID, which a scan
    /// finds until the view reports again. Before any report, the newest few, which a timeline
    /// opens on.
    pub fn on_screen<'a>(&'a self, shown: Option<&'a Shown>) -> Box<dyn Iterator<Item = &'a Entry> + 'a> {
        match shown {
            Some(shown) if shown.revision == self.revision => {
                Box::new(self.entries.get(shown.range.clone()).unwrap_or_default().iter())
            }
            Some(shown) => Box::new(self.entries.iter().filter(move |entry| shown.ids.contains(&entry.id))),
            None => Box::new(self.entries[self.entries.len().saturating_sub(SHOWN_ON_OPEN)..].iter()),
        }
    }

    pub fn messages(&self) -> impl DoubleEndedIterator<Item = &Message> {
        self.entries.iter().filter_map(Entry::message)
    }

    /// A sender's latest name and picture in this timeline. Built once per revision: the view
    /// asks for every reader on screen in every frame it builds.
    pub fn sender(&self, user_id: &str) -> Option<matrix_core::Person> {
        let Ok(mut cache) = self.senders.lock() else {
            return self.messages().rev().find(|m| m.sender_id == user_id).map(sender_of);
        };
        let index = match &*cache {
            Some((revision, index)) if *revision == self.revision => std::sync::Arc::clone(index),
            _ => {
                // Oldest first, so each sender's newest name and picture win.
                let index: HashMap<_, _> = self.messages().map(|m| (m.sender_id.clone(), sender_of(m))).collect();
                let index = std::sync::Arc::new(index);
                *cache = Some((self.revision, std::sync::Arc::clone(&index)));
                index
            }
        };
        index.get(user_id).cloned()
    }

    /// The GIF links its messages show, found once when each message arrived.
    pub fn gif_links(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(Entry::gif_link)
    }
}

/// A GIF link in a message, and the message as it reads without it: the GIF stands in for the
/// link, which no longer needs showing.
#[derive(Debug)]
pub struct GifLink {
    pub url: String,
    /// The rest of the message; `None` when the message is only the link.
    pub rest: Option<Body>,
}

/// A text body: plain, and parsed formatting when the sender sent some.
#[derive(Debug)]
pub struct Body {
    pub plain: String,
    pub rich: Option<RichText>,
}

/// Formatted text as views show it: Rune's rich text, and its HTML's code blocks (each one's
/// language and lines), which Rune's code blocks don't carry, so code shows in its language's
/// colours.
#[derive(Clone, Debug, Default)]
pub struct RichText {
    pub edit: TextEdit,
    pub code: Vec<matrix_core::html::CodeBlock>,
}

impl RichText {
    /// Matrix HTML as it shows: its mentions, spoilers, maths and custom emoji as pills.
    pub fn from_html(html: &str) -> Self {
        let edit = TextEdit::from_html(&matrix_core::html::to_pills(html, matrix_core::html::Pills::Display));
        Self { edit, code: matrix_core::html::code_blocks(html) }
    }
}

/// A code block placed in a field: its first and last line, and its language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedCode<'a> {
    pub first: usize,
    pub last: usize,
    pub language: Option<&'a str>,
}

/// Where `blocks` (a message's code blocks, in order) are among `edit`'s code-block lines,
/// which Rune joins into one block when blocks follow each other: each takes as many lines as
/// it has, in turn. Rune may part two blocks with a blank line of its own, which belongs to
/// neither. Empty when the lines and the blocks don't line up, as colours or fences on the
/// wrong lines would mislead.
pub fn place_code<'a>(edit: &TextEdit, blocks: &'a [matrix_core::html::CodeBlock]) -> Vec<PlacedCode<'a>> {
    let lines: Vec<(usize, bool)> = edit
        .paragraphs()
        .into_iter()
        .filter(|line| line.block == rune::Block::Code)
        .map(|line| (line.index, edit.text[line.range].trim().is_empty()))
        .collect();
    let needed: usize = blocks.iter().map(|block| block.lines.max(1)).sum();
    let Some(mut spare) = lines.len().checked_sub(needed) else { return Vec::new() };
    let mut at = 0;
    let mut placed = Vec::new();
    for block in blocks {
        while spare > 0 && lines.get(at).is_some_and(|&(_, blank)| blank) {
            at += 1;
            spare -= 1;
        }
        let Some(run) = lines.get(at..at + block.lines.max(1)) else { return Vec::new() };
        // A block's lines follow each other.
        if run.windows(2).any(|pair| pair[1].0 != pair[0].0 + 1) {
            return Vec::new();
        }
        let (Some(&(first, _)), Some(&(last, _))) = (run.first(), run.last()) else { return Vec::new() };
        placed.push(PlacedCode { first, last, language: block.language.as_deref() });
        at += run.len();
    }
    if at + spare != lines.len() || (spare > 0 && lines[at..].iter().any(|&(_, blank)| !blank)) {
        return Vec::new();
    }
    placed
}

impl TimelineItem {
    /// The link that gets a preview card: the first link that isn't a GIF, else, while linked
    /// GIFs don't play, the GIF link (which then shows as an ordinary link).
    pub fn card_link(&self, gifs_shown: bool) -> Option<&str> {
        self.link.as_deref().or_else(|| self.gif.as_ref().filter(|_| !gifs_shown).map(|gif| gif.url.as_str()))
    }

    fn new(message: Message) -> Self {
        // A hidden spoiler's fallback may name its link: nothing is fetched for it.
        let link = match &message.content {
            MessageContent::Text { body, html } if !html.as_deref().is_some_and(matrix_core::html::holds_spoiler) => {
                matrix_core::links::find_preview_link(body).map(str::to_owned)
            }
            _ => None,
        };
        let (rich, gif) = match &message.content {
            MessageContent::Emote { body, html } => (rich_body(body, html.as_deref()), None),
            MessageContent::Text { body, html } => {
                let hides = html.as_deref().is_some_and(matrix_core::html::holds_spoiler);
                let gif = matrix_core::links::find_gif_link(body).filter(|_| !hides).map(|url| {
                    let plain = matrix_core::links::without_link(body, url);
                    let html = html.as_deref().map(|html| matrix_core::links::html_without_link(html, url));
                    let rest = (!plain.is_empty()).then(|| Body { rich: rich_body(&plain, html.as_deref()), plain });
                    GifLink { url: url.to_owned(), rest }
                });
                (rich_body(body, html.as_deref()), gif)
            }
            _ => (None, None),
        };
        let emotes = rich
            .iter()
            .chain(gif.as_ref().and_then(|gif| gif.rest.as_ref()).and_then(|rest| rest.rich.as_ref()))
            .flat_map(|rich| rich.edit.pills())
            .filter_map(|(_, id)| id.strip_prefix("emote:").and_then(matrix_core::MediaRef::from_mxc))
            .collect();
        Self { message, rich, gif, link, emotes }
    }
}

/// A text body as the timeline shows it: the sender's formatting (or, without any, the
/// Markdown in its plain text), with every bare web address made a link (but not inside code).
/// `None` for plain text without Markdown or links, which shows as plain selectable text.
pub fn rich_body(plain: &str, html: Option<&str>) -> Option<RichText> {
    use rune::{Mark, MarkRange};
    let markdown = html.is_none().then(|| matrix_core::markdown::to_html(plain)).flatten();
    let RichText { mut edit, code } = match html.or(markdown.as_deref()) {
        // Mentions, spoilers, maths and custom emoji become pills the view draws.
        Some(html) => RichText::from_html(html),
        None if matrix_core::links::link_ranges(plain).next().is_some() => {
            RichText { edit: TextEdit::from_parts(plain, Vec::new(), Vec::new()), code: Vec::new() }
        }
        None => return None,
    };
    let taken = |range: &std::ops::Range<usize>| {
        edit.marks().iter().any(|mark| {
            matches!(mark.mark, Mark::Link(_) | Mark::Code | Mark::Pill(_))
                && mark.start < range.end
                && range.start < mark.end
        })
    };
    let links: Vec<MarkRange> = matrix_core::links::link_ranges(&edit.text)
        .filter(|range| !taken(range))
        .map(|range| MarkRange::new(range.clone(), Mark::Link(edit.text[range].to_owned())))
        .collect();
    if !links.is_empty() {
        let marks = edit.marks().iter().cloned().chain(links).collect();
        edit.set_marks(marks);
    }
    Some(RichText { edit, code })
}

/// How many timelines off screen keep their entries, per account.
const KEPT_TIMELINES: usize = 8;

/// One account's synced data.
#[derive(Debug)]
pub struct AccountData {
    pub summary: AccountSummary,
    pub invitations: Vec<matrix_core::Invitation>,
    pub rooms: Vec<RoomSummary>,
    pub spaces: Vec<SpaceSummary>,
    /// Lookups worked out from `rooms` and `spaces`, on first use after the list changes
    /// ([`Accounts::set_room_list`] starts it again).
    index: std::sync::OnceLock<RoomIndex>,
    timelines: HashMap<TimelineKey, RoomTimeline>,
    /// Timelines no longer on screen whose entries are still kept, oldest first: switching back
    /// shows them at once. Older ones are dropped and load again when opened.
    closed: Vec<TimelineKey>,
    /// Each room's newest message, from sync, for previews.
    latest: HashMap<String, Message>,
    /// When this device heard of each room's newest message, in milliseconds since the epoch:
    /// the room's place in the list when the message's own time can't be believed.
    heard_at: HashMap<String, u64>,
    /// How encryption stands on this device; `None` until the engine reports.
    pub security: Option<matrix_core::Security>,
    /// The signed-in sessions, once listed.
    pub sessions: Option<Vec<matrix_core::Session>>,
    /// What the user knows of other people's identities, by user ID, once asked: `None` for
    /// someone with no encryption identity (nothing to verify).
    pub identities: HashMap<String, Option<matrix_core::Identity>>,
    /// The linked email addresses and phone numbers, and whether the app may change them, once
    /// listed.
    pub contacts: Option<matrix_core::Contacts>,
    /// Rooms' members, by room ID, once listed.
    pub members: HashMap<String, matrix_core::Members>,
    /// Where each member is in `members`, by room and user ID, for read indicators.
    member_index: HashMap<String, HashMap<String, usize>>,
    /// Moves on whenever any room's member list is replaced or forgotten: what the member list's
    /// rows were worked out for.
    pub members_revision: u64,
    /// Snapshots kept visible while a newer membership list is fetched.
    stale_members: HashSet<String>,
    /// Rooms' threads, by room ID, once listed for the info panel.
    pub threads: HashMap<String, Vec<matrix_core::ThreadInfo>>,
    /// The files shared in rooms, by room ID, once listed for the info panel.
    pub files: HashMap<String, Vec<matrix_core::SharedFile>>,
    /// Rooms' widgets, by room ID, once listed for the info panel.
    pub widgets: HashMap<String, matrix_core::Widgets>,
    /// Rooms' and spaces' settings, by room ID, as last read for the room settings.
    pub room_settings: HashMap<String, matrix_core::RoomSettings>,
    /// Shared pinned-message previews by room, including unavailable placeholders.
    pub pins: HashMap<String, Vec<matrix_core::MessagePreview>>,
    /// Pin-list failures shown inline, alongside cached previews and a retry.
    pub pin_errors: HashMap<String, String>,
    /// The messages the user saved, most recently saved first, once listed.
    pub saved: Option<Vec<matrix_core::SavedMessage>>,
    /// Why the saved messages couldn't be listed, shown with the list and a retry.
    pub saved_error: Option<String>,
    /// Who is typing, by room ID, in the rooms whose timelines are open.
    pub typing: HashMap<String, Vec<matrix_core::Person>>,
    /// How the user arranged the sidebar.
    pub sidebar: matrix_core::SidebarLayout,
    pub search_usage: matrix_core::SearchUsage,
    /// Whether `sidebar` is the saved layout yet, rather than the empty one before it is read.
    pub sidebar_loaded: bool,
    /// What others see of the user, as they set it, including a choice still being saved;
    /// online until the engine reports.
    pub status: matrix_core::Status,
    /// The status as last saved: what a choice that fails to save falls back to.
    pub saved_status: matrix_core::Status,
    /// What people show others (presence and status message), the user included, by user ID:
    /// as the engine reports it, and from member lists for whoever it hasn't reported yet.
    pub presence: HashMap<String, matrix_core::PersonPresence>,
    /// The user's status message as the engine last reported it.
    pub saved_status_message: Option<String>,
    /// The words that notify the user wherever they are written, once the engine reports them.
    pub keywords: Option<Vec<String>>,
    /// The latest status message chosen while it is saved: until that request finishes, it shows
    /// over what the engine reports.
    pub saving_status_message: Option<matrix_core::RequestId>,
    /// What the user keeps away on this account, including a change still being saved; nothing
    /// until the engine reports.
    pub safety: matrix_core::Safety,
    /// The choices as last reported: what a change that fails to save falls back to.
    pub saved_safety: matrix_core::Safety,
    /// Changes still being saved, by request, oldest first: `safety` is `saved_safety` with
    /// them applied, whichever finishes first.
    pending_safety: Vec<(matrix_core::RequestId, matrix_core::SafetyChange)>,
}

impl AccountData {
    /// A complete member list is authoritative; a partial list is only a lower bound on
    /// the count from sync (which some homeservers omit or leave stale).
    pub fn member_count(&self, room: &RoomSummary) -> u64 {
        match self.members.get(&room.room_id) {
            Some(members) if members.complete && !self.stale_members.contains(&room.room_id) => {
                members.people.len() as u64
            }
            Some(_) if self.stale_members.contains(&room.room_id) && room.member_count > 0 => room.member_count,
            Some(members) => room.member_count.max(members.people.len() as u64),
            None => room.member_count,
        }
    }

    fn new(summary: AccountSummary) -> Self {
        Self {
            summary,
            invitations: Vec::new(),
            rooms: Vec::new(),
            spaces: Vec::new(),
            index: std::sync::OnceLock::new(),
            timelines: HashMap::new(),
            closed: Vec::new(),
            latest: HashMap::new(),
            heard_at: HashMap::new(),
            security: None,
            sessions: None,
            identities: HashMap::new(),
            contacts: None,
            members: HashMap::new(),
            member_index: HashMap::new(),
            members_revision: 0,
            stale_members: HashSet::new(),
            threads: HashMap::new(),
            files: HashMap::new(),
            widgets: HashMap::new(),
            room_settings: HashMap::new(),
            pins: HashMap::new(),
            pin_errors: HashMap::new(),
            saved: None,
            saved_error: None,
            typing: HashMap::new(),
            sidebar: matrix_core::SidebarLayout::default(),
            search_usage: matrix_core::SearchUsage::default(),
            sidebar_loaded: false,
            status: matrix_core::Status::default(),
            saved_status: matrix_core::Status::default(),
            presence: HashMap::new(),
            saved_status_message: None,
            saving_status_message: None,
            safety: matrix_core::Safety::default(),
            saved_safety: matrix_core::Safety::default(),
            pending_safety: Vec::new(),
            keywords: None,
        }
    }

    /// The status message `user_id` set, if any.
    pub fn status_message_of(&self, user_id: &str) -> Option<&str> {
        self.presence.get(user_id).and_then(|shown| shown.status_message.as_deref())
    }

    /// The status message the user set on this account, if any.
    pub fn status_message(&self) -> Option<&str> {
        self.status_message_of(self.summary.id.as_str())
    }

    fn index(&self) -> &RoomIndex {
        self.index.get_or_init(|| RoomIndex::new(&self.rooms, &self.spaces))
    }

    pub fn room(&self, room_id: &str) -> Option<&RoomSummary> {
        let at = *self.index().rooms.get(room_id)?;
        self.rooms.get(at).filter(|room| room.room_id == room_id)
    }

    /// A room, looked up without building the lookups when they aren't yet: for a list about
    /// to be replaced.
    pub fn room_unindexed(&self, room_id: &str) -> Option<&RoomSummary> {
        match self.index.get() {
            Some(_) => self.room(room_id),
            None => self.rooms.iter().find(|room| room.room_id == room_id),
        }
    }

    /// The rooms of `newer` (the list replacing this one) whose member count changed.
    pub fn member_counts_changed(&self, newer: &[RoomSummary]) -> Vec<String> {
        // The lookups if the view built them already, else just the counts.
        let counts: Option<HashMap<&str, u64>> = self.index.get().is_none().then(|| {
            let mut counts = HashMap::with_capacity(self.rooms.len());
            for room in &self.rooms {
                counts.entry(room.room_id.as_str()).or_insert(room.member_count);
            }
            counts
        });
        let old = |room_id: &str| match &counts {
            Some(counts) => counts.get(room_id).copied(),
            None => self.room(room_id).map(|room| room.member_count),
        };
        newer
            .iter()
            .filter(|room| old(&room.room_id).is_some_and(|count| count != room.member_count))
            .map(|room| room.room_id.clone())
            .collect()
    }

    /// The name of the room at `at` in `rooms` in lower case, worked out once per room list:
    /// the sidebar's order among rooms with no activity known.
    pub fn sort_name(&self, at: usize) -> &str {
        self.index().sort_names.get(at).map_or("", String::as_str)
    }

    /// Whether the user saved the message `event_id` in the room.
    pub fn is_saved(&self, room_id: &str, event_id: &str) -> bool {
        self.room(room_id).is_some_and(|room| room.saved.iter().any(|id| id == event_id))
    }

    /// The saved messages listed, as rooms' summaries know them now: one no longer saved
    /// leaves at once, and those of people the user ignores don't show (they stay saved).
    pub fn saved_messages(&self) -> impl Iterator<Item = &matrix_core::SavedMessage> {
        self.saved.iter().flatten().filter(|saved| {
            self.is_saved(&saved.room_id, &saved.preview.event_id) && !self.ignores(&saved.preview.sender_id)
        })
    }

    /// A room's pinned messages as listed, those still pinned, but not those of people the user
    /// ignores (they stay pinned).
    pub fn pinned_messages(&self, room: &RoomSummary) -> Option<impl Iterator<Item = &matrix_core::MessagePreview>> {
        let pins = self.pins.get(&room.room_id)?;
        Some(pins.iter().filter(|pin| room.pinned.contains(&pin.event_id) && !self.ignores(&pin.sender_id)))
    }

    /// Whether a message saved in some room isn't in the list yet: saved here or on another
    /// session since it was listed.
    pub fn saved_unlisted(&self) -> bool {
        let Some(listed) = &self.saved else { return false };
        self.rooms.iter().any(|room| {
            room.saved.iter().any(|id| !listed.iter().any(|s| s.room_id == room.room_id && s.preview.event_id == *id))
        })
    }

    pub fn space(&self, room_id: &str) -> Option<&SpaceSummary> {
        let at = *self.index().spaces.get(room_id)?;
        self.spaces.get(at).filter(|space| space.room_id == room_id)
    }

    /// The rooms a scope shows, of one kind, in list order. Home shows every direct chat and the
    /// rooms in no space; a space shows its rooms (subspaces included) and the direct chats with
    /// its members.
    pub fn rooms_in<'a>(&'a self, scope: &Scope, kind: RoomKind) -> impl Iterator<Item = &'a RoomSummary> + 'a {
        self.rooms_at(scope, kind).map(|(_, room)| room)
    }

    /// [`AccountData::rooms_in`], with each room's place in `rooms`.
    pub fn rooms_at<'a>(
        &'a self,
        scope: &Scope,
        kind: RoomKind,
    ) -> impl Iterator<Item = (usize, &'a RoomSummary)> + 'a {
        let index = self.index();
        let listed = match scope {
            Scope::Home => index.home.as_slice(),
            Scope::Space(id) => index.in_space.get(id).map_or(&[][..], Vec::as_slice),
        };
        listed.iter().filter_map(|at| Some((*at, self.rooms.get(*at)?))).filter(move |(_, room)| room.kind == kind)
    }

    /// Drops the cached newest messages of people the user is now confirmed to ignore, so no
    /// preview shows them: the room's next message from sync takes their place. (While an
    /// ignore is still saving, previews skip them as they read instead, so a failure brings
    /// them back.)
    fn forget_ignored_latest(&mut self) {
        let safety = &self.saved_safety;
        self.latest.retain(|_, message| !safety.ignores(&message.sender_id));
    }

    /// The choices shown: the saved ones with every change still being saved applied.
    fn replay_safety(&mut self) {
        self.safety =
            self.pending_safety.iter().fold(self.saved_safety.clone(), |safety, (_, change)| safety.with(change));
    }

    /// Whether listed media (a file in the Files panel, or one opened from anywhere) waits for a
    /// press: as its message in the room's timeline when loaded there, else as the Files panel
    /// lists its sender; with neither, someone else's. Only visual media counts ([`visual`]).
    pub fn hides_listed_media(
        &self,
        room_id: &str,
        event_id: &str,
        attachment: &Attachment,
        revealed: &HashSet<(AccountId, String)>,
    ) -> bool {
        if !visual(attachment) {
            return false;
        }
        let loaded = self
            .timelines
            .iter()
            .filter(|(key, _)| key.room_id == room_id)
            .find_map(|(_, timeline)| timeline.messages().find(|m| m.event_id == event_id));
        match loaded {
            Some(message) => self.hides_media(room_id, message, revealed),
            None => {
                // The Files panel's list knows the sender; without it, someone else.
                let own = self
                    .files
                    .get(room_id)
                    .and_then(|files| files.iter().find(|file| file.event_id == event_id))
                    .is_some_and(|file| file.sender_id == self.summary.id.as_str());
                let public = self.room(room_id).is_none_or(|room| room.public);
                !self.safety.shows_media(public, own)
                    && !revealed.contains(&(self.summary.id.clone(), event_id.to_owned()))
            }
        }
    }

    /// Whether the message `event_id` in a room was sent by someone the user ignores ([`ignores`]),
    /// as a kept timeline or the Files panel's list knows its sender.
    ///
    /// [`ignores`]: AccountData::ignores
    pub fn ignores_sender_of(&self, room_id: &str, event_id: &str) -> bool {
        let loaded = self
            .timelines
            .iter()
            .filter(|(key, _)| key.room_id == room_id)
            .find_map(|(_, timeline)| timeline.messages().find(|m| m.event_id == event_id))
            .map(|message| message.sender_id.as_str());
        let listed = || {
            self.files
                .get(room_id)
                .and_then(|files| files.iter().find(|file| file.event_id == event_id))
                .map(|file| file.sender_id.as_str())
        };
        loaded.or_else(listed).is_some_and(|sender| self.ignores(sender))
    }

    /// Whether the user ignores someone, or is saving an ignore of them (even with a Stop ignoring
    /// queued after it): what lists of their things (the Files panel) leave out.
    pub fn ignores(&self, user_id: &str) -> bool {
        self.safety.ignores(user_id)
            || self.saved_safety.ignores(user_id)
            || self.pending_safety.iter().any(|(_, change)| {
                matches!(change, matrix_core::SafetyChange::Ignore { user_id: pending, ignored: true } if pending == user_id)
            })
    }

    /// A fingerprint of everyone [`AccountData::ignores`] counts: what views built from it (a
    /// timeline's rows) are made again for when it changes.
    pub fn ignores_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.safety.ignored.hash(&mut hasher);
        self.saved_safety.ignored.hash(&mut hasher);
        for (_, change) in &self.pending_safety {
            if let matrix_core::SafetyChange::Ignore { user_id, ignored: true } = change {
                user_id.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// The entries carrying the message `event_id`, in the timelines kept.
    pub fn timelines_holding<'a>(&'a self, event_id: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.timelines.values().flat_map(move |timeline| {
            timeline.entries.iter().filter(move |e| e.message().is_some_and(|m| m.event_id == event_id))
        })
    }

    /// Who sent the attachment `media`, as a kept timeline knows it.
    pub fn sender_of_media(&self, media: &matrix_core::MediaRef) -> Option<String> {
        self.timelines.values().flat_map(RoomTimeline::messages).find_map(|message| match &message.content {
            MessageContent::Attachment(file) if file.media.as_ref() == Some(media) => Some(message.sender_id.clone()),
            _ => None,
        })
    }

    /// The invitations listed anywhere: all but those from people the user ignores ([`ignores`]),
    /// which stay out of sight without being declined.
    ///
    /// [`ignores`]: AccountData::ignores
    pub fn listed_invitations(&self) -> impl Iterator<Item = &matrix_core::Invitation> {
        self.invitations
            .iter()
            .filter(|invite| invite.inviter.as_ref().is_none_or(|inviter| !self.ignores(&inviter.id)))
    }

    /// The invitations the sidebar and Home list: all but strangers' when the user hides them,
    /// and never those of people the user ignores.
    pub fn shown_invitations(&self) -> impl Iterator<Item = &matrix_core::Invitation> {
        self.listed_invitations().filter(|invite| self.safety.shows_invitation(invite))
    }

    /// Strangers' invitations the user hides, which only Settings → Safety lists (not those of
    /// people the user ignores).
    pub fn hidden_invitations(&self) -> impl Iterator<Item = &matrix_core::Invitation> {
        self.listed_invitations().filter(|invite| !self.safety.shows_invitation(invite))
    }

    /// Whether a message's pictures, video poster, linked GIF and link card image wait for a
    /// press: the user's media choice keeps them back in its room, and `revealed` doesn't hold it.
    /// A room with no summary here counts as public.
    pub fn hides_media(&self, room_id: &str, message: &Message, revealed: &HashSet<(AccountId, String)>) -> bool {
        let public = self.room(room_id).is_none_or(|room| room.public);
        let own = message.sender_id == self.summary.id.as_str();
        !self.safety.shows_media(public, own)
            && !revealed.contains(&(self.summary.id.clone(), message.event_id.clone()))
    }

    /// Whether any room in the scope has unread messages, for the rail's dots.
    pub fn has_unread(&self, scope: &Scope) -> bool {
        (*scope == Scope::Home && self.shown_invitations().next().is_some())
            || [RoomKind::Group, RoomKind::Direct]
                .into_iter()
                .any(|kind| self.rooms_in(scope, kind).any(wants_attention))
    }

    pub fn timeline(&self, key: &TimelineKey) -> Option<&RoomTimeline> {
        self.timelines.get(key)
    }
}

/// Who sent a message, as it names them.
fn sender_of(message: &Message) -> matrix_core::Person {
    matrix_core::Person {
        id: message.sender_id.clone(),
        name: message.sender_name.clone(),
        avatar: message.sender_avatar.clone(),
    }
}

impl AccountData {
    /// A room's member by user ID, once its members are listed.
    pub fn member(&self, room_id: &str, user_id: &str) -> Option<&matrix_core::RoomMember> {
        let people = &self.members.get(room_id)?.people;
        match self.member_index.get(room_id).filter(|index| index.len() == people.len()) {
            Some(index) => index.get(user_id).and_then(|at| people.get(*at)).filter(|m| m.person.id == user_id),
            None => people.iter().find(|m| m.person.id == user_id),
        }
    }

    /// Lets go of a room's member list.
    fn forget_members(&mut self, room_id: &str) {
        if self.members.remove(room_id).is_some() {
            self.members_revision = next_revision();
        }
        self.member_index.remove(room_id);
        self.stale_members.remove(room_id);
    }

    /// The newest message in a room: from its timeline when open, else from sync. Never one
    /// from someone the user ignores, whose messages a kept timeline may still hold until the
    /// engine reloads it.
    pub fn last_message(&self, room_id: &str) -> Option<&Message> {
        // Asked for every room the sidebar sorts on every commit: the few timelines kept are
        // looked through rather than a key made for each room.
        let heard = |message: &&Message| !self.ignores(&message.sender_id);
        let from_timeline = self
            .timelines
            .iter()
            .find(|(key, _)| key.thread.is_none() && key.room_id == room_id)
            .and_then(|(_, timeline)| timeline.messages().rev().find(heard));
        match (from_timeline, self.latest.get(room_id).filter(heard)) {
            (Some(a), Some(b)) => Some(if a.timestamp_ms >= b.timestamp_ms { a } else { b }),
            (a, b) => a.or(b),
        }
    }

    /// The one-to-one chat with `user_id`, if there is one: a direct chat with them alone, never
    /// a group chat they're in, nor one a third person joined (a bridge bot, someone invited
    /// later, even one only invited), where a call would ring them too: the same rule as the
    /// engine's `Account::direct_chat`, which makes a fresh chat when there's none.
    pub fn direct_chat_with(&self, user_id: &str) -> Option<&RoomSummary> {
        self.rooms
            .iter()
            .find(|room| room.kind == RoomKind::Direct && room.dm_users == [user_id] && room.active_member_count <= 2)
    }

    /// When something last happened in a room, for the room list's order: its newest message
    /// seen this session, or the latest event the store kept from before (so a quiet chat
    /// keeps its place after a restart); 0 when nothing is known.
    ///
    /// A time further in the future than clocks drift (a misdated event, by accident or on
    /// purpose, or a server whose clock runs fast) doesn't count, so it can't hold a room at the
    /// top: a message heard this session counts from when it was heard instead; a stored time
    /// counts once it has passed (until then the room sorts as unknown).
    pub fn activity(&self, room: &RoomSummary, now_ms: u64) -> u64 {
        /// How far ahead of this device's clock a server's may be.
        const DRIFT_MS: u64 = 5 * 60_000;
        let plausible = |at: &u64| *at <= now_ms.saturating_add(DRIFT_MS);
        let seen = self
            .last_message(&room.room_id)
            .map(|message| message.timestamp_ms)
            .and_then(|at| if plausible(&at) { Some(at) } else { self.heard_at.get(&room.room_id).copied() });
        seen.max(room.activity_ms.filter(plausible)).unwrap_or(0)
    }

    /// What a scope's overview shows: rooms wanting attention (mentions first), and the rooms
    /// with the most recent messages, newest first.
    pub fn overview(&self, scope: &Scope) -> Overview<'_> {
        let in_scope = || [RoomKind::Group, RoomKind::Direct].into_iter().flat_map(|kind| self.rooms_in(scope, kind));
        let mut unread: Vec<_> = in_scope().filter(|room| wants_attention(room)).collect();
        unread.sort_by_key(|room| (std::cmp::Reverse(room.mentions), std::cmp::Reverse(room.unread)));
        let mut recent: Vec<_> =
            in_scope().filter_map(|room| self.last_message(&room.room_id).map(|message| (room, message))).collect();
        // By the same believable time as the room list, so a misdated message can't hold a slot.
        let now = now_ms();
        recent.sort_by_key(|(room, _)| std::cmp::Reverse(self.activity(room, now)));
        recent.truncate(RECENT_LIMIT);
        Overview { unread, recent, rooms: self.rooms_in(scope, RoomKind::Group).count() }
    }
}

/// How many recent conversations an overview lists.
const RECENT_LIMIT: usize = 6;

/// The content of a scope's overview.
#[derive(Debug)]
pub struct Overview<'a> {
    pub unread: Vec<&'a RoomSummary>,
    pub recent: Vec<(&'a RoomSummary, &'a Message)>,
    /// Group rooms the scope lists (direct chats not counted).
    pub rooms: usize,
}

/// How many of a timeline's newest entries count as on screen before the view reports which
/// are: about a window's worth, which it opens on.
const SHOWN_ON_OPEN: usize = 30;

/// Which rooms a [`Scope`] shows.
enum Filter<'a> {
    NotIn(HashSet<&'a str>),
    Space { rooms: HashSet<&'a str>, members: HashSet<&'a str> },
}

impl Filter<'_> {
    fn shows(&self, room: &RoomSummary) -> bool {
        match (self, room.kind) {
            (Filter::NotIn(_), RoomKind::Direct) => true,
            (Filter::NotIn(in_spaces), RoomKind::Group) => !in_spaces.contains(room.room_id.as_str()),
            (Filter::Space { rooms, .. }, RoomKind::Group) => rooms.contains(room.room_id.as_str()),
            (Filter::Space { rooms, members }, RoomKind::Direct) => {
                rooms.contains(room.room_id.as_str()) || room.dm_users.iter().any(|u| members.contains(u.as_str()))
            }
        }
    }
}

/// What [`AccountData`] looks up in its room list again and again, worked out once per list:
/// where each room and space is, and which rooms each scope lists.
#[derive(Debug, Default)]
struct RoomIndex {
    rooms: HashMap<String, usize>,
    spaces: HashMap<String, usize>,
    /// The rooms Home lists, of both kinds, as places in the room list, in its order.
    home: Vec<usize>,
    /// The rooms each space lists (its subspaces' included), likewise.
    in_space: HashMap<String, Vec<usize>>,
    /// Each room's name in lower case, by place in the room list.
    sort_names: Vec<String>,
}

impl RoomIndex {
    fn new(rooms: &[RoomSummary], spaces: &[SpaceSummary]) -> Self {
        // The first of a repeated ID is the one found, as a search through the list finds it.
        let mut room_at = HashMap::with_capacity(rooms.len());
        for (at, room) in rooms.iter().enumerate() {
            room_at.entry(room.room_id.clone()).or_insert(at);
        }
        let mut space_at = HashMap::with_capacity(spaces.len());
        for (at, space) in spaces.iter().enumerate() {
            space_at.entry(space.room_id.clone()).or_insert(at);
        }
        let space = |id: &str| space_at.get(id).and_then(|at| spaces.get(*at));
        // Every room ID under a space, following subspaces; safe against cycles.
        let space_rooms = |space_id: &str| {
            let mut found = HashSet::new();
            let mut seen = HashSet::new();
            let mut pending = vec![space_id];
            while let Some(id) = pending.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let Some(space) = space(id) else { continue };
                for child in &space.children {
                    if space_at.contains_key(child.as_str()) {
                        pending.push(child);
                    } else {
                        found.insert(child.as_str());
                    }
                }
            }
            found
        };
        let listed = |filter: &Filter<'_>| {
            rooms.iter().enumerate().filter(|(_, room)| filter.shows(room)).map(|(at, _)| at).collect::<Vec<_>>()
        };
        // A space lists the rooms under it and the direct chats with its members: found from its
        // children and among the direct chats, rather than by testing every room. A list
        // repeating a room's ID (which the engine never sends) is tested room by room instead.
        let direct: Vec<usize> =
            rooms.iter().enumerate().filter(|(_, room)| room.kind == RoomKind::Direct).map(|(at, _)| at).collect();
        let unique = room_at.len() == rooms.len();
        let space_listed = |under: Vec<&str>, filter: &Filter<'_>| {
            if !unique {
                return listed(filter);
            }
            let mut found: Vec<usize> = under.iter().filter_map(|id| room_at.get(*id).copied()).collect();
            found.extend(direct.iter().copied().filter(|at| rooms.get(*at).is_some_and(|room| filter.shows(room))));
            found.sort_unstable();
            found.dedup();
            found
        };
        let mut in_spaces = HashSet::new();
        let mut in_space = HashMap::with_capacity(spaces.len());
        for (id, at) in &space_at {
            let under = space_rooms(id);
            in_spaces.extend(under.iter().copied());
            let children: Vec<&str> = under.iter().copied().collect();
            let members = spaces.get(*at).map(|s| s.members.iter().map(String::as_str).collect()).unwrap_or_default();
            in_space.insert(id.clone(), space_listed(children, &Filter::Space { rooms: under, members }));
        }
        let home = listed(&Filter::NotIn(in_spaces));
        let sort_names = rooms.iter().map(|room| room.name.to_lowercase()).collect();
        Self { rooms: room_at, spaces: space_at, home, in_space, sort_names }
    }
}

/// Every signed-in account, in the order they were added.
#[derive(Debug, Default)]
pub struct Accounts {
    list: Vec<AccountData>,
    /// Moves on whenever an account, or its rooms, spaces or invitations, change.
    revision: u64,
}

impl Accounts {
    /// Marks every timeline changed, so views plan their rows again: after a setting that
    /// changes which entries show.
    pub fn replan_timelines(&mut self) {
        for account in &mut self.list {
            for timeline in account.timelines.values_mut() {
                timeline.revision = next_revision();
            }
        }
    }

    /// Moves each timeline's [`Landing`] on a frame, and starts or ends read markers' fades: a
    /// fade waits while a change lands whole, which would land it too.
    pub fn poll_timelines(&mut self, tx: &mut Tx, reduced_motion: bool) -> bool {
        let hold = self.list.iter().any(|account| account.timelines.values().any(|t| t.landing == Landing::Due));
        let mut changed = false;
        for account in &mut self.list {
            for timeline in account.timelines.values_mut() {
                timeline.landing = match timeline.landing {
                    Landing::Due => Landing::Now,
                    Landing::Now | Landing::Idle => Landing::Idle,
                };
                changed |= timeline.poll_read_marker(tx, reduced_motion, hold);
            }
        }
        changed
    }

    pub fn forget_invitation(&mut self, account: &AccountId, room_id: &str) {
        if let Some(data) = self.get_mut(account) {
            data.invitations.retain(|invite| invite.room_id != room_id);
            self.revision += 1;
        }
    }

    /// Moves on whenever an account, or its rooms, spaces or invitations, change: what is
    /// worked out from those lists is worked out again only then.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn iter(&self) -> impl Iterator<Item = &AccountData> {
        self.list.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, id: &AccountId) -> Option<&AccountData> {
        self.list.iter().find(|account| &account.summary.id == id)
    }

    fn get_mut(&mut self, id: &AccountId) -> Option<&mut AccountData> {
        self.list.iter_mut().find(|account| &account.summary.id == id)
    }

    /// Adds an account, or refreshes its summary when it is already known (a re-login).
    pub fn upsert(&mut self, summary: AccountSummary) {
        match self.get_mut(&summary.id) {
            Some(account) => account.summary = summary,
            None => self.list.push(AccountData::new(summary)),
        }
        self.revision += 1;
    }

    pub fn remove(&mut self, id: &AccountId) {
        self.list.retain(|account| &account.summary.id != id);
        self.revision += 1;
    }

    /// The safety choices the engine reported: kept as the confirmed ones, and shown with the
    /// changes still being saved applied.
    pub fn safety_saved(&mut self, id: &AccountId, safety: matrix_core::Safety) {
        if let Some(account) = self.get_mut(id) {
            account.saved_safety = safety;
            account.replay_safety();
            account.forget_ignored_latest();
            // The lists (invitations, their pictures) may change with them.
            self.revision += 1;
        }
    }

    /// Shows a safety change at once, while `request` saves it.
    pub fn safety_pending(
        &mut self,
        id: &AccountId,
        request: matrix_core::RequestId,
        change: matrix_core::SafetyChange,
    ) {
        if let Some(account) = self.get_mut(id) {
            account.pending_safety.push((request, change));
            account.replay_safety();
            self.revision += 1;
        }
    }

    /// `request` finished saving a change, or failed to: the choices shown are the saved ones
    /// with the changes still being saved applied.
    pub fn safety_settled(&mut self, id: &AccountId, request: matrix_core::RequestId) {
        if let Some(account) = self.get_mut(id) {
            account.pending_safety.retain(|(pending, _)| *pending != request);
            account.replay_safety();
            self.revision += 1;
        }
    }

    pub fn set_search_usage(&mut self, id: &AccountId, usage: matrix_core::SearchUsage) {
        if let Some(account) = self.get_mut(id) {
            account.search_usage = usage;
        }
    }

    pub fn set_room_list(&mut self, id: &AccountId, list: RoomList) {
        if let Some(account) = self.get_mut(id) {
            account.rooms = list.rooms;
            account.spaces = list.spaces;
            account.invitations = list.invitations;
            account.index = std::sync::OnceLock::new();
            self.revision += 1;
        }
    }

    /// Keeps a room's or space's settings, as the engine read them.
    pub fn set_room_settings(&mut self, id: &AccountId, settings: matrix_core::RoomSettings) {
        if let Some(account) = self.get_mut(id) {
            account.room_settings.insert(settings.room_id.clone(), settings);
        }
    }

    /// Shows a space's rooms in a new order at once, before sync brings it.
    pub fn order_space(&mut self, id: &AccountId, space: &str, children: Vec<String>) {
        if let Some(account) = self.get_mut(id)
            && let Some(shown) = account.spaces.iter_mut().find(|known| known.room_id == space)
        {
            shown.children = children;
            account.index = std::sync::OnceLock::new();
            self.revision += 1;
        }
    }

    pub fn set_security(&mut self, id: &AccountId, security: matrix_core::Security) {
        if let Some(account) = self.get_mut(id) {
            account.security = Some(security);
        }
    }

    pub fn set_identity(&mut self, id: &AccountId, user_id: String, identity: Option<matrix_core::Identity>) {
        if let Some(account) = self.get_mut(id) {
            account.identities.insert(user_id, identity);
        }
    }

    pub fn set_sessions(&mut self, id: &AccountId, sessions: Vec<matrix_core::Session>) {
        if let Some(account) = self.get_mut(id) {
            account.sessions = Some(sessions);
        }
    }

    /// Shows a status the user chose, before it is saved.
    pub fn set_status(&mut self, id: &AccountId, status: matrix_core::Status) {
        if let Some(account) = self.get_mut(id) {
            account.status = status;
        }
    }

    /// A status saved, here or on another session: shown, and what a failed choice falls back to.
    pub fn status_saved(&mut self, id: &AccountId, status: matrix_core::Status) {
        if let Some(account) = self.get_mut(id) {
            account.status = status;
            account.saved_status = status;
        }
    }

    /// What people show now, as the engine reported it.
    pub fn set_presence(&mut self, id: &AccountId, people: Vec<matrix_core::PersonPresence>) {
        if let Some(account) = self.get_mut(id) {
            for mut shown in people {
                if shown.user_id == account.summary.id.as_str() {
                    account.saved_status_message.clone_from(&shown.status_message);
                    if account.saving_status_message.is_some()
                        && let Some(chosen) = account.presence.get_mut(&shown.user_id)
                    {
                        shown.status_message = chosen.status_message.take();
                    }
                }
                account.presence.insert(shown.user_id.clone(), shown);
            }
        }
    }

    /// Shows the user's own status message as `message` at once, while `request` saves it.
    pub fn status_message_chosen(&mut self, id: &AccountId, message: Option<String>, request: matrix_core::RequestId) {
        let Some(account) = self.get_mut(id) else { return };
        account.saving_status_message = Some(request);
        let presence = match account.status {
            matrix_core::Status::Online => matrix_core::Presence::Online,
            matrix_core::Status::Away => matrix_core::Presence::Away,
            matrix_core::Status::Hidden => matrix_core::Presence::Offline,
        };
        let me = account.summary.id.as_str().to_owned();
        let shown = account.presence.entry(me.clone()).or_insert(matrix_core::PersonPresence {
            user_id: me,
            presence,
            status_message: None,
        });
        shown.status_message = message;
    }

    /// Saving a status message finished: once the latest one has, the message shows as the
    /// engine last reported it (the one saved, or the one before when saving failed). Whether it
    /// was the latest.
    pub fn status_message_saved(&mut self, id: &AccountId, request: matrix_core::RequestId) -> bool {
        let Some(account) = self.get_mut(id) else { return false };
        if account.saving_status_message != Some(request) {
            return false;
        }
        account.saving_status_message = None;
        let saved = account.saved_status_message.clone();
        if let Some(me) = account.presence.get_mut(account.summary.id.as_str()) {
            me.status_message = saved;
        }
        true
    }

    pub fn set_sidebar(&mut self, id: &AccountId, layout: matrix_core::SidebarLayout) {
        if let Some(account) = self.get_mut(id) {
            account.sidebar = layout;
            account.sidebar_loaded = true;
        }
    }

    /// Who is typing in a room; nobody forgets the room.
    pub fn set_typing(&mut self, id: &AccountId, room_id: String, people: Vec<matrix_core::Person>) {
        let Some(account) = self.get_mut(id) else { return };
        if people.is_empty() {
            account.typing.remove(&room_id);
        } else {
            account.typing.insert(room_id, people);
        }
    }

    /// Keeps a room's members; a partial list never replaces a complete one.
    pub fn set_members(&mut self, id: &AccountId, room_id: String, members: matrix_core::Members) {
        let Some(account) = self.get_mut(id) else { return };
        // The engine's reports are newer than a member list read from the store: a list only
        // fills in those it hasn't reported.
        // The user's own is the engine's to report: the list's may predate a save.
        for member in &members.people {
            if member.person.id == account.summary.id.as_str() {
                continue;
            }
            if member.presence != matrix_core::Presence::Unknown || member.status_message.is_some() {
                account.presence.entry(member.person.id.clone()).or_insert_with(|| matrix_core::PersonPresence {
                    user_id: member.person.id.clone(),
                    presence: member.presence,
                    status_message: member.status_message.clone(),
                });
            }
        }
        let stale = account.stale_members.remove(&room_id);
        let had_all = account.members.get(&room_id).is_some_and(|known| known.complete);
        if members.complete || !had_all || stale {
            let index = members.people.iter().enumerate().map(|(at, m)| (m.person.id.clone(), at)).collect();
            account.member_index.insert(room_id.clone(), index);
            account.members.insert(room_id, members);
            account.members_revision = next_revision();
        }
    }

    pub fn invalidate_members(&mut self, id: &AccountId, room_id: &str) -> Option<bool> {
        let account = self.get_mut(id)?;
        let complete = account.members.get(room_id)?.complete;
        account.stale_members.insert(room_id.to_owned());
        Some(complete)
    }

    pub fn members_fresh(&self, id: &AccountId, room_id: &str) -> bool {
        self.get(id).is_some_and(|account| !account.stale_members.contains(room_id))
    }

    /// Viewport metadata doesn't change the row plan or its virtual-list version.
    pub fn set_read_position(&mut self, id: &AccountId, key: &TimelineKey, event_id: String) -> bool {
        let Some(timeline) = self.get_mut(id).and_then(|account| account.timelines.get_mut(key)) else { return false };
        if timeline.read_up_to.as_deref() == Some(event_id.as_str()) {
            return false;
        }
        // Where the position read so far and the new one are, found in one pass.
        if let Some(read) = timeline.read_up_to.as_deref() {
            let (mut previous, mut next) = (None, None);
            let messages = timeline.entries.iter().enumerate().filter_map(|(at, entry)| Some((at, entry.message()?)));
            for (at, message) in messages {
                if previous.is_none() && message.event_id == read {
                    previous = Some(at);
                }
                if next.is_none() && message.event_id == event_id {
                    next = Some(at);
                }
                if previous.is_some() && next.is_some() {
                    break;
                }
            }
            if let (Some(previous), Some(next)) = (previous, next)
                && next < previous
            {
                return false;
            }
        }
        timeline.read_up_to = Some(event_id);
        true
    }

    /// This is viewport bookkeeping; only dismissal changes the row plan.
    pub fn set_visible_read_marker(&mut self, id: &AccountId, key: &TimelineKey, marker: Option<String>) {
        if let Some(timeline) = self.get_mut(id).and_then(|account| account.timelines.get_mut(key)) {
            timeline.visible_read_marker = marker;
        }
    }

    pub fn dismiss_read_marker(&mut self, id: &AccountId, key: &TimelineKey, marker: &str) -> bool {
        let Some(timeline) = self.get_mut(id).and_then(|account| account.timelines.get_mut(key)) else { return false };
        if timeline.read_marker_seen(marker) || !timeline.see_read_marker(marker) {
            return false;
        }
        let Some(at) = timeline.entries.iter().position(|entry| entry.id == marker) else { return false };
        let marker = ReadMarker {
            id: marker.to_owned(),
            previous: at.checked_sub(1).and_then(|at| timeline.entries.get(at)).map(|entry| entry.id.clone()),
            next: timeline.entries.get(at + 1).map(|entry| entry.id.clone()),
        };
        timeline.read_marker_fade = Some(ReadMarkerFade::Pending(marker));
        timeline.revision = next_revision();
        true
    }

    pub fn pins_failed(&mut self, id: &AccountId, room_id: String, error: String) {
        if let Some(account) = self.get_mut(id) {
            account.pin_errors.insert(room_id, error);
        }
    }

    pub fn set_pins(&mut self, id: &AccountId, room_id: String, messages: Vec<matrix_core::MessagePreview>) {
        if let Some(account) = self.get_mut(id) {
            account.pin_errors.remove(&room_id);
            account.pins.insert(room_id, messages);
        }
    }

    pub fn set_saved_messages(&mut self, id: &AccountId, messages: Vec<matrix_core::SavedMessage>) {
        if let Some(account) = self.get_mut(id) {
            account.saved_error = None;
            account.saved = Some(messages);
        }
    }

    /// The saved messages are being listed again: the last failure no longer stands.
    pub fn listing_saved(&mut self, id: &AccountId) {
        if let Some(account) = self.get_mut(id) {
            account.saved_error = None;
        }
    }

    pub fn saved_failed(&mut self, id: &AccountId, error: String) {
        if let Some(account) = self.get_mut(id) {
            account.saved_error = Some(error);
        }
    }

    pub fn set_threads(&mut self, id: &AccountId, room_id: String, threads: Vec<matrix_core::ThreadInfo>) {
        if let Some(account) = self.get_mut(id) {
            account.threads.insert(room_id, threads);
        }
    }

    pub fn set_files(&mut self, id: &AccountId, room_id: String, files: Vec<matrix_core::SharedFile>) {
        if let Some(account) = self.get_mut(id) {
            account.files.insert(room_id, files);
        }
    }

    pub fn set_widgets(&mut self, id: &AccountId, room_id: String, widgets: matrix_core::Widgets) {
        if let Some(account) = self.get_mut(id) {
            account.widgets.insert(room_id, widgets);
        }
    }

    pub fn set_contacts(&mut self, id: &AccountId, contacts: matrix_core::Contacts) {
        if let Some(account) = self.get_mut(id) {
            account.contacts = Some(contacts);
        }
    }

    /// Records a room's newest message, for previews: never one from someone the user ignores.
    pub fn set_latest(&mut self, id: &AccountId, room_id: &str, message: Message) {
        if let Some(account) = self.get_mut(id)
            && !account.saved_safety.ignores(&message.sender_id)
        {
            account.latest.insert(room_id.to_owned(), message);
            account.heard_at.insert(room_id.to_owned(), now_ms());
        }
    }

    /// Starts a thread's timeline with its root, copied from the room's, so the thread panel
    /// shows it at once; the engine's first update replaces it.
    pub fn seed_thread(&mut self, id: &AccountId, key: &TimelineKey) {
        let Some(root) = &key.thread else { return };
        let Some(account) = self.get_mut(id) else { return };
        if account.timelines.get(key).is_some_and(|thread| !thread.entries.is_empty()) {
            return;
        }
        let room = account.timelines.get(&TimelineKey::room(&key.room_id));
        let Some(message) = room.and_then(|room| room.messages().find(|m| m.event_id == *root)).cloned() else {
            return;
        };
        let entry = TimelineEntry { id: format!("seed-{root}"), kind: EntryKind::Message(Box::new(message)) };
        let thread = account.timelines.entry(key.clone()).or_default();
        thread.revision = next_revision();
        thread.entries.push(entry.into());
        thread.note_thread_root(root);
    }

    /// A timeline left the screen: its entries stay for a while, as many as the engine keeps
    /// warm timelines, then go.
    pub fn timeline_closed(&mut self, id: &AccountId, key: &TimelineKey) {
        let Some(account) = self.get_mut(id) else { return };
        if let Some(timeline) = account.timelines.get_mut(key) {
            timeline.read_marker_fade = None;
        }
        // Typing shows only while the room is open: what arrives later is dropped, so what was
        // shown would go stale.
        if key.thread.is_none() {
            account.typing.remove(&key.room_id);
        }
        account.closed.retain(|closed| closed != key);
        account.closed.push(key.clone());
        while account.closed.len() > KEPT_TIMELINES {
            let oldest = account.closed.remove(0);
            account.timelines.remove(&oldest);
            // A room's members go with the last of its timelines kept: opening it lists them again.
            if !account.timelines.keys().any(|key| key.room_id == oldest.room_id) {
                account.forget_members(&oldest.room_id);
            }
        }
    }

    /// A timeline is on screen again: it is kept while shown.
    pub fn timeline_opened(&mut self, id: &AccountId, key: &TimelineKey) {
        if let Some(account) = self.get_mut(id) {
            account.closed.retain(|closed| closed != key);
            if let Some(timeline) = account.timelines.get_mut(key) {
                timeline.read_marker_hidden = false;
                timeline.visible_read_marker = None;
                timeline.read_up_to = None;
                timeline.revision = next_revision();
            }
        }
    }

    /// A timeline to change, created empty when first touched: for one being opened. Its
    /// revision moves on.
    pub fn timeline_mut(&mut self, id: &AccountId, key: &TimelineKey) -> Option<&mut RoomTimeline> {
        let timeline = self.get_mut(id)?.timelines.entry(key.clone()).or_default();
        timeline.revision = next_revision();
        Some(timeline)
    }

    /// A timeline still kept, to change; none for one dropped since (an answer that comes late
    /// changes nothing). Its revision moves on.
    pub fn kept_timeline_mut(&mut self, id: &AccountId, key: &TimelineKey) -> Option<&mut RoomTimeline> {
        let timeline = self.get_mut(id)?.timelines.get_mut(key)?;
        timeline.revision = next_revision();
        Some(timeline)
    }

    /// Sets a room's notification mode locally, ahead of the server's confirmation; `None`
    /// returns it to the account default.
    pub fn set_notifications(&mut self, id: &AccountId, room_id: &str, mode: Option<NotificationMode>) {
        let default = self.get(id).map(|account| account.summary.default_notifications).unwrap_or_default();
        if let Some(room) = self.room_mut(id, room_id) {
            room.notifications = mode.unwrap_or(default);
            room.custom_notifications = mode.is_some();
        }
    }

    /// Sets an account's default notification mode locally, and the mode of every room that
    /// follows it, ahead of the server's confirmation.
    pub fn set_default_notifications(&mut self, id: &AccountId, mode: NotificationMode) {
        let Some(account) = self.get_mut(id) else { return };
        account.summary.default_notifications = mode;
        for room in account.rooms.iter_mut().filter(|room| !room.custom_notifications) {
            room.notifications = mode;
        }
    }

    fn room_mut(&mut self, id: &AccountId, room_id: &str) -> Option<&mut RoomSummary> {
        let account = self.get_mut(id)?;
        let at = *account.index().rooms.get(room_id)?;
        account.rooms.get_mut(at).filter(|room| room.room_id == room_id)
    }

    /// Clears a room's unread counts locally, ahead of the server's next count.
    pub fn mark_read(&mut self, id: &AccountId, room_id: &str) {
        if let Some(room) = self.room_mut(id, room_id) {
            room.unread = 0;
            room.mentions = 0;
            room.marked_unread = false;
        }
    }

    /// Marks a room unread or takes the mark away locally, ahead of the server's confirmation.
    pub fn set_marked(&mut self, id: &AccountId, room_id: &str, marked: bool) {
        if let Some(room) = self.room_mut(id, room_id) {
            room.marked_unread = marked;
        }
    }

    /// Puts a room among the favourites or the low priority rooms locally, ahead of the server's
    /// confirmation.
    pub fn set_priority(&mut self, id: &AccountId, room_id: &str, priority: matrix_core::RoomPriority) {
        if let Some(room) = self.room_mut(id, room_id) {
            room.priority = priority;
        }
    }

    /// Takes back the one change to a room the server refused, keeping whatever else changed
    /// about the room since.
    pub fn undo(&mut self, id: &AccountId, room_id: &str, undo: &RoomUndo) {
        let Some(room) = self.room_mut(id, room_id) else { return };
        match *undo {
            RoomUndo::Priority(priority) => room.priority = priority,
            RoomUndo::Mark(marked) => room.marked_unread = marked,
            RoomUndo::Read { marked, unread, mentions } => {
                room.marked_unread = marked;
                room.unread = room.unread.max(unread);
                room.mentions = room.mentions.max(mentions);
            }
        }
    }

    /// The account's notification keywords, as the engine reports them.
    pub fn set_keywords(&mut self, id: &AccountId, keywords: Vec<String>) {
        if let Some(account) = self.get_mut(id) {
            account.keywords = Some(keywords);
        }
    }
}

/// Now, in milliseconds since the epoch: the room list's clock.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(edit: &TextEdit) -> Vec<(&str, String)> {
        edit.marks()
            .iter()
            .filter_map(|mark| match &mark.mark {
                rune::Mark::Link(url) => Some((&edit.text[mark.range()], url.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_web_address_in_plain_text_should_show_as_a_link() {
        let edit = rich_body("see https://matrix.org/blog.", None).map(|rich| rich.edit).unwrap_or_default();
        assert_eq!(links(&edit), [("https://matrix.org/blog", "https://matrix.org/blog".to_owned())]);
    }

    fn text(body: &str, html: &str) -> TimelineItem {
        let content = MessageContent::Text { body: body.into(), html: Some(html.into()) };
        TimelineItem::new(Message { content, ..Message::default() })
    }

    #[test]
    fn a_link_beside_a_hidden_spoiler_should_get_no_preview() {
        let item = text("https://secret.example", "<span data-mx-spoiler>a secret</span>");
        assert_eq!(item.card_link(false), None);
    }

    #[test]
    fn a_gif_beside_a_hidden_spoiler_should_not_play() {
        let item = text("https://giphy.com/gifs/x-abc", "<span data-mx-spoiler>a gif</span>");
        assert!(matrix_core::links::find_gif_link("https://giphy.com/gifs/x-abc").is_some());
        assert!(item.gif.is_none());
    }

    #[test]
    fn a_link_in_text_without_a_spoiler_should_get_a_preview() {
        let item = text("https://matrix.org", "<b>https://matrix.org</b>");
        assert_eq!(item.card_link(false), Some("https://matrix.org"));
    }

    #[test]
    fn plain_text_without_links_should_stay_plain() {
        assert!(rich_body("hello there", None).is_none());
    }

    #[test]
    fn markdown_in_a_plain_body_should_be_formatted() {
        let edit =
            rich_body("the **build** is [green](https://ci.example)", None).map(|rich| rich.edit).unwrap_or_default();
        assert_eq!(edit.text, "the build is green");
        assert!(edit.marks().iter().any(|mark| mark.mark == rune::Mark::Bold && &edit.text[mark.range()] == "build"));
        assert_eq!(links(&edit), [("green", "https://ci.example".to_owned())]);
    }

    #[test]
    fn markdown_should_not_override_the_senders_formatting() {
        let edit = rich_body("**raw**", Some("<em>**raw**</em>")).map(|rich| rich.edit).unwrap_or_default();
        assert_eq!(edit.text, "**raw**");
    }

    #[test]
    fn a_link_the_sender_made_should_keep_its_address() {
        let edit = rich_body("", Some(r#"<a href="https://a.example">https://b.example</a> and https://c.example"#))
            .map(|rich| rich.edit)
            .unwrap_or_default();
        assert_eq!(
            links(&edit),
            [
                ("https://b.example", "https://a.example".to_owned()),
                ("https://c.example", "https://c.example".to_owned())
            ]
        );
    }

    #[test]
    fn a_code_block_should_keep_its_language() {
        let rich = rich_body("", Some(r#"<pre><code class="language-rust">fn main() {}</code></pre>"#));
        let languages = rich.map(|rich| rich.code.into_iter().map(|block| block.language).collect::<Vec<_>>());
        assert_eq!(languages, Some(vec![Some("rust".to_owned())]));
    }

    #[test]
    fn a_markdown_code_fence_in_a_plain_body_should_keep_its_language() {
        let rich = rich_body("```python\nprint(1)\n```", None);
        let languages = rich.map(|rich| rich.code.into_iter().map(|block| block.language).collect::<Vec<_>>());
        assert_eq!(languages, Some(vec![Some("python".to_owned())]));
    }

    fn placed(html: &str) -> Vec<(usize, usize, Option<String>)> {
        let rich = RichText::from_html(html);
        place_code(&rich.edit, &rich.code)
            .into_iter()
            .map(|code| (code.first, code.last, code.language.map(str::to_owned)))
            .collect()
    }

    #[test]
    fn code_blocks_that_follow_each_other_should_each_keep_their_lines() {
        let html =
            "<pre><code class=\"language-rust\">a\nb</code></pre><pre><code class=\"language-python\">c</code></pre>";
        assert_eq!(placed(html), [(0, 1, Some("rust".into())), (2, 2, Some("python".into()))]);
    }

    #[test]
    fn a_blank_line_rune_puts_between_code_blocks_should_belong_to_neither() {
        let html = "<pre><code class=\"language-rust\">a\n</code></pre>\n<pre><code class=\"language-python\">b\n</code></pre>";
        assert_eq!(placed(html), [(0, 0, Some("rust".into())), (2, 2, Some("python".into()))]);
    }

    #[test]
    fn a_code_block_starting_with_a_blank_line_should_keep_its_place() {
        let html = "<pre><code class=\"language-rust\">\n\nfn a() {}</code></pre><pre><code class=\"language-python\">b</code></pre>";
        assert_eq!(placed(html), [(0, 1, Some("rust".into())), (2, 2, Some("python".into()))]);
    }

    #[test]
    fn code_lines_that_dont_line_up_with_the_blocks_should_place_nothing() {
        let rich = RichText::from_html("<pre><code class=\"language-rust\">a\nb\nc</code></pre>");
        let one_line = [matrix_core::html::CodeBlock { language: Some("rust".into()), lines: 1 }];
        assert!(place_code(&rich.edit, &one_line).is_empty());
    }

    #[test]
    fn an_address_in_code_should_stay_code() {
        let edit = rich_body("", Some("<code>https://a.example</code>")).map(|rich| rich.edit).unwrap_or_default();
        assert!(links(&edit).is_empty());
    }

    fn summary(user_id: &str) -> AccountSummary {
        AccountSummary {
            id: AccountId::new(user_id),
            display_name: user_id.into(),
            homeserver: "example.org".into(),
            default_notifications: NotificationMode::AllMessages,
            avatar: None,
            sign_in: matrix_core::SignInMethod::Password,
        }
    }

    fn message(event_id: &str) -> Message {
        Message {
            event_id: event_id.into(),
            sender_id: "@a:example.org".into(),
            sender_name: "a".into(),
            content: MessageContent::Text { body: "hi".into(), html: None },
            ..Message::default()
        }
    }

    fn entry(id: &str) -> TimelineEntry {
        TimelineEntry { id: id.into(), kind: EntryKind::Message(Box::new(message(&format!("${id}")))) }
    }

    fn entry_ids(timeline: &RoomTimeline) -> Vec<&str> {
        timeline.entries.iter().map(|e| e.id.as_str()).collect()
    }

    fn room(id: &str) -> RoomSummary {
        RoomSummary {
            room_id: id.into(),
            name: id.into(),
            topic: None,
            kind: RoomKind::Group,
            unread: 0,
            mentions: 0,
            dm_users: Vec::new(),
            avatar: None,
            member_count: 0,
            active_member_count: 0,
            history: matrix_core::HistoryVisibility::Shared,
            notifications: NotificationMode::AllMessages,
            custom_notifications: false,
            call: matrix_core::RoomCall::default(),
            activity_ms: None,
            encrypted: false,
            public: false,
            can_rename: false,
            pinned: Vec::new(),
            can_pin: true,
            saved: Vec::new(),
            priority: matrix_core::RoomPriority::Normal,
            marked_unread: false,
            can_change_settings: false,
            knocks: 0,
            successor: None,
        }
    }

    fn dm(id: &str, with: &str) -> RoomSummary {
        RoomSummary { kind: RoomKind::Direct, dm_users: vec![with.into()], ..room(id) }
    }

    fn space(id: &str, children: &[&str], members: &[&str]) -> SpaceSummary {
        let strings = |xs: &[&str]| xs.iter().map(|x| (*x).to_owned()).collect();
        SpaceSummary {
            room_id: id.into(),
            name: id.into(),
            avatar: None,
            can_add_rooms: true,
            can_rename: true,
            can_change_settings: true,
            children: strings(children),
            members: strings(members),
        }
    }

    /// Rooms `!a`, `!b`, `!c` and direct chats with `@grace` and `@kai`. Space `!s` (member
    /// `@grace`) holds `!a` and subspace `!t`, which holds `!b` and, cyclically, `!s`.
    fn nested() -> AccountData {
        let mut account = AccountData::new(summary("@a:example.org"));
        account.rooms = vec![room("!a"), room("!b"), room("!c"), dm("!dg", "@grace"), dm("!dk", "@kai")];
        account.spaces = vec![space("!s", &["!a", "!t"], &["@grace"]), space("!t", &["!b", "!s"], &[])];
        account
    }

    fn ids<'a>(rooms: impl Iterator<Item = &'a RoomSummary>) -> Vec<&'a str> {
        rooms.map(|r| r.room_id.as_str()).collect()
    }

    #[test]
    fn upsert_should_not_duplicate_a_known_account() {
        let mut accounts = Accounts::default();
        accounts.upsert(summary("@a:example.org"));
        accounts.upsert(summary("@a:example.org"));
        assert_eq!(accounts.iter().count(), 1);
    }

    #[test]
    fn diffs_should_apply_in_order() {
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![
            TimelineDiff::Reset(vec![entry("b"), entry("c")]),
            TimelineDiff::PushFront(entry("a")),
            TimelineDiff::Insert { index: 3, entry: entry("d") },
            TimelineDiff::Remove { index: 1 },
        ]);
        assert_eq!(entry_ids(&timeline), ["a", "c", "d"]);
    }

    #[test]
    fn a_diff_out_of_range_should_be_ignored() {
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![TimelineDiff::Reset(vec![entry("a")]), TimelineDiff::Remove { index: 5 }]);
        assert_eq!(entry_ids(&timeline), ["a"]);
    }

    #[test]
    fn the_start_entry_should_mark_the_history_complete() {
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![TimelineDiff::PushFront(TimelineEntry { id: "s".into(), kind: EntryKind::Start })]);
        assert!(timeline.reached_start);
    }

    #[test]
    fn a_formatted_message_should_be_parsed_once_on_arrival() {
        let mut bold = message("$1");
        bold.content = MessageContent::Text { body: "hi".into(), html: Some("<b>hi</b>".into()) };
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![TimelineDiff::PushBack(TimelineEntry {
            id: "1".into(),
            kind: EntryKind::Message(Box::new(bold)),
        })]);
        let EntryView::Message(item) = &timeline.entries[0].kind else { panic!("a message") };
        assert_eq!(item.rich.as_ref().map(|r| r.edit.text.as_str()), Some("hi"));
    }

    #[test]
    fn remove_should_forget_the_account() {
        let mut accounts = Accounts::default();
        let id = AccountId::new("@a:example.org");
        accounts.upsert(summary(id.as_str()));
        accounts.remove(&id);
        assert!(accounts.is_empty());
    }

    #[test]
    fn a_muted_room_should_draw_attention_only_for_mentions() {
        let muted = RoomSummary { unread: 5, notifications: NotificationMode::Mute, ..room("!m") };
        assert!(!wants_attention(&muted));
    }

    #[test]
    fn a_room_marked_unread_should_draw_attention_even_when_muted() {
        let room = RoomSummary { unread: 0, marked_unread: true, notifications: NotificationMode::Mute, ..room("!m") };
        assert!(wants_attention(&room));
    }

    #[test]
    fn a_mention_should_draw_attention_even_without_unread_messages() {
        let room = RoomSummary { unread: 0, mentions: 1, ..room("!m") };
        assert!(wants_attention(&room));
    }

    #[test]
    fn a_mentions_only_room_should_draw_attention_when_mentioned() {
        let room = RoomSummary { unread: 5, mentions: 1, notifications: NotificationMode::MentionsOnly, ..room("!m") };
        assert!(wants_attention(&room));
    }

    #[test]
    fn a_new_default_should_reach_rooms_without_their_own_mode() {
        let mut accounts = Accounts::default();
        let id = AccountId::new("@a:example.org");
        accounts.upsert(summary(id.as_str()));
        let own = RoomSummary { custom_notifications: true, ..room("!own") };
        accounts.set_room_list(
            &id,
            RoomList { rooms: vec![room("!follows"), own], spaces: Vec::new(), invitations: Vec::new() },
        );
        accounts.set_default_notifications(&id, NotificationMode::Mute);
        let modes: Vec<_> = accounts.get(&id).unwrap().rooms.iter().map(|r| r.notifications).collect();
        assert_eq!(modes, [NotificationMode::Mute, NotificationMode::AllMessages]);
    }

    #[test]
    fn the_overview_should_list_mentions_before_other_unread_rooms() {
        let mut account = nested();
        account.rooms[2] = RoomSummary { unread: 9, ..room("!c") };
        account.rooms[3] = RoomSummary { unread: 1, mentions: 1, ..dm("!dg", "@grace") };
        let unread: Vec<_> = account.overview(&Scope::Home).unread.iter().map(|r| r.room_id.as_str()).collect();
        assert_eq!(unread, ["!dg", "!c"]);
    }

    #[test]
    fn the_overview_should_list_recent_conversations_newest_first() {
        let mut account = nested();
        let at = |id: &str, ms| Message { timestamp_ms: ms, ..message(id) };
        account.latest.insert("!c".into(), at("$old", 1));
        account.latest.insert("!dk".into(), at("$new", 2));
        let recent: Vec<_> = account.overview(&Scope::Home).recent.iter().map(|(r, _)| r.room_id.as_str()).collect();
        assert_eq!(recent, ["!dk", "!c"]);
    }

    #[test]
    fn a_new_room_list_should_be_found_by_id() {
        let mut accounts = Accounts::default();
        let id = AccountId::new("@a:example.org");
        accounts.upsert(summary(id.as_str()));
        let list = |rooms: Vec<RoomSummary>| RoomList { rooms, spaces: Vec::new(), invitations: Vec::new() };
        accounts.set_room_list(&id, list(vec![room("!old")]));
        assert!(accounts.get(&id).unwrap().room("!old").is_some());
        accounts.set_room_list(&id, list(vec![room("!new"), room("!other")]));
        let account = accounts.get(&id).unwrap();
        assert_eq!(
            (account.room("!old").is_some(), account.room("!other").map(|r| r.name.as_str())),
            (false, Some("!other"))
        );
    }

    #[test]
    fn home_should_show_only_rooms_outside_every_space() {
        assert_eq!(ids(nested().rooms_in(&Scope::Home, RoomKind::Group)), ["!c"]);
    }

    #[test]
    fn home_should_show_every_direct_chat() {
        assert_eq!(ids(nested().rooms_in(&Scope::Home, RoomKind::Direct)), ["!dg", "!dk"]);
    }

    #[test]
    fn a_space_should_show_direct_chats_only_with_its_members() {
        assert_eq!(ids(nested().rooms_in(&Scope::Space("!s".into()), RoomKind::Direct)), ["!dg"]);
    }

    #[test]
    fn a_space_should_show_rooms_of_its_subspaces_despite_cycles() {
        assert_eq!(ids(nested().rooms_in(&Scope::Space("!s".into()), RoomKind::Group)), ["!a", "!b"]);
    }

    #[test]
    fn before_the_view_reports_the_newest_entries_should_count_as_on_screen() {
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![TimelineDiff::Reset((0..40).map(|i| entry(&i.to_string())).collect())]);
        let shown: Vec<&str> = timeline.on_screen(None).map(|e| e.id.as_str()).collect();
        assert_eq!((shown.len(), shown.first().copied()), (SHOWN_ON_OPEN, Some("10")));
    }

    #[test]
    fn once_the_view_reports_only_its_rows_should_count_as_on_screen() {
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![TimelineDiff::Reset((0..40).map(|i| entry(&i.to_string())).collect())]);
        let reported = Shown { revision: timeline.revision, range: 3..4, ids: HashSet::from(["3".to_owned()]) };
        let shown: Vec<&str> = timeline.on_screen(Some(&reported)).map(|e| e.id.as_str()).collect();
        assert_eq!(shown, ["3"]);
    }

    #[test]
    fn a_report_from_before_a_change_should_still_find_its_entries() {
        let mut timeline = RoomTimeline::default();
        timeline.apply(vec![TimelineDiff::Reset((0..40).map(|i| entry(&i.to_string())).collect())]);
        let reported = Shown { revision: timeline.revision, range: 3..4, ids: HashSet::from(["3".to_owned()]) };
        timeline.revision += 1;
        timeline.apply(vec![TimelineDiff::PushFront(entry("older"))]);
        let shown: Vec<&str> = timeline.on_screen(Some(&reported)).map(|e| e.id.as_str()).collect();
        assert_eq!(shown, ["3"]);
    }

    #[test]
    fn changing_a_timeline_should_give_it_a_new_revision() {
        let mut accounts = Accounts::default();
        let id = AccountId::new("@a:example.org");
        accounts.upsert(summary("@a:example.org"));
        let key = TimelineKey::room("!r");
        let first = accounts.timeline_mut(&id, &key).unwrap().revision;
        let second = accounts.timeline_mut(&id, &key).unwrap().revision;
        assert!(first != 0 && second != first);
    }
}
