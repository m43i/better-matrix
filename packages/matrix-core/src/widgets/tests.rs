use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tokio::sync::mpsc;

use crate::engine::{Answer, Event, EventSink, RequestId};
use crate::test_server::{Server, thread_response};

use super::integrations::{After, SENDABLE, answer_request, named_manager, policies, terms_url, widget_content};
use super::meetings::{BigBlueButton, Jitsi, Place, base32, jwt, named_jitsi, place};
use super::session::{
    Grants, Handshake, IDENTITY, PermissionKind, Step, WidgetAccessKind, WidgetPermission, WidgetUrlField, api_url,
    describe, grants_key, identity, manage_permissions, refused, run,
};
use super::*;

const ROOM: &str = "!room:example.org";

async fn synced(server: &Server) -> Account {
    let account = Account::for_test(&server.url()).await;
    account.client().sync_once(matrix_sdk::config::SyncSettings::default()).await.unwrap();
    account
}

fn pinned_widget(content: Value) -> Server {
    Server::new(move |request| {
        if request.contains("/state ") {
            return json!([{
                "type": LEGACY_TYPE, "state_key": "fixture", "sender": "@a:example.org",
                "event_id": "$fixture", "origin_server_ts": 1, "content": content,
            }]);
        }
        thread_response(request)
    })
}

fn viewer() -> Viewer {
    Viewer {
        user_id: "@ada:example.org".into(),
        display_name: "Ada Lovelace".into(),
        avatar_url: String::new(),
        device_id: "DEVICE".into(),
        room_id: "!room:example.org".into(),
        homeserver: "https://example.org".into(),
    }
}

#[test]
fn a_widget_url_should_name_the_viewer_and_room() {
    let url = fill(
        "https://w.example/?u=$matrix_user_id&r=$matrix_room_id&n=$matrix_display_name",
        "w1",
        &json!({}),
        &viewer(),
    );
    assert_eq!(url, "https://w.example/?u=%40ada%3Aexample.org&r=%21room%3Aexample.org&n=Ada%20Lovelace");
}

#[test]
fn a_widget_url_should_take_values_from_the_widgets_data() {
    let url = fill("https://pad.example/p/$padName?w=$matrix_widget_id", "w1", &json!({"padName": "notes"}), &viewer());
    assert_eq!(url, "https://pad.example/p/notes?w=w1");
}

#[test]
fn a_jitsi_widget_should_open_its_meeting_with_the_users_name() {
    let content = json!({
        "type": "jitsi",
        "url": "https://app.element.io/jitsi.html#conferenceDomain=$domain",
        "name": "Standup",
        "data": {"domain": "meet.example.org", "conferenceId": "TeamStandup"}
    });
    let widget = widget("j1", &content, None, &viewer()).expect("widget");
    assert_eq!(
        widget.url,
        "https://meet.example.org/TeamStandup#userInfo.displayName=%22Ada%20Lovelace%22&config.disableDeepLinking=true"
    );
    assert_eq!(widget.host, "meet.example.org");
    assert_eq!(widget.meeting, Some(Meeting::Jitsi));
}

#[test]
fn an_audio_only_jitsi_meeting_should_start_without_video() {
    let jitsi = Jitsi { domain: "m.example".into(), conference: "Call".into(), audio_only: true, auth: false };
    let url = jitsi.url("Ada", None).expect("url");
    assert!(url.contains("config.startWithVideoMuted=true"), "{url}");
}

#[test]
fn a_checking_jitsi_server_should_get_the_jwt_in_the_query() {
    let jitsi = Jitsi { domain: "m.example".into(), conference: "Call".into(), audio_only: false, auth: true };
    let url = jitsi.url("Ada", Some("a.b.c")).expect("url");
    assert!(url.starts_with("https://m.example/Call?jwt=a.b.c#"), "{url}");
}

