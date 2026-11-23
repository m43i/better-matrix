//! Desktop updates: a serial worker checks on startup and hourly. On macOS and Windows the app
//! updates itself, and only downloads after a user action; helpers wait for a clean process exit
//! before replacing the install. Linux installs belong to their package manager, so the app
//! offers the command that updates the way it was installed.

use super::Shell;

mod install;
mod linux;
mod release;
mod worker;

/// A newer stable release, and how it reaches this install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub upgrade: Upgrade,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Upgrade {
    /// The app downloads this release asset, checks its SHA-256 and replaces itself.
    Archive { asset_id: u64, digest: String },
    /// A command for a terminal that installs the release the way this copy was installed.
    Command(String),
    /// The app can't tell how it was installed: the release's page offers its downloads.
    Page(String),
}

pub fn start(
    app: rune::AppHandle<crate::state::State>,
    cache: std::path::PathBuf,
) -> Option<std::sync::mpsc::Sender<Release>> {
    // Development builds and headless runs never replace their executable.
    if cfg!(any(target_os = "macos", target_os = "windows", target_os = "linux")) && !cfg!(debug_assertions) {
        return worker::start(app, cache);
    }
    let _ = (app, cache);
    None
}

impl Shell {
    #[cfg(test)]
    pub fn record_updates(&mut self, sender: std::sync::mpsc::Sender<Release>) {
        self.updates = Some(sender);
    }

    pub fn install_update(&self, release: Release) -> Result<(), String> {
        if let Some(sender) = &self.updates {
            sender.send(release).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
