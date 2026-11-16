//! Cameras the platform runs. On phones `nokhwa` has no backend, so the app films with the
//! system's own camera API (Camera2, AVFoundation) and pushes each frame here; the engine still
//! decides when a camera starts, stops and switches, exactly as with `nokhwa` on the desktop.
//! Once an app registers a [`PlatformCamera`], every camera comes from it; the desktop registers
//! none and keeps `nokhwa`.

use std::sync::{Arc, Mutex, RwLock};

use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::{I420Buffer, VideoBuffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;

use super::VideoId;
use super::media::{PREVIEW_WIDTH, Sinks};

/// A camera API the app drives for the engine. Calls come from the engine's blocking threads.
pub trait PlatformCamera: Send + Sync {
    /// The cameras there are, by name, the system's default first.
    fn cameras(&self) -> Vec<String>;
    /// Starts filming with the camera named `device` (the default when `None`, or when it's
    /// gone). Its frames come through [`push_camera_frame`] until [`PlatformCamera::stop`].
    fn start(&self, device: Option<String>) -> Result<(), String>;
    /// Stops filming and lets go of the camera.
    fn stop(&self);
}

/// The registered camera API, if the app has one.
static CAMERA: RwLock<Option<Arc<dyn PlatformCamera>>> = RwLock::new(None);

/// Where pushed frames go while a camera runs.
static FEED: Mutex<Option<Feed>> = Mutex::new(None);

struct Feed {
    source: NativeVideoSource,
    sinks: Sinks,
    preview: VideoId,
    instance: u64,
}

/// Films every camera of every later call with `camera` instead of `nokhwa`.
pub fn use_platform_camera(camera: Arc<dyn PlatformCamera>) {
    if let Ok(mut registered) = CAMERA.write() {
        *registered = Some(camera);
    }
}

/// The registered camera API.
pub(crate) fn camera() -> Option<Arc<dyn PlatformCamera>> {
    CAMERA.read().ok().and_then(|camera| camera.clone())
}

/// The planes of one camera frame, in the layouts cameras deliver.
#[derive(Debug, Clone, Copy)]
pub enum CameraPlanes<'a> {
    /// Planar 4:2:0: Android's `YUV_420_888` as `VideoFrame.Buffer.toI420()` gives it.
    I420 {
        /// Luma.
        y: &'a [u8],
        /// Bytes per row of `y`.
        stride_y: u32,
        /// Blue chroma, half size each way.
        u: &'a [u8],
        /// Bytes per row of `u`.
        stride_u: u32,
        /// Red chroma, half size each way.
        v: &'a [u8],
        /// Bytes per row of `v`.
        stride_v: u32,
    },
    /// Luma and interleaved chroma: iOS's `420YpCbCr8BiPlanar`.
    Nv12 {
        /// Luma.
        y: &'a [u8],
        /// Bytes per row of `y`.
        stride_y: u32,
        /// Blue and red chroma interleaved, half size each way.
        uv: &'a [u8],
        /// Bytes per row of `uv`.
        stride_uv: u32,
    },
}

/// One frame from the platform's camera.
#[derive(Debug, Clone, Copy)]
pub struct CameraFrame<'a> {
    /// Its width as the sensor delivers it, before rotating.
    pub width: u32,
    /// Its height, before rotating.
    pub height: u32,
    /// Degrees clockwise the frame needs to stand upright: 0, 90, 180 or 270.
    pub rotation: u32,
    /// Its pixels.
    pub planes: CameraPlanes<'a>,
}

/// Hands one frame to the camera running in the call: upright, to the call and to the user's
/// preview. False when no camera runs (a frame that raced the stop), which the app may ignore.
pub fn push_camera_frame(frame: CameraFrame<'_>) -> bool {
    let Some((source, sinks, preview)) =
        FEED.lock().ok().and_then(|feed| feed.as_ref().map(|f| (f.source.clone(), f.sinks.clone(), f.preview)))
    else {
        return false;
    };
    let Some(i420) = to_i420(&frame) else { return false };
    let i420 = rotate(&i420, frame.rotation);
    sinks.send(preview, &i420, Some(PREVIEW_WIDTH));
    source.capture_frame(&VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: i420,
    });
    true
}

