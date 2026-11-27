use matrix_core::{Answer, Command, DetachedCommands, EngineHandle, Event, RequestId, RoomChange};
use rune::{Env, Runtime};

use crate::demo;
use crate::platform::Shell;
use crate::state::{Modal, Settings, State};
use crate::ui::tests::{press, runtime};

/// A runtime on `scene` whose commands can be read.
fn sending(scene: fn(&mut State)) -> (Runtime<State>, DetachedCommands) {
    let (engine, mut commands) = EngineHandle::detached();
    let mut state = State::new(engine, Shell::headless(), Settings::fixed_english());
    scene(&mut state);
    let mut rt = Runtime::new(state, crate::ui::view, (1200.0, 760.0), Env::new(true, 1.0));
    rt.poll = crate::spec().poll;
    rt.frame(0.0);
    sent(&mut commands);
    (rt, commands)
}

fn sent(commands: &mut DetachedCommands) -> Vec<(RequestId, Command)> {
    std::iter::from_fn(|| commands.try_next()).collect()
}

/// The widget runs sent: which widget, and whether in the browser.
fn runs(commands: &mut DetachedCommands) -> Vec<(String, bool)> {
    sent(commands)
        .into_iter()
        .filter_map(|(_, command)| match command {
            Command::RunWidget { widget_id, browser, .. } => Some((widget_id, browser)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_video_button_should_join_the_meeting_where_calls_dont_work() {
    let (mut rt, mut commands) = sending(demo::meeting_without_calls);
    press(&mut rt, crate::ui::call::VIDEO);
    assert_eq!(runs(&mut commands), [("standup".to_owned(), true)]);
}

#[test]
fn the_meetings_join_link_should_open_in_the_browser() {
    let (mut rt, mut commands) = sending(demo::meeting_without_calls);
    press(&mut rt, crate::ui::call::VIDEO);
    let (request, _) = sent(&mut commands).pop().expect("a run");
    let url = "https://meet.example.org/TeamStandup#jwt".to_owned();
    rt.update(|s, tx| s.apply(Event::OpenUrl { request, url: url.clone() }, tx));
    assert_eq!(rt.take_url_requests(), [url]);
}

#[test]
fn the_video_button_should_start_a_video_call_where_calls_work() {
    let (mut rt, mut commands) = sending(demo::meeting);
    press(&mut rt, crate::ui::call::VIDEO);
    assert!(sent(&mut commands).iter().any(|(_, command)| matches!(command, Command::StartCall { video: true, .. })));
}

#[test]
fn the_chevron_should_list_the_other_ways_to_meet() {
    let mut rt = runtime(demo::meeting);
    press(&mut rt, crate::ui::call::VIDEO_OPTIONS);
    let listed =
        ["video-option-call", "video-option-join-standup", "video-option-start"].map(|key| rt.find(key).is_some());
    assert_eq!(listed, [false, true, true]);
}

#[test]
fn a_meeting_in_the_menu_should_join_in_the_browser() {
    let (mut rt, mut commands) = sending(demo::video_options);
    press(&mut rt, "video-option-join-standup");
    assert_eq!(runs(&mut commands), [("standup".to_owned(), true)]);
}

#[test]
fn pressing_a_widget_should_open_it_in_its_window() {
    let (mut rt, mut commands) = sending(demo::widgets);
    press(&mut rt, "widget-board");
    assert_eq!(runs(&mut commands), [("board".to_owned(), false)]);
}

#[test]
fn pressing_a_meeting_in_the_panel_should_open_it_in_the_browser() {
    let (mut rt, mut commands) = sending(demo::widgets);
    press(&mut rt, "widget-standup");
    assert_eq!(runs(&mut commands), [("standup".to_owned(), true)]);
}

#[test]
fn open_in_browser_should_open_a_widget_in_the_browser() {
    let (mut rt, mut commands) = sending(demo::widgets);
    press(&mut rt, "widget-open-board");
    assert_eq!(runs(&mut commands), [("board".to_owned(), true)]);
}

/// The widgets panel with its menu open.
fn menu_open(s: &mut State) {
    demo::widgets(s);
    s.toggle_widgets_menu();
}

#[test]
fn the_panels_plus_should_open_its_menu() {
    let mut rt = runtime(demo::widgets);
    press(&mut rt, crate::ui::developer::WIDGETS_MENU);
    assert!(rt.find("widgets-menu").is_some());
}

#[test]
fn the_panel_should_open_the_integration_manager() {
    let (mut rt, mut commands) = sending(demo::widgets);
    press(&mut rt, "widget-integrations");
    assert!(sent(&mut commands).iter().any(|(_, command)| matches!(command, Command::OpenIntegrations { .. })));
}

#[test]
fn the_panel_should_start_a_meeting() {
    let (mut rt, mut commands) = sending(menu_open);
    press(&mut rt, "widget-start-meeting");
    assert!(sent(&mut commands).iter().any(|(_, command)| matches!(command, Command::StartMeeting { .. })));
}

#[test]
fn allowing_should_grant_only_the_permission_ticked() {
    let (mut rt, mut commands) = sending(demo::widget_permissions);
    let id = shown_id(&rt);
    press(&mut rt, &format!("widget-permission-{id}-0"));
    press(&mut rt, &format!("widget-permissions-allow-{id}"));
    let granted = sent(&mut commands).into_iter().find_map(|(_, command)| match command {
        Command::Answer { answer: Answer::WidgetPermissions { granted, .. }, .. } => Some(granted),
        _ => None,
    });
    assert_eq!(granted, Some(vec!["org.matrix.msc2762.receive.event:m.room.message".to_owned()]));
}

#[test]
fn permissions_should_read_in_words() {
    let rt = runtime(demo::widget_permissions);
    let tree = rt.a11y_json();
    assert!(tree.contains("Send m.room.message events as you") && tree.contains("See the room's m.room.member state"));
}

#[test]
fn agreeing_to_the_terms_should_open_the_integration_manager() {
    let (mut rt, mut commands) = sending(demo::integration_terms);
    let key = format!("integration-terms-agree-{}", shown_id(&rt));
    press(&mut rt, &key);
    let answers: Vec<Answer> = sent(&mut commands)
        .into_iter()
        .filter_map(|(_, command)| match command {
            Command::Answer { answer, .. } => Some(answer),
            _ => None,
        })
        .collect();
    assert_eq!((answers, rt.state.modal.is_none()), (vec![Answer::Agree], true));
}

#[test]
fn cancelling_the_terms_should_cancel_the_integration_manager() {
    let (mut rt, mut commands) = sending(demo::integration_terms);
    let key = format!("integration-terms-cancel-{}", shown_id(&rt));
    press(&mut rt, &key);
    assert!(sent(&mut commands).iter().any(|(_, command)| matches!(command, Command::Cancel(_))));
}

#[test]
fn a_policy_should_open_in_the_browser() {
    let mut rt = runtime(demo::integration_terms);
    let key = format!("integration-policy-{}-1", shown_id(&rt));
    press(&mut rt, &key);
    assert_eq!(rt.take_url_requests(), ["https://integrations.example.org/privacy"]);
}

#[test]
fn a_started_meeting_should_read_as_one_in_the_timeline() {
    let rt = runtime(demo::meeting);
    assert!(rt.a11y_json().contains("Grace Hopper started a meeting"));
}

/// The general room's timeline ending with Grace's `change`, room changes shown in it (widgets
/// pinned or taken off are otherwise left to the room log).
fn ending_with(s: &mut State, change: RoomChange) {
    demo::meeting(s);
    s.set_show_room_changes(true);
    let account = s.nav.account.clone().expect("an account");
    let key = matrix_core::TimelineKey::room(demo::DEMO_THREAD.0);
    let entry = matrix_core::TimelineEntry {
        id: "ev-widget".into(),
        kind: matrix_core::EntryKind::RoomEvent(Box::new(matrix_core::RoomEvent {
            sender: matrix_core::Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None },
            change,
            timestamp_ms: 0,
        })),
    };
    s.apply(
        Event::TimelineChanged { account, key, diffs: vec![matrix_core::TimelineDiff::PushBack(entry)] },
        &mut rune::Tx::default(),
    );
}

#[test]
fn an_added_widget_should_read_with_its_name_in_the_timeline() {
    let rt = runtime(|s| ending_with(s, RoomChange::WidgetAdded { name: "Sprint board".into(), meeting: None }));
    assert!(rt.a11y_json().contains("Grace Hopper added the widget Sprint board"));
}

#[test]
fn a_removed_widget_should_read_as_one_in_the_timeline() {
    let rt = runtime(|s| ending_with(s, RoomChange::WidgetRemoved));
    assert!(rt.a11y_json().contains("Grace Hopper removed a widget"));
}

#[test]
fn the_permissions_dialog_should_show_over_the_room() {
    let rt = runtime(demo::widget_permissions);
    assert!(
        matches!(rt.state.modal, Some(Modal::WidgetPrompt(_)))
            && rt.find(&format!("widget-permissions-{}", shown_id(&rt))).is_some()
    );
}

/// The general room with its widgets known, shown without a side panel.
fn room_with_widgets(s: &mut State) {
    demo::widgets(s);
    s.side = None;
}

#[test]
fn a_room_should_open_integrations_from_room_info() {
    let mut rt = runtime(room_with_widgets);
    press(&mut rt, crate::ui::people::INFO);
    press(&mut rt, "info-widgets");
    assert_eq!(rt.state.side, Some(crate::state::SidePanel::Widgets));
    assert!(rt.find("widgets-panel").is_some());
}

#[test]
fn a_room_with_widgets_should_have_no_dedicated_header_button() {
    let rt = runtime(room_with_widgets);
    assert!(rt.find("room-header").is_some());
    assert!(rt.find("room-widgets").is_none());
}

#[test]
fn a_rooms_widget_count_should_be_known_once_its_widgets_are() {
    let rt = runtime(room_with_widgets);
    let account = rt.state.active_account().expect("account");
    let room = rt.state.active_room_id().and_then(|id| account.room(id)).expect("room");
    assert_eq!(crate::ui::room::widget_count(account, room), Some(4));
}

#[test]
fn asking_who_the_user_is_should_read_in_words() {
    let permission = matrix_core::WidgetPermission {
        capability: "io.bettermatrix.identity".into(),
        kind: matrix_core::PermissionKind::Identity,
        subject: Some("https://jitsi.example".into()),
        disclosed: None,
    };
    assert_eq!(super::permission_text(&permission), "Confirm your Matrix ID to https://jitsi.example");
}

/// The demo's general room, with the homeserver offering meetings or not.
fn meetings_offered(s: &mut State, offered: bool) {
    let account = s.nav.account.clone().expect("an account");
    let mut rooms = s.active_account().map(|data| data.rooms.clone()).unwrap_or_default();
    rooms.iter_mut().for_each(|room| room.can_start_meeting = offered);
    s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, ..Default::default() });
}

/// The demo's general room, its calls working or not.
fn calls_work(s: &mut State, work: bool) {
    let account = s.nav.account.clone().expect("an account");
    let mut rooms = s.active_account().map(|data| data.rooms.clone()).unwrap_or_default();
    let via = rooms.iter().find_map(|room| room.call.via);
    rooms.iter_mut().for_each(|room| room.call.via = if work { via } else { None });
    s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, ..Default::default() });
}

