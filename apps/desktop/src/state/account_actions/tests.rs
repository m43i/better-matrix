use matrix_core::{
    AccountSummary, Answer, Command, DetachedCommands, EngineHandle, Event, Identity, KeyImport, NotificationMode,
    OfflineDevice, Password, Person, QrImage, QrLoginStep, RecoveryStatus, RequestId, Security, SignInMethod,
    VerificationStep,
};
use rune::{TextEdit, Tx};

use super::super::{Confirm, Guarded, Modal, RecoveryKeyFor, SidePanel, State, VerificationFlow};
use super::{KeyFileStep, KeyTransfer};
use crate::platform::Shell;
use crate::state::Settings;

const ME: &str = "@a:example.org";

fn state(sign_in: SignInMethod) -> (State, DetachedCommands) {
    let (engine, commands) = EngineHandle::detached();
    let mut s = State::new(engine, Shell::headless(), Settings::fixed_english());
    let summary = AccountSummary {
        id: matrix_core::AccountId::new(ME),
        display_name: "A".into(),
        homeserver: "example.org".into(),
        default_notifications: NotificationMode::AllMessages,
        avatar: None,
        sign_in,
    };
    apply(&mut s, Event::AccountAdded(summary));
    let security =
        Security { session_verified: true, recovery: RecoveryStatus::Enabled, offline_device: OfflineDevice::Off };
    apply(&mut s, Event::SecurityChanged { account: account(), security });
    let mut commands = commands;
    drain(&mut commands);
    (s, commands)
}

fn account() -> matrix_core::AccountId {
    matrix_core::AccountId::new(ME)
}

fn apply(s: &mut State, event: Event) {
    s.apply(event, &mut Tx::default());
}

fn drain(commands: &mut DetachedCommands) -> Vec<(RequestId, Command)> {
    std::iter::from_fn(|| commands.try_next()).collect()
}

fn grace() -> Person {
    Person { id: "@grace:example.org".into(), name: "Grace".into(), avatar: None }
}

fn sent(commands: &mut DetachedCommands) -> Vec<Command> {
    drain(commands).into_iter().map(|(_, command)| command).collect()
}

// ----- verifying people --------------------------------------------------------------------

#[test]
fn verifying_a_person_should_ask_them_and_open_the_dialog_for_them() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.verify_person(&grace());
    assert!(
        matches!(&sent(&mut commands)[..], [Command::VerifyUser { user_id, .. }] if user_id == "@grace:example.org")
    );
    assert!(
        matches!(&s.modal, Some(Modal::Verification { with: Some(person), flow: VerificationFlow::Requested(_), .. }) if person.id == "@grace:example.org")
    );
}

/// Asks to verify Grace; the engine names the flow it started, `f`.
fn verifying_grace(s: &mut State, commands: &mut DetachedCommands) {
    s.verify_person(&grace());
    let (request, _) = drain(commands).remove(0);
    apply(s, Event::VerificationRequested { request, account: account(), flow_id: "f".into() });
}

fn ready() -> VerificationStep {
    VerificationStep::Ready { qr: None, scan: false }
}

#[test]
fn the_dialog_should_follow_the_flow_its_request_names() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    assert!(matches!(&s.modal, Some(Modal::Verification { flow: VerificationFlow::Following(f), .. }) if f == "f"));
}

#[test]
fn a_request_from_someone_else_should_not_take_over_a_verification_being_asked_for() {
    let (mut s, _) = state(SignInMethod::Password);
    s.verify_person(&grace());
    let bob = Person { id: "@bob:example.org".into(), name: "Bob".into(), avatar: None };
    let step = VerificationStep::Incoming { other_device: "BOBPHONE".into(), user: Some(bob) };
    apply(&mut s, Event::Verification { account: account(), flow_id: "bob".into(), step });
    assert!(matches!(
        &s.modal,
        Some(Modal::Verification { with: Some(person), flow: VerificationFlow::Requested(_), step: VerificationStep::Waiting, .. })
            if person.id == "@grace:example.org"
    ));
}

