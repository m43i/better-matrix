//! Video for the attachment preview: a saved file played with its sound, drawn into an element.
//!
//! On macOS and Windows, Rune's own player (AVFoundation, Media Foundation) decodes and draws.
//! Rune has none for Linux, so there GStreamer decodes (`playbin`, with whatever codecs the
//! system has), into RGBA frames handed to a [`rune::Pixels`] from its streaming thread, and
//! plays the sound on the picked speaker. Headless, a [`Player::Detached`] stand-in keeps what
//! was asked of it and plays nothing, so tests and renders never touch a decoder.

use std::path::PathBuf;

use rune::{El, SurfaceFit};

/// Where a video is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Playback {
    /// Opening the file and reading its first frame.
    Loading,
    Paused,
    Playing,
    /// Played to its end: play starts it over.
    Ended,
    Failed(String),
}

/// The platform's own player: GStreamer where Rune has none.
#[cfg(all(unix, not(target_os = "macos")))]
type Native = gst_player::GstPlayer;
#[cfg(not(all(unix, not(target_os = "macos"))))]
type Native = rune_player::RunePlayer;

/// A video to play, or a stand-in for one when headless.
#[derive(Debug)]
pub enum Player {
    Native(Native),
    Detached(Detached),
}

impl Player {
    /// A player for `path` on the picked `speaker`, starting at once; `changed` runs (from any
    /// thread) when it loads, ends or fails.
    pub fn open(path: PathBuf, speaker: Option<&str>, changed: rune::Waker) -> Self {
        Self::Native(Native::open(path, speaker, changed))
    }

    pub fn play(&self) {
        match self {
            Self::Native(player) => player.play(),
            Self::Detached(player) => player.set(|p| p.playing = true),
        }
    }

    pub fn pause(&self) {
        match self {
            Self::Native(player) => player.pause(),
            Self::Detached(player) => player.set(|p| p.playing = false),
        }
    }

    /// Moves to `share` of the way through (0 the start, 1 the end).
    pub fn seek(&self, share: f64) {
        let share = share.clamp(0.0, 1.0);
        match self {
            Self::Native(player) => player.seek(share),
            Self::Detached(player) => player.set(|p| p.at = share),
        }
    }

    pub fn set_muted(&self, muted: bool) {
        match self {
            Self::Native(player) => player.set_muted(muted),
            Self::Detached(player) => player.set(|p| p.muted = muted),
        }
    }

    pub fn muted(&self) -> bool {
        match self {
            Self::Native(player) => player.muted(),
            Self::Detached(player) => player.get().muted,
        }
    }

    pub fn playback(&self) -> Playback {
        match self {
            Self::Native(player) => player.playback(),
            Self::Detached(player) if player.get().playing => Playback::Playing,
            Self::Detached(_) => Playback::Paused,
        }
    }

    /// Whether play is in effect: playing, or loading to play.
    pub fn is_playing(&self) -> bool {
        match self {
            Self::Native(player) => player.is_playing(),
            Self::Detached(player) => player.get().playing,
        }
    }

    /// Where it is, in seconds.
    pub fn time(&self) -> f64 {
        match self {
            Self::Native(player) => player.time(),
            Self::Detached(player) => {
                let state = player.get();
                state.at * state.duration.unwrap_or(0.0)
            }
        }
    }

    /// Its length in seconds, once known.
    pub fn duration(&self) -> Option<f64> {
        match self {
            Self::Native(player) => player.duration(),
            Self::Detached(player) => player.get().duration,
        }
    }

    /// How far it has played, from 0 to 1.
    pub fn progress(&self) -> f64 {
        match self.duration().filter(|duration| *duration > 0.0) {
            Some(duration) => (self.time() / duration).clamp(0.0, 1.0),
            None => 0.0,
        }
    }

    /// Its picture's size in pixels, once known.
    pub fn natural_size(&self) -> Option<(u32, u32)> {
        match self {
            Self::Native(player) => player.natural_size(),
            Self::Detached(player) => player.get().size,
        }
    }

