use std::sync::{Arc, Mutex};

use matrix_core::{
    AccountSummary, DetachedCommands, EntryKind, Homeserver, LoginMethods, Message, Notification, Outgoing, RoomKind,
    RoomList, RoomSummary, SpaceSummary, TimelineDiff, TimelineEntry, TimelineKey,
};
use rune::TextEdit;

use super::*;

fn state() -> (State, DetachedCommands) {
    let (engine, commands) = EngineHandle::detached();
    (State::new(engine, Shell::headless(), Settings::fixed_english()), commands)
}

fn apply(s: &mut State, event: Event) {
    s.apply(event, &mut Tx::default());
}

fn account(user_id: &str) -> AccountSummary {
    AccountSummary {
        id: AccountId::new(user_id),
        display_name: user_id.into(),
        homeserver: "example.org".into(),
        default_notifications: NotificationMode::AllMessages,
        avatar: None,
        sign_in: matrix_core::SignInMethod::Password,
    }
}

fn room(room_id: &str) -> RoomSummary {
    RoomSummary {
        room_id: room_id.into(),
        name: room_id.into(),
        topic: None,
        kind: RoomKind::Group,
        unread: 2,
        mentions: 0,
        dm_users: Vec::new(),
        avatar: None,
        member_count: 0,
        active_member_count: 0,
        history: matrix_core::HistoryVisibility::Shared,
        notifications: NotificationMode::AllMessages,
        custom_notifications: false,
        call: matrix_core::RoomCall::default(),
        activity_ms: None,
        encrypted: false,
        can_rename: true,
        pinned: Vec::new(),
        can_pin: true,
        saved: Vec::new(),
    }
}

fn with_rooms(s: &mut State, id: &AccountId, rooms: &[&str], spaces: Vec<SpaceSummary>) {
    apply(s, Event::AccountAdded(account(id.as_str())));
    let list = RoomList { rooms: rooms.iter().map(|r| room(r)).collect(), spaces, invitations: Vec::new() };
    apply(s, Event::RoomsUpdated { account: id.clone(), list });
}

fn drain(commands: &mut DetachedCommands) -> Vec<(RequestId, Command)> {
    std::iter::from_fn(|| commands.try_next()).collect()
}

/// Runs step 1 against a server offering `methods`, returning the commands sent after discovery.
fn discover(s: &mut State, commands: &mut DetachedCommands, oauth: bool, password: bool) -> Vec<Command> {
    s.submit_server();
    let (request, _) = commands.try_next().unwrap();
    let homeserver = Homeserver {
        server_name: Some("example.org".into()),
        url: "https://matrix.example.org/".into(),
        methods: LoginMethods { oauth, password },
    };
    apply(s, Event::HomeserverDiscovered { request, homeserver });
    drain(commands).into_iter().map(|(_, c)| c).collect()
}

fn fill_password(s: &mut State) {
    s.sign_in.username = TextEdit::new("alice");
    s.sign_in.password = TextEdit::new("secret").secure();
}

fn notification(room_id: &str) -> Notification {
    Notification {
        account: AccountId::new("@a:example.org"),
        room_id: room_id.into(),
        room_name: "general".into(),
        event_id: "$1".into(),
        sender_name: "Grace".into(),
        body: "hi".into(),
    }
}

fn recording_state() -> (State, Arc<Mutex<Vec<Toast>>>) {
    let (shell, posted) = Shell::recording();
    let mut s = State::new(EngineHandle::detached().0, shell, Settings::fixed_english());
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    (s, posted)
}

#[test]
fn a_new_state_should_ask_to_sign_in() {
    let (s, _) = state();
    assert_eq!(s.screen(), Screen::SignIn);
}

#[test]
fn the_first_account_should_become_active() {
    let (mut s, _) = state();
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    assert_eq!(s.nav.account, Some(AccountId::new("@a:example.org")));
}

#[test]
fn a_restored_account_should_not_steal_focus_from_the_active_one() {
    let (mut s, _) = state();
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    apply(&mut s, Event::AccountAdded(account("@b:example.org")));
    assert_eq!(s.nav.account, Some(AccountId::new("@a:example.org")));
}

#[test]
fn restarting_should_open_the_last_used_account_even_when_it_restores_last() {
    let (mut s, _) = state();
    let first = AccountId::new("@a:example.org");
    let last = AccountId::new("@b:example.org");
    apply(&mut s, Event::AccountAdded(account(first.as_str())));
    apply(&mut s, Event::AccountAdded(account(last.as_str())));
    s.select_account(&last);

    let mut restarted = State::new(EngineHandle::detached().0, Shell::headless(), s.settings.clone());
    apply(&mut restarted, Event::Restoring(vec![first.clone(), last.clone()]));
    apply(&mut restarted, Event::AccountAdded(account(first.as_str())));
    apply(&mut restarted, Event::AccountAdded(account(last.as_str())));

    assert_eq!(restarted.nav.account, Some(last));
}

#[test]
fn other_accounts_restoring_first_should_wait_for_the_last_used_account() {
    let first = AccountId::new("@a:example.org");
    let last = AccountId::new("@b:example.org");
    let settings = Settings { last_account: Some(last.clone()), ..Settings::fixed_english() };
    let mut s = State::new(EngineHandle::detached().0, Shell::headless(), settings);
    apply(&mut s, Event::Restoring(vec![first.clone(), last.clone()]));
    apply(&mut s, Event::AccountAdded(account(first.as_str())));

    assert_eq!((s.screen(), s.settings.last_account), (Screen::Restoring, Some(last)));
}

#[test]
fn the_last_used_account_restoring_first_should_keep_focus() {
    let first = AccountId::new("@a:example.org");
    let last = AccountId::new("@b:example.org");
    let settings = Settings { last_account: Some(last.clone()), ..Settings::fixed_english() };
    let mut s = State::new(EngineHandle::detached().0, Shell::headless(), settings);
    apply(&mut s, Event::Restoring(vec![first.clone(), last.clone()]));
    apply(&mut s, Event::AccountAdded(account(last.as_str())));
    apply(&mut s, Event::AccountAdded(account(first.as_str())));

    assert_eq!(s.nav.account, Some(last));
}

#[test]
fn a_missing_last_used_account_should_fall_back_to_another() {
    let settings = Settings { last_account: Some(AccountId::new("@gone:example.org")), ..Settings::fixed_english() };
    let mut s = State::new(EngineHandle::detached().0, Shell::headless(), settings);
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::Restoring(vec![id.clone()]));
    apply(&mut s, Event::AccountAdded(account(id.as_str())));

    assert_eq!((s.screen(), s.settings.last_account), (Screen::Chat, Some(id)));
}

#[test]
fn a_last_used_account_that_fails_to_restore_should_fall_back_to_another() {
    let last = AccountId::new("@b:example.org");
    let settings = Settings { last_account: Some(last.clone()), ..Settings::fixed_english() };
    let mut s = State::new(EngineHandle::detached().0, Shell::headless(), settings);
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::Restoring(vec![id.clone(), last.clone()]));
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    apply(&mut s, Event::AccountError { account: last, error: "unavailable".into() });

    assert_eq!((s.screen(), s.settings.last_account), (Screen::Chat, Some(id)));
}

#[test]
fn selecting_an_account_during_restore_should_override_the_saved_choice() {
    let last = AccountId::new("@b:example.org");
    let settings = Settings { last_account: Some(last.clone()), ..Settings::fixed_english() };
    let mut s = State::new(EngineHandle::detached().0, Shell::headless(), settings);
    let selected = AccountId::new("@a:example.org");
    apply(&mut s, Event::Restoring(vec![selected.clone(), last.clone()]));
    apply(&mut s, Event::AccountAdded(account(selected.as_str())));
    s.select_account(&selected);
    apply(&mut s, Event::AccountAdded(account(last.as_str())));

    assert_eq!(s.nav.account, Some(selected));
}

#[test]
fn removing_the_active_account_should_remember_its_replacement() {
    let (mut s, _) = state();
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    apply(&mut s, Event::AccountAdded(account("@b:example.org")));
    apply(&mut s, Event::AccountRemoved(AccountId::new("@a:example.org")));

    assert_eq!(s.settings.last_account, Some(AccountId::new("@b:example.org")));
}

#[test]
fn closing_the_sign_in_modal_should_cancel_its_pending_request() {
    let (mut s, mut commands) = state();
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    s.start_adding_account();
    s.submit_server();
    let (request, _) = commands.try_next().unwrap();
    s.close_modal();

    assert!(matches!(commands.try_next(), Some((_, Command::Cancel(cancelled))) if cancelled == request));
}

#[test]
fn an_incoming_dialog_during_sign_in_should_still_select_the_added_account() {
    let (mut s, mut commands) = state();
    let current = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(current.as_str())));
    s.start_adding_account();
    discover(&mut s, &mut commands, true, false);
    apply(&mut s, Event::SessionEnded { account: current, kept: true });
    apply(&mut s, Event::AccountAdded(account("@b:example.org")));

    assert_eq!(s.nav.account, Some(AccountId::new("@b:example.org")));
}

#[test]
fn signing_in_from_the_modal_should_close_it_and_remember_the_new_account() {
    let (mut s, mut commands) = state();
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    s.start_adding_account();
    discover(&mut s, &mut commands, false, true);
    fill_password(&mut s);
    s.submit_password();
    let (request, _) = commands.try_next().unwrap();
    let added = AccountId::new("@b:example.org");
    apply(&mut s, Event::AccountAdded(account(added.as_str())));
    apply(&mut s, Event::Finished { request, result: Ok(()) });

    assert!(s.modal.is_none() && s.settings.last_account == Some(added));
}

#[test]
fn a_sign_in_finishing_after_cancel_should_not_change_the_account_on_screen() {
    let (mut s, mut commands) = state();
    let current = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(current.as_str())));
    s.start_adding_account();
    discover(&mut s, &mut commands, true, false);
    s.close_modal();
    apply(&mut s, Event::AccountAdded(account("@b:example.org")));

    assert_eq!(s.nav.account, Some(current));
}

#[test]
fn the_last_account_leaving_should_continue_sign_in_full_screen() {
    let (mut s, _) = state();
    let current = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(current.as_str())));
    apply(&mut s, Event::SessionEnded { account: current.clone(), kept: false });
    s.sign_in_again();
    let request = s.sign_in.pending().expect("discovery under way");
    apply(&mut s, Event::AccountRemoved(current));

    assert!(s.modal.is_none() && s.screen() == Screen::SignIn && s.sign_in.pending() == Some(request));
}

#[test]
fn a_restoring_account_should_not_replace_the_account_behind_sign_in() {
    let current = AccountId::new("@a:example.org");
    let other = AccountId::new("@b:example.org");
    let settings = Settings { last_account: Some(current.clone()), ..Settings::fixed_english() };
    let mut s = State::new(EngineHandle::detached().0, Shell::headless(), settings);
    apply(&mut s, Event::Restoring(vec![current.clone(), other.clone()]));
    apply(&mut s, Event::AccountAdded(account(current.as_str())));
    s.start_adding_account();
    apply(&mut s, Event::AccountAdded(account(other.as_str())));

    assert_eq!((s.nav.account, s.settings.last_account), (Some(current.clone()), Some(current)));
}

#[test]
fn no_room_should_open_by_itself() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x", "!two:x"], Vec::new());
    assert_eq!(s.active_room_id(), None);
}

#[test]
fn selecting_a_space_should_show_its_overview() {
    let (mut s, _) = state();
    let space = SpaceSummary {
        room_id: "!space:x".into(),
        name: "Space".into(),
        avatar: None,
        can_add_rooms: true,
        can_rename: true,
        children: vec!["!two:x".into()],
        members: Vec::new(),
    };
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x", "!two:x"], vec![space]);
    s.select_room("!one:x");
    s.select_scope(Scope::Space("!space:x".into()));
    assert_eq!(s.active_room_id(), None);
}

#[test]
fn submitting_a_server_should_discover_it() {
    let (mut s, mut commands) = state();
    s.submit_server();
    assert!(matches!(commands.try_next(), Some((_, Command::Discover { server })) if server == "matrix.org"));
}

#[test]
fn a_matrix_id_should_discover_its_server_and_fill_in_the_username() {
    let (mut s, mut commands) = state();
    s.sign_in.homeserver = TextEdit::new("@alice:example.org");
    s.submit_server();
    let discovered = matches!(commands.try_next(), Some((_, Command::Discover { server })) if server == "example.org");
    assert!(discovered && s.sign_in.username.text == "alice");
}

#[test]
fn an_oauth_server_should_go_straight_to_the_browser() {
    let (mut s, mut commands) = state();
    let sent = discover(&mut s, &mut commands, true, true);
    assert!(
        matches!(&sent[..], [Command::LoginOAuth { homeserver, resume: None }] if homeserver == "https://matrix.example.org/")
    );
}

#[test]
fn a_password_server_should_ask_for_username_and_password() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, false, true);
    assert!(matches!(s.sign_in.step, SignInStep::Password { submitting: None, .. }));
}

#[test]
fn a_server_without_known_methods_should_stay_on_step_one_with_an_error() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, false, false);
    assert!(matches!(s.sign_in.step, SignInStep::Server { error: Some(_) }));
}

#[test]
fn a_password_sign_in_should_log_in_at_the_discovered_url() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, false, true);
    fill_password(&mut s);
    s.submit_password();
    assert!(matches!(
        commands.try_next(),
        Some((_, Command::Login(login))) if login.username == "alice" && login.homeserver == "https://matrix.example.org/"
    ));
}

#[test]
fn a_failed_password_sign_in_should_show_its_error_on_step_two() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, false, true);
    fill_password(&mut s);
    s.submit_password();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("wrong password".into()) });
    assert!(matches!(s.sign_in.step, SignInStep::Password { .. }) && s.sign_in.error() == Some("wrong password"));
}

#[test]
fn a_browser_sign_in_should_remember_the_url_to_reopen() {
    let (mut s, mut commands) = state();
    s.submit_server();
    let (discovery, _) = commands.try_next().unwrap();
    let homeserver = Homeserver {
        server_name: None,
        url: "https://hs/".into(),
        methods: LoginMethods { oauth: true, password: false },
    };
    apply(&mut s, Event::HomeserverDiscovered { request: discovery, homeserver });
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::OpenUrl { request, url: "https://auth.example.org/authorize".into() });
    assert!(matches!(&s.sign_in.step, SignInStep::Browser { url: Some(url), .. } if url.contains("authorize")));
}

#[test]
fn switching_to_the_password_should_cancel_the_browser_sign_in() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, true, true);
    s.use_password();
    let cancelled = drain(&mut commands).into_iter().any(|(_, c)| matches!(c, Command::Cancel(_)));
    assert!(cancelled && matches!(s.sign_in.step, SignInStep::Password { .. }));
}

#[test]
fn changing_the_server_should_return_to_step_one() {
    let (mut s, mut commands) = state();
    discover(&mut s, &mut commands, true, false);
    s.change_server();
    assert!(matches!(s.sign_in.step, SignInStep::Server { .. }));
}

#[test]
fn removing_the_active_account_should_switch_to_another() {
    let (mut s, _) = state();
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    apply(&mut s, Event::AccountAdded(account("@b:example.org")));
    apply(&mut s, Event::AccountRemoved(AccountId::new("@a:example.org")));
    assert_eq!(s.nav.account, Some(AccountId::new("@b:example.org")));
}

#[test]
fn signing_out_should_send_a_logout_for_that_account() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    s.sign_out(&id);
    assert!(matches!(commands.try_next(), Some((_, Command::Logout(sent))) if sent == id));
}

#[test]
fn opening_a_room_should_clear_its_unread_count() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!two:x");
    assert_eq!(s.accounts.get(&id).unwrap().room("!two:x").unwrap().unread, 0);
}

#[test]
fn sending_should_send_text_and_each_attachment_to_the_open_room() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.composer.edit.insert("hi");
    s.files_picked(
        PickFor::Attachments(ComposerTarget::Room),
        &[PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
    );
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    let sent: Vec<_> = drain(&mut commands).into_iter().map(|(_, c)| c).collect();
    assert!(matches!(
        &sent[..],
        [
            Command::Send { message: Outgoing::Text { .. }, .. },
            Command::Send { message: Outgoing::File(_), .. },
            Command::Send { message: Outgoing::File(_), .. },
        ]
    ));
}

#[test]
fn muting_a_room_should_update_it_at_once_and_tell_the_server() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.set_notification_mode("!one:x", Some(NotificationMode::Mute));
    let told = drain(&mut commands)
        .into_iter()
        .any(|(_, c)| matches!(c, Command::SetNotificationMode { mode: Some(NotificationMode::Mute), .. }));
    assert!(told && s.accounts.get(&id).unwrap().room("!one:x").unwrap().notifications == NotificationMode::Mute);
}

#[test]
fn a_notification_for_another_room_should_be_shown() {
    let (mut s, posted) = recording_state();
    apply(&mut s, Event::Notification(notification("!two:x")));
    assert_eq!(posted.lock().unwrap().len(), 1);
}

#[test]
fn a_notification_for_the_room_being_read_should_be_skipped() {
    let (mut s, posted) = recording_state();
    apply(&mut s, Event::Notification(notification("!one:x")));
    assert!(posted.lock().unwrap().is_empty());
}

#[test]
fn a_notification_for_the_open_room_should_be_shown_when_the_window_is_in_the_background() {
    let (mut s, posted) = recording_state();
    s.window_focused = false;
    apply(&mut s, Event::Notification(notification("!one:x")));
    assert_eq!(posted.lock().unwrap().len(), 1);
}

#[test]
fn resetting_a_room_should_make_it_follow_the_account_default() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.set_notification_mode("!one:x", Some(NotificationMode::Mute));
    s.set_notification_mode("!one:x", None);
    let room = s.accounts.get(&id).unwrap().room("!one:x").unwrap();
    assert!(!room.custom_notifications && room.notifications == NotificationMode::AllMessages);
}

#[test]
fn changing_the_default_should_tell_the_server() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.set_default_notifications(&id, NotificationMode::MentionsOnly);
    assert!(
        drain(&mut commands).into_iter().any(|(_, c)| matches!(
            c,
            Command::SetDefaultNotificationMode { mode: NotificationMode::MentionsOnly, .. }
        ))
    );
}

#[test]
fn choosing_a_status_should_show_it_at_once_and_tell_the_server() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.toggle_popover(Popover::AccountSwitcher);
    s.set_status(&id, matrix_core::Status::Hidden);
    assert_eq!(s.accounts.get(&id).map(|a| a.status), Some(matrix_core::Status::Hidden));
    assert!(s.popover.is_none());
    assert!(matches!(&drain(&mut commands)[..], [(_, Command::SetStatus { status: matrix_core::Status::Hidden, .. })]));
}

#[test]
fn choosing_the_current_status_should_send_nothing() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status(&id, matrix_core::Status::Online);
    assert!(drain(&mut commands).is_empty());
}

#[test]
fn a_status_the_server_refused_should_show_the_previous_one_and_why() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status(&id, matrix_core::Status::Away);
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    assert_eq!(s.accounts.get(&id).map(|a| a.status), Some(matrix_core::Status::Online));
    assert!(s.notice.as_deref().is_some_and(|notice| notice.contains("offline")));
}

#[test]
fn a_late_failure_should_not_undo_a_status_chosen_since() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status(&id, matrix_core::Status::Away);
    let (away, _) = commands.try_next().unwrap();
    s.set_status(&id, matrix_core::Status::Hidden);
    apply(&mut s, Event::Finished { request: away, result: Err("offline".into()) });
    assert_eq!(s.accounts.get(&id).map(|a| a.status), Some(matrix_core::Status::Hidden));
}

#[test]
fn choices_that_all_fail_should_fall_back_to_the_saved_status() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status(&id, matrix_core::Status::Away);
    s.set_status(&id, matrix_core::Status::Hidden);
    for (request, _) in drain(&mut commands) {
        apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    }
    assert_eq!(s.accounts.get(&id).map(|a| a.status), Some(matrix_core::Status::Online));
}

#[test]
fn a_status_set_on_another_session_should_show_here() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    apply(&mut s, Event::Status { account: id.clone(), status: matrix_core::Status::Away });
    assert_eq!(s.accounts.get(&id).map(|a| a.status), Some(matrix_core::Status::Away));
}

/// What `user_id` shows on account `id`, as the state holds it.
fn shown(s: &State, id: &AccountId, user_id: &str) -> Option<matrix_core::PersonPresence> {
    s.accounts.get(id).and_then(|account| account.presence.get(user_id)).cloned()
}

fn person_presence(
    user_id: &str,
    presence: matrix_core::Presence,
    message: Option<&str>,
) -> matrix_core::PersonPresence {
    matrix_core::PersonPresence { user_id: user_id.into(), presence, status_message: message.map(Into::into) }
}

#[test]
fn writing_a_status_message_should_start_from_the_one_set() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let me = person_presence(id.as_str(), matrix_core::Presence::Online, Some("🌴 Away"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![me] });
    s.toggle_popover(Popover::AccountSwitcher);
    s.edit_status_message();
    assert_eq!((s.status_draft.as_ref(), s.status_edit.text.as_str()), (Some(&id), "🌴 Away"));
}

#[test]
fn saving_a_status_message_should_show_it_at_once_and_tell_the_server() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.toggle_popover(Popover::AccountSwitcher);
    s.edit_status_message();
    s.status_edit = rune::TextEdit::new("  🌴 On vacation ");
    s.save_status_message();
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), Some("🌴 On vacation"));
    assert!(s.popover.is_none() && s.status_draft.is_none());
    let sent = drain(&mut commands);
    assert!(matches!(&sent[..], [(_, Command::SetStatusMessage { message: Some(m), .. })] if m == "🌴 On vacation"));
}

#[test]
fn clearing_the_status_message_should_send_none() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let me = person_presence(id.as_str(), matrix_core::Presence::Online, Some("Out"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![me] });
    s.set_status_message(&id, "");
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), None);
    assert!(matches!(&drain(&mut commands)[..], [(_, Command::SetStatusMessage { message: None, .. })]));
}

#[test]
fn a_long_status_message_should_be_cut_at_the_longest_length() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status_message(&id, &"a".repeat(150));
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()).map(|m| m.chars().count()), Some(100));
}