#[test]
fn a_room_with_one_way_to_meet_should_show_no_chevron() {
    let rt = runtime(|s| {
        meetings_offered(s, false);
        s.select_room(demo::DEMO_THREAD.0);
    });
    assert!(rt.find(crate::ui::call::VIDEO).is_some() && rt.find(crate::ui::call::VIDEO_OPTIONS).is_none());
}

#[test]
fn a_room_with_more_ways_to_meet_should_show_the_chevron() {
    let rt = runtime(demo::meeting);
    assert!(rt.find(crate::ui::call::VIDEO_OPTIONS).is_some());
}

#[test]
fn a_room_with_no_way_to_meet_should_show_no_video_button() {
    let rt = runtime(|s| {
        meetings_offered(s, false);
        calls_work(s, false);
        s.select_room(demo::DEMO_THREAD.0);
    });
    assert!(rt.find(crate::ui::call::VIDEO).is_none());
}

#[test]
fn the_video_button_should_start_a_meeting_where_only_that_is_offered() {
    let (mut rt, mut commands) = sending(|s| {
        demo::seed(s);
        calls_work(s, false);
        s.select_room(demo::DEMO_THREAD.0);
    });
    press(&mut rt, crate::ui::call::VIDEO);
    assert!(sent(&mut commands).iter().any(|(_, command)| matches!(
        command,
        Command::StartMeeting { room_id, video: true, browser: true, .. } if room_id == demo::DEMO_THREAD.0
    )));
}

