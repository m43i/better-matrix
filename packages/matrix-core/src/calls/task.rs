//! One call, as one task: it joins (or calls), keeps the user's media going, follows everyone
//! else, and reports the call to the app until it ends. The engine starts it and hands it the
//! user's controls and the signals sync delivers for it.

use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use livekit::PlatformAudio;
use livekit::prelude::{LocalAudioTrack, LocalVideoTrack};
use livekit::webrtc::peer_connection::PeerConnectionState;
use livekit::webrtc::prelude::MediaStreamTrack;
use matrix_sdk::Room;
use matrix_sdk::ruma::{RoomId, UserId};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::AbortHandle;

use super::direct::{self, Direct, PeerEvent};
use super::media::{self, Capture, Sinks};
use super::signals::{DirectEvent, IceCandidate, Signal};
use super::{
    CallEnd, CallError, CallInfo, CallParticipant, CallPhase, CallVia, DeviceChoice, DeviceKind, MediaToggle, VideoId,
    VideoSink, session,
};
use crate::account::Account;
use crate::engine::{Event, EventSink};
use crate::model::{AccountId, Person};

/// How a call starts.
#[derive(Clone, Debug)]
pub(crate) enum Start {
    /// A call the user starts: MatrixRTC where the room allows it, else one-to-one.
    New { video: bool },
    /// Join the room's MatrixRTC session (answering its ring).
    Rtc { video: bool },
    /// Answer a one-to-one invite.
    Answer { video: bool, invite: Invite },
}

/// A one-to-one invite, kept while it rings.
#[derive(Clone, Debug)]
pub(crate) struct Invite {
    pub call_id: String,
    pub party_id: String,
    pub sdp: String,
    pub caller: String,
    /// The caller's candidates that came while it rang.
    pub candidates: Vec<IceCandidate>,
}

/// What the engine hands a running call.
pub(crate) enum Control {
    Toggle(MediaToggle),
    Watch(VideoId, VideoSink),
    Unwatch(VideoId),
    Signal(Signal),
    /// The user picked another microphone, speaker or camera (`None`: the system's default).
    Device(DeviceKind, Option<String>),
    HangUp,
}

/// A call's controls as its task reads them. While the call sets up, only a hang-up is acted
/// on; the rest wait their turn.
pub(super) struct Controls {
    receiver: UnboundedReceiver<Control>,
    held: std::collections::VecDeque<Control>,
}

impl Controls {
    /// The next control: held ones first. `None` once the engine let go of the call.
    pub async fn next(&mut self) -> Option<Control> {
        if let Some(control) = self.held.pop_front() {
            return Some(control);
        }
        self.receiver.recv().await
    }

    /// Waits for a hang-up (or the engine letting go), keeping every other control for later.
    pub async fn hung_up(&mut self) {
        while let Some(control) = self.receiver.recv().await {
            if matches!(control, Control::HangUp) {
                return;
            }
            self.held.push_back(control);
        }
    }
}

/// The engine's hold on the call under way.
pub(crate) struct CallHandle {
    pub account: AccountId,
    pub room_id: String,
    /// The one-to-one call's ID, for routing its signals.
    pub call_id: String,
    controls: UnboundedSender<Control>,
    task: tokio::task::JoinHandle<()>,
}

impl CallHandle {
    pub fn send(&self, control: Control) {
        let _ = self.controls.send(control);
    }

    /// Whether the call has ended.
    pub fn is_over(&self) -> bool {
        self.task.is_finished()
    }

    /// Hangs up and hands back the task, to wait for its teardown.
    pub fn hang_up(self) -> tokio::task::JoinHandle<()> {
        self.send(Control::HangUp);
        self.task
    }
}

/// A call's teardown (leaving it, letting go of its devices), which anyone may wait for, any
/// number of times.
pub(crate) type Teardown = futures_util::future::Shared<futures_util::future::BoxFuture<'static, ()>>;

/// The calls hung up and still leaving: whose and where each was, and its teardown. Each stays
/// until it has finished, however many came after it, and waiting never takes one away, so
/// the next call, leaving a room, signing out and quitting all wait for the same teardowns.
#[derive(Default)]
pub(crate) struct Leaving(Vec<(AccountId, String, Teardown)>);

impl Leaving {
    /// The call in `account`'s `room_id` was hung up; `task` is its teardown.
    pub fn hung_up(&mut self, account: AccountId, room_id: String, task: tokio::task::JoinHandle<()>) {
        use futures_util::FutureExt;
        let done = async move {
            let _ = task.await;
        };
        // Those that finished go: only unfinished teardowns are waited for.
        self.0.retain(|(_, _, done)| done.peek().is_none());
        self.0.push((account, room_id, done.boxed().shared()));
    }

    /// Every teardown under way, whoever's.
    pub fn any(&self) -> Option<Teardown> {
        all(self.0.iter().map(|(_, _, done)| done))
    }