#[test]
fn saving_the_status_message_already_set_should_send_nothing() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status_message(&id, "   ");
    assert!(drain(&mut commands).is_empty());
}

#[test]
fn a_status_message_the_server_refused_should_show_the_previous_one_and_why() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let me = person_presence(id.as_str(), matrix_core::Presence::Online, Some("Before"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![me] });
    s.set_status_message(&id, "After");
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), Some("Before"));
    assert!(s.notice.as_deref().is_some_and(|notice| notice.contains("offline")));
}

#[test]
fn status_messages_that_all_fail_should_fall_back_to_the_saved_one() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let me = person_presence(id.as_str(), matrix_core::Presence::Online, Some("Saved"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![me] });
    s.set_status_message(&id, "First");
    s.set_status_message(&id, "Second");
    for (request, _) in drain(&mut commands) {
        apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    }
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), Some("Saved"));
}

#[test]
fn an_earlier_save_reported_while_a_later_one_saves_should_not_replace_it() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status_message(&id, "A");
    s.set_status_message(&id, "B");
    let me = person_presence(id.as_str(), matrix_core::Presence::Online, Some("A"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![me] });
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), Some("B"));
    s.set_status_message(&id, "A");
    assert_eq!(drain(&mut commands).len(), 3, "choosing A again is saved again");
}

#[test]
fn an_earlier_save_failing_should_not_undo_a_later_choice() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status_message(&id, "A");
    s.set_status_message(&id, "B");
    s.set_status_message(&id, "A");
    let (first, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request: first, result: Err("offline".into()) });
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), Some("A"));
    assert_eq!(s.accounts.get(&id).and_then(|a| a.saving_status_message), drain(&mut commands).last().map(|(r, _)| *r));
}

#[test]
fn the_latest_save_finishing_should_show_the_message_as_reported() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status_message(&id, "Out");
    let (request, _) = commands.try_next().unwrap();
    let me = person_presence(id.as_str(), matrix_core::Presence::Online, Some("Out"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![me] });
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    let data = s.accounts.get(&id);
    assert_eq!(data.and_then(|a| a.status_message()), Some("Out"));
    assert!(data.is_some_and(|a| a.saving_status_message.is_none()));
}

#[test]
fn a_superseded_save_failing_should_show_no_notice() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.set_status_message(&id, "A");
    s.set_status_message(&id, "B");
    let (first, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request: first, result: Err("offline".into()) });
    assert!(s.notice.is_none());
}

#[test]
fn a_member_list_should_not_supply_the_users_own_status_message() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let mut me = read_member(id.as_str(), "A");
    me.presence = matrix_core::Presence::Online;
    me.status_message = Some("Stale".into());
    let members = matrix_core::Members { people: vec![me], complete: true, ..Default::default() };
    apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: "!r:x".into(), members });
    assert_eq!(s.accounts.get(&id).and_then(|a| a.status_message()), None);
}

#[test]
fn escape_while_writing_a_status_message_should_keep_the_account_menu_open() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.toggle_popover(Popover::AccountSwitcher);
    s.edit_status_message();
    s.cancel_status_message();
    assert!(s.status_draft.is_none() && s.popover == Some(Popover::AccountSwitcher));
}

#[test]
fn reopening_the_account_menu_should_drop_a_half_written_status_message() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.toggle_popover(Popover::AccountSwitcher);
    s.edit_status_message();
    s.toggle_popover(Popover::AccountSwitcher);
    s.toggle_popover(Popover::AccountSwitcher);
    assert!(s.status_draft.is_none());
}

#[test]
fn an_emoji_picked_for_the_status_message_should_start_it_and_return_to_writing() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    s.toggle_popover(Popover::AccountSwitcher);
    s.edit_status_message();
    s.status_edit = rune::TextEdit::new("Back at 3");
    s.toggle_popover(Popover::Emoji(EmojiFor::StatusMessage));
    s.choose_emoji(EmojiFor::StatusMessage, "🕒");
    assert_eq!(s.popover, Some(Popover::AccountSwitcher));
    assert_eq!((s.status_draft.as_ref(), s.status_edit.text.as_str()), (Some(&id), "🕒 Back at 3"));
}

#[test]
fn reported_presence_should_replace_what_was_known() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let away = person_presence("@kai:x", matrix_core::Presence::Away, Some("🌴 Away"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![away] });
    let back = person_presence("@kai:x", matrix_core::Presence::Online, None);
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![back.clone()] });
    assert_eq!(shown(&s, &id, "@kai:x"), Some(back));
}

#[test]
fn a_member_list_should_not_overwrite_presence_the_engine_reported() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let reported = person_presence("@kai:x", matrix_core::Presence::Away, Some("🌴 Away"));
    apply(&mut s, Event::Presence { account: id.clone(), people: vec![reported.clone()] });
    let mut kai = read_member("@kai:x", "Kai");
    kai.presence = matrix_core::Presence::Online;
    let mut ana = read_member("@ana:x", "Ana");
    ana.presence = matrix_core::Presence::Offline;
    let members = matrix_core::Members { people: vec![kai, ana], complete: true, ..Default::default() };
    apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: "!r:x".into(), members });
    assert_eq!(shown(&s, &id, "@kai:x"), Some(reported));
    assert_eq!(shown(&s, &id, "@ana:x").map(|p| p.presence), Some(matrix_core::Presence::Offline));
}

#[test]
fn opening_settings_should_show_the_general_section_and_close_popovers() {
    let (mut s, _) = state();
    s.toggle_popover(Popover::AccountSwitcher);
    s.open_settings();
    assert!(s.popover.is_none() && s.modal == Some(Modal::Settings(SettingsSection::General)));
}

#[test]
fn leaving_a_room_should_wait_for_confirmation() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.ask_to_leave("!one:x");
    let asked = matches!(&s.modal, Some(Modal::Confirm(Confirm::LeaveRoom { room_id, .. })) if room_id == "!one:x");
    assert!(asked && drain(&mut commands).is_empty());
}

#[test]
fn confirming_should_leave_the_room_without_forgetting_it() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.ask_to_leave("!one:x");
    s.confirm();
    assert!(matches!(commands.try_next(), Some((_, Command::LeaveRoom { forget: false, .. }))));
}

#[test]
fn deleting_a_chat_should_leave_and_forget_it() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let chat = RoomSummary { kind: RoomKind::Direct, ..room("!dm:x") };
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id,
            list: RoomList { rooms: vec![chat], spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    s.ask_to_leave("!dm:x");
    s.confirm();
    assert!(matches!(commands.try_next(), Some((_, Command::LeaveRoom { forget: true, .. }))));
}

#[test]
fn closing_the_confirmation_should_send_nothing() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.ask_to_leave("!one:x");
    s.close_modal();
    s.confirm();
    assert!(drain(&mut commands).is_empty());
}

#[test]
fn a_left_open_room_should_give_way_to_the_overview() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id,
            list: RoomList { rooms: vec![room("!two:x")], spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    assert_eq!(s.active_room_id(), None);
}

fn open_commands(commands: &mut DetachedCommands) -> Vec<Command> {
    drain(commands)
        .into_iter()
        .map(|(_, c)| c)
        .filter(|c| {
            matches!(c, Command::OpenTimeline { .. } | Command::CloseTimeline { .. } | Command::LoadOlder { .. })
        })
        .collect()
}

#[test]
fn opening_a_room_should_follow_its_timeline() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    assert!(
        matches!(&open_commands(&mut commands)[..], [Command::OpenTimeline { key, .. }] if *key == TimelineKey::room("!one:x"))
    );
}

#[test]
fn switching_rooms_should_close_the_previous_timeline() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.select_room("!two:x");
    assert!(matches!(
        &open_commands(&mut commands)[..],
        [Command::CloseTimeline { key: closed, .. }, Command::OpenTimeline { key: opened, .. }]
            if closed.room_id == "!one:x" && opened.room_id == "!two:x"
    ));
}

#[test]
fn the_overview_should_close_the_timeline() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.show_overview();
    assert!(matches!(&open_commands(&mut commands)[..], [Command::CloseTimeline { .. }]));
}

/// A state with `!one:x` open and its first page loaded.
fn scrolled_room() -> (State, DetachedCommands) {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    apply(&mut s, Event::HistoryLoaded { account: id, key: TimelineKey::room("!one:x"), reached_start: false });
    drain(&mut commands);
    (s, commands)
}

#[test]
fn scrolling_near_the_top_should_load_older_messages() {
    let (mut s, mut commands) = scrolled_room();
    s.near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    assert!(matches!(&open_commands(&mut commands)[..], [Command::LoadOlder { .. }]));
}

#[test]
fn a_page_in_flight_should_not_be_requested_twice() {
    let (mut s, mut commands) = scrolled_room();
    s.near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    s.near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    assert_eq!(open_commands(&mut commands).len(), 1);
}

#[test]
fn nothing_more_should_load_once_the_start_is_reached() {
    let (mut s, mut commands) = scrolled_room();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::HistoryLoaded { account: id, key: TimelineKey::room("!one:x"), reached_start: true });
    s.near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    assert!(open_commands(&mut commands).is_empty());
}

#[test]
fn a_failed_page_should_not_load_again_until_retried() {
    let (mut s, mut commands) = scrolled_room();
    s.near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    s.near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    assert!(open_commands(&mut commands).is_empty());
    s.retry_history(&TimelineKey::room("!one:x"));
    assert!(matches!(&open_commands(&mut commands)[..], [Command::LoadOlder { .. }]));
}

#[test]
fn a_failed_first_page_should_show_its_error() {
    let (mut s, _) = scrolled_room();
    let (id, key) = (AccountId::new("@a:example.org"), TimelineKey::room("!one:x"));
    apply(&mut s, Event::HistoryFailed { account: id.clone(), key: key.clone(), error: "offline".into() });
    let timeline = s.accounts.timeline_mut(&id, &key).unwrap();
    assert_eq!(timeline.history_error.as_deref(), Some("offline"));
    assert!(!timeline.loading);
}

#[test]
fn a_timeline_that_failed_to_open_should_open_again_on_retry() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.select_room("!one:x");
    let request = drain(&mut commands)
        .into_iter()
        .find_map(|(request, command)| matches!(command, Command::OpenTimeline { .. }).then_some(request))
        .unwrap();
    apply(&mut s, Event::Finished { request, result: Err("timeline closed".into()) });
    s.retry_history(&TimelineKey::room("!one:x"));
    assert!(matches!(&open_commands(&mut commands)[..], [Command::OpenTimeline { .. }]));
}

#[test]
fn an_opened_timeline_should_keep_loading_until_its_first_page_arrives() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let key = TimelineKey::room("!one:x");
    apply(&mut s, Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs: Vec::new() });
    drain(&mut commands);
    s.show_overview();
    s.select_room("!one:x");
    let request = drain(&mut commands)
        .into_iter()
        .find_map(|(request, command)| matches!(command, Command::OpenTimeline { .. }).then_some(request))
        .unwrap();
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(s.accounts.timeline_mut(&id, &key).unwrap().loading);
    apply(&mut s, Event::HistoryLoaded { account: id.clone(), key: key.clone(), reached_start: false });
    assert!(!s.accounts.timeline_mut(&id, &key).unwrap().loading);
}

#[test]
fn opening_a_thread_should_show_its_root_at_once() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let root = matrix_core::TimelineEntry {
        id: "root".into(),
        kind: matrix_core::EntryKind::Message(Box::new(Message { event_id: "$root".into(), ..Message::default() })),
    };
    let diffs = vec![matrix_core::TimelineDiff::Reset(vec![root])];
    apply(&mut s, Event::TimelineChanged { account: id.clone(), key: TimelineKey::room("!one:x"), diffs });
    s.open_thread("$root");
    let thread = s.accounts.timeline_mut(&id, &TimelineKey::thread("!one:x", "$root")).unwrap();
    assert_eq!(thread.messages().next().map(|m| m.event_id.as_str()), Some("$root"));
}

#[test]
fn a_thread_should_stop_loading_once_its_root_is_in() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.open_thread("$root");
    let key = TimelineKey::thread("!one:x", "$root");
    let root = matrix_core::TimelineEntry {
        id: "root".into(),
        kind: matrix_core::EntryKind::Message(Box::new(Message { event_id: "$root".into(), ..Message::default() })),
    };
    apply(
        &mut s,
        Event::TimelineChanged { account: id, key, diffs: vec![matrix_core::TimelineDiff::Reset(vec![root])] },
    );
    drain(&mut commands);
    s.thread_near_top.store(true, Ordering::Relaxed);
    s.poll(&mut Tx::default());
    assert!(open_commands(&mut commands).is_empty());
}

#[test]
fn verifying_the_session_should_fetch_the_keys_of_open_rooms() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    let unverified = matrix_core::Security { session_verified: false, recovery: matrix_core::RecoveryStatus::Enabled };
    apply(&mut s, Event::SecurityChanged { account: id.clone(), security: unverified });
    drain(&mut commands);
    let verified = matrix_core::Security { session_verified: true, ..unverified };
    apply(&mut s, Event::SecurityChanged { account: id, security: verified });
    assert!(matches!(commands.try_next(), Some((_, Command::FetchRoomKeys(_)))));
}

#[test]
fn an_unverified_session_should_not_fetch_room_keys() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    drain(&mut commands);
    let unverified = matrix_core::Security { session_verified: false, recovery: matrix_core::RecoveryStatus::Enabled };
    apply(&mut s, Event::SecurityChanged { account: id, security: unverified });
    assert!(commands.try_next().is_none());
}

#[test]
fn opening_a_thread_should_follow_it_beside_the_room() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.open_thread("$root");
    assert!(
        matches!(&open_commands(&mut commands)[..], [Command::OpenTimeline { key, .. }] if *key == TimelineKey::thread("!one:x", "$root"))
    );
}

#[test]
fn switching_rooms_should_close_the_open_thread() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    s.open_thread("$root");
    s.select_room("!two:x");
    assert_eq!(s.active_thread(), None);
}

#[test]
fn showing_someones_profile_should_take_the_open_threads_place() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.open_thread("$root");
    let person = matrix_core::Person { id: "@b:x".into(), name: "B".into(), avatar: None };
    s.show_side(SidePanel::Person(person.clone()));
    assert_eq!(s.active_thread(), None);
    assert_eq!(s.side, Some(SidePanel::Person(person)));
}

#[test]
fn a_reply_should_be_sent_with_the_message_it_answers() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    let answered = Message { event_id: "$q".into(), sender_name: "Grace".into(), ..Message::default() };
    s.start_reply(ComposerTarget::Room, &answered, &mut Tx::default());
    s.composer.edit.insert("sure");
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    assert!(matches!(commands.try_next(), Some((_, Command::Send { reply_to: Some(id), .. })) if id == "$q"));
}

#[test]
fn a_thread_reply_should_go_into_the_thread() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.open_thread("$root");
    drain(&mut commands);
    s.thread_composer.edit.insert("in thread");
    s.send_message(ComposerTarget::Thread, &mut Tx::default());
    assert!(
        matches!(commands.try_next(), Some((_, Command::Send { key, .. })) if key.thread.as_deref() == Some("$root"))
    );
}

#[test]
fn reacting_should_close_the_picker_and_toggle_the_reaction() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    let key = TimelineKey::room("!one:x");
    s.toggle_popover(Popover::Emoji(EmojiFor::Reaction { key: key.clone(), event_id: "$m".into() }));
    s.react(key, "$m", "👍");
    let sent = matches!(commands.try_next(), Some((_, Command::React { emoji, .. })) if emoji == "👍");
    assert!(sent && s.popover.is_none());
}

#[test]
fn a_verification_request_from_another_session_should_open_a_dialog() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    let step = VerificationStep::Incoming { other_device: "Phone".into() };
    apply(&mut s, Event::Verification { account: id, flow_id: "f".into(), step });
    assert!(matches!(&s.modal, Some(Modal::Verification { flow_id: Some(f), .. }) if f == "f"));
}

#[test]
fn accepting_and_confirming_should_answer_the_open_verification() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    let step = VerificationStep::Incoming { other_device: "Phone".into() };
    apply(&mut s, Event::Verification { account: id.clone(), flow_id: "f".into(), step });
    drain(&mut commands);
    s.accept_verification();
    apply(
        &mut s,
        Event::Verification { account: id, flow_id: "f".into(), step: VerificationStep::Compare(Vec::new()) },
    );
    s.confirm_verification(true);
    let sent: Vec<_> = drain(&mut commands).into_iter().map(|(_, c)| c).collect();
    assert!(matches!(
        &sent[..],
        [Command::AcceptVerification { .. }, Command::ConfirmVerification { matches: true, .. }]
    ));
}

#[test]
fn a_wrong_recovery_key_should_show_its_error_in_the_dialog() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.open_recovery_key_entry();
    *s.modal_field() = TextEdit::new("wrong").secure();
    drain(&mut commands);
    s.submit_recovery_key();
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("bad key".into()) });
    assert!(
        matches!(&s.modal, Some(Modal::RecoveryKeyEntry { error: Some(e), submitting: false, .. }) if e == "bad key")
    );
}

#[test]
fn ending_a_session_should_ask_for_the_password_when_the_server_does() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.end_session("OLD");
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::PasswordNeeded { request });
    apply(&mut s, Event::Finished { request, result: Err("password required".into()) });
    *s.modal_field() = TextEdit::new("secret").secure();
    s.confirm_with_password();
    assert!(matches!(
        commands.try_next(),
        Some((_, Command::EndSession { password: Some(Password(p)), .. })) if p == "secret"
    ));
}

#[test]
fn asking_for_the_password_should_not_show_an_error_yet() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.end_session("OLD");
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::PasswordNeeded { request });
    apply(&mut s, Event::Finished { request, result: Err("password required".into()) });
    assert!(matches!(&s.modal, Some(Modal::Password { error: None, .. })));
}

/// Opens the identity reset and confirms its warning; returns the reset's request.
fn reset_identity(s: &mut State, commands: &mut DetachedCommands) -> RequestId {
    with_rooms(s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(commands);
    s.start_identity_reset();
    s.confirm_identity_reset();
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::ResetIdentity { password: None, .. }));
    request
}

#[test]
fn an_identity_reset_should_warn_before_it_starts() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.start_identity_reset();
    assert!(matches!(&s.modal, Some(Modal::ResetIdentity { step: ResetStep::Warning { error: None }, .. })));
    assert!(commands.try_next().is_none());
}

#[test]
fn an_oauth_identity_reset_should_wait_for_approval_at_the_opened_page() {
    let (mut s, mut commands) = state();
    let request = reset_identity(&mut s, &mut commands);
    apply(&mut s, Event::OpenUrl { request, url: "https://account.example.org/reset".into() });
    assert!(matches!(
        &s.modal,
        Some(Modal::ResetIdentity { step: ResetStep::Resetting { url: Some(u), .. }, .. })
            if u == "https://account.example.org/reset"
    ));
}

#[test]
fn a_password_identity_reset_should_retry_with_the_password() {
    let (mut s, mut commands) = state();
    let request = reset_identity(&mut s, &mut commands);
    apply(&mut s, Event::PasswordNeeded { request });
    assert!(matches!(&s.modal, Some(Modal::Password { action: Guarded::ResetIdentity, .. })));
    *s.modal_field() = TextEdit::new("secret").secure();
    s.confirm_with_password();
    assert!(matches!(
        commands.try_next(),
        Some((_, Command::ResetIdentity { password: Some(Password(p)), .. })) if p == "secret"
    ));
}

#[test]
fn a_finished_identity_reset_should_offer_recovery() {
    let (mut s, mut commands) = state();
    let request = reset_identity(&mut s, &mut commands);
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(matches!(&s.modal, Some(Modal::ResetIdentity { step: ResetStep::Done, .. })));
    s.set_up_recovery_after_reset();
    assert!(matches!(commands.try_next(), Some((_, Command::SetUpRecovery(_)))));
}

#[test]
fn a_failed_identity_reset_should_return_to_the_warning_with_its_error() {
    let (mut s, mut commands) = state();
    let request = reset_identity(&mut s, &mut commands);
    apply(&mut s, Event::Finished { request, result: Err("timed out".into()) });
    assert!(matches!(
        &s.modal,
        Some(Modal::ResetIdentity { step: ResetStep::Warning { error: Some(e) }, .. }) if e == "timed out"
    ));
}

#[test]
fn closing_a_waiting_identity_reset_should_cancel_it() {
    let (mut s, mut commands) = state();
    let request = reset_identity(&mut s, &mut commands);
    s.close_modal();
    assert!(matches!(commands.try_next(), Some((_, Command::Cancel(r))) if r == request));
    assert!(s.modal.is_none());
}

#[test]
fn a_new_recovery_key_should_be_shown_once() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::RecoveryKey { account: id, key: Password("KEY".into()) });
    assert!(matches!(&s.modal, Some(Modal::NewRecoveryKey { key }) if key == "KEY"));
}

// ----- profile ------------------------------------------------------------------------------

fn signed_in_with(s: &mut State, sign_in: matrix_core::SignInMethod) -> AccountId {
    let id = AccountId::new("@a:example.org");
    with_rooms(s, &id, &["!one:x"], Vec::new());
    apply(s, Event::AccountChanged(AccountSummary { sign_in, ..account("@a:example.org") }));
    id
}

#[test]
fn the_profile_entry_should_open_the_profile_page_for_a_password_account() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    drain(&mut commands);
    s.open_profile();
    assert_eq!(s.modal, Some(Modal::Settings(SettingsSection::Profile)));
    assert!(matches!(commands.try_next(), Some((_, Command::ListContactDetails(_)))));
}

#[test]
fn the_profile_entry_should_open_the_profile_page_for_an_oauth_account_too() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::OAuth);
    drain(&mut commands);
    s.open_profile();
    assert_eq!(s.modal, Some(Modal::Settings(SettingsSection::Profile)));
    assert!(matches!(commands.try_next(), Some((_, Command::ListContactDetails(_)))));
}

#[test]
fn saving_the_display_name_should_send_it_trimmed() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.show_settings_section(SettingsSection::Profile);
    drain(&mut commands);
    s.profile.name = TextEdit::new("  Ada  ");
    s.save_display_name();
    assert!(matches!(commands.try_next(), Some((_, Command::SetDisplayName { name, .. })) if name == "Ada"));
    assert!(s.profile.saving_name);
}

