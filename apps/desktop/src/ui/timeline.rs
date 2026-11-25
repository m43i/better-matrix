//! A timeline on screen, for the room and for a thread: messages grouped by sender with date
//! dividers, the start of visible history, and each message's parts: the quoted message it
//! answers, its body or attachment, a linked GIF, reactions, its thread, and the hover toolbar.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use matrix_core::{
    AccountId, Attachment, AttachmentKind, CallCard, HistoryVisibility, Message, MessageContent, Person, Quote,
    RoomChange, RoomEvent, RoomSummary, ThreadSummary, TimelineKey,
};
use rune::{Color, El, ScrollSpec, SurfaceFit, col, el, icon, row, row as row_el, text};

use super::components::{
    ButtonKind, PRESENCE_OVERHANG, button, clock_time, local_ms, media_avatar, presence_text, with_presence,
};
use super::icons;
use super::room::{display_name, inline_name};
use super::theme::{Palette, font};
use crate::i18n::{I18N, t, tf, tp};
use crate::state::{
    AccountData, AudioClip, ComposerTarget, EmojiFor, EntryView, GifCache, LinkPreviews, MediaCache, MessageLayout,
    PlaybackSpeed, Popover, RichText, RoomTimeline, Scrub, State, TimelineItem, audio_supported,
};
use matrix_core::links::LinkPreview;

/// A timeline row's height before it is measured: a message with its header.
const ESTIMATED_ROW: f32 = 64.0;

const AVATAR: f32 = 36.0;
/// A message row's side padding, and the gap between its parts.
const MESSAGE_PAD_X: f32 = 20.0;
const MESSAGE_GAP: f32 = 10.0;

/// What every message row in one timeline needs.
pub struct Context<'a> {
    pub account: &'a AccountId,
    pub key: &'a TimelineKey,
    pub room: &'a RoomSummary,
    pub media: &'a MediaCache,
    pub data: &'a AccountData,
    pub gifs: &'a GifCache,
    pub show_gifs: bool,
    /// Links' preview cards, when they show.
    pub link_previews: Option<&'a LinkPreviews>,
    /// The message whose reaction picker is open, if any.
    pub picking: Option<&'a str>,
    /// The standard of the call the user is in, here in this room.
    pub call_here: Option<matrix_core::CallVia>,
    /// A one-to-one call rings in this room.
    pub ringing: bool,
    /// The audio playing, if any.
    pub audio: Option<&'a AudioClip>,
    /// How fast clips play.
    pub playback_speed: PlaybackSpeed,
    /// What clips played here read from their files.
    pub decoded_audio: &'a [crate::state::DecodedAudio],
    /// The most reader pictures a read indicator shows: fewer in a narrow timeline.
    pub reader_faces: usize,
    /// Whether messages keep the readers' column: not in a timeline too narrow for it, where an
    /// indicator takes a line under its message instead.
    pub readers_column: bool,
    /// The widest a message's content can be, once the timeline has a size: pictures and GIFs
    /// fit it, so they never reach into the readers' column.
    pub content_width: f32,
    /// The message just jumped to, lit a moment.
    pub flash: Option<&'a str>,
    /// Spoilers the user revealed.
    pub spoilers: &'a std::collections::HashSet<String>,
    /// Shared places show their map.
    pub show_maps: bool,
    /// Messages whose held-back media the user chose to show.
    pub revealed: &'a std::collections::HashSet<(AccountId, String)>,
    /// Hidden events get a line (developer tools).
    pub show_hidden: bool,
    /// Profile and room-setting changes get a line too, not only the room log.
    pub show_room_changes: bool,
    /// Classic rows or chat bubbles.
    pub layout: MessageLayout,
    /// Which message's hover toolbar the pointer is with.
    pub hover: &'a crate::state::MessageHover,
    /// Something floats over the timelines (`State::timeline_covered`): its rows don't hover.
    pub covered: bool,
}

impl<'a> Context<'a> {
    /// What the rows of `key`, in `room` of the account `data`, need from the state.
    pub fn new(s: &'a State, data: &'a AccountData, room: &'a RoomSummary, key: &'a TimelineKey) -> Self {
        Context {
            account: &data.summary.id,
            key,
            room,
            media: &s.media,
            data,
            gifs: &s.gifs,
            show_gifs: s.settings.show_link_gifs,
            link_previews: s.settings.show_link_previews.then_some(&s.link_previews),
            picking: super::room::picking(s, key),
            call_here: s.call_here().filter(|call| call.info.room_id == key.room_id).map(|call| call.info.via),
            ringing: s.rings.iter().any(|ring| ring.account == data.summary.id && ring.room_id == key.room_id),
            audio: s.audio.as_ref(),
            playback_speed: s.settings.playback_speed,
            decoded_audio: &s.decoded_audio,
            reader_faces: crate::state::READER_FACES,
            readers_column: key.thread.is_none(),
            content_width: f32::INFINITY,
            flash: s.flash.as_ref().map(|flash| flash.event_id.as_str()),
            spoilers: &s.revealed,
            show_maps: s.settings.show_maps,
            revealed: &s.revealed_media,
            show_hidden: s.settings.show_hidden_events,
            show_room_changes: s.settings.show_room_changes,
            layout: s.settings.message_layout,
            hover: &s.message_hover,
            covered: false,
        }
    }

