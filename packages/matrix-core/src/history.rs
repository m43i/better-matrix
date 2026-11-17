//! A room's history, gathered for its info panel: its threads (the server's thread list) and the
//! files shared in it (messages with media, newest first). Both read decrypted events as raw
//! JSON, so one event in an unexpected shape costs only itself.

use matrix_sdk::Room;
use matrix_sdk::room::{ListThreadsOptions, MessagesOptions};
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{RoomId, UInt, UserId};
use serde_json::Value;

use crate::Error;
use crate::account::Account;
use crate::model::{Attachment, MessageContent};

/// How many threads or files the panel lists.
const LIMIT: usize = 50;
/// How many pages of an encrypted room's history are searched for files: the server can't
/// tell which encrypted events carry one.
const ENCRYPTED_PAGES: usize = 6;

/// A thread of a room, as the info panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct ThreadInfo {
    /// Its root event, which opens it.
    pub root: String,
    /// Who started it.
    pub sender_name: String,
    /// The root message's text.
    pub body: String,
    /// How many replies it has.
    pub replies: u64,
    /// When the last reply came, in milliseconds since the epoch.
    pub latest_at: Option<u64>,
}

/// A file shared in a room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct SharedFile {
    /// The message that carries it.
    pub event_id: String,
    /// Who shared it.
    pub sender_name: String,
    /// When, in milliseconds since the epoch.
    pub timestamp: u64,
    /// The file.
    pub attachment: Attachment,
}

/// A pinned message preview. Missing or unreadable events remain visible as placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PinnedMessage {
    /// The pinned event's ID.
    pub event_id: String,
    /// Its sender's display name, when available.
    pub sender_name: String,
    /// The text, filename or poll question; absent if it cannot be read.
    pub body: Option<String>,
    /// The thread containing it, if any.
    pub thread: Option<String>,
}

/// Latest text or attachment content, without the source's relations. An edit's replacement
/// is nested under `m.new_content`; forwarding must never send an edit of the source event.
pub(crate) fn forward_content(event: &Value) -> Option<RoomMessageEventContent> {
    if event["type"] != "m.room.message" {
        return None;
    }
    let content = &event["content"];
    let mut content = content.get("m.new_content").unwrap_or(content).clone();
    let object = content.as_object_mut()?;
    let reply = object.get("m.relates_to").is_some_and(|relation| relation["m.in_reply_to"].is_object());
    object.remove("m.mentions");
    object.remove("m.relates_to");
    object.remove("m.new_content");
    let mut content: RoomMessageEventContent = serde_json::from_value(content).ok()?;
    if reply {
        use matrix_sdk::ruma::html::{HtmlSanitizerMode, RemoveReplyFallback};
        content.msgtype.sanitize(HtmlSanitizerMode::Compat, RemoveReplyFallback::Yes);
    }
    Some(content)
}

impl Account {
    fn room(&self, room_id: &str) -> Result<Room, Error> {
        self.client()
            .get_room(&RoomId::parse(room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.to_owned() })
    }

    /// Send forwarded content through the destination room, retaining its encryption policy.
    pub async fn forward_message(&self, room_id: &str, content: RoomMessageEventContent) -> Result<(), Error> {
        self.room(room_id)?.send(content).await?;
        Ok(())
    }

    /// Change the room's shared pins. The server enforces the user's power level.
    pub async fn set_pinned(&self, room_id: &str, event_id: &str, pinned: bool) -> Result<(), Error> {
        let room = self.room(room_id)?;
        let event_id = matrix_sdk::ruma::EventId::parse(event_id)?;
        if pinned {
            room.pin_event(&event_id).await?;
        } else {
            room.unpin_event(&event_id).await?;
        }
        Ok(())
    }

