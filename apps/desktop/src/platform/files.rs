//! File dialogs requested through Rune so their answers arrive on the app's thread.

use std::path::PathBuf;

use rune::{FileDialogFilesResult, FileDialogOptions, Tx};

use super::Shell;
use crate::i18n::t;
use crate::state::{PickFor, State};

const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

#[derive(Clone, Copy)]
enum Dialog {
    Open,
    OpenMany,
    Save,
}

impl Shell {
    /// Opens a file dialog and routes every answer to its original destination.
    pub fn pick_files(&mut self, tx: &mut Tx, picked_for: PickFor) {
        let (title, images, dialog) = match &picked_for {
            PickFor::Attachments { .. } => (t!("dialog.attach_title"), false, Dialog::OpenMany),
            PickFor::Avatar(_) | PickFor::RoomAvatar { .. } => (t!("dialog.avatar_title"), true, Dialog::Open),
            PickFor::PackImages { .. } => (t!("dialog.pack_images_title"), true, Dialog::OpenMany),
            PickFor::KeyFile(_) => (t!("dialog.key_file_title"), false, Dialog::Open),
        };
        let mut options = FileDialogOptions { title: title.into(), ..FileDialogOptions::default() };
        if images {
            options = options.filter(t!("dialog.images"), &IMAGE_EXTENSIONS);
        }
        self.request_file_dialog(tx, dialog, options, move |s, paths| s.files_picked(picked_for, &paths));
    }

    /// Asks where to save a file, suggesting `name`; `chosen` runs with the path once picked.
    pub fn pick_save_path(
        &mut self,
        tx: &mut Tx,
        name: &str,
        chosen: impl FnOnce(&mut State, PathBuf) + Send + 'static,
    ) {
        let options = FileDialogOptions {
            title: t!("dialog.save_title").into(),
            file_name: Some(name.into()),
            ..FileDialogOptions::default()
        };
        self.request_file_dialog(tx, Dialog::Save, options, move |s, paths| {
            if let Some(path) = paths.into_iter().next() {
                chosen(s, path);
            }
        });
    }

