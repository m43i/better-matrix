//! The open room: header, timeline and composer, with the open thread beside it. Files dropped
//! on the room attach to its composer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use matrix_core::{RoomKind, RoomSummary, TimelineKey};
use rune::{Cx, El, col, row, text};

use super::components::icon_button;
use super::composer::composer;
use super::icons;
use super::message_text::Look;
use super::theme::{Palette, font};
use super::thread::thread_panel;
use super::timeline::{Context, timeline};
use crate::i18n::{t, tf, tp};
use crate::state::{AccountData, ComposerTarget, EmojiFor, PaneLayout, Popover, RichText, SidePanel, State};

pub fn room_pane(s: &State, account: &AccountData, p: &Palette, ui: &mut Cx<State>, layout: &PaneLayout) -> El {
    let Some(room) = s.active_room_id().and_then(|id| account.room(id)) else {
        let window_width = ui.layout_size("root").map_or(1200.0, |size| size[0]);
        let width = window_width - super::rail::WIDTH - layout.sidebar.map_or(0.0, |size| size.width);
        return super::overview::overview(s, account, width, layout.sidebar.is_some(), p);
    };
    let key = TimelineKey::room(&room.room_id);
    let cx = Context { covered: s.timeline_covered(ComposerTarget::Room), ..Context::new(s, account, room, &key) };
    let mut main = col()
        .key("room")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .fill(p.bg)
        .border(if s.drop_hover { 2.0 } else { 0.0 }, p.accent)
        .child(header(s, account, room, p, layout));
    let mut chat = || {
        col()
            .key("room-chat")
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(timeline(&cx, account.timeline(&key), "timeline", &s.near_top, Some(&s.at_bottom), p))
            // A room that was replaced takes no more messages: the way to the new one shows instead.
            .child(super::admin::successor_banner(s, room, p).unwrap_or_else(|| {
                composer(s, ComposerTarget::Room, &tf!("composer.placeholder", room = inline_name(room)), p, ui)
            }))
    };
    let body = match s.call_here() {
        Some(call) if call.chat && call.chat_beside() => row()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(super::call::stage(s, account, call, p))
            .child(col().w(340.0).h_full().child(chat())),
        Some(call) if call.chat => col()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(
                col()
                    .w_full()
                    .h((call.room_size[1] * 0.6).clamp(180.0, if call.video() { 280.0 } else { 220.0 }))
                    .child(super::call::stage(s, account, call, p)),
            )
            .child(chat()),
        Some(call) => super::call::stage(s, account, call, p),
        None => chat(),
    };
    main = main.child(
        col()
            .key(
                s.call_here()
                    .map_or_else(|| "room-body".to_owned(), |call| format!("room-body-call-{}", call.info.call)),
            )
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(body)
            .on_layout_tx(|s: &mut State, rect, tx| {
                let size = [rect[2], rect[3]];
                match s.call.as_mut() {
                    Some(call) if call.room_size != size => call.room_size = size,
                    _ => tx.live_only(),
                }
                s.sync_watching();
            }),
    );
    let main = main
        .on_drop_files(|s: &mut State, files, _| s.attach_files_to(ComposerTarget::Room, files))
        .on_drop_hover(|s: &mut State, over, _| s.drop_hover = over);
    let pane = row().grow(1.0).h_full().min_w(0.0).child(main);
    let panel = match &s.side {
        Some(SidePanel::Info) => Some(super::info::overview(s, room, p)),
        Some(SidePanel::Person(person)) => Some(super::members::person(s, account, room, person, p)),
        Some(SidePanel::Members) => Some(super::members::panel(s, account, room, p)),
        Some(SidePanel::Threads) => Some(super::info::threads(account, room, p)),
        Some(SidePanel::Pins) => Some(super::info::pins(s, account, room, p)),
        Some(SidePanel::Log) => Some(super::info::log(account, room, p)),
        Some(SidePanel::Files) => Some(super::info::files(s, account, room, p)),
        Some(SidePanel::Widgets) => Some(super::developer::widgets(s, account, room, p)),
        None => s.active_thread().map(|root| thread_panel(s, account, room, root, p, ui)),
    };
    pane.children(panel.zip(layout.right).map(|(panel, (which, size))| super::panes::resizable(panel, which, size, p)))
}

/// The event whose reaction picker is open in this timeline, for keeping its toolbar shown.
pub fn picking<'a>(s: &'a State, key: &TimelineKey) -> Option<&'a str> {
    match &s.popover {
        Some(
            Popover::Emoji(EmojiFor::Reaction { key: open, event_id })
            | Popover::MessageMenu { key: open, event_id, .. },
        ) if open == key => Some(event_id),
        _ => None,
    }
}

/// The room's name inside a sentence: `#general`, or a person's name.
pub fn inline_name(room: &RoomSummary) -> String {
    match room.kind {
        RoomKind::Group => format!("#{}", room.name),
        RoomKind::Direct => room.name.clone(),
    }
}

pub fn display_name(room: &RoomSummary) -> String {
    match room.kind {
        RoomKind::Group => format!("# {}", room.name),
        RoomKind::Direct => room.name.clone(),
    }
}