#[test]
fn an_unchanged_display_name_should_not_be_saved() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::AccountChanged(AccountSummary { display_name: "Ada".into(), ..account("@a:example.org") }));
    s.show_settings_section(SettingsSection::Profile);
    drain(&mut commands);
    s.save_display_name();
    assert!(commands.try_next().is_none());
}

#[test]
fn a_picked_picture_should_become_the_profile_picture() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    drain(&mut commands);
    s.files_picked(PickFor::Avatar, &[PathBuf::from("/tmp/me.png")]);
    assert!(matches!(
        commands.try_next(),
        Some((_, Command::SetAvatar { file: Some(f), .. })) if f.as_path() == std::path::Path::new("/tmp/me.png")
    ));
}

/// Types `address` into the `kind` form and asks for its link or code; returns the request.
fn request_code(s: &mut State, commands: &mut DetachedCommands, kind: ContactKind, address: &str) -> RequestId {
    s.profile.contact_mut(kind).address = TextEdit::new(address);
    drain(commands);
    s.request_contact_code(kind);
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::RequestContactCode { kind: k, .. } if k == kind));
    request
}

#[test]
fn adding_an_email_should_wait_for_its_link_then_add_it_with_the_password() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    let request = request_code(&mut s, &mut commands, ContactKind::Email, "ada@example.org");
    let pending = PendingContact::offline(ContactKind::Email, "ada@example.org");
    apply(&mut s, Event::ContactCodeSent { request, pending });
    s.confirm_contact(ContactKind::Email);
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::AddContact { password: None, .. }));
    apply(&mut s, Event::PasswordNeeded { request });
    *s.modal_field() = TextEdit::new("secret").secure();
    s.confirm_with_password();
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::AddContact { password: Some(Password(p)), .. } if p == "secret"));
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(s.modal.is_none());
    assert_eq!(s.profile.email.step, ContactStep::Entering);
}

#[test]
fn a_phone_code_should_be_sent_before_the_number_is_added() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    let request = request_code(&mut s, &mut commands, ContactKind::Phone, "+49 170 1234567");
    let pending = PendingContact::offline(ContactKind::Phone, "+49 170 1234567");
    apply(&mut s, Event::ContactCodeSent { request, pending });
    s.profile.phone.code = TextEdit::new("123456");
    s.confirm_contact(ContactKind::Phone);
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::ConfirmContactCode { code, .. } if code == "123456"));
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(matches!(commands.try_next(), Some((_, Command::AddContact { .. }))));
}

#[test]
fn a_wrong_code_should_show_its_error_and_keep_the_number_waiting() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    let request = request_code(&mut s, &mut commands, ContactKind::Phone, "+49 170 1234567");
    apply(&mut s, Event::ContactCodeSent { request, pending: PendingContact::offline(ContactKind::Phone, "x") });
    s.profile.phone.code = TextEdit::new("000000");
    s.confirm_contact(ContactKind::Phone);
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("wrong code".into()) });
    assert!(matches!(s.profile.phone.step, ContactStep::Sent { busy: false, .. }));
    assert_eq!(s.profile.phone.error.as_deref(), Some("wrong code"));
}

#[test]
fn a_refused_address_should_show_its_error_on_the_form() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    let request = request_code(&mut s, &mut commands, ContactKind::Email, "taken@example.org");
    apply(&mut s, Event::Finished { request, result: Err("already in use".into()) });
    assert_eq!(s.profile.email.step, ContactStep::Entering);
    assert_eq!(s.profile.email.error.as_deref(), Some("already in use"));
}

#[test]
fn cancelling_the_password_should_leave_the_detail_waiting() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    let request = request_code(&mut s, &mut commands, ContactKind::Email, "ada@example.org");
    apply(&mut s, Event::ContactCodeSent { request, pending: PendingContact::offline(ContactKind::Email, "a") });
    s.confirm_contact(ContactKind::Email);
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::PasswordNeeded { request });
    s.close_modal();
    assert!(matches!(s.profile.email.step, ContactStep::Sent { busy: false, .. }));
}

// ----- launch ---------------------------------------------------------------------------------

#[test]
fn saved_accounts_should_show_the_restoring_screen_instead_of_sign_in() {
    let (mut s, _) = state();
    apply(&mut s, Event::Restoring(vec![AccountId::new("@a:example.org")]));
    assert_eq!(s.screen(), Screen::Restoring);
}

#[test]
fn a_restored_account_should_open_the_chat() {
    let (mut s, _) = state();
    apply(&mut s, Event::Restoring(vec![AccountId::new("@a:example.org")]));
    apply(&mut s, Event::AccountAdded(account("@a:example.org")));
    assert_eq!(s.screen(), Screen::Chat);
}

#[test]
fn an_account_that_could_not_be_restored_should_fall_back_to_sign_in() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::Restoring(vec![id.clone()]));
    apply(&mut s, Event::AccountError { account: id, error: "the server ended this session".into() });
    assert_eq!(s.screen(), Screen::SignIn);
}

#[test]
fn an_ended_session_should_sign_the_account_out_and_offer_to_sign_in_again() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    drain(&mut commands);
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: false });
    assert!(matches!(commands.try_next(), Some((_, Command::Logout(gone))) if gone == id));
    assert!(matches!(&s.modal, Some(Modal::SessionEnded { account, .. }) if *account == id));
}

#[test]
fn signing_in_again_should_look_up_the_server_of_the_account() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SessionEnded { account: id, kept: false });
    drain(&mut commands);
    s.sign_in_again();
    assert!(matches!(s.modal, Some(Modal::SignIn)));
    assert!(matches!(commands.try_next(), Some((_, Command::Discover { server })) if server == "example.org"));
}

/// Answers the discovery just sent with an OAuth server, returning the commands after it.
fn discovered(s: &mut State, commands: &mut DetachedCommands) -> Vec<Command> {
    let (request, _) = commands.try_next().unwrap();
    let methods = LoginMethods { oauth: true, password: false };
    let homeserver = Homeserver { server_name: None, url: "https://matrix.example.org/".into(), methods };
    apply(s, Event::HomeserverDiscovered { request, homeserver });
    drain(commands).into_iter().map(|(_, c)| c).collect()
}

#[test]
fn a_soft_sign_out_should_keep_the_accounts_data() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    drain(&mut commands);
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    let sent = all_commands(&mut commands);
    assert!(sent.iter().any(|c| matches!(c, Command::Suspend(kept) if *kept == id)));
    assert!(!sent.iter().any(|c| matches!(c, Command::Logout(_))));
}

#[test]
fn signing_in_again_after_a_soft_sign_out_should_take_the_data_back() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    drain(&mut commands);
    s.sign_in_again();
    let sent = discovered(&mut s, &mut commands);
    assert!(matches!(&sent[..], [Command::LoginOAuth { resume: Some(resumed), .. }] if *resumed == id));
}

#[test]
fn removing_an_account_signed_out_softly_should_forget_it_here() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    s.remove_signed_out();
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::Logout(gone) if *gone == id)));
}

#[test]
fn adding_another_account_should_not_take_back_a_signed_out_ones_data() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SessionEnded { account: id, kept: true });
    s.close_modal();
    s.start_adding_account();
    drain(&mut commands);
    s.submit_server();
    let sent = discovered(&mut s, &mut commands);
    assert!(matches!(&sent[..], [Command::LoginOAuth { resume: None, .. }]));
}

// ----- attachments ------------------------------------------------------------------------------

fn attachment(name: &str, kind: matrix_core::AttachmentKind) -> matrix_core::Attachment {
    matrix_core::Attachment {
        kind,
        name: name.into(),
        size: None,
        media: Some(matrix_core::MediaRef::opaque(name)),
        thumbnail: None,
        dimensions: None,
        audio: None,
    }
}

#[test]
fn clicking_an_image_should_preview_it_full_size() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.open_attachment(&attachment("cat.png", matrix_core::AttachmentKind::Image));
    assert!(matches!(
        &s.preview,
        Some(Preview { content: PreviewContent::Image { status: ImageStatus::Downloading, .. }, .. })
    ));
    assert!(matches!(
        commands.try_next(),
        Some((_, Command::FetchMedia { size: matrix_core::MediaSize::Original, .. }))
    ));
}

#[test]
fn a_decoded_attachment_preview_should_refresh_without_another_user_action() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.media.attach(Arc::new(|| {}));
    s.poll(&mut Tx::default());
    drain(&mut commands);
    s.open_attachment(&attachment("cat.png", matrix_core::AttachmentKind::Image));
    let (request, _) = commands.try_next().unwrap();
    let bytes: Arc<[u8]> = include_bytes!("../../../../packaging/icons/better-matrix-256.png").as_slice().into();
    apply(
        &mut s,
        Event::MediaLoaded {
            request,
            media: MediaRef::opaque("cat.png"),
            size: MediaSize::Original,
            bytes: bytes.clone(),
        },
    );
    s.media.decode(bytes).unwrap().wait().unwrap();
    assert!(s.poll(&mut Tx::default()), "finishing the original decode must rebuild the viewer");
    assert!(!s.poll(&mut Tx::default()), "a ready preview should stop rebuilding");
}

#[test]
fn opening_an_attachment_should_show_its_cached_thumbnail_while_the_original_loads() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.media.attach(Arc::new(|| {}));
    let file = attachment("cat.png", matrix_core::AttachmentKind::Image);
    let bytes: Arc<[u8]> = include_bytes!("../../../../packaging/icons/better-matrix-256.png").as_slice().into();
    s.media.loaded(file.media.clone().unwrap(), bytes.clone(), Some((256, 256)));
    s.media.decode(bytes).unwrap().wait().unwrap();
    s.media.measure();
    s.open_attachment(&file);
    assert!(matches!(
        &s.preview,
        Some(Preview { content: PreviewContent::Image { thumbnail: Some(_), status: ImageStatus::Downloading }, .. })
    ));
}

#[test]
fn clicking_a_pdf_should_preview_its_pages() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.open_attachment(&attachment("Report.PDF", matrix_core::AttachmentKind::File));
    assert!(matches!(
        &s.preview,
        Some(Preview { content: PreviewContent::Document { status: DocumentStatus::Downloading, .. }, .. })
    ));
}

#[test]
fn clicking_another_file_should_not_preview_it() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.open_attachment(&attachment("notes.zip", matrix_core::AttachmentKind::File));
    assert!(s.preview.is_none());
    assert!(commands.try_next().is_none());
}

#[test]
fn a_preview_that_failed_to_download_should_say_so() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.open_attachment(&attachment("Report.pdf", matrix_core::AttachmentKind::File));
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("gone".into()) });
    assert!(matches!(
        &s.preview,
        Some(Preview { content: PreviewContent::Document { status: DocumentStatus::Failed(e), .. }, .. }) if e == "gone"
    ));
}

#[test]
fn pasting_an_image_should_attach_it_to_the_draft() {
    let (mut s, _) = state();
    // The smallest PNG header Rune accepts: signature, IHDR, 1 × 1 pixels.
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1]);
    let image = rune::clipboard::ClipboardImage::from_png(png).unwrap();
    let content = rune::clipboard::ClipboardContent::new("").with_image(image);
    assert!(s.paste_into_composer(ComposerTarget::Room, &content));
    assert_eq!(s.composer.attachments.first().map(matrix_core::Upload::name).as_deref(), Some("image.png"));
}

#[test]
fn pasting_text_should_be_left_to_the_editor() {
    let (mut s, _) = state();
    let content = rune::clipboard::ClipboardContent::new("hello");
    assert!(!s.paste_into_composer(ComposerTarget::Room, &content));
    assert!(s.composer.attachments.is_empty());
}

#[test]
fn pasting_a_web_address_should_make_it_a_link() {
    let (mut s, _) = state();
    let content = rune::clipboard::ClipboardContent::new("https://github.com/m43i/better-matrix\n");
    assert!(s.paste_into_composer(ComposerTarget::Room, &content));
    assert!(s.composer.edit.to_html().contains("<a href=\"https://github.com/m43i/better-matrix\">"));
}

// ----- composer, people and audio ---------------------------------------------------------------

#[test]
fn an_emoji_picked_for_a_draft_should_land_in_it() {
    let (mut s, _) = state();
    s.toggle_popover(Popover::Emoji(EmojiFor::Composer(ComposerTarget::Room)));
    s.choose_emoji(EmojiFor::Composer(ComposerTarget::Room), "🎉");
    assert_eq!(s.composer.edit.text, "🎉");
    assert!(s.popover.is_none());
}

#[test]
fn opening_a_room_should_list_its_members() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.select_room("!one:x");
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::ListMembers { room_id, .. } if room_id == "!one:x"))
    );
}

/// Lets what was typed in the searches rest long enough to be searched.
fn rest_search(s: &mut State) {
    s.open_search();
    let now = std::time::Instant::now();
    s.search_people(now);
    s.search_people(now + search::SEARCH_PAUSE);
}

#[test]
fn typing_a_search_should_look_up_people_and_show_results() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.search.field.insert("ada");
    rest_search(&mut s);
    assert!(
        all_commands(&mut commands).iter().any(|c| matches!(c, Command::SearchPeople { term, .. } if term == "ada"))
    );
    assert_eq!(s.popover, Some(Popover::Search));
}

#[test]
fn clearing_the_search_should_keep_its_filters_available() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.search.field.insert("ada");
    rest_search(&mut s);
    s.search.field = TextEdit::new("");
    s.poll(&mut Tx::default());
    assert_eq!(s.popover, Some(Popover::Search));
}

#[test]
fn inviting_someone_twice_should_send_one_invite() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.open_invite();
    drain(&mut commands);
    s.invite("@b:example.org");
    s.invite("@b:example.org");
    let invites = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::Invite { .. })).count();
    assert_eq!(invites, 1);
}

#[test]
fn opening_a_direct_chat_should_show_it_once_ready() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x", "!dm:x"], Vec::new());
    drain(&mut commands);
    s.open_direct_chat("@b:example.org");
    let (request, command) = commands.try_next().unwrap();
    assert!(matches!(command, Command::OpenDirectChat { user_id, .. } if user_id == "@b:example.org"));
    apply(&mut s, Event::DirectChat { request, room_id: "!dm:x".into() });
    assert_eq!(s.active_room_id(), Some("!dm:x"));
}

#[test]
fn playing_audio_should_download_it_once_to_play() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.toggle_audio(&memo);
    s.toggle_audio(&memo);
    let saves = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::SaveMedia { .. })).count();
    assert_eq!(saves, 1);
}

#[test]
fn scrubbing_a_clip_before_it_plays_should_start_it_there() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.seek_audio(&memo, 0.4);
    s.seek_audio(&memo, 0.6);
    let saves = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::SaveMedia { .. })).count();
    assert_eq!((saves, s.audio.as_ref().and_then(|clip| clip.start_at)), (1, Some(0.6)));
}

/// A second of silence playing, decoded, on no speaker, as the current clip.
fn playing_clip(s: &mut State) -> matrix_core::Attachment {
    use crate::platform::sound::{NativePlayer, Pcm, Player};
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    let player = Player::Native(NativePlayer::decoded(Pcm { rate: 24_000, channels: 1, samples: vec![0.0; 24_000] }));
    player.play();
    let media = memo.media.clone().unwrap_or_else(|| panic!("an attachment with media"));
    s.audio = Some(AudioClip {
        id: 0,
        media,
        player: Some(player),
        start_at: None,
        play: true,
        held: false,
        scrub_from: None,
    });
    memo
}

fn clip_playing(s: &State) -> bool {
    s.audio.as_ref().and_then(|clip| clip.player.as_ref()).is_some_and(crate::platform::sound::Player::is_playing)
}

#[test]
fn dragging_along_a_playing_clip_should_pause_it_until_let_go() {
    let (mut s, _commands) = state();
    let memo = playing_clip(&mut s);
    s.scrub_audio(&memo, 0.3, Scrub::Press);
    let held = clip_playing(&s);
    s.scrub_audio(&memo, 0.6, Scrub::Release);
    assert_eq!((held, clip_playing(&s)), (false, true));
}

#[test]
fn letting_go_of_a_clip_at_its_end_should_leave_it_paused_there() {
    let (mut s, _commands) = state();
    let memo = playing_clip(&mut s);
    s.scrub_audio(&memo, 0.8, Scrub::Press);
    s.scrub_audio(&memo, 1.0, Scrub::Release);
    let progress = s.audio.as_ref().map_or(0.0, AudioClip::progress);
    assert_eq!((clip_playing(&s), progress), (false, 1.0));
}

#[test]
fn a_clip_that_comes_while_its_waveform_is_held_should_wait_to_be_let_go() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.scrub_audio(&memo, 0.5, Scrub::Press);
    let held = s.audio.as_ref().is_some_and(|clip| clip.held && clip.play && clip.player.is_none());
    s.scrub_audio(&memo, 0.6, Scrub::Release);
    let waiting = s.audio.as_ref().is_some_and(|clip| !clip.held && clip.start_at == Some(0.6));
    assert!(held && waiting, "held {held}, then waiting to play from 0.6: {waiting}");
}

#[test]
fn play_on_a_downloading_clip_moved_to_its_end_should_start_it_over() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.seek_audio(&memo, 1.0);
    s.toggle_audio(&memo);
    assert_eq!(s.audio.as_ref().map(|clip| clip.start_at), Some(None));
}

#[test]
fn a_played_clip_should_keep_its_waveform_once_another_plays() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let first = playing_clip(&mut s);
    s.toggle_audio(&attachment("second.ogg", matrix_core::AttachmentKind::Audio));
    let kept = s.decoded_audio.iter().find(|decoded| first.media.as_ref() == Some(&decoded.media));
    assert_eq!(kept.map(|decoded| (decoded.waveform.len(), decoded.duration)), Some((100, Some(1.0))));
}

#[test]
fn an_earlier_download_of_a_clip_with_its_player_should_not_start_it_over() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = playing_clip(&mut s);
    s.seek_audio(&memo, 0.5);
    // The download started before it was last played arrives now.
    let media = memo.media.clone().unwrap_or_else(|| panic!("an attachment with media"));
    let save = Command::SaveMedia { account: AccountId::new("@a:example.org"), media: media.clone(), path: "a".into() };
    let request = s.dispatch(save, Purpose::Audio { media, path: "a".into() });
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    let progress = s.audio.as_ref().map_or(0.0, AudioClip::progress);
    assert_eq!(progress, 0.5);
}

#[test]
fn a_clip_played_again_while_its_file_downloads_should_download_it_once() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let audio = |name| attachment(name, matrix_core::AttachmentKind::Audio);
    let (first, second) = (audio("a.ogg"), audio("b.ogg"));
    s.toggle_audio(&first);
    s.toggle_audio(&second);
    s.toggle_audio(&first);
    // Only the second clip's download follows the first's.
    let saves = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::SaveMedia { .. })).count();
    assert_eq!(saves, 2);
}

#[test]
fn a_saved_clip_played_again_should_play_its_file_without_downloading_it_again() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let audio = |name| attachment(name, matrix_core::AttachmentKind::Audio);
    let (first, second) = (audio("a.ogg"), audio("b.ogg"));
    s.toggle_audio(&first);
    let (request, _) = commands.try_next().unwrap_or_else(|| panic!("a download"));
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    s.toggle_audio(&second);
    s.toggle_audio(&first);
    // Only the second clip's download follows the first's.
    let saves = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::SaveMedia { .. })).count();
    let playing = s.audio.as_ref().is_some_and(|clip| {
        clip.media == first.media.clone().unwrap_or_else(|| panic!("media")) && clip.player.is_some()
    });
    assert_eq!((saves, playing), (1, true));
}

#[test]
fn a_failure_from_a_clip_picked_before_should_not_stop_it_now() {
    let (mut s, _commands) = state();
    let memo = playing_clip(&mut s);
    if let Some(clip) = s.audio.as_mut() {
        clip.id = 2;
    }
    let media = memo.media.unwrap_or_else(|| panic!("media"));
    s.audio_failed(&media, Some(1), "gone".into());
    assert!(s.audio.is_some() && s.notice.is_none());
}

#[test]
fn holding_a_saved_clip_s_waveform_should_wait_for_the_release_to_play() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let audio = |name| attachment(name, matrix_core::AttachmentKind::Audio);
    let (first, second) = (audio("a.ogg"), audio("b.ogg"));
    s.toggle_audio(&first);
    let (request, _) = commands.try_next().unwrap_or_else(|| panic!("a download"));
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    s.toggle_audio(&second);
    s.scrub_audio(&first, 0.3, Scrub::Press);
    let waiting = s
        .audio
        .as_ref()
        .is_some_and(|clip| clip.held && clip.play && clip.player.as_ref().is_some_and(|player| !player.is_playing()));
    assert!(waiting, "the saved file is open, paused, waiting to be let go");
}

#[test]
fn a_cancelled_press_on_a_clip_should_unpick_it() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.scrub_audio(&memo, 0.4, Scrub::Press);
    s.scrub_audio(&memo, 0.7, Scrub::Cancel);
    assert!(s.audio.is_none());
}

#[test]
fn a_cancelled_drag_should_put_a_playing_clip_back_where_it_was_playing() {
    let (mut s, _commands) = state();
    let memo = playing_clip(&mut s);
    s.seek_audio(&memo, 0.25);
    s.scrub_audio(&memo, 0.3, Scrub::Press);
    s.scrub_audio(&memo, 0.9, Scrub::Drag);
    s.scrub_audio(&memo, 0.9, Scrub::Cancel);
    let progress = s.audio.as_ref().map_or(0.0, AudioClip::progress);
    assert_eq!((clip_playing(&s), progress), (true, 0.25));
}

#[test]
fn a_release_after_the_clip_failed_should_not_pick_it_again() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.scrub_audio(&memo, 0.4, Scrub::Press);
    let (request, _) = commands.try_next().unwrap_or_else(|| panic!("a download"));
    apply(&mut s, Event::Finished { request, result: Err("gone".into()) });
    s.scrub_audio(&memo, 0.6, Scrub::Release);
    let saves = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::SaveMedia { .. })).count();
    assert_eq!((s.audio.is_none(), saves), (true, 0));
}

