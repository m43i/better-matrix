//! The widget window: a room's widget, or the integration manager, in the system's web view
//! (WebKitGTK, WKWebView, WebView2). Rune draws no web pages, and on Wayland a web view can't be
//! placed inside another toolkit's window, so each runs in a window of its own, in a process of
//! its own: this binary started with [`FLAG`] ([`helper`]).
//!
//! The app and a helper talk in JSON lines, one object each, tagged by `type`:
//!
//! - App to helper ([`ToHelper`]), on its stdin: `open` (the page's `url`, the window's `title`
//!   and the page's `bridge`), first, and again to load the page anew in the same window (the
//!   widget starts over; messages not yet delivered are dropped); `post` (a `message` for the
//!   page, JSON text); `close`. The helper closes too when its stdin ends, so it never outlives
//!   the app.
//! - Helper to app ([`FromHelper`]), on its stdout: `loading` and `loaded` (the page started and
//!   finished loading a document, each time); `message` (what the page posted, JSON text, as its
//!   bridge lets through); `open_url` (a link the page opens in a new window, or a page off its
//!   origin, for the browser); `closed`, once, as it goes.
//!
//! The app side ([`Windows`]) starts one helper per running widget and hands what it says to
//! the state on the UI thread, as the engine's events come ([`rune::AppHandle::update`]). Each
//! helper has a thread that writes to it and one that reads it, so the UI thread never waits on
//! a pipe or a process. Headless, nothing starts and the calls are kept ([`WindowCall`]).

pub mod helper;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Duration;

use matrix_core::{RequestId, WidgetBridge};
use rune::AppHandle;
use serde::{Deserialize, Serialize};

use crate::state::State;

/// The argument that starts this binary as a widget window instead of the app.
pub const FLAG: &str = "--widget-window";

/// How long a helper gets to close its window once asked before it is killed.
const CLOSE_WAIT: Duration = Duration::from_secs(2);

/// Which of a page's messages the helper passes on, as [`WidgetBridge`] says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bridge {
    /// None: the page talks to nothing.
    None,
    /// The widget API (MSC2762).
    WidgetApi,
    /// An integration manager's messages.
    Integrations,
}

impl From<WidgetBridge> for Bridge {
    fn from(bridge: WidgetBridge) -> Self {
        match bridge {
            WidgetBridge::None => Self::None,
            WidgetBridge::WidgetApi => Self::WidgetApi,
            WidgetBridge::Integrations => Self::Integrations,
        }
    }
}

/// What the app tells a helper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToHelper {
    /// Show `url` in a window titled `title`, passing on what `bridge` lets through.
    Open { url: String, title: String, bridge: Bridge },
    /// Post `message` (JSON text) into the page.
    Post { message: String },
    /// Close the window.
    Close,
}

/// What a helper tells the app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromHelper {
    /// The page started loading a document.
    Loading,
    /// The page finished loading.
    Loaded,
    /// The page posted `message` (JSON text).
    Message { message: String },
    /// The page opens `url` in a new window: the browser's.
    OpenUrl { url: String },
    /// The window closed.
    Closed,
    /// The window could not be created or its page could not be loaded.
    Failed,
}

/// One protocol line: the message as JSON, then a newline.
fn line(message: &impl Serialize) -> String {
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    line
}

impl ToHelper {
    /// The line that says this.
    pub fn line(&self) -> String {
        line(self)
    }

    /// The message a line says, if it says one.
    pub fn parse(line: &str) -> Option<Self> {
        serde_json::from_str(line.trim()).ok()
    }
}

impl FromHelper {
    /// The line that says this.
    pub fn line(&self) -> String {
        line(self)
    }

    /// The message a line says, if it says one.
    pub fn parse(line: &str) -> Option<Self> {
        serde_json::from_str(line.trim()).ok()
    }
}

/// What the app asked of widget windows, as a headless [`Windows`] keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCall {
    Open {
        request: RequestId,
        url: String,
        title: String,
        bridge: Bridge,
    },
    /// The run's window, already open, loads its page anew.
    Reload {
        request: RequestId,
        url: String,
    },
    Post {
        request: RequestId,
        message: String,
    },
    Close(RequestId),
}

/// The open widget windows, by the widget run each shows.
#[derive(Debug)]
pub struct Windows {
    kind: Kind,
}

enum Kind {
    Native { app: AppHandle<State>, open: HashMap<RequestId, Helper> },
    Headless { open: HashSet<RequestId>, calls: Vec<WindowCall> },
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native { open, .. } => f.debug_struct("Native").field("open", &open.len()).finish_non_exhaustive(),
            Self::Headless { open, calls } => {
                f.debug_struct("Headless").field("open", open).field("calls", calls).finish()
            }
        }
    }
}

impl Windows {
    /// Real windows, each in a helper process, reporting to the app through `app`.
    pub fn native(app: AppHandle<State>) -> Self {
        Self { kind: Kind::Native { app, open: HashMap::new() } }
    }

    /// No windows: what was asked is kept, for tests.
    pub fn headless() -> Self {
        Self { kind: Kind::Headless { open: HashSet::new(), calls: Vec::new() } }
    }

