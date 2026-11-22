use matrix_core::{DetachedCommands, EngineHandle, Event, MediaSize, ShowMedia};
use rune::Tx;

use super::*;
use crate::demo::{ADA, DEMO_THREAD};
use crate::platform::Shell;
use crate::state::{AttachmentAction, Settings};

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

/// Sends the demo report with "Also ignore" ticked; the report's request, with nothing ignored
/// before it finishes.
fn report_and_ignore(s: &mut State, commands: &mut DetachedCommands) -> RequestId {
    crate::demo::report(s);
    drain(commands);
    s.toggle_report_ignore();
    s.send_report();
    let sent = drain(commands);
    assert!(!sent.iter().any(|(_, command)| matches!(command, Command::SetSafety { .. })), "ignored before reporting");
    sent.into_iter()
        .find_map(|(request, command)| matches!(command, Command::Report { .. }).then_some(request))
        .unwrap()
}

fn ignores_grace(sent: &[(RequestId, Command)]) -> bool {
    sent.iter().any(|(_, command)| matches!(command,
        Command::SetSafety { change: SafetyChange::Ignore { user_id, ignored: true }, .. } if user_id == "@grace:example.org"))
}

#[test]
fn reporting_with_also_ignore_should_ignore_the_sender_once_reported() {
    let (mut s, mut commands) = state();
    let request = report_and_ignore(&mut s, &mut commands);
    finish(&mut s, request, Ok(()));
    assert!(ignores_grace(&drain(&mut commands)));
}

#[test]
fn a_failed_report_should_not_ignore_anyone() {
    let (mut s, mut commands) = state();
    let request = report_and_ignore(&mut s, &mut commands);
    finish(&mut s, request, Err("offline".into()));
    assert!(!ignores_grace(&drain(&mut commands)));
    assert!(!s.accounts.get(&ada()).unwrap().safety.ignores("@grace:example.org"));
}

#[test]
fn declining_and_ignoring_while_an_ignore_is_still_saving_should_wait_for_a_save() {
    let (mut s, mut commands) = state();
    crate::demo::invitation(&mut s);
    drain(&mut commands);
    // Ignored from elsewhere first: shown, not saved yet.
    s.set_safety(&ada(), SafetyChange::Ignore { user_id: "@grace:example.org".into(), ignored: true });
    drain(&mut commands);
    s.ask_to_ignore_inviter(&ada(), "!design:example.org");
    s.confirm();
    let sent = drain(&mut commands);
    assert!(!sent.iter().any(|(_, command)| matches!(command, Command::LeaveRoom { .. })), "declined before saving");
    let request = sent
        .into_iter()
        .find_map(|(request, command)| matches!(command, Command::SetSafety { .. }).then_some(request))
        .unwrap();
    finish(&mut s, request, Ok(()));
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command,
        Command::LeaveRoom { room_id, forget: false, .. } if room_id == "!design:example.org")));
}

#[test]
fn declining_and_ignoring_someone_already_ignored_should_decline_at_once() {
    let (mut s, mut commands) = state();
    crate::demo::invitation(&mut s);
    let safety = Safety { ignored: vec!["@grace:example.org".into()], ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    drain(&mut commands);
    s.ask_to_ignore_inviter(&ada(), "!design:example.org");
    s.confirm();
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::LeaveRoom { .. })));
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

/// Ada's account with pictures and videos shown nowhere.
fn media_never(s: &mut State) {
    let safety = Safety { media: ShowMedia::Never, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
}

#[test]
fn a_change_still_saving_should_keep_showing_when_an_earlier_one_finishes() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    s.set_safety(&ada(), SafetyChange::Media(ShowMedia::Never));
    s.set_safety(&ada(), SafetyChange::HideStrangerInvites(true));
    let requests: Vec<_> = drain(&mut commands)
        .into_iter()
        .filter_map(|(request, command)| matches!(command, Command::SetSafety { .. }).then_some(request))
        .collect();
    // The first is saved: the engine answers with the server's choices, then finishes.
    let saved = Safety { media: ShowMedia::Never, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety: saved }, &mut Tx::default());
    finish(&mut s, requests[0], Ok(()));
    let shown = &s.accounts.get(&ada()).unwrap().safety;
    assert_eq!(shown.media, ShowMedia::Never);
    assert!(shown.hide_stranger_invites, "the second change, still saving, must still show");
}

