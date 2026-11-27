//! The sticker manager (`Modal::Packs`): the user's own pack, the open room's packs and those the
//! user uses in all their rooms, one at a time. In a pack the user may change, stickers are
//! added from files, renamed, described, made usable as stickers, emoji or both, made the pack's
//! picture or removed, and the pack renamed; a room's pack can be deleted, and used in all the
//! user's rooms. A room with the right to gets new packs. Each change goes to the engine
//! (`Command::EditImagePack`), and the packs are listed again once it is through.

use matrix_core::{
    AccountId, Command, ImagePack, MediaRef, NewPackImage, NewPackImageSource, PackEdit, PackImage, PackImageChange,
    PackPlace, RequestId,
};
use rune::TextEdit;

use super::{Modal, PickFor, Purpose, State};
use crate::i18n::t;

/// The manager's state, while [`Modal::Packs`] shows.
#[derive(Debug)]
pub struct PackManager {
    pub account: AccountId,
    /// The room whose packs show beside the user's own, if it opened from one.
    pub room_id: Option<String>,
    /// The pack shown, by [`ImagePack::id`].
    pub selected: String,
    /// The shown pack's name, as typed.
    pub name: TextEdit,
    /// The image whose details are open.
    pub image: Option<ImageForm>,
    /// A new room pack being named.
    pub new_pack: Option<TextEdit>,
    /// Delete was pressed once: pressing it again deletes.
    pub deleting: bool,
    /// The change on its way, if any.
    pub busy: Option<RequestId>,
    /// The new room pack on its way, shown once made.
    pub creating: Option<RequestId>,
}

/// An image's details as being changed.
#[derive(Debug)]
pub struct ImageForm {
    /// Its short code before the change.
    pub shortcode: String,
    pub code: TextEdit,
    pub body: TextEdit,
    pub emoticon: bool,
    pub sticker: bool,
}

impl ImageForm {
    fn of(image: &PackImage) -> Self {
        let body = if image.body == image.shortcode { String::new() } else { image.body.clone() };
        Self {
            shortcode: image.shortcode.clone(),
            code: TextEdit::new(&image.shortcode),
            body: TextEdit::new(&body),
            emoticon: image.emoticon,
            sticker: image.sticker,
        }
    }
}

/// A text field of the sticker manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackField {
    /// The shown pack's name.
    Name,
    /// A new room pack's name.
    NewPack,
    /// The open image's short code.
    Shortcode,
    /// The open image's description.
    Description,
}

impl State {
    /// Opens the sticker manager for the active account, with the open room's packs.
    pub fn open_pack_manager(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        let room_id = self.active_room_id().map(str::to_owned);
        self.popover = None;
        let mut name = TextEdit::default();
        name.focused = false;
        self.pack_manager = Some(PackManager {
            account: account.clone(),
            room_id: room_id.clone(),
            selected: "user".to_owned(),
            name,
            image: None,
            new_pack: None,
            deleting: false,
            busy: None,
            creating: None,
        });
        self.modal = Some(Modal::Packs);
        self.list_image_packs(account, room_id.unwrap_or_default());
        self.select_pack("user");
    }

    /// One of the sticker manager's fields; a scratch field when it doesn't show.
    pub fn pack_field(&mut self, field: PackField) -> &mut TextEdit {
        let manager = self.pack_manager.as_mut();
        let edit = match field {
            PackField::Name => manager.map(|manager| &mut manager.name),
            PackField::NewPack => manager.and_then(|manager| manager.new_pack.as_mut()),
            PackField::Shortcode => manager.and_then(|manager| manager.image.as_mut()).map(|form| &mut form.code),
            PackField::Description => manager.and_then(|manager| manager.image.as_mut()).map(|form| &mut form.body),
        };
        match edit {
            Some(edit) => edit,
            None => &mut self.scratch,
        }
    }

    /// The packs the manager lists: the user's own, the room's, then those used everywhere.
    pub fn managed_packs(&self) -> Vec<&ImagePack> {
        let Some(manager) = self.pack_manager.as_ref().filter(|_| matches!(self.modal, Some(Modal::Packs))) else {
            return Vec::new();
        };
        self.image_packs(&manager.account, manager.room_id.as_deref().unwrap_or_default()).collect()
    }

    /// The pack the manager shows.
    pub fn managed_pack(&self) -> Option<&ImagePack> {
        let manager = self.pack_manager.as_ref()?;
        self.managed_packs().into_iter().find(|pack| pack.id == manager.selected)
    }

    /// Whether a new pack can be made in the manager's room.
    pub fn may_add_managed_pack(&self) -> bool {
        self.pack_manager.as_ref().is_some_and(|manager| {
            manager.room_id.as_deref().is_some_and(|room| self.may_add_pack(&manager.account, room))
        })
    }

    /// Shows a pack in the manager.
    pub fn select_pack(&mut self, id: &str) {
        let name = self.managed_packs().into_iter().find(|pack| pack.id == id).map(|pack| pack.name.clone());
        let Some(manager) = &mut self.pack_manager else { return };
        manager.selected = id.to_owned();
        manager.name = TextEdit::new(name.unwrap_or_default());
        manager.image = None;
        manager.new_pack = None;
        manager.deleting = false;
        self.request_pack_images();
    }

