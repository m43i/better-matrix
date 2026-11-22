use matrix_core::{
    AccountId, AccountSummary, Answer, Command, DetachedCommands, EngineHandle, Event, Homeserver, LoginMethods,
    NotificationMode, Policy, QrImage, QrLoginStep, QrSignIn, RegistrationStep, RequestId,
};
use rune::{TextEdit, Tx};

use super::{Asked, SignInStep};
use crate::platform::Shell;
use crate::state::{Settings, State};

fn state() -> (State, DetachedCommands) {
    let (engine, commands) = EngineHandle::detached();
    (State::new(engine, Shell::headless(), Settings::fixed_english()), commands)
}

fn apply(s: &mut State, event: Event) {
    s.apply(event, &mut Tx::default());
}

fn drain(commands: &mut DetachedCommands) -> Vec<(RequestId, Command)> {
    std::iter::from_fn(|| commands.try_next()).collect()
}

fn sent(commands: &mut DetachedCommands) -> Vec<Command> {
    drain(commands).into_iter().map(|(_, command)| command).collect()
}

/// Step 1 against a server offering `methods`; step 2 then shows what it offers.
fn discover(s: &mut State, commands: &mut DetachedCommands, methods: LoginMethods) {
    s.submit_server();
    let (request, _) = commands.try_next().unwrap();
    let homeserver =
        Homeserver { server_name: Some("example.org".into()), url: "https://matrix.example.org/".into(), methods };
    apply(s, Event::HomeserverDiscovered { request, homeserver });
    drain(commands);
}

fn password_server() -> LoginMethods {
    LoginMethods { oauth: false, password: true, create_account: true, qr: false }
}

fn oauth_server() -> LoginMethods {
    LoginMethods { oauth: true, password: false, create_account: true, qr: true }
}

/// A registration sent from a complete form; returns its request.
fn registering(s: &mut State, commands: &mut DetachedCommands) -> RequestId {
    discover(s, commands, password_server());
    s.start_registration();
    s.sign_in.username = TextEdit::new("alice");
    s.sign_in.password = TextEdit::new("secret");
    s.sign_in.confirm = TextEdit::new("secret");
    s.submit_registration();
    commands.try_next().unwrap().0
}

// ----- creating an account -----------------------------------------------------------------

#[test]
fn creating_an_account_on_an_oauth_server_should_open_its_page_to_create_one() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, oauth_server());
    s.start_registration();
    assert!(sent(&mut commands).iter().any(|command| matches!(command, Command::LoginOAuth { create: true, .. })));
}

#[test]
fn creating_an_account_on_a_password_server_should_show_the_registration_form() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, password_server());
    s.start_registration();
    assert!(matches!(s.sign_in.step, SignInStep::Register { request: None, asked: None, .. }));
}

#[test]
fn a_complete_registration_form_should_register_at_the_discovered_server() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, password_server());
    s.start_registration();
    s.sign_in.username = TextEdit::new("@alice");
    s.sign_in.password = TextEdit::new("secret");
    s.sign_in.confirm = TextEdit::new("secret");
    s.submit_registration();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::Register(request)]
            if request.username == "alice" && request.homeserver == "https://matrix.example.org/"
    ));
}

#[test]
fn passwords_that_differ_should_not_register() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, password_server());
    s.start_registration();
    s.sign_in.username = TextEdit::new("alice");
    s.sign_in.password = TextEdit::new("secret");
    s.sign_in.confirm = TextEdit::new("secrets");
    s.submit_registration();
    assert!(commands.try_next().is_none());
    assert!(s.sign_in.error().is_some());
}

#[test]
fn a_registration_without_a_username_should_not_register() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, password_server());
    s.start_registration();
    s.sign_in.password = TextEdit::new("secret");
    s.sign_in.confirm = TextEdit::new("secret");
    s.submit_registration();
    assert!(commands.try_next().is_none());
}

#[test]
fn a_server_asking_for_a_token_should_get_the_one_typed_in() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Token });
    s.sign_in.token = TextEdit::new(" invite-42 ");
    s.submit_registration_token();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::Answer { request: asked, answer: Answer::RegistrationToken(token) }]
            if *asked == request && token == "invite-42"
    ));
}

#[test]
fn agreeing_to_the_policies_should_answer_once() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    let policies = vec![Policy { name: "Terms".into(), url: "https://example.org/terms".into() }];
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Terms(policies) });
    s.agree_to_terms();
    s.agree_to_terms();
    assert!(matches!(&sent(&mut commands)[..], [Command::Answer { answer: Answer::Agree, .. }]));
}

