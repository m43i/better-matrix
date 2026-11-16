//! Android's way in, through JNI: libwebrtc's start-up, video views drawing into a `Surface`, and
//! camera frames. The app's Kotlin (`chat.bettermatrix.callmedia.CallMediaNative`, in
//! `apps/mobile/modules/call-media`) loads this library with `System.loadLibrary`, which runs
//! [`JNI_OnLoad`]: libwebrtc needs the Java VM before it creates anything, and the application
//! context (`initialize`) before its audio device module can open the microphone and speaker.

#![expect(unsafe_code, reason = "JNI entry points and the NDK's ANativeWindow are C APIs")]

use std::ffi::c_void;

use jni::JNIEnv;
use jni::JavaVM;
use jni::objects::{JByteBuffer, JClass, JObject};
use jni::sys::{JNI_FALSE, JNI_TRUE, JNI_VERSION_1_6, jboolean, jint, jlong};
use matrix_core::VideoId;
use matrix_core::calls::{CameraFrame, CameraPlanes, camera_lost, push_camera_frame};

use crate::video::{self, Fit, Target};

/// Runs when Kotlin loads the library: hands libwebrtc the Java VM.
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(vm: *mut jni::sys::JavaVM, _reserved: *mut c_void) -> jint {
    // SAFETY: the VM calls this with its own pointer.
    if let Ok(vm) = unsafe { JavaVM::from_raw(vm) } {
        livekit::webrtc::android::initialize_android(&vm);
    }
    JNI_VERSION_1_6
}

/// `CallMediaNative.initialize(context)`: gives libwebrtc the application context, which its
/// audio device module needs. Idempotent.
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_bettermatrix_callmedia_CallMediaNative_initialize(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) -> jboolean {
    let Ok(vm) = env.get_java_vm() else { return JNI_FALSE };
    if livekit::webrtc::android::initialize_android_context(&vm, &context) { JNI_TRUE } else { JNI_FALSE }
}

/// `CallMediaNative.attach(surface, video, contain)`: draws video `video` into `surface` until
/// `detach`; returns the view's token (0 when the surface is unusable).
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_bettermatrix_callmedia_CallMediaNative_attach(
    env: JNIEnv,
    _class: JClass,
    surface: JObject,
    video: jlong,
    contain: jboolean,
) -> jlong {
    // SAFETY: a live JNIEnv and a android.view.Surface from Kotlin.
    let window = unsafe { ANativeWindow_fromSurface(env.get_raw().cast(), surface.as_raw().cast()) };
    if window.is_null() {
        return 0;
    }
    let fit = if contain == JNI_TRUE { Fit::Contain } else { Fit::Cover };
    video::attach(VideoId(video as u64), fit, Box::new(Window { window, size: (0, 0) })) as jlong
}

/// `CallMediaNative.resize(token, width, height)`: the view's size in pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_bettermatrix_callmedia_CallMediaNative_resize(
    _env: JNIEnv,
    _class: JClass,
    token: jlong,
    width: jint,
    height: jint,
) {
    video::resize(token as u64, width.max(0) as u32, height.max(0) as u32);
}

/// `CallMediaNative.detach(token)`: stops drawing and releases the surface.
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_bettermatrix_callmedia_CallMediaNative_detach(
    _env: JNIEnv,
    _class: JClass,
    token: jlong,
) {
    video::detach(token as u64);
}

