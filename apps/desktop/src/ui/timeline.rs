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
use rune::{Color, El, ScrollSpec, SurfaceFit, col, el, icon, row, row as row_el, text, text_input};

use super::components::{ButtonKind, button, clock_time, local_ms, media_avatar};
use super::icons;
use super::room::{display_name, inline_name};
use super::theme::{Palette, font};
use crate::i18n::{I18N, t, tf, tp};
use crate::state::{
    AudioClip, ComposerTarget, EmojiFor, EntryView, GifCache, LinkPreviews, MediaCache, Popover, RoomTimeline, State,
    TimelineItem, audio_supported,
};
use matrix_core::links::LinkPreview;

/// A timeline row's height before it is measured: a message with its header.
const ESTIMATED_ROW: f32 = 64.0;

/// Messages from one sender this close together share one header.
const GROUP_WINDOW_MS: u64 = 5 * 60_000;
const AVATAR: f32 = 36.0;

/// What every message row in one timeline needs.
pub struct Context<'a> {
    pub account: &'a AccountId,
    pub key: &'a TimelineKey,
    pub room: &'a RoomSummary,
    pub media: &'a MediaCache,
    pub gifs: &'a GifCache,
    pub show_gifs: bool,
    /// Links' preview cards, when they show.
    pub link_previews: Option<&'a LinkPreviews>,
    /// The message whose reaction picker is open, if any.
    pub picking: Option<&'a str>,
    /// The audio playing, if any.
    pub audio: Option<&'a AudioClip>,
}