    /// The teardowns of `account`'s calls.
    pub fn of_account(&self, account: &AccountId) -> Option<Teardown> {
        all(self.0.iter().filter(|(owner, _, _)| owner == account).map(|(_, _, done)| done))
    }

    /// The teardowns of calls in `account`'s `room_id`.
    pub fn of_room(&self, account: &AccountId, room_id: &str) -> Option<Teardown> {
        let here = |(owner, room, _): &&(AccountId, String, Teardown)| owner == account && room == room_id;
        all(self.0.iter().filter(here).map(|(_, _, done)| done))
    }
}

/// One teardown that ends when all of `teardowns` have; `None` when there are none.
fn all<'a>(teardowns: impl Iterator<Item = &'a Teardown>) -> Option<Teardown> {
    use futures_util::FutureExt;
    let pending: Vec<Teardown> = teardowns.filter(|done| done.peek().is_none()).cloned().collect();
    match pending.len() {
        0 => None,
        1 => pending.into_iter().next(),
        _ => Some(futures_util::future::join_all(pending).map(|_| ()).boxed().shared()),
    }
}

/// Numbers the calls of this process.
static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// How long a new call waits for the one it replaces to leave.
const REPLACE_WAIT: Duration = Duration::from_secs(10);

/// Starts a call in `room_id` once `previous` (the call it replaces) has left; it reports
/// through `sink` and ends with `Event::CallEnded`.
pub(crate) fn start(
    account: Account,
    room_id: String,
    start: Start,
    devices: DeviceChoice,
    sink: Arc<dyn EventSink>,
    http: Option<matrix_sdk::reqwest::Client>,
    previous: Option<Teardown>,
) -> CallHandle {
    let generation = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let call_id = match &start {
        Start::Answer { invite, .. } => invite.call_id.clone(),
        Start::New { .. } | Start::Rtc { .. } => random_id(),
    };
    let (controls, receiver) = mpsc::unbounded_channel();
    let task = {
        let (account, room_id, call_id) = (account.clone(), room_id.clone(), call_id.clone());
        tokio::spawn(async move {
            // The call it replaces leaves first, so its membership and devices are gone.
            if let Some(previous) = previous {
                let _ = tokio::time::timeout(REPLACE_WAIT, previous).await;
            }
            let id = account.id().clone();
            let controls = Controls { receiver, held: std::collections::VecDeque::new() };
            let ended = std::sync::atomic::AtomicBool::new(false);
            let call = Call { generation, sink: &*sink, account: &id, room_id: &room_id, ended: &ended };
            let run = async {
                match (room_of(&account, &room_id), start) {
                    (Err(error), _) => Err(error),
                    (Ok(room), Start::New { video }) => match decide(&account, &room).await {
                        Some(CallVia::Rtc) => session::run(&account, &room, video, devices, controls, call, http).await,
                        Some(CallVia::Direct) => {
                            run_direct(&account, &room, video, devices, call_id, None, controls, call).await
                        }
                        None => Err(CallError::Unavailable),
                    },
                    (Ok(room), Start::Rtc { video }) => {
                        session::run(&account, &room, video, devices, controls, call, http).await
                    }
                    (Ok(room), Start::Answer { video, invite }) => {
                        run_direct(
                            &account,
                            &room,
                            video,
                            devices,
                            invite.call_id.clone(),
                            Some(invite),
                            controls,
                            call,
                        )
                        .await
                    }
                }
            };
            // A panic deep in the media stack ends the call with an error rather than leaving it
            // connecting forever.
            let result =
                std::panic::AssertUnwindSafe(run).catch_unwind().await.unwrap_or_else(|panic| Err(panicked(&*panic)));
            call.end(&result);
        })
    };
    CallHandle { account: account.id().clone(), room_id, call_id, controls, task }
}

/// Which call a task runs, and where it reports.
#[derive(Clone, Copy)]
pub(super) struct Call<'a> {
    pub generation: u64,
    pub sink: &'a dyn EventSink,
    account: &'a AccountId,
    room_id: &'a str,
    /// The end was reported.
    ended: &'a std::sync::atomic::AtomicBool,
}

impl Call<'_> {
    /// Reports how the call ended, once: as soon as it's over, before leaving it (which may
    /// wait on the network), so the app closes it at once.
    pub fn end(&self, result: &Result<CallEnd, CallError>) {
        if self.ended.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let reason = match result {
            Ok(end) => end.clone(),
            Err(error) => CallEnd::Failed(error.to_string()),
        };
        let (account, room_id) = (self.account.clone(), self.room_id.to_owned());
        self.sink.emit(Event::CallEnded { account, room_id, call: self.generation, reason });
    }

    pub fn report(&self, info: &CallInfo) {
        self.sink.emit(Event::CallUpdated(Box::new(info.clone())));
    }

    pub fn failed(&self, error: &CallError) {
        self.sink.emit(Event::CallMediaFailed(error.to_string()));
    }

    /// The camera or a share stopped on its own.
    pub fn stopped(&self, media: MediaToggle) {
        self.sink.emit(Event::CallMediaStopped(media));
    }
}