#[test]
fn cancelling_a_scrub_of_a_downloading_clip_left_at_its_end_should_not_play_it() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.seek_audio(&memo, 1.0);
    s.scrub_audio(&memo, 0.5, Scrub::Press);
    assert!(s.audio.as_ref().is_some_and(|clip| clip.held && !clip.play));
}

#[test]
fn scrubbing_back_a_downloading_clip_left_at_its_end_should_keep_it_paused() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.seek_audio(&memo, 1.0);
    s.scrub_audio(&memo, 0.5, Scrub::Press);
    s.scrub_audio(&memo, 0.5, Scrub::Release);
    let clip = s.audio.as_ref().map(|clip| (clip.play, clip.start_at));
    assert_eq!(clip, Some((false, Some(0.5))));
}

#[test]
fn dragging_a_playing_clip_to_its_end_and_back_should_play_on_when_let_go() {
    let (mut s, _commands) = state();
    let memo = playing_clip(&mut s);
    s.scrub_audio(&memo, 0.5, Scrub::Press);
    s.scrub_audio(&memo, 1.0, Scrub::Drag);
    s.scrub_audio(&memo, 0.9, Scrub::Release);
    assert!(clip_playing(&s));
}

#[test]
fn a_press_past_the_start_of_a_clip_s_waveform_should_start_it_at_the_start() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.scrub_audio(&memo, -0.02, Scrub::Press);
    assert_eq!(s.audio.as_ref().and_then(|clip| clip.start_at), Some(0.0));
}

#[test]
fn a_drag_from_the_end_of_a_new_clip_should_play_it_from_where_it_is_let_go() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.scrub_audio(&memo, 1.0, Scrub::Press);
    s.scrub_audio(&memo, 0.4, Scrub::Release);
    assert_eq!(s.audio.as_ref().map(|clip| (clip.play, clip.start_at)), Some((true, Some(0.4))));
}

#[test]
fn a_second_press_on_a_downloading_clip_should_take_back_playing_it() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.toggle_audio(&memo);
    s.toggle_audio(&memo);
    let paused = s.audio.as_ref().is_some_and(|clip| !clip.play);
    s.toggle_audio(&memo);
    assert_eq!((paused, s.audio.as_ref().map(|clip| clip.play)), (true, Some(true)));
}

#[test]
fn going_to_the_end_of_a_playing_clip_should_leave_it_paused_there() {
    let (mut s, _commands) = state();
    let memo = playing_clip(&mut s);
    s.seek_audio(&memo, 1.0);
    let progress = s.audio.as_ref().map_or(0.0, AudioClip::progress);
    assert_eq!((clip_playing(&s), progress), (false, 1.0));
}

#[test]
fn an_arrow_key_on_a_clip_should_move_it_a_step_along() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    let memo = attachment("memo.ogg", matrix_core::AttachmentKind::Audio);
    s.seek_audio(&memo, 0.5);
    s.step_audio(&memo, -2);
    let start = s.audio.as_ref().and_then(|clip| clip.start_at).unwrap_or_default();
    assert!((start - 0.4).abs() < 1e-9, "at {start}");
}

#[test]
fn the_speed_button_should_step_through_the_speeds_and_back() {
    let (mut s, _commands) = state();
    let speeds: Vec<f64> = (0..3)
        .map(|_| {
            s.cycle_playback_speed();
            s.settings.playback_speed.rate()
        })
        .collect();
    assert_eq!(speeds, vec![1.5, 2.0, 1.0]);
}

#[test]
fn a_saved_playback_speed_should_read_back() {
    let speed = crate::state::PlaybackSpeed::Fastest;
    assert_eq!(crate::state::PlaybackSpeed::from_percent(speed.percent()), speed);
}

#[test]
fn a_recorded_voice_memo_should_be_sent_as_one() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.start_voice_memo(ComposerTarget::Room);
    assert!(s.composer.voice.is_some());
    s.send_voice_memo(ComposerTarget::Room, &mut Tx::default());
    let sent = all_commands(&mut commands)
        .into_iter()
        .any(|c| matches!(c, Command::Send { message: matrix_core::Outgoing::Voice { .. }, .. }));
    assert!(sent || s.notice.is_some(), "a memo is sent, or the recorder says why not");
    assert!(s.composer.voice.is_none());
}

#[test]
fn a_recorded_voice_memo_should_carry_its_waveform() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    let memo_for = crate::state::MemoFor {
        target: ComposerTarget::Room,
        account: AccountId::new("@a:example.org"),
        key: TimelineKey::room("!one:x"),
        reply_to: None,
    };
    let recording =
        crate::platform::sound::Recording { path: "memo.wav".into(), duration: 2.0, waveform: vec![10, 900] };
    s.voice_memo_recorded(memo_for, Ok(recording), &mut Tx::default());
    let waveform = all_commands(&mut commands).into_iter().find_map(|c| match c {
        Command::Send { message: matrix_core::Outgoing::Voice { waveform, .. }, .. } => Some(waveform),
        _ => None,
    });
    assert_eq!(waveform, Some(vec![10, 900]));
}

/// Every command sent, in order.
fn all_commands(commands: &mut DetachedCommands) -> Vec<Command> {
    drain(commands).into_iter().map(|(_, c)| c).collect()
}

#[test]
fn a_direct_chat_ready_after_switching_accounts_should_not_open() {
    let (mut s, mut commands) = state();
    let (a, b) = (AccountId::new("@a:example.org"), AccountId::new("@b:example.org"));
    with_rooms(&mut s, &a, &["!dm:x"], Vec::new());
    with_rooms(&mut s, &b, &["!other:x"], Vec::new());
    drain(&mut commands);
    s.open_direct_chat("@c:example.org");
    let (request, _) = commands.try_next().unwrap();
    s.select_account(&b);
    apply(&mut s, Event::DirectChat { request, room_id: "!dm:x".into() });
    assert_eq!(s.active_room_id(), None);
}

#[test]
fn new_messages_in_the_open_room_should_be_marked_read() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    apply(&mut s, Event::TimelineChanged { account: id, key: TimelineKey::room("!one:x"), diffs: Vec::new() });
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::MarkRead { .. })));
}

#[test]
fn new_messages_should_stay_unread_while_the_window_is_in_the_background() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.focus_changed(false);
    drain(&mut commands);
    apply(&mut s, Event::TimelineChanged { account: id, key: TimelineKey::room("!one:x"), diffs: Vec::new() });
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::MarkRead { .. })));
}

#[test]
fn coming_back_to_the_window_should_mark_the_open_room_read() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.focus_changed(false);
    apply(
        &mut s,
        Event::TimelineChanged {
            account: AccountId::new("@a:example.org"),
            key: TimelineKey::room("!one:x"),
            diffs: Vec::new(),
        },
    );
    drain(&mut commands);
    s.focus_changed(true);
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::MarkRead { .. })));
}

#[test]
fn a_layout_change_during_a_save_should_be_saved_after_it() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SidebarLayout { account: id, layout: matrix_core::SidebarLayout::default() });
    drain(&mut commands);
    s.toggle_section("rooms");
    s.toggle_section("direct");
    let saves: Vec<_> =
        drain(&mut commands).into_iter().filter(|(_, c)| matches!(c, Command::SetSidebarLayout { .. })).collect();
    assert_eq!(saves.len(), 1, "one save at a time");
    apply(&mut s, Event::Finished { request: saves[0].0, result: Ok(()) });
    let Some(Command::SetSidebarLayout { layout, .. }) =
        all_commands(&mut commands).into_iter().find(|c| matches!(c, Command::SetSidebarLayout { .. }))
    else {
        panic!("the later change should be saved");
    };
    assert_eq!(layout.scopes["home"].collapsed.len(), 2);
}

#[test]
fn a_layout_from_sync_should_not_replace_one_being_saved() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SidebarLayout { account: id.clone(), layout: matrix_core::SidebarLayout::default() });
    s.toggle_section("rooms");
    drain(&mut commands);
    apply(&mut s, Event::SidebarLayout { account: id.clone(), layout: matrix_core::SidebarLayout::default() });
    assert!(!s.accounts.get(&id).unwrap().sidebar.scopes.is_empty());
}

#[test]
fn a_thumbnail_of_the_previewed_file_should_not_replace_its_preview() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.open_attachment(&attachment("Report.pdf", matrix_core::AttachmentKind::File));
    let media = s.preview.as_ref().map(|preview| preview.media.clone()).unwrap();
    // Another fetch of the same file: a thumbnail for the timeline.
    apply(
        &mut s,
        Event::MediaLoaded {
            request: RequestId(u64::MAX),
            media,
            size: matrix_core::MediaSize::Thumbnail { width: 64, height: 64 },
            bytes: Arc::from(&b"not a pdf"[..]),
        },
    );
    assert!(matches!(
        &s.preview,
        Some(Preview { content: PreviewContent::Document { status: DocumentStatus::Downloading, .. }, .. })
    ));
}

#[test]
fn timelines_long_closed_should_let_go_of_their_messages() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    let rooms: Vec<String> = (0..10).map(|i| format!("!{i}:x")).collect();
    with_rooms(&mut s, &id, &rooms.iter().map(String::as_str).collect::<Vec<_>>(), Vec::new());
    for room in &rooms {
        s.select_room(room);
        let entry =
            matrix_core::TimelineEntry { id: "m".into(), kind: matrix_core::EntryKind::Message(Box::default()) };
        let diffs = vec![matrix_core::TimelineDiff::Reset(vec![entry])];
        apply(&mut s, Event::TimelineChanged { account: id.clone(), key: TimelineKey::room(room), diffs });
    }
    let kept = |s: &mut State, room: &str| {
        s.accounts.timeline_mut(&id, &TimelineKey::room(room)).is_some_and(|t| !t.entries.is_empty())
    };
    assert!(!kept(&mut s, "!0:x"));
    assert!(kept(&mut s, "!1:x"));
}

fn member(id: &str) -> matrix_core::RoomMember {
    matrix_core::RoomMember {
        person: matrix_core::Person { id: id.into(), name: id.into(), avatar: None },
        role: matrix_core::MemberRole::Member,
        presence: matrix_core::Presence::Unknown,
        status_message: None,
        verified: false,
        level: 0,
    }
}

#[test]
fn rooms_long_closed_should_let_go_of_their_members() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    let rooms: Vec<String> = (0..10).map(|i| format!("!{i}:x")).collect();
    with_rooms(&mut s, &id, &rooms.iter().map(String::as_str).collect::<Vec<_>>(), Vec::new());
    for room in &rooms {
        s.select_room(room);
        let members = matrix_core::Members { people: vec![member("@b:x")], complete: true, ..Default::default() };
        apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: room.clone(), members });
    }
    let data = s.accounts.get(&id).unwrap();
    assert_eq!((data.members.contains_key("!0:x"), data.members.contains_key("!1:x")), (false, true));
}

#[test]
fn a_late_history_answer_should_not_bring_back_a_timeline_let_go_of() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    let key = TimelineKey::room("!one:x");
    apply(&mut s, Event::HistoryLoaded { account: id.clone(), key: key.clone(), reached_start: true });
    apply(&mut s, Event::HistoryFailed { account: id.clone(), key: key.clone(), error: "gone".into() });
    apply(&mut s, Event::ReadBoundary { account: id.clone(), room_id: "!one:x".into(), event_id: None, loaded: true });
    assert!(s.accounts.get(&id).unwrap().timeline(&key).is_none());
}

#[test]
fn retrying_history_of_a_timeline_not_kept_should_not_make_one() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    let key = TimelineKey::room("!one:x");
    s.retry_history(&key);
    assert!(s.accounts.get(&id).unwrap().timeline(&key).is_none());
}

#[test]
fn the_sidebar_should_not_be_saved_before_the_saved_layout_is_read() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.toggle_section("rooms");
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::SetSidebarLayout { .. })));
}

#[test]
fn opening_a_room_should_list_only_the_members_already_known() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.select_room("!one:x");
    assert_eq!(
        all_commands(&mut commands).iter().filter(|c| matches!(c, Command::ListMembers { all: false, .. })).count(),
        1
    );
}

#[test]
fn opening_the_member_list_should_fetch_everyone() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.toggle_members();
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::ListMembers { all: true, .. })));
}

#[test]
fn a_partial_member_list_should_not_replace_a_complete_one() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    let person = |id: &str| matrix_core::RoomMember {
        person: matrix_core::Person { id: id.into(), name: id.into(), avatar: None },
        role: matrix_core::MemberRole::Member,
        presence: matrix_core::Presence::Unknown,
        status_message: None,
        verified: false,
        level: 0,
    };
    let all =
        matrix_core::Members { people: vec![person("@a:x"), person("@b:x")], complete: true, ..Default::default() };
    let known = matrix_core::Members { people: vec![person("@a:x")], complete: false, ..Default::default() };
    apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: "!one:x".into(), members: all });
    apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: "!one:x".into(), members: known });
    assert_eq!(s.accounts.get(&id).unwrap().members["!one:x"].people.len(), 2);
}

#[test]
fn switching_rooms_should_close_the_previous_side_panel() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    s.show_side(SidePanel::Info);
    s.select_room("!two:x");
    assert_eq!(s.side, None);
}

#[test]
fn an_invitation_should_remain_available_after_closing_its_preview() {
    let (mut s, _) = state();
    crate::demo::invitation(&mut s);
    s.close_modal();
    assert_eq!(s.active_account().unwrap().invitations.len(), 1);
}

#[test]
fn accepting_an_invitation_should_join_with_its_receiving_account() {
    let (mut s, mut commands) = state();
    crate::demo::invitation(&mut s);
    drain(&mut commands);
    let id = AccountId::new(crate::demo::ADA);
    s.respond_to_invitation(&id, "!design:example.org", true);
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command,
        Command::JoinRoom { account, room_id, .. } if account == &id && room_id == "!design:example.org")));
}

#[test]
fn declining_an_invitation_should_leave_without_forgetting_the_room() {
    let (mut s, mut commands) = state();
    crate::demo::invitation(&mut s);
    drain(&mut commands);
    let id = AccountId::new(crate::demo::ADA);
    s.respond_to_invitation(&id, "!design:example.org", false);
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command,
        Command::LeaveRoom { account, room_id, forget: false } if account == &id && room_id == "!design:example.org")));
}

#[test]
fn a_failed_invitation_response_should_leave_the_invitation_available_to_retry() {
    let (mut s, mut commands) = state();
    crate::demo::invitation(&mut s);
    drain(&mut commands);
    let id = AccountId::new(crate::demo::ADA);
    s.respond_to_invitation(&id, "!design:example.org", true);
    let (request, _) = commands.try_next().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    assert!(!s.membership_pending(&id, "!design:example.org") && s.active_account().unwrap().invitations.len() == 1);
}

#[test]
fn opening_a_space_should_request_its_public_rooms_once() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    s.select_scope(Scope::Space(crate::demo::WORK_SPACE.into()));
    s.show_overview();
    assert_eq!(
        all_commands(&mut commands).iter().filter(|command| matches!(command, Command::ListPublicRooms { .. })).count(),
        1
    );
}

#[test]
fn leaving_the_selected_space_should_return_to_the_home_overview() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    let id = AccountId::new(crate::demo::ADA);
    s.select_scope(Scope::Space(crate::demo::WORK_SPACE.into()));
    s.select_room(crate::demo::DEMO_THREAD.0);
    let rooms = s.accounts.get(&id).unwrap().rooms.clone();
    apply(&mut s, Event::RoomsUpdated { account: id, list: RoomList { rooms, ..Default::default() } });
    assert_eq!((s.active_scope(), s.active_room_id()), (Scope::Home, None));
}

#[test]
fn reaching_the_new_marker_should_advance_the_cross_device_read_marker() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let key = TimelineKey::room("!one:x");
    let entries = vec![
        matrix_core::TimelineEntry { id: "new".into(), kind: matrix_core::EntryKind::ReadMarker },
        matrix_core::TimelineEntry {
            id: "message".into(),
            kind: matrix_core::EntryKind::Message(Box::new(Message {
                event_id: "$message".into(),
                ..Default::default()
            })),
        },
    ];
    apply(
        &mut s,
        Event::TimelineChanged {
            account: id.clone(),
            key: key.clone(),
            diffs: vec![matrix_core::TimelineDiff::Reset(entries)],
        },
    );
    let before = drain(&mut commands);
    assert!(before.iter().any(|(_, command)| matches!(command, Command::MarkRead { fully_read: false, .. })));
    for (request, command) in before {
        if matches!(command, Command::MarkRead { .. }) {
            apply(&mut s, Event::Finished { request, result: Ok(()) });
        }
    }
    s.read_marker_reached(&id, &key, "new");
    assert!(
        all_commands(&mut commands).iter().any(|command| matches!(command, Command::MarkRead { fully_read: true, .. }))
    );
}

#[test]
fn an_unloaded_read_boundary_should_not_be_treated_as_a_read_room() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    apply(
        &mut s,
        Event::ReadBoundary {
            account: id.clone(),
            room_id: "!one:x".into(),
            event_id: Some("$old".into()),
            loaded: false,
        },
    );
    apply(
        &mut s,
        Event::TimelineChanged {
            account: id,
            key: TimelineKey::room("!one:x"),
            diffs: vec![matrix_core::TimelineDiff::Reset(vec![matrix_core::TimelineEntry {
                id: "latest".into(),
                kind: matrix_core::EntryKind::Message(Box::new(Message {
                    event_id: "$latest".into(),
                    ..Default::default()
                })),
            }])],
        },
    );
    let reads = all_commands(&mut commands);
    assert!(reads.iter().any(|command| matches!(command, Command::MarkRead { fully_read: false, .. })));
    assert!(!reads.iter().any(|command| matches!(command, Command::MarkRead { fully_read: true, .. })));
}

#[test]
fn a_loaded_boundary_without_unread_messages_should_allow_cross_device_reads() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    apply(
        &mut s,
        Event::ReadBoundary {
            account: id.clone(),
            room_id: "!one:x".into(),
            event_id: Some("$latest".into()),
            loaded: true,
        },
    );
    apply(
        &mut s,
        Event::TimelineChanged {
            account: id,
            key: TimelineKey::room("!one:x"),
            diffs: vec![matrix_core::TimelineDiff::Reset(vec![matrix_core::TimelineEntry {
                id: "latest".into(),
                kind: matrix_core::EntryKind::Message(Box::new(Message {
                    event_id: "$latest".into(),
                    ..Default::default()
                })),
            }])],
        },
    );
    assert!(
        all_commands(&mut commands).iter().any(|command| matches!(command, Command::MarkRead { fully_read: true, .. }))
    );
}

#[test]
fn reading_older_history_should_target_the_last_viewport_message_instead_of_new_arrivals() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    apply(&mut s, Event::TimelineChanged { account: id.clone(), key: TimelineKey::room("!one:x"), diffs: Vec::new() });
    for (request, command) in drain(&mut commands) {
        if matches!(command, Command::MarkRead { .. }) {
            apply(&mut s, Event::Finished { request, result: Ok(()) });
        }
    }
    s.read_position_shown(&id, &TimelineKey::room("!one:x"), "$visible".into());
    let mut reads = all_commands(&mut commands);
    reads.extend(s.receipts.due(std::time::Instant::now() + std::time::Duration::from_secs(1)));
    assert!(reads.iter().any(
        |command| matches!(command, Command::MarkRead { event_id: Some(event_id), .. } if event_id == "$visible")
    ));
}

#[test]
fn membership_changes_should_refresh_complete_lists_and_ignore_older_requests() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let old_request =
        drain(&mut commands).into_iter().find(|(_, command)| matches!(command, Command::ListMembers { .. })).unwrap().0;
    apply(
        &mut s,
        Event::Members {
            request: None,
            account: id.clone(),
            room_id: "!one:x".into(),
            members: matrix_core::Members { complete: true, ..Default::default() },
        },
    );
    apply(&mut s, Event::MembersInvalidated { account: id.clone(), room_id: "!one:x".into() });
    assert!(s.accounts.get(&id).unwrap().members.contains_key("!one:x"));
    assert!(!s.accounts.members_fresh(&id, "!one:x"));
    let refresh = drain(&mut commands)
        .into_iter()
        .find(|(_, command)| matches!(command, Command::ListMembers { all: true, .. }))
        .unwrap()
        .0;
    apply(
        &mut s,
        Event::Members {
            request: Some(old_request),
            account: id.clone(),
            room_id: "!one:x".into(),
            members: matrix_core::Members { complete: true, ..Default::default() },
        },
    );
    assert!(!s.accounts.members_fresh(&id, "!one:x"), "the older snapshot must stay invalid");
    apply(
        &mut s,
        Event::Members {
            request: Some(refresh),
            account: id.clone(),
            room_id: "!one:x".into(),
            members: matrix_core::Members { complete: true, ..Default::default() },
        },
    );
    assert!(s.accounts.get(&id).unwrap().members["!one:x"].complete);
    assert!(s.accounts.members_fresh(&id, "!one:x"));
}

#[test]
fn viewport_progress_should_not_invalidate_the_timeline_row_plan() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let key = TimelineKey::room("!one:x");
    let before = s.accounts.get(&id).unwrap().timeline(&key).unwrap().revision;
    s.read_position_shown(&id, &key, "$visible".into());
    assert_eq!(before, s.accounts.get(&id).unwrap().timeline(&key).unwrap().revision);
}

#[test]
fn clearing_viewport_marker_metadata_should_keep_the_current_row_plan_valid() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let key = TimelineKey::room("!one:x");
    s.focus_changed(false);
    s.read_marker_visible(&id, &key, Some("marker".into()));
    let revision = s.accounts.get(&id).unwrap().timeline(&key).unwrap().revision;
    assert!(!s.read_marker_visible(&id, &key, None));
    assert_eq!(revision, s.accounts.get(&id).unwrap().timeline(&key).unwrap().revision);
}

#[test]
fn membership_changes_in_a_cached_background_room_should_wait_until_it_is_opened() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    apply(
        &mut s,
        Event::Members {
            request: None,
            account: id.clone(),
            room_id: "!one:x".into(),
            members: matrix_core::Members { complete: true, ..Default::default() },
        },
    );
    s.select_room("!two:x");
    drain(&mut commands);
    apply(&mut s, Event::MembersInvalidated { account: id.clone(), room_id: "!one:x".into() });
    assert!(!s.accounts.members_fresh(&id, "!one:x"));
    assert!(all_commands(&mut commands).iter().all(|command| !matches!(command, Command::ListMembers { .. })));
    s.select_room("!one:x");
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|command| matches!(command, Command::ListMembers { room_id, .. } if room_id == "!one:x"))
    );
}

