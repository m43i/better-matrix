//! Signed-in accounts that survive a restart. The list of accounts (IDs, homeservers, store
//! directories) is a plain JSON file; tokens and database passphrases go to the
//! [`SecretStore`].

use std::path::{Path, PathBuf};

use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::authentication::oauth::{OAuthSession, UserSession};
use matrix_sdk::{AuthSession, Client};
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::model::AccountId;
use crate::secrets::SecretStore;

/// The non-secret part of a stored account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAccount {
    /// The account's user ID.
    pub id: String,
    /// The homeserver URL the client talks to.
    pub homeserver: String,
    /// The account's SDK store directory.
    pub store_dir: PathBuf,
    /// The server signed the session out softly: its store and device are kept for signing in
    /// as it again, and it isn't restored at launch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub signed_out: bool,
}

/// The secret part: how to resume the session, and the local database passphrase.
#[derive(Serialize, Deserialize)]
pub(crate) struct StoredSecrets {
    pub auth: StoredAuth,
    pub passphrase: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "api", rename_all = "snake_case")]
pub(crate) enum StoredAuth {
    Password(MatrixSession),
    #[serde(rename = "oauth")]
    OAuth {
        client_id: String,
        user: UserSession,
    },
}

impl StoredAuth {
    /// The client's current session, ready to store.
    pub fn of(client: &Client) -> Option<Self> {
        match client.session()? {
            AuthSession::Matrix(session) => Some(Self::Password(session)),
            AuthSession::OAuth(session) => {
                Some(Self::OAuth { client_id: session.client_id.as_str().to_owned(), user: session.user })
            }
            _ => None,
        }
    }

    /// The device the session signed in as.
    pub fn device_id(&self) -> &matrix_sdk::ruma::DeviceId {
        match self {
            Self::Password(session) => &session.meta.device_id,
            Self::OAuth { user, .. } => &user.meta.device_id,
        }
    }

    /// The session's tokens.
    pub fn tokens(&self) -> matrix_sdk::SessionTokens {
        match self {
            Self::Password(session) => session.tokens.clone(),
            Self::OAuth { user, .. } => user.tokens.clone(),
        }
    }

    pub fn into_session(self) -> AuthSession {
        match self {
            Self::Password(session) => session.into(),
            Self::OAuth { client_id, user } => {
                OAuthSession { client_id: matrix_sdk::authentication::oauth::ClientId::new(client_id), user }.into()
            }
        }
    }
}

/// The accounts file and the secret store, together.
#[derive(Clone)]
pub(crate) struct Sessions {
    index: PathBuf,
    secrets: std::sync::Arc<dyn SecretStore>,
    /// Held across every read-modify-write of the list or a secret, which run on blocking
    /// threads side by side: a sign-out and a token refresh must not undo each other.
    lock: std::sync::Arc<std::sync::Mutex<()>>,
}

// Hand-written: the secret store has no `Debug`, and its contents mustn't reach a log.
impl std::fmt::Debug for Sessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sessions").field("index", &self.index).finish_non_exhaustive()
    }
}

const SECRET_PREFIX: &str = "session:";

/// Where an account's offline device key is kept (`crate::offline_device`), beside its session.
const OFFLINE_KEY_PREFIX: &str = "offline-device:";

