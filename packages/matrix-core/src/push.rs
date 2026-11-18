//! Push notifications: this device's pusher on each account, and turning one push into what to
//! show.
//!
//! The homeserver sends the app's push gateway (`packages/push-gateway`) only the room and
//! event IDs of what notified (`event_id_only`), and the gateway hands them to Expo's push
//! service, which delivers through FCM or APNs. Several accounts share the device's one token,
//! so every pusher's `default_payload` carries its account, which the gateway copies into each
//! push. The device then fetches and decrypts the event itself, with the
//! account's own store: through the running engine ([`Command::FetchNotification`]), or, while
//! the app isn't running, without it ([`fetch_without_engine`]: iOS's Notification Service
//! Extension, Android's headless push task). ADR 0010 records the design.
//!
//! [`Command::FetchNotification`]: crate::Command::FetchNotification

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use matrix_sdk::Client;
use matrix_sdk::ruma::events::room::message::SyncRoomMessageEvent;
use matrix_sdk::ruma::events::sticker::SyncStickerEvent;
use matrix_sdk::ruma::events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent};
use matrix_sdk::ruma::{EventId, RoomId};
use matrix_sdk_ui::notification_client::{
    NotificationClient, NotificationEvent, NotificationItem, NotificationProcessSetup, NotificationStatus,
};
use serde_json::{Map, Value};

use crate::Error;
use crate::model::{AccountId, Notification, PushContent, PushTarget, Pusher};
use crate::secrets::SecretStore;

/// The key of `default_payload` that names the account a push is for.
pub const ACCOUNT_KEY: &str = "account";

/// How long a push handled without the engine may take: iOS gives a Notification Service
/// Extension about 30 seconds, and the generic text must still go out after it.
const WITHOUT_ENGINE: Duration = Duration::from_secs(22);

/// The pusher the homeserver keeps for `pusher` on `account`.
pub(crate) fn pusher(
    account: &AccountId,
    pusher: &Pusher,
) -> Result<matrix_sdk::ruma::api::client::push::Pusher, Error> {
    use matrix_sdk::ruma::api::client::push::{PusherIds, PusherInit, PusherKind};
    use matrix_sdk::ruma::push::{HttpPusherData, PushFormat};

    let mut data = HttpPusherData::new(pusher.gateway_url.clone());
    data.format = Some(PushFormat::EventIdOnly);
    data.data.insert("default_payload".to_owned(), Value::Object(default_payload(account, pusher.payload.as_deref())?));
    Ok(PusherInit {
        ids: PusherIds::new(pusher.push_key.clone(), pusher.app_id.clone()),
        kind: PusherKind::Http(data),
        app_display_name: pusher.app_display_name.clone(),
        device_display_name: pusher.device_display_name.clone(),
        profile_tag: None,
        lang: pusher.lang.clone(),
    }
    .into())
}

/// What the gateway puts in every push for this pusher: the app's extra JSON object, and the
/// account.
pub(crate) fn default_payload(account: &AccountId, extra: Option<&str>) -> Result<Map<String, Value>, Error> {
    let mut payload = match extra {
        None => Map::new(),
        Some(json) => match serde_json::from_str(json) {
            Ok(Value::Object(object)) => object,
            Ok(_) => return Err(Error::Push("the pusher's payload must be a JSON object".into())),
            Err(error) => return Err(Error::Push(format!("the pusher's payload isn't JSON: {error}"))),
        },
    };
    payload.insert(ACCOUNT_KEY.to_owned(), Value::String(account.to_string()));
    Ok(payload)
}

impl PushTarget {
    /// The target of a push as the device receives it. The gateway sends the pusher's
    /// `default_payload` (with the account), `room_id`, `event_id` and `unread` as the push's
    /// data; Expo's push service delivers that data under `body` (an object in APNs payloads, a
    /// JSON string in FCM's), and Android's task hands it over as `dataString` too. Counts may be
    /// numbers or strings. `None` when it names no event (a badge-only push, someone else's).
    pub fn parse(payload: &Value) -> Option<Self> {
        let data = data_of(payload)?;
        let text = |key: &str| data.get(key)?.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        let count = |key: &str| match data.get(key)? {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        };
        Some(Self {
            account: AccountId::new(text(ACCOUNT_KEY)?),
            room_id: text("room_id")?,
            event_id: text("event_id")?,
            unread: count("unread").or_else(|| count("unread_count")),
        })
    }
}

