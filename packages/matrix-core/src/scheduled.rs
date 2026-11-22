//! Scheduled messages: MSC4140's delayed events. The server holds a message and sends it when
//! its delay runs out, whether or not any of the user's devices is online; until then it can
//! be listed, sent at once or cancelled.
//!
//! In an encrypted room the message is encrypted before the server holds it, as the room's
//! other messages are: the SDK encrypts only what it sends itself, so this does what its send
//! path does first, through the account's own base client ([`Crypto`]): members synced, keys
//! of members whose devices aren't known queried, sessions claimed, the room key shared, then
//! the content encrypted with the room's current session. The session may rotate before the
//! message goes; the receivers keep every session they were given, so it still decrypts.

use std::time::Duration;

use matrix_sdk::Room;
use matrix_sdk::ruma::api::client::delayed_events::{
    DelayParameters, delayed_message_event, get_all_delayed_events, update_delayed_event,
};
use matrix_sdk::ruma::events::{AnyMessageLikeEventContent, MessageLikeEventContent, MessageLikeEventType};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedUserId, TransactionId};
use serde_json::Value;

use crate::account::Crypto;
use crate::model::{Outgoing, ScheduledAction, ScheduledMessage, TimelineKey};
use crate::{Account, Error};

/// The longest wait the app offers: a server may allow less, and refuses a longer one.
pub const LONGEST_DELAY: Duration = Duration::from_secs(60 * 60 * 24 * 30);

impl Account {
    /// Asks the server to send `message` into `key` after `delay`, as a reply to `reply_to`
    /// when set, in its thread when `key` is one. Uploads (files, voice memos) can't be
    /// scheduled. Returns the server's ID for it.
    pub async fn schedule(
        &self,
        key: &TimelineKey,
        reply_to: Option<&str>,
        message: &Outgoing,
        delay: Duration,
    ) -> Result<String, Error> {
        let room = self
            .client()
            .get_room(&matrix_sdk::ruma::RoomId::parse(&key.room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: key.room_id.clone() })?;
        let content = crate::outgoing::event(message)?.ok_or(Error::MessageUnavailable)?;
        let event_type = content.event_type().to_string();
        let mut json = serde_json::to_value(&content).map_err(|e| Error::Storage(e.into()))?;
        relate(&mut json, key.thread.as_deref(), reply_to);
        let raw: Raw<AnyMessageLikeEventContent> =
            Raw::new(&json).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        let (event_type, raw) = match room.latest_encryption_state().await?.is_encrypted() {
            true => {
                let crypto = self.crypto.as_ref().ok_or(Error::MessageUnavailable)?;
                ("m.room.encrypted".to_owned(), encrypt(self, crypto, &room, &event_type, &raw).await?)
            }
            false => (event_type, raw),
        };
        let request = delayed_message_event::unstable::Request::new_raw(
            room.room_id().to_owned(),
            TransactionId::new(),
            MessageLikeEventType::from(event_type),
            DelayParameters::Timeout { timeout: delay.min(LONGEST_DELAY) },
            raw,
        );
        Ok(self.client().send(request).await?.delay_id)
    }

    /// The messages the server holds for the account, soonest first. A message this device
    /// can't read (another device's encrypted one) has no body.
    pub async fn scheduled_messages(&self) -> Result<Vec<ScheduledMessage>, Error> {
        let response = self.client().send(get_all_delayed_events::unstable::Request::new()).await?;
        let mut messages = Vec::new();
        for event in response.delayed_events {
            // Delayed state (a call membership's own expiry) isn't a message.
            if event.state_key.is_some() {
                continue;
            }
            let content: Value = event.content.deserialize_as_unchecked().unwrap_or_default();
            let body = match event.event_type.to_string().as_str() {
                "m.room.encrypted" => self.decrypt_scheduled(&event.room_id, &content).await,
                _ => body_of(&content),
            };
            let since = u64::from(event.running_since.0);
            let delay = u64::try_from(event.delay.as_millis()).unwrap_or(u64::MAX);
            messages.push(ScheduledMessage {
                delay_id: event.delay_id,
                room_id: event.room_id.to_string(),
                send_at_ms: since.saturating_add(delay),
                body,
            });
        }
        messages.sort_by_key(|message| message.send_at_ms);
        Ok(messages)
    }

    /// Cancels a scheduled message, or sends it at once.
    pub async fn update_scheduled(&self, delay_id: &str, action: ScheduledAction) -> Result<(), Error> {
        let action = match action {
            ScheduledAction::Cancel => update_delayed_event::UpdateAction::Cancel,
            ScheduledAction::SendNow => update_delayed_event::UpdateAction::Send,
        };
        let request = update_delayed_event::unstable_v1::Request::new(delay_id.to_owned(), action);
        self.client().send(request).await?;
        Ok(())
    }