impl Sessions {
    pub fn new(root: &Path, secrets: std::sync::Arc<dyn SecretStore>) -> Self {
        Self { index: root.join("accounts.json"), secrets, lock: std::sync::Arc::default() }
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, ()> {
        // The lock guards no data, so a panic while holding it leaves nothing half-done in it.
        self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn list(&self) -> Vec<StoredAccount> {
        let mut list: Vec<StoredAccount> =
            std::fs::read(&self.index).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        for account in &mut list {
            account.store_dir = self.rebased(&account.store_dir);
        }
        list
    }

    /// A store directory under this data directory, wherever the list says it was: stores live
    /// in `<root>/accounts/<name>`, and the data directory may have moved since the list was
    /// written (an iOS app's container path changes with updates, and the data moves into the
    /// App Group container for push, ADR 0010). Paths elsewhere stay as they are.
    fn rebased(&self, dir: &Path) -> PathBuf {
        let in_accounts = dir.parent().and_then(Path::file_name).is_some_and(|parent| parent == "accounts");
        match (in_accounts, dir.file_name(), self.index.parent()) {
            (true, Some(name), Some(root)) => root.join("accounts").join(name),
            _ => dir.to_owned(),
        }
    }

    /// The saved tokens of an account, for a client whose tokens another process refreshed.
    pub fn tokens(&self, id: &str) -> Result<Option<matrix_sdk::SessionTokens>, Error> {
        Ok(self.secrets(id)?.map(|secrets| secrets.auth.tokens()))
    }

    fn write_list(&self, accounts: &[StoredAccount]) -> Result<(), Error> {
        if let Some(dir) = self.index.parent() {
            std::fs::create_dir_all(dir).map_err(Error::Storage)?;
        }
        let json = serde_json::to_vec_pretty(accounts).map_err(|e| Error::Storage(e.into()))?;
        // Write then rename, so a crash never leaves a half-written list.
        let temp = self.index.with_extension("json.tmp");
        std::fs::write(&temp, json).map_err(Error::Storage)?;
        std::fs::rename(&temp, &self.index).map_err(Error::Storage)
    }

    pub fn secrets(&self, id: &str) -> Result<Option<StoredSecrets>, Error> {
        let Some(json) = self.secrets.get(&format!("{SECRET_PREFIX}{id}"))? else { return Ok(None) };
        serde_json::from_str(&json).map(Some).map_err(|e| Error::Storage(e.into()))
    }

    /// Saves (or replaces) an account and its secrets. A replaced account's store goes, unless
    /// the new one took it over, but only once the new secrets and list are written: a failed
    /// write leaves it in place.
    pub fn save(&self, account: StoredAccount, secrets: &StoredSecrets) -> Result<(), Error> {
        let json = serde_json::to_string(secrets).map_err(|e| Error::Storage(e.into()))?;
        let _locked = self.locked();
        let key = format!("{SECRET_PREFIX}{}", account.id);
        // The secrets go first, so a listed account always has them; if the list then can't be
        // written, the previous secrets go back, so the store it still lists stays readable.
        let previous = self.secrets.get(&key)?;
        self.secrets.set(&key, &json)?;
        let mut list = self.list();
        let replaced: Vec<PathBuf> = list
            .iter()
            .filter(|known| known.id == account.id && known.store_dir != account.store_dir)
            .map(|known| known.store_dir.clone())
            .collect();
        list.retain(|known| known.id != account.id);
        list.push(account);
        if let Err(error) = self.write_list(&list) {
            let _ = match previous {
                Some(previous) => self.secrets.set(&key, &previous),
                None => self.secrets.delete(&key),
            };
            return Err(error);
        }
        for store in replaced {
            let _ = std::fs::remove_dir_all(store);
        }
        Ok(())
    }

    /// Replaces only the session of a saved account, after a token refresh.
    pub fn update_auth(&self, id: &AccountId, auth: StoredAuth) -> Result<(), Error> {
        let _locked = self.locked();
        let Some(mut secrets) = self.secrets(id.as_str())? else { return Ok(()) };
        secrets.auth = auth;
        let json = serde_json::to_string(&secrets).map_err(|e| Error::Storage(e.into()))?;
        Ok(self.secrets.set(&format!("{SECRET_PREFIX}{id}"), &json)?)
    }

    /// Marks an account signed out softly: it stays, with its store, for signing in again.
    pub fn set_signed_out(&self, id: &AccountId) -> Result<(), Error> {
        let _locked = self.locked();
        let mut list = self.list();
        for known in list.iter_mut().filter(|known| known.id == id.as_str()) {
            known.signed_out = true;
        }
        self.write_list(&list)
    }

    /// An account signed out softly, with its secrets: what signing in as it again reuses.
    pub fn signed_out(&self, id: &AccountId) -> Result<Option<(StoredAccount, StoredSecrets)>, Error> {
        let Some(stored) = self.list().into_iter().find(|known| known.id == id.as_str() && known.signed_out) else {
            return Ok(None);
        };
        Ok(self.secrets(id.as_str())?.map(|secrets| (stored, secrets)))
    }

    /// The key of the offline device this session runs for an account, if any.
    pub fn offline_key(&self, id: &AccountId) -> Result<Option<String>, Error> {
        Ok(self.secrets.get(&format!("{OFFLINE_KEY_PREFIX}{id}"))?)
    }

    /// Keeps (or with `None` forgets) the key of the offline device this session runs. Under the
    /// same lock as [`Sessions::remove`], and only for an account still listed: a replacement
    /// finishing after a sign-out leaves no key behind.
    pub fn set_offline_key(&self, id: &AccountId, key: Option<&str>) -> Result<(), Error> {
        let _locked = self.locked();
        let name = format!("{OFFLINE_KEY_PREFIX}{id}");
        if key.is_some() && !self.list().iter().any(|known| known.id == id.as_str()) {
            return Err(Error::UnknownAccount(id.clone()));
        }
        match key {
            Some(key) => self.secrets.set(&name, key)?,
            None => self.secrets.delete(&name)?,
        }
        Ok(())
    }

    /// Forgets an account: its secrets, its entry, and its local store.
    pub fn remove(&self, id: &AccountId) -> Result<(), Error> {
        let _locked = self.locked();
        self.secrets.delete(&format!("{SECRET_PREFIX}{id}"))?;
        self.secrets.delete(&format!("{OFFLINE_KEY_PREFIX}{id}"))?;
        let mut list = self.list();
        if let Some(known) = list.iter().find(|known| known.id == id.as_str()) {
            let _ = std::fs::remove_dir_all(&known.store_dir);
        }
        list.retain(|known| known.id != id.as_str());
        self.write_list(&list)
    }
}

/// A fresh random passphrase for an account's local database.
pub(crate) fn new_passphrase() -> Result<String, Error> {
    crate::random_hex(32)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::secrets::MemorySecrets;

    fn sessions() -> (Sessions, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "matrix-core-sessions-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        (Sessions::new(&root, Arc::new(MemorySecrets::default())), root)
    }

    fn account(id: &str) -> StoredAccount {
        StoredAccount {
            id: id.into(),
            homeserver: "https://example.org".into(),
            store_dir: PathBuf::from("/nonexistent"),
            signed_out: false,
        }
    }

    fn secrets() -> StoredSecrets {
        let session: MatrixSession = serde_json::from_value(serde_json::json!({
            "user_id": "@a:example.org", "device_id": "DEV", "access_token": "token"
        }))
        .unwrap();
        StoredSecrets { auth: StoredAuth::Password(session), passphrase: "p".into() }
    }

    #[test]
    fn a_saved_account_should_be_listed() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        let listed = s.list();
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(listed, [account("@a:example.org")]);
    }

    #[test]
    fn saving_an_account_again_should_replace_it() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        let count = s.list().len();
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(count, 1);
    }

