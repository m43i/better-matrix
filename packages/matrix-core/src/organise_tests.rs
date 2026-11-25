//! Room tags, the unread mark and notification keywords against a loopback homeserver.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::Account;
use crate::model::RoomPriority;

const ROOM: &str = "!room:example.org";

/// A homeserver whose sync gives the room `room_data` and the account `global_data`, recording
/// the first line and body of every other request.
fn server(room_data: Value, global_data: Value) -> (crate::test_server::Server, Arc<Mutex<Vec<String>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&sent);
    let server = crate::test_server::Server::new(move |request| {
        let mut response = crate::test_server::thread_response(request);
        if request.contains("/sync?") {
            response["rooms"]["join"][ROOM]["account_data"] = json!({ "events": room_data });
            response["account_data"] = json!({ "events": global_data });
        } else if !request.contains("/keys/") {
            let line = request.lines().next().unwrap_or_default();
            let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
            captured.lock().expect("requests").push(format!("{line} {body}"));
        }
        response
    });
    (server, sent)
}

async fn synced(server: &crate::test_server::Server) -> Account {
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .expect("sync");
    account
}

async fn summary(account: &Account) -> crate::RoomSummary {
    let room = account.joined_room(ROOM).expect("room");
    let settings = account.client().notification_settings().await;
    crate::convert::room_summary(&room, &settings, None, false, false, &account.sticky).await
}

fn tags(names: &[&str]) -> Value {
    let tags: serde_json::Map<String, Value> = names.iter().map(|name| ((*name).to_owned(), json!({}))).collect();
    json!([{ "type": "m.tag", "content": { "tags": tags } }])
}

fn requests(sent: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    sent.lock().expect("requests").clone()
}

#[tokio::test]
async fn a_room_tagged_favourite_should_be_summarised_as_one() {
    let (server, _) = server(tags(&["m.favourite"]), json!([]));
    let account = synced(&server).await;
    assert_eq!(summary(&account).await.priority, RoomPriority::Favourite);
}

#[tokio::test]
async fn a_room_tagged_both_ways_should_count_as_a_favourite() {
    let (server, _) = server(tags(&["m.lowpriority", "m.favourite"]), json!([]));
    let account = synced(&server).await;
    assert_eq!(summary(&account).await.priority, RoomPriority::Favourite);
}

#[tokio::test]
async fn a_room_tagged_low_priority_should_be_summarised_as_one() {
    let (server, _) = server(tags(&["m.lowpriority"]), json!([]));
    let account = synced(&server).await;
    assert_eq!(summary(&account).await.priority, RoomPriority::Low);
}

#[tokio::test]
async fn a_room_marked_unread_elsewhere_should_be_summarised_as_marked() {
    let (server, _) = server(json!([{ "type": "m.marked_unread", "content": { "unread": true } }]), json!([]));
    let account = synced(&server).await;
    assert!(summary(&account).await.marked_unread);
}

#[tokio::test]
async fn a_room_marked_with_the_unstable_flag_should_be_summarised_as_marked() {
    let (server, _) =
        server(json!([{ "type": "com.famedly.marked_unread", "content": { "unread": true } }]), json!([]));
    let account = synced(&server).await;
    assert!(summary(&account).await.marked_unread);
}

#[tokio::test]
async fn favouriting_a_room_should_tag_it_on_the_server() {
    let (server, sent) = server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_room_priority(ROOM, RoomPriority::Favourite).await.expect("favourite");
    let requests = requests(&sent);
    assert!(requests.iter().any(|r| r.starts_with("PUT") && r.contains("/tags/m.favourite")), "{requests:?}");
}

#[tokio::test]
async fn a_low_priority_room_set_back_to_normal_should_lose_its_tag() {
    let (server, sent) = server(tags(&["m.lowpriority"]), json!([]));
    let account = synced(&server).await;
    account.set_room_priority(ROOM, RoomPriority::Normal).await.expect("normal");
    let requests = requests(&sent);
    assert!(requests.iter().any(|r| r.starts_with("DELETE") && r.contains("/tags/m.lowpriority")), "{requests:?}");
    assert!(!requests.iter().any(|r| r.starts_with("PUT") && r.contains("/tags/")), "{requests:?}");
}

#[tokio::test]
async fn unfavouriting_right_after_favouriting_should_remove_the_tag_before_sync_confirms_it() {
    let (server, sent) = server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_room_priority(ROOM, RoomPriority::Favourite).await.expect("favourite");
    account.set_room_priority(ROOM, RoomPriority::Normal).await.expect("normal");
    let requests = requests(&sent);
    assert!(requests.iter().any(|r| r.starts_with("DELETE") && r.contains("/tags/m.favourite")), "{requests:?}");
}

