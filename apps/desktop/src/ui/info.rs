//! The room's panels beside its timeline, besides the member list: its overview (from the info
//! button: name, topic, and the ways to its members, threads and files, and to leave it), its
//! threads, and the files shared in it.

use matrix_core::{AttachmentKind, RoomKind, RoomSummary};
use rune::{El, ScrollSpec, col, el, icon, row, text};

use super::components::{ButtonKind, button, divider, icon_button, media_avatar, when};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tp};
use crate::state::{AccountData, SidePanel, State};

/// A panel's frame: a title and a close button, then its content, which scrolls, and a footer
/// that stays at the bottom.
fn frame(key: &str, title: &str, content: El, footer: Option<El>, p: &Palette) -> El {
    let header = row()
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .px(16.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(title.to_owned()).font(font::HEADER).color(p.text).grow(1.0).truncate())
        .child(
            icon_button(&icons::CLOSE, t!("info.close"), false, p.text_muted, p)
                .key(format!("{key}-close"))
                .on_press(|s: &mut State| s.side = None),
        );
    col()
        .key(key.to_owned())
        .w_full()
        .h_full()
        .fill(p.bg)
        .border_sides([0.0, 0.0, 0.0, 1.0], p.divider)
        .child(header)
        .child(col().w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(content))
        .children(footer)
}

/// A row leading somewhere: an icon, a title, a detail at the end.
fn link_row(glyph: &'static rune::Icon, title: &str, detail: Option<String>, key: &str, p: &Palette) -> El {
    let mut entry = row()
        .key(key.to_owned())
        .w_full()
        .h(44.0)
        .px(16.0)
        .gap(12.0)
        .center_y()
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(title.to_owned())
        .child(icon(glyph, 18.0).color(p.text_muted))
        .child(text(title.to_owned()).font(font::BODY).color(p.text).grow(1.0));
    if let Some(detail) = detail {
        entry = entry.child(text(detail).font(font::META).color(p.text_muted));
    }
    entry.child(icon(&icons::CHEVRON_RIGHT, 16.0).color(p.text_muted))
}

/// The room's overview.
pub fn overview(s: &State, room: &RoomSummary, p: &Palette) -> El {
    let count = s.active_account().map_or(room.member_count, |account| account.member_count(room));
    let topic = room.topic.clone().unwrap_or_else(|| t!("room.no_topic").to_owned());
    let head = col()
        .w_full()
        .padding(20.0)
        .gap(10.0)
        .items_center()
        .child(media_avatar(&room.room_id, &room.name, room.avatar.as_ref(), &s.media, 72.0))
        .child(text(super::room::display_name(room)).font(font::HEADER).color(p.text).wrap())
        .child(text(topic).key("info-topic").font(font::BODY).color(p.text_muted).wrap().selectable(true));
    let room_id = room.room_id.clone();
    let (leave_label, leave_icon) = match room.kind {
        RoomKind::Group => (t!("room_menu.leave"), &icons::LEAVE),
        RoomKind::Direct => (t!("room_menu.delete_chat"), &icons::TRASH),
    };
    let leave = row()
        .key("info-leave")
        .w_full()
        .h(44.0)
        .px(16.0)
        .gap(12.0)
        .center_y()
        .hover(p.danger.alpha(0.1))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(leave_label.to_owned())
        .child(icon(leave_icon, 18.0).color(p.danger))
        .child(text(leave_label).font(font::BODY).color(p.danger))
        .on_press(move |s: &mut State| s.ask_to_leave(&room_id));
    let content = col()
        .w_full()
        .child(head)
        .child(divider(p))
        .children((room.kind == RoomKind::Group || count > 2).then(|| {
            link_row(&icons::USER, t!("members.title"), Some(count.to_string()), "info-members", p)
                .on_press(|s: &mut State| s.show_side(SidePanel::Members))
        }))
        .child(
            link_row(&icons::THREAD, t!("info.threads"), None, "info-threads", p)
                .on_press(|s: &mut State| s.show_side(SidePanel::Threads)),
        )
        .child(
            link_row(&icons::PIN, t!("info.pins"), Some(room.pinned.len().to_string()), "info-pins", p)
                .on_press(|s: &mut State| s.show_side(SidePanel::Pins)),
        )
        .child(
            link_row(&icons::FILE, t!("info.files"), None, "info-files", p)
                .on_press(|s: &mut State| s.show_side(SidePanel::Files)),
        );
    // Leaving sits apart from the rest, at the bottom, however long the topic.
    let footer = col().w_full().child(divider(p)).child(leave.mb(8.0));
    frame("info-panel", t!("info.title"), content, Some(footer), p)
}

