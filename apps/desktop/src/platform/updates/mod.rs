//! Desktop self-updates: a serial worker checks on startup and hourly, and only downloads
//! after a user action. Helpers wait for a clean process exit before replacing the install.

use super::Shell;

mod install;
mod release;
mod worker;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub asset_id: u64,
    pub digest: String,
}

pub fn start(
    app: rune::AppHandle<crate::state::State>,
    cache: std::path::PathBuf,
) -> Option<std::sync::mpsc::Sender<Release>> {
    // Development builds and headless runs never replace their executable.
    if cfg!(any(target_os = "macos", target_os = "windows")) && !cfg!(debug_assertions) {
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