    /// The body of an encrypted scheduled message, when this device holds its key.
    async fn decrypt_scheduled(&self, room_id: &matrix_sdk::ruma::RoomId, content: &Value) -> Option<String> {
        let room = self.client().get_room(room_id)?;
        let event = serde_json::json!({
            "type": "m.room.encrypted",
            "content": content,
            "sender": self.id().as_str(),
            "event_id": "$scheduled",
            "origin_server_ts": 0,
            "room_id": room_id.as_str(),
        });
        let raw = Raw::new(&event).ok()?.cast_unchecked();
        let decrypted = room.decrypt_event(&raw, None).await.ok()?;
        let event: Value = decrypted.raw().deserialize_as_unchecked().ok()?;
        body_of(&event["content"])
    }
}

/// A message's plain body, if it has one.
fn body_of(content: &Value) -> Option<String> {
    content["body"]
        .as_str()
        .or_else(|| content["org.matrix.msc3381.poll.start"]["question"]["body"].as_str())
        .map(str::to_owned)
}

/// Adds the relations a message sent now would get: into the thread `thread` (answering its
/// root, with the reply fallback clients without threads read), or as a reply to `reply_to`.
fn relate(content: &mut Value, thread: Option<&str>, reply_to: Option<&str>) {
    let relation = match (thread, reply_to) {
        (Some(root), reply) => serde_json::json!({
            "rel_type": "m.thread",
            "event_id": root,
            "is_falling_back": reply.is_none(),
            "m.in_reply_to": { "event_id": reply.unwrap_or(root) },
        }),
        (None, Some(reply)) => serde_json::json!({ "m.in_reply_to": { "event_id": reply } }),
        (None, None) => return,
    };
    content["m.relates_to"] = relation;
}

