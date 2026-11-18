//! Message read positions, with the same avatars and member actions as the rest of the room.

use matrix_core::{Message, TimelineKey};
use rune::{Align, Cursor, El, ScrollSpec, Side, col, el, row, text};

use super::components::{media_avatar, popover_panel, when};
use super::theme::{Palette, font};
use crate::i18n::tp;
use crate::state::{AccountData, MediaCache, ReaderMenu, State};

const ROW: f32 = 52.0;
/// A reader's picture in the indicator, and how much the next one overlaps it.
const FACE: f32 = 18.0;
const FACE_OVERLAP: f32 = 6.0;
/// Room for the overflow count ("+99") after the pictures.
const COUNT: f32 = 22.0;
/// The pictures a narrow timeline shows, and the width below which it is narrow.
pub const NARROW_FACES: usize = 2;
pub const NARROW_TIMELINE: f32 = 640.0;

/// The width every room message keeps free at its right for an indicator of up to `faces`
/// pictures, whether or not it has one, so a receipt never rewraps or grows its message.
pub fn column_width(faces: usize) -> f32 {
    FACE + faces.saturating_sub(1) as f32 * (FACE - FACE_OVERLAP) + 6.0 + COUNT + 8.0
}

fn indicator_key(key: &TimelineKey, event_id: &str) -> String {
    format!("readers-{}-{event_id}", key.room_id)
}

fn reader_key(user_id: &str) -> String {
    format!("reader-{user_id}")
}

/// The read indicator, up to `faces` pictures and how many more, for the column at the lower
/// right of its message's row ([`column_width`]), no taller than a line of text.
pub fn indicator(
    account: &AccountData,
    key: &TimelineKey,
    message: &Message,
    faces: usize,
    media: &MediaCache,
    p: &Palette,
) -> El {
    let mut stack = row().center_y();
    for (index, receipt) in message.read_receipts.iter().take(faces).enumerate() {
        let person = account.reader(&key.room_id, &receipt.user_id);
        let face = media_avatar(&person.id, &person.name, person.avatar.as_ref(), media, FACE)
            .radius(FACE / 2.0)
            .border(1.5, p.bg);
        stack = stack.child(if index == 0 { face } else { face.ml(-FACE_OVERLAP) });
    }
    let label = tp!("message.seen_by", message.read_receipts.len());
    let (target, event_id) = (key.clone(), message.event_id.clone());
    let mut button = row()
        .key(indicator_key(key, &event_id))
        .gap(6.0)
        .center_y()
        .px(4.0)
        .py(2.0)
        .radius(5.0)
        .hover(p.hover)
        .role("button")
        .cursor(Cursor::Pointer)
        .focusable(true)
        .label(label.clone())
        .help(label)
        .child(stack);
    if message.read_receipts.len() > faces {
        button = button
            .child(text(format!("+{}", message.read_receipts.len() - faces)).font(font::META).color(p.text_muted));
    }
    button.on_press(move |s: &mut State| s.toggle_readers(&target, &event_id))
}

pub fn panels(
    s: &State,
    account: &AccountData,
    key: &TimelineKey,
    event_id: &str,
    member: Option<&ReaderMenu>,
    p: &Palette,
) -> Vec<El> {
    if s.active_room_id() != Some(key.room_id.as_str()) {
        return Vec::new();
    }
    let readers = account.readers(key, event_id);
    if readers.is_empty() {
        return Vec::new();
    }
    let (target, id, palette) = (key.clone(), event_id.to_owned(), *p);
    let (row_target, row_id) = (target.clone(), id.clone());
    let list = col()
        .key("readers-list")
        .w_full()
        .growth(rune::GrowthSpec::default())
        .virtual_rows(
            readers.len(),
            ROW,
            move |s: &State, index| {
                let user = s
                    .active_account()
                    .and_then(|a| a.readers(&target, &id).get(index))
                    .map(|r| r.user_id.as_str())
                    .unwrap_or_default();
                std::borrow::Cow::Owned(reader_key(user))
            },
            move |s: &State, index| {
                let Some(account) = s.active_account() else {
                    return el();
                };
                let Some(receipt) = account.readers(&row_target, &row_id).get(index) else {
                    return el();
                };
                let person = account.reader(&row_target.room_id, &receipt.user_id);
                let mut details = col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(2.0)
                    .child(text(person.name.clone()).font(font::BODY).color(palette.text).truncate());
                if let Some(at) = receipt.timestamp_ms {
                    details = details.child(
                        text(when(at))
                            .key(format!("reader-time-{}", person.id))
                            .font(font::META)
                            .color(palette.text_muted),
                    );
                }
                let user_id = person.id.clone();
                row()
                    .key(reader_key(&person.id))
                    .w_full()
                    .h(ROW)
                    .px(12.0)
                    .gap(10.0)
                    .center_y()
                    .radius(6.0)
                    .hover(palette.hover)
                    .role("button")
                    .cursor(Cursor::Pointer)
                    .focusable(true)
                    .label(person.name.clone())
                    .help(person.id.clone())
                    .child(media_avatar(&person.id, &person.name, person.avatar.as_ref(), &s.media, 32.0).radius(16.0))
                    .child(details)
                    .on_press(move |s: &mut State| s.toggle_reader_menu(&user_id))
            },
        )
        .on_visible_rows(|s: &mut State, range| s.readers_shown(range));
    let panel = popover_panel(p)
        .key("readers-panel")
        .w(280.0)
        .child(text(tp!("message.seen_by", readers.len())).font(font::BODY_STRONG).color(p.text).px(12.0).py(8.0))
        .child(col().w_full().max_h(320.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    let mut panels = vec![col().child(panel).anchored(indicator_key(key, event_id), Side::Top, Align::End, 6.0)];
    if let Some(menu) = member.filter(|menu| readers.iter().any(|r| r.user_id == menu.user_id)) {
        let person = account.reader(&key.room_id, &menu.user_id);
        panels.extend(super::members::menu(
            s,
            account,
            &menu.user_id,
            menu.submenu,
            &reader_key(&menu.user_id),
            Some(&person),
            p,
        ));
    }
    panels
}
