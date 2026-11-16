//! Finding and gathering people: the top bar with its search, the open room's member list and
//! info, and the invite dialog.

use matrix_core::{Person, RoomKind, RoomSummary};
use rune::{Align, Color, El, ScrollSpec, Side, col, el, icon, row, text, text_input};

use super::components::{ButtonKind, button, dialog, media_avatar, popover_panel};
use super::icons;
use super::room::display_name;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, MediaCache, State, matches};

/// The search field's key, which its results anchor to.
const SEARCH: &str = "top-search";
/// The members button's key, which the member list anchors to.
pub const MEMBERS: &str = "room-members";
/// The info button's key, which the room's info anchors to.
pub const INFO: &str = "room-info";
/// How many results of each kind the search lists.
const RESULTS: usize = 6;

/// The bar across the top of the window, with the search in its middle.
pub fn top_bar(s: &State, account: &AccountData, p: &Palette) -> El {
    let field = text_input(&s.search.field, font::BODY)
        .key(SEARCH)
        .single_line()
        .placeholder(tf!("search.placeholder", account = account.summary.display_name), p.sidebar_text_muted)
        .color(p.sidebar_text)
        .grow(1.0)
        .on_edit(|s: &mut State| &mut s.search.field);
    let search = row()
        .w_full()
        .max_w(640.0)
        .h(30.0)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .fill(p.sidebar_hover)
        .border(1.0, p.sidebar_divider)
        .child(icon(&icons::SEARCH, 16.0).color(p.sidebar_text_muted))
        .child(field);
    row()
        .key("top-bar")
        .w_full()
        .h(44.0)
        .px(16.0)
        .center()
        .fill(p.rail_bg)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.sidebar_divider)
        .child(search)
}

/// One result or member: a picture, a name, and a line under it.
fn person_row(key: String, person: &Person, media: &MediaCache, p: &Palette) -> El {
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
        .cursor(rune::Cursor::Pointer)
        .child(media_avatar(&person.id, &person.name, person.avatar.as_ref(), media, 28.0))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(person.name.clone()).font(font::BODY).color(p.text).truncate())
                .child(text(person.id.clone()).font(font::META).color(p.text_muted).truncate()),
        )
}

fn section(title: &str, p: &Palette) -> El {
    text(title.to_owned()).font(font::SECTION).color(p.text_muted).px(10.0).pt(8.0).pb(4.0)
}

/// The search's results under the field: rooms, direct chats and spaces on this device, then
/// people from the user directory.
pub fn search_results(s: &State, account: &AccountData, p: &Palette) -> El {
    let query = s.search.query();
    let mut list = col().w_full().px(6.0).pb(6.0);
    let mut any = false;
    let room_row = |room: &RoomSummary| {
        let id = room.room_id.clone();
        let mark = match room.kind {
            RoomKind::Group => el().size(28.0, 28.0).center().child(text("#").font(font::HEADER).color(p.text_muted)),
            RoomKind::Direct => media_avatar(
                room.dm_users.first().unwrap_or(&room.room_id),
                &room.name,
                room.avatar.as_ref(),
                &s.media,
                28.0,
            ),
        };
        row()
            .key(format!("found-room-{id}"))
            .w_full()
            .px(10.0)
            .py(6.0)
            .gap(10.0)
            .center_y()
            .radius(6.0)
            .hover(p.hover.alpha(p.hover.a * 2.0))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .child(mark)
            .child(text(display_name(room)).font(font::BODY).color(p.text).grow(1.0).truncate())
            .on_press_tx(crate::state::navigate(move |s| s.open_found_room(&id)))
    };
    for (kind, title) in [(RoomKind::Group, t!("search.rooms")), (RoomKind::Direct, t!("search.direct"))] {
        let found: Vec<&RoomSummary> =
            account.rooms.iter().filter(|room| room.kind == kind && matches(&room.name, query)).take(RESULTS).collect();
        if !found.is_empty() {
            any = true;
            list = list.child(section(title, p));
            for room in found {
                list = list.child(room_row(room));
            }
        }
    }
    let spaces: Vec<_> = account.spaces.iter().filter(|space| matches(&space.name, query)).take(RESULTS).collect();
    if !spaces.is_empty() {
        any = true;
        list = list.child(section(t!("search.spaces"), p));
        for space in spaces {
            let id = space.room_id.clone();
            list = list.child(
                person_row(
                    format!("found-space-{id}"),
                    &Person { id: space.room_id.clone(), name: space.name.clone(), avatar: space.avatar.clone() },
                    &s.media,
                    p,
                )
                .on_press_tx(crate::state::navigate(move |s| s.open_found_space(&id))),
            );
        }
    }
    let me = account.summary.id.as_str();
    let people: Vec<&Person> = s.search.people.iter().filter(|person| person.id != me).take(RESULTS).collect();
    if !people.is_empty() {
        any = true;
        list = list.child(section(t!("search.people"), p));
        for person in people {
            let id = person.id.clone();
            list = list.child(
                person_row(format!("found-person-{id}"), person, &s.media, p)
                    .on_press(move |s: &mut State| s.open_direct_chat(&id)),
            );
        }
    }
    if !any {
        let note = if s.search.request.is_some() { t!("search.searching") } else { t!("search.nothing") };
        list = list.child(text(note).font(font::BODY).color(p.text_muted).px(10.0).py(10.0));
    }
    let panel = popover_panel(p)
        .key("search-results")
        .w(560.0)
        .preserve_focus_on_press(true)
        .child(col().w_full().max_h(460.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    col().child(panel).anchored(SEARCH, Side::Bottom, Align::Center, 6.0)
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
        list = list.child(person_row(format!("invitee-{}", person.id), person, &s.media, p).child(trailing));
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
