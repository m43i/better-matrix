//! Everything that talks to the operating system on the app's behalf: picking files, posting
//! notifications, the taskbar badge, and the keychain. State code calls [`Shell`]; it never touches the OS itself,
//! so tests run it headless.

mod attachments;
mod files;
pub mod launch;
mod notify;
mod secrets;
pub mod sound;
mod taskbar;
mod tray;
pub mod updates;
pub mod video;
pub mod widget_window;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};

use rune::AppHandle;
use rune::prefs::Prefs;

pub use notify::Toast;
pub use secrets::{system_keychain, system_secrets};
pub use taskbar::Taskbar;

use crate::i18n::Language;
use crate::state::{
    Appearance, Emoticons, MessageLayout, PlaybackSpeed, RecentStickers, Settings, State, Visits, Zoom,
};

/// The OS services the app uses, or inert stand-ins when headless.
pub struct Shell {
    app: Option<AppHandle<State>>,
    updates: Option<std::sync::mpsc::Sender<updates::Release>>,
    notifier: Box<dyn notify::Notifier>,
    taskbar: Taskbar,
    /// The user can reach the app here while its window is closed.
    tray: Option<tray::Tray>,
    /// Whether the system shows the jump list: macOS only (see `taskbar`'s notes on Windows).
    /// Headless, as on macOS, so tests see it.
    offers_jump_list: bool,
    prefs: Arc<Mutex<Prefs>>,
    /// Counts document renders: a render keeps going while it is the latest and not stopped.
    document: Arc<std::sync::atomic::AtomicU64>,
    /// Counts settings saves: only the latest writes, so quick changes can't land out of order.
    saves: Arc<std::sync::atomic::AtomicU64>,
    /// Finish the latest settings write even if the user quits straight after making a change.
    settings_save: Option<std::thread::JoinHandle<()>>,
    /// Counts saves of the rooms opened, which go apart from the settings.
    visit_saves: Arc<std::sync::atomic::AtomicU64>,
    sticker_saves: Arc<std::sync::atomic::AtomicU64>,
    /// The system keychain alone (no files standing in for it): the user's GIPHY key lives
    /// there. None headless.
    secrets: Option<Arc<dyn matrix_core::SecretStore>>,
    /// Counts GIPHY key saves, and keeps them one at a time: only the latest writes.
    key_saves: Arc<(std::sync::atomic::AtomicU64, Mutex<()>)>,
    /// Where files made for a moment go (audio to play, voice memos being recorded): private to
    /// the user, and emptied at launch and exit.
    scratch: std::path::PathBuf,
    /// Where a later launch of the app leaves its permalinks for this process
    /// ([`launch`]); none headless.
    handoff: Option<std::path::PathBuf>,
    /// The widget windows open, each in a process of its own; headless, none (what was asked is
    /// kept).
    widget_windows: widget_window::Windows,
}

impl std::fmt::Debug for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell").field("windowed", &self.app.is_some()).finish_non_exhaustive()
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.tray = None;
        if let Some(save) = self.settings_save.take() {
            let _ = save.join();
        }
    }
}

impl Shell {
    /// The real services, delivering results to the app through `app`.
    /// `taskbar` is the one `main` attaches the window to.
    pub fn native(app: AppHandle<State>, prefs: Prefs, scratch: std::path::PathBuf, taskbar: Taskbar) -> Self {
        video::prepare();
        let notifier = notify::system(app.clone());
        Self {
            updates: updates::start(app.clone(), scratch.with_file_name("updates")),
            widget_windows: widget_window::Windows::native(app.clone()),
            app: Some(app),
            notifier,
            taskbar,
            tray: None,
            offers_jump_list: cfg!(target_os = "macos"),
            prefs: Arc::new(Mutex::new(prefs)),
            document: Arc::default(),
            saves: Arc::default(),
            settings_save: None,
            visit_saves: Arc::default(),
            sticker_saves: Arc::default(),
            secrets: None,
            key_saves: Arc::default(),
            scratch,
            handoff: None,
        }
    }

