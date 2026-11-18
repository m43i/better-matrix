//! Saved messages: the user's private bookmarks. Each room keeps its own in the room's account
//! data, as MSC2437 tagged events (`m.favourite`) under the proposal's unstable name, so they
//! follow the user to every session, no one else sees them, and leaving a room takes its saved
//! messages with it.

use matrix_sdk::Room;
use matrix_sdk::ruma::api::client::config::{get_room_account_data, set_room_account_data};
use matrix_sdk::ruma::api::error::ErrorKind;
use matrix_sdk::ruma::events::RoomAccountDataEventType;
use matrix_sdk::ruma::{EventId, UserId};
use serde_json::{Value, json};

use crate::Error;
use crate::account::Account;
use crate::history::MessagePreview;

/// The room account data holding a room's tagged events: MSC2437's `m.tagged_events`, under
/// its unstable name while the proposal is open.
pub(crate) const EVENT_TYPE: &str = "org.matrix.msc2437.tagged_events";
/// The tag of a saved message (the proposal's favourite).
const SAVED: &str = "m.favourite";

/// A message the user saved, as their saved messages list it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct SavedMessage {
    /// The room it was sent in.
    pub room_id: String,
    /// When the user saved it, in milliseconds since the epoch, if the saving client said.
    pub saved_at_ms: Option<u64>,
    /// The message; a placeholder when it can't be read (deleted, not decryptable, gone).
    pub preview: MessagePreview,
}

/// The saved messages in one room's tagged events, newest saved first.
pub(crate) fn saved_in(content: &Value) -> Vec<(String, Option<u64>)> {
    let Some(tagged) = content["tags"][SAVED].as_object() else { return Vec::new() };
    let mut saved: Vec<_> =
        tagged.iter().map(|(event_id, info)| (event_id.clone(), info["tagged_at"].as_u64())).collect();
    saved.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    saved
}

/// The tagged events with `event_id` saved or not, keeping every other tag and field.
fn with_saved(mut content: Value, event_id: &str, saved: bool, now_ms: u64) -> Value {
    if !content.is_object() {
        content = json!({});
    }
    let Some(root) = content.as_object_mut() else { return content };
    let tags = root.entry("tags").or_insert_with(|| json!({}));
    if !tags.is_object() {
        *tags = json!({});
    }
    let Some(tags) = tags.as_object_mut() else { return content };
    let tagged = tags.entry(SAVED).or_insert_with(|| json!({}));
    if !tagged.is_object() {
        *tagged = json!({});
    }
    if let Some(tagged) = tagged.as_object_mut() {
        if saved {
            // Saving again keeps when it was first saved.
            tagged.entry(event_id).or_insert_with(|| json!({ "tagged_at": now_ms }));
        } else {
            tagged.remove(event_id);
        }
        if tagged.is_empty() {
            tags.remove(SAVED);
        }
    }
    content
}

/// What a room's tagged events hold, from this device's copy (no request).
async fn stored(room: &Room) -> Value {
    let raw = room.account_data(RoomAccountDataEventType::from(EVENT_TYPE)).await.ok().flatten();
    raw.and_then(|raw| raw.get_field::<Value>("content").ok().flatten()).unwrap_or(Value::Null)
}

/// A message read from its raw event, decrypted when it could be. The text is as first sent:
/// a later edit shows once the message is opened.
async fn preview(room: &Room, event_id: &str, event: Option<Value>) -> MessagePreview {
    let mut preview = MessagePreview {
        event_id: event_id.to_owned(),
        sender_name: String::new(),
        sender_id: String::new(),
        sender_avatar: None,
        timestamp_ms: None,
        body: None,
        thread: None,
    };
    let Some(event) = event else { return preview };
    preview.sender_id = event["sender"].as_str().unwrap_or_default().to_owned();
    preview.timestamp_ms = event["origin_server_ts"].as_u64();
    let member = match UserId::parse(&preview.sender_id) {
        Ok(user) => room.get_member_no_sync(&user).await.ok().flatten(),
        Err(_) => None,
    };
    preview.sender_name = member.as_ref().map_or_else(|| preview.sender_id.clone(), |m| m.name().to_owned());
    preview.sender_avatar = member.and_then(|m| m.avatar_url().map(ToOwned::to_owned)).and_then(crate::convert::avatar);
    let kind = event["type"].as_str().unwrap_or_default();
    preview.body = crate::search::message_body(kind, &event["content"]).map(str::to_owned);
    let relation = &event["content"]["m.relates_to"];
    if relation["rel_type"] == "m.thread" {
        preview.thread = relation["event_id"].as_str().map(str::to_owned);
    }
    preview
}

/// The messages saved in a room, newest saved first, from this device's copy of its account
/// data.
pub(crate) async fn saved_ids(room: &Room) -> Vec<String> {
    saved_in(&stored(room).await).into_iter().map(|(event_id, _)| event_id).collect()
}

