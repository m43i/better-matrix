//! Installing a desktop release is an explicit user action; discovery only offers it.

use super::State;
use crate::i18n::{t, tf};
use crate::platform::updates::Release;

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

    pub fn install_update(&mut self) {
        let Update::Available(release) = &self.update else { return };
        let release = release.clone();
        self.update = Update::Installing(release.clone());
        if let Err(error) = self.shell.install_update(release) {
            self.update_failed(error);
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
