//! Invitations and the public rooms suggested by a space's Overview.

use matrix_core::{AccountId, PublicRoom};
use rune::{El, col, icon, row, text};

use super::components::{ButtonKind, NavEmphasis, button, dialog, media_avatar, nav_row};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf, tp};
use crate::state::{AccountData, Directory, INVITATIONS, State};

pub fn invitations(s: &State, account: &AccountData, p: &Palette) -> Option<El> {
    if account.invitations.is_empty() {
        return None;
    }
    let collapsed = s.section_collapsed(INVITATIONS);
    let title = tf!("invitations.heading", count = account.invitations.len());
    let header = row()
        .key("section-header-invitations")
        .w_full()
        .h(28.0)
        .px(6.0)
        .gap(4.0)
        .center_y()
        .radius(6.0)
        .hover(p.sidebar_hover)
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .expanded(!collapsed)
        .label(title.clone())
        .child(
            icon(if collapsed { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN }, 14.0)
                .color(p.sidebar_text_muted),
        )
        .child(text(title).font(font::SECTION).color(p.sidebar_text_muted).grow(1.0).truncate())
        .on_press(|s: &mut State| s.toggle_section(INVITATIONS));
    let mut list = col().key("invitations").w_full().gap(2.0).child(header);
    if collapsed {
        return Some(list);
    }
    for invite in &account.invitations {
        let room_id = invite.room_id.clone();
        let leading = if !invite.space && invite.kind == matrix_core::RoomKind::Group {
            rune::el().w(18.0).center().child(text("#").font(font::NAV).color(p.sidebar_text_muted))
        } else {
            media_avatar(&room_id, &invite.name, invite.avatar.as_ref(), &s.media, 18.0)
        };
        list = list.child(
            nav_row(leading, &invite.name, NavEmphasis::Unread, None, p)
                .key(format!("invitation-{room_id}"))
                .on_press(move |s: &mut State| s.show_invitation(&room_id)),
        );
    }
    Some(list)
}

fn invitation_name(invite: &matrix_core::Invitation) -> String {
    if !invite.space && invite.kind == matrix_core::RoomKind::Group {
        format!("# {}", invite.name)
    } else {
        invite.name.clone()
    }
}

pub fn home_invitations(s: &State, account: &AccountData, p: &Palette) -> Option<El> {
    if account.invitations.is_empty() {
        return None;
    }
    let mut list = col().key("overview-invitations").w_full().gap(6.0).child(
        text(tf!("invitations.heading", count = account.invitations.len())).font(font::SECTION).color(p.text_muted),
    );
    for invite in &account.invitations {
        let room_id = invite.room_id.clone();
        let mut titles = col()
            .grow(1.0)
            .min_w(0.0)
            .gap(3.0)
            .child(text(invitation_name(invite)).font(font::BODY_STRONG).color(p.text).truncate());
        if let Some(inviter) = &invite.inviter {
            titles = titles.child(
                text(tf!("invitations.from", name = inviter.name, user = inviter.id))
                    .font(font::META)
                    .color(p.text_muted)
                    .truncate(),
            );
        }
        list = list.child(
            row()
                .key(format!("overview-invitation-{room_id}"))
                .w_full()
                .padding(14.0)
                .gap(12.0)
                .center_y()
                .radius(10.0)
                .fill(p.surface)
                .border(1.0, p.divider)
                .hover(p.hover)
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .label(invitation_name(invite))
                .child(media_avatar(&room_id, &invite.name, invite.avatar.as_ref(), &s.media, 36.0))
                .child(titles)
                .child(icon(&icons::CHEVRON_RIGHT, 18.0).color(p.text_muted))
                .on_press(move |s: &mut State| s.show_invitation(&room_id)),
        );
    }
    Some(list)
}

pub fn invitation(s: &State, account: &AccountId, room_id: &str, p: &Palette) -> Option<El> {
    let invite = s.accounts.get(account)?.invitations.iter().find(|invite| invite.room_id == room_id)?;
    let mut content = col()
        .w_full()
        .gap(12.0)
        .child(media_avatar(room_id, &invite.name, invite.avatar.as_ref(), &s.media, 56.0))
        .child(
            text(tf!("invitations.account", account = account.as_str())).font(font::META).color(p.text_muted).wrap(),
        );
    if let Some(inviter) = &invite.inviter {
        content = content.child(
            text(tf!("invitations.from", name = inviter.name, user = inviter.id)).font(font::BODY).color(p.text).wrap(),
        );
    }
    if let Some(topic) = &invite.topic {
        content = content.child(text(topic.clone()).font(font::BODY).color(p.text_muted).wrap());
    }
    let busy = s.membership_pending(account, room_id);
    let (accept_account, accept_room) = (account.clone(), room_id.to_owned());
    let (decline_account, decline_room) = (account.clone(), room_id.to_owned());
    Some(dialog(
        "invitation-preview",
        &invitation_name(invite),
        content,
        vec![
            button(t!("common.close"), ButtonKind::Secondary, true, p)
                .key("invitation-close")
                .on_press(State::close_modal),
            button(t!("invitations.decline"), ButtonKind::Secondary, !busy, p)
                .key("invitation-decline")
                .on_press(move |s: &mut State| s.respond_to_invitation(&decline_account, &decline_room, false)),
            button(t!("invitations.accept"), ButtonKind::Primary, !busy, p)
                .key("invitation-accept")
                .on_press(move |s: &mut State| s.respond_to_invitation(&accept_account, &accept_room, true)),
        ],
        p,
    ))
}

