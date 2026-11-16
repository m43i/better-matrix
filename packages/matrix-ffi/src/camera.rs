//! The phone's camera for calls. `nokhwa` has no phone backend, so the app films with the
//! system's camera API: it registers a [`NativeCamera`] once, the engine starts, stops and
//! switches it like any camera, and the app's native code pushes each frame straight into the
//! engine (Android: [`crate::android`], iOS: [`crate::native`]), never through JavaScript.

use std::sync::Arc;

use matrix_core::calls::PlatformCamera;

/// A camera call the platform couldn't carry out.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CameraFailure {
    /// The camera didn't start: no permission, no camera, or it's taken.
    #[error("{reason}")]
    Failed {
        /// Why, for the user.
        reason: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for CameraFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { reason: error.reason }
    }
}

/// The system's camera API, implemented by the app. Calls come from the engine's blocking
/// threads and wait for the answer.
#[uniffi::export(with_foreign)]
pub trait NativeCamera: Send + Sync {
    /// The cameras there are, by name, the default first.
    fn cameras(&self) -> Vec<String>;
    /// Starts filming with the camera named `device` (the default when `None` or gone). Frames
    /// follow through the native frame entry until `stop`.
    fn start(&self, device: Option<String>) -> Result<(), CameraFailure>;
    /// Stops filming and lets go of the camera.
    fn stop(&self);
}

/// Films the cameras of every later call with `camera`.
#[uniffi::export]
pub fn register_native_camera(camera: Arc<dyn NativeCamera>) {
    matrix_core::calls::use_platform_camera(Arc::new(Platform(camera)));
}

struct Platform(Arc<dyn NativeCamera>);

impl PlatformCamera for Platform {
    fn cameras(&self) -> Vec<String> {
        self.0.cameras()
    }

    fn start(&self, device: Option<String>) -> Result<(), String> {
        self.0.start(device).map_err(|error| error.to_string())
    }

    fn stop(&self) {
        self.0.stop();
    }
}
