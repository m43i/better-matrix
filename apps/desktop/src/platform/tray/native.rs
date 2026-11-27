use std::sync::Arc;

use tray_icon::TrayIconBuilder;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};

use super::{AppHandle, State, TITLE, icon, t};

const OPEN: &str = "better-matrix.tray.open";
const QUIT: &str = "better-matrix.tray.quit";

pub struct Tray {
    // The native tray and its menu belong to Rune's UI thread on both platforms.
    _icon: tray_icon::TrayIcon,
    #[cfg(target_os = "windows")]
    registration: Registration,
}

impl Tray {
    pub fn show(app: AppHandle<State>) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::with_items(&[
            &MenuItem::with_id(OPEN, t!("tray.open"), true, None),
            &MenuItem::with_id(QUIT, t!("tray.quit"), true, None),
        ])?;
        let icon = icon()?;
        let (width, height) = icon.dimensions();
        let icon = tray_icon::Icon::from_rgba(icon.into_raw(), width, height)?;
        let builder = TrayIconBuilder::new().with_tooltip(TITLE).with_menu(Box::new(menu));
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(icon);
        #[cfg(target_os = "windows")]
        let builder = builder.with_icon(icon).with_menu_on_left_click(false);
        #[cfg(target_os = "macos")]
        // AppKit opens the menu itself. Discard pointer events so they don't queue up unread.
        tray_icon::TrayIconEvent::set_event_handler(Some(|_| {}));
        #[cfg(target_os = "windows")]
        open_on_click(app.clone());
        let icon = builder.build()?;
        #[cfg(target_os = "windows")]
        let registration = {
            // The backend keeps its window even if Explorer refused the initial NIM_ADD.
            // A tooltip update confirms the icon actually registered, including in overflow.
            icon.set_tooltip(Some(TITLE))?;
            Registration::watch(app.clone())?
        };
        MenuEvent::set_event_handler(Some(Arc::new(move |event| match event.id.as_ref() {
            OPEN => app.open(),
            QUIT => app.quit(),
            _ => {}
        })));
        Ok(Self {
            _icon: icon,
            #[cfg(target_os = "windows")]
            registration,
        })
    }

    #[cfg(target_os = "windows")]
    pub fn unavailable(&mut self) -> bool {
        self.registration.unavailable(&self._icon)
    }
}

#[cfg(target_os = "windows")]
const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(target_os = "windows")]
struct Registration {
    _stop: std::sync::mpsc::Sender<()>,
    next_check: std::time::Instant,
    failures: u8,
}

#[cfg(target_os = "windows")]
impl Registration {
    fn watch(app: AppHandle<State>) -> std::io::Result<Self> {
        use std::sync::mpsc::{self, RecvTimeoutError};

        let (stop, dropped) = mpsc::channel();
        std::thread::Builder::new().name("system-tray".into()).spawn(move || {
            while dropped.recv_timeout(CHECK_INTERVAL) == Err(RecvTimeoutError::Timeout) {
                app.wake();
            }
        })?;
        Ok(Self { _stop: stop, next_check: std::time::Instant::now() + CHECK_INTERVAL, failures: 0 })
    }

    fn unavailable(&mut self, icon: &tray_icon::TrayIcon) -> bool {
        let now = std::time::Instant::now();
        if now >= self.next_check {
            self.next_check = now + CHECK_INTERVAL;
            // Explorer can disappear briefly. The backend re-adds the icon on TaskbarCreated,
            // and the same shell call that confirmed registration tells us if it came back.
            self.failures = if icon.set_tooltip(Some(TITLE)).is_ok() { 0 } else { self.failures.saturating_add(1) };
        }
        self.failures >= 5
    }
}

#[cfg(target_os = "windows")]
fn open_on_click(app: AppHandle<State>) {
    use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

    // tray-icon's click handler is process-wide and set once. Every tray belongs to this app.
    static HANDLER: std::sync::Once = std::sync::Once::new();
    HANDLER.call_once(move || {
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
            ) {
                app.open();
            }
        }));
    });
}

impl Drop for Tray {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None);
    }
}