#[tokio::test]
async fn resetting_jitsi_permissions_should_clear_the_grant_for_the_canonical_origin() {
    let server = pinned_widget(json!({
        "type": "jitsi", "url": "https://app.element.io/jitsi.html", "name": "Jitsi",
        "data": {"domain": "Meet.Example.org:443", "conferenceId": "Call", "auth": "openidtoken-jwt"}
    }));
    let account = synced(&server).await;
    let store = account.client().state_store();
    let origin = "https://meet.example.org";
    let key = grants_key(ROOM, "fixture", origin);
    let grants = json!({
        "asked": [], "granted": [],
        "meeting_identity": { "kind": "Jitsi", "fields": ["DisplayName", "AvatarUrl", "RoomId"], "allowed": true },
    });
    store.set_custom_value_no_read(&key, serde_json::to_vec(&grants).unwrap()).await.unwrap();
    let (answer, answers) = mpsc::unbounded_channel();
    answer.send(Answer::Continue).unwrap();
    let (send, mut events) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |event| send.send(event).unwrap());

    manage_permissions(account.clone(), ROOM.into(), "fixture".into(), RequestId(1), sink, answers).await.unwrap();

    let Event::WidgetAccess { access, .. } = events.try_recv().unwrap() else { panic!("expected permission dialog") };
    assert_eq!(access.origin, origin);
    let permission = WidgetPermission {
        disclosed: Some(vec![WidgetUrlField::DisplayName, WidgetUrlField::AvatarUrl, WidgetUrlField::RoomId]),
        ..identity(origin)
    };
    assert_eq!(access.kind, WidgetAccessKind::Saved(vec![permission]));
    assert!(store.get_custom_value(&key).await.unwrap().is_none());
}

#[test]
fn widgets_in_different_rooms_should_have_distinct_grant_keys() {
    let origin = "https://board.example.org";
    assert_ne!(
        grants_key("!room:example.org:8448", "board", origin),
        grants_key("!room:example.org", "8448:board", origin),
    );
}

#[tokio::test]
async fn a_jitsi_meeting_should_ask_again_if_only_a_legacy_decision_was_saved() {
    let server = pinned_widget(json!({
        "type": "jitsi", "url": "https://app.element.io/jitsi.html", "name": "Jitsi",
        "data": {"domain": "meet.example.org", "conferenceId": "Call", "auth": "openidtoken-jwt"}
    }));
    let account = synced(&server).await;
    let old_key = b"bettermatrix.widget_grants:!room:example.org:fixture:https://meet.example.org";
    let old_decision = json!({ "asked": [IDENTITY], "granted": [] });
    account
        .client()
        .state_store()
        .set_custom_value_no_read(old_key, serde_json::to_vec(&old_decision).unwrap())
        .await
        .unwrap();
    let (answer, answers) = mpsc::unbounded_channel();
    answer.send(Answer::Continue).unwrap();
    drop(answer);
    let (send, mut events) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |event| send.send(event).unwrap());

    let result =
        run(account, ROOM.into(), "fixture".into(), false, Freshness::Server, RequestId(1), sink, answers).await;

    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(matches!(events.try_recv().unwrap(), Event::WidgetAccess { .. }));
    let Event::WidgetPermissions { permissions, .. } = events.try_recv().unwrap() else {
        panic!("expected identity permission");
    };
    assert_eq!(
        permissions,
        [WidgetPermission {
            disclosed: Some(vec![WidgetUrlField::DisplayName, WidgetUrlField::AvatarUrl, WidgetUrlField::RoomId]),
            ..identity("https://meet.example.org")
        }]
    );
}

#[tokio::test]
async fn a_dimension_meeting_should_ask_again_when_the_viewer_adds_a_display_name() {
    let server = pinned_widget(json!({
        "type": "m.custom", "name": "Class",
        "url": "https://dimension.example/widgets/bigbluebutton?meetingId=m1&meetingPassword=pw"
    }));
    let account = synced(&server).await;
    // The account previously used its Matrix ID in place of a display name.
    let prior = json!({
        "asked": [], "granted": [],
        "meeting_identity": { "kind": "Dimension", "fields": ["AvatarUrl"], "allowed": true },
    });
    let key = grants_key(ROOM, "fixture", "https://dimension.example");
    account.client().state_store().set_custom_value_no_read(&key, serde_json::to_vec(&prior).unwrap()).await.unwrap();
    let (answer, answers) = mpsc::unbounded_channel();
    drop(answer);
    let (send, mut events) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |event| send.send(event).unwrap());

    let result =
        run(account, ROOM.into(), "fixture".into(), false, Freshness::Server, RequestId(1), sink, answers).await;

    assert!(matches!(result, Err(Error::Cancelled)));
    let Event::WidgetPermissions { permissions, .. } = events.try_recv().unwrap() else {
        panic!("expected identity permission");
    };
    assert_eq!(
        permissions,
        [WidgetPermission {
            disclosed: Some(vec![WidgetUrlField::DisplayName, WidgetUrlField::AvatarUrl]),
            ..identity("https://dimension.example")
        }]
    );
}

