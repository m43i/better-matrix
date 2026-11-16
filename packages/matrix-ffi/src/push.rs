//! Pushes handled without the engine: one account's store opened from the data directory, the
//! event fetched and decrypted, the store closed again (`matrix_core::push`, ADR 0010).
//!
//! Two ways in. Android's headless push task (JavaScript, while the app isn't running) calls
//! [`fetch_push`] through UniFFI, with the app's secret store. iOS's Notification Service
//! Extension (Swift, its own process) calls the C entry point [`bm_push_fetch`] with a payload
//! as APNs delivered it and its own keychain functions; it is declared for Swift in
//! `apps/mobile/plugins/push/ios/BetterMatrixPush.h`.

#![expect(unsafe_code, reason = "the Notification Service Extension's entry point is a C function")]

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use matrix_core::push::fetch_without_engine;
use matrix_core::{PushContent, PushTarget, SecretError, SecretStore};
use serde::{Deserialize, Serialize};

use crate::{BridgeError, NativeSecrets, Secrets};

/// Hears the answer to [`fetch_push`]: a JSON `PushContent`.
#[uniffi::export(with_foreign)]
pub trait PushListener: Send + Sync {
    /// The push's content, once fetched (or given up on).
    fn on_content(&self, content: String);
}

/// Fetches what a push is about without the engine, on a thread of its own; `listener` hears
/// the answer, a JSON `PushContent`. `target` is a JSON `PushTarget`; `data_dir` the engine's.
/// Never call it for an account the running engine has open in this JavaScript context: send
/// `FetchNotification` to the engine instead.
#[uniffi::export]
pub fn fetch_push(
    data_dir: String,
    target: String,
    secrets: Arc<dyn NativeSecrets>,
    listener: Arc<dyn PushListener>,
) -> Result<(), BridgeError> {
    let target: PushTarget =
        serde_json::from_str(&target).map_err(|e| BridgeError::Command { reason: e.to_string() })?;
    std::thread::Builder::new()
        .name("push".into())
        .spawn(move || {
            let content = fetch_blocking(Path::new(&data_dir), Arc::new(Secrets(secrets)), &target);
            match serde_json::to_string(&content) {
                Ok(json) => listener.on_content(json),
                Err(error) => tracing::error!("a push's content didn't serialise: {error}"),
            }
        })
        .map_err(|e| BridgeError::Start { reason: e.to_string() })?;
    Ok(())
}

/// [`fetch_without_engine`] on a runtime of its own, for a thread that may block.
fn fetch_blocking(data_dir: &Path, secrets: Arc<dyn SecretStore>, target: &PushTarget) -> PushContent {
    matrix_core::push::install_tls();
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!("no runtime for a push: {error}");
            return matrix_core::push::hidden(target);
        }
    };
    let content = runtime
        .block_on(fetch_without_engine(data_dir, secrets, target))
        .unwrap_or_else(|_| matrix_core::push::hidden(target));
    // Blocking work still running (a keychain call) needn't hold the push up.
    runtime.shutdown_background();
    content
}

// ----- iOS: the Notification Service Extension ----------------------------------------------

/// The file the app keeps fresh while it is in the foreground (`apps/mobile/src/engine/push`),
/// holding the time it last did, in milliseconds since the epoch.
pub const ACTIVE_FILE: &str = "app-active";

/// How long after its last word the app counts as in the foreground: it writes every 30 s.
const ACTIVE_FOR: Duration = Duration::from_secs(75);

/// The texts a notification shown by the extension uses, in the user's language: written by the
/// app (`push-strings.json` in the data directory) from its catalogs. English when missing.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PushStrings {
    /// The app's name: the title when the room isn't known.
    pub app_name: String,
    /// A message with its sender, `{{sender}}` and `{{body}}` (the desktop's `overview.preview`).
    pub preview: String,
    /// What a push says when its event can't be read.
    pub hidden: String,
    /// An invite, `{{sender}}`.
    pub invite: String,
}

impl Default for PushStrings {
    fn default() -> Self {
        Self {
            app_name: "Better Matrix".into(),
            preview: "{{sender}}: {{body}}".into(),
            hidden: "New message".into(),
            invite: "{{sender}} invited you".into(),
        }
    }
}

/// The file the app writes the strings to.
pub const STRINGS_FILE: &str = "push-strings.json";