    /// Fetch previews even when the timeline has never loaded the pinned events.
    pub async fn pinned_messages(&self, room_id: &str) -> Result<Vec<PinnedMessage>, Error> {
        let room = self.room(room_id)?;
        use matrix_sdk_ui::timeline::{RoomExt, TimelineFocus};
        let ids = room.pinned_event_ids().unwrap_or_default();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // Use the SDK's pinned timeline so edits, redactions, polls and decryption follow the
        // same aggregation as the conversation, even for events outside loaded history.
        let timeline = room.timeline_builder().with_focus(TimelineFocus::PinnedEvents).build().await?;
        // Building a pinned timeline subscribes to the cache; its first network reload is
        // asynchronous. Subscribe before checking items so the arrival cannot be missed.
        let (_, mut updates) = timeline.subscribe().await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            use futures_util::StreamExt;
            loop {
                let mut loaded = true;
                for id in &ids {
                    loaded &= timeline.item_by_event_id(id).await.is_some();
                }
                if loaded || updates.next().await.is_none() {
                    break;
                }
            }
        })
        .await;

        let mut messages = Vec::new();
        for id in ids {
            let mut preview =
                PinnedMessage { event_id: id.to_string(), sender_name: String::new(), body: None, thread: None };
            if let Some(item) = timeline.item_by_event_id(&id).await {
                if let Some(message) = crate::timeline::message(&item, room.own_user_id()) {
                    preview.sender_name = message.sender_name;
                    if !matches!(message.content, MessageContent::Redacted | MessageContent::Undecryptable) {
                        preview.body = Some(message.content.plain().to_owned());
                    }
                }
                if let Some(raw) = item.original_json()
                    && let Ok(event) = raw.deserialize_as_unchecked::<Value>()
                {
                    let relation = &event["content"]["m.relates_to"];
                    if relation["rel_type"] == "m.thread" {
                        preview.thread = relation["event_id"].as_str().map(str::to_owned);
                    }
                }
            }
            messages.push(preview);
        }
        Ok(messages)
    }

    /// The room's threads, most recently active first.
    pub async fn threads(&self, room_id: &str) -> Result<Vec<ThreadInfo>, Error> {
        let room = self.room(room_id)?;
        let options = ListThreadsOptions { limit: UInt::new(LIMIT as u64), ..ListThreadsOptions::default() };
        let roots = room.list_threads(options).await?;
        let mut threads = Vec::new();
        for event in roots.chunk {
            let Ok(event) = event.raw().deserialize_as_unchecked::<Value>() else { continue };
            let thread = &event["unsigned"]["m.relations"]["m.thread"];
            threads.push(ThreadInfo {
                root: event["event_id"].as_str().unwrap_or_default().to_owned(),
                sender_name: sender_name(&room, &event).await,
                body: event["content"]["body"].as_str().unwrap_or_default().to_owned(),
                replies: thread["count"].as_u64().unwrap_or_default(),
                latest_at: thread["latest_event"]["origin_server_ts"].as_u64(),
            });
        }
        Ok(threads)
    }

    /// The files shared in the room, newest first.
    pub async fn files(&self, room_id: &str) -> Result<Vec<SharedFile>, Error> {
        let room = self.room(room_id)?;
        let encrypted = room.encryption_state().is_encrypted();
        let mut files = Vec::new();
        let mut from = None;
        let pages = if encrypted { ENCRYPTED_PAGES } else { 1 };
        for _ in 0..pages {
            let mut options = MessagesOptions::backward();
            options.from = from.take();
            options.limit = UInt::new(if encrypted { 100 } else { LIMIT as u64 }).unwrap_or_default();
            // In the clear the server finds them itself.
            if !encrypted {
                options.filter.url_filter = Some(matrix_sdk::ruma::api::client::filter::UrlFilter::EventsWithUrl);
            }
            let page = room.messages(options).await?;
            for event in page.chunk {
                let Ok(event) = event.raw().deserialize_as_unchecked::<Value>() else { continue };
                if let Some(attachment) = attachment(&event) {
                    files.push(SharedFile {
                        event_id: event["event_id"].as_str().unwrap_or_default().to_owned(),
                        sender_name: sender_name(&room, &event).await,
                        timestamp: event["origin_server_ts"].as_u64().unwrap_or_default(),
                        attachment,
                    });
                }
            }
            match page.end {
                Some(end) if files.len() < LIMIT => from = Some(end),
                _ => break,
            }
        }
        files.truncate(LIMIT);
        Ok(files)
    }
}

/// The file a message carries, if it is a file, image, video or audio message.
fn attachment(event: &Value) -> Option<Attachment> {
    if event["type"] != "m.room.message" {
        return None;
    }
    let content: RoomMessageEventContent = serde_json::from_value(event["content"].clone()).ok()?;
    match crate::convert::content(&content.msgtype) {
        MessageContent::Attachment(attachment) => Some(attachment),
        _ => None,
    }
}

