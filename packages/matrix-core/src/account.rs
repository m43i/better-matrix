//! One signed-in account and the async operations on it. The [`engine`](crate::engine) calls
//! these from its tasks; they never touch UI state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use matrix_sdk::attachment::AttachmentConfig;
use matrix_sdk::authentication::oauth::registration::{ApplicationType, ClientMetadata, Localized, OAuthGrantType};
use matrix_sdk::encryption::{BackupDownloadStrategy, EncryptionSettings};
use matrix_sdk::media::MediaRequestParameters;
use matrix_sdk::notification_settings::{IsEncrypted, IsOneToOne};
use matrix_sdk::reqwest::Url;
use matrix_sdk::ruma::RoomId;
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::utils::local_server::{LocalServerBuilder, LocalServerRedirectHandle};
use matrix_sdk::{Client, Room, SessionChange};
use tokio::sync::broadcast;

use crate::model::{
    AccountId, AccountSummary, MediaRef, MediaSize, NotificationMode, Outgoing, RoomList, RoomPriority, SignInMethod,
    TimelineKey, Upload,
};
use crate::session::{Sessions, StoredAccount, StoredAuth, StoredSecrets, new_passphrase};
use crate::store::StoreLayout;
use crate::{DEVICE_DISPLAY_NAME, Error, convert};

/// How long a browser sign-in may take before the engine gives up waiting.
const BROWSER_LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// What a password login needs.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct LoginRequest {
    /// The resolved homeserver URL, from [`Homeserver::url`](crate::Homeserver::url).
    pub homeserver: String,
    /// A localpart (`alice`) or full user ID (`@alice:example.org`).
    pub username: String,
    /// The account password.
    pub password: String,
    /// An account the server signed out softly, whose store and device this sign-in takes
    /// back, so its history and keys carry on.
    pub resume: Option<AccountId>,
}

// Hand-written so a password never reaches a log line.
impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("homeserver", &self.homeserver)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// How this app describes itself to OAuth servers when it registers.
#[derive(Clone, Debug)]
pub struct ClientIdentity {
    /// The app's name, shown on the consent screen.
    pub name: String,
    /// The app's public `https` home page; servers show it and may require it.
    pub uri: Url,
}

/// A signed-in account and the client that talks to its homeserver. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Account {
    id: AccountId,
    client: Client,
    /// Token changes, subscribed before the client's first request: a refresh during that request
    /// would otherwise go unsaved, and the server rejects the old refresh token at the next
    /// launch. [`Account::session_changes`] hands it to whoever saves them.
    session_changes: Arc<Mutex<Option<broadcast::Receiver<SessionChange>>>>,
    /// The LiveKit service the homeserver names for calls, once looked up.
    rtc: Arc<Mutex<Option<String>>>,
    /// Whether the homeserver names TURN servers, for one-to-one calls, once looked up.
    turn: Arc<std::sync::atomic::AtomicBool>,
    /// The Jitsi server the homeserver names for meetings, once looked up (`crate::widgets::meetings`).
    pub(crate) meeting_server: Arc<Mutex<Option<String>>>,
    /// The user's status as this session knows it (`crate::status`).
    pub(crate) status: Arc<tokio::sync::Mutex<crate::status::Known>>,
    /// Held while a status is saved: one save at a time, apart from the lock syncs take.
    pub(crate) status_saves: Arc<tokio::sync::Mutex<()>>,
    /// Held while a room's saved messages change: one change at a time (`crate::saved`).
    pub(crate) saved_changes: Arc<tokio::sync::Mutex<()>>,
    /// Held while a safety choice changes: one change at a time, each on the server's latest
    /// copy; it holds the choices the server last confirmed (`crate::safety`).
    pub(crate) safety_changes: Arc<tokio::sync::Mutex<Option<crate::Safety>>>,
    /// Safety changes, run in the order they were asked for: two quick choices of one setting
    /// end on the later one (`crate::safety`).
    pub(crate) safety_queue: crate::drafts::DraftQueue,
    /// Changes to rooms' and spaces' settings, power levels and rooms, run in the order the
    /// engine got them: each reads the state the one before left, so two edits of the same event
    /// can't undo each other (`crate::admin`).
    pub(crate) admin_changes: crate::drafts::DraftQueue,
    /// Draft reads and writes, in the order they were asked for (`crate::drafts`).
    pub(crate) drafts: crate::drafts::DraftQueue,
    /// The room list's sliding sync, while it runs (servers with MSC4186; ADR 0002).
    room_list: Arc<Mutex<Option<Arc<matrix_sdk_ui::RoomListService>>>>,
    /// Rooms' state the room list's sliding sync doesn't bring (image packs, widgets), as read
    /// from the server, by room, once a run (`crate::widgets::EXTRA_STATE`).
    pub(crate) extra_state: Arc<Mutex<crate::widgets::ExtraState>>,
    /// Held while the account's image packs change, so changes go one at a time.
    pub(crate) pack_edits: Arc<tokio::sync::Mutex<()>>,
    /// The rooms open timelines show, which the room list's sliding sync subscribes to.
    followed: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Held while the room list's subscriptions are updated (`Account::subscribe_followed`).
    subscribing: Arc<tokio::sync::Mutex<()>>,
    /// Call memberships in Element Call's matrix_2_0 mode, which the SDK doesn't keep.
    pub(crate) sticky: crate::calls::StickyMembers,
    /// Changes to rooms' tags and unread marks and to the keywords, run one at a time in the
    /// order the engine was asked for them (queued as commands arrive, not as their tasks start).
    pub(crate) organising: crate::drafts::DraftQueue,
    /// Whether this session runs the account's offline device (`crate::offline_device`).
    pub(crate) offline_device: Arc<tokio::sync::watch::Sender<crate::OfflineDevice>>,
    /// Held while the offline device is turned on or off, taken over or replaced: one at a time.
    pub(crate) offline_changes: Arc<tokio::sync::Mutex<()>>,
    /// Where the account's secrets are saved (none in tests): the offline device's key goes there.
    pub(crate) sessions: Option<Sessions>,
    /// Held while a scheduled message is encrypted: one at a time (`crate::scheduled`).
    pub(crate) scheduling: Arc<tokio::sync::Mutex<()>>,
    /// Held while the scheduled messages are listed and reported: lists go out in the order
    /// asked, so an older one never covers a newer.
    pub(crate) listing_scheduled: Arc<tokio::sync::Mutex<()>>,
    /// Held while the messages this account scheduled are read and written in its store, for a
    /// server that can't list them (`crate::scheduled`).
    pub(crate) scheduled_here: Arc<tokio::sync::Mutex<()>>,
    /// The client's own crypto, for sends the SDK doesn't encrypt itself (`crate::scheduled`);
    /// `None` in tests.
    pub(crate) crypto: Option<Box<Crypto>>,
}

/// The base client a [`Client`] was built on, kept to reach its encryption machine: the SDK
/// encrypts only what it sends itself, and MSC4140's scheduled messages must be encrypted
/// before they go (`crate::scheduled`).
#[derive(Clone)]
pub(crate) struct Crypto(pub(crate) matrix_sdk_base::BaseClient);

impl std::fmt::Debug for Crypto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Crypto")
    }
}

/// Where the browser comes back to after an OAuth sign-in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OAuthRedirect {
    /// A server on `127.0.0.1` the engine runs for the sign-in: desktops, where the system
    /// browser can reach the app's own port.
    #[default]
    Loopback,
    /// A URI the app is registered for (a phone's private-use scheme, such as
    /// `com.example.app:/oauth`). The app hands the redirect back with
    /// [`Command::FinishOAuth`](crate::Command::FinishOAuth).
    App(Url),
}

/// How a pending browser sign-in hears that the browser came back.
pub(crate) enum Redirect {
    /// The engine's own loopback server.
    Loopback(LocalServerRedirectHandle),
    /// The app, with the full URL its scheme received.
    App(tokio::sync::oneshot::Receiver<String>),
}

/// The store directory of a sign-in that hasn't saved its session yet: removed when the sign-in
/// fails or is cancelled (its task dropped), kept once [`NewStore::signed_in`] saved an account in it.
pub(crate) struct NewStore(Option<PathBuf>);

impl NewStore {
    /// A fresh, empty store directory.
    pub(crate) fn new(layout: &StoreLayout) -> Result<Self, Error> {
        Ok(Self(Some(layout.new_account_dir()?)))
    }

    /// Its directory.
    pub(crate) fn path(&self) -> PathBuf {
        self.0.clone().unwrap_or_default()
    }

    /// Saves the session `client` signed in, into this store, and keeps the store. The saving
    /// runs on a task of its own: a sign-in cancelled meanwhile still finishes saving (the
    /// account then comes back at the next start), and never loses the store a saved account
    /// points at.
    pub(crate) async fn signed_in(
        mut self,
        client: Client,
        crypto: Crypto,
        sessions: &Sessions,
        holder: Holder,
        passphrase: String,
    ) -> Result<Account, Error> {
        let sessions = sessions.clone();
        let saving = tokio::spawn(async move {
            let account = Account::signed_in(client, crypto, &sessions, holder, self.path(), passphrase).await?;
            // Saved: the store stays.
            self.0.take();
            Ok(account)
        });
        saving.await.map_err(|e| Error::Storage(std::io::Error::other(e.to_string())))?
    }
}

impl Drop for NewStore {
    fn drop(&mut self) {
        let Some(dir) = self.0.take() else { return };
        let remove = move || {
            let _ = std::fs::remove_dir_all(dir);
        };
        // File I/O stays off the engine's workers where there is a runtime to hand it to.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn_blocking(remove)),
            Err(_) => remove(),
        }
    }
}