    /// Shows a run's page in a window of its own; a run already showing one loads it there anew
    /// (the widget starts over), in the same window, with the same bridge.
    pub fn open(&mut self, request: RequestId, url: String, title: String, bridge: Bridge) {
        match &mut self.kind {
            Kind::Native { app, open } => {
                let message = ToHelper::Open { url, title, bridge };
                match open.get(&request) {
                    Some(helper) => helper.send(message),
                    None => {
                        open.insert(request, Helper::start(app.clone(), request, message));
                    }
                }
            }
            Kind::Headless { open, calls } => {
                if open.insert(request) {
                    calls.push(WindowCall::Open { request, url, title, bridge });
                } else {
                    calls.push(WindowCall::Reload { request, url });
                }
            }
        }
    }

    /// Posts `message` into a run's page, once it has loaded. Nothing for a run without a window.
    pub fn post(&mut self, request: RequestId, message: String) {
        match &mut self.kind {
            Kind::Native { open, .. } => {
                if let Some(helper) = open.get(&request) {
                    helper.send(ToHelper::Post { message });
                }
            }
            Kind::Headless { open, calls } => {
                if open.contains(&request) {
                    calls.push(WindowCall::Post { request, message });
                }
            }
        }
    }

    /// Closes a run's window, if it shows one.
    pub fn close(&mut self, request: RequestId) {
        match &mut self.kind {
            // Dropped, the helper is told to close.
            Kind::Native { open, .. } => drop(open.remove(&request)),
            Kind::Headless { open, calls } => {
                if open.remove(&request) {
                    calls.push(WindowCall::Close(request));
                }
            }
        }
    }

    /// Forgets a run's window, which closed by itself; whether it was still open.
    pub fn forget(&mut self, request: RequestId) -> bool {
        match &mut self.kind {
            Kind::Native { open, .. } => open.remove(&request).is_some(),
            Kind::Headless { open, .. } => open.remove(&request),
        }
    }

    /// What was asked of the windows, headless.
    #[cfg(test)]
    pub fn calls(&self) -> &[WindowCall] {
        match &self.kind {
            Kind::Native { .. } => &[],
            Kind::Headless { calls, .. } => calls,
        }
    }
}

/// One helper process, as the UI thread holds it: what goes to its writer thread. Dropped, it
/// tells the helper to close; the writer then waits for it a moment and kills it if it stays.
struct Helper {
    to: mpsc::Sender<ToHelper>,
}

impl Helper {
    /// Starts a helper on a thread of its own, opening it with `open`. Its messages reach the
    /// state through `app`; if it can't start, [`State::widget_window_failed`] hears why.
    fn start(app: AppHandle<State>, request: RequestId, open: ToHelper) -> Self {
        let (to, from_app) = mpsc::channel();
        let _ = to.send(open);
        let failed = app.clone();
        let spawned = std::thread::Builder::new().name("widget-window".into()).spawn(move || {
            if serve(&app, request, &from_app).is_err() {
                app.update(move |s: &mut State, _| s.widget_window_failed(request));
            }
        });
        if spawned.is_err() {
            failed.update(move |s: &mut State, _| s.widget_window_failed(request));
        }
        Self { to }
    }

    fn send(&self, message: ToHelper) {
        let _ = self.to.send(message);
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.to.send(ToHelper::Close);
    }
}

/// Runs one helper process: starts it, reads it on another thread, writes to it here until
/// the app lets go of it or asks it to close, then makes sure it is gone.
fn serve(app: &AppHandle<State>, request: RequestId, from_app: &mpsc::Receiver<ToHelper>) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(std::env::current_exe()?);
    command.arg(FLAG).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// No console window for a helper of a console build.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn()?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other("the widget window has no pipes"));
    };
    let reader = app.clone();
    let read = std::thread::Builder::new().name("widget-window-read".into()).spawn(move || {
        for said in std::io::BufReader::new(stdout).lines() {
            let Ok(said) = said else { break };
            match FromHelper::parse(&said) {
                Some(FromHelper::Loading) => reader.update(move |s: &mut State, _| s.widget_loading(request)),
                Some(FromHelper::Loaded) => reader.update(move |s: &mut State, _| s.widget_loaded(request)),
                Some(FromHelper::Message { message }) => {
                    reader.update(move |s: &mut State, _| s.widget_posted(request, message));
                }
                Some(FromHelper::OpenUrl { url }) => reader.update(move |s: &mut State, tx| s.open_link(&url, tx)),
                Some(FromHelper::Closed) => {
                    reader.update(move |s: &mut State, _| s.widget_window_closed(request));
                    return;
                }
                Some(FromHelper::Failed) => {
                    reader.update(move |s: &mut State, _| s.widget_window_failed(request));
                    return;
                }
                None => {}
            }
        }
        reader.update(move |s: &mut State, _| s.widget_window_failed(request));
    });
    if let Err(error) = read {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    for message in from_app {
        let close = message == ToHelper::Close;
        let written = stdin.write_all(message.line().as_bytes()).and_then(|()| stdin.flush());
        if close || written.is_err() {
            break;
        }
    }
    drop(stdin);
    let deadline = std::time::Instant::now() + CLOSE_WAIT;
    while std::time::Instant::now() < deadline {
        if !matches!(child.try_wait(), Ok(None)) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