/// The error a call ends with when its task panicked, logged with the panic's message.
pub(super) fn panicked(panic: &(dyn std::any::Any + Send)) -> CallError {
    let reason = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    tracing::error!("call: the call task panicked: {reason}");
    CallError::Media(format!("the call stopped unexpectedly: {reason}"))
}

/// Which standard a new call in the room speaks, by the same rule its summary shows.
async fn decide(account: &Account, room: &Room) -> Option<CallVia> {
    let kind = if room.is_direct().await.unwrap_or(false) { crate::RoomKind::Direct } else { crate::RoomKind::Group };
    crate::convert::room_call(room, kind, account.call_services(), &account.sticky).await.via
}

fn random_id() -> String {
    let mut bytes = [0u8; 12];
    let _ = getrandom::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn room_of(account: &Account, room_id: &str) -> Result<Room, CallError> {
    let id = RoomId::parse(room_id).map_err(|_| CallError::NoCall)?;
    account.client().get_room(&id).ok_or(CallError::NoCall)
}

pub(super) fn media_error(error: impl std::fmt::Display) -> CallError {
    CallError::Media(error.to_string())
}

/// Someone in the room, as the call shows them.
pub(crate) async fn person(room: &Room, user_id: &str) -> Person {
    let member = match UserId::parse(user_id) {
        Ok(id) => room.get_member_no_sync(&id).await.ok().flatten(),
        Err(_) => None,
    };
    match member {
        Some(member) => Person {
            id: user_id.to_owned(),
            name: member.name().to_owned(),
            avatar: member.avatar_url().map(ToOwned::to_owned).and_then(crate::convert::avatar),
        },
        None => Person { id: user_id.to_owned(), name: user_id.to_owned(), avatar: None },
    }
}

/// The user's camera or screen while it's on: its capture and the track sending it.
pub(super) struct Shared {
    pub capture: Capture,
    pub track: LocalVideoTrack,
}

impl Shared {
    /// Moves the capture to another camera, feeding the same track, so nothing renegotiates.
    /// Blocking. On failure the camera is left off.
    pub fn switch_camera(&mut self, sinks: &Sinks, device: Option<&str>) -> Result<(), CallError> {
        // The old capture lets go of its device before the new one opens.
        self.pause().stop_and_wait();
        let (source, preview) = (self.capture.source.clone(), self.capture.preview);
        self.capture = media::start_camera(sinks.clone(), preview, device, Some(source))?;
        Ok(())
    }

    /// Puts an idle capture in place, keeping the source and preview; returns the one that ran.
    pub fn pause(&mut self) -> Capture {
        let idle = Capture::idle(self.capture.source.clone(), self.capture.preview);
        std::mem::replace(&mut self.capture, idle)
    }
}

// ----- One-to-one --------------------------------------------------------------------------

/// The other member of a direct chat.
async fn partner(room: &Room, me: &str) -> Option<String> {
    if let Some(target) = room.direct_targets().iter().filter_map(|t| t.as_user_id()).find(|u| u.as_str() != me) {
        return Some(target.to_string());
    }
    let members =
        room.members_no_sync(matrix_sdk::RoomMemberships::JOIN | matrix_sdk::RoomMemberships::INVITE).await.ok()?;
    members.iter().map(|m| m.user_id().to_string()).find(|u| u != me)
}

#[expect(clippy::too_many_arguments, reason = "the call's inputs, handed over by its task")]
async fn run_direct(
    account: &Account,
    room: &Room,
    video: bool,
    mut devices: DeviceChoice,
    call_id: String,
    invite: Option<Invite>,
    mut controls: Controls,
    call: Call<'_>,
) -> Result<CallEnd, CallError> {
    let room_id = room.room_id().as_str();
    let me = account.user_id().map_err(|_| CallError::NoCall)?.to_string();
    let party_id = account.client().device_id().ok_or(CallError::NoCall)?.to_string();
    let other = match &invite {
        Some(invite) => invite.caller.clone(),
        None => partner(room, &me).await.ok_or(CallError::Unavailable)?,
    };
    let mut info = CallInfo {
        call: call.generation,
        account: account.id().clone(),
        room_id: room_id.to_owned(),
        via: CallVia::Direct,
        phase: if invite.is_some() { CallPhase::Connecting } else { CallPhase::Calling },
        video,
        microphone: true,
        camera: None,
        screen: None,
        participants: vec![CallParticipant {
            key: other.clone(),
            person: person(room, &other).await,
            muted: false,
            camera: None,
            screen: None,
        }],
    };
    call.report(&info);

    let (peer_events, peer) = mpsc::unbounded_channel();
    let audio = match PlatformAudio::new() {
        Ok(audio) => audio,
        Err(error) => {
            // A ringing caller hears that the answer won't come.
            if invite.is_some() {
                direct::hangup(room, &call_id, &party_id).await;
            }
            return Err(media_error(error));
        }
    };
    media::use_audio_devices(&audio, &devices);
    let microphone = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    let (sinks, mut ended) = Sinks::new();
    let mut camera = None;
    let mut tracks = vec![MediaStreamTrack::Audio(microphone.rtc_track())];
    // A video call carries a camera track from the start, as one can't be added later; a camera
    // that won't start leaves it off (and says why) until the user turns it on again.
    if video {
        let (source, preview) = (media::camera_source(), sinks.next_id());
        let (opening, device) = ((sinks.clone(), source.clone()), devices.camera.clone());
        let opened =
            media::blocking(move || media::start_camera(opening.0, preview, device.as_deref(), Some(opening.1)));
        // A camera that hangs (a stuck driver, a prompt left open) leaves the call going with
        // the camera off rather than connecting forever; it stops on its own once it opens.
        let opened = async {
            tokio::time::timeout(CAMERA_LIMIT, opened).await.unwrap_or(Err(CallError::Stalled("opening the camera")))
        };
        // A camera can be slow to open (or wait on a permission prompt): a hang-up meanwhile
        // ends the call; the camera, once open, stops on its own.
        let started = tokio::select! {
            biased;
            () = controls.hung_up() => None,
            started = opened => Some(started),
        };
        let Some(started) = started else {
            let end = Ok(CallEnd::HungUp);
            call.end(&end);
            // Only a caller waiting on an answer hears of it: no invite went out yet.
            if invite.is_some() {
                direct::hangup(room, &call_id, &party_id).await;
            }
            return end;
        };
        let capture = started.unwrap_or_else(|error| {
            call.failed(&error);
            Capture::idle(source, preview)
        });
        let track = LocalVideoTrack::create_video_track("camera", capture.rtc_source());
        track.rtc_track().set_enabled(capture.is_running());
        tracks.push(MediaStreamTrack::Video(track.rtc_track()));
        info.camera = capture.is_running().then_some(capture.preview);
        camera = Some(Shared { capture, track });
    }
    // Setting up: the connection, then the invite or the answer. A hang-up meanwhile stops it.
    // The other side knows of the call once the invite is on its way (or it called).
    let known = std::sync::atomic::AtomicBool::new(invite.is_some());
    let setup = async {
        let mut connection = Direct::new(room, call_id.clone(), party_id.clone(), peer_events).await?;
        connection.add_tracks(tracks)?;
        match &invite {
            Some(invite) => {
                let answer = connection.accept(&invite.sdp).await?;
                connection.peer = Some(invite.party_id.clone());
                // Candidates that came while it rang.
                connection.candidates(invite.candidates.clone()).await;
                direct::answer(room, &connection, answer).await?;
            }
            None => {
                let offer = connection.offer(video).await?;
                known.store(true, std::sync::atomic::Ordering::Relaxed);
                direct::invite(room, &connection, offer, &other).await?;
            }
        }
        Ok::<_, CallError>(connection)
    };
    // Bounded: a send the server keeps retrying mustn't leave the call calling or connecting.
    let setup = async {
        tokio::time::timeout(SETUP_LIMIT, setup).await.unwrap_or(Err(CallError::Stalled("setting up the call")))
    };
    let connection = tokio::select! {
        biased;
        () = controls.hung_up() => None,
        connection = setup => Some(connection),
    };
    let end = match connection {
        None => Ok(CallEnd::HungUp),
        Some(Err(error)) => Err(error),
        Some(Ok(mut connection)) => {
            call.report(&info);
            let callee = invite.is_some();
            let parts = DirectParts { room, me: &me, other: &other, party_id: &party_id, call_id: &call_id, callee };
            let end = direct_loop(
                &parts,
                &mut connection,
                peer,
                &mut controls,
                call,
                &mut info,
                (&sinks, &mut ended),
                &microphone,
                &mut camera,
                &audio,
                &mut devices,
            )
            .await;
            connection.close();
            end
        }
    };
    call.end(&end);
    // The devices go first, then the other side hears of every end but theirs and a pickup
    // elsewhere.
    drop(audio);
    let release = media::release(camera.map(|shared| shared.capture));
    let tell = async {
        let ended_by_them = matches!(end, Ok(CallEnd::Left | CallEnd::Declined | CallEnd::AnsweredElsewhere));
        if !ended_by_them && known.load(std::sync::atomic::Ordering::Relaxed) {
            direct::hangup(room, &call_id, &party_id).await;
        }
    };
    tokio::join!(release, tell);
    end
}

/// Who the one-to-one call is between.
struct DirectParts<'a> {
    room: &'a Room,
    me: &'a str,
    /// The other person.
    other: &'a str,
    party_id: &'a str,
    call_id: &'a str,
    /// The user answered it, rather than called.
    callee: bool,
}

