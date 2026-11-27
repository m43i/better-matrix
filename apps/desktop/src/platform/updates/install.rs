//! Stage beside the install (same filesystem, writable before quitting) while the app runs, then,
//! when the user restarts, hand over to a helper. Its ready file proves the script started before
//! the app is asked to quit.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::platform::launch::app_bundle;

use super::release::{self, Platform};
use super::{Release, Upgrade};

#[derive(Debug)]
pub struct Install {
    platform: Platform,
    target: PathBuf,
}

impl Install {
    pub fn current() -> Result<Option<Self>, String> {
        let platform = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Platform::Mac
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Platform::Windows
        } else {
            return Ok(None);
        };
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let target = match platform {
            Platform::Mac => app_bundle(&executable).ok_or("Updates require an installed Better Matrix.app")?,
            Platform::Windows => executable,
        };
        Ok(Some(Self { platform, target }))
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Downloads `release`, checks its digest (and on macOS its signature), and unpacks it beside
    /// the install, ready to swap in at [`Install::hand_over`]. Dropping the result removes it.
    /// The download gives up after `limit`.
    pub fn stage(
        &self,
        client: &reqwest::blocking::Client,
        release: &Release,
        limit: Duration,
    ) -> Result<Staged, String> {
        let Upgrade::Archive { asset_id, digest } = &release.upgrade else {
            return Err("This release installs outside the app".into());
        };
        let writable = |error: std::io::Error| format!("The install folder must be writable: {error}");
        let root = self.staging()?;
        fs::create_dir_all(&root).map_err(writable)?;
        let work = tempfile::Builder::new().tempdir_in(&root).map_err(writable)?;
        let archive = work.path().join("release.zip");
        let mut file = File::create(&archive).map_err(|error| error.to_string())?;
        release::download(client, *asset_id, digest, &mut file, limit)?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        let app = match self.platform {
            Platform::Mac => {
                let output = Command::new("/usr/bin/ditto")
                    .args(["-x", "-k"])
                    .arg(&archive)
                    .arg(work.path())
                    .output()
                    .map_err(|error| error.to_string())?;
                if !output.status.success() {
                    return Err(String::from_utf8_lossy(&output.stderr).into_owned());
                }
                let staged = work.path().join("Better Matrix.app");
                if !staged.join("Contents/MacOS/better-matrix").is_file() {
                    return Err("The update archive has no app bundle".into());
                }
                let status = Command::new("/usr/bin/codesign")
                    .args(["--verify", "--deep", "--strict"])
                    .arg(&staged)
                    .status()
                    .map_err(|error| error.to_string())?;
                if !status.success() {
                    return Err("The app bundle failed its signature check".into());
                }
                staged
            }
            Platform::Windows => extract_executable(&archive, work.path())?,
        };
        fs::remove_file(archive).map_err(|error| error.to_string())?;
        Ok(Staged { version: release.version.clone(), app, work })
    }

    /// Starts the helper that swaps `staged` in once this process exits, and waits until it has
    /// started. On success the caller [keeps](Staged::keep) the staged files for the helper,
    /// then quits.
    pub fn hand_over(&self, staged: &Staged, error_file: &Path) -> Result<(), String> {
        let Staged { version, app, work } = staged;
        // A helper an earlier try gave up on may have left its acknowledgement.
        let _ = fs::remove_file(work.path().join("ready"));
        let script = work.path().join(match self.platform {
            Platform::Mac => "install.sh",
            Platform::Windows => "install.ps1",
        });
        fs::write(
            &script,
            match self.platform {
                Platform::Mac => include_str!("install.sh"),
                Platform::Windows => include_str!("install.ps1"),
            },
        )
        .map_err(|error| error.to_string())?;
        let mut command = match self.platform {
            Platform::Mac => Command::new("/bin/sh"),
            Platform::Windows => {
                // Use the OS copy, never a PowerShell found on PATH.
                let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is missing")?;
                let mut command =
                    Command::new(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
                command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]);
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                }
                command
            }
        };
        let mut helper = command
            .arg(script)
            .arg(&self.target)
            .arg(app)
            .arg(std::process::id().to_string())
            .arg(work.path())
            .arg(error_file)
            .arg(version)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if helper.try_wait().map_err(|error| error.to_string())?.is_some() {
                return Err("The update helper could not start".into());
            }
            if work.path().join("ready").is_file() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                let _ = helper.kill();
                let _ = helper.wait();
                return Err("The update helper did not become ready".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The updater's own folder beside the install (the same filesystem, so a staged release
    /// moves into place), holding one folder per staged release. A link in its place is never
    /// followed: what it points to isn't the updater's.
    fn staging(&self) -> Result<PathBuf, String> {
        let root =
            self.target.parent().ok_or("The install has no parent directory")?.join(".better-matrix-update-staging");
        match fs::symlink_metadata(&root) {
            Ok(meta) if !meta.is_dir() => Err(format!("{} is not the updater's folder", root.display())),
            _ => Ok(root),
        }
    }

    /// Removes what an earlier run staged and never handed over (it quit, or crashed, while an
    /// update waited for a restart). A folder with a ready file belongs to a helper: kept.
    pub fn clear_stale(&self) {
        let Ok(root) = self.staging() else { return };
        let Ok(entries) = fs::read_dir(&root) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) && !path.join("ready").exists() {
                let _ = fs::remove_dir_all(path);
            }
        }
        // Gone once empty, so no folder is left beside the install.
        let _ = fs::remove_dir(root);
    }
}

/// A release downloaded, checked and unpacked beside the install, waiting for a restart.
#[derive(Debug)]
pub struct Staged {
    pub version: String,
    /// The app bundle (macOS) or executable (Windows) to swap in.
    app: PathBuf,
    work: tempfile::TempDir,
}

impl Staged {
    /// Leaves the files for the helper [`Install::hand_over`] started, which removes them.
    pub fn keep(self) {
        let _ = self.work.keep();
    }
}

fn extract_executable(archive: &Path, destination: &Path) -> Result<PathBuf, String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
    // Extract only the known executable; archive paths and extra files never reach the install.
    let mut entry = archive.by_name("better-matrix.exe").map_err(|error| error.to_string())?;
    if !entry.is_file() || entry.is_symlink() {
        return Err("The update has no executable".into());
    }
    let path = destination.join("better-matrix.exe");
    let mut file = File::create(&path).map_err(|error| error.to_string())?;
    std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests;
