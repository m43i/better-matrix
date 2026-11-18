//! Compiles the message catalogs in `i18n/` into the binary (see `src/i18n.rs`) and, for Windows,
//! gives the executable its version resource: the product name and version that Explorer shows
//! and that code signing checks before it signs (ADR 0001).

use std::{env, fs, io, path::Path};

fn main() -> io::Result<()> {
    let out = env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("OUT_DIR is not set"))?;
    let out = Path::new(&out);
    rune_i18n::codegen::generate(Path::new("i18n"), out)?;
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        version_resource(out)?;
    }
    Ok(())
}

/// Writes `version.rc` from the package version and links it into the `better-matrix` binary.
/// The numeric versions drop a pre-release (1.2.3-rc.1 is 1.2.3.0); the text keeps it.
fn version_resource(out: &Path) -> io::Result<()> {
    let var = |name: &str| env::var(name).map_err(|_| io::Error::other(format!("{name} is not set")));
    let numeric = format!(
        "{},{},{},0",
        var("CARGO_PKG_VERSION_MAJOR")?,
        var("CARGO_PKG_VERSION_MINOR")?,
        var("CARGO_PKG_VERSION_PATCH")?
    );
    let version = var("CARGO_PKG_VERSION")?;
    let rc = out.join("version.rc");
    fs::write(
        &rc,
        format!(
            r#"1 VERSIONINFO
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
    embed_resource::compile_for(&rc, ["better-matrix"], embed_resource::NONE)
        .manifest_required()
        .map_err(|error| io::Error::other(format!("the version resource didn't compile: {error}")))
}
