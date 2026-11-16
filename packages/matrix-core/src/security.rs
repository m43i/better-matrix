//! Encryption and sessions for one account: whether this session is verified, the recovery key
//! (which restores message keys on a new session), self-verification with emoji, resetting the
//! identity when both are lost, and the list of signed-in sessions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use matrix_sdk::Client;
use matrix_sdk::encryption::CrossSigningResetAuthType;
use matrix_sdk::encryption::VerificationState;
use matrix_sdk::encryption::recovery::{RecoveryError, RecoveryState};
use matrix_sdk::encryption::verification::{
    SasState, SasVerification, Verification, VerificationRequest, VerificationRequestState,
};
use matrix_sdk::ruma::OwnedDeviceId;
use matrix_sdk::ruma::api::client::uiaa;
use matrix_sdk::ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEvent;

use crate::Error;
use crate::account::Account;
use crate::engine::{Event, EventSink};
use crate::model::AccountId;

/// How the account's encryption stands on this device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Security {
    /// This session is verified: signed by the user's identity, so it can read and share keys.
    pub session_verified: bool,
    /// Whether a recovery key protects the account's keys on the server.
    pub recovery: RecoveryStatus,
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

/// Where a verification between this session and another of the user's sessions stands.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum VerificationStep {
    /// Another session asks to verify this one; accept or cancel.
    Incoming {
        /// The other session's name or ID.
        other_device: String,
    },
    /// Waiting for the other session.
    Waiting,
    /// Compare these emoji with the other session, then confirm or reject.
    Compare(Vec<SasEmoji>),
    /// Confirmed here; waiting for the other session to confirm.
    Confirmed,
    /// Verified.
    Done,
    /// Stopped, with a reason fit to show.
    Cancelled(String),
}

/// Verification flows in progress, by flow ID.
#[derive(Clone, Default)]
pub(crate) struct Flows(Arc<Mutex<HashMap<String, Flow>>>);

#[derive(Clone)]
struct Flow {
    request: VerificationRequest,
    sas: Option<SasVerification>,
}

impl Flows {
    fn insert(&self, request: VerificationRequest) {
        if let Ok(mut flows) = self.0.lock() {
            flows.insert(request.flow_id().to_owned(), Flow { request, sas: None });
        }
    }

    fn set_sas(&self, flow_id: &str, sas: SasVerification) {
        if let Ok(mut flows) = self.0.lock()
            && let Some(flow) = flows.get_mut(flow_id)
        {
            flow.sas = Some(sas);
        }
    }

    fn remove(&self, flow_id: &str) {
        if let Ok(mut flows) = self.0.lock() {
            flows.remove(flow_id);
        }
    }

    fn get(&self, flow_id: &str) -> Result<Flow, Error> {
        self.0.lock().ok().and_then(|flows| flows.get(flow_id).cloned()).ok_or(Error::VerificationGone)
    }

    /// Accepts an incoming request.
    pub async fn accept(&self, flow_id: &str) -> Result<(), Error> {
        Ok(self.get(flow_id)?.request.accept().await?)
    }

    /// Confirms (or rejects) that the emoji match.
    pub async fn confirm(&self, flow_id: &str, matches: bool) -> Result<(), Error> {
        let sas = self.get(flow_id)?.sas.ok_or(Error::VerificationGone)?;
        if matches {
            sas.confirm().await?
        } else {
            sas.mismatch().await?
        }
        Ok(())
    }