    /// What the text of the message `event_id` needs to draw.
    fn look(&self, message: &'a Message) -> super::message_text::Look<'a> {
        super::message_text::Look {
            event_id: &message.event_id,
            me: self.account.as_str(),
            revealed: self.spoilers,
            media: self.media,
            pictures: !self.hides(message),
        }
    }

    /// Whether a message's media waits for a press here (ADR 0011).
    fn hides(&self, message: &Message) -> bool {
        self.data.hides_media(&self.key.room_id, message, self.revealed)
    }

    fn in_thread(&self) -> bool {
        self.key.thread.is_some()
    }

    /// Whether a room event gets a line: a profile or room-setting change only with
    /// `show_room_changes`, else the room log alone lists it.
    fn draws(&self, event: &RoomEvent) -> bool {
        self.show_room_changes || !event.change.log_only()
    }

    /// Whether `message` sits at the timeline's right: the user's own, in chat bubbles.
    fn on_right(&self, message: &Message) -> bool {
        self.layout == MessageLayout::Bubbles && message.sender_id == self.account.as_str()
    }

    /// `message` in this timeline, as the hover toolbar's state names it.
    fn at(&self, message: &Message) -> crate::state::MessageAt {
        (self.key.clone(), message.event_id.clone())
    }

    /// A key unique to this timeline, so the room and a thread can show the same message.
    fn key(&self, name: &str) -> String {
        match self.key.thread {
            None => name.to_owned(),
            Some(_) => format!("thread-{name}"),
        }
    }
}

/// The most avatars a room event line stacks, and names it lists, before "+" and "and N more".
const EVENT_PEOPLE: usize = 4;

/// One room event, or a run of the same kind ("Ada, Grace and 3 more joined"), as a small line
/// with the people's pictures stacked in front.
fn room_event_line(key: &str, run: &[&RoomEvent], media: &MediaCache, p: &Palette) -> El {
    let mut people: Vec<&Person> = Vec::new();
    for event in run {
        if !people.iter().any(|known| known.id == event.sender.id) {
            people.push(&event.sender);
        }
    }
    let mut stack = row().center_y();
    for (index, person) in people.iter().take(EVENT_PEOPLE).enumerate() {
        let face = media_avatar(&person.id, &person.name, person.avatar.as_ref(), media, EVENT_AVATAR)
            .radius(EVENT_AVATAR / 2.0)
            .border(1.5, p.bg)
            .help(person.name.clone());
        stack = stack.child(if index == 0 { face } else { face.ml(-6.0) });
    }
    if people.len() > EVENT_PEOPLE {
        stack = stack.child(
            el().size(EVENT_AVATAR, EVENT_AVATAR)
                .ml(-6.0)
                .radius(EVENT_AVATAR / 2.0)
                .fill(p.surface)
                .border(1.5, p.bg)
                .center()
                .child(text("+").font(font::META).color(p.text_muted)),
        );
    }
    let muted = |s: String| text(s).font(font::META).color(p.text_muted);
    let line = row().key(key.to_owned()).w_full().px(20.0).py(4.0).gap(8.0).center_y().child(stack);
    if people.len() > 1 {
        // The template's words around the names, which are laid out in between so "and N more"
        // can show everyone on hover.
        let template = match run[0].change {
            RoomChange::Left => tf!("event.left_many", names = NAMES),
            _ => tf!("event.joined_many", names = NAMES),
        };
        let (before, after) = template.split_once(NAMES).unwrap_or((template.as_str(), ""));
        let names: Vec<&str> = people.iter().map(|person| person.name.as_str()).collect();
        let mut sentence = row().gap(0.0).center_y().flex_wrap().child(muted(before.to_owned()));
        if names.len() <= EVENT_PEOPLE {
            let (last, rest) = names.split_last().map_or(("", &[][..]), |(last, rest)| (*last, rest));
            sentence = sentence.child(muted(format!("{}{}{last}", rest.join(", "), t!("list.and"))));
        } else {
            let more = tf!("list.more", count = names.len() - EVENT_PEOPLE);
            sentence = sentence
                .child(muted(names[..EVENT_PEOPLE].join(", ")))
                .child(muted(more).help(names.join(", ")).cursor(rune::Cursor::Default));
        }
        line.child(sentence.child(muted(after.to_owned())))
    } else {
        line.child(muted(room_event_text(run[0])).grow(1.0).wrap())
    }
}

/// A hidden event as developer tools show it: who sent what type, and its source.
fn hidden_line(key: &str, cx: &Context<'_>, event: &matrix_core::HiddenEvent, p: &Palette) -> El {
    let line = row()
        .key(key.to_owned())
        .w_full()
        .px(20.0)
        .py(2.0)
        .gap(8.0)
        .center_y()
        .child(icon(&icons::CODE, 14.0).color(p.text_muted))
        .child(
            text(tf!("event.hidden", name = event.sender.name, kind = event.event_type))
                .font(font::META)
                .color(p.text_muted)
                .grow(1.0)
                .min_w(0.0)
                .truncate()
                .selectable(true),
        );
    let Some(event_id) = event.event_id.clone() else { return line };
    let (account, room_id) = (cx.account.clone(), cx.key.room_id.clone());
    line.child(
        button(t!("actions.view_source"), ButtonKind::Link, true, p)
            .key(format!("{key}-source"))
            .on_press(move |s: &mut State| s.view_source(account.clone(), room_id.clone(), event_id.clone())),
    )
}

/// Marks where the names go in a "many" template.
const NAMES: &str = "\u{1}";
/// The size of the pictures on a room event line.
const EVENT_AVATAR: f32 = 18.0;

/// What one room event says, in the user's language.
pub(super) fn room_event_text(event: &RoomEvent) -> String {
    let name = event.sender.name.as_str();
    match &event.change {
        RoomChange::Created => tf!("event.created", name = name),
        RoomChange::Joined => tf!("event.joined", name = name),
        RoomChange::Left => tf!("event.left", name = name),
        RoomChange::Invited(target) => tf!("event.invited", name = name, target = target.name),
        RoomChange::InviteRevoked(target) => tf!("event.invite_revoked", name = name, target = target.name),
        RoomChange::InviteDeclined => tf!("event.invite_declined", name = name),
        RoomChange::Removed(target) => tf!("event.removed", name = name, target = target.name),
        RoomChange::Banned(target) => tf!("event.banned", name = name, target = target.name),
        RoomChange::Unbanned(target) => tf!("event.unbanned", name = name, target = target.name),
        RoomChange::Knocked => tf!("event.knocked", name = name),
        RoomChange::DisplayName(Some(new_name)) => tf!("event.display_name", name = name, new_name = new_name),
        RoomChange::DisplayName(None) => tf!("event.display_name_removed", name = name),
        RoomChange::ProfilePicture => tf!("event.profile_picture", name = name),
        RoomChange::RoomName(room) => tf!("event.room_name", name = name, room = room),
        RoomChange::RoomTopic(topic) => tf!("event.room_topic", name = name, topic = topic),
        RoomChange::RoomPicture => tf!("event.room_picture", name = name),
        RoomChange::Encrypted => tf!("event.encrypted", name = name),
        RoomChange::HistoryVisibility(_) => tf!("event.history", name = name),
        RoomChange::JoinRules => tf!("event.join_rules", name = name),
        RoomChange::Permissions => tf!("event.permissions", name = name),
        RoomChange::PinnedMessages => tf!("event.pinned", name = name),
        RoomChange::Upgraded => tf!("event.upgraded", name = name),
        RoomChange::Addresses => tf!("event.addresses", name = name),
        RoomChange::GuestAccess => tf!("event.guest_access", name = name),
        RoomChange::ServerAccess => tf!("event.server_access", name = name),
        RoomChange::SpaceRooms => tf!("event.space_rooms", name = name),
        RoomChange::Spaces => tf!("event.spaces", name = name),
        RoomChange::InvitedByContact(target) => tf!("event.invited", name = name, target = target),
        RoomChange::ModerationRules => tf!("event.moderation_rules", name = name),
        RoomChange::LiveLocation => tf!("event.live_location", name = name),
        RoomChange::WidgetAdded { meeting: Some(_), .. } => tf!("event.meeting_started", name = name),
        RoomChange::WidgetAdded { name: widget, meeting: None } => {
            tf!("event.widget_added", name = name, widget = widget)
        }
        RoomChange::WidgetRemoved => tf!("event.widget_removed", name = name),
    }
}

/// The element key of a message's row: from its timeline entry, which keeps its ID when a sent
/// message's local ID turns into the server's event ID, so the row isn't replaced then.
fn row_key(key: &TimelineKey, entry_id: &str) -> String {
    match key.thread {
        None => format!("msg-{entry_id}"),
        Some(_) => format!("thread-msg-{entry_id}"),
    }
}

/// One row of a timeline, planned before any is built: rows are built only when the virtual
/// list mounts them, from the state, by this plan.
#[derive(Clone, Debug)]
enum Row {
    /// Where the room's visible history begins.
    Start,
    HistoryFailed(String),
    Loading,
    /// A message, continuing the one before (same sender, close in time) or not. `latest_call`:
    /// the newest call card, the one that shows whether the room's call is under way.
    Message {
        entry: usize,
        continues: bool,
        latest_call: bool,
    },
    /// Room events from `start` to `end` (entries), shown as one line.
    Events {
        start: usize,
        end: usize,
    },
    Divider(usize),
    ReadMarker(String),
    /// A hidden event (an entry), while hidden events show.
    Hidden(usize),
}

/// Rows either side of the viewport whose media loads before they scroll into view.
const SHOWN_MARGIN: usize = 10;

impl Row {
    /// The timeline entries the row shows.
    fn entries(&self) -> Option<std::ops::Range<usize>> {
        match *self {
            Row::Message { entry, .. } | Row::Divider(entry) | Row::Hidden(entry) => Some(entry..entry + 1),
            Row::Events { start, end } => Some(start..end),
            Row::Start | Row::HistoryFailed(_) | Row::Loading | Row::ReadMarker(_) => None,
        }
    }
}

type Plan = Rc<Vec<(String, Row)>>;

thread_local! {
    /// Each timeline view's last plan, by view key, with the timeline and revision it is of.
    static PLANS: std::cell::RefCell<HashMap<String, (AccountId, TimelineKey, u64, Plan)>> =
        std::cell::RefCell::new(HashMap::new());
}

/// The plan of a timeline, made again only when it changed since the view last planned it.
fn cached_plan(view_key: &str, cx: &Context<'_>, timeline: Option<&RoomTimeline>, revision: u64) -> Plan {
    PLANS.with(|plans| {
        let mut plans = plans.borrow_mut();
        if revision != 0
            && let Some((account, key, planned, plan)) = plans.get(view_key)
            && account == cx.account
            && key == cx.key
            && *planned == revision
        {
            return Rc::clone(plan);
        }
        let plan = Rc::new(plan(cx, timeline));
        plans.insert(view_key.to_owned(), (cx.account.clone(), cx.key.clone(), revision, Rc::clone(&plan)));
        plan
    })
}

/// What a timeline's plan is made of: its revision, and whom the user ignores. 0 (never cached)
/// while the timeline has no revision.
fn plan_version(revision: u64, ignored: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    if revision == 0 {
        return 0;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (revision, ignored).hash(&mut hasher);
    hasher.finish().max(1)
}

/// The rows of a timeline and their keys: one cheap pass over the entries. Messages from people
/// the user ignores have no row: a timeline kept from before the ignore still holds them until
/// the engine reloads it. A date divider whose day draws nothing (only room changes the room log
/// keeps, hidden events, ignored people) has no row either.
fn plan(cx: &Context<'_>, timeline: Option<&RoomTimeline>) -> Vec<(String, Row)> {
    let mut rows = Vec::new();
    let entries = timeline.map_or(&[][..], |t| t.entries.as_slice());
    let has_start = entries.iter().any(|e| matches!(e.kind, EntryView::Start));
    if !cx.in_thread() && timeline.is_some_and(|t| t.reached_start) && !has_start {
        rows.push(("timeline-start".to_owned(), Row::Start));
    }
    if let Some(error) = timeline.and_then(|t| t.history_error.clone()) {
        rows.push((cx.key("timeline-history-failed"), Row::HistoryFailed(error)));
    } else if timeline.is_some_and(|t| t.loading && !t.reached_start) {
        rows.push((cx.key("timeline-loading"), Row::Loading));
    }
    let mut previous: Option<&Message> = None;
    let fading = timeline.and_then(RoomTimeline::fading_read_marker);
    // The last date divider's row, until a row under it draws something.
    let mut empty_day: Option<usize> = None;
    let mut index = 0;
    while let Some(entry) = entries.get(index) {
        let at = index;
        if let Some((id, boundary)) = fading
            && boundary == at
        {
            rows.push((cx.key(id), Row::ReadMarker(id.to_owned())));
        }
        index += 1;
        let drawn = rows.len();
        match &entry.kind {
            // Left to the room log, it is passed over like a hidden event.
            EntryView::RoomEvent(event) if !cx.draws(event) => {}
            EntryView::RoomEvent(event) => {
                // Joins (or leaves) in a row read as one line; entries between them that draw
                // nothing don't break the run.
                while event.change.groups()
                    && fading.is_none_or(|(_, boundary)| boundary != index)
                    && let Some(next) = entries.get(index)
                {
                    match &next.kind {
                        EntryView::RoomEvent(other) if other.change == event.change => {}
                        EntryView::RoomEvent(other) if !cx.draws(other) => {}
                        EntryView::Hidden(_) if !cx.show_hidden => {}
                        _ => break,
                    }
                    index += 1;
                }
                rows.push((cx.key(&entry.id), Row::Events { start: at, end: index }));
                previous = None;
            }
            EntryView::Message(item) if cx.data.ignores(&item.message.sender_id) => {}
            EntryView::Message(item) => {
                let message = &item.message;
                let continues = previous.is_some_and(|prev| crate::state::continues_group(prev, message));
                rows.push((row_key(cx.key, &entry.id), Row::Message { entry: at, continues, latest_call: false }));
                previous = Some(message);
            }
            EntryView::ReadMarker if timeline.is_some_and(|t| t.read_marker_seen(&entry.id)) => {}
            EntryView::ReadMarker => rows.push((cx.key(&entry.id), Row::ReadMarker(entry.id.clone()))),
            EntryView::DateDivider(_) => {
                if let Some(empty) = empty_day.take() {
                    rows.remove(empty);
                }
                empty_day = Some(rows.len());
                rows.push((cx.key(&entry.id), Row::Divider(at)));
                previous = None;
                continue;
            }
            EntryView::Start if !cx.in_thread() => {
                rows.push((entry.id.clone(), Row::Start));
                previous = None;
            }
            // Not even developer tools show what people the user ignores sent.
            EntryView::Hidden(event) if cx.show_hidden && !cx.data.ignores(&event.sender.id) => {
                rows.push((cx.key(&entry.id), Row::Hidden(at)));
                previous = None;
            }
            EntryView::Start | EntryView::Hidden(_) => {}
        }
        // Anything but the "New" marker gives the day under the divider something to show.
        if rows[drawn..].iter().any(|(_, row)| !matches!(row, Row::ReadMarker(_))) {
            empty_day = None;
        }
    }
    if let Some(empty) = empty_day {
        rows.remove(empty);
    }
    if let Some((id, boundary)) = fading
        && boundary == entries.len()
    {
        rows.push((cx.key(id), Row::ReadMarker(id.to_owned())));
    }
    let is_call = |entry: usize| matches!(&entries[entry].kind, EntryView::Message(item) if matches!(item.message.content, MessageContent::Call(_)));
    if let Some((_, Row::Message { latest_call, .. })) =
        rows.iter_mut().rev().find(|(_, row)| matches!(row, Row::Message { entry, .. } if is_call(*entry)))
    {
        *latest_call = true;
    }
    rows
}

/// What a row needs from the state, found again when the row is built.
fn row_context<'a>(s: &'a State, account: &AccountId, key: &'a TimelineKey) -> Option<(Context<'a>, &'a RoomTimeline)> {
    let data = s.accounts.get(account)?;
    Some((Context::new(s, data, data.room(&key.room_id)?, key), data.timeline(key)?))
}

/// The room events a run of entries shows: its first event and the others of the same kind. What
/// the run passed over (hidden events, room changes the room log keeps) isn't part of it.
fn run_of(entries: &[crate::state::Entry]) -> Vec<&RoomEvent> {
    let mut events = entries.iter().filter_map(|entry| match &entry.kind {
        EntryView::RoomEvent(event) => Some(event.as_ref()),
        _ => None,
    });
    let Some(first) = events.next() else { return Vec::new() };
    std::iter::once(first).chain(events.filter(|event| event.change == first.change)).collect()
}

/// Builds one planned row.
fn build_row(cx: &Context<'_>, timeline: &RoomTimeline, key: &str, row: &Row, p: &Palette) -> El {
    let entries = &timeline.entries;
    match row {
        Row::Start => start(key, cx.room, p),
        Row::HistoryFailed(error) => {
            let timeline_key = cx.key.clone();
            row_el()
                .key(key.to_owned())
                .w_full()
                .px(20.0)
                .py(8.0)
                .gap(8.0)
                .center()
                .child(text(t!("timeline.history_failed")).font(font::META).color(p.text_muted))
                .child(
                    button(t!("common.try_again"), ButtonKind::Link, true, p)
                        .key(cx.key("timeline-retry"))
                        .label(tf!("timeline.history_failed_detail", error = error))
                        .on_press(move |s: &mut State| s.retry_history(&timeline_key)),
                )
        }
        Row::Loading => row_el()
            .key(key.to_owned())
            .w_full()
            .h(40.0)
            .center()
            .child(text(t!("timeline.loading")).font(font::META).color(p.text_muted)),
        Row::Message { entry, continues, latest_call } => match entries.get(*entry).map(|e| &e.kind) {
            Some(EntryView::Message(item)) => message_row(cx, key, item, *continues, *latest_call, p),
            _ => el().key(key.to_owned()),
        },
        Row::Events { start: from, end } => {
            let run = run_of(&entries[(*from).min(entries.len())..(*end).min(entries.len())]);
            if run.is_empty() { el().key(key.to_owned()) } else { room_event_line(key, &run, cx.media, p) }
        }
        Row::Hidden(entry) => match entries.get(*entry).map(|e| &e.kind) {
            Some(EntryView::Hidden(event)) => hidden_line(key, cx, event, p),
            _ => el().key(key.to_owned()),
        },
        Row::Divider(entry) => match entries.get(*entry).map(|e| &e.kind) {
            Some(EntryView::DateDivider(at)) => {
                rule_with_label(key, &date_label(local_ms(*at), local_ms(now_ms())), p.text_muted, p)
            }
            _ => el().key(key.to_owned()),
        },
        Row::ReadMarker(id) => {
            let opacity = timeline.read_marker_opacity(id);
            let divider = rule_with_label(key, t!("timeline.new"), p.mention, p)
                .opacity(opacity)
                .opacity_motion(rune::Spring::new(crate::state::READ_MARKER_FADE, 0.0));
            // A fade runs on through history landing around it; a divider a page brings lands
            // with the page.
            if opacity < 1.0 { divider.animated_changes() } else { divider }
        }
    }
}

/// The scrolling list of a timeline. `view_key` names the viewport; `near_top` is raised when it
/// is scrolled within a screen of its oldest message.
pub fn timeline(
    cx: &Context<'_>,
    timeline: Option<&RoomTimeline>,
    view_key: &str,
    near_top: &Arc<AtomicBool>,
    at_bottom: Option<&Arc<AtomicBool>>,
    p: &Palette,
) -> El {
    // Only the rows near the viewport are built, laid out and drawn: a room of thousands of
    // messages commits as fast as a short one. The plan is one pass, made again only when the
    // timeline changed; each mounted row is built from the state by its plan entry.
    let revision = timeline.map_or(0, |t| t.revision);
    // The plan leaves out people the user ignores: it is made again when that list changes too.
    // Everyone left out as ignored (shown, saved or being saved): the rows change with them.
    let version = plan_version(revision, cx.data.ignores_fingerprint());
    let plan = cached_plan(view_key, cx, timeline, version);
    let (keys, shown) = (Rc::clone(&plan), Rc::clone(&plan));
    let (account, key, palette) = (cx.account.clone(), cx.key.clone(), *p);
    let (seen_account, seen_key) = (account.clone(), key.clone());
    let frame_width = rune::memo::layout_with(&format!("{view_key}-frame"), |frame| frame[2]);
    let reader_faces = match frame_width {
        Some(width) if width < super::readers::NARROW_TIMELINE => super::readers::NARROW_FACES,
        _ => crate::state::READER_FACES,
    };
    // Less the row's padding, the sender's picture and the readers' column, each with its gap;
    // a timeline that can't spare the column for messages drops it.
    let beside = frame_width.map_or(f32::INFINITY, |width| width - 2.0 * MESSAGE_PAD_X - AVATAR - MESSAGE_GAP);
    let readers = super::readers::column_width(reader_faces) + MESSAGE_GAP;
    let readers_column = !cx.in_thread() && beside - readers >= super::readers::MIN_CONTENT;
    let content_width = if readers_column { beside - readers } else { beside };
    let mut list = col()
        .w_full()
        .py(16.0)
        .growth(rune::GrowthSpec::default())
        .virtual_rows(
            plan.len(),
            ESTIMATED_ROW,
            move |_: &State, i| std::borrow::Cow::Owned(keys.get(i).map(|(key, _)| key.clone()).unwrap_or_default()),
            move |s: &State, i| {
                let Some((row_key, row)) = plan.get(i) else { return el() };
                match row_context(s, &account, &key) {
                    Some((mut cx, timeline)) => {
                        cx.reader_faces = reader_faces;
                        cx.readers_column = readers_column;
                        cx.content_width = content_width;
                        build_row(&cx, timeline, row_key, row, &palette)
                    }
                    None => el().key(row_key.clone()),
                }
            },
        )
        // What shows loads (pictures, previews, GIFs), with a margin to scroll into.
        .on_visible_rows(move |s: &mut State, rows| {
            let mut changed = false;
            if let Some(timeline) = s
                .accounts
                .get(&seen_account)
                .and_then(|data| data.timeline(&seen_key))
                .filter(|timeline| timeline.revision == revision)
            {
                let marker = shown.get(rows.clone()).unwrap_or_default().iter().find_map(|(_, row)| {
                    let Row::ReadMarker(id) = row else { return None };
                    Some(id.clone())
                });
                let last_message = shown.get(rows.clone()).unwrap_or_default().iter().rev().find_map(|(_, row)| {
                    let Row::Message { entry, .. } = row else { return None };
                    let item = timeline.entries.get(*entry)?.message()?;
                    (!item.pending && !item.event_id.starts_with("local:")).then(|| item.event_id.clone())
                });
                // Where reading got to is bookkeeping for read receipts: no view shows it.
                if let Some(event_id) = last_message {
                    s.read_position_shown(&seen_account, &seen_key, event_id);
                }
                changed |= s.read_marker_visible(&seen_account, &seen_key, marker);
            }
            let near = rows.start.saturating_sub(SHOWN_MARGIN)..(rows.end + SHOWN_MARGIN).min(shown.len());
            let entries = shown.get(near).unwrap_or_default().iter().filter_map(|(_, row)| row.entries());
            let (start, end) =
                entries.fold((usize::MAX, 0), |(start, end), range| (start.min(range.start), end.max(range.end)));
            if start < end {
                s.rows_shown(&seen_account, &seen_key, start..end);
            }
            changed
        });
    // The keys change only with the plan, so Rune need not ask for them again until then.
    if revision != 0 {
        list = list.virtual_rows_version(version);
    }
    let flag = Arc::clone(near_top);
    let bottom = at_bottom.map(Arc::clone);
    // Short timelines sit at the bottom, by the composer, as in chat apps: the frame takes the
    // space and pushes the viewport down; the viewport is as tall as its content, up to all of it.
    let mut viewport = col()
        .key(view_key.to_owned())
        .w_full()
        .shrink()
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        // Hold the reading position when older messages arrive above it.
        .scroll_anchor()
        // Within a screen of the oldest loaded message: ask for the page before it.
        .on_scroll_offset(move |at| {
            if let Some(bottom) = &bottom {
                bottom.store(at.max[1] - at.offset[1] <= 1.0, Ordering::Relaxed);
            }
            if at.offset[1] < at.viewport[1] {
                flag.store(true, Ordering::Relaxed);
            }
        })
        .child(list);
    // History lands whole, in this timeline only, and the viewport with it: a short timeline's
    // viewport easing taller would uncover the rows from the bottom. News elsewhere in the
    // window keeps its motion.
    if timeline.is_some_and(|t| t.landing != crate::state::Landing::Idle) {
        viewport = viewport.instant_changes();
    }
    let frame = col().key(format!("{view_key}-frame")).w_full().grow(1.0).min_h(0.0).justify_end().child(viewport);
    // Rune hovers by rectangle, not by the topmost hit: under a popover (the emoji picker, a
    // menu, search) or a composer's suggestions the rows would light up and show their toolbars
    // as the pointer crossed it. Inert, they don't, and the rest of the window keeps its focus
    // and input.
    if cx.covered { frame.inert() } else { frame }
}

/// Where the room's visible history begins: its true start, or as far back as its history
/// visibility lets this user read.
fn start(key: &str, room: &RoomSummary, p: &Palette) -> El {
    let note = match room.history {
        HistoryVisibility::Joined => Some(t!("timeline.start_hidden_joined")),
        HistoryVisibility::Invited => Some(t!("timeline.start_hidden_invited")),
        HistoryVisibility::Shared | HistoryVisibility::WorldReadable => None,
    };
    let mut block = col()
        .key(key.to_owned())
        .w_full()
        .px(20.0)
        .pb(16.0)
        .gap(6.0)
        .child(text(display_name(room)).font(font::TITLE).color(p.text))
        .child(text(tf!("timeline.start", room = inline_name(room))).font(font::BODY).color(p.text_muted).wrap());
    if let Some(note) = note {
        block = block.child(
            row()
                .key("history-hidden")
                .mt(6.0)
                .gap(8.0)
                .center_y()
                .child(icon(&icons::EYE_OFF, 16.0).color(p.text_muted))
                .child(text(note).font(font::META).color(p.text_muted).wrap()),
        );
    }
    block
}

/// A horizontal rule with a label in its middle: date dividers and the "New" marker.
fn rule_with_label(key: &str, label: &str, color: rune::Color, p: &Palette) -> El {
    let line = || el().h(1.0).grow(1.0).fill(if color == p.text_muted { p.divider } else { color });
    row()
        .key(key.to_owned())
        .w_full()
        .px(20.0)
        .py(10.0)
        .gap(12.0)
        .center_y()
        .child(line())
        .child(text(label.to_owned()).font(font::SECTION).color(color))
        .child(line())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// "Today", "Yesterday", or the date in the user's language, of times from
/// [`super::components::local_ms`].
pub fn date_label(at_ms: u64, now_ms: u64) -> String {
    const DAY: u64 = 86_400_000;
    let (day, today) = (at_ms / DAY, now_ms / DAY);
    if day == today {
        return t!("date.today").to_owned();
    }
    if day + 1 == today {
        return t!("date.yesterday").to_owned();
    }
    let (year, month, day) = civil_date(day as i64);
    let months = [
        t!("date.month_01"),
        t!("date.month_02"),
        t!("date.month_03"),
        t!("date.month_04"),
        t!("date.month_05"),
        t!("date.month_06"),
        t!("date.month_07"),
        t!("date.month_08"),
        t!("date.month_09"),
        t!("date.month_10"),
        t!("date.month_11"),
        t!("date.month_12"),
    ];
    tf!("date.format", day = day, month = months[(month - 1) as usize], year = year)
}

/// Year, month and day of a day number since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The emoji a sender's status message starts with, beside their name; the message on hover.
fn status_emoji(event_id: &str, message: Option<&str>) -> Option<El> {
    let message = message?;
    let emoji = crate::state::leading_emoji(message)?;
    Some(text(emoji.to_owned()).key(format!("status-emoji-{event_id}")).font(font::BODY).help(message.to_owned()))
}

/// One message, laid out as the settings ask: a classic row or a chat bubble.
fn message_row(
    cx: &Context<'_>,
    key: &str,
    item: &TimelineItem,
    continues: bool,
    latest_call: bool,
    p: &Palette,
) -> El {
    match cx.layout {
        MessageLayout::Rows => classic_row(cx, key, item, continues, latest_call, p),
        MessageLayout::Bubbles => bubble_row(cx, key, item, continues, latest_call, p),
    }
}

/// What a message shows, whatever its layout: `inside`, what it says (the quoted message it
/// answers, its content, a linked GIF, a link's card), and `below`, what hangs off it (reactions,
/// its thread, and in a timeline too narrow for the readers' column, its read indicator).
struct Parts {
    inside: Vec<El>,
    below: Vec<El>,
}

/// `said` paints what the message says (a bubble's shades, in a bubble), `p` what hangs off it.
fn message_parts(
    cx: &Context<'_>,
    item: &TimelineItem,
    latest_call: bool,
    card_width: f32,
    said: &Palette,
    p: &Palette,
) -> Parts {
    let message = &item.message;
    let interactive = !matches!(message.content, MessageContent::Call(_));
    let mut inside = Vec::new();
    if let Some(quote) = &message.reply_to {
        inside.push(quote_block(cx, quote, said));
    }
    // A GIF link shows as the GIF, below the rest of the message (if any) like an attachment.
    // Held back, it keeps its Show even when an earlier download failed: Show tries again.
    match item.gif.as_ref().filter(|gif| cx.show_gifs && (cx.hides(message) || !cx.gifs.has_failed(&gif.url))) {
        Some(gif) => {
            if let Some(rest) = &gif.rest {
                let caption = text_body(message, &rest.plain, rest.rich.as_ref(), cx, said);
                // At the right, the caption hugs its words, so they end where the GIF does.
                inside.push(if cx.on_right(message) {
                    col().self_end().max_w(cx.content_width).child(caption)
                } else {
                    caption
                });
            }
            inside.push(if cx.hides(message) {
                hidden_media(cx, &message.event_id, gif_frame(None, cx.content_width), said)
            } else {
                linked_gif(&gif.url, cx.content_width, cx.gifs, said)
            });
        }
        None => inside.push(message_body(item, cx, latest_call, said)),
    }
    // A link's card, below the text, once the homeserver described the page.
    if let Some((url, card)) =
        item.card_link(cx.show_gifs).and_then(|url| Some((url, cx.link_previews?.get(cx.account, url)?)))
    {
        let held = cx.hides(message).then(|| (cx.account.clone(), message.event_id.clone()));
        let key = cx.key(&format!("link-{}", message.event_id));
        inside.push(link_card(&key, url, card, (card_width, held), cx.media, said));
    }
    let mut below = Vec::new();
    if interactive && !message.reactions.is_empty() {
        below.push(reactions(cx, message, p));
    }
    if let (true, Some(thread), false) = (interactive, &message.thread, cx.in_thread()) {
        below.push(thread_link(&message.event_id, thread, cx.media, p));
    }
    // Too narrow a timeline for the readers' column: the indicator takes a line of its own.
    if !cx.in_thread() && !cx.readers_column && is_read(message) {
        let indicator = super::readers::indicator(cx.data, cx.key, message, cx.reader_faces, cx.media, p);
        below.push(row().w_full().justify_end().child(indicator));
    }
    Parts { inside, below }
}

/// Whether anyone's read indicator stands at the message.
fn is_read(message: &Message) -> bool {
    !message.pending && !message.read_receipts.is_empty()
}

/// "Edited" (opening the message's earlier versions) and "Sending…", when either applies.
fn status_line(cx: &Context<'_>, message: &Message, p: &Palette) -> Option<El> {
    let edited = message
        .edited
        .then(|| edit_history_opener(cx, message, text(t!("message.edited")).font(font::META).color(p.text_muted)));
    let sending = message.pending.then(|| text(t!("message.sending")).font(font::META).color(p.text_muted));
    (edited.is_some() || sending.is_some()).then(|| row().gap(6.0).center_y().children(edited).children(sending))
}

/// `el` opening the earlier versions of the edited `message` when pressed.
fn edit_history_opener(cx: &Context<'_>, message: &Message, el: El) -> El {
    let (room_id, event_id) = (cx.key.room_id.clone(), message.event_id.clone());
    el.key(cx.key(&format!("edited-{}", message.event_id)))
        .role("button")
        .label(t!("message.edit_history"))
        .help(t!("message.edit_history"))
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .on_press(move |s: &mut State| s.open_edit_history(&room_id, &event_id))
}

/// "Edited" and "Sending…", when either applies.
fn status_words(message: &Message) -> Option<String> {
    let mut status = Vec::new();
    if message.edited {
        status.push(t!("message.edited"));
    }
    if message.pending {
        status.push(t!("message.sending"));
    }
    (!status.is_empty()).then(|| status.join(" · "))
}

/// "Pinned", above a pinned message.
fn pinned_label(p: &Palette) -> El {
    row()
        .gap(5.0)
        .center_y()
        .child(icon(&icons::PIN, 12.0).color(p.accent))
        .child(text(t!("message.pinned")).font(font::META).color(p.accent))
}

/// The sender's picture with their presence dot, and on hover what they show in words; pressing
/// it opens their person panel.
fn sender_face(cx: &Context<'_>, key: &str, message: &Message, p: &Palette) -> El {
    let shown = cx.data.presence.get(&message.sender_id);
    let face = media_avatar(&message.sender_id, &message.sender_name, message.sender_avatar.as_ref(), cx.media, AVATAR);
    // Pressable out to the dot's overhang, which the layout doesn't count.
    let reach = AVATAR + PRESENCE_OVERHANG;
    let face = el()
        .key(format!("{key}-sender-face"))
        .size(reach, reach)
        .mr(-PRESENCE_OVERHANG)
        .mb(-PRESENCE_OVERHANG)
        .child(with_presence(face, AVATAR, shown, p.bg, p));
    let face = match shown.and_then(presence_text) {
        Some(words) => face.help(words),
        None => face,
    };
    opens_person(face, message)
}

/// The column at a room message's right for who has read it, filled or not and as tall as the
/// indicator, so a receipt arriving never rewraps or grows the message (threads track no
/// receipts).
fn readers_cell(cx: &Context<'_>, message: &Message, p: &Palette) -> Option<El> {
    if !cx.readers_column {
        return None;
    }
    let mut readers = row_el()
        .w(super::readers::column_width(cx.reader_faces))
        .min_h(super::readers::HEIGHT)
        .self_end()
        .justify_end();
    if is_read(message) {
        readers = readers.child(super::readers::indicator(cx.data, cx.key, message, cx.reader_faces, cx.media, p));
    }
    Some(readers)
}

/// What every message row does, whatever its layout: the hover group its toolbar shows with,
/// the menu on a right click, and in a room, opening the message's thread on a press. The hover
/// toolbar is the layout's to place.
fn message_actions(row: El, cx: &Context<'_>, message: &Message) -> El {
    let (menu_key, id) = (cx.key.clone(), message.event_id.clone());
    let row = row.hover_group();
    let mut row = row.on_secondary_press(move |s: &mut State, point, _| {
        s.popover = Some(Popover::MessageMenu {
            key: menu_key.clone(),
            event_id: id.clone(),
            at: Some((point.x as i32, point.y as i32)),
        });
    });
    if !cx.in_thread() {
        let root = message.event_id.clone();
        row = row.cursor(rune::Cursor::Pointer).on_press(move |s: &mut State| s.open_thread(&root));
    }
    row
}

/// A classic row: avatar and name (unless it continues the sender's run), then the message's
/// parts; the toolbar shows on hover. Clicking a room message opens its thread; clicking the
/// sender's name or picture, their person panel.
fn classic_row(
    cx: &Context<'_>,
    key: &str,
    item: &TimelineItem,
    continues: bool,
    latest_call: bool,
    p: &Palette,
) -> El {
    let message = &item.message;
    let interactive = !matches!(message.content, MessageContent::Call(_));
    let parts = message_parts(cx, item, latest_call, cx.content_width, p, p);
    let body = col().w_full().gap(4.0).children(parts.inside).children(parts.below);
    let status = status_line(cx, message, p);
    let content = if continues {
        let mut content = col().grow(1.0).min_w(0.0).child(body);
        if let Some(status) = status {
            content = content.child(status);
        }
        content
    } else {
        // A long name is cut short, keeping the time beside it within the message's width.
        let name = text(message.sender_name.clone())
            .key(format!("{key}-sender-name"))
            .font(font::BODY_STRONG)
            .color(p.text)
            .shrink()
            .truncate();
        // As wide as its words, and no wider than the content, so a long name is cut short.
        let mut meta = row()
            .key(format!("{key}-meta"))
            .self_start()
            .max_w(cx.content_width)
            .gap(8.0)
            .center_y()
            .min_w(0.0)
            .child(opens_person(name, message).focusable(true))
            .children(status_emoji(&message.event_id, cx.data.status_message_of(&message.sender_id)))
            .child(text(clock_time(local_ms(message.timestamp_ms))).font(font::META).color(p.text_muted));
        if let Some(status) = status {
            meta = meta.child(status);
        }
        col().grow(1.0).min_w(0.0).gap(2.0).child(meta).child(body)
    };
    let pinned = cx.room.pinned.contains(&message.event_id);
    let content =
        if pinned { col().grow(1.0).min_w(0.0).gap(3.0).child(pinned_label(p)).child(content) } else { content };
    let actions = interactive && !message.pending;
    // At the content's top right on every row, so it keeps its place from one row to the next.
    let content = if actions {
        content.child(toolbar(cx, key, message, ToolbarAt::TopEnd { said: cx.content_width }, p))
    } else {
        content
    };
    let gutter = if continues { el().w(AVATAR) } else { sender_face(cx, key, message, p) };
    let mut row = row()
        .key(key.to_owned())
        .w_full()
        .px(MESSAGE_PAD_X)
        .pt(if continues { 2.0 } else { 10.0 })
        .pb(2.0)
        .gap(MESSAGE_GAP)
        .items_start()
        .opacity(if message.pending { 0.6 } else { 1.0 })
        .child(gutter)
        .child(content)
        .children(readers_cell(cx, message, p));
    // The message jumped to lights up a moment; one mentioning the user keeps a warm mark.
    if cx.flash == Some(message.event_id.as_str()) {
        row = row.fill(p.accent.alpha(0.16)).border_sides([0.0, 0.0, 0.0, 3.0], p.accent);
    } else if pinned {
        row = row.fill(p.accent.alpha(0.06)).border_sides([0.0, 0.0, 0.0, 3.0], p.accent);
    } else if message.highlighted && message.sender_id != cx.account.as_str() {
        row = row.fill(p.warning.alpha(0.10)).border_sides([0.0, 0.0, 0.0, 3.0], p.warning);
    }
    // Under another message's toolbar, which holds the pointer, the row shows no hover.
    if interactive && !under_held_toolbar(cx, key, message) {
        row = row.hover(p.hover);
    }
    if actions { message_actions(row, cx, message) } else { row }
}

/// The most of the timeline's width a bubble takes, and the most it takes at all.
const BUBBLE_SHARE: f32 = 0.8;
const BUBBLE_MAX: f32 = 600.0;
/// A bubble's padding around what it says.
const BUBBLE_PAD_X: f32 = 12.0;
/// Narrower content than this is a compact timeline: bubbles take all of it, with less padding.
const BUBBLE_COMPACT: f32 = 320.0;
const BUBBLE_PAD_X_COMPACT: f32 = 8.0;
const BUBBLE_PAD_Y: f32 = 7.0;

/// Whether a bubble names `sender` in `room`: everyone but the other person of a one-to-one
/// direct chat, where who wrote what is plain. Once someone else is there, or for anyone who was
/// and left, names show.
pub(super) fn names_sender(room: &RoomSummary, sender: &str) -> bool {
    let one_to_one = room.kind == matrix_core::RoomKind::Direct && room.member_count <= 2;
    !(one_to_one && room.dm_users.iter().any(|partner| partner == sender))
}

/// Whether a message's content reads as words, which a bubble holds; pictures, files, clips,
/// polls, call cards and GIFs bring their own frames and stand without one.
fn in_bubble(item: &TimelineItem, cx: &Context<'_>) -> bool {
    let gif = item.gif.as_ref().is_some_and(|gif| cx.show_gifs && !cx.gifs.has_failed(&gif.url));
    !gif && matches!(
        item.message.content,
        MessageContent::Text { .. } | MessageContent::Redacted | MessageContent::Undecryptable
    )
}

/// A chat bubble: other people's messages at the left beside their picture (on the first of a
/// run), the user's own at the right without one. A bubble holds the sender's name (others', on
/// the first of a run, outside direct chats), what the message says, and its time; reactions and
/// the thread hang below it. Pressing it does what a classic row's press does.
fn bubble_row(cx: &Context<'_>, key: &str, item: &TimelineItem, continues: bool, latest_call: bool, p: &Palette) -> El {
    let message = &item.message;
    let own = cx.on_right(message);
    let interactive = !matches!(message.content, MessageContent::Call(_));
    let compact = cx.content_width < BUBBLE_COMPACT;
    let (share, pad_x) = if compact { (1.0, BUBBLE_PAD_X_COMPACT) } else { (BUBBLE_SHARE, BUBBLE_PAD_X) };
    let width = (cx.content_width * share).min(BUBBLE_MAX);
    let bubbled = in_bubble(item, cx);
    // Muted words inside a bubble (a quote, a notice) take the shade that stays clear on it.
    let inside = if bubbled { Palette { text_muted: p.bubble_meta, ..*p } } else { *p };
    // A card in a bubble fits the bubble; one standing alone, the classic row's width.
    let card_width = if bubbled { width - 2.0 * pad_x } else { cx.content_width };
    let mut parts = message_parts(cx, item, latest_call, card_width, &inside, p);
    let actions = interactive && !message.pending;
    // The toolbar sits on the top edge of what the message says: a bubble as wide as it was
    // laid out last frame (the widest it can be before then), or the column media and cards
    // stand in.
    let said_width = if bubbled {
        rune::memo::layout_with(&format!("{key}-bubble"), |bubble| bubble[2]).map_or(width, |laid| laid.min(width))
    } else {
        cx.content_width
    };
    if own && !bubbled {
        // A GIF's caption keeps to its side, as the GIF does.
        parts.inside = parts.inside.into_iter().map(El::self_end).collect();
    }

    let named = !own && !continues && names_sender(cx.room, &message.sender_id);
    let name = named.then(|| {
        let name = text(message.sender_name.clone())
            .key(format!("{key}-sender-name"))
            .font(font::SECTION)
            .color(super::theme::identity_color(&message.sender_id))
            .shrink()
            .truncate();
        row()
            .gap(6.0)
            .center_y()
            .min_w(0.0)
            .child(opens_person(name, message).focusable(true))
            .children(status_emoji(&message.event_id, cx.data.status_message_of(&message.sender_id)))
    });
    let mut time = clock_time(local_ms(message.timestamp_ms));
    if let Some(status) = status_words(message) {
        time = format!("{status} · {time}");
    }
    let time = text(time).key(format!("{key}-time")).font(font::META);
    // An edited message's time says so, and opens its earlier versions.
    let time = if message.edited { edit_history_opener(cx, message, time) } else { time };

    let said = col().key(format!("{key}-bubble")).gap(4.0);
    let said = if bubbled {
        said.max_w(width)
            .px(pad_x)
            .py(BUBBLE_PAD_Y)
            .radius(16.0)
            .fill(if own { p.bubble_own } else { p.bubble })
            .children(name)
            .children(parts.inside)
            .child(time.color(p.bubble_meta).self_end())
    } else {
        // Cards and media take the width a classic row gives them, which they are sized for: a
        // poll fills it, a picture keeps its size on the message's side, the time under it.
        // No wider than a classic row's content, though the user's own leave out the picture.
        let said = said.w_full().max_w(cx.content_width);
        let said = if own { said.items_end() } else { said.items_start() };
        said.children(name).children(parts.inside).child(time.color(p.text_muted))
    };
    let said = if actions {
        let at = if own && bubbled {
            ToolbarAt::OwnBubble { said: said_width }
        } else {
            ToolbarAt::TopEnd { said: said_width }
        };
        said.child(toolbar(cx, key, message, at, p))
    } else {
        said
    };

    let mut column = col().grow(1.0).min_w(0.0).gap(4.0);
    column = if own { column.items_end() } else { column.items_start() };
    if cx.room.pinned.contains(&message.event_id) {
        column = column.child(pinned_label(p));
    }
    // What hangs below keeps within the message's width, as a classic row's body holds it, so a
    // thread link's last-reply text is cut short rather than run past the row.
    let below = parts.below.into_iter().map(|part| part.max_w(cx.content_width));
    let column = column.child(said).children(below);

    let mut row = row()
        .key(key.to_owned())
        .w_full()
        .px(MESSAGE_PAD_X)
        .pt(if continues { 2.0 } else { 10.0 })
        .pb(2.0)
        .gap(MESSAGE_GAP)
        .items_start()
        .opacity(if message.pending { 0.6 } else { 1.0 });
    if !own {
        // Without the name, the picture is the keyboard's way to the person.
        row = row.child(if continues { el().w(AVATAR) } else { sender_face(cx, key, message, p).focusable(!named) });
    }
    let mut row = row.child(column).children(readers_cell(cx, message, p));
    // The message jumped to lights up a moment; one mentioning the user keeps a warm mark.
    if cx.flash == Some(message.event_id.as_str()) {
        row = row.fill(p.accent.alpha(0.16));
    } else if message.highlighted && message.sender_id != cx.account.as_str() {
        row = row.fill(p.warning.alpha(0.10)).border_sides([0.0, 0.0, 0.0, 3.0], p.warning);
    }
    if actions { message_actions(row, cx, message) } else { row }
}

/// The sender's name or picture on a message, opening their person panel rather than the thread.
/// The name is the keyboard's stop for it; the picture is the same action for the pointer.
fn opens_person(sender: El, message: &Message) -> El {
    let person = matrix_core::Person {
        id: message.sender_id.clone(),
        name: message.sender_name.clone(),
        avatar: message.sender_avatar.clone(),
    };
    sender
        .role("button")
        .label(message.sender_name.clone())
        .cursor(rune::Cursor::Pointer)
        .on_press(move |s: &mut State| s.show_side(crate::state::SidePanel::Person(person.clone())))
}

/// A toolbar button's side, the space between buttons, and the toolbar's padding around them.
const TOOL: f32 = 30.0;
const TOOL_GAP: f32 = 2.0;
const TOOLBAR_PAD: f32 = 3.0;
/// The rule between the message's own actions and More, and the room beside it.
const TOOLBAR_RULE: f32 = 1.0;
const TOOLBAR_RULE_MARGIN: f32 = 3.0;
const TOOLBAR_HEIGHT: f32 = TOOL + 2.0 * TOOLBAR_PAD;
/// How far the toolbar reaches down over the top edge of what its message says.
pub(super) const TOOLBAR_OVERLAP: f32 = 10.0;
/// The toolbar's distance from the end of what its message says.
pub(super) const TOOLBAR_INSET: f32 = 8.0;
/// The gap between the user's own bubble and the toolbar beside it.
pub(super) const TOOLBAR_BESIDE: f32 = 6.0;

/// The width of a toolbar of `buttons`, More among them, behind a rule when it isn't alone.
pub(super) fn toolbar_width(buttons: usize) -> f32 {
    let n = buttons as f32;
    let buttons = 2.0 * TOOLBAR_PAD + n * TOOL + (n - 1.0).max(0.0) * TOOL_GAP;
    // The rule is one more child, so one more gap.
    if n > 1.0 { buttons + TOOL_GAP + TOOLBAR_RULE + 2.0 * TOOLBAR_RULE_MARGIN } else { buttons }
}

/// Where a message's toolbar sits, against what the message says.
enum ToolbarAt {
    /// On the top edge of what the message says (`said` wide: a bubble, a classic row's
    /// content, or the column media and cards stand in), at its top right, as Teams places its
    /// own.
    TopEnd { said: f32 },
    /// Beside the user's own bubble (`said` wide), on its left and level with its top, where the
    /// timeline has room; else over its top left corner.
    OwnBubble { said: f32 },
}

/// The toolbar's `(left, top, right)` pins inside what its message says, `width` wide, for `at`.
/// On something narrower than the toolbar it starts at the message's own edge and reaches into
/// the open side: the right for other people's, the left for the user's own at the right.
fn toolbar_pins(cx: &Context<'_>, message: &Message, at: ToolbarAt, width: f32) -> (Option<f32>, f32, Option<f32>) {
    let top = TOOLBAR_OVERLAP - TOOLBAR_HEIGHT;
    let fits = |said: f32| said >= width + 2.0 * TOOLBAR_INSET;
    let narrow = if cx.on_right(message) { (None, top, Some(0.0)) } else { (Some(0.0), top, None) };
    match at {
        ToolbarAt::TopEnd { said } if fits(said) => (None, top, Some(TOOLBAR_INSET)),
        // The user's own rows leave out the picture, so their bubbles have its room as well.
        ToolbarAt::OwnBubble { said } if cx.content_width + AVATAR + MESSAGE_GAP - said >= width + TOOLBAR_BESIDE => {
            (Some(-(width + TOOLBAR_BESIDE)), 0.0, None)
        }
        ToolbarAt::OwnBubble { said } if fits(said) => (Some(TOOLBAR_INSET), top, None),
        ToolbarAt::TopEnd { .. } | ToolbarAt::OwnBubble { .. } => narrow,
    }
}

/// Whether `message` is the user's own text and can be edited (`State::start_edit`): its toolbar
/// and its menu (`message_actions::menu`) offer Edit.
pub(super) fn editable(message: &Message, own: &str) -> bool {
    message.editable && message.sender_id == own && matches!(message.content, MessageContent::Text { .. })
}

/// React, Reply, Thread, Edit on the user's own messages, then More behind a rule, while the
/// message is hovered (or its reaction picker or menu is open), placed `at` what the message
/// says. A timeline too narrow for the whole toolbar keeps only More, whose menu offers the
/// rest.
fn toolbar(cx: &Context<'_>, key: &str, message: &Message, at: ToolbarAt, p: &Palette) -> El {
    let event_id = message.event_id.clone();
    // A call's card keeps no reactions, and a thread has no threads.
    let (react, thread) = (!matches!(message.content, MessageContent::Call(_)), !cx.in_thread());
    let edit = editable(message, cx.account.as_str());
    let roomy = toolbar_width(2 + usize::from(react) + usize::from(thread) + usize::from(edit)) <= cx.content_width;
    let (react, reply, thread, edit) = (roomy && react, roomy, roomy && thread, roomy && edit);
    let buttons = 1 + usize::from(react) + usize::from(reply) + usize::from(thread) + usize::from(edit);
    let width = toolbar_width(buttons);
    let (left, top, right) = toolbar_pins(cx, message, at, width);
    let open = cx.picking == Some(event_id.as_str());
    // The pointer on this toolbar, above its row, keeps it shown; on another's, hides it.
    let at = cx.at(message);
    let (held, covered) = (cx.hover.holds(&at), under_held_toolbar(cx, key, message) && !open);
    let target = if cx.in_thread() { ComposerTarget::Thread } else { ComposerTarget::Room };
    let button = |glyph: &'static rune::Icon, label: &str, name: &str| {
        el().key(cx.key(&format!("{name}-{event_id}")))
            .size(TOOL, TOOL)
            .radius(6.0)
            .center()
            .hover(p.hover.alpha(p.hover.a * 3.0))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(label.to_owned())
            .help(label.to_owned())
            .focusable(true)
            .child(icon(glyph, 18.0).color(p.text_muted))
    };
    let (key, quoted) = (cx.key.clone(), message.clone());
    let bar = row()
        .key(cx.key(&format!("actions-{event_id}")))
        .w(width)
        .h(TOOLBAR_HEIGHT)
        .px(TOOLBAR_PAD)
        .gap(TOOL_GAP)
        .center_y()
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .shadow(2.0, 10.0, Color::BLACK.alpha(0.16))
        .group_opacity(if open || held { 1.0 } else { 0.0 }, if covered { 0.0 } else { 1.0 })
        // Held or let go, it shows or hides with the pointer, not on a fade of its own.
        .instant_changes()
        // Rune hit-tests only what draws: a hidden toolbar never hears the pointer, so it
        // holds nothing and takes no press, and one shown under the pointer hears it arrive. A
        // press between its buttons is its own, not the message's under it, and shows no hand.
        .on_hover_change(move |s: &mut State, inside, _| s.message_hover.toolbar(&at, inside))
        .on_press(|_: &mut State| {})
        .cursor(rune::Cursor::Default);
    // Under another message's toolbar, its own is out of reach: Tab doesn't stop in it unseen.
    let mut bar = if covered { bar.inert() } else { bar };
    if react {
        bar = bar.child(button(&icons::SMILE, t!("actions.react"), "react").on_press(move |s: &mut State| {
            s.toggle_popover(Popover::Emoji(EmojiFor::Reaction {
                key: key.clone(),
                event_id: quoted.event_id.clone(),
            }));
        }));
    }
    if reply {
        let reply = message.clone();
        bar = bar.child(
            button(&icons::REPLY, t!("actions.reply"), "reply")
                .on_press_tx(move |s: &mut State, tx| s.start_reply(target, &reply, tx)),
        );
    }
    if thread {
        let root = event_id.clone();
        bar = bar.child(
            button(&icons::THREAD, t!("actions.thread"), "thread").on_press(move |s: &mut State| s.open_thread(&root)),
        );
    }
    if edit {
        let edited = message.clone();
        bar = bar.child(
            button(&icons::PENCIL, t!("actions.edit"), "edit")
                .on_press_tx(move |s: &mut State, tx| s.start_edit(target, &edited, tx)),
        );
    }
    if buttons > 1 {
        bar = bar.child(el().w(TOOLBAR_RULE).h(TOOL * 0.6).mx(TOOLBAR_RULE_MARGIN).fill(p.divider));
    }
    // Every sent message has utilities, including other people's messages.
    let (key, id) = (cx.key.clone(), event_id.clone());
    let bar = bar.child(button(&icons::MORE, t!("actions.more"), "more").on_press(move |s: &mut State| {
        s.toggle_popover(Popover::MessageMenu { key: key.clone(), event_id: id.clone(), at: None });
    }));
    el().pin(left, Some(top), right, None).child(bar)
}

/// Whether the row `key` of `message` lies under another message's toolbar that holds the
/// pointer, as laid out last frame: the row shows no hover, and its own toolbar hides, out of
/// reach. Rows elsewhere keep theirs, keyboard focus in them included.
fn under_held_toolbar(cx: &Context<'_>, key: &str, message: &Message) -> bool {
    let Some(held) = cx.hover.other_held(&cx.at(message)) else { return false };
    let bar = rune::memo::layout(&cx.key(&format!("actions-{held}")));
    let (Some(bar), Some(row)) = (bar, rune::memo::layout(key)) else { return false };
    bar[0] < row[0] + row[2] && row[0] < bar[0] + bar[2] && bar[1] < row[1] + row[3] && row[1] < bar[1] + bar[3]
}

/// The message a reply answers: who wrote it and an excerpt, behind a bar. Pressing it goes to
/// that message.
fn quote_block(cx: &Context<'_>, quote: &Quote, p: &Palette) -> El {
    let sender = quote.sender_name.clone().unwrap_or_default();
    let body = quote_text(quote);
    let (account, room_id, event_id) = (cx.account.clone(), cx.key.room_id.clone(), quote.event_id.clone());
    row()
        .key(cx.key(&format!("quote-{}", quote.event_id)))
        .max_w(560.0)
        .gap(8.0)
        .radius(4.0)
        .hover(p.hover)
        .role("button")
        .label(tf!("quote.jump", name = sender))
        .cursor(rune::Cursor::Pointer)
        .on_press(move |s: &mut State| s.jump_to_message(&account, &room_id, &event_id))
        .child(el().w(3.0).h_full().min_h(32.0).radius(2.0).fill(p.text_muted.alpha(0.5)))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .py(2.0)
                .child(text(sender).font(font::META.family(rune::Family::Sans)).color(p.text).truncate())
                .child(text(body).font(font::META).color(p.text_muted).max_lines(2).truncate()),
        )
}