#[tokio::test]
async fn a_meeting_should_ask_for_identity_again_if_only_the_widget_api_was_allowed() {
    let server = pinned_widget(json!({
        "type": "m.custom", "name": "Class",
        "url": "https://dimension.example/widgets/bigbluebutton?meetingId=m1&meetingPassword=pw"
    }));
    let account = synced(&server).await;
    let prior = json!({ "asked": [IDENTITY], "granted": [IDENTITY] });
    let key = grants_key(ROOM, "fixture", "https://dimension.example");
    account.client().state_store().set_custom_value_no_read(&key, serde_json::to_vec(&prior).unwrap()).await.unwrap();
    let (answer, answers) = mpsc::unbounded_channel();
    drop(answer);
    let (send, mut events) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |event| send.send(event).unwrap());

    let result =
        run(account, ROOM.into(), "fixture".into(), false, Freshness::Server, RequestId(1), sink, answers).await;

    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(matches!(events.try_recv().unwrap(), Event::WidgetPermissions { .. }));
}

#[tokio::test]
async fn bigbluebutton_should_ask_before_opening_the_meeting_host() {
    let server = pinned_widget(json!({
        "type": "bigbluebutton", "url": "https://dimension.example/widget",
        "data": {"conferenceUrl": "https://bbb.example.org/b/abc-def"},
    }));
    let (answer, answers) = mpsc::unbounded_channel();
    drop(answer);
    let (send, mut events) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |event| send.send(event).unwrap());
    let account = synced(&server).await;

    let result =
        run(account, ROOM.into(), "fixture".into(), false, Freshness::Server, RequestId(1), sink, answers).await;
    assert!(matches!(result, Err(Error::Cancelled)));
    let Event::WidgetAccess { access, .. } = events.try_recv().unwrap() else { panic!("expected permission dialog") };
    assert_eq!(access.origin, "https://bbb.example.org");
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn the_preload_prompt_should_show_the_widget_id_added_by_the_bridge() {
    let server = pinned_widget(json!({"type": "m.custom", "url": "https://board.example.org/team"}));
    let (answer, answers) = mpsc::unbounded_channel();
    drop(answer);
    let (send, mut events) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |event| send.send(event).unwrap());
    let account = synced(&server).await;

    let _ = run(account, ROOM.into(), "fixture".into(), false, Freshness::Server, RequestId(1), sink, answers).await;
    let Event::WidgetAccess { access, .. } = events.try_recv().unwrap() else { panic!("expected permission dialog") };
    assert_eq!(access.kind, WidgetAccessKind::Open(vec![WidgetUrlField::WidgetId]));
}

#[test]
fn a_jitsi_widget_without_data_should_be_read_from_its_url() {
    let found =
        place("dimension-jitsi", "https://dimension.example/widgets/jitsi?confId=Old&isAudioConf=true", &json!({}));
    let expected = Jitsi { domain: "jitsi.riot.im".into(), conference: "Old".into(), audio_only: true, auth: false };
    assert_eq!(found, Some(Place::Jitsi(expected)));
}

#[test]
fn a_jitsi_widget_should_say_when_its_server_checks_who_joins() {
    let data = json!({"domain": "m.example", "conferenceId": "ABC", "auth": "openidtoken-jwt"});
    let found = place("jitsi", "https://app.element.io/jitsi.html", &data);
    assert!(matches!(found, Some(Place::Jitsi(Jitsi { auth: true, .. }))));
}

#[test]
fn a_jitsi_widget_naming_a_path_as_its_server_should_not_be_a_meeting() {
    let data = json!({"domain": "evil.example/phish?", "conferenceId": "ABC"});
    assert_eq!(place("jitsi", "", &data), None);
}

#[test]
fn a_bigbluebutton_widget_should_open_its_rooms_page() {
    let data = json!({"conferenceUrl": "https://bbb.example.org/b/abc-def"});
    let template = "https://dimension.example/widgets/bigbluebutton?conferenceUrl=$conferenceUrl";
    let content = json!({"type": "bigbluebutton", "url": template, "data": data});
    let widget = widget("b1", &content, None, &viewer()).expect("widget");
    assert_eq!(widget.url, "https://bbb.example.org/b/abc-def");
    assert_eq!(widget.meeting, Some(Meeting::BigBlueButton));
}