    /// Draws the video into `el`, fitted.
    pub fn show(&self, el: El, fit: SurfaceFit) -> El {
        match self {
            Self::Native(player) => player.show(el, fit),
            Self::Detached(_) => el,
        }
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
mod rune_player {
    //! Rune's player, unmuted (Rune starts videos muted), playing as soon as it opens.

    use std::path::PathBuf;

    use rune::{El, SurfaceFit};

    use super::Playback;

    #[derive(Debug)]
    pub struct RunePlayer {
        player: rune::Player,
    }

    impl RunePlayer {
        /// Rune plays on the system's default speaker.
        pub fn open(path: PathBuf, _speaker: Option<&str>, changed: rune::Waker) -> Self {
            let player = rune::Player::file(path);
            player.set_muted(false);
            player.set_waker(changed);
            player.play();
            Self { player }
        }

        pub fn play(&self) {
            self.player.play();
        }

        pub fn pause(&self) {
            self.player.pause();
        }

        pub fn seek(&self, share: f64) {
            if let Some(duration) = self.player.duration() {
                self.player.seek(share * duration);
            }
        }

        pub fn set_muted(&self, muted: bool) {
            self.player.set_muted(muted);
        }

        pub fn muted(&self) -> bool {
            self.player.muted()
        }

        pub fn playback(&self) -> Playback {
            match self.player.state() {
                rune::PlayerState::Loading => Playback::Loading,
                rune::PlayerState::Playing => Playback::Playing,
                rune::PlayerState::Ended => Playback::Ended,
                rune::PlayerState::Failed(error) => Playback::Failed(error),
                _ => Playback::Paused,
            }
        }

        pub fn is_playing(&self) -> bool {
            self.player.is_playing()
        }

        pub fn time(&self) -> f64 {
            self.player.time()
        }

        pub fn duration(&self) -> Option<f64> {
            self.player.duration()
        }

        pub fn natural_size(&self) -> Option<(u32, u32)> {
            self.player.natural_size()
        }

        pub fn show(&self, el: El, fit: SurfaceFit) -> El {
            el.video(&self.player, fit)
        }
    }
}

/// What a headless stand-in was asked: nothing plays, nothing draws.
#[derive(Clone, Debug, Default)]
pub struct DetachedState {
    pub playing: bool,
    pub muted: bool,
    /// Where it is, as a share of its length.
    pub at: f64,
    pub duration: Option<f64>,
    pub size: Option<(u32, u32)>,
}

#[derive(Debug, Default)]
pub struct Detached(std::cell::RefCell<DetachedState>);

impl Detached {
    /// A stand-in that reports `duration` and `size`, as a loaded video would; it starts playing,
    /// as an opened video does.
    pub fn new(duration: Option<f64>, size: Option<(u32, u32)>) -> Self {
        Self(std::cell::RefCell::new(DetachedState { playing: true, duration, size, ..DetachedState::default() }))
    }

    fn get(&self) -> DetachedState {
        self.0.borrow().clone()
    }

    fn set(&self, change: impl FnOnce(&mut DetachedState)) {
        change(&mut self.0.borrow_mut());
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod gst_player {
    //! GStreamer's `playbin`: the file in, RGBA frames out to an app sink, sound to the picked
    //! speaker (found by the name the settings list, else the system's default).

    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use gstreamer as gst;
    use gstreamer::prelude::*;
    use gstreamer_app as gst_app;
    use gstreamer_video as gst_video;
    use gstreamer_video::prelude::*;

    use super::Playback;

    /// What the streaming threads report, for the UI thread to read.
    #[derive(Debug, Default)]
    struct Facts {
        /// Play is in effect (also while loading to play).
        playing: bool,
        /// The first frame is here (or the file is known to have none to show).
        loaded: bool,
        ended: bool,
        /// Kept here: reading `playbin`'s own answers the sound server's, which comes later.
        muted: bool,
        failed: Option<String>,
        size: Option<(u32, u32)>,
        duration: Option<f64>,
    }

    pub struct GstPlayer {
        /// None when GStreamer or its player couldn't start: the failure is in `facts`.
        pipeline: Option<gst::Element>,
        pixels: rune::Pixels,
        facts: Arc<Mutex<Facts>>,
    }

    impl std::fmt::Debug for GstPlayer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("GstPlayer").field("facts", &self.facts).finish_non_exhaustive()
        }
    }

    fn lock(facts: &Mutex<Facts>) -> std::sync::MutexGuard<'_, Facts> {
        facts.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    impl GstPlayer {
        pub fn open(path: PathBuf, speaker: Option<&str>, changed: rune::Waker) -> Self {
            let pixels = rune::Pixels::new();
            let facts = Arc::new(Mutex::new(Facts { playing: true, ..Facts::default() }));
            let pipeline = match build(&path, speaker, &pixels, &facts, &changed) {
                Ok(pipeline) => Some(pipeline),
                Err(error) => {
                    lock(&facts).failed = Some(error);
                    changed();
                    None
                }
            };
            Self { pipeline, pixels, facts }
        }

        pub fn show(&self, el: rune::El, fit: rune::SurfaceFit) -> rune::El {
            el.pixels_fit(&self.pixels, fit)
        }

        pub fn play(&self) {
            let Some(pipeline) = &self.pipeline else { return };
            let ended = {
                let mut facts = lock(&self.facts);
                facts.playing = true;
                std::mem::take(&mut facts.ended)
            };
            if ended {
                seek_to(pipeline, 0.0);
            }
            let _ = pipeline.set_state(gst::State::Playing);
        }

        pub fn pause(&self) {
            let Some(pipeline) = &self.pipeline else { return };
            lock(&self.facts).playing = false;
            let _ = pipeline.set_state(gst::State::Paused);
        }

        /// After the end, the pipeline is still playing: a seek there would play on unasked, so
        /// it pauses first and waits for play.
        pub fn seek(&self, share: f64) {
            let Some(pipeline) = &self.pipeline else { return };
            let Some(duration) = self.duration() else { return };
            if std::mem::take(&mut lock(&self.facts).ended) {
                let _ = pipeline.set_state(gst::State::Paused);
            }
            seek_to(pipeline, share * duration);
        }

        pub fn set_muted(&self, muted: bool) {
            if let Some(pipeline) = &self.pipeline {
                lock(&self.facts).muted = muted;
                pipeline.set_property("mute", muted);
            }
        }

        pub fn muted(&self) -> bool {
            lock(&self.facts).muted
        }

        pub fn playback(&self) -> Playback {
            let facts = lock(&self.facts);
            if let Some(error) = &facts.failed {
                Playback::Failed(error.clone())
            } else if facts.ended {
                Playback::Ended
            } else if !facts.loaded {
                Playback::Loading
            } else if facts.playing {
                Playback::Playing
            } else {
                Playback::Paused
            }
        }

        pub fn is_playing(&self) -> bool {
            let facts = lock(&self.facts);
            facts.playing && !facts.ended && facts.failed.is_none()
        }

        pub fn time(&self) -> f64 {
            if lock(&self.facts).ended {
                return self.duration().unwrap_or(0.0);
            }
            self.pipeline
                .as_ref()
                .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
                .map_or(0.0, |at| at.nseconds() as f64 / 1e9)
        }

        pub fn duration(&self) -> Option<f64> {
            if let Some(duration) = lock(&self.facts).duration {
                return Some(duration);
            }
            let duration = self.pipeline.as_ref()?.query_duration::<gst::ClockTime>()?.nseconds() as f64 / 1e9;
            lock(&self.facts).duration = Some(duration);
            Some(duration)
        }

        pub fn natural_size(&self) -> Option<(u32, u32)> {
            lock(&self.facts).size
        }
    }

    impl Drop for GstPlayer {
        /// Stopping waits for the streaming threads; it happens on a thread of its own, so
        /// closing the preview never stalls the window.
        fn drop(&mut self) {
            if let Some(pipeline) = self.pipeline.take() {
                let stopped = std::thread::Builder::new().name("video-stop".into()).spawn(move || {
                    let _ = pipeline.set_state(gst::State::Null);
                });
                drop(stopped);
            }
        }
    }

    fn seek_to(pipeline: &gst::Element, seconds: f64) {
        let at = gst::ClockTime::from_nseconds((seconds.max(0.0) * 1e9) as u64);
        let _ = pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, at);
    }

    /// The speaker the settings name, as a GStreamer sink: the device lists use the same
    /// descriptions PulseAudio (and PipeWire's Pulse server) gives cpal.
    fn speaker_sink(name: &str) -> Option<gst::Element> {
        let monitor = gst::DeviceMonitor::new();
        monitor.add_filter(Some("Audio/Sink"), None);
        monitor.devices().into_iter().find(|device| device.display_name() == name)?.create_element(None).ok()
    }

    fn build(
        path: &std::path::Path,
        speaker: Option<&str>,
        pixels: &rune::Pixels,
        facts: &Arc<Mutex<Facts>>,
        changed: &rune::Waker,
    ) -> Result<gst::Element, String> {
        gst::init().map_err(|error| error.to_string())?;
        let uri = gst::glib::filename_to_uri(path, None).map_err(|error| error.to_string())?;
        let caps = gst_video::VideoCapsBuilder::new().format(gst_video::VideoFormat::Rgba).build();
        let sink = gst_app::AppSink::builder().caps(&caps).max_buffers(2).drop(true).build();
        let (frames, sizes, loaded) = (pixels.clone(), Arc::clone(facts), changed.clone());
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_preroll(move |sink| {
                    if let Ok(sample) = sink.pull_preroll() {
                        show(&sample, &frames, &sizes, &loaded);
                    }
                    Ok(gst::FlowSuccess::Ok)
                })
                .new_sample({
                    let (frames, sizes, loaded) = (pixels.clone(), Arc::clone(facts), changed.clone());
                    move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        show(&sample, &frames, &sizes, &loaded);
                        Ok(gst::FlowSuccess::Ok)
                    }
                })
                .build(),
        );
        let pipeline = gst::ElementFactory::make("playbin")
            .property("uri", uri.as_str())
            .property("video-sink", &sink)
            .build()
            .map_err(|error| error.to_string())?;
        if let Some(audio) = speaker.and_then(speaker_sink) {
            pipeline.set_property("audio-sink", &audio);
        }
        // Phones record upright pictures sideways with a tag saying how to turn them.
        if let Ok(upright) = gst::ElementFactory::make("videoflip").property_from_str("video-direction", "auto").build()
        {
            pipeline.set_property("video-filter", &upright);
        }
        let bus = pipeline.bus().ok_or("no bus")?;
        let (heard, changed) = (Arc::clone(facts), changed.clone());
        bus.set_sync_handler(move |_, message| {
            use gst::MessageView;
            let mut facts = lock(&heard);
            let tell = match message.view() {
                MessageView::Eos(_) => {
                    facts.ended = true;
                    facts.playing = false;
                    true
                }
                MessageView::Error(error) => {
                    facts.failed = Some(error.error().to_string());
                    true
                }
                MessageView::DurationChanged(_) => {
                    facts.duration = None;
                    true
                }
                // Loaded (a file with sound and no picture shows nothing, but it has loaded), or
                // a seek done: the controls show where it is now.
                MessageView::AsyncDone(_) => {
                    facts.loaded = true;
                    true
                }
                _ => false,
            };
            drop(facts);
            if tell {
                changed();
            }
            gst::BusSyncReply::Drop
        });
        pipeline.set_state(gst::State::Playing).map_err(|error| error.to_string())?;
        Ok(pipeline)
    }

    /// Hands one decoded frame to the element showing it, row by row (rows may be padded).
    fn show(sample: &gst::Sample, pixels: &rune::Pixels, facts: &Mutex<Facts>, changed: &rune::Waker) {
        let (Some(buffer), Some(caps)) = (sample.buffer(), sample.caps()) else { return };
        let Ok(info) = gst_video::VideoInfo::from_caps(caps) else { return };
        let Ok(frame) = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info) else { return };
        let (width, height) = (info.width(), info.height());
        let (Ok(data), Some(&stride)) = (frame.plane_data(0), frame.plane_stride().first()) else { return };
        let row = width as usize * 4;
        let mut rgba = Vec::with_capacity(row * height as usize);
        for y in 0..height as usize {
            let start = y * stride.max(0) as usize;
            let Some(line) = data.get(start..start + row) else { return };
            rgba.extend_from_slice(line);
        }
        pixels.set(width, height, rgba);
        let first = {
            let mut facts = lock(facts);
            let first = !facts.loaded || facts.size != Some((width, height));
            facts.loaded = true;
            facts.size = Some((width, height));
            first
        };
        if first {
            changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a real file through GStreamer, as the preview does on Linux: it needs GStreamer's
    /// VP8 and Opus plugins (`gst-plugins-good`, `gst-plugins-base`), so it runs only when asked
    /// (`cargo test -p better-matrix-desktop -- --ignored`).
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    #[ignore = "needs GStreamer's VP8 and Opus plugins and a sound server"]
    fn gstreamer_should_play_a_video_file_seek_in_it_and_end() {
        use std::time::Duration;

        let (_dir, path) = encoded(
            "videotestsrc num-buffers=60 ! video/x-raw,width=320,height=240,framerate=30/1 ! vp8enc deadline=1 \
             ! webmmux name=m ! filesink location={} audiotestsrc num-buffers=90 ! audioconvert ! opusenc ! m.",
            "clip.webm",
        );
        let player = Player::open(path, None, std::sync::Arc::new(|| {}));
        until(&player, &|p| p.playback() == Playback::Playing && p.natural_size().is_some());
        assert_eq!(player.natural_size(), Some((320, 240)));
        assert!(player.duration().is_some_and(|d| (d - 2.0).abs() < 0.2), "{:?}", player.duration());
        player.pause();
        player.seek(0.5);
        until(&player, &|p| (p.time() - 1.0).abs() < 0.1);
        assert_eq!(player.playback(), Playback::Paused);
        player.set_muted(true);
        assert!(player.muted());
        player.play();
        until(&player, &|p| p.playback() == Playback::Ended);
        assert!((player.progress() - 1.0).abs() < 0.01);
        player.seek(0.25);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(player.playback(), Playback::Paused);
        assert!((player.time() - 0.5).abs() < 0.1, "a seek after the end should stay put: {}", player.time());
        player.play();
        until(&player, &|p| p.playback() == Playback::Playing && p.time() < 1.0);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    #[ignore = "needs GStreamer's libav plugin"]
    fn gstreamer_should_turn_a_video_recorded_sideways_upright() {
        let (_dir, path) = encoded(
            "videotestsrc num-buffers=10 ! video/x-raw,width=320,height=240 ! avenc_mpeg4 \
             ! taginject tags=image-orientation=rotate-90 ! mp4mux ! filesink location={}",
            "sideways.mp4",
        );
        let player = Player::open(path, None, std::sync::Arc::new(|| {}));
        until(&player, &|p| p.natural_size().is_some());
        assert_eq!(player.natural_size(), Some((240, 320)));
    }

    /// Encodes a clip with GStreamer's `pipeline` (its `filesink` writing to `{}`) into a
    /// temporary folder, kept for as long as the caller holds it.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn encoded(pipeline: &str, name: &str) -> (tempfile::TempDir, PathBuf) {
        use gstreamer::prelude::*;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        gstreamer::init().unwrap();
        let encode = gstreamer::parse::launch(&pipeline.replace("{}", &path.display().to_string())).unwrap();
        encode.set_state(gstreamer::State::Playing).unwrap();
        let bus = encode.bus().unwrap();
        bus.timed_pop_filtered(gstreamer::ClockTime::from_seconds(20), &[gstreamer::MessageType::Eos]).unwrap();
        encode.set_state(gstreamer::State::Null).unwrap();
        (dir, path)
    }

    /// Waits up to 10 s for `done`.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn until(player: &Player, done: &dyn Fn(&Player) -> bool) {
        let start = std::time::Instant::now();
        while !done(player) {
            assert!(start.elapsed() < std::time::Duration::from_secs(10), "stuck at {:?}", player.playback());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn a_detached_player_should_keep_what_it_was_asked() {
        let player = Player::Detached(Detached::new(Some(10.0), Some((1280, 720))));
        assert_eq!(player.playback(), Playback::Playing);
        player.pause();
        player.seek(0.5);
        player.set_muted(true);
        assert_eq!((player.playback(), player.time(), player.muted()), (Playback::Paused, 5.0, true));
    }
}
