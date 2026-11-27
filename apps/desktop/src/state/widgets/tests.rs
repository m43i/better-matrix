use matrix_core::{
    Answer, Command, DetachedCommands, EngineHandle, Event, PermissionKind, Policy, RequestId, WidgetBridge,
    WidgetPage, WidgetPermission,
};
use rune::Tx;

use crate::demo;
use crate::platform::Shell;
use crate::platform::widget_window::{Bridge, WindowCall};
use crate::state::{Modal, PromptKey, Settings, State, VideoOption, WidgetPrompt};

/// The demo's general room with its widgets panel, and what it sends.
fn state() -> (State, DetachedCommands) {
    let (engine, mut commands) = EngineHandle::detached();
    let mut s = State::new(engine, Shell::headless(), Settings::fixed_english());
    demo::widgets(&mut s);
    sent(&mut commands);
    (s, commands)
}

fn apply(s: &mut State, event: Event) {
    s.apply(event, &mut Tx::default());
}

fn sent(commands: &mut DetachedCommands) -> Vec<Command> {
    std::iter::from_fn(|| commands.try_next()).map(|(_, command)| command).collect()
}

fn page(title: &str, bridge: WidgetBridge) -> WidgetPage {
    WidgetPage { url: "https://board.example.org/team".into(), title: title.into(), bridge }
}

/// The sprint board running in its window.
fn board_open(s: &mut State) -> RequestId {
    let request = s.run_widget("board", false).expect("a room shows");
    apply(s, Event::WidgetPage { request, page: page("Sprint board", WidgetBridge::WidgetApi) });
    request
}

fn permission(capability: &str) -> WidgetPermission {
    WidgetPermission { capability: capability.into(), kind: PermissionKind::Other, subject: None, disclosed: None }
}

/// The demo's general room with its Jitsi meeting pinned, and what it sends.
fn meeting() -> (State, DetachedCommands) {
    let (engine, mut commands) = EngineHandle::detached();
    let mut s = State::new(engine, Shell::headless(), Settings::fixed_english());
    demo::meeting(&mut s);
    sent(&mut commands);
    (s, commands)
}

/// Joining the demo's Jitsi meeting.
fn join_standup() -> VideoOption {
    VideoOption::Join { widget_id: "standup".into(), name: "Standup".into() }
}

#[test]
fn a_room_with_calls_and_a_meeting_should_offer_the_call_first() {
    let (s, _commands) = meeting();
    assert_eq!(s.video_options(), [VideoOption::Call, join_standup(), VideoOption::Start]);
}

#[test]
fn a_room_without_calls_should_offer_its_meeting_first() {
    let (mut s, _commands) = meeting();
    s.accounts.set_room_list(&s.nav.account.clone().unwrap(), {
        let mut rooms = s.active_account().map(|data| data.rooms.clone()).unwrap_or_default();
        rooms.iter_mut().for_each(|room| room.call.via = None);
        matrix_core::RoomList { rooms, ..Default::default() }
    });
    assert_eq!(s.video_options(), [join_standup(), VideoOption::Start]);
}

#[test]
fn the_video_button_should_start_a_video_call_where_calls_work() {
    let (mut s, mut commands) = meeting();
    s.start_video();
    assert!(matches!(sent(&mut commands).as_slice(), [Command::StartCall { video: true, .. }]));
}

#[test]
fn choosing_a_meeting_should_join_it_in_the_browser() {
    let (mut s, mut commands) = meeting();
    s.choose_video_option(&join_standup());
    assert!(sent(&mut commands).iter().any(|command| matches!(
        command,
        Command::RunWidget { widget_id, browser: true, .. } if widget_id == "standup"
    )));
}

#[test]
fn choosing_to_start_a_meeting_should_start_one_in_the_browser() {
    let (mut s, mut commands) = meeting();
    s.choose_video_option(&VideoOption::Start);
    assert!(matches!(sent(&mut commands).as_slice(), [Command::StartMeeting { video: true, browser: true, .. }]));
}

#[test]
fn only_meetings_should_count_as_the_rooms_meetings() {
    let (s, _commands) = state();
    let names: Vec<&str> = s.meetings_here().iter().map(|widget| widget.name.as_str()).collect();
    assert_eq!(names, ["Design review", "Standup"]);
}