#[test]
fn a_meeting_dimension_created_should_be_joined_through_dimension() {
    let template =
        "https://dim.example/widgets/bigbluebutton?widgetId=$matrix_widget_id&meetingId=m1&meetingPassword=pw";
    let found = place("m.custom", template, &json!({"title": "Class"}));
    let expected =
        BigBlueButton::Dimension { api: "https://dim.example".into(), meeting_id: "m1".into(), password: "pw".into() };
    assert_eq!(found, Some(Place::BigBlueButton(expected)));
    let content = json!({"type": "m.custom", "url": template, "name": "Class"});
    let widget = widget("b2", &content, None, &viewer()).expect("widget");
    assert_eq!(widget.meeting, Some(Meeting::BigBlueButton));
    assert!(widget.url.starts_with("https://dim.example/widgets/bigbluebutton?widgetId=b2"), "{}", widget.url);
}

#[test]
fn an_ordinary_custom_widget_should_not_be_a_meeting() {
    let content = json!({"type": "m.custom", "url": "https://board.example/"});
    assert_eq!(widget("w", &content, None, &viewer()).map(|widget| widget.meeting), Some(None));
}

#[test]
fn a_widget_without_a_web_url_should_be_left_out() {
    assert!(widget("w", &json!({"type": "m.custom", "url": "javascript:alert(1)"}), None, &viewer()).is_none());
    assert!(widget("w", &json!({"type": "m.custom"}), None, &viewer()).is_none());
}

#[test]
fn widget_state_should_skip_other_state_and_removed_widgets() {
    let widget =
        json!({"type":"im.vector.modular.widgets","state_key":"w1","sender":"@g:x","content":{"url":"https://w"}});
    let removed = json!({"type":"m.widget","state_key":"w2","sender":"@g:x","content":{}});
    let name = json!({"type":"m.room.name","state_key":"","sender":"@g:x","content":{"name":"Room"}});
    assert_eq!(read_state(&widget).map(|(kind, id, ..)| (kind, id)), Some((LEGACY_TYPE, "w1".to_owned())));
    assert!(read_state(&removed).is_none() && read_state(&name).is_none());
}

#[test]
fn a_nameless_widget_should_go_by_its_kind() {
    let widget = widget("w", &json!({"type": "m.grafana", "url": "https://g.example"}), None, &viewer());
    assert_eq!(widget.map(|widget| widget.name), Some("m.grafana".to_owned()));
}

#[test]
fn the_newer_copy_of_a_state_event_should_count() {
    let old = json!({"type": LEGACY_TYPE, "state_key": "w", "origin_server_ts": 1, "content": {"name": "old"}});
    let new = json!({"type": LEGACY_TYPE, "state_key": "w", "origin_server_ts": 2, "content": {}});
    let other = json!({"type": SPEC_TYPE, "state_key": "w", "origin_server_ts": 1, "content": {"name": "spec"}});
    let kept = newest(vec![old, other.clone()], vec![new.clone()]);
    assert_eq!(kept, vec![new, other]);
}

#[test]
fn the_homeservers_jitsi_server_should_come_from_its_well_known_file() {
    let known = json!({"im.vector.riot.jitsi": {"preferredDomain": "old.example"}, "io.element.jitsi": {"preferredDomain": "jitsi.example"}});
    assert_eq!(named_jitsi(&known), Some("jitsi.example".to_owned()));
    assert_eq!(
        named_jitsi(&json!({"im.vector.riot.jitsi": {"preferredDomain": "old.example"}})),
        Some("old.example".to_owned())
    );
    assert_eq!(named_jitsi(&json!({})), None);
}

#[test]
fn a_checking_servers_conference_should_name_the_room_in_base32() {
    assert_eq!(base32(b"!room:example.org"), "EFZG633NHJSXQYLNOBWGKLTPOJTQ");
}

#[test]
fn a_jitsi_jwt_should_be_signed_as_elements_wrapper_signs_it() {
    assert_eq!(
        jwt(&json!({"a": 1})),
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJhIjoxfQ.Alflk23GbiY_2Q40ZDiBWFc0aXN_gJfDax1-NyKYXHs"
    );
}

#[test]
fn a_widget_permission_should_say_what_it_allows_and_for_what() {
    let send = describe("org.matrix.msc2762.send.event:m.room.message#m.text");
    assert_eq!((send.kind, send.subject.as_deref()), (PermissionKind::SendEvents, Some("m.room.message#m.text")));
    let state = describe("org.matrix.msc2762.receive.state_event:m.room.member");
    assert_eq!((state.kind, state.subject.as_deref()), (PermissionKind::ReadState, Some("m.room.member")));
    assert_eq!(describe("io.element.requires_client").kind, PermissionKind::StayInApp);
    assert_eq!(describe("m.always_on_screen").kind, PermissionKind::Other);
}

