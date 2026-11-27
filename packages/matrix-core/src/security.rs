//! Encryption and sessions for one account: whether this session is verified, the recovery key
//! (which restores message keys on a new session), verifying sessions and people with emoji or a
//! QR code, message keys exported to a file, the offline device (dehydrated device, MSC3814),
//! resetting the identity when both are lost, and the list of signed-in sessions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use matrix_sdk::Client;
use matrix_sdk::encryption::CrossSigningResetAuthType;
use matrix_sdk::encryption::VerificationState;
use matrix_sdk::encryption::recovery::{RecoveryError, RecoveryState};
use matrix_sdk::encryption::verification::{
    QrVerification, QrVerificationData, QrVerificationState, SasState, SasVerification, Verification,
    VerificationRequest, VerificationRequestState,
};
use matrix_sdk::ruma::OwnedDeviceId;
use matrix_sdk::ruma::api::client::uiaa;
use matrix_sdk::ruma::events::key::verification::VerificationMethod;
use matrix_sdk::ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEvent;
use matrix_sdk::ruma::events::room::message::{MessageType, OriginalSyncRoomMessageEvent};

use crate::Error;
use crate::account::Account;
use crate::engine::{Event, EventSink};
use crate::qr::QrImage;

/// How the account's encryption stands on this device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Security {
    /// This session is verified: signed by the user's identity, so it can read and share keys.
    pub session_verified: bool,
    /// Whether a recovery key protects the account's keys on the server.
    pub recovery: RecoveryStatus,
    /// Whether an offline device catches messages while no session is signed in.
    pub offline_device: OfflineDevice,
}

/// The account's offline device (a dehydrated device, MSC3814): one the server keeps on the
/// user's behalf, so a session signed in later can read what was sent while none was. It is
/// locked with a key kept with the recovery key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum OfflineDevice {
    /// Not known yet.
    #[default]
    Unknown,
    /// The server keeps no offline devices.
    Unsupported,
    /// Off for the account (turning it on needs the recovery key).
    Off,
    /// On: the account has one (this session keeps it fresh, or another does).
    On,
}

/// What the user knows of someone's identity: their cross-signing keys, as this session sees them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Identity {
    /// The user verified them (or, for the user, this session is verified).
    pub verified: bool,
    /// Their identity changed since the user verified it: whoever holds it now may not be them.
    pub changed: bool,
}

/// What [`Command::ImportRoomKeys`](crate::Command::ImportRoomKeys) found in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct KeyImport {
    /// Keys new to this session.
    pub imported: u32,
    /// Keys in the file.
    pub total: u32,
}

/// Whether the account has a recovery key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum RecoveryStatus {
    /// Not known yet.
    #[default]
    Unknown,
    /// Set up and complete.
    Enabled,
    /// Never set up, or turned off.
    Disabled,
    /// Set up, but some secrets are missing on this device (verify to complete it).
    Incomplete,
}

/// One signed-in session (device) of the account.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Session {
    /// The device ID.
    pub device_id: String,
    /// Its display name, if set.
    pub name: Option<String>,
    /// When it was last active, in milliseconds since the Unix epoch.
    pub last_seen_ms: Option<u64>,
    /// From where it was last active.
    pub last_seen_ip: Option<String>,
    /// This session.
    pub current: bool,
    /// Verified by the user's identity.
    pub verified: bool,
}

/// One emoji of a verification's short authentication string.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct SasEmoji {
    /// The emoji.
    pub symbol: String,
    /// Its name in English, as the spec lists it.
    pub description: String,
}

/// Where a verification stands: between this session and another of the user's, or between the
/// user and another person.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum VerificationStep {
    /// Another session, or another person, asks to verify; accept or cancel.
    Incoming {
        /// The other session's name or ID.
        other_device: String,
        /// The person asking, when it isn't the user.
        user: Option<crate::Person>,
    },
    /// Waiting for the other side.
    Waiting,
    /// Both sides are ready: the other side may scan `qr`; this one may scan the other side's
    /// code ([`Command::ScanVerificationCode`](crate::Command::ScanVerificationCode)) when `scan`;
    /// or compare emoji ([`Command::CompareEmoji`](crate::Command::CompareEmoji)).
    Ready {
        /// A code for the other side to scan, when it can.
        qr: Option<QrImage>,
        /// The other side shows a code this one can scan.
        scan: bool,
    },
    /// The other side scanned this one's code: confirm when it says so too, else reject
    /// ([`Command::ConfirmVerification`](crate::Command::ConfirmVerification)).
    Scanned,
    /// Compare these emoji with the other side, then confirm or reject.
    Compare(Vec<SasEmoji>),
    /// Confirmed here; waiting for the other session to confirm.
    Confirmed,
    /// Verified.
    Done,
    /// Stopped, with a reason fit to show.
    Cancelled(String),
}

