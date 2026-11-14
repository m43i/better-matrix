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
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::utils::local_server::{LocalServerBuilder, LocalServerRedirectHandle};
use matrix_sdk::{Client, Room, SessionChange};
use tokio::sync::broadcast;

use crate::model::{
    AccountId, AccountSummary, MediaRef, MediaSize, NotificationMode, Outgoing, RoomList, SignInMethod, TimelineKey,
    Upload,
};
use crate::session::{Sessions, StoredAccount, StoredAuth, StoredSecrets, new_passphrase};
use crate::store::StoreLayout;
use crate::{DEVICE_DISPLAY_NAME, Error, convert};

/// How long a browser sign-in may take before the engine gives up waiting.
const BROWSER_LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// What a password login needs.
#[derive(Clone, PartialEq, Eq)]
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
}

/// A browser sign-in waiting for the user: the URL to open, and the redirect to wait for.
pub(crate) struct PendingOAuth {
    pub url: Url,
    client: Client,
    redirect: LocalServerRedirectHandle,
    store_dir: PathBuf,
    passphrase: String,
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

/// Room summaries kept between syncs, so each sync re-reads only the rooms it touched.
#[derive(Debug, Default)]
pub(crate) struct RoomCache {
    rooms: HashMap<String, crate::RoomSummary>,
    spaces: HashMap<String, crate::SpaceSummary>,
}

/// A client for an already resolved homeserver URL (see [`crate::discovery`]).
async fn build_client(homeserver_url: &str, store_dir: &Path, passphrase: &str) -> Result<Client, Error> {
    Ok(Client::builder()
        .homeserver_url(homeserver_url)
        .sqlite_store(store_dir, Some(passphrase))
        .with_encryption_settings(ENCRYPTION)
        .handle_refresh_tokens()
        .build()
        .await?)
}

impl Account {
    /// Signs in with a password and saves the session.
    pub(crate) async fn login_password(
        layout: &StoreLayout,
        sessions: &Sessions,
        request: &LoginRequest,
    ) -> Result<Self, Error> {
        let resume = request.resume.as_ref();
        let (store_dir, passphrase, device) =
            store_for(layout, sessions, resume, &request.homeserver, Some(&request.username)).await?;
        let client = build_client(&request.homeserver, &store_dir, &passphrase).await?;
        let mut login = client.matrix_auth().login_username(&request.username, &request.password);
        login = match &device {
            Some(device) => login.device_id(device.as_str()),
            None => login.initial_device_display_name(DEVICE_DISPLAY_NAME),
        };
        login.await?;
        Self::signed_in(client, sessions, store_dir, passphrase).await
    }

    /// Starts a browser sign-in on a resolved homeserver URL: registers this app with the
    /// server's OAuth provider and returns the URL the user must open. [`PendingOAuth::finish`]
    /// completes it.
    pub(crate) async fn start_oauth(
        layout: &StoreLayout,
        sessions: &Sessions,
        homeserver: &str,
        identity: &ClientIdentity,
        resume: Option<&AccountId>,
    ) -> Result<PendingOAuth, Error> {
        let (store_dir, passphrase, device) = store_for(layout, sessions, resume, homeserver, None).await?;
        let client = build_client(homeserver, &store_dir, &passphrase).await?;
        let (redirect_uri, redirect) = LocalServerBuilder::new().spawn().await.map_err(Error::Storage)?;
        let mut metadata = ClientMetadata::new(
            ApplicationType::Native,
            vec![OAuthGrantType::AuthorizationCode { redirect_uris: vec![redirect_uri.clone()] }],
            Localized::new(identity.uri.clone(), []),
        );
        metadata.client_name = Some(Localized::new(identity.name.clone(), []));
        let metadata = Raw::new(&metadata).map_err(|e| Error::Storage(e.into()))?;
        let data = client.oauth().login(redirect_uri, device, Some(metadata.into()), None).build().await?;
        Ok(PendingOAuth { url: data.url, client, redirect, store_dir, passphrase })
    }

