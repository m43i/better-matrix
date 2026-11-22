//! MSC4140's requests as the proposal now has them. Ruma 0.17 sends `delay` where the proposal
//! and Synapse take `delay_ms`, and reads a list in the shape servers sent before; these speak
//! the current shape, and [`Listed`] reads both.

use matrix_sdk::ruma::api::auth_scheme::AccessToken;
use matrix_sdk::ruma::api::{metadata, request, response};
use matrix_sdk::ruma::events::TimelineEventType;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedRoomId, OwnedTransactionId};
use serde_json::Value;

/// `PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}`:
/// schedule an event.
pub mod schedule {
    use super::*;

    metadata! {
        method: PUT,
        rate_limited: true,
        authentication: AccessToken,
        history: {
            unstable("org.matrix.msc4140") => "/_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}",
        }
    }

    /// What to send and when.
    #[request]
    pub struct Request {
        /// The room it goes to.
        #[ruma_api(path)]
        pub room_id: OwnedRoomId,
        /// Its type.
        #[ruma_api(path)]
        pub event_type: TimelineEventType,
        /// Makes a retry the same request.
        #[ruma_api(path)]
        pub txn_id: OwnedTransactionId,
        /// How long the server waits, in milliseconds.
        pub delay_ms: u64,
        /// The event's content.
        pub content: Raw<Value>,
    }

    /// The server's ID for it.
    #[response]
    pub struct Response {
        /// To send it now or cancel it.
        pub delay_id: String,
    }

    impl Request {
        /// A message event `content` of `event_type`, sent into `room_id` after `delay_ms`.
        pub fn new(
            room_id: OwnedRoomId,
            event_type: TimelineEventType,
            txn_id: OwnedTransactionId,
            delay_ms: u64,
            content: Raw<Value>,
        ) -> Self {
            Self { room_id, event_type, txn_id, delay_ms, content }
        }
    }
}

/// `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events`: the user's delayed events.
pub mod list {
    use super::*;

    metadata! {
        method: GET,
        rate_limited: true,
        authentication: AccessToken,
        history: {
            unstable("org.matrix.msc4140") => "/_matrix/client/unstable/org.matrix.msc4140/delayed_events",
        }
    }

    /// Nothing to say.
    #[request]
    #[derive(Default)]
    pub struct Request {}

    /// Each delayed event, as the server sends it.
    #[response]
    pub struct Response {
        /// Read with [`Listed::read`].
        pub delayed_events: Vec<Value>,
    }
}

/// `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`: one delayed
/// event, finalised ones included while the server remembers them.
pub mod get {
    use super::*;

    metadata! {
        method: GET,
        rate_limited: false,
        authentication: AccessToken,
        history: {
            unstable("org.matrix.msc4140") => "/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}",
        }
    }

    /// Which.
    #[request]
    pub struct Request {
        /// Its ID.
        #[ruma_api(path)]
        pub delay_id: String,
    }

    /// The delayed event, as a list holds one.
    #[response]
    pub struct Response {
        /// Read with [`Listed::read`].
        #[ruma_api(body)]
        pub event: Value,
    }

    impl Request {
        pub fn new(delay_id: String) -> Self {
            Self { delay_id }
        }
    }
}

/// One delayed event from a list, in the proposal's current shape (`delay_ms`,
/// `delayed_since_ts`, `finalised`) or the one before it (`delay`, `running_since`,
/// `finalised_ts`, `event_id`, `error`).
#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub delay_id: String,
    pub room_id: String,
    pub event_type: String,
    /// When it goes, in milliseconds since the epoch.
    pub send_at_ms: u64,
    pub content: Value,
    /// It is a state event.
    pub state: bool,
    /// It was sent, cancelled or failed.
    pub finished: bool,
    /// Why it failed: the server's error message, else its code.
    pub error: Option<String>,
}

impl Listed {
    /// `None` when it isn't a delayed event this app can read.
    pub fn read(event: &Value) -> Option<Self> {
        let delay = event["delay_ms"].as_u64().or_else(|| event["delay"].as_u64())?;
        let since = event["delayed_since_ts"].as_u64().or_else(|| event["running_since"].as_u64())?;
        let error =
            [&event["finalised"]["error"], &event["error"]].into_iter().find(|error| !error.is_null()).map(|error| {
                let said = error["error"].as_str().or_else(|| error["errcode"].as_str()).or_else(|| error.as_str());
                said.unwrap_or("M_UNKNOWN").to_owned()
            });
        let finished = !event["finalised"].is_null()
            || !event["finalised_ts"].is_null()
            || !event["event_id"].is_null()
            || !event["error"].is_null();
        Some(Self {
            delay_id: event["delay_id"].as_str()?.to_owned(),
            room_id: event["room_id"].as_str()?.to_owned(),
            event_type: event["type"].as_str()?.to_owned(),
            send_at_ms: since.saturating_add(delay),
            content: event["content"].clone(),
            state: !event["state_key"].is_null(),
            finished,
            error,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_delayed_event_in_the_current_shape_should_be_read() {
        let event = json!({
            "delay_id": "d1", "room_id": "!r:x", "type": "m.room.message", "delay_ms": 60000,
            "delayed_since_ts": 1000, "content": { "body": "later" }
        });
        let listed = Listed::read(&event).expect("read");
        assert_eq!((listed.send_at_ms, listed.finished, listed.state), (61_000, false, false));
    }

    #[test]
    fn a_finalised_delayed_event_should_read_as_finished() {
        let event = json!({
            "delay_id": "d1", "room_id": "!r:x", "type": "m.room.message", "delay_ms": 1,
            "delayed_since_ts": 1, "content": {}, "finalised": { "event_id": "$e", "finalised_ts": 2 }
        });
        assert!(Listed::read(&event).expect("read").finished);
    }

    #[test]
    fn a_delayed_event_that_failed_should_say_why() {
        let event = json!({
            "delay_id": "d1", "room_id": "!r:x", "type": "m.room.message", "delay_ms": 1,
            "delayed_since_ts": 1, "content": {},
            "finalised": { "error": { "errcode": "M_FORBIDDEN", "error": "No posting" }, "finalised_ts": 2 }
        });
        assert_eq!(Listed::read(&event).expect("read").error.as_deref(), Some("No posting"));
    }

    #[test]
    fn a_delayed_event_sent_should_have_no_error() {
        let event = json!({
            "delay_id": "d1", "room_id": "!r:x", "type": "m.room.message", "delay_ms": 1,
            "delayed_since_ts": 1, "content": {}, "finalised": { "event_id": "$e", "finalised_ts": 2 }
        });
        assert_eq!(Listed::read(&event).expect("read").error, None);
    }

    #[test]
    fn a_delayed_event_in_the_earlier_shape_should_be_read() {
        let event = json!({
            "delay_id": "d1", "room_id": "!r:x", "type": "m.room.message", "delay": 60000,
            "running_since": 1000, "content": {}, "finalised_ts": 5
        });
        let listed = Listed::read(&event).expect("read");
        assert_eq!((listed.send_at_ms, listed.finished), (61_000, true));
    }
}
