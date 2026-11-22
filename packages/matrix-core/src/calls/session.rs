//! A MatrixRTC call, as Element Call runs one in its default (multi-SFU) mode: the user publishes
//! on their own homeserver's LiveKit service and subscribes to every service another member
//! publishes on, one LiveKit connection each.
//!
//! - **Who shows:** the room's call memberships decide who is in the call. A LiveKit participant
//!   shows when its identity belongs to a member (or names a user, `@user:server:DEVICE`), once
//!   across connections.
//! - **Keys:** each device sends its key to every member device, and again to those it couldn't
//!   reach; it rotates the key a little after someone leaves, sending the new one first and using
//!   it on its outgoing tracks a second later, so the others have it.
//! - **Work beside the loop:** connecting, sending keys, keeping the membership and looking up
//!   names run as tasks that report back, so mute and hang-up never wait on the network. Setting
//!   up runs against a hang-up too.
//! - **One way out:** [`Session::close`] closes every LiveKit connection (a dropped one stays
//!   connected), stops the devices and leaves the membership, whatever ended the call.

use std::collections::{HashMap, HashSet};

use std::time::Duration;

use futures_util::FutureExt;

use livekit::PlatformAudio;
use livekit::e2ee::key_provider::{KeyDerivationAlgorithm, KeyProvider, KeyProviderOptions};
use livekit::e2ee::{E2eeOptions, EncryptionType};
use livekit::options::TrackPublishOptions;
use livekit::prelude::{
    LocalAudioTrack, LocalTrack, LocalVideoTrack, Participant, ParticipantIdentity, RemoteTrack, Room as LiveRoom,
    RoomEvent, RoomOptions, TrackSid, TrackSource,
};
use matrix_sdk::Room;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::{AbortHandle, JoinSet};
use tokio::time::Instant;

use super::keys::{self, OwnKey};
use super::media::{self, Capture, Sinks};
use super::rtc::{self, Member, Transport};
use super::signals::Signal;
use super::task::{Call, Control, Controls, Shared, media_error, panicked, person, switch_camera};
use super::{
    CallEnd, CallError, CallInfo, CallParticipant, CallPhase, CallVia, DeviceChoice, DeviceKind, MediaToggle, VideoId,
};
use crate::account::Account;
use crate::engine::Event;
use crate::model::Person;

/// How often the participants' audio levels are reported.
const LEVELS_EVERY: Duration = Duration::from_millis(100);
/// How long a new key waits after it was sent before the user's media uses it.
const USE_KEY_AFTER: Duration = Duration::from_secs(1);
/// The least time between two rotations: several leaving at once rotate once.
const ROTATE_GRACE: Duration = Duration::from_secs(10);
/// How often keys that didn't arrive, services that dropped and a lost membership are tried
/// again.
const RETRY_EVERY: Duration = Duration::from_secs(5);
/// How often the membership is sent again, well inside its four hours.
const RENEW_EVERY: Duration = Duration::from_secs(60 * 60);
/// How long a LiveKit connection may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long any other step of joining may take.
const STEP_LIMIT: Duration = Duration::from_secs(20);

/// A device in the call, as keys are sent: its user and device ID.
type Device = (String, String);

pub(super) async fn run(
    account: &Account,
    room: &Room,
    video: bool,
    devices: DeviceChoice,
    mut controls: Controls,
    call: Call<'_>,
    http: Option<matrix_sdk::reqwest::Client>,
) -> Result<CallEnd, CallError> {
    let own = Transport::Service(account.rtc_service().ok_or(CallError::Unavailable)?);
    let mut info = CallInfo {
        call: call.generation,
        account: account.id().clone(),
        room_id: room.room_id().to_string(),
        via: CallVia::Rtc,
        phase: CallPhase::Connecting,
        video,
        microphone: true,
        camera: None,
        screen: None,
        participants: Vec::new(),
    };
    call.report(&info);
    let (events, live_events) = mpsc::unbounded_channel();
    let cx = Context {
        room: room.clone(),
        user_id: account.user_id().map_err(|_| CallError::NoCall)?,
        device_id: account.client().device_id().ok_or(CallError::NoCall)?.to_string(),
        own,
        sticky: account.sticky.clone(),
        http: http.ok_or_else(|| CallError::Service("no HTTP client".into()))?,
        key_provider: KeyProvider::new(KeyProviderOptions {
            ratchet_window_size: 10,
            key_ring_size: 256,
            key_derivation_algorithm: KeyDerivationAlgorithm::HKDF,
            ..KeyProviderOptions::default()
        }),
        // Media is encrypted end to end where the room is, as Element Call does.
        encrypted: room.encryption_state().is_encrypted(),
        video,
        events,
    };
    let mut session = Session::default();
    let mut live = None;
    let (sinks, ended) = Sinks::new();
    let work = async {
        let ready = tokio::select! {
            biased;
            () = controls.hung_up() => None,
            ready = set_up(&cx, call, &mut session, &devices, &sinks, &mut info) => Some(ready),
        };
        match ready {
            None => Ok(CallEnd::HungUp),
            Some(Err(error)) => Err(error),
            Some(Ok(setup)) => {
                let live = live.insert(Live::new(&cx, call, sinks, devices, setup));
                Ok(live.run(&mut session, &mut controls, &mut info, live_events, ended).await)
            }
        }
    };
    // A panic deep in the media stack still lets go of what the call holds: a dropped LiveKit
    // room stays connected, and the membership would linger.
    let end = std::panic::AssertUnwindSafe(work).catch_unwind().await.unwrap_or_else(|panic| Err(panicked(&*panic)));
    call.end(&end);
    if let Some(live) = live {
        live.finish().await;
    }
    session.close(&cx).await;
    end
}

