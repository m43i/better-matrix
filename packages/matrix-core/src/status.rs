//! The status the user sets for an account: online, away, or hidden (shown as offline). Kept in
//! the account's data on the homeserver, so every session of this app syncs with the same one:
//! Matrix presence belongs to the user, not to one device, and one session left online would
//! show the user online.

use matrix_sdk::ruma::api::client::presence::set_presence;
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

/// Whether a sync's account data carries a new status.
pub(crate) fn changed_in(account_data: &[Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>]) -> bool {
    account_data.iter().any(|raw| raw.get_field::<String>("type").ok().flatten().as_deref() == Some(EVENT_TYPE))
}

impl Account {
    /// The status from this device's copy of the account data; `None` when it holds none: never
    /// set, or not synced yet.
    pub async fn status(&self) -> Result<Option<Status>, Error> {
        let raw = self.client().account().account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE)).await?;
        Ok(raw.and_then(|raw| serde_json::from_str::<Content>(raw.json().get()).ok()).map(|content| content.status))
    }

    /// Saves the status to the account's data, which the next sync announces, and announces it
    /// now. A server that refuses presence only logs: the status stays saved, and syncs carry it.
    pub async fn set_status(&self, status: Status) -> Result<(), Error> {
        let raw = Raw::new(&Content { status }).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE), raw).await?;
        if let Some(me) = self.client().user_id() {
            let request = set_presence::v3::Request::new(me.to_owned(), status.presence());
            if let Err(error) = self.client().send(request).await {
                eprintln!("matrix-engine: could not set the presence of {}: {error}", self.id());
            }
        }
        Ok(())
    }
}

/// The presence a sync announces: the saved status, or until the first sync has caught up, none
/// at all (offline), so a hidden user never shows online while their status is still unknown.
pub(crate) fn sync_presence(saved: Option<Status>, caught_up: bool) -> PresenceState {
    match saved {
        Some(status) => status.presence(),
        None if caught_up => PresenceState::Online,
        None => PresenceState::Offline,
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
    fn a_saved_status_should_decide_every_sync() {
        assert_eq!(sync_presence(Some(Status::Hidden), true), PresenceState::Offline);
        assert_eq!(sync_presence(Some(Status::Away), false), PresenceState::Unavailable);
    }

    #[tokio::test]
    async fn setting_a_status_should_save_it_and_announce_its_presence() {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sent.clone();
        let server = crate::test_server::Server::new(move |request| {
            if request.starts_with("PUT") {
                let line = request.lines().next().unwrap_or_default().to_owned();
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned();
                captured.lock().unwrap().push((line, body));
            }
            serde_json::json!({})
        });
        let account = Account::for_test(&server.url()).await;
        account.set_status(Status::Hidden).await.unwrap();
        let sent = sent.lock().unwrap();
        let body = |path: &str| {
            let (_, body) = sent.iter().find(|(line, _)| line.contains(path)).unwrap();
            serde_json::from_str::<serde_json::Value>(body).unwrap()
        };
        assert_eq!(body("/account_data/chat.bettermatrix.status")["status"], "Hidden");
        assert_eq!(body("/presence/")["presence"], "offline");
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

    #[test]
    fn the_status_should_be_stored_by_name() {
        let json = serde_json::to_value(Content { status: Status::Hidden }).unwrap();
        assert_eq!(json, serde_json::json!({ "status": "Hidden" }));
    }
}
