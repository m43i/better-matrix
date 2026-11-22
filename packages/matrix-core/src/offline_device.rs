//! The offline device: a dehydrated device (MSC3814) the server keeps for the account, so a
//! session signed in later can read messages sent while no session was signed in. Senders
//! encrypt for it like for any device; a new session takes it over ("rehydrates" it) and reads
//! the keys it caught.
//!
//! It is locked with a key kept in secret storage beside the account's other secrets, so it
//! takes the recovery key to turn it on, and a session that recovers with the recovery key takes
//! it over by itself. A device on the server is only ever replaced after reading what it caught,
//! so a failed read never loses messages. A session running it replaces it every week (as its
//! one-time keys run out); this session keeps the key in its own encrypted store for that, as
//! secret storage can't be opened again without the recovery key. Whether the account wants an
//! offline device at all is the account data event `chat.bettermatrix.offline_device`, so turning
//! it off on one session keeps the others' recoveries from bringing it back.

use std::time::Duration;

use base64::Engine as _;
use matrix_sdk::encryption::dehydrated_devices::DehydratedDeviceError;
use matrix_sdk::encryption::secret_storage::SecretStore;
use matrix_sdk::ruma::events::GlobalAccountDataEventType;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk_base::crypto::dehydrated_devices::DehydrationError;
use matrix_sdk_base::crypto::store::types::DehydratedDeviceKey;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::account::Account;
use crate::security::OfflineDevice;

/// Where this session keeps when it made the offline device, in its state store. The key itself
/// goes to the app's secret store (the keychain): the state store doesn't encrypt values.
const KEPT: &[u8] = b"chat.bettermatrix.offline_device";

/// The account data event saying whether the account wants an offline device.
const WANTED: &str = "chat.bettermatrix.offline_device";

/// The key's name in secret storage, as matrix-sdk stores it.
const SECRET_NAME: &str = "org.matrix.msc3814";

/// How often a running session replaces the offline device: as matrix-sdk does.
const ROTATION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How long a failed replacement waits before trying again.
const RETRY: Duration = Duration::from_secs(60 * 60);

/// What this session keeps to replace the offline device without the recovery key.
struct Kept {
    /// The device's key, base64 as secret storage holds it.
    key: String,
    /// When the device now on the server was made, in milliseconds since the Unix epoch; 0 while
    /// one is being made (kept first, so a device the server took is never left without its key).
    made_ms: u64,
    /// The key is in secret storage, so a session recovering later can read the device. Until
    /// then this session runs the device but reports it off, so turning it on stores the key.
    stored: bool,
}

/// The part of [`Kept`] in the state store.
#[derive(Serialize, Deserialize)]
struct Timing {
    made_ms: u64,
    stored: bool,
}

/// The account data saying whether the account wants an offline device.
#[derive(Serialize, Deserialize)]
struct Wanted {
    enabled: bool,
    /// Its setup finished: a device is on the server and its key in secret storage. Until then
    /// sessions show it off, so "Turn on" can finish it.
    #[serde(default)]
    ready: bool,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn failed(error: impl std::fmt::Display) -> Error {
    Error::OfflineDevice(error.to_string())
}

/// A key as secret storage holds it: base64, with or without padding.
fn decode_key(base64: &str) -> Result<DehydratedDeviceKey, Error> {
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    let engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let bytes = engine.decode(base64.trim()).map_err(failed)?;
    DehydratedDeviceKey::from_slice(&bytes).map_err(failed)
}

/// Whether reading a device failed because the key doesn't open it, rather than on the way.
fn opens_not(error: &DehydratedDeviceError) -> bool {
    matches!(error, DehydratedDeviceError::Crypto(DehydrationError::Pickle(_) | DehydrationError::LegacyPickle(_)))
}

/// When the next replacement is due for a device made at `made_ms`, from `now_ms`.
fn due_in(made_ms: u64, now_ms: u64) -> Duration {
    let age = Duration::from_millis(now_ms.saturating_sub(made_ms));
    ROTATION.saturating_sub(age)
}

impl Account {
    /// Turns the offline device on (with the recovery key, which unlocks its key) or off, for
    /// the account. On, it first reads what the account's current offline device caught.
    ///
    /// Each way runs in an order a failure can be retried from: off deletes the device before
    /// saying so for the account (a session finding it off deletes it again); on makes a fresh
    /// device before putting its key in secret storage, so a key there always opens the device
    /// on the server or one already replaced.
    pub async fn set_offline_device(&self, on: bool, recovery_key: Option<&str>) -> Result<(), Error> {
        let _one_at_a_time = self.offline_changes.lock().await;
        if !on {
            self.client().encryption().dehydrated_devices().delete().await.map_err(failed)?;
            self.set_wanted(Wanted { enabled: false, ready: false }).await?;
            self.forget_kept().await?;
            self.offline_device.send_replace(OfflineDevice::Off);
            return Ok(());
        }
        let store = self.secret_store(recovery_key.ok_or(Error::RecoveryKeyRequired)?).await?;
        self.set_wanted(Wanted { enabled: true, ready: false }).await?;
        self.turn_on(&store).await?;
        if self.kept_offline_device().await.is_some_and(|kept| kept.stored) && self.set_ready(true).await? {
            self.offline_device.send_replace(OfflineDevice::On);
        }
        Ok(())
    }