#[test]
fn a_stage_in_the_browser_should_continue_when_the_user_says_so() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    let step = RegistrationStep::Browser { url: "https://matrix.example.org/captcha".into() };
    apply(&mut s, Event::Registration { request, step });
    s.continue_registration();
    assert!(matches!(&sent(&mut commands)[..], [Command::Answer { answer: Answer::Continue, .. }]));
}

#[test]
fn a_server_asking_for_an_email_address_should_get_the_one_typed_in() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Email });
    s.sign_in.email = TextEdit::new(" alice@example.org ");
    s.submit_registration_email();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::Answer { request: asked, answer: Answer::Email(address) }]
            if *asked == request && address == "alice@example.org"
    ));
}

#[test]
fn an_empty_email_address_should_not_be_sent() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Email });
    s.submit_registration_email();
    assert!(commands.try_next().is_none());
    assert_eq!(s.sign_in.error(), Some("Enter a valid email address."));
}

#[test]
fn an_email_address_without_an_at_sign_should_not_be_sent() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Email });
    s.sign_in.email = TextEdit::new("alice.example.org");
    s.submit_registration_email();
    assert!(commands.try_next().is_none());
    assert_eq!(s.sign_in.error(), Some("Enter a valid email address."));
}

#[test]
fn a_corrected_email_address_should_clear_the_error() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Email });
    s.submit_registration_email();
    s.sign_in.email = TextEdit::new("alice@example.org");
    s.submit_registration_email();
    assert_eq!(s.sign_in.error(), None);
}

#[test]
fn an_emailed_link_should_continue_the_registration_once_opened() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    let step = RegistrationStep::EmailSent { address: "alice@example.org".into(), unconfirmed: false };
    apply(&mut s, Event::Registration { request, step });
    s.continue_registration();
    assert!(
        matches!(&sent(&mut commands)[..], [Command::Answer { request: asked, answer: Answer::Continue }] if *asked == request)
    );
}

#[test]
fn an_unopened_link_should_be_asked_about_again() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    let step = RegistrationStep::EmailSent { address: "alice@example.org".into(), unconfirmed: false };
    apply(&mut s, Event::Registration { request, step });
    s.continue_registration();
    let step = RegistrationStep::EmailSent { address: "alice@example.org".into(), unconfirmed: true };
    apply(&mut s, Event::Registration { request, step });
    drain(&mut commands);
    s.continue_registration();
    assert!(matches!(&sent(&mut commands)[..], [Command::Answer { answer: Answer::Continue, .. }]));
}

#[test]
fn a_question_for_another_registration_should_be_ignored() {
    let (mut s, mut commands) = state();
    registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request: RequestId(9_999), step: RegistrationStep::Token });
    assert!(matches!(s.sign_in.step, SignInStep::Register { asked: None, .. }));
}

#[test]
fn a_failed_registration_should_show_the_form_again_with_its_error() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Token });
    apply(&mut s, Event::Finished { request, result: Err("that username is taken".into()) });
    assert!(
        matches!(&s.sign_in.step, SignInStep::Register { request: None, asked: None, error: Some(e), .. } if e == "that username is taken")
    );
}

#[test]
fn a_finished_registration_should_sign_the_new_account_in() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    let summary = AccountSummary {
        id: matrix_core::AccountId::new("@alice:example.org"),
        display_name: "alice".into(),
        homeserver: "example.org".into(),
        default_notifications: NotificationMode::AllMessages,
        avatar: None,
        sign_in: matrix_core::SignInMethod::Password,
    };
    apply(&mut s, Event::AccountAdded(summary));
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert_eq!(s.nav.account, Some(matrix_core::AccountId::new("@alice:example.org")));
    assert!(matches!(s.sign_in.step, SignInStep::Server { .. }));
}

#[test]
fn going_back_from_the_registration_should_cancel_it() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    s.sign_in_instead();
    assert!(matches!(&sent(&mut commands)[..], [Command::Cancel(cancelled)] if *cancelled == request));
    assert!(matches!(s.sign_in.step, SignInStep::Password { .. }));
}