/// Verification flows in progress, by flow ID, and the tasks following them: they end with the
/// account ([`Flows::stop`]).
#[derive(Clone, Default)]
pub(crate) struct Flows {
    flows: Arc<Mutex<HashMap<String, Flow>>>,
    tasks: Arc<Mutex<Followers>>,
}

/// The tasks following flows, and whether the account stopped (no new ones then).
#[derive(Default)]
struct Followers {
    tasks: Vec<tokio::task::AbortHandle>,
    stopped: bool,
}

#[derive(Clone)]
struct Flow {
    request: VerificationRequest,
    method: Option<Method>,
}

/// How a flow verifies, once one side started.
#[derive(Clone)]
enum Method {
    Sas(SasVerification),
    Qr(QrVerification),
}

/// The methods this app offers: emoji, showing a code and confirming it was scanned, and
/// scanning the other side's code where the app has a camera to.
pub(crate) fn methods(scans_qr: bool) -> Vec<VerificationMethod> {
    let mut methods =
        vec![VerificationMethod::SasV1, VerificationMethod::QrCodeShowV1, VerificationMethod::ReciprocateV1];
    if scans_qr {
        methods.push(VerificationMethod::QrCodeScanV1);
    }
    methods
}

impl Flows {
    /// Keeps `request` and follows it to its end on a task of its own, reporting each step.
    /// Returns at once: an event handler that waited for the flow would hold up the sync that
    /// brings its next steps.
    ///
    /// `announce` hears the flow's ID once commands can reach it and before any of its steps.
    /// Once the account stopped, nothing is followed and this returns false.
    fn follow(
        &self,
        account: Account,
        request: VerificationRequest,
        scans_qr: bool,
        sink: Arc<dyn EventSink>,
        announce: impl FnOnce(&str),
    ) -> bool {
        let Ok(mut followers) = self.tasks.lock() else { return false };
        if followers.stopped {
            return false;
        }
        if let Ok(mut flows) = self.flows.lock() {
            flows.insert(request.flow_id().to_owned(), Flow { request: request.clone(), method: None });
        }
        announce(request.flow_id());
        let task = tokio::spawn(follow(account, request, self.clone(), scans_qr, sink));
        followers.tasks.retain(|task| !task.is_finished());
        followers.tasks.push(task.abort_handle());
        true
    }

    /// Stops following every flow, now and later: the account stopped.
    pub(crate) fn stop(&self) {
        if let Ok(mut followers) = self.tasks.lock() {
            followers.stopped = true;
            for task in followers.tasks.drain(..) {
                task.abort();
            }
        }
    }

    fn set_method(&self, flow_id: &str, method: Method) {
        if let Ok(mut flows) = self.flows.lock()
            && let Some(flow) = flows.get_mut(flow_id)
        {
            flow.method = Some(method);
        }
    }

    fn remove(&self, flow_id: &str) {
        if let Ok(mut flows) = self.flows.lock() {
            flows.remove(flow_id);
        }
    }

    fn get(&self, flow_id: &str) -> Result<Flow, Error> {
        self.flows.lock().ok().and_then(|flows| flows.get(flow_id).cloned()).ok_or(Error::VerificationGone)
    }

    /// Accepts an incoming request, offering the app's methods.
    pub async fn accept(&self, flow_id: &str, scans_qr: bool) -> Result<(), Error> {
        Ok(self.get(flow_id)?.request.accept_with_methods(methods(scans_qr)).await?)
    }