/// What the SDK does before it sends into an encrypted room, then the encryption itself: the
/// content as `m.room.encrypted` content for the room's current session.
async fn encrypt(
    account: &Account,
    crypto: &Crypto,
    room: &Room,
    event_type: &str,
    content: &Raw<AnyMessageLikeEventContent>,
) -> Result<Raw<AnyMessageLikeEventContent>, Error> {
    let client = account.client();
    // Another process sharing the store (a push extension) never writes it at the same time.
    let _lock = client.encryption().spin_lock_store(Some(60_000)).await?;
    if !room.are_members_synced() {
        room.sync_members().await?;
    }
    let members: Vec<OwnedUserId> = room
        .members(matrix_sdk::RoomMemberships::ACTIVE)
        .await?
        .iter()
        .map(|member| member.user_id().to_owned())
        .collect();
    let machine = crypto.0.olm_machine().await;
    let olm = machine.as_ref().ok_or(Error::MessageUnavailable)?;
    let crypto_failed =
        |error: matrix_sdk_base::crypto::OlmError| Error::Sdk(matrix_sdk::Error::OlmError(Box::new(error)));
    let store_failed = |error: matrix_sdk_base::crypto::CryptoStoreError| {
        Error::Sdk(matrix_sdk::Error::CryptoStoreError(Box::new(error)))
    };
    // Members whose devices this device doesn't know, or knows from before a change.
    let tracked: std::collections::HashMap<OwnedUserId, bool> = olm
        .store()
        .load_tracked_users()
        .await
        .map_err(store_failed)?
        .into_iter()
        .map(|user| (user.user_id, user.dirty))
        .collect();
    let unknown: Vec<OwnedUserId> =
        members.iter().filter(|member| tracked.get(*member).is_none_or(|dirty| *dirty)).cloned().collect();
    if !unknown.is_empty() {
        olm.update_tracked_users(unknown.iter().map(std::ops::Deref::deref)).await.map_err(store_failed)?;
        let (request_id, request) = olm.query_keys_for_users(unknown.iter().map(std::ops::Deref::deref));
        let mut query = matrix_sdk::ruma::api::client::keys::get_keys::v3::Request::new();
        query.device_keys = request.device_keys;
        let response = client.send(query).await?;
        olm.mark_request_as_sent(&request_id, &response).await.map_err(crypto_failed)?;
    }
    if let Some((request_id, request)) =
        olm.get_missing_sessions(members.iter().map(std::ops::Deref::deref)).await.map_err(store_failed)?
    {
        let response = client.send(request).await?;
        olm.mark_request_as_sent(&request_id, &response).await.map_err(crypto_failed)?;
    }
    // `share_room_key` takes the machine itself.
    drop(machine);
    for request in crypto.0.share_room_key(room.room_id()).await.map_err(|e| Error::Sdk(e.into()))? {
        let send = matrix_sdk::ruma::api::client::to_device::send_event_to_device::v3::Request::new_raw(
            request.event_type.clone(),
            request.txn_id.clone(),
            request.messages.clone(),
        );
        let response = client.send(send).await?;
        let olm = crypto.0.olm_machine().await;
        olm.as_ref()
            .ok_or(Error::MessageUnavailable)?
            .mark_request_as_sent(&request.txn_id, &response)
            .await
            .map_err(crypto_failed)?;
    }
    let olm = crypto.0.olm_machine().await;
    let encrypted = olm
        .as_ref()
        .ok_or(Error::MessageUnavailable)?
        .encrypt_room_event_raw(room.room_id(), event_type, content)
        .await
        .map_err(|e| Error::Sdk(matrix_sdk::Error::MegolmError(Box::new(e))))?;
    Ok(encrypted.content.cast_unchecked())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_server::Server;

    #[test]
    fn a_scheduled_reply_should_answer_its_message() {
        let mut content = json!({ "msgtype": "m.text", "body": "later" });
        relate(&mut content, None, Some("$q"));
        assert_eq!(content["m.relates_to"], json!({ "m.in_reply_to": { "event_id": "$q" } }));
    }

    #[test]
    fn a_scheduled_thread_message_should_join_its_thread() {
        let mut content = json!({ "msgtype": "m.text", "body": "later" });
        relate(&mut content, Some("$root"), None);
        assert_eq!(content["m.relates_to"]["rel_type"], "m.thread");
        assert_eq!(content["m.relates_to"]["m.in_reply_to"]["event_id"], "$root");
        assert_eq!(content["m.relates_to"]["is_falling_back"], true);
    }

    /// A server that holds what is sent, and a room `!room:example.org` (encrypted when
    /// `encrypted`) synced from it.
    fn server(encrypted: bool) -> (Server, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sent.clone();
        let server = Server::new(move |request| {
            captured.lock().expect("requests").push(request.to_owned());
            if request.contains("org.matrix.msc4140.delay=") {
                return json!({ "delay_id": "d1" });
            }
            if request.contains("/versions") {
                return json!({ "versions": ["v1.12"] });
            }
            if !encrypted && request.contains("/state/m.room.encryption") {
                return json!({ "errcode": "M_NOT_FOUND", "error": "no encryption" });
            }
            if request.contains("/joined_members") || request.contains("/members") {
                return json!({ "chunk": [
                    {"type":"m.room.member","state_key":"@a:example.org","sender":"@a:example.org","event_id":"$join","origin_server_ts":0,"content":{"membership":"join"}}
                ] });
            }
            if request.contains("/keys/claim") {
                return json!({ "one_time_keys": {} });
            }
            let mut response = crate::test_server::thread_response(request);
            if encrypted && request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().expect("state").push(json!({
                    "type":"m.room.encryption","state_key":"","sender":"@a:example.org","event_id":"$e","origin_server_ts":0,
                    "content":{"algorithm":"m.megolm.v1.aes-sha2"}
                }));
            }
            response
        });
        (server, sent)
    }

    fn store(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("matrix-core-scheduled-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The send that scheduled a message: its request line and body.
    fn scheduled(sent: &std::sync::Mutex<Vec<String>>) -> (String, Value) {
        let sent = sent.lock().expect("requests");
        let request =
            sent.iter().find(|request| request.contains("org.matrix.msc4140.delay=")).expect("a scheduled send");
        let line = request.lines().next().unwrap_or_default().to_owned();
        let body = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).expect("a JSON body");
        (line, body)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_should_be_scheduled_with_its_delay() {
        let (server, sent) = server(false);
        let dir = store("plain");
        let account = Account::for_test_with_store(&server.url(), &dir).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO))
            .await
            .expect("sync");
        let message = Outgoing::Text { body: "later".into(), html: None, mentions: Default::default() };
        let id =
            account.schedule(&TimelineKey::room("!room:example.org"), None, &message, Duration::from_secs(90)).await;
        assert_eq!(id.expect("scheduled"), "d1");
        let (line, body) = scheduled(&sent);
        assert!(line.starts_with("PUT /_matrix/client/v3/rooms/!room:example.org/send/m.room.message/"), "{line}");
        assert!(line.contains("org.matrix.msc4140.delay=90000"), "{line}");
        assert_eq!(body["body"], "later");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_scheduled_in_an_encrypted_room_should_be_encrypted() {
        let (server, sent) = server(true);
        let dir = store("encrypted");
        let account = Account::for_test_with_store(&server.url(), &dir).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO))
            .await
            .expect("sync");
        let message = Outgoing::Text { body: "a secret".into(), html: None, mentions: Default::default() };
        let id =
            account.schedule(&TimelineKey::room("!room:example.org"), None, &message, Duration::from_secs(60)).await;
        assert_eq!(id.expect("scheduled"), "d1");
        let (line, body) = scheduled(&sent);
        assert!(line.contains("/send/m.room.encrypted/"), "{line}");
        assert_eq!(body["algorithm"], "m.megolm.v1.aes-sha2");
        assert!(body["ciphertext"].is_string() && !body.to_string().contains("a secret"), "{body}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_encrypted_scheduled_message_should_list_with_its_body() {
        let (server, sent) = server(true);
        let dir = store("listed");
        let account = Account::for_test_with_store(&server.url(), &dir).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO))
            .await
            .expect("sync");
        let message = Outgoing::Text { body: "a secret".into(), html: None, mentions: Default::default() };
        account
            .schedule(&TimelineKey::room("!room:example.org"), None, &message, Duration::from_secs(60))
            .await
            .expect("scheduled");
        let (_, body) = scheduled(&sent);
        let content = account.decrypt_scheduled(matrix_sdk::ruma::room_id!("!room:example.org"), &body).await;
        assert_eq!(content.as_deref(), Some("a secret"));
    }
}