impl Context<'_> {
    fn in_thread(&self) -> bool {
        self.key.thread.is_some()
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

/// Marks where the names go in a "many" template.
const NAMES: &str = "\u{1}";
/// The size of the pictures on a room event line.
const EVENT_AVATAR: f32 = 18.0;

/// What one room event says, in the user's language.
fn room_event_text(event: &RoomEvent) -> String {
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
}

/// Rows either side of the viewport whose media loads before they scroll into view.
const SHOWN_MARGIN: usize = 10;

impl Row {
    /// The timeline entries the row shows.
    fn entries(&self) -> Option<std::ops::Range<usize>> {
        match *self {
            Row::Message { entry, .. } | Row::Divider(entry) => Some(entry..entry + 1),
            Row::Events { start, end } => Some(start..end),
            Row::Start | Row::HistoryFailed(_) | Row::Loading => None,
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

/// The rows of a timeline and their keys: one cheap pass over the entries.
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
    let mut index = 0;
    while let Some(entry) = entries.get(index) {
        let at = index;
        index += 1;
        match &entry.kind {
            EntryView::RoomEvent(event) => {
                // Joins (or leaves) in a row read as one line; hidden entries between them
                // don't break the run.
                while event.change.groups()
                    && let Some(next) = entries.get(index)
                {
                    match &next.kind {
                        EntryView::RoomEvent(other) if other.change == event.change => {}
                        EntryView::Other => {}
                        _ => break,
                    }
                    index += 1;
                }
                rows.push((cx.key(&entry.id), Row::Events { start: at, end: index }));
                previous = None;
            }
            EntryView::Message(item) => {
                let message = &item.message;
                let continues = previous.is_some_and(|prev| {
                    prev.sender_id == message.sender_id
                        && message.timestamp_ms.saturating_sub(prev.timestamp_ms) < GROUP_WINDOW_MS
                        && message.reply_to.is_none()
                });
                rows.push((row_key(cx.key, &entry.id), Row::Message { entry: at, continues, latest_call: false }));
                previous = Some(message);
            }
            EntryView::DateDivider(_) | EntryView::ReadMarker => {
                rows.push((cx.key(&entry.id), Row::Divider(at)));
                if matches!(entry.kind, EntryView::DateDivider(_)) {
                    previous = None;
                }
            }
            EntryView::Start if !cx.in_thread() => {
                rows.push((entry.id.clone(), Row::Start));
                previous = None;
            }
            EntryView::Start | EntryView::Other => {}
        }
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
    let cx = Context {
        account: &data.summary.id,
        key,
        room: data.room(&key.room_id)?,
        media: &s.media,
        gifs: &s.gifs,
        show_gifs: s.settings.show_link_gifs,
        link_previews: s.settings.show_link_previews.then_some(&s.link_previews),
        picking: super::room::picking(s, key),
        audio: s.audio.as_ref(),
    };
    Some((cx, data.timeline(key)?))
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
            let run: Vec<&RoomEvent> = entries[*from..(*end).min(entries.len())]
                .iter()
                .filter_map(|entry| match &entry.kind {
                    EntryView::RoomEvent(event) => Some(event.as_ref()),
                    _ => None,
                })
                .collect();
            if run.is_empty() { el().key(key.to_owned()) } else { room_event_line(key, &run, cx.media, p) }
        }
        Row::Divider(entry) => match entries.get(*entry).map(|e| &e.kind) {
            Some(EntryView::DateDivider(at)) => {
                rule_with_label(key, &date_label(local_ms(*at), local_ms(now_ms())), p.text_muted, p)
            }
            _ => rule_with_label(key, t!("timeline.new"), p.mention, p),
        },
    }
}

/// The scrolling list of a timeline. `view_key` names the viewport; `near_top` is raised when it
/// is scrolled within a screen of its oldest message.
pub fn timeline(
    cx: &Context<'_>,
    timeline: Option<&RoomTimeline>,
    view_key: &str,
    near_top: &Arc<AtomicBool>,
    p: &Palette,
) -> El {
    // Only the rows near the viewport are built, laid out and drawn: a room of thousands of
    // messages commits as fast as a short one. The plan is one pass, made again only when the
    // timeline changed; each mounted row is built from the state by its plan entry.
    let revision = timeline.map_or(0, |t| t.revision);
    let plan = cached_plan(view_key, cx, timeline, revision);
    let (keys, shown) = (Rc::clone(&plan), Rc::clone(&plan));
    let (account, key, palette) = (cx.account.clone(), cx.key.clone(), *p);
    let (seen_account, seen_key) = (account.clone(), key.clone());
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
                    Some((cx, timeline)) => build_row(&cx, timeline, row_key, row, &palette),
                    None => el().key(row_key.clone()),
                }
            },
        )
        // What shows loads (pictures, previews, GIFs), with a margin to scroll into.
        .on_visible_rows(move |s: &mut State, rows| {
            let near = rows.start.saturating_sub(SHOWN_MARGIN)..(rows.end + SHOWN_MARGIN).min(shown.len());
            let entries = shown.get(near).unwrap_or_default().iter().filter_map(|(_, row)| row.entries());
            let (start, end) =
                entries.fold((usize::MAX, 0), |(start, end), range| (start.min(range.start), end.max(range.end)));
            if start < end {
                s.rows_shown(&seen_account, &seen_key, start..end);
            }
            false
        });
    // The keys change only with the plan, so Rune need not ask for them again until then.
    if revision != 0 {
        list = list.virtual_rows_version(revision);
    }
    let flag = Arc::clone(near_top);
    // Short timelines sit at the bottom, by the composer, as in chat apps: the frame takes the
    // space and pushes the viewport down; the viewport is as tall as its content, up to all of it.
    let viewport = col()
        .key(view_key.to_owned())
        .w_full()
        .shrink()
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        // Hold the reading position when older messages arrive above it.
        .scroll_anchor()
        // Within a screen of the oldest loaded message: ask for the page before it.
        .on_scroll_offset(move |at| {
            if at.offset[1] < at.viewport[1] {
                flag.store(true, Ordering::Relaxed);
            }
        })
        .child(list);
    col().key(format!("{view_key}-frame")).w_full().grow(1.0).min_h(0.0).justify_end().child(viewport)
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