/// What the call holds that must be let go of when it ends.
#[derive(Default)]
struct Session {
    primary: Option<Link>,
    links: HashMap<Transport, Link>,
    audio: Option<PlatformAudio>,
    microphone: Option<LocalAudioTrack>,
    camera: Option<Shared>,
    screen: Option<Shared>,
    /// The membership was sent, so leaving sends the delayed leave (or an empty membership).
    joined: bool,
    delay_id: Option<String>,
}

impl Session {
    fn primary(&self) -> Option<&LiveRoom> {
        self.primary.as_ref().map(|link| &link.live)
    }

    fn links(&self) -> impl Iterator<Item = &Link> {
        self.primary.iter().chain(self.links.values())
    }

    /// Leaves the membership while the connections close and the cameras stop, so a quit's
    /// short wait covers the leave.
    async fn close(mut self, cx: &Context) {
        // The microphone and speaker go first, so nothing is heard or sent while leaving.
        drop(self.audio.take());
        let leave = async {
            if self.joined {
                rtc::leave(&cx.room, cx.user_id.clone(), &cx.device_id, self.delay_id.as_deref()).await;
            }
        };
        let links = self.primary.take().into_iter().chain(self.links.drain().map(|(_, link)| link));
        let captures =
            media::release([self.camera.take(), self.screen.take()].into_iter().flatten().map(|shared| shared.capture));
        tokio::join!(leave, futures_util::future::join_all(links.map(Link::close)), captures);
    }

    /// Points the encryption of every outgoing track at the user's key `index`. Only the
    /// user's own service carries their tracks.
    fn use_key_index(&self, index: u8) {
        let Some(primary) = self.primary() else { return };
        let me = primary.local_participant().identity();
        for ((identity, _), cryptor) in primary.e2ee_manager().frame_cryptors() {
            if identity == me {
                cryptor.set_key_index(i32::from(index));
            }
        }
    }
}

/// One LiveKit connection: the room on one SFU and the task forwarding its events.
struct Link {
    live: LiveRoom,
    forward: AbortHandle,
}

impl Link {
    /// Closes the connection: a dropped `Room` stays connected.
    async fn close(self) {
        self.forward.abort();
        let _ = self.live.close().await;
    }
}

/// What the session needs, cheap to clone into the work beside its loop.
#[derive(Clone)]
struct Context {
    room: Room,
    user_id: OwnedUserId,
    device_id: String,
    /// The user's own call service, where they publish.
    own: Transport,
    /// The account's matrix_2_0 memberships.
    sticky: super::StickyMembers,
    http: matrix_sdk::reqwest::Client,
    key_provider: KeyProvider,
    encrypted: bool,
    video: bool,
    events: UnboundedSender<(Transport, RoomEvent)>,
}

impl Context {
    /// Connects to `service`, forwarding its events tagged with it.
    async fn connect(self, service: Transport) -> Result<Link, CallError> {
        let grant = rtc::grant(&self.http, &self.room, &service, self.user_id.clone(), &self.device_id);
        let grant = step("asking the call service for a token", STEP_LIMIT, grant).await?;
        let mut options = RoomOptions::default();
        // The app asks for no particular size, so every video comes whole.
        options.adaptive_stream = false;
        options.dynacast = true;
        if self.encrypted {
            options.encryption =
                Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider: self.key_provider.clone() });
        }
        tracing::debug!(%service, sfu = %grant.url, "call: connecting");
        let connect = async { LiveRoom::connect(&grant.url, &grant.jwt, options).await.map_err(media_error) };
        let (live, mut events) = step("connecting to the call service", CONNECT_TIMEOUT, connect).await?;
        let sender = self.events.clone();
        let forward = tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if sender.send((service.clone(), event)).is_err() {
                    break;
                }
            }
        })
        .abort_handle();
        Ok(Link { live, forward })
    }

    /// The user's membership; `since`: when they first joined, which a renewal keeps, so the
    /// call's oldest member stays the same.
    fn membership(&self, since: Option<u64>) -> Value {
        let room_id = self.room.room_id().as_str();
        rtc::membership(self.user_id.as_str(), &self.device_id, room_id, self.own.address(), self.video, since)
    }

    fn is_me(&self, member: &Member) -> bool {
        member.user_id == self.user_id.as_str() && member.device_id == self.device_id
    }

    /// The call's members besides this device, and this device's own membership.
    async fn members(&self) -> (Vec<Member>, Option<Member>) {
        // matrix_2_0 members who joined before this session heard them, once per room.
        self.sticky.fetch(&self.room, &self.http).await;
        let (mut me, others): (Vec<Member>, Vec<Member>) =
            rtc::members(&self.room, &self.sticky).await.into_iter().partition(|member| self.is_me(member));
        (others, me.pop())
    }
}

/// What set-up hands the loop: the key in use and the members it found.
struct Setup {
    key: OwnKey,
    members: Vec<Member>,
}