#[expect(clippy::too_many_arguments, reason = "the call's parts, owned by run_direct")]
async fn direct_loop(
    parts: &DirectParts<'_>,
    connection: &mut Direct,
    mut peer: UnboundedReceiver<PeerEvent>,
    controls: &mut Controls,
    call: Call<'_>,
    info: &mut CallInfo,
    (sinks, ended): (&Sinks, &mut UnboundedReceiver<media::Ended>),
    microphone: &LocalAudioTrack,
    camera: &mut Option<Shared>,
    audio: &PlatformAudio,
    devices: &mut DeviceChoice,
) -> Result<CallEnd, CallError> {
    let DirectParts { room, me, other, party_id, call_id, callee } = *parts;
    let lifetime = tokio::time::sleep(Duration::from_millis(direct::INVITE_LIFETIME));
    tokio::pin!(lifetime);
    // Once both sides have described the call, the devices must get through to each other in
    // time: a connection that never does ends the call rather than leaving it connecting.
    let connecting = tokio::time::sleep(CONNECT_LIMIT);
    tokio::pin!(connecting);
    let mut flush = tokio::time::interval(Duration::from_millis(200));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut outgoing: Vec<IceCandidate> = Vec::new();
    // After a batch failed, candidates wait a second before going again.
    let mut hold_until = tokio::time::Instant::now();
    // Candidates being sent, beside the loop; a batch that failed goes again.
    let mut sending: tokio::task::JoinSet<(Vec<IceCandidate>, bool)> = tokio::task::JoinSet::new();
    let mut remote_video: Option<AbortHandle> = None;
    let end = loop {
        tokio::select! {
            () = &mut lifetime, if connection.peer.is_none() => break Ok(CallEnd::NoAnswer),
            () = &mut connecting, if connection.peer.is_some() && info.phase == CallPhase::Connecting => {
                tracing::warn!("call: the one-to-one connection didn't get through");
                break Err(CallError::Stalled("connecting to the other device"));
            }
            _ = flush.tick() => {
                if !outgoing.is_empty() && sending.is_empty() && tokio::time::Instant::now() >= hold_until {
                    let (room, call_id, party_id) = (room.clone(), call_id.to_owned(), party_id.to_owned());
                    let batch = std::mem::take(&mut outgoing);
                    sending.spawn(async move {
                        let sent = direct::candidates(&room, &call_id, &party_id, &batch).await.is_ok();
                        (batch, sent)
                    });
                }
            }
            Some(Ok((batch, sent))) = sending.join_next() => {
                if !sent {
                    outgoing.extend(batch);
                    hold_until = tokio::time::Instant::now() + Duration::from_secs(1);
                }
            }
            event = peer.recv() => match event {
                Some(PeerEvent::Candidate(candidate)) => outgoing.push(candidate),
                Some(PeerEvent::Track(MediaStreamTrack::Video(track))) => {
                    let id = sinks.next_id();
                    if let Some(task) = remote_video.replace(tokio::spawn(media::watch_remote(track, id, sinks.clone())).abort_handle()) {
                        task.abort();
                    }
                    if let Some(person) = info.participants.first_mut() { person.camera = Some(id) }
                    call.report(info);
                }
                Some(PeerEvent::Track(_)) => {}
                Some(PeerEvent::State(state)) => {
                    tracing::debug!(?state, "call: one-to-one connection");
                    let phase = match state {
                        PeerConnectionState::Connected => CallPhase::Connected,
                        PeerConnectionState::Failed => break Ok(CallEnd::Failed("the connection failed".into())),
                        // It may come back on its own; failing, it reports so.
                        PeerConnectionState::Disconnected if info.phase == CallPhase::Connected => CallPhase::Reconnecting,
                        _ => info.phase,
                    };
                    if phase != info.phase {
                        info.phase = phase;
                        call.report(info);
                    }
                }
                None => break Ok(CallEnd::Left),
            },
            // The camera stopped on its own: it shows as off, and the user hears why.
            // (An earlier capture's end, from before a switch, is ignored.)
            Some(media::Ended { instance, .. }) = ended.recv() => {
                if let Some(shared) = camera.as_mut().filter(|shared| shared.capture.instance == instance) {
                    drop(shared.pause());
                    shared.track.rtc_track().set_enabled(false);
                    info.camera = None;
                    call.report(info);
                    call.stopped(MediaToggle::Camera(false));
                }
            }
            control = controls.next() => {
                let Some(control) = control else { break Ok(CallEnd::HungUp) };
                match control {
                    Control::HangUp => break Ok(CallEnd::HungUp),
                    Control::Watch(id, video_sink) => sinks.watch(id, video_sink),
                    Control::Unwatch(id) => sinks.unwatch(id),
                    Control::Toggle(MediaToggle::Microphone(on)) => {
                        if on { microphone.unmute() } else { microphone.mute() }
                        info.microphone = on;
                        call.report(info);
                    }
                    // The connection was negotiated with or without a camera track: the camera
                    // stops and starts (letting go of the device while off), the track stays.
                    Control::Toggle(MediaToggle::Camera(on)) => {
                        if let Some(shared) = camera.take() {
                            tokio::select! {
                                biased;
                                () = controls.hung_up() => break Ok(CallEnd::HungUp),
                                shared = set_camera(shared, on, sinks, devices.camera.clone(), call) => *camera = shared,
                            }
                            info.camera = shown_camera(camera);
                            call.report(info);
                        }
                    }
                    Control::Toggle(MediaToggle::Screen(_)) => {}
                    Control::Device(kind, id) => {
                        devices.set(kind, id);
                        media::switch_audio_device(audio, devices, kind);
                        // A camera that's off only remembers the pick.
                        if let Some(shared) = camera.take_if(|shared| kind == DeviceKind::Camera && shared.capture.is_running()) {
                            let switch = switch_camera(shared, sinks, devices.camera.clone());
                            tokio::select! {
                                biased;
                                () = controls.hung_up() => break Ok(CallEnd::HungUp),
                                (shared, result) = switch => {
                                    *camera = shared;
                                    if let Err(error) = result {
                                        call.failed(&error);
                                    }
                                }
                            }
                            // A switch that failed leaves the camera off.
                            if let Some(shared) = camera.as_ref() {
                                shared.track.rtc_track().set_enabled(shared.capture.is_running());
                            }
                            info.camera = shown_camera(camera);
                            call.report(info);
                        }
                    }
                    // Answering it sends to the room: a hang-up meanwhile still ends the call.
                    Control::Signal(Signal::Direct { sender, event, .. }) if event.call_id() == call_id => {
                        let peer = connection.peer.clone();
                        let sides = Sides { me, other, party: party_id, peer: peer.as_deref(), callee };
                        let signal = direct_signal(room, connection, &sides, &sender, event);
                        tokio::select! {
                            biased;
                            () = controls.hung_up() => break Ok(CallEnd::HungUp),
                            result = signal => match result {
                                Ok(Some(end)) => break Ok(end),
                                Ok(None) => {}
                                Err(error) => break Err(error),
                            },
                        }
                        // Their answer came: the call connects now, and has its time to.
                        if info.phase == CallPhase::Calling && connection.peer.is_some() {
                            connecting.as_mut().reset(tokio::time::Instant::now() + CONNECT_LIMIT);
                            info.phase = CallPhase::Connecting;
                            call.report(info);
                        }
                    }
                    Control::Signal(_) => {}
                }
            }
        }
    };
    if let Some(task) = remote_video {
        task.abort();
    }
    end
}

