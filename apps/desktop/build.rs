//! Compiles the message catalogs in `i18n/` into the binary (see `src/i18n.rs`) and embeds the
//! app icon and version info in the Windows executable (ADR 0001).

use std::{env, fs, io, path::Path};

fn main() -> io::Result<()> {
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("OUT_DIR is not set"))?;
    let out = Path::new(&out);
    rune_i18n::codegen::generate(Path::new("i18n"), out)?;
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources(out)?;
    }
    Ok(())
}

/// The app icon's resource ID, `APP_ICON` in `main.rs` too.
const APP_ICON: u16 = 1;

/// Links the app icon and package version into the `better-matrix` binary.
/// The numeric versions drop a pre-release (1.2.3-rc.1 is 1.2.3.0); the text keeps it.
fn windows_resources(out: &Path) -> io::Result<()> {
    let icon = "../../packaging/icons/better-matrix.ico";
    println!("cargo:rerun-if-changed={icon}");
    fs::copy(icon, out.join("better-matrix.ico"))?;

    let var = |name: &str| env::var(name).map_err(|_| io::Error::other(format!("{name} is not set")));
    let numeric = format!(
        "{},{},{},0",
        var("CARGO_PKG_VERSION_MAJOR")?,
        var("CARGO_PKG_VERSION_MINOR")?,
        var("CARGO_PKG_VERSION_PATCH")?
    );
    let version = var("CARGO_PKG_VERSION")?;
    let rc = out.join("better-matrix.rc");
    fs::write(
        &rc,
        format!(
            r#"{APP_ICON} ICON "better-matrix.ico"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Better Matrix contributors"
      VALUE "FileDescription", "Better Matrix"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "better-matrix"
      VALUE "LegalCopyright", "Copyright (c) 2026 Malte Eiting and the Better Matrix contributors"
      VALUE "OriginalFilename", "better-matrix.exe"
      VALUE "ProductName", "Better Matrix"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
        ),
    )?;
    embed_resource::compile_for(&rc, ["better-matrix"], embed_resource::ParamsIncludeDirs([out]))
        .manifest_required()
        .map_err(|error| io::Error::other(format!("the Windows resources didn't compile: {error}")))
}
