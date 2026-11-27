use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::{Action, NewMessage, run};
use crate::test_server::{Server, thread_response};
use crate::{Account, Mentions, Outgoing, SearchUsage, TimelineKey};

const ROOM: &str = "!room:example.org";

fn fixture_response(request: &str) -> Value {
    if request.contains("/versions") {
        return json!({ "versions": ["v1.11"], "unstable_features": {} });
    }
    thread_response(request)
}

async fn synced(server: &Server) -> Account {
    let account = Account::for_test(&server.url()).await;
    account
        .client()
        .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
        .await
        .expect("sync");
    account
}

async fn execute(account: Account, action: Action) -> Value {
    let result = Mutex::new(None);
    run(
        account,
        action,
        SearchUsage::default(),
        |value| {
            *result.lock().expect("result") = Some(value);
        },
        None,
    )
    .await
    .expect("operation");
    result.into_inner().expect("result").expect("one result")
}

#[tokio::test]
async fn a_confirmed_send_should_return_the_homeserver_event_id() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&requests);
    let server = Server::new(move |request| {
        if request.starts_with("PUT ") && request.contains("/send/m.room.message/retry-deploy") {
            observed.lock().expect("requests").push(request.to_owned());
            return json!({ "event_id": "$accepted" });
        }
        if request.contains("/state/m.room.encryption") {
            return json!({ "errcode": "M_NOT_FOUND", "error": "This room is not encrypted" });
        }
        fixture_response(request)
    });
    let account = synced(&server).await;
    let action = Action::Send(NewMessage::Text {
        key: TimelineKey::room(ROOM),
        message: Outgoing::Text {
            body: "Deploy complete".to_owned(),
            html: None,
            mentions: Mentions { user_ids: vec!["@b:example.org".into()], room: false },
        },
        reply_to: None,
        transaction_id: Some("retry-deploy".to_owned()),
    });
    let result = execute(account, action).await;
    assert_eq!(result, json!({ "room_id": ROOM, "event_id": "$accepted" }));
    assert!(requests.lock().expect("requests")[0].contains(r#""m.mentions":{"user_ids":["@b:example.org"]}"#));
}

#[tokio::test]
async fn a_history_page_should_return_the_servers_continuation_cursor() {
    let server = Server::new(|request| {
        if request.starts_with("GET ") && request.contains("/messages?") {
            return json!({
                "start": "here",
                "end": "earlier",
                "chunk": [{
                    "type": "m.room.message",
                    "room_id": ROOM,
                    "event_id": "$first",
                    "sender": "@b:example.org",
                    "origin_server_ts": 100,
                    "content": { "msgtype": "m.text", "body": "A message" }
                }]
            });
        }
        fixture_response(request)
    });
    let account = synced(&server).await;
    let action = Action::History { key: TimelineKey::room(ROOM), limit: 1, cursor: None };
    let result = execute(account, action).await;
    assert_eq!(result["next_cursor"], "earlier");
    assert_eq!(result["messages"][0]["event_id"], "$first");
}

const USER: &str = "@a:example.org";
const DEVICE: &str = "BM";

fn login(server: &Server, user: &str, resume: bool) -> crate::LoginRequest {
    crate::LoginRequest {
        homeserver: server.url(),
        username: user.into(),
        password: "test".into(),
        resume: resume.then(|| crate::AccountId::new(user)),
    }
}

#[tokio::test]
async fn an_unexpected_login_should_leave_the_saved_account_untouched() {
    let server = Server::new(|request| {
        if request.starts_with("POST ") && request.contains("/login") {
            return json!({ "user_id": USER, "device_id": DEVICE, "access_token": "test" });
        }
        fixture_response(request)
    });
    let dir = tempfile::tempdir().expect("data");
    let root = crate::StoreLayout::new(dir.path());
    let sessions = crate::session::Sessions::new(dir.path(), Arc::new(crate::MemorySecrets::default()));
    let first =
        Account::login_password(&root, &sessions, crate::account::Holder::Alone, &login(&server, USER, false), true)
            .await
            .expect("initial login");
    drop(first);
    let saved = sessions.list()[0].store_dir.clone();
    let result = Account::login_password(
        &root,
        &sessions,
        crate::account::Holder::Alone,
        &login(&server, "@someone:example.org", false),
        true,
    )
    .await;
    assert!(matches!(result, Err(crate::Error::UnexpectedAccount { .. })));
    assert_eq!(sessions.list()[0].store_dir, saved);
    assert!(saved.exists());
}

async fn receive(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<crate::Event>,
    matches: impl Fn(&crate::Event) -> bool,
) -> crate::Event {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("engine running");
            if matches(&event) {
                return event;
            }
        }
    })
    .await
    .expect("event in time")
}

