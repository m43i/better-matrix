//! GIFs linked from messages: fetched once per link, decoded and played on a worker that streams
//! frames into a [`Pixels`], so animation redraws only the picture and never rebuilds the view.
//! The GIF picker's previews have a cache of their own ([`PICKER_PLAYERS`]), apart from the
//! timelines', and are still: each holds its first frame and plays only while the pointer is
//! over it.
//! Bounded: a GIF whose frames would exceed [`MAX_FRAME_BYTES`] keeps every other frame (each
//! shown twice as long) until they fit, at most [`MAX_PLAYERS`] play at once, and past [`MAX_TOTAL_BYTES`] of frames across them the GIFs
//! shown longest ago that are off screen are let go.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use image::AnimationDecoder;
use rune::Pixels;

use super::retry::Retries;

/// Decoded frames one GIF may hold.
const MAX_FRAME_BYTES: usize = 48 << 20;
/// GIFs a timeline's players keep; the oldest stop beyond this.
const MAX_PLAYERS: usize = 16;
/// GIFs the GIF picker's players keep: two pages of previews, which hold still.
pub const PICKER_PLAYERS: usize = 48;
/// Decoded frames all GIFs may hold together before those off screen are let go; those on
/// screen keep theirs.
const MAX_TOTAL_BYTES: usize = 128 << 20;

#[derive(Debug)]
enum Entry {
    Loading,
    Playing(Player),
    /// Stopped to make room for newer ones while its timeline is still shown: not fetched
    /// again until the timeline is opened anew.
    Stopped,
    /// Downloaded, but not a GIF: the link shows as text, and it isn't fetched again.
    Broken,
}

/// One playing GIF. Dropping it stops its worker; pausing holds its frame, the worker parked
/// until resumed.
#[derive(Debug)]
pub struct Player {
    pixels: Pixels,
    /// Its width and height in pixels, as its header gives them: known from the start, so the
    /// view sizes it before its first frame is decoded.
    size: Option<(u32, u32)>,
    /// Bytes of decoded frames its worker holds.
    held: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    worker: Option<std::thread::Thread>,
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wake();
    }
}

impl Player {
    /// Plays `bytes` on a worker (held on its first frame when `paused`), counting its decoded
    /// frames in `decoded` as well as its own.
    fn start(bytes: Arc<[u8]>, decoded: &Arc<AtomicUsize>, paused: bool) -> Self {
        let size = header_size(&bytes);
        let pixels = Pixels::new();
        let (stop, paused) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(paused)));
        let held = Arc::new(AtomicUsize::new(0));
        let (out, halt, hold) = (pixels.clone(), Arc::clone(&stop), Arc::clone(&paused));
        let counted = Held { own: Arc::clone(&held), all: Arc::clone(decoded) };
        let worker = std::thread::Builder::new()
            .name("gif".into())
            .spawn(move || play(bytes, &out, &halt, &hold, &counted))
            .ok()
            .map(|handle| handle.thread().clone());
        Self { pixels, size, held, stop, paused, worker }
    }

    /// Holds or resumes the animation.
    fn set_paused(&self, paused: bool) {
        if self.paused.swap(paused, Ordering::Relaxed) && !paused {
            self.wake();
        }
    }

    fn wake(&self) {
        if let Some(worker) = &self.worker {
            worker.unpark();
        }
    }

    pub fn pixels(&self) -> &Pixels {
        &self.pixels
    }

    /// Its width and height in pixels, when its header gives both.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.size
    }
}

/// A GIF's width and height from its header (the logical screen's, little-endian after the
/// six-byte signature), when both are there and not zero.
fn header_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let side = |at: usize| Some(u32::from(u16::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?])));
    Some((side(6)?, side(8)?)).filter(|(w, h)| *w > 0 && *h > 0)
}

/// The decoded bytes a worker holds, counted for its player and for all of them; given back
/// when the worker ends and its frames go.
struct Held {
    own: Arc<AtomicUsize>,
    all: Arc<AtomicUsize>,
}

impl Held {
    fn add(&self, bytes: usize) {
        self.own.fetch_add(bytes, Ordering::Relaxed);
        self.all.fetch_add(bytes, Ordering::Relaxed);
    }

