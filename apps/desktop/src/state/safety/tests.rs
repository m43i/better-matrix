use matrix_core::{DetachedCommands, EngineHandle, Event, MediaSize, ShowMedia};
use rune::Tx;

use super::*;
use crate::demo::{ADA, DEMO_THREAD};
use crate::platform::Shell;
use crate::state::Settings;

fn state() -> (State, DetachedCommands) {
    let (engine, commands) = EngineHandle::detached();
    (State::new(engine, Shell::headless(), Settings::fixed_english()), commands)
}

fn ada() -> AccountId {
    AccountId::new(ADA)
}

fn drain(commands: &mut DetachedCommands) -> Vec<(RequestId, Command)> {
    std::iter::from_fn(|| commands.try_next()).collect()
}

fn finish(s: &mut State, request: RequestId, result: Result<(), String>) {
    s.apply(Event::Finished { request, result }, &mut Tx::default());
}

fn report_dialog(s: &State) -> &ReportForm {
    match &s.modal {
        Some(Modal::Report(form)) => form,
        other => panic!("no report dialog: {other:?}"),
    }
}

#[test]
fn safety_the_engine_reports_should_show_for_its_account() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    let safety = Safety { media: ShowMedia::Never, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety: safety.clone() }, &mut Tx::default());
    assert_eq!(s.accounts.get(&ada()).unwrap().safety, safety);
}

#[test]
fn a_safety_change_should_show_at_once_and_be_sent_to_the_engine() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    s.set_safety(&ada(), SafetyChange::HideStrangerInvites(true));
    assert!(s.accounts.get(&ada()).unwrap().safety.hide_stranger_invites);
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command,
        Command::SetSafety { account, change: SafetyChange::HideStrangerInvites(true) } if account == &ada())));
}

#[test]
fn a_safety_change_that_fails_should_fall_back_to_the_saved_choices() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    s.set_safety(&ada(), SafetyChange::Media(ShowMedia::Never));
    let (request, _) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::SetSafety { .. })).unwrap();
    finish(&mut s, request, Err("offline".into()));
    assert_eq!(s.accounts.get(&ada()).unwrap().safety.media, ShowMedia::Always);
    assert!(s.notice.is_some());
}

#[test]
fn hiding_strangers_invitations_should_keep_them_out_of_the_sidebar_but_in_settings() {
    let (mut s, _) = state();
    crate::demo::safety(&mut s);
    let data = s.accounts.get(&ada()).unwrap();
    assert!(data.shown_invitations().all(|invite| !invite.stranger));
    assert_eq!(
        data.hidden_invitations().map(|invite| invite.room_id.as_str()).collect::<Vec<_>>(),
        vec!["!prizes:spam.example"]
    );
}

#[test]
fn ignoring_someone_should_ask_first() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    s.ask_to_ignore("@troll:spam.example", "Troll");
    assert!(
        matches!(&s.modal, Some(Modal::Confirm(Confirm::Ignore { user_id, .. })) if user_id == "@troll:spam.example")
    );
    assert!(drain(&mut commands).is_empty());
    s.confirm();
    assert!(s.accounts.get(&ada()).unwrap().safety.ignores("@troll:spam.example"));
}

/// Ignores the inviter of the demo invitation from its preview, and the request that saves it.
fn ignore_inviter(s: &mut State, commands: &mut DetachedCommands) -> RequestId {
    crate::demo::invitation(s);
    drain(commands);
    s.ask_to_ignore_inviter(&ada(), "!design:example.org");
    s.confirm();
    let sent = drain(commands);
    assert!(!sent.iter().any(|(_, command)| matches!(command, Command::LeaveRoom { .. })), "declined before saving");
    sent.into_iter()
        .find_map(|(request, command)| match command {
            Command::SetSafety { change: SafetyChange::Ignore { user_id, ignored: true }, .. }
                if user_id == "@grace:example.org" =>
            {
                Some(request)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn ignoring_an_inviter_should_decline_their_invitation_once_saved() {
    let (mut s, mut commands) = state();
    let request = ignore_inviter(&mut s, &mut commands);
    finish(&mut s, request, Ok(()));
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command,
        Command::LeaveRoom { room_id, forget: false, .. } if room_id == "!design:example.org")));
}

#[test]
fn ignoring_an_inviter_that_fails_should_keep_their_invitation() {
    let (mut s, mut commands) = state();
    let request = ignore_inviter(&mut s, &mut commands);
    finish(&mut s, request, Err("offline".into()));
    assert!(!drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::LeaveRoom { .. })));
    let data = s.accounts.get(&ada()).unwrap();
    assert!(data.invitations.iter().any(|invite| invite.room_id == "!design:example.org"));
    assert!(!data.safety.ignores("@grace:example.org"));
    assert!(s.notice.is_some());
}

#[test]
fn a_rooms_preview_should_drop_a_message_from_someone_the_user_then_ignores() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    let troll = "@troll:spam.example";
    let message = Message {
        event_id: "$spam".into(),
        sender_id: troll.into(),
        sender_name: "Troll".into(),
        timestamp_ms: u64::MAX / 2,
        ..Message::default()
    };
    let room = "!lab:example.org";
    s.apply(
        Event::LatestMessage { account: ada(), room_id: room.into(), message: Box::new(message) },
        &mut Tx::default(),
    );
    let newest = |s: &State| s.accounts.get(&ada()).unwrap().last_message(room).map(|m| m.event_id.clone());
    assert_eq!(newest(&s).as_deref(), Some("$spam"));
    let safety = Safety { ignored: vec![troll.into()], ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert_ne!(newest(&s).as_deref(), Some("$spam"));
}