#[test]
fn a_widget_asking_for_nothing_new_should_get_what_was_granted_before() {
    let mut grants = Grants::default();
    let asked = vec!["a".to_owned(), "b".to_owned()];
    grants.remember(&asked, &["a".to_owned()]);
    assert_eq!(grants.answer(&["a".to_owned(), "b".to_owned()]), Some(vec!["a".to_owned()]));
    assert_eq!(grants.answer(&["b".to_owned()]), Some(Vec::new()));
}

#[test]
fn a_widget_asking_for_something_new_should_ask_the_user() {
    let mut grants = Grants::default();
    grants.remember(&["a".to_owned()], &["a".to_owned()]);
    assert_eq!(grants.answer(&["a".to_owned(), "c".to_owned()]), None);
}

#[test]
fn a_permission_refused_later_should_no_longer_be_granted() {
    let mut grants = Grants::default();
    grants.remember(&["a".to_owned()], &["a".to_owned()]);
    grants.remember(&["a".to_owned()], &[]);
    assert_eq!(grants.answer(&["a".to_owned()]), Some(Vec::new()));
}

#[test]
fn a_widget_api_page_should_name_the_widget_and_post_to_its_own_origin() {
    let url = api_url("https://board.example:8443/app?x=1&widgetId=old", "w1").expect("url");
    assert_eq!(url, "https://board.example:8443/app?x=1&widgetId=w1&parentUrl=https%3A%2F%2Fboard.example%3A8443");
}

#[test]
fn the_homeservers_integration_manager_should_come_from_its_well_known_file() {
    let known = json!({"m.integrations": {"managers": [{"api_url": "https://dim.example/api/", "ui_url": "https://dim.example/element"}]}});
    let manager = named_manager(&known).expect("manager");
    assert_eq!((manager.api.as_str(), manager.ui.as_str()), ("https://dim.example/api", "https://dim.example/element"));
    assert!(named_manager(&json!({"m.integrations": {"managers": []}})).is_none());
}

#[test]
fn an_integration_manager_with_only_an_api_url_should_open_that_url() {
    let known = json!({"m.integrations": {"managers": [{"api_url": "https://im.example/api/"}]}});
    let manager = named_manager(&known).expect("manager");
    assert_eq!(manager.api, "https://im.example/api");
    assert_eq!(manager.ui, "https://im.example/api/");
}

#[tokio::test]
async fn an_equal_bot_power_level_should_succeed_without_writing_room_state() {
    let wrote_level = Arc::new(AtomicBool::new(false));
    let captured = Arc::clone(&wrote_level);
    let server = Server::new(move |request| {
        if request.contains("/state/m.room.power_levels") {
            if request.starts_with("PUT ") {
                captured.store(true, Ordering::Relaxed);
            }
            return json!({"users": {"@a:example.org": 100, "@bot:example.org": 50}});
        }
        thread_response(request)
    });
    let account = synced(&server).await;
    let room = account.joined_room(ROOM).unwrap();
    let asked = json!({
        "action": "set_bot_power",
        "room_id": ROOM,
        "user_id": "@bot:example.org",
        "level": 50,
        "ignoreIfGreater": true
    });
    assert_eq!(answer_request(&account, &room, &asked).await, (json!({"success": true}), After::Nothing));
    assert!(!wrote_level.load(Ordering::Relaxed));
}

#[tokio::test]
async fn a_manager_should_get_the_full_membership_content_of_a_bot_missing_from_sync() {
    let member = json!({"membership": "join", "displayname": "Release bot", "com.example.service": true});
    let expected = member.clone();
    let server = Server::new(move |request| {
        if request.contains("/state/m.room.member/") {
            return member.clone();
        }
        thread_response(request)
    });
    let account = synced(&server).await;
    let room = account.joined_room(ROOM).unwrap();
    let asked = json!({"action": "membership_state", "room_id": ROOM, "user_id": "@bot:example.org"});

    assert_eq!(answer_request(&account, &room, &asked).await, (expected, After::Nothing));
}