    /// Confirms (or rejects) that the emoji match, or that the other side scanned this one's code.
    pub async fn confirm(&self, flow_id: &str, matches: bool) -> Result<(), Error> {
        match self.get(flow_id)?.method.ok_or(Error::VerificationGone)? {
            Method::Sas(sas) if matches => sas.confirm().await?,
            Method::Sas(sas) => sas.mismatch().await?,
            Method::Qr(qr) if matches => qr.confirm().await?,
            Method::Qr(qr) => qr.cancel().await?,
        }
        Ok(())
    }

    /// Switches a ready flow to comparing emoji; the flow's task follows it.
    pub async fn compare_emoji(&self, flow_id: &str) -> Result<(), Error> {
        self.get(flow_id)?.request.start_sas().await?;
        Ok(())
    }

    /// Takes the code the other side shows, as scanned; the flow's task follows it.
    pub async fn scan(&self, flow_id: &str, code: &[u8]) -> Result<(), Error> {
        let data = QrVerificationData::from_bytes(code).map_err(|_| Error::NotAVerificationCode)?;
        self.get(flow_id)?.request.scan_qr_code(data).await?.ok_or(Error::NotAVerificationCode)?;
        Ok(())
    }

    pub async fn cancel(&self, flow_id: &str) -> Result<(), Error> {
        let flow = self.get(flow_id)?;
        match flow.method {
            Some(Method::Sas(sas)) => sas.cancel().await?,
            Some(Method::Qr(qr)) => qr.cancel().await?,
            None => flow.request.cancel().await?,
        }
        Ok(())
    }
}

fn recovery_status(state: RecoveryState) -> RecoveryStatus {
    match state {
        RecoveryState::Enabled => RecoveryStatus::Enabled,
        RecoveryState::Disabled => RecoveryStatus::Disabled,
        RecoveryState::Incomplete => RecoveryStatus::Incomplete,
        _ => RecoveryStatus::Unknown,
    }
}

impl Account {
    pub(crate) fn security(&self) -> Security {
        let encryption = self.client().encryption();
        Security {
            session_verified: encryption.verification_state().get() == VerificationState::Verified,
            recovery: recovery_status(encryption.recovery().state()),
            offline_device: *self.offline_device.borrow(),
        }
    }

    /// Restores the account's secrets (cross-signing keys, message key backup) with a recovery
    /// key, which also verifies this session. Takes over the account's offline device when it
    /// has one, reading what it caught first.
    pub async fn recover(&self, key: &str) -> Result<(), Error> {
        self.client().encryption().recovery().recover(key.trim()).await?;
        if let Err(error) = self.adopt_offline_device(key.trim()).await {
            eprintln!("matrix-core: could not take over the offline device: {error}");
        }
        Ok(())
    }

    /// Writes every message key this session holds to `path`, encrypted with `passphrase`, for
    /// [`Account::import_room_keys`] on another session or client.
    pub async fn export_room_keys(&self, path: &std::path::Path, passphrase: &str) -> Result<(), Error> {
        Ok(self.client().encryption().export_room_keys(path.to_owned(), passphrase, |_| true).await?)
    }

    /// Reads message keys from a file [`Account::export_room_keys`] (or another client) wrote.
    /// Messages they unlock decrypt by themselves.
    pub async fn import_room_keys(&self, path: &std::path::Path, passphrase: &str) -> Result<KeyImport, Error> {
        let result = self
            .client()
            .encryption()
            .import_room_keys(path.to_owned(), passphrase)
            .await
            .map_err(|e| Error::KeyImport(e.to_string()))?;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(KeyImport { imported: count(result.imported_count), total: count(result.total_count) })
    }

    /// What this session knows of `user_id`'s identity; `None` when they have none (no
    /// encryption set up), or this session hasn't seen it.
    pub async fn identity(&self, user_id: &str) -> Result<Option<Identity>, Error> {
        let user = matrix_sdk::ruma::UserId::parse(user_id)?;
        let identity = self.client().encryption().request_user_identity(&user).await?;
        Ok(identity.map(|identity| Identity {
            verified: identity.is_verified(),
            changed: identity.has_verification_violation(),
        }))
    }

    /// Sets up recovery and returns the new recovery key, to show the user once.
    pub async fn set_up_recovery(&self) -> Result<String, Error> {
        self.offline_key_unready().await?;
        let key = self.client().encryption().recovery().enable().wait_for_backups_to_upload().await?;
        self.keep_offline_key_after_reset(&key).await;
        Ok(key)
    }