#[test]
fn returning_to_an_account_should_refresh_its_restored_rooms_stale_members() {
    let (mut s, mut commands) = state();
    let a = AccountId::new("@a:example.org");
    let b = AccountId::new("@b:example.org");
    with_rooms(&mut s, &a, &["!one:x"], Vec::new());
    with_rooms(&mut s, &b, &["!two:x"], Vec::new());
    s.select_account(&a);
    s.select_room("!one:x");
    apply(
        &mut s,
        Event::Members {
            request: None,
            account: a.clone(),
            room_id: "!one:x".into(),
            members: matrix_core::Members { complete: true, ..Default::default() },
        },
    );
    s.select_account(&b);
    s.select_room("!two:x");
    apply(&mut s, Event::MembersInvalidated { account: a.clone(), room_id: "!one:x".into() });
    drain(&mut commands);
    s.select_account(&a);
    assert_eq!(s.active_room_id(), Some("!one:x"));
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command, Command::ListMembers { account, room_id, all: true } if account == &a && room_id == "!one:x")));
}

#[test]
fn returning_to_an_account_should_retry_a_cancelled_initial_member_request() {
    let (mut s, mut commands) = state();
    let a = AccountId::new("@a:example.org");
    let b = AccountId::new("@b:example.org");
    with_rooms(&mut s, &a, &["!one:x"], Vec::new());
    with_rooms(&mut s, &b, &["!two:x"], Vec::new());
    s.select_account(&a);
    s.select_room("!one:x");
    s.select_account(&b);
    s.select_room("!two:x");
    apply(&mut s, Event::MembersInvalidated { account: a.clone(), room_id: "!one:x".into() });
    drain(&mut commands);
    s.select_account(&a);
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command, Command::ListMembers { account, room_id, all: false } if account == &a && room_id == "!one:x")));
}

#[test]
fn accepting_an_invitation_outside_the_current_space_should_open_it_from_home() {
    let (mut s, _) = state();
    crate::demo::invitation(&mut s);
    let id = AccountId::new(crate::demo::ADA);
    let scope = Scope::Space(crate::demo::WORK_SPACE.into());
    s.select_scope(scope.clone());
    let data = s.accounts.get(&id).unwrap();
    let (mut rooms, mut spaces, invitations) = (data.rooms.clone(), data.spaces.clone(), data.invitations.clone());
    for space in &mut spaces {
        space.children.retain(|child| child != "!design:example.org");
    }
    rooms.push(room("!design:example.org"));
    s.accounts.set_room_list(&id, RoomList { rooms, spaces, invitations });
    s.show_invitation("!design:example.org");
    s.membership_finished(&id, "!design:example.org", room_discovery::MembershipAction::JoinRoom, &scope);
    assert_eq!((s.active_scope(), s.active_room_id()), (Scope::Home, Some("!design:example.org")));
}

#[test]
fn opening_a_room_in_another_joined_space_should_select_a_sidebar_that_lists_it() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    let space = SpaceSummary {
        room_id: "!space:x".into(),
        name: "Space".into(),
        avatar: None,
        can_add_rooms: false,
        can_rename: false,
        children: vec!["!one:x".into()],
        members: Vec::new(),
    };
    with_rooms(&mut s, &id, &["!one:x"], vec![space]);
    s.open_room(&id, "!one:x");
    assert_eq!(s.active_scope(), Scope::Space("!space:x".into()));
    assert!(
        s.active_account().unwrap().rooms_in(&s.active_scope(), RoomKind::Group).any(|room| room.room_id == "!one:x")
    );
}

#[test]
fn warm_viewport_rows_should_wait_for_the_sdk_subscription_before_sending_receipts() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    let key = TimelineKey::room("!one:x");
    s.read_position_shown(&id, &key, "$visible".into());
    assert!(all_commands(&mut commands).is_empty());
    apply(&mut s, Event::TimelineChanged { account: id, key, diffs: Vec::new() });
    assert!(all_commands(&mut commands).iter().any(
        |command| matches!(command, Command::MarkRead { event_id: Some(event_id), .. } if event_id == "$visible")
    ));
}

#[test]
fn a_changed_sync_member_count_should_invalidate_a_complete_snapshot() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    apply(
        &mut s,
        Event::Members {
            request: None,
            account: id.clone(),
            room_id: "!one:x".into(),
            members: matrix_core::Members { complete: true, ..Default::default() },
        },
    );
    let mut joined = room("!one:x");
    joined.member_count = 3;
    apply(
        &mut s,
        Event::RoomsUpdated { account: id.clone(), list: RoomList { rooms: vec![joined], ..Default::default() } },
    );
    let data = s.accounts.get(&id).unwrap();
    assert_eq!(data.member_count(&data.rooms[0]), 3);
    assert!(
        all_commands(&mut commands).iter().any(|command| matches!(command, Command::ListMembers { all: true, .. }))
    );
}

#[test]
fn removing_the_open_room_should_close_its_thread_and_show_the_overview() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.open_thread("$root");
    apply(&mut s, Event::RoomsUpdated { account: id, list: RoomList::default() });
    assert!(s.active_room_id().is_none() && s.active_thread().is_none());
}

#[test]
fn public_rooms_should_be_ranked_by_member_count() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    let id = AccountId::new(crate::demo::ADA);
    s.select_scope(Scope::Space(crate::demo::WORK_SPACE.into()));
    let request = drain(&mut commands)
        .into_iter()
        .find(|(_, command)| matches!(command, Command::ListPublicRooms { .. }))
        .unwrap()
        .0;
    let rooms = [("!small:x", 3), ("!large:x", 80)]
        .into_iter()
        .map(|(id, count)| matrix_core::PublicRoom {
            room_id: id.into(),
            name: id.into(),
            topic: None,
            avatar: None,
            member_count: count,
            via: Vec::new(),
        })
        .collect();
    apply(&mut s, Event::PublicRooms { request, rooms });
    let Directory::Ready(rooms) = &s.directories[&(id, crate::demo::WORK_SPACE.into())] else { panic!("rooms ready") };
    assert_eq!(rooms.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(), ["!large:x", "!small:x"]);
}

#[test]
fn an_ended_session_reported_twice_should_sign_out_once() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    drain(&mut commands);
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: false });
    apply(&mut s, Event::SessionEnded { account: id, kept: false });
    let logouts = all_commands(&mut commands).into_iter().filter(|c| matches!(c, Command::Logout(_))).count();
    assert_eq!(logouts, 1);
}

#[test]
fn an_account_added_while_adding_one_should_become_active() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.start_adding_account();
    discover(&mut s, &mut commands, true, false);
    // The engine reports the account before answering the sign-in.
    apply(&mut s, Event::AccountAdded(account("@b:example.org")));
    assert_eq!(s.nav.account, Some(AccountId::new("@b:example.org")));
}

#[test]
fn an_older_layout_arriving_after_a_save_should_not_undo_it() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SidebarLayout { account: id.clone(), layout: matrix_core::SidebarLayout::default() });
    s.toggle_section("rooms");
    let older = s.accounts.get(&id).unwrap().sidebar.clone();
    let (request, _) =
        drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::SetSidebarLayout { .. })).unwrap();
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    s.toggle_section("direct");
    let (request, _) =
        drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::SetSidebarLayout { .. })).unwrap();
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    // Sync delivers the first save late.
    apply(&mut s, Event::SidebarLayout { account: id.clone(), layout: older });
    assert_eq!(s.accounts.get(&id).unwrap().sidebar.scopes["home"].collapsed.len(), 2);
}

#[test]
fn turning_linked_gifs_off_should_stop_the_ones_downloading() {
    let (mut s, _) = state();
    s.gifs.loading("https://x/a.gif");
    s.set_show_link_gifs(false);
    apply(&mut s, Event::GifLoaded { url: "https://x/a.gif".into(), bytes: Arc::from(&b"GIF89a"[..]) });
    assert!(s.gifs.player("https://x/a.gif").is_none());
}

fn typist(name: &str) -> matrix_core::Person {
    matrix_core::Person { id: format!("@{name}:x"), name: name.into(), avatar: None }
}

#[test]
fn people_typing_in_the_open_room_should_show() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    apply(&mut s, Event::Typing { account: id.clone(), room_id: "!one:x".into(), people: vec![typist("grace")] });
    assert_eq!(s.accounts.get(&id).unwrap().typing["!one:x"].len(), 1);
}

#[test]
fn typing_in_a_room_left_should_be_forgotten() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    apply(&mut s, Event::Typing { account: id.clone(), room_id: "!one:x".into(), people: vec![typist("grace")] });
    s.select_room("!two:x");
    assert!(s.accounts.get(&id).unwrap().typing.is_empty());
}

#[test]
fn typing_reported_for_a_closed_room_should_be_dropped() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    apply(&mut s, Event::Typing { account: id.clone(), room_id: "!two:x".into(), people: vec![typist("grace")] });
    assert!(s.accounts.get(&id).unwrap().typing.is_empty());
}

#[test]
fn writing_in_the_composer_should_tell_the_room() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.poll(&mut Tx::default());
    drain(&mut commands);
    s.composer.edit.insert("hello");
    s.poll(&mut Tx::default());
    let sent = all_commands(&mut commands);
    assert!(sent.iter().any(|c| matches!(c, Command::Typing { typing: true, room_id, .. } if room_id == "!one:x")));
}

fn gif_entry(id: &str) -> matrix_core::TimelineEntry {
    let message = Message {
        event_id: format!("${id}"),
        content: matrix_core::MessageContent::Text { body: format!("https://x.org/{id}.gif"), html: None },
        ..Message::default()
    };
    matrix_core::TimelineEntry { id: id.into(), kind: matrix_core::EntryKind::Message(Box::new(message)) }
}

fn gifs_fetched(commands: &mut DetachedCommands) -> usize {
    all_commands(commands).iter().filter(|c| matches!(c, Command::FetchGif { .. })).count()
}

#[test]
fn history_loaded_above_the_rows_on_screen_should_not_fetch_its_media() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    let key = TimelineKey::room("!one:x");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let reset = matrix_core::TimelineDiff::Reset(vec![gif_entry("new")]);
    apply(&mut s, Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs: vec![reset] });
    s.rows_shown(&id, &key, 0..1);
    drain(&mut commands);
    let older = matrix_core::TimelineDiff::PushFront(gif_entry("old"));
    apply(&mut s, Event::TimelineChanged { account: id, key, diffs: vec![older] });
    assert_eq!(gifs_fetched(&mut commands), 0);
}

#[test]
fn scrolling_to_a_row_should_fetch_its_media() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    let key = TimelineKey::room("!one:x");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let reset = matrix_core::TimelineDiff::Reset(vec![gif_entry("new")]);
    apply(&mut s, Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs: vec![reset] });
    s.rows_shown(&id, &key, 0..1);
    let older = matrix_core::TimelineDiff::PushFront(gif_entry("old"));
    apply(&mut s, Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs: vec![older] });
    drain(&mut commands);
    s.rows_shown(&id, &key, 0..1);
    assert_eq!(gifs_fetched(&mut commands), 1);
}

#[test]
fn a_failed_gif_on_screen_should_be_fetched_again_after_a_wait() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let reset = matrix_core::TimelineDiff::Reset(vec![gif_entry("a")]);
    apply(&mut s, Event::TimelineChanged { account: id, key: TimelineKey::room("!one:x"), diffs: vec![reset] });
    let (request, _) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::FetchGif { .. })).unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    let later = std::time::Instant::now() + std::time::Duration::from_secs(6);
    s.retry_due(later, &mut Tx::default());
    assert_eq!(gifs_fetched(&mut commands), 1);
}

fn link_entry(id: &str) -> matrix_core::TimelineEntry {
    let message = Message {
        event_id: format!("${id}"),
        content: matrix_core::MessageContent::Text { body: format!("read https://x.org/{id}"), html: None },
        ..Message::default()
    };
    matrix_core::TimelineEntry { id: id.into(), kind: matrix_core::EntryKind::Message(Box::new(message)) }
}

fn previews_asked(commands: &mut DetachedCommands) -> usize {
    all_commands(commands).iter().filter(|c| matches!(c, Command::PreviewLink { .. })).count()
}

fn room_with_link(s: &mut State) -> AccountId {
    let id = AccountId::new("@a:example.org");
    with_rooms(s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let reset = matrix_core::TimelineDiff::Reset(vec![link_entry("a")]);
    apply(s, Event::TimelineChanged { account: id.clone(), key: TimelineKey::room("!one:x"), diffs: vec![reset] });
    id
}

#[test]
fn a_link_on_screen_should_ask_the_homeserver_for_its_card() {
    let (mut s, mut commands) = state();
    room_with_link(&mut s);
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::PreviewLink { url, .. } if url == "https://x.org/a"))
    );
}

#[test]
fn with_link_previews_off_no_card_should_be_asked_for() {
    let (mut s, mut commands) = state();
    s.set_show_link_previews(false);
    room_with_link(&mut s);
    assert_eq!(previews_asked(&mut commands), 0);
}

#[test]
fn a_gif_link_should_get_a_card_while_linked_gifs_are_off() {
    let (mut s, mut commands) = state();
    s.set_show_link_gifs(false);
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let reset = matrix_core::TimelineDiff::Reset(vec![gif_entry("g")]);
    apply(&mut s, Event::TimelineChanged { account: id, key: TimelineKey::room("!one:x"), diffs: vec![reset] });
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::PreviewLink { url, .. } if url == "https://x.org/g.gif"))
    );
}

#[test]
fn a_link_card_should_be_asked_for_once() {
    let (mut s, mut commands) = state();
    let id = room_with_link(&mut s);
    drain(&mut commands);
    apply(&mut s, Event::LinkPreview { account: id.clone(), url: "https://x.org/a".into(), preview: None });
    s.rows_shown(&id, &TimelineKey::room("!one:x"), 0..1);
    assert_eq!(previews_asked(&mut commands), 0);
}

#[test]
fn a_link_card_that_failed_should_be_asked_for_again_after_a_wait() {
    let (mut s, mut commands) = state();
    room_with_link(&mut s);
    let (request, _) =
        drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::PreviewLink { .. })).unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    let later = std::time::Instant::now() + std::time::Duration::from_secs(6);
    s.retry_due(later, &mut Tx::default());
    assert_eq!(previews_asked(&mut commands), 1);
}

#[test]
fn a_quiet_chat_should_keep_its_place_from_the_stored_activity_after_a_restart() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    let old = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(1_000), ..room("!old:x") };
    let recent = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(9_000), ..room("!recent:x") };
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let list = RoomList { rooms: vec![old, recent], spaces: Vec::new(), invitations: Vec::new() };
    apply(&mut s, Event::RoomsUpdated { account: id.clone(), list });
    let data = s.accounts.get(&id).unwrap();
    let sections = super::sections(data, &matrix_core::SidebarLayout::default(), &Scope::Home);
    let direct = sections.iter().find(|section| section.id == DIRECT).unwrap();
    assert_eq!(direct.rooms.iter().map(|room| room.room_id.as_str()).collect::<Vec<_>>(), ["!recent:x", "!old:x"]);
}

#[test]
fn a_memo_whose_microphone_failed_should_end_at_once_and_say_so() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    let recorder = crate::platform::sound::Recorder::Native(crate::platform::sound::NativeRecorder::failing("gone"));
    s.composer.voice = Some(VoiceMemo { recorder, timeline: (id, TimelineKey::room("!one:x")) });
    assert!(s.animate(0.0, &mut Tx::default()));
    assert!(s.composer.voice.is_none() && s.notice.is_some());
}

#[test]
fn a_room_with_a_misdated_message_should_not_stay_at_the_top() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let future = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(now + 365 * 86_400_000), ..room("!future:x") };
    let recent = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(now - 60_000), ..room("!recent:x") };
    let list = RoomList { rooms: vec![future, recent], spaces: Vec::new(), invitations: Vec::new() };
    apply(&mut s, Event::RoomsUpdated { account: id.clone(), list });
    let data = s.accounts.get(&id).unwrap();
    let sections = super::sections(data, &matrix_core::SidebarLayout::default(), &Scope::Home);
    let direct = sections.iter().find(|section| section.id == DIRECT).unwrap();
    assert_eq!(direct.rooms[0].room_id, "!recent:x");
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn direct_order(s: &State, id: &AccountId) -> Vec<String> {
    let data = s.accounts.get(id).unwrap();
    let sections = super::sections(data, &matrix_core::SidebarLayout::default(), &Scope::Home);
    let direct = sections.iter().find(|section| section.id == DIRECT).unwrap();
    direct.rooms.iter().map(|room| room.room_id.clone()).collect()
}

fn latest_at(s: &mut State, id: &AccountId, room_id: &str, timestamp_ms: u64) {
    let message = Message { timestamp_ms, ..Message::default() };
    apply(s, Event::LatestMessage { account: id.clone(), room_id: room_id.into(), message: Box::new(message) });
}

#[test]
fn a_misdated_message_this_session_should_count_from_when_it_came() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let chats = ["!fast:x", "!older:x"].map(|r| RoomSummary { kind: RoomKind::Direct, ..room(r) });
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id.clone(),
            list: RoomList { rooms: chats.to_vec(), spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    latest_at(&mut s, &id, "!older:x", now_ms() - 60_000);
    // A server whose clock runs an hour fast: its message came just now, so it's first, but
    // only by when it came.
    latest_at(&mut s, &id, "!fast:x", now_ms() + 3_600_000);
    assert_eq!(direct_order(&s, &id), ["!fast:x", "!older:x"]);
    latest_at(&mut s, &id, "!older:x", now_ms() + 1_000);
    assert_eq!(direct_order(&s, &id), ["!older:x", "!fast:x"]);
}

#[test]
fn a_stored_time_ahead_of_the_clock_should_count_once_it_has_passed() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let later = now_ms() + 3_600_000;
    let chat = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(later), ..room("!later:x") };
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id.clone(),
            list: RoomList { rooms: vec![chat.clone()], spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    let data = s.accounts.get(&id).unwrap();
    assert_eq!(data.activity(&chat, now_ms()), 0);
    assert_eq!(data.activity(&chat, later + 1), later);
}

#[test]
fn a_time_within_clock_drift_should_count_as_it_is() {
    let (mut s, _commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let ahead = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(now_ms() + 60_000), ..room("!ahead:x") };
    let now = RoomSummary { kind: RoomKind::Direct, activity_ms: Some(now_ms()), ..room("!now:x") };
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id.clone(),
            list: RoomList { rooms: vec![now, ahead], spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    assert_eq!(direct_order(&s, &id), ["!ahead:x", "!now:x"]);
}

#[test]
fn the_sign_in_backdrop_should_move_while_it_shows() {
    let (mut s, _) = state();
    s.backdrop.resized([800.0, 600.0]);
    s.backdrop.measured(8.0);
    s.animate(0.0, &mut Tx::default());
    assert!(s.animate(1.0, &mut Tx::default()));
}

#[test]
fn the_backdrop_should_rest_once_the_chat_shows() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    assert!(!s.animate(1.0, &mut Tx::default()));
}

#[test]
fn the_sign_in_modal_should_animate_its_backdrop_over_chat() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.start_adding_account();
    s.backdrop.resized([1080.0, 684.0]);
    s.backdrop.measured(8.0);
    s.animate(0.0, &mut Tx::default());

    assert!(s.animate(1.0, &mut Tx::default()));
}

#[test]
fn closing_the_sign_in_modal_should_stop_its_backdrop() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.start_adding_account();
    s.backdrop.resized([1080.0, 684.0]);
    s.backdrop.measured(8.0);
    s.animate(0.0, &mut Tx::default());
    s.animate(1.0, &mut Tx::default());
    s.close_modal();

    assert!(!s.animate(2.0, &mut Tx::default()));
}

#[test]
fn the_backdrop_should_rest_while_the_window_is_in_the_background() {
    let (mut s, _) = state();
    s.backdrop.resized([800.0, 600.0]);
    s.backdrop.measured(8.0);
    s.animate(0.0, &mut Tx::default());
    s.focus_changed(false);
    assert!(!s.animate(1.0, &mut Tx::default()));
}

#[test]
fn the_selected_tile_should_stop_breathing_in_an_idle_window() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.animate(0.0, &mut Tx::default());
    assert!(s.animate(60.0, &mut Tx::default()), "coming to rest changes the view");
    assert!(!s.breath.breathing());
}

#[test]
fn choosing_a_scope_should_make_the_selected_tile_breathe_again() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.animate(0.0, &mut Tx::default());
    s.animate(60.0, &mut Tx::default());
    s.select_scope(Scope::Home);
    s.animate(61.0, &mut Tx::default());
    assert!(s.breath.breathing());
}

#[test]
fn an_account_suspended_and_put_off_should_stay_listed() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    s.close_modal();
    apply(&mut s, Event::AccountRemoved(id.clone()));
    assert_eq!(s.suspended, [id]);
}

#[test]
fn signing_in_as_a_listed_suspended_account_should_take_its_data_back() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    s.close_modal();
    drain(&mut commands);
    s.sign_in_as(&id);
    let sent = discovered(&mut s, &mut commands);
    assert!(matches!(&sent[..], [Command::LoginOAuth { resume: Some(resumed), .. }] if *resumed == id));
}

#[test]
fn typing_the_user_id_of_a_suspended_account_should_take_its_data_back() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    s.close_modal();
    drain(&mut commands);
    s.sign_in.homeserver = TextEdit::new("@a:example.org");
    s.submit_server();
    let sent = discovered(&mut s, &mut commands);
    assert!(matches!(&sent[..], [Command::LoginOAuth { resume: Some(resumed), .. }] if *resumed == id));
}

#[test]
fn every_account_suspended_at_launch_should_be_listed() {
    let (mut s, _) = state();
    apply(&mut s, Event::SessionEnded { account: AccountId::new("@a:x"), kept: true });
    apply(&mut s, Event::SessionEnded { account: AccountId::new("@b:x"), kept: true });
    assert_eq!(s.suspended.len(), 2);
}

