//! Call video for the platform's own views, without JavaScript in the way: a view hands the
//! bridge a drawing target (an Android `Surface`, an iOS callback) for one `VideoId`, and the
//! engine's frames go straight from its media threads to the target, cropped (`Cover`) or
//! letterboxed (`Contain`) to the view's shape. Several views may show the same video; the
//! engine watches a video while at least one view does (`Command::WatchVideo`, whose sink can't
//! cross as JSON). ADR 0010 records the design.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use matrix_core::{Command, EngineHandle, VideoId, VideoSink};

/// How a frame fills a view of another shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Fills the view, cropping what sticks out: cameras.
    Cover,
    /// Shows the whole frame, black bars on the sides: shared screens.
    Contain,
}

/// Something a frame can be drawn into: tightly packed RGBA rows of `width` × `height`.
pub(crate) trait Target: Send {
    fn draw(&mut self, width: u32, height: u32, rgba: &[u8]);
}

/// The engine views ask for their videos, while the bridge runs.
static ENGINE: Mutex<Option<EngineHandle>> = Mutex::new(None);

/// The views showing one video; its sink draws into each.
type Views = Arc<Mutex<Vec<View>>>;

/// Every video shown, by its ID.
static SHOWN: Mutex<Option<HashMap<VideoId, Views>>> = Mutex::new(None);

/// Which video each view shows, by its token.
static TOKENS: Mutex<Option<HashMap<u64, VideoId>>> = Mutex::new(None);

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

struct View {
    token: u64,
    fit: Fit,
    /// The view's size in pixels; 0 until it's laid out (frames then keep their own shape).
    size: (u32, u32),
    target: Box<dyn Target>,
    /// The last frame's shaped pixels, reused from frame to frame.
    scratch: Vec<u8>,
}

/// Lets views ask `engine` for video (`None` when the bridge stops).
pub(crate) fn set_engine(engine: Option<EngineHandle>) {
    if let Ok(mut current) = ENGINE.lock() {
        *current = engine;
    }
}

fn send(command: Command) {
    if let Some(engine) = ENGINE.lock().ok().and_then(|engine| engine.clone()) {
        engine.send(command);
    }
}

/// Shows `video` in `target` until [`detach`]; returns the view's token.
pub(crate) fn attach(video: VideoId, fit: Fit, target: Box<dyn Target>) -> u64 {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let view = View { token, fit, size: (0, 0), target, scratch: Vec::new() };
    let mut first = false;
    if let Ok(mut shown) = SHOWN.lock() {
        let views = shown.get_or_insert_with(HashMap::new).entry(video).or_insert_with(|| {
            first = true;
            Arc::new(Mutex::new(Vec::new()))
        });
        if let Ok(mut list) = views.lock() {
            list.push(view);
        }
        if first {
            send(Command::WatchVideo { video, sink: sink(Arc::clone(views)) });
        }
    }
    if let Ok(mut tokens) = TOKENS.lock() {
        tokens.get_or_insert_with(HashMap::new).insert(token, video);
    }
    token
}

/// The view `token` is now `width` × `height` pixels.
pub(crate) fn resize(token: u64, width: u32, height: u32) {
    with_view(token, |view| view.size = (width, height));
}

/// Stops showing video in the view `token`; no frame reaches its target after this returns.
pub(crate) fn detach(token: u64) {
    let Some(video) = TOKENS.lock().ok().and_then(|mut tokens| tokens.as_mut()?.remove(&token)) else { return };
    let Ok(mut shown) = SHOWN.lock() else { return };
    let Some(map) = shown.as_mut() else { return };
    let Some(views) = map.get(&video).cloned() else { return };
    // Waits for a frame being drawn into it.
    let empty = views.lock().map(|mut list| {
        list.retain(|view| view.token != token);
        list.is_empty()
    });
    if empty.unwrap_or(true) {
        map.remove(&video);
        send(Command::UnwatchVideo(video));
    }
}