    fn give_back(&self, bytes: usize) {
        self.own.fetch_sub(bytes, Ordering::Relaxed);
        self.all.fetch_sub(bytes, Ordering::Relaxed);
    }
}

/// A decoded frame: width, height, RGBA pixels, and how long it shows.
type Frame = (u32, u32, Vec<u8>, Duration);

/// Keeps every other frame, each shown as long as the pair it stands for; the bytes let go.
fn thin(frames: &mut Vec<Frame>) -> usize {
    let mut freed = 0;
    let mut kept: Vec<Frame> = Vec::with_capacity(frames.len().div_ceil(2));
    for (index, frame) in frames.drain(..).enumerate() {
        match kept.last_mut() {
            Some(last) if index % 2 == 1 => {
                last.3 += frame.3;
                freed += frame.2.len();
            }
            _ => kept.push(frame),
        }
    }
    *frames = kept;
    freed
}

impl Drop for Held {
    fn drop(&mut self) {
        self.all.fetch_sub(self.own.swap(0, Ordering::Relaxed), Ordering::Relaxed);
    }
}

/// Decodes the frames, lets go of the file, then loops them at their own delays until stopped,
/// holding the frame shown while paused.
fn play(bytes: Arc<[u8]>, out: &Pixels, stop: &AtomicBool, paused: &AtomicBool, held: &Held) {
    let Ok(decoder) = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes[..])) else { return };
    let mut frames: Vec<Frame> = Vec::new();
    let mut total = 0usize;
    // One frame in `stride` is kept: more once the frames outgrow the budget.
    let mut stride = 1usize;
    for (index, frame) in decoder.into_frames().enumerate() {
        let Ok(frame) = frame else { break };
        // Stopped: let go now, rather than decode frames that are only left out.
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = Duration::from_millis(u64::from(numerator / denominator.max(1)).max(20));
        if index % stride != 0 {
            // Left out to fit: the frame kept before it shows for its time too.
            if let Some(last) = frames.last_mut() {
                last.3 += delay;
            }
            continue;
        }
        let image = frame.into_buffer();
        total += image.as_raw().len();
        held.add(image.as_raw().len());
        let first = frames.is_empty();
        frames.push((image.width(), image.height(), image.into_raw(), delay));
        if first {
            let (w, h, rgba, _) = &frames[0];
            out.set(*w, *h, rgba.clone());
            // Held from the start (a still picker preview, a GIF off screen): only its first
            // frame is decoded until it plays, so a grid of still GIFs holds one frame each.
            while paused.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_secs(5));
            }
        }
        if stop.load(Ordering::Relaxed) {
            frames.truncate(1);
            break;
        }
        while total > MAX_FRAME_BYTES && frames.len() > 1 {
            let freed = thin(&mut frames);
            total -= freed;
            held.give_back(freed);
            stride *= 2;
        }
        // One frame alone is too big to animate: it shows still.
        if total > MAX_FRAME_BYTES {
            break;
        }
    }
    drop(bytes);
    if frames.len() < 2 {
        return;
    }
    for (w, h, rgba, delay) in frames.iter().cycle() {
        // Off screen or in a window in the background: hold until woken. The timeout only
        // guards against a missed wake.
        while paused.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
            std::thread::park_timeout(Duration::from_secs(5));
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }
        out.set(*w, *h, rgba.clone());
        wait_frame(*delay, stop, paused);
    }
}

/// Waits out a frame's delay (a GIF may ask for minutes), returning early once stopped or
/// paused: `Player` unparks the worker for both, so a dropped player lets go of its frames at
/// once rather than when the delay ends.
fn wait_frame(delay: Duration, stop: &AtomicBool, paused: &AtomicBool) {
    let deadline = Instant::now() + delay;
    while !stop.load(Ordering::Relaxed) && !paused.load(Ordering::Relaxed) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::park_timeout(left);
    }
}

