//! Dialogs for encryption and sessions: verifying this session with another one (emoji), entering
//! the recovery key, the new recovery key shown once, resetting the identity when both are lost,
//! the password for a guarded action, and a session's name.

use matrix_core::VerificationStep;
use rune::{Color, El, Icon, col, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input, status_line};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{Guarded, Modal, ResetStep, State};

/// The dialog for a security modal, if `modal` is one.
pub fn dialog_for(modal: &Modal, p: &Palette) -> Option<El> {
    Some(match modal {
        Modal::Verification { step, .. } => verification(step, p),
        Modal::RecoveryKeyEntry { edit, error, submitting, .. } => {
            recovery_entry(edit, error.as_deref(), *submitting, p)
        }
        Modal::NewRecoveryKey { key } => new_recovery_key(key, p),
        Modal::Password { action, edit, error, submitting, .. } => {
            password(action, edit, error.as_deref(), *submitting, p)
        }
        Modal::ResetIdentity { step, .. } => reset_identity(step, p),
        Modal::SessionEnded { account, kept } => session_ended(account.as_str(), *kept, p),
        Modal::RenameSession { edit, .. } => rename_session(edit, p),
        Modal::SectionName { section, edit } => section_name(section.is_some(), edit, p),
        Modal::RoomForm { form, name, topic } => room_form(form, name, topic, p),
        Modal::Forward { .. }
        | Modal::Translate(_)
        | Modal::Settings(_)
        | Modal::Confirm(_)
        | Modal::Invite { .. }
        | Modal::Invitation { .. }
        | Modal::PublicRooms { .. }
        | Modal::Poll
        | Modal::Link(_) => return None,
    })
}

fn muted(message: &str, p: &Palette) -> El {
    text(message.to_owned()).font(font::BODY).color(p.text_muted).wrap()
}

fn verification(step: &VerificationStep, p: &Palette) -> El {
    let close = |label: &str| {
        button(label, ButtonKind::Secondary, true, p).key("verify-cancel").on_press(State::cancel_verification)
    };
    let (title, content, buttons): (String, El, Vec<El>) = match step {
        VerificationStep::Incoming { other_device } => (
            t!("verify.incoming_title").to_owned(),
            muted(&tf!("verify.incoming", device = other_device), p),
            vec![
                close(t!("verify.decline")),
                button(t!("verify.accept"), ButtonKind::Primary, true, p)
                    .key("verify-accept")
                    .on_press(State::accept_verification),
            ],
        ),
        VerificationStep::Waiting => {
            (t!("verify.title").to_owned(), muted(t!("verify.waiting"), p), vec![close(t!("common.cancel"))])
        }
        VerificationStep::Compare(emojis) => {
            let grid = emojis.iter().fold(row().w_full().gap(4.0).flex_wrap().justify_center(), |grid, emoji| {
                grid.child(
                    col()
                        .w(52.0)
                        .gap(4.0)
                        .items_center()
                        .child(text(emoji.symbol.clone()).font(rune::TextStyle::new(30.0, 400.0)))
                        .child(text(emoji.description.clone()).font(font::META).color(p.text_muted)),
                )
            });
            (
                t!("verify.title").to_owned(),
                col().w_full().gap(16.0).child(muted(t!("verify.compare"), p)).child(grid),
                vec![
                    button(t!("verify.mismatch"), ButtonKind::Secondary, true, p)
                        .key("verify-mismatch")
                        .on_press(|s: &mut State| s.confirm_verification(false)),
                    button(t!("verify.match"), ButtonKind::Primary, true, p)
                        .key("verify-match")
                        .on_press(|s: &mut State| s.confirm_verification(true)),
                ],
            )
        }
        VerificationStep::Confirmed => {
            (t!("verify.title").to_owned(), muted(t!("verify.confirmed"), p), vec![close(t!("common.cancel"))])
        }
        VerificationStep::Done => (
            t!("verify.done_title").to_owned(),
            muted(t!("verify.done"), p),
            vec![
                button(t!("common.done"), ButtonKind::Primary, true, p).key("verify-done").on_press(State::close_modal),
            ],
        ),
        VerificationStep::Cancelled(reason) => (
            t!("verify.title").to_owned(),
            muted(&tf!("verify.cancelled", reason = reason), p),
            vec![close(t!("common.close"))],
        ),
    };
    dialog("verify-dialog", &title, content, buttons, p)
}