/// What a quote says of the answered message: its text, the kind of call it was, or that it
/// isn't loaded.
fn quote_text(quote: &Quote) -> String {
    match (&quote.body, &quote.call) {
        (Some(body), _) => body.clone(),
        (None, Some(card)) => crate::state::call_title(card).to_owned(),
        (None, None) => t!("quote.unavailable").to_owned(),
    }
}

/// Reaction chips: emoji and count, outlined when the user's own; each toggles the user's.
fn reactions(cx: &Context<'_>, message: &Message, p: &Palette) -> El {
    let mut chips = row().gap(6.0).flex_wrap().pt(2.0);
    // Each wrapped line of chips keeps to the message's side.
    if cx.on_right(message) {
        chips = chips.justify_end();
    }
    for reaction in &message.reactions {
        let (key, event_id, emoji) = (cx.key.clone(), message.event_id.clone(), reaction.key.clone());
        chips = chips.child(
            row()
                .key(cx.key(&format!("reaction-{}-{}", message.event_id, reaction.key)))
                .max_w(cx.content_width)
                .h(26.0)
                .px(8.0)
                .gap(6.0)
                .center_y()
                .radius(13.0)
                .fill(if reaction.mine { p.accent.alpha(0.15) } else { p.surface })
                .border(1.0, if reaction.mine { p.accent } else { p.divider })
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(format!("{} {}", reaction.key, reaction.count))
                .checked(reaction.mine)
                // A long reaction (a shortcode, say) is cut short; its label keeps all of it.
                .child(text(reaction.key.clone()).font(font::BODY).shrink().truncate())
                .child(text(reaction.count.to_string()).font(font::META).color(if reaction.mine {
                    p.accent
                } else {
                    p.text_muted
                }))
                .on_press(move |s: &mut State| s.react(key.clone(), &event_id, &emoji)),
        );
    }
    chips
}

