//! Decoded media for the timeline, bounded by the bytes its decoded images hold. Rune decodes
//! on its own worker and wakes the window when an image is ready; this tracks what was asked
//! for, counts each image at an estimate until it is decoded and at its exact size after
//! ([`MediaCache::measure`]), and evicts the oldest entries past the budget. A failed download
//! is tried again after a wait ([`Retries`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use matrix_core::{MediaRef, Upload};
use rune::{Image, ImageCache, Waker};

use super::retry::Retries;

/// 64 MiB of decoded pixels.
const BUDGET_BYTES: usize = 64 << 20;
/// The most an animation's frames may hold before Rune keeps only its first (its own limit).
const ANIMATION_BYTES: usize = 256 << 20;
/// The most failed downloads remembered; past it, the longest ago go first.
const FAILURES_KEPT: usize = 512;

#[derive(Debug)]
enum Entry {
    Loading,
    Ready {
        image: Image,
        /// What it counts for in the budget: an estimate until decoded, then exact.
        bytes: usize,
        /// The frames in the file, for an animation's size once decoded.
        frames: usize,
        measured: bool,
        /// When it was last used, as a place in [`MediaCache::used`].
        used: u64,
    },
    /// Downloaded but not an image Rune can decode: trying again would fetch the same bytes.
    Undecodable,
}

#[derive(Default)]
pub struct MediaCache {
    decoder: Option<ImageCache>,
    entries: HashMap<MediaRef, Entry>,
    /// Ready entries by when they were last used, least recently first, for eviction.
    used: BTreeMap<u64, MediaRef>,
    /// The next use's place in `used`.
    uses: u64,
    /// Images eviction leaves, and the budget doesn't count: the pictures lists show.
    kept: HashSet<MediaRef>,
    /// Ready entries still counted at their estimate.
    unmeasured: Vec<MediaRef>,
    bytes: usize,
    retries: Retries<MediaRef>,
}

impl std::fmt::Debug for MediaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaCache").field("entries", &self.entries.len()).field("bytes", &self.bytes).finish()
    }
}

/// The bytes Rune holds for a decoded still image of `width` by `height`: RGBA at full size and
/// at every halving down to one pixel (its mip chain).
pub fn decoded_bytes(width: u32, height: u32) -> usize {
    let (mut w, mut h, mut total) = (width.max(1) as usize, height.max(1) as usize, 0);
    loop {
        total += w * h * 4;
        if w == 1 && h == 1 {
            return total;
        }
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
    }
}

/// The bytes Rune holds for an image of `frames` frames: every frame's mip chain, or the first
/// frame's alone when Rune drops the rest. Rune decides that by its own estimate of a frame
/// (`width * height * 16 / 3`), not by the exact chain, so this decides by the same rule.
fn animation_bytes(width: u32, height: u32, frames: usize) -> usize {
    let one = decoded_bytes(width, height);
    let estimate = (width as usize).saturating_mul(height as usize).saturating_mul(16) / 3;
    if frames > 1 && frames.saturating_mul(estimate) > ANIMATION_BYTES {
        one
    } else {
        one.saturating_mul(frames.max(1))
    }
}

