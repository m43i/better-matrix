//! Voice and video calls. Two standards (see ADR 0009): MatrixRTC sessions through the
//! homeserver's LiveKit service ([`rtc`]), and the older one-to-one calls between two devices
//! ([`direct`]). One call runs at a time, as one task ([`task`]); the app steers it with
//! [`Command`](crate::Command)s and hears it through [`Event`](crate::Event)s. Video reaches the
//! app only through the [`VideoSink`]s it hands over, as RGBA frames.

mod direct;
mod keys;
mod media;
mod platform;
mod rtc;
mod session;
mod signals;
mod sticky;
mod support;
mod task;

use std::fmt;
use std::sync::Arc;

use crate::model::{AccountId, Person};

pub(crate) use direct::{has_video, reject};
pub(crate) use media::list_devices;
pub use platform::{CameraFrame, CameraPlanes, PlatformCamera, camera_lost, push_camera_frame, use_platform_camera};
pub(crate) use rtc::{Member, joined_sticky};
pub(crate) use signals::{DirectEvent, Signal, listen};
pub(crate) use sticky::StickyMembers;
pub use support::via;
pub(crate) use task::{CallHandle, Control, Invite, Leaving, RingHeard, Start, hear_ringing, person, start};

/// Which standard a call speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum CallVia {
    /// A MatrixRTC session through the homeserver's LiveKit service: any number of people.
    Rtc,
    /// The older one-to-one standard, device to device, in a direct chat.
    Direct,
}

/// A room's calls, as its summary carries them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomCall {
    /// How a call would start here; `None` when the server or the room allows none, which hides
    /// the call buttons.
    pub via: Option<CallVia>,
    /// Who is in the room's call now, by user ID; empty when no call is under way.
    pub participants: Vec<String>,
}

/// A video track the app can watch: the user's camera or screen, or someone else's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct VideoId(pub u64);

/// Takes one video frame: its width, height and straight-alpha RGBA pixels (sRGB, rows packed).
pub type FrameFn = dyn Fn(u32, u32, Vec<u8>) + Send + Sync;

/// Where a video's frames go: called on a media thread with each frame.
#[derive(Clone)]
pub struct VideoSink(pub Arc<FrameFn>);

impl PartialEq for VideoSink {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for VideoSink {}

impl fmt::Debug for VideoSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VideoSink")
    }
}

/// Turns one of the user's own media on or off during a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum MediaToggle {
    /// The microphone: `false` mutes.
    Microphone(bool),
    /// The camera.
    Camera(bool),
    /// Sharing a screen or window (the system picker chooses which, where it has one).
    Screen(bool),
}

/// A kind of media device a call uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum DeviceKind {
    /// What the user speaks into.
    Microphone,
    /// What everyone else plays on.
    Speaker,
    /// What films the user.
    Camera,
}

/// A microphone, speaker or camera.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct MediaDevice {
    /// Its name, which also identifies it: the app's own sound finds the same device by it.
    pub name: String,
}

/// The devices calls can use now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct CallDevices {
    /// Microphones.
    pub microphones: Vec<MediaDevice>,
    /// Speakers and headphones.
    pub speakers: Vec<MediaDevice>,
    /// Cameras.
    pub cameras: Vec<MediaDevice>,
}

/// The devices the user picked, by name; `None` (or a device that's gone) follows the system's
/// default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceChoice {
    /// The microphone.
    pub microphone: Option<String>,
    /// The speaker.
    pub speaker: Option<String>,
    /// The camera.
    pub camera: Option<String>,
}

impl DeviceChoice {
    /// The device picked for `kind`.
    pub fn get(&self, kind: DeviceKind) -> Option<&str> {
        match kind {
            DeviceKind::Microphone => self.microphone.as_deref(),
            DeviceKind::Speaker => self.speaker.as_deref(),
            DeviceKind::Camera => self.camera.as_deref(),
        }
    }

    /// Picks `id` for `kind`.
    pub fn set(&mut self, kind: DeviceKind, id: Option<String>) {
        match kind {
            DeviceKind::Microphone => self.microphone = id,
            DeviceKind::Speaker => self.speaker = id,
            DeviceKind::Camera => self.camera = id,
        }
    }
}

/// Where a call stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum CallPhase {
    /// A one-to-one call the other side hasn't answered yet.
    Calling,
    /// Joining: tokens, the SFU, keys.
    Connecting,
    /// In the call.
    Connected,
    /// The connection dropped and is coming back.
    Reconnecting,
}

/// One person in a call (other than the user).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct CallParticipant {
    /// Stable for the call: their LiveKit identity, or their user ID in a one-to-one call.
    pub key: String,
    /// Who they are, as the room knows them.
    pub person: Person,
    /// Whether their microphone is off.
    pub muted: bool,
    /// Their camera's video, while it is on.
    pub camera: Option<VideoId>,
    /// Their shared screen, while they share one.
    pub screen: Option<VideoId>,
}

/// The call under way, as the app shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct CallInfo {
    /// Which call this is: unique in the process, so a call ending can't be taken for the one
    /// that replaced it in the same room.
    pub call: u64,
    /// Whose call.
    pub account: AccountId,
    /// Its room.
    pub room_id: String,
    /// The standard it speaks.
    pub via: CallVia,
    /// Where it stands.
    pub phase: CallPhase,
    /// Started as a video call: the stage shows the video grid rather than faces.
    pub video: bool,
    /// The user's microphone is on.
    pub microphone: bool,
    /// The user's camera, while it is on, for its preview.
    pub camera: Option<VideoId>,
    /// The user's shared screen, while it is shared, for its preview.
    pub screen: Option<VideoId>,
    /// Everyone else in the call.
    pub participants: Vec<CallParticipant>,
}

/// Why a call ended.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum CallEnd {
    /// The user hung up.
    HungUp,
    /// The other side hung up, or everyone else left.
    Left,
    /// The other side declined.
    Declined,
    /// Nobody answered in time.
    NoAnswer,
    /// It was answered on another of the user's devices.
    AnsweredElsewhere,
    /// Something failed: the reason, for the user.
    Failed(String),
}

/// A call ringing for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct IncomingCall {
    /// Whose account it rings on.
    pub account: AccountId,
    /// Its room.
    pub room_id: String,
    /// Who calls.
    pub caller: Person,
    /// Whether they started it with video.
    pub video: bool,
    /// Which standard answering it takes.
    pub via: CallVia,
}

/// What can go wrong in a call.
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// The homeserver names no call service, or the room allows no call.
    #[error("calls aren't available in this room")]
    Unavailable,
    /// The call service refused or couldn't be reached.
    #[error("the call service failed: {0}")]
    Service(String),
    /// The media engine failed.
    #[error("the call's media failed: {0}")]
    Media(String),
    /// A step of joining didn't finish in time.
    #[error("joining the call stalled while {0}")]
    Stalled(&'static str),
    /// No call is under way, or none rings in that room.
    #[error("there is no such call")]
    NoCall,
}
