//! A tray entry for the closed main window. Its actions go through Rune's app handle.

#[cfg(target_os = "linux")]
#[path = "tray/linux.rs"]
mod linux;
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[path = "tray/native.rs"]
mod native;

use rune::AppHandle;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use crate::i18n::t;
use crate::state::State;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const TITLE: &str = "Better Matrix";

#[cfg(target_os = "linux")]
pub use linux::Tray;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::Tray;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub struct Tray;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn show(app: AppHandle<State>) -> Option<Tray> {
    Tray::show(app).inspect_err(|error| eprintln!("better-matrix: could not show the system tray icon: {error}")).ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn show(_: AppHandle<State>) -> Option<Tray> {
    None
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn icon() -> image::ImageResult<image::RgbaImage> {
    #[cfg(target_os = "macos")]
    let png = include_bytes!("../../../../packaging/icons/better-matrix-tray-template.png");
    #[cfg(not(target_os = "macos"))]
    let png = include_bytes!("../../../../packaging/icons/better-matrix-256.png");
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)?.into_rgba8();
    if cfg!(target_os = "macos") {
        return Ok(image);
    }
    Ok(image::imageops::resize(&image, 32, 32, image::imageops::FilterType::Triangle))
}