/// Connects, starts the microphone (and camera), sends the membership, and rings a direct chat
/// nobody else is in the call of. `session` holds what it makes, so a hang-up half-way still lets
/// go of it.
async fn set_up(
    cx: &Context,
    call: Call<'_>,
    session: &mut Session,
    devices: &DeviceChoice,
    sinks: &Sinks,
    info: &mut CallInfo,
) -> Result<Setup, CallError> {
    let primary = &session.primary.insert(cx.clone().connect(cx.own.clone()).await?).live;
    let key = OwnKey::new().map_err(media_error)?;
    if cx.encrypted {
        cx.key_provider.set_key(&primary.local_participant().identity(), i32::from(key.index), key.key.to_vec());
    }
    let audio = session.audio.insert(PlatformAudio::new().map_err(media_error)?);
    media::use_audio_devices(audio, devices);
    let microphone = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    let options = TrackPublishOptions { source: TrackSource::Microphone, ..TrackPublishOptions::default() };
    let participant = primary.local_participant();
    let publish =
        async { participant.publish_track(LocalTrack::Audio(microphone.clone()), options).await.map_err(media_error) };
    step("sending the microphone", STEP_LIMIT, publish).await?;
    session.microphone = Some(microphone);
    if cx.video {
        // A camera that won't start leaves a voice call, and says why.
        let camera = async {
            share(primary, media::open_camera(sinks, devices.camera.clone()).await, TrackSource::Camera).await
        };
        match step("starting the camera", STEP_LIMIT, camera).await {
            Ok(shared) => session.camera = Some(shared),
            Err(error) => call.failed(&error),
        }
        info.camera = session.camera.as_ref().map(|shared| shared.capture.preview);
    }
    let (members, _) = cx.members().await;
    session.joined = true;
    let join = rtc::join(&cx.room, cx.user_id.clone(), &cx.device_id, cx.membership(None));
    session.delay_id = step("joining the room's call", STEP_LIMIT, join).await?;
    if members.is_empty() {
        // The first one in tells the room: a ring in a direct chat, a notification in a group
        // (as Element Call does), so every call has its card. Untold, the call still runs.
        let ring = cx.room.is_direct().await.unwrap_or(false);
        let _ = tokio::time::timeout(STEP_LIMIT, rtc::ring(&cx.room, cx.video, ring)).await;
    }
    tracing::info!(members = members.len(), "call: joined");
    Ok(Setup { key, members })
}

/// One step of joining, logged and bounded: a step that stalls fails the call and names itself,
/// rather than leaving it connecting.
async fn step<T>(
    name: &'static str,
    limit: Duration,
    work: impl std::future::Future<Output = Result<T, CallError>>,
) -> Result<T, CallError> {
    let started = Instant::now();
    tracing::debug!("call: {name}");
    let result = tokio::time::timeout(limit, work).await.unwrap_or(Err(CallError::Stalled(name)));
    let ms = started.elapsed().as_millis();
    match &result {
        Ok(_) => tracing::debug!(ms, "call: {name}: done"),
        Err(error) => tracing::warn!(ms, %error, "call: {name}: failed"),
    }
    result
}

/// What the work beside the loop reports.
enum Work {
    Connected(Transport, Result<Link, CallError>),
    /// Keys went out; these devices didn't get them.
    KeysSent(HashSet<Device>),
    Person(String, Person),
    /// The call's members besides this device, and this device's own membership.
    Members(Vec<Member>, Option<Member>),
    /// Whether the delayed leave was put off.
    Restarted(bool),
    Rejoined(Result<Option<String>, CallError>),
}

/// Whether a LiveKit event changed what the call shows.
enum Flow {
    Report,
    Quiet,
    End(CallEnd),
}

/// A remote video the app may watch.
struct RemoteVideo {
    owner: String,
    /// The service it comes over.
    service: Transport,
    sid: TrackSid,
    task: AbortHandle,
}

/// One person's media, by LiveKit identity.
#[derive(Clone, Copy, Default)]
struct Media {
    muted: bool,
    camera: Option<VideoId>,
    screen: Option<VideoId>,
}

/// Everyone else in the call: where each LiveKit identity is connected, who the memberships say
/// they are, their media, and their videos.
#[derive(Default)]
struct Others {
    /// The services each identity is on, in the order identities arrived.
    present: Vec<(String, HashSet<Transport>)>,
    /// The user each member identity stands for.
    members: HashMap<String, String>,
    people: HashMap<String, Person>,
    /// Users whose names are being looked up.
    looking: HashSet<String>,
    media: HashMap<String, Media>,
    videos: HashMap<VideoId, RemoteVideo>,
}

impl Others {
    /// Someone on `service`.
    fn arrived(&mut self, identity: &str, service: &Transport) {
        match self.present.iter_mut().find(|(known, _)| known == identity) {
            Some((_, services)) => {
                services.insert(service.clone());
            }
            None => self.present.push((identity.to_owned(), HashSet::from([service.clone()]))),
        }
    }

