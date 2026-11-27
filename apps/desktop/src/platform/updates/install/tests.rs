use super::*;
use std::io::Write;

#[test]
fn a_mac_install_should_replace_the_whole_bundle() {
    assert_eq!(
        app_bundle(Path::new("/Applications/Better Matrix.app/Contents/MacOS/better-matrix")),
        Some(PathBuf::from("/Applications/Better Matrix.app"))
    );
}

#[test]
fn an_unbundled_mac_executable_should_not_be_updated() {
    assert!(app_bundle(Path::new("/usr/local/bin/better-matrix")).is_none());
}

#[test]
fn a_windows_archive_should_only_extract_the_known_executable() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("release.zip");
    let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
    zip.start_file("../escape", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(b"unwanted").unwrap();
    zip.start_file("better-matrix.exe", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(b"new executable").unwrap();
    zip.finish().unwrap();
    let destination = temp.path().join("stage");
    fs::create_dir(&destination).unwrap();
    let executable = extract_executable(&archive, &destination).unwrap();
    assert_eq!(fs::read(executable).unwrap(), b"new executable");
    assert!(!temp.path().join("escape").exists());
    assert_eq!(fs::read_dir(destination).unwrap().count(), 1);
}

#[test]
fn a_windows_archive_without_an_executable_should_be_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("release.zip");
    zip::ZipWriter::new(File::create(&path).unwrap()).finish().unwrap();
    assert!(extract_executable(&path, temp.path()).is_err());
}

#[test]
fn a_start_should_clear_staged_updates_no_helper_took() {
    let temp = tempfile::tempdir().unwrap();
    let install = Install { platform: Platform::Windows, target: temp.path().join("better-matrix.exe") };
    let root = temp.path().join(".better-matrix-update-staging");
    let (abandoned, handed, unrelated) =
        (root.join("old"), root.join("run"), temp.path().join(".better-matrix-update-x"));
    for folder in [&abandoned, &handed, &unrelated] {
        fs::create_dir_all(folder).unwrap();
    }
    File::create(handed.join("ready")).unwrap();
    install.clear_stale();
    assert!(!abandoned.exists() && handed.exists() && unrelated.exists());
}

/// Live: downloads v0.0.17's Windows archive from GitHub
/// (`cargo test -p better-matrix-desktop -- --ignored staging`).
#[test]
#[ignore = "downloads a release from GitHub"]
fn staging_a_release_should_unpack_it_beside_the_install_until_dropped() {
    let client = release::client().unwrap();
    let metadata: release::Metadata = client
        .get("https://api.github.com/repos/m43i/better-matrix/releases/tags/v0.0.17")
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::json)
        .unwrap();
    let found = release::select(metadata, "0.0.16", &release::Target::Archive(Platform::Windows)).unwrap().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let install = Install { platform: Platform::Windows, target: folder.path().join("better-matrix.exe") };
    let staged = install.stage(&client, &found, Duration::from_secs(60)).unwrap();
    assert_eq!(staged.version, "0.0.17");
    assert!(fs::metadata(&staged.app).unwrap().len() > 1_000_000, "the executable is unpacked");
    let work = staged.app.parent().unwrap().to_owned();
    assert_eq!(
        work.parent(),
        Some(folder.path().join(".better-matrix-update-staging").as_path()),
        "staged beside the install"
    );
    drop(staged);
    assert!(!work.exists(), "an update never handed over leaves nothing behind");
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_the_staging_folder_should_never_be_cleared() {
    let (temp, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let install = Install { platform: Platform::Windows, target: temp.path().join("better-matrix.exe") };
    let theirs = elsewhere.path().join("theirs");
    fs::create_dir(&theirs).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), temp.path().join(".better-matrix-update-staging")).unwrap();
    install.clear_stale();
    assert!(theirs.exists());
}
