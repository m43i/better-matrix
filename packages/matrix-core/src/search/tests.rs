use super::*;
use matrix_sdk::ruma::{OwnedRoomId, RoomId};
use serde_json::json;

fn event(value: Value) -> TimelineEvent {
    TimelineEvent::from_plaintext(serde_json::from_value(value).unwrap())
}

fn message(id: &str, text: &str) -> Value {
    json!({"type":"m.room.message", "event_id":id, "sender":"@ada:example.org", "origin_server_ts":100,
        "content":{"msgtype":"m.text", "body":text}})
}

#[test]
fn fuzzy_matches_should_follow_the_shared_unicode_vectors() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("matches.json")).unwrap();
    for case in cases {
        let actual = fuzzy_match(case["text"].as_str().unwrap(), case["query"].as_str().unwrap()).map(|m| m.positions);
        let expected: Option<Vec<usize>> = serde_json::from_value(case["positions"].clone()).unwrap();
        assert_eq!(actual, expected, "{case}");
    }
}

#[test]
fn an_edit_should_replace_the_original_searchable_text() {
    let mut edit = message("$edit", "* replacement text");
    edit["content"]["m.relates_to"] = json!({"rel_type":"m.replace", "event_id":"$original"});
    edit["content"]["m.new_content"] = json!({"msgtype":"m.text", "body":"replacement text"});
    let events = [event(message("$original", "secret old text")), event(edit)];
    assert!(search_events("!room:example.org", &events, "secret").is_empty());
    assert_eq!(search_events("!room:example.org", &events, "replacement")[0].1.event_id, "$original");
}

#[test]
fn a_redacted_message_should_not_be_searchable() {
    let redaction = json!({"type":"m.room.redaction", "event_id":"$redaction", "content":{"redacts":"$original"}});
    let events = [event(message("$original", "secret")), event(redaction)];
    assert!(search_events("!room:example.org", &events, "secret").is_empty());
}

#[test]
fn another_senders_edit_should_not_replace_a_message() {
    let mut edit = message("$edit", "* forged");
    edit["sender"] = json!("@eve:example.org");
    edit["content"]["m.relates_to"] = json!({"rel_type":"m.replace", "event_id":"$original"});
    edit["content"]["m.new_content"] = json!({"body":"forged"});
    assert_eq!(
        search_events("!room:example.org", &[event(message("$original", "original")), event(edit)], "original").len(),
        1
    );
}

#[test]
fn a_thread_reply_should_keep_its_root() {
    let mut reply = message("$reply", "found in thread");
    reply["content"]["m.relates_to"] = json!({"rel_type":"m.thread", "event_id":"$root"});
    assert_eq!(search_events("!room:example.org", &[event(reply)], "thread")[0].1.thread.as_deref(), Some("$root"));
}

#[test]
fn excerpts_should_keep_unicode_highlights_near_a_late_match() {
    let body = format!("{}café 🦀", "hello ".repeat(60));
    let hits = search_events("!room:example.org", &[event(message("$late", &body))], "café");
    let hit = &hits[0].1;
    let matched: String =
        hit.body.chars().enumerate().filter(|(i, _)| hit.highlights.contains(i)).map(|(_, c)| c).collect();
    assert_eq!(matched, "café");
}

#[tokio::test]
async fn search_should_read_closed_rooms_from_the_store_without_a_request() {
    let server = crate::test_server::Server::new(crate::test_server::thread_response);
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .unwrap();
    let room: OwnedRoomId = RoomId::parse("!room:example.org").unwrap();

    {
        let lock = account.client().event_cache_store().lock().await.unwrap();
        lock.as_clean().unwrap().save_event(&room, event(message("$stored", "hello device"))).await.unwrap();
    }
    drop(server);
    let hits = account.search_messages("device", &SearchUsage::default()).await.unwrap();
    assert_eq!(hits.iter().map(|h| h.event_id.as_str()).collect::<Vec<_>>(), ["$stored"]);
}

#[test]
fn a_polls_question_should_be_searchable() {
    let mut poll = message("$poll", "unused");
    poll["type"] = json!("org.matrix.msc3381.poll.start");
    poll["content"] = json!({"org.matrix.msc3381.poll.start":{"question":{"org.matrix.msc1767.text":"Lunch?"}}});
    assert_eq!(search_events("!room:example.org", &[event(poll)], "lunch").len(), 1);
}

#[test]
fn undecrypted_events_should_not_be_searchable() {
    let mut encrypted = message("$encrypted", "secret");
    encrypted["type"] = json!("m.room.encrypted");
    assert!(search_events("!room:example.org", &[event(encrypted)], "secret").is_empty());
}

#[tokio::test]
async fn local_search_should_bound_results_and_put_newer_equal_matches_first() {
    let server = crate::test_server::Server::new(crate::test_server::thread_response);
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .unwrap();
    let room = RoomId::parse("!room:example.org").unwrap();
    {
        let lock = account.client().event_cache_store().lock().await.unwrap();
        let store = lock.as_clean().unwrap();
        for i in 0..40 {
            let mut value = message(&format!("$stored-{i}"), "hello device");
            value["origin_server_ts"] = json!(i);
            store.save_event(&room, event(value)).await.unwrap();
        }
    }
    let hits = account.search_messages("device", &SearchUsage::default()).await.unwrap();
    assert_eq!((hits.len(), hits[0].event_id.as_str()), (30, "$stored-39"));
}

#[tokio::test]
async fn frequent_senders_should_be_ranked_before_newer_equal_message_matches() {
    let server = crate::test_server::Server::new(crate::test_server::thread_response);
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .unwrap();
    let room = RoomId::parse("!room:example.org").unwrap();
    {
        let lock = account.client().event_cache_store().lock().await.unwrap();
        let store = lock.as_clean().unwrap();
        let mut frequent = message("$frequent", "hello device");
        frequent["sender"] = json!("@grace:example.org");
        frequent["origin_server_ts"] = json!(1);
        store.save_event(&room, event(frequent)).await.unwrap();
        store.save_event(&room, event(message("$new", "hello device"))).await.unwrap();
    }
    let usage = SearchUsage { people: BTreeMap::from([("@grace:example.org".into(), 10)]), ..SearchUsage::default() };
    assert_eq!(account.search_messages("device", &usage).await.unwrap()[0].event_id, "$frequent");
}