/// The room's threads; pressing one opens it.
pub fn threads(account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let list = match account.threads.get(&room.room_id) {
        None => col().w_full().child(text(t!("info.loading")).font(font::BODY).color(p.text_muted).px(16.0).py(12.0)),
        Some(threads) if threads.is_empty() => {
            col().w_full().child(text(t!("info.no_threads")).font(font::BODY).color(p.text_muted).px(16.0).py(12.0))
        }
        Some(threads) => threads.iter().fold(col().w_full(), |list, thread| {
            let root = thread.root.clone();
            let mut meta = tp!("thread.replies", thread.replies);
            if let Some(at) = thread.latest_at {
                meta = format!("{meta} · {}", when(at));
            }
            list.child(
                col()
                    .key(format!("listed-thread-{}", thread.root))
                    .w_full()
                    .px(16.0)
                    .py(10.0)
                    .gap(3.0)
                    .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
                    .hover(p.hover.alpha(p.hover.a * 2.0))
                    .role("button")
                    .cursor(rune::Cursor::Pointer)
                    .focusable(true)
                    .label(thread.body.clone())
                    .child(text(thread.sender_name.clone()).font(font::BODY_STRONG).color(p.text).truncate())
                    .child(text(thread.body.clone()).font(font::BODY).color(p.text).max_lines(2))
                    .child(text(meta).font(font::META).color(p.accent))
                    .on_press(move |s: &mut State| s.open_listed_thread(&root)),
            )
        }),
    };
    frame("threads-panel", t!("info.threads"), list, None, p)
}

/// The files shared in the room; pressing one opens it.
pub fn files(account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let list = match account.files.get(&room.room_id) {
        None => col().w_full().child(text(t!("info.loading")).font(font::BODY).color(p.text_muted).px(16.0).py(12.0)),
        Some(files) if files.is_empty() => {
            col().w_full().child(text(t!("info.no_files")).font(font::BODY).color(p.text_muted).px(16.0).py(12.0))
        }
        Some(files) => files.iter().fold(col().w_full(), |list, file| {
            let glyph = match file.attachment.kind {
                AttachmentKind::Image => &icons::IMAGE,
                AttachmentKind::Video => &icons::VIDEO,
                AttachmentKind::Audio => &icons::AUDIO,
                AttachmentKind::File => &icons::FILE,
            };
            let opened = file.attachment.clone();
            list.child(
                row()
                    .key(format!("listed-file-{}", file.event_id))
                    .w_full()
                    .px(16.0)
                    .py(10.0)
                    .gap(12.0)
                    .center_y()
                    .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
                    .hover(p.hover.alpha(p.hover.a * 2.0))
                    .role("button")
                    .cursor(rune::Cursor::Pointer)
                    .focusable(true)
                    .label(file.attachment.name.clone())
                    .child(
                        el().size(36.0, 36.0)
                            .radius(8.0)
                            .fill(p.accent.alpha(0.12))
                            .center()
                            .child(icon(glyph, 18.0).color(p.accent)),
                    )
                    .child(
                        col()
                            .grow(1.0)
                            .min_w(0.0)
                            .child(text(file.attachment.name.clone()).font(font::BODY).color(p.text).truncate())
                            .child(
                                text(format!("{} · {}", file.sender_name, when(file.timestamp)))
                                    .font(font::META)
                                    .color(p.text_muted)
                                    .truncate(),
                            ),
                    )
                    .on_press(move |s: &mut State| s.open_attachment(&opened)),
            )
        }),
    };
    frame("files-panel", t!("info.files"), list, None, p)
}