#[test]
fn signing_back_in_should_take_an_account_off_the_suspended_list() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    assert!(s.suspended.is_empty());
}

#[test]
fn changing_the_server_should_not_carry_a_suspended_account_over() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::SessionEnded { account: id.clone(), kept: true });
    s.sign_in_as(&id);
    s.change_server();
    assert_eq!(s.sign_in.resume, None);
}

#[test]
fn an_account_suspended_in_an_earlier_run_should_be_listed_without_a_dialog() {
    let (mut s, _) = state();
    apply(&mut s, Event::SessionEnded { account: AccountId::new("@a:x"), kept: true });
    assert!(s.modal.is_none() && s.suspended.len() == 1);
}

// ----- calls ---------------------------------------------------------------------------------

fn call_info(account: &AccountId, room_id: &str, camera: Option<u64>) -> matrix_core::CallInfo {
    matrix_core::CallInfo {
        call: 1,
        account: account.clone(),
        room_id: room_id.into(),
        via: matrix_core::CallVia::Rtc,
        phase: matrix_core::CallPhase::Connected,
        video: camera.is_some(),
        microphone: true,
        camera: camera.map(matrix_core::VideoId),
        screen: None,
        participants: Vec::new(),
    }
}

fn ring(account: &AccountId, room_id: &str) -> matrix_core::IncomingCall {
    matrix_core::IncomingCall {
        account: account.clone(),
        room_id: room_id.into(),
        caller: matrix_core::Person { id: "@b:example.org".into(), name: "B".into(), avatar: None },
        video: false,
        via: matrix_core::CallVia::Rtc,
    }
}

#[test]
fn starting_a_call_should_ask_for_one_in_the_room_shown() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    drain(&mut commands);
    s.start_call(true);
    assert!(
        matches!(commands.try_next(), Some((_, Command::StartCall { room_id, video: true, .. })) if room_id == "!one:x")
    );
}

#[test]
fn a_call_with_a_new_video_should_watch_it() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    drain(&mut commands);
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", Some(7)))));
    let watched = all_commands(&mut commands)
        .into_iter()
        .any(|c| matches!(c, Command::WatchVideo { video, .. } if video == matrix_core::VideoId(7)));
    assert!(watched);
}

#[test]
fn a_video_that_leaves_the_call_should_be_unwatched() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", Some(7)))));
    drain(&mut commands);
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", None))));
    assert!(all_commands(&mut commands).contains(&Command::UnwatchVideo(matrix_core::VideoId(7))));
}

#[test]
fn a_call_in_another_room_should_not_stream_its_video() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", Some(7)))));
    drain(&mut commands);
    s.select_room("!two:x");
    s.sync_watching();
    assert!(all_commands(&mut commands).contains(&Command::UnwatchVideo(matrix_core::VideoId(7))));
}

#[test]
fn paging_a_call_should_stop_hidden_videos_and_start_visible_ones() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    let mut info = call_info(&id, "!one:x", None);
    info.participants = (1..=10)
        .map(|n| matrix_core::CallParticipant {
            key: n.to_string(),
            person: matrix_core::Person { id: n.to_string(), name: n.to_string(), avatar: None },
            muted: false,
            camera: Some(matrix_core::VideoId(n)),
            screen: None,
        })
        .collect();
    apply(&mut s, Event::CallUpdated(Box::new(info)));
    s.set_call_layout(CallLayout::Grid);
    s.resize_call([300.0, 180.0]);
    let old = s.call.as_ref().unwrap().watched.clone();
    drain(&mut commands);
    s.page_call(true);
    let new = &s.call.as_ref().unwrap().watched;
    let dispatched = all_commands(&mut commands);
    assert!(
        !old.is_empty()
            && old.is_disjoint(new)
            && old.iter().all(|id| dispatched.contains(&Command::UnwatchVideo(*id)))
            && new
                .iter()
                .all(|id| dispatched.iter().any(|c| matches!(c, Command::WatchVideo { video, .. } if video == id)))
    );
}

#[test]
fn showing_chat_should_keep_visible_call_videos_watched() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", Some(7)))));
    drain(&mut commands);
    s.toggle_call_chat();
    s.sync_watching();
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::UnwatchVideo(_))));
}

#[test]
fn compact_chat_should_close_the_call_layout_menu() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", Some(7)))));
    s.call.as_mut().unwrap().room_size = [400.0, 600.0];
    s.open_call_layout();
    s.toggle_call_chat();
    assert!(s.popover.is_none());
}

#[test]
fn the_end_of_a_call_should_close_its_layout_menu() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", Some(7)))));
    s.open_call_layout();
    apply(
        &mut s,
        Event::CallEnded { account: id, room_id: "!one:x".into(), call: 1, reason: matrix_core::CallEnd::HungUp },
    );
    assert!(s.popover.is_none());
}

#[test]
fn compact_chat_paging_should_watch_only_the_selected_video() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    let mut info = call_info(&id, "!one:x", None);
    info.participants = (1..=2)
        .map(|n| matrix_core::CallParticipant {
            key: n.to_string(),
            person: matrix_core::Person { id: n.to_string(), name: n.to_string(), avatar: None },
            muted: false,
            camera: Some(matrix_core::VideoId(n)),
            screen: None,
        })
        .collect();
    apply(&mut s, Event::CallUpdated(Box::new(info)));
    let call = s.call.as_mut().unwrap();
    call.chat = true;
    call.room_size = [400.0, 600.0];
    s.set_call_layout(CallLayout::Grid);
    drain(&mut commands);
    s.page_call(true);
    let dispatched = all_commands(&mut commands);
    assert!(
        dispatched.contains(&Command::UnwatchVideo(matrix_core::VideoId(1)))
            && dispatched
                .iter()
                .any(|c| matches!(c, Command::WatchVideo { video, .. } if *video == matrix_core::VideoId(2)))
            && s.call.as_ref().unwrap().watched == [matrix_core::VideoId(2)].into_iter().collect()
    );
}

#[test]
fn the_end_of_a_replaced_call_should_leave_the_new_one() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::CallUpdated(Box::new(matrix_core::CallInfo { call: 2, ..call_info(&id, "!one:x", None) })));
    apply(
        &mut s,
        Event::CallEnded { account: id, room_id: "!one:x".into(), call: 1, reason: matrix_core::CallEnd::HungUp },
    );
    assert!(s.call.is_some());
}

#[test]
fn a_call_ending_should_close_the_stage() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", None))));
    apply(
        &mut s,
        Event::CallEnded { account: id, room_id: "!one:x".into(), call: 1, reason: matrix_core::CallEnd::Left },
    );
    assert!(s.call.is_none() && s.notice.is_none());
}

#[test]
fn a_declined_call_should_say_so() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(
        &mut s,
        Event::CallEnded { account: id, room_id: "!one:x".into(), call: 1, reason: matrix_core::CallEnd::Declined },
    );
    assert!(s.notice.is_some());
}

#[test]
fn answering_a_ring_should_open_its_room_and_answer() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::IncomingCall(Box::new(ring(&id, "!one:x"))));
    drain(&mut commands);
    s.answer_call(&id, "!one:x", false);
    assert!(s.rings.is_empty() && s.active_room_id() == Some("!one:x"));
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::AnswerCall { video: false, .. })));
}

#[test]
fn a_ring_that_ended_should_leave_the_corner() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::IncomingCall(Box::new(ring(&id, "!one:x"))));
    apply(&mut s, Event::RingEnded { account: id, room_id: "!one:x".into() });
    assert!(s.rings.is_empty());
}

#[test]
fn muting_should_turn_the_microphone_off() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    apply(&mut s, Event::CallUpdated(Box::new(call_info(&id, "!one:x", None))));
    drain(&mut commands);
    s.toggle_microphone();
    assert!(all_commands(&mut commands).contains(&Command::SetCallMedia(matrix_core::MediaToggle::Microphone(false))));
}

#[test]
fn picking_a_device_should_remember_it_and_tell_calls() {
    let (mut s, mut commands) = state();
    drain(&mut commands);
    s.choose_device(matrix_core::DeviceKind::Speaker, Some("USB Headset".into()));
    assert_eq!(s.settings.devices.speaker.as_deref(), Some("USB Headset"));
    let told = all_commands(&mut commands).into_iter().any(|c| {
        matches!(c, Command::ChooseCallDevice { kind: matrix_core::DeviceKind::Speaker, id: Some(name) } if name == "USB Headset")
    });
    assert!(told);
}

#[test]
fn devices_picked_on_an_earlier_run_should_reach_calls_at_launch() {
    let (engine, mut commands) = EngineHandle::detached();
    let mut settings = Settings::fixed_english();
    settings.devices.camera = Some("Integrated Camera".into());
    let _s = State::new(engine, Shell::headless(), settings);
    assert!(matches!(
        commands.try_next(),
        Some((_, Command::ChooseCallDevice { kind: matrix_core::DeviceKind::Camera, .. }))
    ));
}

#[test]
fn a_silenced_ring_should_still_show_the_ringing_call() {
    let (mut s, _commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.set_ring_sound(false);
    apply(&mut s, Event::IncomingCall(Box::new(ring(&id, "!one:x"))));
    assert!(!s.settings.ring_sound && s.rings.len() == 1);
}

// ----- side panels, members, polls ----------------------------------------------------------

#[test]
fn opening_the_threads_panel_should_list_the_rooms_threads() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    drain(&mut commands);
    s.show_side(SidePanel::Threads);
    assert_eq!(s.side, Some(SidePanel::Threads));
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::ListThreads { room_id, .. } if room_id == "!one:x"))
    );
}

#[test]
fn switching_rooms_should_close_the_files_panel() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    s.show_side(SidePanel::Files);
    drain(&mut commands);
    s.select_room("!two:x");
    assert_eq!(s.side, None);
    assert!(
        !all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::ListFiles { room_id, .. } if room_id == "!two:x"))
    );
}

#[test]
fn a_list_that_failed_to_load_should_stop_loading_and_say_so() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    drain(&mut commands);
    s.show_side(SidePanel::Threads);
    let request = drain(&mut commands)
        .into_iter()
        .find_map(|(request, c)| matches!(c, Command::ListThreads { .. }).then_some(request))
        .unwrap();
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    let threads = s.accounts.get(&id).and_then(|account| account.threads.get("!one:x"));
    assert!(threads.is_some_and(Vec::is_empty) && s.notice.is_some());
}

#[test]
fn opening_a_listed_thread_should_close_the_side_panel() {
    let (mut s, _commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    s.show_side(SidePanel::Threads);
    s.open_listed_thread("$root");
    assert!(s.side.is_none() && s.active_thread() == Some("$root"));
}

#[test]
fn ending_a_poll_should_ask_first() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    drain(&mut commands);
    s.end_poll(TimelineKey::room("!one:x"), "$poll", "Lunch?");
    assert!(matches!(s.modal, Some(Modal::Confirm(Confirm::EndPoll { .. }))));
    assert!(all_commands(&mut commands).is_empty());
    s.confirm();
    assert!(
        all_commands(&mut commands).iter().any(|c| matches!(c, Command::EndPoll { poll_id, .. } if poll_id == "$poll"))
    );
}

#[test]
fn a_camera_that_stopped_on_its_own_should_say_so() {
    let (mut s, _commands) = state();
    apply(&mut s, Event::CallMediaStopped(matrix_core::MediaToggle::Camera(false)));
    assert!(s.notice.is_some());
}

#[test]
fn raising_someone_to_ones_own_level_should_ask_first() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    let members = matrix_core::Members { own_level: 50, can_set_roles: true, ..matrix_core::Members::default() };
    apply(&mut s, Event::Members { request: None, account: id, room_id: "!one:x".into(), members });
    drain(&mut commands);
    s.moderate("@b:x", matrix_core::Moderation::Role(matrix_core::MemberRole::Moderator));
    assert!(matches!(s.modal, Some(Modal::Confirm(Confirm::Moderate { .. }))));
    assert!(all_commands(&mut commands).is_empty());
}

#[test]
fn calling_someone_should_use_the_chat_with_them_alone_not_a_group_chat() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let group =
        RoomSummary { kind: RoomKind::Direct, dm_users: vec!["@b:x".into(), "@c:x".into()], ..room("!group:x") };
    let alone = RoomSummary { kind: RoomKind::Direct, dm_users: vec!["@b:x".into()], ..room("!bob:x") };
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id,
            list: RoomList { rooms: vec![group, alone], spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    drain(&mut commands);
    s.call_person("@b:x", false);
    assert_eq!(s.active_room_id(), Some("!bob:x"));
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::StartCall { room_id, .. } if room_id == "!bob:x"))
    );
}

#[test]
fn calling_someone_should_skip_a_chat_a_third_person_joined() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    apply(&mut s, Event::AccountAdded(account(id.as_str())));
    let crowded = RoomSummary {
        kind: RoomKind::Direct,
        dm_users: vec!["@b:x".into()],
        active_member_count: 3,
        ..room("!bob-and-bot:x")
    };
    apply(
        &mut s,
        Event::RoomsUpdated {
            account: id,
            list: RoomList { rooms: vec![crowded], spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    drain(&mut commands);
    s.call_person("@b:x", false);
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::OpenDirectChat { user_id, .. } if user_id == "@b:x"))
    );
}

#[test]
fn kicking_a_member_should_ask_first() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    drain(&mut commands);
    s.moderate("@b:x", matrix_core::Moderation::Kick);
    assert!(matches!(s.modal, Some(Modal::Confirm(Confirm::Moderate { action: matrix_core::Moderation::Kick, .. }))));
    s.confirm();
    let kicked = all_commands(&mut commands)
        .into_iter()
        .any(|c| matches!(c, Command::ModerateMember { action: matrix_core::Moderation::Kick, .. }));
    assert!(kicked);
}

#[test]
fn calling_someone_without_a_direct_chat_should_start_one_first() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    drain(&mut commands);
    s.call_person("@b:x", true);
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::OpenDirectChat { user_id, .. } if user_id == "@b:x"))
    );
}

#[test]
fn a_written_poll_should_be_sent_to_its_composers_room() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    s.select_room("!one:x");
    s.open_poll_form(ComposerTarget::Room);
    s.poll_form.question.insert("Lunch?");
    s.poll_form.answers[0].insert("Pizza");
    s.poll_form.answers[1].insert("Sushi");
    drain(&mut commands);
    s.send_poll(&mut Tx::default());
    assert!(s.modal.is_none());
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::Send { message: matrix_core::Outgoing::Poll { .. }, .. }))
    );
}

#[test]
fn a_poll_should_go_to_the_room_it_was_written_for_after_switching_rooms() {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    s.open_poll_form(ComposerTarget::Room);
    s.poll_form.question.insert("Lunch?");
    s.poll_form.answers[0].insert("Pizza");
    s.poll_form.answers[1].insert("Sushi");
    s.select_room("!two:x");
    drain(&mut commands);
    s.send_poll(&mut Tx::default());
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::Send { key, message: matrix_core::Outgoing::Poll { .. }, .. } if key.room_id == "!one:x"))
    );
}

#[test]
fn opening_the_emoji_picker_should_clear_its_search() {
    let (mut s, _commands) = state();
    s.emoji_search.insert("old");
    s.toggle_popover(Popover::Emoji(EmojiFor::Composer(ComposerTarget::Room)));
    assert!(s.emoji_search.text.is_empty() && s.emoji_search.focused);
}

// ----- editing and deleting ---------------------------------------------------------------------

/// A room with the user's message `$mine` ("helo") and someone else's after it.
fn with_own_message(s: &mut State) -> AccountId {
    let id = AccountId::new("@a:example.org");
    with_rooms(s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    let message = |event_id: &str, sender_id: &str, body: &str| matrix_core::TimelineEntry {
        id: event_id.into(),
        kind: matrix_core::EntryKind::Message(Box::new(Message {
            event_id: event_id.into(),
            sender_id: sender_id.into(),
            content: MessageContent::Text { body: body.into(), html: None },
            editable: sender_id == "@a:example.org",
            ..Message::default()
        })),
    };
    let entries = vec![message("$mine", "@a:example.org", "helo"), message("$theirs", "@b:example.org", "hi")];
    let diffs = vec![matrix_core::TimelineDiff::Reset(entries)];
    apply(s, Event::TimelineChanged { account: id.clone(), key: TimelineKey::room("!one:x"), diffs });
    id
}

#[test]
fn up_in_an_empty_composer_should_edit_the_users_last_message() {
    let (mut s, _) = state();
    with_own_message(&mut s);
    assert!(s.edit_last(ComposerTarget::Room, &mut Tx::default()));
    let editing = s.composer.editing.as_ref().map(|editing| editing.target.event_id.as_str());
    assert_eq!((editing, s.composer.edit.text.as_str()), (Some("$mine"), "helo"));
}

#[test]
fn sending_an_edit_should_change_the_message_instead_of_sending_a_new_one() {
    let (mut s, mut commands) = state();
    with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    drain(&mut commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    let sent = all_commands(&mut commands);
    assert!(
        sent.iter()
            .any(|c| matches!(c, Command::Edit { event_id, body, .. } if event_id == "$mine" && body == "helo!"))
    );
    assert!(!sent.iter().any(|c| matches!(c, Command::Send { .. })));
}

#[test]
fn deleting_a_message_should_ask_first_then_delete_it() {
    let (mut s, mut commands) = state();
    with_own_message(&mut s);
    drain(&mut commands);
    s.delete_message(TimelineKey::room("!one:x"), "$mine");
    assert!(all_commands(&mut commands).is_empty());
    s.confirm();
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::Redact { event_id, .. } if event_id == "$mine"))
    );
}

#[test]
fn an_edit_left_by_navigating_should_come_back_with_its_room() {
    let (mut s, _) = state();
    let id = with_own_message(&mut s);
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.composer.edit.insert("draft");
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    s.select_room("!two:x");
    assert!(s.composer.editing.is_none());
    // The room's draft stays with the room.
    assert_eq!(s.composer.edit.text, "");
    s.select_room("!one:x");
    assert_eq!((s.composer.editing.is_some(), s.composer.edit.text.as_str()), (true, "helo!"));
    s.cancel_edit(ComposerTarget::Room);
    assert_eq!(s.composer.edit.text, "draft");
}

#[test]
fn replying_should_end_an_edit() {
    let (mut s, _) = state();
    with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    let theirs = Message { event_id: "$theirs".into(), sender_name: "b".into(), ..Message::default() };
    s.start_reply(ComposerTarget::Room, &theirs, &mut Tx::default());
    assert!(s.composer.editing.is_none());
    assert_eq!(s.composer.reply_to.as_ref().map(|reply| reply.event_id.as_str()), Some("$theirs"));
}

#[test]
fn a_failed_edit_should_come_back_into_the_composer() {
    let (mut s, mut commands) = state();
    with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    drain(&mut commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    let Some((request, _)) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::Edit { .. })) else {
        panic!("expected an edit");
    };
    apply(&mut s, Event::Finished { request, result: Err("gone".into()) });
    assert_eq!((s.composer.editing.is_some(), s.composer.edit.text.as_str()), (true, "helo!"));
    assert!(s.notice.is_some());
}

#[test]
fn an_edit_should_take_no_attachments() {
    let (mut s, _) = state();
    with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.attach_files_to(ComposerTarget::Room, &[PathBuf::from("/a.png")]);
    assert!(s.composer.attachments.is_empty());
}

#[test]
fn an_address_pasted_in_a_code_block_should_paste_as_text() {
    let (mut s, _) = state();
    s.composer.format(crate::state::Format::CodeBlock);
    let content = rune::clipboard::ClipboardContent::new("https://matrix.org");
    assert!(!s.paste_into_composer(ComposerTarget::Room, &content));
}

#[test]
fn a_failed_edit_should_come_back_when_its_room_shows_again() {
    let (mut s, mut commands) = state();
    let id = with_own_message(&mut s);
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    drain(&mut commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    let Some((request, _)) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::Edit { .. })) else {
        panic!("expected an edit");
    };
    s.select_room("!two:x");
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    assert!(s.composer.editing.is_none());
    s.select_room("!one:x");
    assert_eq!((s.composer.editing.is_some(), s.composer.edit.text.as_str()), (true, "helo!"));
}

#[test]
fn editing_should_wait_for_a_voice_memo_being_recorded() {
    let (mut s, _) = state();
    with_own_message(&mut s);
    s.start_voice_memo(ComposerTarget::Room);
    assert!(s.composer.voice.is_some());
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    assert!(s.composer.editing.is_none());
}

#[test]
fn a_failed_edit_should_come_back_once_the_composer_is_free() {
    let (mut s, mut commands) = state();
    with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    drain(&mut commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    let Some((request, _)) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::Edit { .. })) else {
        panic!("expected an edit");
    };
    s.start_voice_memo(ComposerTarget::Room);
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    assert!(s.composer.editing.is_none());
    s.cancel_voice_memo(ComposerTarget::Room);
    assert_eq!((s.composer.editing.is_some(), s.composer.edit.text.as_str()), (true, "helo!"));
}

#[test]
fn removing_an_account_should_drop_its_kept_edits() {
    let (mut s, _) = state();
    let id = with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    apply(&mut s, Event::AccountRemoved(id));
    with_own_message(&mut s);
    assert!(s.composer.editing.is_none());
}

#[test]
fn switching_threads_should_keep_the_threads_edit() {
    let (mut s, _) = state();
    let id = with_own_message(&mut s);
    s.open_thread("$mine");
    let reply = matrix_core::TimelineEntry {
        id: "$reply".into(),
        kind: matrix_core::EntryKind::Message(Box::new(Message {
            event_id: "$reply".into(),
            sender_id: "@a:example.org".into(),
            content: MessageContent::Text { body: "in thread".into(), html: None },
            editable: true,
            ..Message::default()
        })),
    };
    let key = TimelineKey::thread("!one:x", "$mine");
    let diffs = vec![matrix_core::TimelineDiff::Reset(vec![reply])];
    apply(&mut s, Event::TimelineChanged { account: id, key, diffs });
    assert!(s.edit_last(ComposerTarget::Thread, &mut Tx::default()));
    s.thread_composer.edit.insert("!");
    s.open_thread("$theirs");
    assert!(s.thread_composer.editing.is_none());
    s.open_thread("$mine");
    assert_eq!((s.thread_composer.editing.is_some(), s.thread_composer.edit.text.as_str()), (true, "in thread!"));
}