/// The object holding a push's data: the payload itself, or what Expo nested in it.
fn data_of(payload: &Value) -> Option<std::borrow::Cow<'_, Map<String, Value>>> {
    use std::borrow::Cow;
    let object = payload.as_object()?;
    if object.contains_key(ACCOUNT_KEY) {
        return Some(Cow::Borrowed(object));
    }
    for key in ["body", "dataString"] {
        match object.get(key) {
            Some(Value::Object(nested)) => return Some(Cow::Borrowed(nested)),
            Some(Value::String(json)) => {
                if let Ok(Value::Object(nested)) = serde_json::from_str(json) {
                    return Some(Cow::Owned(nested));
                }
            }
            _ => {}
        }
    }
    None
}

/// What `event_id` in `room_id` should show, fetched and decrypted with `client`'s store.
/// Anything that can't be read shows as [`PushContent::Hidden`] rather than failing: a push
/// always says something unless the event really doesn't notify.
pub(crate) async fn fetch(
    client: &Client,
    account: &AccountId,
    room_id: &str,
    event_id: &str,
) -> Result<PushContent, Error> {
    let (room, event) = (RoomId::parse(room_id)?, EventId::parse(event_id)?);
    let hidden = || PushContent::Hidden {
        account: account.clone(),
        room_id: room_id.to_owned(),
        room_name: client.get_room(&room).and_then(|room| room.cached_display_name()).map(|name| name.to_string()),
        event_id: event_id.to_owned(),
    };
    // Fetching a push is no sign of the user: its syncs mark no one present, whatever the status
    // (`crate::status`). This sets the default for the syncs the SDK makes itself; the engine's
    // own sync says its presence each time.
    let _ = client.set_presence(matrix_sdk::ruma::presence::PresenceState::Offline, None, false).await;
    // Multiple processes: the engine's own sync or another process may run beside it, so its
    // writes to the crypto store go through the SDK's cross-process lock.
    let notifications = match NotificationClient::new(client.clone(), NotificationProcessSetup::MultipleProcesses).await
    {
        Ok(notifications) => notifications,
        Err(error) => {
            eprintln!("matrix-core: could not start fetching a push: {error}");
            return Ok(hidden());
        }
    };
    // Sliding sync first (it also brings invites); servers without it answer through
    // `/context`.
    let status = match notifications.get_notification_with_sliding_sync(&room, &event).await {
        Ok(NotificationStatus::EventNotFound) | Err(_) => {
            notifications.get_notification_with_context(&room, &event).await
        }
        found => found,
    };
    Ok(match status {
        Ok(NotificationStatus::Event(item)) => content_of(account, room_id, event_id, *item),
        Ok(NotificationStatus::EventFilteredOut | NotificationStatus::EventRedacted) => PushContent::Nothing,
        Ok(NotificationStatus::EventNotFound) => hidden(),
        Err(error) => {
            eprintln!("matrix-core: could not fetch a push's event: {error}");
            hidden()
        }
    })
}

/// What a fetched notification shows.
fn content_of(account: &AccountId, room_id: &str, event_id: &str, item: NotificationItem) -> PushContent {
    let sender_name = item.sender_display_name.clone().unwrap_or_else(|| item.event.sender().to_string());
    let room_name = item.room_computed_display_name;
    match item.event {
        NotificationEvent::Invite(_) => PushContent::Invite {
            account: account.clone(),
            room_id: room_id.to_owned(),
            room_name,
            event_id: event_id.to_owned(),
            sender_name,
        },
        NotificationEvent::Timeline(event) => match body_of(&event) {
            Some(body) => PushContent::Message(Notification {
                account: account.clone(),
                room_id: room_id.to_owned(),
                room_name,
                event_id: event_id.to_owned(),
                sender_name,
                body,
            }),
            None => PushContent::Hidden {
                account: account.clone(),
                room_id: room_id.to_owned(),
                room_name: Some(room_name),
                event_id: event_id.to_owned(),
            },
        },
    }
}