fn with_view(token: u64, change: impl FnOnce(&mut View)) {
    let Some(video) = TOKENS.lock().ok().and_then(|tokens| tokens.as_ref()?.get(&token).copied()) else { return };
    let views = SHOWN.lock().ok().and_then(|shown| shown.as_ref()?.get(&video).cloned());
    if let Some(views) = views
        && let Ok(mut list) = views.lock()
        && let Some(view) = list.iter_mut().find(|view| view.token == token)
    {
        change(view);
    }
}

/// The sink the engine feeds one video's frames to: every view showing it draws them.
fn sink(views: Views) -> VideoSink {
    VideoSink(Arc::new(move |width, height, rgba: Vec<u8>| {
        let Ok(mut list) = views.lock() else { return };
        for view in list.iter_mut() {
            let placed = place((width, height), view.size, view.fit);
            if placed.is_identity((width, height)) {
                view.target.draw(width, height, &rgba);
            } else {
                shape(&rgba, width, &placed, &mut view.scratch);
                view.target.draw(placed.out.0, placed.out.1, &view.scratch);
            }
        }
    }))
}

/// Where a frame's pixels go in the buffer a view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    /// The buffer's size: the view's shape at about the frame's resolution.
    pub out: (u32, u32),
    /// The part of the frame shown: x, y, width, height.
    pub src: (u32, u32, u32, u32),
    /// Where that part lands in the buffer: x, y.
    pub dst: (u32, u32),
}

impl Placement {
    fn is_identity(&self, frame: (u32, u32)) -> bool {
        self.out == frame && self.src == (0, 0, frame.0, frame.1)
    }
}

/// How a `frame` fills a `view` (its own shape while the view's size isn't known).
pub(crate) fn place(frame: (u32, u32), view: (u32, u32), fit: Fit) -> Placement {
    let (fw, fh) = frame;
    let (vw, vh) = view;
    let whole = Placement { out: frame, src: (0, 0, fw, fh), dst: (0, 0) };
    if vw == 0 || vh == 0 || fw == 0 || fh == 0 {
        return whole;
    }
    // Compare fw/fh with vw/vh without dividing.
    let (frame_wide, view_wide) = (u64::from(fw) * u64::from(vh), u64::from(vw) * u64::from(fh));
    let scaled = |a: u32, b: u32, c: u32| (u64::from(a) * u64::from(b) / u64::from(c).max(1)) as u32;
    match fit {
        Fit::Cover if frame_wide > view_wide => {
            // Wider than the view: crop the sides.
            let w = scaled(fh, vw, vh).max(1);
            Placement { out: (w, fh), src: ((fw - w) / 2, 0, w, fh), dst: (0, 0) }
        }
        Fit::Cover if frame_wide < view_wide => {
            let h = scaled(fw, vh, vw).max(1);
            Placement { out: (fw, h), src: (0, (fh - h) / 2, fw, h), dst: (0, 0) }
        }
        Fit::Contain if frame_wide > view_wide => {
            // Wider than the view: bars above and below.
            let h = scaled(fw, vh, vw).max(fh);
            Placement { out: (fw, h), src: (0, 0, fw, fh), dst: (0, (h - fh) / 2) }
        }
        Fit::Contain if frame_wide < view_wide => {
            let w = scaled(fh, vw, vh).max(fw);
            Placement { out: (w, fh), src: (0, 0, fw, fh), dst: ((w - fw) / 2, 0) }
        }
        _ => whole,
    }
}