/// How many frames an image file holds, read from its structure without decoding pixels: a
/// GIF's image blocks, an APNG's frame count, an animated WebP's frame chunks. Anything else
/// (or anything malformed) is one frame.
pub fn frame_count(bytes: &[u8]) -> usize {
    let frames = if bytes.starts_with(b"GIF8") {
        gif_frames(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_frames(bytes)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        webp_frames(bytes)
    } else {
        None
    };
    frames.unwrap_or(1).max(1)
}

/// Walks a GIF's blocks, skipping their data sub-blocks, counting image descriptors.
fn gif_frames(bytes: &[u8]) -> Option<usize> {
    // Skips a run of data sub-blocks (a length byte, then that many bytes) ending in an empty one.
    let sub_blocks = |mut at: usize| -> Option<usize> {
        loop {
            let length = usize::from(*bytes.get(at)?);
            at += 1 + length;
            if length == 0 {
                return Some(at);
            }
        }
    };
    let color_table = |packed: u8| if packed & 0x80 != 0 { 3 << ((packed & 7) + 1) } else { 0 };
    let mut at = 13 + color_table(*bytes.get(10)?);
    let mut frames = 0;
    loop {
        // A file cut short plays the frames it has whole.
        let next = match bytes.get(at) {
            Some(0x21) => sub_blocks(at + 2),
            Some(0x2C) => {
                // The descriptor, its colour table, the LZW code size, then the pixel data.
                let next = bytes.get(at + 9).and_then(|packed| sub_blocks(at + 10 + color_table(*packed) + 1));
                frames += usize::from(next.is_some());
                next
            }
            _ => None,
        };
        match next {
            Some(next) => at = next,
            None => return Some(frames),
        }
    }
}

/// An APNG's frame count from its `acTL` chunk, which comes before the image data.
fn png_frames(bytes: &[u8]) -> Option<usize> {
    let mut at = 8;
    loop {
        let length = u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        match bytes.get(at + 4..at + 8)? {
            b"acTL" => return Some(u32::from_be_bytes(bytes.get(at + 8..at + 12)?.try_into().ok()?) as usize),
            b"IDAT" => return Some(1),
            _ => at += 12 + length,
        }
    }
}

/// An animated WebP's `ANMF` chunks; a still WebP has none.
fn webp_frames(bytes: &[u8]) -> Option<usize> {
    let mut at = 12;
    let mut frames = 0;
    while let Some(header) = bytes.get(at..at + 8) {
        let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        if &header[..4] == b"ANMF" {
            frames += 1;
        }
        at += 8 + length + (length & 1);
    }
    Some(frames)
}

/// The size of the image a fetch of `size` brings, for an original of `dimensions`: a thumbnail
/// fits its box, keeping the aspect ratio, and is never larger than the original.
pub fn fetched_size(size: matrix_core::MediaSize, dimensions: Option<(u32, u32)>) -> Option<(u32, u32)> {
    match (size, dimensions) {
        (matrix_core::MediaSize::Original, dimensions) => dimensions,
        (matrix_core::MediaSize::Thumbnail { width, height }, None) => Some((width, height)),
        (matrix_core::MediaSize::Thumbnail { width, height }, Some((w, h))) => {
            let scale = (f64::from(width) / f64::from(w.max(1))).min(f64::from(height) / f64::from(h.max(1))).min(1.0);
            let fit = |side: u32| (f64::from(side) * scale).ceil() as u32;
            Some((fit(w), fit(h)))
        }
    }
}

/// The size of an image fetched for a timeline preview, for the budget: `size` is what
/// [`matrix_core::Attachment::preview`] asks for. A sender's own thumbnail comes as `Original`,
/// but it is a thumbnail all the same, so it counts as fitting the preview box, not as the full
/// image whose `dimensions` the attachment carries.
pub fn preview_size(size: matrix_core::MediaSize, dimensions: Option<(u32, u32)>) -> Option<(u32, u32)> {
    match size {
        matrix_core::MediaSize::Original => fetched_size(matrix_core::MediaSize::PREVIEW, dimensions),
        thumbnail => fetched_size(thumbnail, dimensions),
    }
}

impl MediaCache {
    /// Gives the cache a decoder that wakes the window; without one (headless) nothing loads.
    pub fn attach(&mut self, waker: Waker) {
        self.decoder = Some(ImageCache::new(waker));
    }

    /// Whether images can load at all: there is a decoder (none headless).
    pub fn can_load(&self) -> bool {
        self.decoder.is_some()
    }

    /// Whether `media` has to be fetched at `now`: it isn't here, and if it failed, its wait
    /// is over.
    pub fn wants(&self, media: &MediaRef, now: Instant) -> bool {
        self.decoder.is_some() && !self.entries.contains_key(media) && self.retries.may_try(media, now)
    }

    pub fn loading(&mut self, media: MediaRef) {
        self.entries.insert(media, Entry::Loading);
    }

    /// A download failed: it is tried again after a wait, when something shows it.
    pub fn failed(&mut self, media: MediaRef, now: Instant) {
        if let Some(Entry::Ready { bytes, used, .. }) = self.entries.remove(&media) {
            self.bytes -= bytes;
            self.used.remove(&used);
        }
        self.retries.failed(media, now);
        self.retries.keep_latest(FAILURES_KEPT);
    }

    /// Lets a failed download be tried again at once, its wait forgotten: media the user just
    /// chose to show.
    pub fn try_again(&mut self, media: &MediaRef) {
        self.retries.succeeded(media);
    }

    /// When the next failed download may be tried again, after `now`.
    pub fn next_retry(&self, now: Instant) -> Option<Instant> {
        self.retries.next_after(now)
    }

    /// Decodes downloaded bytes. `dimensions` sizes the estimate it counts at until decoded.
    pub fn loaded(&mut self, media: MediaRef, encoded: std::sync::Arc<[u8]>, dimensions: Option<(u32, u32)>) {
        let Some(decoder) = &self.decoder else { return };
        self.retries.succeeded(&media);
        let frames = frame_count(&encoded);
        let bytes = dimensions.map_or(encoded.len() * 4, |(w, h)| animation_bytes(w, h, frames));
        let image = decoder.load(encoded);
        let used = self.next_use();
        let replaced = self.entries.insert(media.clone(), Entry::Ready { image, bytes, frames, measured: false, used });
        if let Some(Entry::Ready { bytes, used, .. }) = replaced {
            self.bytes -= bytes;
            self.used.remove(&used);
        }
        self.used.insert(used, media.clone());
        self.unmeasured.push(media);
        self.bytes += bytes;
        self.evict();
    }

    /// Counts the images decoded since the last call at their exact size, and drops the ones
    /// that could not be decoded. Cheap when nothing is waiting; call after every wake.
    pub fn measure(&mut self) -> bool {
        if self.unmeasured.is_empty() {
            return false;
        }
        let mut changed = false;
        let entries = &mut self.entries;
        let mut total = self.bytes;
        let mut broken = Vec::new();
        self.unmeasured.retain(|media| {
            let Some(Entry::Ready { image, bytes, frames, measured, .. }) = entries.get_mut(media) else {
                return false;
            };
            if !image.is_ready() {
                return true;
            }
            changed = true;
            match image.size() {
                Some([width, height]) => {
                    let exact = animation_bytes(width, height, *frames);
                    total = total - *bytes + exact;
                    *bytes = exact;
                    *measured = true;
                }
                None => broken.push(media.clone()),
            }
            false
        });
        self.bytes = total;
        for media in broken {
            if let Some(Entry::Ready { bytes, used, .. }) = self.entries.insert(media, Entry::Undecodable) {
                self.bytes -= bytes;
                self.used.remove(&used);
            }
        }
        self.evict();
        changed
    }

    fn next_use(&mut self) -> u64 {
        self.uses += 1;
        self.uses
    }

    /// Past the budget, lets go of the least recently used images but the newest. Kept images
    /// (the pictures lists show) stay and don't count: they are bounded by the lists, and
    /// letting them go would only fetch them again when the lists next change, over and over
    /// once they alone outgrew the budget. The budget bounds everything else.
    fn evict(&mut self) {
        if self.bytes <= BUDGET_BYTES {
            return;
        }
        let kept: usize = self
            .kept
            .iter()
            .filter_map(|media| match self.entries.get(media) {
                Some(Entry::Ready { bytes, .. }) => Some(*bytes),
                _ => None,
            })
            .sum();
        let mut others = self.bytes - kept;
        while others > BUDGET_BYTES {
            let mut unkept = self.used.iter().filter(|(_, media)| !self.kept.contains(*media));
            let Some((&oldest, _)) = unkept.next() else { break };
            // The newest stays even if it alone outgrows the budget.
            if unkept.next().is_none() {
                break;
            }
            let Some(media) = self.used.remove(&oldest) else { break };
            if let Some(Entry::Ready { bytes, .. }) = self.entries.remove(&media) {
                self.bytes -= bytes;
                others -= bytes;
            }
        }
    }

    /// Decodes an image without keeping it here: for one shown on its own, like a preview.
    pub fn decode(&self, encoded: std::sync::Arc<[u8]>) -> Option<Image> {
        self.decoder.as_ref().map(|decoder| decoder.load(encoded))
    }

    /// An image upload, read and decoded on Rune's worker: a file is read there, never here.
    /// `None` without a decoder (headless) or for an upload that isn't an image.
    pub fn upload(&self, upload: &Upload) -> Option<Image> {
        let decoder = self.decoder.as_ref().filter(|_| is_image(upload))?;
        Some(match upload {
            Upload::Path(path) => decoder.file(path, THUMBNAIL_SIZE, 2.0),
            Upload::Bytes { bytes, .. } => decoder.load(std::sync::Arc::clone(bytes)),
        })
    }

    /// The image, once decoded.
    pub fn image(&self, media: &MediaRef) -> Option<&Image> {
        match self.entries.get(media)? {
            Entry::Ready { image, .. } if image.is_ready() && image.error().is_none() => Some(image),
            _ => None,
        }
    }

    /// Whether `media` can't be shown: its last download failed (and isn't being tried again),
    /// or it isn't an image.
    pub fn has_failed(&self, media: &MediaRef) -> bool {
        match self.entries.get(media) {
            // While a retry loads, it still shows as failed, so the row doesn't change height
            // back and forth on every attempt.
            None | Some(Entry::Loading) => self.retries.has_failed(media),
            Some(entry) => matches!(entry, Entry::Undecodable),
        }
    }

    /// Marks images as just used: eviction takes the least recently used first.
    pub fn touch<'a>(&mut self, media: impl Iterator<Item = &'a MediaRef>) {
        for media in media {
            let now = self.uses + 1;
            let Some(Entry::Ready { used, .. }) = self.entries.get_mut(media) else { continue };
            if let Some(media) = self.used.remove(used) {
                self.used.insert(now, media);
            }
            *used = now;
            self.uses = now;
        }
    }

    /// The images that stay outside the budget: the pictures lists show (rooms, spaces,
    /// invitations, accounts). Those no longer kept count again, and may go.
    pub fn keep(&mut self, kept: HashSet<MediaRef>) {
        self.kept = kept;
        self.evict();
    }

    /// Lets go of `media` as eviction would, for tests of what comes back after.
    #[cfg(test)]
    pub fn forget(&mut self, media: &MediaRef) {
        if let Some(Entry::Ready { bytes, used, .. }) = self.entries.remove(media) {
            self.bytes -= bytes;
            self.used.remove(&used);
        }
    }
}