/// The latest replier's picture, "3 replies" and "Last reply today at 09:51", opening the
/// thread.
fn thread_link(root: &str, thread: &ThreadSummary, media: &MediaCache, p: &Palette) -> El {
    let root = root.to_owned();
    let mut link = row()
        .key(format!("thread-link-{root}"))
        .mt(2.0)
        .px(4.0)
        .py(3.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer);
    link = match &thread.latest {
        Some(person) => link.child(media_avatar(&person.id, &person.name, person.avatar.as_ref(), media, 22.0)),
        None => link.child(icon(&icons::THREAD, 18.0).color(p.accent)),
    };
    link = link.child(text(tp!("thread.replies", thread.replies)).font(font::BODY_STRONG).color(p.accent));
    if let Some(at) = thread.latest_at {
        const DAY: u64 = 86_400_000;
        let (at, now) = (local_ms(at), local_ms(now_ms()));
        let (day, today, time) = (at / DAY, now / DAY, clock_time(at));
        let when = if day == today {
            tf!("thread.last_reply_today", time = time)
        } else if day + 1 == today {
            tf!("thread.last_reply_yesterday", time = time)
        } else {
            tf!("thread.last_reply", date = date_label(at, now), time = time)
        };
        // Cut short in a narrow message rather than reach into the readers' column.
        link = link.child(text(when).font(font::META).color(p.text_muted).shrink().truncate());
    }
    link.on_press(move |s: &mut State| s.open_thread(&root))
}

