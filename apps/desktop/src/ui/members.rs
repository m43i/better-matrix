//! The member list beside the open room, from the members button in its header: a search, the
//! count, and everyone (those around first, then by role, then by name) with their presence,
//! whether the user verified them, and their role. Pressing someone opens what can be done with
//! them: start a chat, call, invite to another room, and with the rights, change their role,
//! kick or ban.

use matrix_core::{MemberRole, Members, Moderation, Presence, RoomKind, RoomMember, RoomSummary};
use rune::{Align, El, ScrollSpec, Side, col, el, icon, row, text, text_input};

use super::components::{
    Tone, divider, icon_button, media_avatar, menu_action, menu_icon, menu_row, popover_panel, submenu_row,
};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, MemberSubmenu, State, matches};

/// A member row's height, for the list's estimate.
const ROW: f32 = 52.0;
/// The submenu rows' keys, which their submenus anchor to.
const INVITE_ROW: &str = "member-menu-invite";
const ROLE_ROW: &str = "member-menu-role";

/// Where the members the search lets through sit in the room's list, in its order. Worked out
/// once per build; the rows look members up by it.
fn shown(s: &State, room_id: &str) -> Vec<usize> {
    let query = s.member_search.text.trim();
    let id_query = query.to_lowercase();
    let people = s.active_account().and_then(|account| account.members.get(room_id)).map(|members| &members.people);
    people
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, member)| {
            query.is_empty()
                || matches(&member.person.name, query)
                || member.person.id.to_lowercase().contains(&id_query)
        })
        .map(|(index, _)| index)
        .collect()
}

/// The member at `index` of the room's list.
fn member_at<'a>(s: &'a State, room_id: &str, index: Option<&usize>) -> Option<&'a RoomMember> {
    s.active_account()?.members.get(room_id)?.people.get(*index?)
}

fn role_label(role: MemberRole) -> Option<&'static str> {
    match role {
        MemberRole::Admin => Some(t!("members.admin")),
        MemberRole::Moderator => Some(t!("members.moderator")),
        MemberRole::Member => None,
    }
}