/// Where a browser sign-in goes: the server, the suspended account it resumes, and whether it
/// creates a new account first.
pub(crate) struct OAuthTarget<'a> {
    pub homeserver: &'a str,
    pub resume: Option<&'a AccountId>,
    pub create: bool,
    pub expected: Option<AccountId>,
}

/// A browser sign-in waiting for the user: the URL to open, and the redirect to wait for.
pub(crate) struct PendingOAuth {
    pub url: Url,
    client: Client,
    crypto: Option<Crypto>,
    expected: Option<PendingAccount>,
    holder: Holder,
    redirect: Redirect,
    store_dir: PathBuf,
    passphrase: String,
}

struct PendingAccount {
    id: AccountId,
    device: Option<matrix_sdk::ruma::OwnedDeviceId>,
    homeserver: String,
}

struct LoginIdentity<'a> {
    account: &'a AccountId,
    device: Option<&'a matrix_sdk::ruma::DeviceId>,
    homeserver: &'a str,
}

/// How every client handles encryption keys:
/// - **Cross-signing** is set up at sign-in when the account has none.
/// - **Key backup** is created when the account has none.
/// - **Room keys** are fetched from the backup when a message can't be decrypted.
///
/// matrix-sdk's default is to never fetch from the backup, which leaves a verified session
/// unable to read history.
const ENCRYPTION: EncryptionSettings = EncryptionSettings {
    auto_enable_cross_signing: true,
    backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
    auto_enable_backups: true,
};

/// How much media each account keeps on disk: enough for the pictures and previews of the rooms
/// in use, never more than a few hundred megabytes. Unused media goes after 30 days.
fn media_cache() -> matrix_sdk::media::MediaRetentionPolicy {
    const MIB: u64 = 1024 * 1024;
    const DAY: Duration = Duration::from_secs(86_400);
    matrix_sdk::media::MediaRetentionPolicy::new()
        .with_max_cache_size(Some(300 * MIB))
        .with_max_file_size(Some(20 * MIB))
        .with_last_access_expiry(Some(30 * DAY))
        .with_cleanup_frequency(Some(DAY))
}

/// Room summaries kept between syncs, so each sync re-reads only the rooms it touched and a
/// list is built only when one of them changed.
#[derive(Debug, Default)]
pub(crate) struct RoomCache {
    rooms: HashMap<String, crate::RoomSummary>,
    spaces: HashMap<String, crate::SpaceSummary>,
    invitations: HashMap<String, crate::Invitation>,
    /// The joined rooms when invitations were last read: joining or leaving one can make an
    /// inviter a stranger or not, so every invitation is read again when they change.
    joined: std::collections::BTreeSet<String>,
    /// Whether a list was built from it yet: the first is, even when empty.
    listed: bool,
}

/// Puts `value` under `id`; true when that changed what `map` held.
fn changed<T: PartialEq>(map: &mut HashMap<String, T>, id: String, value: T) -> bool {
    if map.get(&id) == Some(&value) {
        return false;
    }
    map.insert(id, value);
    true
}

/// Which process opens an account's store: the engine, or a push handled without it (iOS's
/// Notification Service Extension, Android's headless push task). Where both may, each holds
/// the SDK's cross-process locks under its own name, so the two never write the crypto store or
/// refresh tokens at the same time (ADR 0010).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holder {
    /// The engine, where nothing else opens its stores (the desktop): no cross-process locks,
    /// whose leases are written to disk every 50 ms while held, as through each long-poll sync.
    Alone,
    /// The engine, beside pushes handled without it (the phones).
    Engine,
    /// A push, fetching one event.
    Push,
}

impl Holder {
    /// The engine's holder for stores shared as `sharing` says.
    pub(crate) fn engine(sharing: crate::StoreSharing) -> Self {
        match sharing {
            crate::StoreSharing::Alone => Self::Alone,
            crate::StoreSharing::WithPush => Self::Engine,
        }
    }

    /// The name it holds the SDK's cross-process locks under; none when it holds none.
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Alone => None,
            Self::Engine => Some("main"),
            Self::Push => Some("push"),
        }
    }
}

/// The most connections the engine opens to each of an account's databases. The SDK's default
/// is four per physical core (each with its own page cache), far more than one account's work
/// runs at once.
const STORE_CONNECTIONS: usize = 8;

/// How `holder` opens an account's databases: a push, in a phone's memory-capped notification
/// process, with the SDK's low-memory settings; the engine with fewer connections than the SDK's
/// default.
fn store_config(store_dir: &Path, holder: Holder) -> matrix_sdk::SqliteStoreConfig {
    match holder {
        Holder::Push => matrix_sdk::SqliteStoreConfig::with_low_memory_config(store_dir),
        Holder::Alone | Holder::Engine => {
            matrix_sdk::SqliteStoreConfig::new(store_dir).pool_max_size(STORE_CONNECTIONS)
        }
    }
}

/// A client for an already resolved homeserver URL (see [`crate::discovery`]).
///
/// Its tokens are shared with the other process that may open the same store: a refresh
/// happens under a cross-process lock and is saved before the lock is let go, and a client
/// that finds the tokens refreshed elsewhere reads them back from the secret store.
pub(crate) async fn build_client(
    homeserver_url: &str,
    store_dir: &Path,
    passphrase: &str,
    sessions: &Sessions,
    holder: Holder,
) -> Result<(Client, Crypto), Error> {
    build_client_with_encryption(homeserver_url, store_dir, passphrase, sessions, holder, ENCRYPTION).await
}

async fn build_client_with_encryption(
    homeserver_url: &str,
    store_dir: &Path,
    passphrase: &str,
    sessions: &Sessions,
    holder: Holder,
    encryption: EncryptionSettings,
) -> Result<(Client, Crypto), Error> {
    use matrix_sdk::cross_process_lock::CrossProcessLockConfig;
    let locks = holder.name().map_or(CrossProcessLockConfig::SingleProcess, CrossProcessLockConfig::multi_process);
    let base = base_client(&store_config(store_dir, holder).passphrase(Some(passphrase)), &locks).await?;
    let client = crate::tls::sdk(Client::builder())
        .homeserver_url(homeserver_url)
        // The base client the SDK's builder would build from these store settings, built here
        // so the account keeps a handle to it (`Crypto`).
        .base_client(base.clone())
        .cross_process_store_config(locks)
        .with_encryption_settings(encryption)
        .handle_refresh_tokens()
        .build()
        .await?;
    // Only OAuth sessions use it; it takes effect when the session is restored.
    if let Some(name) = holder.name() {
        client.oauth().enable_cross_process_refresh_lock(name.to_owned()).await?;
    }
    let (reload, save) = (sessions.clone(), sessions.clone());
    client
        .set_session_callbacks(
            Box::new(move |client| {
                let user = client.user_id().ok_or("the client has no user")?;
                Ok(reload.tokens(user.as_str())?.ok_or("the account has no saved session")?)
            }),
            // Called inside the refresh's lock: the other process reads the new tokens back.
            Box::new(move |client| {
                let user = client.user_id().ok_or("the client has no user")?;
                if let Some(auth) = StoredAuth::of(&client) {
                    save.update_auth(&AccountId::new(user.as_str()), auth)?;
                }
                Ok(())
            }),
        )
        .map_err(Error::Sdk)?;
    Ok((client, Crypto(base)))
}

/// A store that failed to open.
fn store_failed(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::Storage(std::io::Error::other(error))
}

/// The base client the SDK's builder makes for a SQLite store: the same four databases (the
/// event cache and media in the store's own folder), and its defaults for threading, direct
/// chats, key recipients and decryption trust.
async fn base_client(
    config: &matrix_sdk::SqliteStoreConfig,
    locks: &matrix_sdk::cross_process_lock::CrossProcessLockConfig,
) -> Result<matrix_sdk_base::BaseClient, Error> {
    use matrix_sdk::{SqliteCryptoStore, SqliteEventCacheStore, SqliteMediaStore, SqliteStateStore};
    let (state, events, media, crypto) = tokio::try_join!(
        async { SqliteStateStore::open_with_config(config).await.map_err(store_failed) },
        async { SqliteEventCacheStore::open_with_config(config).await.map_err(store_failed) },
        async { SqliteMediaStore::open_with_config(config).await.map_err(store_failed) },
        async { SqliteCryptoStore::open_with_config(config).await.map_err(store_failed) },
    )?;
    let stores = matrix_sdk_base::store::StoreConfig::new(locks.clone())
        .state_store(state)
        .event_cache_store(events)
        .media_store(media)
        .crypto_store(crypto);
    let mut base = matrix_sdk_base::BaseClient::new(
        stores,
        matrix_sdk_base::ThreadingSupport::Disabled,
        matrix_sdk_base::DmRoomDefinition::MatrixSpec,
    );
    base.room_key_recipient_strategy = matrix_sdk_base::crypto::CollectStrategy::default();
    base.decryption_settings = matrix_sdk_base::crypto::DecryptionSettings {
        sender_device_trust_requirement: matrix_sdk_base::crypto::TrustRequirement::Untrusted,
    };
    Ok(base)
}