#[test]
fn steps_of_another_flow_should_not_change_a_dialog_waiting_for_its_own() {
    let (mut s, _) = state(SignInMethod::Password);
    s.verify_person(&grace());
    apply(&mut s, Event::Verification { account: account(), flow_id: "other".into(), step: ready() });
    assert!(matches!(&s.modal, Some(Modal::Verification { step: VerificationStep::Waiting, .. })));
}

#[test]
fn steps_of_another_flow_should_not_change_the_verification_followed() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    let step = VerificationStep::Cancelled("elsewhere".into());
    apply(&mut s, Event::Verification { account: account(), flow_id: "other".into(), step });
    assert!(matches!(&s.modal, Some(Modal::Verification { step: VerificationStep::Waiting, .. })));
}

#[test]
fn a_request_from_someone_else_should_not_replace_a_verification_in_progress() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    apply(&mut s, Event::Verification { account: account(), flow_id: "f".into(), step: ready() });
    let step = VerificationStep::Incoming { other_device: "PHONE".into(), user: None };
    apply(&mut s, Event::Verification { account: account(), flow_id: "bob".into(), step });
    assert!(matches!(&s.modal, Some(Modal::Verification { flow: VerificationFlow::Following(f), .. }) if f == "f"));
}

#[test]
fn a_request_should_open_its_dialog_once_the_verification_before_it_ended() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    apply(&mut s, Event::Verification { account: account(), flow_id: "f".into(), step: VerificationStep::Done });
    let step = VerificationStep::Incoming { other_device: "PHONE".into(), user: None };
    apply(&mut s, Event::Verification { account: account(), flow_id: "next".into(), step });
    assert!(matches!(&s.modal, Some(Modal::Verification { flow: VerificationFlow::Following(f), .. }) if f == "next"));
}

#[test]
fn a_verification_closed_before_it_started_should_be_stopped_once_it_starts() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.verify_person(&grace());
    let (request, _) = drain(&mut commands).remove(0);
    s.cancel_verification();
    apply(&mut s, Event::VerificationRequested { request, account: account(), flow_id: "f".into() });
    assert!(matches!(&sent(&mut commands)[..], [Command::CancelVerification { flow_id, .. }] if flow_id == "f"));
}

#[test]
fn a_verification_closed_before_it_started_should_end_without_a_notice() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.verify_person(&grace());
    let (request, _) = drain(&mut commands).remove(0);
    s.cancel_verification();
    apply(&mut s, Event::Finished { request, result: Err("cancelled".into()) });
    assert!(s.notice.is_none() && s.modal.is_none());
}

#[test]
fn a_verification_both_sides_are_ready_for_should_show_its_code() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    let qr = QrImage::encode(b"code").unwrap();
    let step = VerificationStep::Ready { qr: Some(qr), scan: false };
    apply(&mut s, Event::Verification { account: account(), flow_id: "f".into(), step });
    assert!(matches!(&s.modal, Some(Modal::Verification { step: VerificationStep::Ready { qr: Some(_), .. }, .. })));
}

#[test]
fn comparing_emoji_instead_should_ask_the_open_verification() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    apply(
        &mut s,
        Event::Verification {
            account: account(),
            flow_id: "f".into(),
            step: VerificationStep::Ready { qr: None, scan: false },
        },
    );
    drain(&mut commands);
    s.compare_emoji();
    assert!(matches!(&sent(&mut commands)[..], [Command::CompareEmoji { flow_id, .. }] if flow_id == "f"));
}

#[test]
fn answering_that_the_code_was_scanned_should_confirm_the_verification() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    verifying_grace(&mut s, &mut commands);
    apply(&mut s, Event::Verification { account: account(), flow_id: "f".into(), step: VerificationStep::Scanned });
    drain(&mut commands);
    s.confirm_verification(true);
    assert!(matches!(&sent(&mut commands)[..], [Command::ConfirmVerification { matches: true, .. }]));
}

