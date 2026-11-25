use serde_json::json;

use super::integrations::{SENDABLE, named_manager, policies, terms_url, widget_content};
use super::meetings::{BigBlueButton, Jitsi, Place, base32, jwt, named_jitsi, place};
use super::session::{Grants, Handshake, IDENTITY, PermissionKind, Step, api_url, describe, identity, refused};
use super::*;

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
    assert_eq!(widget_content(&json!({"widget_id": "w", "url": null})), json!({}));
    let set = widget_content(&json!({"widget_id": "w", "type": "m.custom", "url": "https://w.example", "name": "W"}));
    assert_eq!(set, json!({"type": "m.custom", "url": "https://w.example", "name": "W", "data": {}}));
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