#[test]
fn starting_a_meeting_should_start_a_video_meeting_in_the_browser() {
    let (mut s, mut commands) = state();
    s.start_meeting();
    assert!(
        sent(&mut commands)
            .iter()
            .any(|command| matches!(command, Command::StartMeeting { video: true, browser: true, .. }))
    );
}

#[test]
fn a_widget_page_should_open_in_a_window_of_its_own() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    assert_eq!(
        s.widget_window_calls(),
        [WindowCall::Open {
            request,
            url: "https://board.example.org/team".into(),
            title: "Sprint board".into(),
            bridge: Bridge::WidgetApi
        }]
    );
}

#[test]
fn the_integration_managers_window_should_be_titled_integrations() {
    let (mut s, _commands) = state();
    let request = s.open_integrations().expect("a room shows");
    apply(&mut s, Event::WidgetPage { request, page: page("", WidgetBridge::Integrations) });
    assert!(matches!(
        s.widget_window_calls(),
        [WindowCall::Open { title, bridge: Bridge::Integrations, .. }] if title == "Integrations"
    ));
}

#[test]
fn a_message_for_the_page_should_be_posted_into_its_window() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    apply(&mut s, Event::WidgetMessage { request, message: "{\"api\":\"toWidget\"}".into() });
    assert_eq!(
        s.widget_window_calls().last(),
        Some(&WindowCall::Post { request, message: "{\"api\":\"toWidget\"}".into() })
    );
}

#[test]
fn a_message_from_the_page_should_reach_the_engine() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    sent(&mut commands);
    s.widget_posted(request, "{\"api\":\"fromWidget\"}".into());
    assert_eq!(
        sent(&mut commands),
        [Command::Answer { request, answer: Answer::WidgetMessage("{\"api\":\"fromWidget\"}".into()) }]
    );
}

#[test]
fn closing_the_window_should_cancel_the_run() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    sent(&mut commands);
    s.widget_window_closed(request);
    assert_eq!(sent(&mut commands), [Command::Cancel(request)]);
}

#[test]
fn a_run_cancelled_by_closing_its_window_should_finish_without_a_notice() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    s.widget_window_closed(request);
    apply(&mut s, Event::Finished { request, result: Err("cancelled".into()) });
    assert_eq!(s.notice, None);
}

#[test]
fn a_window_the_app_closed_should_cancel_nothing_when_it_goes() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    sent(&mut commands);
    s.widget_window_closed(request);
    assert_eq!(sent(&mut commands), []);
}

#[test]
fn a_run_the_engine_finished_should_close_its_window() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert_eq!(s.widget_window_calls().last(), Some(&WindowCall::Close(request)));
}

#[test]
fn a_run_that_failed_should_say_why() {
    let (mut s, _commands) = state();
    let request = s.run_widget("board", false).expect("a room shows");
    apply(&mut s, Event::Finished { request, result: Err("the widget is gone".into()) });
    assert!(s.notice.as_deref().is_some_and(|notice| notice.contains("the widget is gone")));
}

#[test]
fn a_window_that_couldnt_open_should_cancel_the_run_and_say_why() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    sent(&mut commands);
    s.widget_window_failed(request);
    assert_eq!(sent(&mut commands), [Command::Cancel(request)]);
    assert!(s.notice.is_some());
}

#[test]
fn a_refused_browser_handoff_should_offer_to_retry_the_same_widget() {
    let (mut s, mut commands) = state();
    let request = s.run_widget("board", true).unwrap();
    let first = sent(&mut commands);
    apply(&mut s, Event::OpenUrl { request, url: "file:///example".into() });
    apply(&mut s, Event::Finished { request, result: Ok(()) });

    s.retry_widget();
    assert_eq!(sent(&mut commands), first);
}

#[test]
fn a_page_for_a_closed_run_should_open_no_window() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    s.widget_window_closed(request);
    apply(&mut s, Event::WidgetPage { request, page: page("Sprint board", WidgetBridge::WidgetApi) });
    assert_eq!(s.widget_window_calls().len(), 1);
}

/// The question showing, which a press on its dialog names.
fn showing(s: &State) -> PromptKey {
    s.widget_prompt().map(WidgetPrompt::key).expect("a question showing")
}

