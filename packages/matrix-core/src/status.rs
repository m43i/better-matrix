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

/// How long announcing presence may take, retries and waits the server asks for included: it
/// runs between syncs, so a server whose presence is down holds them only briefly. The next sync
/// tries again.
const ANNOUNCE_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Content {
    status: Status,
    /// When it was chosen, in milliseconds since the Unix epoch: of two, the later one wins.
    #[serde(default)]
    changed_at: u64,
}

/// One account's status as this session knows it. Saves and syncs both settle it here, under one
/// lock, by `changed_at`: a save answered late, a sync answered before a save, and choices that
/// run out of order all lose to what was chosen later.
#[derive(Debug, Default)]
pub(crate) struct Known {
    /// Whether `saved` was read from this device's copy of the account data yet.
    loaded: bool,
    /// The latest status chosen, here or on another session.
    saved: Option<Content>,
    /// What this session last announced as presence, if that went through.
    announced: Option<Status>,
}

impl Account {
    /// The status the user chose last: from this device's copy of the account data until a save
    /// or a sync says otherwise; `None` when there is none (never set, or not synced yet).
    pub async fn status(&self) -> Option<Status> {
        self.known().await.saved.map(|saved| saved.status)
    }

    async fn known(&self) -> tokio::sync::MutexGuard<'_, Known> {
        let mut known = self.status.lock().await;
        if !known.loaded {
            let account = self.client().account();
            if let Ok(raw) = account.account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE)).await {
                known.saved = raw.and_then(|raw| serde_json::from_str(raw.json().get()).ok());
                known.loaded = true;
            }
        }
        known
    }

    /// Saves a status chosen at `changed_at` (ms since the epoch) to the account's data and
    /// announces it, unless a later one was chosen meanwhile; `report` hears the status that
    /// holds afterwards. Saves go one at a time, without holding up syncs.
    pub async fn set_status(&self, status: Status, changed_at: u64, report: impl Fn(Status)) -> Result<(), Error> {
        let _one_at_a_time = self.status_saves.lock().await;
        let content = Content { status, changed_at };
        let later = |known: &Known| known.saved.filter(|saved| saved.changed_at >= changed_at);
        {
            let known = self.known().await;
            if let Some(saved) = later(&known) {
                report(saved.status);
                return Ok(());
            }
        }
        let raw = Raw::new(&content).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE), raw).await?;
        let mut known = self.known().await;
        if later(&known).is_none() {
            known.saved = Some(content);
        }
        self.settle(&mut known, false).await;
        if let Some(saved) = known.saved {
            report(saved.status);
        }
        Ok(())
    }

    /// After each sync: takes the status it brought (set here or on another session) unless a
    /// later one is known, and `report`s it; announces the status when it came or this session
    /// hasn't announced it yet (the first sync, an announcement that failed). Syncing as offline
    /// leaves a session's presence as it was, so hiding needs this on every session; and a sync
    /// that raced a save here brings the save back, so announcing it lands after that sync.
    pub(crate) async fn follow_status(
        &self,
        account_data: &[Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>],
        report: impl Fn(Status),
    ) {
        let synced = account_data
            .iter()
            .rev()
            .filter(|raw| raw.get_field::<String>("type").ok().flatten().as_deref() == Some(EVENT_TYPE))
            .find_map(|raw| raw.get_field::<Content>("content").ok().flatten());
        let mut known = self.known().await;
        let came = synced.filter(|synced| known.saved.is_none_or(|saved| saved.changed_at <= synced.changed_at));
        if came.is_some() {
            known.saved = came;
        }
        self.settle(&mut known, came.is_some()).await;
        if let Some(came) = came {
            report(came.status);
        }
    }

    /// Announces the latest status if this session hasn't yet, or again when `again`.
    async fn settle(&self, known: &mut Known, again: bool) {
        let Some(saved) = known.saved else { return };
        if again || known.announced != Some(saved.status) {
            known.announced = self.announce_status(saved.status).await.then_some(saved.status);
        }
    }

    /// Sets this session's presence to `status` now; false when that failed or took longer than
    /// [`ANNOUNCE_WAIT`]. The status message another client set is kept, so without reading it
    /// first nothing is sent.
    async fn announce_status(&self, status: Status) -> bool {
        let Some(me) = self.client().user_id() else { return false };
        let announced = async {
            let current = self.client().send(get_presence::v3::Request::new(me.to_owned())).await?;
            let mut request = set_presence::v3::Request::new(me.to_owned(), status.presence());
            request.status_msg = current.status_msg;
            self.client().send(request).await.map(|_| ())
        };
        match tokio::time::timeout(ANNOUNCE_WAIT, announced).await {
            Ok(Ok(())) => true,
            Ok(Err(error)) => {
                eprintln!("matrix-engine: could not set the presence of {}: {error}", self.id());
                false
            }
            Err(_) => {
                eprintln!("matrix-engine: setting the presence of {} took too long", self.id());
                false
            }
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

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

    #[test]
    fn the_status_should_be_stored_by_name_with_when_it_changed() {
        let json = serde_json::to_value(Content { status: Status::Hidden, changed_at: 7 }).unwrap();
        assert_eq!(json, json!({ "status": "Hidden", "changed_at": 7 }));
    }

    #[test]
    fn a_status_saved_without_a_time_should_still_be_read() {
        let content: Content = serde_json::from_value(json!({ "status": "Away" })).unwrap();
        assert_eq!((content.status, content.changed_at), (Status::Away, 0));
    }

    /// A homeserver for status tests: each sync brings `synced` as the status event (none when
    /// `Value::Null`), presence reads answer with `message` (or fail when `readable` says so), and
    /// every PUT is recorded as (request line, JSON body).
    struct Homeserver {
        server: crate::test_server::Server,
        writes: Arc<Mutex<Vec<(String, Value)>>>,
    }

    impl Homeserver {
        fn new(synced: Value, message: Option<&'static str>, readable: impl Fn() -> bool + Send + 'static) -> Self {
            let writes = Arc::new(Mutex::new(Vec::new()));
            let captured = writes.clone();
            let server = crate::test_server::Server::new(move |request| {
                if request.starts_with("PUT") {
                    let line = request.lines().next().unwrap_or_default().to_owned();
                    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                    captured.lock().unwrap().push((line, serde_json::from_str(body).unwrap()));
                    return json!({});
                }
                if request.starts_with("GET") && request.contains("/presence/") {
                    return if readable() { json!({ "presence": "online", "status_msg": message }) } else { json!({}) };
                }
                let mut response = crate::test_server::thread_response(request);
                if request.contains("/sync?") && !synced.is_null() {
                    response["account_data"] = json!({ "events": [{ "type": EVENT_TYPE, "content": synced }] });
                }
                response
            });
            Self { server, writes }
        }

        fn plain() -> Self {
            Self::new(Value::Null, None, || true)
        }

        async fn account(&self) -> Account {
            Account::for_test(&self.server.url()).await
        }

        /// The bodies of the PUTs whose request line contains `path`.
        fn sent(&self, path: &str) -> Vec<Value> {
            let writes = self.writes.lock().unwrap();
            writes.iter().filter(|(line, _)| line.contains(path)).map(|(_, body)| body.clone()).collect()
        }

        fn presence(&self) -> Vec<Value> {
            self.sent("/presence/").into_iter().map(|body| body["presence"].clone()).collect()
        }
    }

    async fn sync(account: &Account) -> matrix_sdk::sync::SyncResponse {
        let settings = matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO);
        account.client().sync_once(settings).await.unwrap()
    }

    /// A `report` that keeps what it hears.
    fn reports() -> (Arc<Mutex<Vec<Status>>>, impl Fn(Status)) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let keep = heard.clone();
        (heard, move |status| keep.lock().unwrap().push(status))
    }

    #[tokio::test]
    async fn setting_a_status_should_save_it_announce_it_and_report_it() {
        let server = Homeserver::plain();
        let account = server.account().await;
        let (heard, report) = reports();
        account.set_status(Status::Hidden, 10, report).await.unwrap();
        assert_eq!(server.sent("/account_data/chat.bettermatrix.status")[0]["status"], "Hidden");
        assert_eq!(server.presence(), vec![json!("offline")]);
        assert_eq!(*heard.lock().unwrap(), vec![Status::Hidden]);
    }

    #[tokio::test]
    async fn setting_a_status_should_keep_the_status_message_set_elsewhere() {
        let server = Homeserver::new(Value::Null, Some("On a train"), || true);
        server.account().await.set_status(Status::Away, 10, |_| {}).await.unwrap();
        assert_eq!(server.sent("/presence/")[0]["status_msg"], "On a train");
    }

    #[tokio::test]
    async fn a_status_message_that_cannot_be_read_should_not_be_overwritten() {
        let server = Homeserver::new(Value::Null, None, || false);
        server.account().await.set_status(Status::Away, 10, |_| {}).await.unwrap();
        assert!(server.presence().is_empty());
    }

    #[tokio::test]
    async fn the_status_should_come_from_this_devices_copy_at_first() {
        let server = Homeserver::new(json!({ "status": "Away" }), None, || true);
        let account = server.account().await;
        sync(&account).await;
        assert_eq!(account.status().await, Some(Status::Away));
    }

    #[tokio::test]
    async fn an_earlier_choice_saved_after_a_later_one_should_not_win() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Hidden, 20, |_| {}).await.unwrap();
        let (heard, report) = reports();
        account.set_status(Status::Online, 10, report).await.unwrap();
        assert_eq!(server.sent("/account_data/").len(), 1);
        assert_eq!(*heard.lock().unwrap(), vec![Status::Hidden]);
        assert_eq!(account.status().await, Some(Status::Hidden));
    }

    #[tokio::test]
    async fn a_save_older_than_a_synced_status_should_keep_the_synced_one() {
        let server = Homeserver::new(json!({ "status": "Hidden", "changed_at": 50 }), None, || true);
        let account = server.account().await;
        account.follow_status(&sync(&account).await.account_data, |_| {}).await;
        let (heard, report) = reports();
        account.set_status(Status::Online, 10, report).await.unwrap();
        assert_eq!(*heard.lock().unwrap(), vec![Status::Hidden]);
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn an_older_status_a_sync_brings_should_not_undo_a_save() {
        let server = Homeserver::new(json!({ "status": "Online", "changed_at": 5 }), None, || true);
        let account = server.account().await;
        account.set_status(Status::Hidden, 10, |_| {}).await.unwrap();
        let (heard, report) = reports();
        account.follow_status(&sync(&account).await.account_data, report).await;
        assert!(heard.lock().unwrap().is_empty());
        assert_eq!(account.status().await, Some(Status::Hidden));
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_sync_answered_before_a_save_should_not_bring_the_old_status_back() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Hidden, 10, |_| {}).await.unwrap();
        account.follow_status(&sync(&account).await.account_data, |_| {}).await;
        assert_eq!(account.status().await, Some(Status::Hidden));
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_status_a_sync_brings_should_be_announced_from_this_session_and_reported() {
        let server = Homeserver::new(json!({ "status": "Hidden", "changed_at": 5 }), None, || true);
        let account = server.account().await;
        let (heard, report) = reports();
        account.follow_status(&sync(&account).await.account_data, report).await;
        assert_eq!(*heard.lock().unwrap(), vec![Status::Hidden]);
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_sync_without_a_status_should_announce_and_report_nothing() {
        let server = Homeserver::plain();
        let account = server.account().await;
        let (heard, report) = reports();
        account.follow_status(&[], report).await;
        assert!(heard.lock().unwrap().is_empty() && server.presence().is_empty());
    }

    #[tokio::test]
    async fn an_announcement_that_failed_should_be_tried_again_after_the_next_sync() {
        let reads = Arc::new(AtomicUsize::new(0));
        // The first presence read fails, later ones succeed.
        let server =
            Homeserver::new(json!({ "status": "Hidden" }), None, move || reads.fetch_add(1, Ordering::Relaxed) > 0);
        let account = server.account().await;
        account.follow_status(&sync(&account).await.account_data, |_| {}).await;
        assert!(server.presence().is_empty());
        account.follow_status(&[], |_| {}).await;
        assert_eq!(server.presence().len(), 1);
        account.follow_status(&[], |_| {}).await;
        assert_eq!(server.presence().len(), 1, "announced once it went through");
    }
}