    /// Turning on, once the account wants it: see [`Account::set_offline_device`].
    async fn turn_on(&self, store: &SecretStore) -> Result<(), Error> {
        if let Some(key) = store.get_secret(SECRET_NAME).await.map_err(failed)? {
            match self.replace_offline_device(&key, true, true).await {
                // The key doesn't open the device on the server (it was replaced under a
                // recovery key reset elsewhere): nothing can read that one, so a fresh one
                // replaces it.
                Err(Error::StaleOfflineKey) => {}
                done => return done,
            }
        }
        // A fresh key, unless an earlier try made a device with this session's key and then
        // failed to store it: that device is read first, like any other.
        let key = match self.kept_offline_device().await {
            Some(kept) => match self.replace_offline_device(&kept.key, true, false).await {
                Ok(()) => kept.key,
                Err(Error::StaleOfflineKey) => self.fresh_offline_device().await?,
                Err(error) => return Err(error),
            },
            None => self.fresh_offline_device().await?,
        };
        // Gone again: another session turned it off meanwhile.
        let Some(kept) = self.kept_offline_device().await else { return Ok(()) };
        store.put_secret(SECRET_NAME, &key).await.map_err(failed)?;
        self.keep(&Kept { stored: true, ..kept }).await
    }

    async fn keep(&self, kept: &Kept) -> Result<(), Error> {
        let sessions = self.sessions.clone().ok_or_else(|| failed("no secret store"))?;
        let (id, key) = (self.id().clone(), kept.key.clone());
        crate::account::blocking(&sessions, move |s| s.set_offline_key(&id, Some(&key))).await?;
        let timing = Timing { made_ms: kept.made_ms, stored: kept.stored };
        let bytes = serde_json::to_vec(&timing).map_err(|e| Error::Storage(e.into()))?;
        self.client().state_store().set_custom_value(KEPT, bytes).await.map_err(failed)?;
        Ok(())
    }

    /// Stops keeping the offline device here: its timing and its key.
    async fn forget_kept(&self) -> Result<(), Error> {
        self.client().state_store().remove_custom_value(KEPT).await.map_err(failed)?;
        if let Some(sessions) = self.sessions.clone() {
            let id = self.id().clone();
            crate::account::blocking(&sessions, move |s| s.set_offline_key(&id, None)).await?;
        }
        Ok(())
    }

    /// Replaces the device on the server with one locked with a new key, returned.
    async fn fresh_offline_device(&self) -> Result<String, Error> {
        let key = DehydratedDeviceKey::new().to_base64();
        self.replace_offline_device(&key, false, false).await?;
        Ok(key)
    }

