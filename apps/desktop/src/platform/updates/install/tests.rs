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