/// How long the camera of a one-to-one video call may take to open before the call goes on
/// without it.
const CAMERA_LIMIT: Duration = Duration::from_secs(15);
/// How long setting up a one-to-one call may take: the connection, then sending the invite or
/// the answer.
const SETUP_LIMIT: Duration = Duration::from_secs(20);
/// How long picking the other side's answer may take to send.
const SELECT_LIMIT: Duration = Duration::from_secs(10);
/// How long a one-to-one call's devices have to get through to each other once it's answered.
const CONNECT_LIMIT: Duration = Duration::from_secs(30);

/// The camera's preview while it runs.
fn shown_camera(camera: &Option<Shared>) -> Option<VideoId> {
    camera.as_ref().filter(|shared| shared.capture.is_running()).map(|shared| shared.capture.preview)
}

/// Starts or stops a one-to-one call's camera, keeping its track; `None` only if the camera
/// was lost on a blocking thread.
async fn set_camera(
    mut shared: Shared,
    on: bool,
    sinks: &Sinks,
    device: Option<String>,
    call: Call<'_>,
) -> Option<Shared> {
    if on && !shared.capture.is_running() {
        let (back, result) = switch_camera(shared, sinks, device).await;
        shared = back?;
        if let Err(error) = result {
            call.failed(&error);
        }
    } else if !on && shared.capture.is_running() {
        let running = shared.pause();
        media::release([running]).await;
    }
    shared.track.rtc_track().set_enabled(shared.capture.is_running());
    Some(shared)
}

