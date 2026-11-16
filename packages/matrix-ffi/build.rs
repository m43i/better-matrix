//! On Android, keeps libwebrtc's JNI entry points (`Java_livekit_org_webrtc_*`) in the library
//! and exported: libwebrtc's Java half (the audio device module, the codec factories, the camera
//! capturer) calls them by name. `webrtc-sys` asks for this in its own build script, but a
//! dependency's link arguments never reach the final library, so the library asks again.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }
    if let Err(error) = webrtc_sys_build::configure_jni_symbols() {
        println!("cargo::error=libwebrtc's JNI symbols couldn't be kept: {error:#}");
    }
}
