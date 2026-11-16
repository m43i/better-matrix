//! The C entry points for the app's native code (iOS's Swift; any platform with a C ABI): video
//! views ([`crate::video`]) and camera frames ([`crate::camera`]). Android's Kotlin reaches the
//! same through JNI ([`crate::android`]). Declared for Swift in
//! `apps/mobile/modules/call-media/ios/MatrixCallMedia.h`.

#![expect(unsafe_code, reason = "C entry points take raw pointers from the platform's native code")]

use std::ffi::c_void;

use matrix_core::VideoId;
use matrix_core::calls::{CameraFrame, CameraPlanes, camera_lost, push_camera_frame};

use crate::video::{self, Fit, Target};

/// Draws one frame: `rgba` holds `width` × `height` tightly packed RGBA pixels, valid only
/// during the call. Called on a media thread.
pub type DrawFn = extern "C" fn(context: *mut c_void, width: u32, height: u32, rgba: *const u8, length: usize);

/// A view's context and drawing function. The view keeps the context alive until it detached.
struct Callback {
    context: *mut c_void,
    draw: DrawFn,
}

// The context is the view's own, which draws from any thread (it hops to the main thread).
unsafe impl Send for Callback {}

impl Target for Callback {
    fn draw(&mut self, width: u32, height: u32, rgba: &[u8]) {
        (self.draw)(self.context, width, height, rgba.as_ptr(), rgba.len());
    }
}

/// Shows video `video` through `draw` with `context`; `contain` letterboxes instead of
/// cropping. Returns the view's token for [`bm_video_resize`] and [`bm_video_detach`].
#[unsafe(no_mangle)]
pub extern "C" fn bm_video_attach(video: u64, contain: bool, context: *mut c_void, draw: DrawFn) -> u64 {
    let fit = if contain { Fit::Contain } else { Fit::Cover };
    video::attach(VideoId(video), fit, Box::new(Callback { context, draw }))
}

/// The view `token` is now `width` × `height` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn bm_video_resize(token: u64, width: u32, height: u32) {
    video::resize(token, width, height);
}

/// Stops drawing into the view `token`; its context isn't used after this returns.
#[unsafe(no_mangle)]
pub extern "C" fn bm_video_detach(token: u64) {
    video::detach(token);
}

/// One frame of the camera, as iOS delivers it (`420YpCbCr8BiPlanar`): `rotation` degrees
/// clockwise to stand upright. False when no camera runs in a call.
///
/// # Safety
/// `y` must hold `stride_y × height` bytes and `uv` `stride_uv × ⌈height / 2⌉`, for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bm_camera_frame_nv12(
    width: u32,
    height: u32,
    rotation: u32,
    y: *const u8,
    stride_y: u32,
    uv: *const u8,
    stride_uv: u32,
) -> bool {
    if y.is_null() || uv.is_null() {
        return false;
    }
    let chroma_rows = height.div_ceil(2) as usize;
    // SAFETY: the caller guarantees these lengths.
    let (y, uv) = unsafe {
        (
            std::slice::from_raw_parts(y, stride_y as usize * height as usize),
            std::slice::from_raw_parts(uv, stride_uv as usize * chroma_rows),
        )
    };
    push_camera_frame(CameraFrame {
        width,
        height,
        rotation,
        planes: CameraPlanes::Nv12 { y, stride_y, uv, stride_uv },
    })
}

/// The camera stopped on its own (taken by another app, the system ended the session).
#[unsafe(no_mangle)]
pub extern "C" fn bm_camera_lost() {
    camera_lost();
}
