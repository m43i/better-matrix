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

fn open_room(app: &AppHandle<State>, account: String, room_id: String, token: Option<String>) {
    let account = matrix_core::AccountId::new(account);
    app.update(crate::state::navigate(move |s| s.open_room(&account, &room_id)));
    if let Some(token) = token {
        app.activate(token);
    } else {
        app.open();
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn system(app: AppHandle<State>) -> Box<dyn Notifier> {
    let _ = rune_notify::register(&rune_notify::Identity::new(crate::APP_ID, "Better Matrix"));
    rune_notify::on_click(move |click| {
        if let (Some(account), Some(room)) = (click.get("account"), click.get("room")) {
            open_room(&app, account.to_owned(), room.to_owned(), None);
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
    let open = move |account, room_id, token| open_room(&app, account, room_id, token);
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
/// the notification history. A failed show closes the connection, which ends its listener, and
/// the next post opens a new one.
#[cfg(all(unix, not(target_os = "macos")))]
mod freedesktop {
    use std::sync::{Arc, Mutex};

    use futures_lite::future::block_on;
    use futures_lite::{Stream, StreamExt};

    use super::{Shown, Toast};

    const SERVICE: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    /// Shows each toast posted, until the notifier goes; a click on one `open`s its account
    /// and room.
    pub fn serve(
        queue: std::sync::mpsc::Receiver<Toast>,
        open: impl Fn(String, String, Option<String>) + Send + Sync + 'static,
    ) {
        let (shown, open) = (Arc::new(Mutex::new(Shown::default())), Arc::new(open));
        let mut bus: Option<zbus::Connection> = None;
        for toast in queue {
            // Worded per toast, in the language chosen by then.
            let action = crate::i18n::t!("attachment.open");
            let id = super::deliver(
                &mut bus,
                || connect(&shown, &open),
                |connection| block_on(show(connection, &toast, action)),
                // Closing ends the listener's stream, so its thread goes with the connection.
                |connection| drop(block_on(connection.close())),
            );
            if let Some(id) = id
                && let Ok(mut shown) = shown.lock()
            {
                shown.remember(id, toast.account, toast.room_id);
            }
        }
    }

    /// Connects, and hears the service's signals on a thread of its own until the connection
    /// closes.
    fn connect(
        shown: &Arc<Mutex<Shown>>,
        open: &Arc<impl Fn(String, String, Option<String>) + Send + Sync + 'static>,
    ) -> Option<zbus::Connection> {
        let (connection, signals) = block_on(async {
            let connection = zbus::Connection::session().await?;
            let rule = zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(SERVICE)
                .interface(SERVICE)
                .path(PATH)
                .build()?;
            let signals = zbus::MessageStream::for_match_rule(rule, &connection, None).await?;
            zbus::Result::Ok((connection, signals))
        })
        .ok()?;
        let (open, shown) = (Arc::clone(open), Arc::clone(shown));
        let _ = std::thread::Builder::new()
            .name("notification-clicks".into())
            .spawn(move || block_on(hear(signals, &shown, |account, room_id, token| open(account, room_id, token))));
        Some(connection)
    }

    /// Opens the room of each notification clicked, and forgets those closed, until `signals`
    /// ends. A message that can't be read is skipped: one bad message doesn't end the clicks.
    pub(super) async fn hear(
        mut signals: impl Stream<Item = zbus::Result<zbus::Message>> + Unpin,
        shown: &Mutex<Shown>,
        open: impl Fn(String, String, Option<String>),
    ) {
        while let Some(message) = signals.next().await {
            let Ok(message) = message else { continue };
            let header = message.header();
            let member = header.member().map(|member| member.as_str());
            let id = match member {
                Some("ActionInvoked") => {
                    message.body().deserialize::<(u32, String)>().ok().map(|(id, action)| (id, Some(action)))
                }
                Some("ActivationToken") => {
                    if let Ok((id, token)) = message.body().deserialize::<(u32, String)>()
                        && let Ok(mut shown) = shown.lock()
                    {
                        shown.set_token(id, token);
                    }
                    continue;
                }
                Some("NotificationClosed") => message.body().deserialize::<(u32, u32)>().ok().map(|(id, _)| (id, None)),
                _ => None,
            };
            let Some((id, action)) = id else { continue };
            let room = shown.lock().ok().and_then(|mut shown| shown.take(id));
            if let (Some(room), Some("default")) = (room, action.as_deref()) {
                open(room.account, room.room_id, room.token);
            }
        }
    }

    /// `Notify`'s arguments for `toast`: app name, id to replace, icon, summary, body, actions
    /// (a default one labelled `action`, which opens its room), hints, and the server's own
    /// timeout.
    pub(super) fn notify<'a>(toast: &'a Toast, action: &'a str) -> impl serde::Serialize + zbus::DynamicType + 'a {
        let hints = std::collections::HashMap::<&str, zbus::Variant<'_>>::new();
        ("Better Matrix", 0u32, "", &toast.title, &toast.body, vec!["default", action], hints, -1i32)
    }

    /// Shows `toast`; the service's id for it.
    async fn show(connection: &zbus::Connection, toast: &Toast, action: &str) -> zbus::Result<u32> {
        connection
            .call_method(Some(SERVICE), PATH, Some(SERVICE), "Notify", &notify(toast, action))
            .await?
            .body()
            .deserialize()
    }
}

/// Shows something over `bus`, connecting first when there is none. A failed show lets go of
/// the connection (`close`), so the next one connects anew rather than failing on a dead bus.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn deliver<C, T, E>(
    bus: &mut Option<C>,
    connect: impl FnOnce() -> Option<C>,
    show: impl FnOnce(&C) -> Result<T, E>,
    close: impl FnOnce(C),
) -> Option<T> {
    if bus.is_none() {
        *bus = connect();
    }
    let shown = show(bus.as_ref()?);
    if shown.is_err()
        && let Some(connection) = bus.take()
    {
        close(connection);
    }
    shown.ok()
}

/// The rooms of notifications still on screen or in the history, by the id the notification
/// service gave them: a click opens one. Past [`Shown::MAX`], the oldest are forgotten.
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
#[derive(Debug, Default)]
struct Shown(std::collections::VecDeque<ShownToast>);

#[cfg(any(test, all(unix, not(target_os = "macos"))))]
#[derive(Debug, PartialEq, Eq)]
struct ShownToast {
    id: u32,
    account: String,
    room_id: String,
    token: Option<String>,
}

#[cfg(any(test, all(unix, not(target_os = "macos"))))]
impl Shown {
    const MAX: usize = 256;

    fn remember(&mut self, id: u32, account: String, room_id: String) {
        self.0.retain(|shown| shown.id != id);
        if self.0.len() >= Self::MAX {
            self.0.pop_front();
        }
        self.0.push_back(ShownToast { id, account, room_id, token: None });
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn set_token(&mut self, id: u32, token: String) {
        if let Some(toast) = self.0.iter_mut().find(|shown| shown.id == id) {
            toast.token = (!token.is_empty()).then_some(token);
        }
    }

    /// The notification was clicked or closed.
    fn take(&mut self, id: u32) -> Option<ShownToast> {
        let at = self.0.iter().position(|shown| shown.id == id)?;
        self.0.remove(at)
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn system(_: AppHandle<State>) -> Box<dyn Notifier> {
    Box::new(Silent)
}

#[cfg(test)]
mod tests {
    use super::{Shown, deliver};

    #[test]
    fn a_failed_show_should_let_go_of_the_connection() {
        let (mut bus, mut closed) = (None, Vec::new());
        let shown = deliver(&mut bus, || Some(1), |_| Err::<u32, ()>(()), |connection| closed.push(connection));
        assert_eq!((shown, bus, closed), (None, None, vec![1]));
    }

    #[test]
    fn the_show_after_a_failed_one_should_connect_again() {
        let (mut bus, mut connects) = (None, 0);
        deliver(&mut bus, || Some(1), |_| Err::<u32, ()>(()), drop);
        let shown = deliver(
            &mut bus,
            || {
                connects += 1;
                Some(2)
            },
            |connection| Ok::<u32, ()>(*connection),
            drop,
        );
        assert_eq!((shown, connects), (Some(2), 1));
    }

    #[test]
    fn a_shown_notification_should_keep_its_connection() {
        let mut bus = None;
        deliver(&mut bus, || Some(1), |_| Ok::<u32, ()>(7), drop);
        let shown = deliver(&mut bus, || Some(2), |connection| Ok::<u32, ()>(*connection), drop);
        assert_eq!(shown, Some(1));
    }

    #[test]
    fn a_clicked_notification_should_give_its_room_once() {
        let mut shown = Shown::default();
        shown.remember(7, "@me:example.org".into(), "!room:example.org".into());
        let first = shown.take(7).map(|toast| (toast.account, toast.room_id));
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
            .build(&super::freedesktop::notify(&toast, "Open"));
        let signature = message.map(|message| message.body().signature().to_string());
        // The body as one struct: `susssasa{sv}i` on the wire.
        assert_eq!(signature.ok().as_deref(), Some("(susssasa{sv}i)"));
    }

    /// The service's signal `member` with `body`, as the listener hears it.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn signal<B: serde::Serialize + zbus::DynamicType>(member: &str, body: &B) -> zbus::Result<zbus::Message> {
        zbus::Message::signal("/org/freedesktop/Notifications", "org.freedesktop.Notifications", member).build(body)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_bad_message_should_not_stop_hearing_clicks() {
        let shown = std::sync::Mutex::new(Shown::default());
        if let Ok(mut shown) = shown.lock() {
            shown.remember(7, "@me:example.org".into(), "!room:example.org".into());
        }
        let broken = Err(zbus::Error::Failure("unreadable".into()));
        let signals = futures_lite::stream::iter([broken, signal("ActionInvoked", &(7u32, "default"))]);
        let opened = std::sync::Mutex::new(Vec::new());
        futures_lite::future::block_on(super::freedesktop::hear(signals, &shown, |account, room_id, _| {
            opened.lock().unwrap().push((account, room_id));
        }));
        assert_eq!(opened.into_inner().unwrap(), [("@me:example.org".to_owned(), "!room:example.org".to_owned())]);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_notification_click_should_take_its_own_activation_token() {
        let mut shown = Shown::default();
        shown.remember(7, "@me:example.org".into(), "!room:example.org".into());
        shown.remember(8, "@me:example.org".into(), "!other:example.org".into());
        let signals = futures_lite::stream::iter([
            signal("ActivationToken", &(7u32, "activation")),
            signal("ActionInvoked", &(8u32, "default")),
            signal("ActionInvoked", &(7u32, "default")),
        ]);
        let opened = std::cell::RefCell::new(Vec::new());
        futures_lite::future::block_on(super::freedesktop::hear(
            signals,
            &std::sync::Mutex::new(shown),
            |_, room, token| {
                opened.borrow_mut().push((room, token));
            },
        ));
        assert_eq!(
            opened.into_inner(),
            [("!other:example.org".into(), None), ("!room:example.org".into(), Some("activation".into())),]
        );
    }
}