/// What the extension shows for one push.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct NativePush {
    /// What the push names; `None` for a push that names no event.
    pub target: Option<PushTarget>,
    /// The app is in the foreground and shows the event itself: leave the push as it came
    /// (the app keeps it from showing).
    pub app_active: bool,
    /// The notification to show, when there is one; `None` keeps the push's own text (the
    /// generic fallback), since the extension can't drop a push.
    pub shown: Option<Shown>,
}

/// A notification's text.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Shown {
    /// The title: the room.
    pub title: String,
    /// The text.
    pub body: String,
    /// Groups a room's notifications (`account|room`, as the app's own notifications do).
    pub thread: String,
}

/// The text of a notification for `content`, as `apps/mobile/src/engine/notificationRules.ts`
/// words it: the room as title; the message with its sender, unless the room is named after
/// the sender (a direct chat).
pub fn shown(content: &PushContent, strings: &PushStrings) -> Option<Shown> {
    let fill = |template: &str, pairs: &[(&str, &str)]| {
        pairs.iter().fold(template.to_owned(), |text, (name, value)| text.replace(&format!("{{{{{name}}}}}"), value))
    };
    let thread = |account: &matrix_core::AccountId, room: &str| format!("{account}|{room}");
    match content {
        PushContent::Message(n) => Some(Shown {
            title: n.room_name.clone(),
            body: if n.sender_name == n.room_name {
                n.body.clone()
            } else {
                fill(&strings.preview, &[("sender", &n.sender_name), ("body", &n.body)])
            },
            thread: thread(&n.account, &n.room_id),
        }),
        PushContent::Invite { account, room_id, room_name, sender_name, .. } => Some(Shown {
            title: room_name.clone(),
            body: fill(&strings.invite, &[("sender", sender_name)]),
            thread: thread(account, room_id),
        }),
        PushContent::Hidden { account, room_id, room_name, .. } => Some(Shown {
            title: room_name.clone().unwrap_or_else(|| strings.app_name.clone()),
            body: strings.hidden.clone(),
            thread: thread(account, room_id),
        }),
        PushContent::Nothing => None,
    }
}

/// Whether the app said it is in the foreground lately, from [`ACTIVE_FILE`]'s contents.
fn app_active(marker: Option<&str>, now: SystemTime) -> bool {
    let Some(at) = marker.and_then(|text| text.trim().parse::<u64>().ok()) else { return false };
    let now = now.duration_since(UNIX_EPOCH).map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    now.saturating_sub(at) < u64::try_from(ACTIVE_FOR.as_millis()).unwrap_or(u64::MAX)
}

/// Handles one push as the extension received it (`payload`: the notification's `userInfo` as
/// JSON). While the app is in the foreground it leaves the push alone: the app's own sync
/// notifies, and two processes would only race on the store.
pub fn handle_native(payload: &str, data_dir: &Path, secrets: Arc<dyn SecretStore>, now: SystemTime) -> NativePush {
    let target = serde_json::from_str(payload).ok().and_then(|payload| PushTarget::parse(&payload));
    let marker = std::fs::read_to_string(data_dir.join(ACTIVE_FILE)).ok();
    let active = app_active(marker.as_deref(), now);
    let target = match target {
        Some(target) if !active => target,
        target => return NativePush { target, app_active: active, shown: None },
    };
    let strings = std::fs::read(data_dir.join(STRINGS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let content = fetch_blocking(data_dir, secrets, &target);
    NativePush { shown: shown(&content, &strings), target: Some(target), app_active: false }
}

/// A secret store call into the extension: `key` as the keychain names it (see [`store_key`]).
/// `get` sets `*value` to a string allocated with `malloc` (which Rust frees), or leaves it
/// null when there is none; each returns false when the keychain failed.
pub type SecretGetFn = extern "C" fn(context: *mut c_void, key: *const c_char, value: *mut *mut c_char) -> bool;
/// Stores `value` under `key`.
pub type SecretSetFn = extern "C" fn(context: *mut c_void, key: *const c_char, value: *const c_char) -> bool;
/// Removes `key`.
pub type SecretDeleteFn = extern "C" fn(context: *mut c_void, key: *const c_char) -> bool;

/// The extension's keychain, as C functions over its context.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BmSecrets {
    /// Passed back to every call.
    pub context: *mut c_void,
    /// Reads a secret.
    pub get: SecretGetFn,
    /// Writes one.
    pub set: SecretSetFn,
    /// Removes one.
    pub delete: SecretDeleteFn,
}

// The context is the extension's keychain wrapper, which the keychain lets any thread use; it
// outlives the call to `bm_push_fetch`, the only time it is used.
unsafe impl Send for BmSecrets {}
unsafe impl Sync for BmSecrets {}

unsafe extern "C" {
    fn free(pointer: *mut c_void);
}

impl SecretStore for BmSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        let key = c_key(key)?;
        let mut value: *mut c_char = std::ptr::null_mut();
        if !(self.get)(self.context, key.as_ptr(), &raw mut value) {
            return Err(SecretError("the keychain refused".into()));
        }
        if value.is_null() {
            return Ok(None);
        }
        // SAFETY: a NUL-terminated string `get` allocated with malloc and hands over.
        let text = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
        // SAFETY: allocated with malloc, freed once.
        unsafe { free(value.cast()) };
        Ok(Some(text))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let (key, value) = (c_key(key)?, CString::new(value).map_err(|e| SecretError(e.to_string()))?);
        if (self.set)(self.context, key.as_ptr(), value.as_ptr()) {
            Ok(())
        } else {
            Err(SecretError("the keychain refused".into()))
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let key = c_key(key)?;
        if (self.delete)(self.context, key.as_ptr()) { Ok(()) } else { Err(SecretError("the keychain refused".into())) }
    }
}