    /// Someone left `service`, and their videos over it went: gone from the call once on no
    /// service at all.
    fn left(&mut self, identity: &str, service: &Transport) {
        let Some(index) = self.present.iter().position(|(known, _)| known == identity) else { return };
        self.present[index].1.remove(service);
        let gone: Vec<VideoId> = self
            .videos
            .iter()
            .filter(|(_, video)| video.owner == identity && video.service == *service)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.drop_video(id);
        }
        if self.present[index].1.is_empty() {
            self.present.remove(index);
            self.media.remove(identity);
        }
    }

    /// Everyone on a service that went.
    fn left_service(&mut self, service: &Transport) {
        let identities: Vec<String> = self.present.iter().map(|(identity, _)| identity.clone()).collect();
        for identity in identities {
            self.left(&identity, service);
        }
    }

    fn drop_video(&mut self, id: VideoId) {
        let Some(video) = self.videos.remove(&id) else { return };
        video.task.abort();
        if let Some(media) = self.media.get_mut(&video.owner) {
            for shown in [&mut media.camera, &mut media.screen] {
                if *shown == Some(id) {
                    *shown = None;
                }
            }
        }
    }

    /// The video arriving as `sid`, if it's subscribed.
    fn video(&self, sid: &TrackSid) -> Option<VideoId> {
        self.videos.iter().find(|(_, video)| video.sid == *sid).map(|(id, _)| *id)
    }

    /// The user an identity stands for: a member's, or the one `@user:server:DEVICE` names.
    fn user(&self, identity: &str) -> Option<String> {
        self.members.get(identity).cloned().or_else(|| keys::parse_identity(identity).map(|(user, _)| user.to_owned()))
    }

    /// Who the call shows: connected identities that stand for someone, in the order they came.
    fn participants(&self) -> Vec<CallParticipant> {
        self.present
            .iter()
            .filter_map(|(identity, _)| {
                let user = self.user(identity)?;
                let media = self.media.get(identity).copied().unwrap_or_default();
                let person = self.people.get(&user).cloned().unwrap_or_else(|| Person {
                    id: user.clone(),
                    name: user,
                    avatar: None,
                });
                Some(CallParticipant {
                    key: identity.clone(),
                    person,
                    muted: media.muted,
                    camera: media.camera,
                    screen: media.screen,
                })
            })
            .collect()
    }

    /// Looks up the name of a user the call shows, once.
    fn look_up(&mut self, user: String, room: &Room, work: &mut JoinSet<Work>) {
        if self.people.contains_key(&user) || !self.looking.insert(user.clone()) {
            return;
        }
        let room = room.clone();
        work.spawn(async move {
            let person = person(&room, &user).await;
            Work::Person(user, person)
        });
    }
}

/// The running call: what it shows and what it keeps going.
struct Live<'a, 'c> {
    cx: &'a Context,
    call: Call<'c>,
    sinks: Sinks,
    devices: DeviceChoice,
    others: Others,
    work: JoinSet<Work>,
    /// The members besides this device, as last read.
    members: Vec<Member>,
    /// The member devices the key went to, and those it didn't reach yet.
    known: HashSet<Device>,
    unsent: HashSet<Device>,
    key: OwnKey,
    /// A new key sent, waiting for its time to be used.
    pending: Option<(Instant, OwnKey)>,
    rotate_at: Option<Instant>,
    last_rotation: Instant,
    /// Services being connected to.
    connecting: HashSet<Transport>,
    /// The membership is being sent again; `lost`: it should be, once this one is through.
    rejoining: bool,
    lost: bool,
    /// The user's membership has been seen since it was last sent, so its absence means it's gone.
    seen: bool,
    /// When the user first joined, as their membership says: renewals keep it.
    since: Option<u64>,
    /// The call as last reported, so an unchanged one isn't reported again.
    reported: Option<CallInfo>,
}

impl<'a, 'c> Live<'a, 'c> {
    fn new(cx: &'a Context, call: Call<'c>, sinks: Sinks, devices: DeviceChoice, setup: Setup) -> Self {
        let mut live = Self {
            cx,
            call,
            sinks,
            devices,
            others: Others::default(),
            work: JoinSet::new(),
            members: Vec::new(),
            known: HashSet::new(),
            unsent: HashSet::new(),
            key: setup.key,
            pending: None,
            rotate_at: None,
            last_rotation: Instant::now(),
            connecting: HashSet::new(),
            rejoining: false,
            lost: false,
            seen: false,
            since: None,
            reported: None,
        };
        live.work.spawn(std::future::ready(Work::Members(setup.members, None)));
        live
    }