    /// No OS at all: dialogs never answer, notifications go nowhere, settings stay in memory.
    pub fn headless() -> Self {
        Self {
            app: None,
            updates: None,
            notifier: Box::new(notify::Silent),
            taskbar: Taskbar::default(),
            tray: None,
            offers_jump_list: true,
            prefs: Arc::new(Mutex::new(Prefs::memory())),
            document: Arc::default(),
            saves: Arc::default(),
            settings_save: None,
            visit_saves: Arc::default(),
            sticker_saves: Arc::default(),
            secrets: None,
            key_saves: Arc::default(),
            scratch: std::env::temp_dir(),
            handoff: None,
            widget_windows: widget_window::Windows::headless(),
        }
    }

    /// Like [`Shell::headless`], but keeps every notification it is asked to show.
    #[cfg(test)]
    pub fn recording() -> (Self, std::sync::Arc<std::sync::Mutex<Vec<Toast>>>) {
        let posted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let prefs = Arc::new(Mutex::new(Prefs::memory()));
        (
            Self {
                app: None,
                updates: None,
                notifier: Box::new(notify::Recording(posted.clone())),
                taskbar: Taskbar::default(),
                tray: None,
                offers_jump_list: true,
                prefs,
                document: Arc::default(),
                saves: Arc::default(),
                settings_save: None,
                visit_saves: Arc::default(),
                sticker_saves: Arc::default(),
                secrets: None,
                key_saves: Arc::default(),
                scratch: std::env::temp_dir(),
                handoff: None,
                widget_windows: widget_window::Windows::headless(),
            },
            posted,
        )
    }

    /// Opens the system file dialog; its answer arrives through [`State::files_picked`].
    pub fn pick_files(&self, picked_for: crate::state::PickFor) {
        if let Some(app) = &self.app {
            files::pick(app.clone(), picked_for);
        }
    }

    /// Renders a PDF's pages on a worker; they arrive through [`State::document_page`], then
    /// [`State::document_rendered`], each with the render's number, which this returns: what an
    /// earlier render of the same file still sends is told apart.
    pub fn render_document(&self, bytes: std::sync::Arc<[u8]>) -> u64 {
        /// More pages than anyone reads in a preview; the file itself has them all.
        const MAX_PAGES: usize = 60;
        use std::sync::atomic::Ordering;
        let render = self.document.fetch_add(1, Ordering::Relaxed) + 1;
        let Some(app) = self.app.clone() else { return render };
        let latest = Arc::clone(&self.document);
        let spawned = std::thread::Builder::new().name("pdf".into()).spawn(move || {
            let keep_going = || latest.load(Ordering::Relaxed) == render;
            let result = crate::state::render_document(&bytes, MAX_PAGES, keep_going, |page, size| {
                app.update(move |s: &mut State, _| s.document_page(render, page, size));
            });
            app.update(move |s: &mut State, _| s.document_rendered(render, result));
        });
        if let Err(error) = spawned {
            eprintln!("better-matrix: could not render the document: {error}");
        }
        render
    }

    /// Keeps the user's GIPHY key in `secrets`: the system keychain ([`system_keychain`]).
    pub fn with_secrets(mut self, secrets: Arc<dyn matrix_core::SecretStore>) -> Self {
        self.secrets = Some(secrets);
        self
    }

    /// Reads the user's GIPHY key from the keychain off the UI thread; it arrives through
    /// [`State::gif_key_loaded`]. False when there is no keychain to ask (headless).
    pub fn load_gif_key(&self) -> bool {
        let (Some(app), Some(secrets)) = (self.app.clone(), self.secrets.clone()) else { return false };
        let spawned = std::thread::Builder::new().name("giphy-key".into()).spawn(move || {
            let key = match secrets.get(GIPHY_SECRET) {
                Ok(key) => key,
                Err(error) => {
                    eprintln!("better-matrix: could not read the GIPHY key: {error}");
                    None
                }
            };
            app.update(move |s: &mut State, _| s.gif_key_loaded(key));
        });
        if spawned.is_err() {
            eprintln!("better-matrix: could not read the GIPHY key");
            return false;
        }
        true
    }

