//! The user's media in a call and the video the app watches. The microphone goes through
//! WebRTC's audio device module, which also plays everyone else. The camera and a shared screen
//! each run on a thread of their own, feeding a WebRTC video source and, for the preview, the
//! app's sink. Video from others is converted to RGBA on a task per track.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use futures_util::StreamExt;
// Screen capture is desktop-only in libwebrtc; phones share no screen yet (ADR 0010).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use livekit::webrtc::desktop_capturer::{
    CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
};
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::{
    I420Buffer, RtcVideoSource, RtcVideoTrack, VideoBuffer, VideoFormatType, VideoFrame, VideoResolution, VideoRotation,
};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};

use super::platform::{self, PlatformCamera};
use super::{CallError, VideoId, VideoSink};

/// The camera's resolution and rate, as asked; the camera picks the closest it has.
const CAMERA: (u32, u32, u32) = (1280, 720, 30);
/// How often a shared screen is captured.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const SCREEN_INTERVAL: Duration = Duration::from_millis(1000 / 15);
/// How long the camera's reads may keep failing before it counts as gone.
const CAMERA_GONE_AFTER: Duration = Duration::from_secs(1);
/// Previews of the user's own video are scaled down to this width at most.
pub(super) const PREVIEW_WIDTH: u32 = 640;

/// The video sinks the app asked for, by video, shared with the threads and tasks that feed them.
#[derive(Clone, Default)]
pub(crate) struct Sinks {
    sinks: Arc<Mutex<HashMap<VideoId, VideoSink>>>,
    next: Arc<AtomicU64>,
    /// Where a capture that ended on its own (a camera unplugged, a share stopped from the
    /// system) says so.
    ended: Option<tokio::sync::mpsc::UnboundedSender<Ended>>,
}

/// A capture that ended on its own.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ended {
    /// Which: its [`Capture::instance`].
    pub instance: u64,
    /// It ended before showing anything: a screen share whose picker was cancelled.
    pub unstarted: bool,
}

impl Sinks {
    /// Sinks whose captures report their own end on the receiver.
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<Ended>) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        (Self { ended: Some(sender), ..Self::default() }, receiver)
    }

    /// Capture `instance` ended without being stopped.
    pub(super) fn ended(&self, instance: u64, unstarted: bool) {
        if let Some(sender) = &self.ended {
            let _ = sender.send(Ended { instance, unstarted });
        }
    }

    /// A new video's ID.
    pub fn next_id(&self) -> VideoId {
        VideoId(self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }

    pub fn watch(&self, id: VideoId, sink: VideoSink) {
        if let Ok(mut sinks) = self.sinks.lock() {
            sinks.insert(id, sink);
        }
    }

    pub fn unwatch(&self, id: VideoId) {
        if let Ok(mut sinks) = self.sinks.lock() {
            sinks.remove(&id);
        }
    }

    fn sink(&self, id: VideoId) -> Option<VideoSink> {
        self.sinks.lock().ok().and_then(|sinks| sinks.get(&id).cloned())
    }

    /// Hands a frame to `id`'s sink, if the app watches it, as RGBA no wider than `max`
    /// (scaled while still I420, a quarter of the pixels' bytes).
    pub(super) fn send(&self, id: VideoId, buffer: &dyn VideoBuffer, max: Option<u32>) {
        let Some(sink) = self.sink(id) else { return };
        let (width, height) = (buffer.width(), buffer.height());
        if width == 0 || height == 0 {
            return;
        }
        let scaled = match max {
            Some(max) if width > max => {
                let (w, h) = fit(width, height, max);
                Some(buffer.to_i420().scale(w as i32, h as i32))
            }
            _ => None,
        };
        let buffer: &dyn VideoBuffer = match &scaled {
            Some(scaled) => scaled,
            None => buffer,
        };
        let (width, height) = (buffer.width(), buffer.height());
        let mut rgba = vec![0; (width * height * 4) as usize];
        // libyuv's ABGR is R, G, B, A in memory.
        buffer.to_argb(VideoFormatType::ABGR, &mut rgba, width * 4, width as i32, height as i32);
        (sink.0)(width, height, rgba);
    }
}