/// One message: avatar and name (unless it continues the sender's run), the quoted message it
/// answers, its content, a linked GIF, reactions and thread; the toolbar shows on hover.
/// Clicking a room message opens its thread.
fn message_row(
    cx: &Context<'_>,
    key: &str,
    item: &TimelineItem,
    continues: bool,
    latest_call: bool,
    p: &Palette,
) -> El {
    let message = &item.message;
    let mut body = col().w_full().gap(4.0);
    if let Some(quote) = &message.reply_to {
        body = body.child(quote_block(quote, p));
    }
    // A GIF link shows as the GIF, below the rest of the message (if any) like an attachment.
    match item.gif.as_ref().filter(|gif| cx.show_gifs && !cx.gifs.has_failed(&gif.url)) {
        Some(gif) => {
            if let Some(rest) = &gif.rest {
                body = body.child(text_body(&message.event_id, &rest.plain, rest.rich.as_ref(), p));
            }
            body = body.child(linked_gif(&gif.url, cx.gifs, p));
        }
        None => body = body.child(message_body(item, cx, latest_call, p)),
    }
    // A link's card, below the text, once the homeserver described the page.
    if let Some((url, card)) =
        item.card_link(cx.show_gifs).and_then(|url| Some((url, cx.link_previews?.get(cx.account, url)?)))
    {
        body = body.child(link_card(&cx.key(&format!("link-{}", message.event_id)), url, card, cx.media, p));
    }
    if !message.reactions.is_empty() {
        body = body.child(reactions(cx, message, p));
    }
    if let (Some(thread), false) = (&message.thread, cx.in_thread()) {
        body = body.child(thread_link(&message.event_id, thread, cx.media, p));
    }

    let mut status = Vec::new();
    if message.edited {
        status.push(t!("message.edited"));
    }
    if message.pending {
        status.push(t!("message.sending"));
    }
    let status = (!status.is_empty()).then(|| text(status.join(" · ")).font(font::META).color(p.text_muted));
    let content = if continues {
        let mut content = col().grow(1.0).min_w(0.0).child(body);
        if let Some(status) = status {
            content = content.child(status);
        }
        content
    } else {
        let mut meta = row()
            .gap(8.0)
            .center_y()
            .child(text(message.sender_name.clone()).font(font::BODY_STRONG).color(p.text))
            .child(text(clock_time(local_ms(message.timestamp_ms))).font(font::META).color(p.text_muted));
        if let Some(status) = status {
            meta = meta.child(status);
        }
        col().grow(1.0).min_w(0.0).gap(2.0).child(meta).child(body)
    };
    let gutter = if continues {
        el().w(AVATAR)
    } else {
        media_avatar(&message.sender_id, &message.sender_name, message.sender_avatar.as_ref(), cx.media, AVATAR)
    };
    let mut row = row()
        .key(key.to_owned())
        .w_full()
        .px(20.0)
        .pt(if continues { 2.0 } else { 10.0 })
        .pb(2.0)
        .gap(10.0)
        .items_start()
        .opacity(if message.pending { 0.6 } else { 1.0 })
        .hover(p.hover)
        .child(gutter)
        .child(content);
    if !message.pending {
        row = row.child(toolbar(cx, message, p));
        if !cx.in_thread() {
            let root = message.event_id.clone();
            row = row.cursor(rune::Cursor::Pointer).on_press(move |s: &mut State| s.open_thread(&root));
        }
    }
    row
}