/// The size an upload thumbnail draws at, in points: SVGs rasterize for it.
pub const THUMBNAIL_SIZE: f32 = 64.0;

/// Whether an upload is a picture, by its name: it shows as a thumbnail in the composer.
pub fn is_image(upload: &Upload) -> bool {
    upload.mime().type_() == "image"
}

/// The composers' upload thumbnails: one decoded picture per image upload waiting to be sent,
/// let go once the upload is sent, removed or set aside with its draft.
#[derive(Debug, Default)]
pub struct UploadThumbnails {
    shown: Vec<Thumbnail>,
}

#[derive(Debug)]
struct Thumbnail {
    upload: Upload,
    image: Image,
    /// Whether the view has drawn it since it decoded (or failed to).
    settled: bool,
}

impl UploadThumbnails {
    /// Keeps a thumbnail for each image in `uploads` and none other, starting the new ones'
    /// decoding. True when what shows changed: a thumbnail came, went, decoded or failed.
    pub fn sync<'a>(&mut self, media: &MediaCache, uploads: impl Iterator<Item = &'a Upload>) -> bool {
        let wanted: Vec<&Upload> = uploads.filter(|upload| is_image(upload)).collect();
        let before = self.shown.len();
        self.shown.retain(|thumbnail| wanted.contains(&&thumbnail.upload));
        let mut changed = self.shown.len() != before;
        for upload in wanted {
            if self.shown.iter().any(|thumbnail| &thumbnail.upload == upload) {
                continue;
            }
            if let Some(image) = media.upload(upload) {
                self.shown.push(Thumbnail { upload: upload.clone(), image, settled: false });
                changed = true;
            }
        }
        for thumbnail in &mut self.shown {
            if !thumbnail.settled && thumbnail.image.is_ready() {
                thumbnail.settled = true;
                changed = true;
            }
        }
        changed
    }

    /// Blocks until every thumbnail has decoded or failed: for headless scenes, which draw once.
    pub fn wait(&self) {
        for thumbnail in &self.shown {
            let _ = thumbnail.image.wait();
        }
    }

    /// The thumbnail of `upload`: decoding (drawn empty until it is) or decoded. `None` for a
    /// file that isn't an image, or one that couldn't be decoded: it shows as a file.
    pub fn get(&self, upload: &Upload) -> Option<&Image> {
        self.shown
            .iter()
            .find(|thumbnail| &thumbnail.upload == upload)
            .map(|thumbnail| &thumbnail.image)
            .filter(|image| image.error().is_none())
    }
}

