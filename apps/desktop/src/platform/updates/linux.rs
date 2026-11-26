//! How a Linux copy was installed, and the command that updates it the same way. The app never
//! replaces a package manager's files: it offers the command, and the user runs it.

use std::path::{Path, PathBuf};

use super::Upgrade;

const RELEASES: &str = "https://github.com/m43i/better-matrix/releases";
/// Where the commands download to: private to the user, and the same in every shell.
const CACHE: &str = "~/.cache/better-matrix";
/// Where every package (`.deb`, `.rpm`, Arch's, the release's `PKGBUILD`) puts the binary.
const PACKAGED: &str = "/usr/bin/better-matrix";

/// The ways the README installs the app on Linux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    /// The `.deb`, through apt.
    Apt,
    /// The `.rpm`, through dnf (Fedora, RHEL and its rebuilds).
    Dnf,
    /// The `.rpm`, through zypper (openSUSE).
    Zypper,
    /// The Arch package, through `pacman -U`.
    Pacman,
    /// Built with `makepkg` from the release's `PKGBUILD` (`better-matrix-bin`).
    Pkgbuild,
    /// An AppImage, replaced where it is.
    AppImage(PathBuf),
    /// Anything else: a source build, an unpacked tarball, a package someone else made.
    Unknown,
}

/// What the system says about this copy, gathered by [`Facts::gather`].
#[derive(Debug, Default)]
pub struct Facts {
    /// The running executable.
    pub executable: PathBuf,
    /// `$APPIMAGE`, which the AppImage runtime sets to the image's path.
    pub appimage: Option<PathBuf>,
    /// `$APPDIR`, where the runtime mounted the image: an AppImage's executable is inside it.
    pub appdir: Option<PathBuf>,
    /// An ostree system (Fedora Silverblue and its kin), where rpm-ostree layers packages.
    pub ostree: bool,
    /// dpkg lists the `better-matrix` package as installed.
    pub dpkg: bool,
    /// The names of the packages pacman lists as installed.
    pub pacman: Vec<String>,
    /// rpm lists the `better-matrix` package as installed.
    pub rpm: bool,
    /// `/etc/os-release`.
    pub os_release: String,
}

impl Facts {
    /// Reads the package databases. Blocking: run it off the UI thread.
    pub fn gather() -> Self {
        let executable = std::env::current_exe().unwrap_or_default();
        let variable = |name| std::env::var_os(name).map(PathBuf::from).filter(|path| path.is_absolute());
        let packaged = executable == Path::new(PACKAGED);
        let pacman = if packaged { pacman_packages(Path::new("/var/lib/pacman/local")) } else { Vec::new() };
        Self {
            dpkg: packaged && Path::new("/var/lib/dpkg/info/better-matrix.list").is_file(),
            rpm: packaged
                && std::process::Command::new("rpm")
                    .args(["--query", "--quiet", "better-matrix"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success()),
            os_release: std::fs::read_to_string("/etc/os-release")
                .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
                .unwrap_or_default(),
            ostree: Path::new("/run/ostree-booted").exists(),
            appimage: variable("APPIMAGE"),
            // Resolved like the executable's path, which the kernel reports without symlinks.
            appdir: variable("APPDIR").map(|dir| std::fs::canonicalize(&dir).unwrap_or(dir)),
            executable,
            pacman,
        }
    }
}

/// pacman keeps one `name-version-release` folder per installed package.
fn pacman_packages(local: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(local) else { return Vec::new() };
    entries
        .filter_map(|entry| {
            let folder = entry.ok()?.file_name().into_string().ok()?;
            let mut parts = folder.rsplitn(3, '-');
            let (_release, _version, name) = (parts.next()?, parts.next()?, parts.next()?);
            Some(name.to_owned())
        })
        .collect()
}

impl Method {
    /// An AppImage first: it runs as itself even where a package is installed too. Its runtime's
    /// variables count only when this executable is inside the mounted image, not inherited from
    /// another AppImage that started it. A package counts only when the running binary is the
    /// one it installed, and not on ostree systems, whose packages rpm-ostree layers.
    pub fn detect(facts: &Facts) -> Self {
        if let (Some(image), Some(appdir)) = (&facts.appimage, &facts.appdir)
            && facts.executable.starts_with(appdir)
        {
            return Self::AppImage(image.clone());
        }
        if facts.executable != Path::new(PACKAGED) || facts.ostree {
            return Self::Unknown;
        }
        if facts.dpkg {
            return Self::Apt;
        }
        if facts.pacman.iter().any(|name| name == "better-matrix-bin") {
            return Self::Pkgbuild;
        }
        if facts.pacman.iter().any(|name| name == "better-matrix") {
            return Self::Pacman;
        }
        if facts.rpm {
            return if suse(&facts.os_release) { Self::Zypper } else { Self::Dnf };
        }
        Self::Unknown
    }