/// The platform's camera stopped on its own (taken by another app, the system ended it): the
/// call stops sending it and says so.
pub fn camera_lost() {
    let feed = FEED.lock().ok().and_then(|mut feed| feed.take());
    if let Some(feed) = feed {
        feed.sinks.ended(feed.instance, false);
    }
}

/// Starts the platform's camera feeding `source` and the preview. Blocking: the app's camera
/// API answers before it returns.
pub(crate) fn start(
    camera: &Arc<dyn PlatformCamera>,
    sinks: Sinks,
    preview: VideoId,
    device: Option<&str>,
    source: NativeVideoSource,
    instance: u64,
) -> Result<(), String> {
    if let Ok(mut feed) = FEED.lock() {
        *feed = Some(Feed { source, sinks, preview, instance });
    }
    let started = camera.start(device.map(ToOwned::to_owned));
    if started.is_err() {
        clear(instance);
    }
    started
}

/// Stops capture `instance`, unless a newer one replaced it. `wait` calls the app at once;
/// otherwise on a thread of its own, so dropping a capture never waits.
pub(crate) fn stop(camera: Arc<dyn PlatformCamera>, instance: u64, wait: bool) {
    if !clear(instance) {
        return;
    }
    if wait {
        camera.stop();
    } else {
        let _ = std::thread::Builder::new().name("call-camera-stop".into()).spawn(move || camera.stop());
    }
}

/// Forgets the feed of `instance`; true when it was the one running.
fn clear(instance: u64) -> bool {
    let Ok(mut feed) = FEED.lock() else { return false };
    if feed.as_ref().is_some_and(|feed| feed.instance == instance) {
        *feed = None;
        true
    } else {
        false
    }
}

/// The frame as I420, copied out of the app's buffers.
fn to_i420(frame: &CameraFrame<'_>) -> Option<I420Buffer> {
    let (width, height) = (frame.width, frame.height);
    if width == 0 || height == 0 {
        return None;
    }
    let mut i420 = I420Buffer::new(width, height);
    let (sy, su, sv) = i420.strides();
    let (chroma_w, chroma_h) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
    let (y, u, v) = i420.data_mut();
    match frame.planes {
        CameraPlanes::I420 { y: src_y, stride_y, u: src_u, stride_u, v: src_v, stride_v } => {
            copy_plane(src_y, stride_y as usize, y, sy as usize, width as usize, height as usize)?;
            copy_plane(src_u, stride_u as usize, u, su as usize, chroma_w, chroma_h)?;
            copy_plane(src_v, stride_v as usize, v, sv as usize, chroma_w, chroma_h)?;
        }
        CameraPlanes::Nv12 { y: src_y, stride_y, uv, stride_uv } => {
            let rows = |stride: u32, h: usize| (stride as usize) * h.saturating_sub(1);
            if src_y.len() < rows(stride_y, height as usize) + width as usize
                || uv.len() < rows(stride_uv, chroma_h) + chroma_w * 2
            {
                return None;
            }
            yuv_helper::nv12_to_i420(src_y, stride_y, uv, stride_uv, y, sy, u, su, v, sv, width as i32, height as i32);
        }
    }
    Some(i420)
}

/// Copies `width` × `height` bytes between planes of different strides.
fn copy_plane(
    src: &[u8],
    src_stride: usize,
    dst: &mut [u8],
    dst_stride: usize,
    width: usize,
    height: usize,
) -> Option<()> {
    for row in 0..height {
        let from = src.get(row * src_stride..row * src_stride + width)?;
        dst.get_mut(row * dst_stride..row * dst_stride + width)?.copy_from_slice(from);
    }
    Some(())
}