/// The user's own text message `event_id`, as the timeline holds it.
fn own_entry(event_id: &str, body: &str) -> matrix_core::TimelineEntry {
    let message = Message { event_id: event_id.into(), ..own_message(body) };
    matrix_core::TimelineEntry { id: event_id.into(), kind: matrix_core::EntryKind::Message(Box::new(message)) }
}

/// Sends an edit of the user's last message, `!` added, and gives its request.
fn send_edit(s: &mut State, commands: &mut DetachedCommands) -> RequestId {
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    s.composer.edit.insert("!");
    drain(commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    drain(commands).into_iter().find(|(_, c)| matches!(c, Command::Edit { .. })).map(|(request, _)| request).unwrap()
}

#[test]
fn a_refused_edit_of_a_removed_account_should_not_come_back() {
    let (mut s, mut commands) = state();
    let id = with_own_message(&mut s);
    let request = send_edit(&mut s, &mut commands);
    apply(&mut s, Event::AccountRemoved(id));
    with_own_message(&mut s);
    apply(&mut s, Event::Finished { request, result: Err("gone".into()) });
    assert!(s.composer.editing.is_none());
}

#[test]
fn editing_a_message_with_a_kept_edit_should_bring_it_back() {
    let (mut s, mut commands) = state();
    let id = with_own_message(&mut s);
    let diffs = vec![matrix_core::TimelineDiff::PushBack(own_entry("$later", "later"))];
    apply(&mut s, Event::TimelineChanged { account: id, key: TimelineKey::room("!one:x"), diffs });
    let mine = Message { event_id: "$mine".into(), ..own_message("helo") };
    let later = Message { event_id: "$later".into(), ..own_message("later") };
    s.start_edit(ComposerTarget::Room, &mine, &mut Tx::default());
    s.composer.edit.insert("!");
    drain(&mut commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    let request =
        drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::Edit { .. })).map(|(r, _)| r).unwrap();
    s.start_edit(ComposerTarget::Room, &later, &mut Tx::default());
    apply(&mut s, Event::Finished { request, result: Err("offline".into()) });
    s.start_edit(ComposerTarget::Room, &mine, &mut Tx::default());
    assert_eq!(s.composer.edit.text, "helo!");
    // The edit of the later message changed nothing, so it isn't kept.
    assert!(s.set_aside.is_empty());
}

#[test]
fn editing_a_message_from_another_timeline_should_drop_its_kept_edit() {
    let (mut s, _) = state();
    let id = with_own_message(&mut s);
    s.open_thread("$mine");
    let diffs = vec![matrix_core::TimelineDiff::Reset(vec![own_entry("$mine", "helo")])];
    apply(&mut s, Event::TimelineChanged { account: id, key: TimelineKey::thread("!one:x", "$mine"), diffs });
    s.edit_last(ComposerTarget::Thread, &mut Tx::default());
    s.thread_composer.edit.insert("!");
    s.open_thread("$theirs");
    s.start_edit(ComposerTarget::Room, &own_message("helo"), &mut Tx::default());
    assert_eq!(s.composer.edit.text, "helo");
    assert!(s.set_aside.is_empty());
}

fn own_message(body: &str) -> Message {
    Message {
        event_id: "$mine".into(),
        sender_id: "@a:example.org".into(),
        content: MessageContent::Text { body: body.into(), html: None },
        editable: true,
        ..Message::default()
    }
}

/// The room composer editing `$mine` ("helo?"), and the thread of `$mine` editing it too
/// ("helo!") until the user switches to another thread, which keeps that edit.
fn edits_in_room_and_thread(s: &mut State) {
    let id = with_own_message(s);
    with_rooms(s, &id, &["!one:x", "!two:x"], Vec::new());
    s.start_edit(ComposerTarget::Room, &own_message("helo"), &mut Tx::default());
    s.composer.edit.insert("?");
    s.open_thread("$mine");
    let diffs = vec![matrix_core::TimelineDiff::Reset(vec![own_entry("$mine", "helo")])];
    apply(s, Event::TimelineChanged { account: id, key: TimelineKey::thread("!one:x", "$mine"), diffs });
    s.edit_last(ComposerTarget::Thread, &mut Tx::default());
    s.thread_composer.edit.insert("!");
    s.open_thread("$theirs");
}

#[test]
fn sending_an_edit_should_drop_an_edit_of_the_message_kept_from_its_thread() {
    let (mut s, _) = state();
    edits_in_room_and_thread(&mut s);
    assert_eq!(s.set_aside.len(), 1);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    assert!(s.set_aside.is_empty());
}

#[test]
fn keeping_an_edit_should_replace_one_of_the_message_kept_from_its_thread() {
    let (mut s, _) = state();
    edits_in_room_and_thread(&mut s);
    s.select_room("!two:x");
    let kept: Vec<&str> = s.set_aside.iter().map(|edit| edit.body.as_str()).collect();
    assert_eq!(kept, ["helo?"]);
}

#[test]
fn typing_a_search_should_also_search_only_the_active_accounts_messages() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@a:example.org");
    with_rooms(&mut s, &account, &["!one:x"], Vec::new());
    drain(&mut commands);
    s.search.field.insert("device");
    rest_search(&mut s);
    assert!(all_commands(&mut commands).iter().any(
        |c| matches!(c, Command::SearchMessages { account: owner, term } if owner == &account && term == "device")
    ));
}

#[test]
fn an_obsolete_message_search_should_not_fill_a_new_query() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.search.field.insert("device");
    rest_search(&mut s);
    let request = s.search.message_request().unwrap();
    s.search.field = TextEdit::new("other");
    s.search.messages_found(request, vec![message_hit()]);
    assert!(s.search.messages.is_empty());
}

#[test]
fn a_failed_message_search_should_finish_its_loading_state() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.search.field.insert("device");
    rest_search(&mut s);
    let request = s.search.message_request().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("store unavailable".into()) });
    assert!(s.search.message_status == SearchStatus::Failed);
}

fn message_hit() -> matrix_core::MessageHit {
    matrix_core::MessageHit {
        room_id: "!one:x".into(),
        event_id: "$hit".into(),
        thread: None,
        sender: "@a:example.org".into(),
        timestamp_ms: 10,
        body: "hello device".into(),
        highlights: vec![6, 7, 8, 9, 10, 11],
    }
}

#[test]
fn picking_a_thread_message_should_open_its_thread() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    let hit = matrix_core::MessageHit { thread: Some("$root".into()), ..message_hit() };
    s.open_found_message(&hit);
    assert_eq!(s.message_jump.as_ref().map(|jump| &jump.key), Some(&TimelineKey::thread("!one:x", "$root")));
}

#[test]
fn search_usage_should_stay_with_its_own_account() {
    let (mut s, _) = state();
    let a = AccountId::new("@a:example.org");
    let b = AccountId::new("@b:example.org");
    with_rooms(&mut s, &a, &["!one:x"], Vec::new());
    with_rooms(&mut s, &b, &["!one:x"], Vec::new());
    let usage = matrix_core::SearchUsage {
        rooms: std::collections::BTreeMap::from([("!one:x".into(), 8)]),
        ..matrix_core::SearchUsage::default()
    };
    apply(&mut s, Event::SearchUsage { account: a.clone(), usage });
    assert_eq!(
        (
            s.accounts.get(&a).unwrap().search_usage.room("!one:x"),
            s.accounts.get(&b).unwrap().search_usage.room("!one:x")
        ),
        (8, 0)
    );
}

#[test]
fn leaving_a_search_should_cancel_its_message_request() {
    let leave: &[fn(&mut State)] = &[
        |s| s.open_found_room("!one:x"),
        |s| s.open_found_space("!space:x"),
        |s| s.open_direct_chat("@grace:example.org"),
        |s| s.open_typed_link("#rust:matrix.org"),
        |s| s.select_account(&AccountId::new("@b:example.org")),
        |s| apply(s, Event::AccountRemoved(AccountId::new("@a:example.org"))),
    ];
    for leave in leave {
        let (mut s, mut commands) = state();
        with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
        with_rooms(&mut s, &AccountId::new("@b:example.org"), &[], Vec::new());
        s.search.field.insert("device");
        rest_search(&mut s);
        let request = s.search.message_request().unwrap();
        drain(&mut commands);
        leave(&mut s);
        assert!(
            all_commands(&mut commands)
                .iter()
                .any(|c| matches!(c, Command::Cancel(cancelled) if *cancelled == request))
        );
    }
}

#[test]
fn a_room_being_shown_should_record_one_visit_before_its_timeline_opens() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.select_room("!one:x");
    s.select_room("!one:x");
    assert_eq!(all_commands(&mut commands).iter().filter(|c| matches!(c, Command::VisitRoom { .. })).count(), 1);
}

fn looking_for_message() -> (State, DetachedCommands) {
    let (mut s, mut commands) = scrolled_room();
    let account = AccountId::new("@a:example.org");
    let key = TimelineKey::room("!one:x");
    apply(&mut s, Event::TimelineChanged { account, key, diffs: vec![matrix_core::TimelineDiff::Reset(Vec::new())] });
    s.open_found_message(&message_hit());
    drain(&mut commands);
    (s, commands)
}

#[test]
fn finding_an_older_message_should_wait_for_each_history_page() {
    let (mut s, mut commands) = looking_for_message();
    s.position_message(&mut Tx::default());
    s.position_message(&mut Tx::default());
    assert_eq!(all_commands(&mut commands).iter().filter(|c| matches!(c, Command::LoadOlder { .. })).count(), 1);
}

#[test]
fn finding_an_older_message_should_pause_after_a_history_error() {
    let (mut s, mut commands) = looking_for_message();
    apply(
        &mut s,
        Event::HistoryFailed {
            account: AccountId::new("@a:example.org"),
            key: TimelineKey::room("!one:x"),
            error: "offline".into(),
        },
    );
    s.position_message(&mut Tx::default());
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::LoadOlder { .. })));
}

#[test]
fn exhausting_history_should_report_that_the_message_is_unavailable() {
    let (mut s, _) = looking_for_message();
    apply(
        &mut s,
        Event::HistoryLoaded {
            account: AccountId::new("@a:example.org"),
            key: TimelineKey::room("!one:x"),
            reached_start: true,
        },
    );
    s.position_message(&mut Tx::default());
    assert!(s.message_jump.is_none() && s.notice.as_deref() == Some(t!("search.message_unavailable")));
}

#[test]
fn navigating_away_should_stop_looking_for_the_message() {
    let (mut s, mut commands) = looking_for_message();
    s.show_overview();
    s.position_message(&mut Tx::default());
    assert!(s.message_jump.is_none());
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::LoadOlder { .. })));
}

#[test]
fn looking_for_a_message_should_not_mark_the_newest_message_read() {
    let (mut s, mut commands) = looking_for_message();
    let account = AccountId::new("@a:example.org");
    let key = TimelineKey::room("!one:x");
    s.accounts.timeline_mut(&account, &key).unwrap().read_up_to = Some("$newest".into());
    s.mark_timeline_read(account, key);
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::MarkRead { .. })));
}

#[test]
fn loading_the_matching_message_should_finish_the_jump() {
    let (mut s, _) = looking_for_message();
    let entry = matrix_core::TimelineEntry {
        id: "stable-row-id".into(),
        kind: matrix_core::EntryKind::Message(Box::new(Message { event_id: "$hit".into(), ..Message::default() })),
    };
    apply(
        &mut s,
        Event::TimelineChanged {
            account: AccountId::new("@a:example.org"),
            key: TimelineKey::room("!one:x"),
            diffs: vec![matrix_core::TimelineDiff::PushFront(entry)],
        },
    );
    s.position_message(&mut Tx::default());
    assert!(s.message_jump.is_none());
}

#[test]
fn retrying_a_timeline_subscription_should_not_record_another_room_visit() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    drain(&mut commands);
    s.select_room("!one:x");
    let request = drain(&mut commands)
        .into_iter()
        .find_map(|(request, command)| matches!(command, Command::OpenTimeline { .. }).then_some(request))
        .unwrap();
    apply(&mut s, Event::Finished { request, result: Err("timeline closed".into()) });
    s.retry_history(&TimelineKey::room("!one:x"));
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::VisitRoom { .. })));
}

#[test]
fn a_failed_forward_should_keep_the_destination_picker_available() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    drain(&mut commands);
    s.start_forward(TimelineKey::thread("!one:x", "$root"), "$reply".into());
    s.forward_to("!other:x".into());
    let (request, command) = commands.try_next().expect("forward command");
    assert!(
        matches!(command, Command::Forward { key, event_id, room_id, .. } if key.thread.as_deref() == Some("$root") && event_id == "$reply" && room_id == "!other:x")
    );
    apply(&mut s, Event::Finished { request, result: Err("forbidden".into()) });
    assert!(matches!(s.modal, Some(Modal::Forward { sending: None, .. })));
}

#[test]
fn an_earlier_forward_should_not_close_a_later_destination_picker() {
    let (mut s, mut commands) = state();
    signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    drain(&mut commands);
    s.start_forward(TimelineKey::room("!one:x"), "$old".into());
    s.forward_to("!other:x".into());
    let (request, _) = commands.try_next().expect("forward command");
    s.start_forward(TimelineKey::room("!one:x"), "$new".into());
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(matches!(s.modal, Some(Modal::Forward { event_id, .. }) if event_id == "$new"));
}

#[test]
fn changing_pins_should_refresh_the_open_pinned_list() {
    let (mut s, mut commands) = state();
    let id = signed_in_with(&mut s, matrix_core::SignInMethod::Password);
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    s.show_side(SidePanel::Pins);
    drain(&mut commands);
    let data = s.accounts.get(&id).expect("account");
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    rooms.iter_mut().find(|r| r.room_id == "!one:x").expect("room").pinned.push("$new".into());
    apply(&mut s, Event::RoomsUpdated { account: id, list: RoomList { rooms, spaces, invitations: Vec::new() } });
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::ListPinnedMessages { room_id, .. } if room_id == "!one:x"))
    );
}

#[test]
fn opening_a_read_indicator_should_request_the_complete_member_list() {
    let (mut s, mut commands) = state();
    crate::demo::read_receipts(&mut s);
    drain(&mut commands);
    s.toggle_readers(&TimelineKey::room(crate::demo::DEMO_THREAD.0), "$read-demo");
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command, Command::ListMembers { room_id, all: true, .. } if room_id == crate::demo::DEMO_THREAD.0)));
}

#[test]
fn thread_read_indicators_should_stay_hidden_until_the_sdk_distinguishes_threads() {
    let (mut s, _) = state();
    crate::demo::read_receipts(&mut s);
    s.open_thread("$read-demo");
    assert!(
        !s.active_account().unwrap().readers(&TimelineKey::room(crate::demo::DEMO_THREAD.0), "$read-demo").is_empty()
    );
    s.toggle_readers(&TimelineKey::thread(crate::demo::DEMO_THREAD.0, "$read-demo"), "$read-demo");
    assert!(s.popover.is_none());
}

#[test]
fn read_indicator_profiles_should_fall_back_to_the_user_id() {
    let (mut s, _) = state();
    crate::demo::read_receipts(&mut s);
    assert_eq!(
        s.active_account().unwrap().reader(crate::demo::DEMO_THREAD.0, "@unknown:example.org"),
        matrix_core::Person { id: "@unknown:example.org".into(), name: "@unknown:example.org".into(), avatar: None }
    );
}
#[test]
fn an_unfiltered_search_should_request_each_source_for_the_active_account() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@a:example.org");
    with_rooms(&mut s, &account, &[], Vec::new());
    drain(&mut commands);
    s.search.field.insert("rust");
    rest_search(&mut s);
    let searches: Vec<_> = all_commands(&mut commands)
        .into_iter()
        .filter(|c| {
            matches!(c, Command::SearchPeople { .. } | Command::SearchMessages { .. } | Command::SearchDirectory { .. })
        })
        .collect();
    assert_eq!(
        searches,
        vec![
            Command::SearchMessages { account: account.clone(), term: "rust".into() },
            Command::SearchPeople { account: account.clone(), term: "rust".into() },
            Command::SearchDirectory {
                account: account.clone(),
                term: "rust".into(),
                kind: matrix_core::DirectoryKind::Rooms
            },
            Command::SearchDirectory { account, term: "rust".into(), kind: matrix_core::DirectoryKind::Spaces },
        ]
    );
}

#[test]
fn a_filter_should_only_search_its_selected_source() {
    for (filter, command) in [
        (
            SearchFilter::Spaces,
            Command::SearchDirectory {
                account: AccountId::new("@a:example.org"),
                term: "rust".into(),
                kind: matrix_core::DirectoryKind::Spaces,
            },
        ),
        (
            SearchFilter::Rooms,
            Command::SearchDirectory {
                account: AccountId::new("@a:example.org"),
                term: "rust".into(),
                kind: matrix_core::DirectoryKind::Rooms,
            },
        ),
        (
            SearchFilter::People,
            Command::SearchPeople { account: AccountId::new("@a:example.org"), term: "rust".into() },
        ),
        (
            SearchFilter::Messages,
            Command::SearchMessages { account: AccountId::new("@a:example.org"), term: "rust".into() },
        ),
    ] {
        let (mut s, mut commands) = state();
        with_rooms(&mut s, &AccountId::new("@a:example.org"), &[], Vec::new());
        s.search.field.insert("rust");
        s.select_search_filter(filter);
        drain(&mut commands);
        rest_search(&mut s);
        assert_eq!(all_commands(&mut commands), [command]);
    }
}

#[test]
fn changing_filters_should_cancel_requests_and_ignore_their_answers() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &[], Vec::new());
    s.search.field.insert("rust");
    rest_search(&mut s);
    let old = s.search.requests();
    let directory = s.search.public_spaces.status.request().unwrap();
    drain(&mut commands);
    s.select_search_filter(SearchFilter::People);
    apply(&mut s, Event::DirectoryFound { request: directory, rooms: vec![directory_room(true)] });
    assert!(s.search.public_spaces.rooms.is_empty());
    assert_eq!(all_commands(&mut commands), old.into_iter().map(Command::Cancel).collect::<Vec<_>>());
}

#[test]
fn a_directory_response_for_changed_text_should_be_ignored_before_the_next_poll() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &[], Vec::new());
    s.search.field.insert("rust");
    rest_search(&mut s);
    let request = s.search.public_rooms.status.request().unwrap();
    s.search.field = TextEdit::new("other");
    apply(&mut s, Event::DirectoryFound { request, rooms: vec![directory_room(false)] });
    assert!(s.search.public_rooms.rooms.is_empty());
}

#[test]
fn a_directory_failure_should_finish_only_its_own_source() {
    let (mut s, _) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &[], Vec::new());
    s.search.field.insert("rust");
    rest_search(&mut s);
    let request = s.search.public_rooms.status.request().unwrap();
    apply(&mut s, Event::Finished { request, result: Err("unavailable".into()) });
    assert_eq!(s.search.public_rooms.status, SearchStatus::Failed);
    assert!(s.search.public_spaces.status.request().is_some());
    assert!(s.search.message_request().is_some());
}

fn directory_room(space: bool) -> matrix_core::RoomPreview {
    matrix_core::RoomPreview {
        room_id: "!public:example.org".into(),
        alias: Some("#public:example.org".into()),
        name: Some("Public".into()),
        topic: None,
        avatar: None,
        member_count: 80,
        join_rule: Some(matrix_core::JoinRule::Public),
        space,
        joined: false,
        invited: false,
    }
}

#[test]
fn picking_a_directory_result_should_preview_it_for_the_search_account_without_joining() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@a:example.org");
    with_rooms(&mut s, &account, &[], Vec::new());
    drain(&mut commands);
    s.open_directory_result(&directory_room(true));
    assert!(all_commands(&mut commands).is_empty());
    let Some(Modal::Link(dialog)) = s.modal else { panic!("expected room preview") };
    assert_eq!(dialog.account, account);
    assert_eq!(dialog.link.via, ["example.org"]);
    assert!(matches!(dialog.step, LinkStep::Preview(room) if room.space));
}

#[test]
fn directory_results_should_request_their_avatar_as_soon_as_they_arrive() {
    let (mut s, mut commands) = state();
    s.media.attach(Arc::new(|| {}));
    let account = AccountId::new("@a:example.org");
    with_rooms(&mut s, &account, &[], Vec::new());
    s.search.field.insert("public");
    rest_search(&mut s);
    let request = s.search.public_rooms.status.request().unwrap();
    drain(&mut commands);
    let avatar = matrix_core::MediaRef::opaque("public-avatar");
    let room = matrix_core::RoomPreview { avatar: Some(avatar.clone()), ..directory_room(false) };
    apply(&mut s, Event::DirectoryFound { request, rooms: vec![room] });
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command,
        Command::FetchMedia { account: owner, media, size } if owner == &account && media == &avatar && size == &AVATAR_SIZE
    )));
}

/// An account whose one room shows a picture, fetched and loaded; returns that picture.
fn listed_picture(s: &mut State, commands: &mut DetachedCommands, id: &AccountId) -> MediaRef {
    s.media.attach(Arc::new(|| {}));
    let avatar = MediaRef::opaque("room-avatar");
    apply(s, Event::AccountAdded(account(id.as_str())));
    let list = |unread| RoomList {
        rooms: vec![RoomSummary { avatar: Some(avatar.clone()), unread, ..room("!one:x") }],
        spaces: Vec::new(),
        invitations: Vec::new(),
    };
    apply(s, Event::RoomsUpdated { account: id.clone(), list: list(1) });
    let (request, _) = drain(commands)
        .into_iter()
        .find(|(_, command)| matches!(command, Command::FetchMedia { media, .. } if media == &avatar))
        .expect("the listed picture to be fetched");
    let bytes: Arc<[u8]> = Arc::from(&b"an avatar"[..]);
    apply(s, Event::MediaLoaded { request, media: avatar.clone(), size: AVATAR_SIZE, bytes });
    avatar
}

