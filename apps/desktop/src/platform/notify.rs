//! Desktop notifications: Rune's notifier on macOS and Windows, the freedesktop notification
//! service on Linux and BSD. A click on any of them opens the room it came from.

use rune::AppHandle;

use crate::state::State;

/// One notification, already worded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    /// Unique per message, so the OS can replace or withdraw it.
    pub id: String,
    pub title: String,
    pub body: String,
    /// Which account and room a click opens.
    pub account: String,
    pub room_id: String,
}

pub trait Notifier: Send + Sync {
    fn post(&self, toast: &Toast);
}

/// Posts nothing: headless runs and platforms without a notification service.
pub struct Silent;

impl Notifier for Silent {
    fn post(&self, _: &Toast) {}
}

/// Keeps what it would post, for tests.
#[cfg(test)]
pub struct Recording(pub std::sync::Arc<std::sync::Mutex<Vec<Toast>>>);

#[cfg(test)]
impl Notifier for Recording {
    fn post(&self, toast: &Toast) {
        if let Ok(mut posted) = self.0.lock() {
            posted.push(toast.clone());
        }
    }
}

fn open_room(app: &AppHandle<State>, account: String, room_id: String) {
    let account = matrix_core::AccountId::new(account);
    app.update(crate::state::navigate(move |s| s.open_room(&account, &room_id)));
    app.open();
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn system(app: AppHandle<State>) -> Box<dyn Notifier> {
    let _ = rune_notify::register(&rune_notify::Identity::new(crate::APP_ID, "Better Matrix"));
    rune_notify::on_click(move |click| {
        if let (Some(account), Some(room)) = (click.get("account"), click.get("room")) {
            open_room(&app, account.to_owned(), room.to_owned());
        }
    });
    Box::new(RuneNotify)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct RuneNotify;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Notifier for RuneNotify {
    fn post(&self, toast: &Toast) {
        let n = rune_notify::Notification::new(&toast.id, &toast.title)
            .body(&toast.body)
            .info("account", &toast.account)
            .info("room", &toast.room_id);
        rune_notify::post(&n);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn system(app: AppHandle<State>) -> Box<dyn Notifier> {
    Box::new(Freedesktop { app })
}

#[cfg(all(unix, not(target_os = "macos")))]
struct Freedesktop {
    app: AppHandle<State>,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Notifier for Freedesktop {
    fn post(&self, toast: &Toast) {
        let (app, toast) = (self.app.clone(), toast.clone());
        // Showing and waiting for the click both talk D-Bus; keep them off the UI thread.
        let _ = std::thread::Builder::new().name("notification".into()).spawn(move || {
            let shown = notify_rust::Notification::new()
                .appname("Better Matrix")
                .summary(&toast.title)
                .body(&toast.body)
                .action("default", "Open")
                .show();
            if let Ok(handle) = shown {
                handle.wait_for_action(|action| {
                    if action == "default" {
                        open_room(&app, toast.account.clone(), toast.room_id.clone());
                    }
                });
            }
        });
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn system(_: AppHandle<State>) -> Box<dyn Notifier> {
    Box::new(Silent)
}