    /// Replaces the recovery key and returns the new one, to show the user once.
    pub async fn reset_recovery_key(&self) -> Result<String, Error> {
        self.offline_key_unready().await?;
        let key = self.client().encryption().recovery().reset_key().await?;
        self.keep_offline_key_after_reset(&key).await;
        Ok(key)
    }

    /// Secret storage starts over under a new recovery key with only the account's own secrets:
    /// the offline device's key goes in again, so a session recovering later takes it over.
    async fn keep_offline_key_after_reset(&self, key: &str) {
        if let Err(error) = self.keep_offline_key(key).await {
            eprintln!("matrix-core: could not keep the offline device's key: {error}");
        }
    }

    /// Fetches a room's keys from the key backup, so its messages that couldn't be decrypted
    /// can be. Does nothing while this session can't read the backup (it isn't verified yet).
    /// The timeline decrypts again by itself once the keys are stored.
    pub async fn fetch_room_keys(&self, room_id: &str) -> Result<(), Error> {
        let backups = self.client().encryption().backups();
        if !backups.are_enabled().await {
            return Ok(());
        }
        let room_id = matrix_sdk::ruma::RoomId::parse(room_id)?;
        Ok(backups.download_room_keys_for_room(&room_id).await?)
    }

    /// Every signed-in session, this one first, then the most recently active.
    pub async fn sessions(&self) -> Result<Vec<Session>, Error> {
        let client = self.client();
        let own = client.device_id().map(ToString::to_string);
        let user = self.user_id()?;
        let devices = client.encryption().get_user_devices(&user).await?;
        let mut sessions: Vec<Session> = client
            .devices()
            .await?
            .devices
            .into_iter()
            .map(|device| Session {
                verified: devices.get(&device.device_id).is_some_and(|d| d.is_verified()),
                current: own.as_deref() == Some(device.device_id.as_str()),
                device_id: device.device_id.to_string(),
                name: device.display_name,
                last_seen_ms: device.last_seen_ts.map(|ts| u64::from(ts.0)),
                last_seen_ip: device.last_seen_ip,
            })
            .collect();
        sessions.sort_by_key(|s| (!s.current, std::cmp::Reverse(s.last_seen_ms)));
        Ok(sessions)
    }

    /// Gives a session a new display name.
    pub async fn rename_session(&self, device_id: &str, name: &str) -> Result<(), Error> {
        let device_id: OwnedDeviceId = device_id.into();
        self.client().rename_device(&device_id, name).await?;
        Ok(())
    }

    /// Ends another session. Password accounts confirm with the password; OAuth accounts end
    /// sessions on their server's account page, so this returns that page's URL to open instead.
    pub async fn end_session(&self, device_id: &str, password: Option<&str>) -> Result<Option<String>, Error> {
        let client = self.client();
        let device_id: OwnedDeviceId = device_id.into();
        if client.oauth().client_id().is_some() {
            use matrix_sdk::ruma::api::client::discovery::get_authorization_server_metadata::v1::{
                AccountManagementActionData, DeviceDeleteData,
            };
            let action = AccountManagementActionData::DeviceDelete(DeviceDeleteData::new(&device_id));
            return Ok(Some(self.account_page_url(action).await?));
        }
        let devices = [device_id];
        match client.delete_devices(&devices, None).await {
            Ok(_) => Ok(None),
            Err(error) => {
                let Some(info) = error.as_uiaa_response() else { return Err(error.into()) };
                let auth = self.password_auth(password.ok_or(Error::PasswordRequired)?, info.session.clone());
                client.delete_devices(&devices, Some(auth)).await?;
                Ok(None)
            }
        }
    }

