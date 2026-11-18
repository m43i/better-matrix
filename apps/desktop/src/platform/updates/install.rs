//! Stage beside the install (same filesystem, writable before quitting), then hand over to a
//! helper. Its ready file proves the script started before the app is asked to quit.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Release;
use super::release::{self, Platform};

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
            Platform::Mac => bundle(&executable).ok_or("Updates require an installed Better Matrix.app")?,
            Platform::Windows => executable,
        };
        Ok(Some(Self { platform, target }))
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    pub fn prepare(
        &self,
        client: &reqwest::blocking::Client,
        release: &Release,
        error_file: &Path,
    ) -> Result<(), String> {
        let parent = self.target.parent().ok_or("The install has no parent directory")?;
        let work = tempfile::Builder::new()
            .prefix(".better-matrix-update-")
            .tempdir_in(parent)
            .map_err(|error| format!("The install folder must be writable: {error}"))?;
        let archive = work.path().join("release.zip");
        let mut file = File::create(&archive).map_err(|error| error.to_string())?;
        release::download(client, release, &mut file)?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        let staged = match self.platform {
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
            .arg(staged)
            .arg(std::process::id().to_string())
            .arg(work.path())
            .arg(error_file)
            .arg(&release.version)
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
                // The helper owns cleanup now; its paths stay alive across our process exit.
                let _ = work.keep();
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
}

fn bundle(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
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