impl Account {
    /// A signed-in test account on a real store in `dir`, its crypto at hand, as the engine's
    /// accounts are.
    #[cfg(test)]
    pub(crate) async fn for_test_with_store(homeserver: &str, dir: &Path) -> Self {
        let sessions = Sessions::new(dir, Arc::new(crate::secrets::MemorySecrets::default()));
        let (client, crypto) =
            build_client(homeserver, &dir.join("store"), "passphrase", &sessions, Holder::Alone).await.expect("client");
        let session: matrix_sdk::authentication::matrix::MatrixSession = serde_json::from_value(serde_json::json!({
            "user_id": "@a:example.org", "device_id": "TEST", "access_token": "test-token"
        }))
        .expect("test session");
        client.restore_session(session).await.expect("restore test session");
        Self {
            id: AccountId::new("@a:example.org"),
            client,
            session_changes: Arc::new(Mutex::new(None)),
            rtc: Arc::new(Mutex::new(None)),
            turn: Arc::default(),
            meeting_server: Arc::default(),
            status: Arc::default(),
            status_saves: Arc::default(),
            saved_changes: Arc::default(),
            safety_changes: Arc::default(),
            safety_queue: crate::drafts::DraftQueue::default(),
            admin_changes: crate::drafts::DraftQueue::default(),
            drafts: crate::drafts::DraftQueue::default(),
            room_list: Arc::default(),
            extra_state: Arc::default(),
            pack_edits: Arc::default(),
            followed: Arc::default(),
            subscribing: Arc::default(),
            sticky: crate::calls::StickyMembers::default(),
            organising: crate::drafts::DraftQueue::default(),
            offline_device: Arc::new(tokio::sync::watch::Sender::new(crate::OfflineDevice::Unknown)),
            offline_changes: Arc::default(),
            sessions: Some(sessions),
            scheduling: Arc::default(),
            listing_scheduled: Arc::default(),
            scheduled_here: Arc::default(),
            crypto: Some(Box::new(crypto)),
        }
    }

    #[cfg(test)]
    pub(crate) async fn for_test(homeserver: &str) -> Self {
        let client = Client::builder()
            .homeserver_url(homeserver)
            .server_versions([matrix_sdk::ruma::api::MatrixVersion::V1_12])
            .build()
            .await
            .expect("test client");
        let session: matrix_sdk::authentication::matrix::MatrixSession = serde_json::from_value(serde_json::json!({
            "user_id": "@a:example.org", "device_id": "TEST", "access_token": "test-token"
        }))
        .expect("test session");
        client.restore_session(session).await.expect("restore test session");
        client.event_cache().subscribe().expect("subscribe test cache");
        Self {
            id: AccountId::new("@a:example.org"),
            client,
            session_changes: Arc::new(Mutex::new(None)),
            rtc: Arc::new(Mutex::new(None)),
            turn: Arc::default(),
            meeting_server: Arc::default(),
            status: Arc::default(),
            status_saves: Arc::default(),
            saved_changes: Arc::default(),
            safety_changes: Arc::default(),
            safety_queue: crate::drafts::DraftQueue::default(),
            admin_changes: crate::drafts::DraftQueue::default(),
            drafts: crate::drafts::DraftQueue::default(),
            room_list: Arc::default(),
            extra_state: Arc::default(),
            pack_edits: Arc::default(),
            followed: Arc::default(),
            subscribing: Arc::default(),
            sticky: crate::calls::StickyMembers::default(),
            organising: crate::drafts::DraftQueue::default(),
            offline_device: Arc::new(tokio::sync::watch::Sender::new(crate::OfflineDevice::Unknown)),
            offline_changes: Arc::default(),
            sessions: None,
            scheduling: Arc::default(),
            listing_scheduled: Arc::default(),
            scheduled_here: Arc::default(),
            crypto: None,
        }
    }

    /// Signs in with a password and saves the session.
    pub(crate) async fn login_password(
        layout: &StoreLayout,
        sessions: &Sessions,
        holder: Holder,
        request: &LoginRequest,
        strict: bool,
    ) -> Result<Self, Error> {
        let resume = request.resume.as_ref();
        let (store_dir, passphrase, device) =
            store_for(layout, sessions, resume, &request.homeserver, Some(&request.username)).await?;
        if strict && resume.is_some() && device.is_none() {
            return Err(Error::ResumeStore);
        }
        let expected = strict.then(|| AccountId::new(&request.username));
        let (client, crypto) =
            login_client(&request.homeserver, &store_dir, &passphrase, sessions, holder, strict).await?;
        let mut login = client.matrix_auth().login_username(&request.username, &request.password);
        login = match &device {
            Some(device) => login.device_id(device.as_str()),
            None => login.initial_device_display_name(DEVICE_DISPLAY_NAME),
        };
        login.await?;
        let (client, crypto) = match expected {
            Some(expected) => {
                let identity =
                    LoginIdentity { account: &expected, device: device.as_deref(), homeserver: &request.homeserver };
                checked_login(client, identity, &store_dir, &passphrase, sessions, holder, Some(&request.password))
                    .await?
            }
            None => (client, crypto.ok_or(Error::NoUserId)?),
        };
        Self::signed_in(client, crypto, sessions, holder, store_dir, passphrase).await
    }

    /// Starts a browser sign-in on a resolved homeserver URL: registers this app with the
    /// server's OAuth provider and returns the URL the user must open. [`PendingOAuth::finish`]
    /// completes it.
    pub(crate) async fn start_oauth(
        layout: &StoreLayout,
        sessions: &Sessions,
        holder: Holder,
        identity: &ClientIdentity,
        target: OAuthTarget<'_>,
        app_redirect: Option<(Url, tokio::sync::oneshot::Receiver<String>)>,
    ) -> Result<PendingOAuth, Error> {
        let OAuthTarget { homeserver, resume, create, expected } = target;
        let (store_dir, passphrase, device) = store_for(layout, sessions, resume, homeserver, None).await?;
        if expected.is_some() && resume.is_some() && device.is_none() {
            return Err(Error::ResumeStore);
        }
        let (client, crypto) =
            login_client(homeserver, &store_dir, &passphrase, sessions, holder, expected.is_some()).await?;
        let expected = expected.map(|id| PendingAccount { id, device: device.clone(), homeserver: homeserver.into() });
        let (redirect_uri, redirect) = match app_redirect {
            Some((uri, returned)) => (uri, Redirect::App(returned)),
            None => {
                let (uri, handle) = LocalServerBuilder::new().spawn().await.map_err(Error::Storage)?;
                (uri, Redirect::Loopback(handle))
            }
        };
        let mut metadata = ClientMetadata::new(
            ApplicationType::Native,
            vec![OAuthGrantType::AuthorizationCode { redirect_uris: vec![redirect_uri.clone()] }],
            Localized::new(identity.uri.clone(), []),
        );
        metadata.client_name = Some(Localized::new(identity.name.clone(), []));
        let metadata = Raw::new(&metadata).map_err(|e| Error::Storage(e.into()))?;
        let mut login = client.oauth().login(redirect_uri, device, Some(metadata.into()), None);
        if create {
            use matrix_sdk::ruma::api::client::discovery::get_authorization_server_metadata::v1::Prompt;
            login = login.prompt(vec![Prompt::Create]);
        }
        let data = login.build().await?;
        Ok(PendingOAuth { url: data.url, client, crypto, expected, holder, redirect, store_dir, passphrase })
    }

    /// Resumes a saved session.
    pub(crate) async fn restore(sessions: &Sessions, holder: Holder, stored: &StoredAccount) -> Result<Self, Error> {
        let id = stored.id.clone();
        let secrets = blocking(sessions, move |s| s.secrets(&id)).await?;
        let secrets = secrets.ok_or_else(|| Error::UnknownAccount(AccountId::new(&stored.id)))?;
        let (client, crypto) =
            build_client(&stored.homeserver, &stored.store_dir, &secrets.passphrase, sessions, holder).await?;
        let changes = client.subscribe_to_session_changes();
        client.restore_session(secrets.auth.into_session()).await?;
        Self::ready(AccountId::new(&stored.id), client, crypto, holder, changes, sessions).await
    }

    pub(crate) async fn signed_in(
        client: Client,
        crypto: Crypto,
        sessions: &Sessions,
        holder: Holder,
        store_dir: PathBuf,
        passphrase: String,
    ) -> Result<Self, Error> {
        let changes = client.subscribe_to_session_changes();
        let id = AccountId::new(client.user_id().ok_or(Error::NoUserId)?.as_str());
        let auth = StoredAuth::of(&client).ok_or(Error::NoUserId)?;
        let homeserver = client.homeserver().to_string();
        let stored = StoredAccount { id: id.to_string(), homeserver, store_dir, signed_out: false };
        blocking(sessions, move |s| s.save(stored, &StoredSecrets { auth, passphrase })).await?;
        Self::ready(id, client, crypto, holder, changes, sessions).await
    }

    /// Turns on what every signed-in account runs: the event cache, which keeps room history on
    /// disk and fills gaps from time offline, and the send queue, which sends what was written
    /// offline (or in an earlier session) once the server is reachable.
    async fn ready(
        id: AccountId,
        client: Client,
        crypto: Crypto,
        holder: Holder,
        changes: broadcast::Receiver<SessionChange>,
        sessions: &Sessions,
    ) -> Result<Self, Error> {
        // A push handled without the engine may write the crypto store (ADR 0010): with the
        // lock, each sync waits for a push to finish and reloads what it changed (`sync_once_locked`).
        if let Some(name) = holder.name()
            && let Err(error) = client.encryption().enable_cross_process_store_lock(name.to_owned()).await
        {
            eprintln!("matrix-core: could not share the crypto store with pushes: {error}");
        }
        client.event_cache().subscribe()?;
        client.send_queue().set_enabled(true).await;
        // A bounded media cache: previews and pictures load from disk, without filling it.
        if let Err(error) = client.media().set_media_retention_policy(media_cache()).await {
            eprintln!("matrix-core: could not limit the media cache: {error}");
        }
        Ok(Self {
            id,
            client,
            session_changes: Arc::new(Mutex::new(Some(changes))),
            rtc: Arc::default(),
            turn: Arc::default(),
            meeting_server: Arc::default(),
            status: Arc::default(),
            status_saves: Arc::default(),
            saved_changes: Arc::default(),
            safety_changes: Arc::default(),
            safety_queue: crate::drafts::DraftQueue::default(),
            admin_changes: crate::drafts::DraftQueue::default(),
            drafts: crate::drafts::DraftQueue::default(),
            room_list: Arc::default(),
            extra_state: Arc::default(),
            pack_edits: Arc::default(),
            followed: Arc::default(),
            subscribing: Arc::default(),
            sticky: crate::calls::StickyMembers::default(),
            organising: crate::drafts::DraftQueue::default(),
            offline_device: Arc::new(tokio::sync::watch::Sender::new(crate::OfflineDevice::Unknown)),
            offline_changes: Arc::default(),
            sessions: Some(sessions.clone()),
            scheduling: Arc::default(),
            listing_scheduled: Arc::default(),
            scheduled_here: Arc::default(),
            crypto: Some(Box::new(crypto)),
        })
    }