fn c_key(key: &str) -> Result<CString, SecretError> {
    CString::new(store_key(key)).map_err(|e| SecretError(e.to_string()))
}

/// The keychain key the app stores `key` under (`apps/mobile/src/engine/secrets.ts`'s
/// `storeKey`, which expo-secure-store needs): letters, digits, `.` and `-` stay, every other
/// byte becomes `_` and two hex digits.
pub fn store_key(key: &str) -> String {
    key.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-' {
                char::from(byte).to_string()
            } else {
                format!("_{byte:02x}")
            }
        })
        .collect()
}

/// The Notification Service Extension's way in: handles the push whose `userInfo` is
/// `payload` (JSON) with the engine's data in `data_dir` and the keychain behind `secrets`.
/// Blocks for up to about 25 seconds. Returns a JSON [`NativePush`] to free with
/// [`bm_push_free`], or null when an argument isn't a string.
///
/// # Safety
/// `payload` and `data_dir` must be NUL-terminated UTF-8 strings; `secrets`' functions must
/// stay callable from any thread until this returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bm_push_fetch(
    payload: *const c_char,
    data_dir: *const c_char,
    secrets: BmSecrets,
) -> *mut c_char {
    if payload.is_null() || data_dir.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the caller passes NUL-terminated strings.
    let (payload, data_dir) = unsafe { (CStr::from_ptr(payload), CStr::from_ptr(data_dir)) };
    let (Ok(payload), Ok(data_dir)) = (payload.to_str(), data_dir.to_str()) else { return std::ptr::null_mut() };
    let push = handle_native(payload, &PathBuf::from(data_dir), Arc::new(secrets), SystemTime::now());
    serde_json::to_string(&push)
        .ok()
        .and_then(|json| CString::new(json).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Frees what [`bm_push_fetch`] returned.
///
/// # Safety
/// `json` must come from `bm_push_fetch`, freed once (null is ignored).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bm_push_free(json: *mut c_char) {
    if !json.is_null() {
        // SAFETY: made by `CString::into_raw` in `bm_push_fetch`.
        drop(unsafe { CString::from_raw(json) });
    }
}

#[cfg(test)]
mod tests {
    use matrix_core::{AccountId, MemorySecrets, Notification};

    use super::*;

    fn account() -> AccountId {
        AccountId::new("@ada:example.org")
    }

    fn message(sender: &str, room: &str) -> PushContent {
        PushContent::Message(Notification {
            account: account(),
            room_id: "!r".into(),
            room_name: room.into(),
            event_id: "$e".into(),
            sender_name: sender.into(),
            body: "hi".into(),
        })
    }

    #[test]
    fn a_message_should_be_titled_by_its_room_and_name_its_sender() {
        let shown = shown(&message("Grace", "Team"), &PushStrings::default()).unwrap();
        assert_eq!((shown.title.as_str(), shown.body.as_str()), ("Team", "Grace: hi"));
        assert_eq!(shown.thread, "@ada:example.org|!r");
    }

    #[test]
    fn a_direct_chat_named_after_the_sender_should_not_repeat_the_name() {
        assert_eq!(shown(&message("Grace", "Grace"), &PushStrings::default()).unwrap().body, "hi");
    }

    #[test]
    fn an_unreadable_event_should_say_something_generic_in_the_users_language() {
        let strings = PushStrings { hidden: "Neue Nachricht".into(), ..PushStrings::default() };
        let hidden =
            PushContent::Hidden { account: account(), room_id: "!r".into(), room_name: None, event_id: "$e".into() };
        let shown = shown(&hidden, &strings).unwrap();
        assert_eq!((shown.title.as_str(), shown.body.as_str()), ("Better Matrix", "Neue Nachricht"));
    }

    #[test]
    fn an_invite_should_name_who_invited() {
        let invite = PushContent::Invite {
            account: account(),
            room_id: "!r".into(),
            room_name: "Team".into(),
            event_id: "$e".into(),
            sender_name: "Grace".into(),
        };
        assert_eq!(shown(&invite, &PushStrings::default()).unwrap().body, "Grace invited you");
    }

    #[test]
    fn nothing_should_keep_the_pushes_own_text() {
        assert_eq!(shown(&PushContent::Nothing, &PushStrings::default()), None);
    }

    #[test]
    fn partial_strings_should_fill_in_english() {
        let strings: PushStrings = serde_json::from_str(r#"{"hidden":"Nouveau message"}"#).unwrap();
        assert_eq!((strings.hidden.as_str(), strings.app_name.as_str()), ("Nouveau message", "Better Matrix"));
    }

    fn at(ms: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(ms)
    }

    #[test]
    fn an_app_that_wrote_lately_should_count_as_in_front() {
        assert!(app_active(Some("1000000"), at(1_030_000)));
    }

    #[test]
    fn an_app_that_stopped_writing_should_not_count_as_in_front() {
        assert!(!app_active(Some("1000000"), at(1_100_000)));
        assert!(!app_active(None, at(1)));
        assert!(!app_active(Some("garbage"), at(1)));
    }

    #[test]
    fn a_push_while_the_app_is_in_front_should_be_left_alone() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(ACTIVE_FILE), "1000000").unwrap();
        let payload = r#"{"body":{"account":"@ada:example.org","room_id":"!r","event_id":"$e"}}"#;
        let push = handle_native(payload, data.path(), Arc::new(MemorySecrets::default()), at(1_010_000));
        assert!(push.app_active && push.shown.is_none() && push.target.is_some());
    }

    #[test]
    fn a_push_for_an_account_not_on_this_device_should_keep_its_own_text() {
        let data = tempfile::tempdir().unwrap();
        let payload = r#"{"body":{"account":"@ada:example.org","room_id":"!r","event_id":"$e"}}"#;
        let push = handle_native(payload, data.path(), Arc::new(MemorySecrets::default()), at(1));
        assert_eq!(push.shown, None);
    }

    #[test]
    fn keys_should_be_escaped_as_the_app_escapes_them() {
        assert_eq!(store_key("a.B-9"), "a.B-9");
        assert_eq!(store_key("s/@a:x"), "s_2f_40a_3ax");
        assert_ne!(store_key("_40"), store_key("@"));
    }

    extern "C" fn get_none(_: *mut c_void, _: *const c_char, _: *mut *mut c_char) -> bool {
        true
    }
    extern "C" fn set_ok(_: *mut c_void, _: *const c_char, _: *const c_char) -> bool {
        true
    }
    extern "C" fn delete_ok(_: *mut c_void, _: *const c_char) -> bool {
        true
    }

    #[test]
    fn the_c_entry_point_should_answer_json_that_the_extension_frees() {
        let data = tempfile::tempdir().unwrap();
        let payload = CString::new(r#"{"aps":{"badge":1}}"#).unwrap();
        let dir = CString::new(data.path().to_str().unwrap()).unwrap();
        let secrets = BmSecrets { context: std::ptr::null_mut(), get: get_none, set: set_ok, delete: delete_ok };
        // SAFETY: valid strings; the functions are plain Rust.
        let json = unsafe { bm_push_fetch(payload.as_ptr(), dir.as_ptr(), secrets) };
        assert!(!json.is_null());
        // SAFETY: just returned.
        let text = unsafe { CStr::from_ptr(json) }.to_str().unwrap().to_owned();
        // SAFETY: from bm_push_fetch, once.
        unsafe { bm_push_free(json) };
        assert_eq!(text, r#"{"target":null,"app_active":false,"shown":null}"#);
    }
}