/// Moves a camera to another device on a blocking thread; the camera comes back either way
/// (`None` only if the thread was lost).
pub(super) async fn switch_camera(
    mut shared: Shared,
    sinks: &Sinks,
    device: Option<String>,
) -> (Option<Shared>, Result<(), CallError>) {
    let sinks = sinks.clone();
    let switched = tokio::task::spawn_blocking(move || {
        let result = shared.switch_camera(&sinks, device.as_deref());
        (shared, result)
    })
    .await;
    match switched {
        Ok((shared, result)) => (Some(shared), result),
        Err(error) => (None, Err(media_error(error))),
    }
}

/// What a signal of the one-to-one call means for it.
#[derive(Debug, PartialEq, Eq)]
enum Heard {
    /// Nothing: an echo of this device's own, or from someone not in the call.
    Nothing,
    /// The other side answered from `party` with `sdp`.
    Answered { party: String, sdp: String },
    /// Their candidates.
    Candidates(Vec<IceCandidate>),
    /// The call is over.
    Ended(CallEnd),
}

/// Who the one-to-one call is with, as signals are checked against it.
struct Sides<'a> {
    me: &'a str,
    /// The other person.
    other: &'a str,
    /// This device's party.
    party: &'a str,
    /// Their device's party, once it answered (or as the invite came).
    peer: Option<&'a str>,
    /// The user answered it, rather than called.
    callee: bool,
}