impl Account {
    /// Saves a message, or stops saving it. The tagged events are read from the server and
    /// written back, one change at a time, so quick changes in a row (or another session's)
    /// aren't lost before sync brings them. Updated room summaries follow through sync.
    pub async fn set_saved(&self, room_id: &str, event_id: &str, saved: bool) -> Result<(), Error> {
        let _one_at_a_time = self.saved_changes.lock().await;
        let room = self.room(room_id)?;
        EventId::parse(event_id)?;
        let me = self.client().user_id().ok_or(Error::NoUserId)?.to_owned();
        let kind = RoomAccountDataEventType::from(EVENT_TYPE);
        let request = get_room_account_data::v3::Request::new(me.clone(), room.room_id().to_owned(), kind.clone());
        let current = match self.client().send(request).await {
            Ok(response) => response.account_data.deserialize_as_unchecked::<Value>().unwrap_or(Value::Null),
            Err(error) if error.client_api_error_kind() == Some(&ErrorKind::NotFound) => Value::Null,
            Err(error) => return Err(error.into()),
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX));
        let content = with_saved(current, event_id, saved, now_ms);
        let raw = matrix_sdk::ruma::serde::Raw::new(&content).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        let request = set_room_account_data::v3::Request::new_raw(me, room.room_id().to_owned(), kind, raw);
        self.client().send(request).await?;
        Ok(())
    }

    /// Every message the user saved, in every joined room, most recently saved first. Each is
    /// read from the event cache or fetched; one that can't be read keeps a placeholder.
    pub async fn saved_messages(&self) -> Result<Vec<SavedMessage>, Error> {
        let mut wanted = Vec::new();
        for room in self.client().joined_rooms() {
            for (event_id, saved_at_ms) in saved_in(&stored(&room).await) {
                wanted.push((room.clone(), event_id, saved_at_ms));
            }
        }
        let loads = wanted.into_iter().map(|(room, event_id, saved_at_ms)| async move {
            let event = match EventId::parse(&event_id) {
                Ok(id) => room
                    .load_or_fetch_event(&id, None)
                    .await
                    .ok()
                    .and_then(|event| event.raw().deserialize_as_unchecked::<Value>().ok()),
                Err(_) => None,
            };
            SavedMessage {
                room_id: room.room_id().to_string(),
                saved_at_ms,
                preview: preview(&room, &event_id, event).await,
            }
        });
        let mut saved = futures_util::future::join_all(loads).await;
        saved.sort_by(|a, b| {
            b.saved_at_ms.cmp(&a.saved_at_ms).then_with(|| b.preview.timestamp_ms.cmp(&a.preview.timestamp_ms))
        });
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_a_message_should_tag_it_as_a_favourite_with_when() {
        let content = with_saved(Value::Null, "$a", true, 42);
        assert_eq!(content, json!({"tags":{"m.favourite":{"$a":{"tagged_at":42}}}}));
    }

    #[test]
    fn saving_a_message_should_keep_other_tags_and_fields() {
        let before = json!({"tags":{"m.hidden":{"$h":{}},"m.favourite":{"$a":{"keywords":["cat"]}}},"other":1});
        let content = with_saved(before, "$b", true, 7);
        assert_eq!(content["other"], 1);
        assert_eq!(content["tags"]["m.hidden"], json!({"$h":{}}));
        assert_eq!(content["tags"]["m.favourite"]["$a"], json!({"keywords":["cat"]}));
        assert_eq!(content["tags"]["m.favourite"]["$b"], json!({"tagged_at":7}));
    }

    #[test]
    fn saving_a_saved_message_again_should_keep_when_it_was_first_saved() {
        let content = with_saved(with_saved(Value::Null, "$a", true, 1), "$a", true, 2);
        assert_eq!(content["tags"]["m.favourite"]["$a"]["tagged_at"], 1);
    }

    #[test]
    fn removing_the_last_saved_message_should_drop_the_tag() {
        let content = with_saved(json!({"tags":{"m.favourite":{"$a":{}},"u.work":{"$w":{}}}}), "$a", false, 3);
        assert_eq!(content, json!({"tags":{"u.work":{"$w":{}}}}));
    }

    #[test]
    fn saved_messages_should_list_the_newest_saved_first() {
        let content = json!({"tags":{"m.favourite":{"$old":{"tagged_at":1},"$new":{"tagged_at":9},"$unknown":{}}}});
        let saved: Vec<_> = saved_in(&content).into_iter().map(|(id, _)| id).collect();
        assert_eq!(saved, ["$new", "$old", "$unknown"]);
    }

    #[test]
    fn content_of_another_shape_should_hold_no_saved_messages() {
        assert!(saved_in(&json!({"tags":["$a"]})).is_empty());
        assert!(saved_in(&Value::Null).is_empty());
    }

    #[tokio::test]
    async fn saving_should_change_the_servers_copy_rather_than_this_devices() {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sent.clone();
        let server = crate::test_server::Server::new(move |request| {
            if request.contains(EVENT_TYPE) {
                if request.starts_with("PUT") {
                    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned();
                    captured.lock().expect("requests").push(body);
                    return json!({});
                }
                // Another session saved `$other` since this device last synced.
                return json!({"tags":{"m.favourite":{"$other":{"tagged_at":5}}}});
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        account.set_saved("!room:example.org", "$root", true).await.expect("save");
        let requests = sent.lock().expect("requests");
        let body: Value = serde_json::from_str(&requests[0]).expect("saved content");
        assert_eq!(body["tags"]["m.favourite"]["$other"], json!({"tagged_at":5}));
        assert!(body["tags"]["m.favourite"]["$root"]["tagged_at"].is_u64());
    }

    #[tokio::test]
    async fn saved_messages_should_read_each_saved_event() {
        let server = crate::test_server::Server::new(|request| {
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["account_data"] = json!({"events":[
                    {"type":EVENT_TYPE,"content":{"tags":{"m.favourite":{"$root":{"tagged_at":3}}}}}
                ]});
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let saved = account.saved_messages().await.expect("saved messages");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].room_id, "!room:example.org");
        assert_eq!(saved[0].saved_at_ms, Some(3));
        assert_eq!(saved[0].preview.body.as_deref(), Some("Root"));
        assert_eq!(saved[0].preview.sender_id, "@b:example.org");
    }
}
