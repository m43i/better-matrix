//! Dialogs for encryption and sessions: verifying this session with another one or another person
//! (a QR code or emoji), entering the recovery key, the new recovery key shown once, resetting the
//! identity when both are lost, the password for a guarded action, and a session's name.

use matrix_core::{Person, VerificationStep};
use rune::{Color, El, Icon, col, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input, status_line};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{Guarded, Modal, RecoveryKeyFor, ResetStep, State};

/// The dialog for a security modal, if `modal` is one.
pub fn dialog_for(modal: &Modal, p: &Palette) -> Option<El> {
    Some(match modal {
        Modal::Verification { step, with, .. } => verification(step, with.as_ref(), p),
        Modal::RecoveryKeyEntry { edit, error, submitting, purpose, .. } => {
            recovery_entry(*purpose, edit, error.as_deref(), *submitting, p)
        }
        Modal::KeyFile(file) => super::account_dialogs::key_file(file, p),
        Modal::ChangePassword(form) => super::account_dialogs::change_password(form, p),
        Modal::LinkDevice { flow, .. } => super::account_dialogs::link_device(flow, p),
        Modal::NewRecoveryKey { key } => new_recovery_key(key, p),
        Modal::Password { action, edit, error, submitting, .. } => {
            password(action, edit, error.as_deref(), *submitting, p)
        }
        Modal::ResetIdentity { step, .. } => reset_identity(step, p),
        Modal::SessionEnded { account, kept } => session_ended(account.as_str(), *kept, p),
        Modal::RenameSession { edit, .. } => rename_session(edit, p),
        Modal::SectionName { section, edit } => section_name(section.is_some(), edit, p),
        Modal::AddWidget { name, url, .. } => add_widget(name, url, p),
        Modal::Forward { .. }
        | Modal::RoomForm { .. }
        | Modal::RoomSettings(_)
        | Modal::SpaceRooms { .. }
        | Modal::LeaveSpace { .. }
        | Modal::MemberLevel { .. }
        | Modal::Saved(_)
        | Modal::Translate(_)
        | Modal::Settings(_)
        | Modal::SignIn
        | Modal::Confirm(_)
        | Modal::Invite { .. }
        | Modal::Invitation { .. }
        | Modal::PublicRooms { .. }
        | Modal::Poll
        | Modal::Location
        | Modal::EditHistory(_)
        | Modal::Report(_)
        | Modal::Source(_)
        | Modal::Link(_) => return None,
    })
}

fn muted(message: &str, p: &Palette) -> El {
    text(message.to_owned()).font(font::BODY).color(p.text_muted).wrap()
}