#[derive(Debug)]
pub struct GifCache {
    /// GIFs kept playing; the oldest stop beyond this.
    max_players: usize,
    entries: HashMap<String, Entry>,
    /// The GIFs playing, those shown longest ago first.
    playing: VecDeque<String>,
    /// Bytes of decoded frames all players hold.
    decoded: Arc<AtomicUsize>,
    retries: Retries<String>,
    /// The GIFs on screen, as last shown.
    on_screen: std::collections::HashSet<String>,
    /// The GIFs that play only under the pointer (the GIF picker's previews).
    still: std::collections::HashSet<String>,
    /// The GIF under the pointer.
    hovered: Option<String>,
    /// Whether the window is focused, as last shown.
    focused: bool,
}

impl Default for GifCache {
    fn default() -> Self {
        Self::with_players(MAX_PLAYERS)
    }
}

impl GifCache {
    /// A cache keeping at most `max_players` GIFs (the GIF picker's has its own).
    pub fn with_players(max_players: usize) -> Self {
        Self {
            max_players,
            entries: HashMap::new(),
            playing: VecDeque::new(),
            decoded: Arc::default(),
            retries: Retries::default(),
            on_screen: std::collections::HashSet::new(),
            still: std::collections::HashSet::new(),
            hovered: None,
            focused: false,
        }
    }

    /// Whether `url` has to be fetched at `now`: it isn't here, and if it failed, its wait is
    /// over.
    pub fn wants(&self, url: &str, now: Instant) -> bool {
        !self.entries.contains_key(url) && self.retries.may_try(&url.to_owned(), now)
    }

    pub fn loading(&mut self, url: &str) {
        self.entries.insert(url.to_owned(), Entry::Loading);
    }

    /// A download failed: it is tried again after a wait, while its timeline is shown.
    pub fn failed(&mut self, url: &str, now: Instant) {
        self.entries.remove(url);
        self.retries.failed(url.to_owned(), now);
    }

    /// Lets a failed download be tried again at once, its wait forgotten: a GIF the user just
    /// chose to show.
    pub fn try_again(&mut self, url: &str) {
        self.retries.succeeded(&url.to_owned());
    }

    /// When the next failed download may be tried again, after `now`.
    pub fn next_retry(&self, now: Instant) -> Option<Instant> {
        self.retries.next_after(now)
    }

    /// Starts playing a downloaded GIF, if it is still wanted: one whose timeline closed while
    /// it downloaded is dropped.
    pub fn loaded(&mut self, url: String, bytes: Arc<[u8]>) {
        if !matches!(self.entries.get(&url), Some(Entry::Loading)) {
            return;
        }
        self.retries.succeeded(&url);
        if !bytes.starts_with(b"GIF8") {
            self.entries.insert(url, Entry::Broken);
            return;
        }
        let paused = !self.plays(&url);
        self.entries.insert(url.clone(), Entry::Playing(Player::start(bytes, &self.decoded, paused)));
        self.playing.push_back(url);
        // Making room: the oldest off screen is forgotten, and fetched again if it comes back;
        // only when every one shows does the oldest stop, holding its place.
        while self.playing.len() > self.max_players {
            match self.playing.iter().position(|url| !self.on_screen.contains(url)) {
                Some(index) => {
                    if let Some(gone) = self.playing.remove(index) {
                        self.entries.remove(&gone);
                    }
                }
                None => {
                    if let Some(oldest) = self.playing.pop_front() {
                        self.entries.insert(oldest, Entry::Stopped);
                    }
                }
            }
        }
    }

    /// Forgets every GIF not in `shown`, stopping it (or dropping it once downloaded): one
    /// shown again is fetched again.
    pub fn keep_only(&mut self, shown: &std::collections::HashSet<String>) {
        self.entries.retain(|url, _| shown.contains(url));
        self.playing.retain(|url| shown.contains(url));
        self.retries.retain(|url| shown.contains(url));
    }

    /// Takes the GIFs on screen (`shown`): they play while the window is focused (a still one
    /// only under the pointer), and every other holds its frame. Past [`MAX_TOTAL_BYTES`] of frames, those off screen
    /// are forgotten, the one shown longest ago first, and fetched again if they come back.
    pub fn show(&mut self, shown: std::collections::HashSet<String>, focused: bool) {
        // Those on screen move behind the rest, keeping their order.
        let (on_screen, off_screen): (VecDeque<String>, VecDeque<String>) =
            self.playing.drain(..).partition(|url| shown.contains(url));
        self.playing = off_screen;
        self.playing.extend(on_screen);
        self.focused = focused;
        self.on_screen = shown;
        self.hold_all_but_hovered();
        self.trim(MAX_TOTAL_BYTES);
    }