fn engine(
    dir: &std::path::Path,
    secrets: Arc<dyn crate::SecretStore>,
    watch: bool,
) -> (crate::EngineHandle, tokio::sync::mpsc::UnboundedReceiver<crate::Event>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let engine = crate::Engine::start_terminal(
        crate::EngineConfig {
            data_dir: dir.to_owned(),
            secrets,
            identity: crate::ClientIdentity {
                name: "Better Matrix CLI test".into(),
                uri: "https://example.org/".parse().expect("URL"),
            },
            oauth_redirect: crate::OAuthRedirect::Loopback,
            stores: crate::StoreSharing::Alone,
            scans_qr: false,
        },
        move |event| {
            let _ = tx.send(event);
        },
        watch,
    )
    .expect("engine");
    (engine, rx)
}

async fn signed_in(
    engine: &crate::EngineHandle,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<crate::Event>,
    request: crate::LoginRequest,
) {
    let request = engine.send(crate::Command::Login(request));
    let event =
        receive(events, |event| matches!(event, crate::Event::Finished { request: id, .. } if *id == request)).await;
    assert!(matches!(event, crate::Event::Finished { result: Ok(()), .. }), "{event:?}");
}

#[tokio::test]
async fn a_terminal_soft_logout_should_keep_the_original_device_for_the_next_process() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let expired = Arc::new(AtomicBool::new(false));
    let reject = Arc::clone(&expired);
    let server = Server::new(move |request| {
        if request.starts_with("POST ") && request.contains("/login") {
            let Some((_, body)) = request.split_once("\r\n\r\n") else { return json!({}) };
            let body: Value = serde_json::from_str(body).expect("login request");
            if let Some(device) = body["device_id"].as_str() {
                assert_eq!(device, DEVICE);
            }
            return json!({ "user_id": USER, "device_id": DEVICE, "access_token": "test" });
        }
        if reject.load(Ordering::Relaxed) && (request.contains("/whoami") || request.contains("/sync?")) {
            return json!({ "errcode": "M_UNKNOWN_TOKEN", "error": "expired", "soft_logout": true });
        }
        fixture_response(request)
    });
    let dir = tempfile::tempdir().expect("data");
    let secrets: Arc<dyn crate::SecretStore> = Arc::new(crate::MemorySecrets::default());
    let sessions = crate::session::Sessions::new(dir.path(), Arc::clone(&secrets));
    let (first, mut events) = engine(dir.path(), Arc::clone(&secrets), false);
    signed_in(&first, &mut events, login(&server, USER, false)).await;
    let original = sessions.list()[0].store_dir.clone();
    expired.store(true, Ordering::Relaxed);
    first.send(crate::Command::Terminal {
        account: crate::AccountId::new(USER),
        action: Box::new(Action::ValidateSession),
    });
    receive(&mut events, |event| matches!(event, crate::Event::SessionEnded { kept: true, .. })).await;
    assert!(sessions.list()[0].signed_out);
    let stopped = first.stopped();
    drop(first);
    stopped.wait(std::time::Duration::from_secs(5));

    expired.store(false, Ordering::Relaxed);
    let (second, mut events) = engine(dir.path(), secrets, false);
    receive(&mut events, |event| matches!(event, crate::Event::SessionEnded { kept: true, .. })).await;
    signed_in(&second, &mut events, login(&server, USER, true)).await;
    assert_eq!(sessions.list()[0].store_dir, original);
}

