//! Custom emoji and stickers: the image packs usable in a room (MSC2545, [`matrix_core::ImagePack`]),
//! asked of the engine once per room and run when something needs them (a `:` suggestion, the
//! emoji picker, the sticker and GIF picker, the pack manager), the stickers sent lately, and
//! sending a sticker.

use std::collections::{HashMap, HashSet};

use matrix_core::{AccountId, Command, ImagePack, MediaRef, Outgoing, PackImage};
use rune::Tx;

use super::{ComposerTarget, Popover, Purpose, State};

/// Each room's packs, once listed ("" for those usable anywhere), whether the account may add
/// one there, and which were asked for this run.
#[derive(Debug, Default)]
pub struct Packs {
    listed: HashMap<(AccountId, String), Vec<ImagePack>>,
    may_add: HashSet<(AccountId, String)>,
    asked: HashSet<(AccountId, String)>,
}

impl Packs {
    /// Takes a room's packs.
    pub fn listed(&mut self, account: AccountId, room_id: String, packs: Vec<ImagePack>, may_add: bool) {
        if may_add {
            self.may_add.insert((account.clone(), room_id.clone()));
        } else {
            self.may_add.remove(&(account.clone(), room_id.clone()));
        }
        self.listed.insert((account, room_id), packs);
    }

    /// Forgets an account's packs but those of `keep`, which stay listed until listed again.
    pub fn forget_but(&mut self, account: &AccountId, keep: &[String]) {
        self.listed.retain(|(owner, room), _| owner != account || keep.contains(room));
        self.asked.retain(|(owner, _)| owner != account);
    }

    /// Forgets an account's packs.
    pub fn forget(&mut self, account: &AccountId) {
        self.listed.retain(|(owner, _), _| owner != account);
        self.may_add.retain(|(owner, _)| owner != account);
        self.asked.retain(|(owner, _)| owner != account);
    }
}

/// How many stickers the picker's Recent row remembers.
const RECENT: usize = 16;

/// A sticker sent lately, as kept on this device: who sent it, and enough to send it again.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentSticker {
    #[serde(default)]
    pub account: String,
    pub shortcode: String,
    pub body: String,
    pub url: String,
    #[serde(default)]
    pub dimensions: Option<(u32, u32)>,
    #[serde(default)]
    pub mimetype: Option<String>,
}

/// The stickers sent lately on this device, the latest first, by account, for the picker's
/// Recent row.
/// Saved in this device's prefs ([`crate::platform::Shell::save_recent_stickers`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RecentStickers(Vec<RecentSticker>);

impl RecentStickers {
    /// Puts a sticker first among an account's, once.
    pub fn used(&mut self, account: &AccountId, image: &PackImage) {
        self.0.retain(|recent| recent.url != image.url || recent.account != account.as_str());
        self.0.insert(
            0,
            RecentSticker {
                account: account.as_str().to_owned(),
                shortcode: image.shortcode.clone(),
                body: image.body.clone(),
                url: image.url.clone(),
                dimensions: image.dimensions,
                mimetype: image.mimetype.clone(),
            },
        );
        let mut kept = 0;
        self.0.retain(|recent| {
            let own = recent.account == account.as_str();
            kept += usize::from(own);
            !own || kept <= RECENT
        });
    }

    /// An account's stickers, the latest first, as pack images to show and send.
    pub fn images(&self, account: &AccountId) -> Vec<PackImage> {
        self.0
            .iter()
            .filter(|recent| recent.account == account.as_str())
            .filter_map(|recent| {
                Some(PackImage {
                    shortcode: recent.shortcode.clone(),
                    body: recent.body.clone(),
                    url: recent.url.clone(),
                    media: MediaRef::from_mxc(&recent.url)?,
                    emoticon: false,
                    sticker: true,
                    dimensions: recent.dimensions,
                    mimetype: recent.mimetype.clone(),
                })
            })
            .collect()
    }
}

