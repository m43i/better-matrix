//! Sending later: the menu beside the send button (when to send the message being written),
//! and the room's list of scheduled messages, each with send now and cancel.

use matrix_core::ScheduledAction;
use rune::{Align, El, ScrollSpec, Side, col, row, text};

use super::components::{icon_button, menu_action, popover_panel, when};
use super::composer::{SCHEDULED_BAR, schedule_button};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{ComposerTarget, State, When, local_offset, now_ms};

/// When to send a composer's message: half an hour, an hour, tomorrow morning, next Monday.
pub fn menu(target: ComposerTarget, p: &Palette) -> El {
    let now = now_ms();
    let offset = local_offset(now);
    let mut panel = popover_panel(p)
        .key("schedule-menu")
        .w(280.0)
        .preserve_focus_on_press(true)
        .child(text(t!("schedule.title")).font(font::SECTION).color(p.text_muted).px(12.0).pt(2.0).pb(4.0));
    let tomorrow = When::TomorrowMorning.at(now, offset);
    // On a Sunday, next Monday morning is tomorrow morning: offered once.
    for choice in
        When::ALL.into_iter().filter(|choice| *choice != When::MondayMorning || choice.at(now, offset) != tomorrow)
    {
        let label = match choice {
            When::HalfHour => t!("schedule.half_hour"),
            When::Hour => t!("schedule.hour"),
            When::TomorrowMorning => t!("schedule.tomorrow"),
            When::MondayMorning => t!("schedule.monday"),
        };
        let at = when(choice.at(now, offset));
        panel = panel.child(
            menu_action(&icons::CLOCK, &format!("{label} · {at}"), super::components::Tone::Normal, p)
                .key(format!("schedule-{}", choice.id()))
                .on_press(move |s: &mut State| s.schedule_message(target, choice)),
        );
    }
    panel = panel.child(text(t!("schedule.hint")).font(font::META).color(p.text_muted).px(12.0).py(6.0).wrap());
    col().child(panel).anchored(schedule_button(target), Side::Top, Align::End, 6.0)
}

/// The room's scheduled messages, soonest first: when each goes, what it says, and ways to send
/// it now or cancel it.
pub fn list(s: &State, p: &Palette) -> El {
    let mut rows = col().w_full().gap(2.0).px(6.0);
    for message in s.scheduled_here() {
        let body = message.body.clone().unwrap_or_else(|| t!("schedule.unreadable").to_owned());
        let (send, cancel) = (message.delay_id.clone(), message.delay_id.clone());
        rows = rows.child(
            row()
                .key(format!("scheduled-{}", message.delay_id))
                .w_full()
                .px(8.0)
                .py(6.0)
                .gap(8.0)
                .center_y()
                .radius(6.0)
                .hover(p.hover)
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(2.0)
                        .child(text(when(message.send_at_ms)).font(font::META).color(p.accent))
                        .child(text(body).font(font::BODY).color(p.text).max_lines(2).truncate()),
                )
                .child(
                    icon_button(&icons::SEND, t!("schedule.send_now"), false, p.text_muted, p)
                        .key(format!("scheduled-send-{}", message.delay_id))
                        .help(t!("schedule.send_now"))
                        .on_press(move |s: &mut State| s.update_scheduled(&send, ScheduledAction::SendNow)),
                )
                .child(
                    icon_button(&icons::TRASH, t!("schedule.cancel"), false, p.text_muted, p)
                        .key(format!("scheduled-cancel-{}", message.delay_id))
                        .help(t!("schedule.cancel"))
                        .on_press(move |s: &mut State| s.update_scheduled(&cancel, ScheduledAction::Cancel)),
                ),
        );
    }
    let panel = popover_panel(p)
        .key("scheduled-list")
        .w(360.0)
        .child(text(t!("schedule.list_title")).font(font::SECTION).color(p.text_muted).px(12.0).pt(2.0).pb(4.0))
        .child(col().w_full().max_h(320.0).scroll(ScrollSpec { stick_bottom: false }).child(rows));
    col().child(panel).anchored(SCHEDULED_BAR, Side::Top, Align::Start, 6.0)
}
