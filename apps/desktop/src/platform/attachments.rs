//! Preparing image copies and opening downloaded files, always off the UI thread.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use rune::clipboard::{ClipboardContent, ClipboardImage};

use super::Shell;
use crate::i18n::{t, tf};

impl Shell {
    /// A fresh path under our private scratch folder, preserving only the file's basename.
    pub fn attachment_path(&self, name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name =
            name.rsplit(['/', '\\']).find(|part| !part.is_empty() && *part != "." && *part != "..").unwrap_or("file");
        let name: String = name
            .trim_end_matches(['.', ' '])
            .chars()
            .map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c })
            .collect();
        self.scratch.join(format!("attachment-{id}-{name}"))
    }

    /// Converts the original image to PNG, then hands the clipboard write to Rune.
    pub fn copy_image(&self, bytes: Arc<[u8]>) {
        let Some(app) = self.app.clone() else { return };
        let spawned = std::thread::Builder::new().name("copy-image".into()).spawn(move || {
            let result = clipboard_image(&bytes);
            app.update(move |s, tx| match result {
                Ok(image) => tx.copy_rich(ClipboardContent::default().with_image(image)),
                Err(error) => s.notice = Some(tf!("notice.copy_image", error = error)),
            });
        });
        if let Err(error) = spawned
            && let Some(app) = &self.app
        {
            let message = tf!("notice.copy_image", error = error);
            app.update(move |s, _| s.notice = Some(message));
        }
    }

    /// Opens a downloaded file with the platform's default application.
    pub fn open_file(&self, path: PathBuf) {
        let Some(app) = self.app.clone() else { return };
        let spawned = std::thread::Builder::new().name("open-file".into()).spawn(move || {
            let result = file_opener(path).status().and_then(|status| {
                if status.success() { Ok(()) } else { Err(std::io::Error::other(status.to_string())) }
            });
            if let Err(error) = result {
                app.update(move |s, _| s.notice = Some(tf!("notice.open_attachment", error = error)));
            }
        });
        if let Err(error) = spawned
            && let Some(app) = &self.app
        {
            let message = tf!("notice.open_attachment", error = error);
            app.update(move |s, _| s.notice = Some(message));
        }
    }
}

fn file_opener(path: PathBuf) -> std::process::Command {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(path);
    command
}

fn clipboard_image(bytes: &[u8]) -> Result<ClipboardImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    ClipboardImage::from_png(png.into_inner()).ok_or_else(|| t!("attachment.copy_too_large").to_owned())
}

#[cfg(test)]
mod tests;
