use matrix_core::RequestId;

use super::helper::{
    accept, delivers_to, forwards, navigable, origin, post_script, reload_script, same_document, shim,
};
use super::*;

#[test]
fn an_open_line_should_parse_back_into_its_page() {
    let open = ToHelper::Open { url: "https://board.example".into(), title: "Board".into(), bridge: Bridge::WidgetApi };
    assert_eq!(ToHelper::parse(&open.line()), Some(open));
}

#[test]
fn an_open_line_should_read_as_the_protocol_names_it() {
    let line = r#"{"type":"open","url":"https://board.example","title":"Board","bridge":"integrations"}"#;
    let parsed = ToHelper::parse(line);
    assert_eq!(
        parsed,
        Some(ToHelper::Open {
            url: "https://board.example".into(),
            title: "Board".into(),
            bridge: Bridge::Integrations
        })
    );
}

#[test]
fn every_line_should_end_with_one_newline() {
    let line = FromHelper::Message { message: "{\"a\":\"b\\nc\"}".into() }.line();
    assert_eq!(line.matches('\n').count(), 1);
}

#[test]
fn a_closed_line_should_parse_back() {
    assert_eq!(FromHelper::parse(&FromHelper::Closed.line()), Some(FromHelper::Closed));
}

#[test]
fn a_line_that_isnt_the_protocol_should_say_nothing() {
    assert_eq!(FromHelper::parse("Gtk-WARNING: something"), None);
}

