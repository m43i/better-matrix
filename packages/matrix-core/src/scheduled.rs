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
use matrix_sdk::ruma::api::client::delayed_events::update_delayed_event;
use matrix_sdk::ruma::events::{AnyMessageLikeEventContent, MessageLikeEventContent};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedUserId, TransactionId};
use serde_json::Value;

use crate::account::Crypto;
use crate::model::{Outgoing, ScheduledAction, ScheduledMessage, TimelineKey};
use crate::{Account, Error};

mod api;

/// What a server says in `/versions` when it holds delayed events.
const FEATURE: &str = "org.matrix.msc4140";

/// Where the account's store keeps the messages it scheduled.
const KEPT: &[u8] = b"chat.bettermatrix.scheduled";

/// A scheduled message as the store keeps it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Kept {
    delay_id: String,
    room_id: String,
    send_at_ms: u64,
    body: Option<String>,
    #[serde(default)]
    failed: Option<String>,
}

/// How long a message whose time came is kept while what became of it is unknown: a little
/// past the week servers are asked to remember sent ones.
const UNSETTLED_KEPT: u64 = 8 * 24 * 3_600_000;

/// How long a message the server failed to send stays to be seen, unless let go first.
const FAILED_KEPT: u64 = 30 * 24 * 3_600_000;