#[cfg(test)]
mod tests {
    use matrix_core::MediaSize;

    use super::*;

    fn upload(name: &str, bytes: &[u8]) -> Upload {
        Upload::Bytes { name: name.into(), bytes: bytes.into() }
    }

    const PNG: &[u8] = include_bytes!("../../../../packaging/icons/better-matrix-256.png");

    #[test]
    fn a_picture_upload_should_get_a_thumbnail() {
        let (media, mut thumbnails, picture) = (cache(), UploadThumbnails::default(), upload("shot.png", PNG));
        thumbnails.sync(&media, [&picture].into_iter());
        assert!(thumbnails.get(&picture).is_some());
    }

    #[test]
    fn a_file_that_is_not_a_picture_should_get_no_thumbnail() {
        let (media, mut thumbnails, pdf) = (cache(), UploadThumbnails::default(), upload("notes.pdf", b"%PDF"));
        thumbnails.sync(&media, [&pdf].into_iter());
        assert!(thumbnails.get(&pdf).is_none());
    }

    #[test]
    fn a_picture_that_cannot_be_decoded_should_show_as_a_file() {
        let (media, mut thumbnails, broken) = (cache(), UploadThumbnails::default(), upload("shot.png", b"nope"));
        thumbnails.sync(&media, [&broken].into_iter());
        thumbnails.wait();
        assert!(thumbnails.get(&broken).is_none());
    }