    /// The account's token changes: the subscription made before its first request, the first
    /// time; a fresh one after.
    pub(crate) fn session_changes(&self) -> broadcast::Receiver<SessionChange> {
        let early = self.session_changes.lock().ok().and_then(|mut early| early.take());
        early.unwrap_or_else(|| self.client.subscribe_to_session_changes())
    }

    /// One sync, holding the crypto store's cross-process lock from the request until the SDK
    /// has processed the response: a push handled outside the engine (the iOS extension, a
    /// headless Android fetch) never writes the store at the same time, and taking the lock
    /// reloads whatever such a push wrote meanwhile. When the lock can't be had in time the sync
    /// fails, and the caller's retry tries again; it never proceeds unprotected (ADR 0010).
    pub(crate) async fn sync_once_locked(
        &self,
        settings: matrix_sdk::config::SyncSettings,
    ) -> Result<matrix_sdk::sync::SyncResponse, matrix_sdk::Error> {
        let _store = self.client.encryption().spin_lock_store(None).await?;
        self.client.sync_once(settings).await
    }

    /// Names the room list's sliding sync while it runs, `None` once it stopped. A new one asks
    /// at once for the rooms open timelines follow.
    pub(crate) async fn set_room_list(&self, service: Option<Arc<matrix_sdk_ui::RoomListService>>) {
        if let Ok(mut room_list) = self.room_list.lock() {
            room_list.clone_from(&service);
        }
        if service.is_some() {
            self.subscribe_followed().await;
        }
    }

    /// Whether the room list's sliding sync runs: rooms then hold only the state it asks for.
    pub(crate) fn sliding(&self) -> bool {
        self.room_list.lock().is_ok_and(|room_list| room_list.is_some())
    }

    /// Asks the room list's sliding sync for a room's recent events and the state it keeps while
    /// it is open, so its timeline gets every event rather than the list's last few.
    /// Nothing more to do over the long-poll sync, which brings every room whole.
    pub(crate) async fn follow_room(&self, room_id: &str) {
        if let Ok(mut followed) = self.followed.lock() {
            followed.insert(room_id.to_owned());
        }
        self.subscribe_followed().await;
    }

    /// Stops following a room at once (its last timeline closed), then lets the room list's
    /// sliding sync know.
    pub(crate) fn unfollow_room(&self, room_id: &str) {
        if let Ok(mut followed) = self.followed.lock() {
            followed.remove(room_id);
        }
        let account = self.clone();
        tokio::spawn(async move { account.subscribe_followed().await });
    }

    /// The rooms open timelines show.
    pub(crate) fn followed_rooms(&self) -> Vec<String> {
        self.followed.lock().map(|followed| followed.iter().cloned().collect()).unwrap_or_default()
    }

    /// Subscribes the room list's sliding sync to the rooms followed now, and only those: the
    /// SDK replaces its whole set of subscriptions at once. One update at a time, each reading
    /// the set when its turn comes, so the newest set is the one applied last.
    async fn subscribe_followed(&self) {
        let _turn = self.subscribing.lock().await;
        let service = self.room_list.lock().ok().and_then(|room_list| room_list.clone());
        let Some(service) = service else { return };
        let followed: Vec<matrix_sdk::ruma::OwnedRoomId> = self
            .followed
            .lock()
            .map(|followed| followed.iter().filter_map(|id| matrix_sdk::ruma::RoomId::parse(id).ok()).collect())
            .unwrap_or_default();
        let rooms: Vec<&matrix_sdk::ruma::RoomId> = followed.iter().map(AsRef::as_ref).collect();
        service.set_room_subscriptions(&rooms).await;
    }

    /// Registers (or updates) this device's pusher: the homeserver sends the account's pushes
    /// to the gateway, as event and room IDs only.
    pub async fn register_pusher(&self, pusher: &crate::model::Pusher) -> Result<(), Error> {
        let pusher = crate::push::pusher(&self.id, pusher)?;
        // `append`: the device's token may be registered for another account on the same server,
        // whose pusher must stay (without it the server drops pushers of other users with it).
        Ok(self.client.pusher().set(pusher, true).await?)
    }

    /// Removes this device's pusher from the account.
    pub async fn remove_pusher(&self, app_id: &str, push_key: &str) -> Result<(), Error> {
        let ids = matrix_sdk::ruma::api::client::push::PusherIds::new(push_key.to_owned(), app_id.to_owned());
        Ok(self.client.pusher().delete(ids).await?)
    }

    /// Which account this is.
    pub fn id(&self) -> &AccountId {
        &self.id
    }

    /// The underlying SDK client, for engine tasks.
    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    /// The LiveKit service the homeserver names for calls, if it named one yet.
    pub(crate) fn rtc_service(&self) -> Option<String> {
        self.rtc.lock().ok().and_then(|rtc| rtc.clone())
    }

