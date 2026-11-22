//! Dialogs the Account and Sessions pages open: a key export or import's passphrase, a new
//! password, and linking a new device with a QR code. The QR sign-in's steps are shared with the
//! sign-in card, which shows the new device's side.

use matrix_core::QrLoginStep;
use rune::{El, col, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input, labelled, status_line};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{KeyFile, KeyFileStep, KeyTransfer, PasswordForm, QrLogin, State};

fn muted(message: &str, p: &Palette) -> El {
    text(message.to_owned()).font(font::BODY).color(p.text_muted).wrap().w_full()
}

fn error_line(error: &str, p: &Palette) -> El {
    text(error.to_owned()).key("dialog-error").font(font::BODY).color(p.danger).wrap()
}

fn cancel(label: &str, p: &Palette) -> El {
    button(label, ButtonKind::Secondary, true, p).key("dialog-cancel").on_press(State::close_modal)
}

/// A key export's or import's passphrase, then how it went.
pub fn key_file(file: &KeyFile, p: &Palette) -> El {
    let export = file.transfer == KeyTransfer::Export;
    let title = if export { t!("keys.export_title") } else { t!("keys.import_title") };
    if let KeyFileStep::Done(message) = &file.step {
        return dialog(
            "key-file-dialog",
            title,
            status_line(&icons::CHECK, message, p.success, p),
            vec![
                button(t!("common.done"), ButtonKind::Primary, true, p)
                    .key("key-file-done")
                    .on_press(State::close_modal),
            ],
            p,
        );
    }
    let working = matches!(file.step, KeyFileStep::Working(_));
    let path = row().w_full().gap(8.0).center_y().child(icon(&icons::FILE, 16.0).color(p.text_muted)).child(
        text(file.path.display().to_string())
            .key("key-file-path")
            .font(font::META)
            .color(p.text_muted)
            .grow(1.0)
            .min_w(0.0)
            .truncate(),
    );
    let passphrase = field_input(&file.passphrase, t!("keys.passphrase"), p)
        .key("key-passphrase")
        .on_edit(|s: &mut State| s.modal_field_at(0))
        .on_submit(|s: &mut State, _| s.submit_key_file());
    let mut content = col()
        .w_full()
        .gap(12.0)
        .child(muted(if export { t!("keys.export_hint") } else { t!("keys.import_hint") }, p))
        .child(path)
        .child(labelled(t!("keys.passphrase"), passphrase, p));
    if export {
        let confirm = field_input(&file.confirm, t!("keys.passphrase_again"), p)
            .key("key-passphrase-confirm")
            .on_edit(|s: &mut State| s.modal_field_at(1))
            .on_submit(|s: &mut State, _| s.submit_key_file());
        content = content.child(labelled(t!("keys.passphrase_again"), confirm, p));
    }
    if let Some(error) = &file.error {
        content = content.child(error_line(error, p));
    }
    let label = match (export, working) {
        (true, false) => t!("keys.export"),
        (true, true) => t!("keys.exporting"),
        (false, false) => t!("keys.import"),
        (false, true) => t!("keys.importing"),
    };
    dialog(
        "key-file-dialog",
        title,
        content,
        vec![
            cancel(t!("common.cancel"), p),
            button(label, ButtonKind::Primary, !working, p).key("key-file-submit").on_press(State::submit_key_file),
        ],
        p,
    )
}

/// The current password and the new one twice; then that it changed.
pub fn change_password(form: &PasswordForm, p: &Palette) -> El {
    if form.done {
        return dialog(
            "password-change-dialog",
            t!("password.changed_title"),
            col()
                .w_full()
                .gap(12.0)
                .child(status_line(&icons::CHECK, t!("password.changed"), p.success, p))
                .child(muted(t!("password.changed_hint"), p)),
            vec![
                button(t!("common.done"), ButtonKind::Primary, true, p)
                    .key("password-change-done")
                    .on_press(State::close_modal),
            ],
            p,
        );
    }
    let field = |edit: &rune::TextEdit, key: &str, label: &str, index: usize| {
        labelled(
            label,
            field_input(edit, label, p)
                .key(key.to_owned())
                .on_edit(move |s: &mut State| s.modal_field_at(index))
                .on_submit(|s: &mut State, _| s.submit_password_change()),
            p,
        )
    };
    let mut content = col()
        .w_full()
        .gap(12.0)
        .child(muted(t!("password.change_hint"), p))
        .child(field(&form.current, "password-current", t!("password.current"), 0))
        .child(field(&form.new, "password-new", t!("password.new"), 1))
        .child(field(&form.confirm, "password-confirm", t!("password.confirm"), 2));
    if let Some(error) = &form.error {
        content = content.child(error_line(error, p));
    }
    dialog(
        "password-change-dialog",
        t!("password.change_title"),
        content,
        vec![
            cancel(t!("common.cancel"), p),
            button(
                if form.submitting { t!("password.changing") } else { t!("password.change") },
                ButtonKind::Primary,
                !form.submitting,
                p,
            )
            .key("password-change-submit")
            .on_press(State::submit_password_change),
        ],
        p,
    )
}