#[test]
fn an_incoming_request_from_a_person_should_show_who_asks() {
    let (mut s, _) = state(SignInMethod::Password);
    let step = VerificationStep::Incoming { other_device: "PHONE".into(), user: Some(grace()) };
    apply(&mut s, Event::Verification { account: account(), flow_id: "f".into(), step });
    assert!(matches!(&s.modal, Some(Modal::Verification { with: Some(person), .. }) if person.name == "Grace"));
}

#[test]
fn a_verification_that_cannot_start_should_say_why_in_its_dialog() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.verify_person(&grace());
    let (request, _) = drain(&mut commands).remove(0);
    apply(&mut s, Event::Finished { request, result: Err("no direct chat".into()) });
    assert!(
        matches!(&s.modal, Some(Modal::Verification { step: VerificationStep::Cancelled(reason), .. }) if reason == "no direct chat")
    );
}

#[test]
fn opening_someones_profile_should_ask_whether_they_are_verified() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.show_side(SidePanel::Person(grace()));
    assert!(
        sent(&mut commands)
            .iter()
            .any(|command| matches!(command, Command::LoadIdentity { user_id, .. } if user_id == "@grace:example.org"))
    );
}

#[test]
fn opening_ones_own_profile_should_not_ask_about_an_identity() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.show_side(SidePanel::Person(Person { id: ME.into(), name: "A".into(), avatar: None }));
    assert!(!sent(&mut commands).iter().any(|command| matches!(command, Command::LoadIdentity { .. })));
}

#[test]
fn a_reported_identity_should_be_kept_for_its_person() {
    let (mut s, _) = state(SignInMethod::Password);
    let identity = Some(Identity { verified: true, changed: false });
    apply(&mut s, Event::Identity { account: account(), user_id: "@grace:example.org".into(), identity });
    let account = s.active_account().unwrap();
    assert_eq!(account.identities.get("@grace:example.org"), Some(&identity));
}

// ----- offline device ----------------------------------------------------------------------

#[test]
fn turning_the_offline_device_on_should_ask_for_the_recovery_key_first() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.set_offline_device(true);
    assert!(matches!(&s.modal, Some(Modal::RecoveryKeyEntry { purpose: RecoveryKeyFor::OfflineDevice, .. })));
    assert!(commands.try_next().is_none());
}

#[test]
fn the_recovery_key_should_turn_the_offline_device_on() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.set_offline_device(true);
    *s.modal_field() = TextEdit::new("EsTc 9Ruv");
    s.submit_recovery_key();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::SetOfflineDevice { on: true, recovery_key: Some(Password(key)), .. }] if key == "EsTc 9Ruv"
    ));
}

#[test]
fn a_wrong_recovery_key_should_keep_the_offline_device_dialog_open_with_its_error() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.set_offline_device(true);
    *s.modal_field() = TextEdit::new("wrong");
    s.submit_recovery_key();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("that recovery key is not right".into()) });
    assert!(matches!(&s.modal, Some(Modal::RecoveryKeyEntry { error: Some(_), submitting: false, .. })));
}

#[test]
fn turning_the_offline_device_off_should_need_no_key() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.set_offline_device(false);
    assert!(matches!(&sent(&mut commands)[..], [Command::SetOfflineDevice { on: false, recovery_key: None, .. }]));
}

// ----- key export and import ---------------------------------------------------------------

fn export_dialog(s: &mut State, passphrase: &str, again: &str) {
    s.key_file_chosen(KeyTransfer::Export, "/tmp/keys.txt".into());
    *s.modal_field_at(0) = TextEdit::new(passphrase);
    *s.modal_field_at(1) = TextEdit::new(again);
}

#[test]
fn an_export_should_write_the_keys_with_the_passphrase_to_the_chosen_file() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    export_dialog(&mut s, "horse", "horse");
    s.submit_key_file();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::ExportRoomKeys { path, passphrase: Password(p), .. }] if p == "horse" && path.ends_with("keys.txt")
    ));
}

#[test]
fn an_export_whose_passphrases_differ_should_not_start() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    export_dialog(&mut s, "horse", "house");
    s.submit_key_file();
    assert!(commands.try_next().is_none());
    assert!(matches!(&s.modal, Some(Modal::KeyFile(file)) if file.error.is_some()));
}