#[tokio::test]
async fn terminal_sync_should_drain_to_device_events_without_announcing_presence() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let syncs = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&syncs);
    let server = Server::new(move |request| {
        if request.starts_with("POST ") && request.contains("/login") {
            return json!({ "user_id": USER, "device_id": DEVICE, "access_token": "test" });
        }
        if request.contains("/sync?") {
            let index = count.fetch_add(1, Ordering::Relaxed);
            assert!(request.contains("set_presence=offline"), "{request}");
            if index < 2 {
                assert!(request.contains("timeout=0"), "{request}");
            }
            if index == 0 {
                return json!({
                    "next_batch": "first",
                    "to_device": { "events": [{ "type": "m.dummy", "sender": "@b:example.org", "content": {} }] },
                });
            }
            return json!({ "next_batch": format!("after-{index}") });
        }
        fixture_response(request)
    });
    let dir = tempfile::tempdir().expect("data");
    let (terminal, mut events) = engine(dir.path(), Arc::new(crate::MemorySecrets::default()), true);
    signed_in(&terminal, &mut events, login(&server, USER, false)).await;
    receive(&mut events, |event| matches!(event, crate::Event::SyncReady(_))).await;
    assert_eq!(syncs.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn a_watch_should_backfill_a_limited_sync_before_emitting_its_live_events() {
    let gap = vec![watch_message("$second", 3), watch_message("$first", 2), watch_message("$root", 1)];
    let server = watch_server(true, gap);
    let dir = tempfile::tempdir().expect("data");
    let (_terminal, mut events, request) = start_watch(&server, dir.path(), Some(TimelineKey::room(ROOM))).await;
    for expected in ["$first", "$second", "$latest"] {
        let value = watch_result(&mut events, request).await.expect("watch event");
        assert_eq!(value["event_id"], expected);
    }
}

#[tokio::test]
async fn a_watch_should_report_an_encrypted_gap_it_cannot_recover() {
    let mut encrypted = watch_message("$encrypted", 2);
    encrypted["type"] = json!("m.room.encrypted");
    encrypted["content"] = json!({
        "algorithm": "m.megolm.v1.aes-sha2",
        "session_id": "ZFD6+OmV7fVCsJ7Gap8UnORH8EnmiAkes8FAvQuCw/I",
        "sender_key": "WJ6Ce7U67a6jqkHYHd8o0+5H4bqdi9hInZdk0+swuXs",
        "ciphertext": "AwgAEiBQs2LgBD2CcB+RLH2bsgp9VadFUJhBXOtCmcJuttBDOeDNjL21d9z0AcVSfQFAh9huh4or7sWuNrHcvu9/sMbweTgc0UtdA5xFLheubHouXy4aewze+ShndWAaTbjWJMLsPSQDUMQHBA"
    });
    let server = watch_server(true, vec![encrypted, watch_message("$root", 1)]);
    let dir = tempfile::tempdir().expect("data");
    let (_terminal, mut events, request) = start_watch(&server, dir.path(), Some(TimelineKey::room(ROOM))).await;
    let error = watch_result(&mut events, request).await.expect_err("unrecoverable gap");
    assert!(error.contains("could not recover a gap"), "{error}");
}

#[tokio::test]
async fn a_watch_should_start_a_new_room_at_its_first_observed_timeline() {
    let server = watch_server(false, Vec::new());
    let dir = tempfile::tempdir().expect("data");
    let (_terminal, mut events, request) = start_watch(&server, dir.path(), None).await;
    let value = watch_result(&mut events, request).await.expect("first room event");
    assert_eq!(value["event_id"], "$latest");
}

fn watch_message(id: &str, timestamp: u32) -> Value {
    json!({
        "type": "m.room.message", "room_id": ROOM, "event_id": id,
        "sender": "@b:example.org", "origin_server_ts": timestamp,
        "content": { "msgtype": "m.text", "body": id }
    })
}

fn watch_server(initially_joined: bool, gap: Vec<Value>) -> Server {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let syncs = AtomicUsize::new(0);
    Server::new(move |request| {
        if request.starts_with("POST ") && request.contains("/login") {
            return json!({ "user_id": USER, "device_id": DEVICE, "access_token": "test" });
        }
        if request.contains("/messages?") {
            assert!(initially_joined);
            assert!(request.contains("from=gap-cursor"));
            return json!({ "start": "gap-cursor", "chunk": gap });
        }
        if request.contains("/sync?") {
            match syncs.fetch_add(1, Ordering::Relaxed) {
                0 if initially_joined => return fixture_response(request),
                0 => return json!({ "next_batch": "before-join" }),
                1 => {
                    let mut reply = fixture_response(request);
                    reply["next_batch"] = json!("live");
                    reply["rooms"]["join"][ROOM]["timeline"] = json!({
                        "events": [watch_message("$latest", 4)], "limited": true, "prev_batch": "gap-cursor"
                    });
                    return reply;
                }
                index => return json!({ "next_batch": format!("sync-{index}") }),
            }
        }
        fixture_response(request)
    })
}

async fn start_watch(
    server: &Server,
    dir: &std::path::Path,
    key: Option<TimelineKey>,
) -> (crate::EngineHandle, tokio::sync::mpsc::UnboundedReceiver<crate::Event>, crate::RequestId) {
    let (terminal, mut events) = engine(dir, Arc::new(crate::MemorySecrets::default()), true);
    signed_in(&terminal, &mut events, login(server, USER, false)).await;
    receive(&mut events, |event| matches!(event, crate::Event::SyncReady(_))).await;
    let request = terminal.send(crate::Command::Terminal {
        account: crate::AccountId::new(USER),
        action: Box::new(Action::Watch { key, highlights: false }),
    });
    (terminal, events, request)
}

async fn watch_result(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<crate::Event>,
    request: crate::RequestId,
) -> Result<Value, String> {
    let event = receive(events, |event| match event {
        crate::Event::TerminalResult(id, _) | crate::Event::Finished { request: id, .. } => *id == request,
        _ => false,
    })
    .await;
    match event {
        crate::Event::TerminalResult(_, value) => Ok(value),
        crate::Event::Finished { result: Err(error), .. } => Err(error),
        _ => panic!("watch should end with an error: {event:?}"),
    }
}

#[tokio::test]
async fn password_login_should_pass_its_credentials_to_cross_signing_setup() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let signed = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&signed);
    let server = Server::new(move |request| {
        if request.starts_with("POST ") && request.contains("/login") {
            return json!({ "user_id": USER, "device_id": DEVICE, "access_token": "test" });
        }
        if request.contains("/keys/device_signing/upload") {
            let body = request.split_once("\r\n\r\n").expect("request body").1;
            let body: Value = serde_json::from_str(body).expect("signing request");
            if body["auth"]["type"] == "m.login.password"
                && body["auth"]["identifier"]["user"] == USER
                && body["auth"]["password"] == "test"
            {
                observed.store(true, Ordering::Relaxed);
            }
        }
        fixture_response(request)
    });
    let dir = tempfile::tempdir().expect("data");
    let root = crate::StoreLayout::new(dir.path());
    let sessions = crate::session::Sessions::new(dir.path(), Arc::new(crate::MemorySecrets::default()));
    Account::login_password(&root, &sessions, crate::account::Holder::Alone, &login(&server, USER, false), true)
        .await
        .expect("initial login");
    assert!(signed.load(Ordering::Relaxed));
}