/// Which device a QR sign-in's steps are told to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QrSide {
    /// The signed-in session, linking a new device.
    Linking,
    /// The new device, signing in.
    SigningIn,
}

/// Where a QR sign-in stands, for either device: the code to scan, the check code to type or
/// show, the approval, and the end.
pub fn qr_login_steps(flow: &QrLogin, side: QrSide, p: &Palette) -> El {
    let linking = side == QrSide::Linking;
    let content = col().w_full().gap(16.0).items_center();
    let content = match &flow.step {
        _ if flow.request.is_none() && flow.error.is_some() => content,
        QrLoginStep::Starting => content.child(muted(t!("qr_login.starting"), p)),
        QrLoginStep::Show(code) => {
            let hint = if linking { t!("qr_login.show_linking") } else { t!("qr_login.show_signing_in") };
            content.child(muted(hint, p)).child(super::qr::qr_code("qr-login-code", code, super::security::QR_SIDE))
        }
        QrLoginStep::EnterCheckCode => {
            let hint = if linking { t!("qr_login.enter_code_linking") } else { t!("qr_login.enter_code") };
            let field = field_input(&flow.code, "00", p)
                .key("check-code")
                .w(96.0)
                .on_edit(State::check_code_field)
                .on_submit(|s: &mut State, _| s.submit_check_code());
            content.child(muted(hint, p)).child(field)
        }
        QrLoginStep::CheckCode(code) => content
            .child(muted(t!("qr_login.type_code"), p))
            .child(text(format!("{code:02}")).key("check-code-shown").font(font::TITLE).color(p.text)),
        QrLoginStep::Approve { .. } => content.child(muted(t!("qr_login.approve"), p)),
        QrLoginStep::Waiting { code } => {
            let content = content.child(muted(t!("qr_login.waiting"), p));
            match code {
                Some(code) => content
                    .child(muted(t!("qr_login.waiting_code"), p))
                    .child(text(code.clone()).key("qr-login-waiting-code").font(font::TITLE).color(p.text)),
                None => content,
            }
        }
        QrLoginStep::Syncing => {
            content.child(muted(if linking { t!("qr_login.syncing_linking") } else { t!("qr_login.syncing") }, p))
        }
        QrLoginStep::Done { .. } => content.child(status_line(
            &icons::CHECK,
            if linking { t!("qr_login.done_linking") } else { t!("qr_login.done") },
            p.success,
            p,
        )),
    };
    // The sign-in card shows its errors itself.
    match &flow.error {
        Some(error) if flow.request.is_none() && linking => content.child(error_line(error, p)),
        _ => content,
    }
}

/// The step's main action, if it has one: send the check code, or open the approval again.
pub fn qr_login_action(flow: &QrLogin, side: QrSide, p: &Palette) -> Option<El> {
    flow.request?;
    match &flow.step {
        QrLoginStep::EnterCheckCode => Some(
            button(t!("common.continue"), ButtonKind::Primary, flow.check_code().is_some() && !flow.answered, p)
                .key("check-code-submit")
                .on_press(State::submit_check_code),
        ),
        QrLoginStep::Approve { .. } if side == QrSide::Linking => Some(
            button(t!("reset.open_again"), ButtonKind::Secondary, true, p)
                .key("qr-login-reopen")
                .on_press_tx(State::reopen_link_approval),
        ),
        _ => None,
    }
}

/// Linking a new device: this session shows a code for it to scan, then approves it.
pub fn link_device(flow: &QrLogin, p: &Palette) -> El {
    let mut buttons = Vec::new();
    if matches!(flow.step, QrLoginStep::Done { .. }) {
        buttons.push(
            button(t!("common.done"), ButtonKind::Primary, true, p)
                .key("link-device-done")
                .on_press(State::close_modal),
        );
    } else {
        buttons.push(cancel(if flow.request.is_none() { t!("common.close") } else { t!("common.cancel") }, p));
        if flow.request.is_none() {
            buttons.push(
                button(t!("common.try_again"), ButtonKind::Primary, true, p)
                    .key("link-device-retry")
                    .on_press(State::link_device),
            );
        }
        buttons.extend(qr_login_action(flow, QrSide::Linking, p));
    }
    dialog("link-device-dialog", t!("link_device.title"), qr_login_steps(flow, QrSide::Linking, p), buttons, p)
}