    /// What the homeserver offers for calls, as far as it said yet.
    pub(crate) fn call_services(&self) -> crate::convert::CallServices {
        crate::convert::CallServices {
            rtc: self.rtc_service().is_some(),
            turn: self.turn.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Looks up whether the homeserver names TURN servers, without which one-to-one calls aren't
    /// offered. True when it names some. A server that refuses (none configured) names none; an
    /// unreachable one is asked again until it answers, as [`Account::discover_rtc`] does.
    #[cfg(feature = "native-media")]
    pub(crate) async fn discover_turn(&self) -> bool {
        use matrix_sdk::ruma::api::client::voip::get_turn_server_info;
        let mut wait = std::time::Duration::from_secs(2);
        let found = loop {
            match self.client.send(get_turn_server_info::v3::Request::new()).await {
                Ok(turn) => break !turn.uris.is_empty(),
                // Only the network failing is asked again: a refusal, or Synapse's empty answer
                // when it has no TURN servers (which doesn't read as one), names none.
                Err(matrix_sdk::HttpError::Reqwest(_)) => {
                    tokio::time::sleep(wait).await;
                    wait = (wait * 2).min(std::time::Duration::from_secs(60));
                }
                Err(_) => break false,
            }
        };
        self.turn.store(found, std::sync::atomic::Ordering::Relaxed);
        found
    }

    /// The Jitsi server the homeserver names for meetings, if it named one yet.
    pub(crate) fn meeting_server(&self) -> Option<String> {
        self.meeting_server.lock().ok().and_then(|server| server.clone())
    }

    /// Looks up the Jitsi server the homeserver names for meetings (its well-known file). True
    /// when it names one.
    pub(crate) async fn discover_meetings(&self) -> bool {
        let http = self.client.http_client().clone();
        let Ok(me) = self.user_id() else { return false };
        let found = crate::widgets::meetings::named_server(&http, me.server_name().as_str()).await;
        let named = found.is_some();
        if let Ok(mut server) = self.meeting_server.lock() {
            *server = found;
        }
        named
    }

    /// Looks up the homeserver's call service (its RTC transports, else its well-known file).
    /// True when it names one.
    #[cfg(feature = "native-media")]
    pub(crate) async fn discover_rtc(&self) -> bool {
        use matrix_sdk::ruma::api::client::rtc::RtcTransport;
        // An unreachable server is asked again until it answers, waiting longer each time, up
        // to a minute (an app started offline gets calls once it's online).
        let mut wait = std::time::Duration::from_secs(2);
        let transports = loop {
            match self.client.discover_rtc_transports().await {
                Ok(found) => break found.unwrap_or_default(),
                Err(_) => {
                    tokio::time::sleep(wait).await;
                    wait = (wait * 2).min(std::time::Duration::from_secs(60));
                }
            }
        };
        let service = transports.into_iter().find_map(|transport| match transport {
            RtcTransport::LiveKit(livekit) => Some(livekit.service_url),
            _ => None,
        });
        let found = service.is_some();
        if let Ok(mut rtc) = self.rtc.lock() {
            *rtc = service;
        }
        found
    }

    /// The account as the account switcher and settings show it, from the server.
    pub async fn summary(&self) -> AccountSummary {
        let display_name = self.client.account().get_display_name().await.ok().flatten();
        let avatar = self.client.account().get_avatar_url().await.ok().flatten();
        self.summary_with(display_name, avatar).await
    }

    /// [`Account::summary`], keeping `cached`'s name or picture when the server can't be asked
    /// for it: offline, the account goes on looking as it did.
    pub async fn refreshed_summary(&self, cached: &AccountSummary) -> AccountSummary {
        let account = self.client.account();
        let (name, avatar) = (account.get_display_name().await, account.get_avatar_url().await);
        let mut summary =
            self.summary_with(name.as_ref().ok().cloned().flatten(), avatar.as_ref().ok().cloned().flatten()).await;
        if name.is_err() {
            summary.display_name.clone_from(&cached.display_name);
        }
        if avatar.is_err() {
            summary.avatar.clone_from(&cached.avatar);
        }
        summary
    }

    /// [`Account::summary`] from what this device stored: no request, so it is there at once
    /// even offline. The name comes from the user's membership in a joined room.
    pub async fn cached_summary(&self) -> AccountSummary {
        let mut display_name = None;
        if let Some(me) = self.client.user_id() {
            for room in self.client.joined_rooms() {
                if let Ok(Some(member)) = room.get_member_no_sync(me).await
                    && let Some(name) = member.display_name()
                {
                    display_name = Some(name.to_owned());
                    break;
                }
            }
        }
        let avatar = self.client.account().get_cached_avatar_url().await.ok().flatten();
        self.summary_with(display_name, avatar).await
    }

    async fn summary_with(
        &self,
        display_name: Option<String>,
        avatar: Option<matrix_sdk::ruma::OwnedMxcUri>,
    ) -> AccountSummary {
        let default = self
            .client
            .notification_settings()
            .await
            .get_default_room_notification_mode(IsEncrypted::No, IsOneToOne::No)
            .await;
        AccountSummary {
            id: self.id.clone(),
            display_name: display_name.unwrap_or_else(|| self.id.to_string()),
            homeserver: self.client.homeserver().host_str().unwrap_or_default().to_owned(),
            default_notifications: convert::notification_mode(default),
            avatar: avatar.and_then(|url| MediaRef::of(&MediaSource::Plain(url))),
            sign_in: if self.client.oauth().client_id().is_some() {
                SignInMethod::OAuth
            } else {
                SignInMethod::Password
            },
        }
    }

    /// Every joined room and space, each sorted by name, re-reading only the rooms in
    /// `changed` (all of them when `None`, or when the cache doesn't know a room yet).
    /// Re-reads the rooms in `changed` (all when `None`) into `cache`, and returns the list when
    /// anything in it changed: most syncs change no room's summary.
    pub(crate) async fn update_room_list(
        &self,
        cache: &mut RoomCache,
        changed: Option<&HashSet<String>>,
    ) -> Option<RoomList> {
        let joined = self.client.joined_rooms();
        let ids: HashSet<String> = joined.iter().map(|room| room.room_id().to_string()).collect();
        // Rooms left: the SDK stops working out their latest event.
        let left: Vec<String> = cache.rooms.keys().filter(|id| !ids.contains(*id)).cloned().collect();
        for id in left {
            if let Ok(room_id) = matrix_sdk::ruma::RoomId::parse(&id) {
                self.client.latest_events().await.forget_room(&room_id).await;
            }
        }
        let held = cache.rooms.len() + cache.spaces.len();
        cache.rooms.retain(|id, _| ids.contains(id));
        cache.spaces.retain(|id, _| ids.contains(id));
        let mut moved = !std::mem::replace(&mut cache.listed, true) || cache.rooms.len() + cache.spaces.len() != held;
        let stale = |id: &str, cache: &RoomCache| {
            changed.is_none_or(|changed| changed.contains(id))
                || (!cache.rooms.contains_key(id) && !cache.spaces.contains_key(id))
        };
        let (mut settings, mut note_to_self) = (None, None);
        for room in &joined {
            let id = room.room_id().to_string();
            if !stale(&id, cache) {
                continue;
            }
            if room.is_space() {
                moved |= self::changed(&mut cache.spaces, id, convert::space_summary(room).await);
            } else {
                // The SDK keeps each room's latest event (and stores it) once asked to: the
                // list's order comes from it. Asking again is a no-op.
                if !cache.rooms.contains_key(&id) {
                    let _ = self.client.latest_events().await.listen_to_room(room.room_id()).await;
                }
                // The push rules and the note to self's mark are read once per update, and only
                // when a room needs them.
                if settings.is_none() {
                    settings = Some(self.client.notification_settings().await);
                    note_to_self = self.marked_note_to_self().await;
                }
                if let Some(settings) = &settings {
                    let (services, marked) = (self.call_services(), note_to_self.as_deref());
                    let meetings = self.meeting_server().is_some();
                    let summary = convert::room_summary(room, settings, marked, services, meetings, &self.sticky).await;
                    moved |= self::changed(&mut cache.rooms, id, summary);
                }
            }
        }
        let invited = self.client.invited_rooms();
        let held = cache.invitations.len();
        cache.invitations.retain(|id, _| invited.iter().any(|room| room.room_id().as_str() == id));
        moved |= cache.invitations.len() != held;
        let joined: std::collections::BTreeSet<String> =
            self.client.joined_rooms().iter().map(|room| room.room_id().to_string()).collect();
        let rejudge = joined != cache.joined;
        cache.joined = joined;
        for room in invited {
            let id = room.room_id().to_string();
            if !rejudge && changed.is_some_and(|changed| !changed.contains(&id)) && cache.invitations.contains_key(&id)
            {
                continue;
            }
            let details = room.invite_details().await.ok();
            let stranger = match details.as_ref() {
                Some(invite) => !crate::safety::shares_room(&self.client, &invite.inviter_id).await,
                None => true,
            };
            let inviter = details.map(|invite| crate::Person {
                id: invite.inviter_id.to_string(),
                name: invite
                    .inviter
                    .as_ref()
                    .and_then(|member| member.display_name())
                    .unwrap_or(invite.inviter_id.as_str())
                    .to_owned(),
                avatar: invite
                    .inviter
                    .as_ref()
                    .and_then(|member| member.avatar_url())
                    .map(ToOwned::to_owned)
                    .and_then(convert::avatar),
            });
            let invitation = crate::Invitation {
                room_id: id.clone(),
                name: room.display_name().await.map_or_else(|_| room.room_id().to_string(), |name| name.to_string()),
                topic: room.topic(),
                avatar: room.avatar_url().and_then(convert::avatar),
                inviter,
                space: room.is_space(),
                kind: if room.is_direct().await.unwrap_or(false) {
                    crate::RoomKind::Direct
                } else {
                    crate::RoomKind::Group
                },
                stranger,
            };
            moved |= self::changed(&mut cache.invitations, id, invitation);
        }
        if !moved {
            return None;
        }
        let mut list = RoomList {
            rooms: cache.rooms.values().cloned().collect(),
            spaces: cache.spaces.values().cloned().collect(),
            invitations: cache.invitations.values().cloned().collect(),
        };
        list.rooms.sort_by_cached_key(|room| room.name.to_lowercase());
        list.spaces.sort_by_cached_key(|space| space.name.to_lowercase());
        list.invitations.sort_by_cached_key(|invite| invite.name.to_lowercase());
        Some(list)
    }

    /// Queues a message for a joined room. The send queue persists it and retries until the
    /// server takes it; the room's timeline shows it at once as pending.
    pub async fn send(&self, room_id: &str, message: &Outgoing) -> Result<(), Error> {
        let room = self.joined_room(room_id)?;
        let queue = room.send_queue();
        match message {
            Outgoing::Text { .. }
            | Outgoing::Emote { .. }
            | Outgoing::Location { .. }
            | Outgoing::Poll { .. }
            | Outgoing::Sticker(_)
            | Outgoing::Gif(_) => {
                if let Some(content) = crate::outgoing::event(message)? {
                    queue.send(content).await?;
                }
            }
            Outgoing::File(upload) | Outgoing::Voice { upload, .. } => {
                let data = match upload {
                    Upload::Path(path) => tokio::fs::read(path)
                        .await
                        .map_err(|source| Error::ReadFile { path: path.to_owned(), source })?,
                    Upload::Bytes { bytes, .. } => bytes.to_vec(),
                };
                let mut config = AttachmentConfig::new();
                if let Some(info) = voice_info(message) {
                    config = config.info(info);
                }
                queue.send_attachment(upload.name(), upload.mime(), data, config).await?;
            }
        }
        Ok(())
    }

    /// A live timeline: a room's, without thread replies, or one thread's.
    pub(crate) async fn timeline(&self, key: &TimelineKey) -> Result<matrix_sdk_ui::timeline::Timeline, Error> {
        use matrix_sdk_ui::timeline::{RoomExt, TimelineFocus, TimelineReadReceiptTracking};
        let focus = match &key.thread {
            None => TimelineFocus::Live { hide_threaded_events: true },
            Some(root) => TimelineFocus::Thread { root_event_id: matrix_sdk::ruma::EventId::parse(root)? },
        };
        let mut builder = self.joined_room(&key.room_id)?.timeline_builder().with_focus(focus);
        // Read positions show in a room's timeline only: tracked in a thread, every receipt in
        // the room would update its items for nothing.
        if key.thread.is_none() {
            builder = builder.track_read_marker_and_receipts(TimelineReadReceiptTracking::MessageLikeEvents);
        }
        Ok(builder.build().await?)
    }

    /// Fetches a cold thread's first replies immediately, independently of the SDK's wait
    /// for a sync pagination token. The SDK remains authoritative as it catches up.
    pub(crate) async fn thread_first_page(&self, key: &TimelineKey) -> Result<Vec<crate::TimelineEntry>, Error> {
        use matrix_sdk::room::{IncludeRelations, RelationsOptions};
        let Some(root) = &key.thread else { return Ok(Vec::new()) };
        let root = matrix_sdk::ruma::EventId::parse(root)?;
        let room = self.joined_room(&key.room_id)?;
        let options = RelationsOptions {
            limit: Some(crate::HISTORY_PAGE.into()),
            include_relations: IncludeRelations::RelationsOfType(
                matrix_sdk::ruma::events::relation::RelationType::Thread,
            ),
            ..Default::default()
        };
        let (root_event, replies) =
            tokio::join!(room.load_or_fetch_event(&root, None), room.relations(root.clone(), options));
        let replies = replies?;
        let complete = replies.next_batch_token.is_none();
        let mut events = replies.chunk;
        // Relations default to backwards topological order. Sender clocks need not agree.
        events.reverse();
        if let (true, Ok(root)) = (complete, root_event) {
            events.insert(0, root);
        }
        let messages = convert::messages(&room, &events).await;
        Ok(messages
            .into_iter()
            .map(|message| crate::TimelineEntry {
                id: format!("first-{}", message.event_id),
                kind: crate::EntryKind::Message(Box::new(message)),
            })
            .collect())
    }

    /// The account's user.
    pub(crate) fn user_id(&self) -> Result<matrix_sdk::ruma::OwnedUserId, Error> {
        Ok(matrix_sdk::ruma::UserId::parse(self.id.as_str())?)
    }

    /// Downloads media, decrypting it if needed. Cached by the SDK on disk.
    pub async fn fetch_media(&self, media: &MediaRef, size: MediaSize) -> Result<Vec<u8>, Error> {
        let request =
            MediaRequestParameters { source: convert::media_source(media)?, format: convert::media_format(size) };
        Ok(self.client.media().get_media_content(&request, true).await?)
    }

    /// Sets when a room notifies, as push rules on the server; `None` makes it follow the
    /// account's default again.
    pub async fn set_notification_mode(&self, room_id: &str, mode: Option<NotificationMode>) -> Result<(), Error> {
        let room_id = RoomId::parse(room_id)?;
        let settings = self.client.notification_settings().await;
        match mode {
            Some(mode) => settings.set_room_notification_mode(&room_id, convert::room_notification_mode(mode)).await?,
            None => settings.delete_user_defined_room_rules(&room_id).await?,
        }
        Ok(())
    }

    /// Sets when rooms notify unless they set their own mode: for every kind of room, encrypted
    /// or not, direct or group.
    pub async fn set_default_notification_mode(&self, mode: NotificationMode) -> Result<(), Error> {
        let settings = self.client.notification_settings().await;
        let mode = convert::room_notification_mode(mode);
        for encrypted in [IsEncrypted::No, IsEncrypted::Yes] {
            for one_to_one in [IsOneToOne::No, IsOneToOne::Yes] {
                settings.set_default_room_notification_mode(encrypted, one_to_one, mode).await?;
            }
        }
        Ok(())
    }

    /// The words that notify the user wherever they are written (push rules matching message
    /// content), as this device last synced them, sorted.
    pub async fn keywords(&self) -> Vec<String> {
        let rules = self.client.account().push_rules().await.unwrap_or_default();
        keywords_in(&rules)
    }

    /// The keywords as the server has them now.
    pub async fn server_keywords(&self) -> Result<Vec<String>, Error> {
        use matrix_sdk::ruma::api::client::push::get_pushrules_global_scope;
        let rules = self.client.send(get_pushrules_global_scope::v3::Request::new()).await?.global;
        Ok(keywords_in(&rules))
    }

    /// Adds a notification keyword (trimmed), or removes one as listed (a rule another client made
    /// may keep spaces around its word), and returns the keywords as they are then. Both work from
    /// the server's rules as they are now rather than this device's copy, which a sync begun
    /// before an earlier change may have left behind. Blank keywords are left alone.
    pub async fn set_keyword(&self, keyword: &str, enabled: bool) -> Result<Vec<String>, Error> {
        use matrix_sdk::ruma::api::client::push::{
            delete_pushrule, get_pushrules_global_scope, set_pushrule, set_pushrule_enabled,
        };
        use matrix_sdk::ruma::push::{
            Action, HighlightTweakValue, NewPatternedPushRule, NewPushRule, RuleKind, SoundTweakValue, Tweak,
        };
        let mut rules = self.client.send(get_pushrules_global_scope::v3::Request::new()).await?.global;
        let keyword = if enabled { keyword.trim() } else { keyword };
        if keyword.trim().is_empty() {
            return Ok(keywords_in(&rules));
        }
        let matching: Vec<(String, bool)> = rules
            .content
            .iter()
            .filter(|rule| notifies(rule) && rule.pattern == keyword)
            .map(|rule| (rule.rule_id.clone(), rule.enabled))
            .collect();
        if !enabled {
            for (rule_id, _) in &matching {
                self.client.send(delete_pushrule::v3::Request::new(RuleKind::Content, rule_id.clone())).await?;
            }
            rules.content.retain(|rule| !(notifies(rule) && rule.pattern == keyword));
            return Ok(keywords_in(&rules));
        }
        if matching.iter().any(|(_, on)| *on) {
            // There already.
        } else if let Some((rule_id, _)) = matching.first() {
            self.client.send(set_pushrule_enabled::v3::Request::enable(RuleKind::Content, rule_id.clone())).await?;
        } else {
            // Notifying with the default sound, as the SDK makes them, and highlighted, so a match
            // counts like a mention (also where only mentions draw attention); named after the
            // word when that is a plain, unused name, else under a name of the app's own.
            let actions = vec![
                Action::Notify,
                Action::SetTweak(Tweak::Sound(SoundTweakValue::Default)),
                Action::SetTweak(Tweak::Highlight(HighlightTweakValue::Yes)),
            ];
            let taken = |id: &str| rules.content.iter().any(|rule| rule.rule_id == id);
            let plain = keyword.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
            let rule_id = if plain && !taken(keyword) {
                keyword.to_owned()
            } else {
                // Random, so two sessions adding words at once don't pick the same name.
                format!("chat.bettermatrix.keyword.{}", crate::random_hex(8)?)
            };
            let rule = NewPushRule::Content(NewPatternedPushRule::new(rule_id, keyword.to_owned(), actions));
            self.client.send(set_pushrule::v3::Request::new(rule)).await?;
        }
        Ok(with_keyword(keywords_in(&rules), keyword))
    }

    /// Puts a room among the favourites or the low priority rooms, or takes it out of both
    /// (room tags). Each tag is set or removed whatever this device last synced, so a change
    /// right after another one still reaches the server.
    pub async fn set_room_priority(&self, room_id: &str, priority: RoomPriority) -> Result<(), Error> {
        use matrix_sdk::ruma::events::tag::{TagInfo, TagName};
        let room = self.joined_room(room_id)?;
        let (set, remove): (Option<TagName>, &[TagName]) = match priority {
            RoomPriority::Favourite => (Some(TagName::Favorite), &[TagName::LowPriority]),
            RoomPriority::Low => (Some(TagName::LowPriority), &[TagName::Favorite]),
            RoomPriority::Normal => (None, &[TagName::Favorite, TagName::LowPriority]),
        };
        if let Some(tag) = set {
            room.set_tag(tag, TagInfo::new()).await?;
        }
        for tag in remove {
            room.remove_tag(tag.clone()).await?;
        }
        Ok(())
    }

    /// Marks a room unread (MSC2867's flag), or takes the mark away, whatever this device last
    /// synced. Reads nothing: [`Account::mark_room_read`] does.
    pub async fn set_unread(&self, room_id: &str, unread: bool) -> Result<(), Error> {
        use matrix_sdk::ruma::events::marked_unread::MarkedUnreadEventContent;
        self.joined_room(room_id)?.set_account_data(MarkedUnreadEventContent::new(unread)).await?;
        Ok(())
    }

    /// Reads a room without opening it: its read receipt and fully-read marker moved to the latest
    /// message the SDK knows, when there are unread ones, and the unread mark taken away. Fails when there are unread
    /// messages but no message to mark read yet.
    pub async fn mark_room_read(&self, room_id: &str) -> Result<(), Error> {
        use matrix_sdk::ruma::events::marked_unread::MarkedUnreadEventContent;
        let room = self.joined_room(room_id)?;
        if room.num_unread_messages() > 0 || room.num_unread_mentions() > 0 {
            let matrix_sdk_base::latest_event::LatestEventValue::Remote(event) = room.latest_event() else {
                return Err(Error::NothingToRead);
            };
            let event_id = event.event_id().ok_or(Error::NothingToRead)?.to_owned();
            let receipts =
                matrix_sdk::room::Receipts::new().fully_read_marker(event_id.clone()).public_read_receipt(event_id);
            room.send_multiple_receipts(receipts).await?;
        }
        room.set_account_data(MarkedUnreadEventContent::new(false)).await?;
        Ok(())
    }

    /// Leaves a room. With `forget`, also asks the server to forget it, so it disappears from
    /// every client's history list (deleting a direct chat).
    pub async fn leave_room(&self, room_id: &str, forget: bool) -> Result<(), Error> {
        let room = self.joined_room(room_id)?;
        room.leave().await?;
        if forget {
            room.forget().await?;
        }
        Ok(())
    }

    /// Signs out on the server and forgets the account here. A session the server already ended
    /// is only forgotten.
    pub(crate) async fn logout(&self, sessions: &Sessions) -> Result<(), Error> {
        match self.client.logout().await {
            Err(error) if !session_ended(&error) => return Err(error.into()),
            _ => {}
        }
        let id = self.id.clone();
        blocking(sessions, move |s| s.remove(&id)).await
    }

    /// Tells a room whether the user is typing in it.
    pub(crate) async fn typing(&self, room_id: &str, typing: bool) -> Result<(), Error> {
        Ok(self.joined_room(room_id)?.typing_notice(typing).await?)
    }

    pub(crate) fn joined_room(&self, room_id: &str) -> Result<Room, Error> {
        let parsed = RoomId::parse(room_id)?;
        self.client
            .get_room(&parsed)
            .ok_or_else(|| Error::UnknownRoom { account: self.id.clone(), room_id: room_id.to_owned() })
    }
}

/// The keywords a ruleset holds: its enabled content rules the user made, by their word, sorted.
fn keywords_in(rules: &matrix_sdk::ruma::push::Ruleset) -> Vec<String> {
    let enabled = rules.content.iter().filter(|rule| notifies(rule) && rule.enabled);
    with_keyword(enabled.map(|rule| rule.pattern.clone()).collect(), "")
}

/// Whether a content rule is one of the user's keywords: one they made that notifies. One that
/// silences a word is not.
fn notifies(rule: &matrix_sdk::ruma::push::PatternedPushRule) -> bool {
    !rule.default && rule.actions.iter().any(|action| matches!(action, matrix_sdk::ruma::push::Action::Notify))
}

/// `keywords` with `keyword` too (a blank one adds nothing), once each, sorted.
fn with_keyword(mut keywords: Vec<String>, keyword: &str) -> Vec<String> {
    if !keyword.is_empty() {
        keywords.push(keyword.to_owned());
    }
    keywords.sort_by_cached_key(|keyword| (keyword.to_lowercase(), keyword.clone()));
    keywords.dedup();
    keywords
}

impl PendingOAuth {
    /// Waits for the browser to come back, then finishes signing in and saves the session.
    pub(crate) async fn finish(self, sessions: &Sessions) -> Result<Account, Error> {
        let timed_out = |_| Error::BrowserLogin("timed out waiting for the browser".into());
        let abandoned = || Error::BrowserLogin("the browser sign-in was abandoned".into());
        let returned = match self.redirect {
            Redirect::Loopback(handle) => {
                let query = tokio::time::timeout(BROWSER_LOGIN_TIMEOUT, handle).await.map_err(timed_out)?;
                matrix_sdk::utils::UrlOrQuery::Query(query.ok_or_else(abandoned)?.0)
            }
            Redirect::App(returned) => {
                let url = tokio::time::timeout(BROWSER_LOGIN_TIMEOUT, returned).await.map_err(timed_out)?;
                let url = url.map_err(|_| abandoned())?;
                matrix_sdk::utils::UrlOrQuery::Url(Url::parse(&url).map_err(|e| Error::BrowserLogin(e.to_string()))?)
            }
        };
        self.client.oauth().finish_login(returned).await?;
        let (client, crypto) = match self.expected {
            Some(expected) => {
                let identity = LoginIdentity {
                    account: &expected.id,
                    device: expected.device.as_deref(),
                    homeserver: &expected.homeserver,
                };
                checked_login(self.client, identity, &self.store_dir, &self.passphrase, sessions, self.holder, None)
                    .await?
            }
            None => (self.client, self.crypto.ok_or(Error::NoUserId)?),
        };
        Account::signed_in(client, crypto, sessions, self.holder, self.store_dir, self.passphrase).await
    }
}

/// Authenticate explicitly selected accounts in memory before opening a persistent crypto store.
async fn login_client(
    homeserver: &str,
    store_dir: &Path,
    passphrase: &str,
    sessions: &Sessions,
    holder: Holder,
    strict: bool,
) -> Result<(Client, Option<Crypto>), Error> {
    if strict {
        let client = crate::tls::sdk(Client::builder()).homeserver_url(homeserver).build().await?;
        Ok((client, None))
    } else {
        let (client, crypto) = build_client(homeserver, store_dir, passphrase, sessions, holder).await?;
        Ok((client, Some(crypto)))
    }
}

fn check_login_identity(
    client: &Client,
    expected: &AccountId,
    device: Option<&matrix_sdk::ruma::DeviceId>,
) -> Result<(), Error> {
    let actual = client.user_id().ok_or(Error::NoUserId)?;
    if actual.as_str() != expected.as_str() {
        return Err(Error::UnexpectedAccount { expected: expected.clone(), actual: actual.to_string() });
    }
    if let Some(device) = device
        && client.device_id() != Some(device)
    {
        return Err(Error::UnexpectedDevice);
    }
    Ok(())
}

async fn checked_login(
    temporary: Client,
    identity: LoginIdentity<'_>,
    store_dir: &Path,
    passphrase: &str,
    sessions: &Sessions,
    holder: Holder,
    password: Option<&str>,
) -> Result<(Client, Crypto), Error> {
    let LoginIdentity { account, device, homeserver } = identity;
    check_login_identity(&temporary, account, device)?;
    let resolved = temporary.homeserver();
    if device.is_some() && !same_server(resolved.as_str(), homeserver) {
        return Err(Error::ResumeStore);
    }
    let session = temporary.session().ok_or(Error::NoUserId)?;
    if let (matrix_sdk::AuthSession::OAuth(_), Some(device)) = (&session, device) {
        // The SDK tags this session with the device it requested in the scope. Check the
        // server's actual device as well before opening that device's crypto store.
        let who = temporary.whoami().await?;
        if who.device_id.as_deref() != Some(device) {
            return Err(Error::UnexpectedDevice);
        }
    }
    // Restoring a session supplies no UIA credentials. For a password sign-in, perform the
    // SDK's guarded initial setup explicitly, once the validated store is open.
    let encryption = EncryptionSettings { auto_enable_cross_signing: password.is_none(), ..ENCRYPTION };
    let (client, crypto) =
        build_client_with_encryption(resolved.as_str(), store_dir, passphrase, sessions, holder, encryption).await?;
    client.restore_session(session).await?;
    if let Some(password) = password {
        initialize_password_identity(&client, account, password).await;
    }
    Ok((client, crypto))
}

async fn initialize_password_identity(client: &Client, account: &AccountId, password: &str) {
    use matrix_sdk::ruma::api::client::uiaa::{AuthData, MatrixUserIdentifier, Password, UserIdentifier};
    let auth = AuthData::Password(Password::new(
        UserIdentifier::Matrix(MatrixUserIdentifier::new(account.to_string())),
        password.to_owned(),
    ));
    let encryption = client.encryption();
    let setup = encryption.bootstrap_cross_signing_if_needed(Some(auth));
    if !matches!(tokio::time::timeout(std::time::Duration::from_secs(5), setup).await, Ok(Ok(()))) {
        tracing::warn!("could not initialize cross-signing after password sign-in");
    }
}

/// Where a sign-in keeps its data: the store and device of the account it resumes, when that
/// one was signed out softly and the sign-in is for it (the same homeserver, and the same user
/// where the username is known), or a new store and device. A sign-in as someone else never
/// opens another account's store.
async fn store_for(
    layout: &StoreLayout,
    sessions: &Sessions,
    resume: Option<&AccountId>,
    homeserver: &str,
    username: Option<&str>,
) -> Result<(PathBuf, String, Option<matrix_sdk::ruma::OwnedDeviceId>), Error> {
    // A password sign-in names its user, and that decides: the suspended account it names
    // resumes, whether the sign-in started from the server's name or from another suspended
    // account. A browser sign-in names nobody yet, so `resume` decides.
    let resume = match username {
        Some(name) => {
            let saved = blocking(sessions, |s| Ok(s.list())).await?;
            suspended_for(&saved, homeserver, name).map(AccountId::new)
        }
        None => resume.cloned(),
    };
    if let Some(id) = resume
        && let Some((stored, secrets)) = blocking(sessions, move |s| s.signed_out(&id)).await?
        && same_server(&stored.homeserver, homeserver)
        && username.is_none_or(|name| names_user(name, &stored.id))
    {
        let device = secrets.auth.device_id().to_owned();
        return Ok((stored.store_dir, secrets.passphrase, Some(device)));
    }
    Ok((layout.new_account_dir()?, new_passphrase()?, None))
}

/// The suspended account a password sign-in as `username` on `homeserver` is for, if any.
fn suspended_for<'a>(accounts: &'a [StoredAccount], homeserver: &str, username: &str) -> Option<&'a str> {
    accounts
        .iter()
        .find(|stored| {
            stored.signed_out && same_server(&stored.homeserver, homeserver) && names_user(username, &stored.id)
        })
        .map(|stored| stored.id.as_str())
}