/// Copies the placed part of `rgba` (rows `width` pixels long) into `out`, black around it.
pub(crate) fn shape(rgba: &[u8], width: u32, placed: &Placement, out: &mut Vec<u8>) {
    let (ow, oh) = (placed.out.0 as usize, placed.out.1 as usize);
    let (sx, sy, sw, sh) = (placed.src.0 as usize, placed.src.1 as usize, placed.src.2 as usize, placed.src.3 as usize);
    let (dx, dy) = (placed.dst.0 as usize, placed.dst.1 as usize);
    out.clear();
    out.resize(ow * oh * 4, 0);
    // Opaque black in the bars.
    out.as_chunks_mut::<4>().0.iter_mut().for_each(|pixel| pixel[3] = 255);
    let row_bytes = sw * 4;
    for row in 0..sh {
        let from = (sy + row) * width as usize * 4 + sx * 4;
        let to = (dy + row) * ow * 4 + dx * 4;
        if let (Some(src), Some(dst)) = (rgba.get(from..from + row_bytes), out.get_mut(to..to + row_bytes)) {
            dst.copy_from_slice(src);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_camera_in_a_tall_view_should_crop_its_sides() {
        let placed = place((1280, 720), (360, 720), Fit::Cover);
        assert_eq!(placed, Placement { out: (360, 720), src: (460, 0, 360, 720), dst: (0, 0) });
    }

    #[test]
    fn a_tall_camera_in_a_wide_view_should_crop_top_and_bottom() {
        let placed = place((720, 1280), (720, 360), Fit::Cover);
        assert_eq!(placed, Placement { out: (720, 360), src: (0, 460, 720, 360), dst: (0, 0) });
    }

    #[test]
    fn a_wide_screen_in_a_tall_view_should_get_bars_above_and_below() {
        let placed = place((1920, 1080), (1080, 1920), Fit::Contain);
        assert_eq!(placed.out, (1920, 3413));
        assert_eq!(placed.dst, (0, 1166));
        assert_eq!(placed.src, (0, 0, 1920, 1080));
    }

    #[test]
    fn a_view_not_laid_out_yet_should_take_the_frame_as_it_is() {
        let placed = place((640, 480), (0, 0), Fit::Cover);
        assert!(placed.is_identity((640, 480)));
    }

    #[test]
    fn a_frame_of_the_views_shape_should_pass_unchanged() {
        assert!(place((640, 360), (320, 180), Fit::Contain).is_identity((640, 360)));
    }

    #[test]
    fn shaping_should_copy_the_shown_part_and_paint_the_bars_black() {
        // A 2 × 1 frame, red then green, shown in a square: a black row, the frame, a black row.
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255];
        let placed = Placement { out: (2, 3), src: (0, 0, 2, 1), dst: (0, 1) };
        let mut out = Vec::new();
        shape(&rgba, 2, &placed, &mut out);
        let black = [0, 0, 0, 255, 0, 0, 0, 255];
        assert_eq!(&out[..8], &black);
        assert_eq!(&out[8..16], &rgba);
        assert_eq!(&out[16..], &black);
    }

    struct Recorder(Arc<Mutex<Vec<(u32, u32)>>>);

    impl Target for Recorder {
        fn draw(&mut self, width: u32, height: u32, _rgba: &[u8]) {
            if let Ok(mut frames) = self.0.lock() {
                frames.push((width, height));
            }
        }
    }

    #[test]
    fn two_views_of_one_video_should_both_draw_and_one_detaching_should_keep_the_other() {
        let (first, second) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let video = VideoId(9001);
        let a = attach(video, Fit::Cover, Box::new(Recorder(Arc::clone(&first))));
        let b = attach(video, Fit::Cover, Box::new(Recorder(Arc::clone(&second))));
        let views = SHOWN.lock().unwrap().as_ref().unwrap().get(&video).cloned().unwrap();
        let feed = sink(Arc::clone(&views));
        (feed.0)(4, 2, vec![0; 32]);
        detach(a);
        (feed.0)(4, 2, vec![0; 32]);
        assert_eq!((first.lock().unwrap().len(), second.lock().unwrap().len()), (1, 2));
        detach(b);
        assert!(SHOWN.lock().unwrap().as_ref().unwrap().get(&video).is_none());
    }

    #[test]
    fn a_resized_view_should_get_frames_in_its_shape() {
        let frames = Arc::new(Mutex::new(Vec::new()));
        let video = VideoId(9002);
        let token = attach(video, Fit::Cover, Box::new(Recorder(Arc::clone(&frames))));
        resize(token, 1, 1);
        let views = SHOWN.lock().unwrap().as_ref().unwrap().get(&video).cloned().unwrap();
        (sink(views).0)(4, 2, vec![0; 32]);
        detach(token);
        assert_eq!(*frames.lock().unwrap(), [(2, 2)]);
    }
}
