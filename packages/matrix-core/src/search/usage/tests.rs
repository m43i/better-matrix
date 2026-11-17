use super::*;

fn interaction(room: &str, person: &str, weight: u64) -> Interaction {
    Interaction { room: room.into(), people: vec![person.into()], weight, reply_to: None }
}

#[test]
fn repeated_interactions_should_raise_the_room_and_person_scores() {
    let mut usage = SearchUsage::default();
    usage.record(interaction("!room:example.org", "@grace:example.org", 1));
    usage.record(interaction("!room:example.org", "@grace:example.org", 3));
    assert_eq!((usage.room("!room:example.org"), usage.person("@grace:example.org")), (4, 4));
}

#[test]
fn message_preference_should_include_the_sender_even_in_another_room() {
    let mut usage = SearchUsage::default();
    usage.record(interaction("!dm:example.org", "@grace:example.org", 3));
    assert_eq!(usage.message("!other:example.org", "@grace:example.org"), 3);
}

#[tokio::test]
async fn usage_should_survive_restarting_the_worker_without_crossing_accounts() {
    let account = Account::for_test("http://127.0.0.1:1").await;
    let other = Account::for_test("http://127.0.0.1:1").await;
    let (tracker, worker) = Usage::start(account.clone(), Arc::new(|_| {}));
    tracker.send.send(UsageCommand::Interact(interaction("!room:example.org", "@grace:example.org", 3))).unwrap();
    let usage = tracker.snapshot().await;
    worker.abort();
    let (restored, worker) = Usage::start(account, Arc::new(|_| {}));
    assert_eq!(restored.snapshot().await, usage);
    assert_eq!(load(&other).await.unwrap(), SearchUsage::default());
    worker.abort();
}

#[tokio::test]
async fn queued_interactions_should_not_overwrite_each_other() {
    let account = Account::for_test("http://127.0.0.1:1").await;
    let (tracker, worker) = Usage::start(account, Arc::new(|_| {}));
    tracker.send.send(UsageCommand::Interact(interaction("!room:example.org", "@grace:example.org", 1))).unwrap();
    tracker.send.send(UsageCommand::Interact(interaction("!room:example.org", "@grace:example.org", 3))).unwrap();
    assert_eq!(tracker.snapshot().await.person("@grace:example.org"), 4);
    worker.abort();
}

#[tokio::test]
async fn a_frequent_person_should_survive_the_directory_result_limit() {
    use serde_json::json;
    let server = crate::test_server::Server::new(
        |_| json!({"results":(0..50).map(|i| json!({"user_id":format!("@person{i}:example.org"),"display_name":format!("Person {i}")})).collect::<Vec<_>>(),"limited":false}),
    );
    let account = Account::for_test(&server.url()).await;
    let usage =
        SearchUsage { people: BTreeMap::from([("@person49:example.org".into(), 12)]), ..SearchUsage::default() };
    let people = account.search_people_ranked("person", &usage).await.unwrap();
    assert_eq!(people[0].id, "@person49:example.org");
}

#[tokio::test]
async fn a_room_open_should_also_count_its_direct_chat_partner() {
    use serde_json::json;
    let server = crate::test_server::Server::new(|request| {
        let mut response = crate::test_server::thread_response(request);
        if request.contains("/sync?") {
            response["account_data"] =
                json!({"events":[{"type":"m.direct","content":{"@b:example.org":["!room:example.org"]}}]});
        }
        response
    });
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .unwrap();
    let (tracker, worker) = Usage::start(account.clone(), Arc::new(|_| {}));
    tracker.room(&account, "!room:example.org", 1, None);
    assert_eq!(tracker.snapshot().await.person("@b:example.org"), 1);
    worker.abort();
}

#[tokio::test]
async fn replying_should_count_the_original_sender_without_a_request() {
    let server = crate::test_server::Server::new(crate::test_server::thread_response);
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .unwrap();
    let (tracker, worker) = Usage::start(account.clone(), Arc::new(|_| {}));
    drop(server);
    tracker.room(&account, "!room:example.org", 3, Some("$root".into()));
    assert_eq!(tracker.snapshot().await.person("@b:example.org"), 3);
    worker.abort();
}

#[tokio::test]
async fn an_unreadable_saved_history_should_not_be_overwritten_with_session_counts() {
    let account = Account::for_test("http://127.0.0.1:1").await;
    let unreadable = b"unreadable saved history".to_vec();
    account.client().state_store().set_custom_value_no_read(KEY, unreadable.clone()).await.unwrap();
    let (tracker, worker) = Usage::start(account.clone(), Arc::new(|_| {}));
    tracker.send.send(UsageCommand::Interact(interaction("!room:example.org", "@grace:example.org", 3))).unwrap();
    assert_eq!(tracker.snapshot().await.room("!room:example.org"), 3);
    assert_eq!(account.client().state_store().get_custom_value(KEY).await.unwrap(), Some(unreadable));
    worker.abort();
}