#[test]
fn an_export_without_a_passphrase_should_not_start() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    export_dialog(&mut s, "", "");
    s.submit_key_file();
    assert!(commands.try_next().is_none());
}

#[test]
fn a_finished_export_should_say_where_the_keys_went() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    export_dialog(&mut s, "horse", "horse");
    s.submit_key_file();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(
        matches!(&s.modal, Some(Modal::KeyFile(file)) if matches!(&file.step, KeyFileStep::Done(m) if m.contains("/tmp/keys.txt")))
    );
}

#[test]
fn an_import_should_need_the_passphrase_once() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.files_picked(crate::state::PickFor::KeyFile, &["/tmp/keys.txt".into()]);
    *s.modal_field() = TextEdit::new("horse");
    s.submit_key_file();
    assert!(
        matches!(&sent(&mut commands)[..], [Command::ImportRoomKeys { passphrase: Password(p), .. }] if p == "horse")
    );
}

#[test]
fn an_import_should_say_how_many_keys_it_read() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.key_file_chosen(KeyTransfer::Import, "/tmp/keys.txt".into());
    *s.modal_field() = TextEdit::new("horse");
    s.submit_key_file();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::KeysImported { request, import: KeyImport { imported: 3, total: 5 } });
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(
        matches!(&s.modal, Some(Modal::KeyFile(file)) if matches!(&file.step, KeyFileStep::Done(m) if m.contains('3') && m.contains('5')))
    );
}

#[test]
fn a_failed_import_should_let_the_passphrase_be_typed_again() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.key_file_chosen(KeyTransfer::Import, "/tmp/keys.txt".into());
    *s.modal_field() = TextEdit::new("wrong");
    s.submit_key_file();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("bad passphrase".into()) });
    assert!(matches!(
        &s.modal,
        Some(Modal::KeyFile(file)) if file.step == KeyFileStep::Entering && file.error.as_deref() == Some("bad passphrase")
    ));
}

// ----- password ----------------------------------------------------------------------------

fn password_dialog(s: &mut State, current: &str, new: &str, again: &str) {
    s.open_change_password();
    *s.modal_field_at(0) = TextEdit::new(current);
    *s.modal_field_at(1) = TextEdit::new(new);
    *s.modal_field_at(2) = TextEdit::new(again);
}

#[test]
fn changing_the_password_should_send_the_new_one_with_the_current_one() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    password_dialog(&mut s, "old", "new", "new");
    s.submit_password_change();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::ChangePassword { new_password: Password(n), password: Some(Password(o)), .. }] if n == "new" && o == "old"
    ));
}

#[test]
fn new_passwords_that_differ_should_not_be_sent() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    password_dialog(&mut s, "old", "new", "knew");
    s.submit_password_change();
    assert!(commands.try_next().is_none());
    assert!(matches!(&s.modal, Some(Modal::ChangePassword(form)) if form.error.is_some()));
}

#[test]
fn an_empty_password_should_not_be_sent() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    password_dialog(&mut s, "", "new", "new");
    s.submit_password_change();
    assert!(commands.try_next().is_none());
}

#[test]
fn a_changed_password_should_say_so() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    password_dialog(&mut s, "old", "new", "new");
    s.submit_password_change();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(matches!(&s.modal, Some(Modal::ChangePassword(form)) if form.done));
}

#[test]
fn a_wrong_current_password_should_show_its_error_in_the_dialog() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    password_dialog(&mut s, "old", "new", "new");
    s.submit_password_change();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("wrong password".into()) });
    assert!(matches!(
        &s.modal,
        Some(Modal::ChangePassword(form)) if form.error.as_deref() == Some("wrong password") && !form.submitting
    ));
}

#[test]
fn a_password_change_the_server_asks_about_again_should_retry_through_the_password_dialog() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    password_dialog(&mut s, "old", "new", "new");
    s.submit_password_change();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::PasswordNeeded { request });
    *s.modal_field() = TextEdit::new("old2");
    s.confirm_with_password();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::ChangePassword { password: Some(Password(p)), .. }] if p == "old2"
    ));
}