#[test]
fn the_voice_button_should_stay_where_calls_work() {
    let rt = runtime(demo::meeting);
    assert!(rt.find("call-start").is_some());
}

#[test]
fn the_room_menu_should_start_a_meeting_in_its_room() {
    let (mut rt, mut commands) = sending(|s| {
        demo::seed(s);
        s.toggle_room_menu("!lab:example.org");
    });
    press(&mut rt, "room-menu-start-meeting");
    assert!(sent(&mut commands).iter().any(|(_, command)| matches!(
        command,
        Command::StartMeeting { room_id, browser: true, .. } if room_id == "!lab:example.org"
    )));
}

#[test]
fn the_room_menu_should_offer_no_meeting_where_none_is_offered() {
    let rt = runtime(|s| {
        meetings_offered(s, false);
        s.toggle_room_menu("!lab:example.org");
    });
    assert!(rt.find("room-menu").is_some() && rt.find("room-menu-start-meeting").is_none());
}

#[test]
fn the_panels_meetings_should_come_before_the_other_widgets() {
    let rt = runtime(demo::widgets);
    let meeting = rt.presented("widget-standup").expect("the meeting").0;
    let other = rt.presented("widget-board").expect("the board").0;
    assert!(meeting[1] < other[1]);
}

#[test]
fn the_panels_join_should_open_the_meeting_in_the_browser() {
    let (mut rt, mut commands) = sending(demo::widgets);
    press(&mut rt, "widget-join-review");
    assert_eq!(runs(&mut commands), [("review".to_owned(), true)]);
}

