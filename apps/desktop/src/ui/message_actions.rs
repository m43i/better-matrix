//! Message menus and the destination picker. More and right-click open the same menu, with every
//! action, the toolbar's included: only where it opens differs.

use matrix_core::{MessageContent, RoomKind, TimelineKey};
use rune::{El, col, row, text};

use super::components::{
    ButtonKind, Tone, at_point, button, dialog as frame, divider, field_input, media_avatar, menu_action, popover_panel,
};
use super::{
    icons,
    theme::{Palette, font},
};
use crate::i18n::{I18N, t};
use crate::state::{ComposerTarget, EmojiFor, Modal, Popover, State};

const MENU_WIDTH: f32 = 300.0;

pub fn menu(
    s: &State,
    key: &TimelineKey,
    event_id: &str,
    at: Option<(i32, i32)>,
    window: [f32; 2],
    p: &Palette,
) -> Vec<El> {
    let Some(account) = s.active_account() else { return Vec::new() };
    let Some(message) = account
        .timeline(key)
        .and_then(|timeline| timeline.entries.iter().filter_map(|e| e.message()).find(|m| m.event_id == event_id))
    else {
        return Vec::new();
    };
    if message.pending || message.event_id.starts_with("local:") || matches!(message.content, MessageContent::Call(_)) {
        return Vec::new();
    }
    let target = if key.thread.is_some() { ComposerTarget::Thread } else { ComposerTarget::Room };
    let readable =
        !matches!(message.content, MessageContent::Redacted | MessageContent::Undecryptable | MessageContent::Call(_));
    let action = |glyph, label, id| menu_action(glyph, label, Tone::Normal, p).key(id);
    let mut panel = popover_panel(p).key("message-menu").w(MENU_WIDTH);
    // Held-back media offers no file actions until shown (ADR 0011).
    if let MessageContent::Attachment(file) = &message.content
        && !account.hides_listed_media(&key.room_id, event_id, file, &s.revealed_media)
    {
        let mut actions = super::attachments::actions(&account.summary.id, file, Some((&key.room_id, event_id)), p);
        // A sticker or picture someone sent can join the user's own stickers.
        if file.kind == matrix_core::AttachmentKind::Image
            && let Some(media) = file.media.clone()
        {
            let (name, dimensions) = (file.name.clone(), file.dimensions);
            actions.push(
                action(&icons::STICKER, t!("actions.add_to_stickers"), "message-add-sticker")
                    .on_press(move |s: &mut State| s.add_to_my_stickers(media.clone(), name.clone(), dimensions)),
            );
        }
        if !actions.is_empty() {
            panel = panel.children(actions).child(divider(p));
        }
    }
    if readable {
        // A bubble's toolbar may have no room for React; the menu always offers it.
        let (react_key, id) = (key.clone(), event_id.to_owned());
        panel =
            panel.child(action(&icons::SMILE, t!("actions.react"), "message-react").on_press(move |s: &mut State| {
                s.popover = Some(Popover::Emoji(EmojiFor::Reaction { key: react_key.clone(), event_id: id.clone() }));
            }));
        let reply = message.clone();
        panel = panel.child(action(&icons::REPLY, t!("actions.reply"), "message-reply").on_press_tx(
            move |s: &mut State, tx| {
                s.popover = None;
                s.start_reply(target, &reply, tx);
            },
        ));
        if key.thread.is_none() {
            let id = event_id.to_owned();
            panel = panel.child(action(&icons::THREAD, t!("actions.thread"), "message-thread").on_press(
                move |s: &mut State| {
                    s.popover = None;
                    s.open_thread(&id);
                },
            ));
        }
        if matches!(message.content, MessageContent::Text { .. } | MessageContent::Attachment(_)) {
            let (key, id) = (key.clone(), event_id.to_owned());
            panel = panel.child(
                action(&icons::FORWARD, t!("actions.forward"), "message-forward")
                    .on_press(move |s: &mut State| s.start_forward(key.clone(), id.clone())),
            );
        }
        if super::timeline::editable(message, account.summary.id.as_str()) {
            let edited = message.clone();
            panel = panel.child(
                action(&icons::PENCIL, t!("actions.edit"), "message-edit")
                    .on_press_tx(move |s: &mut State, tx| s.start_edit(target, &edited, tx)),
            );
        }
    }
    panel = panel.child(divider(p));
    let link = matrix_core::permalinks::message_link(&key.room_id, event_id, account.summary.id.as_str());
    panel = panel.child(action(&icons::LINK, t!("actions.copy_link"), "message-copy-link").on_press_tx(
        move |s: &mut State, tx| {
            tx.copy(link.clone());
            s.popover = None;
        },
    ));
    if let Some(room) = account.room(&key.room_id)
        && room.can_pin
        && readable
    {
        let pinned = room.pinned.iter().any(|id| id == event_id);
        let (key, id) = (key.clone(), event_id.to_owned());
        panel = panel.child(
            action(&icons::PIN, if pinned { t!("actions.unpin") } else { t!("actions.pin") }, "message-pin")
                .on_press(move |s: &mut State| s.set_message_pinned(&key, &id, !pinned)),
        );
    }
    if readable {
        let saved = account.is_saved(&key.room_id, event_id);
        let (owner, room_id, id) = (account.summary.id.clone(), key.room_id.clone(), event_id.to_owned());
        let (glyph, label) =
            if saved { (&icons::UNBOOKMARK, t!("actions.unsave")) } else { (&icons::BOOKMARK, t!("actions.save")) };
        panel = panel.child(
            action(glyph, label, "message-save")
                .on_press(move |s: &mut State| s.set_message_saved(&owner, &room_id, &id, !saved)),
        );
    }
    let body = match &message.content {
        // As it shows: a hidden spoiler's words are neither copied nor sent to be translated.
        MessageContent::Text { body, .. } if !body.trim().is_empty() => Some(message.content.plain().into_owned()),
        MessageContent::Poll(poll) => Some(poll.question.clone()),
        _ => None,
    };
    if let Some(body) = body {
        let copied = body.clone();
        panel = panel.child(action(&icons::COPY, t!("actions.copy_text"), "message-copy").on_press_tx(
            move |s: &mut State, tx| {
                tx.copy(copied.clone());
                s.popover = None;
            },
        ));
        if matches!(message.content, MessageContent::Text { .. }) && !body.is_empty() {
            panel = panel.child(action(&icons::LANGUAGE, t!("actions.translate"), "message-translate").on_press(
                move |s: &mut State| {
                    s.popover = None;
                    s.modal = Some(Modal::Translate(body.clone()));
                },
            ));
        }
    }
    let (owner, room_id, id) = (account.summary.id.clone(), key.room_id.clone(), event_id.to_owned());
    panel = panel.child(
        action(&icons::CODE, t!("actions.view_source"), "message-source")
            .on_press(move |s: &mut State| s.view_source(owner.clone(), room_id.clone(), id.clone())),
    );
    // Someone else's message can go to the homeserver's admins.
    if message.sender_id != account.summary.id.as_str() {
        let (key, id) = (key.clone(), event_id.to_owned());
        panel = panel.child(divider(p)).child(
            menu_action(&icons::FLAG, t!("report.message_action"), Tone::Danger, p)
                .key("message-report")
                .on_press(move |s: &mut State| s.report_message(&key, &id)),
        );
    }
    if message.sender_id == account.summary.id.as_str() {
        if let MessageContent::Poll(poll) = &message.content
            && !poll.ended
        {
            let (key, id, question) = (key.clone(), event_id.to_owned(), poll.question.clone());
            panel = panel.child(
                action(&icons::POLL, t!("poll.end"), "message-end-poll")
                    .on_press(move |s: &mut State| s.end_poll(key.clone(), &id, &question)),
            );
        }
        if !matches!(message.content, MessageContent::Redacted) {
            let (key, id) = (key.clone(), event_id.to_owned());
            panel = panel.child(divider(p)).child(
                menu_action(&icons::TRASH, t!("actions.delete"), Tone::Danger, p)
                    .key("message-delete")
                    .on_press(move |s: &mut State| s.delete_message(key.clone(), &id)),
            );
        }
    }
    // The panel shrinks to the room `anchored_fit` leaves it and scrolls the rest.
    let panel = panel.max_h((window[1] - 16.0).max(100.0)).shrink().min_h(0.0);
    let menu = col()
        .key("message-menu-layer")
        .role("menu")
        .focus_group()
        .modal(true, State::close_popover)
        .min_h(0.0)
        .child(panel.scroll(rune::ScrollSpec { stick_bottom: false }));
    match at {
        Some(at) => at_point(menu, "message-menu", at, MENU_WIDTH, window).into(),
        // Under More, or over it where the window has more room there; a menu taller than both
        // takes the larger and scrolls, rather than running off the window's bottom.
        None => vec![menu.anchored_fit(
            format!("{}more-{event_id}", if key.thread.is_some() { "thread-" } else { "" }),
            rune::Side::Bottom,
            rune::Align::End,
            4.0,
        )],
    }
}

