//! The report dialog (ADR 0011): what is reported, an optional reason, and ignoring them too.

use matrix_core::ReportTarget;
use rune::{El, col, el, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input, labelled};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{ReportForm, State};

pub fn report(form: &ReportForm, p: &Palette) -> El {
    let (title, body) = match &form.target {
        ReportTarget::Message { .. } => (t!("report.message_title").to_owned(), t!("report.message_body")),
        ReportTarget::Room { .. } => (tf!("report.room_title", room = form.name), t!("report.room_body")),
        ReportTarget::Person { .. } => (tf!("report.person_title", name = form.name), t!("report.person_body")),
    };
    let idle = form.sending.is_none();
    let reason = field_input(&form.reason, t!("report.reason_placeholder"), p)
        .key("report-reason")
        .label(t!("report.reason"))
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.send_report());
    let mut content = col()
        .w_full()
        .gap(14.0)
        .child(text(body).font(font::BODY).color(p.text_muted).wrap())
        .child(labelled(t!("report.reason"), reason, p));
    if form.person.is_some() {
        let name = if form.name.is_empty() { t!("report.them").to_owned() } else { form.name.clone() };
        content = content.child(checkbox(&tf!("report.also_ignore", name = name), form.also_ignore, idle, p));
    }
    if !idle {
        content = content.child(text(t!("report.sending")).key("report-sending").font(font::META).color(p.text_muted));
    }
    dialog(
        "report-dialog",
        &title,
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("report-cancel")
                .on_press(State::close_modal),
            button(t!("report.send"), ButtonKind::Danger, idle, p).key("report-send").on_press(State::send_report),
        ],
        p,
    )
}

/// "Also ignore them": a box ticked or not, with its words beside it.
fn checkbox(label: &str, checked: bool, enabled: bool, p: &Palette) -> El {
    let mut tick =
        el().size(18.0, 18.0).radius(4.0).center().border(2.0, if checked { p.accent } else { p.text_muted });
    if checked {
        tick = tick.fill(p.accent).child(icon(&icons::CHECK, 14.0).color(p.on_accent));
    }
    row()
        .key("report-ignore")
        .w_full()
        .py(4.0)
        .gap(10.0)
        .center_y()
        .role("checkbox")
        .cursor(rune::Cursor::Pointer)
        .label(label.to_owned())
        .checked(checked)
        .focusable(enabled)
        .opacity(if enabled { 1.0 } else { 0.55 })
        .child(tick)
        .child(text(label.to_owned()).font(font::BODY).color(p.text).grow(1.0).wrap())
        .on_press(State::toggle_report_ignore)
}