    /// The release asset the command downloads, which a release must carry to be offered.
    pub fn asset(&self) -> Option<String> {
        let arch = std::env::consts::ARCH;
        match self {
            Self::Apt => Some(format!("better-matrix-linux-{arch}.deb")),
            Self::Dnf | Self::Zypper => Some(format!("better-matrix-linux-{arch}.rpm")),
            Self::Pacman => Some(format!("better-matrix-linux-{arch}.pkg.tar.xz")),
            Self::Pkgbuild => Some("PKGBUILD".into()),
            Self::AppImage(_) => Some(format!("better-matrix-linux-{arch}.AppImage")),
            Self::Unknown => None,
        }
    }

    /// The README's install line for this method, pinned to the release `tag` rather than the
    /// moving "latest" link, so it installs the version the app offered. Packages download as the
    /// user (keeping their proxy settings) into the user's own cache, never a shared name in
    /// `/tmp` another user could hold, and without shell variables, so the line works in any
    /// shell (fish too).
    pub fn upgrade(&self, tag: &str) -> Upgrade {
        let page = || Upgrade::Page(format!("{RELEASES}/tag/{tag}"));
        let Some(asset) = self.asset() else { return page() };
        let url = quote(&format!("{RELEASES}/download/{tag}/{asset}"));
        // Fetches the package, then hands its file to the package manager.
        let install = |file: &str, manager: &str| {
            format!("mkdir -p {CACHE} && curl -fLo {CACHE}/{file} {url} && sudo {manager} {CACHE}/{file}")
        };
        Upgrade::Command(match self {
            Self::Apt => install("better-matrix.deb", "apt install -y"),
            Self::Dnf => install("better-matrix.rpm", "dnf install -y"),
            Self::Zypper => install("better-matrix.rpm", "zypper install -y --allow-unsigned-rpm"),
            Self::Pacman => install("better-matrix.pkg.tar.xz", "pacman -U --noconfirm"),
            Self::Pkgbuild => format!("mkdir -p {CACHE} && cd {CACHE} && curl -fLO {url} && makepkg -si"),
            // Download beside the image, then swap it in: the running copy keeps its old file. A
            // path that isn't UTF-8, or has a backslash (which fish reads inside single quotes),
            // can't be written out faithfully for every shell, so it gets the page instead.
            Self::AppImage(image) => {
                let Some(image) = image.to_str().filter(|image| !image.contains('\\')) else { return page() };
                let (staged, image) = (quote(&format!("{image}.new")), quote(image));
                format!("curl -fLo {staged} {url} && chmod +x {staged} && mv {staged} {image}")
            }
            Self::Unknown => return page(),
        })
    }
}

/// The page of the release tagged `v{version}`, which offers its downloads.
pub fn release_page(version: &str) -> String {
    format!("{RELEASES}/tag/v{version}")
}

/// openSUSE and SUSE Linux Enterprise name themselves, or what they are like, with "suse".
fn suse(os_release: &str) -> bool {
    os_release.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else { return false };
        matches!(key.trim(), "ID" | "ID_LIKE") && value.to_ascii_lowercase().contains("suse")
    })
}

/// One shell word: single quotes keep everything literal, and a quote inside closes, escapes
/// and reopens them.
fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests;
