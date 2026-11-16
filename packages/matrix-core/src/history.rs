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

impl Account {
    fn room(&self, room_id: &str) -> Result<Room, Error> {
        self.client()
            .get_room(&RoomId::parse(room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.to_owned() })
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