/// Reads a signal of the call. Only the other person and the user's own other devices count;
/// once their device is known, only it can hang up or send candidates.
fn hear(sides: &Sides<'_>, sender: &str, event: DirectEvent) -> Heard {
    let Sides { me, other, party, peer, callee } = *sides;
    match event {
        // Answered on another of the user's devices (a call they're making).
        DirectEvent::Answer { party_id, .. } if sender == me && party_id != party && peer.is_none() && !callee => {
            Heard::Ended(CallEnd::AnsweredElsewhere)
        }
        _ if sender != other => Heard::Nothing,
        DirectEvent::Answer { party_id, sdp, .. } if peer.is_none() && !callee => {
            Heard::Answered { party: party_id, sdp }
        }
        DirectEvent::Candidates { party_id, candidates, .. } if peer.is_none_or(|peer| peer == party_id) => {
            Heard::Candidates(candidates)
        }
        // The caller picked another device's answer: only an answering side hears that.
        DirectEvent::SelectAnswer { selected, .. } if callee && selected != party => {
            Heard::Ended(CallEnd::AnsweredElsewhere)
        }
        DirectEvent::Hangup { party_id, .. } if peer.is_none_or(|peer| peer == party_id) => Heard::Ended(CallEnd::Left),
        DirectEvent::Reject { .. } if peer.is_none() && !callee => Heard::Ended(CallEnd::Declined),
        _ => Heard::Nothing,
    }
}

/// What a signal does to a one-to-one call still ringing for the user, `me`: only the caller's
/// ringing device adds candidates or hangs up; only the user's own other devices answer or
/// decline it elsewhere.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RingHeard {
    Nothing,
    Candidates,
    Over,
}

pub(crate) fn hear_ringing(invite: &Invite, me: &str, sender: &str, event: &DirectEvent) -> RingHeard {
    if event.call_id() != invite.call_id {
        return RingHeard::Nothing;
    }
    let from_caller = |party: &str| sender == invite.caller && party == invite.party_id;
    match event {
        DirectEvent::Candidates { party_id, .. } if from_caller(party_id) => RingHeard::Candidates,
        DirectEvent::Hangup { party_id, .. } if from_caller(party_id) => RingHeard::Over,
        DirectEvent::Answer { .. } | DirectEvent::Reject { .. } if sender == me => RingHeard::Over,
        _ => RingHeard::Nothing,
    }
}