/// The plain-text preview of a message or sticker, as the sync's notifications say it. `None`
/// for anything else, an event still encrypted included.
fn body_of(event: &AnySyncTimelineEvent) -> Option<String> {
    let AnySyncTimelineEvent::MessageLike(event) = event else { return None };
    match event {
        AnySyncMessageLikeEvent::RoomMessage(SyncRoomMessageEvent::Original(message)) => {
            Some(crate::convert::content(&message.content.msgtype).plain().to_owned())
        }
        AnySyncMessageLikeEvent::Sticker(SyncStickerEvent::Original(sticker)) => Some(sticker.content.body.clone()),
        _ => None,
    }
}

/// Handles one push without the engine: opens the account's store under `data_dir` with its
/// secrets, fetches and decrypts the event, and closes it again. For a process that doesn't run
/// the engine (iOS's Notification Service Extension) or hasn't started it (Android's headless
/// push task). The account's tokens and crypto store are shared with the engine through the
/// SDK's cross-process locks, so either may run while the other does.
///
/// Gives [`PushContent::Hidden`] when the account's store can't be opened in time.
pub async fn fetch_without_engine(
    data_dir: &Path,
    secrets: Arc<dyn SecretStore>,
    target: &PushTarget,
) -> Result<PushContent, Error> {
    let work = async {
        let client = crate::account::open_for_push(data_dir, secrets, &target.account).await?;
        fetch(&client, &target.account, &target.room_id, &target.event_id).await
    };
    match tokio::time::timeout(WITHOUT_ENGINE, work).await {
        Ok(Ok(content)) => Ok(content),
        // An account this device doesn't know (any more): nothing of it should show.
        Ok(Err(Error::UnknownAccount(_))) => Ok(PushContent::Nothing),
        Ok(Err(error)) => {
            eprintln!("matrix-core: could not handle a push for {}: {error}", target.account);
            Ok(hidden(target))
        }
        Err(_) => Ok(hidden(target)),
    }
}

/// Installs the TLS provider a process needs before its first request; the engine does this
/// itself, a process handling a push without it calls this first.
pub fn install_tls() {
    crate::tls::install_provider();
}