    /// Resumes a saved session.
    pub(crate) async fn restore(sessions: &Sessions, stored: &StoredAccount) -> Result<Self, Error> {
        let id = stored.id.clone();
        let secrets = blocking(sessions, move |s| s.secrets(&id)).await?;
        let secrets = secrets.ok_or_else(|| Error::UnknownAccount(AccountId::new(&stored.id)))?;
        let client = build_client(&stored.homeserver, &stored.store_dir, &secrets.passphrase).await?;
        let changes = client.subscribe_to_session_changes();
        client.restore_session(secrets.auth.into_session()).await?;
        Self::ready(AccountId::new(&stored.id), client, changes).await
    }

    async fn signed_in(
        client: Client,
        sessions: &Sessions,
        store_dir: PathBuf,
        passphrase: String,
    ) -> Result<Self, Error> {
        let changes = client.subscribe_to_session_changes();
        let id = AccountId::new(client.user_id().ok_or(Error::NoUserId)?.as_str());
        let auth = StoredAuth::of(&client).ok_or(Error::NoUserId)?;
        let homeserver = client.homeserver().to_string();
        let stored = StoredAccount { id: id.to_string(), homeserver, store_dir, signed_out: false };
        blocking(sessions, move |s| s.save(stored, &StoredSecrets { auth, passphrase })).await?;
        Self::ready(id, client, changes).await
    }

    /// Turns on what every signed-in account runs: the event cache, which keeps room history on
    /// disk and fills gaps from time offline, and the send queue, which sends what was written
    /// offline (or in an earlier session) once the server is reachable.
    async fn ready(id: AccountId, client: Client, changes: broadcast::Receiver<SessionChange>) -> Result<Self, Error> {
        client.event_cache().subscribe()?;
        client.send_queue().set_enabled(true).await;
        // A bounded media cache: previews and pictures load from disk, without filling it.
        if let Err(error) = client.media().set_media_retention_policy(media_cache()).await {
            eprintln!("matrix-core: could not limit the media cache: {error}");
        }
        Ok(Self { id, client, session_changes: Arc::new(Mutex::new(Some(changes))), rtc: Arc::default() })
    }