/// Whether two homeserver URLs identify the same Matrix API, ignoring trailing path slashes.
fn same_server(a: &str, b: &str) -> bool {
    use url::Position::BeforePath;

    let (Ok(a), Ok(b)) = (Url::parse(a), Url::parse(b)) else { return false };
    a[..BeforePath] == b[..BeforePath]
        && a.path().trim_end_matches('/') == b.path().trim_end_matches('/')
        && a.query() == b.query()
        && a.fragment() == b.fragment()
}

/// Whether a typed username (a localpart, or a full user ID) names the user `id`.
fn names_user(username: &str, id: &str) -> bool {
    let typed = username.trim().trim_start_matches('@');
    let id = id.trim_start_matches('@');
    let localpart = id.split(':').next().unwrap_or(id);
    if typed.contains(':') { typed.eq_ignore_ascii_case(id) } else { typed.eq_ignore_ascii_case(localpart) }
}

/// A client for one saved account, for handling a push without the engine: its store opened
/// under the push's own lock names, its session restored, nothing started. Fails with
/// [`Error::UnknownAccount`] for an account that isn't saved here or was signed out softly.
pub(crate) async fn open_for_push(
    data_dir: &Path,
    secrets: Arc<dyn crate::SecretStore>,
    id: &AccountId,
) -> Result<Client, Error> {
    let sessions = Sessions::new(data_dir, secrets);
    let unknown = || Error::UnknownAccount(id.clone());
    let wanted = id.clone();
    let stored = blocking(&sessions, move |s| Ok(s.list().into_iter().find(|s| s.id == wanted.as_str()))).await?;
    let stored = stored.filter(|stored| !stored.signed_out).ok_or_else(unknown)?;
    let key = stored.id.clone();
    let saved = blocking(&sessions, move |s| s.secrets(&key)).await?.ok_or_else(unknown)?;
    let (client, _) =
        build_client(&stored.homeserver, &stored.store_dir, &saved.passphrase, &sessions, Holder::Push).await?;
    client.restore_session(saved.auth.into_session()).await?;
    Ok(client)
}