/// `text` as JSON, which the test writes valid.
fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn the_widget_api_should_forward_the_widgets_requests() {
    assert!(forwards(Bridge::WidgetApi, &json(r#"{"api":"fromWidget","action":"supported_api_versions","data":{}}"#)));
}

#[test]
fn the_widget_api_should_forward_the_widgets_answers() {
    assert!(forwards(Bridge::WidgetApi, &json(r#"{"api":"toWidget","action":"capabilities","response":{}}"#)));
}

#[test]
fn the_widget_api_should_keep_back_what_the_app_posted() {
    let answered = json(r#"{"api":"fromWidget","action":"send_event","response":{}}"#);
    let asked = json(r#"{"api":"toWidget","action":"capabilities","data":{}}"#);
    assert!(!forwards(Bridge::WidgetApi, &answered) && !forwards(Bridge::WidgetApi, &asked));
}

#[test]
fn a_null_response_should_still_count_as_an_answer() {
    let answered = json(r#"{"api":"fromWidget","action":"send_event","response":null}"#);
    assert!(!forwards(Bridge::WidgetApi, &answered));
}

#[test]
fn integrations_should_forward_the_managers_requests() {
    assert!(forwards(Bridge::Integrations, &json(r#"{"action":"bot_options","room_id":"!r:x","request_id":"1"}"#)));
}

#[test]
fn integrations_should_keep_back_widget_api_messages_and_answers() {
    let api = json(r#"{"api":"fromWidget","action":"send_event"}"#);
    let answer = json(r#"{"action":"bot_options","response":{"success":true}}"#);
    assert!(!forwards(Bridge::Integrations, &api) && !forwards(Bridge::Integrations, &answer));
}

#[test]
fn an_integration_managers_answer_holding_null_should_not_go_back_out() {
    let answer = json(r#"{"action":"membership_state","room_id":"!r:x","request_id":"1","response":null}"#);
    assert!(!forwards(Bridge::Integrations, &answer));
}

#[test]
fn no_bridge_should_forward_nothing() {
    assert!(!forwards(Bridge::None, &json(r#"{"api":"fromWidget","action":"x"}"#)));
}

#[test]
fn a_message_that_isnt_an_object_should_never_be_forwarded() {
    assert!(!forwards(Bridge::WidgetApi, &json(r#"["fromWidget"]"#)));
}

#[test]
fn a_message_with_the_windows_token_should_go_to_the_app() {
    let sent = r#"{"token":"secret","message":{"api":"fromWidget","action":"content_loaded"}}"#;
    assert_eq!(
        accept(Bridge::WidgetApi, "secret", sent).as_deref(),
        Some(r#"{"action":"content_loaded","api":"fromWidget"}"#)
    );
}

#[test]
fn a_message_with_another_token_should_be_dropped() {
    let sent = r#"{"token":"guess","message":{"api":"fromWidget","action":"content_loaded"}}"#;
    assert_eq!(accept(Bridge::WidgetApi, "secret", sent), None);
}

#[test]
fn a_message_without_a_token_should_be_dropped() {
    assert_eq!(accept(Bridge::WidgetApi, "secret", r#"{"api":"fromWidget","action":"content_loaded"}"#), None);
}

#[test]
fn a_message_the_bridge_keeps_back_should_be_dropped_even_with_the_token() {
    let sent = r#"{"token":"secret","message":{"api":"fromWidget","action":"x","response":null}}"#;
    assert_eq!(accept(Bridge::WidgetApi, "secret", sent), None);
}

#[test]
fn a_page_should_stay_on_its_origin() {
    assert!(navigable("https://board.example.org/team/2", "https://board.example.org"));
}

#[test]
fn a_page_should_not_go_to_another_origin() {
    assert!(!navigable("https://evil.example/", "https://board.example.org"));
}

#[test]
fn another_port_should_be_another_origin() {
    assert!(!navigable("https://board.example.org:8443/", "https://board.example.org"));
}

#[test]
fn a_page_should_not_go_to_a_file() {
    assert!(!navigable("file:///etc/passwd", "https://board.example.org"));
}

#[test]
fn a_frame_of_the_pages_own_making_should_load() {
    assert!(navigable("about:blank", "https://board.example.org"));
}

#[test]
fn an_address_origin_should_leave_out_its_path() {
    assert_eq!(origin("https://Board.Example.org/team?x=1").as_deref(), Some("https://board.example.org"));
}

#[test]
fn a_message_should_go_into_the_page_only_after_it_checks_its_origin() {
    let script = post_script(r#"{"api":"toWidget","data":{"n":1}}"#, "https://board.example");
    assert_eq!(
        script.as_deref(),
        Some(
            r#"(function () { if (window.location.origin !== "https://board.example") return; var message = {"api":"toWidget","data":{"n":1}}; window.postMessage(message, "https://board.example"); })();"#
        )
    );
}

#[test]
fn a_message_should_go_in_as_an_object_rather_than_parsed_text() {
    let script = post_script(r#"{"api":"toWidget"}"#, "https://board.example").unwrap_or_default();
    assert!(!script.contains("JSON.parse") && !script.contains(r#""*""#));
}

#[test]
fn a_message_should_not_close_the_script_or_end_its_line() {
    let script = post_script("{\"data\":\"</script>\u{2028}\u{2029}\"}", "https://board.example").unwrap_or_default();
    assert!(script.contains(r#""\u003c/script>\u2028\u2029""#));
}

#[test]
fn a_message_should_be_addressed_to_the_approved_origin() {
    let script = post_script(r#"{"api":"toWidget"}"#, "https://board.example:8443").unwrap_or_default();
    assert!(script.ends_with(r#"window.postMessage(message, "https://board.example:8443"); })();"#));
}

#[test]
fn a_message_that_isnt_an_object_should_not_be_posted() {
    assert_eq!(post_script("alert(1)", "https://board.example"), None);
}

#[test]
fn a_page_without_a_bridge_should_get_no_shim() {
    assert_eq!(shim(Bridge::None, "secret"), None);
}

#[test]
fn the_shim_should_send_with_the_windows_token() {
    assert!(shim(Bridge::WidgetApi, "secret").is_some_and(|shim| shim.contains(r#"var token = "secret";"#)));
}

#[test]
fn the_shim_should_take_over_posting_rather_than_listen() {
    let shim = shim(Bridge::Integrations, "secret").unwrap_or_default();
    assert!(shim.contains("window.postMessage = function") && !shim.contains("addEventListener"));
}

#[test]
fn the_shim_should_read_a_response_by_its_presence() {
    assert!(shim(Bridge::WidgetApi, "secret").is_some_and(|shim| shim.contains(r#"!("response" in data)"#)));
}

#[test]
fn a_headless_window_should_keep_what_was_asked() {
    let mut windows = Windows::headless();
    windows.open(RequestId(1), "https://board.example".into(), "Board".into(), Bridge::WidgetApi);
    windows.post(RequestId(1), "{}".into());
    windows.close(RequestId(1));
    assert_eq!(
        windows.calls(),
        [
            WindowCall::Open {
                request: RequestId(1),
                url: "https://board.example".into(),
                title: "Board".into(),
                bridge: Bridge::WidgetApi
            },
            WindowCall::Post { request: RequestId(1), message: "{}".into() },
            WindowCall::Close(RequestId(1)),
        ]
    );
}

#[test]
fn a_post_for_a_run_without_a_window_should_go_nowhere() {
    let mut windows = Windows::headless();
    windows.post(RequestId(1), "{}".into());
    assert!(windows.calls().is_empty());
}

#[test]
fn a_window_closed_by_itself_should_be_forgotten_once() {
    let mut windows = Windows::headless();
    windows.open(RequestId(1), "https://board.example".into(), "Board".into(), Bridge::None);
    assert_eq!((windows.forget(RequestId(1)), windows.forget(RequestId(1))), (true, false));
}

#[test]
fn messages_should_go_into_a_document_on_the_approved_origin() {
    assert!(delivers_to("https://board.example.org/team#x", "https://board.example.org"));
}

#[test]
fn messages_should_never_go_into_a_document_of_another_origin() {
    assert!(!delivers_to("https://evil.example/", "https://board.example.org"));
}

#[test]
fn messages_should_never_go_into_a_document_of_the_pages_own_making() {
    let approved = "https://board.example.org";
    let opaque = ["data:text/html,hi", "blob:https://board.example.org/1234", "about:blank"];
    assert!(opaque.iter().all(|showing| !delivers_to(showing, approved)));
}

#[test]
fn a_loading_line_should_parse_back() {
    assert_eq!(FromHelper::parse(r#"{"type":"loading"}"#), Some(FromHelper::Loading));
}

#[test]
fn a_page_at_another_fragment_should_count_as_the_same_document() {
    assert!(same_document("https://w.example/app#board", "https://w.example/app#initial"));
    assert!(same_document("https://w.example/app", "https://w.example/app#initial"));
    assert!(!same_document("https://w.example/app?x=1", "https://w.example/app"));
}

#[test]
fn a_reload_should_restore_the_address_asked_for() {
    let script = reload_script("https://w.example/app#initial");
    assert_eq!(
        script,
        "try { history.replaceState(history.state, \"\", \"https://w.example/app#initial\"); } finally { location.reload(); }"
    );
}