#[tokio::test]
async fn room_changes_asked_for_together_should_land_in_the_order_asked() {
    let (server, sent) = server(json!([]), json!([]));
    let account = synced(&server).await;
    let change = |priority| {
        let changing = account.clone();
        account.organising.run(async move { changing.set_room_priority(ROOM, priority).await })
    };
    let (favourite, normal) = (change(RoomPriority::Favourite), change(RoomPriority::Normal));
    let (normal, favourite) = tokio::join!(normal, favourite);
    favourite.expect("favourite");
    normal.expect("normal");
    let favourites: Vec<String> = requests(&sent).into_iter().filter(|r| r.contains("/tags/m.favourite")).collect();
    assert!(favourites.last().is_some_and(|r| r.starts_with("DELETE")), "{favourites:?}");
    assert!(favourites.first().is_some_and(|r| r.starts_with("PUT")), "{favourites:?}");
}

#[tokio::test]
async fn favouriting_should_take_the_low_priority_tag_away_whatever_sync_said() {
    let (server, sent) = server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_room_priority(ROOM, RoomPriority::Favourite).await.expect("favourite");
    let requests = requests(&sent);
    assert!(requests.iter().any(|r| r.starts_with("DELETE") && r.contains("/tags/m.lowpriority")), "{requests:?}");
}