/// The frame turned `degrees` clockwise (a right angle; anything else leaves it).
fn rotate(i420: &I420Buffer, degrees: u32) -> I420Buffer {
    let quarter = match degrees % 360 {
        90 => Quarter::Right,
        180 => Quarter::Half,
        270 => Quarter::Left,
        _ => return copy(i420),
    };
    let (width, height) = (i420.width(), i420.height());
    let (out_w, out_h) = if quarter == Quarter::Half { (width, height) } else { (height, width) };
    let mut out = I420Buffer::new(out_w, out_h);
    let (sy, su, sv) = i420.strides();
    let (dy, du, dv) = out.strides();
    let (y, u, v) = i420.data();
    let (oy, ou, ov) = out.data_mut();
    let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
    rotate_plane(y, sy as usize, width as usize, height as usize, oy, dy as usize, quarter);
    rotate_plane(u, su as usize, cw, ch, ou, du as usize, quarter);
    rotate_plane(v, sv as usize, cw, ch, ov, dv as usize, quarter);
    out
}

fn copy(i420: &I420Buffer) -> I420Buffer {
    let (width, height) = (i420.width(), i420.height());
    let mut out = I420Buffer::new(width, height);
    let (sy, su, sv) = i420.strides();
    let (dy, du, dv) = out.strides();
    let (y, u, v) = i420.data();
    let (oy, ou, ov) = out.data_mut();
    let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
    let _ = copy_plane(y, sy as usize, oy, dy as usize, width as usize, height as usize);
    let _ = copy_plane(u, su as usize, ou, du as usize, cw, ch);
    let _ = copy_plane(v, sv as usize, ov, dv as usize, cw, ch);
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Quarter {
    /// 90° clockwise.
    Right,
    Half,
    /// 90° anticlockwise.
    Left,
}

/// Rotates one plane of `width` × `height` into `dst` (sized for the rotated plane).
fn rotate_plane(
    src: &[u8],
    stride: usize,
    width: usize,
    height: usize,
    dst: &mut [u8],
    dst_stride: usize,
    quarter: Quarter,
) {
    for row in 0..height {
        for col in 0..width {
            let Some(&pixel) = src.get(row * stride + col) else { continue };
            let (to_row, to_col) = match quarter {
                Quarter::Right => (col, height - 1 - row),
                Quarter::Half => (height - 1 - row, width - 1 - col),
                Quarter::Left => (width - 1 - col, row),
            };
            if let Some(out) = dst.get_mut(to_row * dst_stride + to_col) {
                *out = pixel;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quarter_turn_right_should_put_the_left_column_on_top() {
        // 3 × 2:  a b c      d a
        //         d e f  ->  e b
        //                    f c
        let src = *b"abcdef";
        let mut dst = [0u8; 6];
        rotate_plane(&src, 3, 3, 2, &mut dst, 2, Quarter::Right);
        assert_eq!(&dst, b"daebfc");
    }

    #[test]
    fn a_quarter_turn_left_should_put_the_right_column_on_top() {
        let src = *b"abcdef";
        let mut dst = [0u8; 6];
        rotate_plane(&src, 3, 3, 2, &mut dst, 2, Quarter::Left);
        assert_eq!(&dst, b"cfbead");
    }

    #[test]
    fn a_half_turn_should_reverse_the_plane() {
        let src = *b"abcd";
        let mut dst = [0u8; 4];
        rotate_plane(&src, 2, 2, 2, &mut dst, 2, Quarter::Half);
        assert_eq!(&dst, b"dcba");
    }

    #[test]
    fn a_plane_with_padded_rows_should_copy_only_its_pixels() {
        let src = [1, 2, 0, 3, 4, 0];
        let mut dst = [9u8; 4];
        assert_eq!(copy_plane(&src, 3, &mut dst, 2, 2, 2), Some(()));
        assert_eq!(dst, [1, 2, 3, 4]);
    }

    #[test]
    fn a_plane_too_short_for_its_size_should_be_refused() {
        let mut dst = [0u8; 4];
        assert_eq!(copy_plane(&[1, 2, 3], 2, &mut dst, 2, 2, 2), None);
    }

    #[test]
    fn a_frame_pushed_while_no_camera_runs_should_be_dropped() {
        let planes = CameraPlanes::I420 { y: &[0; 4], stride_y: 2, u: &[0], stride_u: 1, v: &[0], stride_v: 1 };
        assert!(!push_camera_frame(CameraFrame { width: 2, height: 2, rotation: 0, planes }));
    }
}