    async fn run(
        &mut self,
        session: &mut Session,
        controls: &mut Controls,
        info: &mut CallInfo,
        mut live_events: UnboundedReceiver<(Transport, RoomEvent)>,
        mut ended: UnboundedReceiver<media::Ended>,
    ) -> CallEnd {
        if let Some(primary) = session.primary() {
            for participant in primary.remote_participants().into_values() {
                self.arrived(participant.identity().as_str(), &self.cx.own.clone());
            }
        }
        info.phase = CallPhase::Connected;
        info.participants = self.others.participants();
        self.report(info);

        let far = Instant::now() + Duration::from_secs(365 * 24 * 3600);
        let mut restart = tokio::time::interval(rtc::RESTART_EVERY);
        let mut levels = tokio::time::interval(LEVELS_EVERY);
        let mut retry = tokio::time::interval(RETRY_EVERY);
        let mut renew = tokio::time::interval_at(Instant::now() + RENEW_EVERY, RENEW_EVERY);
        // A loop held up (publishing a camera) catches up with one tick, not a burst.
        for timer in [&mut restart, &mut levels, &mut retry, &mut renew] {
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        }
        loop {
            tokio::select! {
                event = live_events.recv() => {
                    let Some((service, event)) = event else { break CallEnd::Left };
                    match self.live_event(session, info, &service, event) {
                        Flow::End(end) => break end,
                        Flow::Quiet => continue,
                        Flow::Report => {}
                    }
                }
                control = controls.next() => {
                    let Some(control) = control else { break CallEnd::HungUp };
                    match control {
                        Control::HangUp => break CallEnd::HungUp,
                        Control::Watch(id, video_sink) => self.sinks.watch(id, video_sink),
                        Control::Unwatch(id) => self.sinks.unwatch(id),
                        // Starting a camera or a share waits on the device and the SFU: a
                        // hang-up meanwhile still ends the call.
                        Control::Toggle(change) => tokio::select! {
                            biased;
                            () = controls.hung_up() => break CallEnd::HungUp,
                            () = self.toggle(session, change, info) => session.use_key_index(self.key.index),
                        },
                        Control::Device(kind, id) => tokio::select! {
                            biased;
                            () = controls.hung_up() => break CallEnd::HungUp,
                            () = self.device(session, kind, id, info) => {}
                        },
                        Control::Signal(Signal::Key { key, .. }) => {
                            if self.cx.encrypted && key.room_id.as_deref() == Some(self.cx.room.room_id().as_str()) {
                                for identity in keys::identities(&key.user_id, &key.device_id, key.member_id.as_deref()) {
                                    let identity = ParticipantIdentity(identity);
                                    self.cx.key_provider.set_key(&identity, i32::from(key.index), key.key.clone());
                                }
                            }
                        }
                        Control::Signal(Signal::Members { .. }) => {
                            let cx = self.cx.clone();
                            self.work.spawn(async move {
                                let (members, own) = cx.members().await;
                                Work::Members(members, own)
                            });
                        }
                        Control::Signal(_) => {}
                    }
                }
                Some(done) = self.work.join_next() => {
                    let Ok(done) = done else { continue };
                    self.done(session, done);
                }
                () = tokio::time::sleep_until(self.rotate_at.unwrap_or(far)), if self.rotate_at.is_some() => {
                    self.rotate_at = None;
                    self.last_rotation = Instant::now();
                    if let Ok(next) = self.key.rotated() {
                        self.send_keys(vec![next.clone()], self.known.clone());
                        self.pending = Some((Instant::now() + USE_KEY_AFTER, next));
                    }
                }
                () = tokio::time::sleep_until(self.pending.as_ref().map_or(far, |(at, _)| *at)), if self.pending.is_some() => {
                    if let (Some((_, next)), Some(primary)) = (self.pending.take(), session.primary()) {
                        let me = primary.local_participant().identity();
                        self.cx.key_provider.set_key(&me, i32::from(next.index), next.key.to_vec());
                        session.use_key_index(next.index);
                        self.key = next;
                    }
                }
                _ = restart.tick() => {
                    if let Some(delay_id) = session.delay_id.clone() {
                        let room = self.cx.room.clone();
                        self.work.spawn(async move { Work::Restarted(rtc::restart(&room, &delay_id).await) });
                    }
                }
                _ = renew.tick() => {
                    self.rejoin(session);
                }
                _ = retry.tick() => {
                    let unsent: HashSet<Device> = self.unsent.drain().filter(|device| self.known.contains(device)).collect();
                    let keys = std::iter::once(self.key.clone()).chain(self.pending.iter().map(|(_, next)| next.clone()));
                    self.send_keys(keys.collect(), unsent);
                    self.connect_missing(session);
                    if self.lost {
                        self.rejoin(session);
                    }
                }
                // A camera or share that stopped on its own stops being sent, and the user hears
                // why.
                Some(capture) = ended.recv() => tokio::select! {
                    biased;
                    () = controls.hung_up() => break CallEnd::HungUp,
                    () = self.capture_ended(session, capture, info) => {}
                },
                _ = levels.tick() => {
                    self.call.sink.emit(Event::CallLevels(levels_of(session)));
                    continue;
                }
            }
            info.participants = self.others.participants();
            self.report(info);
        }
    }

    /// Lets go of what the loop leaves, however it ended (a panic too): work still running
    /// stops, a connection that came in as the call ended closes, and the videos stop.
    async fn finish(mut self) {
        self.work.abort_all();
        while let Some(done) = self.work.join_next().await {
            if let Ok(Work::Connected(_, Ok(link))) = done {
                link.close().await;
            }
        }
        for (_, video) in self.others.videos.drain() {
            video.task.abort();
        }
    }

    /// Reports the call, unless nothing changed since the last report.
    fn report(&mut self, info: &CallInfo) {
        if self.reported.as_ref() != Some(info) {
            self.call.report(info);
            self.reported = Some(info.clone());
        }
    }

    /// Someone on `service`, whose name is then looked up.
    fn arrived(&mut self, identity: &str, service: &Transport) {
        self.others.arrived(identity, service);
        if let Some(user) = self.others.user(identity) {
            self.others.look_up(user, &self.cx.room, &mut self.work);
        }
    }