#[test]
fn a_change_still_saving_should_keep_showing_when_an_earlier_one_fails() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    s.set_safety(&ada(), SafetyChange::Media(ShowMedia::Never));
    s.set_safety(&ada(), SafetyChange::HideStrangerInvites(true));
    let first = drain(&mut commands)
        .into_iter()
        .find_map(|(request, command)| matches!(command, Command::SetSafety { .. }).then_some(request))
        .unwrap();
    finish(&mut s, first, Err("offline".into()));
    let shown = &s.accounts.get(&ada()).unwrap().safety;
    assert_eq!(shown.media, ShowMedia::Always);
    assert!(shown.hide_stranger_invites);
}

#[test]
fn a_failed_ignore_should_bring_the_rooms_preview_back() {
    let (mut s, mut commands) = state();
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
    drain(&mut commands);
    s.set_safety(&ada(), SafetyChange::Ignore { user_id: troll.into(), ignored: true });
    let newest = |s: &State| s.accounts.get(&ada()).unwrap().last_message(room).map(|m| m.event_id.clone());
    assert_ne!(newest(&s).as_deref(), Some("$spam"), "hidden while the ignore saves");
    let request = drain(&mut commands)
        .into_iter()
        .find_map(|(request, command)| matches!(command, Command::SetSafety { .. }).then_some(request))
        .unwrap();
    finish(&mut s, request, Err("offline".into()));
    assert_eq!(newest(&s).as_deref(), Some("$spam"));
}

#[test]
fn a_held_back_video_without_a_poster_should_not_open_directly() {
    let (mut s, mut commands) = state();
    let video = matrix_core::Attachment {
        kind: matrix_core::AttachmentKind::Video,
        name: "clip.mp4".into(),
        size: None,
        media: Some(matrix_core::MediaRef::opaque("clip")),
        thumbnail: None,
        dimensions: None,
        audio: None,
    };
    crate::demo::image_message(&mut s, video.clone());
    media_never(&mut s);
    drain(&mut commands);
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), "$attachment", &video);
    assert!(s.preview.is_none());
    assert!(!drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::SaveMedia { .. })));
    assert!(s.revealed_media.contains(&(ada(), "$attachment".to_owned())));
}

#[test]
fn a_batch_should_keep_its_audio_and_documents_and_hold_back_its_pictures_until_shown() {
    let (mut s, mut commands) = state();
    crate::demo::batch_messages(&mut s);
    media_never(&mut s);
    let files = crate::demo::batch_attachments();
    let key = TimelineKey::room(DEMO_THREAD.0);
    // The recording opens, never held back, with the whole batch to step through.
    s.open_attachment(&key, "$batch-2", &files[2]);
    assert_eq!(s.preview.as_ref().map(|preview| preview.batch.len()), Some(4));
    drain(&mut commands);
    // Onto the video: held back, nothing fetched.
    s.show_preview_item(1);
    assert!(matches!(s.preview.as_ref().map(|p| &p.content), Some(crate::state::PreviewContent::Held)));
    assert!(!drain(&mut commands).iter().any(|(_, command)| matches!(
        command,
        Command::SaveMedia { .. } | Command::FetchMedia { size: MediaSize::Original, .. }
    )));
    // The PDF shows as ever.
    s.show_preview_item(3);
    assert!(matches!(s.preview.as_ref().map(|p| &p.content), Some(crate::state::PreviewContent::Document { .. })));
    // Back on the video, Show loads it.
    s.show_preview_item(1);
    s.reveal_preview_item();
    assert!(matches!(s.preview.as_ref().map(|p| &p.content), Some(crate::state::PreviewContent::Video { .. })));
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::SaveMedia { .. })));
}

#[test]
fn a_link_to_a_room_the_account_is_invited_to_should_hide_its_picture_when_invitations_do() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    let preview = matrix_core::RoomPreview {
        room_id: "!club:example.org".into(),
        alias: None,
        name: Some("Club".into()),
        topic: None,
        avatar: Some(matrix_core::MediaRef::opaque("club-picture")),
        member_count: 3,
        join_rule: Some(matrix_core::JoinRule::Invite),
        space: false,
        joined: false,
        invited: true,
    };
    let Some(link) = matrix_core::Permalink::parse("https://matrix.to/#/!club:example.org") else { panic!("a link") };
    s.modal = Some(Modal::Link(crate::state::LinkDialog {
        link,
        account: ada(),
        chosen: false,
        step: crate::state::LinkStep::Preview(Box::new(preview)),
    }));
    assert!(s.link_pictures().is_some());
    let safety = Safety { invite_pictures: false, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert!(s.link_pictures().is_none());
}