#[test]
fn the_panel_should_not_start_meetings_where_none_are_offered() {
    let rt = runtime(|s| {
        menu_open(s);
        meetings_offered(s, false);
    });
    assert!(rt.find("widget-start-meeting").is_none() && rt.find("widget-integrations").is_some());
}

/// The panel with no widget listed, which the user may or may not add.
fn none_listed(s: &mut State, can_add: bool) {
    demo::widgets(s);
    let account = s.nav.account.clone().expect("an account");
    s.accounts.set_widgets(
        &account,
        demo::DEMO_THREAD.0.into(),
        matrix_core::Widgets { can_add, ..Default::default() },
    );
}

#[test]
fn the_panel_should_not_add_widgets_where_the_user_may_not() {
    let rt = runtime(|s| {
        none_listed(s, false);
        s.toggle_widgets_menu();
    });
    assert!(rt.find("widget-add").is_none() && rt.find("widget-integrations").is_some());
}

/// The element key part of the widget question showing.
fn shown_id(rt: &Runtime<State>) -> String {
    rt.state.widget_prompt().map(|prompt| prompt.key().id()).expect("a question showing")
}

#[test]
fn successive_questions_should_show_under_different_keys() {
    let mut rt = runtime(demo::widget_permissions);
    let first = format!("widget-permissions-allow-{}", shown_id(&rt));
    let request = rt.state.widget_prompt().map(|prompt| prompt.request()).expect("a question showing");
    let permission = matrix_core::WidgetPermission {
        capability: "io.element.requires_client".into(),
        kind: matrix_core::PermissionKind::StayInApp,
        subject: None,
        disclosed: None,
    };
    rt.update(|s, tx| {
        s.apply(
            Event::WidgetPermissions { request, question: 1, widget: "Sprint board".into(), permissions: Vec::new() },
            tx,
        );
        s.apply(
            Event::WidgetPermissions {
                request,
                question: 2,
                widget: "Sprint board".into(),
                permissions: vec![permission],
            },
            tx,
        );
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.find(&first).is_none() && rt.find(&format!("widget-permissions-allow-{}", shown_id(&rt))).is_some());
}

#[test]
fn a_room_without_widgets_should_say_so() {
    let rt = runtime(|s| none_listed(s, true));
    assert!(rt.find("widgets-none").is_some() && rt.a11y_json().contains("0 widgets"));
}

#[test]
fn downloading_files_should_read_as_any_file_the_account_can_open() {
    let permission = matrix_core::WidgetPermission {
        capability: "org.matrix.msc4039.download_file".into(),
        kind: matrix_core::PermissionKind::DownloadFiles,
        subject: None,
        disclosed: None,
    };
    assert_eq!(super::permission_text(&permission), "Download files your account can open");
}

#[test]
fn a_widget_the_engine_sends_to_the_browser_should_open_there_without_a_window() {
    let (mut rt, mut commands) = sending(demo::widgets);
    press(&mut rt, "widget-board");
    let (request, _) = sent(&mut commands).pop().expect("a run");
    let url = "http://board.example.org/team".to_owned();
    rt.update(|s, tx| s.apply(Event::OpenUrl { request, url: url.clone() }, tx));
    assert_eq!((rt.take_url_requests(), rt.state.widget_window_calls().is_empty()), (vec![url], true));
}
