//! What the main pane shows when no room is open: the current scope at a glance.

use matrix_core::{Message, RoomKind, RoomSummary};
use rune::{El, Icon, ScrollSpec, col, el, icon, row, text};

use super::components::{account_avatar, badge, clock_time, local_ms, media_avatar};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf, tp};
use crate::state::{AccountData, Scope, State};

pub fn overview(s: &State, account: &AccountData, width: f32, sidebar_shown: bool, p: &Palette) -> El {
    let scope = s.active_scope();
    let data = account.overview(&scope);
    let [gutter, inset] = if width >= 700.0 { [32.0, 28.0] } else { [16.0, 18.0] };
    let home = scope == Scope::Home;
    let (title, greeting, detail, avatar) = match &scope {
        Scope::Home => (
            t!("common.home").to_owned(),
            tf!("overview.welcome", name = s.settings.person_name(&account.summary.display_name)),
            account.summary.id.to_string(),
            account_avatar(&account.summary, &s.media, 52.0),
        ),
        Scope::Space(id) => {
            let space = account.space(id);
            let name = space.map_or_else(|| id.clone(), |space| space.name.clone());
            let members = space.map_or(0, |space| space.members.len());
            (
                name.clone(),
                name.clone(),
                format!("{} · {}", tp!("overview.rooms", data.rooms), tp!("overview.members", members)),
                media_avatar(id, &name, space.and_then(|space| space.avatar.as_ref()), &s.media, 52.0),
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
        .child(text(title).font(font::HEADER).color(p.text).truncate());

    let mut hero = col()
        .w_full()
        .padding(inset)
        .gap(18.0)
        .radius(16.0)
        .border(1.0, p.accent.alpha(0.22))
        .gradient(p.surface.mix(p.accent, 0.16), p.bg.mix(p.accent, 0.07))
        .child(
            row().w_full().gap(14.0).center_y().child(avatar).child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(5.0)
                    .child(text(greeting).key("overview-hero").font(font::TITLE).color(p.text).wrap())
                    .child(text(detail).font(font::BODY).color(p.text_muted).truncate()),
            ),
        );
    if home {
        let actions = if width >= 420.0 { row() } else { col() };
        hero = hero.child(
            actions
                .gap(8.0)
                .items_start()
                .children(sidebar_shown.then(|| {
                    shortcut("overview-new-chat", &icons::CHAT, t!("create.chat"), p).on_press(State::open_new_chat)
                }))
                .child(shortcut("overview-saved", &icons::BOOKMARK, t!("saved.title"), p).on_press(State::open_saved)),
        );
    }

    let unread = if data.unread.is_empty() {
        empty_row(&icons::CHECK, t!("overview.caught_up"), t!("overview.caught_up_hint"), p)
    } else {
        data.unread.iter().fold(col().w_full(), |list, room| {
            let message = account.last_message(&room.room_id);
            list.child(room_card(room, message, message.map(|last| last.timestamp_ms), true, s, account, p))
        })
    };
    let unread_count = u64::try_from(data.unread.len()).unwrap_or_default();
    let unread = section(&icons::UNREAD, t!("overview.unread"), unread_count, unread, p);

    let recent = if data.recent.is_empty() {
        empty_row(&icons::HISTORY, t!("overview.no_recent"), t!("overview.no_recent_hint"), p)
    } else {
        data.recent.iter().fold(col().w_full(), |list, entry| {
            list.child(room_card(
                entry.room,
                entry.message,
                (entry.activity > 0).then_some(entry.activity),
                false,
                s,
                account,
                p,
            ))
        })
    };
    let recent = section(&icons::CLOCK, t!("overview.recent"), 0, recent, p);
    let activity = if width >= 760.0 {
        row().w_full().gap(18.0).items_start().child(unread.grow(1.0).min_w(0.0)).child(recent.grow(1.0).min_w(0.0))
    } else {
        col().w_full().gap(16.0).child(unread).child(recent)
    };

    let mut body = col().w_full().max_w(1180.0).padding(gutter).gap(22.0).child(hero);
    if home {
        body = body.children(super::room_discovery::home_invitations(s, account, p));
    }
    body = body.child(activity);
    if let Scope::Space(space) = &scope {
        body = body.child(super::room_discovery::suggestions(s, account, space, p));
    }

    col().key("overview").grow(1.0).h_full().min_w(0.0).fill(p.bg).child(header).child(
        col()
            .key("overview-body")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .items_center()
            .scroll(ScrollSpec { stick_bottom: false })
            .child(body),
    )
}

fn shortcut(key: &str, glyph: &'static Icon, title: &str, p: &Palette) -> El {
    row()
        .key(key.to_owned())
        .min_h(36.0)
        .px(12.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .hover(p.hover)
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(title.to_owned())
        .child(icon(glyph, 16.0).color(p.accent))
        .child(text(title.to_owned()).font(font::BODY_STRONG).color(p.text))
}

fn section(glyph: &'static Icon, title: &str, count: u64, content: El, p: &Palette) -> El {
    col()
        .w_full()
        .padding(8.0)
        .gap(3.0)
        .radius(12.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .child(
            row()
                .w_full()
                .center_y()
                .gap(8.0)
                .px(8.0)
                .py(8.0)
                .child(icon(glyph, 17.0).color(p.accent))
                .child(text(title.to_owned()).font(font::HEADER).color(p.text).grow(1.0).min_w(0.0).wrap())
                .children((count > 0).then(|| {
                    text(count.to_string())
                        .font(font::BADGE)
                        .color(p.text)
                        .px(7.0)
                        .py(3.0)
                        .radius(8.0)
                        .fill(p.accent.alpha(0.16))
                })),
        )
        .child(content)
}

fn empty_row(glyph: &'static Icon, title: &str, hint: &str, p: &Palette) -> El {
    row()
        .w_full()
        .padding(11.0)
        .gap(11.0)
        .radius(8.0)
        .fill(p.bg)
        .center_y()
        .child(
            el().size(34.0, 34.0)
                .radius(9.0)
                .fill(p.accent.alpha(0.12))
                .center()
                .child(icon(glyph, 18.0).color(p.accent)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(text(title.to_owned()).font(font::BODY_STRONG).color(p.text).wrap())
                .child(text(hint.to_owned()).font(font::META).color(p.text_muted).wrap()),
        )
}

/// The room name, latest message and attention count. Opens the room in its scope.
fn room_card(
    room: &RoomSummary,
    last: Option<&Message>,
    activity: Option<u64>,
    unread: bool,
    s: &State,
    account: &AccountData,
    p: &Palette,
) -> El {
    let mark = if room.note_to_self {
        room_mark(icon(&icons::NOTE, 18.0).color(p.accent), p)
    } else if room.avatar.is_some() || room.kind == RoomKind::Direct {
        media_avatar(
            room.dm_users.first().unwrap_or(&room.room_id),
            &s.settings.room_name(room),
            room.avatar.as_ref(),
            &s.media,
            34.0,
        )
    } else {
        room_mark(text("#").font(font::HEADER).color(p.accent), p)
    };
    let name = if room.note_to_self { t!("note_to_self.name").into() } else { s.settings.room_name(room) };
    let mut titles =
        col().grow(1.0).min_w(0.0).gap(3.0).child(text(name.as_ref()).font(font::BODY_STRONG).color(p.text).truncate());
    if let Some(message) = last {
        let body = match &message.content {
            matrix_core::MessageContent::Call(card) => std::borrow::Cow::Borrowed(crate::state::call_title(card)),
            content => content.plain(),
        };
        let preview = if super::sidebar::dm_partner(room) == Some(message.sender_id.as_str()) {
            body.into_owned()
        } else {
            let sender = if message.sender_id == account.summary.id.as_str() {
                t!("overview.you").into()
            } else {
                s.settings.person_name(&message.sender_name)
            };
            tf!("overview.preview", sender = sender, body = body)
        };
        titles = titles.child(text(preview).font(font::META).color(p.text_muted).truncate());
    }
    let time = activity.map(|at| text(clock_time(local_ms(at))).font(font::META).color(p.text_muted));
    let counts = unread.then(|| badge(room.mentions, p)).flatten();
    let room_id = room.room_id.clone();
    row()
        .key(format!("overview-{}{}", if unread { "unread-" } else { "recent-" }, room.room_id))
        .w_full()
        .px(8.0)
        .py(8.0)
        .gap(10.0)
        .center_y()
        .radius(7.0)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(name)
        .focusable(true)
        .child(mark)
        .child(titles)
        .children(time)
        .children(counts)
        .child(icon(&icons::CHEVRON_RIGHT, 14.0).color(p.text_muted))
        .on_press_tx(crate::state::navigate(move |s| s.select_room(&room_id)))
}

fn room_mark(content: El, p: &Palette) -> El {
    el().size(34.0, 34.0).radius(8.0).fill(p.accent.alpha(0.12)).center().child(content)
}
