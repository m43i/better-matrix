//! The link dialog: what a permalink names, a room seen from outside with Join, or a person with
//! Start chat, and the account it acts for when several are signed in.

use matrix_core::{JoinRule, LinkTarget, RoomPreview};
use rune::{El, col, row, text};

use super::components::{ButtonKind, account_avatar, avatar, button, dialog, media_avatar, status_line};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tp};
use crate::state::{LinkDialog, LinkStep, State};

pub fn link_dialog(s: &State, link: &LinkDialog, p: &Palette) -> El {
    let room = match &link.link.target {
        LinkTarget::Room { room, .. } => room.as_str(),
        LinkTarget::User(user) => user.as_str(),
    };
    let mut content = col().w_full().gap(14.0);
    if s.accounts.iter().count() > 1 {
        content = content.child(accounts(s, link, p));
    }
    let cancel =
        button(t!("common.cancel"), ButtonKind::Secondary, true, p).key("link-cancel").on_press(State::close_modal);
    let (title, body, action) = match &link.step {
        LinkStep::Previewing(_) => {
            (t!("link.loading").to_owned(), text(room.to_owned()).font(font::BODY).color(p.text_muted).wrap(), None)
        }
        LinkStep::Preview(preview) => {
            (title_of(preview), preview_body(s, link, preview, p), Some(join_action(preview, p)))
        }
        LinkStep::NoPreview => (
            room.to_owned(),
            text(t!("link.no_preview")).font(font::BODY).color(p.text_muted).wrap(),
            Some(
                button(t!("link.join"), ButtonKind::Primary, true, p)
                    .key("link-join")
                    .on_press(State::join_linked_room),
            ),
        ),
        LinkStep::Joining { preview, .. } => (
            preview.as_deref().map_or_else(|| room.to_owned(), title_of),
            match preview {
                Some(preview) => preview_body(s, link, preview, p),
                None => text(t!("link.no_preview")).font(font::BODY).color(p.text_muted).wrap(),
            },
            Some(button(t!("link.joining"), ButtonKind::Primary, false, p).key("link-join")),
        ),
        LinkStep::Person { person, request } => {
            let mut who = row()
                .w_full()
                .gap(14.0)
                .center_y()
                .child(media_avatar(
                    &person.id,
                    &s.settings.person_name(&person.name),
                    person.avatar.as_ref(),
                    &s.media,
                    56.0,
                ))
                .child(text(person.id.clone()).font(font::BODY).color(p.text_muted).grow(1.0).wrap());
            if request.is_some() {
                who = who.child(text(t!("link.person_loading")).font(font::META).color(p.text_muted));
            }
            (
                s.settings.person_name(&person.name).into_owned(),
                who,
                Some(
                    button(t!("members.chat"), ButtonKind::Primary, true, p)
                        .key("link-chat")
                        .on_press(State::chat_with_linked_person),
                ),
            )
        }
    };
    content = content.child(body);
    dialog("link-dialog", &title, content, std::iter::once(cancel).chain(action).collect(), p)
}

/// "Open as" and the signed-in accounts, the dialog's own picked.
fn accounts(s: &State, link: &LinkDialog, p: &Palette) -> El {
    let mut chips = row().w_full().gap(6.0).wrap();
    for account in s.accounts.iter() {
        let id = account.summary.id.clone();
        let picked = id == link.account;
        chips = chips.child(
            row()
                .key(format!("link-account-{id}"))
                .gap(6.0)
                .center_y()
                .pl(4.0)
                .pr(10.0)
                .py(4.0)
                .radius(8.0)
                .border(1.0, if picked { p.accent } else { p.divider })
                .fill(if picked { p.accent.alpha(0.12) } else { p.bg })
                .role("radio")
                .label(s.settings.person_name(&account.summary.display_name))
                .help(id.to_string())
                .cursor(rune::Cursor::Pointer)
                .child(account_avatar(&account.summary, &s.media, 20.0))
                .child(text(id.to_string()).font(font::META).color(p.text))
                .on_press_tx(crate::state::navigate(move |s| s.choose_link_account(&id))),
        );
    }
    col().w_full().gap(6.0).child(text(t!("link.as")).font(font::SECTION).color(p.text_muted)).child(chips)
}

fn title_of(preview: &RoomPreview) -> String {
    preview.name.clone().or_else(|| preview.alias.clone()).unwrap_or_else(|| preview.room_id.clone())
}

/// The room's picture, address and size, its topic, and why it can't be joined when it can't.
fn preview_body(s: &State, link: &LinkDialog, preview: &RoomPreview, p: &Palette) -> El {
    let address = preview.alias.clone().unwrap_or_else(|| preview.room_id.clone());
    let head = row()
        .w_full()
        .gap(14.0)
        .center_y()
        .child(if s.link_hides_picture(link, preview) {
            avatar(&preview.room_id, &title_of(preview), 56.0)
        } else {
            media_avatar(&preview.room_id, &title_of(preview), preview.avatar.as_ref(), &s.media, 56.0)
        })
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text(address).font(font::BODY).color(p.text).truncate())
                .child(text(tp!("overview.members", preview.member_count)).font(font::META).color(p.text_muted)),
        );
    let mut body = col().w_full().gap(12.0).child(head);
    if let Some(topic) = &preview.topic {
        body = body.child(text(topic.clone()).font(font::BODY).color(p.text_muted).wrap().max_lines(6));
    }
    let status = if preview.invited {
        Some((&icons::USER_PLUS, t!("link.invited"), p.success))
    } else {
        match preview.join_rule {
            Some(JoinRule::Invite) => Some((&icons::LOCK, t!("link.invite_only"), p.warning)),
            Some(JoinRule::Knock) => Some((&icons::LOCK, t!("link.knock"), p.warning)),
            _ => None,
        }
    };
    if let Some((glyph, message, color)) = status {
        body = body.child(status_line(glyph, message, color, p));
    }
    body
}

/// Join the room or space, offered when it may work. (A room this account is in has opened
/// instead of showing here.)
fn join_action(preview: &RoomPreview, p: &Palette) -> El {
    let label = if preview.space { t!("link.join_space") } else { t!("link.join") };
    button(label, ButtonKind::Primary, preview.joinable(), p).key("link-join").on_press(State::join_linked_room)
}