fn recovery_entry(edit: &rune::TextEdit, error: Option<&str>, submitting: bool, p: &Palette) -> El {
    let field = field_input(edit, t!("recovery.key_placeholder"), p)
        .key("recovery-key")
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.submit_recovery_key());
    let mut content = col().w_full().gap(10.0).child(muted(t!("recovery.enter_hint"), p)).child(field);
    if let Some(error) = error {
        content = content.child(text(error.to_owned()).font(font::BODY).color(p.danger).wrap());
    }
    content = content.child(
        button(t!("reset.forgot"), ButtonKind::Link, true, p)
            .key("recovery-forgot")
            .on_press(State::start_identity_reset),
    );
    dialog(
        "recovery-dialog",
        t!("recovery.enter_title"),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("recovery-cancel")
                .on_press(State::close_modal),
            button(
                if submitting { t!("recovery.verifying") } else { t!("recovery.verify") },
                ButtonKind::Primary,
                !submitting,
                p,
            )
            .key("recovery-submit")
            .on_press(State::submit_recovery_key),
        ],
        p,
    )
}

fn new_recovery_key(key: &str, p: &Palette) -> El {
    let shown = row()
        .key("recovery-key-shown")
        .w_full()
        .padding(14.0)
        .radius(8.0)
        .fill(p.bg)
        .border(1.0, p.divider)
        .child(text(key.to_owned()).font(font::BODY.mono()).color(p.text).wrap().selectable(true));
    dialog(
        "new-key-dialog",
        t!("recovery.new_title"),
        col().w_full().gap(12.0).child(muted(t!("recovery.new_hint"), p)).child(shown),
        vec![
            button(t!("recovery.copy"), ButtonKind::Secondary, true, p)
                .key("recovery-copy")
                .on_press_tx(State::copy_recovery_key),
            button(t!("recovery.saved"), ButtonKind::Primary, true, p)
                .key("recovery-saved")
                .on_press(State::close_modal),
        ],
        p,
    )
}

/// The password for an action the server guards with it; what it confirms depends on the action.
fn password(action: &Guarded, edit: &rune::TextEdit, error: Option<&str>, submitting: bool, p: &Palette) -> El {
    let (title, hint, confirm) = match action {
        Guarded::EndSession { .. } => {
            (t!("sessions.sign_out_title"), t!("sessions.password_hint"), t!("sessions.sign_out"))
        }
        Guarded::ResetIdentity => (t!("reset.password_title"), t!("reset.password_hint"), t!("reset.confirm")),
        Guarded::AddContact { .. } => (t!("reset.password_title"), t!("profile.password_hint"), t!("profile.add")),
    };
    let kind = if matches!(action, Guarded::AddContact { .. }) { ButtonKind::Primary } else { ButtonKind::Danger };
    let field = field_input(edit, t!("sign_in.password"), p)
        .key("guard-password")
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.confirm_with_password());
    let mut content = col().w_full().gap(10.0).child(muted(hint, p)).child(field);
    if let Some(error) = error {
        content = content.child(text(error.to_owned()).font(font::BODY).color(p.danger).wrap());
    }
    dialog(
        "password-dialog",
        title,
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("password-cancel")
                .on_press(State::close_modal),
            button(confirm, kind, !submitting, p).key("password-submit").on_press(State::confirm_with_password),
        ],
        p,
    )
}

/// One line of what a reset keeps or costs.
fn consequence(glyph: &'static Icon, message: &str, color: Color, p: &Palette) -> El {
    row()
        .w_full()
        .gap(10.0)
        .child(icon(glyph, 18.0).color(color).mt(2.0))
        .child(text(message.to_owned()).font(font::BODY).color(p.text).grow(1.0).wrap())
}

fn reset_identity(step: &ResetStep, p: &Palette) -> El {
    let cancel =
        |label: &str| button(label, ButtonKind::Secondary, true, p).key("reset-cancel").on_press(State::close_modal);
    let (title, content, buttons) = match step {
        ResetStep::Warning { error } => {
            let mut content = col()
                .w_full()
                .gap(12.0)
                .child(muted(t!("reset.warning"), p))
                .child(consequence(&icons::CHECK, t!("reset.keeps"), p.success, p))
                .child(consequence(&icons::ALERT, t!("reset.loses_history"), p.warning, p))
                .child(consequence(&icons::ALERT, t!("reset.verify_again"), p.warning, p));
            if let Some(error) = error {
                content = content.child(text(error.clone()).font(font::BODY).color(p.danger).wrap());
            }
            let confirm = if error.is_some() { t!("common.try_again") } else { t!("reset.confirm") };
            (
                t!("reset.title"),
                content,
                vec![
                    cancel(t!("common.cancel")),
                    button(confirm, ButtonKind::Danger, true, p)
                        .key("reset-confirm")
                        .on_press(State::confirm_identity_reset),
                ],
            )
        }
        ResetStep::Resetting { url: None, .. } => {
            (t!("reset.title"), muted(t!("reset.resetting"), p), vec![cancel(t!("common.cancel"))])
        }
        ResetStep::Resetting { url: Some(_), .. } => (
            t!("reset.approve_title"),
            muted(t!("reset.approve"), p),
            vec![
                button(t!("reset.open_again"), ButtonKind::Secondary, true, p)
                    .key("reset-reopen")
                    .on_press_tx(State::reopen_reset_approval),
                cancel(t!("common.cancel")),
            ],
        ),
        ResetStep::Done => (
            t!("reset.done_title"),
            col()
                .w_full()
                .gap(12.0)
                .child(status_line(&icons::CHECK, t!("reset.done"), p.success, p))
                .child(muted(t!("reset.done_hint"), p)),
            vec![
                cancel(t!("reset.later")),
                button(t!("recovery.set_up"), ButtonKind::Primary, true, p)
                    .key("reset-set-up-recovery")
                    .on_press(State::set_up_recovery_after_reset),
            ],
        ),
    };
    dialog("reset-dialog", title, content, buttons, p)
}