/// A size `max` wide in the same proportions, even each way as I420 needs.
fn fit(width: u32, height: u32, max: u32) -> (u32, u32) {
    let even = |n: u32| (n & !1).max(2);
    (even(max), even((u64::from(height) * u64::from(max) / u64::from(width)) as u32))
}

/// Numbers captures: a switched camera keeps its preview, so its end is told apart by this.
static CAPTURES: AtomicU64 = AtomicU64::new(1);

/// A capture thread and the flag that stops it, or the platform's camera (phones). Dropping it
/// stops the capture without waiting; [`Capture::stop_and_wait`] (through [`release`]) also
/// waits for the device to close.
pub(crate) struct Capture {
    pub source: NativeVideoSource,
    pub preview: VideoId,
    /// This capture, as its end is reported.
    pub instance: u64,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// The platform's camera, while it films for this capture.
    platform: Option<Arc<dyn PlatformCamera>>,
}

impl Capture {
    /// A capture of nothing, holding a source and preview while the camera is off or switches.
    pub fn idle(source: NativeVideoSource, preview: VideoId) -> Self {
        Self { source, preview, instance: 0, stop: Arc::new(AtomicBool::new(true)), thread: None, platform: None }
    }

    fn running(
        source: NativeVideoSource,
        preview: VideoId,
        stop: Arc<AtomicBool>,
        thread: JoinHandle<()>,
        instance: u64,
    ) -> Self {
        Self { source, preview, instance, stop, thread: Some(thread), platform: None }
    }

    pub fn rtc_source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.source.clone())
    }

    /// Whether a device feeds it (it isn't [`Capture::idle`]).
    pub fn is_running(&self) -> bool {
        self.thread.is_some() || self.platform.is_some()
    }

    /// Stops the thread and waits for it, so its device is free. Blocking.
    pub fn stop_and_wait(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some(camera) = self.platform.take() {
            platform::stop(camera, self.instance, true);
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(camera) = self.platform.take() {
            platform::stop(camera, self.instance, false);
        }
    }
}

