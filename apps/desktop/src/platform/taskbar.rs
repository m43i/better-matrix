//! The app's taskbar button (Windows) or Dock icon (macOS): its badge counts what wants the
//! user's attention, a notification flashes it (Windows), and the Dock icon's menu offers the
//! rooms opened most (macOS). Rune does all three ([`rune::WindowHandle::set_taskbar`],
//! [`rune::WindowHandle::flash`], [`rune::set_jump_list`]); elsewhere they show nothing. Called on
//! the UI thread, as Rune asks.
//!
//! Windows gets no jump list: the app has an explicit AppUserModelID (`rune-notify`), and Rune's
//! jump list doesn't name it (`ICustomDestinationList::SetAppID`), which Windows requires then.

use std::sync::{Arc, Mutex};

use rune::WindowHandle;

/// The count shown and the window showing it; shared with the hook that hears the window made.
#[derive(Clone, Default)]
pub struct Taskbar(Arc<Mutex<Shown>>);

#[derive(Default)]
struct Shown {
    /// None until the native window exists, and always headless.
    window: Option<WindowHandle>,
    count: u32,
    /// None until the first is offered, which replaces whatever an earlier run left.
    jump_list: Option<rune::JumpList>,
}

impl Taskbar {
    /// The app's native window was made (again): its new button shows the count.
    pub fn attach(&self, window: &WindowHandle) {
        let Ok(mut shown) = self.0.lock() else { return };
        shown.window = Some(window.clone());
        badge(window, shown.count);
    }

    /// Shows `count` on the badge (0 hides it); the window is told only when it changes.
    pub fn show(&self, count: u32) {
        let Ok(mut shown) = self.0.lock() else { return };
        if shown.count == count {
            return;
        }
        shown.count = count;
        if let Some(window) = &shown.window {
            badge(window, count);
        }
    }

    /// Flashes the button for a notification, until the window comes to the front. A window
    /// already in front flashes nothing.
    pub fn flash(&self) {
        if let Ok(shown) = self.0.lock()
            && let Some(window) = &shown.window
        {
            window.flash(true);
        }
    }

    /// Offers `list` as the jump list when it changed, to the system only when `native` and on
    /// macOS (see the module's notes on Windows).
    pub fn jump_list(&self, list: rune::JumpList, native: bool) {
        let Ok(mut shown) = self.0.lock() else { return };
        if shown.jump_list.as_ref() == Some(&list) {
            return;
        }
        shown.jump_list = Some(list.clone());
        if native && cfg!(target_os = "macos") {
            rune::set_jump_list(list);
        }
    }

    /// The jump list last offered.
    #[cfg(test)]
    pub fn shown_jump_list(&self) -> rune::JumpList {
        self.0.lock().ok().and_then(|shown| shown.jump_list.clone()).unwrap_or_default()
    }

    /// The count the badge shows.
    #[cfg(test)]
    pub fn count(&self) -> u32 {
        self.0.lock().map_or(0, |shown| shown.count)
    }
}

fn badge(window: &WindowHandle, count: u32) {
    let description = crate::i18n::tp!("taskbar.badge", count);
    window.set_taskbar(rune::Taskbar::new().badge(count, description));
}