/// The generic notification for a push whose event couldn't be read.
pub fn hidden(target: &PushTarget) -> PushContent {
    PushContent::Hidden {
        account: target.account.clone(),
        room_id: target.room_id.clone(),
        room_name: None,
        event_id: target.event_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn fetching_a_push_should_not_mark_the_user_present() {
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = requests.clone();
        let server = crate::test_server::Server::new(move |request| {
            captured.lock().unwrap().push(request.lines().next().unwrap_or_default().to_owned());
            json!({})
        });
        let account = crate::Account::for_test(&server.url()).await;
        account.client().set_sliding_sync_version(matrix_sdk::sliding_sync::Version::Native);
        let _ = fetch(account.client(), account.id(), "!room:example.org", "$event").await;
        let requests = requests.lock().unwrap();
        let syncs: Vec<_> = requests.iter().filter(|line| line.contains("/sync")).collect();
        assert!(!syncs.is_empty(), "no sync in {requests:?}");
        assert!(syncs.iter().all(|line| line.contains("set_presence=offline")), "{syncs:?}");
    }

    fn account() -> AccountId {
        AccountId::new("@alice:example.org")
    }

    #[test]
    fn every_push_should_carry_its_account() {
        let payload = default_payload(&account(), None).unwrap();
        assert_eq!(payload.get(ACCOUNT_KEY), Some(&json!("@alice:example.org")));
    }

    #[test]
    fn the_apps_payload_should_join_the_account() {
        let payload = default_payload(&account(), Some(r#"{"alert":{"title":"Better Matrix"}}"#)).unwrap();
        assert_eq!(
            Value::Object(payload),
            json!({"alert": {"title": "Better Matrix"}, "account": "@alice:example.org"})
        );
    }

    #[test]
    fn a_payload_that_is_no_json_object_should_be_refused() {
        assert!(matches!(default_payload(&account(), Some("[1]")), Err(Error::Push(_))));
        assert!(matches!(default_payload(&account(), Some("{")), Err(Error::Push(_))));
    }

    #[test]
    fn a_pusher_should_ask_for_event_ids_only() {
        let registered = pusher(
            &account(),
            &Pusher {
                app_id: "chat.bettermatrix.mobile".into(),
                push_key: "ExponentPushToken[x]".into(),
                gateway_url: "https://push.example.org/_matrix/push/v1/notify".into(),
                app_display_name: "Better Matrix".into(),
                device_display_name: "Pixel".into(),
                lang: "de".into(),
                payload: None,
            },
        )
        .unwrap();
        let json = serde_json::to_value(&registered).unwrap();
        assert_eq!(json["kind"], "http");
        assert_eq!(json["data"]["format"], "event_id_only");
        assert_eq!(json["data"]["url"], "https://push.example.org/_matrix/push/v1/notify");
        assert_eq!(json["data"]["default_payload"]["account"], "@alice:example.org");
        assert_eq!((json["pushkey"].as_str(), json["lang"].as_str()), (Some("ExponentPushToken[x]"), Some("de")));
    }

    fn target(unread: Option<u64>) -> PushTarget {
        PushTarget { account: account(), room_id: "!r:example.org".into(), event_id: "$e".into(), unread }
    }

    #[test]
    fn an_apns_push_from_expo_should_name_its_account_and_event() {
        let push = json!({
            "aps": {"alert": {"title": "Better Matrix", "body": "New message"}, "mutable-content": 1},
            "body": {"account": "@alice:example.org", "room_id": "!r:example.org", "event_id": "$e", "unread": 3},
            "experienceId": "@owner/better-matrix",
            "scopeKey": "@owner/better-matrix"
        });
        assert_eq!(PushTarget::parse(&push), Some(target(Some(3))));
    }

    #[test]
    fn an_fcm_push_from_expo_should_read_its_data_from_json() {
        let data = r#"{"account":"@alice:example.org","room_id":"!r:example.org","event_id":"$e","unread":"2"}"#;
        let push = json!({"body": data, "dataString": data, "experienceId": "@owner/better-matrix"});
        assert_eq!(PushTarget::parse(&push), Some(target(Some(2))));
    }

    #[test]
    fn flat_push_data_should_name_its_event() {
        let push = json!({"account": "@alice:example.org", "room_id": "!r:example.org", "event_id": "$e"});
        assert_eq!(PushTarget::parse(&push), Some(target(None)));
    }

    #[test]
    fn a_push_without_an_event_should_name_nothing() {
        assert_eq!(PushTarget::parse(&json!({"account": "@alice:example.org", "unread_count": 0})), None);
    }

    #[test]
    fn a_push_without_an_account_should_name_nothing() {
        assert_eq!(PushTarget::parse(&json!({"room_id": "!r", "event_id": "$e"})), None);
    }

    #[test]
    fn a_message_should_preview_its_text() {
        let event: AnySyncTimelineEvent = serde_json::from_value(json!({
            "type": "m.room.message", "event_id": "$e", "sender": "@bob:example.org",
            "origin_server_ts": 1, "content": {"msgtype": "m.text", "body": "hello"}
        }))
        .unwrap();
        assert_eq!(body_of(&event).as_deref(), Some("hello"));
    }

    #[test]
    fn an_encrypted_event_should_have_no_preview() {
        let event: AnySyncTimelineEvent = serde_json::from_value(json!({
            "type": "m.room.encrypted", "event_id": "$e", "sender": "@bob:example.org", "origin_server_ts": 1,
            "content": {"algorithm": "m.megolm.v1.aes-sha2", "ciphertext": "x", "session_id": "s",
                        "sender_key": "k", "device_id": "D"}
        }))
        .unwrap();
        assert_eq!(body_of(&event), None);
    }

    #[test]
    fn a_push_for_an_unknown_account_should_show_nothing() {
        let data = tempfile_dir();
        let target =
            PushTarget { account: account(), room_id: "!r:example.org".into(), event_id: "$e".into(), unread: None };
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let content = runtime
            .block_on(fetch_without_engine(&data, Arc::new(crate::secrets::MemorySecrets::default()), &target))
            .unwrap();
        let _ = std::fs::remove_dir_all(&data);
        assert_eq!(content, PushContent::Nothing);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("matrix-core-push-{}-{:?}", std::process::id(), std::thread::current().id()))
    }
}
