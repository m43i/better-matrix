//! A room's context menu: marking it unread or read, putting it among the favourites or the low
//! priority rooms, a notifications submenu, then leaving the room (or, for a direct chat,
//! deleting it), which asks for confirmation first.

use matrix_core::{NotificationMode, RoomKind, RoomPriority, RoomSummary};
use rune::{Align, El, Side, col, icon, text};

use super::components::{Tone, divider, menu_action, menu_icon, menu_row, popover_panel, submenu_row};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, State};

/// The key the notifications submenu anchors to.
const NOTIFICATIONS_ROW: &str = "room-menu-notifications";

/// Every notification mode, how it reads, and its icon. Shared with the settings modal.
pub fn modes() -> [(NotificationMode, &'static str, &'static str, &'static rune::Icon); 3] {
    [
        (NotificationMode::AllMessages, t!("notify.all"), t!("notify.all_hint"), &icons::BELL),
        (NotificationMode::MentionsOnly, t!("notify.mentions"), t!("notify.mentions_hint"), &icons::AT),
        (NotificationMode::Mute, t!("notify.mute"), t!("notify.mute_hint"), &icons::BELL_OFF),
    ]
}

/// The name of a mode, for "As in your settings: Mentions only".
pub fn mode_label(mode: NotificationMode) -> &'static str {
    modes().into_iter().find(|(m, ..)| *m == mode).map_or("", |(_, label, ..)| label)
}

/// A stable key part per mode, the same in every language.
pub fn mode_id(mode: NotificationMode) -> &'static str {
    match mode {
        NotificationMode::AllMessages => "all",
        NotificationMode::MentionsOnly => "mentions",
        NotificationMode::Mute => "mute",
    }
}

/// The menu, and its submenu when open: both are drawn as popovers.
pub fn room_menu(account: &AccountData, room_id: &str, notifications_open: bool, p: &Palette) -> Vec<El> {
    let Some(room) = account.room(room_id) else { return Vec::new() };
    let (leave_label, leave_icon) = match room.kind {
        RoomKind::Group => (t!("room_menu.leave"), &icons::LEAVE),
        RoomKind::Direct => (t!("room_menu.delete_chat"), &icons::TRASH),
    };
    let leave = room.room_id.clone();
    // Read when it has anything unread (also when muted), else unread.
    let unread = !(room.marked_unread || room.unread > 0 || room.mentions > 0);
    let (unread_label, unread_icon) =
        if unread { (t!("room_menu.mark_unread"), &icons::UNREAD) } else { (t!("room_menu.mark_read"), &icons::CHECK) };
    let mark = room.room_id.clone();
    let favourite = room.priority == RoomPriority::Favourite;
    let (favourite_label, favourite_icon) = if favourite {
        (t!("room_menu.unfavourite"), &icons::STAR_OFF)
    } else {
        (t!("room_menu.favourite"), &icons::STAR)
    };
    let low = room.priority == RoomPriority::Low;
    let (low_label, low_icon) = if low {
        (t!("room_menu.not_low_priority"), &icons::ARROW_UP)
    } else {
        (t!("room_menu.low_priority"), &icons::ARROW_DOWN)
    };
    let (to_favourite, to_low) = (room.room_id.clone(), room.room_id.clone());
    let menu = popover_panel(p)
        .w(260.0)
        .key("room-menu")
        .child(text(room.name.clone()).font(font::SECTION).color(p.text_muted).px(12.0).py(6.0).truncate())
        .child(
            menu_action(unread_icon, unread_label, Tone::Normal, p)
                .key("room-menu-unread")
                .on_press(move |s: &mut State| s.set_unread(&mark, unread)),
        )
        .child(menu_action(favourite_icon, favourite_label, Tone::Normal, p).key("room-menu-favourite").on_press(
            move |s: &mut State| {
                let priority = if favourite { RoomPriority::Normal } else { RoomPriority::Favourite };
                s.set_room_priority(&to_favourite, priority);
            },
        ))
        .child(menu_action(low_icon, low_label, Tone::Normal, p).key("room-menu-low-priority").on_press(
            move |s: &mut State| {
                let priority = if low { RoomPriority::Normal } else { RoomPriority::Low };
                s.set_room_priority(&to_low, priority);
            },
        ))
        .child(divider(p))
        .child(
            submenu_row(&icons::BELL, t!("notify.heading"), notifications_open, p)
                .key(NOTIFICATIONS_ROW)
                .on_press(State::show_room_notifications)
                .on_hover_change(|s: &mut State, over, _| {
                    if over {
                        s.show_room_notifications();
                    }
                }),
        )
        .children(room.can_change_settings.then(|| {
            let id = room.room_id.clone();
            menu_action(&icons::SETTINGS, t!("room_settings.title"), Tone::Normal, p)
                .key("room-menu-settings")
                .on_press(move |s: &mut State| s.open_room_settings(&id))
        }))
        .child(divider(p))
        .child({
            let reported = room.room_id.clone();
            menu_action(&icons::FLAG, t!("report.room_action"), Tone::Normal, p)
                .key("room-menu-report")
                .on_press(move |s: &mut State| s.report_room(&reported))
        })
        .child(
            menu_action(leave_icon, leave_label, Tone::Danger, p)
                .key("room-menu-leave")
                .on_press(move |s: &mut State| s.ask_to_leave(&leave)),
        );
    let mut panels = vec![col().child(menu).anchored(format!("room-{room_id}"), Side::Right, Align::Start, 6.0)];
    if notifications_open {
        panels.push(notifications_menu(account, room, p));
    }
    panels
}

/// The three modes, then (after a rule) following the account default.
fn notifications_menu(account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let check = |on: bool| on.then(|| icon(&icons::CHECK, 18.0).color(p.accent));
    let mut panel = popover_panel(p).key("room-notifications-menu");
    for (mode, title, subtitle, glyph) in modes() {
        let chosen = room.custom_notifications && room.notifications == mode;
        let id = room.room_id.clone();
        panel = panel.child(
            menu_row(menu_icon(glyph, p), title, Some(subtitle), check(chosen), p)
                .key(format!("notify-{}", mode_id(mode)))
                .role("menuitemradio")
                .checked(chosen)
                .on_press(move |s: &mut State| s.set_notification_mode(&id, Some(mode))),
        );
    }
    let default = account.summary.default_notifications;
    let id = room.room_id.clone();
    panel = panel.child(divider(p)).child(
        menu_row(
            menu_icon(&icons::SETTINGS, p),
            t!("notify.default"),
            Some(&tf!("notify.default_hint", mode = mode_label(default))),
            check(!room.custom_notifications),
            p,
        )
        .key("notify-default")
        .role("menuitemradio")
        .checked(!room.custom_notifications)
        .on_press(move |s: &mut State| s.set_notification_mode(&id, None)),
    );
    col().child(panel).anchored(NOTIFICATIONS_ROW, Side::Right, Align::Start, 4.0)
}