pub fn panel(s: &State, account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let members = account.members.get(&room.room_id);
    let header = row()
        .key("members-header")
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .px(16.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(t!("members.title")).font(font::HEADER).color(p.text).grow(1.0))
        .child(
            icon_button(&icons::CLOSE, t!("members.close"), false, p.text_muted, p)
                .key("members-close")
                .on_press(State::toggle_members),
        );
    let field = text_input(&s.member_search, font::BODY)
        .key("member-search")
        .single_line()
        .placeholder(t!("members.search"), p.text_muted)
        .color(p.text)
        .grow(1.0)
        .on_edit(|s: &mut State| &mut s.member_search);
    let mut search = row().w_full().px(16.0).pt(14.0).gap(10.0).center_y().child(
        row()
            .grow(1.0)
            .min_w(0.0)
            .h(38.0)
            .px(14.0)
            .gap(8.0)
            .center_y()
            .radius(19.0)
            .border(1.0, p.divider)
            .child(icon(&icons::SEARCH, 16.0).color(p.text_muted))
            .child(field),
    );
    if members.is_some_and(|members| members.can_invite) {
        search = search.child(
            el().key("members-invite")
                .size(38.0, 38.0)
                .radius(19.0)
                .center()
                .fill(p.hover.alpha(p.hover.a * 2.0))
                .hover(p.hover.alpha(p.hover.a * 3.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .label(t!("members.invite").to_owned())
                .help(t!("members.invite").to_owned())
                .child(icon(&icons::USER_PLUS, 18.0).color(p.text_muted))
                .on_press(State::open_invite),
        );
    }
    let count = text(tf!("members.count", count = account.member_count(room)))
        .key("member-count")
        .font(font::SECTION)
        .color(p.text_muted)
        .px(16.0)
        .pt(16.0)
        .pb(8.0);
    let list = match members {
        None => col().w_full().child(text(t!("members.loading")).font(font::BODY).color(p.text_muted).px(16.0).py(8.0)),
        // Only the rows in view are built: a room may have thousands of members.
        Some(_) => {
            let (room_id, palette, me) = (room.room_id.clone(), *p, account.summary.id.to_string());
            let key_room = room_id.clone();
            let found = std::sync::Arc::new(shown(s, &room.room_id));
            let key_found = std::sync::Arc::clone(&found);
            col().key("member-list").w_full().growth(rune::GrowthSpec::default()).virtual_rows(
                found.len(),
                ROW,
                move |s: &State, i| {
                    let member = member_at(s, &key_room, key_found.get(i));
                    let id = member.map(|member| member.person.id.as_str()).unwrap_or_default();
                    std::borrow::Cow::Owned(format!("member-{id}"))
                },
                move |s: &State, i| match member_at(s, &room_id, found.get(i)) {
                    Some(member) => member_row(member, member.person.id == me, &s.media, &palette),
                    None => el(),
                },
            )
        }
    };
    col()
        .key("members-panel")
        .w_full()
        .h_full()
        .fill(p.bg)
        .border_sides([0.0, 0.0, 0.0, 1.0], p.divider)
        .child(header)
        .child(search)
        .child(count)
        .child(divider(p))
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
}

/// One member: their picture with a presence dot, their name, a badge when verified, their role.
fn member_row(member: &RoomMember, me: bool, media: &crate::state::MediaCache, p: &Palette) -> El {
    let person = &member.person;
    let dot = match member.presence {
        Presence::Online => Some(p.success),
        Presence::Away => Some(p.warning),
        Presence::Offline => Some(p.text_muted),
        Presence::Unknown => None,
    };
    let mut face = el()
        .size(36.0, 36.0)
        .child(media_avatar(&person.id, &person.name, person.avatar.as_ref(), media, 36.0).radius(18.0));
    if let Some(color) = dot {
        face = face.child(el().size(12.0, 12.0).radius(6.0).fill(color).border(2.0, p.bg).absolute(26.0, 26.0));
    }
    let mut entry = row()
        .key(format!("member-{}", person.id))
        .w_full()
        .h(ROW)
        .px(16.0)
        .gap(12.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(face)
        .child(text(person.name.clone()).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).truncate());
    if member.verified {
        entry = entry.child(icon(&icons::VERIFIED, 18.0).color(p.success).help(t!("members.verified").to_owned()));
    }
    if let Some(label) = role_label(member.role) {
        entry = entry.child(text(label).font(font::META).color(p.text_muted));
    }
    if me {
        return entry;
    }
    let id = person.id.clone();
    entry
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(person.name.clone())
        .on_press(move |s: &mut State| s.toggle_member_menu(&id))
}

/// What can be done with a member, beside their row, and its open submenu.
pub fn menu(
    s: &State,
    account: &AccountData,
    user_id: &str,
    submenu: Option<MemberSubmenu>,
    anchor: &str,
    fallback: Option<&matrix_core::Person>,
    p: &Palette,
) -> Vec<El> {
    let Some(room) = s.active_room_id().and_then(|id| account.room(id)) else { return Vec::new() };
    let members: Option<&Members> = account.members.get(&room.room_id);
    let member = members.and_then(|members| members.people.iter().find(|m| m.person.id == user_id));
    let Some(person) = member.map(|member| &member.person).or(fallback) else {
        return Vec::new();
    };
    // Calls go through the direct chat with them: offered where it allows them, or where a new
    // one would (a new direct chat of two always does).
    let direct = account.direct_chat_with(user_id);
    let calls = direct.is_none_or(|room| room.call.via.is_some());
    let id = user_id.to_owned();
    let mut panel = popover_panel(p)
        .key("member-menu")
        .w(260.0)
        .child(text(person.name.clone()).font(font::SECTION).color(p.text_muted).px(12.0).py(6.0).truncate())
        .child({
            let id = id.clone();
            menu_action(&icons::CHAT, t!("members.chat"), Tone::Normal, p)
                .key("member-menu-chat")
                .on_press(move |s: &mut State| s.open_direct_chat(&id))
        });
    if calls {
        for (video, glyph, label, key) in [
            (false, &icons::CALL, t!("call.start"), "member-menu-call"),
            (true, &icons::VIDEO, t!("call.start_video"), "member-menu-video"),
        ] {
            let id = id.clone();
            panel = panel.child(
                menu_action(glyph, label, Tone::Normal, p)
                    .key(key)
                    .on_press_tx(crate::state::navigate(move |s| s.call_person(&id, video))),
            );
        }
    }
    panel = panel.child(
        submenu_row(&icons::PLUS, t!("members.invite_to"), submenu == Some(MemberSubmenu::Invite), p)
            .key(INVITE_ROW)
            .on_press(|s: &mut State| s.toggle_member_submenu(MemberSubmenu::Invite)),
    );
    let roles: Vec<MemberRole> = [MemberRole::Admin, MemberRole::Moderator, MemberRole::Member]
        .into_iter()
        .filter(|role| {
            member.is_some_and(|member| members.is_some_and(|members| members.may(Moderation::Role(*role), member)))
        })
        .collect();
    let may = |action| member.is_some_and(|member| members.is_some_and(|members| members.may(action, member)));
    if !roles.is_empty() || may(Moderation::Kick) || may(Moderation::Ban) {
        panel = panel.child(divider(p));
    }
    if !roles.is_empty() {
        panel = panel.child(
            submenu_row(&icons::SHIELD, t!("members.role"), submenu == Some(MemberSubmenu::Role), p)
                .key(ROLE_ROW)
                .on_press(|s: &mut State| s.toggle_member_submenu(MemberSubmenu::Role)),
        );
    }
    for (action, glyph, label, key) in [
        (Moderation::Kick, &icons::LEAVE, t!("members.kick"), "member-menu-kick"),
        (Moderation::Ban, &icons::ALERT, t!("members.ban"), "member-menu-ban"),
    ] {
        if may(action) {
            let id = id.clone();
            panel = panel.child(
                menu_action(glyph, label, Tone::Danger, p)
                    .key(key)
                    .on_press(move |s: &mut State| s.moderate(&id, action)),
            );
        }
    }
    let mut panels = vec![col().child(panel).anchored(anchor.to_owned(), Side::Left, Align::Start, 6.0)];
    match submenu {
        Some(MemberSubmenu::Invite) => panels.push(invite_menu(account, &room.room_id, user_id, p)),
        Some(MemberSubmenu::Role) => {
            if let Some(member) = member {
                panels.push(role_menu(member, &roles, p));
            }
        }
        None => {}
    }
    panels
}

/// The account's other group rooms, to invite the member to.
fn invite_menu(account: &AccountData, current: &str, user_id: &str, p: &Palette) -> El {
    let mut list = col().w_full();
    for room in account.rooms.iter().filter(|room| room.kind == RoomKind::Group && room.room_id != current) {
        let (room_id, id) = (room.room_id.clone(), user_id.to_owned());
        list = list.child(
            menu_row(menu_icon(&icons::PLUS, p), &super::room::display_name(room), None, None, p)
                .key(format!("member-invite-{}", room.room_id))
                .on_press(move |s: &mut State| s.invite_member_to(&room_id, &id)),
        );
    }
    let panel = popover_panel(p)
        .key("member-invite-menu")
        .w(260.0)
        .child(col().w_full().max_h(320.0).scroll(ScrollSpec { stick_bottom: false }).child(list));
    col().child(panel).anchored(INVITE_ROW, Side::Left, Align::Start, 6.0)
}

/// The roles the user may give, the member's own checked.
fn role_menu(member: &RoomMember, roles: &[MemberRole], p: &Palette) -> El {
    let mut panel = popover_panel(p).key("member-role-menu").w(220.0);
    let check =
        |on: bool| el().size(28.0, 28.0).center().children(on.then(|| icon(&icons::CHECK, 16.0).color(p.accent)));
    let all = [MemberRole::Admin, MemberRole::Moderator, MemberRole::Member];
    for role in all.into_iter().filter(|role| *role == member.role || roles.contains(role)) {
        let label = role_label(role).unwrap_or(t!("members.member"));
        let id = member.person.id.clone();
        let mut entry = menu_row(check(role == member.role), label, None, None, p).key(format!("member-role-{role:?}"));
        if role != member.role {
            entry = entry.on_press(move |s: &mut State| s.moderate(&id, Moderation::Role(role)));
        }
        panel = panel.child(entry);
    }
    col().child(panel).anchored(ROLE_ROW, Side::Left, Align::Start, 6.0)
}
