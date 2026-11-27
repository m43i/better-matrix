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
    pub fn pick_files(&self, tx: &mut Tx, picked_for: PickFor) {
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
    pub fn pick_save_path(&self, tx: &mut Tx, name: &str, chosen: impl FnOnce(&mut State, PathBuf) + Send + 'static) {
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
        &self,
        tx: &mut Tx,
        dialog: Dialog,
        options: FileDialogOptions,
        chosen: impl FnOnce(&mut State, Vec<PathBuf>) + Send + 'static,
    ) {
        #[cfg(all(unix, not(target_os = "macos")))]
        let fallback = self.app.clone().map(|app| (app, options.clone()));
        let answer = move |s: &mut State, result: FileDialogFilesResult| match result {
            Ok(Some(paths)) if !paths.is_empty() => chosen(s, paths),
            Ok(_) => {}
            Err(error) => {
                eprintln!("better-matrix: could not show the file dialog: {error}");
                #[cfg(all(unix, not(target_os = "macos")))]
                if let Some((app, options)) = fallback {
                    open_fallback(app, dialog, options, chosen);
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

/// Rune uses the portal on Linux. rfd preserves access through Zenity when the portal is absent.
#[cfg(all(unix, not(target_os = "macos")))]
fn open_fallback(
    app: rune::AppHandle<State>,
    dialog: Dialog,
    options: FileDialogOptions,
    chosen: impl FnOnce(&mut State, Vec<PathBuf>) + Send + 'static,
) {
    use futures_lite::future::block_on;

    let started = std::thread::Builder::new().name("file-dialog-fallback".into()).spawn(move || {
        let mut picker = rfd::AsyncFileDialog::new().set_title(options.title);
        if let Some(name) = options.file_name {
            picker = picker.set_file_name(name);
        }
        for (name, extensions) in options.filters {
            picker = picker.add_filter(name, &extensions);
        }
        let files = match dialog {
            Dialog::Open => block_on(picker.pick_file()).into_iter().collect(),
            Dialog::OpenMany => block_on(picker.pick_files()).unwrap_or_default(),
            Dialog::Save => block_on(picker.save_file()).into_iter().collect(),
        };
        if !files.is_empty() {
            let paths = files.iter().map(|file| file.path().to_owned()).collect();
            app.update(move |s, _| chosen(s, paths));
        }
    });
    if let Err(error) = started {
        eprintln!("better-matrix: could not start the file dialog fallback: {error}");
    }
}