    /// The packs were listed again: the shown pack's name follows, unless it is being typed.
    pub(super) fn packs_relisted(&mut self) {
        let name = self.managed_pack().map(|pack| pack.name.clone());
        if let (Some(manager), Some(name)) = (&mut self.pack_manager, name)
            && !manager.name.focused
        {
            manager.name = TextEdit::new(&name);
        }
    }

    /// Sends a change of the shown pack, or of the pack at `place`.
    fn edit_pack(&mut self, place: PackPlace, edit: PackEdit, added: bool) {
        let Some(manager) = &self.pack_manager else { return };
        let account = manager.account.clone();
        let command = Command::EditImagePack { account: account.clone(), pack: place, edit };
        let request = self.dispatch(command, Purpose::PackEdit { account, added });
        if let Some(manager) = &mut self.pack_manager {
            manager.busy = Some(request);
        }
    }

    /// The shown pack's place, if the user may change it.
    fn editable_place(&self) -> Option<PackPlace> {
        self.managed_pack().filter(|pack| pack.editable).map(|pack| pack.place.clone())
    }

    /// Renames the shown pack as typed.
    pub fn rename_pack(&mut self) {
        let Some(place) = self.editable_place() else { return };
        let Some(manager) = &mut self.pack_manager else { return };
        manager.name.focused = false;
        let name = manager.name.text.trim().to_owned();
        if self.managed_pack().is_some_and(|pack| pack.name == name) {
            return;
        }
        self.edit_pack(place, PackEdit::Rename(name), false);
    }

    /// Asks for pictures to add to the shown pack.
    pub fn pick_pack_images(&mut self, tx: &mut rune::Tx) {
        let (Some(place), Some(manager)) = (self.editable_place(), &self.pack_manager) else { return };
        self.shell.pick_files(tx, PickFor::PackImages { account: manager.account.clone(), pack: place });
    }

    /// Adds the pictures picked to a pack, as stickers.
    pub(super) fn add_pack_images(&mut self, pack: PackPlace, paths: &[std::path::PathBuf]) {
        let images = paths
            .iter()
            .map(|path| NewPackImage {
                source: NewPackImageSource::File(path.clone()),
                shortcode: None,
                body: None,
                emoticon: false,
                sticker: true,
            })
            .collect();
        self.edit_pack(pack, PackEdit::AddImages(images), false);
    }

    /// Opens an image's details.
    pub fn open_pack_image(&mut self, shortcode: &str) {
        let form = self
            .managed_pack()
            .filter(|pack| pack.editable)
            .and_then(|pack| pack.images.iter().find(|image| image.shortcode == shortcode))
            .map(ImageForm::of);
        if let Some(manager) = &mut self.pack_manager {
            manager.image = form;
        }
    }

    /// Closes the image's details without changing it.
    pub fn close_pack_image(&mut self) {
        if let Some(manager) = &mut self.pack_manager {
            manager.image = None;
        }
    }

    /// Flips whether the open image is usable as a sticker (`sticker`) or as an emoji; one of
    /// the two stays on.
    pub fn toggle_pack_image_use(&mut self, sticker: bool) {
        let Some(form) = self.pack_manager.as_mut().and_then(|manager| manager.image.as_mut()) else { return };
        if sticker {
            form.sticker = !form.sticker || !form.emoticon;
        } else {
            form.emoticon = !form.emoticon || !form.sticker;
        }
    }

    /// Saves the open image's details.
    pub fn save_pack_image(&mut self) {
        let Some(place) = self.editable_place() else { return };
        let Some(form) = self.pack_manager.as_mut().and_then(|manager| manager.image.take()) else { return };
        let image = PackImageChange {
            shortcode: form.code.text.trim().to_owned(),
            body: form.body.text.trim().to_owned(),
            emoticon: form.emoticon,
            sticker: form.sticker,
        };
        if !matrix_core::packs::valid_shortcode(&image.shortcode) {
            self.notice = Some(t!("packs.invalid_shortcode").to_owned());
            if let Some(manager) = &mut self.pack_manager {
                manager.image = Some(form);
            }
            return;
        }
        self.edit_pack(place, PackEdit::ChangeImage { shortcode: form.shortcode, image }, false);
    }

    /// Takes the open image out of its pack.
    pub fn remove_pack_image(&mut self) {
        let Some(place) = self.editable_place() else { return };
        let Some(form) = self.pack_manager.as_mut().and_then(|manager| manager.image.take()) else { return };
        self.edit_pack(place, PackEdit::RemoveImage(form.shortcode), false);
    }

    /// Makes the open image the pack's picture.
    pub fn use_as_pack_picture(&mut self) {
        let Some(place) = self.editable_place() else { return };
        let shortcode =
            self.pack_manager.as_ref().and_then(|manager| manager.image.as_ref()).map(|f| f.shortcode.clone());
        let url = self
            .managed_pack()
            .and_then(|pack| pack.images.iter().find(|image| Some(&image.shortcode) == shortcode.as_ref()))
            .map(|image| image.url.clone());
        if let Some(manager) = &mut self.pack_manager {
            manager.image = None;
        }
        if let Some(url) = url {
            self.edit_pack(place, PackEdit::SetAvatar(Some(url)), false);
        }
    }