impl State {
    /// The packs usable in `room_id`, as last listed: the user's own first.
    pub fn image_packs<'a>(&'a self, account: &AccountId, room_id: &str) -> impl Iterator<Item = &'a ImagePack> + 'a {
        self.packs.listed.get(&(account.clone(), room_id.to_owned())).into_iter().flatten()
    }

    /// Whether the account may add a pack to `room_id`, as last listed.
    pub fn may_add_pack(&self, account: &AccountId, room_id: &str) -> bool {
        self.packs.may_add.contains(&(account.clone(), room_id.to_owned()))
    }

    /// The stickers usable in `room_id`, by pack, those whose short code or description holds
    /// `filter` (any case).
    pub fn sticker_packs<'a>(
        &'a self,
        account: &AccountId,
        room_id: &str,
        filter: &str,
    ) -> Vec<(&'a ImagePack, Vec<&'a PackImage>)> {
        let filter = filter.trim().to_lowercase();
        let matches = |image: &PackImage| {
            filter.is_empty()
                || image.shortcode.to_lowercase().contains(&filter)
                || image.body.to_lowercase().contains(&filter)
        };
        self.image_packs(account, room_id)
            .map(|pack| (pack, pack.images.iter().filter(|image| image.sticker && matches(image)).collect::<Vec<_>>()))
            .filter(|(_, stickers)| !stickers.is_empty())
            .collect()
    }

    /// The stickers an account sent lately that are still usable in `room_id` (their pack is
    /// one of the room's, the user's own or used everywhere), the latest first.
    pub fn recent_stickers_here(&self, account: &AccountId, room_id: &str) -> Vec<PackImage> {
        let usable: HashMap<&str, &PackImage> = self
            .image_packs(account, room_id)
            .flat_map(|pack| pack.images.iter().filter(|image| image.sticker).map(|image| (image.url.as_str(), image)))
            .collect();
        // Each as its pack has it now: a description changed since goes with it.
        self.recent_stickers
            .images(account)
            .into_iter()
            .filter_map(|image| usable.get(image.url.as_str()).map(|&current| current.clone()))
            .collect()
    }

    /// The custom emoji usable in `room_id`.
    pub fn custom_emoji<'a>(&'a self, account: &AccountId, room_id: &str) -> Vec<&'a PackImage> {
        self.image_packs(account, room_id).flat_map(|pack| pack.images.iter()).filter(|image| image.emoticon).collect()
    }

    /// Asks for a room's packs, once a run.
    pub(super) fn want_image_packs(&mut self, account: AccountId, room_id: String) {
        if self.packs.asked.insert((account.clone(), room_id.clone())) {
            self.list_image_packs(account, room_id);
        }
    }

    /// Asks for a room's packs ("": those usable anywhere) again.
    pub(super) fn list_image_packs(&mut self, account: AccountId, room_id: String) {
        self.packs.asked.insert((account.clone(), room_id.clone()));
        let command = Command::ListImagePacks { account, room_id: (!room_id.is_empty()).then_some(room_id) };
        self.dispatch(command, Purpose::Packs);
    }

    /// Opens the sticker and GIF picker for a composer, on the tab last used, listing the room's
    /// packs again: someone may have added stickers since.
    pub fn open_stickers(&mut self, target: ComposerTarget) {
        if self.popover == Some(Popover::Stickers(target)) {
            self.popover = None;
            // Its GIF previews go with it.
            self.request_gifs();
            return;
        }
        if let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned)) {
            self.list_image_packs(account, room_id);
        }
        self.popover = Some(Popover::Stickers(target));
        // GIPHY's answers aren't kept between visits: the GIF tab searches afresh.
        self.picker.gifs.reset();
        self.picker_opened();
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

    /// Sends a sticker from a composer's room or thread, answering what its draft answers, and
    /// puts it first among the recent ones.
    pub fn send_sticker(&mut self, target: ComposerTarget, image: PackImage, tx: &mut Tx) {
        self.popover = None;
        if let Some(account) = self.nav.account.clone() {
            self.recent_stickers.used(&account, &image);
        }
        self.shell.save_recent_stickers(self.recent_stickers.clone());
        let reply = self.composer_mut(target).reply_to.take();
        self.send_from(target, reply, vec![Outgoing::Sticker(image)], tx);
    }

    /// Fetches the images an open picker shows: the stickers and recent ones of the sticker
    /// picker, the custom emoji of the emoji picker for a composer, or every pack's images in
    /// the pack manager.
    pub(super) fn request_pack_images(&mut self) {
        if let Some(manager) = self.pack_manager.as_ref().filter(|_| matches!(self.modal, Some(super::Modal::Packs))) {
            let account = manager.account.clone();
            let room = manager.room_id.clone().unwrap_or_default();
            let wanted: Vec<WantedMedia> = self
                .image_packs(&account, &room)
                .flat_map(|pack| {
                    let avatar = pack.avatar.clone().map(|media| (media, PACK_IMAGE_SIZE, Some(PACK_IMAGE_PIXELS)));
                    pack.images.iter().map(wanted_pack_image).chain(avatar)
                })
                .collect();
            self.request_media(&account, wanted);
            return;
        }
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        let images: Vec<PackImage> = match self.popover {
            Some(Popover::Stickers(_)) => self
                .sticker_packs(&account, &room_id, "")
                .into_iter()
                .flat_map(|(_, images)| images)
                .cloned()
                .chain(self.recent_stickers_here(&account, &room_id))
                .collect(),
            Some(Popover::Emoji(super::EmojiFor::Composer(_))) => {
                self.custom_emoji(&account, &room_id).into_iter().cloned().collect()
            }
            _ => return,
        };
        let wanted = images.iter().map(wanted_pack_image).collect();
        self.request_media(&account, wanted);
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

/// How a pack image is fetched: a sticker as its file, as the timeline shows a sent sticker (the
/// cache keeps one rendition per image, and a server thumbnail would stop it moving); an
/// emoji-only image as a small thumbnail.
pub(super) fn wanted_pack_image(image: &PackImage) -> WantedMedia {
    if image.sticker {
        (image.media.clone(), matrix_core::MediaSize::Original, image.dimensions)
    } else {
        (image.media.clone(), PACK_IMAGE_SIZE, Some(PACK_IMAGE_PIXELS))
    }
}

/// Media to fetch: what, at which size, and its size in pixels once fetched, when known.
pub(super) type WantedMedia = (matrix_core::MediaRef, matrix_core::MediaSize, Option<(u32, u32)>);

/// The size pack images and custom emoji are fetched at.
pub const PACK_IMAGE_SIZE: matrix_core::MediaSize = matrix_core::MediaSize::Thumbnail { width: 128, height: 128 };
/// [`PACK_IMAGE_SIZE`] in pixels.
pub(super) const PACK_IMAGE_PIXELS: (u32, u32) = (128, 128);