    /// Replaces the account's cryptographic identity, for when the recovery key and every
    /// verified session are lost. Deletes the key backup and recovery, makes new cross-signing
    /// keys with this session as their first verified one, and restarts key backup.
    ///
    /// The server guards the new keys: an OAuth account approves them on the account page, whose
    /// URL goes to `approve` before this waits (up to two minutes) for the approval; a password
    /// account needs `password`, or this fails with [`Error::PasswordRequired`]. Messages
    /// encrypted before stay unreadable unless another session still holds their keys.
    pub async fn reset_identity(&self, password: Option<&str>, approve: impl FnOnce(String)) -> Result<(), Error> {
        let Some(handle) = self.client().encryption().recovery().reset_identity().await? else { return Ok(()) };
        let auth = match handle.auth_type() {
            CrossSigningResetAuthType::OAuth(info) => {
                approve(info.approval_url.to_string());
                None
            }
            CrossSigningResetAuthType::Uiaa(info) => match password {
                Some(password) => Some(self.password_auth(password, info.session.clone())),
                None => {
                    handle.cancel().await;
                    return Err(Error::PasswordRequired);
                }
            },
        };
        match handle.reset(auth).await {
            Err(RecoveryError::Sdk(matrix_sdk::Error::Timeout)) => Err(Error::ApprovalTimedOut),
            result => Ok(result?),
        }
    }

    /// Interactive auth with the account password, continuing the server's auth `session`.
    pub(crate) fn password_auth(&self, password: &str, session: Option<String>) -> uiaa::AuthData {
        let user = uiaa::UserIdentifier::Matrix(uiaa::MatrixUserIdentifier::new(self.id().to_string()));
        let mut auth = uiaa::Password::new(user, password.to_owned());
        auth.session = session;
        uiaa::AuthData::Password(auth)
    }

    /// Asks the user's other sessions to verify this one (`user_id` is `None`), or another person
    /// to verify each other in the direct chat they have both joined. `started` hears
    /// the new flow's ID once it can be cancelled and before any of its steps; the flow is then
    /// reported through
    /// [`Event::Verification`].
    pub(crate) async fn request_verification(
        &self,
        user_id: Option<&str>,
        flows: &Flows,
        scans_qr: bool,
        sink: Arc<dyn EventSink>,
        started: impl FnOnce(String),
    ) -> Result<(), Error> {
        let user = match user_id {
            Some(id) => matrix_sdk::ruma::UserId::parse(id)?,
            None => self.user_id()?,
        };
        // Their client takes a request only from devices it knows, which it learns in a chat
        // both have joined: in a chat started by the request itself, theirs would drop it.
        if user_id.is_some() {
            let joined = match self.client().get_dm_room(&user) {
                Some(room) => room.get_member_no_sync(&user).await?.is_some_and(|member| {
                    *member.membership() == matrix_sdk::ruma::events::room::member::MembershipState::Join
                }),
                None => false,
            };
            if !joined {
                return Err(Error::NoDirectChat);
            }
        }
        let encryption = self.client().encryption();
        let identity = match user_id {
            Some(_) => encryption.request_user_identity(&user).await?.ok_or(Error::NoTheirIdentity)?,
            None => encryption.get_user_identity(&user).await?.ok_or(Error::NoIdentity)?,
        };
        let request = identity
            .request_verification_with_methods(methods(scans_qr))
            .await
            .map_err(|e| Error::Account(e.to_string()))?;
        if !flows.follow(self.clone(), request.clone(), scans_qr, sink, |id| started(id.to_owned())) {
            // The account stopped while the server was asked.
            let _ = request.cancel().await;
            return Err(Error::Cancelled);
        }
        Ok(())
    }
}

/// Turns verification requests (from the user's other sessions, or from other people in a
/// room) into followed flows. Returns the handlers, for removing them when the account stops:
/// they hold the account, whose client holds them.
pub(crate) fn listen(
    account: &Account,
    flows: &Flows,
    scans_qr: bool,
    sink: &Arc<dyn EventSink>,
) -> Vec<matrix_sdk::event_handler::EventHandlerHandle> {
    let client = account.client();
    let to_device = {
        let (account, flows, sink) = (account.clone(), flows.clone(), Arc::clone(sink));
        client.add_event_handler(move |event: ToDeviceKeyVerificationRequestEvent, client: Client| {
            let (account, flows, sink) = (account.clone(), flows.clone(), Arc::clone(&sink));
            async move {
                let request =
                    client.encryption().get_verification_request(&event.sender, &event.content.transaction_id).await;
                if let Some(request) = request.filter(VerificationRequest::is_self_verification) {
                    flows.follow(account, request, scans_qr, sink, |_| {});
                }
            }
        })
    };
    let in_room = {
        let (account, flows, sink) = (account.clone(), flows.clone(), Arc::clone(sink));
        client.add_event_handler(move |event: OriginalSyncRoomMessageEvent, client: Client| {
            let (account, flows, sink) = (account.clone(), flows.clone(), Arc::clone(&sink));
            async move {
                let MessageType::VerificationRequest(content) = &event.content.msgtype else { return };
                if client.user_id() != Some(&*content.to) {
                    return;
                }
                let request = client.encryption().get_verification_request(&event.sender, &event.event_id).await;
                if let Some(request) = request.filter(|r| !r.we_started() && !r.is_done() && !r.is_cancelled()) {
                    flows.follow(account, request, scans_qr, sink, |_| {});
                }
            }
        })
    };
    vec![to_device, in_room]
}