    /// Keeps the user's GIPHY key in the keychain (`None` removes it), off the UI thread; of
    /// quick changes, the latest writes. A failure comes back through
    /// [`State::gif_key_not_kept`] with the key asked for and `previous`, the key kept before.
    pub fn save_gif_key(&self, key: Option<String>, previous: Option<String>) {
        use std::sync::atomic::Ordering;
        let Some(secrets) = self.secrets.clone() else { return };
        let (app, saves) = (self.app.clone(), Arc::clone(&self.key_saves));
        let save = saves.0.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = std::thread::Builder::new().name("save-giphy-key".into()).spawn(move || {
            let Ok(_one_at_a_time) = saves.1.lock() else { return };
            if saves.0.load(Ordering::Relaxed) != save {
                return;
            }
            let written = match &key {
                Some(key) => secrets.set(GIPHY_SECRET, key),
                None => secrets.delete(GIPHY_SECRET),
            };
            if let (Err(error), Some(app)) = (written, app) {
                let error = error.to_string();
                app.update(move |s: &mut State, _| s.gif_key_not_kept(key, previous, &error));
            }
        });
    }

    /// Reads the permalinks later launches leave in `handoff` (see [`launch`]).
    pub fn with_handoff(mut self, handoff: std::path::PathBuf) -> Self {
        self.handoff = Some(handoff);
        self
    }

    /// Takes the permalinks a later launch handed on, off the UI thread; each opens through
    /// [`State::open_permalink`].
    pub fn take_handed_links(&self) {
        let (Some(app), Some(handoff)) = (self.app.clone(), self.handoff.clone()) else { return };
        let spawned = std::thread::Builder::new().name("handed-links".into()).spawn(move || {
            let links = launch::take(&handoff);
            if !links.is_empty() {
                app.update(move |s: &mut State, _| links.iter().for_each(|link| s.open_permalink(link)));
            }
        });
        if let Err(error) = spawned {
            eprintln!("better-matrix: could not read the links handed on: {error}");
        }
    }

    /// The folder for files made for a moment.
    pub fn scratch(&self) -> &std::path::Path {
        &self.scratch
    }

    /// Stops the document render under way, if any: its preview closed.
    pub fn stop_document(&self) {
        self.document.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Asks where to save a file named `name`; `chosen` runs with the path once picked.
    pub fn pick_save_path(&self, name: &str, chosen: impl FnOnce(&mut State, std::path::PathBuf) + Send + 'static) {
        if let Some(app) = &self.app {
            files::save(app.clone(), name.to_owned(), chosen);
        }
    }

    /// Saves this device's settings, off the UI thread (the prefs file is rewritten whole).
    pub fn save_settings(&mut self, settings: Settings) {
        self.settings_save = save(&self.prefs, &self.saves, "settings", move |prefs| write_settings(prefs, &settings));
    }

    /// Keep the closed window reachable. Linux finishes registration off the UI thread and
    /// quits through the app handle if its tray is unavailable while the window is closed.
    pub fn close_window(&mut self, close_to_tray: bool, tx: &mut rune::Tx) {
        self.tray = None;
        if close_to_tray && let Some(app) = &self.app {
            self.tray = tray::show(app.clone());
        }
        if self.tray.is_none() {
            tx.quit();
        }
    }

    pub fn dismiss_tray(&mut self) {
        self.tray = None;
    }

    /// Rune has now applied this turn's window requests and published the visibility.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn settle_tray(&mut self) {
        let (Some(app), Some(tray)) = (&self.app, &mut self.tray) else { return };
        if app.is_visible() {
            self.dismiss_tray();
        } else if !rune::quitting() && tray.unavailable() {
            app.quit();
        }
    }

    /// The rooms opened on this device, as last saved: read from the prefs already loaded.
    pub fn visits(&self) -> Visits {
        self.prefs.lock().ok().and_then(|prefs| prefs.get(VISITS)).unwrap_or_default()
    }

    /// Saves the rooms opened, off the UI thread, apart from the settings: one write per open.
    pub fn save_visits(&self, visits: Visits) {
        let _ = save(&self.prefs, &self.visit_saves, "visits", move |prefs| prefs.set(VISITS, &visits));
    }