/// One signal of the call: `Some` when it ends the call.
async fn direct_signal(
    room: &Room,
    call: &mut Direct,
    sides: &Sides<'_>,
    sender: &str,
    event: DirectEvent,
) -> Result<Option<CallEnd>, CallError> {
    match hear(sides, sender, event) {
        Heard::Nothing => {}
        Heard::Answered { party, sdp } => {
            call.answered(&sdp).await?;
            call.peer = Some(party.clone());
            let select = direct::select_answer(room, call, &party);
            tokio::time::timeout(SELECT_LIMIT, select)
                .await
                .unwrap_or(Err(CallError::Stalled("picking the answer")))?;
        }
        Heard::Candidates(candidates) => call.candidates(candidates).await,
        Heard::Ended(end) => return Ok(Some(end)),
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicked_call_should_end_with_the_panics_message() {
        let error = panicked(&String::from("no crypto provider"));
        assert!(error.to_string().contains("no crypto provider"), "{error}");
    }

    fn sides(callee: bool, peer: Option<&'static str>) -> Sides<'static> {
        Sides { me: "@me:x", other: "@them:x", party: "MINE", peer, callee }
    }

    fn hangup(party: &str) -> DirectEvent {
        DirectEvent::Hangup { call_id: "c".into(), party_id: party.into() }
    }

    fn invite() -> Invite {
        Invite {
            call_id: "c".into(),
            party_id: "THEIRS".into(),
            sdp: String::new(),
            caller: "@them:x".into(),
            candidates: Vec::new(),
        }
    }

    #[tokio::test]
    async fn waiting_for_a_teardown_should_leave_it_for_the_next_wait() {
        let (finish, finished) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _ = finished.await;
        });
        let mut leaving = Leaving::default();
        let account = AccountId::new("@me:x");
        leaving.hung_up(account.clone(), "!r".into(), task);
        assert!(leaving.of_room(&account, "!other").is_none());
        let first = leaving.of_room(&account, "!r").unwrap_or_else(|| panic!("no teardown"));
        let wait = tokio::time::timeout(Duration::from_millis(20), first.clone()).await;
        assert!(wait.is_err(), "the teardown hasn't finished");
        // Still there for the next call, and done for every waiter once it finishes.
        assert!(leaving.any().is_some() && leaving.of_account(&account).is_some());
        assert!(leaving.of_account(&AccountId::new("@other:x")).is_none());
        let _ = finish.send(());
        first.await;
        // Finished, nothing is left to wait for.
        assert!(leaving.any().is_none());
    }

    #[tokio::test]
    async fn a_teardown_should_stay_while_newer_calls_are_hung_up() {
        let (finish_a, a_finished) = tokio::sync::oneshot::channel::<()>();
        let (a, b) = (AccountId::new("@a:x"), AccountId::new("@b:x"));
        let mut leaving = Leaving::default();
        leaving.hung_up(
            a.clone(),
            "!one".into(),
            tokio::spawn(async move {
                let _ = a_finished.await;
            }),
        );
        leaving.hung_up(b.clone(), "!two".into(), tokio::spawn(async {}));
        leaving.hung_up(b.clone(), "!three".into(), tokio::spawn(async {}));
        // A's teardown is still found by its room and account after two more calls.
        let one = leaving.of_room(&a, "!one").unwrap_or_else(|| panic!("A's teardown is gone"));
        assert!(leaving.of_account(&a).is_some());
        assert!(tokio::time::timeout(Duration::from_millis(20), one.clone()).await.is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(20), leaving.any().unwrap_or_else(|| panic!("none")))
                .await
                .is_err()
        );
        let _ = finish_a.send(());
        one.await;
    }

    #[test]
    fn a_ring_should_end_only_by_its_caller_or_the_users_own_devices() {
        let hang = |party: &str| DirectEvent::Hangup { call_id: "c".into(), party_id: party.into() };
        assert_eq!(hear_ringing(&invite(), "@me:x", "@stranger:x", &hang("THEIRS")), RingHeard::Nothing);
        assert_eq!(hear_ringing(&invite(), "@me:x", "@them:x", &hang("OTHER")), RingHeard::Nothing);
        assert_eq!(hear_ringing(&invite(), "@me:x", "@them:x", &hang("THEIRS")), RingHeard::Over);
        let answer = DirectEvent::Answer { call_id: "c".into(), party_id: "LAPTOP".into(), sdp: String::new() };
        assert_eq!(hear_ringing(&invite(), "@me:x", "@me:x", &answer), RingHeard::Over);
        assert_eq!(hear_ringing(&invite(), "@me:x", "@stranger:x", &answer), RingHeard::Nothing);
    }

    #[test]
    fn a_hangup_from_someone_else_in_the_room_should_be_ignored() {
        assert_eq!(hear(&sides(true, Some("THEIRS")), "@stranger:x", hangup("THEIRS")), Heard::Nothing);
    }

    #[test]
    fn a_hangup_from_the_other_device_should_end_the_call() {
        assert_eq!(hear(&sides(true, Some("THEIRS")), "@them:x", hangup("THEIRS")), Heard::Ended(CallEnd::Left));
        assert_eq!(hear(&sides(true, Some("THEIRS")), "@them:x", hangup("OTHER")), Heard::Nothing);
    }

    #[test]
    fn the_users_own_echo_should_change_nothing() {
        let select =
            DirectEvent::SelectAnswer { call_id: "c".into(), party_id: "MINE".into(), selected: "MINE".into() };
        assert_eq!(hear(&sides(false, Some("THEIRS")), "@me:x", select), Heard::Nothing);
    }

    #[test]
    fn an_answer_from_another_of_the_users_devices_should_end_a_call_they_make() {
        let answer = DirectEvent::Answer { call_id: "c".into(), party_id: "LAPTOP".into(), sdp: String::new() };
        assert_eq!(hear(&sides(false, None), "@me:x", answer), Heard::Ended(CallEnd::AnsweredElsewhere));
    }

    #[test]
    fn the_caller_picking_another_device_should_end_the_answer() {
        let select =
            DirectEvent::SelectAnswer { call_id: "c".into(), party_id: "THEIRS".into(), selected: "LAPTOP".into() };
        assert_eq!(hear(&sides(true, Some("THEIRS")), "@them:x", select), Heard::Ended(CallEnd::AnsweredElsewhere));
    }
}
