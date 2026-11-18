//! The status the user sets for an account: online, away, or hidden (shown as offline). Kept in
//! the account's data on the homeserver, so every session of this app syncs with the same one:
//! Matrix presence belongs to the user, not to one device, and one session left online would
//! show the user online.

use matrix_sdk::ruma::api::client::presence::{get_presence, set_presence};
use matrix_sdk::ruma::events::GlobalAccountDataEventType;
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::ruma::serde::Raw;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::account::Account;

/// The account data event the status is stored in.
const EVENT_TYPE: &str = "chat.bettermatrix.status";

/// What others see of the user, as the user set it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(ts_rs::TS))]
pub enum Status {
    /// Shown as online while a session runs.
    #[default]
    Online,
    /// Shown as away.
    Away,
    /// Shown as offline, while the sessions keep working.
    Hidden,
}

impl Status {
    /// The Matrix presence this status announces.
    pub(crate) fn presence(self) -> PresenceState {
        match self {
            Self::Online => PresenceState::Online,
            Self::Away => PresenceState::Unavailable,
            Self::Hidden => PresenceState::Offline,
        }
    }
}

/// The account data event's content.
#[derive(Serialize, Deserialize)]
struct Content {
    status: Status,
}

impl Account {
    /// The status from this device's copy of the account data; `None` when it holds none: never
    /// set, or not synced yet.
    pub async fn status(&self) -> Result<Option<Status>, Error> {
        let raw = self.client().account().account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE)).await?;
        Ok(raw.and_then(|raw| serde_json::from_str::<Content>(raw.json().get()).ok()).map(|content| content.status))
    }

    /// Saves the status to the account's data, which the next sync announces, and announces it
    /// now. Changes save one at a time, in the order they were asked for.
    pub async fn set_status(&self, status: Status) -> Result<(), Error> {
        let _one_at_a_time = self.status_writes.lock().await;
        let raw = Raw::new(&Content { status }).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE), raw).await?;
        self.announce_status(status).await;
        Ok(())
    }

    /// When a sync's account data carries a status (set here or on another session), announces
    /// it from this session too and returns it: syncing as offline leaves a session's presence
    /// as it was. A sync that raced a change here brings the change back, so the announcement
    /// lands after any sync that still carried the old status.
    pub(crate) async fn follow_status(
        &self,
        account_data: &[Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>],
    ) -> Option<Status> {
        let changed = account_data
            .iter()
            .any(|raw| raw.get_field::<String>("type").ok().flatten().as_deref() == Some(EVENT_TYPE));
        if !changed {
            return None;
        }
        let _one_at_a_time = self.status_writes.lock().await;
        let status = self.status().await.ok().flatten()?;
        self.announce_status(status).await;
        Some(status)
    }

    /// Sets this session's presence to `status` now. A sync only marks a session online or away;
    /// one that syncs as offline is left as it was, so hiding needs this on every session. The
    /// status message another client set is kept. A server that refuses only logs: syncs carry
    /// the status anyway.
    async fn announce_status(&self, status: Status) {
        let Some(me) = self.client().user_id() else { return };
        let message = match self.client().send(get_presence::v3::Request::new(me.to_owned())).await {
            Ok(current) => current.status_msg,
            Err(_) => None,
        };
        let mut request = set_presence::v3::Request::new(me.to_owned(), status.presence());
        request.status_msg = message;
        if let Err(error) = self.client().send(request).await {
            eprintln!("matrix-engine: could not set the presence of {}: {error}", self.id());
        }
    }
}

