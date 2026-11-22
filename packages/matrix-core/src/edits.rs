//! A message's past: the versions an edited message went through, and where a message lives
//! (its room, or a thread in it), for jumping to it from a link.

use matrix_sdk::room::{IncludeRelations, RelationsOptions};
use matrix_sdk::ruma::events::relation::RelationType;
use serde_json::Value;

use crate::model::{EditVersion, TimelineKey};
use crate::{Account, Error};

/// The most edits read for a history: a message edited more often shows its latest ones.
const MOST_EDITS: usize = 200;

impl Account {
    /// Every version of the message `event_id` in `room_id`, oldest (the original) first. Edits
    /// count only from the message's own sender, as the spec says, and a version its sender
    /// can't read (undecryptable) is left out.
    pub async fn edit_history(&self, room_id: &str, event_id: &str) -> Result<Vec<EditVersion>, Error> {
        let room = self.room(room_id)?;
        let event_id = matrix_sdk::ruma::EventId::parse(event_id)?;
        let original = room.load_or_fetch_event(&event_id, None).await?;
        let original: Value = original.raw().deserialize_as_unchecked().map_err(|_| Error::MessageUnavailable)?;
        let mut edits = Vec::new();
        let mut from = None;
        loop {
            let options = RelationsOptions {
                from,
                limit: Some(50u32.into()),
                include_relations: IncludeRelations::RelationsOfType(RelationType::Replacement),
                ..Default::default()
            };
            let page = room.relations(event_id.clone(), options).await?;
            edits.extend(page.chunk.iter().filter_map(|event| event.raw().deserialize_as_unchecked::<Value>().ok()));
            from = page.next_batch_token;
            if from.is_none() || edits.len() >= MOST_EDITS {
                break;
            }
        }
        Ok(versions(&original, edits))
    }

    /// Where the message `event_id` in `room_id` shows: the room's timeline, or the thread it
    /// replies in; and the message's own ID there. An edit names the message it edits, which
    /// shows its latest version (as the spec has a link to an edit go to the message).
    pub async fn locate_message(&self, room_id: &str, event_id: &str) -> Result<(TimelineKey, String), Error> {
        let room = self.room(room_id)?;
        let load = |event_id: matrix_sdk::ruma::OwnedEventId| {
            let room = room.clone();
            async move {
                let event = room.load_or_fetch_event(&event_id, None).await?;
                event.raw().deserialize_as_unchecked::<Value>().map_err(|_| Error::MessageUnavailable)
            }
        };
        let mut event = load(matrix_sdk::ruma::EventId::parse(event_id)?).await?;
        let mut event_id = event_id.to_owned();
        if let Some(edited) = edited_by(&event) {
            event = load(matrix_sdk::ruma::EventId::parse(&edited)?).await?;
            event_id = edited;
        }
        Ok((TimelineKey { room_id: room_id.to_owned(), thread: thread_of(&event) }, event_id))
    }
}

/// The message `event` edits, when it is an edit.
fn edited_by(event: &Value) -> Option<String> {
    let relation = &event["content"]["m.relates_to"];
    (relation["rel_type"].as_str() == Some("m.replace"))
        .then(|| relation["event_id"].as_str().map(str::to_owned))
        .flatten()
}

/// The thread root a message replies in, if it is in a thread.
fn thread_of(event: &Value) -> Option<String> {
    let relation = &event["content"]["m.relates_to"];
    (relation["rel_type"].as_str() == Some("m.thread"))
        .then(|| relation["event_id"].as_str().map(str::to_owned))
        .flatten()
}

/// A message's versions from its event and its edits' events, oldest first.
fn versions(original: &Value, edits: Vec<Value>) -> Vec<EditVersion> {
    let sender = original["sender"].as_str();
    let mut versions: Vec<EditVersion> = edits
        .iter()
        .filter(|edit| edit["sender"].as_str() == sender)
        .filter_map(|edit| version(edit, &edit["content"]["m.new_content"]))
        .collect();
    versions.sort_by_key(|version| version.timestamp_ms);
    if let Some(first) = version(original, &original["content"]) {
        versions.insert(0, first);
    }
    versions
}

/// One version: `event`'s ID and time, the text in `content`.
fn version(event: &Value, content: &Value) -> Option<EditVersion> {
    let body = content["body"].as_str()?;
    let html = (content["format"].as_str() == Some("org.matrix.custom.html"))
        .then(|| content["formatted_body"].as_str().map(str::to_owned))
        .flatten();
    let body = match content["msgtype"].as_str() {
        Some("m.emote") => format!("* {body}"),
        _ => body.to_owned(),
    };
    Some(EditVersion {
        event_id: event["event_id"].as_str()?.to_owned(),
        body,
        html,
        timestamp_ms: event["origin_server_ts"].as_u64().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn original() -> Value {
        json!({ "event_id": "$1", "sender": "@ada:x", "origin_server_ts": 10, "content": { "msgtype": "m.text", "body": "helo" } })
    }

    fn edit(id: &str, sender: &str, at: u64, body: &str) -> Value {
        json!({
            "event_id": id, "sender": sender, "origin_server_ts": at,
            "content": {
                "msgtype": "m.text", "body": format!("* {body}"),
                "m.new_content": { "msgtype": "m.text", "body": body },
                "m.relates_to": { "rel_type": "m.replace", "event_id": "$1" }
            }
        })
    }

    #[test]
    fn versions_should_start_with_the_original_and_follow_in_time() {
        let found = versions(&original(), vec![edit("$3", "@ada:x", 30, "hello!"), edit("$2", "@ada:x", 20, "hello")]);
        let bodies: Vec<_> = found.iter().map(|version| version.body.as_str()).collect();
        assert_eq!(bodies, ["helo", "hello", "hello!"]);
    }

    #[test]
    fn an_edit_by_someone_else_should_not_count() {
        let found = versions(&original(), vec![edit("$2", "@eve:x", 20, "pwned")]);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_version_should_keep_its_formatting() {
        let mut formatted = edit("$2", "@ada:x", 20, "hello");
        formatted["content"]["m.new_content"]["format"] = json!("org.matrix.custom.html");
        formatted["content"]["m.new_content"]["formatted_body"] = json!("<b>hello</b>");
        assert_eq!(versions(&original(), vec![formatted])[1].html.as_deref(), Some("<b>hello</b>"));
    }

    #[test]
    fn a_thread_reply_should_be_found_in_its_thread() {
        let reply = json!({ "content": { "m.relates_to": { "rel_type": "m.thread", "event_id": "$root" } } });
        assert_eq!(thread_of(&reply).as_deref(), Some("$root"));
    }

    #[test]
    fn an_edit_should_name_the_message_it_edits() {
        let edit = json!({ "content": { "m.relates_to": { "rel_type": "m.replace", "event_id": "$original" } } });
        assert_eq!(edited_by(&edit).as_deref(), Some("$original"));
    }

    #[test]
    fn a_reply_should_name_no_edited_message() {
        let reply = json!({ "content": { "m.relates_to": { "m.in_reply_to": { "event_id": "$q" } } } });
        assert_eq!(edited_by(&reply), None);
    }

    #[test]
    fn a_reply_outside_threads_should_be_found_in_the_room() {
        let reply = json!({ "content": { "m.relates_to": { "m.in_reply_to": { "event_id": "$q" } } } });
        assert_eq!(thread_of(&reply), None);
    }
}
