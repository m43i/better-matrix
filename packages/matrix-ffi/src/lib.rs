//! The mobile bridge to [`matrix_core`]: the engine behind one UniFFI object, [`Bridge`].
//!
//! Commands go in and events come out as JSON in the shapes `matrix_core` derives with its `ffi`
//! feature. Their TypeScript types are generated from the same Rust types (see [`typescript`]), so
//! the app and the engine can't drift apart. Two kinds of payload don't fit JSON and change on
//! the way out: media and GIF bytes are written to files under the cache directory and reported as
//! [`BridgeEvent`]s with the file's path, which the platform's image views open directly.
//! Call video and camera frames don't fit either: the platform's native views and cameras
//! exchange them with the engine directly ([`video`], [`camera`], [`native`], and JNI on
//! Android).
//!
//! Events leave on a thread of their own, in the order the engine reported them. Calling into
//! JavaScript from it blocks until JavaScript has handled the event, which keeps the engine's
//! threads free and the order intact. ADR 0010 records the design.

uniffi::setup_scaffolding!();

#[cfg(target_os = "android")]
mod android;
pub mod camera;
pub mod events;
pub mod native;
pub mod push;
pub mod typescript;
pub mod video;

use std::path::PathBuf;
use std::sync::Arc;

use matrix_core::{
    ClientIdentity, Command, Engine, EngineConfig, EngineHandle, OAuthRedirect, SecretError, SecretStore,
};

pub use camera::{CameraFailure, NativeCamera, register_native_camera};
pub use events::BridgeEvent;

/// What can go wrong at the bridge.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BridgeError {
    /// The app sent JSON that isn't a [`Command`].
    #[error("not a command: {reason}")]
    Command {
        /// Why it didn't parse.
        reason: String,
    },
    /// The engine couldn't start, or the configuration was invalid.
    #[error("could not start the engine: {reason}")]
    Start {
        /// Why.
        reason: String,
    },
}

/// A secret store call the platform couldn't carry out.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SecretFailure {
    /// The keychain or keystore refused.
    #[error("{reason}")]
    Failed {
        /// Why.
        reason: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SecretFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { reason: error.reason }
    }
}

/// The platform's keychain (iOS) or keystore-backed storage (Android), implemented by the app.
/// Calls come from the engine's blocking threads and wait for the answer.
#[uniffi::export(with_foreign)]
pub trait NativeSecrets: Send + Sync {
    /// The secret under `key`, or `None` when there is none.
    fn get(&self, key: String) -> Result<Option<String>, SecretFailure>;
    /// Stores `value` under `key`, replacing any previous value.
    fn set(&self, key: String, value: String) -> Result<(), SecretFailure>;
    /// Removes the secret under `key`; removing a missing key succeeds.
    fn delete(&self, key: String) -> Result<(), SecretFailure>;
}

/// Hears the engine's events as JSON: an `Event` or a [`BridgeEvent`], or an array of those that
/// were waiting together (`events::next_batch`).
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    /// Handles one event, or a batch of them in order.
    fn on_event(&self, event: String);
}

/// What the engine needs from the app.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeConfig {
    /// The app's private data directory: account stores and the saved-session list.
    pub data_dir: String,
    /// A directory the app may lose: fetched media and GIFs are written under it.
    pub cache_dir: String,
    /// The app's name, shown on OAuth consent screens.
    pub client_name: String,
    /// The app's public `https` home page, for OAuth registration.
    pub client_uri: String,
    /// The private-use URI the app is registered for, where OAuth sign-ins return
    /// (`Command::FinishOAuth` hands the URL back). `None` uses a loopback server.
    pub oauth_redirect: Option<String>,
}

/// The engine, for an app that speaks JSON. Dropping the last reference stops it.
#[derive(Debug, uniffi::Object)]
pub struct Bridge {
    engine: EngineHandle,
}