#[test]
fn a_link_to_a_room_the_account_holds_an_invitation_to_should_hide_its_picture_when_invitations_do() {
    let (mut s, _) = state();
    crate::demo::invitation(&mut s);
    let preview = matrix_core::RoomPreview {
        room_id: "!design:example.org".into(),
        alias: None,
        name: Some("Design".into()),
        topic: None,
        avatar: Some(matrix_core::MediaRef::opaque("design-picture")),
        member_count: 3,
        join_rule: Some(matrix_core::JoinRule::Invite),
        space: false,
        joined: false,
        // The preview doesn't say so; the account's invitations do.
        invited: false,
    };
    let Some(link) = matrix_core::Permalink::parse("https://matrix.to/#/!design:example.org") else { panic!("a link") };
    s.modal = Some(Modal::Link(crate::state::LinkDialog {
        link,
        account: ada(),
        chosen: false,
        step: crate::state::LinkStep::Preview(Box::new(preview)),
    }));
    let safety = Safety { invite_pictures: false, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert!(s.link_pictures().is_none());
}

#[test]
fn a_held_preview_item_should_not_be_copied_saved_or_opened() {
    let (mut s, mut commands) = state();
    crate::demo::preview_held(&mut s);
    assert!(s.preview_held());
    drain(&mut commands);
    let file = s.preview.as_ref().unwrap().attachment.clone();
    for action in [AttachmentAction::CopyImage, AttachmentAction::Open, AttachmentAction::Download] {
        s.act_on_attachment(ada(), &file, action);
    }
    s.download_preview();
    assert!(drain(&mut commands).is_empty());
}

#[test]
fn the_files_panel_should_go_by_each_files_sender() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    media_never(&mut s);
    let picture = |sender: &str| matrix_core::SharedFile {
        event_id: "$listed".into(),
        sender_name: sender.into(),
        sender_id: sender.into(),
        timestamp: 0,
        attachment: crate::demo::demo_attachment(),
    };
    let held = |s: &State| {
        s.accounts.get(&ada()).unwrap().hides_listed_media(
            DEMO_THREAD.0,
            "$listed",
            &crate::demo::demo_attachment(),
            &s.revealed_media,
        )
    };
    s.accounts.set_files(&ada(), DEMO_THREAD.0.into(), vec![picture(ADA)]);
    assert!(!held(&s), "the user's own file shows");
    s.accounts.set_files(&ada(), DEMO_THREAD.0.into(), vec![picture("@grace:example.org")]);
    assert!(held(&s), "someone else's waits for a press");
}

#[test]
fn a_held_back_pictures_file_actions_from_its_message_should_do_nothing() {
    let (mut s, mut commands) = state();
    crate::demo::hidden_media(&mut s);
    drain(&mut commands);
    let file = crate::demo::demo_attachment();
    for action in [AttachmentAction::CopyImage, AttachmentAction::Open, AttachmentAction::Download] {
        s.act_on_message_attachment(&ada(), DEMO_THREAD.0, "$attachment", &file, action);
    }
    assert!(drain(&mut commands).is_empty());
    // Shown, they work again.
    s.reveal_media(&ada(), "$attachment");
    drain(&mut commands);
    s.act_on_message_attachment(&ada(), DEMO_THREAD.0, "$attachment", &file, AttachmentAction::CopyImage);
    assert!(drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::FetchMedia { .. })));
}

#[test]
fn an_open_preview_should_hold_back_someone_elses_picture_once_media_shows_nowhere() {
    let (mut s, mut commands) = state();
    crate::demo::attachment_preview(&mut s);
    assert!(s.preview.is_some() && !s.preview_held());
    media_never(&mut s);
    assert!(s.preview_held());
    drain(&mut commands);
    let file = s.preview.as_ref().unwrap().attachment.clone();
    for action in [AttachmentAction::CopyImage, AttachmentAction::Open, AttachmentAction::Download] {
        s.act_on_attachment(ada(), &file, action);
    }
    assert!(drain(&mut commands).is_empty());
}