/// Forgets an account that isn't running (one signed out softly): its entry, secrets and store.
pub(crate) async fn forget(sessions: &Sessions, id: &AccountId) -> Result<(), Error> {
    let id = id.clone();
    blocking(sessions, move |s| s.remove(&id)).await
}

/// Marks an account signed out softly, keeping its store for signing in again.
pub(crate) async fn set_signed_out(sessions: &Sessions, id: &AccountId) -> Result<(), Error> {
    let id = id.clone();
    blocking(sessions, move |s| s.set_signed_out(&id)).await
}

/// Runs a session-store call on the blocking pool: secret stores may wait on a keychain or D-Bus.
pub(crate) async fn blocking<T: Send + 'static>(
    sessions: &Sessions,
    f: impl FnOnce(&Sessions) -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    let sessions = sessions.clone();
    tokio::task::spawn_blocking(move || f(&sessions))
        .await
        .map_err(|e| Error::Storage(std::io::Error::other(e.to_string())))?
}

/// Whether `error` means the server no longer accepts the session: its access token is unknown,
/// or its refresh token was rejected or is missing. Network trouble is not that.
pub(crate) fn session_ended(error: &matrix_sdk::Error) -> bool {
    use matrix_sdk::authentication::oauth::OAuthError;
    use matrix_sdk::authentication::oauth::error::{BasicErrorResponseType, RequestTokenError};
    use matrix_sdk::ruma::api::error::ErrorKind;
    use matrix_sdk::{HttpError, RefreshTokenError};

    let unknown_token = |kind: Option<&ErrorKind>| matches!(kind, Some(ErrorKind::UnknownToken { .. }));
    if unknown_token(error.client_api_error_kind()) {
        return true;
    }
    let matrix_sdk::Error::Http(http) = error else { return false };
    let HttpError::RefreshToken(refresh) = &**http else { return false };
    match refresh {
        RefreshTokenError::RefreshTokenRequired => true,
        RefreshTokenError::MatrixAuth(inner) => unknown_token(inner.client_api_error_kind()),
        RefreshTokenError::OAuth(inner) => matches!(
            &**inner,
            OAuthError::RefreshToken(RequestTokenError::ServerResponse(response))
                if *response.error() == BasicErrorResponseType::InvalidGrant
        ),
    }
}