/// Starts a camera (`device`, else the first), feeding `source` (a new one when `None`, the same
/// one when switching cameras, so the call needn't renegotiate) and the preview `preview`.
pub(crate) fn start_camera(
    sinks: Sinks,
    preview: VideoId,
    device: Option<&str>,
    source: Option<NativeVideoSource>,
) -> Result<Capture, CallError> {
    let (width, height, fps) = CAMERA;
    let source = source.unwrap_or_else(|| NativeVideoSource::new(VideoResolution { width, height }, false));
    // Phones film with the system's camera API, which pushes its frames in.
    if let Some(camera) = platform::camera() {
        let instance = CAPTURES.fetch_add(1, Ordering::Relaxed);
        platform::start(&camera, sinks, preview, device, source.clone(), instance).map_err(CallError::Media)?;
        let stop = Arc::new(AtomicBool::new(false));
        return Ok(Capture { source, preview, instance, stop, thread: None, platform: Some(camera) });
    }
    // The camera by its name, else the first.
    let index = device
        .and_then(|name| cameras().into_iter().find(|(_, found)| found == name).map(|(index, _)| index))
        .unwrap_or(CameraIndex::Index(0));
    let stop = Arc::new(AtomicBool::new(false));
    let instance = CAPTURES.fetch_add(1, Ordering::Relaxed);
    // The camera opens on its own thread, which reports whether it could.
    let (opened, ready) = std::sync::mpsc::channel();
    let thread = {
        let (source, stop) = (source.clone(), Arc::clone(&stop));
        std::thread::Builder::new()
            .name("call-camera".into())
            .spawn(move || {
                let format = CameraFormat::new_from(width, height, FrameFormat::MJPEG, fps);
                let requested =
                    RequestedFormat::new::<nokhwa::pixel_format::RgbFormat>(RequestedFormatType::Closest(format));
                let mut camera = match nokhwa::Camera::new(index, requested)
                    .and_then(|mut camera| camera.open_stream().map(|()| camera))
                {
                    Ok(camera) => {
                        let _ = opened.send(Ok(()));
                        camera
                    }
                    Err(error) => {
                        let _ = opened.send(Err(error.to_string()));
                        return;
                    }
                };
                // A read can fail once (a busy system); failing for a second means the camera
                // went (unplugged, or taken by another app).
                let mut failing_since: Option<std::time::Instant> = None;
                while !stop.load(Ordering::Relaxed) {
                    let Ok(frame) = camera.frame() else {
                        let since = *failing_since.get_or_insert_with(std::time::Instant::now);
                        if since.elapsed() >= CAMERA_GONE_AFTER {
                            sinks.ended(instance, false);
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    };
                    failing_since = None;
                    let size = frame.resolution();
                    let Some(i420) = to_i420(frame.source_frame_format(), frame.buffer(), size.width(), size.height())
                    else {
                        continue;
                    };
                    sinks.send(preview, &i420, Some(PREVIEW_WIDTH));
                    let frame = VideoFrame {
                        rotation: VideoRotation::VideoRotation0,
                        timestamp_us: 0,
                        frame_metadata: None,
                        buffer: i420,
                    };
                    source.capture_frame(&frame);
                }
                let _ = camera.stop_stream();
            })
            .map_err(|error| CallError::Media(error.to_string()))?
    };
    match ready.recv() {
        Ok(Ok(())) => Ok(Capture::running(source, preview, stop, thread, instance)),
        Ok(Err(error)) => Err(CallError::Media(error)),
        Err(_) => Err(CallError::Media("the camera stopped".into())),
    }
}

/// Phones share no screen yet: libwebrtc's desktop capturer doesn't exist there (ADR 0010).
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn start_screen(_sinks: Sinks, _preview: VideoId) -> Result<Capture, CallError> {
    Err(CallError::Media("screen sharing isn't available here".to_owned()))
}

/// Starts sharing a screen or window: the system picker chooses on Linux (the desktop portal)
/// and macOS, the first screen elsewhere.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn start_screen(sinks: Sinks, preview: VideoId) -> Result<Capture, CallError> {
    let source = NativeVideoSource::new(VideoResolution { width: 1920, height: 1080 }, true);
    let stop = Arc::new(AtomicBool::new(false));
    let instance = CAPTURES.fetch_add(1, Ordering::Relaxed);
    let (opened, ready) = std::sync::mpsc::channel();
    let thread = {
        let (source, stop) = (source.clone(), Arc::clone(&stop));
        std::thread::Builder::new()
            .name("call-screen".into())
            .spawn(move || {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                let kind = DesktopCaptureSourceType::Generic;
                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                let kind = DesktopCaptureSourceType::Screen;
                let mut options = DesktopCapturerOptions::new(kind);
                options.set_include_cursor(true);
                let Some(mut capturer) = DesktopCapturer::new(options) else {
                    let _ = opened.send(Err("screen sharing isn't available here".to_owned()));
                    return;
                };
                let picked = if cfg!(any(target_os = "linux", target_os = "macos")) {
                    None
                } else {
                    capturer.get_source_list().into_iter().next()
                };
                let _ = opened.send(Ok(()));
                let (source_for_frames, frames_sinks) = (source.clone(), sinks.clone());
                // The share ended from outside (the system's stop button, the window closed).
                let lost = Arc::new(AtomicBool::new(false));
                let lost_in_capture = Arc::clone(&lost);
                let shown = Arc::new(AtomicBool::new(false));
                let shown_in_capture = Arc::clone(&shown);
                capturer.start_capture(picked, move |result| {
                    let frame = match result {
                        Ok(frame) => frame,
                        Err(CaptureError::Permanent) => {
                            lost_in_capture.store(true, Ordering::Relaxed);
                            return;
                        }
                        Err(CaptureError::Temporary) => return,
                    };
                    let (width, height) = (frame.width().max(0) as u32, frame.height().max(0) as u32);
                    if width == 0 || height == 0 {
                        return;
                    }
                    let mut i420 = I420Buffer::new(width, height);
                    let (sy, su, sv) = i420.strides();
                    let (y, u, v) = i420.data_mut();
                    // Desktop frames are BGRA in memory: libyuv's ARGB.
                    yuv_helper::argb_to_i420(
                        frame.data(),
                        frame.stride(),
                        y,
                        sy,
                        u,
                        su,
                        v,
                        sv,
                        width as i32,
                        height as i32,
                    );
                    shown_in_capture.store(true, Ordering::Relaxed);
                    frames_sinks.send(preview, &i420, Some(PREVIEW_WIDTH));
                    let frame = VideoFrame {
                        rotation: VideoRotation::VideoRotation0,
                        timestamp_us: 0,
                        frame_metadata: None,
                        buffer: i420,
                    };
                    source_for_frames.capture_frame(&frame);
                });
                while !stop.load(Ordering::Relaxed) {
                    if lost.load(Ordering::Relaxed) {
                        sinks.ended(instance, !shown.load(Ordering::Relaxed));
                        break;
                    }
                    capturer.capture_frame();
                    std::thread::sleep(SCREEN_INTERVAL);
                }
            })
            .map_err(|error| CallError::Media(error.to_string()))?
    };
    match ready.recv() {
        Ok(Ok(())) => Ok(Capture::running(source, preview, stop, thread, instance)),
        Ok(Err(error)) => Err(CallError::Media(error)),
        Err(_) => Err(CallError::Media("screen sharing stopped".into())),
    }
}

