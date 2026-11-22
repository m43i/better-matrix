//! Where the core keeps data on disk. Every account gets its own directory, so accounts never
//! share a state, crypto or media store and can run side by side.

use std::path::{Path, PathBuf};

/// The on-disk layout under one data directory.
#[derive(Clone, Debug)]
pub struct StoreLayout {
    root: PathBuf,
}

impl StoreLayout {
    /// A layout rooted at `root`, usually the platform's per-user data directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where map tiles from OpenStreetMap are kept between runs (`crate::geo`).
    pub fn map_tiles(&self) -> PathBuf {
        self.root.join("map-tiles")
    }

    /// A new, empty store directory for one login: `<root>/accounts/<random>`. Every login gets
    /// its own, so accounts never share a state, crypto or media store, and a re-login starts as a
    /// fresh device.
    pub fn new_account_dir(&self) -> Result<PathBuf, crate::Error> {
        Ok(self.root.join("accounts").join(crate::random_hex(8)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_login_should_get_its_own_directory() {
        let layout = StoreLayout::new("data");
        assert_ne!(layout.new_account_dir().unwrap(), layout.new_account_dir().unwrap());
    }

    #[test]
    fn account_directories_should_live_under_accounts() {
        assert!(StoreLayout::new("data").new_account_dir().unwrap().starts_with("data/accounts"));
    }
}