    /// After recovering with `recovery_key`: takes over the account's offline device, when the
    /// account wants one and secret storage holds its key.
    pub(crate) async fn adopt_offline_device(&self, recovery_key: &str) -> Result<(), Error> {
        let _one_at_a_time = self.offline_changes.lock().await;
        let devices = self.client().encryption().dehydrated_devices();
        if self.wanted().await? == Some(false) || !devices.is_supported().await.map_err(failed)? {
            return Ok(());
        }
        let store = self.secret_store(recovery_key).await?;
        match store.get_secret(SECRET_NAME).await.map_err(failed)? {
            Some(key) => match self.replace_offline_device(&key, true, true).await {
                // Nothing this key opens: there is none to take over.
                Err(Error::StaleOfflineKey) => Ok(()),
                Ok(()) => {
                    self.offline_device.send_replace(OfflineDevice::On);
                    Ok(())
                }
                Err(error) => Err(error),
            },
            None => Ok(()),
        }
    }

    /// Before the recovery key changes: the offline device's key won't open with the new one
    /// until it is carried over, so the account's device stops counting as set up first. A
    /// reset failing after this leaves it not ready, which "Turn on" finishes.
    pub(crate) async fn offline_key_unready(&self) -> Result<(), Error> {
        let _one_at_a_time = self.offline_changes.lock().await;
        self.set_ready(false).await.map(|_| ())
    }

    /// After the recovery key changed to `recovery_key`: secret storage starts over with the
    /// account's own secrets only, so the offline device this session runs goes in again. Its
    /// key is first checked against the device on the server by replacing that device with it (a
    /// key replaced elsewhere is dropped, not carried over), and the device reports off until
    /// the key is stored.
    pub(crate) async fn keep_offline_key(&self, recovery_key: &str) -> Result<(), Error> {
        let _one_at_a_time = self.offline_changes.lock().await;
        // Not ready since before the reset (`offline_key_unready`): it shows off everywhere until
        // its key is stored under the new recovery key, and for good when the key isn't here to
        // carry over (turning it on again makes a fresh one).
        let Some(kept) = self.kept_offline_device().await else { return Ok(()) };
        self.keep(&Kept { key: kept.key.clone(), made_ms: kept.made_ms, stored: false }).await?;
        self.offline_device.send_replace(OfflineDevice::Off);
        match self.replace_offline_device(&kept.key, true, false).await {
            Err(Error::StaleOfflineKey) => {
                self.forget_kept().await?;
                return Ok(());
            }
            done => done?,
        }
        // Gone again: the account turned it off meanwhile.
        let Some(made) = self.kept_offline_device().await else { return Ok(()) };
        self.secret_store(recovery_key).await?.put_secret(SECRET_NAME, &made.key).await.map_err(failed)?;
        self.keep(&Kept { stored: true, ..made }).await?;
        if self.set_ready(true).await? {
            self.offline_device.send_replace(OfflineDevice::On);
        }
        Ok(())
    }

    /// Puts a fresh offline device locked with `key` in place of the one on the server, after
    /// reading what that one caught (`read_first`; a failed read leaves it as it was), keeping
    /// the key here first (`stored`: it is in secret storage too, so the device reports on).
    /// When another session turned it off for the account meanwhile, the fresh one goes again.
    async fn replace_offline_device(&self, key: &str, read_first: bool, stored: bool) -> Result<(), Error> {
        let devices = self.client().encryption().dehydrated_devices();
        let decoded = decode_key(key)?;
        if read_first && let Err(error) = devices.rehydrate(&decoded).await {
            return Err(if opens_not(&error) { Error::StaleOfflineKey } else { failed(error) });
        }
        // Kept before the upload: should the server take it but the answer never come, a retry
        // still has the key to read it with. Due at once, and not shown on, until it is made.
        self.keep(&Kept { key: key.to_owned(), made_ms: 0, stored: false }).await?;
        devices.create(None, &decoded).await.map_err(failed)?;
        self.keep(&Kept { key: key.to_owned(), made_ms: now_ms(), stored }).await?;
        match self.wanted().await {
            Ok(Some(false)) => {
                devices.delete().await.map_err(failed)?;
                self.forget_kept().await?;
                self.offline_device.send_replace(OfflineDevice::Off);
            }
            Ok(_) => {}
            Err(error) => eprintln!("matrix-core: could not ask whether the offline device is wanted: {error}"),
        }
        Ok(())
    }

    async fn secret_store(&self, recovery_key: &str) -> Result<SecretStore, Error> {
        self.client()
            .encryption()
            .secret_storage()
            .open_secret_store(recovery_key.trim())
            .await
            .map_err(|_| Error::WrongRecoveryKey)
    }