pub fn dialog(s: &State, p: &Palette) -> Option<El> {
    let cancel = button(t!("common.cancel"), ButtonKind::Secondary, true, p)
        .key("message-dialog-cancel")
        .on_press(State::close_modal);
    match &s.modal {
        Some(Modal::Translate(body)) => {
            let language = I18N.locales()[s.settings.language.resolve()].id();
            let url = matrix_core::links::translation_url(body, language);
            Some(frame(
                "translate-dialog",
                t!("actions.translate"),
                text(t!("message.translate_explanation")).font(font::BODY).color(p.text).wrap(),
                vec![
                    cancel,
                    button(t!("actions.translate"), ButtonKind::Primary, true, p)
                        .key("message-translate-confirm")
                        .on_press_tx(move |s: &mut State, tx| {
                            s.open_link(&url, tx);
                            s.modal = None;
                        }),
                ],
                p,
            ))
        }
        Some(Modal::Forward { account, search, sending, .. }) => {
            let mut content = col().w_full().gap(8.0).child(
                field_input(search, t!("message.find_room"), p).key("forward-search").on_edit(State::modal_field),
            );
            if let Some(data) = s.accounts.get(account) {
                let rooms: Vec<_> =
                    data.rooms.iter().filter(|room| crate::state::matches(&room.name, &search.text)).collect();
                if rooms.is_empty() {
                    content = content.child(text(t!("message.no_rooms")).font(font::BODY).color(p.text_muted));
                }
                for (kind, title, section_key) in [
                    (RoomKind::Group, t!("sidebar.rooms"), "forward-rooms"),
                    (RoomKind::Direct, t!("sidebar.direct"), "forward-direct"),
                ] {
                    if !rooms.iter().any(|room| room.kind == kind) {
                        continue;
                    }
                    content = content.child(
                        text(title).key(section_key).font(font::SECTION).color(p.text_muted).px(12.0).pt(8.0).pb(2.0),
                    );
                    for room in rooms.iter().filter(|room| room.kind == kind) {
                        let id = room.room_id.clone();
                        let name = s.settings.room_name(room);
                        let leading = match kind {
                            RoomKind::Group => rune::el()
                                .size(28.0, 28.0)
                                .center()
                                .child(text("#").font(font::HEADER).color(p.text_muted)),
                            RoomKind::Direct => media_avatar(
                                room.dm_users.first().unwrap_or(&room.room_id),
                                &name,
                                room.avatar.as_ref(),
                                &s.media,
                                28.0,
                            ),
                        };
                        content = content.child(
                            row()
                                .key(format!("forward-room-{id}"))
                                .w_full()
                                .px(12.0)
                                .py(8.0)
                                .gap(10.0)
                                .center_y()
                                .hover(p.hover)
                                .role("button")
                                .focusable(sending.is_none())
                                .label(name.as_ref())
                                .child(leading)
                                .child(text(name).font(font::BODY).color(p.text).grow(1.0).truncate())
                                .on_press(move |s: &mut State| s.forward_to(id.clone())),
                        );
                    }
                }
            }
            if sending.is_some() {
                content = content.child(text(t!("message.sending")).font(font::META).color(p.text_muted));
            }
            Some(frame("forward-dialog", t!("actions.forward"), content, vec![cancel], p))
        }
        _ => None,
    }
}