/// The microphones, speakers and cameras there are now. Blocking: asks the audio device module
/// and the camera backend.
pub(crate) fn list_devices() -> super::CallDevices {
    use super::MediaDevice;
    let mut devices = super::CallDevices::default();
    if let Ok(audio) = livekit::PlatformAudio::new() {
        devices.microphones = audio.recording_devices().map(|d| MediaDevice { name: d.name }).collect();
        devices.speakers = audio.playout_devices().map(|d| MediaDevice { name: d.name }).collect();
    }
    devices.cameras = match platform::camera() {
        Some(camera) => camera.cameras().into_iter().map(|name| MediaDevice { name }).collect(),
        None => cameras().into_iter().map(|(_, name)| MediaDevice { name }).collect(),
    };
    devices
}

/// Points the audio device module at the picked microphone and speaker before they start,
/// found by name; the first device of each (the system's default) when none is picked or it's
/// gone.
pub(crate) fn use_audio_devices(audio: &livekit::PlatformAudio, choice: &super::DeviceChoice) {
    let _ = audio.set_recording_device_by_index(microphone_index(audio, choice));
    let _ = audio.set_playout_device_by_index(speaker_index(audio, choice));
}

/// Moves the call to the device picked for `kind` (a switch restarts the device, so only the
/// one picked switches).
pub(crate) fn switch_audio_device(
    audio: &livekit::PlatformAudio,
    choice: &super::DeviceChoice,
    kind: super::DeviceKind,
) {
    let _ = match kind {
        super::DeviceKind::Microphone => audio.switch_recording_device_by_index(microphone_index(audio, choice)),
        super::DeviceKind::Speaker => audio.switch_playout_device_by_index(speaker_index(audio, choice)),
        super::DeviceKind::Camera => return,
    };
}

fn microphone_index(audio: &livekit::PlatformAudio, choice: &super::DeviceChoice) -> usize {
    let picked = choice.microphone.as_ref().and_then(|name| audio.recording_devices().find(|d| d.name == *name));
    picked.map_or(0, |device| device.index)
}