    #[test]
    fn a_removed_account_should_lose_its_secrets() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        s.remove(&AccountId::new("@a:example.org")).unwrap();
        let gone = s.secrets("@a:example.org").unwrap().is_none();
        let _ = std::fs::remove_dir_all(root);
        assert!(gone);
    }

    #[test]
    fn a_removed_account_should_lose_its_offline_device_key() {
        let (s, root) = sessions();
        let id = AccountId::new("@a:example.org");
        s.save(account("@a:example.org"), &secrets()).unwrap();
        s.set_offline_key(&id, Some("key")).unwrap();
        s.remove(&id).unwrap();
        let gone = s.offline_key(&id).unwrap().is_none();
        let _ = std::fs::remove_dir_all(root);
        assert!(gone);
    }

    #[test]
    fn an_offline_device_key_for_a_removed_account_should_not_be_kept() {
        let (s, root) = sessions();
        let id = AccountId::new("@a:example.org");
        s.save(account("@a:example.org"), &secrets()).unwrap();
        s.remove(&id).unwrap();
        let refused = s.set_offline_key(&id, Some("key")).is_err();
        let gone = s.offline_key(&id).unwrap().is_none();
        let _ = std::fs::remove_dir_all(root);
        assert!(refused && gone);
    }

    #[test]
    fn accounts_saved_at_the_same_time_should_all_be_listed() {
        let (s, root) = sessions();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let s = s.clone();
                std::thread::spawn(move || s.save(account(&format!("@{i}:example.org")), &secrets()).unwrap())
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let count = s.list().len();
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(count, 8);
    }

    #[test]
    fn a_store_should_be_found_under_a_data_directory_that_moved() {
        let (s, root) = sessions();
        let moved =
            StoredAccount { store_dir: PathBuf::from("/old/container/accounts/abc"), ..account("@a:example.org") };
        s.save(moved, &secrets()).unwrap();
        let listed = s.list();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(listed[0].store_dir, root.join("accounts").join("abc"));
    }

    #[test]
    fn a_store_outside_an_accounts_directory_should_keep_its_path() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        let listed = s.list();
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(listed[0].store_dir, PathBuf::from("/nonexistent"));
    }

    #[test]
    fn saved_tokens_should_be_readable_for_another_process() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        let tokens = s.tokens("@a:example.org").unwrap();
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(tokens.unwrap().access_token, "token");
    }

    #[test]
    fn passphrases_should_differ_each_time() {
        assert_ne!(new_passphrase().unwrap(), new_passphrase().unwrap());
    }

    #[test]
    fn an_account_signed_out_softly_should_keep_its_store_and_secrets() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        s.set_signed_out(&AccountId::new("@a:example.org")).unwrap();
        let kept = s.signed_out(&AccountId::new("@a:example.org")).unwrap();
        let _ = std::fs::remove_dir_all(root);
        let (stored, secrets) = kept.unwrap();
        assert_eq!((stored.store_dir, secrets.auth.device_id().as_str()), (PathBuf::from("/nonexistent"), "DEV"));
    }

    #[test]
    fn signing_in_again_should_clear_the_signed_out_mark() {
        let (s, root) = sessions();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        s.set_signed_out(&AccountId::new("@a:example.org")).unwrap();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        let listed = s.list();
        let _ = std::fs::remove_dir_all(root);
        assert!(!listed[0].signed_out);
    }

    #[test]
    fn replacing_an_account_should_remove_its_old_store() {
        let (s, root) = sessions();
        let old = root.join("old-store");
        std::fs::create_dir_all(&old).unwrap();
        s.save(StoredAccount { store_dir: old.clone(), ..account("@a:example.org") }, &secrets()).unwrap();
        s.save(account("@a:example.org"), &secrets()).unwrap();
        let gone = !old.exists();
        let _ = std::fs::remove_dir_all(root);
        assert!(gone);
    }

    #[test]
    fn a_failed_list_write_should_put_the_previous_secrets_back() {
        let (s, root) = sessions();
        let old = root.join("old-store");
        std::fs::create_dir_all(&old).unwrap();
        s.save(StoredAccount { store_dir: old.clone(), ..account("@a:example.org") }, &secrets()).unwrap();
        let before = s.secrets("@a:example.org").unwrap().unwrap().passphrase;
        // A directory where the list's temp file goes makes writing the list fail.
        std::fs::create_dir_all(root.join("accounts.json.tmp")).unwrap();
        let replacement = StoredSecrets { passphrase: "new".into(), ..secrets() };
        let failed = s.save(account("@a:example.org"), &replacement).is_err();
        let after = s.secrets("@a:example.org").unwrap().unwrap().passphrase;
        let kept = old.exists();
        let _ = std::fs::remove_dir_all(root);
        assert!(failed && after == before && kept);
    }
}