/// A linked GIF's height, loading or not, and the most of the width it takes.
pub(super) const GIF_HEIGHT: f32 = 160.0;
const GIF_MAX_WIDTH: f32 = 360.0;

/// The frame of a GIF of `size` pixels (4:3 while unknown), no wider than `max_width`: always
/// [`GIF_HEIGHT`] tall, so the message keeps its height as the GIF arrives, and as wide as the
/// GIF's shape makes it.
fn gif_frame(size: Option<(u32, u32)>, max_width: f32) -> (f32, f32) {
    let shape = size.map_or(4.0 / 3.0, |(w, h)| w as f32 / h as f32);
    ((GIF_HEIGHT * shape).min(GIF_MAX_WIDTH).clamp(0.0, max_width.max(0.0)), GIF_HEIGHT)
}

/// A GIF behind a link in the message, playing once it has loaded, up to `max_width` wide; until
/// then a placeholder of the same height. Its frame lands at once when the GIF arrives, rather
/// than fading the picture in over the placeholder.
fn linked_gif(url: &str, max_width: f32, gifs: &GifCache, p: &Palette) -> El {
    let player = gifs.player(url);
    let (w, h) = gif_frame(player.and_then(|player| player.size()), max_width);
    // The placeholder's ground stays under the picture: until the worker decodes its first
    // frame, the frame shows it rather than nothing.
    let frame = el()
        .key(format!("gif-{url}"))
        .size(w, h)
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .role("img")
        .label(url.to_owned())
        .instant_changes();
    match player {
        // Too wide a GIF for the room keeps its shape, centred in the frame.
        Some(player) => frame.pixels_fit(player.pixels(), rune::SurfaceFit::Fit),
        None => frame.center().child(text("GIF").font(font::SECTION).color(p.text_muted)),
    }
}