fn change_the_room_list(s: &mut State, account: &AccountId, avatar: &MediaRef) {
    let room = RoomSummary { avatar: Some(avatar.clone()), unread: 5, ..room("!one:x") };
    let list = RoomList { rooms: vec![room], spaces: Vec::new(), invitations: Vec::new() };
    apply(s, Event::RoomsUpdated { account: account.clone(), list });
}

fn fetches(commands: &mut DetachedCommands, avatar: &MediaRef) -> bool {
    all_commands(commands).iter().any(|command| matches!(command, Command::FetchMedia { media, .. } if media == avatar))
}

#[test]
fn a_list_change_showing_the_same_pictures_should_not_fetch_them_again() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@a:example.org");
    let avatar = listed_picture(&mut s, &mut commands, &account);
    change_the_room_list(&mut s, &account, &avatar);
    assert!(!fetches(&mut commands, &avatar));
}

#[test]
fn a_listed_picture_let_go_of_should_be_fetched_again_when_the_lists_change() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@a:example.org");
    let avatar = listed_picture(&mut s, &mut commands, &account);
    s.media.forget(&avatar);
    change_the_room_list(&mut s, &account, &avatar);
    assert!(fetches(&mut commands, &avatar));
}

fn read_member(id: &str, name: &str) -> matrix_core::RoomMember {
    matrix_core::RoomMember {
        person: matrix_core::Person { id: id.into(), name: name.into(), avatar: None },
        role: matrix_core::MemberRole::Member,
        presence: matrix_core::Presence::Unknown,
        status_message: None,
        verified: false,
        level: 0,
    }
}

fn post(s: &mut State, account: &AccountId, event_id: &str, sender: &str, name: &str) {
    let message = matrix_core::Message {
        event_id: event_id.into(),
        sender_id: sender.into(),
        sender_name: name.into(),
        ..matrix_core::Message::default()
    };
    apply(
        s,
        Event::TimelineChanged {
            account: account.clone(),
            key: TimelineKey::room("!one:x"),
            diffs: vec![matrix_core::TimelineDiff::PushBack(matrix_core::TimelineEntry {
                id: event_id.into(),
                kind: matrix_core::EntryKind::Message(Box::new(message)),
            })],
        },
    );
}

#[test]
fn a_reader_should_take_the_name_the_room_members_give_them() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    post(&mut s, &id, "$1", "@b:x", "Bea from a message");
    let members =
        matrix_core::Members { people: vec![read_member("@b:x", "Bea")], complete: true, ..Default::default() };
    apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: "!one:x".into(), members });
    assert_eq!(s.accounts.get(&id).unwrap().reader("!one:x", "@b:x").name, "Bea");
}

#[test]
fn a_newer_member_list_should_rename_its_readers() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    for name in ["Bea", "Beatrice"] {
        let members = matrix_core::Members {
            people: vec![read_member("@c:x", "Cy"), read_member("@b:x", name)],
            complete: true,
            ..Default::default()
        };
        apply(&mut s, Event::Members { request: None, account: id.clone(), room_id: "!one:x".into(), members });
    }
    assert_eq!(s.accounts.get(&id).unwrap().reader("!one:x", "@b:x").name, "Beatrice");
}

#[test]
fn a_reader_outside_the_members_should_take_their_newest_message_name() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    post(&mut s, &id, "$1", "@b:x", "Bea");
    assert_eq!(s.accounts.get(&id).unwrap().reader("!one:x", "@b:x").name, "Bea");
    post(&mut s, &id, "$2", "@b:x", "Beatrice");
    assert_eq!(s.accounts.get(&id).unwrap().reader("!one:x", "@b:x").name, "Beatrice");
}

#[test]
fn a_reader_the_room_knows_nothing_about_should_go_by_their_user_id() {
    let (mut s, _) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x"], Vec::new());
    post(&mut s, &id, "$1", "@b:x", "Bea");
    assert_eq!(s.accounts.get(&id).unwrap().reader("!one:x", "@z:x").name, "@z:x");
}

/// A signed-in account with two rooms, the first open, and the commands sent so far drained.
fn two_rooms() -> (State, DetachedCommands, AccountId) {
    let (mut s, mut commands) = state();
    let id = AccountId::new("@a:example.org");
    with_rooms(&mut s, &id, &["!one:x", "!two:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    (s, commands, id)
}

fn type_into(s: &mut State, target: ComposerTarget, text: &str) {
    s.composer_mut(target).edit = TextEdit::new(text);
}

#[test]
fn a_draft_left_in_a_room_should_be_there_on_coming_back() {
    let (mut s, _, _) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "half a thought");
    s.select_room("!two:x");
    assert_eq!(s.composer.edit.text, "");
    s.select_room("!one:x");
    assert_eq!(s.composer.edit.text, "half a thought");
}

#[test]
fn a_draft_should_keep_its_reply_while_another_room_shows() {
    let (mut s, _, _) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "yes");
    s.composer.reply_to =
        Some(ReplyTarget { event_id: "$q".into(), sender_name: "Grace".into(), excerpt: "Lunch?".into() });
    s.select_room("!two:x");
    assert_eq!(s.composer.reply_to, None);
    s.select_room("!one:x");
    assert_eq!(s.composer.reply_to.as_ref().map(|reply| reply.event_id.as_str()), Some("$q"));
}

#[test]
fn a_threads_draft_should_come_back_with_its_thread() {
    let (mut s, _, _) = two_rooms();
    s.open_thread("$root");
    type_into(&mut s, ComposerTarget::Thread, "in the thread");
    s.open_thread("$other");
    assert_eq!(s.thread_composer.edit.text, "");
    s.open_thread("$root");
    assert_eq!(s.thread_composer.edit.text, "in the thread");
}

#[test]
fn leaving_a_room_should_keep_its_draft_in_the_store() {
    let (mut s, mut commands, id) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "for later");
    s.select_room("!two:x");
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { account, key, draft: Some(draft) }
        if *account == id && *key == TimelineKey::room("!one:x") && draft.body == "for later")));
}

#[test]
fn sending_should_forget_the_kept_draft_at_once() {
    let (mut s, mut commands, _) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "for later");
    s.select_room("!two:x");
    s.select_room("!one:x");
    drain(&mut commands);
    s.send_message(ComposerTarget::Room, &mut Tx::default());
    s.poll(&mut Tx::default());
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { key, draft: None, .. } if *key == TimelineKey::room("!one:x"))));
}

#[test]
fn a_room_should_ask_the_store_for_its_draft_once() {
    let (mut s, mut commands, _) = two_rooms();
    s.select_room("!two:x");
    let (request, _) = drain(&mut commands)
        .into_iter()
        .find(|(_, c)| matches!(c, Command::LoadDraft { key, .. } if *key == TimelineKey::room("!two:x")))
        .expect("draft asked");
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    s.select_room("!one:x");
    s.select_room("!two:x");
    assert!(
        !all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::LoadDraft { key, .. } if *key == TimelineKey::room("!two:x")))
    );
}

#[test]
fn a_kept_draft_should_fill_an_empty_composer() {
    let (mut s, _, id) = two_rooms();
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    assert_eq!(s.composer.edit.text, "from last time");
}

#[test]
fn a_kept_draft_should_not_replace_what_was_typed_meanwhile() {
    let (mut s, _, id) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "new words");
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    assert_eq!(s.composer.edit.text, "new words");
}

#[test]
fn a_kept_draft_for_a_room_no_longer_shown_should_wait_for_the_next_visit() {
    let (mut s, mut commands, id) = two_rooms();
    s.select_room("!two:x");
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    assert_eq!(s.composer.edit.text, "");
    drain(&mut commands);
    s.select_room("!one:x");
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::LoadDraft { key, .. } if *key == TimelineKey::room("!one:x")))
    );
}

#[test]
fn a_draft_coming_back_should_not_tell_the_room_the_user_is_typing() {
    let (mut s, mut commands, id) = two_rooms();
    s.poll(&mut Tx::default());
    drain(&mut commands);
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    s.poll(&mut Tx::default());
    assert!(!all_commands(&mut commands).iter().any(|c| matches!(c, Command::Typing { typing: true, .. })));
}

#[test]
fn saving_a_message_should_ask_the_engine_to_save_it() {
    let (mut s, mut commands, id) = two_rooms();
    s.set_message_saved(&id, "!one:x", "$m", true);
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SetSaved { account, room_id, event_id, saved: true }
        if *account == id && room_id == "!one:x" && event_id == "$m")));
}

#[test]
fn opening_saved_messages_should_list_them() {
    let (mut s, mut commands, id) = two_rooms();
    s.open_saved();
    assert_eq!(s.modal, Some(Modal::Saved(id.clone())));
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::ListSavedMessages { account } if *account == id))
    );
}

fn saved_message(room_id: &str, event_id: &str) -> matrix_core::SavedMessage {
    matrix_core::SavedMessage {
        room_id: room_id.into(),
        saved_at_ms: Some(1),
        preview: matrix_core::MessagePreview {
            event_id: event_id.into(),
            sender_name: "Grace".into(),
            sender_id: "@grace:x".into(),
            sender_avatar: None,
            timestamp_ms: Some(1),
            body: Some("Remember this".into()),
            thread: None,
        },
    }
}

fn set_saved_ids(s: &mut State, id: &AccountId, room_id: &str, saved: &[&str]) {
    let data = s.accounts.get(id).expect("account");
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    rooms.iter_mut().find(|r| r.room_id == room_id).expect("room").saved =
        saved.iter().map(|id| (*id).to_owned()).collect();
    apply(s, Event::RoomsUpdated { account: id.clone(), list: RoomList { rooms, spaces, invitations: Vec::new() } });
}

#[test]
fn a_message_no_longer_saved_should_leave_the_list_at_once() {
    let (mut s, _, id) = two_rooms();
    set_saved_ids(&mut s, &id, "!one:x", &["$a", "$b"]);
    apply(
        &mut s,
        Event::SavedMessages {
            account: id.clone(),
            messages: vec![saved_message("!one:x", "$a"), saved_message("!one:x", "$b")],
        },
    );
    set_saved_ids(&mut s, &id, "!one:x", &["$b"]);
    let listed: Vec<_> =
        s.accounts.get(&id).expect("account").saved_messages().map(|m| m.preview.event_id.as_str()).collect();
    assert_eq!(listed, ["$b"]);
}

#[test]
fn a_message_saved_elsewhere_should_join_the_open_list() {
    let (mut s, mut commands, id) = two_rooms();
    s.open_saved();
    let (request, _) = drain(&mut commands).pop().expect("listed");
    apply(&mut s, Event::SavedMessages { account: id.clone(), messages: Vec::new() });
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    set_saved_ids(&mut s, &id, "!two:x", &["$new"]);
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::ListSavedMessages { .. })));
}

#[test]
fn opening_a_saved_message_should_show_its_room() {
    let (mut s, _, _) = two_rooms();
    s.open_saved();
    s.open_saved_message(&AccountId::new("@a:example.org"), "!two:x", None, "$m");
    assert_eq!(s.modal, None);
    assert_eq!(s.active_room_id(), Some("!two:x"));
}

#[test]
fn a_kept_draft_should_not_return_once_the_user_typed_and_cleared_meanwhile() {
    let (mut s, mut commands, id) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "new words");
    s.poll(&mut Tx::default());
    type_into(&mut s, ComposerTarget::Room, "");
    s.poll(&mut Tx::default());
    drain(&mut commands);
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    assert_eq!(s.composer.edit.text, "");
    s.poll(&mut Tx::default());
    // What the user left (nothing) replaces the stored draft too.
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { key, draft: None, .. } if *key == TimelineKey::room("!one:x"))));
}

#[test]
fn losing_focus_should_write_a_changed_draft_at_once() {
    let (mut s, mut commands, _) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "almost done");
    s.poll(&mut Tx::default());
    drain(&mut commands);
    s.focus_changed(false);
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { draft: Some(draft), .. } if draft.body == "almost done")));
}

#[test]
fn a_draft_changed_after_it_was_written_should_be_written_again() {
    let (mut s, mut commands, _) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "almost");
    s.focus_changed(false);
    drain(&mut commands);
    s.composer.edit.insert(" done");
    s.focus_changed(false);
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { draft: Some(draft), .. } if draft.body == "almost done")));
}

#[test]
fn closing_a_thread_typed_in_should_tell_the_room_typing_stopped() {
    let (mut s, mut commands, _) = two_rooms();
    s.open_thread("$root");
    s.poll(&mut Tx::default());
    type_into(&mut s, ComposerTarget::Thread, "in the thread");
    s.poll(&mut Tx::default());
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::Typing { typing: true, .. })));
    s.close_thread();
    s.poll(&mut Tx::default());
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::Typing { typing: false, .. })));
}

#[test]
fn a_message_saved_while_the_list_loads_should_be_listed_after() {
    let (mut s, mut commands, id) = two_rooms();
    s.open_saved();
    let (request, _) = drain(&mut commands).pop().expect("listed");
    // Saved while the list was on its way: the list can't have it.
    set_saved_ids(&mut s, &id, "!two:x", &["$new"]);
    apply(&mut s, Event::SavedMessages { account: id.clone(), messages: Vec::new() });
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::ListSavedMessages { .. })));
}

#[test]
fn a_saved_message_of_another_account_should_open_on_that_account() {
    let (mut s, _, a) = two_rooms();
    let b = AccountId::new("@b:example.org");
    with_rooms(&mut s, &b, &["!b-room:x"], Vec::new());
    s.select_account(&b);
    s.open_saved_message(&a, "!two:x", None, "$m");
    assert_eq!(s.nav.account, Some(a));
    assert_eq!(s.active_room_id(), Some("!two:x"));
}

#[test]
fn removing_a_saved_message_should_use_the_lists_account() {
    let (mut s, mut commands, a) = two_rooms();
    let b = AccountId::new("@b:example.org");
    with_rooms(&mut s, &b, &["!two:x"], Vec::new());
    s.select_account(&b);
    drain(&mut commands);
    s.set_message_saved(&a, "!two:x", "$m", false);
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|c| matches!(c, Command::SetSaved { account, saved: false, .. } if *account == a))
    );
}

#[test]
fn a_draft_cleared_before_its_room_was_left_should_not_come_back_from_a_late_read() {
    let (mut s, mut commands, id) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "new words");
    s.poll(&mut Tx::default());
    type_into(&mut s, ComposerTarget::Room, "");
    s.poll(&mut Tx::default());
    s.select_room("!two:x");
    drain(&mut commands);
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { key, draft: None, .. } if *key == TimelineKey::room("!one:x"))));
}

#[test]
fn a_kept_draft_should_go_under_an_edit_started_before_it_came() {
    let (mut s, _) = state();
    let id = with_own_message(&mut s);
    s.edit_last(ComposerTarget::Room, &mut Tx::default());
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    s.cancel_edit(ComposerTarget::Room);
    assert_eq!(s.composer.edit.text, "from last time");
}

#[test]
fn a_kept_draft_should_join_a_reply_picked_before_it_came() {
    let (mut s, _, id) = two_rooms();
    s.composer.reply_to =
        Some(ReplyTarget { event_id: "$q".into(), sender_name: "Grace".into(), excerpt: "Lunch?".into() });
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    assert_eq!(s.composer.edit.text, "from last time");
    assert!(s.composer.reply_to.is_some());
}

#[test]
fn a_read_answered_after_a_write_should_change_nothing() {
    let (mut s, mut commands, id) = two_rooms();
    type_into(&mut s, ComposerTarget::Room, "replacement");
    s.focus_changed(false);
    type_into(&mut s, ComposerTarget::Room, "original");
    let draft = matrix_core::Draft { body: "original".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    drain(&mut commands);
    s.focus_changed(false);
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c,
        Command::SaveDraft { draft: Some(draft), .. } if draft.body == "original")));
}

#[test]
fn switching_to_a_thread_with_a_draft_should_tell_the_room_typing_stopped() {
    let (mut s, mut commands, _) = two_rooms();
    s.open_thread("$b");
    type_into(&mut s, ComposerTarget::Thread, "kept in b");
    s.open_thread("$a");
    s.poll(&mut Tx::default());
    type_into(&mut s, ComposerTarget::Thread, "typing in a");
    s.poll(&mut Tx::default());
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::Typing { typing: true, .. })));
    s.open_thread("$b");
    s.poll(&mut Tx::default());
    assert!(all_commands(&mut commands).iter().any(|c| matches!(c, Command::Typing { typing: false, .. })));
}

#[test]
fn a_kept_draft_should_join_a_reply_left_in_its_room_before_it_came() {
    let (mut s, _, id) = two_rooms();
    s.composer.reply_to =
        Some(ReplyTarget { event_id: "$q".into(), sender_name: "Grace".into(), excerpt: "Lunch?".into() });
    s.select_room("!two:x");
    let draft = matrix_core::Draft { body: "from last time".into(), html: None };
    apply(
        &mut s,
        Event::DraftLoaded { request: RequestId(0), account: id, key: TimelineKey::room("!one:x"), draft: Some(draft) },
    );
    s.select_room("!one:x");
    assert_eq!(s.composer.edit.text, "from last time");
    assert!(s.composer.reply_to.is_some());
}

#[test]
fn switching_threads_while_recording_should_not_mix_their_drafts() {
    let (mut s, _) = state();
    let id = with_own_message(&mut s);
    // Thread B: an edit left unfinished, set aside when its thread closed.
    s.open_thread("$b");
    let edited =
        composer::EditTarget { account: id, key: TimelineKey::thread("!one:x", "$b"), event_id: "$mine".into() };
    s.thread_composer.start_edit(edited, "helo", None);
    s.open_thread("$a");
    // Thread A: a draft, and a memo recording.
    type_into(&mut s, ComposerTarget::Thread, "draft in a");
    s.start_voice_memo(ComposerTarget::Thread);
    s.open_thread("$b");
    assert!(s.thread_composer.editing.is_some());
    s.cancel_edit(ComposerTarget::Thread);
    assert_eq!(s.thread_composer.edit.text, "");
    s.open_thread("$a");
    assert_eq!(s.thread_composer.edit.text, "draft in a");
}

/// A timeline holding a date divider and then `messages`.
fn history_timeline(messages: &[&str]) -> RoomTimeline {
    let mut timeline = RoomTimeline::default();
    let mut entries = vec![history_entry("day", EntryKind::DateDivider(0))];
    entries.extend(messages.iter().map(|id| history_message(id)));
    timeline.apply(vec![TimelineDiff::Reset(entries)]);
    timeline
}

fn history_entry(id: &str, kind: EntryKind) -> TimelineEntry {
    TimelineEntry { id: id.to_owned(), kind }
}

fn history_message(id: &str) -> TimelineEntry {
    history_entry(id, EntryKind::Message(Box::new(Message { event_id: format!("${id}"), ..Message::default() })))
}

#[test]
fn older_messages_above_the_first_should_count_as_history() {
    let timeline = history_timeline(&["a", "b"]);
    assert!(timeline.adds_history(&[TimelineDiff::PushFront(history_message("older"))]));
    assert!(timeline.adds_history(&[TimelineDiff::Insert { index: 1, entry: history_message("older") }]));
}

#[test]
fn a_new_message_at_the_end_should_not_count_as_history() {
    let timeline = history_timeline(&["a", "b"]);
    assert!(!timeline.adds_history(&[TimelineDiff::PushBack(history_message("new"))]));
    assert!(!timeline.adds_history(&[
        TimelineDiff::Remove { index: 2 },
        TimelineDiff::Insert { index: 2, entry: history_message("b") },
    ]));
}

#[test]
fn a_timelines_first_messages_should_count_as_history() {
    let timeline = history_timeline(&[]);
    assert!(timeline.adds_history(&[TimelineDiff::Append(vec![history_message("a"), history_message("b")])]));
}

#[test]
fn history_after_a_removed_divider_should_still_count_as_history() {
    let timeline = history_timeline(&["a"]);
    assert!(timeline.adds_history(&[
        TimelineDiff::Remove { index: 0 },
        TimelineDiff::Insert { index: 0, entry: history_message("older") },
    ]));
}

#[test]
fn a_read_marker_above_the_first_message_should_not_count_as_history() {
    let timeline = history_timeline(&["a"]);
    assert!(!timeline.adds_history(&[
        TimelineDiff::Insert { index: 1, entry: history_entry("marker", EntryKind::ReadMarker) },
        TimelineDiff::PushBack(history_message("new")),
    ]));
}

#[test]
fn history_above_the_next_message_should_count_once_the_first_is_removed() {
    let mut timeline = history_timeline(&["a"]);
    timeline.apply(vec![
        TimelineDiff::PushBack(history_entry("day-2", EntryKind::DateDivider(1))),
        TimelineDiff::PushBack(history_message("b")),
    ]);
    assert!(timeline.adds_history(&[
        TimelineDiff::Remove { index: 1 },
        TimelineDiff::Insert { index: 2, entry: history_message("older") },
    ]));
}

#[test]
fn an_insert_past_the_end_should_not_count_as_history() {
    let timeline = history_timeline(&[]);
    assert!(!timeline.adds_history(&[TimelineDiff::Insert { index: 9, entry: history_message("stray") }]));
}

#[test]
fn opening_someones_profile_should_fetch_their_picture() {
    let (mut s, mut commands) = state();
    s.media.attach(Arc::new(|| {}));
    let account = AccountId::new("@a:example.org");
    with_rooms(&mut s, &account, &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    let avatar = MediaRef::opaque("person-avatar");
    s.show_side(SidePanel::Person(matrix_core::Person {
        id: "@b:x".into(),
        name: "B".into(),
        avatar: Some(avatar.clone()),
    }));
    assert!(all_commands(&mut commands).iter().any(|command| matches!(command,
        Command::FetchMedia { account: owner, media, .. } if owner == &account && media == &avatar
    )));
}

#[test]
fn opening_someones_profile_should_request_the_complete_member_list() {
    let (mut s, mut commands) = state();
    with_rooms(&mut s, &AccountId::new("@a:example.org"), &["!one:x"], Vec::new());
    s.select_room("!one:x");
    drain(&mut commands);
    s.show_side(SidePanel::Person(matrix_core::Person { id: "@b:x".into(), ..Default::default() }));
    assert!(
        all_commands(&mut commands)
            .iter()
            .any(|command| matches!(command, Command::ListMembers { room_id, all: true, .. } if room_id == "!one:x"))
    );
}