#[test]
fn media_opened_from_the_files_panel_should_wait_for_a_press_when_its_sender_is_unknown() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    let safety = Safety { media: ShowMedia::Never, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    s.select_room(DEMO_THREAD.0);
    s.media = crate::state::MediaCache::default();
    s.media.attach(std::sync::Arc::new(|| {}));
    drain(&mut commands);
    let file = crate::demo::demo_attachment();
    let key = TimelineKey::room(DEMO_THREAD.0);
    s.open_attachment(&key, "$listed-only", &file);
    assert!(s.preview.is_none());
    assert!(!drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::FetchMedia { .. })));
    // Shown now: the next press opens it.
    s.open_attachment(&key, "$listed-only", &file);
    assert!(s.preview.is_some());
}

#[test]
fn stopping_ignoring_someone_should_not_ask() {
    let (mut s, mut commands) = state();
    crate::demo::safety(&mut s);
    drain(&mut commands);
    s.stop_ignoring(&ada(), "@troll:spam.example");
    assert!(!s.accounts.get(&ada()).unwrap().safety.ignores("@troll:spam.example"));
}

#[test]
fn a_report_should_send_the_trimmed_reason_and_wait() {
    let (mut s, mut commands) = state();
    crate::demo::report(&mut s);
    drain(&mut commands);
    if let Some(Modal::Report(form)) = &mut s.modal {
        form.reason = TextEdit::new("  spam  ");
    }
    s.send_report();
    let sent = drain(&mut commands);
    assert!(sent.iter().any(|(_, command)| matches!(command,
        Command::Report { target: ReportTarget::Message { event_id, .. }, reason, .. }
            if event_id == "$attachment" && reason == "spam")));
    assert!(report_dialog(&s).sending.is_some());
}

#[test]
fn a_report_that_went_through_should_close_its_dialog_with_a_notice() {
    let (mut s, mut commands) = state();
    crate::demo::report(&mut s);
    drain(&mut commands);
    s.send_report();
    let (request, _) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::Report { .. })).unwrap();
    finish(&mut s, request, Ok(()));
    assert!(s.modal.is_none());
    assert_eq!(s.notice.as_deref(), Some(t!("report.sent")));
}

#[test]
fn a_report_that_failed_should_keep_its_dialog_open_to_try_again() {
    let (mut s, mut commands) = state();
    crate::demo::report(&mut s);
    drain(&mut commands);
    s.send_report();
    let (request, _) = drain(&mut commands).into_iter().find(|(_, c)| matches!(c, Command::Report { .. })).unwrap();
    finish(&mut s, request, Err("offline".into()));
    assert!(report_dialog(&s).sending.is_none());
    assert!(s.notice.is_some());
}

#[test]
fn reporting_with_also_ignore_should_ignore_the_sender() {
    let (mut s, mut commands) = state();
    crate::demo::report(&mut s);
    drain(&mut commands);
    s.toggle_report_ignore();
    s.send_report();
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command,
        Command::SetSafety { change: SafetyChange::Ignore { user_id, ignored: true }, .. } if user_id == "@grace:example.org")));
}

#[test]
fn reporting_a_room_should_not_offer_to_ignore_anyone() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    s.report_room(DEMO_THREAD.0);
    assert_eq!(report_dialog(&s).person, None);
}

#[test]
fn media_in_a_public_room_should_wait_for_a_press_without_loading() {
    let (mut s, mut commands) = state();
    s.media = crate::state::MediaCache::default();
    crate::demo::hidden_media(&mut s);
    s.media = crate::state::MediaCache::default();
    s.media.attach(std::sync::Arc::new(|| {}));
    drain(&mut commands);
    s.request_timeline_media();
    let fetched = |commands: &mut DetachedCommands| {
        drain(commands).into_iter().any(|(_, command)| {
            matches!(command,
            Command::FetchMedia { size: MediaSize::Thumbnail { .. } | MediaSize::Original, media, .. }
                if media == matrix_core::MediaRef::opaque("demo-image"))
        })
    };
    assert!(!fetched(&mut commands));
    s.reveal_media(&ada(), "$attachment");
    assert!(fetched(&mut commands));
}

#[test]
fn opening_held_back_media_should_show_it_in_place_first() {
    let (mut s, _) = state();
    crate::demo::hidden_media(&mut s);
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), "$attachment", &crate::demo::demo_attachment());
    assert!(s.preview.is_none());
    assert!(s.revealed_media.contains(&(ada(), "$attachment".to_owned())));
}

#[test]
fn the_users_own_media_should_always_show() {
    let (mut s, _) = state();
    crate::demo::hidden_media(&mut s);
    let own = Message { sender_id: ADA.into(), event_id: "$own".into(), ..Message::default() };
    let theirs = Message { sender_id: "@grace:example.org".into(), event_id: "$theirs".into(), ..Message::default() };
    assert!(!s.hides_media(&ada(), DEMO_THREAD.0, &own));
    assert!(s.hides_media(&ada(), DEMO_THREAD.0, &theirs));
}
