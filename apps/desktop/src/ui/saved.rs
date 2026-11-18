//! The messages the user saved, from the account menu: a dialog listing them across the account's
//! rooms, most recently saved first. A row opens its message where it was sent; its bookmark
//! button stops saving it.

use matrix_core::RoomKind;
use rune::{El, col, row, text};

use super::components::{ButtonKind, button, dialog, icon_button, media_avatar, when};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, State};

/// The saved messages of `account`, which may not be the account on screen (a call answered for
/// another account switches it): every row acts on `account`'s own.
pub fn saved(s: &State, account: &AccountData, p: &Palette) -> El {
    let owner = &account.summary.id;
    let mut list = col().w_full();
    if let Some(error) = &account.saved_error {
        let failed = tf!("saved.failed", error = error);
        list = list.child(
            col().w_full().py(8.0).gap(8.0).child(text(failed).font(font::BODY).color(p.danger).wrap()).child(
                button(t!("saved.retry"), ButtonKind::Secondary, true, p)
                    .key("saved-retry")
                    .on_press(State::retry_saved),
            ),
        );
    }
    let messages: Vec<_> = account.saved_messages().collect();
    match &account.saved {
        None if account.saved_error.is_none() => {
            list = list.child(text(t!("info.loading")).font(font::BODY).color(p.text_muted).py(8.0));
        }
        Some(_) if messages.is_empty() => {
            list = list.child(text(t!("saved.empty")).font(font::BODY).color(p.text_muted).py(8.0).wrap());
        }
        _ => {}
    }
    for saved in messages {
        let message = &saved.preview;
        // Rooms read as the sidebar has them: `#general`, or a direct chat's person.
        let room = match account.room(&saved.room_id) {
            Some(room) if room.kind == RoomKind::Group => format!("#{}", room.name),
            Some(room) => room.name.clone(),
            None => saved.room_id.clone(),
        };
        let (removed, room_id, event_id) = (owner.clone(), saved.room_id.clone(), message.event_id.clone());
        let header = row()
            .w_full()
            .min_h(28.0)
            .gap(6.0)
            .center_y()
            .child(text(message.sender_name.clone()).font(font::BODY_STRONG).color(p.text).shrink().truncate())
            .child(text(tf!("saved.in_room", room = room)).font(font::META).color(p.text_muted).grow(1.0).truncate())
            .child(
                icon_button(&icons::UNBOOKMARK, t!("actions.unsave"), false, p.text_muted, p)
                    .key(format!("saved-remove-{room_id}-{event_id}"))
                    .help(t!("actions.unsave"))
                    .on_press(move |s: &mut State| s.set_message_saved(&removed, &room_id, &event_id, false)),
            );
        let mut body = col().grow(1.0).min_w(0.0).gap(3.0).child(header).child(
            text(message.body.clone().unwrap_or_else(|| t!("message.unavailable").to_owned()))
                .font(font::BODY)
                .color(if message.body.is_some() { p.text } else { p.text_muted })
                .max_lines(4)
                .wrap(),
        );
        if let Some(at) = message.timestamp_ms {
            body = body.child(text(when(at)).font(font::META).color(p.text_muted));
        }
        let mut entry = row()
            .key(format!("saved-open-{}-{}", saved.room_id, message.event_id))
            .w_full()
            .px(8.0)
            .py(10.0)
            .gap(10.0)
            .items_start()
            .radius(8.0)
            .child(
                media_avatar(&message.sender_id, &message.sender_name, message.sender_avatar.as_ref(), &s.media, 32.0)
                    .mt(3.0),
            )
            .child(body);
        if message.body.is_some() {
            let (opened, room_id, thread, event_id) =
                (owner.clone(), saved.room_id.clone(), message.thread.clone(), message.event_id.clone());
            entry = entry
                .role("button")
                .label(t!("message.open"))
                .focusable(true)
                .cursor(rune::Cursor::Pointer)
                .hover(p.hover)
                .on_press_tx(crate::state::navigate(move |s| {
                    s.open_saved_message(&opened, &room_id, thread.as_deref(), &event_id);
                }));
        }
        list = list.child(entry);
    }
    let close =
        button(t!("common.close"), ButtonKind::Secondary, true, p).key("saved-close").on_press(State::close_modal);
    dialog("saved-dialog", t!("saved.title"), list, vec![close], p)
}