/// React, reply and reply in thread, at the message's top right while it is hovered (or its
/// reaction picker is open).
fn toolbar(cx: &Context<'_>, message: &Message, p: &Palette) -> El {
    let event_id = message.event_id.clone();
    let open = cx.picking == Some(event_id.as_str());
    let target = if cx.in_thread() { ComposerTarget::Thread } else { ComposerTarget::Room };
    let button = |glyph: &'static rune::Icon, label: &str, name: &str| {
        el().key(cx.key(&format!("{name}-{event_id}")))
            .size(30.0, 30.0)
            .radius(6.0)
            .center()
            .hover(p.hover.alpha(p.hover.a * 3.0))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(label.to_owned())
            .focusable(true)
            .child(icon(glyph, 18.0).color(p.text_muted))
    };
    let (key, quoted) = (cx.key.clone(), message.clone());
    let mut bar = row()
        .key(cx.key(&format!("actions-{event_id}")))
        .pin(None, Some(-14.0), Some(16.0), None)
        .px(2.0)
        .py(2.0)
        .gap(2.0)
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .shadow(2.0, 8.0, Color::BLACK.alpha(0.2))
        .group_opacity(if open { 1.0 } else { 0.0 }, 1.0)
        .child(button(&icons::SMILE, t!("actions.react"), "react").on_press(move |s: &mut State| {
            s.toggle_popover(Popover::Emoji(EmojiFor::Reaction {
                key: key.clone(),
                event_id: quoted.event_id.clone(),
            }));
        }));
    let reply = message.clone();
    bar = bar.child(
        button(&icons::REPLY, t!("actions.reply"), "reply")
            .on_press_tx(move |s: &mut State, tx| s.start_reply(target, &reply, tx)),
    );
    if !cx.in_thread() {
        let root = event_id.clone();
        bar = bar.child(
            button(&icons::THREAD, t!("actions.thread"), "thread").on_press(move |s: &mut State| s.open_thread(&root)),
        );
    }
    // The user's own messages: more, in a menu (edit, delete).
    if message.sender_id == cx.account.as_str() && !matches!(message.content, MessageContent::Redacted) {
        let (key, id) = (cx.key.clone(), event_id.clone());
        bar = bar.child(button(&icons::MORE, t!("actions.more"), "more").on_press(move |s: &mut State| {
            s.toggle_popover(Popover::MessageMenu { key: key.clone(), event_id: id.clone() });
        }));
    }
    bar
}

/// The menu of one of the user's messages, under its toolbar's ⋮: edit (text only) and delete.
pub fn message_menu(s: &State, key: &TimelineKey, event_id: &str, p: &Palette) -> Vec<El> {
    use super::components::{Tone, menu_action, popover_panel};
    let message = s.nav.account.as_ref().and_then(|account| s.accounts.get(account)).and_then(|data| {
        data.timeline(key)?.entries.iter().find_map(|entry| match &entry.kind {
            EntryView::Message(item) if item.message.event_id == event_id => Some(item.message.clone()),
            _ => None,
        })
    });
    let Some(message) = message else { return Vec::new() };
    let (target, thread) = match key.thread {
        None => (ComposerTarget::Room, ""),
        Some(_) => (ComposerTarget::Thread, "thread-"),
    };
    let mut panel = popover_panel(p).key("message-menu").w(200.0);
    if matches!(message.content, MessageContent::Text { .. }) {
        panel = panel.child(
            menu_action(&icons::PENCIL, t!("actions.edit"), Tone::Normal, p)
                .key("message-edit")
                .on_press_tx(move |s: &mut State, tx| s.start_edit(target, &message, tx)),
        );
    }
    let (key, deleted) = (key.clone(), event_id.to_owned());
    panel = panel.child(
        menu_action(&icons::TRASH, t!("actions.delete"), Tone::Danger, p)
            .key("message-delete")
            .on_press(move |s: &mut State| s.delete_message(key.clone(), &deleted)),
    );
    let anchor = format!("{thread}more-{event_id}");
    vec![rune::col().child(panel).anchored(anchor, rune::Side::Bottom, rune::Align::End, 4.0)]
}

/// The message a reply answers: who wrote it and an excerpt, behind a bar.
fn quote_block(quote: &Quote, p: &Palette) -> El {
    let sender = quote.sender_name.clone().unwrap_or_default();
    let body = quote.body.clone().unwrap_or_else(|| t!("quote.unavailable").to_owned());
    row().max_w(560.0).gap(8.0).child(el().w(3.0).h_full().min_h(32.0).radius(2.0).fill(p.text_muted.alpha(0.5))).child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .py(2.0)
            .child(text(sender).font(font::META.family(rune::Family::Sans)).color(p.text))
            .child(text(body).font(font::META).color(p.text_muted).max_lines(2).truncate()),
    )
}

