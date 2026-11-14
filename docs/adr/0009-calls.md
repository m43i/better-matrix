# 0009. Calls

Status: Accepted
Date: 2026-11-14

## Context

- Matrix has two call standards. MatrixRTC (MSC4143, MSC4195) runs group and one-to-one calls
  in any room through a LiveKit SFU that the homeserver names; Element Call and Element X speak
  it. The older one-to-one VoIP (`m.call.*`, MSC2746) connects two devices directly over WebRTC;
  Element Web and some other clients still use it.
- Rune has no web view, so Element Call can't be embedded as a widget, and Rune records and plays
  audio only on some platforms. Calls need their own media engine.
- LiveKit's Rust SDK wraps Google's libwebrtc: microphone and speakers through WebRTC's audio
  device module (with echo cancellation) on every desktop platform, frame encryption,
  screen capture (the desktop portal and PipeWire on Wayland), and a peer connection factory
  the one-to-one calls can share. It has no camera capture.

## Decision

- **Media engine:** `livekit` (default features off; `tokio`, rustls, `glib-main-loop` for
  Wayland screen sharing) in `matrix-core`. Its build downloads a prebuilt libwebrtc. Cameras
  come from `nokhwa`. LiveKit's TLS and the SDK's ask rustls for different crypto providers, so
  `Engine::start` installs one (aws-lc-rs, the SDK's) before anything connects; without it rustls
  won't choose and the first call's connection panics. No other part of the app touches either: the desktop sees calls only as
  `Command`s and `Event`s, and video only through a `VideoSink` it hands the engine.
- **One call at a time,** across accounts, run by one engine task (`calls::task`). Starting another
  hangs up the first and waits (up to 10 s) for it to leave before setting up, so the two never hold
  the devices or the membership at once; a call just hung up counts too. The engine keeps every
  call hung up and still leaving as a shared teardown, with its account and room
  (`calls::Leaving`), until it finishes: the next call and quitting wait for all of them, leaving
  a room for that room's, signing out for that account's, and none takes one from the others.
  Each call has a number (`CallInfo::call`, `Event::CallEnded::call`); the desktop ignores a
  replaced call's late reports. Leaving a room (or being removed from it, which sync reports) ends
  its call and its ring first; leaving waits (up to 3 s) for the call to leave. Signing an account
  out ends its call and its rings first, and waits (up to 3 s) for the call to leave while the token
  still works. Quitting hangs up and waits up to 3 s for the call to leave (`EngineStopped::wait`).
  Calling a person goes through the direct chat with them alone: never a group chat they're in, nor
  one a third person joined, which would ring them too.
- **The call task's shape:** setting up races a hang-up, and everything the call holds sits in
  one owner (`session::Session`) that closes it on every path: each LiveKit connection is
  closed explicitly (a dropped one stays connected), devices stop, the membership leaves.
  Network work (connecting to services, sending keys, keeping the membership, looking up
  names, one-to-one candidates) runs as tasks beside the call's loop and reports back. What
  must happen in order (publishing a camera or screen, switching cameras, answering a
  one-to-one call) runs inside the loop but races a hang-up, and device work (opening and
  closing a camera, a screen capture) runs on blocking threads. A connection that finishes as
  the call ends is closed with the rest. The call reports itself after every change, and not
  when nothing changed. Its end is reported as soon as its loop ends, before leaving, so the
  app closes the call at once; leaving then takes at most a few seconds (each request 5 s, not
  retried: offline, the delayed leave goes out on its own). Timers that fall behind (a slow
  publish) catch up with one tick; candidates that failed to send wait a second. Each step of
  joining (the token, the connection, publishing the microphone, the camera, the membership,
  the ring) has a time limit, so a stall ends the call naming the step
  (`CallError::Stalled`) rather than leaving it connecting; a panic in the call task ends the
  call with an error too. The steps log through `tracing`, which the desktop prints when
  `RUST_LOG` is set (`RUST_LOG=matrix_core=debug,livekit=debug`).
- **Which standard:** MatrixRTC where the account's homeserver names a LiveKit transport
  (`Client::rtc_transports`, cached per account) and the user may send call memberships in the
  room (power level for `org.matrix.msc3401.call.member`). Otherwise the one-to-one standard in
  a direct chat with exactly one other member. Otherwise no call: the room header hides the
  call buttons (`RoomSummary::call`).
- **MatrixRTC session** (`calls::rtc`, `calls::session`), interoperable with Element Call in
  its default "compatibility" mode (checked against matrix-js-sdk's `matrixrtc` and Element
  Call's sources):
  1. Multi-SFU: the user publishes on their own homeserver's LiveKit service and subscribes to
     every service another member publishes on (each multi-SFU member's first preferred
     focus; the oldest member's for clients that join wherever it is), one LiveKit
     connection each (`rtc::subscriptions`). People are followed across connections by
     identity, so one who subscribes on one SFU and publishes on another shows once. Only
     identities that belong to a member (or name a user, `@user:server:DEVICE`) show. A
     service that drops loses its people and videos and is reconnected every 5 s while
     someone publishes there; the user's own dropping ends the call. Connections take the
     whole video (`adaptive_stream` off: the app asks for no sizes).
  2. Token: an OpenID token from the homeserver, exchanged at each service (`/sfu/get`,
     falling back to `/get_token`) for the SFU URL and a JWT; another homeserver's service
     only lets the user subscribe.
  3. Membership: Element's session-format state event `org.matrix.msc3401.call.member` under
     `_{user}_{device}_m.call`, written as raw JSON in Element's shape (`membershipID`
     `user:device`, `focus_active` `multi_sfu`, the user's service in `foci_preferred`,
     `m.call.intent`). Memberships are read leniently from raw JSON (both event types, the
     oldest list format), so one in an unknown shape (or with times that overflow) costs only
     that member. A delayed empty
     event (MSC4140, 18 s, restarted every 4 s) leaves the call if the app dies. A refused
     restart, or the user's membership vanishing after it was seen, sends the membership
     again with a new delayed leave; so does every hour, well inside its 4-hour expiry. A
     renewal keeps when the user joined (`created_ts`, with `expires` counted from it), so the
     call's oldest member doesn't change.
  4. Encryption in encrypted rooms: a random 16-byte key per device, sent to each member
     device in an encrypted to-device `io.element.call.encryption_keys` carrying `media_key`
     and `member_id` and, for older readers, `keys` and `member.id`; read back from either.
     Older clients' room-event keys are read too. LiveKit's key provider: HKDF, its default
     salt, ratchet window 10, key ring 256 (as Element Call's `MatrixKeyProvider`). A member
     leaving rotates the key (at most every 10 s); the new key is sent at once and used a
     second later on every outgoing track (`FrameCryptor::set_key_index`), so the others have
     it first. Newcomers get the key in use and any on its way; devices a send didn't reach
     get it again every 5 s. Keys arrive only Olm-encrypted from the device they name (a
     to-device key in the clear is dropped), and only for the call's room (older clients'
     room-event keys take the room they came in, and count only when they came encrypted).
  5. LiveKit identities, for matching keys: `user:device`, SHA-256 of `[user, member]` (newer
     memberships) and of `[user, device, member]` (the newer endpoint), unpadded base64;
     a member without an ID has `user:device`.
  6. Not yet: Element's "matrix_2_0" mode (sticky `m.rtc.member` events and transports
     authenticated over federation).
- **One-to-one standard** (`calls::direct`): MSC2746 version 1 (invite, answer, candidates,
  select answer, hangup, reject) over a libwebrtc peer connection from LiveKit's runtime, with
  the homeserver's TURN servers. Media is WebRTC's own DTLS-SRTP; the signalling is encrypted
  like any event in the room. The connection is negotiated once: its camera pauses and
  resumes but can't be added later, and it shares no screen (the stage hides that control).
  Its signals reach it only from its own account and room, and count only from the other
  person (once their device answered, only that device) or, for a pickup elsewhere, the user's
  own other devices (`task::hear`). While it rings, only the caller's ringing device adds
  candidates or hangs up, and only the user's own other devices answer or decline it elsewhere
  (`task::hear_ringing`). Opening the camera at the start races a hang-up too. A hangup goes
  out only for a call the other side knows of (it called, or the invite went out). The TURN
  lookup is short and unretried: without an answer, the call goes without TURN. A voice call's
  offer asks for no video, and an invite counts as a video call only when the caller sends
  video (a video section that only receives is a voice call).
- **Ringing:** MatrixRTC ring notifications (`m.rtc.notification`) and one-to-one invites raise
  `Event::IncomingCall`; the desktop shows a card with answer and decline, a notification
  while the window is in the background, and rings a soft chime on the picked speaker
  (`platform::sound::Ring`) unless the user made rings silent. A ring expires after its
  lifetime, at most 2 minutes; each ring is numbered so a lapse ends only its own, and its card
  shows (after the caller's name is looked up) only if that ring still rings; its lapse is timed
  from when it came, not from the lookup. A MatrixRTC ring
  sounds only on an account that has a call service to join with, while the room's session
  still has someone in it; it stops when the session empties or the user joins it from another
  device (one of their memberships besides this device's own). A one-to-one invite naming no invitee
  rings only in a direct chat. A one-to-one invite's
  candidates that arrive while it rings are kept and used when answering.
- **Devices:** `Command::ListCallDevices` lists microphones and speakers (the audio device
  module) and cameras (nokhwa) by name; `Command::ChooseCallDevice` picks one by name for the
  next calls and switches the one under way (a camera switch feeds the same track, so nothing
  renegotiates). No pick, or a picked device that's gone, means the system's default (the
  device module's first). A pick switches only that device (a switch restarts it). Cameras are
  listed once by name, the first node of each (Linux webcams have a metadata node by the same
  name). A screen share whose system picker is cancelled just goes, without a notice. Hanging
  up stops the microphone, speaker and cameras before (or while) the leave goes out. The call
  controls have a chevron beside the microphone (microphones and
  speakers) and the camera.
- **Media:** the microphone through `PlatformAudio`; remote audio plays through the same
  device module. Cameras: `nokhwa` frames converted to I420. Screens: libwebrtc's desktop
  capturer (the system picker on Linux and macOS). Remote and local video arrive as RGBA through
  `VideoSink`s the desktop asks for by `VideoId` (`Command::WatchVideo`). A camera or share
  that stops on its own (unplugged, taken by another app, stopped from the system) is reported
  by its capture thread (a camera's reads failing for a second; a single failed read is
  ignored); the call stops sending it and says so (`Event::CallMediaStopped`). Each capture is
  numbered, so the end of one replaced by a camera switch is ignored, and a switch that fails
  turns the camera off rather than freezing it. Dropping a capture never waits; releasing one
  waits on a blocking thread until its device is free. A one-to-one video call carries a camera
  track from the start (a camera that won't start leaves it off, with a notice): turning the
  camera off releases the device, turning it on starts it again, and a camera picked while
  it's off is only remembered. Each frame
  is handed over by value; the user's own previews are scaled to 640 wide while still I420. The
  desktop
  watches only while the stage shows (`State::sync_watching`), so a call in another room or
  behind its chat converts no frames.
- **Audio levels** come every 100 ms (`Event::CallLevels`, 0 to 255) for the background glow and
  the speaking rings. Someone stays speaking for 400 ms after going quiet, so rings don't
  flicker between words. The view rebuilds only when who speaks changes; otherwise the event
  is `live_only`.
- **Desktop:**
  - Call and video call buttons sit in the room header before the people button; a room
    with a call under way shows Join instead.
  - The open call's room shows the stage instead of its timeline: profile pictures for a
    voice call, a grid of video tiles for a video call (a shared screen takes the largest
    tile, with the cameras in a strip below), on an animated glow that wanders and swells with
    the participants' voices (`state::call_glow`, drawn into `Pixels` like the voice memo's
    water; still under reduced motion or in an unfocused window). Each video streams into its
    own `Pixels` from the engine's media threads, so frames rebuild nothing.
  - The controls: microphone, camera, screen share, show chat, hang up. Showing the chat puts
    the timeline back and a "Back to the call" button in the header.
  - Elsewhere in the app (or while the chat shows) a call bar at the bottom of the sidebar
    leads back to it, with mute and hang up.
  - Rings show as cards at the top right (`ui::call::rings`), each one line: the caller (a long
    name cut short) and the controls; a notice stacks above them rather than under. Scenes:
    `demo-call-voice`,
    `demo-call-video`, `demo-call-ringing`, `demo-call-chat`.

## Consequences

- libwebrtc needs link settings in the app's own binary (`.cargo/config.toml`: `-ObjC` on macOS,
  the static C runtime on Windows; clang 21 and lld on Linux): see ADR 0001.
- The build downloads libwebrtc (once per target) and needs a C++ compiler; Linux needs glib
  and PipeWire headers. The binary grows by tens of megabytes.
- Interoperability rests on Element Call's conventions (key event format, identities, key
  derivation), which are partly unstable MSCs. Test changes against a real Element Call.
- Calls can't run headless: tests cover the protocol pieces (contents, identities, focus
  choice, support rules) and the desktop's call state and views from injected events.