#[uniffi::export]
impl Bridge {
    /// Starts the engine. Saved accounts restore right away; their events follow on `listener`.
    #[uniffi::constructor]
    pub fn start(
        config: BridgeConfig,
        secrets: Arc<dyn NativeSecrets>,
        listener: Arc<dyn EventListener>,
    ) -> Result<Arc<Self>, BridgeError> {
        let invalid =
            |what: &str, error: &dyn std::fmt::Display| BridgeError::Start { reason: format!("{what}: {error}") };
        let uri = config.client_uri.parse().map_err(|e| invalid("client URI", &e))?;
        let oauth_redirect = match config.oauth_redirect {
            Some(redirect) => OAuthRedirect::App(redirect.parse().map_err(|e| invalid("OAuth redirect", &e))?),
            None => OAuthRedirect::Loopback,
        };
        let sink = events::Forwarder::start(PathBuf::from(config.cache_dir), listener)
            .map_err(|e| invalid("event thread", &e))?;
        let engine = Engine::start(
            EngineConfig {
                data_dir: PathBuf::from(config.data_dir),
                secrets: Arc::new(Secrets(secrets)),
                identity: ClientIdentity { name: config.client_name, uri },
                oauth_redirect,
                // The iOS push extension and Android's headless push task open them too.
                stores: matrix_core::StoreSharing::WithPush,
                // The phones scan verification codes with the camera.
                scans_qr: true,
            },
            sink,
        )
        .map_err(|e| invalid("engine", &e))?;
        video::set_engine(Some(engine.clone()));
        Ok(Arc::new(Self { engine }))
    }

    /// Sends one command, a JSON `Command`, and returns its request ID. Its answer arrives as
    /// events, ending with exactly one `Finished` for that ID.
    pub fn send(&self, command: String) -> Result<u64, BridgeError> {
        let command = parse_command(&command)?;
        Ok(self.engine.send(command).0)
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // Video views stop asking this engine; it stops once its last handle goes.
        video::set_engine(None);
    }
}

/// A JSON command as the engine takes it.
pub fn parse_command(json: &str) -> Result<Command, BridgeError> {
    serde_json::from_str(json).map_err(|e| BridgeError::Command { reason: e.to_string() })
}

/// The app's secret store, as the engine asks it.
struct Secrets(Arc<dyn NativeSecrets>);

impl SecretStore for Secrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        self.0.get(key.to_owned()).map_err(|e| SecretError(e.to_string()))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.0.set(key.to_owned(), value.to_owned()).map_err(|e| SecretError(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.0.delete(key.to_owned()).map_err(|e| SecretError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use matrix_core::{AccountId, MessageContent, Outgoing, TimelineKey};

    use super::*;

    #[test]
    fn a_send_command_should_parse_from_its_json() {
        let json = r#"{"type":"Send","data":{"account":"@a:example.org","key":{"room_id":"!r:example.org","thread":null},"reply_to":null,"message":{"type":"Text","data":{"body":"hi","html":null}}}}"#;
        let expected = Command::Send {
            account: AccountId::new("@a:example.org"),
            key: TimelineKey::room("!r:example.org"),
            reply_to: None,
            message: Outgoing::Text { body: "hi".into(), html: None, mentions: Default::default() },
        };
        assert_eq!(parse_command(json).unwrap(), expected);
    }

    #[test]
    fn a_command_without_fields_should_parse_from_its_name() {
        assert_eq!(parse_command(r#"{"type":"HangUp"}"#).unwrap(), Command::HangUp);
    }

    #[test]
    fn json_that_is_no_command_should_be_refused() {
        assert!(matches!(parse_command(r#"{"type":"Nonsense"}"#), Err(BridgeError::Command { .. })));
    }

    #[test]
    fn message_content_should_carry_its_kind_as_a_tag() {
        let text = MessageContent::Text { body: "hi".into(), html: None };
        assert_eq!(serde_json::to_string(&text).unwrap(), r#"{"type":"Text","data":{"body":"hi","html":null}}"#);
    }
}
