//! What the main pane shows when no room is open: the current scope at a glance. Home greets the
//! user; a space shows its size. Then the rooms wanting attention, and recent conversations.

use matrix_core::{Message, RoomKind, RoomSummary};
use rune::{El, ScrollSpec, col, el, icon, row, text};

use super::components::{badge, clock_time, local_ms, media_avatar};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf, tp};
use crate::state::{AccountData, Scope, State};

pub fn overview(s: &State, account: &AccountData, p: &Palette) -> El {
    let scope = s.active_scope();
    let data = account.overview(&scope);
    let (title, hero, subtitle) = match &scope {
        Scope::Home => (
            t!("common.home").to_owned(),
            tf!("overview.welcome", name = account.summary.display_name),
            account.summary.id.to_string(),
        ),
        Scope::Space(id) => {
            let space = account.space(id);
            let name = space.map_or_else(|| id.clone(), |space| space.name.clone());
            let members = space.map_or(0, |space| space.members.len());
            (
                name.clone(),
                name,
                format!("{} · {}", tp!("overview.rooms", data.rooms), tp!("overview.members", members)),
            )
        }
    };

    let header = row()
        .key("overview-header")
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .px(20.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(title).font(font::HEADER).color(p.text));

    let mut body = col().w_full().max_w(760.0).padding(32.0).gap(32.0).child(
        col()
            .gap(6.0)
            .child(text(hero).key("overview-hero").font(font::TITLE).color(p.text).wrap())
            .child(text(subtitle).font(font::BODY).color(p.text_muted)),
    );

    let unread = if data.unread.is_empty() {
        row()
            .key("caught-up")
            .w_full()
            .padding(16.0)
            .gap(14.0)
            .center_y()
            .radius(10.0)
            .fill(p.surface)
            .border(1.0, p.divider)
            .child(
                el().size(36.0, 36.0)
                    .radius(18.0)
                    .fill(p.accent.alpha(0.15))
                    .center()
                    .child(icon(&icons::CHECK, 20.0).color(p.accent)),
            )
            .child(
                col()
                    .gap(2.0)
                    .child(text(t!("overview.caught_up")).font(font::BODY_STRONG).color(p.text))
                    .child(text(t!("overview.caught_up_hint")).font(font::META).color(p.text_muted)),
            )
    } else {
        data.unread.iter().fold(col().w_full().gap(6.0), |list, room| {
            list.child(room_card(room, account.last_message(&room.room_id), true, &s.media, p))
        })
    };
    if scope == Scope::Home {
        body = body.children(super::room_discovery::home_invitations(s, account, p));
    }
    body = body.child(section(t!("overview.unread"), unread, p));

    if let Scope::Space(space) = &scope {
        body = body.child(super::room_discovery::suggestions(s, account, space, p));
    }
    if !data.recent.is_empty() {
        let recent = data.recent.iter().fold(col().w_full().gap(6.0), |list, (room, message)| {
            list.child(room_card(room, Some(message), false, &s.media, p))
        });
        body = body.child(section(t!("overview.recent"), recent, p));
    }

    col().key("overview").grow(1.0).h_full().min_w(0.0).fill(p.bg).child(header).child(
        col()
            .key("overview-body")
            .w_full()
            .grow(1.0)
            .shrink()
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(body),
    )
}

fn section(title: &str, content: El, p: &Palette) -> El {
    col().w_full().gap(10.0).child(text(title.to_owned()).font(font::SECTION).color(p.text_muted)).child(content)
}

/// One room: its mark, name, latest message and, when unread, the mention badge. Opens the room.
fn room_card(
    room: &RoomSummary,
    last: Option<&Message>,
    unread: bool,
    media: &crate::state::MediaCache,
    p: &Palette,
) -> El {
    let mark = match room.kind {
        RoomKind::Group => el()
            .size(32.0, 32.0)
            .radius(7.0)
            .fill(p.surface)
            .center()
            .child(text("#").font(font::HEADER).color(p.text_muted)),
        RoomKind::Direct => {
            media_avatar(room.dm_users.first().unwrap_or(&room.room_id), &room.name, room.avatar.as_ref(), media, 32.0)
        }
    };
    let mut titles = col().grow(1.0).min_w(0.0).gap(2.0).child(
        text(room.name.clone()).font(if unread { font::BODY_STRONG } else { font::BODY }).color(p.text).truncate(),
    );
    if let Some(message) = last {
        let body = match &message.content {
            matrix_core::MessageContent::Call(card) => std::borrow::Cow::Borrowed(crate::state::call_title(card)),
            content => content.plain(),
        };
        let preview = tf!("overview.preview", sender = message.sender_name, body = body);
        titles = titles.child(text(preview).font(font::META).color(p.text_muted).truncate());
    }
    let mut card = row()
        .key(format!("overview-{}{}", if unread { "unread-" } else { "recent-" }, room.room_id))
        .w_full()
        .px(14.0)
        .py(10.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(room.name.clone())
        .focusable(true)
        .child(mark)
        .child(titles);
    if let Some(message) = last {
        card = card.child(text(clock_time(local_ms(message.timestamp_ms))).font(font::META).color(p.text_muted));
    }
    if unread && let Some(mentions) = badge(room.mentions, p) {
        card = card.child(mentions);
    }
    let room_id = room.room_id.clone();
    card.on_press_tx(crate::state::navigate(move |s| s.select_room(&room_id)))
}