/// Shared pins, independent of how far the room's timeline has been paged.
pub fn pins(s: &State, account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let mut list = col().w_full().py(8.0);
    if let Some(error) = account.pin_errors.get(&room.room_id) {
        list = list.child(
            col()
                .w_full()
                .px(16.0)
                .py(12.0)
                .gap(8.0)
                .child(text(error.clone()).font(font::BODY).color(p.danger).wrap())
                .child(
                    button(t!("info.pins_retry"), ButtonKind::Secondary, true, p)
                        .key("pins-retry")
                        .on_press(|s: &mut State| s.show_side(SidePanel::Pins)),
                ),
        );
    }
    match account.pins.get(&room.room_id) {
        None if account.pin_errors.contains_key(&room.room_id) => {}
        None => list = list.child(text(t!("info.loading")).font(font::BODY).color(p.text_muted).px(16.0).py(12.0)),
        Some(pins) => {
            if room.pinned.is_empty() {
                list =
                    list.child(text(t!("info.no_pins")).font(font::BODY).color(p.text_muted).px(16.0).py(12.0).wrap());
            }
            for pin in pins.iter().filter(|pin| room.pinned.contains(&pin.event_id)) {
                let destination =
                    matrix_core::TimelineKey { room_id: room.room_id.clone(), thread: pin.thread.clone() };
                let event_id = pin.event_id.clone();
                let (key, id) = (matrix_core::TimelineKey::room(&room.room_id), pin.event_id.clone());
                let mut header =
                    row().w_full().min_h(28.0).gap(4.0).center_y().child(
                        text(pin.sender_name.clone()).font(font::BODY_STRONG).color(p.text).grow(1.0).truncate(),
                    );
                if room.can_pin {
                    header = header.child(
                        icon_button(&icons::UNPIN, t!("actions.unpin"), false, p.text_muted, p)
                            .key(format!("pinned-unpin-{}", pin.event_id))
                            .help(t!("actions.unpin"))
                            .on_press(move |s: &mut State| s.set_message_pinned(&key, &id, false)),
                    );
                }
                let mut body = col().grow(1.0).min_w(0.0).gap(3.0).child(header).child(
                    text(pin.body.clone().unwrap_or_else(|| t!("message.unavailable").to_owned()))
                        .font(font::BODY)
                        .color(if pin.body.is_some() { p.text } else { p.text_muted })
                        .max_lines(4)
                        .wrap(),
                );
                if let Some(at) = pin.timestamp_ms {
                    body = body.child(text(when(at)).font(font::META).color(p.text_muted));
                }
                let mut message = row()
                    .key(format!("pinned-open-{}", pin.event_id))
                    .w_full()
                    .px(16.0)
                    .py(10.0)
                    .gap(10.0)
                    .items_start()
                    .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
                    .child(
                        media_avatar(&pin.sender_id, &pin.sender_name, pin.sender_avatar.as_ref(), &s.media, 32.0)
                            .mt(3.0),
                    )
                    .child(body);
                if pin.body.is_some() {
                    message = message
                        .role("button")
                        .label(t!("message.open"))
                        .focusable(true)
                        .cursor(rune::Cursor::Pointer)
                        .hover(p.hover)
                        .on_press(move |s: &mut State| {
                            s.side = None;
                            s.open_message(&destination, &event_id);
                        });
                }
                list = list.child(message);
            }
        }
    }
    frame("pins-panel", t!("info.pins"), list, None, p)
}