pub fn suggestions(s: &State, account: &AccountData, space: &str, p: &Palette) -> El {
    let header = row()
        .w_full()
        .center_y()
        .child(text(t!("directory.suggested")).font(font::SECTION).color(p.text_muted).grow(1.0))
        .child(
            button(t!("actions.more"), ButtonKind::Link, true, p)
                .key("public-rooms-more")
                .on_press(State::browse_public_rooms),
        );
    col().key("suggested-rooms").w_full().gap(10.0).child(header).child(room_list(
        s,
        &account.summary.id,
        space,
        Some(5),
        p,
    ))
}

pub fn public_rooms(s: &State, account: &AccountId, space: &str, p: &Palette) -> El {
    dialog(
        "public-rooms",
        t!("directory.title"),
        room_list(s, account, space, None, p),
        vec![
            button(t!("common.close"), ButtonKind::Secondary, true, p)
                .key("public-rooms-close")
                .on_press(State::close_modal),
        ],
        p,
    )
    .w(640.0)
}

fn room_list(s: &State, account: &AccountId, space: &str, limit: Option<usize>, p: &Palette) -> El {
    let rooms = match s.directories.get(&(account.clone(), space.to_owned())) {
        Some(Directory::Ready(rooms)) => rooms,
        Some(Directory::Failed(error)) => {
            let (account, space) = (account.clone(), space.to_owned());
            return col().gap(8.0).child(text(t!("directory.failed")).font(font::BODY).color(p.text_muted)).child(
                button(t!("common.try_again"), ButtonKind::Link, true, p)
                    .key("public-rooms-retry")
                    .help(error.clone())
                    .on_press(move |s: &mut State| s.load_public_rooms(account.clone(), space.clone())),
            );
        }
        _ => return text(t!("directory.loading")).font(font::BODY).color(p.text_muted),
    };
    let data = s.accounts.get(account);
    let indices: Vec<usize> = rooms
        .iter()
        .enumerate()
        .filter(|(_, room)| data.is_none_or(|data| data.room(&room.room_id).is_none()))
        .map(|(i, _)| i)
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    if indices.is_empty() {
        return text(t!("directory.empty")).font(font::BODY).color(p.text_muted).wrap();
    }
    if limit.is_some() {
        return indices
            .into_iter()
            .fold(col().w_full().gap(6.0), |list, i| list.child(room_card(s, account, space, &rooms[i], p)));
    }
    let keys: Vec<_> = indices.iter().map(|&i| rooms[i].room_id.clone()).collect();
    let (account, space, palette) = (account.clone(), space.to_owned(), *p);
    col().w_full().gap(6.0).growth(rune::GrowthSpec::default()).virtual_rows(
        indices.len(),
        92.0,
        move |_: &State, i| std::borrow::Cow::Owned(keys[i].clone()),
        move |s: &State, i| {
            let Some(Directory::Ready(rooms)) = s.directories.get(&(account.clone(), space.clone())) else {
                return rune::el();
            };
            rooms.get(indices[i]).map_or_else(rune::el, |room| room_card(s, &account, &space, room, &palette))
        },
    )
}

fn room_card(s: &State, account: &AccountId, space: &str, room: &PublicRoom, p: &Palette) -> El {
    let mut titles = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(3.0)
        .child(text(format!("# {}", room.name)).font(font::BODY_STRONG).color(p.text).truncate())
        .child(text(tp!("overview.members", room.member_count)).font(font::META).color(p.text_muted));
    if let Some(topic) = &room.topic {
        titles = titles.child(text(topic.clone()).font(font::META).color(p.text_muted).max_lines(2).wrap());
    }
    let (owner, parent, room_id) = (account.clone(), space.to_owned(), room.room_id.clone());
    row()
        .key(format!("public-room-{room_id}"))
        .w_full()
        .padding(14.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .child(titles)
        .child(
            button(t!("directory.join"), ButtonKind::Primary, !s.membership_pending(account, &room_id), p)
                .key(format!("join-{room_id}"))
                .on_press(move |s: &mut State| s.join_public_room(&owner, &parent, &room_id)),
        )
}