/// Reaction chips: emoji and count, outlined when the user's own; each toggles the user's.
fn reactions(cx: &Context<'_>, message: &Message, p: &Palette) -> El {
    let mut chips = row().gap(6.0).flex_wrap().pt(2.0);
    for reaction in &message.reactions {
        let (key, event_id, emoji) = (cx.key.clone(), message.event_id.clone(), reaction.key.clone());
        chips = chips.child(
            row()
                .key(cx.key(&format!("reaction-{}-{}", message.event_id, reaction.key)))
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
                .child(text(reaction.key.clone()).font(font::BODY))
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
        link = link.child(text(when).font(font::META).color(p.text_muted));
    }
    link.on_press(move |s: &mut State| s.open_thread(&root))
}

/// A GIF behind a link in the message, playing once it has loaded.
fn linked_gif(url: &str, gifs: &GifCache, p: &Palette) -> El {
    let frame = el().key(format!("gif-{url}")).w(280.0).radius(8.0).role("img").label(url.to_owned());
    match gifs.player(url) {
        Some(player) => frame.pixels(player.pixels()),
        None => frame
            .h(160.0)
            .fill(p.surface)
            .border(1.0, p.divider)
            .center()
            .child(text("GIF").font(font::SECTION).color(p.text_muted)),
    }
}

fn message_body(item: &TimelineItem, cx: &Context<'_>, latest_call: bool, p: &Palette) -> El {
    let media = cx.media;
    match (&item.message.content, &item.rich) {
        (MessageContent::Text { body, .. }, rich) => text_body(&item.message.event_id, body, rich.as_ref(), p),
        (MessageContent::Attachment(attachment), _)
            if attachment.kind == AttachmentKind::Audio && audio_supported() =>
        {
            audio_card(attachment, cx.audio, p)
        }
        (MessageContent::Attachment(attachment), _) => match attachment.preview() {
            Some((preview, _)) if !media.has_failed(&preview) => image_preview(attachment, media, p),
            _ => attachment_card(attachment, p),
        },
        (MessageContent::Redacted, _) => notice_text(t!("message.deleted"), p),
        (MessageContent::Undecryptable, _) => notice_text(t!("message.undecryptable"), p),
        (MessageContent::Poll(poll), _) => {
            let own = item.message.sender_id == cx.account.as_str();
            super::poll::card(poll, &item.message.event_id, own, cx.key, p)
        }
        (MessageContent::Call(card), _) => call_card(cx, &item.message.event_id, card, latest_call, p),
    }
}

/// The name of a kind of call: video, voice, or just a call when the caller didn't say.
pub fn call_title(card: &CallCard) -> &'static str {
    match card.video {
        Some(true) => t!("call.card.video"),
        Some(false) => t!("call.card.voice"),
        None => t!("call.card.call"),
    }
}

