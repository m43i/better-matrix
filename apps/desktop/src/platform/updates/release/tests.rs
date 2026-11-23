use super::*;

fn metadata(version: &str) -> Metadata {
    serde_json::from_value(serde_json::json!({
        "tag_name": version, "draft": false, "prerelease": false,
        "assets": [
            { "id": 1, "name": "better-matrix-macos-arm64.zip", "digest": format!("sha256:{}", "a".repeat(64)) },
            { "id": 2, "name": "better-matrix-windows-x86_64.zip", "digest": format!("sha256:{}", "b".repeat(64)) },
            { "id": 3, "name": "better-matrix-linux-x86_64.deb", "digest": null },
            { "id": 4, "name": "better-matrix-linux-aarch64.deb", "digest": null }
        ]
    }))
    .unwrap()
}

fn archive(release: Release) -> u64 {
    match release.upgrade {
        Upgrade::Archive { asset_id, .. } => asset_id,
        other => panic!("not an archive: {other:?}"),
    }
}

#[test]
fn a_newer_version_should_use_numeric_semantic_order() {
    let release = select(metadata("v0.0.10"), "0.0.9", &Target::Archive(Platform::Windows)).unwrap().unwrap();
    assert_eq!(release.version, "0.0.10");
}

#[test]
fn a_mac_update_should_select_the_bundle_archive() {
    assert_eq!(archive(select(metadata("v1.0.0"), "0.0.7", &Target::Archive(Platform::Mac)).unwrap().unwrap()), 1);
}

#[test]
fn a_windows_update_should_select_the_executable_archive() {
    assert_eq!(archive(select(metadata("v1.0.0"), "0.0.7", &Target::Archive(Platform::Windows)).unwrap().unwrap()), 2);
}

#[test]
fn the_current_version_should_not_offer_an_update() {
    assert!(select(metadata("v0.0.7"), "0.0.7", &Target::Archive(Platform::Windows)).unwrap().is_none());
}

#[test]
fn an_older_version_should_not_offer_a_downgrade() {
    assert!(select(metadata("v0.0.6"), "0.0.7", &Target::Archive(Platform::Mac)).unwrap().is_none());
}

#[test]
fn build_metadata_should_not_make_an_update_newer() {
    assert!(select(metadata("v0.0.7+new"), "0.0.7+old", &Target::Archive(Platform::Windows)).unwrap().is_none());
}

#[test]
fn prereleases_should_not_be_offered() {
    assert!(select(metadata("v1.0.0-rc.1"), "0.0.7", &Target::Archive(Platform::Windows)).unwrap().is_none());
}

#[test]
fn releases_flagged_as_prerelease_should_not_be_offered() {
    let mut metadata = metadata("v1.0.0");
    metadata.prerelease = true;
    assert!(select(metadata, "0.0.7", &Target::Archive(Platform::Mac)).unwrap().is_none());
}

#[test]
fn drafts_should_not_be_offered() {
    let mut metadata = metadata("v1.0.0");
    metadata.draft = true;
    assert!(select(metadata, "0.0.7", &Target::Archive(Platform::Mac)).unwrap().is_none());
}

#[test]
fn a_release_without_a_matching_asset_should_not_be_offered() {
    let mut metadata = metadata("v1.0.0");
    metadata.assets.clear();
    assert!(select(metadata, "0.0.7", &Target::Archive(Platform::Mac)).unwrap().is_none());
}

#[test]
fn a_release_without_a_digest_should_not_be_offered() {
    let mut metadata = metadata("v1.0.0");
    metadata.assets[0].digest = None;
    assert!(select(metadata, "0.0.7", &Target::Archive(Platform::Mac)).unwrap().is_none());
}

#[test]
fn an_invalid_digest_should_be_rejected() {
    let mut metadata = metadata("v1.0.0");
    metadata.assets[0].digest = Some("sha256:nope".into());
    assert!(select(metadata, "0.0.7", &Target::Archive(Platform::Mac)).is_err());
}

#[test]
fn a_verified_download_should_preserve_the_archive_bytes() {
    let bytes = b"a release archive";
    let digest = format!("sha256:{:x}", Sha256::digest(bytes));
    let mut target = Vec::new();
    copy_verified(&bytes[..], &mut target, &digest).unwrap();
    assert_eq!(target, bytes);
}

#[test]
fn a_corrupt_download_should_fail_verification() {
    let digest = format!("sha256:{:x}", Sha256::digest(b"a release archive"));
    assert!(copy_verified(&b"corrupt"[..], &mut Vec::new(), &digest).is_err());
}

#[test]
fn a_linux_package_update_should_offer_its_command_for_the_release() {
    let release = select(metadata("v1.0.0"), "0.0.7", &Target::Linux(Method::Apt)).unwrap().unwrap();
    let Upgrade::Command(command) = release.upgrade else { panic!("no command") };
    assert!(command.contains("/releases/download/v1.0.0/better-matrix-linux-"));
}

#[test]
fn a_linux_release_without_the_installed_package_should_not_be_offered() {
    assert!(select(metadata("v1.0.0"), "0.0.7", &Target::Linux(Method::Dnf)).unwrap().is_none());
}

#[test]
fn an_unknown_linux_install_should_be_sent_to_the_release_page() {
    let mut metadata = metadata("v1.0.0");
    metadata.assets.clear();
    let release = select(metadata, "0.0.7", &Target::Linux(Method::Unknown)).unwrap().unwrap();
    assert_eq!(release.upgrade, Upgrade::Page("https://github.com/m43i/better-matrix/releases/tag/v1.0.0".into()));
}

#[test]
fn a_linux_install_should_not_be_offered_an_older_release() {
    assert!(select(metadata("v0.0.6"), "0.0.7", &Target::Linux(Method::Apt)).unwrap().is_none());
}
