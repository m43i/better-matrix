//! An event's source, for developer tools: its JSON as this device reads it (decrypted when it
//! was encrypted) and, for an encrypted event, as the server holds it.

use matrix_sdk::ruma::api::client::room::get_room_event;
use serde_json::Value;

use crate::Error;
use crate::account::Account;

/// An event's JSON, pretty-printed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct EventSource {
    /// The room it was sent in.
    pub room_id: String,
    /// The event.
    pub event_id: String,
    /// The event as this device reads it: decrypted, when it came encrypted.
    pub json: String,
    /// For an encrypted event, as it was sent, when the server still has it.
    pub encrypted: Option<String>,
}

impl Account {
    /// An event's source, from the local cache or else the server.
    pub async fn event_source(&self, room_id: &str, event_id: &str) -> Result<EventSource, Error> {
        let room = self.room(room_id)?;
        let id = matrix_sdk::ruma::EventId::parse(event_id)?;
        let event = room.load_or_fetch_event(&id, None).await?;
        let json = pretty(event.raw().json().get());
        // The cache keeps the decrypted event; the server still has it as it was sent.
        let encrypted = if event.encryption_info().is_some() {
            let request = get_room_event::v3::Request::new(room.room_id().to_owned(), id);
            self.client().send(request).await.ok().map(|response| pretty(response.event.json().get()))
        } else {
            None
        };
        Ok(EventSource { room_id: room_id.to_owned(), event_id: event_id.to_owned(), json, encrypted })
    }
}

/// JSON indented for reading; as it came when it doesn't parse.
fn pretty(json: &str) -> String {
    serde_json::from_str::<Value>(json)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| json.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[tokio::test]
    async fn an_events_source_should_be_its_json_indented() {
        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = crate::Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let source = account.event_source("!room:example.org", "$root").await.expect("source");
        let parsed: serde_json::Value = serde_json::from_str(&source.json).expect("json");
        assert_eq!(parsed["content"]["body"], json!("Root"));
        assert!(source.json.contains("\n  \""), "indented: {}", source.json);
        assert_eq!(source.encrypted, None, "it came unencrypted");
    }
}