    /// Takes the GIFs that play only under the pointer.
    pub fn keep_still(&mut self, still: std::collections::HashSet<String>) {
        self.still = still;
        self.hold_all_but_hovered();
    }

    /// Takes the GIF under the pointer (`None`: none): a still one plays while it is there.
    pub fn hover(&mut self, url: Option<String>) {
        self.hovered = url;
        self.hold_all_but_hovered();
    }

    /// The GIF under the pointer.
    pub fn hovered(&self) -> Option<&str> {
        self.hovered.as_deref()
    }

    /// Whether `url` plays now: on screen in a focused window, and if still, under the pointer.
    fn plays(&self, url: &str) -> bool {
        self.focused
            && self.on_screen.contains(url)
            && (!self.still.contains(url) || self.hovered.as_deref() == Some(url))
    }

    fn hold_all_but_hovered(&self) {
        for (url, entry) in &self.entries {
            if let Entry::Playing(player) = entry {
                player.set_paused(!self.plays(url));
            }
        }
    }

    /// Forgets GIFs off screen, the one shown longest ago first, until their frames come to
    /// `budget` or only those on screen are left.
    fn trim(&mut self, budget: usize) {
        let mut total = self.decoded.load(Ordering::Relaxed);
        while total > budget {
            let Some(index) = self.playing.iter().position(|url| !self.on_screen.contains(url)) else { return };
            let Some(gone) = self.playing.remove(index) else { return };
            if let Some(Entry::Playing(player)) = self.entries.remove(&gone) {
                // Its worker gives the bytes back as it ends.
                total = total.saturating_sub(player.held.load(Ordering::Relaxed));
            }
        }
    }

    pub fn player(&self, url: &str) -> Option<&Player> {
        match self.entries.get(url)? {
            Entry::Playing(player) => Some(player),
            _ => None,
        }
    }