fn message_body(item: &TimelineItem, cx: &Context<'_>, latest_call: bool, p: &Palette) -> El {
    let media = cx.media;
    match (&item.message.content, &item.rich) {
        (MessageContent::Text { body, .. }, rich) => text_body(&item.message, body, rich.as_ref(), cx, p),
        (MessageContent::Emote { body, .. }, rich) => emote_body(item, body, rich.as_ref(), cx, p),
        (MessageContent::Location(location), _) => location_card(&item.message.event_id, location, cx, p),
        (MessageContent::Attachment(attachment), _)
            if attachment.kind == AttachmentKind::Audio && audio_supported() =>
        {
            audio_card(&item.message, attachment, cx, p)
        }
        (MessageContent::Attachment(attachment), _) => {
            // Held back or not is decided first: a picture or video with no preview to draw
            // falls back to its file card, which then offers Show too.
            let held = crate::state::visual(attachment) && cx.hides(&item.message);
            match attachment.preview() {
                Some((preview, _)) if !media.has_failed(&preview) => {
                    image_preview(&item.message.event_id, attachment, cx, held, p)
                }
                _ => attachment_card(&item.message.event_id, attachment, cx, held, p),
            }
        }
        (MessageContent::Redacted, _) => notice_text(t!("message.deleted"), p),
        (MessageContent::Undecryptable, _) => notice_text(t!("message.undecryptable"), p),
        (MessageContent::Poll(poll), _) => {
            let own = item.message.sender_id == cx.account.as_str();
            super::poll::card(poll, &item.message.event_id, own, cx.key, p)
        }
        (MessageContent::Call(card), _) => call_card(cx, &item.message.event_id, card, latest_call, p),
    }
}

/// The least width a call card's title and status keep beside its picture.
const CALL_WORDS_MIN: f32 = 100.0;

/// A call's card: what kind it is and how it stands. The newest card in the room shows whether
/// the room's call is under way, with the way in; older ones have ended.
fn call_card(cx: &Context<'_>, event_id: &str, card: &CallCard, latest_call: bool, p: &Palette) -> El {
    let people = &cx.room.call.participants;
    // Only the newest card speaks for the call now, as matrix-sdk-ui has it: a one-to-one call by
    // this app's own one-to-one call and rings, MatrixRTC's by the room's members.
    let ongoing = latest_call
        && if card.direct { cx.call_here == Some(matrix_core::CallVia::Direct) } else { !people.is_empty() };
    let declined = !card.declined_by.is_empty() && cx.room.kind == matrix_core::RoomKind::Direct;
    let status = match () {
        () if ongoing && card.direct => t!("call.card.ongoing").to_owned(),
        () if ongoing => format!("{} · {}", t!("call.card.ongoing"), tf!("call.people", count = people.len())),
        () if latest_call && card.direct && cx.ringing => t!("call.card.ringing").to_owned(),
        () if declined => t!("call.card.declined").to_owned(),
        () => t!("call.card.ended").to_owned(),
    };
    let glyph = if card.video == Some(true) { &icons::VIDEO } else { &icons::CALL };
    let tile = el()
        .size(40.0, 40.0)
        .radius(10.0)
        .center()
        .fill(if ongoing { p.success } else { p.hover })
        .child(icon(glyph, 20.0).color(if ongoing { p.on_accent } else { p.text_muted }));
    let words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(crate::state::call_title(card)).font(font::BODY_STRONG).color(p.text).truncate())
        .child(text(status).font(font::META).color(p.text_muted).truncate());
    // The picture and words keep a readable width, or all the card has; in a narrow message,
    // Join takes a line under them.
    const PADDING: f32 = 12.0;
    let room = cx.content_width - 2.0 * (PADDING + 1.0);
    let head =
        row().grow(1.0).min_w((40.0 + 12.0 + CALL_WORDS_MIN).min(room)).gap(12.0).center_y().child(tile).child(words);
    let mut entry = row()
        .key(cx.key(&format!("call-card-{event_id}")))
        .max_w(460.0)
        .mt(2.0)
        .padding(PADDING)
        .gap(12.0)
        .flex_wrap()
        .center_y()
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, if ongoing { p.success.alpha(0.6) } else { p.divider })
        .child(head);
    // Join as the header offers it: where the room allows calls, and not when already in it here.
    if cx.call_here.is_some() && ongoing {
        entry = entry.child(text(t!("call.card.joined")).font(font::META).color(p.text_muted));
    } else if ongoing && !card.direct && cx.room.call.via.is_some() {
        let video = card.video == Some(true);
        entry = entry.child(
            super::call::pill(&icons::CALL, t!("call.join"), p.success, p)
                .key(cx.key(&format!("call-card-join-{event_id}")))
                .on_press(move |s: &mut State| s.start_call(video)),
        );
    }
    entry
}

/// Message text, formatted when the sender formatted it. It selects, so it shows the text cursor
/// even inside a row that opens its thread.
fn text_body(message: &Message, plain: &str, rich: Option<&RichText>, cx: &Context<'_>, p: &Palette) -> El {
    let event_id = &message.event_id;
    match rich {
        Some(rich) => super::message_text::formatted(&format!("rich-{event_id}"), rich, cx.look(message), p),
        // Its own handler keeps the clicks, and those just beside it: a double-click takes a
        // word, a triple-click the paragraph, rather than opening the thread.
        None => super::message_text::reach(
            text(plain.to_owned())
                .key(format!("text-{event_id}"))
                .font(font::BODY)
                .color(p.text)
                .self_start()
                // A word longer than the line (a hash, say) breaks rather than runs out of the
                // message.
                .wrap_anywhere()
                .selectable(true)
                .cursor(rune::Cursor::Text)
                .on_press(|_: &mut State| {}),
        ),
    }
}

/// An emote: the sender's name, then what they do, in italics ("* Ada waves").
fn emote_body(item: &TimelineItem, body: &str, rich: Option<&RichText>, cx: &Context<'_>, p: &Palette) -> El {
    let event_id = &item.message.event_id;
    let name = &item.message.sender_name;
    match rich {
        Some(rich) => row()
            .gap(4.0)
            .items_start()
            .child(text(format!("* {name}")).font(font::BODY.italic()).color(p.text_muted))
            .child(
                super::message_text::formatted(&format!("rich-{event_id}"), rich, cx.look(&item.message), p).shrink(),
            ),
        None => super::message_text::reach(
            text(format!("* {name} {body}"))
                .key(format!("text-{event_id}"))
                .font(font::BODY.italic())
                .color(p.text)
                .self_start()
                .wrap_anywhere()
                .selectable(true)
                .cursor(rune::Cursor::Text)
                .on_press(|_: &mut State| {}),
        ),
    }
}