#[test]
fn an_oauth_account_should_change_its_password_on_the_account_page() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    s.open_change_password();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::OpenAccountPage { page: matrix_core::AccountPage::Profile, .. }]
    ));
}

// ----- deactivation ------------------------------------------------------------------------

#[test]
fn deactivating_a_password_account_should_ask_for_confirmation_first() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.ask_to_deactivate();
    assert!(matches!(&s.modal, Some(Modal::Confirm(Confirm::Deactivate { erase: false, .. }))));
    assert!(commands.try_next().is_none());
}

#[test]
fn a_confirmed_deactivation_should_erase_messages_when_asked() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.ask_to_deactivate();
    s.toggle_deactivate_erase();
    s.confirm();
    assert!(matches!(&sent(&mut commands)[..], [Command::DeactivateAccount { erase: true, password: None, .. }]));
}

#[test]
fn a_deactivation_should_ask_for_the_password_when_the_server_does() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.ask_to_deactivate();
    s.confirm();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::PasswordNeeded { request });
    assert!(matches!(&s.modal, Some(Modal::Password { action: Guarded::Deactivate { erase: false }, .. })));
}

#[test]
fn deactivating_an_oauth_account_should_go_straight_to_its_account_page() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    s.ask_to_deactivate();
    assert!(matches!(&sent(&mut commands)[..], [Command::DeactivateAccount { .. }]));
    assert!(s.modal.is_none());
}

// ----- linking a device --------------------------------------------------------------------

fn linking(commands: &mut DetachedCommands, s: &mut State) -> RequestId {
    s.link_device();
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::LinkDevice { scanned: None, .. }));
    request
}

#[test]
fn linking_a_device_should_need_an_oauth_account() {
    let (mut s, mut commands) = state(SignInMethod::Password);
    s.link_device();
    assert!(commands.try_next().is_none() && s.modal.is_none());
}

#[test]
fn linking_a_device_should_need_a_verified_session() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    let security =
        Security { session_verified: false, recovery: RecoveryStatus::Enabled, offline_device: OfflineDevice::Off };
    apply(&mut s, Event::SecurityChanged { account: account(), security });
    drain(&mut commands);
    s.link_device();
    assert!(commands.try_next().is_none());
}

#[test]
fn a_device_link_should_show_each_step_the_engine_reports() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    let request = linking(&mut commands, &mut s);
    let code = QrImage::encode(b"rendezvous").unwrap();
    apply(&mut s, Event::QrLogin { request, step: QrLoginStep::Show(code) });
    assert!(matches!(&s.modal, Some(Modal::LinkDevice { flow, .. }) if matches!(flow.step, QrLoginStep::Show(_))));
}

#[test]
fn the_check_code_should_answer_the_device_link() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    let request = linking(&mut commands, &mut s);
    apply(&mut s, Event::QrLogin { request, step: QrLoginStep::EnterCheckCode });
    *s.check_code_field() = TextEdit::new("42");
    s.submit_check_code();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::Answer { request: asked, answer: Answer::CheckCode(42) }] if *asked == request
    ));
}

#[test]
fn closing_a_device_link_under_way_should_cancel_it() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    let request = linking(&mut commands, &mut s);
    s.close_modal();
    assert!(matches!(&sent(&mut commands)[..], [Command::Cancel(cancelled)] if *cancelled == request));
}

#[test]
fn closing_a_finished_device_link_should_cancel_nothing() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    let request = linking(&mut commands, &mut s);
    apply(&mut s, Event::QrLogin { request, step: QrLoginStep::Done { account: None } });
    s.close_modal();
    assert!(commands.try_next().is_none());
}

#[test]
fn a_failed_device_link_should_show_its_error() {
    let (mut s, mut commands) = state(SignInMethod::OAuth);
    let request = linking(&mut commands, &mut s);
    apply(&mut s, Event::Finished { request, result: Err("the code expired".into()) });
    assert!(
        matches!(&s.modal, Some(Modal::LinkDevice { flow, .. }) if flow.error.as_deref() == Some("the code expired"))
    );
}