/// Reports the account's security now and on every change, and keeps the offline device fresh
/// when this session runs it. Runs until aborted.
pub(crate) async fn watch(account: Account, sink: Arc<dyn EventSink>) {
    let id = account.id().clone();
    sink.emit(Event::SecurityChanged { account: id.clone(), security: account.security() });
    let encryption = account.client().encryption();
    let mut recovery = encryption.recovery().state_stream();
    let mut verification = encryption.verification_state();
    let mut offline = account.offline_device.subscribe();
    let report = async {
        loop {
            tokio::select! {
                Some(_) = recovery.next() => {}
                Some(_) = verification.next() => {}
                Ok(()) = offline.changed() => {}
                else => break,
            }
            sink.emit(Event::SecurityChanged { account: id.clone(), security: account.security() });
        }
    };
    tokio::join!(report, crate::offline_device::keep_fresh(account.clone()));
}

/// The person on the other side of a verification with someone else.
async fn other_person(account: &Account, request: &VerificationRequest) -> Option<crate::Person> {
    if request.is_self_verification() {
        return None;
    }
    let user = request.other_user_id();
    let member = match request.room_id().and_then(|room| account.client().get_room(room)) {
        Some(room) => room.get_member_no_sync(user).await.ok().flatten(),
        None => None,
    };
    Some(crate::Person {
        id: user.to_string(),
        name: member.as_ref().and_then(|m| m.display_name()).unwrap_or(user.as_str()).to_owned(),
        avatar: member.as_ref().and_then(|m| m.avatar_url()).map(ToOwned::to_owned).and_then(crate::convert::avatar),
    })
}

/// A change of the method a flow verifies with.
enum MethodChange {
    Sas(SasState),
    Qr(QrVerificationState),
}

type MethodChanges = std::pin::Pin<Box<dyn futures_util::Stream<Item = MethodChange> + Send>>;

/// What a flow's method says now: the step to report, and whether the flow ended.
fn method_step(change: MethodChange) -> Option<(VerificationStep, bool)> {
    match change {
        MethodChange::Sas(SasState::KeysExchanged { emojis: Some(emojis), .. }) => {
            let emojis = emojis
                .emojis
                .iter()
                .map(|e| SasEmoji { symbol: e.symbol.to_owned(), description: e.description.to_owned() })
                .collect();
            Some((VerificationStep::Compare(emojis), false))
        }
        MethodChange::Sas(SasState::Confirmed)
        | MethodChange::Qr(QrVerificationState::Confirmed | QrVerificationState::Reciprocated) => {
            Some((VerificationStep::Confirmed, false))
        }
        MethodChange::Qr(QrVerificationState::Scanned) => Some((VerificationStep::Scanned, false)),
        MethodChange::Sas(SasState::Done { .. }) | MethodChange::Qr(QrVerificationState::Done { .. }) => {
            Some((VerificationStep::Done, true))
        }
        MethodChange::Sas(SasState::Cancelled(info)) | MethodChange::Qr(QrVerificationState::Cancelled(info)) => {
            Some((VerificationStep::Cancelled(info.reason().to_owned()), true))
        }
        _ => None,
    }
}

