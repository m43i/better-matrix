//! The system file dialog: several files to attach, pictures for an image pack, one picture for
//! the profile or a room, or a key file to import. Rune's file dialog picks one file, so this uses rfd's async dialog, driven
//! from a worker thread: that is safe on every platform (on macOS rfd hops to the main thread
//! itself) and never blocks the window.

use rune::AppHandle;

use crate::state::{PickFor, State};

/// Picture types a profile picture or a pack image may be.
const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

pub fn pick(app: AppHandle<State>, picked_for: PickFor) {
    let spawned = std::thread::Builder::new().name("file-picker".into()).spawn(move || {
        let paths: Vec<_> = match picked_for {
            PickFor::Attachments(_) => {
                let dialog = rfd::AsyncFileDialog::new().set_title(crate::i18n::t!("dialog.attach_title"));
                let picked = pollster::block_on(dialog.pick_files()).unwrap_or_default();
                picked.iter().map(|file| file.path().to_owned()).collect()
            }
            PickFor::Avatar | PickFor::RoomAvatar { .. } => {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title(crate::i18n::t!("dialog.avatar_title"))
                    .add_filter(crate::i18n::t!("dialog.images"), &IMAGE_EXTENSIONS);
                pollster::block_on(dialog.pick_file()).into_iter().map(|file| file.path().to_owned()).collect()
            }
            PickFor::PackImages { .. } => {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title(crate::i18n::t!("dialog.pack_images_title"))
                    .add_filter(crate::i18n::t!("dialog.images"), &IMAGE_EXTENSIONS);
                let picked = pollster::block_on(dialog.pick_files()).unwrap_or_default();
                picked.iter().map(|file| file.path().to_owned()).collect()
            }
            PickFor::KeyFile => {
                let dialog = rfd::AsyncFileDialog::new().set_title(crate::i18n::t!("dialog.key_file_title"));
                pollster::block_on(dialog.pick_file()).into_iter().map(|file| file.path().to_owned()).collect()
            }
        };
        if !paths.is_empty() {
            app.update(move |s: &mut State, _| s.files_picked(picked_for, &paths));
        }
    });
    if let Err(error) = spawned {
        eprintln!("better-matrix: could not open the file picker: {error}");
    }
}

/// Asks where to save a file, suggesting `name`; `chosen` runs on the app with the path.
pub fn save(app: AppHandle<State>, name: String, chosen: impl FnOnce(&mut State, std::path::PathBuf) + Send + 'static) {
    let spawned = std::thread::Builder::new().name("save-dialog".into()).spawn(move || {
        let dialog = rfd::AsyncFileDialog::new().set_title(crate::i18n::t!("dialog.save_title")).set_file_name(&name);
        if let Some(file) = pollster::block_on(dialog.save_file()) {
            let path = file.path().to_owned();
            app.update(move |s: &mut State, _| chosen(s, path));
        }
    });
    if let Err(error) = spawned {
        eprintln!("better-matrix: could not open the save dialog: {error}");
    }
}
