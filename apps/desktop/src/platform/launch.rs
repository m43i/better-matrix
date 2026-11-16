//! Permalinks the app is launched with (`better-matrix matrix:r/rust:matrix.org`, as the desktop
//! entry or the Windows registry start it for a link), and handing them to the app's running
//! process.
//!
//! Rune keeps one process per app (`rune::single_instance`) and passes a later launch's links to
//! the running one, but only links shaped `scheme://…`: `https://matrix.to/#/…` reaches
//! `App::on_open_url`, an opaque `matrix:r/…` URI doesn't (the launch only reopens the app). So
//! a launch writes the permalinks Rune won't pass into a handoff folder, private to the user,
//! before it asks for the instance; the running process reads them when Rune reopens it
//! (`App::on_reopen`), and a launch that becomes the running process reads them itself. macOS
//! hands every link to `on_open_url` and never starts a second process.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use matrix_core::Permalink;

/// A link handed on longer ago than this was never taken (its launch was killed, or the app it
/// went to quit): it is dropped rather than opened out of the blue.
const FRESH: Duration = Duration::from_secs(60);

/// The permalinks among `args` that Rune doesn't pass on by itself.
pub fn links(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter().filter(|arg| Permalink::is_permalink(arg) && !rune_passes_on(arg)).collect()
}

/// Whether Rune takes `arg` for a link (`scheme://…`, a scheme of two or more letters, digits,
/// `+`, `-` or `.`, starting with a letter) and gives it to `App::on_open_url` itself.
fn rune_passes_on(arg: &str) -> bool {
    let Some((scheme, _)) = arg.split_once("://") else { return false };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Writes `links` into `dir` for the running process, each in a file of its own, named by the
/// time and this process so they read back in order. Each is written whole before it shows.
pub fn hand_on(dir: &Path, links: &[String]) -> std::io::Result<()> {
    make_private_dir(dir)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let pid = std::process::id();
    for (i, link) in links.iter().enumerate() {
        let name = format!("{now:039}-{pid}-{i:04}");
        let partial = dir.join(format!(".{name}.partial"));
        std::fs::write(&partial, link)?;
        std::fs::rename(&partial, dir.join(format!("{name}.link")))?;
    }
    Ok(())
}

/// Takes every link handed on in `dir`, oldest first, and removes them all; ones older than
/// [`FRESH`] are dropped.
pub fn take(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "link"))
        .collect();
    files.sort();
    let mut links = Vec::new();
    for path in files {
        let fresh = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age < FRESH));
        let link = std::fs::read_to_string(&path);
        let _ = std::fs::remove_file(&path);
        if let (true, Ok(link)) = (fresh, link)
            && !links.contains(&link)
        {
            links.push(link);
        }
    }
    links
}

fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("better-matrix-launch-{}-{}", std::process::id(), rand_suffix()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn rand_suffix() -> u128 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos()
    }

    #[test]
    fn an_opaque_matrix_uri_should_be_handed_on() {
        assert_eq!(links(["matrix:r/rust:matrix.org".to_owned()]), ["matrix:r/rust:matrix.org"]);
    }

    #[test]
    fn a_matrix_to_link_should_be_left_to_rune() {
        assert!(links(["https://matrix.to/#/#rust:matrix.org".to_owned()]).is_empty());
    }

    #[test]
    fn arguments_that_are_not_permalinks_should_be_ignored() {
        assert!(links(["--snap".to_owned(), "shot.png".to_owned(), "matrix:nothing".to_owned()]).is_empty());
    }

    #[test]
    fn links_handed_on_should_be_taken_once_in_order() {
        let dir = folder();
        hand_on(&dir, &["matrix:r/a:example.org".into()]).unwrap();
        hand_on(&dir, &["matrix:r/b:example.org".into()]).unwrap();
        assert_eq!(take(&dir), ["matrix:r/a:example.org", "matrix:r/b:example.org"]);
        assert!(take(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_link_handed_on_twice_should_be_taken_once() {
        let dir = folder();
        hand_on(&dir, &["matrix:u/grace:example.org".into(), "matrix:u/grace:example.org".into()]).unwrap();
        assert_eq!(take(&dir), ["matrix:u/grace:example.org"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_handed_on_should_take_nothing() {
        assert!(take(&folder()).is_empty());
    }
}