    fn live_event(
        &mut self,
        session: &mut Session,
        info: &mut CallInfo,
        service: &Transport,
        event: RoomEvent,
    ) -> Flow {
        tracing::trace!(%service, ?event, "call: livekit");
        let own = *service == self.cx.own;
        match event {
            RoomEvent::ParticipantConnected(participant) => self.arrived(participant.identity().as_str(), service),
            RoomEvent::ParticipantDisconnected(participant) => {
                self.others.left(participant.identity().as_str(), service)
            }
            RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), publication, participant } => {
                let id = self.sinks.next_id();
                let owner = participant.identity().to_string();
                let screen = publication.source() == TrackSource::Screenshare;
                let task = tokio::spawn(media::watch_remote(track.rtc_track(), id, self.sinks.clone())).abort_handle();
                let video =
                    RemoteVideo { owner: owner.clone(), service: service.clone(), sid: publication.sid(), task };
                self.others.videos.insert(id, video);
                if !publication.is_muted() {
                    let media = self.others.media.entry(owner).or_default();
                    if screen { media.screen = Some(id) } else { media.camera = Some(id) }
                }
            }
            RoomEvent::TrackSubscribed { publication, participant, .. } => {
                if publication.source() != TrackSource::Microphone {
                    return Flow::Quiet;
                }
                self.others.media.entry(participant.identity().to_string()).or_default().muted = publication.is_muted();
            }
            RoomEvent::TrackUnsubscribed { publication, .. } => match self.others.video(&publication.sid()) {
                Some(id) => self.others.drop_video(id),
                None => return Flow::Quiet,
            },
            RoomEvent::TrackMuted { participant: Participant::Remote(participant), publication } => {
                self.mute_changed(participant.identity().as_str(), &publication, true);
            }
            RoomEvent::TrackUnmuted { participant: Participant::Remote(participant), publication } => {
                self.mute_changed(participant.identity().as_str(), &publication, false);
            }
            // Tracks published again after a reconnect start on the first key.
            RoomEvent::LocalTrackPublished { .. } if own => {
                session.use_key_index(self.key.index);
                return Flow::Quiet;
            }
            RoomEvent::Reconnecting if own => info.phase = CallPhase::Reconnecting,
            RoomEvent::Reconnected if own => info.phase = CallPhase::Connected,
            // The user's own service gone ends the call; another's only loses its people, until
            // it's reconnected.
            RoomEvent::Disconnected { .. } if own => {
                return Flow::End(CallEnd::Failed("the call's connection ended".into()));
            }
            RoomEvent::Disconnected { .. } => {
                if let Some(link) = session.links.remove(service) {
                    tokio::spawn(link.close());
                }
                self.others.left_service(service);
            }
            _ => return Flow::Quiet,
        }
        Flow::Report
    }

    fn mute_changed(&mut self, identity: &str, publication: &livekit::prelude::TrackPublication, muted: bool) {
        let video = self.others.video(&publication.sid());
        let media = self.others.media.entry(identity.to_owned()).or_default();
        match publication.source() {
            TrackSource::Microphone => media.muted = muted,
            TrackSource::Camera => media.camera = if muted { None } else { video },
            TrackSource::Screenshare => media.screen = if muted { None } else { video },
            _ => {}
        }
    }

    fn done(&mut self, session: &mut Session, done: Work) {
        match done {
            Work::Connected(service, result) => {
                self.connecting.remove(&service);
                let Ok(link) = result else { return };
                let wanted = rtc::subscriptions(&self.members, &self.cx.own).contains(&service);
                if !wanted || session.links.contains_key(&service) {
                    tokio::spawn(link.close());
                    return;
                }
                for participant in link.live.remote_participants().into_values() {
                    self.arrived(participant.identity().as_str(), &service);
                }
                session.links.insert(service, link);
            }
            Work::KeysSent(failed) => self.unsent.extend(failed),
            Work::Person(user, person) => {
                self.others.looking.remove(&user);
                self.others.people.insert(user, person);
            }
            Work::Members(members, own) => {
                self.members_changed(session, members);
                if let Some(own) = own {
                    self.since.get_or_insert(own.since);
                    self.seen = true;
                } else if self.seen {
                    // The membership went (the server sent the delayed leave after a stall).
                    self.rejoin(session);
                }
            }
            Work::Restarted(true) => {}
            // The server may have sent the delayed leave already.
            Work::Restarted(false) => self.rejoin(session),
            Work::Rejoined(result) => {
                self.rejoining = false;
                self.seen = false;
                match result {
                    Ok(delay_id) => session.delay_id = delay_id,
                    // Tried again with the next retry.
                    Err(_) => self.lost = true,
                }
            }
        }
    }

    fn members_changed(&mut self, session: &mut Session, members: Vec<Member>) {
        let now: HashSet<Device> = members.iter().map(|m| (m.user_id.clone(), m.device_id.clone())).collect();
        if self.cx.encrypted {
            // Newcomers get the key in use, and the next one if it's on its way.
            let joined: HashSet<Device> = now.difference(&self.known).cloned().collect();
            let keys = std::iter::once(self.key.clone()).chain(self.pending.iter().map(|(_, next)| next.clone()));
            self.send_keys(keys.collect(), joined);
            // Someone left: a new key they never get, once the grace allows.
            if self.known.iter().any(|device| !now.contains(device)) && self.rotate_at.is_none() {
                self.rotate_at = Some((self.last_rotation + ROTATE_GRACE).max(Instant::now()));
            }
        }
        self.unsent.retain(|device| now.contains(device));
        self.known = now;
        self.others.members = members
            .iter()
            .flat_map(|m| {
                keys::identities(&m.user_id, &m.device_id, Some(&m.member_id))
                    .into_iter()
                    .map(|identity| (identity, m.user_id.clone()))
            })
            .collect();
        for member in &members {
            self.others.look_up(member.user_id.clone(), &self.cx.room, &mut self.work);
        }
        self.members = members;
        // Services nobody publishes on any more.
        let wanted = rtc::subscriptions(&self.members, &self.cx.own);
        let unused: Vec<Transport> =
            session.links.keys().filter(|service| !wanted.contains(service)).cloned().collect();
        for service in unused {
            if let Some(link) = session.links.remove(&service) {
                tokio::spawn(link.close());
            }
            self.others.left_service(&service);
        }
        self.connect_missing(session);
    }

    /// Connects to each service someone publishes on that isn't connected (or connecting) yet.
    fn connect_missing(&mut self, session: &Session) {
        for service in rtc::subscriptions(&self.members, &self.cx.own) {
            if service == self.cx.own
                || session.links.contains_key(&service)
                || !self.connecting.insert(service.clone())
            {
                continue;
            }
            let cx = self.cx.clone();
            self.work.spawn(async move {
                let result = cx.connect(service.clone()).await;
                Work::Connected(service, result)
            });
        }
    }

    /// Sends the membership again with a new delayed leave, unless that's under way.
    fn rejoin(&mut self, session: &mut Session) {
        if self.rejoining {
            self.lost = true;
            return;
        }
        (self.rejoining, self.lost) = (true, false);
        let (cx, old, since) = (self.cx.clone(), session.delay_id.take(), self.since);
        self.work.spawn(async move {
            Work::Rejoined(rtc::rejoin(&cx.room, cx.user_id.clone(), &cx.device_id, cx.membership(since), old).await)
        });
    }

    /// Sends `keys` to `devices` beside the loop.
    fn send_keys(&mut self, keys: Vec<OwnKey>, devices: HashSet<Device>) {
        if !self.cx.encrypted || devices.is_empty() {
            return;
        }
        let room = self.cx.room.clone();
        self.work.spawn(async move {
            let mut failed = HashSet::new();
            for key in keys {
                failed.extend(send_key(&room, &key, &devices).await);
            }
            Work::KeysSent(failed)
        });
    }

    async fn device(&mut self, session: &mut Session, kind: DeviceKind, id: Option<String>, info: &mut CallInfo) {
        self.devices.set(kind, id);
        if let Some(audio) = &session.audio {
            media::switch_audio_device(audio, &self.devices, kind);
        }
        let Some(camera) = session.camera.take_if(|_| kind == DeviceKind::Camera) else { return };
        let (camera, result) = switch_camera(camera, &self.sinks, self.devices.camera.clone()).await;
        session.camera = camera;
        if let Err(error) = result {
            // The camera is off now: it stops being sent rather than freezing on its last frame.
            if let Some(live) = session.primary.as_ref().map(|link| &link.live) {
                stop_sharing(live, session.camera.take()).await;
            }
            info.camera = None;
            self.call.failed(&error);
        }
    }

    /// Capture `instance` ended on its own; one that ran before a camera switch is ignored.
    async fn capture_ended(
        &self,
        session: &mut Session,
        media::Ended { instance, unstarted }: media::Ended,
        info: &mut CallInfo,
    ) {
        let Some(live) = session.primary.as_ref().map(|link| &link.live) else { return };
        let is = |shared: &Option<Shared>| shared.as_ref().is_some_and(|shared| shared.capture.instance == instance);
        let stopped = if is(&session.camera) {
            stop_sharing(live, session.camera.take()).await;
            MediaToggle::Camera(false)
        } else if is(&session.screen) {
            stop_sharing(live, session.screen.take()).await;
            MediaToggle::Screen(false)
        } else {
            return;
        };
        // A share whose picker was cancelled just goes: the user chose that.
        if !unstarted {
            self.call.stopped(stopped);
        }
        info.camera = session.camera.as_ref().map(|shared| shared.capture.preview);
        info.screen = session.screen.as_ref().map(|shared| shared.capture.preview);
    }

    async fn toggle(&self, session: &mut Session, toggle: MediaToggle, info: &mut CallInfo) {
        let Some(live) = session.primary.as_ref().map(|link| &link.live) else { return };
        let sinks = &self.sinks;
        let result = match toggle {
            MediaToggle::Microphone(on) => {
                if let Some(microphone) = &session.microphone {
                    if on { microphone.unmute() } else { microphone.mute() }
                }
                info.microphone = on;
                Ok(())
            }
            MediaToggle::Camera(true) if session.camera.is_none() => {
                let capture = media::open_camera(sinks, self.devices.camera.clone()).await;
                share(live, capture, TrackSource::Camera).await.map(|shared| session.camera = Some(shared))
            }
            MediaToggle::Camera(false) => {
                stop_sharing(live, session.camera.take()).await;
                Ok(())
            }
            MediaToggle::Screen(true) if session.screen.is_none() => {
                let sinks = sinks.clone();
                let capture = media::blocking(move || media::start_screen(sinks.clone(), sinks.next_id())).await;
                share(live, capture, TrackSource::Screenshare).await.map(|shared| session.screen = Some(shared))
            }
            MediaToggle::Screen(false) => {
                stop_sharing(live, session.screen.take()).await;
                Ok(())
            }
            MediaToggle::Camera(true) | MediaToggle::Screen(true) => Ok(()),
        };
        if let Err(error) = result {
            self.call.failed(&error);
        }
        info.camera = session.camera.as_ref().map(|shared| shared.capture.preview);
        info.screen = session.screen.as_ref().map(|shared| shared.capture.preview);
    }
}

