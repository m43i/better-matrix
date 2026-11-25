//! Finding and gathering people: the top bar with its search, the open room's member list and
//! info, and the invite dialog.

use matrix_core::{Person, RoomSummary};
use rune::{ChromeInsets, Color, El, ScrollSpec, col, el, icon, row, text, text_input};

use super::components::{ButtonKind, button, dialog, media_avatar};
use super::icons;
use super::room::display_name;
use super::search::highlighted;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, MediaCache, State};

/// The search field's key.
const SEARCH: &str = "top-search";
/// The search field's frame, which its results anchor to when there is no banner.
pub(super) const SEARCH_ANCHOR: &str = "search-anchor";
/// The members button's key, which the member list anchors to.
pub const MEMBERS: &str = "room-members";
/// The info button's key, which the room's info anchors to.
pub const INFO: &str = "room-info";

/// The top bar's height; the search's backdrop starts below it.
pub const TOP_BAR_HEIGHT: f32 = 44.0;
/// The saved-messages button's key, beside the search.
pub const SAVED: &str = "top-saved";
/// The saved-messages button's width and its gap to the search; the search keeps its middle.
const SAVED_SPAN: f32 = 30.0 + 8.0;

/// The bar across the top of the window, with the search in its middle and the account's saved
/// messages beside it.
pub fn top_bar(s: &State, account: &AccountData, p: &Palette, chrome: ChromeInsets) -> El {
    let placeholder = tf!("search.placeholder", account = account.summary.display_name);
    let field = if s.popover == Some(crate::state::Popover::Search) {
        text_input(&s.search.field, font::BODY)
            .key(SEARCH)
            .single_line()
            .placeholder(placeholder, p.sidebar_text_muted)
            .color(p.sidebar_text)
            .grow(1.0)
            .on_edit(|s: &mut State| &mut s.search.field)
            .on_input_key(State::popover_input_key)
            // Enter opens a pasted invitation link or a typed address.
            .on_submit(|s: &mut State, tx| {
                let typed = s.search.query().to_owned();
                crate::state::navigate_now(s, tx, |s| s.open_typed_link(&typed));
            })
    } else {
        // The closed search is a button: text fields consume their own pointer presses.
        let empty = s.search.query().is_empty();
        let label = if empty { placeholder.as_str() } else { s.search.field.text.as_str() };
        row()
            .key(SEARCH)
            .grow(1.0)
            .h_full()
            .center_y()
            .role("button")
            .label(placeholder.clone())
            .focusable(true)
            .cursor(rune::Cursor::Pointer)
            .child(
                text(label.to_owned())
                    .font(font::BODY)
                    .color(if empty { p.sidebar_text_muted } else { p.sidebar_text })
                    .truncate(),
            )
            .on_press(State::open_search)
    };
    let search = row()
        .key(SEARCH_ANCHOR)
        .on_press(State::open_search)
        .grow(1.0)
        .min_w(0.0)
        .max_w(640.0)
        .h(30.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .fill(p.sidebar_hover)
        .border(1.0, p.sidebar_divider)
        .child(icon(&icons::SEARCH, 16.0).color(p.sidebar_text_muted))
        .children(super::search::active_filter(s, p))
        .child(field)
        .child(text(rune::Keybind::new("mod+k").label()).font(font::META).color(p.sidebar_text_muted));
    // Keep search centered. The spacer already clears part of the left inset.
    let padding = 16.0 + (chrome.leading - SAVED_SPAN).max(chrome.trailing);
    row()
        .key("top-bar")
        .w_full()
        .h(TOP_BAR_HEIGHT)
        .px(padding)
        .center()
        .fill(p.rail_bg)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.sidebar_divider)
        .child(el().w(SAVED_SPAN).h(30.0))
        .child(search)
        .child(saved_button(s, p))
}

