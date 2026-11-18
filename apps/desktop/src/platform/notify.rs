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
    let (posts, queue) = std::sync::mpsc::channel();
    // Showing and hearing clicks both talk D-Bus; keep them off the UI thread.
    let open = move |account, room_id| open_room(&app, account, room_id);
    let _ = std::thread::Builder::new().name("notifications".into()).spawn(move || freedesktop::serve(queue, open));
    Box::new(Freedesktop { posts })
}

#[cfg(all(unix, not(target_os = "macos")))]
struct Freedesktop {
    posts: std::sync::mpsc::Sender<Toast>,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Notifier for Freedesktop {
    fn post(&self, toast: &Toast) {
        let _ = self.posts.send(toast.clone());
    }
}

/// The freedesktop notification service, over one session-bus connection the first post opens:
/// one thread shows notifications, another hears every click and close, however many stay in
/// the notification history.
#[cfg(all(unix, not(target_os = "macos")))]
mod freedesktop {
    use std::sync::{Arc, Mutex};

    use futures_lite::StreamExt;
    use futures_lite::future::block_on;

    use super::{Shown, Toast};

    const SERVICE: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    /// Shows each toast posted, until the notifier goes; a click on one `open`s its account
    /// and room.
    pub fn serve(queue: std::sync::mpsc::Receiver<Toast>, open: impl Fn(String, String) + Send + Sync + 'static) {
        let (shown, open) = (Arc::new(Mutex::new(Shown::default())), Arc::new(open));
        let mut bus: Option<zbus::Connection> = None;
        for toast in queue {
            if bus.is_none() {
                bus = connect(&shown, &open);
            }
            let Some(connection) = &bus else { continue };
            if let Ok(id) = block_on(show(connection, &toast))
                && let Ok(mut shown) = shown.lock()
            {
                shown.remember(id, toast.account, toast.room_id);
            }
        }
    }

    /// Connects, and hears the service's signals on a thread of its own from then on.
    fn connect(
        shown: &Arc<Mutex<Shown>>,
        open: &Arc<impl Fn(String, String) + Send + Sync + 'static>,
    ) -> Option<zbus::Connection> {
        let (connection, mut signals) = block_on(async {
            let connection = zbus::Connection::session().await?;
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .interface(SERVICE)
                .path(PATH)
                .build()?;
            let signals = zbus::MessageStream::for_match_rule(rule, &connection, None).await?;
            zbus::Result::Ok((connection, signals))
        })
        .ok()?;
        let (open, shown) = (Arc::clone(open), Arc::clone(shown));
        let _ = std::thread::Builder::new().name("notification-clicks".into()).spawn(move || {
            block_on(async {
                while let Some(Ok(message)) = signals.next().await {
                    let header = message.header();
                    let member = header.member().map(|member| member.as_str());
                    let id = match member {
                        Some("ActionInvoked") => {
                            message.body().deserialize::<(u32, String)>().ok().map(|(id, action)| (id, Some(action)))
                        }
                        Some("NotificationClosed") => {
                            message.body().deserialize::<(u32, u32)>().ok().map(|(id, _)| (id, None))
                        }
                        _ => None,
                    };
                    let Some((id, action)) = id else { continue };
                    let room = shown.lock().ok().and_then(|mut shown| shown.take(id));
                    if let (Some((account, room_id)), Some("default")) = (room, action.as_deref()) {
                        open(account, room_id);
                    }
                }
            });
        });
        Some(connection)
    }

    /// `Notify`'s arguments for `toast`: app name, id to replace, icon, summary, body, actions
    /// (a default one, which opens its room), hints, and the server's own timeout.
    pub(super) fn notify(toast: &Toast) -> impl serde::Serialize + zbus::DynamicType + '_ {
        let hints = std::collections::HashMap::<&str, zbus::Variant<'_>>::new();
        ("Better Matrix", 0u32, "", &toast.title, &toast.body, vec!["default", "Open"], hints, -1i32)
    }

    /// Shows `toast`; the service's id for it.
    async fn show(connection: &zbus::Connection, toast: &Toast) -> zbus::Result<u32> {
        connection.call_method(Some(SERVICE), PATH, Some(SERVICE), "Notify", &notify(toast)).await?.body().deserialize()
    }
}

/// The rooms of notifications still on screen or in the history, by the id the notification
/// service gave them: a click opens one. Past [`Shown::MAX`], the oldest are forgotten.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
#[derive(Debug, Default)]
struct Shown(std::collections::VecDeque<(u32, String, String)>);

#[cfg(any(test, all(unix, not(target_os = "macos"))))]
impl Shown {
    const MAX: usize = 256;

    fn remember(&mut self, id: u32, account: String, room_id: String) {
        self.0.retain(|(shown, ..)| *shown != id);
        if self.0.len() >= Self::MAX {
            self.0.pop_front();
        }
        self.0.push_back((id, account, room_id));
    }

    /// The account and room of notification `id`, forgotten: it was clicked or closed.
    fn take(&mut self, id: u32) -> Option<(String, String)> {
        let at = self.0.iter().position(|(shown, ..)| *shown == id)?;
        self.0.remove(at).map(|(_, account, room_id)| (account, room_id))
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn system(_: AppHandle<State>) -> Box<dyn Notifier> {
    Box::new(Silent)
}

#[cfg(test)]
mod tests {
    use super::Shown;

    #[test]
    fn a_clicked_notification_should_give_its_room_once() {
        let mut shown = Shown::default();
        shown.remember(7, "@me:example.org".into(), "!room:example.org".into());
        let first = shown.take(7);
        assert_eq!((first, shown.take(7)), (Some(("@me:example.org".into(), "!room:example.org".into())), None));
    }

    #[test]
    fn notifications_past_the_bound_should_forget_the_oldest() {
        let mut shown = Shown::default();
        for id in 0..=Shown::MAX as u32 {
            shown.remember(id, "@me:example.org".into(), format!("!{id}:example.org"));
        }
        assert_eq!((shown.0.len(), shown.take(0), shown.take(1).is_some()), (Shown::MAX, None, true));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_notification_should_carry_the_arguments_the_service_expects() {
        let toast = super::Toast {
            id: "$event".into(),
            title: "Alice".into(),
            body: "hi".into(),
            account: "@me:example.org".into(),
            room_id: "!room:example.org".into(),
        };
        let message = zbus::Message::method_call("/org/freedesktop/Notifications", "Notify")
            .build(&super::freedesktop::notify(&toast));
        let signature = message.map(|message| message.body().signature().to_string());
        // The body as one struct: `susssasa{sv}i` on the wire.
        assert_eq!(signature.ok().as_deref(), Some("(susssasa{sv}i)"));
    }
}