    async fn kept_offline_device(&self) -> Option<Kept> {
        let bytes = self.client().state_store().get_custom_value(KEPT).await.ok().flatten()?;
        let Timing { made_ms, stored } = serde_json::from_slice(&bytes).ok()?;
        let (sessions, id) = (self.sessions.clone()?, self.id().clone());
        let key = crate::account::blocking(&sessions, move |s| s.offline_key(&id)).await.ok().flatten()?;
        Some(Kept { key, made_ms, stored })
    }

    /// Whether the account wants an offline device, asked of the server (this device's copy may
    /// predate another session turning it off); `None` when no session said yet.
    async fn wanted(&self) -> Result<Option<bool>, Error> {
        Ok(self.wanted_setup().await?.map(|wanted| wanted.enabled))
    }

    async fn wanted_setup(&self) -> Result<Option<Wanted>, Error> {
        let account = self.client().account();
        let raw = account.fetch_account_data(GlobalAccountDataEventType::from(WANTED)).await?;
        Ok(raw.and_then(|raw| serde_json::from_str::<Wanted>(raw.json().get()).ok()))
    }

    /// Says whether the account's offline device is set up (`ready`), leaving whether the
    /// account wants one as it is: nothing changes when it was turned off (true when written).
    async fn set_ready(&self, ready: bool) -> Result<bool, Error> {
        match self.wanted_setup().await? {
            Some(Wanted { enabled: true, ready: was }) if was != ready => {
                self.set_wanted(Wanted { enabled: true, ready }).await?;
                Ok(true)
            }
            Some(Wanted { enabled: true, .. }) => Ok(true),
            _ => Ok(false),
        }
    }

    async fn set_wanted(&self, wanted: Wanted) -> Result<(), Error> {
        let raw = Raw::new(&wanted).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(WANTED), raw).await?;
        Ok(())
    }

    /// Follows the account setting as a sync brings it: a session that doesn't run the device
    /// itself shows what the account has, so another session turning it on or off shows here.
    pub(crate) async fn follow_offline_device(
        &self,
        account_data: &[Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>],
    ) {
        let Some(wanted) = account_data
            .iter()
            .rev()
            .filter(|raw| raw.get_field::<String>("type").ok().flatten().as_deref() == Some(WANTED))
            .find_map(|raw| raw.get_field::<Wanted>("content").ok().flatten())
        else {
            return;
        };
        if matches!(*self.offline_device.borrow(), OfflineDevice::Unknown | OfflineDevice::Unsupported) {
            return;
        }
        // What the account has shows everywhere, the session running it included: turned off
        // elsewhere, the change wakes its maintenance, which deletes the device here too; not
        // ready (a recovery key reset), "Turn on" there stores its key under the new one.
        let on = wanted.enabled && wanted.ready;
        self.offline_device.send_replace(if on { OfflineDevice::On } else { OfflineDevice::Off });
    }

    /// Replaces the offline device when its week is up; `Some` with the wait until the next
    /// look, `None` when this session doesn't run it. Stops running it when another session
    /// turned it off for the account.
    async fn rotate_offline_device(&self) -> Option<Duration> {
        let _one_at_a_time = self.offline_changes.lock().await;
        let kept = self.kept_offline_device().await?;
        match self.wanted_setup().await {
            Ok(Some(Wanted { enabled: false, .. })) => {
                // Deleted again: the session that turned it off may have failed to.
                if let Err(error) = self.client().encryption().dehydrated_devices().delete().await {
                    eprintln!("matrix-core: could not delete the offline device: {error}");
                    return Some(RETRY);
                }
                let _ = self.forget_kept().await;
                self.offline_device.send_replace(OfflineDevice::Off);
                return None;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("matrix-core: could not ask whether the offline device is wanted: {error}");
                return Some(RETRY);
            }
        }
        let due = due_in(kept.made_ms, now_ms());
        if !due.is_zero() {
            return Some(due);
        }
        match self.replace_offline_device(&kept.key, true, kept.stored).await {
            Ok(()) => Some(ROTATION),
            // Replaced elsewhere under another key: this session no longer runs it, and its
            // stale key never goes into secret storage again.
            // What shows stays the account's setting.
            Err(Error::StaleOfflineKey) => {
                let _ = self.forget_kept().await;
                None
            }
            Err(error) => {
                eprintln!("matrix-core: could not replace the offline device: {error}");
                Some(RETRY)
            }
        }
    }
}

