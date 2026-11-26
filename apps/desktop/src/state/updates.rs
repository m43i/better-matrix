//! On macOS and Windows a newer release downloads in the background, then the rail asks the user
//! to restart into it: restarting is their action. A release the app can't install itself (Linux)
//! opens a panel with the command or page that does.

use rune::Tx;

use super::{Popover, State};
use crate::i18n::{t, tf};
use crate::platform::updates::{Release, Upgrade};

#[derive(Debug, Default)]
pub enum Update {
    #[default]
    Current,
    /// A release the user installs outside the app: the rail opens its command or page.
    Available(Release),
    /// A release downloaded and checked, waiting for the user to restart into it.
    Ready(Release),
    /// The user restarted: the app quits once the install helper has started.
    Installing(Release),
}

impl Update {
    pub fn release(&self) -> Option<&Release> {
        match self {
            Self::Current => None,
            Self::Available(release) | Self::Ready(release) | Self::Installing(release) => Some(release),
        }
    }
}

impl State {
    /// A release to install outside the app. On macOS and Windows that is the page of one the
    /// updater could not stage, so nothing is left to restart into.
    pub fn update_found(&mut self, release: Release) {
        if !matches!(self.update, Update::Installing(_)) {
            self.update = Update::Available(release);
        }
    }

    /// The updater downloaded `release`: the rail offers to restart into it.
    pub fn update_ready(&mut self, release: Release) {
        if !matches!(self.update, Update::Installing(_)) {
            self.update = Update::Ready(release);
        }
    }

    /// The rail's update button: restarts into a downloaded release, or shows how to install one.
    pub fn install_update(&mut self) {
        let release = match &self.update {
            Update::Ready(release) => release.clone(),
            Update::Available(_) => return self.toggle_popover(Popover::Update),
            Update::Current | Update::Installing(_) => return,
        };
        self.update = Update::Installing(release.clone());
        if let Err(error) = self.shell.install_update(release) {
            self.update_failed(error);
        }
    }

    /// Copies the offered release's terminal command.
    pub fn copy_update_command(&mut self, tx: &mut Tx) {
        if let Some(Release { upgrade: Upgrade::Command(command), .. }) = self.update.release() {
            tx.copy(command.clone());
            self.popover = None;
            self.notice = Some(t!("update.copied").to_owned());
        }
    }

    /// Opens the offered release's page, for an install the app can't name a command for.
    pub fn open_update_page(&mut self, tx: &mut Tx) {
        if let Some(Release { upgrade: Upgrade::Page(url), .. }) = self.update.release() {
            let url = url.clone();
            self.popover = None;
            if let Err(error) = tx.open_url(&url) {
                self.notice = Some(tf!("notice.browser", error = error));
            }
        }
    }

    pub fn update_interrupted(&mut self) {
        self.notice = Some(t!("update.incomplete").to_owned());
    }

    /// The restart failed. The updater offers the release's page, stages it afresh, and the rail
    /// offers the restart again once that is done.
    pub fn update_failed(&mut self, error: String) {
        if matches!(self.update, Update::Installing(_)) {
            self.update = Update::Current;
        }
        self.notice = Some(tf!("update.failed", error = error));
    }
}

#[cfg(test)]
mod tests;