    #[test]
    fn a_removed_upload_should_let_its_thumbnail_go() {
        let (media, mut thumbnails, picture) = (cache(), UploadThumbnails::default(), upload("shot.png", PNG));
        thumbnails.sync(&media, [&picture].into_iter());
        assert!(thumbnails.sync(&media, std::iter::empty()), "a change to draw");
        assert!(thumbnails.get(&picture).is_none());
    }

    #[test]
    fn a_thumbnail_decoding_should_ask_for_a_redraw_once() {
        let (media, mut thumbnails, picture) = (cache(), UploadThumbnails::default(), upload("shot.png", PNG));
        thumbnails.sync(&media, [&picture].into_iter());
        thumbnails.wait();
        assert!(thumbnails.sync(&media, [&picture].into_iter()));
        assert!(!thumbnails.sync(&media, [&picture].into_iter()));
    }

    #[test]
    fn headless_uploads_should_show_as_files() {
        let (media, mut thumbnails, picture) =
            (MediaCache::default(), UploadThumbnails::default(), upload("shot.png", PNG));
        thumbnails.sync(&media, [&picture].into_iter());
        assert!(thumbnails.get(&picture).is_none());
    }

    /// A cache that decodes, and an image counted at 2400×2400: two fit the budget, three don't.
    fn cache() -> MediaCache {
        let mut cache = MediaCache::default();
        cache.attach(std::sync::Arc::new(|| {}));
        cache
    }

    fn load(cache: &mut MediaCache, id: &str) {
        cache.loaded(MediaRef::opaque(id), std::sync::Arc::from(&b"not an image"[..]), Some((2400, 2400)));
    }

    #[test]
    fn making_room_should_let_go_of_the_least_recently_used_image() {
        let mut cache = cache();
        load(&mut cache, "a");
        load(&mut cache, "b");
        cache.touch([MediaRef::opaque("a")].iter());
        load(&mut cache, "c");
        let kept: Vec<bool> = ["a", "b", "c"].map(|id| cache.entries.contains_key(&MediaRef::opaque(id))).into();
        assert_eq!(kept, [true, false, true]);
    }

    fn here(cache: &MediaCache, ids: &[&str]) -> Vec<bool> {
        ids.iter().map(|id| cache.entries.contains_key(&MediaRef::opaque(id))).collect()
    }

    #[test]
    fn making_room_should_keep_the_pictures_lists_show() {
        let mut cache = cache();
        cache.keep([MediaRef::opaque("a")].into_iter().collect());
        for id in ["a", "b", "c", "d"] {
            load(&mut cache, id);
        }
        assert_eq!(here(&cache, &["a", "b", "c", "d"]), [true, false, true, true]);
    }