/// The number of the board's question within its run.
const QUESTION: u64 = 7;

/// The board asking for three permissions, as question [`QUESTION`].
fn asked(s: &mut State) -> RequestId {
    let request = board_open(s);
    let permissions = vec![permission("one"), permission("two"), permission("three")];
    apply(s, Event::WidgetPermissions { request, question: QUESTION, widget: "Sprint board".into(), permissions });
    request
}

#[test]
fn asked_permissions_should_all_be_off_at_first() {
    let (mut s, _commands) = state();
    asked(&mut s);
    assert!(
        matches!(s.widget_prompt(), Some(WidgetPrompt::Permissions(asked)) if asked.granted == [false, false, false])
    );
}

#[test]
fn allowing_should_grant_only_the_ticked_permissions() {
    let (mut s, mut commands) = state();
    let request = asked(&mut s);
    s.toggle_widget_permission(showing(&s), 1);
    sent(&mut commands);
    s.answer_widget_permissions(showing(&s), true);
    assert_eq!(
        sent(&mut commands),
        [Command::Answer {
            request,
            answer: Answer::WidgetPermissions { question: QUESTION, granted: vec!["two".into()] }
        }]
    );
}

#[test]
fn denying_should_grant_no_permission() {
    let (mut s, mut commands) = state();
    let request = asked(&mut s);
    sent(&mut commands);
    s.answer_widget_permissions(showing(&s), false);
    assert_eq!(
        sent(&mut commands),
        [Command::Answer { request, answer: Answer::WidgetPermissions { question: QUESTION, granted: Vec::new() } }]
    );
}

#[test]
fn closing_the_permissions_dialog_should_grant_no_permission() {
    let (mut s, mut commands) = state();
    let request = asked(&mut s);
    sent(&mut commands);
    s.close_modal();
    assert_eq!(
        sent(&mut commands),
        [Command::Answer { request, answer: Answer::WidgetPermissions { question: QUESTION, granted: Vec::new() } }]
    );
}

#[test]
fn a_run_that_ended_should_close_its_permissions_dialog() {
    let (mut s, _commands) = state();
    let request = asked(&mut s);
    apply(&mut s, Event::Finished { request, result: Ok(()) });
    assert_eq!(s.modal, None);
}

/// The integration manager asking to agree to its terms.
fn terms(s: &mut State) -> RequestId {
    let request = s.open_integrations().expect("a room shows");
    let policies = vec![Policy { name: "Terms".into(), url: "https://integrations.example.org/terms".into() }];
    apply(s, Event::IntegrationTerms { request, policies });
    request
}

#[test]
fn agreeing_to_the_terms_should_answer_agree() {
    let (mut s, mut commands) = state();
    let request = terms(&mut s);
    sent(&mut commands);
    s.agree_to_integration_terms(showing(&s));
    assert_eq!(
        (sent(&mut commands), s.modal.clone()),
        (vec![Command::Answer { request, answer: Answer::Agree }], None)
    );
}

#[test]
fn cancelling_the_terms_should_cancel_the_integration_manager() {
    let (mut s, mut commands) = state();
    let request = terms(&mut s);
    sent(&mut commands);
    s.decline_integration_terms(showing(&s));
    assert_eq!((sent(&mut commands), s.modal.clone()), (vec![Command::Cancel(request)], None));
}

#[test]
fn closing_the_terms_dialog_should_cancel_the_integration_manager() {
    let (mut s, mut commands) = state();
    let request = terms(&mut s);
    sent(&mut commands);
    s.close_modal();
    assert_eq!(sent(&mut commands), [Command::Cancel(request)]);
}

#[test]
fn the_video_options_should_close_once_only_the_default_is_left() {
    let (mut s, _commands) = meeting();
    s.toggle_video_options();
    let account = s.nav.account.clone().expect("an account");
    let mut rooms = s.active_account().map(|data| data.rooms.clone()).unwrap_or_default();
    rooms.iter_mut().for_each(|room| room.can_start_meeting = false);
    s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, ..Default::default() });
    let mut widgets = s.active_account().and_then(|data| data.widgets.get(demo::DEMO_THREAD.0)).cloned().unwrap();
    widgets.widgets.retain(|widget| widget.meeting.is_none());
    apply(&mut s, Event::Widgets { account, room_id: demo::DEMO_THREAD.0.into(), widgets });
    assert_eq!(s.popover, None);
}