    /// The stickers sent lately on this device, as last saved: read from the prefs already
    /// loaded.
    pub fn recent_stickers(&self) -> RecentStickers {
        self.prefs.lock().ok().and_then(|prefs| prefs.get(RECENT_STICKERS)).unwrap_or_default()
    }

    /// Saves the stickers sent lately, off the UI thread, apart from the settings.
    pub fn save_recent_stickers(&self, recent: RecentStickers) {
        let _ = save(&self.prefs, &self.sticker_saves, "stickers", move |prefs| prefs.set(RECENT_STICKERS, &recent));
    }

    /// A player for `media`'s file on the picked speaker; the view runs again once it is
    /// decoded, and if it can't play, the app hears it through [`State::audio_failed`]. Headless, Rune's player and its fake speaker.
    pub fn player(
        &self,
        path: std::path::PathBuf,
        speaker: Option<String>,
        media: matrix_core::MediaRef,
        clip: u64,
    ) -> sound::Player {
        if let Some(app) = self.app.clone() {
            // Decoded, the clip's card shows its length and waveform, even while it is paused.
            let decoded = {
                let app = app.clone();
                move || app.update(|_: &mut State, _| {})
            };
            let failed =
                move |error: String| app.update(move |s: &mut State, _| s.audio_failed(&media, Some(clip), error));
            sound::Player::Native(sound::NativePlayer::open(path, speaker, decoded, failed))
        } else {
            sound::Player::Rune(rune::Player::audio(path))
        }
    }

    /// A video player for a saved file on the picked speaker, playing at once; the view runs
    /// again when it loads, ends or fails. GStreamer's opens on a worker (its first start can
    /// wait on the plugin scan begun at launch) and arrives through [`State::video_opened`] with
    /// `visit`: `None` then. Rune's opens here. Headless, a stand-in that plays nothing and
    /// reports `dimensions` as its size.
    pub fn open_video(
        &self,
        path: std::path::PathBuf,
        speaker: Option<String>,
        dimensions: Option<(u32, u32)>,
        visit: u64,
    ) -> Option<video::Player> {
        let Some(app) = self.app.clone() else {
            return Some(video::Player::Detached(video::Detached::new(None, dimensions)));
        };
        let changed: rune::Waker = {
            let app = app.clone();
            Arc::new(move || app.update(|_: &mut State, _| {}))
        };
        if !video::OPENS_ON_A_WORKER {
            return Some(video::Player::open(path, speaker.as_deref(), changed));
        }
        let spawned = std::thread::Builder::new().name("video-open".into()).spawn(move || {
            let player = video::Player::open(path, speaker.as_deref(), changed);
            app.update(move |s: &mut State, _| s.video_opened(visit, player));
        });
        if let Err(error) = spawned {
            eprintln!("better-matrix: could not open the video: {error}");
        }
        None
    }

    /// A recording from the picked microphone; headless, Rune's recorder and its fake microphone.
    pub fn recorder(&self, path: std::path::PathBuf, microphone: Option<String>) -> sound::Recorder {
        if self.app.is_some() {
            sound::Recorder::Native(sound::NativeRecorder::to_file(path, microphone))
        } else {
            sound::Recorder::Rune(rune::Recorder::to_file(path))
        }
    }

    /// Stops a voice memo and writes its file: on a worker for the app's own recorder, whose
    /// result arrives through [`State::voice_memo_recorded`] (`None` here), at once otherwise.
    pub fn finish_recording(
        &self,
        recorder: sound::Recorder,
        memo: crate::state::MemoFor,
    ) -> Option<Result<sound::Recording, String>> {
        match (recorder, &self.app) {
            (sound::Recorder::Native(native), Some(app)) => {
                let app = app.clone();
                let spawned = std::thread::Builder::new().name("voice-memo".into()).spawn(move || {
                    let result = native.stop();
                    app.update(move |s: &mut State, tx| s.voice_memo_recorded(memo, result, tx));
                });
                spawned.err().map(|error| Err(error.to_string()))
            }
            (recorder, _) => Some(recorder.stop()),
        }
    }