/// The presence a sync announces. Until the first sync has caught up, none (offline marks no
/// one): the status saved here may be stale, and one set elsewhere meanwhile could be Hidden.
/// After that the saved status, or Online when none was ever set.
pub(crate) fn sync_presence(saved: Option<Status>, caught_up: bool) -> PresenceState {
    match saved {
        _ if !caught_up => PresenceState::Offline,
        Some(status) => status.presence(),
        None => PresenceState::Online,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_status_should_announce_its_matrix_presence() {
        assert_eq!(Status::Online.presence(), PresenceState::Online);
        assert_eq!(Status::Away.presence(), PresenceState::Unavailable);
        assert_eq!(Status::Hidden.presence(), PresenceState::Offline);
    }

    #[test]
    fn a_sync_before_the_status_is_known_should_not_announce_the_user_online() {
        assert_eq!(sync_presence(None, false), PresenceState::Offline);
    }

    #[test]
    fn a_sync_after_catching_up_without_a_saved_status_should_announce_the_user_online() {
        assert_eq!(sync_presence(None, true), PresenceState::Online);
    }

    #[test]
    fn a_saved_status_should_decide_every_sync_after_catching_up() {
        assert_eq!(sync_presence(Some(Status::Hidden), true), PresenceState::Offline);
        assert_eq!(sync_presence(Some(Status::Away), true), PresenceState::Unavailable);
    }

    #[test]
    fn a_stale_saved_status_should_not_be_announced_before_catching_up() {
        assert_eq!(sync_presence(Some(Status::Online), false), PresenceState::Offline);
    }

    /// Sets `status` against a server whose presence carries `message`, and returns the bodies of
    /// the requests that wrote: (request line, JSON body).
    async fn writes_for(status: Status, message: Option<&'static str>) -> Vec<(String, serde_json::Value)> {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sent.clone();
        let server = crate::test_server::Server::new(move |request| {
            if request.starts_with("PUT") {
                let line = request.lines().next().unwrap_or_default().to_owned();
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                captured.lock().unwrap().push((line, serde_json::from_str(body).unwrap()));
            } else if request.starts_with("GET") && request.contains("/presence/") {
                return serde_json::json!({ "presence": "online", "status_msg": message });
            }
            serde_json::json!({})
        });
        let account = Account::for_test(&server.url()).await;
        account.set_status(status).await.unwrap();
        sent.lock().unwrap().clone()
    }

    fn body<'a>(writes: &'a [(String, serde_json::Value)], path: &str) -> &'a serde_json::Value {
        &writes.iter().find(|(line, _)| line.contains(path)).unwrap().1
    }

    #[tokio::test]
    async fn setting_a_status_should_save_it_and_announce_its_presence() {
        let writes = writes_for(Status::Hidden, None).await;
        assert_eq!(body(&writes, "/account_data/chat.bettermatrix.status")["status"], "Hidden");
        assert_eq!(body(&writes, "/presence/")["presence"], "offline");
    }

    #[tokio::test]
    async fn setting_a_status_should_keep_the_status_message_set_elsewhere() {
        let writes = writes_for(Status::Away, Some("On a train")).await;
        assert_eq!(body(&writes, "/presence/")["status_msg"], "On a train");
    }

    #[tokio::test]
    async fn the_status_should_come_from_the_synced_account_data() {
        let server = crate::test_server::Server::new(|request| {
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["account_data"] =
                    serde_json::json!({"events":[{"type":EVENT_TYPE,"content":{"status":"Away"}}]});
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        assert_eq!(account.status().await.unwrap(), None);
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .unwrap();
        assert_eq!(account.status().await.unwrap(), Some(Status::Away));
    }

    #[tokio::test]
    async fn a_status_a_sync_brings_should_be_announced_from_this_session_too() {
        let presence = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = presence.clone();
        let server = crate::test_server::Server::new(move |request| {
            if request.starts_with("PUT") && request.contains("/presence/") {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                captured.lock().unwrap().push(serde_json::from_str::<serde_json::Value>(body).unwrap());
                return serde_json::json!({});
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["account_data"] =
                    serde_json::json!({"events":[{"type":EVENT_TYPE,"content":{"status":"Hidden"}}]});
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        let response = account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .unwrap();
        assert_eq!(account.follow_status(&response.account_data).await, Some(Status::Hidden));
        assert_eq!(presence.lock().unwrap()[0]["presence"], "offline");
    }

    #[tokio::test]
    async fn a_sync_without_a_status_should_announce_nothing() {
        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = Account::for_test(&server.url()).await;
        assert_eq!(account.follow_status(&[]).await, None);
    }

    #[test]
    fn the_status_should_be_stored_by_name() {
        let json = serde_json::to_value(Content { status: Status::Hidden }).unwrap();
        assert_eq!(json, serde_json::json!({ "status": "Hidden" }));
    }
}