#[test]
fn a_loaded_page_should_tell_the_engine() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    sent(&mut commands);
    s.widget_loaded(request);
    assert_eq!(sent(&mut commands), [Command::Answer { request, answer: Answer::WidgetLoaded }]);
}

#[test]
fn a_page_starting_to_load_should_tell_the_engine() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    sent(&mut commands);
    s.widget_loading(request);
    assert_eq!(sent(&mut commands), [Command::Answer { request, answer: Answer::WidgetLoading }]);
}

#[test]
fn a_page_loading_for_a_closed_run_should_tell_nothing() {
    let (mut s, mut commands) = state();
    let request = board_open(&mut s);
    s.widget_window_closed(request);
    sent(&mut commands);
    s.widget_loaded(request);
    assert_eq!(sent(&mut commands), []);
}

#[test]
fn a_meeting_opening_in_the_browser_should_ask_its_permissions_too() {
    let (mut s, _commands) = state();
    let request = s.run_widget("standup", true).expect("a room shows");
    let permissions = vec![permission("io.bettermatrix.identity")];
    apply(&mut s, Event::WidgetPermissions { request, question: 1, widget: "Standup".into(), permissions });
    assert!(matches!(s.widget_prompt(), Some(WidgetPrompt::Permissions(asked)) if asked.request == request));
}

/// Two runs, the board's and the integration manager's, each asking something.
fn two_asking(s: &mut State) -> (RequestId, RequestId) {
    let first = asked(s);
    let second = terms(s);
    (first, second)
}

#[test]
fn a_second_runs_question_should_wait_behind_the_first() {
    let (mut s, _commands) = state();
    let (first, _) = two_asking(&mut s);
    assert!(matches!(s.widget_prompt(), Some(WidgetPrompt::Permissions(asked)) if asked.request == first));
}

#[test]
fn answering_a_question_should_show_the_next_one() {
    let (mut s, _commands) = state();
    let (_, second) = two_asking(&mut s);
    s.answer_widget_permissions(showing(&s), true);
    assert!(matches!(s.widget_prompt(), Some(WidgetPrompt::Terms { request, .. }) if *request == second));
}

#[test]
fn a_run_that_ended_should_show_the_next_question() {
    let (mut s, _commands) = state();
    let (first, second) = two_asking(&mut s);
    apply(&mut s, Event::Finished { request: first, result: Ok(()) });
    assert!(matches!(s.widget_prompt(), Some(WidgetPrompt::Terms { request, .. }) if *request == second));
}

#[test]
fn a_run_that_ended_should_drop_its_waiting_questions() {
    let (mut s, _commands) = state();
    let (_, second) = two_asking(&mut s);
    apply(&mut s, Event::Finished { request: second, result: Ok(()) });
    s.answer_widget_permissions(showing(&s), true);
    assert_eq!(s.modal, None);
}

#[test]
fn a_widgets_question_should_wait_behind_a_new_recovery_key() {
    let (mut s, _commands) = state();
    s.modal = Some(Modal::NewRecoveryKey { key: "EsTc 1234".into() });
    asked(&mut s);
    assert!(matches!(s.modal, Some(Modal::NewRecoveryKey { .. })));
}

#[test]
fn a_widgets_question_should_show_once_the_dialog_before_it_closes() {
    let (mut s, _commands) = state();
    s.modal = Some(Modal::NewRecoveryKey { key: "EsTc 1234".into() });
    let request = asked(&mut s);
    s.close_modal();
    assert!(s.widget_prompt().is_some_and(|prompt| prompt.request() == request));
}

#[test]
fn a_widgets_question_another_dialog_replaced_should_come_back() {
    let (mut s, _commands) = state();
    let request = asked(&mut s);
    s.toggle_widget_permission(showing(&s), 1);
    s.modal = Some(Modal::NewRecoveryKey { key: "EsTc 1234".into() });
    s.modal = None;
    s.poll(&mut Tx::default());
    assert!(matches!(
        s.widget_prompt(),
        Some(WidgetPrompt::Permissions(asked)) if asked.request == request && asked.granted == [false, true, false]
    ));
}

