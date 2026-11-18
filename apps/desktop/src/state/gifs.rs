//! GIFs linked from messages: fetched once per link, decoded and played on a worker that streams
//! frames into a [`Pixels`], so animation redraws only the picture and never rebuilds the view.
//! Bounded: a GIF whose frames would exceed [`MAX_FRAME_BYTES`] plays only its first frame, at
//! most [`MAX_PLAYERS`] play at once, and past [`MAX_TOTAL_BYTES`] of frames across them the GIFs
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
/// GIFs kept playing; the oldest stop beyond this.
const MAX_PLAYERS: usize = 16;
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
    /// Plays `bytes` on a worker, counting its decoded frames in `decoded` as well as its own.
    fn start(bytes: Arc<[u8]>, decoded: &Arc<AtomicUsize>) -> Self {
        let pixels = Pixels::new();
        let (stop, paused) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let held = Arc::new(AtomicUsize::new(0));
        let (out, halt, hold) = (pixels.clone(), Arc::clone(&stop), Arc::clone(&paused));
        let counted = Held { own: Arc::clone(&held), all: Arc::clone(decoded) };
        let worker = std::thread::Builder::new()
            .name("gif".into())
            .spawn(move || play(bytes, &out, &halt, &hold, &counted))
            .ok()
            .map(|handle| handle.thread().clone());
        Self { pixels, held, stop, paused, worker }
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
    let mut frames = Vec::new();
    let mut total = 0usize;
    for frame in decoder.into_frames() {
        let Ok(frame) = frame else { break };
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = Duration::from_millis(u64::from(numerator / denominator.max(1)).max(20));
        let image = frame.into_buffer();
        total += image.as_raw().len();
        held.add(image.as_raw().len());
        let first = frames.is_empty();
        frames.push((image.width(), image.height(), image.into_raw(), delay));
        if first {
            let (w, h, rgba, _) = &frames[0];
            out.set(*w, *h, rgba.clone());
        }
        if total > MAX_FRAME_BYTES || stop.load(Ordering::Relaxed) {
            frames.truncate(1);
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

#[derive(Debug, Default)]
pub struct GifCache {
    entries: HashMap<String, Entry>,
    /// The GIFs playing, those shown longest ago first.
    playing: VecDeque<String>,
    /// Bytes of decoded frames all players hold.
    decoded: Arc<AtomicUsize>,
    retries: Retries<String>,
    /// The GIFs on screen, as last shown.
    on_screen: std::collections::HashSet<String>,
}

impl GifCache {
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
        self.entries.insert(url.clone(), Entry::Playing(Player::start(bytes, &self.decoded)));
        self.playing.push_back(url);
        // Making room: the oldest off screen is forgotten, and fetched again if it comes back;
        // only when every one shows does the oldest stop, holding its place.
        while self.playing.len() > MAX_PLAYERS {
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

    /// Plays the GIFs in `shown` while the window is focused, and holds every other. Past
    /// [`MAX_TOTAL_BYTES`] of frames, those off screen are forgotten, the one shown longest ago
    /// first, and fetched again if they come back.
    pub fn show(&mut self, shown: std::collections::HashSet<String>, focused: bool) {
        for (url, entry) in &self.entries {
            if let Entry::Playing(player) = entry {
                player.set_paused(!focused || !shown.contains(url));
            }
        }
        // Those on screen move behind the rest, keeping their order.
        let (on_screen, off_screen): (VecDeque<String>, VecDeque<String>) =
            self.playing.drain(..).partition(|url| shown.contains(url));
        self.playing = off_screen;
        self.playing.extend(on_screen);
        self.on_screen = shown;
        self.trim(MAX_TOTAL_BYTES);
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
            held: Arc::new(AtomicUsize::new(bytes)),
            stop: Arc::default(),
            paused: Arc::default(),
            worker: None,
        };
        gifs.decoded.fetch_add(bytes, Ordering::Relaxed);
        gifs.entries.insert(url.to_owned(), Entry::Playing(player));
        gifs.playing.push_back(url.to_owned());
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
        let player = Player::start(Arc::from(bytes), &decoded);
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