/// A shared place: its map from OpenStreetMap with a pin, what the sender called it and its
/// coordinates, the map's credit, and a way to open it on openstreetmap.org.
fn location_card(event_id: &str, location: &matrix_core::Location, cx: &Context<'_>, p: &Palette) -> El {
    let (w, h) = (crate::state::MAP_PIXELS.0 as f32, crate::state::MAP_PIXELS.1 as f32);
    let width = w.min(cx.content_width.max(120.0) - 2.0).min(360.0);
    let height = (width * h / w).round();
    let image = location.map.as_ref().filter(|_| cx.show_maps).and_then(|map| cx.media.image(map));
    let map = el()
        .key(cx.key(&format!("map-{event_id}")))
        .size(width, height)
        .fill(p.surface)
        .role("img")
        .label(t!("location.map"));
    let map = match image {
        Some(image) => map.pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => map.center().child(icon(&icons::MAP_PIN, 28.0).color(p.text_muted)),
    };
    let coordinates = matrix_core::geo::coordinates(&location.geo_uri)
        .map(|(latitude, longitude)| format!("{latitude:.5}, {longitude:.5}"))
        .unwrap_or_else(|| location.geo_uri.clone());
    let title = location.description.clone().unwrap_or_else(|| t!("location.shared").to_owned());
    let link = matrix_core::geo::web_link(&location.geo_uri);
    let mut words = col()
        .w_full()
        .px(12.0)
        .py(8.0)
        .gap(2.0)
        .child(text(title).font(font::BODY_STRONG).color(p.text).wrap())
        .child(text(coordinates).font(font::META).color(p.text_muted).selectable(true));
    let credit = text(t!("location.credit")).font(font::META).color(p.text_muted).shrink().truncate();
    let mut footer = row().w_full().gap(8.0).center_y().child(credit).child(el().grow(1.0));
    if let Some(link) = link.clone() {
        footer = footer.child(
            text(t!("location.open"))
                .key(cx.key(&format!("map-open-{event_id}")))
                .font(font::META)
                .color(p.accent)
                .role("link")
                .cursor(rune::Cursor::Pointer)
                .on_press_tx(move |s: &mut State, tx| s.open_link(&link, tx)),
        );
    }
    words = words.child(footer.pt(4.0));
    let mut card = col()
        .key(cx.key(&format!("location-{event_id}")))
        .w(width + 2.0)
        .mt(4.0)
        .radius(10.0)
        .clip_rounded(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .child(map)
        .child(words);
    if let Some(link) = link {
        card = card.cursor(rune::Cursor::Pointer).on_press_tx(move |s: &mut State, tx| s.open_link(&link, tx));
    }
    card
}

/// A message's stand-in when its content can't be shown.
fn notice_text(message: &str, p: &Palette) -> El {
    text(message.to_owned()).font(font::BODY.italic()).color(p.text_muted).w_full().wrap()
}

/// The largest box an image preview may take.
const PREVIEW_BOX: (f32, f32) = (360.0, 280.0);

/// A link's preview card, up to `max_width` wide: the site, the page's title and description,
/// and its image beside them when that leaves the words room.
/// Pressing it opens the link.
/// `fit`: the widest it may be, and the account and message whose held-back image waits for a
/// press, if it does.
fn link_card(
    key: &str,
    url: &str,
    card: &LinkPreview,
    fit: (f32, Option<(AccountId, String)>),
    media: &MediaCache,
    p: &Palette,
) -> El {
    let (max_width, held) = fit;
    /// The image's side.
    const THUMB: f32 = 72.0;
    let mut words = col().grow(1.0).min_w(0.0).gap(2.0);
    if let Some(site) = &card.site {
        words = words.child(text(site.clone()).font(font::META).color(p.text_muted).truncate());
    }
    if let Some(title) = &card.title {
        words = words.child(text(title.clone()).font(font::BODY_STRONG).color(p.accent).max_lines(2).wrap_anywhere());
    }
    if let Some(description) = &card.description {
        words = words.child(text(description.clone()).font(font::META).color(p.text).max_lines(3).wrap_anywhere());
    }
    // An image that failed to load leaves the words on their own. Beside them while that leaves
    // the words a readable width; in a narrower message, under them.
    const WORDS_MIN: f32 = 120.0;
    let room = max_width - 2.0 * 12.0 - 3.0 - 2.0 * 12.0 - THUMB;
    // Held back, it keeps its Show even when an earlier download failed: Show tries again.
    let thumb =
        card.image.as_ref().filter(|(image, _)| held.is_some() || !media.has_failed(image)).map(|(image, _)| {
            let thumb = el().size(THUMB, THUMB).radius(8.0).clip_rounded(8.0);
            match (held, media.image(image)) {
                // Held back: a press on it shows it, rather than opening the link.
                (Some((account, event_id)), _) => thumb
                    .key(format!("{key}-hidden"))
                    .fill(p.hover)
                    .center()
                    .role("button")
                    .label(t!("safety.show_media").to_owned())
                    .help(t!("safety.show_media").to_owned())
                    .focusable(true)
                    .cursor(rune::Cursor::Pointer)
                    .child(
                        col()
                            .gap(4.0)
                            .items_center()
                            .child(icon(&icons::EYE_OFF, 18.0).color(p.text_muted))
                            .child(text(t!("safety.show_media")).font(font::META).color(p.accent)),
                    )
                    .on_press({
                        let image = image.clone();
                        move |s: &mut State| {
                            s.media.try_again(&image);
                            s.reveal_media(&account, &event_id);
                        }
                    }),
                (None, Some(image)) => thumb.pixels_fit(image.pixels(), SurfaceFit::Fill),
                (None, None) => thumb.fill(p.hover).center().child(icon(&icons::IMAGE, 20.0).color(p.text_muted)),
            }
        });
    let (beside, under) = match thumb {
        Some(thumb) if room >= WORDS_MIN => (Some(thumb), None),
        thumb => (None, thumb),
    };
    if let Some(thumb) = under {
        words = words.child(thumb.mt(6.0));
    }
    let label = card.title.clone().unwrap_or_else(|| url.to_owned());
    let opened = url.to_owned();
    let entry = row()
        .key(key.to_owned())
        .max_w(460.0)
        .mt(4.0)
        .padding(12.0)
        .gap(12.0)
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("link")
        .label(label)
        .help(url.to_owned())
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .child(el().w(3.0).h_full().min_h(THUMB.min(40.0)).radius(1.5).fill(p.accent))
        .child(words)
        .children(beside);
    entry.on_press_tx(move |s: &mut State, tx| s.open_link(&opened, tx))
}

/// An image or a video's poster, scaled to fit [`PREVIEW_BOX`] and the message's width by the
/// sender's dimensions so the timeline doesn't jump when it loads; a placeholder of the same size
/// until then. Without the sender's dimensions, or with ones the picture doesn't match, the loaded
/// picture's own decide. A video's poster carries a play button.
fn image_preview(event_id: &str, attachment: &Attachment, cx: &Context<'_>, held: bool, p: &Palette) -> El {
    let max_width = cx.content_width;
    let image = attachment.preview().and_then(|(media_ref, _)| cx.media.image(&media_ref)).filter(|_| !held);
    let loaded = image.and_then(|image| image.size()).map(|[w, h]| (w, h));
    let (w, h) = preview_size(shown_dimensions(attachment.dimensions, loaded));
    let scale = (max_width / w).clamp(0.0, 1.0);
    let (w, h) = (w * scale, h * scale);
    if held {
        return hidden_media(cx, event_id, (w, h), p).mt(4.0);
    }
    let (opened, key, id) = (attachment.clone(), cx.key.clone(), event_id.to_owned());
    let frame = el()
        .key(format!("image-{}", attachment.name))
        .size(w, h)
        .mt(4.0)
        .radius(8.0)
        .role("img")
        .label(attachment.name.clone())
        .cursor(rune::Cursor::Pointer)
        .on_press(move |s: &mut State| s.open_attachment(&key, &id, &opened));
    let video = attachment.kind == AttachmentKind::Video;
    let frame = match image {
        Some(image) => frame.pixels_fit(image.pixels(), SurfaceFit::Fit).center(),
        // A video's placeholder is its play button alone.
        None if video => frame.fill(p.surface).border(1.0, p.divider).center(),
        None => {
            frame.fill(p.surface).border(1.0, p.divider).center().child(icon(&icons::IMAGE, 24.0).color(p.text_muted))
        }
    };
    if video { frame.child(play_badge()) } else { frame }
}

/// Media the account's choice holds back (ADR 0011), in its place and at its size: nothing is
/// fetched until Show.
fn hidden_media(cx: &Context<'_>, event_id: &str, (w, h): (f32, f32), p: &Palette) -> El {
    let (account, id) = (cx.account.clone(), event_id.to_owned());
    let show = button(t!("safety.show_media"), ButtonKind::Secondary, true, p)
        .key(cx.key(&format!("show-media-{event_id}")))
        .on_press(move |s: &mut State| s.reveal_media(&account, &id));
    let mut inside = col().gap(8.0).items_center();
    // A short frame keeps only the button.
    if h >= 110.0 {
        inside = inside
            .child(icon(&icons::EYE_OFF, 24.0).color(p.text_muted))
            .child(text(t!("safety.hidden_media")).font(font::META).color(p.text_muted));
    }
    el().key(cx.key(&format!("hidden-media-{event_id}")))
        .size(w, h)
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .center()
        .child(inside.child(show))
}

/// The play button over a video's poster.
fn play_badge() -> El {
    el().size(44.0, 44.0)
        .radius(22.0)
        .fill(rune::Color::BLACK.alpha(0.55))
        .center()
        .child(icon(&icons::PLAY, 22.0).color(rune::Color::WHITE))
}

/// The dimensions an image preview is sized by: the sender's, unless the `loaded` picture
/// has none to compare or a different shape, so a picture is never cropped to a box it
/// doesn't fit.
fn shown_dimensions(declared: Option<(u32, u32)>, loaded: Option<(u32, u32)>) -> Option<(u32, u32)> {
    let aspect = |(w, h): (u32, u32)| w as f32 / h.max(1) as f32;
    match (declared.filter(|(w, h)| *w > 0 && *h > 0), loaded.filter(|(w, h)| *w > 0 && *h > 0)) {
        (Some(declared), Some(loaded)) if (aspect(declared) / aspect(loaded) - 1.0).abs() > 0.02 => Some(loaded),
        (declared, loaded) => declared.or(loaded),
    }
}

/// Fits `dimensions` into [`PREVIEW_BOX`] without upscaling; a default box when unknown.
pub fn preview_size(dimensions: Option<(u32, u32)>) -> (f32, f32) {
    let (max_w, max_h) = PREVIEW_BOX;
    let Some((w, h)) = dimensions.filter(|(w, h)| *w > 0 && *h > 0) else { return (max_w, max_h * 0.75) };
    let (w, h) = (w as f32, h as f32);
    let scale = (max_w / w).min(max_h / h).min(1.0);
    ((w * scale).round(), (h * scale).round())
}

/// A file's card, up to the message's width: a long name is cut short.
/// `held`: a picture or video the media choice holds back, which opens only once its Show is
/// pressed (ADR 0011).
fn attachment_card(event_id: &str, attachment: &Attachment, cx: &Context<'_>, held: bool, p: &Palette) -> El {
    let max_width = cx.content_width;
    let glyph = match attachment.kind {
        _ if held => &icons::EYE_OFF,
        AttachmentKind::Image => &icons::IMAGE,
        AttachmentKind::Video => &icons::VIDEO,
        AttachmentKind::Audio => &icons::AUDIO,
        AttachmentKind::File => &icons::FILE,
    };
    let mut titles =
        col().min_w(0.0).shrink().child(text(attachment.name.clone()).font(font::BODY_STRONG).color(p.text).truncate());
    if let Some(size) = attachment.size {
        titles = titles.child(text(human_size(size)).font(font::META).color(p.text_muted));
    }
    let (opened, key, id) = (attachment.clone(), cx.key.clone(), event_id.to_owned());
    let card = row()
        .key(format!("file-{}", attachment.name))
        .max_w(max_width.clamp(0.0, 420.0))
        .mt(4.0)
        .px(12.0)
        .py(10.0)
        .gap(12.0)
        .center_y()
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .label(attachment.name.clone())
        .child(
            el().size(36.0, 36.0)
                .radius(6.0)
                .fill(p.accent.alpha(0.15))
                .center()
                .child(icon(glyph, 20.0).color(p.accent)),
        )
        .child(titles);
    if !held {
        return card.cursor(rune::Cursor::Pointer).on_press(move |s: &mut State| s.open_attachment(&key, &id, &opened));
    }
    let account = cx.account.clone();
    card.child(el().grow(1.0)).child(
        button(t!("safety.show_media"), ButtonKind::Secondary, true, p)
            .key(cx.key(&format!("show-media-{event_id}")))
            .on_press(move |s: &mut State| s.reveal_media(&account, &id)),
    )
}

/// Bars in an audio clip's waveform: as many as fit the message's width, within these.
const WAVE_MAX_BARS: usize = 36;
/// The fewest bars a roomy card draws; a compact one draws down to half as many.
const WAVE_MIN_BARS: usize = 8;
const WAVE_BAR: f32 = 3.0;
const WAVE_GAP: f32 = 2.0;
const WAVE_HEIGHT: f32 = 28.0;
/// The shortest bar, for silence or a clip whose waveform is unknown.
const WAVE_MIN_BAR: f32 = 3.0;
/// The scrubber's dot.
const KNOB: f32 = 12.0;
/// The play button.
const PLAY: f32 = 36.0;

/// How an audio card fits a message: the bars beside the play button with room to spare, then
/// beside it with tighter padding, and under it, across the card, only where not even a few
/// bars fit beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CardFit {
    Roomy,
    Compact,
    Stacked,
}

impl CardFit {
    /// The card's padding, left and right.
    fn padding(self) -> (f32, f32) {
        match self {
            Self::Roomy => (10.0, 14.0),
            Self::Compact | Self::Stacked => (8.0, 8.0),
        }
    }

    /// The gap between the play button and the bars.
    fn gap(self) -> f32 {
        match self {
            Self::Roomy => 10.0,
            Self::Compact | Self::Stacked => 6.0,
        }
    }

    /// The fewest bars beside the play button this fit takes.
    fn least_bars(self) -> usize {
        match self {
            Self::Roomy => WAVE_MIN_BARS,
            Self::Compact | Self::Stacked => WAVE_MIN_BARS / 2,
        }
    }

    /// How far the bars, the name and the time start from the card's padding.
    fn indent(self) -> f32 {
        match self {
            Self::Stacked => 0.0,
            fit => PLAY + fit.gap(),
        }
    }
}

/// How many bars a message `width` wide fits, and how the card lays them out.
fn wave_layout(width: f32) -> (usize, CardFit) {
    let fit = |room: f32| {
        if room.is_finite() {
            ((room + WAVE_GAP) / (WAVE_BAR + WAVE_GAP)).floor().max(0.0) as usize
        } else {
            WAVE_MAX_BARS
        }
    };
    let room = |fit: CardFit| {
        let (left, right) = fit.padding();
        width - left - right - 2.0 - fit.indent()
    };
    [CardFit::Roomy, CardFit::Compact]
        .into_iter()
        .map(|card| (fit(room(card)), card))
        .find(|(bars, card)| *bars >= card.least_bars())
        .map(|(bars, card)| (bars.min(WAVE_MAX_BARS), card))
        .unwrap_or_else(|| (fit(room(CardFit::Stacked)).clamp(WAVE_MIN_BARS / 2, WAVE_MAX_BARS), CardFit::Stacked))
}

/// The width of `bars` bars and the gaps between them.
fn wave_width(bars: usize) -> f32 {
    bars as f32 * (WAVE_BAR + WAVE_GAP) - WAVE_GAP
}

/// An audio clip or voice memo, as WhatsApp draws one: play and pause beside the clip's
/// waveform as bars that fill as it plays, a dot to drag (or a press anywhere along the bars,
/// or the arrow keys) to jump, and the time under them. While a clip is the current one, a
/// button beside the time steps its speed. A voice memo shows no file name; another audio file
/// keeps its name above the bars.
fn audio_card(message: &Message, attachment: &Attachment, cx: &Context<'_>, p: &Palette) -> El {
    let event_id = message.event_id.as_str();
    let clips = Clips { audio: cx.audio, decoded: cx.decoded_audio, speed: cx.playback_speed };
    // Beside play, the bars and the speed, a press opens the clip in the preview.
    let (opened, key, id) = (attachment.clone(), cx.key.clone(), event_id.to_owned());
    let card = audio_player(event_id, attachment, &clips, cx.content_width, p);
    // A voice card keeps to its message's side: the right for the user's own bubbles.
    let card = if cx.on_right(message) { card.self_end() } else { card };
    card.cursor(rune::Cursor::Pointer).on_press(move |s: &mut State| s.open_attachment(&key, &id, &opened))
}

/// What an audio card needs to know of the clips playing and played.
pub struct Clips<'a> {
    /// The clip playing, if any.
    pub audio: Option<&'a AudioClip>,
    /// What clips played here read from their files.
    pub decoded: &'a [crate::state::DecodedAudio],
    pub speed: PlaybackSpeed,
}