#[test]
fn saved_and_pinned_messages_of_someone_ignored_should_not_show_but_stay_saved_and_pinned() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    let preview = |event_id: &str, sender: &str| matrix_core::MessagePreview {
        event_id: event_id.into(),
        sender_name: sender.into(),
        sender_id: sender.into(),
        sender_avatar: None,
        timestamp_ms: Some(1),
        body: Some("Hello".into()),
        thread: None,
    };
    let room = s.accounts.get(&ada()).unwrap().room(DEMO_THREAD.0).unwrap().clone();
    let (mut rooms, id) = (vec![room], ada());
    rooms[0].pinned = vec!["$troll".into(), "$grace".into()];
    rooms[0].saved = vec!["$troll".into(), "$grace".into()];
    s.accounts.set_room_list(&id, matrix_core::RoomList { rooms: rooms.clone(), ..Default::default() });
    let previews = vec![preview("$troll", "@troll:spam.example"), preview("$grace", "@grace:example.org")];
    s.accounts.set_pins(&id, DEMO_THREAD.0.into(), previews.clone());
    let saved = previews
        .iter()
        .map(|preview| matrix_core::SavedMessage {
            room_id: DEMO_THREAD.0.into(),
            saved_at_ms: None,
            preview: preview.clone(),
        })
        .collect();
    s.accounts.set_saved_messages(&id, saved);
    let safety = Safety { ignored: vec!["@troll:spam.example".into()], ..Safety::default() };
    s.apply(Event::Safety { account: id.clone(), safety }, &mut Tx::default());
    let data = s.accounts.get(&id).unwrap();
    let shown = |ids: Vec<&str>| ids.into_iter().map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(data.saved_messages().map(|m| m.preview.event_id.clone()).collect::<Vec<_>>(), shown(vec!["$grace"]));
    assert_eq!(
        data.pinned_messages(&rooms[0]).unwrap().map(|m| m.event_id.clone()).collect::<Vec<_>>(),
        shown(vec!["$grace"])
    );
    assert_eq!(data.room(DEMO_THREAD.0).unwrap().pinned.len(), 2, "still pinned");
    assert!(data.is_saved(DEMO_THREAD.0, "$troll"), "still saved");
}

#[test]
fn pinned_messages_of_someone_ignored_should_not_fetch_their_pictures() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    let id = ada();
    let mut room = s.accounts.get(&id).unwrap().room(DEMO_THREAD.0).unwrap().clone();
    room.pinned = vec!["$troll".into(), "$grace".into()];
    let rooms = s
        .accounts
        .get(&id)
        .unwrap()
        .rooms
        .iter()
        .map(|r| if r.room_id == room.room_id { room.clone() } else { r.clone() })
        .collect();
    s.accounts.set_room_list(&id, matrix_core::RoomList { rooms, ..Default::default() });
    s.select_room(DEMO_THREAD.0);
    s.show_side(crate::state::SidePanel::Pins);
    let safety = Safety { ignored: vec!["@troll:spam.example".into()], ..Safety::default() };
    s.apply(Event::Safety { account: id.clone(), safety }, &mut Tx::default());
    s.media = crate::state::MediaCache::default();
    s.media.attach(std::sync::Arc::new(|| {}));
    drain(&mut commands);
    let pin = |event_id: &str, sender: &str, picture: &str| matrix_core::MessagePreview {
        event_id: event_id.into(),
        sender_name: sender.into(),
        sender_id: sender.into(),
        sender_avatar: Some(matrix_core::MediaRef::opaque(picture)),
        timestamp_ms: Some(1),
        body: Some("Hello".into()),
        thread: None,
    };
    let messages =
        vec![pin("$troll", "@troll:spam.example", "troll-face"), pin("$grace", "@grace:example.org", "grace-face")];
    s.apply(Event::PinnedMessages { account: id, room_id: DEMO_THREAD.0.into(), messages }, &mut Tx::default());
    let fetched: Vec<_> = drain(&mut commands)
        .into_iter()
        .filter_map(|(_, command)| match command {
            Command::FetchMedia { media, .. } => Some(media),
            _ => None,
        })
        .collect();
    assert!(fetched.contains(&matrix_core::MediaRef::opaque("grace-face")));
    assert!(!fetched.contains(&matrix_core::MediaRef::opaque("troll-face")));
}

/// Makes the demo's #general public or not.
fn general_public(s: &mut State, public: bool) {
    let data = s.accounts.get(&ada()).unwrap();
    let mut list = matrix_core::RoomList {
        rooms: data.rooms.clone(),
        spaces: data.spaces.clone(),
        invitations: data.invitations.clone(),
    };
    for room in list.rooms.iter_mut().filter(|room| room.room_id == DEMO_THREAD.0) {
        room.public = public;
    }
    s.apply(Event::RoomsUpdated { account: ada(), list }, &mut Tx::default());
}

fn loads_original(commands: &mut DetachedCommands) -> bool {
    drain(commands).iter().any(|(_, command)| matches!(command, Command::FetchMedia { size: MediaSize::Original, .. }))
}