/// Drives one verification flow to its end, reporting each step. It follows the request and the
/// method it verifies with at once: showing a QR code already moves the request on to that
/// method, and choosing emoji or scanning the other side's code replaces it. A verification of
/// someone else reports their identity once it ends.
async fn follow(
    account: Account,
    request: VerificationRequest,
    flows: Flows,
    scans_qr: bool,
    sink: Arc<dyn EventSink>,
) {
    let flow_id = request.flow_id().to_owned();
    let id = account.id().clone();
    let report = |step: VerificationStep| {
        sink.emit(Event::Verification { account: id.clone(), flow_id: flow_id.clone(), step });
    };
    let mut changes = request.changes();
    let mut method: MethodChanges = Box::pin(futures_util::stream::pending());
    // Whether the other side shows a code this app may scan, from when both were ready.
    let mut scan = false;
    let mut state = Some(request.state());
    loop {
        if let Some(current) = state.take() {
            match current {
                VerificationRequestState::Requested { other_device_data, .. } if !request.we_started() => {
                    let name = other_device_data
                        .display_name()
                        .map_or_else(|| other_device_data.device_id().to_string(), str::to_owned);
                    let user = other_person(&account, &request).await;
                    report(VerificationStep::Incoming { other_device: name, user });
                }
                VerificationRequestState::Ready { their_methods, .. } => {
                    scan = scans_qr && their_methods.contains(&VerificationMethod::QrCodeShowV1);
                    // Showing a code moves the request on to it: reported as it arrives.
                    let shown = matches!(request.generate_qr_code().await, Ok(Some(_)));
                    if !shown && scan {
                        report(VerificationStep::Ready { qr: None, scan });
                    } else if !shown {
                        // Emoji are all both sides share: the side that asked starts them.
                        report(VerificationStep::Waiting);
                        if request.we_started() {
                            let _ = request.start_sas().await;
                        }
                    }
                }
                VerificationRequestState::Transitioned { verification: Verification::SasV1(sas) } => {
                    flows.set_method(&flow_id, Method::Sas(sas.clone()));
                    method = Box::pin(sas.changes().map(MethodChange::Sas));
                    if !sas.we_started() {
                        let _ = sas.accept().await;
                    }
                }
                VerificationRequestState::Transitioned { verification: Verification::QrV1(qr) } => {
                    flows.set_method(&flow_id, Method::Qr(qr.clone()));
                    method = Box::pin(qr.changes().map(MethodChange::Qr));
                    match qr.state() {
                        // This side's code, shown until the other side scans it or picks another way.
                        QrVerificationState::Started => {
                            let image = qr.to_qr_code().ok().map(|code| QrImage::from_code(&code));
                            report(VerificationStep::Ready { qr: image, scan });
                        }
                        now => {
                            if let Some((step, ended)) = method_step(MethodChange::Qr(now)) {
                                report(step);
                                if ended {
                                    break;
                                }
                            }
                        }
                    }
                }
                VerificationRequestState::Done => {
                    report(VerificationStep::Done);
                    break;
                }
                VerificationRequestState::Cancelled(info) => {
                    report(VerificationStep::Cancelled(info.reason().to_owned()));
                    break;
                }
                _ => report(VerificationStep::Waiting),
            }
        }
        tokio::select! {
            next = changes.next() => match next {
                Some(next) => state = Some(next),
                None => break,
            },
            Some(change) = method.next() => {
                if let Some((step, ended)) = method_step(change) {
                    report(step);
                    if ended {
                        break;
                    }
                }
            }
        }
    }
    flows.remove(&flow_id);
    if !request.is_self_verification() {
        // The signature a verification makes reaches the identity a moment after it is done:
        // looked at again for a few seconds, so the person shows as verified.
        let user = request.other_user_id().to_string();
        let done = request.is_done();
        let mut identity = account.identity(&user).await.ok().flatten();
        for _ in 0..10 {
            if !done || identity.is_some_and(|known| known.verified) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            identity = account.identity(&user).await.ok().flatten();
        }
        sink.emit(Event::Identity { account: id.clone(), user_id: user, identity });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_with_a_camera_should_offer_to_scan_codes() {
        assert!(methods(true).contains(&VerificationMethod::QrCodeScanV1));
    }

    #[test]
    fn an_app_without_a_camera_should_still_show_codes_and_compare_emoji() {
        let offered = methods(false);
        assert!(!offered.contains(&VerificationMethod::QrCodeScanV1));
        assert!(offered.contains(&VerificationMethod::QrCodeShowV1));
        assert!(offered.contains(&VerificationMethod::SasV1));
    }
}