/// The server ended an account's session: it is signed out here.
/// Signed out by the server. Signed out softly, the account's data waits on this device: signing
/// in again carries on from it, or the user removes it.
fn session_ended(account: &str, kept: bool, p: &Palette) -> El {
    let mut buttons = Vec::new();
    if kept {
        buttons.push(
            button(t!("session_ended.remove"), ButtonKind::Secondary, true, p)
                .key("session-ended-remove")
                .on_press(State::remove_signed_out),
        );
    }
    buttons.push(
        button(t!("reset.later"), ButtonKind::Secondary, true, p)
            .key("session-ended-later")
            .on_press(State::close_modal),
    );
    buttons.push(
        button(t!("session_ended.sign_in"), ButtonKind::Primary, true, p)
            .key("session-ended-sign-in")
            .on_press_tx(crate::state::navigate(State::sign_in_again)),
    );
    let hint = if kept { t!("session_ended.kept_hint") } else { t!("session_ended.hint") };
    dialog("session-ended-dialog", &tf!("session_ended.title", account = account), muted(hint, p), buttons, p)
}

/// Naming a new room or space, or renaming one; a new room also takes a topic.
fn room_form(form: &crate::state::RoomForm, name: &rune::TextEdit, topic: &rune::TextEdit, p: &Palette) -> El {
    use crate::state::RoomForm;
    let (title, confirm) = match form {
        RoomForm::NewRoom { .. } => (t!("create.room_title"), t!("create.create")),
        RoomForm::NewSpace => (t!("create.space_title"), t!("create.create")),
        RoomForm::Rename { .. } => (t!("create.rename_title"), t!("common.save")),
    };
    let mut content = col().w_full().gap(10.0).child(
        field_input(name, t!("create.name"), p)
            .key("room-form-name")
            .on_edit(|s: &mut State| s.room_form_field(false))
            .on_submit(|s: &mut State, _| s.submit_room_form()),
    );
    if matches!(form, RoomForm::NewRoom { .. }) {
        content = content.child(
            field_input(topic, t!("create.topic"), p)
                .key("room-form-topic")
                .on_edit(|s: &mut State| s.room_form_field(true))
                .on_submit(|s: &mut State, _| s.submit_room_form()),
        );
    }
    dialog(
        "room-form",
        title,
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("room-form-cancel")
                .on_press(State::close_modal),
            button(confirm, ButtonKind::Primary, !name.text.trim().is_empty(), p)
                .key("room-form-submit")
                .on_press(State::submit_room_form),
        ],
        p,
    )
}

/// Naming a sidebar section, new or renamed.
fn section_name(renaming: bool, edit: &rune::TextEdit, p: &Palette) -> El {
    let field = field_input(edit, t!("sections.name_placeholder"), p)
        .key("section-name")
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.submit_section_name());
    let title = if renaming { t!("sections.rename_title") } else { t!("sections.new_title") };
    dialog(
        "section-dialog",
        title,
        field,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("section-cancel")
                .on_press(State::close_modal),
            button(t!("common.save"), ButtonKind::Primary, !edit.text.trim().is_empty(), p)
                .key("section-save")
                .on_press(State::submit_section_name),
        ],
        p,
    )
}

fn rename_session(edit: &rune::TextEdit, p: &Palette) -> El {
    let field = field_input(edit, t!("sessions.name_placeholder"), p)
        .key("session-name")
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.submit_session_name());
    dialog(
        "rename-dialog",
        t!("sessions.rename_title"),
        field,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("rename-cancel")
                .on_press(State::close_modal),
            button(t!("common.save"), ButtonKind::Primary, true, p)
                .key("rename-save")
                .on_press(State::submit_session_name),
        ],
        p,
    )
}