#[test]
fn an_open_preview_should_hold_back_and_show_again_as_its_room_turns_public_and_private() {
    let (mut s, mut commands) = state();
    crate::demo::attachment_preview(&mut s);
    let safety = Safety { media: ShowMedia::Private, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert!(!s.preview_held());
    general_public(&mut s, true);
    assert!(s.preview_held());
    drain(&mut commands);
    general_public(&mut s, false);
    assert!(matches!(s.preview.as_ref().map(|p| &p.content), Some(crate::state::PreviewContent::Image { .. })));
    assert!(loads_original(&mut commands));
}

#[test]
fn a_held_preview_should_load_once_media_shows_everywhere_again() {
    let (mut s, mut commands) = state();
    crate::demo::attachment_preview(&mut s);
    media_never(&mut s);
    assert!(s.preview_held());
    drain(&mut commands);
    s.apply(Event::Safety { account: ada(), safety: Safety::default() }, &mut Tx::default());
    assert!(!s.preview_held());
    assert!(loads_original(&mut commands));
}

#[test]
fn media_in_a_room_with_no_summary_should_count_as_public() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    let safety = Safety { media: ShowMedia::Private, ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    let theirs = Message { sender_id: "@grace:example.org".into(), event_id: "$theirs".into(), ..Message::default() };
    assert!(s.hides_media(&ada(), "!unknown:example.org", &theirs));
}

#[test]
fn a_kept_timeline_should_load_nothing_for_someone_ignored_since() {
    let (mut s, mut commands) = state();
    crate::demo::attachment_message(&mut s);
    s.media = crate::state::MediaCache::default();
    s.media.attach(std::sync::Arc::new(|| {}));
    let safety = Safety { ignored: vec!["@grace:example.org".into()], ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    drain(&mut commands);
    s.request_timeline_media();
    assert!(!drain(&mut commands).iter().any(|(_, command)| matches!(command,
        Command::FetchMedia { media, .. } if *media == matrix_core::MediaRef::opaque("demo-image"))));
}

fn fetches_demo_image(commands: &mut DetachedCommands) -> bool {
    drain(commands).iter().any(|(_, command)| {
        matches!(command,
        Command::FetchMedia { media, .. } if *media == matrix_core::MediaRef::opaque("demo-image"))
    })
}

#[test]
fn media_on_screen_should_load_once_the_choice_allows_it() {
    let (mut s, mut commands) = state();
    crate::demo::attachment_message(&mut s);
    media_never(&mut s);
    s.media = crate::state::MediaCache::default();
    s.media.attach(std::sync::Arc::new(|| {}));
    drain(&mut commands);
    s.apply(Event::Safety { account: ada(), safety: Safety::default() }, &mut Tx::default());
    assert!(fetches_demo_image(&mut commands));
}

#[test]
fn media_on_screen_should_load_once_its_room_turns_private() {
    let (mut s, mut commands) = state();
    crate::demo::hidden_media(&mut s);
    s.media = crate::state::MediaCache::default();
    s.media.attach(std::sync::Arc::new(|| {}));
    drain(&mut commands);
    general_public(&mut s, false);
    assert!(fetches_demo_image(&mut commands));
}

#[test]
fn someone_ignored_and_unignored_with_both_still_saving_should_stay_left_out() {
    let (mut s, mut commands) = state();
    crate::demo::seed(&mut s);
    drain(&mut commands);
    let troll = "@troll:spam.example";
    s.set_safety(&ada(), SafetyChange::Ignore { user_id: troll.into(), ignored: true });
    s.stop_ignoring(&ada(), troll);
    let requests: Vec<_> = drain(&mut commands)
        .into_iter()
        .filter_map(|(request, command)| matches!(command, Command::SetSafety { .. }).then_some(request))
        .collect();
    assert_eq!(requests.len(), 2);
    assert!(s.accounts.get(&ada()).unwrap().ignores(troll), "left out while the ignore may still land");
    // Both saved, the last one wins: no longer ignored.
    s.apply(Event::Safety { account: ada(), safety: Safety::default() }, &mut Tx::default());
    finish(&mut s, requests[0], Ok(()));
    finish(&mut s, requests[1], Ok(()));
    assert!(!s.accounts.get(&ada()).unwrap().ignores(troll));
}

/// Grace's message `body` as the only entry of #general's timeline.
fn grace_says(s: &mut State, event_id: &str, body: &str) {
    let message = Message {
        event_id: event_id.into(),
        sender_id: "@grace:example.org".into(),
        sender_name: "Grace Hopper".into(),
        timestamp_ms: 1,
        content: matrix_core::MessageContent::Text { body: body.into(), html: None },
        ..Message::default()
    };
    let entry =
        matrix_core::TimelineEntry { id: "said".into(), kind: matrix_core::EntryKind::Message(Box::new(message)) };
    s.apply(
        Event::TimelineChanged {
            account: ada(),
            key: TimelineKey::room(DEMO_THREAD.0),
            diffs: vec![matrix_core::TimelineDiff::Reset(vec![entry])],
        },
        &mut Tx::default(),
    );
}

#[test]
fn showing_a_held_back_gif_that_failed_should_fetch_it_again() {
    let (mut s, mut commands) = state();
    crate::demo::hidden_media(&mut s);
    let url = "https://x.org/party.gif";
    grace_says(&mut s, "$gif", url);
    s.gifs.failed(url, std::time::Instant::now());
    drain(&mut commands);
    s.reveal_media(&ada(), "$gif");
    assert!(
        drain(&mut commands)
            .iter()
            .any(|(_, command)| matches!(command, Command::FetchGif { url: asked } if asked == url))
    );
}

#[test]
fn an_open_preview_should_close_once_its_sender_is_ignored() {
    let (mut s, mut commands) = state();
    crate::demo::attachment_preview(&mut s);
    assert!(s.preview.is_some());
    let safety = Safety { ignored: vec!["@grace:example.org".into()], ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert!(s.preview.is_none());
    // Nor does it open again, nor do its file actions run.
    drain(&mut commands);
    let file = crate::demo::demo_attachment();
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), "$attachment", &file);
    assert!(s.preview.is_none());
    for action in [AttachmentAction::CopyImage, AttachmentAction::Open, AttachmentAction::Download] {
        s.act_on_message_attachment(&ada(), DEMO_THREAD.0, "$attachment", &file, action);
    }
    assert!(drain(&mut commands).is_empty());
}

#[test]
fn a_batch_should_leave_out_items_of_someone_ignored() {
    let (mut s, _) = state();
    crate::demo::batch_messages(&mut s);
    let data = s.accounts.get(&ada()).unwrap();
    assert!(!data.ignores_sender_of(DEMO_THREAD.0, "$batch-0"));
    let safety = Safety { ignored: vec!["@grace:example.org".into()], ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert!(s.accounts.get(&ada()).unwrap().ignores_sender_of(DEMO_THREAD.0, "$batch-0"));
    let files = crate::demo::batch_attachments();
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), "$batch-2", &files[2]);
    assert!(s.preview.is_none());
}