    fn request_file_dialog(
        &mut self,
        tx: &mut Tx,
        dialog: Dialog,
        options: FileDialogOptions,
        chosen: impl FnOnce(&mut State, Vec<PathBuf>) + Send + 'static,
    ) {
        #[cfg(all(unix, not(target_os = "macos")))]
        let fallback = match &self.app {
            Some(app) => {
                let Some(cancelled) = self.file_dialog.begin() else { return };
                Some((app.clone(), options.clone(), cancelled))
            }
            None => None,
        };
        let answer = move |s: &mut State, result: FileDialogFilesResult| {
            #[cfg(all(unix, not(target_os = "macos")))]
            if fallback.as_ref().is_some_and(|(_, _, cancelled)| cancelled.is_closed()) {
                return;
            }
            match result {
                Ok(Some(paths)) if !paths.is_empty() => chosen(s, paths),
                Ok(_) => {}
                Err(error) => {
                    eprintln!("better-matrix: could not show the file dialog: {error}");
                    #[cfg(all(unix, not(target_os = "macos")))]
                    if portal_unavailable(&error)
                        && let Some((app, options, cancelled)) = fallback
                    {
                        open_fallback(app, dialog, options, cancelled, chosen);
                    }
                }
            }
        };
        match dialog {
            Dialog::Open => {
                tx.open_file(options, move |s, result, _| answer(s, result.map(|path| path.map(|path| vec![path]))));
            }
            Dialog::OpenMany => tx.open_files(options, move |s, result, _| answer(s, result)),
            Dialog::Save => {
                tx.save_file(options, move |s, result, _| answer(s, result.map(|path| path.map(|path| vec![path]))));
            }
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn portal_unavailable(error: &rune::FileDialogError) -> bool {
    // Rune's file dialog errors carry a message only.
    matches!(
        error.0.as_str(),
        "no XDG desktop portal FileChooser is available" | "native file dialogs aren't supported on this platform"
    )
}

/// One picker through its Rune request and any Zenity fallback. Closing the window
/// drops the sender, so a fallback stops and a queued answer is discarded.
#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Default)]
pub(super) struct DialogScope(Option<async_channel::Sender<()>>);

#[cfg(all(unix, not(target_os = "macos")))]
impl DialogScope {
    fn begin(&mut self) -> Option<async_channel::Receiver<()>> {
        if self.0.as_ref().is_some_and(|cancel| !cancel.is_closed()) {
            return None;
        }
        let (cancel, cancelled) = async_channel::bounded(1);
        self.0 = Some(cancel);
        Some(cancelled)
    }

    pub(super) fn cancel(&mut self) {
        self.0 = None;
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_fallback(
    app: rune::AppHandle<State>,
    dialog: Dialog,
    options: FileDialogOptions,
    cancelled: async_channel::Receiver<()>,
    chosen: impl FnOnce(&mut State, Vec<PathBuf>) + Send + 'static,
) {
    use futures_lite::future::{block_on, race};

    let started = std::thread::Builder::new().name("file-dialog-fallback".into()).spawn(move || {
        let result = block_on(race(
            async {
                let _ = cancelled.recv().await;
                None
            },
            async { Some(zenity(dialog, options).await) },
        ));
        match result {
            Some(Ok(paths)) if !paths.is_empty() => app.update(move |s, _| {
                if !cancelled.is_closed() {
                    chosen(s, paths);
                }
            }),
            Some(Err(error)) => eprintln!("better-matrix: file dialog fallback failed: {error}"),
            _ => {}
        }
    });
    if let Err(error) = started {
        eprintln!("better-matrix: could not start the file dialog fallback: {error}");
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
async fn zenity(dialog: Dialog, options: FileDialogOptions) -> std::io::Result<Vec<PathBuf>> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    // Zenity appends a newline. The multiple selection separator is used only in that mode.
    const SEPARATOR: &str = "\u{1f}";
    let mut command = async_process::Command::new("zenity");
    command.args(["--no-markup", "--file-selection", "--title", &options.title]).kill_on_drop(true);
    match dialog {
        Dialog::Open => {}
        Dialog::OpenMany => {
            command.args(["--multiple", "--separator", SEPARATOR]);
        }
        Dialog::Save => {
            command.args(["--save", "--confirm-overwrite"]);
        }
    }
    if let Some(name) = options.file_name {
        command.args(["--filename", &name]);
    }
    for (name, extensions) in options.filters {
        let patterns = extensions.iter().map(|ext| format!("*.{ext}")).collect::<Vec<_>>().join(" ");
        command.args(["--file-filter", &format!("{name} | {patterns}")]);
    }
    let output = command.output().await?;
    let paths = match output.status.code() {
        Some(0) => output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout),
        Some(1) => return Ok(Vec::new()), // Cancel.
        _ => return Err(std::io::Error::other(format!("Zenity exited with {:?}", output.status))),
    };
    let field = |bytes: &[u8]| PathBuf::from(OsStr::from_bytes(bytes));
    if paths.is_empty() {
        Ok(Vec::new())
    } else if matches!(dialog, Dialog::OpenMany) {
        Ok(paths.split(|byte| *byte == SEPARATOR.as_bytes()[0]).map(field).collect())
    } else {
        Ok(vec![field(paths)])
    }
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn a_busy_rune_dialog_should_not_start_the_fallback() {
        assert!(!portal_unavailable(&rune::FileDialogError("a file dialog is already open".into())));
    }

    #[test]
    fn an_unavailable_portal_should_allow_the_fallback() {
        assert!(portal_unavailable(&rune::FileDialogError("no XDG desktop portal FileChooser is available".into())));
    }

    #[test]
    fn closing_the_window_should_cancel_its_fallback_dialog() {
        let mut dialog = DialogScope::default();
        let receiver = dialog.begin().unwrap();
        dialog.cancel();
        assert!(receiver.is_closed());
    }
    #[test]
    fn a_picker_should_keep_the_window_busy_until_its_answer_is_delivered() {
        let mut dialog = DialogScope::default();
        let receiver = dialog.begin().unwrap();
        assert!(dialog.begin().is_none());
        drop(receiver);
        assert!(dialog.begin().is_some());
    }
}