    /// Uses the shown room pack in all the user's rooms, or stops.
    pub fn use_pack_everywhere(&mut self, on: bool) {
        let Some(pack) = self.managed_pack() else { return };
        if matches!(pack.place, PackPlace::Room { .. }) {
            let place = pack.place.clone();
            self.edit_pack(place, PackEdit::UseEverywhere(on), false);
        }
    }

    /// Deletes the shown room pack: the first press asks, the second deletes.
    pub fn delete_pack(&mut self) {
        let Some(place) = self.editable_place().filter(|place| matches!(place, PackPlace::Room { .. })) else {
            return;
        };
        let Some(manager) = &mut self.pack_manager else { return };
        if !manager.deleting {
            manager.deleting = true;
            return;
        }
        manager.deleting = false;
        manager.selected = "user".to_owned();
        self.edit_pack(place, PackEdit::Delete, false);
    }

    /// Starts naming a new pack for the manager's room.
    pub fn start_new_pack(&mut self) {
        if !self.may_add_managed_pack() {
            return;
        }
        if let Some(manager) = &mut self.pack_manager {
            let mut name = TextEdit::default();
            name.focused = true;
            manager.new_pack = Some(name);
            manager.image = None;
        }
    }

    /// Makes the new room pack as named, and shows it.
    pub fn create_pack(&mut self) {
        let Some(manager) = &self.pack_manager else { return };
        let (Some(room_id), Some(name)) = (manager.room_id.clone(), manager.new_pack.as_ref()) else { return };
        let name = name.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        if let Some(manager) = &mut self.pack_manager {
            manager.new_pack = None;
        }
        // The engine picks the new pack's state key; the pack shows once made.
        self.edit_pack(PackPlace::Room { room_id, state_key: String::new() }, PackEdit::Create(name), false);
        if let Some(manager) = &mut self.pack_manager {
            manager.creating = manager.busy;
        }
    }

    /// A pack change went through: a pack the manager just made shows.
    pub(super) fn pack_made(&mut self, request: RequestId, pack: &PackPlace) {
        let Some(manager) = self.pack_manager.as_mut().filter(|manager| manager.creating == Some(request)) else {
            return;
        };
        manager.creating = None;
        if let PackPlace::Room { room_id, state_key } = pack {
            let id = format!("{room_id}/{state_key}");
            if manager.selected != id && manager.room_id.as_deref() == Some(room_id.as_str()) {
                manager.selected = id;
                manager.name.focused = false;
            }
        }
    }

    /// Adds a sticker or picture someone sent to the user's own pack, as a sticker; one from an
    /// encrypted message asks first, as its copy goes up unencrypted.
    pub fn add_to_my_stickers(&mut self, media: MediaRef, name: String, dimensions: Option<(u32, u32)>) {
        self.popover = None;
        if media.is_encrypted() {
            self.modal = Some(Modal::Confirm(super::Confirm::CopyToStickers { media, name, dimensions }));
            return;
        }
        self.copy_to_my_stickers(media, name, dimensions);
    }

    /// Adds a picture to the user's own pack, as a sticker.
    pub(super) fn copy_to_my_stickers(&mut self, media: MediaRef, name: String, dimensions: Option<(u32, u32)>) {
        let Some(account) = self.nav.account.clone() else { return };
        let image = NewPackImage {
            source: NewPackImageSource::Media { media, name, dimensions },
            shortcode: None,
            body: None,
            emoticon: false,
            sticker: true,
        };
        let command = Command::EditImagePack {
            account: account.clone(),
            pack: PackPlace::User,
            edit: PackEdit::AddImages(vec![image]),
        };
        self.dispatch(command, Purpose::PackEdit { account, added: true });
    }

    /// A pack change went through (or failed): the packs shown are listed again, and others
    /// are listed when next needed.
    pub(super) fn pack_edited(&mut self, request: RequestId, account: AccountId, added: bool, error: Option<String>) {
        if let Some(manager) = &mut self.pack_manager
            && manager.busy == Some(request)
        {
            manager.busy = None;
        }
        match error {
            Some(error) => self.notice = Some(crate::i18n::tf!("notice.pack_edit", error = error)),
            None if added => self.notice = Some(t!("packs.added").to_owned()),
            None => {}
        }
        let mut wanted: Vec<String> = Vec::new();
        if let Some(manager) = self.pack_manager.as_ref().filter(|manager| manager.account == account) {
            wanted.push(manager.room_id.clone().unwrap_or_default());
        }
        if self.nav.account.as_ref() == Some(&account)
            && let Some(room) = self.active_room_id()
        {
            wanted.push(room.to_owned());
        }
        wanted.dedup();
        self.packs.forget_but(&account, &wanted);
        for room in wanted {
            self.list_image_packs(account.clone(), room);
        }
    }
}