#[tokio::test]
async fn a_manager_should_kick_a_bot_whose_membership_was_not_in_the_sync() {
    let kicked = Arc::new(AtomicBool::new(false));
    let captured = Arc::clone(&kicked);
    let server = Server::new(move |request| {
        if request.contains("/state/m.room.member/") {
            return json!({"membership": "join"});
        }
        if request.starts_with("POST ") && request.contains("/kick") {
            captured.store(request.contains("@bot:example.org"), Ordering::Relaxed);
        }
        thread_response(request)
    });
    let account = synced(&server).await;
    let room = account.joined_room(ROOM).unwrap();
    let asked = json!({"action": "kick", "room_id": ROOM, "user_id": "@bot:example.org"});

    assert_eq!(answer_request(&account, &room, &asked).await, (json!({"success": true}), After::Nothing));
    assert!(kicked.load(Ordering::Relaxed));
}

#[test]
fn an_integration_managers_terms_should_be_read_in_english() {
    let answer = json!({"policies": {"privacy": {
        "version": "1.0",
        "de": {"name": "Datenschutz", "url": "https://im.example/de"},
        "en": {"name": "Privacy", "url": "https://im.example/en"}
    }}});
    let read = policies(&answer);
    assert_eq!(read.len(), 1);
    assert_eq!((read[0].name.as_str(), read[0].url.as_str()), ("Privacy", "https://im.example/en"));
}

#[test]
fn a_manager_setting_a_widget_without_a_url_should_remove_it() {
    assert_eq!(widget_content(&json!({"widget_id": "w", "url": null})).unwrap(), json!({}));
    let set = widget_content(&json!({"widget_id": "w", "type": "m.custom", "url": "https://w.example", "name": "W"}))
        .unwrap();
    assert_eq!(set, json!({"type": "m.custom", "url": "https://w.example", "name": "W", "data": {}}));
}

#[tokio::test]
async fn a_manager_should_update_a_widget_under_its_existing_state_type() {
    let updated = Arc::new(AtomicBool::new(false));
    let captured = Arc::clone(&updated);
    let server = Server::new(move |request| {
        if request.contains("/state ") {
            return json!([{
                "type": SPEC_TYPE, "state_key": "board", "sender": "@a:example.org",
                "event_id": "$board", "origin_server_ts": 1,
                "content": {"type": "m.custom", "url": "https://board.example/"}
            }]);
        }
        if request.starts_with("PUT ") {
            captured.store(request.contains("/state/m.widget/board"), Ordering::Relaxed);
            return json!({"event_id": "$updated"});
        }
        thread_response(request)
    });
    let account = synced(&server).await;
    let room = account.joined_room(ROOM).unwrap();
    let asked = json!({
        "action": "set_widget", "room_id": ROOM, "widget_id": "board",
        "type": "m.custom", "name": "Board", "url": "https://board.example/updated",
    });

    assert_eq!(answer_request(&account, &room, &asked).await, (json!({"success": true}), After::Widgets));
    assert!(updated.load(Ordering::Relaxed));
}

#[tokio::test]
async fn a_manager_should_remove_a_widget_pinned_with_the_standard_state_type() {
    let removed = Arc::new(AtomicBool::new(false));
    let captured = Arc::clone(&removed);
    let server = Server::new(move |request| {
        if request.contains("/state ") {
            return json!([{
                "type": "m.widget", "state_key": "board", "sender": "@a:example.org",
                "event_id": "$board", "origin_server_ts": 1,
                "content": {"type": "m.custom", "url": "https://board.example/"}
            }]);
        }
        if request.starts_with("PUT ") && request.contains("/state/m.widget/board") {
            captured.store(request.ends_with("\r\n\r\n{}"), Ordering::Relaxed);
            return json!({"event_id": "$removed"});
        }
        thread_response(request)
    });
    let account = synced(&server).await;
    let room = account.joined_room(ROOM).unwrap();
    let asked = json!({"action": "set_widget", "room_id": ROOM, "widget_id": "board", "url": null});

    assert_eq!(answer_request(&account, &room, &asked).await, (json!({"success": true}), After::Widgets));
    assert!(removed.load(Ordering::Relaxed));
}

#[test]
fn a_jitsi_widget_pinned_should_read_as_a_meeting_in_the_timeline() {
    let content = json!({"type": "jitsi", "name": "Jitsi", "data": {"domain": "m.example", "conferenceId": "A"}});
    assert_eq!(
        change(&content),
        crate::RoomChange::WidgetAdded { name: "Jitsi".into(), meeting: Some(Meeting::Jitsi) }
    );
}

#[test]
fn a_widget_emptied_should_read_as_removed_in_the_timeline() {
    assert_eq!(change(&json!({})), crate::RoomChange::WidgetRemoved);
}

