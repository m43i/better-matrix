//! Events on their way to the app: serialised to JSON on a thread of their own, with media bytes
//! written to files first.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;

use matrix_core::{Event, EventSink, MediaRef, MediaSize, RequestId};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::EventListener;

/// The bridge's own events: what it reports in place of an engine event it can't pass as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "type", content = "data")]
pub enum BridgeEvent {
    /// Media the app fetched (`Event::MediaLoaded`), as a file.
    MediaFile {
        /// The `FetchMedia` request it answers.
        request: RequestId,
        /// Which media.
        media: MediaRef,
        /// Which rendition.
        size: MediaSize,
        /// The file, an absolute path.
        path: String,
    },
    /// A linked GIF (`Event::GifLoaded`), as a file.
    GifFile {
        /// The link it was fetched for.
        url: String,
        /// The file, an absolute path.
        path: String,
    },
    /// Media arrived but couldn't be written to the cache.
    MediaFailed {
        /// The `FetchMedia` request it answers, when it was media.
        request: Option<RequestId>,
        /// Why.
        error: String,
    },
}

/// The engine's sink: hands events to the forwarding thread.
pub(crate) struct Forwarder(mpsc::Sender<Event>);

impl EventSink for Forwarder {
    fn emit(&self, event: Event) {
        // The thread only stops with the process; a send can't fail before that.
        let _ = self.0.send(event);
    }
}

impl Forwarder {
    /// Starts the thread that turns events into JSON for `listener`.
    pub(crate) fn start(cache_dir: PathBuf, listener: Arc<dyn EventListener>) -> std::io::Result<Self> {
        let (sender, events) = mpsc::channel::<Event>();
        std::thread::Builder::new().name("bridge-events".into()).spawn(move || {
            for event in events {
                if let Some(json) = to_json(&cache_dir, event) {
                    listener.on_event(json);
                }
            }
        })?;
        Ok(Self(sender))
    }
}

/// One event as the app receives it.
pub(crate) fn to_json(cache_dir: &Path, event: Event) -> Option<String> {
    let json = match event {
        Event::MediaLoaded { request, media, size, bytes } => {
            let bridged = match write_media(cache_dir, &media, size, &bytes) {
                Ok(path) => BridgeEvent::MediaFile { request, media, size, path: path_string(&path) },
                Err(error) => BridgeEvent::MediaFailed { request: Some(request), error: error.to_string() },
            };
            serde_json::to_string(&bridged)
        }
        Event::GifLoaded { url, bytes } => {
            let bridged = match write_file(&cache_dir.join("gifs"), &key_of(&url), "gif", &bytes) {
                Ok(path) => BridgeEvent::GifFile { url, path: path_string(&path) },
                Err(error) => BridgeEvent::MediaFailed { request: None, error: error.to_string() },
            };
            serde_json::to_string(&bridged)
        }
        event => serde_json::to_string(&event),
    };
    match json {
        Ok(json) => Some(json),
        Err(error) => {
            tracing::error!("an event didn't serialise: {error}");
            None
        }
    }
}

/// Writes fetched media to the cache, named by what it is, so a second fetch reuses the name.
fn write_media(cache_dir: &Path, media: &MediaRef, size: MediaSize, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let name = key_of(&format!("{media:?}/{size:?}"));
    write_file(&cache_dir.join("media"), &name, extension(bytes), bytes)
}

/// Writes `bytes` as `<dir>/<name>.<extension>` through a temporary file, so a reader never sees
/// half a file.
fn write_file(dir: &Path, name: &str, extension: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.{extension}"));
    if path.exists() {
        return Ok(path);
    }
    let partial = dir.join(format!("{name}.part"));
    std::fs::File::create(&partial)?.write_all(bytes)?;
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

/// A file name for a cache key: its SHA-256, in hex.
fn key_of(key: &str) -> String {
    Sha256::digest(key.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// The extension the platform's image decoders expect, by the file's first bytes.
fn extension(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xFF, 0xD8, 0xFF, ..] => "jpg",
        [b'G', b'I', b'F', b'8', ..] => "gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        [b'%', b'P', b'D', b'F', ..] => "pdf",
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => "mp4",
        [b'O', b'g', b'g', b'S', ..] => "ogg",
        _ => "bin",
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use matrix_core::AccountId;

    use super::*;

    fn png() -> std::sync::Arc<[u8]> {
        std::sync::Arc::from(&[0x89, b'P', b'N', b'G', 0, 1, 2][..])
    }

    #[test]
    fn loaded_media_should_reach_the_app_as_a_file() {
        let cache = tempfile::tempdir().unwrap();
        let event = Event::MediaLoaded {
            request: RequestId(4),
            media: MediaRef::opaque("mxc://example.org/a"),
            size: MediaSize::Original,
            bytes: png(),
        };
        let json: serde_json::Value = serde_json::from_str(&to_json(cache.path(), event).unwrap()).unwrap();
        assert_eq!(json["type"], "MediaFile");
        let path = json["data"]["path"].as_str().unwrap();
        assert!(path.ends_with(".png"));
        assert_eq!(std::fs::read(path).unwrap(), png().to_vec());
    }

    #[test]
    fn the_same_media_should_reuse_its_file() {
        let cache = tempfile::tempdir().unwrap();
        let media = MediaRef::opaque("mxc://example.org/a");
        let first = write_media(cache.path(), &media, MediaSize::PREVIEW, &png()).unwrap();
        let second = write_media(cache.path(), &media, MediaSize::PREVIEW, &png()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn other_sizes_of_the_same_media_should_get_their_own_files() {
        let cache = tempfile::tempdir().unwrap();
        let media = MediaRef::opaque("mxc://example.org/a");
        let original = write_media(cache.path(), &media, MediaSize::Original, &png()).unwrap();
        let preview = write_media(cache.path(), &media, MediaSize::PREVIEW, &png()).unwrap();
        assert_ne!(original, preview);
    }

    #[test]
    fn a_linked_gif_should_reach_the_app_as_a_file() {
        let cache = tempfile::tempdir().unwrap();
        let event = Event::GifLoaded { url: "https://example.org/a.gif".into(), bytes: Arc::from(&b"GIF89a"[..]) };
        let json: serde_json::Value = serde_json::from_str(&to_json(cache.path(), event).unwrap()).unwrap();
        assert_eq!(json["type"], "GifFile");
        assert!(json["data"]["path"].as_str().unwrap().ends_with(".gif"));
    }

    #[test]
    fn other_events_should_pass_as_the_engines_json() {
        let cache = tempfile::tempdir().unwrap();
        let event = Event::AccountRemoved(AccountId::new("@a:example.org"));
        assert_eq!(to_json(cache.path(), event).unwrap(), r#"{"type":"AccountRemoved","data":"@a:example.org"}"#);
    }

    #[test]
    fn a_finished_command_should_carry_its_result() {
        let cache = tempfile::tempdir().unwrap();
        let event = Event::Finished { request: RequestId(2), result: Err("no".into()) };
        assert_eq!(
            to_json(cache.path(), event).unwrap(),
            r#"{"type":"Finished","data":{"request":2,"result":{"Err":"no"}}}"#
        );
    }
}
