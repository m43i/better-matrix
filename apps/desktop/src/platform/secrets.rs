//! Secrets in the OS keychain (macOS Keychain, Windows Credential Manager, the Secret Service
//! on Linux), falling back to private files when no keychain is reachable.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use matrix_core::{FileSecrets, SecretError, SecretStore};

const SERVICE: &str = crate::APP_ID;

/// The best secret store this machine offers. The keychain is probed on first use (on the
/// engine's thread), not at launch.
pub fn system_secrets(data_dir: &Path) -> Arc<dyn SecretStore> {
    Arc::new(Fallback { file: FileSecrets::new(data_dir.join("secrets")), told: OnceLock::new() })
}

/// The system keychain alone, never files: for secrets that may live only there (the user's
/// GIPHY key). It is probed on first use (off the UI thread); without a reachable keychain,
/// every call fails.
pub fn system_keychain() -> Arc<dyn SecretStore> {
    Arc::new(KeychainOnly)
}

struct KeychainOnly;

impl KeychainOnly {
    fn store(&self) -> Result<&Keychain, SecretError> {
        if keychain_works() { Ok(&Keychain) } else { Err(SecretError("no system keychain is reachable".into())) }
    }
}

impl SecretStore for KeychainOnly {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        self.store()?.get(key)
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.store()?.set(key, value)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.store()?.delete(key)
    }
}

/// Whether the system keychain keeps and gives back a secret: probed once for the whole app, so
/// the session store and the GIPHY key's never probe at once (one's probe deleting the entry
/// the other is reading would send sessions to files).
fn keychain_works() -> bool {
    static WORKS: OnceLock<bool> = OnceLock::new();
    *WORKS.get_or_init(probe)
}

/// Whether the system keychain keeps and gives back a secret.
fn probe() -> bool {
    let probe = "probe";
    let works = Keychain.set(probe, "ok").is_ok() && Keychain.get(probe).ok().flatten().as_deref() == Some("ok");
    let _ = Keychain.delete(probe);
    works
}

struct Keychain;

impl Keychain {
    fn entry(key: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, key).map_err(|e| SecretError(e.to_string()))
    }
}

impl SecretStore for Keychain {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(SecretError(error.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        Self::entry(key)?.set_password(value).map_err(|e| SecretError(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(SecretError(error.to_string())),
        }
    }
}

struct Fallback {
    file: FileSecrets,
    /// Whether the fallback was announced, once.
    told: OnceLock<()>,
}

impl Fallback {
    fn store(&self) -> &dyn SecretStore {
        if keychain_works() {
            return &Keychain;
        }
        self.told
            .get_or_init(|| eprintln!("better-matrix: no system keychain; keeping secrets in private files instead"));
        &self.file
    }
}

impl SecretStore for Fallback {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        self.store().get(key)
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.store().set(key, value)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.store().delete(key)
    }
}
