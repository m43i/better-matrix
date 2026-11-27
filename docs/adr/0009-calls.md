# 0009. Calls

Status: Accepted
Date: 2026-11-16

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

- **Media engine:** `livekit` (default features off; `tokio`, rustls, `glib-main-loop` for Wayland
  screen sharing) in `matrix-core`. Its build downloads a prebuilt libwebrtc. Cameras come from
  `nokhwa`. LiveKit's TLS and the SDK's ask rustls for different crypto providers, so
  `Engine::start` installs one (aws-lc-rs, the SDK's) before anything connects; without it rustls
  won't choose and the first call's connection panics. No other part of the app touches either: the
  desktop sees calls only as `Command`s and `Event`s, and video only through a `VideoSink` it hands
  the engine.
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
- **The call task's shape:** setting up races a hang-up, and everything the call holds sits in one
  owner (`session::Session`) that closes it on every path: each LiveKit connection is closed
  explicitly (a dropped one stays connected), devices stop, the membership leaves. Network work
  (connecting to services, sending keys, keeping the membership, looking up names, one-to-one
  candidates) runs as tasks beside the call's loop and reports back. What must happen in order
  (publishing a camera or screen, switching cameras, answering a one-to-one call) runs inside the
  loop but races a hang-up, and device work (opening and closing a camera, a screen capture) runs on
  blocking threads. A connection that finishes as the call ends is closed with the rest. The call
  reports itself after every change, and not when nothing changed. Its end is reported as soon as
  its loop ends, before leaving, so the app closes the call at once; leaving then takes at most a
  few seconds (each request 5 s, not retried: offline, the delayed leave goes out on its own).
  Timers that fall behind (a slow publish) catch up with one tick; candidates that failed to send
  wait a second. Each step of joining (the token, the connection, publishing the microphone, the
  camera, the membership, the ring) has a time limit, so a stall ends the call naming the step
  (`CallError::Stalled`) rather than leaving it connecting; a panic in the call task ends the call
  with an error too, and a MatrixRTC session still closes what it holds after one: the loop's work
  stops and a connection that came in meanwhile closes (`Live::finish`), then the session's
  connections, devices and membership (`Session::close`). The steps log through `tracing`, which the
  desktop prints when `RUST_LOG` is set (`RUST_LOG=matrix_core=debug,livekit=debug`).
- **Which standard:** MatrixRTC where the account's homeserver names a LiveKit transport
  (`Client::rtc_transports`, cached per account) and the user may send call memberships in the
  room (power level for `org.matrix.msc3401.call.member`). Otherwise the one-to-one standard in
  a direct chat with exactly one other member, where the homeserver names TURN servers
  (`voip/turnServer` with URIs, looked up once per account beside the LiveKit lookup,
  `Account::call_services`): without a relay two devices behind different routers rarely get
  through. A refusal, or Synapse's empty answer, names none; the network or the server failing
  for a while (429, 5xx, a token refresh that failed) is asked again, waiting longer each time. Each lookup reports as soon
  as it answers whether the account can call at all (`Event::CallsOffered`), so a person's
  profile and member menu offer a call (through a new direct chat) only then. Otherwise no call: the room header hides the call buttons (`RoomSummary::call`) and its
  video button offers the room's meetings instead (ADR 0012). A one-to-one invite still rings
  on an account without TURN: the caller's relay is enough.
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
  6. Element's "matrix_2_0" mode (`calls::sticky`): its members write a sticky
     `org.matrix.msc4143.rtc.member` (or `m.rtc.member`) event (MSC4354, at most an hour, sent
     again before it lapses) under a random member ID, rather than state. Every Element Call
     reads both kinds, so the app keeps writing its state membership and reads sticky ones too:
     - The SDK keeps no sticky events, so each account does (`StickyMembers`, living as long as
       the account), per room, keyed by type, sender and sticky key, the current one the one
       sticky longest by its sender's clock (sent plus duration, then the highest event ID; so
       a snapshot that arrives late can't undo a leave live sync brought), lapsing by the
       server's remaining time. Only events sent sticky count, only for the
       room-wide call's slot (`m.call#ROOM`), and in an encrypted room only ones that came
       encrypted (anyone who may send could fake one in the clear). They come from the
       timeline as they are sent (the call handlers see them decrypted), and, once per room
       where the server lists `org.matrix.msc4354`, from before: a sync without a token for that
       room alone that marks no one present (`StickyMembers::fetch`; its token is never used,
       so its to-device messages stay undelivered). It runs when a room's timeline first opens
       and when a call starts; a call waits for one under way; one that failed, or met events
       this device can't decrypt yet, runs again next time, and after the sync reconnects every
       room's runs again: the rooms on screen (a call's among them) at once, tried again until the
       connection answers, others when opened
       (memberships sent meanwhile may have come only in the failed sync). A leave (or a content without a
       member) replaces the join, a redaction removes it, and members of rooms they left don't
       count, and a snapshot skips their events (a call asks for one lazy loading left out; a room's summary counts only those
       the store knows as joined), and such a member leaving the room (or being removed) takes
       their memberships with them, which the call, rings and summaries hear at once. The store itself reports every change,
       and each membership's lapse, as `Signal::Members` (`StickyMembers::report_to`).
     - Known gaps: a sticky event that arrives only outside the timeline window between two
       snapshots (MSC4354's sync section, which the SDK drops) is seen at the member's next
       renewal or the next snapshot; a sticky membership the SDK decrypts only later (its key
       came after it) isn't replayed (call signals are, see below); MSC4195 grants use the unstable route, the only one
       ruma 0.17 and today's deployments have.
     - A sticky member counts wherever a state member does: the call's members
       (`rtc::members`), the room's call participants in its summary, whether a ring's session
       still has someone, and whether the user joined from another device. A change reports
       `Signal::Members` and reads the room's summary again.
     - Its member ID is the UUID: keys name it (`member_id`), and the identities already
       include SHA-256 of `[user, member]`, the one a homeserver-vouched SFU gives.
     - Transports (`rtc::Transport`): a `livekit_service_url` is a JWT service, asked as before;
       a LiveKit transport with only `url` is an SFU its homeserver (`server_name`, else the
       publisher's) vouches for (MSC4195): the user's own homeserver asks it over federation
       (`rtc/livekit/get_token`, our member ID, slot `m.call#ROOM`), and the SFU is the URL. The
       user still publishes on their own homeserver's JWT service.
     - Not done: writing sticky memberships (matrix-sdk 0.19 can't send an encrypted sticky
       event) and publishing on a homeserver that names only MSC4195 transports.
- **One-to-one standard** (`calls::direct`): MSC2746 version 1 (invite, answer, candidates,
  select answer, hangup, reject) over a libwebrtc peer connection from LiveKit's runtime, with
  the homeserver's TURN servers. Media is WebRTC's own DTLS-SRTP; the signalling is encrypted
  like any event in the room. The connection is negotiated once: its camera pauses and
  resumes but can't be added later, and it shares no screen (the stage hides that control).
  Its signals reach it only from its own account and room, and count only from the other
  person (once their device answered, only that device) or, for a pickup elsewhere, the user's
  own other devices (`task::hear`). While it rings, only the caller's ringing device adds
  candidates or hangs up, and only the user's own other devices answer or decline it elsewhere
  (`task::hear_ringing`). Opening the camera at the start races a hang-up too, and has 15 s: a camera that hangs
  (a stuck driver, a prompt left open) leaves the call going with the camera off. Turning it on
  or switching it later has the same 15 s, after which the camera is let go for the call. A hangup goes
  out only for a call the other side knows of (it called, or the invite went out). The TURN
  lookup is short and unretried: without an answer, the call goes without TURN. Candidates are
  gathered once, as browsers do (LiveKit's default gathers continually, for its SFU, which keeps
  a connection that can't get through checking forever, never failing). Once answered (the
  caller then shows connecting), the devices have 30 s to get through (setting up, with the
  invite or answer sent, has 20 s; sending the caller's pick of an answer, 10 s), else the call ends as
  stalled (`CallError::Stalled`); a connection that drops shows reconnecting until it comes
  back or fails. A voice call's
  offer asks for no video, and an invite counts as a video call only when the caller sends
  video (a video section that only receives is a voice call).
- **Signals in order, after their keys:** call events (and fresh events the SDK couldn't decrypt
  yet, up to two minutes old) go through one queue per account, read beside the sync by one
  reader per room (`signals::read_in_order`), all aborted with the account. One still encrypted
  waits for its key until 10 s after it arrived (decrypting again every 250 ms), and the events after it wait
  their turn, so an invite still rings and its candidates and answer don't overtake it; waits in
  a row don't add up, and another room's wait holds up nothing here; the last try comes at the
  deadline itself. A MatrixRTC ring heard late doesn't ring once the user joined the call on
  another device. An invite heard after the user left its room doesn't ring. Under sliding sync the room's events and its room
  keys come through different connections (the to-device long poll), and a call's first events
  are usually the first of a new Megolm session, so they often arrive before their key; dropped
  as before, a one-to-one call never rang or never connected.
- **Telling the room:** the first one into a call sends `m.rtc.notification` (the unstable type,
  as Element Call does): `ring` in a direct chat, `notification` in a group, so every call has its
  card in the timeline and group calls don't ring.
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
  module) and cameras (nokhwa, or the platform camera) by name; `Command::ChooseCallDevice`
  picks one by name for the next calls and switches the one under way (a camera switch feeds the same track, so nothing
  renegotiates). No pick, or a picked device that's gone, means the system's default (the
  device module's first). A pick switches only that device (a switch restarts it). Cameras are
  listed once by name, the first node of each (Linux webcams have a metadata node by the same
  name). A screen share whose system picker is cancelled just goes, without a notice. Hanging
  up stops the microphone, speaker and cameras before (or while) the leave goes out. The call
  controls have a chevron beside the microphone (microphones and
  speakers) and the camera.
- **Media:** the microphone through `PlatformAudio`; remote audio plays through the same
  device module. Cameras: `nokhwa` frames converted to I420. Where the app registers a
  **platform camera** (`calls::use_platform_camera`: phones, where nokhwa has no backend), every
  camera comes from it instead: the engine starts, stops and switches it through
  `PlatformCamera` from its blocking threads, and the app's native code pushes each frame
  (`calls::push_camera_frame`, I420 or NV12 with the rotation to stand upright) into the running
  capture, which rotates it upright and feeds the track and the preview; a camera the system
  takes away ends through `calls::camera_lost`. Stopping a platform capture on drop happens on a
  thread of its own, so dropping still never waits. The desktop registers none. Screens: libwebrtc's desktop
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
  watches only the videos in the current call plan (`State::sync_watching`): hidden gallery
  pages, a hidden self-view and calls in another room convert no frames. Removing a sink stops
  conversion and upload, not SFU reception or WebRTC decode. Adaptive SFU subscriptions and
  requested receive sizes remain separate media-engine work. The UI measures `Pixels::size`
  between frames and replans only when the source aspect changes; frames still stream directly
  to `Pixels`, without rebuilding the view.
- **Audio levels** come every 100 ms (`Event::CallLevels`, 0 to 255) for the background glow and
  the speaking rings. Someone stays speaking for 400 ms after going quiet, so rings don't
  flicker between words. Speaker focus has a separate state machine: a remote participant must
  remain the loudest unmuted speaker for 1.5 seconds to take focus, using the same 400 ms
  speaking hold across pauses between words. Silence keeps the last focus;
  brief interjections do not switch it. The view rebuilds when speaking rings or speaker focus
  change; otherwise the event is `live_only`.
- **Desktop:**
  - Call and video call buttons sit in the room header before the people button; a room
    with a call under way shows Join instead.
  - **Call layout:** a pure desktop planner (`state::call_layout`) takes the tile viewport,
    tile identities and source aspects, layout choice, speaker, page and strip proportion. Its
    rectangles both place the tiles and select video watches. Grid candidates maximize equally
    sized 16:9 cells in the available rectangle, centre incomplete rows, and page before cells
    drop below 160×90 points (one tile remains possible in smaller viewports). At most 25 tiles
    occupy a gallery page. Voice calls use compact avatars and speaking rings, with a smaller
    104×80-point minimum so several faces fit above chat.
  - The call stage's top-right layout icon opens a dropdown for Auto, Grid and Focus, with
    the current choice checked. This control appears only on the call stage, not other rooms.
  - **Auto, Grid and Focus:** Auto uses a gallery for groups, a large remote view and small
    self-view for one-to-one calls, and a large shared screen when content is shared. Grid gives
    every surface equal space, including simultaneous shares for comparison. Focus follows the
    stabilized speaker; selecting a tile pins that participant's camera or screen locally.
    Tile identity uses the participant key and surface kind, never `VideoId`, so a camera restart
    preserves the pin. A departed participant or ended pinned share returns to Auto.
  - **One-to-one self-view:** it floats in a corner of the remote video (not the viewport), in
    its own camera's shape (`SelfView`, placed by the planner). Dragging it follows the pointer
    over the video; let go, it snaps to the corner nearest where it was dropped (a cancelled
    drag puts it back). Dragging its inner corner (the one facing the video's middle, marked only
    by the resize cursor) resizes it along that corner's diagonal, also with the arrow keys. Its
    scale (15% to 45%) spans what fits: at most 45% of the video's width and half its height, at
    least a third of that and 96 points along its longer side, so a portrait camera over a wide
    video still resizes. A drag never counts as pressing it (which pins it). The
    remote tile moves its fit button and name clear of the corner the self-view covers. Both
    last for the call.
  - Screens always show their full image. The main focused surface takes its native aspect;
    portrait cameras default to showing the full frame. Every camera offers Full video / Fill
    tile. Hiding self-view stops its local watch without changing the published camera or screen.
  - A participant strip goes beside focused content when the viewport is at least 640 points
    wide and 1.5 times as wide as tall, otherwise below. Its divider supports dragging and arrow
    keys, within 18–40% of the relevant dimension; strip participants page too.
  - The stage draws over the animated call glow (`state::call_glow`, drawn into `Pixels` like
    the voice memo's water; still under reduced motion or in an unfocused window). Each video
    streams into its own `Pixels` from the engine's media threads.
  - The controls: microphone, camera, screen share, show/hide chat, hang up. Chat opens beside
    the stage in a room body at least 780 points wide; below a compact stage otherwise. A compact
    video stage shows one surface at a time, with small controls and single-tile paging
    through every participant and shared screen. The room header returns to
    the expanded call. Hide sidebar / Show sidebar sits on the left before the room name.
    The toggle is hidden while a right-hand panel leaves no room to restore the sidebar.
    Hiding the rooms/direct messages
    sidebar applies while viewing that call; elsewhere it reappears, and the account/space rail
    stays available. These display preferences last for the call.
  - Elsewhere in the app a call bar at the bottom of the sidebar leads back to the call, with
    mute and hang up.
  - Rings show as cards at the top right (`ui::call::rings`), each one line: the caller (a long
    name cut short) and the controls; a notice stacks above them rather than under. Scenes:
    `demo-call-voice`,
    `demo-call-video`, `demo-call-gallery`, `demo-call-many`, `demo-call-direct`, `demo-call-direct-moved`,
    `demo-call-shares`, `demo-call-focus`, `demo-call-no-sidebar`, `demo-call-layout-menu`, `demo-call-video-chat`,
    `demo-call-ringing`, `demo-call-chat`.

- **Phones** show calls with the same commands and events and film through a platform camera,
  and lay them out with the same planner, call layouts and speaker focus, ported to
  TypeScript. The phone's chat shows the call's room under a compact stage, one surface at a
  time, rather than beside the stage. How their views, layouts and native media work is in
  [0010](0010-mobile.md).

## Consequences

- libwebrtc needs link settings in the app's own binary (`.cargo/config.toml`: `-ObjC` on macOS,
  the static C runtime on Windows; clang 21 and lld on Linux): see ADR 0001.
- The build downloads libwebrtc (once per target) and needs a C++ compiler; Linux needs glib
  and PipeWire headers. The binary grows by tens of megabytes.
- Interoperability rests on Element Call's conventions (key event format, identities, key
  derivation), which are partly unstable MSCs. Test changes against a real Element Call.
- Calls can't run headless: tests cover the protocol pieces (contents, identities, focus
  choice, support rules) and the desktop's call state and views from injected events.
