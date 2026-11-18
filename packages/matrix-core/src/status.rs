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
    /// Whether a sync has brought the status yet: until then `saved` may be stale.
    synced: bool,
    /// The number of the latest status message set ([`Account::set_status_message`]).
    message_number: u64,
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
        let mut content = Content { status, changed_at };
        {
            let known = self.known().await;
            if let Some(later) = known.saved.filter(|saved| saved.changed_at >= changed_at) {
                report(later.status);
                return Ok(());
            }
        }
        loop {
            let raw = Raw::new(&content).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
            self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE), raw).await?;
            let mut known = self.known().await;
            match known.saved.filter(|saved| saved.changed_at > content.changed_at) {
                // A later status synced in while this one was saved, and this save replaced it on
                // the server: put the later one back.
                Some(later) => content = later,
                None => {
                    known.saved = Some(content);
                    self.settle(&mut known, false).await;
                    report(content.status);
                    return Ok(());
                }
            }
        }
    }

    /// Sets the status message others see beside the user's presence, or clears it with `None`,
    /// and `report`s what the user shows then. `number` orders the messages as they were set: one
    /// whose task runs after a later one's is dropped. It goes with the presence of the status
    /// that holds (Online when none was ever set), or offline until a sync has brought the status,
    /// which could be Hidden; the sync's announcement then sets the presence, keeping the message.
    /// Under the status's lock, so an announcement can't put the old message back.
    pub async fn set_status_message(
        &self,
        message: Option<&str>,
        number: u64,
        report: impl Fn(crate::PersonPresence),
    ) -> Result<(), Error> {
        let me = self.user_id()?;
        let message = crate::people::status_message(message);
        let mut known = self.known().await;
        if known.message_number > number {
            return Ok(());
        }
        known.message_number = number;
        let presence = match known.saved {
            _ if !known.synced => PresenceState::Offline,
            Some(saved) => saved.status.presence(),
            None => PresenceState::Online,
        };
        let mut request = set_presence::v3::Request::new(me.clone(), presence.clone());
        request.status_msg = message.clone();
        // Syncs wait for the lock: a server asking to retry later must not hold them up long.
        tokio::time::timeout(ANNOUNCE_WAIT, self.client().send(request)).await.map_err(|_| Error::TimedOut)??;
        report(crate::PersonPresence {
            user_id: me.to_string(),
            presence: crate::people::presence(&presence),
            status_message: message,
        });
        Ok(())
    }

    /// `report`s what the user shows as the server says now: at start, and when a sync brings a
    /// change, as what a sync or this device's store says may be older than a status message
    /// saved meanwhile. Read without the status's lock, which syncs wait for; a message saved
    /// while reading reported itself, so the read is then dropped. Nothing when the server
    /// can't be read.
    pub(crate) async fn report_own_presence(&self, report: impl Fn(crate::PersonPresence)) {
        let Ok(me) = self.user_id() else { return };
        let before = self.known().await.message_number;
        let read = self.client().send(get_presence::v3::Request::new(me.clone()));
        let current = match tokio::time::timeout(ANNOUNCE_WAIT, read).await {
            Ok(Ok(current)) => current,
            Ok(Err(error)) => return eprintln!("matrix-engine: could not read the presence of {}: {error}", self.id()),
            Err(_) => return eprintln!("matrix-engine: reading the presence of {} took too long", self.id()),
        };
        let presence = crate::people::presence(&current.presence);
        let status_message = match presence {
            crate::Presence::Unknown => None,
            _ => crate::people::status_message(current.status_msg.as_deref()),
        };
        let shown = crate::PersonPresence { user_id: me.to_string(), presence, status_message };
        self.report_read_presence(before, shown, report).await;
    }

    /// `report`s the user's own presence as read when the last status message saved was number
    /// `before`, unless one was saved since.
    async fn report_read_presence(
        &self,
        before: u64,
        shown: crate::PersonPresence,
        report: impl Fn(crate::PersonPresence),
    ) {
        if self.known().await.message_number == before {
            report(shown);
        }
    }

    /// `report`s the status known now, if any: at start, in turn with saves and syncs.
    pub(crate) async fn report_status(&self, report: impl Fn(Status)) {
        if let Some(saved) = self.known().await.saved {
            report(saved.status);
        }
    }

    /// After each sync, which `sent` this presence: takes the status it brought (set here or on
    /// another session) unless a later one is known, and `report`s it. Announces the status when
    /// one came, when the sync sent another presence (it was asked for before a save here, and
    /// may have reached the server after the save's announcement), or when this session hasn't
    /// announced it yet (the first sync, an announcement that failed). Syncing as offline leaves
    /// a session's presence as it was, so hiding needs this on every session.
    pub(crate) async fn follow_status(
        &self,
        account_data: &[Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>],
        sent: &PresenceState,
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
        known.synced = true;
        let raced = known.saved.is_some_and(|saved| saved.status.presence() != *sent);
        self.settle(&mut known, came.is_some() || raced).await;
        if let Some(came) = came {
            report(came.status);
        }
    }

    /// After a sync that failed, which `sent` this presence: the server may have applied it
    /// before failing, so when it isn't the current status's, the next sync announces again.
    pub(crate) async fn sync_failed(&self, sent: &PresenceState) {
        let mut known = self.known().await;
        if known.saved.is_some_and(|saved| saved.status.presence() != *sent) {
            known.announced = None;
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

    /// A presence `report` that keeps what it hears.
    fn shown() -> (Arc<Mutex<Vec<crate::PersonPresence>>>, impl Fn(crate::PersonPresence)) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let keep = heard.clone();
        (heard, move |me| keep.lock().unwrap().push(me))
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
    async fn a_later_status_synced_while_saving_should_be_put_back_on_the_server() {
        let (release, released) = std::sync::mpsc::channel::<()>();
        let (released, saving) = (Mutex::new(released), Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let writes = Arc::new(Mutex::new(Vec::new()));
        let (seen, captured) = (saving.clone(), writes.clone());
        let server = crate::test_server::Server::new(move |request| {
            if request.starts_with("PUT") && request.contains("/account_data/") {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                captured.lock().unwrap().push(serde_json::from_str::<Value>(body).unwrap());
                // The first save waits until the test let a later status in.
                if !seen.swap(true, Ordering::Relaxed) {
                    released.lock().unwrap().recv().unwrap();
                }
            }
            json!({})
        });
        let account = Account::for_test(&server.url()).await;
        let (heard, report) = reports();
        let later_status_syncs = async {
            while !saving.load(Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let mut known = account.status.lock().await;
            known.saved = Some(Content { status: Status::Hidden, changed_at: 50 });
            drop(known);
            release.send(()).unwrap();
        };
        let (saved, ()) = tokio::join!(account.set_status(Status::Online, 10, report), later_status_syncs);
        saved.unwrap();
        let statuses: Vec<Value> = writes.lock().unwrap().iter().map(|body| body["status"].clone()).collect();
        assert_eq!(statuses, vec![json!("Online"), json!("Hidden")]);
        assert_eq!(*heard.lock().unwrap(), vec![Status::Hidden]);
    }

    #[tokio::test]
    async fn a_save_older_than_a_synced_status_should_keep_the_synced_one() {
        let server = Homeserver::new(json!({ "status": "Hidden", "changed_at": 50 }), None, || true);
        let account = server.account().await;
        account.follow_status(&sync(&account).await.account_data, &PresenceState::Offline, |_| {}).await;
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
        account.follow_status(&sync(&account).await.account_data, &PresenceState::Offline, report).await;
        assert!(heard.lock().unwrap().is_empty());
        assert_eq!(account.status().await, Some(Status::Hidden));
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_sync_answered_before_a_save_should_not_bring_the_old_status_back() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Hidden, 10, |_| {}).await.unwrap();
        account.follow_status(&sync(&account).await.account_data, &PresenceState::Offline, |_| {}).await;
        assert_eq!(account.status().await, Some(Status::Hidden));
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_sync_that_sent_the_presence_from_before_a_save_should_be_followed_by_the_saved_one() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Hidden, 10, |_| {}).await.unwrap();
        account.follow_status(&[], &PresenceState::Online, |_| {}).await;
        assert_eq!(server.presence(), vec![json!("offline"), json!("offline")]);
    }

    #[tokio::test]
    async fn a_failed_sync_that_sent_the_presence_from_before_a_save_should_be_followed_by_the_saved_one() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Hidden, 10, |_| {}).await.unwrap();
        account.sync_failed(&PresenceState::Online).await;
        account.follow_status(&[], &PresenceState::Offline, |_| {}).await;
        assert_eq!(server.presence(), vec![json!("offline"), json!("offline")]);
    }

    #[tokio::test]
    async fn a_failed_sync_that_sent_the_current_presence_should_not_announce_again() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Hidden, 10, |_| {}).await.unwrap();
        account.sync_failed(&PresenceState::Offline).await;
        account.follow_status(&[], &PresenceState::Offline, |_| {}).await;
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_status_a_sync_brings_should_be_announced_from_this_session_and_reported() {
        let server = Homeserver::new(json!({ "status": "Hidden", "changed_at": 5 }), None, || true);
        let account = server.account().await;
        let (heard, report) = reports();
        account.follow_status(&sync(&account).await.account_data, &PresenceState::Offline, report).await;
        assert_eq!(*heard.lock().unwrap(), vec![Status::Hidden]);
        assert_eq!(server.presence(), vec![json!("offline")]);
    }

    #[tokio::test]
    async fn a_sync_without_a_status_should_announce_and_report_nothing() {
        let server = Homeserver::plain();
        let account = server.account().await;
        let (heard, report) = reports();
        account.follow_status(&[], &PresenceState::Offline, report).await;
        assert!(heard.lock().unwrap().is_empty() && server.presence().is_empty());
    }

    #[tokio::test]
    async fn an_announcement_that_failed_should_be_tried_again_after_the_next_sync() {
        let reads = Arc::new(AtomicUsize::new(0));
        // The first presence read fails, later ones succeed.
        let server =
            Homeserver::new(json!({ "status": "Hidden" }), None, move || reads.fetch_add(1, Ordering::Relaxed) > 0);
        let account = server.account().await;
        account.follow_status(&sync(&account).await.account_data, &PresenceState::Offline, |_| {}).await;
        assert!(server.presence().is_empty());
        account.follow_status(&[], &PresenceState::Offline, |_| {}).await;
        assert_eq!(server.presence().len(), 1);
        account.follow_status(&[], &PresenceState::Offline, |_| {}).await;
        assert_eq!(server.presence().len(), 1, "announced once it went through");
    }

    #[tokio::test]
    async fn a_status_message_should_be_announced_trimmed_with_the_status_presence() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status(Status::Away, 10, |_| {}).await.unwrap();
        account.follow_status(&[], &PresenceState::Unavailable, |_| {}).await;
        let (heard, report) = shown();
        account.set_status_message(Some("  🌴 On vacation "), 1, report).await.unwrap();
        let sent = server.sent("/presence/");
        assert_eq!(sent.last(), Some(&json!({ "presence": "unavailable", "status_msg": "🌴 On vacation" })));
        let me = heard.lock().unwrap().pop().unwrap();
        assert_eq!((me.presence, me.status_message.as_deref()), (crate::Presence::Away, Some("🌴 On vacation")));
    }

    #[tokio::test]
    async fn a_blank_status_message_should_clear_it() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.follow_status(&[], &PresenceState::Online, |_| {}).await;
        let (heard, report) = shown();
        account.set_status_message(Some("   "), 1, report).await.unwrap();
        assert_eq!(server.sent("/presence/").last(), Some(&json!({ "presence": "online" })));
        assert_eq!(heard.lock().unwrap().pop().map(|me| me.status_message), Some(None));
    }

    #[tokio::test]
    async fn the_users_own_presence_should_be_read_from_the_server() {
        let server = Homeserver::new(Value::Null, Some("🌴 Out"), || true);
        let account = server.account().await;
        let (heard, report) = shown();
        account.report_own_presence(report).await;
        let me = heard.lock().unwrap().pop().unwrap();
        assert_eq!((me.presence, me.status_message.as_deref()), (crate::Presence::Online, Some("🌴 Out")));
    }

    #[tokio::test]
    async fn own_presence_read_before_a_status_message_was_saved_should_be_dropped() {
        let server = Homeserver::plain();
        let account = server.account().await;
        let read =
            crate::PersonPresence { user_id: "@me:x".into(), presence: crate::Presence::Online, status_message: None };
        account.set_status_message(Some("Out"), 1, |_| {}).await.unwrap();
        let (heard, report) = shown();
        account.report_read_presence(0, read, report).await;
        assert!(heard.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reading_own_presence_should_not_hold_the_status_for_syncs() {
        // The server answers presence reads only once the status could be read meanwhile.
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let server = Homeserver::new(Value::Null, Some("Out"), move || {
            rx.lock().unwrap().recv_timeout(std::time::Duration::from_secs(5)).is_ok()
        });
        let account = Arc::new(server.account().await);
        let reading = {
            let account = Arc::clone(&account);
            tokio::spawn(async move { account.report_own_presence(|_| {}).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let status = tokio::time::timeout(std::time::Duration::from_secs(1), account.status()).await;
        tx.send(()).unwrap();
        reading.await.unwrap();
        assert!(status.is_ok(), "a sync could read the status while presence was being read");
    }

    #[tokio::test]
    async fn an_unreadable_own_presence_should_report_nothing() {
        let server = Homeserver::new(Value::Null, Some("🌴 Out"), || false);
        let account = server.account().await;
        let (heard, report) = shown();
        account.report_own_presence(report).await;
        assert!(heard.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_status_message_set_before_a_sync_brought_the_status_should_go_out_offline() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status_message(Some("Out"), 1, |_| {}).await.unwrap();
        assert_eq!(server.sent("/presence/"), vec![json!({ "presence": "offline", "status_msg": "Out" })]);
    }

    #[tokio::test]
    async fn an_earlier_status_message_saved_after_a_later_one_should_be_dropped() {
        let server = Homeserver::plain();
        let account = server.account().await;
        account.set_status_message(None, 2, |_| {}).await.unwrap();
        let (heard, report) = shown();
        account.set_status_message(Some("Out"), 1, report).await.unwrap();
        assert_eq!(server.sent("/presence/").len(), 1);
        assert!(heard.lock().unwrap().is_empty());
    }
}