#[tokio::test]
async fn marking_a_room_unread_should_set_its_flag_on_the_server() {
    let (server, sent) = server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_unread(ROOM, true).await.expect("mark unread");
    let requests = requests(&sent);
    let flag = requests.iter().find(|r| r.starts_with("PUT") && r.contains("m.marked_unread")).expect("flag set");
    assert!(flag.ends_with(r#"{"unread":true}"#), "{flag}");
}

#[tokio::test]
async fn taking_the_mark_away_should_clear_the_flag() {
    let (server, sent) = server(json!([{ "type": "m.marked_unread", "content": { "unread": true } }]), json!([]));
    let account = synced(&server).await;
    account.set_unread(ROOM, false).await.expect("unmark");
    let requests = requests(&sent);
    let flag = requests.iter().find(|r| r.starts_with("PUT") && r.contains("m.marked_unread")).expect("flag set");
    assert!(flag.ends_with(r#"{"unread":false}"#), "{flag}");
    assert!(!requests.iter().any(|r| r.contains("/receipt/")), "{requests:?}");
}

#[tokio::test]
async fn unmarking_right_after_marking_should_reach_the_server_before_sync_confirms_it() {
    let (server, sent) = server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_unread(ROOM, true).await.expect("mark");
    account.set_unread(ROOM, false).await.expect("unmark");
    let flags: Vec<String> =
        requests(&sent).into_iter().filter(|r| r.starts_with("PUT") && r.contains("m.marked_unread")).collect();
    assert_eq!(flags.len(), 2, "{flags:?}");
    assert!(flags[1].ends_with(r#"{"unread":false}"#), "{flags:?}");
}

#[tokio::test]
async fn reading_a_room_should_move_its_receipt_and_fully_read_marker_to_its_latest_message() {
    // The synced room holds one message from someone else, which the user hasn't read.
    let (server, sent) = server(json!([{ "type": "m.marked_unread", "content": { "unread": true } }]), json!([]));
    let account = synced(&server).await;
    account.mark_room_read(ROOM).await.expect("mark read");
    let requests = requests(&sent);
    let markers = requests.iter().find(|r| r.starts_with("POST") && r.contains("/read_markers")).expect("markers");
    assert!(markers.contains(r#""m.fully_read":"$root""#) && markers.contains(r#""m.read":"$root""#), "{markers}");
    let flag = requests.iter().find(|r| r.starts_with("PUT") && r.contains("m.marked_unread")).expect("flag set");
    assert!(flag.ends_with(r#"{"unread":false}"#), "{flag}");
}

fn push_rules(content: Value) -> Value {
    json!([{ "type": "m.push_rules", "content": { "global": ruleset(content) } }])
}

fn ruleset(content: Value) -> Value {
    json!({ "content": content, "override": [], "room": [], "sender": [], "underride": [] })
}

fn keyword_rule(keyword: &str, enabled: bool) -> Value {
    json!({ "rule_id": keyword, "pattern": keyword, "enabled": enabled, "default": false,
            "actions": ["notify", { "set_tweak": "sound", "value": "default" }] })
}

/// A homeserver keeping content push rules: its sync gives `synced` (the device's copy, which may
/// be older), while the push rule API reads and changes `current`. Also records every other
/// request, as [`server`] does.
fn rules_server(synced: Value, current: Value) -> (crate::test_server::Server, Arc<Mutex<Vec<String>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&sent);
    let rules = Arc::new(Mutex::new(current.as_array().cloned().unwrap_or_default()));
    let server = crate::test_server::Server::new(move |request| {
        let mut response = crate::test_server::thread_response(request);
        let line = request.lines().next().unwrap_or_default().to_owned();
        if request.contains("/sync?") {
            response["account_data"] = json!({ "events": push_rules(synced.clone()) });
            return response;
        }
        if request.contains("/keys/") {
            return response;
        }
        let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
        captured.lock().expect("requests").push(format!("{line} {body}"));
        let mut rules = rules.lock().expect("rules");
        let id = line.split("/pushrules/global/content/").nth(1).and_then(|rest| rest.split([' ', '/']).next());
        let id = id.map(|id| id.replace("%20", " "));
        match (line.split(' ').next(), id) {
            (Some("GET"), _) if line.contains("/pushrules/global") => return ruleset(Value::Array(rules.clone())),
            (Some("PUT"), Some(id)) if line.contains("/enabled") => {
                for rule in rules.iter_mut().filter(|rule| rule["rule_id"] == id.as_str()) {
                    rule["enabled"] = json!(true);
                }
            }
            (Some("PUT"), Some(id)) => {
                let content: Value = serde_json::from_str(body).unwrap_or_default();
                rules.push(json!({ "rule_id": id, "pattern": content["pattern"], "enabled": true,
                                   "default": false, "actions": content["actions"] }));
            }
            (Some("DELETE"), Some(id)) => rules.retain(|rule| rule["rule_id"] != id.as_str()),
            _ => {}
        }
        json!({})
    });
    (server, sent)
}

fn deleted(sent: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    requests(sent).into_iter().filter(|r| r.starts_with("DELETE") && r.contains("/pushrules/")).collect()
}

#[tokio::test]
async fn keywords_should_be_the_enabled_keyword_rules_sorted() {
    let rules = json!([keyword_rule("zebra", true), keyword_rule("Apple", true), keyword_rule("off", false)]);
    let (server, _) = rules_server(rules, json!([]));
    let account = synced(&server).await;
    assert_eq!(account.keywords().await, ["Apple", "zebra"]);
}

#[tokio::test]
async fn adding_a_keyword_should_add_a_content_rule_for_it_trimmed() {
    let (server, sent) = rules_server(json!([]), json!([]));
    let account = synced(&server).await;
    let keywords = account.set_keyword("  deploy ", true).await.expect("add keyword");
    assert_eq!(keywords, ["deploy"]);
    let requests = requests(&sent);
    let rule =
        requests.iter().find(|r| r.starts_with("PUT") && r.contains("/pushrules/global/content/")).expect("rule");
    assert!(rule.contains(r#""pattern":"deploy""#), "{rule}");
    assert!(rule.contains(r#"{"set_tweak":"highlight"}"#), "{rule}");
}

#[tokio::test]
async fn adding_a_keyword_the_server_has_but_disabled_should_enable_its_rule() {
    let (server, sent) = rules_server(json!([]), json!([keyword_rule("deploy", false)]));
    let account = synced(&server).await;
    assert_eq!(account.set_keyword("deploy", true).await.expect("add keyword"), ["deploy"]);
    assert!(requests(&sent).iter().any(|r| r.starts_with("PUT") && r.contains("/content/deploy/enabled")));
}

#[tokio::test]
async fn adding_a_keyword_an_older_sync_still_lists_should_add_it_on_the_server() {
    // Removed meanwhile; this device's copy is older than that.
    let (server, sent) = rules_server(json!([keyword_rule("deploy", true)]), json!([]));
    let account = synced(&server).await;
    account.set_keyword("deploy", true).await.expect("add keyword");
    assert!(requests(&sent).iter().any(|r| r.starts_with("PUT") && r.contains(r#""pattern":"deploy""#)));
}

#[tokio::test]
async fn removing_a_keyword_should_delete_its_rule_and_list_the_rest() {
    let rules = json!([keyword_rule("deploy", true), keyword_rule("release", true)]);
    let (server, sent) = rules_server(rules.clone(), rules);
    let account = synced(&server).await;
    assert_eq!(account.set_keyword("deploy", false).await.expect("remove keyword"), ["release"]);
    assert_eq!(deleted(&sent).len(), 1);
}

#[tokio::test]
async fn removing_a_keyword_right_after_adding_it_should_delete_its_rule() {
    let (server, sent) = rules_server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_keyword("deploy", true).await.expect("add keyword");
    let keywords = account.set_keyword("deploy", false).await.expect("remove keyword");
    assert!(keywords.is_empty());
    assert!(deleted(&sent).iter().any(|r| r.contains("/content/deploy")), "{:?}", requests(&sent));
}

#[tokio::test]
async fn removing_a_keyword_should_leave_a_rule_with_its_name_but_another_word() {
    let other =
        json!({ "rule_id": "deploy", "pattern": "release", "enabled": true, "default": false, "actions": ["notify"] });
    let (server, sent) = rules_server(json!([keyword_rule("deploy", true)]), json!([other]));
    let account = synced(&server).await;
    assert_eq!(account.set_keyword("deploy", false).await.expect("remove keyword"), ["release"]);
    assert!(deleted(&sent).is_empty(), "{:?}", requests(&sent));
}

#[tokio::test]
async fn a_keyword_with_spaces_from_another_client_should_be_removed_as_listed() {
    let rules = json!([keyword_rule(" deploy ", true)]);
    let (server, sent) = rules_server(rules.clone(), rules);
    let account = synced(&server).await;
    assert_eq!(account.keywords().await, [" deploy "]);
    account.set_keyword(" deploy ", false).await.expect("remove keyword");
    assert_eq!(deleted(&sent).len(), 1, "{:?}", requests(&sent));
}

#[tokio::test]
async fn a_new_keyword_should_not_take_the_name_of_a_rule_for_another_word() {
    let other = json!({ "rule_id": "deploy", "pattern": "release", "enabled": true, "default": false,
                        "actions": ["notify"] });
    let (server, sent) = rules_server(json!([]), json!([other]));
    let account = synced(&server).await;
    account.set_keyword("deploy", true).await.expect("add keyword");
    let requests = requests(&sent);
    let put = requests.iter().find(|r| r.starts_with("PUT") && r.contains(r#""pattern":"deploy""#)).expect("rule");
    assert!(put.contains("/content/chat.bettermatrix.keyword."), "{put}");
}

#[tokio::test]
async fn a_keyword_with_characters_unfit_for_a_name_should_get_a_name_of_its_own() {
    let (server, sent) = rules_server(json!([]), json!([]));
    let account = synced(&server).await;
    assert_eq!(account.set_keyword("CI/CD", true).await.expect("add keyword"), ["CI/CD"]);
    assert!(requests(&sent).iter().any(|r| r.starts_with("PUT") && r.contains("/content/chat.bettermatrix.keyword.")));
}

#[tokio::test]
async fn a_rule_silencing_a_word_should_be_no_keyword_and_stay() {
    let silence = json!({ "rule_id": "spoilers", "pattern": "spoilers", "enabled": true, "default": false,
                          "actions": [] });
    let (server, sent) = rules_server(json!([silence.clone()]), json!([silence]));
    let account = synced(&server).await;
    assert!(account.keywords().await.is_empty());
    account.set_keyword("spoilers", false).await.expect("remove keyword");
    assert!(deleted(&sent).is_empty());
}

#[tokio::test]
async fn keywords_differing_only_in_case_should_each_show_once() {
    let rules = json!([keyword_rule("Deploy", true), keyword_rule("deploy", true), {
        "rule_id": "again", "pattern": "Deploy", "enabled": true, "default": false, "actions": ["notify"] }]);
    let (server, _) = rules_server(rules, json!([]));
    let account = synced(&server).await;
    assert_eq!(account.keywords().await, ["Deploy", "deploy"]);
}

#[tokio::test]
async fn a_blank_keyword_should_change_nothing() {
    let (server, sent) = rules_server(json!([]), json!([]));
    let account = synced(&server).await;
    account.set_keyword("   ", true).await.expect("blank keyword");
    assert!(!requests(&sent).iter().any(|r| r.starts_with("PUT") || r.starts_with("DELETE")));
}

#[tokio::test]
async fn a_room_should_offer_meetings_where_the_server_names_a_jitsi_server() {
    // The user may change the room's state.
    let server = crate::test_server::Server::new(|request| {
        let mut response = crate::test_server::thread_response(request);
        if request.contains("/sync?") {
            let levels = json!({"type": "m.room.power_levels", "state_key": "", "sender": "@a:example.org",
                "event_id": "$levels", "origin_server_ts": 0, "content": {"users": {"@a:example.org": 100}}});
            if let Some(events) = response["rooms"]["join"][ROOM]["state"]["events"].as_array_mut() {
                events.push(levels);
            }
        }
        response
    });
    let account = synced(&server).await;
    let room = account.joined_room(ROOM).expect("room");
    let settings = account.client().notification_settings().await;
    let summary = crate::convert::room_summary(&room, &settings, None, false, true, &account.sticky).await;
    assert!(summary.can_start_meeting);
}

#[tokio::test]
async fn a_room_should_offer_no_meetings_where_the_server_names_no_jitsi_server() {
    let (server, _) = server(json!([]), json!([]));
    let account = synced(&server).await;
    assert!(!summary(&account).await.can_start_meeting);
}