/// Finds out whether the server keeps offline devices and whether this session runs one, then,
/// while it does, replaces it every week; turned on later (here, or by recovering), it starts
/// then. Runs until the account stops.
pub(crate) async fn keep_fresh(account: Account) {
    let devices = account.client().encryption().dehydrated_devices();
    let mut wait = Duration::from_secs(30);
    loop {
        match devices.is_supported().await {
            Ok(true) => break,
            Ok(false) => {
                account.offline_device.send_replace(OfflineDevice::Unsupported);
                return;
            }
            Err(error) => {
                eprintln!("matrix-core: could not ask about offline devices: {error}");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(RETRY);
            }
        }
    }
    let mut changes = account.offline_device.subscribe();
    // On when this session runs it, or when the account has one another session runs: this one
    // can then turn it off (no recovery key needed), or on to take it over.
    // What the account has (set up, by any session); without an answer, what this session runs.
    let running = match account.wanted_setup().await {
        Ok(wanted) => wanted.is_some_and(|wanted| wanted.enabled && wanted.ready),
        Err(_) => account.kept_offline_device().await.is_some_and(|kept| kept.stored),
    };
    // Unknown until now; turned on or off meanwhile already says so.
    account.offline_device.send_if_modified(|state| {
        let first = *state == OfflineDevice::Unknown;
        if first {
            *state = if running { OfflineDevice::On } else { OfflineDevice::Off };
        }
        first
    });
    loop {
        match account.rotate_offline_device().await {
            // Woken early by a change of state (turned off elsewhere, say).
            Some(wait) => {
                tokio::select! {
                    changed = changes.changed() => if changed.is_err() { return },
                    () = tokio::time::sleep(wait) => {}
                }
            }
            // Not run here: look again once it is turned on, and every hour for a device a
            // try to turn it on left behind.
            None => {
                tokio::select! {
                    changed = changes.changed() => if changed.is_err() { return },
                    () = tokio::time::sleep(RETRY) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_device_should_be_replaced_a_week_later() {
        assert_eq!(due_in(1_000, 1_000), ROTATION);
    }

    #[test]
    fn a_device_older_than_a_week_should_be_replaced_now() {
        let week = u64::try_from(ROTATION.as_millis()).expect("fits");
        assert_eq!(due_in(0, week + 5), Duration::ZERO);
    }

    #[test]
    fn a_key_should_read_with_or_without_padding() {
        let key = DehydratedDeviceKey::from_bytes(&[3; 32]);
        let unpadded = key.to_base64();
        assert_eq!(decode_key(&unpadded).expect("unpadded").to_base64(), unpadded);
        assert_eq!(decode_key(&format!("{unpadded}=")).expect("padded").to_base64(), unpadded);
    }

    fn setting(enabled: bool, ready: bool) -> Vec<Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>> {
        let event = serde_json::json!({ "type": WANTED, "content": { "enabled": enabled, "ready": ready } });
        vec![Raw::new(&event).expect("raw").cast_unchecked()]
    }

    #[tokio::test]
    async fn another_session_turning_it_on_should_show_here() {
        let server = crate::test_server::Server::new(|_| serde_json::json!({}));
        let account = Account::for_test(&server.url()).await;
        account.offline_device.send_replace(OfflineDevice::Off);
        account.follow_offline_device(&setting(true, true)).await;
        assert_eq!(*account.offline_device.borrow(), OfflineDevice::On);
    }

    #[tokio::test]
    async fn a_setup_another_session_left_unfinished_should_show_off() {
        let server = crate::test_server::Server::new(|_| serde_json::json!({}));
        let account = Account::for_test(&server.url()).await;
        account.offline_device.send_replace(OfflineDevice::On);
        account.follow_offline_device(&setting(true, false)).await;
        assert_eq!(*account.offline_device.borrow(), OfflineDevice::Off);
    }

    #[test]
    fn a_key_of_the_wrong_length_should_be_refused() {
        assert!(decode_key("AAAA").is_err());
    }
}