    pub async fn cancel(&self, flow_id: &str) -> Result<(), Error> {
        let flow = self.get(flow_id)?;
        match flow.sas {
            Some(sas) => sas.cancel().await?,
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
    fn security(&self) -> Security {
        let encryption = self.client().encryption();
        Security {
            session_verified: encryption.verification_state().get() == VerificationState::Verified,
            recovery: recovery_status(encryption.recovery().state()),
        }
    }

    /// Restores the account's secrets (cross-signing keys, message key backup) with a recovery
    /// key, which also verifies this session.
    pub async fn recover(&self, key: &str) -> Result<(), Error> {
        Ok(self.client().encryption().recovery().recover(key.trim()).await?)
    }

    /// Sets up recovery and returns the new recovery key, to show the user once.
    pub async fn set_up_recovery(&self) -> Result<String, Error> {
        Ok(self.client().encryption().recovery().enable().wait_for_backups_to_upload().await?)
    }

    /// Replaces the recovery key and returns the new one, to show the user once.
    pub async fn reset_recovery_key(&self) -> Result<String, Error> {
        Ok(self.client().encryption().recovery().reset_key().await?)
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

    /// Asks the user's other sessions to verify this one; the flow is reported through
    /// [`Event::Verification`].
    pub(crate) async fn verify_with_other_device(&self, flows: &Flows, sink: Arc<dyn EventSink>) -> Result<(), Error> {
        let user = self.user_id()?;
        let identity = self.client().encryption().get_user_identity(&user).await?.ok_or(Error::NoIdentity)?;
        let request = identity.request_verification().await.map_err(|e| Error::Account(e.to_string()))?;
        flows.insert(request.clone());
        tokio::spawn(follow(self.id().clone(), request, flows.clone(), sink));
        Ok(())
    }
}

/// Reports the account's security now and on every change, and turns verification requests from
/// the user's other sessions into [`Event::Verification`]s. Runs until aborted.
pub(crate) async fn watch(account: Account, flows: Flows, sink: Arc<dyn EventSink>) {
    let id = account.id().clone();
    {
        let (id, flows, sink) = (id.clone(), flows.clone(), Arc::clone(&sink));
        account.client().add_event_handler(move |event: ToDeviceKeyVerificationRequestEvent, client: Client| {
            let (id, flows, sink) = (id.clone(), flows.clone(), Arc::clone(&sink));
            async move {
                let request =
                    client.encryption().get_verification_request(&event.sender, &event.content.transaction_id).await;
                if let Some(request) = request.filter(VerificationRequest::is_self_verification) {
                    flows.insert(request.clone());
                    follow(id, request, flows, sink).await;
                }
            }
        });
    }
    sink.emit(Event::SecurityChanged { account: id.clone(), security: account.security() });
    let encryption = account.client().encryption();
    let mut recovery = encryption.recovery().state_stream();
    let mut verification = encryption.verification_state();
    loop {
        tokio::select! {
            Some(_) = recovery.next() => {}
            Some(_) = verification.next() => {}
            else => break,
        }
        sink.emit(Event::SecurityChanged { account: id.clone(), security: account.security() });
    }
}

/// Drives one verification flow to its end, reporting each step.
async fn follow(account: AccountId, request: VerificationRequest, flows: Flows, sink: Arc<dyn EventSink>) {
    let flow_id = request.flow_id().to_owned();
    let report = |step: VerificationStep| {
        sink.emit(Event::Verification { account: account.clone(), flow_id: flow_id.clone(), step });
    };
    let mut changes = request.changes();
    let mut state = request.state();
    loop {
        match state {
            VerificationRequestState::Requested { other_device_data, .. } if !request.we_started() => {
                let name = other_device_data
                    .display_name()
                    .map_or_else(|| other_device_data.device_id().to_string(), str::to_owned);
                report(VerificationStep::Incoming { other_device: name });
            }
            VerificationRequestState::Ready { .. } => {
                report(VerificationStep::Waiting);
                if request.we_started()
                    && let Ok(Some(sas)) = request.start_sas().await
                {
                    flows.set_sas(&flow_id, sas.clone());
                    follow_sas(sas, &report).await;
                    break;
                }
            }
            VerificationRequestState::Transitioned { verification: Verification::SasV1(sas) } => {
                flows.set_sas(&flow_id, sas.clone());
                if !sas.we_started() {
                    let _ = sas.accept().await;
                }
                follow_sas(sas, &report).await;
                break;
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
        match changes.next().await {
            Some(next) => state = next,
            None => break,
        }
    }
    flows.remove(&flow_id);
}

async fn follow_sas(sas: SasVerification, report: &impl Fn(VerificationStep)) {
    let mut changes = sas.changes();
    while let Some(state) = changes.next().await {
        match state {
            SasState::KeysExchanged { emojis: Some(emojis), .. } => {
                let emojis = emojis
                    .emojis
                    .iter()
                    .map(|e| SasEmoji { symbol: e.symbol.to_owned(), description: e.description.to_owned() })
                    .collect();
                report(VerificationStep::Compare(emojis));
            }
            SasState::Confirmed => report(VerificationStep::Confirmed),
            SasState::Done { .. } => {
                report(VerificationStep::Done);
                return;
            }
            SasState::Cancelled(info) => {
                report(VerificationStep::Cancelled(info.reason().to_owned()));
                return;
            }
            _ => {}
        }
    }
}