async fn sender_name(room: &Room, event: &Value) -> String {
    let sender = event["sender"].as_str().unwrap_or_default();
    match UserId::parse(sender) {
        Ok(user) => room
            .get_member_no_sync(&user)
            .await
            .ok()
            .flatten()
            .map_or_else(|| sender.to_owned(), |m| m.name().to_owned()),
        Err(_) => sender.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_message_should_read_as_its_attachment() {
        let event = serde_json::json!({
            "type": "m.room.message", "event_id": "$f", "sender": "@a:x", "origin_server_ts": 1,
            "content": { "msgtype": "m.file", "body": "notes.pdf", "url": "mxc://x/abc", "info": { "size": 10 } }
        });
        assert_eq!(attachment(&event).map(|a| a.name), Some("notes.pdf".to_owned()));
    }

    #[test]
    fn a_text_message_should_carry_no_file() {
        let event = serde_json::json!({ "type": "m.room.message", "content": { "msgtype": "m.text", "body": "hi" } });
        assert!(attachment(&event).is_none());
    }
}

#[cfg(test)]
mod message_action_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn forwarding_should_not_carry_mentions_into_the_destination() {
        let event = json!({"type":"m.room.message","content":{"msgtype":"m.text","body":"Hi Alice","m.mentions":{"user_ids":["@alice:example.org"],"room":true}}});
        let content = serde_json::to_value(forward_content(&event).expect("content")).expect("JSON");
        assert!(content.get("m.mentions").is_none());
    }

    #[test]
    fn forwarding_a_legacy_reply_should_remove_its_quote_fallbacks() {
        let event = json!({"type":"m.room.message","content":{"msgtype":"m.text","body":"> <@a:x> quoted\n> text\n\nA reply","format":"org.matrix.custom.html","formatted_body":"<mx-reply><blockquote>quoted text</blockquote></mx-reply><b>A reply</b>","m.relates_to":{"m.in_reply_to":{"event_id":"$old"}}}});
        let content = serde_json::to_value(forward_content(&event).expect("content")).expect("JSON");
        assert_eq!(content["body"], "A reply");
        assert_eq!(content["formatted_body"], "<b>A reply</b>");
    }

    #[test]
    fn forwarding_an_edited_reply_should_send_the_replacement_without_relations() {
        let event = json!({"type":"m.room.message","content":{"msgtype":"m.text","body":"* old","m.relates_to":{"rel_type":"m.replace","event_id":"$old"},"m.new_content":{"msgtype":"m.text","body":"new","format":"org.matrix.custom.html","formatted_body":"<b>new</b>","m.relates_to":{"m.in_reply_to":{"event_id":"$reply"}}}}});
        let content = serde_json::to_value(forward_content(&event).expect("forwardable")).expect("JSON");
        assert_eq!(
            content,
            json!({"msgtype":"m.text","body":"new","format":"org.matrix.custom.html","formatted_body":"<b>new</b>"})
        );
    }

    #[test]
    fn forwarding_an_encrypted_attachment_should_keep_its_file_keys() {
        let event = json!({"type":"m.room.message","content":{"msgtype":"m.file","body":"notes.pdf","file":{"url":"mxc://x/file","key":{"kty":"oct","key_ops":["decrypt","encrypt"],"alg":"A256CTR","k":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","ext":true},"iv":"AAAAAAAAAAAAAAAAAAAAAA","hashes":{"sha256":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"},"v":"v2"},"m.relates_to":{"rel_type":"m.thread","event_id":"$root"}}});
        let content = serde_json::to_value(forward_content(&event).expect("attachment")).expect("JSON");
        assert_eq!(content["file"], event["content"]["file"]);
        assert!(content.get("m.relates_to").is_none());
    }

    #[test]
    fn forwarding_a_redacted_message_should_fail() {
        assert!(forward_content(&json!({"type":"m.room.message","content":{}})).is_none());
    }

    #[tokio::test]
    async fn pinning_should_preserve_the_rooms_existing_pins() {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sent.clone();
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/state/m.room.pinned_events") && request.starts_with("PUT") {
                captured
                    .lock()
                    .expect("requests")
                    .push(request.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned());
                return json!({"event_id":"$pin-state"});
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().expect("state").push(json!({"type":"m.room.pinned_events","state_key":"","event_id":"$pins","sender":"@a:example.org","origin_server_ts":1,"content":{"pinned":["$old"]}}));
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        account.set_pinned("!room:example.org", "$new", true).await.expect("pin");
        let requests = sent.lock().expect("requests");
        let body: Value = serde_json::from_str(&requests[0]).expect("pin content");
        assert_eq!(body["pinned"], json!(["$old", "$new"]));
    }
}

#[cfg(test)]
mod pinned_loading_tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn a_server_only_pin_should_load_before_its_preview_is_returned() {
        let server = crate::test_server::Server::new(|request| {
            if request.contains("/event/") || request.contains("/context/") {
                std::thread::sleep(std::time::Duration::from_millis(80));
                return json!({"type":"m.room.message","room_id":"!room:example.org","event_id":"$server-only","sender":"@a:example.org","origin_server_ts":2,"content":{"msgtype":"m.text","body":"A pin outside loaded history"}});
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["timeline"]["events"] = json!([]);
                response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().expect("state").push(json!({"type":"m.room.pinned_events","state_key":"","event_id":"$pins","sender":"@a:example.org","origin_server_ts":1,"content":{"pinned":["$server-only"]}}));
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let previews = account.pinned_messages("!room:example.org").await.expect("load pins");
        assert_eq!(previews[0].body.as_deref(), Some("A pin outside loaded history"));
    }
}