/// The attachment info that makes an upload a voice message, for [`Outgoing::Voice`].
pub(crate) fn voice_info(message: &Outgoing) -> Option<matrix_sdk::attachment::AttachmentInfo> {
    let Outgoing::Voice { duration_ms, waveform, .. } = message else { return None };
    let max = f32::from(crate::model::AudioDetails::WAVEFORM_MAX);
    Some(matrix_sdk::attachment::AttachmentInfo::Voice(matrix_sdk::attachment::BaseAudioInfo {
        duration: Some(std::time::Duration::from_millis(*duration_ms)),
        size: None,
        waveform: (!waveform.is_empty()).then(|| waveform.iter().map(|&v| f32::from(v) / max).collect()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What an account learns of TURN from a homeserver answering `voip/turnServer` with `turn`.
    #[cfg(feature = "native-media")]
    async fn turn_from(turn: serde_json::Value) -> (bool, crate::convert::CallServices) {
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/voip/turnServer") {
                turn.clone()
            } else {
                crate::test_server::thread_response(request)
            }
        });
        let account = Account::for_test(&server.url()).await;
        let found = account.discover_turn().await;
        (found, account.call_services())
    }

    #[cfg(feature = "native-media")]
    #[tokio::test]
    async fn a_homeserver_naming_turn_servers_should_offer_one_to_one_calls() {
        let turn = serde_json::json!({
            "uris": ["turn:turn.example.org:3478?transport=udp"], "username": "u", "password": "p", "ttl": 86400
        });
        assert_eq!(turn_from(turn).await, (true, crate::convert::CallServices { rtc: false, turn: true }));
    }

    #[cfg(feature = "native-media")]
    #[tokio::test]
    async fn a_homeserver_without_turn_servers_should_offer_no_one_to_one_calls() {
        // Synapse answers an empty object when it has none; others refuse.
        assert_eq!(turn_from(serde_json::json!({})).await, (false, crate::convert::CallServices::default()));
        let refused = serde_json::json!({ "errcode": "M_NOT_FOUND", "error": "no TURN" });
        assert!(!turn_from(refused).await.0);
    }

    /// An account on a store shared with another client, as the engine and a push share one.
    async fn sharing(server: &str, dir: &Path, holder: Holder) -> Account {
        let sessions = Sessions::new(dir, Arc::new(crate::secrets::MemorySecrets::default()));
        let (client, _) =
            build_client(server, &dir.join("store"), "passphrase", &sessions, holder).await.expect("client");
        let session: matrix_sdk::authentication::matrix::MatrixSession = serde_json::from_value(serde_json::json!({
            "user_id": "@a:example.org", "device_id": "TEST", "access_token": "test-token"
        }))
        .expect("test session");
        client.restore_session(session).await.expect("restore test session");
        // As `ready` does for the engine, and the notification client for a push.
        if let Some(name) = holder.name() {
            client.encryption().enable_cross_process_store_lock(name.to_owned()).await.expect("store lock");
        }
        Account {
            id: AccountId::new("@a:example.org"),
            client,
            session_changes: Arc::new(Mutex::new(None)),
            rtc: Arc::new(Mutex::new(None)),
            turn: Arc::default(),
            meeting_server: Arc::default(),
            status: Arc::default(),
            status_saves: Arc::default(),
            saved_changes: Arc::default(),
            safety_changes: Arc::default(),
            safety_queue: crate::drafts::DraftQueue::default(),
            admin_changes: crate::drafts::DraftQueue::default(),
            drafts: crate::drafts::DraftQueue::default(),
            room_list: Arc::default(),
            extra_state: Arc::default(),
            pack_edits: Arc::default(),
            followed: Arc::default(),
            subscribing: Arc::default(),
            sticky: crate::calls::StickyMembers::default(),
            organising: crate::drafts::DraftQueue::default(),
            offline_device: Arc::new(tokio::sync::watch::Sender::new(crate::OfflineDevice::Unknown)),
            offline_changes: Arc::default(),
            sessions: Some(sessions),
            scheduling: Arc::default(),
            listing_scheduled: Arc::default(),
            scheduled_here: Arc::default(),
            crypto: None,
        }
    }

    fn shared_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("matrix-core-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A server that knows the client API's versions and answers every sync with batch `s1`.
    fn sync_server() -> crate::test_server::Server {
        crate::test_server::Server::new(|request| {
            if request.contains("/versions") {
                serde_json::json!({ "versions": ["v1.12"] })
            } else {
                serde_json::json!({ "next_batch": "s1" })
            }
        })
    }

    fn quick_sync() -> matrix_sdk::config::SyncSettings {
        matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sync_should_not_run_while_a_push_holds_the_crypto_store() {
        let server = sync_server();
        let dir = shared_dir("held");
        let engine = sharing(&server.url(), &dir, Holder::Engine).await;
        let push = sharing(&server.url(), &dir, Holder::Push).await;
        let _held =
            push.client().encryption().spin_lock_store(None).await.expect("lock").expect("multi-process locking");
        assert!(engine.sync_once_locked(quick_sync()).await.is_err(), "a sync must not touch the store a push holds");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sync_should_run_once_the_crypto_store_is_free() {
        let server = sync_server();
        let dir = shared_dir("free");
        let engine = sharing(&server.url(), &dir, Holder::Engine).await;
        let push = sharing(&server.url(), &dir, Holder::Push).await;
        drop(push.client().encryption().spin_lock_store(None).await.expect("lock"));
        assert_eq!(engine.sync_once_locked(quick_sync()).await.expect("sync").next_batch, "s1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_alone_should_sync_without_a_cross_process_lock() {
        let server = sync_server();
        let engine = sharing(&server.url(), &shared_dir("alone"), Holder::Alone).await;
        assert!(engine.client().encryption().spin_lock_store(None).await.expect("lock").is_none());
        assert_eq!(engine.sync_once_locked(quick_sync()).await.expect("sync").next_batch, "s1");
    }

    #[test]
    fn a_localpart_should_name_its_user() {
        assert!(names_user("alice", "@alice:example.org"));
    }

    #[test]
    fn a_full_user_id_should_name_only_that_user() {
        assert!(names_user("@alice:example.org", "@alice:example.org"));
        assert!(!names_user("@alice:other.org", "@alice:example.org"));
    }

    #[test]
    fn another_username_should_not_resume_an_account() {
        assert!(!names_user("bob", "@alice:example.org"));
    }

    fn stored(id: &str, signed_out: bool) -> StoredAccount {
        StoredAccount {
            id: id.into(),
            homeserver: "https://matrix.example.org/".into(),
            store_dir: PathBuf::from("/nonexistent"),
            signed_out,
        }
    }

    #[test]
    fn a_password_sign_in_should_find_the_suspended_account_it_names() {
        let accounts = [stored("@bob:example.org", true), stored("@alice:example.org", true)];
        assert_eq!(suspended_for(&accounts, "https://matrix.example.org", "Alice"), Some("@alice:example.org"));
    }

    #[test]
    fn a_running_account_should_not_count_as_suspended() {
        let accounts = [stored("@alice:example.org", false)];
        assert_eq!(suspended_for(&accounts, "https://matrix.example.org", "alice"), None);
    }

    #[test]
    fn a_trailing_slash_should_not_make_another_server() {
        assert!(same_server("https://matrix.example.org/", "https://matrix.example.org"));
        assert!(!same_server("https://matrix.example.org/", "https://matrix.other.org/"));
    }

    #[test]
    fn server_comparison_should_preserve_the_case_of_the_api_path() {
        assert!(same_server("https://MATRIX.example.org/Matrix/", "https://matrix.example.org/Matrix"));
        assert!(!same_server("https://matrix.example.org/Matrix", "https://matrix.example.org/matrix"));
    }
}

#[cfg(test)]
#[path = "organise_tests.rs"]
mod organise_tests;
