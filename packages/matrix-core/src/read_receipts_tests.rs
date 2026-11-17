use std::time::Duration;

use super::*;
use crate::{EntryKind, ReadReceipt};

fn sync_response(request: &str) -> serde_json::Value {
    let mut response = crate::test_server::thread_response(request);
    if request.contains("/sync?") {
        response["rooms"]["join"]["!room:example.org"]["ephemeral"] = serde_json::json!({"events":[{
            "type":"m.receipt", "content":{"$root":{
                "m.read": {"@a:example.org":{"ts":30}, "@b:example.org":{"ts":20}, "@c:example.org":{}, "@thread-only:example.org":{"ts":40,"thread_id":"$root"}},
                "m.read.private": {"@hidden:example.org":{"ts":40}}
            }}
        }]});
    }
    response
}

#[tokio::test]
async fn exposed_receipts_should_keep_their_times_and_exclude_self_and_private_readers() {
    let server = crate::test_server::Server::new(sync_response);
    let account = Account::for_test(&server.url()).await;
    account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.unwrap();
    let open = open_timeline(account, TimelineKey::room("!room:example.org"), None, false).await.unwrap();
    let TimelineDiff::Reset(entries) = open.reset else { panic!("initial reset") };
    let message = entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Message(message) if message.event_id == "$root" => Some(message),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        message.read_receipts,
        vec![
            ReadReceipt { user_id: "@b:example.org".into(), timestamp_ms: Some(20) },
            ReadReceipt { user_id: "@c:example.org".into(), timestamp_ms: None },
        ]
    );
}

#[tokio::test]
async fn advancing_a_receipt_should_remove_its_old_indicator_and_update_the_new_one() {
    use futures_util::StreamExt;
    let advanced = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&advanced);
    let server = crate::test_server::Server::new(move |request| {
        if !flag.load(std::sync::atomic::Ordering::Relaxed) || !request.contains("/sync?") {
            return sync_response(request);
        }
        serde_json::json!({"next_batch":"advanced", "rooms":{"join":{"!room:example.org":{
            "timeline":{"events":[{"type":"m.room.message","event_id":"$next","sender":"@a:example.org","origin_server_ts":50,"content":{"msgtype":"m.text","body":"Next"}}],"limited":false},
            "ephemeral":{"events":[{"type":"m.receipt","content":{"$next":{"m.read":{"@b:example.org":{"ts":60},"@thread-only:example.org":{"ts":70,"thread_id":"$root"}}}}}]}
        }}}})
    });
    let account = Account::for_test(&server.url()).await;
    account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.unwrap();
    let mut open = open_timeline(account.clone(), TimelineKey::room("!room:example.org"), None, false).await.unwrap();
    let TimelineDiff::Reset(mut entries) = open.reset else { panic!("initial reset") };
    advanced.store(true, std::sync::atomic::Ordering::Relaxed);
    account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(batch) = open.changes.next().await {
            // Apply the same SDK vector diffs to the mirror before checking the positions.
            let mut mirror: eyeball_im::Vector<_> = entries.into_iter().collect();
            for diff in batch {
                diff.map(|item| crate::timeline::entry(&item, &open.me)).apply(&mut mirror);
            }
            entries = mirror.into_iter().collect();
            let readers = |event: &str| {
                entries.iter().find_map(|entry| match &entry.kind {
                    EntryKind::Message(m) if m.event_id == event => Some(&m.read_receipts),
                    _ => None,
                })
            };
            if readers("$next").is_some_and(|r| r.iter().any(|r| r.user_id == "@b:example.org")) {
                assert!(readers("$root").unwrap().iter().all(|r| r.user_id != "@b:example.org"));
                assert!(readers("$next").unwrap().iter().all(|r| r.user_id != "@thread-only:example.org"));
                return;
            }
        }
        panic!("subscription ended");
    })
    .await
    .expect("receipt update");
}

#[tokio::test]
async fn a_receipt_on_a_room_state_event_should_mark_the_preceding_message() {
    let server = crate::test_server::Server::new(|request| {
        let mut response = crate::test_server::thread_response(request);
        if request.contains("/sync?") {
            let room = &mut response["rooms"]["join"]["!room:example.org"];
            room["timeline"]["events"].as_array_mut().unwrap().push(serde_json::json!({
                "type":"m.room.topic", "state_key":"", "event_id":"$topic", "sender":"@a:example.org",
                "origin_server_ts":10, "content":{"topic":"New topic"}
            }));
            room["ephemeral"] = serde_json::json!({"events":[{
                "type":"m.receipt", "content":{"$topic":{"m.read":{"@b:example.org":{"ts":20}}}}
            }]});
        }
        response
    });
    let account = Account::for_test(&server.url()).await;
    account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.unwrap();
    let open = open_timeline(account, TimelineKey::room("!room:example.org"), None, false).await.unwrap();
    let TimelineDiff::Reset(entries) = open.reset else { panic!("initial reset") };
    assert!(entries.iter().any(|entry| matches!(&entry.kind, EntryKind::Message(m) if m.event_id == "$root" && m.read_receipts.iter().any(|r| r.user_id == "@b:example.org"))));
}