#[tokio::test]
async fn a_redirected_login_should_preserve_the_suspended_store() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let changed = Arc::new(AtomicBool::new(false));
    let redirect = Arc::clone(&changed);
    let server = Server::new(move |request| {
        if request.starts_with("POST ") && request.contains("/login") {
            let mut response = json!({ "user_id": USER, "device_id": DEVICE, "access_token": "test" });
            if redirect.load(Ordering::Relaxed) {
                response["well_known"] = json!({ "m.homeserver": { "base_url": "http://127.0.0.1:9" } });
            }
            return response;
        }
        fixture_response(request)
    });
    let dir = tempfile::tempdir().expect("data");
    let root = crate::StoreLayout::new(dir.path());
    let sessions = crate::session::Sessions::new(dir.path(), Arc::new(crate::MemorySecrets::default()));
    let account =
        Account::login_password(&root, &sessions, crate::account::Holder::Alone, &login(&server, USER, false), true)
            .await
            .expect("initial login");
    sessions.set_signed_out(account.id()).expect("suspend");
    drop(account);
    let original = sessions.list()[0].store_dir.clone();
    changed.store(true, Ordering::Relaxed);
    let resumed =
        Account::login_password(&root, &sessions, crate::account::Holder::Alone, &login(&server, USER, true), true)
            .await;
    assert!(matches!(resumed, Err(crate::Error::ResumeStore)));
    assert!(sessions.list()[0].signed_out);
    assert_eq!(sessions.list()[0].store_dir, original);
}
