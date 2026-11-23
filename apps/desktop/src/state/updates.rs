//! Installing a desktop release is an explicit user action; discovery only offers it. A release
//! the app can't install itself (Linux) opens a panel with the command or page that does.

use rune::Tx;

use super::{Popover, State};
use crate::i18n::{t, tf};
use crate::platform::updates::{Release, Upgrade};

#[derive(Debug, Default)]
pub enum Update {
    #[default]
    Current,
    Available(Release),
    Installing(Release),
}

impl Update {
    pub fn release(&self) -> Option<&Release> {
        match self {
            Self::Current => None,
            Self::Available(release) | Self::Installing(release) => Some(release),
        }
    }
}

impl State {
    pub fn update_found(&mut self, release: Release) {
        if !matches!(self.update, Update::Installing(_)) {
            self.update = Update::Available(release);
        }
    }

    /// The rail's update button: installs a self-update, or shows how to install the release.
    pub fn install_update(&mut self) {
        let Update::Available(release) = &self.update else { return };
        if !matches!(release.upgrade, Upgrade::Archive { .. }) {
            self.toggle_popover(Popover::Update);
            return;
        }
        let release = release.clone();
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

    pub fn update_failed(&mut self, error: String) {
        if let Update::Installing(release) = &self.update {
            self.update = Update::Available(release.clone());
        }
        self.notice = Some(tf!("update.failed", error = error));
    }
}

#[cfg(test)]
mod tests;
