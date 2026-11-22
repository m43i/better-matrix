//! Custom emoji and stickers: the image packs usable in a room (MSC2545, [`matrix_core::ImagePack`]),
//! asked of the engine once per room and run when something needs them (a `:` suggestion, the
//! emoji or sticker picker), and sending a sticker.

use std::collections::{HashMap, HashSet};

use matrix_core::{AccountId, Command, ImagePack, Outgoing, PackImage};
use rune::Tx;

use super::{ComposerTarget, Popover, Purpose, State};

/// Each room's packs, once listed, and which were asked for this run.
#[derive(Debug, Default)]
pub struct Packs {
    listed: HashMap<(AccountId, String), Vec<ImagePack>>,
    asked: HashSet<(AccountId, String)>,
}

impl Packs {
    /// Takes a room's packs.
    pub fn listed(&mut self, account: AccountId, room_id: String, packs: Vec<ImagePack>) {
        self.listed.insert((account, room_id), packs);
    }

    /// Forgets an account's packs.
    pub fn forget(&mut self, account: &AccountId) {
        self.listed.retain(|(owner, _), _| owner != account);
        self.asked.retain(|(owner, _)| owner != account);
    }
}

impl State {
    /// The packs usable in `room_id`, as last listed.
    pub fn image_packs<'a>(&'a self, account: &AccountId, room_id: &str) -> impl Iterator<Item = &'a ImagePack> + 'a {
        self.packs.listed.get(&(account.clone(), room_id.to_owned())).into_iter().flatten()
    }

    /// The stickers usable in `room_id`, by pack.
    pub fn sticker_packs<'a>(&'a self, account: &AccountId, room_id: &str) -> Vec<(&'a ImagePack, Vec<&'a PackImage>)> {
        self.image_packs(account, room_id)
            .map(|pack| (pack, pack.images.iter().filter(|image| image.sticker).collect::<Vec<_>>()))
            .filter(|(_, stickers)| !stickers.is_empty())
            .collect()
    }

    /// The custom emoji usable in `room_id`.
    pub fn custom_emoji<'a>(&'a self, account: &AccountId, room_id: &str) -> Vec<&'a PackImage> {
        self.image_packs(account, room_id).flat_map(|pack| pack.images.iter()).filter(|image| image.emoticon).collect()
    }

    /// Asks for a room's packs, once a run.
    pub(super) fn want_image_packs(&mut self, account: AccountId, room_id: String) {
        if self.packs.asked.insert((account.clone(), room_id.clone())) {
            let command = Command::ListImagePacks { account, room_id: Some(room_id) };
            self.dispatch(command, Purpose::Packs);
        }
    }

    /// Opens the sticker picker for a composer, listing the room's packs again: someone may
    /// have added stickers since.
    pub fn open_stickers(&mut self, target: ComposerTarget) {
        if self.popover == Some(Popover::Stickers(target)) {
            self.popover = None;
            return;
        }
        if let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned)) {
            self.packs.asked.remove(&(account.clone(), room_id.clone()));
            self.want_image_packs(account, room_id);
        }
        self.popover = Some(Popover::Stickers(target));
        self.request_pack_images();
    }

    /// Opens or closes the emoji picker of a composer, with the room's custom emoji.
    pub fn open_composer_emoji(&mut self, target: ComposerTarget) {
        self.toggle_popover(Popover::Emoji(super::EmojiFor::Composer(target)));
        if let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned)) {
            self.want_image_packs(account, room_id);
        }
        self.request_pack_images();
    }

    /// Puts a custom emoji where a composer's caret is, as a pill, and goes back to writing.
    pub fn choose_custom_emoji(&mut self, target: ComposerTarget, image: &PackImage) {
        self.popover = None;
        let edit = &mut self.composer_mut(target).edit;
        edit.insert_pill(format!("emote:{}", image.url), &format!(":{}:", image.shortcode));
        edit.focused = true;
    }

    /// Sends a sticker from a composer's room or thread, answering what its draft answers.
    pub fn send_sticker(&mut self, target: ComposerTarget, image: PackImage, tx: &mut Tx) {
        self.popover = None;
        let reply = self.composer_mut(target).reply_to.take();
        self.send_from(target, reply, vec![Outgoing::Sticker(image)], tx);
    }

    /// Fetches the images an open picker shows: stickers, or the custom emoji of the emoji
    /// picker for a composer.
    pub(super) fn request_pack_images(&mut self) {
        if !matches!(self.popover, Some(Popover::Stickers(_) | Popover::Emoji(super::EmojiFor::Composer(_)))) {
            return;
        }
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        let images: Vec<PackImage> = match self.popover {
            Some(Popover::Stickers(_)) => {
                self.sticker_packs(&account, &room_id).into_iter().flat_map(|(_, images)| images).cloned().collect()
            }
            _ => self.custom_emoji(&account, &room_id).into_iter().cloned().collect(),
        };
        let wanted = images.into_iter().map(|image| (image.media, PACK_IMAGE_SIZE, Some(PACK_IMAGE_PIXELS)));
        self.request_media(&account, wanted.collect());
    }

    /// Fetches media not here yet, each at the size given, as the timeline's are fetched.
    pub(super) fn request_media(&mut self, account: &AccountId, wanted: Vec<WantedMedia>) {
        let now = std::time::Instant::now();
        self.media.touch(wanted.iter().map(|(media, _, _)| media));
        let mut seen = HashSet::new();
        for (media, size, dimensions) in wanted {
            if !self.media.wants(&media, now) || !seen.insert(media.clone()) {
                continue;
            }
            self.media.loading(media.clone());
            let command = Command::FetchMedia { account: account.clone(), media: media.clone(), size };
            self.dispatch(command, Purpose::Media { media, dimensions });
        }
    }
}

/// Media to fetch: what, at which size, and its size in pixels once fetched, when known.
pub(super) type WantedMedia = (matrix_core::MediaRef, matrix_core::MediaSize, Option<(u32, u32)>);

/// The size pack images and custom emoji are fetched at.
pub const PACK_IMAGE_SIZE: matrix_core::MediaSize = matrix_core::MediaSize::Thumbnail { width: 128, height: 128 };
/// [`PACK_IMAGE_SIZE`] in pixels.
const PACK_IMAGE_PIXELS: (u32, u32) = (128, 128);