    /// The ring of an incoming call on the picked speaker, until dropped; none headless.
    pub fn ring(&self, speaker: Option<String>) -> Option<sound::Ring> {
        self.app.is_some().then(|| sound::Ring::start(speaker))
    }

    /// The widget windows: running widgets' pages, beside the app.
    pub fn widget_windows(&mut self) -> &mut widget_window::Windows {
        &mut self.widget_windows
    }

    /// What was asked of the widget windows, headless.
    #[cfg(test)]
    pub fn widget_window_calls(&self) -> &[widget_window::WindowCall] {
        self.widget_windows.calls()
    }

    /// Shows a notification, and flashes the taskbar button. A click opens its room.
    pub fn notify(&self, toast: &Toast) {
        self.notifier.post(toast);
        self.taskbar.flash();
    }

    /// Brings the window forward, restoring it if minimised: a link from the system opened
    /// something in it.
    pub fn bring_forward(&self) -> bool {
        let Some(app) = &self.app else { return false };
        app.open();
        true
    }

    /// Shows `count` on the taskbar badge; 0 hides it.
    pub fn show_badge(&self, count: u32) {
        self.taskbar.show(count);
    }

    /// The count the taskbar badge shows.
    #[cfg(test)]
    pub fn badge(&self) -> u32 {
        self.taskbar.count()
    }

    /// Whether the system shows a jump list; elsewhere the rooms opened aren't even kept.
    pub fn offers_jump_list(&self) -> bool {
        self.offers_jump_list
    }

    /// Offers `list` in the Dock icon's menu (macOS); a pick opens its link. Headless, it is
    /// only kept.
    pub fn show_jump_list(&self, list: rune::JumpList) {
        self.taskbar.jump_list(list, self.app.is_some());
    }

    /// The jump list last offered.
    #[cfg(test)]
    pub fn jump_list(&self) -> rune::JumpList {
        self.taskbar.shown_jump_list()
    }
}

const LANGUAGE: &str = "language";
const APPEARANCE: &str = "appearance";
const MESSAGE_LAYOUT: &str = "message_layout";
const LINK_GIFS: &str = "link_gifs";
const LINK_PREVIEWS: &str = "link_previews";
const MAPS: &str = "maps";
const EMOTICONS: &str = "emoticons";
const ZOOM: &str = "zoom";
const CLOSE_TO_TRAY: &str = "close_to_tray";
const RING_SOUND: &str = "ring_sound";
const PLAYBACK_SPEED: &str = "playback_speed";
const MICROPHONE: &str = "microphone";
const SPEAKER: &str = "speaker";
const CAMERA: &str = "camera";
const LAST_ACCOUNT: &str = "last_account";
const HIDDEN_EVENTS: &str = "hidden_events";
const ROOM_CHANGES: &str = "room_changes";
const VISITS: &str = "visits";
/// The user's GIPHY key in the keychain.
const GIPHY_SECRET: &str = "giphy-api-key";
const RECENT_STICKERS: &str = "recent_stickers";

/// Runs `write` on the prefs on a worker of its own, unless a newer save counted by `saves`
/// comes first: that one writes instead, so quick changes can't land out of order.
fn save(
    prefs: &Arc<Mutex<Prefs>>,
    saves: &Arc<std::sync::atomic::AtomicU64>,
    what: &'static str,
    write: impl FnOnce(&mut Prefs) -> Result<(), rune::prefs::PrefsError> + Send + 'static,
) -> Option<std::thread::JoinHandle<()>> {
    use std::sync::atomic::Ordering;
    let (prefs, saves) = (Arc::clone(prefs), Arc::clone(saves));
    let save = saves.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::Builder::new()
        .name(format!("save-{what}"))
        .spawn(move || {
            let Ok(mut prefs) = prefs.lock() else { return };
            // A newer save is on its way: it writes instead.
            if saves.load(Ordering::Relaxed) != save {
                return;
            }
            if let Err(error) = write(&mut prefs) {
                eprintln!("better-matrix: could not save {what}: {error}");
            }
        })
        .inspect_err(|error| eprintln!("better-matrix: could not start the {what} save: {error}"))
        .ok()
}