/// `CallMediaNative.cameraFrame(…)`: one camera frame as libwebrtc's `VideoFrame.I420Buffer`
/// gives it (direct buffers), `rotation` degrees clockwise to stand upright.
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_bettermatrix_callmedia_CallMediaNative_cameraFrame(
    env: JNIEnv,
    _class: JClass,
    width: jint,
    height: jint,
    rotation: jint,
    y: JByteBuffer,
    stride_y: jint,
    u: JByteBuffer,
    stride_u: jint,
    v: JByteBuffer,
    stride_v: jint,
) -> jboolean {
    let plane = |buffer: &JByteBuffer| -> Option<&[u8]> {
        let address = env.get_direct_buffer_address(buffer).ok()?;
        let length = env.get_direct_buffer_capacity(buffer).ok()?;
        // SAFETY: a direct buffer's memory, alive for this call.
        Some(unsafe { std::slice::from_raw_parts(address, length) })
    };
    let (Some(y), Some(u), Some(v)) = (plane(&y), plane(&u), plane(&v)) else { return JNI_FALSE };
    let planes = CameraPlanes::I420 {
        y,
        stride_y: stride_y.max(0) as u32,
        u,
        stride_u: stride_u.max(0) as u32,
        v,
        stride_v: stride_v.max(0) as u32,
    };
    let frame = CameraFrame {
        width: width.max(0) as u32,
        height: height.max(0) as u32,
        rotation: rotation.rem_euclid(360) as u32,
        planes,
    };
    if push_camera_frame(frame) { JNI_TRUE } else { JNI_FALSE }
}

/// `CallMediaNative.cameraLost()`: the camera stopped on its own.
#[unsafe(no_mangle)]
pub extern "system" fn Java_chat_bettermatrix_callmedia_CallMediaNative_cameraLost(_env: JNIEnv, _class: JClass) {
    camera_lost();
}

// ----- ANativeWindow (libandroid) ------------------------------------------------------------

#[repr(C)]
struct ANativeWindow {
    _private: [u8; 0],
}

#[repr(C)]
struct ANativeWindowBuffer {
    width: i32,
    height: i32,
    /// Pixels per row.
    stride: i32,
    format: i32,
    bits: *mut c_void,
    reserved: [u32; 6],
}

/// `AHARDWAREBUFFER_FORMAT_R8G8B8A8_UNORM`: R, G, B, A bytes in memory.
const RGBA_8888: i32 = 1;

#[link(name = "android")]
unsafe extern "C" {
    fn ANativeWindow_fromSurface(env: *mut c_void, surface: *mut c_void) -> *mut ANativeWindow;
    fn ANativeWindow_release(window: *mut ANativeWindow);
    fn ANativeWindow_setBuffersGeometry(window: *mut ANativeWindow, width: i32, height: i32, format: i32) -> i32;
    fn ANativeWindow_lock(window: *mut ANativeWindow, buffer: *mut ANativeWindowBuffer, dirty: *mut c_void) -> i32;
    fn ANativeWindow_unlockAndPost(window: *mut ANativeWindow) -> i32;
}

/// A view's surface, drawn with the CPU.
struct Window {
    window: *mut ANativeWindow,
    /// The buffers' size, set when a frame of another size comes.
    size: (u32, u32),
}

// An ANativeWindow may be drawn from any thread; the registry draws one frame at a time.
unsafe impl Send for Window {}

impl Target for Window {
    fn draw(&mut self, width: u32, height: u32, rgba: &[u8]) {
        if rgba.len() < (width * height * 4) as usize {
            return;
        }
        // SAFETY: `window` is held (released only on drop); the locked buffer is
        // `stride × height` pixels of four bytes, written only between lock and post.
        unsafe {
            if self.size != (width, height) {
                if ANativeWindow_setBuffersGeometry(self.window, width as i32, height as i32, RGBA_8888) != 0 {
                    return;
                }
                self.size = (width, height);
            }
            let mut buffer = ANativeWindowBuffer {
                width: 0,
                height: 0,
                stride: 0,
                format: 0,
                bits: std::ptr::null_mut(),
                reserved: [0; 6],
            };
            if ANativeWindow_lock(self.window, &mut buffer, std::ptr::null_mut()) != 0 {
                return;
            }
            if !buffer.bits.is_null() && buffer.width as u32 == width && buffer.height as u32 == height {
                let row = width as usize * 4;
                let stride = buffer.stride.max(0) as usize * 4;
                let bits = buffer.bits.cast::<u8>();
                for line in 0..height as usize {
                    std::ptr::copy_nonoverlapping(rgba.as_ptr().add(line * row), bits.add(line * stride), row);
                }
            }
            ANativeWindow_unlockAndPost(self.window);
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the reference `ANativeWindow_fromSurface` took.
        unsafe { ANativeWindow_release(self.window) };
    }
}
