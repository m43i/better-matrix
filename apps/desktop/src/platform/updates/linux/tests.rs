use super::*;

fn packaged() -> Facts {
    Facts { executable: PathBuf::from(PACKAGED), ..Facts::default() }
}

fn command(method: &Method) -> String {
    match method.upgrade("v1.2.3") {
        Upgrade::Command(command) => command,
        other => panic!("no command: {other:?}"),
    }
}

#[test]
fn a_dpkg_install_should_update_through_apt() {
    assert_eq!(Method::detect(&Facts { dpkg: true, ..packaged() }), Method::Apt);
}

#[test]
fn an_rpm_install_on_fedora_should_update_through_dnf() {
    let facts = Facts { rpm: true, os_release: "ID=fedora\nVERSION_ID=41\n".into(), ..packaged() };
    assert_eq!(Method::detect(&facts), Method::Dnf);
}

#[test]
fn an_rpm_install_on_opensuse_should_update_through_zypper() {
    let facts =
        Facts { rpm: true, os_release: "ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n".into(), ..packaged() };
    assert_eq!(Method::detect(&facts), Method::Zypper);
}

#[test]
fn the_release_arch_package_should_update_through_pacman() {
    let facts = Facts { pacman: vec!["glibc".into(), "better-matrix".into()], ..packaged() };
    assert_eq!(Method::detect(&facts), Method::Pacman);
}

#[test]
fn a_pkgbuild_install_should_update_through_makepkg() {
    let facts = Facts { pacman: vec!["better-matrix-bin".into()], ..packaged() };
    assert_eq!(Method::detect(&facts), Method::Pkgbuild);
}

fn mounted(image: &str) -> Facts {
    Facts {
        executable: PathBuf::from("/tmp/.mount_betterAbc123/usr/bin/better-matrix"),
        appimage: Some(PathBuf::from(image)),
        appdir: Some(PathBuf::from("/tmp/.mount_betterAbc123")),
        ..Facts::default()
    }
}

#[test]
fn an_appimage_should_be_replaced_where_it_is() {
    let facts = mounted("/home/ada/.local/bin/better-matrix");
    assert_eq!(Method::detect(&facts), Method::AppImage(PathBuf::from("/home/ada/.local/bin/better-matrix")));
}

#[test]
fn an_appimage_variable_inherited_from_another_app_should_be_ignored() {
    let facts = Facts { executable: PathBuf::from(PACKAGED), dpkg: true, ..mounted("/home/ada/Apps/Editor.AppImage") };
    assert_eq!(Method::detect(&facts), Method::Apt);
}

#[test]
fn an_rpm_layered_on_an_ostree_system_should_be_unknown() {
    let facts =
        Facts { rpm: true, ostree: true, os_release: "ID=fedora\nVARIANT_ID=silverblue\n".into(), ..packaged() };
    assert_eq!(Method::detect(&facts), Method::Unknown);
}

#[test]
fn a_binary_outside_the_package_path_should_not_count_as_packaged() {
    let facts =
        Facts { executable: PathBuf::from("/home/ada/better-matrix/better-matrix"), dpkg: true, ..Facts::default() };
    assert_eq!(Method::detect(&facts), Method::Unknown);
}

#[test]
fn a_packaged_path_no_package_owns_should_be_unknown() {
    assert_eq!(Method::detect(&packaged()), Method::Unknown);
}

#[test]
fn pacman_folders_should_name_their_packages() {
    let local = tempfile::tempdir().unwrap();
    for folder in ["better-matrix-bin-0.0.11-1", "glibc-2.40+r16-1", "ALPM_DB_VERSION"] {
        std::fs::create_dir(local.path().join(folder)).unwrap();
    }
    let mut names = pacman_packages(local.path());
    names.sort();
    assert_eq!(names, ["better-matrix-bin", "glibc"]);
}

#[test]
fn the_apt_command_should_install_the_offered_release() {
    let arch = std::env::consts::ARCH;
    assert_eq!(
        command(&Method::Apt),
        format!(
            "mkdir -p ~/.cache/better-matrix && curl -fLo ~/.cache/better-matrix/better-matrix.deb 'https://github.com/m43i/better-matrix/releases/download/v1.2.3/better-matrix-linux-{arch}.deb' && sudo apt install -y ~/.cache/better-matrix/better-matrix.deb"
        )
    );
}

#[test]
fn the_dnf_command_should_install_the_rpm() {
    assert!(command(&Method::Dnf).ends_with("&& sudo dnf install -y ~/.cache/better-matrix/better-matrix.rpm"));
}

#[test]
fn the_zypper_command_should_accept_the_unsigned_rpm() {
    assert!(
        command(&Method::Zypper)
            .ends_with("&& sudo zypper install -y --allow-unsigned-rpm ~/.cache/better-matrix/better-matrix.rpm")
    );
}

#[test]
fn the_pacman_command_should_install_the_arch_package() {
    assert!(
        command(&Method::Pacman)
            .ends_with("&& sudo pacman -U --noconfirm ~/.cache/better-matrix/better-matrix.pkg.tar.xz")
    );
}

#[test]
fn the_pkgbuild_command_should_build_the_release_pkgbuild() {
    assert!(command(&Method::Pkgbuild).contains("/releases/download/v1.2.3/PKGBUILD' && makepkg -si"));
}

#[test]
fn the_appimage_command_should_replace_the_image_in_place() {
    let image = PathBuf::from("/home/ada/Apps/Better Matrix.AppImage");
    let command = command(&Method::AppImage(image));
    assert!(command.starts_with("curl -fLo '/home/ada/Apps/Better Matrix.AppImage.new' 'https://"));
    assert!(command.ends_with(
        "&& chmod +x '/home/ada/Apps/Better Matrix.AppImage.new' && mv '/home/ada/Apps/Better Matrix.AppImage.new' '/home/ada/Apps/Better Matrix.AppImage'"
    ));
}

#[test]
fn a_quote_in_a_path_should_stay_one_shell_word() {
    assert_eq!(quote("/home/o'brien/app"), r"'/home/o'\''brien/app'");
}

#[test]
fn an_unknown_install_should_link_the_release_page() {
    assert_eq!(
        Method::Unknown.upgrade("v1.2.3"),
        Upgrade::Page("https://github.com/m43i/better-matrix/releases/tag/v1.2.3".into())
    );
}

#[cfg(unix)]
#[test]
fn an_appimage_path_that_is_not_utf8_should_link_the_release_page() {
    use std::os::unix::ffi::OsStrExt;
    let image = PathBuf::from(std::ffi::OsStr::from_bytes(b"/home/ada/Better\xffMatrix.AppImage"));
    assert!(matches!(Method::AppImage(image).upgrade("v1.2.3"), Upgrade::Page(_)));
}

#[test]
fn an_appimage_path_with_a_backslash_should_link_the_release_page() {
    let image = PathBuf::from(r"/home/ada/Better\\Matrix.AppImage");
    assert!(matches!(Method::AppImage(image).upgrade("v1.2.3"), Upgrade::Page(_)));
}