/// [`audio_card`] at up to `max_width`, keyed by `event_id`: a message's card, or the preview's
/// (keyed `preview`).
pub fn audio_player(event_id: &str, attachment: &Attachment, clips: &Clips<'_>, max_width: f32, p: &Palette) -> El {
    let clip = clips.audio.filter(|clip| Some(&clip.media) == attachment.media.as_ref());
    let player = clip.and_then(|clip| clip.player.as_ref());
    let playing = player.is_some_and(crate::platform::sound::Player::is_playing);
    // Pause while it plays, or is queued to play once its file comes.
    let pausable = playing || clip.is_some_and(|clip| clip.player.is_none() && clip.play && !clip.held);
    let progress = clip.map_or(0.0, AudioClip::progress) as f32;
    let details = attachment.audio.as_ref();
    let voice = details.is_some_and(|details| details.voice);
    // The sender's waveform; else, once played here, the one read from the file.
    let decoded = clips.decoded.iter().find(|decoded| attachment.media.as_ref() == Some(&decoded.media));
    let mut amplitudes = details.map(|details| details.waveform.clone()).unwrap_or_default();
    if amplitudes.is_empty() {
        amplitudes = player.map(crate::platform::sound::Player::waveform).unwrap_or_default();
    }
    if amplitudes.is_empty() {
        amplitudes = decoded.map(|decoded| decoded.waveform.clone()).unwrap_or_default();
    }
    let duration = details
        .and_then(|details| details.duration_ms)
        .map(|ms| ms as f64 / 1000.0)
        .or_else(|| player.and_then(crate::platform::sound::Player::duration))
        .or_else(|| decoded.and_then(|decoded| decoded.duration));
    // While it plays, is held or has been moved along, the time it is at (its share of the
    // length while its file is still coming); otherwise, its length.
    let moved = playing || progress > 0.0 || clip.is_some_and(|clip| clip.held);
    let time = match player.filter(|player| player.duration().is_some()) {
        Some(player) if moved => Some(player.time()),
        _ if moved => duration.map(|duration| duration * f64::from(progress)),
        _ => duration,
    };
    let (bars, fit) = wave_layout(max_width);
    let width = wave_width(bars);

    let opened = attachment.clone();
    let play = el()
        .key(format!("audio-play-{event_id}"))
        .size(PLAY, PLAY)
        .radius(PLAY / 2.0)
        .fill(p.accent)
        .center()
        .role("button")
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .label(if pausable { t!("audio.pause") } else { t!("audio.play") })
        .child(icon(if pausable { &icons::PAUSE } else { &icons::PLAY }, 18.0).color(p.on_accent))
        .on_press(move |s: &mut State| s.toggle_audio(&opened));

    // The name and the time line up with the bars, past the play button.
    let indent = fit.indent();
    let mut card = col()
        .key(format!("audio-{event_id}"))
        .self_start()
        .mt(4.0)
        .pl(fit.padding().0)
        // The slider reaches half a dot past the bars: the padding gives that room back.
        .pr(fit.padding().1 - KNOB / 2.0)
        .py(8.0)
        .gap(2.0)
        .radius(12.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .label(if voice { t!("audio.voice").to_owned() } else { attachment.name.clone() });
    if !voice {
        card = card
            .child(text(attachment.name.clone()).font(font::BODY_STRONG).color(p.text).truncate().ml(indent).w(width));
    }
    // In a narrow message the speed button takes a line under the time.
    // A compact card's time line spans the card, so the speed button fits beside or under the
    // time even where the bars are few.
    let (footer_indent, footer_width) = match fit {
        CardFit::Roomy => (indent, width),
        _ => (0.0, indent + width),
    };
    let mut footer = row().ml(footer_indent).w(footer_width).min_h(20.0).center_y().flex_wrap().gap(4.0);
    if let Some(time) = time {
        footer = footer.child(text(minutes(time)).font(font::META).color(p.text_muted));
    }
    if clip.is_some() {
        footer = footer.child(el().grow(1.0)).child(speed_button(event_id, clips.speed, p));
    }
    let wave = waveform(event_id, attachment, &amplitudes, bars, progress, p);
    match fit {
        CardFit::Stacked => card.gap(6.0).child(play).child(wave.ml(-KNOB / 2.0)).child(footer),
        _ => card.child(row().gap(fit.gap() - KNOB / 2.0).center_y().child(play).child(wave)).child(footer),
    }
}

/// `seconds` as `m:ss`.
pub fn minutes(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// `amplitudes` (each up to [`matrix_core::AudioDetails::WAVEFORM_MAX`]) as `bars` heights
/// from [`WAVE_MIN_BAR`] to the waveform's height: each bar the loudest of its stretch, scaled to
/// the loudest bar, so a quiet memo still shows its shape. Flat when there are none.
fn bar_heights(amplitudes: &[u16], bars: usize) -> Vec<f32> {
    if amplitudes.is_empty() || bars == 0 {
        return vec![WAVE_MIN_BAR; bars];
    }
    let peaks: Vec<u16> = (0..bars)
        .map(|i| {
            let start = i * amplitudes.len() / bars;
            let end = ((i + 1) * amplitudes.len() / bars).max(start + 1).min(amplitudes.len());
            amplitudes[start.min(amplitudes.len() - 1)..end].iter().copied().max().unwrap_or(0)
        })
        .collect();
    let loudest = f32::from(peaks.iter().copied().max().unwrap_or(0).max(1));
    peaks.into_iter().map(|peak| WAVE_MIN_BAR + (WAVE_HEIGHT - WAVE_MIN_BAR) * f32::from(peak) / loudest).collect()
}

/// The bars and the scrubber's dot: bars played so far in the accent, the rest muted. Pressing
/// or dragging anywhere along them jumps there; focused, the arrow keys step it, and Home and
/// End go to the start and the end.
fn waveform(
    event_id: &str,
    attachment: &Attachment,
    amplitudes: &[u16],
    bars: usize,
    progress: f32,
    p: &Palette,
) -> El {
    let width = wave_width(bars);
    let heights = bar_heights(amplitudes, bars).into_iter().enumerate().map(|(i, height)| {
        let played = (i as f32 + 0.5) / bars as f32 <= progress;
        el().w(WAVE_BAR).h(height).radius(WAVE_BAR / 2.0).fill(if played { p.accent } else { p.text_muted.alpha(0.45) })
    });
    // The slider reaches half a dot past the bars at each end, so all of the dot at either end
    // is the scrubber's (and a press there focuses it), not the message's under it.
    let knob = el()
        .key(format!("audio-knob-{event_id}"))
        .absolute(progress * width, (WAVE_HEIGHT - KNOB) / 2.0)
        .size(KNOB, KNOB)
        .radius(KNOB / 2.0)
        .fill(p.accent)
        .border(2.0, p.surface);
    let (scrubbed, stepped, ended) = (attachment.clone(), attachment.clone(), attachment.clone());
    el().key(format!("audio-wave-{event_id}"))
        .w(width + KNOB)
        .h(WAVE_HEIGHT)
        .role("slider")
        .label(t!("audio.seek"))
        .range_value(f64::from(progress), 0.0, 1.0)
        .range_step(crate::state::AUDIO_STEP)
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .on_drag(move |s: &mut State, drag, _| {
            let scrub = match drag.phase {
                rune::Phase::Down => Scrub::Press,
                rune::Phase::Move => Scrub::Drag,
                rune::Phase::Up if drag.cancelled() => Scrub::Cancel,
                rune::Phase::Up => Scrub::Release,
            };
            s.scrub_audio(&scrubbed, f64::from((drag.x - KNOB / 2.0) / width.max(1.0)), scrub);
        })
        .on_step(move |s: &mut State, steps, _| s.step_audio(&stepped, steps))
        .on_step_limit(move |s: &mut State, end, _| s.seek_audio(&ended, if end { 1.0 } else { 0.0 }))
        .child(row().absolute(KNOB / 2.0, 0.0).w(width).h(WAVE_HEIGHT).gap(WAVE_GAP).center_y().children(heights))
        .child(knob)
}

/// The speed a clip plays at, `1×`, `1.5×` or `2×`; pressing it steps to the next.
fn speed_button(event_id: &str, speed: PlaybackSpeed, p: &Palette) -> El {
    let rate = I18N.decimal(speed.rate(), 1);
    el().key(format!("audio-speed-{event_id}"))
        .px(7.0)
        .h(20.0)
        .center()
        .radius(10.0)
        .border(1.0, p.divider)
        .hover(p.hover)
        .role("button")
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .label(tf!("audio.speed_label", rate = rate.clone()))
        .child(text(tf!("audio.speed", rate = rate)).font(font::META).color(p.text_muted))
        .on_press(State::cycle_playback_speed)
}

/// `1.4 MB`-style sizes, with the locale's decimal separator and units.
pub fn human_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < 3 {
        value /= 1000.0;
        unit += 1;
    }
    let n = I18N.decimal(value, if unit == 0 { 0 } else { 1 });
    match unit {
        0 => tf!("size.b", n = n),
        1 => tf!("size.kb", n = n),
        2 => tf!("size.mb", n = n),
        _ => tf!("size.gb", n = n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys of the rows #general plans with the changes of its setup (`demo::room_changes`).
    fn planned(show_room_changes: bool) -> Vec<String> {
        let mut s = State::detached();
        crate::demo::room_changes(&mut s);
        s.settings.show_room_changes = show_room_changes;
        let account = AccountId::new(crate::demo::ADA);
        let key = TimelineKey::room(crate::demo::DEMO_THREAD.0);
        let data = s.accounts.get(&account).expect("the account");
        let (room, timeline) = (data.room(&key.room_id).expect("the room"), data.timeline(&key));
        plan(&Context::new(&s, data, room, &key), timeline).into_iter().map(|(key, _)| key).collect()
    }

    #[test]
    fn a_room_setting_change_should_leave_the_timeline_by_default() {
        let rows = planned(false);
        assert!(!rows.iter().any(|key| key == "ev-topic" || key == "ev-created"), "{rows:?}");
    }

    #[test]
    fn a_profile_change_should_leave_the_timeline_by_default() {
        assert!(!planned(false).iter().any(|key| key == "ev-grace-picture"));
    }

    #[test]
    fn an_invite_should_stay_in_the_timeline() {
        assert!(planned(false).iter().any(|key| key == "ev-invited"));
    }

    #[test]
    fn room_changes_should_show_in_the_timeline_when_turned_on() {
        let rows = planned(true);
        assert!(["ev-topic", "ev-created", "ev-grace-picture"].iter().all(|key| rows.iter().any(|row| row == key)));
    }

    #[test]
    fn joins_should_read_as_one_line_across_a_change_the_timeline_leaves_out() {
        let rows = planned(false);
        let joins = rows.iter().filter(|key| key.starts_with("ev-joined-")).count();
        assert_eq!(joins, 1, "{rows:?}");
    }

    #[test]
    fn a_run_of_joins_should_not_name_whoever_made_a_change_it_passed_over() {
        let person = |id: &str| Person { id: id.into(), name: id.into(), avatar: None };
        let event = |id: &str, change: RoomChange| crate::state::Entry {
            id: id.into(),
            kind: EntryView::RoomEvent(Box::new(RoomEvent { sender: person(id), change, timestamp_ms: 0 })),
        };
        let entries = [
            event("alice", RoomChange::Joined),
            event("bob", RoomChange::RoomTopic("News".into())),
            event("carol", RoomChange::Joined),
        ];
        let named: Vec<&str> = run_of(&entries).iter().map(|event| event.sender.id.as_str()).collect();
        assert_eq!(named, ["alice", "carol"]);
    }

    #[test]
    fn a_day_of_only_changes_the_timeline_leaves_out_should_have_no_date_divider() {
        assert!(!planned(false).iter().any(|key| key == "day-setup"));
    }

    #[test]
    fn a_day_with_something_to_show_should_keep_its_date_divider() {
        assert!(planned(false).iter().any(|key| key == "day"));
    }

    #[test]
    fn a_day_of_room_changes_should_keep_its_date_divider_while_they_show() {
        assert!(planned(true).iter().any(|key| key == "day-setup"));
    }

    #[test]
    fn a_reply_to_a_call_should_quote_the_kind_of_call() {
        let card = matrix_core::CallCard { video: Some(true), ..matrix_core::CallCard::default() };
        let quote = Quote { event_id: "$call".into(), sender_name: None, body: None, call: Some(card) };
        assert_eq!(quote_text(&quote), t!("call.card.video"));
    }

    #[test]
    fn preview_size_should_fit_a_wide_image_to_the_box_width() {
        assert_eq!(preview_size(Some((1440, 720))), (360.0, 180.0));
    }

    #[test]
    fn preview_size_should_not_upscale_small_images() {
        assert_eq!(preview_size(Some((100, 50))), (100.0, 50.0));
    }

    #[test]
    fn an_image_should_be_sized_by_its_senders_dimensions_when_the_picture_matches() {
        assert_eq!(shown_dimensions(Some((1440, 720)), Some((720, 360))), Some((1440, 720)));
    }

    #[test]
    fn an_image_without_its_senders_dimensions_should_be_sized_by_its_picture() {
        assert_eq!(shown_dimensions(None, Some((800, 120))), Some((800, 120)));
    }

    #[test]
    fn an_image_whose_picture_has_another_shape_should_be_sized_by_its_picture() {
        assert_eq!(shown_dimensions(Some((400, 400)), Some((800, 120))), Some((800, 120)));
    }

    #[test]
    fn a_date_should_read_as_year_month_and_day() {
        assert_eq!(civil_date(20_405), (2025, 11, 13));
    }

    #[test]
    fn the_date_of_today_should_read_today() {
        assert_eq!(date_label(1_000, 2_000), "Today");
    }

    #[test]
    fn an_older_date_should_be_spelled_out() {
        let nov_13_2025 = 20_405 * 86_400_000;
        assert_eq!(date_label(nov_13_2025, nov_13_2025 + 5 * 86_400_000), "November 13, 2025");
    }

    #[test]
    fn a_waveform_should_scale_to_its_loudest_bar() {
        assert_eq!(bar_heights(&[0, 256, 512], 3), vec![WAVE_MIN_BAR, (WAVE_MIN_BAR + WAVE_HEIGHT) / 2.0, WAVE_HEIGHT]);
    }

    #[test]
    fn a_narrow_message_should_draw_fewer_bars_and_a_wide_one_no_more_than_the_most() {
        let (narrow, wide, unknown) = (wave_layout(208.0), wave_layout(2000.0), wave_layout(f32::INFINITY));
        let roomy = CardFit::Roomy;
        assert_eq!((narrow, wide, unknown), ((27, roomy), (WAVE_MAX_BARS, roomy), (WAVE_MAX_BARS, roomy)));
    }

    #[test]
    fn a_narrow_message_should_keep_the_bars_beside_play_in_a_compact_card() {
        assert_eq!(wave_layout(100.0), (8, CardFit::Compact));
    }

    #[test]
    fn a_message_too_narrow_for_bars_beside_play_should_put_them_under_it() {
        assert_eq!(wave_layout(59.0), (8, CardFit::Stacked));
    }

    #[test]
    fn an_unknown_waveform_should_draw_flat_bars() {
        assert_eq!(bar_heights(&[], 4), vec![WAVE_MIN_BAR; 4]);
    }

    #[test]
    fn a_long_waveform_should_keep_each_stretch_peak() {
        assert_eq!(bar_heights(&[0, 1024, 0, 0], 2)[0], WAVE_HEIGHT);
    }

    #[test]
    fn human_size_should_keep_small_sizes_in_bytes() {
        assert_eq!(human_size(512), "512 B");
    }

    #[test]
    fn human_size_should_use_one_decimal_for_larger_units() {
        assert_eq!(human_size(1_420_000), "1.4 MB");
    }
}