/// Everyone's audio level, 0–255, by identity (the user's own under the empty key), the loudest
/// connection for someone on several.
fn levels_of(session: &Session) -> Vec<(String, u8)> {
    let level = |level: f32| (level.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut loudest: HashMap<String, u8> = HashMap::new();
    for link in session.links() {
        for participant in link.live.remote_participants().into_values() {
            let entry = loudest.entry(participant.identity().to_string()).or_default();
            *entry = (*entry).max(level(participant.audio_level()));
        }
    }
    let mut list: Vec<(String, u8)> = loudest.into_iter().collect();
    if let Some(primary) = session.primary() {
        list.push((String::new(), level(primary.local_participant().audio_level())));
    }
    list
}

/// Starts a capture and publishes it as `source`.
async fn share(live: &LiveRoom, capture: Result<Capture, CallError>, source: TrackSource) -> Result<Shared, CallError> {
    let capture = capture?;
    let name = if source == TrackSource::Screenshare { "screen" } else { "camera" };
    let track = LocalVideoTrack::create_video_track(name, capture.rtc_source());
    let options = TrackPublishOptions { source, ..TrackPublishOptions::default() };
    live.local_participant().publish_track(LocalTrack::Video(track.clone()), options).await.map_err(media_error)?;
    Ok(Shared { capture, track })
}

async fn stop_sharing(live: &LiveRoom, shared: Option<Shared>) {
    if let Some(shared) = shared {
        let _ = live.local_participant().unpublish_track(&shared.track.sid()).await;
        media::release([shared.capture]).await;
    }
}

/// Hands the user's key to each of `devices` in an encrypted to-device event; returns those it
/// didn't reach.
async fn send_key(room: &Room, key: &OwnKey, devices: &HashSet<Device>) -> HashSet<Device> {
    let client = room.client();
    let (Some(own_device), Some(own_user)) = (client.device_id(), client.user_id()) else { return devices.clone() };
    let sent = matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now().get().into();
    let content = keys::to_device_content(key, own_user.as_str(), own_device.as_str(), room.room_id().as_str(), sent);
    let Ok(raw) = matrix_sdk::ruma::serde::Raw::new(&content) else { return devices.clone() };
    let encryption = client.encryption();
    let mut recipients = Vec::new();
    let mut failed = HashSet::new();
    for device in devices {
        let found = match UserId::parse(device.0.as_str()) {
            Ok(user) => encryption.get_device(&user, device.1.as_str().into()).await.ok().flatten(),
            Err(_) => None,
        };
        match found {
            Some(found) => recipients.push(found),
            None => {
                failed.insert(device.clone());
            }
        }
    }
    if recipients.is_empty() {
        return failed;
    }
    let sent = encryption
        .encrypt_and_send_raw_to_device(
            recipients.iter().collect(),
            keys::KEYS_EVENT,
            raw.cast_unchecked(),
            matrix_sdk_base::crypto::CollectStrategy::AllDevices,
        )
        .await;
    match sent {
        Ok(missed) => failed.extend(missed.into_iter().map(|(user, device)| (user.to_string(), device.to_string()))),
        Err(_) => failed.extend(recipients.iter().map(|d| (d.user_id().to_string(), d.device_id().to_string()))),
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_step_that_stalls_should_fail_naming_itself() {
        let stalled = step("joining", Duration::from_millis(5), std::future::pending::<Result<(), CallError>>());
        assert!(matches!(stalled.await, Err(CallError::Stalled("joining"))));
    }

    #[tokio::test]
    async fn a_step_that_finishes_should_give_its_result() {
        let done = step("joining", Duration::from_secs(1), async { Ok::<_, CallError>(7) });
        assert!(matches!(done.await, Ok(7)));
    }

    fn on(service: &str) -> Transport {
        Transport::Service(service.into())
    }

    fn video(owner: &str, service: &str, sid: &str) -> RemoteVideo {
        let task = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap_or_else(|_| panic!("no runtime"))
            .block_on(async { tokio::spawn(async {}).abort_handle() });
        RemoteVideo {
            owner: owner.into(),
            service: on(service),
            sid: sid.to_owned().try_into().unwrap_or_else(|_| panic!("sid")),
            task,
        }
    }

    #[test]
    fn someone_on_two_services_should_stay_until_gone_from_both() {
        let mut others = Others::default();
        others.arrived("@a:x:D", &on("https://one"));
        others.arrived("@a:x:D", &on("https://two"));
        others.left("@a:x:D", &on("https://one"));
        assert_eq!(others.participants().len(), 1);
        others.left("@a:x:D", &on("https://two"));
        assert!(others.participants().is_empty());
    }

    #[test]
    fn a_connection_that_belongs_to_no_member_should_not_show() {
        let mut others = Others::default();
        others.arrived("c29tZWhhc2g", &on("https://one"));
        assert!(others.participants().is_empty());
        others.members.insert("c29tZWhhc2g".into(), "@a:x".into());
        assert_eq!(others.participants()[0].person.id, "@a:x");
    }

    #[test]
    fn a_service_going_should_take_the_videos_that_came_over_it() {
        let mut others = Others::default();
        others.arrived("@a:x:D", &on("https://one"));
        others.arrived("@a:x:D", &on("https://two"));
        others.videos.insert(VideoId(1), video("@a:x:D", "https://two", "TR_1"));
        others.media.entry("@a:x:D".into()).or_default().camera = Some(VideoId(1));
        others.left_service(&on("https://two"));
        assert_eq!(others.participants()[0].camera, None);
        assert!(others.videos.is_empty());
    }
}