/// Opens the account's saved messages; lit while they show.
fn saved_button(s: &State, p: &Palette) -> El {
    let open = matches!(s.modal, Some(crate::state::Modal::Saved(_)));
    el().key(SAVED)
        .ml(8.0)
        .size(30.0, 30.0)
        .radius(8.0)
        .center()
        .fill(if open { p.sidebar_hover.alpha(p.sidebar_hover.a * 2.0) } else { Color::TRANSPARENT })
        .hover(p.sidebar_hover)
        .role("button")
        .label(t!("saved.title").to_owned())
        .help(t!("saved.title"))
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .child(icon(&icons::BOOKMARK, 18.0).color(if open { p.sidebar_text } else { p.sidebar_text_muted }))
        .on_press(State::open_saved)
}

/// One result or member: a picture, a name, and a line under it.
pub(super) fn person_row(key: String, person: &Person, media: &MediaCache, p: &Palette, query: &str) -> El {
    row()
        .key(key)
        .w_full()
        .px(10.0)
        .py(6.0)
        .gap(10.0)
        .center_y()
        .radius(6.0)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .child(media_avatar(&person.id, &person.name, person.avatar.as_ref(), media, 28.0))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(highlighted(&person.name, query, p))
                .child(highlighted(&person.id, query, p).font(font::META).color(p.text_muted)),
        )
}

/// The members button in a room's header: up to three faces and the count.
pub fn members_button(s: &State, account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let count = account.member_count(room);
    let mut faces = row().center_y();
    if let Some(members) = account.members.get(&room.room_id) {
        for (index, person) in members.people.iter().take(3).map(|member| &member.person).enumerate() {
            let face = media_avatar(&person.id, &person.name, person.avatar.as_ref(), &s.media, 22.0)
                .radius(6.0)
                .border(1.5, p.bg);
            faces = faces.child(if index == 0 { face } else { face.ml(-6.0) });
        }
    }
    let open = s.side == Some(crate::state::SidePanel::Members);
    row()
        .key(MEMBERS)
        .h(32.0)
        .px(6.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .border(1.0, p.divider)
        .fill(if open { p.hover.alpha(p.hover.a * 2.0) } else { Color::TRANSPARENT })
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(tf!("members.count", count = count))
        .child(faces)
        .child(text(count.to_string()).font(font::BODY_STRONG).color(p.text))
        .on_press(State::toggle_members)
}

/// The invite dialog: search the directory (or type a user ID) and invite with a press.
pub fn invite_dialog(s: &State, account: &AccountData, room_id: &str, invited: &[String], p: &Palette) -> El {
    let room = account.room(room_id);
    let title = tf!("invite.title", room = room.map_or_else(|| room_id.to_owned(), display_name));
    let field = text_input(&s.invite_search.field, font::BODY)
        .key("invite-search")
        .single_line()
        .placeholder(t!("invite.placeholder"), p.text_muted)
        .color(p.text)
        .w_full()
        .px(12.0)
        .py(10.0)
        .radius(8.0)
        .fill(p.bg)
        .border(1.0, p.divider)
        .on_edit(|s: &mut State| &mut s.invite_search.field);
    let members = account.members.get(room_id);
    let is_member =
        |id: &str| members.is_some_and(|members| members.people.iter().any(|member| member.person.id == id));
    let mut list = col().w_full().gap(2.0);
    for person in &s.invite_search.people {
        let id = person.id.clone();
        let status = if is_member(&id) {
            Some(t!("invite.member"))
        } else if invited.contains(&id) {
            Some(t!("invite.invited"))
        } else {
            None
        };
        let trailing = match status {
            Some(status) => text(status).font(font::META).color(p.text_muted),
            None => button(t!("invite.invite"), ButtonKind::Secondary, true, p)
                .key(format!("invite-{id}"))
                .on_press(move |s: &mut State| s.invite(&id)),
        };
        list = list.child(
            person_row(format!("invitee-{}", person.id), person, &s.media, p, s.invite_search.query()).child(trailing),
        );
    }
    let content = col()
        .w_full()
        .gap(10.0)
        .child(field)
        .child(col().w_full().max_h(320.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    dialog(
        "invite-dialog",
        &title,
        content,
        vec![button(t!("common.done"), ButtonKind::Primary, true, p).key("invite-done").on_press(State::close_modal)],
        p,
    )
}