    /// Whether the last download of `url` failed and isn't being tried again yet.
    pub fn has_failed(&self, url: &str) -> bool {
        // A retry still loading shows as failed, so the row keeps its height.
        match self.entries.get(url) {
            Some(Entry::Broken) => true,
            None | Some(Entry::Loading) => self.retries.has_failed(&url.to_owned()),
            Some(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn shown(urls: &[String]) -> HashSet<String> {
        urls.iter().cloned().collect()
    }

    #[test]
    fn thinning_frames_should_keep_every_other_one_for_the_time_of_both() {
        let frame = |ms| (1, 1, vec![0; 4], Duration::from_millis(ms));
        let mut frames = vec![frame(10), frame(20), frame(30), frame(40), frame(50)];
        assert_eq!(thin(&mut frames), 8);
        let delays: Vec<u128> = frames.iter().map(|frame| frame.3.as_millis()).collect();
        assert_eq!(delays, [30, 70, 50]);
    }

    #[test]
    fn a_gifs_size_should_be_read_from_its_header() {
        assert_eq!(header_size(b"GIF89a\x2c\x01\xc8\x00"), Some((300, 200)));
    }

    #[test]
    fn a_gif_header_cut_short_should_give_no_size() {
        assert_eq!(header_size(b"GIF89a\x2c\x01"), None);
    }

    #[test]
    fn a_gif_off_screen_should_be_the_one_to_make_room() {
        let mut gifs = GifCache::default();
        let urls: Vec<String> = (0..=MAX_PLAYERS).map(|i| format!("https://x/{i}.gif")).collect();
        gifs.show(urls[1..].iter().cloned().collect(), true);
        for url in &urls {
            gifs.loading(url);
            gifs.loaded(url.clone(), Arc::from(&b"GIF89a"[..]));
        }
        assert!(gifs.wants(&urls[0], Instant::now()) && gifs.player(&urls[1]).is_some());
    }

    #[test]
    fn a_gif_stopped_to_make_room_should_not_be_fetched_again_while_shown() {
        let mut gifs = GifCache::default();
        let urls: Vec<String> = (0..=MAX_PLAYERS).map(|i| format!("https://x/{i}.gif")).collect();
        gifs.show(urls.iter().cloned().collect(), true);
        for url in &urls {
            gifs.loading(url);
            gifs.loaded(url.clone(), Arc::from(&b"GIF89a"[..]));
        }
        gifs.keep_only(&shown(&urls));
        assert!(!gifs.wants(&urls[0], Instant::now()));
    }

    #[test]
    fn a_gif_no_longer_shown_should_be_let_go() {
        let mut gifs = GifCache::default();
        gifs.loading("https://x/a.gif");
        gifs.loaded("https://x/a.gif".into(), Arc::from(&b"GIF89a"[..]));
        gifs.keep_only(&HashSet::new());
        assert!(gifs.player("https://x/a.gif").is_none());
    }

    #[test]
    fn a_gif_downloaded_after_its_timeline_closed_should_not_play() {
        let mut gifs = GifCache::default();
        gifs.loading("https://x/a.gif");
        gifs.keep_only(&HashSet::new());
        gifs.loaded("https://x/a.gif".into(), Arc::from(&b"GIF89a"[..]));
        assert!(gifs.player("https://x/a.gif").is_none());
    }

    #[test]
    fn a_failed_gif_should_be_fetched_again_after_a_wait() {
        let (mut gifs, now) = (GifCache::default(), Instant::now());
        gifs.loading("https://x/a.gif");
        gifs.failed("https://x/a.gif", now);
        assert!(!gifs.wants("https://x/a.gif", now));
        assert!(gifs.wants("https://x/a.gif", now + Duration::from_secs(5)));
    }

    #[test]
    fn a_download_that_is_not_a_gif_should_show_as_failed() {
        let mut gifs = GifCache::default();
        gifs.loading("https://x/a.gif");
        gifs.loaded("https://x/a.gif".into(), Arc::from(&b"<html>"[..]));
        assert!(gifs.has_failed("https://x/a.gif") && !gifs.wants("https://x/a.gif", Instant::now()));
    }

    /// A player holding `bytes` of frames, with no worker.
    fn holding(gifs: &mut GifCache, url: &str, bytes: usize) {
        let player = Player {
            pixels: Pixels::new(),
            size: None,
            held: Arc::new(AtomicUsize::new(bytes)),
            stop: Arc::default(),
            paused: Arc::default(),
            worker: None,
        };
        gifs.decoded.fetch_add(bytes, Ordering::Relaxed);
        gifs.entries.insert(url.to_owned(), Entry::Playing(player));
        gifs.playing.push_back(url.to_owned());
    }

    fn paused(gifs: &GifCache, url: &str) -> bool {
        gifs.player(url).is_some_and(|player| player.paused.load(Ordering::Relaxed))
    }

    #[test]
    fn a_gif_in_a_message_should_play_while_on_screen() {
        let mut gifs = GifCache::default();
        gifs.show(shown(&["a".into()]), true);
        gifs.loading("a");
        gifs.loaded("a".into(), Arc::from(&b"GIF89a"[..]));
        assert!(!paused(&gifs, "a"));
    }

    #[test]
    fn a_still_gif_should_hold_its_frame_until_the_pointer_is_over_it() {
        let mut gifs = GifCache::default();
        gifs.keep_still(shown(&["a".into()]));
        gifs.show(shown(&["a".into()]), true);
        gifs.loading("a");
        gifs.loaded("a".into(), Arc::from(&b"GIF89a"[..]));
        assert!(paused(&gifs, "a"));
        gifs.hover(Some("a".into()));
        assert!(!paused(&gifs, "a"));
        gifs.hover(None);
        assert!(paused(&gifs, "a"));
    }

    #[test]
    fn a_hovered_gif_should_hold_while_the_window_is_not_focused() {
        let mut gifs = GifCache::default();
        gifs.keep_still(shown(&["a".into()]));
        gifs.loading("a");
        gifs.loaded("a".into(), Arc::from(&b"GIF89a"[..]));
        gifs.hover(Some("a".into()));
        gifs.show(shown(&["a".into()]), false);
        assert!(paused(&gifs, "a"));
    }

    #[test]
    fn past_the_budget_the_gif_shown_longest_ago_off_screen_should_go() {
        let mut gifs = GifCache::default();
        for url in ["a", "b", "c"] {
            holding(&mut gifs, url, 10);
        }
        gifs.show(shown(&["a".into()]), true);
        gifs.show(shown(&["c".into()]), true);
        gifs.trim(25);
        let kept = ["a", "b", "c"].map(|url| gifs.player(url).is_some());
        assert_eq!((kept, gifs.wants("b", Instant::now())), ([true, false, true], true));
    }

    #[test]
    fn gifs_on_screen_should_keep_their_frames_past_the_budget() {
        let mut gifs = GifCache::default();
        holding(&mut gifs, "a", 10);
        holding(&mut gifs, "b", 10);
        gifs.show(shown(&["a".into(), "b".into()]), false);
        gifs.trim(0);
        assert!(gifs.player("a").is_some() && gifs.player("b").is_some());
    }

    #[test]
    fn a_stopped_gif_should_give_back_its_decoded_frames() {
        let frame = || image::Frame::new(image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255])));
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder.encode_frames([frame(), frame()]).unwrap();
        }
        let decoded = Arc::new(AtomicUsize::new(0));
        let player = Player::start(Arc::from(bytes), &decoded, false);
        let deadline = Instant::now() + Duration::from_secs(2);
        while decoded.load(Ordering::Relaxed) < 2 * 8 * 8 * 4 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let playing = decoded.load(Ordering::Relaxed);
        drop(player);
        while decoded.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!((playing, decoded.load(Ordering::Relaxed)), (2 * 8 * 8 * 4, 0));
    }

    #[test]
    fn a_held_player_should_decode_only_its_first_frame_until_it_plays() {
        let frame = || image::Frame::new(image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255])));
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder.encode_frames([frame(), frame(), frame()]).unwrap();
        }
        let decoded = Arc::new(AtomicUsize::new(0));
        let player = Player::start(Arc::from(bytes), &decoded, true);
        let one = 8 * 8 * 4;
        let deadline = Instant::now() + Duration::from_secs(2);
        while decoded.load(Ordering::Relaxed) < one && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(50));
        let held = decoded.load(Ordering::Relaxed);
        player.set_paused(false);
        while decoded.load(Ordering::Relaxed) < 3 * one && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!((held, decoded.load(Ordering::Relaxed)), (one, 3 * one));
    }

    #[test]
    fn a_gif_over_its_budget_should_keep_animating_with_fewer_frames() {
        // 17 frames of 4 MiB: 68 MiB decoded, over the 48 MiB a GIF may hold.
        let side = 1024;
        let frame =
            |shade| image::Frame::new(image::RgbaImage::from_pixel(side, side, image::Rgba([shade, 0, 0, 255])));
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut bytes, 30);
            encoder.encode_frames((0..17u8).map(|index| frame(index * 10))).unwrap();
        }
        let decoded = Arc::new(AtomicUsize::new(0));
        let _player = Player::start(Arc::from(bytes), &decoded, false);
        // Decoding is done once the count holds still.
        let deadline = Instant::now() + Duration::from_secs(60);
        let (mut last, mut since) = (usize::MAX, Instant::now());
        while Instant::now() < deadline {
            let now = decoded.load(Ordering::Relaxed);
            if now != last {
                (last, since) = (now, Instant::now());
            } else if since.elapsed() > Duration::from_secs(1) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Thinned to every other frame once the thirteenth crossed the budget: 0, 2, …, 16.
        let one = (side * side * 4) as usize;
        assert_eq!(last, 9 * one);
    }

    #[test]
    fn stopping_a_player_should_end_a_long_frame_delay_at_once() {
        let (stop, paused) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (halt, hold) = (Arc::clone(&stop), Arc::clone(&paused));
        let worker = std::thread::spawn(move || wait_frame(Duration::from_secs(600), &halt, &hold));
        std::thread::sleep(Duration::from_millis(20));
        stop.store(true, Ordering::Relaxed);
        worker.thread().unpark();
        let started = Instant::now();
        worker.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