    #[test]
    fn the_pictures_lists_show_should_stay_though_they_alone_outgrow_the_budget() {
        let mut cache = cache();
        cache.keep(["a", "b", "c"].map(MediaRef::opaque).into_iter().collect());
        for id in ["a", "b", "c", "d"] {
            load(&mut cache, id);
        }
        assert_eq!(here(&cache, &["a", "b", "c", "d"]), [true; 4]);
    }

    #[test]
    fn pictures_no_longer_listed_should_count_against_the_budget_again() {
        let mut cache = cache();
        cache.keep(["a", "b", "c"].map(MediaRef::opaque).into_iter().collect());
        for id in ["a", "b", "c"] {
            load(&mut cache, id);
        }
        cache.keep(HashSet::new());
        assert_eq!(here(&cache, &["a", "b", "c"]), [false, true, true]);
    }

    #[test]
    fn a_thumbnail_should_be_counted_at_its_own_size_not_the_original() {
        let thumbnail = MediaSize::Thumbnail { width: 800, height: 600 };
        assert_eq!(fetched_size(thumbnail, Some((4000, 3000))), Some((800, 600)));
    }

    #[test]
    fn a_senders_own_thumbnail_should_be_counted_as_a_thumbnail() {
        let (width, height) = preview_size(MediaSize::Original, Some((4032, 3024))).unwrap();
        let MediaSize::Thumbnail { width: box_w, height: box_h } = MediaSize::PREVIEW else { unreachable!() };
        assert!(width <= box_w && height <= box_h);
    }

    #[test]
    fn a_small_image_should_not_grow_to_its_thumbnail_box() {
        let thumbnail = MediaSize::Thumbnail { width: 800, height: 600 };
        assert_eq!(fetched_size(thumbnail, Some((200, 100))), Some((200, 100)));
    }

    #[test]
    fn a_still_images_mips_should_count_down_to_one_pixel() {
        assert_eq!(decoded_bytes(4, 2), (8 + 2 + 1) * 4);
    }

    #[test]
    fn an_animation_should_count_every_frame() {
        assert_eq!(animation_bytes(10, 10, 3), decoded_bytes(10, 10) * 3);
    }

    #[test]
    fn an_animation_past_runes_limit_should_count_one_frame() {
        assert_eq!(animation_bytes(4000, 4000, 100), decoded_bytes(4000, 4000));
    }

    #[test]
    fn a_thin_animation_within_runes_limit_should_count_every_frame() {
        // Rune's estimate of a 1×8192 frame is 43,690 bytes, so 5,000 of them stay in memory,
        // though their exact chains add up to far more.
        assert_eq!(animation_bytes(1, 8192, 5000), decoded_bytes(1, 8192) * 5000);
    }

    /// A GIF of `frames` 1×1 frames, each behind a graphic control extension.
    fn gif(frames: usize) -> Vec<u8> {
        let mut bytes = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff".to_vec();
        for _ in 0..frames {
            bytes.extend_from_slice(b"\x21\xf9\x04\x00\x0a\x00\x00\x00");
            bytes.extend_from_slice(b"\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00");
        }
        bytes.push(0x3b);
        bytes
    }

    #[test]
    fn a_gifs_frames_should_be_counted_without_decoding_it() {
        assert_eq!(frame_count(&gif(3)), 3);
    }

    #[test]
    fn a_gif_cut_short_should_count_the_frames_it_has() {
        let mut bytes = gif(2);
        bytes.truncate(bytes.len() - 5);
        assert_eq!(frame_count(&bytes), 1);
    }

    #[test]
    fn an_apngs_frames_should_come_from_its_animation_chunk() {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 13]);
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&[0; 17]);
        bytes.extend_from_slice(&[0, 0, 0, 8]);
        bytes.extend_from_slice(b"acTL");
        bytes.extend_from_slice(&[0, 0, 0, 12, 0, 0, 0, 0]);
        assert_eq!(frame_count(&bytes), 12);
    }

    #[test]
    fn an_animated_webps_frames_should_be_its_frame_chunks() {
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        for _ in 0..4 {
            bytes.extend_from_slice(b"ANMF\x01\0\0\0\0\0");
        }
        assert_eq!(frame_count(&bytes), 4);
    }

    #[test]
    fn anything_else_should_be_one_frame() {
        assert_eq!(frame_count(b"\xff\xd8\xff\xe0 a jpeg"), 1);
    }
}