    /// The account's token changes: the subscription made before its first request, the first
    /// time; a fresh one after.
    pub(crate) fn session_changes(&self) -> broadcast::Receiver<SessionChange> {
        let early = self.session_changes.lock().ok().and_then(|mut early| early.take());
        early.unwrap_or_else(|| self.client.subscribe_to_session_changes())
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

    /// Looks up the homeserver's call service (its RTC transports, else its well-known file).
    /// True when it names one.
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

    /// Saves the current tokens, after the SDK refreshed them.
    pub(crate) async fn save_tokens(&self, sessions: &Sessions) -> Result<(), Error> {
        let Some(auth) = StoredAuth::of(&self.client) else { return Ok(()) };
        let id = self.id.clone();
        blocking(sessions, move |s| s.update_auth(&id, auth)).await
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
    pub(crate) async fn update_room_list(&self, cache: &mut RoomCache, changed: Option<&HashSet<String>>) -> RoomList {
        let joined = self.client.joined_rooms();
        let ids: HashSet<String> = joined.iter().map(|room| room.room_id().to_string()).collect();
        // Rooms left: the SDK stops working out their latest event.
        let left: Vec<String> = cache.rooms.keys().filter(|id| !ids.contains(*id)).cloned().collect();
        for id in left {
            if let Ok(room_id) = matrix_sdk::ruma::RoomId::parse(&id) {
                self.client.latest_events().await.forget_room(&room_id).await;
            }
        }
        cache.rooms.retain(|id, _| ids.contains(id));
        cache.spaces.retain(|id, _| ids.contains(id));
        let stale = |id: &str, cache: &RoomCache| {
            changed.is_none_or(|changed| changed.contains(id))
                || (!cache.rooms.contains_key(id) && !cache.spaces.contains_key(id))
        };
        let mut settings = None;
        for room in &joined {
            let id = room.room_id().to_string();
            if !stale(&id, cache) {
                continue;
            }
            if room.is_space() {
                cache.spaces.insert(id, convert::space_summary(room).await);
            } else {
                // The SDK keeps each room's latest event (and stores it) once asked to: the
                // list's order comes from it. Asking again is a no-op.
                if !cache.rooms.contains_key(&id) {
                    let _ = self.client.latest_events().await.listen_to_room(room.room_id()).await;
                }
                // The push rules are read once per update, and only when a room needs them.
                if settings.is_none() {
                    settings = Some(self.client.notification_settings().await);
                }
                if let Some(settings) = &settings {
                    cache.rooms.insert(id, convert::room_summary(room, settings, self.rtc_service().is_some()).await);
                }
            }
        }
        let mut list = RoomList {
            rooms: cache.rooms.values().cloned().collect(),
            spaces: cache.spaces.values().cloned().collect(),
        };
        list.rooms.sort_by_cached_key(|room| room.name.to_lowercase());
        list.spaces.sort_by_cached_key(|space| space.name.to_lowercase());
        list
    }

    /// Queues a message for a joined room. The send queue persists it and retries until the
    /// server takes it; the room's timeline shows it at once as pending.
    pub async fn send(&self, room_id: &str, message: &Outgoing) -> Result<(), Error> {
        let room = self.joined_room(room_id)?;
        let queue = room.send_queue();
        match message {
            Outgoing::Text { body, html: Some(html) } => {
                queue.send(RoomMessageEventContent::text_html(body, html).into()).await?;
            }
            Outgoing::Text { body, html: None } => {
                queue.send(RoomMessageEventContent::text_plain(body).into()).await?;
            }
            Outgoing::Poll { question, answers, disclosed } => {
                queue.send(crate::polls::start(question, answers, *disclosed)?).await?;
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
        use matrix_sdk_ui::timeline::{RoomExt, TimelineFocus};
        let focus = match &key.thread {
            None => TimelineFocus::Live { hide_threaded_events: true },
            Some(root) => TimelineFocus::Thread { root_event_id: matrix_sdk::ruma::EventId::parse(root)? },
        };
        // The SDK's filter, and the unstable ring Element Call sends, which it doesn't know: it
        // shows as a call's card.
        let filter = |event: &matrix_sdk::ruma::events::AnySyncTimelineEvent, rules: &_| {
            matrix_sdk_ui::timeline::default_event_filter(event, rules)
                || event.event_type().to_string() == crate::calls::RING_EVENT
        };
        Ok(self.joined_room(&key.room_id)?.timeline_builder().with_focus(focus).event_filter(filter).build().await?)
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

    fn joined_room(&self, room_id: &str) -> Result<Room, Error> {
        let parsed = RoomId::parse(room_id)?;
        self.client
            .get_room(&parsed)
            .ok_or_else(|| Error::UnknownRoom { account: self.id.clone(), room_id: room_id.to_owned() })
    }
}

impl PendingOAuth {
    /// Waits for the browser to come back, then finishes signing in and saves the session.
    pub(crate) async fn finish(self, sessions: &Sessions) -> Result<Account, Error> {
        let query = tokio::time::timeout(BROWSER_LOGIN_TIMEOUT, self.redirect)
            .await
            .map_err(|_| Error::BrowserLogin("timed out waiting for the browser".into()))?
            .ok_or_else(|| Error::BrowserLogin("the browser sign-in was abandoned".into()))?;
        self.client.oauth().finish_login(matrix_sdk::utils::UrlOrQuery::Query(query.0)).await?;
        Account::signed_in(self.client, sessions, self.store_dir, self.passphrase).await
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

/// Whether two homeserver URLs are the same server, ignoring a trailing slash and case.
fn same_server(a: &str, b: &str) -> bool {
    a.trim_end_matches('/').eq_ignore_ascii_case(b.trim_end_matches('/'))
}

/// Whether a typed username (a localpart, or a full user ID) names the user `id`.
fn names_user(username: &str, id: &str) -> bool {
    let typed = username.trim().trim_start_matches('@');
    let id = id.trim_start_matches('@');
    let localpart = id.split(':').next().unwrap_or(id);
    if typed.contains(':') { typed.eq_ignore_ascii_case(id) } else { typed.eq_ignore_ascii_case(localpart) }
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
async fn blocking<T: Send + 'static>(
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
    let Outgoing::Voice { duration_ms, .. } = message else { return None };
    Some(matrix_sdk::attachment::AttachmentInfo::Voice(matrix_sdk::attachment::BaseAudioInfo {
        duration: Some(std::time::Duration::from_millis(*duration_ms)),
        size: None,
        waveform: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