fn speaker_index(audio: &livekit::PlatformAudio, choice: &super::DeviceChoice) -> usize {
    let picked = choice.speaker.as_ref().and_then(|name| audio.playout_devices().find(|d| d.name == *name));
    picked.map_or(0, |device| device.index)
}

/// The cameras there are, once each by name, in the system's order. On Linux one webcam has
/// several nodes by the same name (the others carry metadata, not video): the first is the one
/// that films.
fn cameras() -> Vec<(CameraIndex, String)> {
    let found = nokhwa::query(nokhwa::utils::ApiBackend::Auto).unwrap_or_default();
    once_by_name(found.into_iter().map(|camera| (camera.index().clone(), camera.human_name())).collect())
}

/// The first camera of each name, in index order.
fn once_by_name(mut cameras: Vec<(CameraIndex, String)>) -> Vec<(CameraIndex, String)> {
    cameras.sort_by_key(|(index, _)| index.as_index().unwrap_or(u32::MAX));
    let mut seen = std::collections::HashSet::new();
    cameras.retain(|(_, name)| seen.insert(name.clone()));
    cameras
}

/// Feeds a remote video track to the sink for `id` until the track ends or the task is dropped.
pub(crate) async fn watch_remote(track: RtcVideoTrack, id: VideoId, sinks: Sinks) {
    let mut stream = NativeVideoStream::new(track);
    while let Some(frame) = stream.next().await {
        sinks.send(id, frame.buffer.as_ref(), None);
    }
}

/// A camera frame as I420, whatever format the camera delivers.
fn to_i420(format: FrameFormat, data: &[u8], width: u32, height: u32) -> Option<I420Buffer> {
    let mut i420 = I420Buffer::new(width, height);
    let (sy, su, sv) = i420.strides();
    let (w, h) = (width as i32, height as i32);
    match format {
        FrameFormat::MJPEG => {
            let options = zune_core::options::DecoderOptions::default()
                .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGBA);
            let mut decoder =
                zune_jpeg::JpegDecoder::new_with_options(zune_core::bytestream::ZCursor::new(data), options);
            let rgba = decoder.decode().ok()?;
            let info = decoder.info()?;
            if (info.width as u32, info.height as u32) != (width, height) {
                return to_i420_rgba(&rgba, info.width as u32, info.height as u32);
            }
            let (y, u, v) = i420.data_mut();
            yuv_helper::abgr_to_i420(&rgba, width * 4, y, sy, u, su, v, sv, w, h);
        }
        FrameFormat::NV12 => {
            let luma = (width * height) as usize;
            let (y, u, v) = i420.data_mut();
            yuv_helper::nv12_to_i420(data.get(..luma)?, width, data.get(luma..)?, width, y, sy, u, su, v, sv, w, h);
        }
        FrameFormat::YUYV => yuyv_to_i420(data, width, height, &mut i420)?,
        FrameFormat::RAWRGB | FrameFormat::RAWBGR | FrameFormat::GRAY => {
            let rgba = expand_to_rgba(format, data, width, height)?;
            return to_i420_rgba(&rgba, width, height);
        }
    }
    Some(i420)
}

fn to_i420_rgba(rgba: &[u8], width: u32, height: u32) -> Option<I420Buffer> {
    if rgba.len() < (width * height * 4) as usize {
        return None;
    }
    let mut i420 = I420Buffer::new(width, height);
    let (sy, su, sv) = i420.strides();
    let (y, u, v) = i420.data_mut();
    yuv_helper::abgr_to_i420(rgba, width * 4, y, sy, u, su, v, sv, width as i32, height as i32);
    Some(i420)
}