#[test]
fn a_registration_answer_should_wait_for_the_next_question() {
    let (mut s, mut commands) = state();
    let request = registering(&mut s, &mut commands);
    apply(&mut s, Event::Registration { request, step: RegistrationStep::Token });
    s.sign_in.token = TextEdit::new("t");
    s.submit_registration_token();
    assert!(matches!(&s.sign_in.step, SignInStep::Register { asked: Some(Asked { answered: true, .. }), .. }));
}

// ----- signing in with a QR code -----------------------------------------------------------

fn signing_in_with_qr(s: &mut State, commands: &mut DetachedCommands) -> RequestId {
    discover(s, commands, oauth_server());
    s.start_qr_sign_in();
    let (request, command) = drain(commands).pop().unwrap();
    assert!(
        matches!(command, Command::SignInWithQr(QrSignIn::Show { homeserver }) if homeserver == "https://matrix.example.org/")
    );
    request
}

#[test]
fn signing_in_with_a_qr_code_should_cancel_the_browser_sign_in() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, oauth_server());
    s.start_qr_sign_in();
    assert!(matches!(&sent(&mut commands)[..], [Command::Cancel(_), Command::SignInWithQr(_)]));
}

#[test]
fn a_qr_sign_in_should_show_the_code_the_engine_draws() {
    let (mut s, mut commands) = state();
    let request = signing_in_with_qr(&mut s, &mut commands);
    let code = QrImage::encode(b"rendezvous").unwrap();
    apply(&mut s, Event::QrLogin { request, step: QrLoginStep::Show(code) });
    assert!(matches!(&s.sign_in.step, SignInStep::Qr { flow, .. } if matches!(flow.step, QrLoginStep::Show(_))));
}

#[test]
fn the_check_code_should_answer_the_qr_sign_in() {
    let (mut s, mut commands) = state();
    let request = signing_in_with_qr(&mut s, &mut commands);
    apply(&mut s, Event::QrLogin { request, step: QrLoginStep::EnterCheckCode });
    *s.check_code_field() = TextEdit::new("09");
    s.submit_check_code();
    assert!(matches!(
        &sent(&mut commands)[..],
        [Command::Answer { request: asked, answer: Answer::CheckCode(9) }] if *asked == request
    ));
}

#[test]
fn a_qr_sign_in_should_be_what_the_form_waits_on() {
    let (mut s, mut commands) = state();
    let request = signing_in_with_qr(&mut s, &mut commands);
    assert_eq!(s.sign_in.pending(), Some(request));
}

#[test]
fn a_failed_qr_sign_in_should_show_its_error() {
    let (mut s, mut commands) = state();
    let request = signing_in_with_qr(&mut s, &mut commands);
    apply(&mut s, Event::Finished { request, result: Err("the code was refused".into()) });
    assert_eq!(s.sign_in.error(), Some("the code was refused"));
}

#[test]
fn changing_the_server_should_cancel_the_qr_sign_in() {
    let (mut s, mut commands) = state();
    let request = signing_in_with_qr(&mut s, &mut commands);
    s.change_server();
    assert!(matches!(&sent(&mut commands)[..], [Command::Cancel(cancelled)] if *cancelled == request));
}

#[test]
fn a_server_without_qr_sign_in_should_not_start_one() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, password_server());
    s.start_qr_sign_in();
    assert!(commands.try_next().is_none());
}

// ----- signing in again as a suspended account ---------------------------------------------

/// Step 2 of signing in again as a suspended account, on a server offering every way in.
fn resuming(s: &mut State, commands: &mut DetachedCommands) {
    let id = AccountId::new("@alice:example.org");
    apply(s, Event::SessionEnded { account: id, kept: true });
    s.close_modal();
    s.sign_in.homeserver = TextEdit::new("@alice:example.org");
    discover(s, commands, LoginMethods { oauth: true, password: true, create_account: true, qr: true });
}

#[test]
fn signing_in_again_as_a_suspended_account_should_not_start_a_qr_sign_in() {
    let (mut s, mut commands) = state();
    resuming(&mut s, &mut commands);
    s.start_qr_sign_in();
    assert!(commands.try_next().is_none());
    assert!(matches!(s.sign_in.step, SignInStep::Browser { .. }));
}

#[test]
fn signing_in_again_as_a_suspended_account_should_not_create_an_account() {
    let (mut s, mut commands) = state();
    resuming(&mut s, &mut commands);
    s.start_registration();
    assert!(commands.try_next().is_none());
    assert!(matches!(s.sign_in.step, SignInStep::Browser { create: false, .. }));
}