fn now_ms() -> u64 {
    u64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis())
        .unwrap_or(u64::MAX)
}

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
        if !self.can_schedule().await? {
            return Err(Error::Unsupported("scheduled messages"));
        }
        let room = self
            .client()
            .get_room(&matrix_sdk::ruma::RoomId::parse(&key.room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: key.room_id.clone() })?;
        let content = crate::outgoing::event(message)?.ok_or(Error::MessageUnavailable)?;
        let event_type = content.event_type().to_string();
        let mut json = serde_json::to_value(&content).map_err(|e| Error::Storage(e.into()))?;
        relate(&mut json, key.thread.as_deref(), reply_to);
        // A reply mentions whom it answers, as one sent now does.
        if let Some(reply_to) = reply_to
            && let Ok(answered) = room.load_or_fetch_event(&matrix_sdk::ruma::EventId::parse(reply_to)?, None).await
            && let Ok(Some(sender)) = answered.raw().get_field::<String>("sender")
        {
            mention(&mut json, &sender, self.id().as_str());
        }
        let raw: Raw<AnyMessageLikeEventContent> =
            Raw::new(&json).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        let (event_type, raw) = match room.latest_encryption_state().await?.is_encrypted() {
            true => {
                let crypto = self.crypto.as_ref().ok_or(Error::MessageUnavailable)?;
                // One at a time: two would share room keys and claim sessions side by side.
                let _one = self.scheduling.lock().await;
                ("m.room.encrypted".to_owned(), encrypt(self, crypto, &room, &event_type, &raw).await?)
            }
            false => (event_type, raw),
        };
        let delay_ms = u64::try_from(delay.min(LONGEST_DELAY).as_millis()).unwrap_or(u64::MAX);
        // MSC4140's own endpoint only. Synapse first took a delay on `/send`, but a server that
        // ignores it posts the message at once, and nothing tells the two apart beforehand.
        let request = api::schedule::Request::new(
            room.room_id().to_owned(),
            event_type.as_str().into(),
            TransactionId::new(),
            delay_ms,
            raw.cast_unchecked(),
        );
        let delay_id = match self.client().send(request).await {
            Ok(response) => response.delay_id,
            Err(error) if unrecognized(&error) => return Err(Error::Unsupported("scheduled messages")),
            Err(error) => return Err(error.into()),
        };
        // Kept, so it can be sent now or cancelled even where the server can't list it.
        let scheduled = ScheduledMessage {
            delay_id: delay_id.clone(),
            room_id: key.room_id.clone(),
            send_at_ms: now_ms().saturating_add(delay_ms),
            body: body_of(&json),
            failed: None,
        };
        self.keep_scheduled(|kept| kept.push(scheduled)).await;
        Ok(delay_id)
    }

    /// The messages this account scheduled, as kept in its store (they outlive a restart),
    /// changed by `change` and written back. One whose time came stays until the server says
    /// what became of it ([`Account::settle_due`]), and one it failed to send until let go;
    /// both for a bounded time.
    async fn keep_scheduled(&self, change: impl FnOnce(&mut Vec<ScheduledMessage>)) -> Vec<ScheduledMessage> {
        let _one = self.scheduled_here.lock().await;
        let store = self.client().state_store();
        let mut kept: Vec<ScheduledMessage> = match store.get_custom_value(KEPT).await {
            Ok(Some(bytes)) => serde_json::from_slice::<Vec<Kept>>(&bytes)
                .unwrap_or_default()
                .into_iter()
                .map(|kept| ScheduledMessage {
                    delay_id: kept.delay_id,
                    room_id: kept.room_id,
                    send_at_ms: kept.send_at_ms,
                    body: kept.body,
                    failed: kept.failed,
                })
                .collect(),
            _ => Vec::new(),
        };
        change(&mut kept);
        let now = now_ms();
        kept.retain(|message| {
            let kept_for = if message.failed.is_some() { FAILED_KEPT } else { UNSETTLED_KEPT };
            message.send_at_ms.saturating_add(kept_for) > now
        });
        let written: Vec<Kept> = kept
            .iter()
            .map(|message| Kept {
                delay_id: message.delay_id.clone(),
                room_id: message.room_id.clone(),
                send_at_ms: message.send_at_ms,
                body: message.body.clone(),
                failed: message.failed.clone(),
            })
            .collect();
        if let Ok(bytes) = serde_json::to_vec(&written) {
            let _ = store.set_custom_value_no_read(KEPT, bytes).await;
        }
        kept
    }

    /// Whether the account's server holds delayed events (`/versions` says so).
    pub async fn can_schedule(&self) -> Result<bool, Error> {
        let features = self.client().unstable_features().await?;
        Ok(features.iter().any(|feature| feature.as_str() == FEATURE))
    }

    /// The messages the server holds for the account, soonest first, then those of this
    /// device's the server failed to send. A message this device can't read (another device's
    /// encrypted one) has no body.
    pub async fn scheduled_messages(&self) -> Result<Vec<ScheduledMessage>, Error> {
        if !self.can_schedule().await? {
            return Ok(Vec::new());
        }
        let listed = match self.client().send(api::list::Request::default()).await {
            Ok(response) => Some(response.delayed_events.iter().filter_map(api::Listed::read).collect::<Vec<_>>()),
            // A server that holds delayed events but can't list them: those this session
            // scheduled stand for them.
            Err(error) if unrecognized(&error) => None,
            Err(error) => return Err(error.into()),
        };
        let kept = self.settle_due(listed.as_deref().unwrap_or_default()).await;
        let mut messages = match listed {
            Some(listed) => self.waiting(listed).await,
            None => kept.iter().filter(|message| message.failed.is_none()).cloned().collect(),
        };
        // One whose time came that the server couldn't be asked about yet stays listed, due, so
        // the apps keep asking until its fate is known, rather than lose sight of it.
        let now = now_ms();
        let unsettled: Vec<_> = kept
            .iter()
            .filter(|message| message.failed.is_none() && message.send_at_ms <= now)
            .filter(|message| messages.iter().all(|other| other.delay_id != message.delay_id))
            .cloned()
            .collect();
        messages.extend(unsettled);
        messages.sort_by_key(|message| message.send_at_ms);
        let failed: Vec<_> = kept
            .into_iter()
            .filter(|message| {
                message.failed.is_some() && messages.iter().all(|other| other.delay_id != message.delay_id)
            })
            .collect();
        messages.extend(failed);
        Ok(messages)
    }

    /// The messages the server holds for the account, as [`Account::scheduled_messages`]; when
    /// the server can't list them now, those this device scheduled, as its store keeps them, so
    /// one just scheduled never drops out of sight.
    pub async fn scheduled_or_known(&self) -> Result<Vec<ScheduledMessage>, (Error, Vec<ScheduledMessage>)> {
        match self.scheduled_messages().await {
            Ok(messages) => Ok(messages),
            Err(error) => {
                let mut known = self.keep_scheduled(|_| {}).await;
                known.sort_by_key(|message| (message.failed.is_some(), message.send_at_ms));
                Err((error, known))
            }
        }
    }

    /// Asks what became of this device's messages whose time came, where the list didn't
    /// say: one the server failed to send stays, marked; one it sent, or no longer knows,
    /// goes. One the server can't be asked about now is asked about next time. Returns what
    /// is then kept.
    async fn settle_due(&self, listed: &[api::Listed]) -> Vec<ScheduledMessage> {
        let now = now_ms();
        let due: Vec<String> = self
            .keep_scheduled(|_| {})
            .await
            .into_iter()
            .filter(|message| message.failed.is_none() && message.send_at_ms <= now)
            .map(|message| message.delay_id)
            .collect();
        let mut fates = Vec::new();
        for delay_id in due {
            let fate = match listed.iter().find(|event| event.delay_id == delay_id) {
                Some(event) => Some(event.clone()),
                // Once: one that fails is asked about at the next listing.
                None => match self
                    .client()
                    .send(api::get::Request::new(delay_id.clone()))
                    .with_request_config(matrix_sdk::config::RequestConfig::new().disable_retry())
                    .await
                {
                    Ok(response) => api::Listed::read(&response.event),
                    Err(error) if unrecognized(&error) => None,
                    Err(_) => continue,
                },
            };
            fates.push((delay_id, fate));
        }
        self.keep_scheduled(|kept| {
            for (delay_id, fate) in fates {
                match fate {
                    // Still to go: the server is sending it.
                    Some(event) if !event.finished => {}
                    Some(api::Listed { error: Some(error), .. }) => {
                        if let Some(message) = kept.iter_mut().find(|message| message.delay_id == delay_id) {
                            message.failed = Some(error);
                        }
                    }
                    _ => kept.retain(|message| message.delay_id != delay_id),
                }
            }
        })
        .await
    }

    /// The messages a server's list holds that are still to go.
    async fn waiting(&self, listed: Vec<api::Listed>) -> Vec<ScheduledMessage> {
        let mut messages = Vec::new();
        for event in listed {
            // Delayed state (a call membership's own expiry) isn't a message, and one already
            // sent, cancelled or failed is done.
            if event.state || event.finished {
                continue;
            }
            let body = match event.event_type.as_str() {
                "m.room.encrypted" => match matrix_sdk::ruma::RoomId::parse(&event.room_id) {
                    Ok(room_id) => self.decrypt_scheduled(&room_id, &event.content).await,
                    Err(_) => None,
                },
                _ => body_of(&event.content),
            };
            messages.push(ScheduledMessage {
                delay_id: event.delay_id,
                room_id: event.room_id,
                send_at_ms: event.send_at_ms,
                body,
                failed: None,
            });
        }
        messages
    }

    /// Cancels a scheduled message, or sends it at once. One the server failed to send is
    /// done there: either only lets it go.
    pub async fn update_scheduled(&self, delay_id: &str, action: ScheduledAction) -> Result<(), Error> {
        let failed = |message: &ScheduledMessage| message.delay_id == delay_id && message.failed.is_some();
        if self.keep_scheduled(|_| {}).await.iter().any(failed) {
            self.keep_scheduled(|kept| kept.retain(|message| message.delay_id != delay_id)).await;
            return Ok(());
        }
        let action = match action {
            ScheduledAction::Cancel => update_delayed_event::UpdateAction::Cancel,
            ScheduledAction::SendNow => update_delayed_event::UpdateAction::Send,
        };
        // The action in the path (MSC4140 as it stands), then in the body (Synapse's first).
        let request = update_delayed_event::unstable_v2::Request::new(delay_id.to_owned(), action.clone());
        match self.client().send(request).await {
            Ok(_) => {}
            Err(error) if unrecognized(&error) => {
                let request = update_delayed_event::unstable_v1::Request::new(delay_id.to_owned(), action);
                self.client().send(request).await?;
            }
            Err(error) => return Err(error.into()),
        }
        self.keep_scheduled(|kept| kept.retain(|message| message.delay_id != delay_id)).await;
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

/// Whether the server doesn't know an endpoint: it answers 404 or 405, or `M_UNRECOGNIZED`.
fn unrecognized(error: &matrix_sdk::HttpError) -> bool {
    use matrix_sdk::ruma::api::error::ErrorKind;
    error.as_client_api_error().is_some_and(|api| {
        matches!(api.status_code.as_u16(), 404 | 405) || matches!(api.error_kind(), Some(ErrorKind::Unrecognized))
    })
}

/// Adds `user_id` to the users a message's content mentions, unless it is `me`.
fn mention(content: &mut Value, user_id: &str, me: &str) {
    if user_id == me {
        return;
    }
    let users = &mut content["m.mentions"]["user_ids"];
    if !users.is_array() {
        *users = Value::Array(Vec::new());
    }
    if let Some(users) = users.as_array_mut()
        && !users.iter().any(|known| known == user_id)
    {
        users.push(Value::String(user_id.to_owned()));
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
pub(crate) fn relate(content: &mut Value, thread: Option<&str>, reply_to: Option<&str>) {
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
    let shared = share_room_key(crypto, room, client).await;
    if shared.is_err() {
        // As the SDK does: a key some members may lack would leave them unable to decrypt.
        let _ = room.discard_room_key().await;
    }
    shared?;
    let olm = crypto.0.olm_machine().await;
    let encrypted = olm
        .as_ref()
        .ok_or(Error::MessageUnavailable)?
        .encrypt_room_event_raw(room.room_id(), event_type, content)
        .await
        .map_err(|e| Error::Sdk(matrix_sdk::Error::MegolmError(Box::new(e))))?;
    Ok(encrypted.content.cast_unchecked())
}

/// Shares the room's current key with its members' devices, marking each request sent.
async fn share_room_key(crypto: &Crypto, room: &Room, client: &matrix_sdk::Client) -> Result<(), Error> {
    let crypto_failed =
        |error: matrix_sdk_base::crypto::OlmError| Error::Sdk(matrix_sdk::Error::OlmError(Box::new(error)));
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
    Ok(())
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

    #[test]
    fn a_mention_should_be_added_once_and_never_of_oneself() {
        let mut content = json!({ "m.mentions": {} });
        mention(&mut content, "@b:x", "@a:x");
        mention(&mut content, "@b:x", "@a:x");
        mention(&mut content, "@a:x", "@a:x");
        assert_eq!(content["m.mentions"]["user_ids"], json!(["@b:x"]));
    }

    /// Which MSC4140 endpoints a test server knows.
    #[derive(Clone, Copy, PartialEq)]
    enum Endpoints {
        /// The proposal's own (`/delayed_event/…`, the action in the path).
        Current,
        /// Only Synapse's first (`?org.matrix.msc4140.delay=` on `/send`, the action in the body).
        Older,
        /// None: `/versions` doesn't name MSC4140, and `/send` would post at once.
        None,
        /// The proposal's own, but no list of them all.
        NoList,
        /// The proposal's own, its list failing for now.
        BrokenList,
    }

    type Sent = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

    /// A server that keeps what is sent, with a room `!room:example.org` (encrypted when
    /// `encrypted`) to sync.
    fn server(encrypted: bool, endpoints: Endpoints) -> (Server, Sent) {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sent.clone();
        let unknown = || json!({ "errcode": "M_NOT_FOUND", "error": "Unrecognized request" });
        let server = Server::new(move |request| {
            captured.lock().expect("requests").push(request.to_owned());
            let first = request.lines().next().unwrap_or_default();
            if first.contains("/delayed_event/") {
                let body: Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap_or_default();
                return match endpoints {
                    // As the proposal and Synapse have it: `delay_ms`, the content apart.
                    Endpoints::Current | Endpoints::NoList | Endpoints::BrokenList
                        if body["delay_ms"].is_u64() && body["content"].is_object() =>
                    {
                        json!({ "delay_id": "d1" })
                    }
                    Endpoints::Current | Endpoints::NoList | Endpoints::BrokenList => {
                        json!({ "errcode": "M_BAD_JSON", "error": "no delay_ms" })
                    }
                    Endpoints::Older | Endpoints::None => unknown(),
                };
            }
            if first.contains("org.matrix.msc4140.delay=") {
                return match endpoints {
                    Endpoints::None => json!({ "event_id": "$posted-at-once" }),
                    Endpoints::Current | Endpoints::Older | Endpoints::NoList | Endpoints::BrokenList => {
                        json!({ "delay_id": "d1" })
                    }
                };
            }
            if first.starts_with("POST") && first.contains("/delayed_events/") {
                let in_path = first.contains("/d1/cancel") || first.contains("/d1/send");
                return match (endpoints, in_path) {
                    (Endpoints::Current | Endpoints::NoList | Endpoints::BrokenList, true)
                    | (Endpoints::Older, false) => {
                        json!({})
                    }
                    _ => unknown(),
                };
            }
            // Not a list: an answer that reads as none, as a failing proxy might give.
            if first.starts_with("GET") && first.contains("/delayed_events ") && endpoints == Endpoints::BrokenList {
                return json!({ "unexpected": true });
            }
            if first.starts_with("GET") && first.contains("/delayed_events") && endpoints == Endpoints::NoList {
                return unknown();
            }
            if first.starts_with("GET") && first.contains("/delayed_events/") {
                // One asked for by ID: `lost` failed, `sent` went, any other is unknown.
                let finalised = if first.contains("/delayed_events/lost ") {
                    json!({ "error": { "errcode": "M_FORBIDDEN", "error": "You may not post here" }, "finalised_ts": 2 })
                } else if first.contains("/delayed_events/flaky ") {
                    return json!({ "errcode": "M_UNKNOWN", "error": "Try again later", crate::test_server::SERVER_ERROR_STATUS: true });
                } else if first.contains("/delayed_events/sent ") {
                    json!({ "event_id": "$sent", "finalised_ts": 2 })
                } else {
                    return unknown();
                };
                return json!({
                    "delay_id": "x", "room_id": "!room:example.org", "type": "m.room.message",
                    "delay_ms": 1, "delayed_since_ts": 1, "content": {}, "finalised": finalised
                });
            }
            if first.starts_with("GET") && first.contains("/delayed_events") {
                let delayed = |id: &str, done: bool| {
                    let mut event = json!({
                        "delay_id": id, "room_id": "!room:example.org", "type": "m.room.message",
                        "delay_ms": 60000, "delayed_since_ts": 1000, "content": { "msgtype": "m.text", "body": id }
                    });
                    if done {
                        event["finalised"] = json!({ "event_id": "$sent", "finalised_ts": 2000 });
                    }
                    event
                };
                return json!({ "delayed_events": [delayed("waiting", false), delayed("sent", true)] });
            }
            if first.contains("/versions") {
                let held = endpoints != Endpoints::None;
                let features = json!({ "org.matrix.msc4140": held });
                return json!({ "versions": ["v1.12"], "unstable_features": features });
            }
            if !encrypted && first.contains("/state/m.room.encryption") {
                return json!({ "errcode": "M_NOT_FOUND", "error": "no encryption" });
            }
            if first.contains("/joined_members") || first.contains("/members") {
                return json!({ "chunk": [
                    {"type":"m.room.member","state_key":"@a:example.org","sender":"@a:example.org","event_id":"$join","origin_server_ts":0,"content":{"membership":"join"}}
                ] });
            }
            if first.contains("/keys/claim") {
                return json!({ "one_time_keys": {} });
            }
            let mut response = crate::test_server::thread_response(request);
            if encrypted && first.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().expect("state").push(json!({
                    "type":"m.room.encryption","state_key":"","sender":"@a:example.org","event_id":"$e","origin_server_ts":0,
                    "content":{"algorithm":"m.megolm.v1.aes-sha2"}
                }));
            }
            response
        });
        (server, sent)
    }

    /// A signed-in account on its own store, synced with `server`.
    async fn account(server: &Server, name: &str) -> Account {
        let dir = std::env::temp_dir().join(format!("matrix-core-scheduled-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let account = Account::for_test_with_store(&server.url(), &dir).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO))
            .await
            .expect("sync");
        account
    }

    /// The request that scheduled a message: its request line and body.
    fn scheduled(sent: &Sent) -> (String, Value) {
        let sent = sent.lock().expect("requests");
        let request = sent
            .iter()
            .rev()
            .find(|request| {
                let first = request.lines().next().unwrap_or_default();
                first.contains("/delayed_event/") || first.contains("org.matrix.msc4140.delay=")
            })
            .expect("a scheduled send");
        let line = request.lines().next().unwrap_or_default().to_owned();
        let body = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).expect("a JSON body");
        (line, body)
    }

    fn text(body: &str) -> Outgoing {
        Outgoing::Text { body: body.into(), html: None, mentions: Default::default() }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_should_be_scheduled_with_its_delay() {
        let (server, sent) = server(false, Endpoints::Current);
        let account = account(&server, "current").await;
        let id = account
            .schedule(&TimelineKey::room("!room:example.org"), None, &text("later"), Duration::from_secs(90))
            .await;
        assert_eq!(id.expect("scheduled"), "d1");
        let (line, body) = scheduled(&sent);
        assert!(line.contains("/org.matrix.msc4140/rooms/!room:example.org/delayed_event/m.room.message/"), "{line}");
        assert_eq!((body["delay_ms"].as_u64(), body["content"]["body"].as_str()), (Some(90_000), Some("later")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_server_without_delayed_events_should_be_sent_nothing() {
        let (server, sent) = server(false, Endpoints::None);
        let account = account(&server, "unsupported").await;
        let id = account
            .schedule(&TimelineKey::room("!room:example.org"), None, &text("later"), Duration::from_secs(90))
            .await;
        assert!(matches!(id, Err(Error::Unsupported(_))), "{id:?}");
        let sent = sent.lock().expect("requests");
        assert!(sent.iter().all(|request| !request.starts_with("PUT")), "nothing may be posted");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_scheduled_on_a_server_that_cant_list_should_still_be_listed_and_cancelled() {
        let (server, _) = server(false, Endpoints::NoList);
        let account = account(&server, "no-list").await;
        let key = TimelineKey::room("!room:example.org");
        account.schedule(&key, None, &text("later"), Duration::from_secs(600)).await.expect("scheduled");
        let listed = account.scheduled_messages().await.expect("listed");
        assert_eq!(
            listed.iter().map(|message| (message.delay_id.as_str(), message.body.as_deref())).collect::<Vec<_>>(),
            [("d1", Some("later"))]
        );
        account.update_scheduled("d1", ScheduledAction::Cancel).await.expect("cancelled");
        assert!(account.scheduled_messages().await.expect("listed").is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_server_with_only_the_first_endpoint_should_be_refused_and_sent_nothing() {
        let (server, sent) = server(false, Endpoints::Older);
        let account = account(&server, "older").await;
        let key = TimelineKey::room("!room:example.org");
        let id = account.schedule(&key, None, &text("later"), Duration::from_secs(90)).await;
        assert!(matches!(id, Err(Error::Unsupported(_))), "{id:?}");
        let sent = sent.lock().expect("requests");
        assert!(sent.iter().all(|request| !request.starts_with("PUT /_matrix/client/v3/")), "nothing may be posted");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_scheduled_before_a_restart_should_still_be_listed_where_the_server_cant() {
        let (server, _) = server(false, Endpoints::NoList);
        let dir = std::env::temp_dir().join(format!("matrix-core-scheduled-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let account = Account::for_test_with_store(&server.url(), &dir).await;
            let sync = matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO);
            account.client().sync_once(sync).await.expect("sync");
            let key = TimelineKey::room("!room:example.org");
            account.schedule(&key, None, &text("later"), Duration::from_secs(600)).await.expect("scheduled");
        }
        let account = Account::for_test_with_store(&server.url(), &dir).await;
        let listed = account.scheduled_messages().await.expect("listed");
        assert_eq!(listed.iter().map(|message| message.delay_id.as_str()).collect::<Vec<_>>(), ["d1"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_scheduled_reply_should_mention_whom_it_answers() {
        let (server, sent) = server(false, Endpoints::Current);
        let account = account(&server, "reply").await;
        let key = TimelineKey::room("!room:example.org");
        account.schedule(&key, Some("$root"), &text("later"), Duration::from_secs(60)).await.expect("scheduled");
        let (_, body) = scheduled(&sent);
        assert_eq!(body["content"]["m.relates_to"]["m.in_reply_to"]["event_id"], "$root");
        assert_eq!(body["content"]["m.mentions"]["user_ids"], json!(["@b:example.org"]));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_scheduled_in_an_encrypted_room_should_be_encrypted() {
        let (server, sent) = server(true, Endpoints::Current);
        let account = account(&server, "encrypted").await;
        let id = account
            .schedule(&TimelineKey::room("!room:example.org"), None, &text("a secret"), Duration::from_secs(60))
            .await;
        assert_eq!(id.expect("scheduled"), "d1");
        let (line, body) = scheduled(&sent);
        assert!(line.contains("/delayed_event/m.room.encrypted/"), "{line}");
        assert_eq!(body["content"]["algorithm"], "m.megolm.v1.aes-sha2");
        assert!(body["content"]["ciphertext"].is_string() && !body.to_string().contains("a secret"), "{body}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_encrypted_scheduled_message_should_be_read_back_by_its_device() {
        let (server, sent) = server(true, Endpoints::Current);
        let account = account(&server, "read-back").await;
        let key = TimelineKey::room("!room:example.org");
        account.schedule(&key, None, &text("a secret"), Duration::from_secs(60)).await.expect("scheduled");
        let (_, body) = scheduled(&sent);
        let content =
            account.decrypt_scheduled(matrix_sdk::ruma::room_id!("!room:example.org"), &body["content"]).await;
        assert_eq!(content.as_deref(), Some("a secret"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_already_sent_should_not_be_listed() {
        let (server, _) = server(false, Endpoints::Current);
        let account = account(&server, "listed").await;
        let listed = account.scheduled_messages().await.expect("listed");
        let bodies: Vec<_> = listed.iter().map(|message| message.body.as_deref()).collect();
        assert_eq!(bodies, [Some("waiting")]);
        assert_eq!(listed[0].send_at_ms, 61_000);
    }

    /// Keeps a message this device scheduled, whose time came long ago.
    async fn kept_due(account: &Account, delay_id: &str) {
        let message = ScheduledMessage {
            delay_id: delay_id.into(),
            room_id: "!room:example.org".into(),
            send_at_ms: now_ms() - 60_000,
            body: Some("later".into()),
            failed: None,
        };
        account.keep_scheduled(|kept| kept.push(message)).await;
    }

    fn kept_ids(kept: &[ScheduledMessage]) -> Vec<&str> {
        kept.iter().map(|message| message.delay_id.as_str()).collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_the_server_failed_to_send_should_stay_listed_with_why() {
        let (server, _) = server(false, Endpoints::Current);
        let account = account(&server, "failed").await;
        kept_due(&account, "lost").await;
        let listed = account.scheduled_messages().await.expect("listed");
        let failed: Vec<_> =
            listed.iter().map(|message| (message.delay_id.as_str(), message.failed.as_deref())).collect();
        assert_eq!(failed, [("waiting", None), ("lost", Some("You may not post here"))]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failure_should_be_kept_for_later_listings() {
        let (server, _) = server(false, Endpoints::Current);
        let account = account(&server, "failed-twice").await;
        kept_due(&account, "lost").await;
        account.scheduled_messages().await.expect("listed");
        let kept = account.keep_scheduled(|_| {}).await;
        assert_eq!(kept.first().and_then(|message| message.failed.as_deref()), Some("You may not post here"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_whose_fate_cant_be_asked_now_should_stay_listed_as_due() {
        let (server, _) = server(false, Endpoints::Current);
        let account = account(&server, "unsettled").await;
        kept_due(&account, "flaky").await;
        let listed = account.scheduled_messages().await.expect("listed");
        let flaky = listed.iter().find(|message| message.delay_id == "flaky").expect("still listed");
        assert!(flaky.failed.is_none() && flaky.send_at_ms <= now_ms());
        assert_eq!(kept_ids(&account.keep_scheduled(|_| {}).await), ["flaky"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_just_scheduled_should_stay_known_when_the_list_fails() {
        let (server, _) = server(false, Endpoints::BrokenList);
        let account = account(&server, "broken-list").await;
        account
            .schedule(&TimelineKey::room("!room:example.org"), None, &text("later"), Duration::from_secs(90))
            .await
            .expect("scheduled");
        let Err((_, known)) = account.scheduled_or_known().await else { panic!("the list should fail") };
        assert_eq!(kept_ids(&known), ["d1"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_the_server_sent_should_be_let_go() {
        let (server, _) = server(false, Endpoints::Current);
        let account = account(&server, "went").await;
        kept_due(&account, "sent").await;
        let listed = account.scheduled_messages().await.expect("listed");
        assert_eq!(kept_ids(&listed), ["waiting"]);
        assert!(account.keep_scheduled(|_| {}).await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_the_server_no_longer_knows_should_be_let_go() {
        let (server, _) = server(false, Endpoints::Current);
        let account = account(&server, "forgotten").await;
        kept_due(&account, "forgotten").await;
        account.scheduled_messages().await.expect("listed");
        assert!(account.keep_scheduled(|_| {}).await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_still_to_come_should_not_be_asked_after() {
        let (server, sent) = server(false, Endpoints::Current);
        let account = account(&server, "to-come").await;
        account
            .schedule(&TimelineKey::room("!room:example.org"), None, &text("later"), Duration::from_secs(90))
            .await
            .expect("scheduled");
        account.scheduled_messages().await.expect("listed");
        assert!(
            !sent
                .lock()
                .expect("requests")
                .iter()
                .any(|request| request.starts_with("GET") && request.contains("/delayed_events/d1"))
        );
        assert_eq!(kept_ids(&account.keep_scheduled(|_| {}).await), ["d1"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn letting_a_failed_message_go_should_ask_nothing_of_the_server() {
        let (server, sent) = server(false, Endpoints::Current);
        let account = account(&server, "dismiss").await;
        kept_due(&account, "lost").await;
        account.scheduled_messages().await.expect("listed");
        account.update_scheduled("lost", ScheduledAction::Cancel).await.expect("let go");
        assert!(
            !sent
                .lock()
                .expect("requests")
                .iter()
                .any(|request| request.starts_with("POST") && request.contains("/delayed_events/lost"))
        );
        assert!(account.keep_scheduled(|_| {}).await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelling_should_name_the_action_in_the_path() {
        let (server, sent) = server(false, Endpoints::Current);
        let account = account(&server, "cancel").await;
        account.update_scheduled("d1", ScheduledAction::Cancel).await.expect("cancelled");
        assert!(
            sent.lock()
                .expect("requests")
                .iter()
                .any(|request| request.starts_with("POST") && request.contains("/delayed_events/d1/cancel"))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sending_now_on_a_server_with_only_the_first_endpoint_should_name_the_action_in_the_body() {
        let (server, sent) = server(false, Endpoints::Older);
        let account = account(&server, "send-now").await;
        account.update_scheduled("d1", ScheduledAction::SendNow).await.expect("sent");
        let sent = sent.lock().expect("requests");
        let request = sent.iter().rev().find(|request| request.contains("/delayed_events/d1 ")).expect("an update");
        assert!(request.contains("\"action\":\"send\""), "{request}");
    }
}