/// RGB, BGR or grey pixels as RGBA.
fn expand_to_rgba(format: FrameFormat, data: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let pixels = (width * height) as usize;
    let mut rgba = Vec::with_capacity(pixels * 4);
    match format {
        FrameFormat::GRAY => data.get(..pixels)?.iter().for_each(|g| rgba.extend_from_slice(&[*g, *g, *g, 255])),
        FrameFormat::RAWBGR => data
            .get(..pixels * 3)?
            .as_chunks::<3>()
            .0
            .iter()
            .for_each(|[b, g, r]| rgba.extend_from_slice(&[*r, *g, *b, 255])),
        _ => data
            .get(..pixels * 3)?
            .as_chunks::<3>()
            .0
            .iter()
            .for_each(|[r, g, b]| rgba.extend_from_slice(&[*r, *g, *b, 255])),
    }
    Some(rgba)
}

/// Packed YUYV (Y0 U Y1 V per two pixels) into planar I420, averaging chroma over row pairs.
fn yuyv_to_i420(data: &[u8], width: u32, height: u32, i420: &mut I420Buffer) -> Option<()> {
    let (w, h) = (width as usize, height as usize);
    if data.len() < w * h * 2 {
        return None;
    }
    let (sy, su, sv) = i420.strides();
    let (y, u, v) = i420.data_mut();
    for row in 0..h {
        for col in 0..w {
            y[row * sy as usize + col] = data[(row * w + col) * 2];
        }
    }
    for row in (0..h).step_by(2) {
        let below = (row + 1).min(h - 1);
        for pair in 0..w / 2 {
            let at = |r: usize, offset: usize| u16::from(data[(r * w + pair * 2) * 2 + offset]);
            let at_out = (row / 2) * su as usize + pair;
            u[at_out] = ((at(row, 1) + at(below, 1)) / 2) as u8;
            v[(row / 2) * sv as usize + pair] = ((at(row, 3) + at(below, 3)) / 2) as u8;
        }
    }
    Some(())
}

/// Runs device work (opening a camera waits for its first frame) on a blocking thread.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, CallError> + Send + 'static,
) -> Result<T, CallError> {
    tokio::task::spawn_blocking(work).await.unwrap_or_else(|error| Err(CallError::Media(error.to_string())))
}

/// A source for a camera, at the size cameras are asked for.
pub(crate) fn camera_source() -> NativeVideoSource {
    let (width, height, _) = CAMERA;
    NativeVideoSource::new(VideoResolution { width, height }, false)
}

/// [`start_camera`], with a new source and preview, off the async threads.
pub(crate) async fn open_camera(sinks: &Sinks, device: Option<String>) -> Result<Capture, CallError> {
    let sinks = sinks.clone();
    blocking(move || start_camera(sinks.clone(), sinks.next_id(), device.as_deref(), None)).await
}

/// Stops captures and waits (on a blocking thread) until their devices are free.
pub(crate) async fn release(captures: impl IntoIterator<Item = Capture> + Send + 'static) {
    let _ = tokio::task::spawn_blocking(move || captures.into_iter().for_each(Capture::stop_and_wait)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_webcams_metadata_node_should_not_list_as_a_second_camera() {
        let found = vec![
            (CameraIndex::Index(1), "Webcam".to_owned()),
            (CameraIndex::Index(0), "Webcam".to_owned()),
            (CameraIndex::Index(2), "Capture card".to_owned()),
        ];
        let listed = once_by_name(found);
        assert_eq!(
            listed,
            [(CameraIndex::Index(0), "Webcam".to_owned()), (CameraIndex::Index(2), "Capture card".to_owned())]
        );
    }

    #[test]
    fn a_preview_should_fit_the_width_and_keep_the_aspect() {
        assert_eq!(fit(1280, 720, 640), (640, 360));
        assert_eq!(fit(1920, 1081, 640), (640, 360));
    }

    #[test]
    fn grey_should_expand_to_opaque_rgba() {
        assert_eq!(expand_to_rgba(FrameFormat::GRAY, &[9], 1, 1), Some(vec![9, 9, 9, 255]));
    }

    #[test]
    fn bgr_should_swap_into_rgba() {
        assert_eq!(expand_to_rgba(FrameFormat::RAWBGR, &[1, 2, 3], 1, 1), Some(vec![3, 2, 1, 255]));
    }
}