#[test]
fn an_integration_manager_without_https_should_not_be_used() {
    let known = json!({"m.integrations": {"managers": [{"api_url": "http://im.example/api", "ui_url": "https://im.example/"}]}});
    assert!(named_manager(&known).is_none());
}

#[test]
fn a_managers_terms_should_be_at_its_origin() {
    assert_eq!(
        terms_url("https://scalar.vector.im/api").as_deref(),
        Some("https://scalar.vector.im/_matrix/integrations/v1/terms")
    );
}

#[test]
fn a_manager_should_send_only_integration_state() {
    assert!(SENDABLE.contains(&"m.room.bot.options"));
    assert!(!SENDABLE.contains(&"m.room.power_levels") && !SENDABLE.contains(&"m.room.message"));
}

#[test]
fn asking_who_the_user_is_should_be_a_permission_of_its_own() {
    assert_eq!(describe(IDENTITY).kind, PermissionKind::Identity);
}

#[test]
fn a_refused_request_should_come_back_blocked() {
    let asked = json!({"api": "fromWidget", "requestId": "r1", "action": "get_openid", "data": {}});
    let reply: serde_json::Value = serde_json::from_str(&refused(&asked)).expect("json");
    assert_eq!(reply["response"], json!({"state": "blocked"}));
    assert_eq!(reply["requestId"], "r1");
}

#[test]
fn the_identity_permission_should_name_who_would_learn_it() {
    let asked = identity("https://jitsi.example");
    assert_eq!((asked.kind, asked.subject.as_deref()), (PermissionKind::Identity, Some("https://jitsi.example")));
    assert_eq!(asked.capability, IDENTITY);
}

#[test]
fn a_dimension_meeting_without_https_should_not_be_joined_through_dimension() {
    let template = "http://dim.example/widgets/bigbluebutton?meetingId=m1&meetingPassword=pw";
    assert_eq!(place("m.custom", template, &json!({})), None);
}

fn fallback_question() -> serde_json::Value {
    json!({"api": "toWidget", "requestId": "caps-1", "action": "capabilities", "widgetId": "w", "data": {}})
}

fn fallback_answer() -> serde_json::Value {
    json!({"api": "toWidget", "requestId": "caps-1", "action": "capabilities", "widgetId": "w", "data": {}, "response": {"capabilities": []}})
}

fn announcement() -> serde_json::Value {
    json!({"api": "fromWidget", "requestId": "ready-1", "action": "content_loaded", "widgetId": "w", "data": {}})
}

#[test]
fn a_widget_answering_the_fallback_before_announcing_itself_should_keep_its_driver() {
    let start = std::time::Instant::now();
    let mut handshake = Handshake::default();
    handshake.asked(start);
    handshake.posted(&fallback_question());
    assert_eq!(handshake.heard(&fallback_answer(), "", start + std::time::Duration::from_secs(1)), Step::Nothing);
    assert_eq!(handshake.heard(&announcement(), "ready", start + std::time::Duration::from_secs(2)), Step::Nothing);
}

#[test]
fn a_widget_announcing_itself_before_answering_the_fallback_should_keep_its_driver() {
    let start = std::time::Instant::now();
    let mut handshake = Handshake::default();
    handshake.asked(start);
    handshake.posted(&fallback_question());
    assert_eq!(handshake.heard(&announcement(), "ready", start + std::time::Duration::from_secs(1)), Step::Hold);
    assert_eq!(handshake.heard(&fallback_answer(), "", start + std::time::Duration::from_secs(2)), Step::Nothing);
    assert_eq!(handshake.grace_over(), Step::Nothing);
}

#[test]
fn a_widget_that_never_heard_the_fallback_should_get_a_new_driver_with_its_announcement() {
    let start = std::time::Instant::now();
    let mut handshake = Handshake::default();
    handshake.asked(start);
    handshake.posted(&fallback_question());
    assert_eq!(handshake.heard(&announcement(), "ready", start + std::time::Duration::from_secs(1)), Step::Hold);
    assert_eq!(handshake.grace_over(), Step::Restart("ready".to_owned()));
}

#[test]
fn a_widget_answering_the_fallback_too_late_should_load_again() {
    let start = std::time::Instant::now();
    let mut handshake = Handshake::default();
    handshake.asked(start);
    handshake.posted(&fallback_question());
    assert_eq!(handshake.heard(&fallback_answer(), "", start + std::time::Duration::from_secs(11)), Step::Reload);
}