#[test]
fn a_withdrawn_question_should_close_unanswered() {
    let (mut s, mut commands) = state();
    let request = asked(&mut s);
    sent(&mut commands);
    apply(
        &mut s,
        Event::WidgetPermissions {
            request,
            question: QUESTION,
            widget: "Sprint board".into(),
            permissions: Vec::new(),
        },
    );
    assert_eq!((s.widget_prompt().is_none(), sent(&mut commands)), (true, Vec::new()));
}

#[test]
fn a_withdrawn_question_waiting_should_leave_the_queue() {
    let (mut s, _commands) = state();
    terms(&mut s);
    let second = asked(&mut s);
    apply(
        &mut s,
        Event::WidgetPermissions {
            request: second,
            question: QUESTION,
            widget: "Sprint board".into(),
            permissions: Vec::new(),
        },
    );
    s.agree_to_integration_terms(showing(&s));
    assert!(s.widget_prompt().is_none());
}

#[test]
fn an_answer_should_name_the_question_it_answers() {
    let (mut s, mut commands) = state();
    asked(&mut s);
    sent(&mut commands);
    s.answer_widget_permissions(showing(&s), true);
    assert!(matches!(
        sent(&mut commands).as_slice(),
        [Command::Answer { answer: Answer::WidgetPermissions { question: QUESTION, .. }, .. }]
    ));
}

#[test]
fn withdrawing_another_question_should_leave_the_one_showing() {
    let (mut s, _commands) = state();
    let request = asked(&mut s);
    apply(
        &mut s,
        Event::WidgetPermissions {
            request,
            question: QUESTION - 1,
            widget: "Sprint board".into(),
            permissions: Vec::new(),
        },
    );
    assert!(matches!(s.widget_prompt(), Some(WidgetPrompt::Permissions(asked)) if asked.question == QUESTION));
}

#[test]
fn an_answer_for_a_question_no_longer_showing_should_be_dropped() {
    let (mut s, mut commands) = state();
    let request = asked(&mut s);
    let old = showing(&s);
    apply(
        &mut s,
        Event::WidgetPermissions {
            request,
            question: QUESTION,
            widget: "Sprint board".into(),
            permissions: Vec::new(),
        },
    );
    let permissions = vec![permission("four")];
    apply(
        &mut s,
        Event::WidgetPermissions { request, question: QUESTION + 1, widget: "Sprint board".into(), permissions },
    );
    sent(&mut commands);
    s.answer_widget_permissions(old, true);
    assert_eq!(
        (sent(&mut commands), s.widget_prompt().map(WidgetPrompt::key)),
        (Vec::new(), Some(PromptKey { request, question: Some(QUESTION + 1) }))
    );
}

#[test]
fn a_tick_for_a_question_no_longer_showing_should_change_nothing() {
    let (mut s, _commands) = state();
    let request = asked(&mut s);
    let stale = PromptKey { request, question: Some(QUESTION + 1) };
    s.toggle_widget_permission(stale, 0);
    assert!(
        matches!(s.widget_prompt(), Some(WidgetPrompt::Permissions(asked)) if asked.granted == [false, false, false])
    );
}

#[test]
fn agreeing_to_terms_no_longer_showing_should_do_nothing() {
    let (mut s, mut commands) = state();
    let request = terms(&mut s);
    sent(&mut commands);
    s.agree_to_integration_terms(PromptKey { request: RequestId(request.0 + 1000), question: None });
    assert_eq!((sent(&mut commands), s.widget_prompt().is_some()), (Vec::new(), true));
}

#[test]
fn a_page_again_for_a_running_run_should_reload_its_window() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    apply(&mut s, Event::WidgetPage { request, page: page("Sprint board", WidgetBridge::WidgetApi) });
    assert_eq!(
        s.widget_window_calls().last(),
        Some(&WindowCall::Reload { request, url: "https://board.example.org/team".into() })
    );
}

#[test]
fn a_page_again_should_open_no_second_window() {
    let (mut s, _commands) = state();
    let request = board_open(&mut s);
    apply(&mut s, Event::WidgetPage { request, page: page("Sprint board", WidgetBridge::WidgetApi) });
    let opened = s.widget_window_calls().iter().filter(|call| matches!(call, WindowCall::Open { .. })).count();
    assert_eq!(opened, 1);
}