/// The room's name beside its topic's first line (a direct chat's presence instead), then its
/// members, its pinned messages when it has any, and its info at the right.
fn header(s: &State, account: &AccountData, room: &RoomSummary, p: &Palette, layout: &PaneLayout) -> El {
    let info_open = s.side == Some(SidePanel::Info);
    let name = text(display_name(room)).key("room-name").font(font::HEADER).color(p.text).shrink().truncate();
    // A direct chat says what its person shows: their dot, then "Away · 🌴 On vacation".
    let shown = super::sidebar::dm_partner(room).and_then(|id| account.presence.get(id));
    let presence = shown.and_then(|shown| {
        let words = super::components::presence_text(shown)?;
        Some(
            row()
                .key("room-presence")
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .center_y()
                .children(super::components::presence_dot(shown.presence, 9.0, p.bg, p))
                .child(text(words).font(font::BODY).color(p.text_muted).shrink().truncate()),
        )
    });
    let line = presence.or_else(|| {
        let topic = topic_line(room.topic.as_deref()?)?;
        let shown = match topic_text(topic) {
            Some(edit) => {
                let look = topic_look(s, account, room);
                super::message_text::line("room-topic", &edit.edit, look, font::META, p.text_muted, p)
            }
            None => text(topic.to_owned()).key("room-topic").font(font::META).color(p.text_muted).truncate(),
        };
        Some(shown.grow(1.0).min_w(0.0))
    });
    // The note to self says who sees it, unless its topic says more.
    let line = line.or_else(|| {
        room.note_to_self.then(|| {
            text(t!("note_to_self.private"))
                .key("room-private")
                .font(font::META)
                .color(p.text_muted)
                .grow(1.0)
                .min_w(0.0)
                .truncate()
        })
    });
    let pins = (!room.pinned.is_empty()).then(|| {
        icon_button(&icons::PIN, t!("info.pins"), s.side == Some(SidePanel::Pins), p.text_muted, p)
            .key(PINS)
            .help(t!("info.pins"))
            .on_press(|s: &mut State| s.toggle_side(SidePanel::Pins))
    });
    // The room's widgets, while it has any (a meeting's Join pill aside).
    let widgets = widget_count(account, room).filter(|count| *count > 0).map(|count| {
        let open = s.side == Some(SidePanel::Widgets);
        icon_button(&icons::LAYOUT, t!("widgets.title"), open, p.text_muted, p)
            .key(WIDGETS)
            .help(tp!("widgets.count", count))
            .on_press(|s: &mut State| s.toggle_side(SidePanel::Widgets))
    });
    row()
        .key("room-header")
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .px(20.0)
        .gap(10.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .children(s.call_here().filter(|_| layout.sidebar_available).map(|call| super::call::sidebar_button(call, p)))
        // Beside the line, the name keeps its width and the line takes the rest.
        .child(if line.is_some() { name } else { name.grow(1.0) })
        .children(line)
        .children(super::call::header_buttons(s, room, p))
        .children((room.kind == RoomKind::Group).then(|| super::people::members_button(s, account, room, p)))
        .children(pins)
        .children(widgets)
        .child(
            icon_button(&icons::INFO, t!("room.info"), info_open, p.text_muted, p)
                .key(super::people::INFO)
                .on_press(|s: &mut State| s.toggle_side(SidePanel::Info)),
        )
}

/// The header's pinned-messages button, shown while the room has pins.
pub const PINS: &str = "room-pins";
/// The header's widgets button.
pub const WIDGETS: &str = "room-widgets";

/// How many widgets the room has, once known.
pub fn widget_count(account: &AccountData, room: &RoomSummary) -> Option<usize> {
    account.widgets.get(&room.room_id).map(|listed| listed.widgets.len())
}

/// The topic's first non-blank line, which the header shows beside the room's name.
pub fn topic_line(topic: &str) -> Option<&str> {
    topic.lines().map(str::trim).find(|line| !line.is_empty())
}

/// A topic as shown: its Markdown formatted and its web addresses linked. `None` for plain
/// words, which show as they are. Worked out once per topic, not on every frame.
pub fn topic_text(topic: &str) -> Option<Rc<RichText>> {
    thread_local! {
        static TOPICS: RefCell<HashMap<String, Option<Rc<RichText>>>> = RefCell::new(HashMap::new());
    }
    TOPICS.with_borrow_mut(|topics| {
        if let Some(known) = topics.get(topic) {
            return known.clone();
        }
        // Topics change rarely; a full cache is just started again.
        if topics.len() >= 64 {
            topics.clear();
        }
        let edit = crate::state::rich_body(topic, None).map(Rc::new);
        topics.insert(topic.to_owned(), edit.clone());
        edit
    })
}

/// What a topic's mentions and custom emoji need to draw: the room's spoilers and the reader.
pub fn topic_look<'a>(s: &'a State, account: &'a AccountData, room: &'a RoomSummary) -> Look<'a> {
    Look {
        event_id: &room.room_id,
        me: account.summary.id.as_str(),
        revealed: &s.revealed,
        media: &s.media,
        pictures: true,
    }
}

pub fn empty_state(message: &str, p: &Palette) -> El {
    col().grow(1.0).h_full().fill(p.bg).center().child(text(message.to_owned()).font(font::BODY).color(p.text_muted))
}