/// The side of a verification: this session with another of the user's, or the user with
/// `with`. Both show this side's QR code once both are ready, or compare emoji.
fn verification(step: &VerificationStep, with: Option<&Person>, p: &Palette) -> El {
    let close = |label: &str| {
        button(label, ButtonKind::Secondary, true, p).key("verify-cancel").on_press(State::cancel_verification)
    };
    let title = match with {
        Some(person) => tf!("verify.person_title", name = person.name),
        None => t!("verify.title").to_owned(),
    };
    let (title, content, buttons): (String, El, Vec<El>) = match step {
        VerificationStep::Incoming { other_device, user } => {
            let message = match user {
                Some(person) => tf!("verify.incoming_person", name = person.name, id = person.id),
                None => tf!("verify.incoming", device = other_device),
            };
            (
                t!("verify.incoming_title").to_owned(),
                muted(&message, p),
                vec![
                    close(t!("verify.decline")),
                    button(t!("verify.accept"), ButtonKind::Primary, true, p)
                        .key("verify-accept")
                        .on_press(State::accept_verification),
                ],
            )
        }
        VerificationStep::Waiting => {
            let message = match with {
                Some(person) => tf!("verify.waiting_person", name = person.name),
                None => t!("verify.waiting").to_owned(),
            };
            (title, muted(&message, p), vec![close(t!("common.cancel"))])
        }
        VerificationStep::Ready { qr, .. } => {
            let emoji = button(t!("verify.compare_emoji"), ButtonKind::Secondary, true, p)
                .key("verify-emoji")
                .on_press(State::compare_emoji);
            let content = match qr {
                Some(code) => {
                    let hint = match with {
                        Some(person) => tf!("verify.scan_person", name = person.name),
                        None => t!("verify.scan").to_owned(),
                    };
                    col().w_full().gap(16.0).items_center().child(muted(&hint, p).w_full()).child(super::qr::qr_code(
                        "verify-qr",
                        code,
                        QR_SIDE,
                    ))
                }
                None => muted(t!("verify.emoji_only"), p),
            };
            (title, content, vec![close(t!("common.cancel")), emoji])
        }
        VerificationStep::Scanned => (
            title,
            muted(t!("verify.scanned"), p),
            vec![
                button(t!("verify.scanned_no"), ButtonKind::Secondary, true, p)
                    .key("verify-scanned-no")
                    .on_press(|s: &mut State| s.confirm_verification(false)),
                button(t!("verify.scanned_yes"), ButtonKind::Primary, true, p)
                    .key("verify-scanned-yes")
                    .on_press(|s: &mut State| s.confirm_verification(true)),
            ],
        ),
        VerificationStep::Compare(emojis) => {
            let grid = col().w_full().gap(16.0).children(emojis.chunks(4).map(|emojis| {
                row().w_full().children(emojis.iter().map(|emoji| {
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(4.0)
                        .items_center()
                        .child(text(emoji.symbol.clone()).font(rune::TextStyle::new(30.0, 400.0)))
                        .child(text(emoji.description.clone()).font(font::META).color(p.text_muted))
                }))
            }));
            let hint = match with {
                Some(person) => tf!("verify.compare_person", name = person.name),
                None => t!("verify.compare").to_owned(),
            };
            (
                title,
                col().w_full().gap(16.0).child(muted(&hint, p)).child(grid),
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
            let message = match with {
                Some(person) => tf!("verify.confirmed_person", name = person.name),
                None => t!("verify.confirmed").to_owned(),
            };
            (title, muted(&message, p), vec![close(t!("common.cancel"))])
        }
        VerificationStep::Done => {
            let (title, message) = match with {
                Some(person) => {
                    (tf!("verify.done_person_title", name = person.name), t!("verify.done_person").to_owned())
                }
                None => (t!("verify.done_title").to_owned(), t!("verify.done").to_owned()),
            };
            (
                title,
                muted(&message, p),
                vec![
                    button(t!("common.done"), ButtonKind::Primary, true, p)
                        .key("verify-done")
                        .on_press(State::close_modal),
                ],
            )
        }
        VerificationStep::Cancelled(reason) => {
            (title, muted(&tf!("verify.cancelled", reason = reason), p), vec![close(t!("common.close"))])
        }
    };
    dialog("verify-dialog", &title, content, buttons, p)
}

/// How large a QR code to scan shows, in points.
pub const QR_SIDE: f32 = 200.0;

/// The recovery key, to verify this session or to turn the offline device on.
fn recovery_entry(
    purpose: RecoveryKeyFor,
    edit: &rune::TextEdit,
    error: Option<&str>,
    submitting: bool,
    p: &Palette,
) -> El {
    let (title, hint, submit, working) = match purpose {
        RecoveryKeyFor::Verify => {
            (t!("recovery.enter_title"), t!("recovery.enter_hint"), t!("recovery.verify"), t!("recovery.verifying"))
        }
        RecoveryKeyFor::OfflineDevice => (
            t!("offline_device.enter_title"),
            t!("offline_device.enter_hint"),
            t!("offline_device.turn_on"),
            t!("offline_device.turning_on"),
        ),
    };
    let field = field_input(edit, t!("recovery.key_placeholder"), p)
        .key("recovery-key")
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.submit_recovery_key());
    let mut content = col().w_full().gap(10.0).child(muted(hint, p)).child(field);
    if let Some(error) = error {
        content = content.child(text(error.to_owned()).font(font::BODY).color(p.danger).wrap());
    }
    if purpose == RecoveryKeyFor::Verify {
        content = content.child(
            button(t!("reset.forgot"), ButtonKind::Link, true, p)
                .key("recovery-forgot")
                .on_press(State::start_identity_reset),
        );
    }
    dialog(
        "recovery-dialog",
        title,
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("recovery-cancel")
                .on_press(State::close_modal),
            button(if submitting { working } else { submit }, ButtonKind::Primary, !submitting, p)
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
        Guarded::ChangePassword { .. } => {
            (t!("reset.password_title"), t!("password.current_hint"), t!("password.change"))
        }
        Guarded::Deactivate { .. } => {
            (t!("reset.password_title"), t!("deactivate.password_hint"), t!("deactivate.confirm"))
        }
    };
    let kind = match action {
        Guarded::AddContact { .. } | Guarded::ChangePassword { .. } => ButtonKind::Primary,
        Guarded::EndSession { .. } | Guarded::ResetIdentity | Guarded::Deactivate { .. } => ButtonKind::Danger,
    };
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
/// Pinning a web page to a room as a widget: its name, and its address.
fn add_widget(name: &rune::TextEdit, url: &rune::TextEdit, p: &Palette) -> El {
    let content = col()
        .w_full()
        .gap(10.0)
        .child(
            field_input(name, t!("widgets.name"), p)
                .key("widget-form-name")
                .on_edit(|s: &mut State| s.widget_field(false))
                .on_submit(|s: &mut State, _| s.submit_widget()),
        )
        .child(
            field_input(url, t!("widgets.url"), p)
                .key("widget-form-url")
                .on_edit(|s: &mut State| s.widget_field(true))
                .on_submit(|s: &mut State, _| s.submit_widget()),
        );
    dialog(
        "widget-form",
        t!("widgets.add_title"),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("widget-form-cancel")
                .on_press(State::close_modal),
            button(t!("widgets.add_confirm"), ButtonKind::Primary, !url.text.trim().is_empty(), p)
                .key("widget-form-submit")
                .on_press(State::submit_widget),
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