/// A call's card: what kind it is and how it stands. The newest card in the room shows whether
/// the room's call is under way, with the way in; older ones have ended.
fn call_card(cx: &Context<'_>, event_id: &str, card: &CallCard, latest_call: bool, p: &Palette) -> El {
    let people = &cx.room.call.participants;
    let ongoing = latest_call && !people.is_empty();
    let status = if ongoing {
        format!("{} · {}", t!("call.card.ongoing"), tf!("call.people", count = people.len()))
    } else if card.declined_by.is_empty() {
        t!("call.card.ended").to_owned()
    } else {
        t!("call.card.declined").to_owned()
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
        .child(text(call_title(card)).font(font::BODY_STRONG).color(p.text).truncate())
        .child(text(status).font(font::META).color(p.text_muted).truncate());
    let mut entry = row()
        .key(cx.key(&format!("call-card-{event_id}")))
        .max_w(460.0)
        .mt(2.0)
        .padding(12.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, if ongoing { p.success.alpha(0.6) } else { p.divider })
        .child(tile)
        .child(words);
    if ongoing {
        entry = entry.child(if people.iter().any(|person| person == cx.account.as_str()) {
            text(t!("call.card.joined")).font(font::META).color(p.text_muted)
        } else {
            let video = card.video == Some(true);
            super::call::pill(&icons::CALL, t!("call.join"), p.success, p)
                .key(cx.key(&format!("call-card-join-{event_id}")))
                .on_press(move |s: &mut State| s.start_call(video))
        });
    }
    entry
}

/// Message text, formatted when the sender formatted it. It selects, so it shows the text cursor
/// even inside a row that opens its thread.
fn text_body(event_id: &str, plain: &str, rich: Option<&rune::TextEdit>, p: &Palette) -> El {
    match rich {
        Some(rich) => text_input(rich, font::BODY)
            .key(format!("rich-{event_id}"))
            .color(p.text)
            .w_full()
            .focusable(false)
            .cursor(rune::Cursor::Text)
            .on_press(|_: &mut State| {})
            .on_link(|s: &mut State, url: &str, tx: &mut rune::Tx| s.open_link(url, tx)),
        // Its own handler keeps the clicks: a double-click takes a word, a triple-click the
        // paragraph, rather than opening the thread.
        None => text(plain.to_owned())
            .font(font::BODY)
            .color(p.text)
            .w_full()
            .wrap()
            .selectable(true)
            .cursor(rune::Cursor::Text)
            .on_press(|_: &mut State| {}),
    }
}

/// A message's stand-in when its content can't be shown.
fn notice_text(message: &str, p: &Palette) -> El {
    text(message.to_owned()).font(font::BODY.italic()).color(p.text_muted).w_full().wrap()
}

/// The largest box an image preview may take.
const PREVIEW_BOX: (f32, f32) = (360.0, 280.0);

/// A link's preview card: the site, the page's title and description, its image beside them.
/// Pressing it opens the link.
fn link_card(key: &str, url: &str, card: &LinkPreview, media: &MediaCache, p: &Palette) -> El {
    /// The image's side.
    const THUMB: f32 = 72.0;
    let mut words = col().grow(1.0).min_w(0.0).gap(2.0);
    if let Some(site) = &card.site {
        words = words.child(text(site.clone()).font(font::META).color(p.text_muted).truncate());
    }
    if let Some(title) = &card.title {
        words = words.child(text(title.clone()).font(font::BODY_STRONG).color(p.accent).max_lines(2));
    }
    if let Some(description) = &card.description {
        words = words.child(text(description.clone()).font(font::META).color(p.text).max_lines(3));
    }
    let label = card.title.clone().unwrap_or_else(|| url.to_owned());
    let opened = url.to_owned();
    let mut entry = row()
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
        .child(words);
    // An image that failed to load leaves the words on their own.
    if let Some((image, _)) = card.image.as_ref().filter(|(image, _)| !media.has_failed(image)) {
        let thumb = el().size(THUMB, THUMB).radius(8.0).clip_rounded(8.0);
        entry = entry.child(match media.image(image) {
            Some(image) => thumb.pixels_fit(image.pixels(), SurfaceFit::Fill),
            None => thumb.fill(p.hover).center().child(icon(&icons::IMAGE, 20.0).color(p.text_muted)),
        });
    }
    entry.on_press_tx(move |s: &mut State, tx| s.open_link(&opened, tx))
}

/// An image, scaled to fit [`PREVIEW_BOX`] by the sender's dimensions so the timeline doesn't
/// jump when it loads; a placeholder of the same size until then.
fn image_preview(attachment: &Attachment, media: &MediaCache, p: &Palette) -> El {
    let (w, h) = preview_size(attachment.dimensions);
    let opened = attachment.clone();
    let frame = el()
        .key(format!("image-{}", attachment.name))
        .size(w, h)
        .mt(4.0)
        .radius(8.0)
        .role("img")
        .label(attachment.name.clone())
        .cursor(rune::Cursor::Pointer)
        .on_press(move |s: &mut State| s.open_attachment(&opened));
    let image = attachment.preview().and_then(|(media_ref, _)| media.image(&media_ref));
    match image {
        Some(image) => frame.pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => {
            frame.fill(p.surface).border(1.0, p.divider).center().child(icon(&icons::IMAGE, 24.0).color(p.text_muted))
        }
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

fn attachment_card(attachment: &Attachment, p: &Palette) -> El {
    let glyph = match attachment.kind {
        AttachmentKind::Image => &icons::IMAGE,
        AttachmentKind::Video => &icons::VIDEO,
        AttachmentKind::Audio => &icons::AUDIO,
        AttachmentKind::File => &icons::FILE,
    };
    let mut titles =
        col().min_w(0.0).child(text(attachment.name.clone()).font(font::BODY_STRONG).color(p.text).truncate());
    if let Some(size) = attachment.size {
        titles = titles.child(text(human_size(size)).font(font::META).color(p.text_muted));
    }
    let opened = attachment.clone();
    row()
        .key(format!("file-{}", attachment.name))
        .max_w(420.0)
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
        .cursor(rune::Cursor::Pointer)
        .on_press(move |s: &mut State| s.open_attachment(&opened))
        .child(
            el().size(36.0, 36.0)
                .radius(6.0)
                .fill(p.accent.alpha(0.15))
                .center()
                .child(icon(glyph, 20.0).color(p.accent)),
        )
        .child(titles)
}

/// An audio file or voice memo: play and pause, its name, and the time played of its length.
fn audio_card(attachment: &Attachment, clip: Option<&AudioClip>, p: &Palette) -> El {
    let player =
        clip.filter(|clip| Some(&clip.media) == attachment.media.as_ref()).and_then(|clip| clip.player.as_ref());
    let playing = player.is_some_and(crate::platform::sound::Player::is_playing);
    let minutes = |seconds: f64| format!("{}:{:02}", seconds as u64 / 60, seconds as u64 % 60);
    let time = player.map(|player| match player.duration() {
        Some(total) => format!("{} / {}", minutes(player.time()), minutes(total)),
        None => minutes(player.time()),
    });
    let opened = attachment.clone();
    let mut titles =
        col().min_w(0.0).child(text(attachment.name.clone()).font(font::BODY_STRONG).color(p.text).truncate());
    if let Some(time) = time {
        titles = titles.child(text(time).font(font::META).color(p.text_muted));
    }
    row()
        .key(format!("audio-{}", attachment.name))
        .max_w(420.0)
        .mt(4.0)
        .px(12.0)
        .py(10.0)
        .gap(12.0)
        .center_y()
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .child(
            el().size(36.0, 36.0)
                .radius(18.0)
                .fill(p.accent)
                .center()
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(if playing { t!("audio.pause") } else { t!("audio.play") })
                .child(icon(if playing { &icons::PAUSE } else { &icons::PLAY }, 18.0).color(p.on_accent))
                .on_press(move |s: &mut State| s.toggle_audio(&opened)),
        )
        .child(titles)
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

    #[test]
    fn preview_size_should_fit_a_wide_image_to_the_box_width() {
        assert_eq!(preview_size(Some((1440, 720))), (360.0, 180.0));
    }

    #[test]
    fn preview_size_should_not_upscale_small_images() {
        assert_eq!(preview_size(Some((100, 50))), (100.0, 50.0));
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
    fn human_size_should_keep_small_sizes_in_bytes() {
        assert_eq!(human_size(512), "512 B");
    }

    #[test]
    fn human_size_should_use_one_decimal_for_larger_units() {
        assert_eq!(human_size(1_420_000), "1.4 MB");
    }
}