#[test]
fn an_answer_to_another_request_should_not_count_as_the_fallbacks() {
    let start = std::time::Instant::now();
    let mut handshake = Handshake::default();
    handshake.asked(start);
    handshake.posted(&fallback_question());
    let other = json!({"api": "toWidget", "requestId": "caps-0", "action": "capabilities", "response": {}});
    assert_eq!(handshake.heard(&other, "", start), Step::Nothing);
    assert_eq!(handshake.heard(&announcement(), "ready", start), Step::Hold);
}

#[test]
fn an_answer_to_a_replaced_drivers_question_should_load_the_page_again() {
    let start = std::time::Instant::now();
    let mut handshake = Handshake::default();
    handshake.asked(start);
    handshake.posted(&fallback_question());
    assert_eq!(handshake.heard(&announcement(), "ready", start), Step::Hold);
    assert_eq!(handshake.grace_over(), Step::Restart("ready".to_owned()));
    assert_eq!(handshake.heard(&fallback_answer(), "", start + std::time::Duration::from_secs(3)), Step::Reload);
}

#[test]
fn a_manager_widget_write_should_reject_a_missing_or_invalid_url() {
    for content in [json!({}), json!({"url": 3}), json!({"url": "javascript:noop"})] {
        assert!(widget_content(&content).is_err());
    }
}

#[tokio::test]
async fn a_widget_should_wait_for_confirmation_before_proceeding() {
    use crate::{Answer, Event, RequestId, WidgetAccess, WidgetAccessKind};
    let (sent, mut events) = tokio::sync::mpsc::unbounded_channel();
    let (answer, mut answers) = tokio::sync::mpsc::unbounded_channel();
    let sink = move |event| {
        sent.send(event).unwrap();
    };
    let request = RequestId(1);
    let access = WidgetAccess {
        name: "Board".into(),
        origin: "https://board.example".into(),
        user_id: "@ada:example.org".into(),
        room: "Release".into(),
        kind: WidgetAccessKind::Open(Vec::new()),
    };
    let waiting = super::session::confirm_access(&sink, request, access, &mut answers);
    tokio::pin!(waiting);
    assert!(futures_util::poll!(&mut waiting).is_pending());
    assert!(matches!(events.try_recv(), Ok(Event::WidgetAccess { request: RequestId(1), .. })));
    answer.send(Answer::Agree).unwrap();
    assert!(futures_util::poll!(&mut waiting).is_pending());
    answer.send(Answer::Continue).unwrap();
    assert!(waiting.await.is_ok());
}

#[test]
fn a_zoom_join_link_should_be_a_meeting() {
    let content = json!({"type": "m.custom", "url": "https://example.zoom.us/j/123456789?pwd=example"});
    let shown = widget("zoom", &content, None, &Viewer::default()).expect("widget");
    assert_eq!(shown.meeting, Some(Meeting::Zoom));
    assert_eq!(shown.url, content["url"].as_str().unwrap());
}

#[test]
fn a_zoom_web_app_join_link_should_be_a_meeting() {
    let content = json!({"type": "m.custom", "url": "https://app.zoom.us/wc/123456789/join?pwd=example"});
    let shown = widget("zoom", &content, None, &Viewer::default()).expect("widget");
    assert_eq!(shown.meeting, Some(Meeting::Zoom));
    assert_eq!(shown.url, content["url"].as_str().unwrap());
}

#[test]
fn a_teams_join_link_should_be_a_meeting() {
    let content = json!({"type": "m.custom", "url": "https://teams.microsoft.com/l/meetup-join/example/0"});
    assert_eq!(widget("teams", &content, None, &Viewer::default()).unwrap().meeting, Some(Meeting::Teams));
}

#[test]
fn a_widget_cannot_declare_an_unrelated_host_to_be_a_meeting_provider() {
    let content = json!({"type": "zoom", "url": "https://zoom.us.example.org/j/123"});
    assert_eq!(widget("zoom", &content, None, &Viewer::default()).unwrap().meeting, None);
}

#[test]
fn only_advertised_service_accounts_and_bridge_destinations_should_be_listed() {
    let state = [
        json!({"type": "io.element.functional_members", "state_key": "", "content": {
            "service_members": ["@service:example.org", "not a Matrix ID"]
        }}),
        json!({"type": "uk.half-shot.bridge", "state_key": "destination", "content": {
            "protocol": {"displayname": "Slack"}, "channel": {"displayname": "#releases"}
        }}),
    ];
    let (services, bridges) = super::room_integrations(&state);
    assert_eq!(services, ["@service:example.org"]);
    assert_eq!(bridges[0].channel, "#releases");
}