fn write_settings(prefs: &mut Prefs, settings: &Settings) -> Result<(), rune::prefs::PrefsError> {
    prefs.set(CLOSE_TO_TRAY, &settings.close_to_tray)?;
    prefs.set(LANGUAGE, &settings.language.id())?;
    prefs.set(APPEARANCE, &settings.appearance.id())?;
    prefs.set(MESSAGE_LAYOUT, &settings.message_layout.id())?;
    prefs.set(ZOOM, &settings.zoom.percent())?;
    prefs.set(LINK_GIFS, &settings.show_link_gifs)?;
    prefs.set(LINK_PREVIEWS, &settings.show_link_previews)?;
    prefs.set(MAPS, &settings.show_maps)?;
    prefs.set(EMOTICONS, &settings.emoticons.id())?;
    prefs.set(RING_SOUND, &settings.ring_sound)?;
    prefs.set(PLAYBACK_SPEED, &settings.playback_speed.percent())?;
    prefs.set(MICROPHONE, &settings.devices.microphone)?;
    prefs.set(SPEAKER, &settings.devices.speaker)?;
    prefs.set(CAMERA, &settings.devices.camera)?;
    prefs.set(HIDDEN_EVENTS, &settings.show_hidden_events)?;
    prefs.set(ROOM_CHANGES, &settings.show_room_changes)?;
    prefs.set(LAST_ACCOUNT, &settings.last_account.as_ref().map(matrix_core::AccountId::as_str))
}

/// This device's saved settings, read at launch before the first frame (a small file).
pub fn load_settings(prefs: &Prefs) -> Settings {
    Settings {
        last_account: prefs.get::<String>(LAST_ACCOUNT).map(matrix_core::AccountId::new),
        language: prefs.get::<String>(LANGUAGE).map_or(Language::System, |id| Language::from_id(&id)),
        appearance: prefs.get::<String>(APPEARANCE).map_or(Appearance::System, |id| Appearance::from_id(&id)),
        message_layout: prefs
            .get::<String>(MESSAGE_LAYOUT)
            .map_or_else(MessageLayout::default, |id| MessageLayout::from_id(&id)),
        zoom: prefs.get::<u16>(ZOOM).map_or_else(Zoom::default, Zoom::from_percent),
        close_to_tray: prefs.get::<bool>(CLOSE_TO_TRAY).unwrap_or(true),
        show_link_gifs: prefs.get::<bool>(LINK_GIFS).unwrap_or(true),
        show_link_previews: prefs.get::<bool>(LINK_PREVIEWS).unwrap_or(true),
        show_maps: prefs.get::<bool>(MAPS).unwrap_or(true),
        emoticons: prefs.get::<String>(EMOTICONS).map_or_else(Emoticons::default, |id| Emoticons::from_id(&id)),
        devices: matrix_core::DeviceChoice {
            microphone: prefs.get::<Option<String>>(MICROPHONE).flatten(),
            speaker: prefs.get::<Option<String>>(SPEAKER).flatten(),
            camera: prefs.get::<Option<String>>(CAMERA).flatten(),
        },
        ring_sound: prefs.get::<bool>(RING_SOUND).unwrap_or(true),
        playback_speed: prefs.get::<u16>(PLAYBACK_SPEED).map_or_else(Default::default, PlaybackSpeed::from_percent),
        show_hidden_events: prefs.get::<bool>(HIDDEN_EVENTS).unwrap_or(false),
        show_room_changes: prefs.get::<bool>(ROOM_CHANGES).unwrap_or(false),
    }
}

/// Empties the scratch folder and makes it again, readable only by the user. Played audio from
/// encrypted rooms lands there decrypted, so nothing in it outlives a run.
pub fn reset_scratch(scratch: &std::path::Path) {
    let _ = std::fs::remove_dir_all(scratch);
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if let Err(error) = builder.create(scratch) {
        eprintln!("better-matrix: could not make the scratch folder {}: {error}", scratch.display());
    }
}