#[test]
fn invitations_from_someone_ignored_should_leave_every_list_without_being_declined() {
    let (mut s, mut commands) = state();
    crate::demo::invitation(&mut s);
    drain(&mut commands);
    assert_eq!(s.accounts.get(&ada()).unwrap().shown_invitations().count(), 1);
    // Ignored but not saved yet: gone already.
    s.set_safety(&ada(), SafetyChange::Ignore { user_id: "@grace:example.org".into(), ignored: true });
    let data = s.accounts.get(&ada()).unwrap();
    assert_eq!(data.shown_invitations().count(), 0);
    assert_eq!(data.hidden_invitations().count(), 0, "ignored, not merely a stranger");
    assert_eq!(data.invitations.len(), 1, "still there");
    assert!(!drain(&mut commands).iter().any(|(_, command)| matches!(command, Command::LeaveRoom { .. })));
}

#[test]
fn a_clip_should_stop_once_its_sender_is_ignored() {
    let (mut s, _) = state();
    let clip = matrix_core::Attachment {
        kind: matrix_core::AttachmentKind::Audio,
        name: "hello.ogg".into(),
        size: None,
        media: Some(matrix_core::MediaRef::opaque("hello")),
        thumbnail: None,
        dimensions: None,
        audio: None,
    };
    crate::demo::image_message(&mut s, clip.clone());
    s.toggle_audio(&clip);
    let audio = s.audio.as_ref().unwrap();
    assert!(audio.play, "queued to play while it downloads");
    assert_eq!(audio.sender, Some((ada(), "@grace:example.org".to_owned())));
    let safety = Safety { ignored: vec!["@grace:example.org".into()], ..Safety::default() };
    s.apply(Event::Safety { account: ada(), safety }, &mut Tx::default());
    assert!(!s.audio.as_ref().unwrap().play, "no longer queued");
}
