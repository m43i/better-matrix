//! The open room: header, timeline and composer, with the open thread beside it. Files dropped
//! on the room attach to its composer.

use matrix_core::{RoomKind, RoomSummary, TimelineKey};
use rune::{Cx, El, col, row, text};

use super::components::icon_button;
use super::composer::composer;
use super::icons;
use super::theme::{Palette, font};
use super::thread::thread_panel;
use super::timeline::{Context, timeline};
use crate::i18n::{t, tf};
use crate::state::{AccountData, ComposerTarget, EmojiFor, PaneLayout, Popover, SidePanel, State};

pub fn room_pane(s: &State, account: &AccountData, p: &Palette, ui: &mut Cx<State>, layout: &PaneLayout) -> El {
    let Some(room) = s.active_room_id().and_then(|id| account.room(id)) else {
        return super::overview::overview(s, account, p);
    };
    let key = TimelineKey::room(&room.room_id);
    let cx = Context::new(s, account, room, &key);
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
            .child(composer(s, ComposerTarget::Room, &tf!("composer.placeholder", room = inline_name(room)), p, ui))
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
        Some(SidePanel::Files) => Some(super::info::files(s, account, room, p)),
        Some(SidePanel::Widgets) => Some(super::developer::widgets(account, room, p)),
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

/// The room's name, then its members and its info at the right. The topic is behind the info
/// button rather than in the header.
fn header(s: &State, account: &AccountData, room: &RoomSummary, p: &Palette, layout: &PaneLayout) -> El {
    let info_open = s.side == Some(SidePanel::Info);
    let name = text(display_name(room)).key("room-name").font(font::HEADER).color(p.text).shrink().truncate();
    // A direct chat says what its person shows: their dot, then "Away · 🌴 On vacation".
    let shown = super::sidebar::dm_partner(room).and_then(|id| account.presence.get(id));
    let line = shown.and_then(|shown| {
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
    row()
        .key("room-header")
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .px(20.0)
        .gap(10.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .children(s.call_here().filter(|_| layout.sidebar_available).map(|call| super::call::sidebar_button(call, p)))
        // Beside the presence line, the name keeps its width and the line takes the rest.
        .child(if line.is_some() { name } else { name.grow(1.0) })
        .children(line)
        .children(super::call::header_buttons(s, room, p))
        .children((room.kind == RoomKind::Group).then(|| super::people::members_button(s, account, room, p)))
        .child(
            icon_button(&icons::INFO, t!("room.info"), info_open, p.text_muted, p)
                .key(super::people::INFO)
                .on_press(|s: &mut State| s.toggle_side(SidePanel::Info)),
        )
}

pub fn empty_state(message: &str, p: &Palette) -> El {
    col().grow(1.0).h_full().fill(p.bg).center().child(text(message.to_owned()).font(font::BODY).color(p.text_muted))
}
