//! Running a widget ([`Command::RunWidget`](crate::Command::RunWidget)): the app shows its page
//! ([`Event::WidgetPage`]) and passes the page's messages both ways ([`Event::WidgetMessage`],
//! [`Answer::WidgetMessage`]); the run lasts until the app closes the page
//! ([`Command::Cancel`](crate::Command::Cancel)) or the widget is done.
//!
//! - A meeting's page is the meeting itself, with nothing to pass ([`WidgetBridge::None`]).
//! - Any other widget talks the widget API (MSC2762) with the SDK's widget driver. A page in a
//!   web view has no parent window, so it posts to itself: its URL names the widget
//!   (`widgetId`) and its own origin as the parent's (`parentUrl`), and the app's web view
//!   forwards what the page posts from the widget and posts back what the driver answers
//!   (see [`WidgetBridge::WidgetApi`]).
//! - The permissions a widget asks for ([`Event::WidgetPermissions`]) are granted by the user
//!   ([`Answer::WidgetPermissions`]) and remembered for that widget in that room: asking again for
//!   nothing new grants what was granted before, without asking.

use std::sync::Arc;

use matrix_sdk::widget::{Capabilities, CapabilitiesProvider, WidgetDriver, WidgetDriverHandle, WidgetSettings};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::meetings;
use crate::engine::{Answer, Event, EventSink, RequestId};
use crate::{Account, Error};

/// The data a widget asks to put in its URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ffi", derive(ts_rs::TS))]
pub enum WidgetUrlField {
    /// The Matrix user ID.
    UserId,
    /// The Matrix room ID.
    RoomId,
    /// The widget ID.
    WidgetId,
    /// The user's display name.
    DisplayName,
    /// The user's avatar URL.
    AvatarUrl,
    /// The current device ID.
    DeviceId,
    /// The account's homeserver URL.
    Homeserver,
}

/// Which action asks for confirmation before a widget run can proceed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum WidgetAccessKind {
    /// Load the widget's website, with the Matrix context the URL contains.
    Open(Vec<WidgetUrlField>),
    /// Connect to a room's integration manager.
    Manager {
        /// Whether this is the optional hosted Element service.
        hosted: bool,
    },
    /// Inspect the current origin's saved grants and reset its decisions.
    Saved(Vec<WidgetPermission>),
}

/// The context of an action the user must confirm before it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct WidgetAccess {
    /// The widget's name, if applicable.
    pub name: String,
    /// The site's scheme, host and port. No query or fragment.
    pub origin: String,
    /// The signed-in Matrix user.
    pub user_id: String,
    /// The room's name or ID.
    pub room: String,
    /// The action and the data needed to describe it.
    pub kind: WidgetAccessKind,
}

/// Emits a confirmation and waits for an answer to this stage of the run.
pub(crate) async fn confirm_access(
    sink: &dyn EventSink,
    request: RequestId,
    access: WidgetAccess,
    answers: &mut mpsc::UnboundedReceiver<Answer>,
) -> Result<(), Error> {
    sink.emit(Event::WidgetAccess { request, access });
    while let Some(answer) = answers.recv().await {
        if matches!(answer, Answer::Continue) {
            return Ok(());
        }
    }
    Err(Error::Cancelled)
}

fn url_fields(template: &str, viewer: &super::Viewer) -> Vec<WidgetUrlField> {
    let mut fields = Vec::new();
    for (token, field) in [
        ("$matrix_user_id", WidgetUrlField::UserId),
        ("$matrix_room_id", WidgetUrlField::RoomId),
        ("$matrix_widget_id", WidgetUrlField::WidgetId),
        ("$matrix_display_name", name_field(viewer)),
        ("$matrix_avatar_url", WidgetUrlField::AvatarUrl),
        ("$org.matrix.msc3819.matrix_device_id", WidgetUrlField::DeviceId),
        ("$org.matrix.msc4039.matrix_base_url", WidgetUrlField::Homeserver),
    ] {
        if template.contains(token) && !fields.contains(&field) {
            fields.push(field);
        }
    }
    fields
}

fn origin(url: &str) -> Result<String, Error> {
    url::Url::parse(url).map(|url| url.origin().ascii_serialization()).map_err(|_| Error::InvalidWidgetUrl)
}

fn access_for(
    account: &Account,
    viewer: &super::Viewer,
    widget: &super::Widget,
    origin: String,
    kind: WidgetAccessKind,
) -> Result<WidgetAccess, Error> {
    let room = account.room(&viewer.room_id)?;
    let room = room.cached_display_name().map_or_else(|| viewer.room_id.clone(), |name| name.to_string());
    Ok(WidgetAccess { name: widget.name.clone(), origin, user_id: viewer.user_id.clone(), room, kind })
}

/// A widget's page for the app to show.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct WidgetPage {
    /// The page's address.
    pub url: String,
    /// Its title: the widget's name.
    pub title: String,
    /// Which of the page's messages the app passes to the engine.
    pub bridge: WidgetBridge,
}

/// Which of a page's messages the app passes to the engine ([`Answer::WidgetMessage`]), and
/// how it posts the engine's back ([`Event::WidgetMessage`]: `window.postMessage(message, "*")`
/// in the page, the message as an object).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum WidgetBridge {
    /// None: the page needs nothing from the app (a meeting).
    None,
    /// The widget API (MSC2762): messages whose `api` is `fromWidget` and that hold no
    /// `response` (the widget asking), or whose `api` is `toWidget` and that hold one (the
    /// widget answering).
    WidgetApi,
    /// An integration manager's messages: those with an `action` and neither an `api` nor a
    /// `response`.
    Integrations,
}

/// Something a widget asks to be allowed to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct WidgetPermission {
    /// The capability as the widget API names it, granted back as is ([`Answer::WidgetPermissions`]).
    pub capability: String,
    /// What it allows.
    pub kind: PermissionKind,
    /// What it allows it for: an event type (`m.room.message`, `m.room.message#m.text`, a state
    /// type and key), when it names one.
    pub subject: Option<String>,
    /// Other fields sent to a server that receives a meeting identity.
    #[cfg_attr(feature = "ffi", serde(default, skip_serializing_if = "Option::is_none"), ts(optional))]
    pub disclosed: Option<Vec<WidgetUrlField>>,
}

/// What a widget permission allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum PermissionKind {
    /// Seeing the room's events of a type.
    ReadEvents,
    /// Sending events of a type to the room as the user.
    SendEvents,
    /// Seeing the room's state of a type.
    ReadState,
    /// Changing the room's state of a type as the user.
    SendState,
    /// Receiving messages sent to the user's devices.
    ReadToDevice,
    /// Sending messages to devices as the user.
    SendToDevice,
    /// Sending events later, and changing those waiting.
    DelayedEvents,
    /// Downloading the room's files.
    DownloadFiles,
    /// Learning the homeserver's call services.
    CallServices,
    /// Running only inside the app, never opened apart from it.
    StayInApp,
    /// Learning who the user is: a token proving their Matrix ID to the server its subject names
    /// (an origin).
    Identity,
    /// Something the app doesn't know by name.
    Other,
}

/// What a widget permission's capability allows, and for what.
pub(crate) fn describe(capability: &str) -> WidgetPermission {
    const NAMED: [(&str, PermissionKind); 6] = [
        ("org.matrix.msc2762.receive.event:", PermissionKind::ReadEvents),
        ("org.matrix.msc2762.send.event:", PermissionKind::SendEvents),
        ("org.matrix.msc2762.receive.state_event:", PermissionKind::ReadState),
        ("org.matrix.msc2762.send.state_event:", PermissionKind::SendState),
        ("org.matrix.msc3819.receive.to_device:", PermissionKind::ReadToDevice),
        ("org.matrix.msc3819.send.to_device:", PermissionKind::SendToDevice),
    ];
    let (kind, subject) = NAMED
        .iter()
        .find_map(|(prefix, kind)| capability.strip_prefix(prefix).map(|subject| (*kind, Some(subject.to_owned()))))
        .unwrap_or_else(|| {
            let kind = match capability {
                "org.matrix.msc4157.send.delayed_event" | "org.matrix.msc4157.update_delayed_event" => {
                    PermissionKind::DelayedEvents
                }
                "org.matrix.msc4039.download_file" => PermissionKind::DownloadFiles,
                "org.matrix.msc4515.rtc_transports" => PermissionKind::CallServices,
                "io.element.requires_client" => PermissionKind::StayInApp,
                IDENTITY => PermissionKind::Identity,
                _ => PermissionKind::Other,
            };
            (kind, None)
        });
    WidgetPermission { capability: capability.to_owned(), kind, subject, disclosed: None }
}

/// What the user granted a widget before: what it asked for then, and what of it they allowed.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Grants {
    pub asked: Vec<String>,
    pub granted: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meeting_identity: Option<MeetingIdentityDecision>,
}

/// The two meeting flows that can ask for the viewer's identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum MeetingIdentityKind {
    Jitsi,
    Dimension,
}

impl MeetingIdentityKind {
    fn for_place(place: &meetings::Place) -> Option<Self> {
        match place {
            meetings::Place::Jitsi(meetings::Jitsi { auth: true, .. }) => Some(Self::Jitsi),
            meetings::Place::BigBlueButton(meetings::BigBlueButton::Dimension { .. }) => Some(Self::Dimension),
            _ => None,
        }
    }

    fn fields(self, viewer: &super::Viewer) -> Vec<WidgetUrlField> {
        let mut fields = vec![WidgetUrlField::AvatarUrl];
        if name_field(viewer) == WidgetUrlField::DisplayName {
            fields.insert(0, WidgetUrlField::DisplayName);
        }
        if self == Self::Jitsi {
            fields.push(WidgetUrlField::RoomId);
        }
        fields
    }
}

/// The user's decision for a meeting provider and the fields they were shown together.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct MeetingIdentityDecision {
    kind: MeetingIdentityKind,
    fields: Vec<WidgetUrlField>,
    allowed: bool,
}

impl MeetingIdentityDecision {
    fn answer(&self, kind: MeetingIdentityKind, fields: &[WidgetUrlField]) -> Option<bool> {
        (self.kind == kind && fields.iter().all(|field| self.fields.contains(field))).then_some(self.allowed)
    }

    fn disclosure(origin: &str, fields: Vec<WidgetUrlField>) -> WidgetPermission {
        WidgetPermission { disclosed: Some(fields), ..identity(origin) }
    }
}

impl Grants {
    /// The answer to asking for `asked` again: what was granted of it, when it asks for
    /// nothing that wasn't asked before; `None` when the user must be asked.
    pub(crate) fn answer(&self, asked: &[String]) -> Option<Vec<String>> {
        asked
            .iter()
            .all(|capability| self.asked.contains(capability))
            .then(|| asked.iter().filter(|capability| self.granted.contains(capability)).cloned().collect())
    }

    /// Remembers an answer, alongside earlier ones.
    pub(crate) fn remember(&mut self, asked: &[String], granted: &[String]) {
        for capability in asked {
            if !self.asked.contains(capability) {
                self.asked.push(capability.clone());
            }
            let allowed = granted.contains(capability);
            let known = self.granted.iter().position(|kept| kept == capability);
            match (allowed, known) {
                (true, None) => self.granted.push(capability.clone()),
                (false, Some(index)) => {
                    self.granted.remove(index);
                }
                _ => {}
            }
        }
    }
}

/// Where a widget's grants are kept in the account's store: per room, widget and the origin its
/// page is on, so a widget moved to another site asks again. The old colon-delimited keys can
/// name more than one widget, so they are never read.
pub(super) fn grants_key(room_id: &str, widget_id: &str, origin: &str) -> Vec<u8> {
    format!("bettermatrix.widget_grants.v2:{}:{}:{room_id}{widget_id}{origin}", room_id.len(), widget_id.len())
        .into_bytes()
}

/// The canonical origin that would receive the widget's permissions.
fn permission_origin(place: Option<&meetings::Place>, url: &str) -> Result<String, Error> {
    match place {
        Some(meetings::Place::Jitsi(jitsi)) => origin(&format!("https://{}", jitsi.domain)),
        Some(meetings::Place::BigBlueButton(meetings::BigBlueButton::Dimension { api, .. })) => origin(api),
        _ => origin(url),
    }
}

impl Account {
    async fn grants(&self, key: &[u8]) -> Grants {
        let stored = self.client().state_store().get_custom_value(key).await;
        stored.ok().flatten().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    async fn keep_grants(&self, key: &[u8], grants: &Grants) {
        if let Ok(bytes) = serde_json::to_vec(grants) {
            // Not kept, the user is asked again next time.
            let _ = self.client().state_store().set_custom_value_no_read(key, bytes).await;
        }
    }
}

/// Shows saved grants for this widget's current origin, then clears the saved decisions on confirmation.
pub(crate) async fn manage_permissions(
    account: Account,
    room_id: String,
    widget_id: String,
    request: RequestId,
    sink: Arc<dyn EventSink>,
    mut answers: mpsc::UnboundedReceiver<Answer>,
) -> Result<(), Error> {
    let (widget, content, viewer) = account.widget(&room_id, &widget_id, super::Freshness::Server).await?;
    let template = content["url"].as_str().unwrap_or_default();
    let place = meetings::place(&widget.kind, template, &content["data"]);
    let origin = permission_origin(place.as_ref(), &widget.url)?;
    let key = grants_key(&room_id, &widget_id, &origin);
    let saved = account.grants(&key).await;
    let permissions = match place.as_ref() {
        Some(place) => saved
            .meeting_identity
            .filter(|saved| saved.allowed && MeetingIdentityKind::for_place(place) == Some(saved.kind))
            .map(|saved| MeetingIdentityDecision::disclosure(&origin, saved.fields))
            .into_iter()
            .collect(),
        None => {
            saved.granted.iter().map(|name| if name == IDENTITY { identity(&origin) } else { describe(name) }).collect()
        }
    };
    let access = access_for(&account, &viewer, &widget, origin, WidgetAccessKind::Saved(permissions))?;
    confirm_access(&*sink, request, access, &mut answers).await?;
    let store = account.client().state_store();
    store.remove_custom_value(&key).await.map_err(|error| Error::Account(error.to_string()))?;
    Ok(())
}

/// The URL a widget-API page opens at: the widget's, naming the widget and, as the parent it
/// posts to, its own origin (a page in a web view posts to itself).
pub(crate) fn api_url(url: &str, widget_id: &str) -> Option<String> {
    let mut parsed = url::Url::parse(url).ok()?;
    let origin = parsed.origin().ascii_serialization();
    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(key, _)| key != "widgetId" && key != "parentUrl")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    {
        let mut query = parsed.query_pairs_mut();
        query.clear();
        for (key, value) in &kept {
            query.append_pair(key, value);
        }
        query.append_pair("widgetId", widget_id).append_pair("parentUrl", &origin);
    }
    Some(parsed.to_string())
}

/// Asks the run loop for the user's answer to a widget's permissions.
struct Asker(mpsc::UnboundedSender<(Capabilities, oneshot::Sender<Capabilities>)>);

impl CapabilitiesProvider for Asker {
    async fn acquire_capabilities(&self, capabilities: Capabilities) -> Capabilities {
        let (reply, answer) = oneshot::channel();
        if self.0.send((capabilities, reply)).is_err() {
            return Capabilities::default();
        }
        answer.await.unwrap_or_default()
    }
}

/// The capabilities, as the widget API names them.
fn names(capabilities: &Capabilities) -> Vec<String> {
    serde_json::to_value(capabilities)
        .ok()
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

/// A widget's permission to learn who the user is (the widget API's `get_openid`, an OpenID
/// token), which the SDK would grant on its own: the user is asked like for the others.
pub(crate) const IDENTITY: &str = "io.bettermatrix.identity";

/// How long an answer to a widget's permissions can take and still reach the driver: its
/// deadline, less a margin for the way there. A later answer is kept, and the page loaded again.
const PATIENCE: std::time::Duration = DRIVER_DEADLINE.saturating_sub(std::time::Duration::from_secs(2));

/// Asking the user, for one run, before a meeting opens.
struct Ask<'a> {
    account: &'a Account,
    room_id: &'a str,
    widget_id: &'a str,
    request: RequestId,
    sink: &'a Arc<dyn EventSink>,
}

impl Ask<'_> {
    /// Whether the user lets `origin` learn who they are: as answered before for it, else asked
    /// now (and kept).
    async fn identity(
        &self,
        origin: &str,
        kind: MeetingIdentityKind,
        fields: Vec<WidgetUrlField>,
        widget: &str,
        answers: &mut mpsc::UnboundedReceiver<Answer>,
    ) -> Result<bool, Error> {
        let key = grants_key(self.room_id, self.widget_id, origin);
        let mut grants = self.account.grants(&key).await;
        if let Some(answer) = grants.meeting_identity.as_ref().and_then(|saved| saved.answer(kind, &fields)) {
            return Ok(answer);
        }
        let permissions = vec![MeetingIdentityDecision::disclosure(origin, fields.clone())];
        // The run's only question.
        let question = 1;
        self.sink.emit(Event::WidgetPermissions {
            request: self.request,
            question,
            widget: widget.to_owned(),
            permissions,
        });
        let granted = loop {
            match answers.recv().await {
                Some(Answer::WidgetPermissions { question: answered, granted }) if answered == question => {
                    break granted;
                }
                Some(_) => {}
                None => return Err(Error::Cancelled),
            }
        };
        let allowed = granted.iter().any(|name| name == IDENTITY);
        grants.meeting_identity = Some(MeetingIdentityDecision { kind, fields, allowed });
        self.account.keep_grants(&key, &grants).await;
        Ok(allowed)
    }
}

/// A question to the user a widget is waiting on.
enum Question {
    /// The permissions it asked the driver for, asked when.
    Capabilities { names: Vec<String>, reply: oneshot::Sender<Capabilities>, at: std::time::Instant },
    /// Learning who the user is: the page's request, held until answered.
    Identity { message: String },
}

/// How long a widget that says it will announce itself (`content_loaded`) gets to do so once
/// its page loaded; then the driver asks first. A web view may inject the bridge after the page
/// announced itself (Android's injection comes as the page starts, not before it).
const ANNOUNCE: std::time::Duration = std::time::Duration::from_secs(5);

/// How long a slow widget, its page loaded again for it, gets to announce itself.
const SLOW_ANNOUNCE: std::time::Duration = std::time::Duration::from_secs(20);

/// How long an announcement waits for the answer to the fallback's question, before taking the
/// question as unheard.
const ANSWER_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// The SDK driver's deadline for an answer: one that comes later is dropped.
const DRIVER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// Whether the driver asked a widget first, before it announced itself, and is waiting for its
/// answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Fallback {
    /// It didn't ask first.
    #[default]
    No,
    /// It asked, then; no answer yet. Its request's ID, once seen going to the page.
    Asked { at: std::time::Instant, id: Option<String> },
}

/// What a run does next about the widget's start.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Nothing.
    Nothing,
    /// Wait a moment ([`ANSWER_GRACE`]) for the fallback's answer, then [`Handshake::grace_over`].
    Hold,
    /// Load the page again: the widget spent its one answer on a driver that dropped it.
    Reload,
    /// Start a new driver and give it the widget's announcement: the fallback's question went
    /// unheard.
    Restart(String),
}

/// How a widget that says it will announce itself (`waitForIframeLoad: false`) starts, when the
/// driver may have asked first ([`ANNOUNCE`]): the widget answers the driver's first question only
/// once per page, so whichever of its answer and its announcement comes first, the driver that
/// asked must hear the answer, and one that never will is replaced.
#[derive(Debug, Default)]
pub(crate) struct Handshake {
    fallback: Fallback,
    /// The announcement held while the fallback's answer may still come.
    held: Option<String>,
    /// The fallback's request, once a new driver replaced the one that asked it: an answer to it
    /// reaches no driver.
    superseded: Option<String>,
}

impl Handshake {
    /// The driver asked first, at `now`.
    pub(crate) fn asked(&mut self, now: std::time::Instant) {
        *self = Self { fallback: Fallback::Asked { at: now, id: None }, ..Self::default() };
    }

    /// A new document: its start begins afresh.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// The driver posted `message` to the page: the fallback's question, once, names its request.
    pub(crate) fn posted(&mut self, message: &Value) {
        if let Fallback::Asked { id: id @ None, .. } = &mut self.fallback
            && message["api"] == "toWidget"
            && message["action"] == "capabilities"
            && message.get("response").is_none()
        {
            *id = message["requestId"].as_str().map(str::to_owned);
        }
    }

    /// The page posted `message` (`raw`, as posted) at `now`.
    pub(crate) fn heard(&mut self, message: &Value, raw: &str, now: std::time::Instant) -> Step {
        let answers = |id: &str| {
            message["api"] == "toWidget"
                && message["action"] == "capabilities"
                && message.get("response").is_some()
                && message["requestId"].as_str() == Some(id)
        };
        // An answer that reaches no driver (it asked too long ago, or was replaced): the widget
        // spent its one answer, so the page loads again.
        if self.superseded.as_deref().is_some_and(answers) {
            *self = Self::default();
            return Step::Reload;
        }
        // The widget's answer to the fallback's question, matched by its request.
        if let Fallback::Asked { at, id: Some(id) } = &self.fallback
            && answers(id)
        {
            // The driver drops an answer that comes after its deadline.
            if now.duration_since(*at) >= DRIVER_DEADLINE {
                *self = Self::default();
                return Step::Reload;
            }
            // In time: the driver goes on, an announcement held for it with it.
            self.fallback = Fallback::No;
            self.held = None;
            return Step::Nothing;
        }
        if !announces(message) {
            return Step::Nothing;
        }
        match self.fallback {
            // Unanswered yet: the answer may be on its way (the widget may have heard the
            // question). The announcement waits a moment; the first driver hears it too.
            Fallback::Asked { .. } => {
                self.held = Some(raw.to_owned());
                Step::Hold
            }
            Fallback::No => Step::Nothing,
        }
    }

    /// The moment an announcement waited for is over: no answer came, so the question went
    /// unheard.
    pub(crate) fn grace_over(&mut self) -> Step {
        match self.held.take() {
            Some(announcement) if matches!(self.fallback, Fallback::Asked { .. }) => {
                let fallback = std::mem::take(&mut self.fallback);
                if let Fallback::Asked { id, .. } = fallback {
                    self.superseded = id;
                }
                Step::Restart(announcement)
            }
            _ => Step::Nothing,
        }
    }
}

/// Whether a page message is the widget announcing it is ready.
fn announces(message: &Value) -> bool {
    message["api"] == "fromWidget" && message["action"] == "content_loaded" && message.get("response").is_none()
}

impl Question {
    fn names(&self) -> Vec<String> {
        match self {
            Self::Capabilities { names, .. } => names.clone(),
            Self::Identity { .. } => vec![IDENTITY.to_owned()],
        }
    }
}

/// The SDK's widget driver running for one page.
struct Driver {
    run: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ()>> + Send>>,
    handle: WidgetDriverHandle,
    asked: mpsc::UnboundedReceiver<(Capabilities, oneshot::Sender<Capabilities>)>,
}

impl Driver {
    fn start(settings: &WidgetSettings, room: &matrix_sdk::Room) -> Self {
        let (driver, handle) = WidgetDriver::new(settings.clone());
        let (asks, asked) = mpsc::unbounded_channel();
        Self { run: Box::pin(driver.run(room.clone(), Asker(asks))), handle, asked }
    }
}

/// The Identity permission, naming the origin that would learn who the user is.
pub(crate) fn identity(origin: &str) -> WidgetPermission {
    WidgetPermission { subject: Some(origin.to_owned()), ..describe(IDENTITY) }
}

/// The field that supplies the name, including the fallback when the account has no name.
fn name_field(viewer: &super::Viewer) -> WidgetUrlField {
    if viewer.display_name == viewer.user_id { WidgetUrlField::UserId } else { WidgetUrlField::DisplayName }
}

/// Whether a page message is the widget asking who the user is.
fn asks_identity(message: &Value) -> bool {
    message["api"] == "fromWidget" && message["action"] == "get_openid" && message.get("response").is_none()
}

/// The widget API's answer refusing a request: the request, with the response.
pub(crate) fn refused(message: &Value) -> String {
    let mut reply = message.clone();
    reply["response"] = serde_json::json!({ "state": "blocked" });
    reply.to_string()
}

/// Runs a widget for the app ([`Command::RunWidget`](crate::Command::RunWidget)): in the
/// browser (`browser`, [`Event::OpenUrl`]), or in the app's web view until it closes.
/// `freshness` is how fresh the widget's state must be to find it (a meeting just started is
/// on the server before sync brings it).
#[expect(clippy::too_many_arguments, reason = "one run's inputs, each used once")]
pub(crate) async fn run(
    account: Account,
    room_id: String,
    widget_id: String,
    browser: bool,
    freshness: super::Freshness,
    request: RequestId,
    sink: Arc<dyn EventSink>,
    mut answers: mpsc::UnboundedReceiver<Answer>,
) -> Result<(), Error> {
    let (widget, content, viewer) = account.widget(&room_id, &widget_id, freshness).await?;
    let template = content.get("url").and_then(Value::as_str).unwrap_or_default();
    let data = content.get("data").cloned().unwrap_or(Value::Null);
    if let Some(place) = meetings::place(&widget.kind, template, &data) {
        let permission_site = permission_origin(Some(&place), &widget.url)?;
        if let meetings::Place::Jitsi(jitsi) = &place {
            let mut fields = vec![name_field(&viewer)];
            // A checking server's conference can name the room even without an OpenID token.
            if jitsi.conference.eq_ignore_ascii_case(&meetings::base32(room_id.as_bytes())) {
                fields.push(WidgetUrlField::RoomId);
            }
            let access =
                access_for(&account, &viewer, &widget, permission_site.clone(), WidgetAccessKind::Open(fields))?;
            confirm_access(&*sink, request, access, &mut answers).await?;
        }
        // Who learns the user's Matrix ID by joining: a Jitsi server that checks who joins, or
        // Dimension making the join link. Only if they let it, named by that origin.
        let identity = if let Some(kind) = MeetingIdentityKind::for_place(&place) {
            let ask = Ask { account: &account, room_id: &room_id, widget_id: &widget_id, request, sink: &sink };
            ask.identity(&permission_site, kind, kind.fields(&viewer), &widget.name, &mut answers).await?
        } else {
            false
        };
        let url = account.join_url(&place, &viewer, identity).await?;
        if matches!(place, meetings::Place::BigBlueButton(_)) {
            let access = access_for(&account, &viewer, &widget, origin(&url)?, WidgetAccessKind::Open(Vec::new()))?;
            confirm_access(&*sink, request, access, &mut answers).await?;
        }
        if browser || !url.starts_with("https://") || matches!(place, meetings::Place::Hosted { .. }) {
            sink.emit(Event::OpenUrl { request, url });
            return Ok(());
        }
        let page = WidgetPage { url, title: widget.name, bridge: WidgetBridge::None };
        sink.emit(Event::WidgetPage { request, page });
        // Open until the app closes it.
        while answers.recv().await.is_some() {}
        return Ok(());
    }
    let origin = origin(&widget.url)?;
    let bridged = !browser && widget.url.starts_with("https://");
    let mut fields = url_fields(template, &viewer);
    if bridged && !fields.contains(&WidgetUrlField::WidgetId) {
        fields.push(WidgetUrlField::WidgetId);
    }
    let access = access_for(&account, &viewer, &widget, origin.clone(), WidgetAccessKind::Open(fields))?;
    confirm_access(&*sink, request, access, &mut answers).await?;
    // The widget API only over HTTPS: on HTTP, whoever is on the way could stand in for the page
    // and use what the user granted it. Such a widget opens in the browser.
    if !bridged {
        sink.emit(Event::OpenUrl { request, url: widget.url });
        return Ok(());
    }
    let url = api_url(&widget.url, &widget_id).ok_or(Error::InvalidWidgetUrl)?;
    // Element starts talking once the page loaded, unless the widget says it will say when it
    // is ready (`waitForIframeLoad: false`, then `content_loaded`). Widgets say so beside their
    // URL; some older ones in their data.
    let says_when_ready = [content.get("waitForIframeLoad"), data.get("waitForIframeLoad")]
        .into_iter()
        .flatten()
        .any(|wait| wait.as_bool() == Some(false));
    let settings =
        WidgetSettings::new(widget_id.clone(), says_when_ready, &url).map_err(|_| Error::InvalidWidgetUrl)?;
    // Started again, the driver asks at once: the page loaded, and said it was ready, before.
    let restarted = WidgetSettings::new(widget_id.clone(), false, &url).map_err(|_| Error::InvalidWidgetUrl)?;
    let room = account.room(&room_id)?;
    let key = grants_key(&room_id, &widget_id, &origin);
    let mut grants = account.grants(&key).await;
    let page = WidgetPage { url, title: widget.name.clone(), bridge: WidgetBridge::WidgetApi };
    sink.emit(Event::WidgetPage { request, page: page.clone() });
    // The driver starts once the page loaded (its first request would wait for nothing and
    // expire); what the page posts before then waits for it.
    let mut driver: Option<Driver> = None;
    let mut early: Vec<String> = Vec::new();
    // Each question numbered: an answer counts only for the question it answers.
    let mut questions: std::collections::VecDeque<(u64, Question)> = std::collections::VecDeque::new();
    let mut asked_count = 0u64;
    let ask = |(number, question): &(u64, Question)| {
        let permissions = question
            .names()
            .iter()
            .map(|name| if name == IDENTITY { identity(&origin) } else { describe(name) })
            .collect();
        sink.emit(Event::WidgetPermissions { request, question: *number, widget: widget.name.clone(), permissions });
    };
    // Until the widget announces itself, once its page loaded and it said it would.
    let mut announce_by: Option<tokio::time::Instant> = None;
    // How the widget starts, when the driver asked first.
    let mut handshake = Handshake::default();
    // After a reload because the widget answered too late: it is slow, so it gets longer to
    // announce itself before the driver asks first.
    let mut patient = false;
    // Whether the run already loaded the page again for that.
    let mut reloaded = false;
    // When an announcement held for the fallback's answer stops waiting.
    let mut grace_by: Option<tokio::time::Instant> = None;
    loop {
        let (run, handle, asked) = match driver.as_mut() {
            Some(Driver { run, handle, asked }) => (Some(run), Some(&*handle), Some(asked)),
            None => (None, None, None),
        };
        tokio::select! {
            _ = async { match run { Some(run) => run.await, None => std::future::pending().await } } => break,
            Some(message) = async { match handle { Some(handle) => handle.recv().await, None => std::future::pending().await } } => {
                handshake.posted(&serde_json::from_str(&message).unwrap_or(Value::Null));
                sink.emit(Event::WidgetMessage { request, message });
            }
            Some((capabilities, reply)) = async { match asked { Some(asked) => asked.recv().await, None => std::future::pending().await } } => {
                let names = names(&capabilities);
                if let Some(granted) = grants.answer(&names) {
                    let _ = reply.send(granted_capabilities(&granted));
                } else {
                    asked_count += 1;
                    let question = (asked_count, Question::Capabilities { names, reply, at: std::time::Instant::now() });
                    if questions.is_empty() {
                        ask(&question);
                    }
                    questions.push_back(question);
                }
            }
            // It didn't (its announcement came before the bridge): the driver asks first.
            () = async { match announce_by { Some(at) => tokio::time::sleep_until(at).await, None => std::future::pending().await } } => {
                announce_by = None;
                handshake.asked(std::time::Instant::now());
                driver = Some(Driver::start(&restarted, &room));
            }
            // No answer to the fallback's question came after all: it went unheard, so the widget's
            // announcement starts the widget API over.
            () = async { match grace_by { Some(at) => tokio::time::sleep_until(at).await, None => std::future::pending().await } } => {
                grace_by = None;
                let Step::Restart(announcement) = handshake.grace_over() else { continue };
                let started = Driver::start(&settings, &room);
                started.handle.send(announcement);
                driver = Some(started);
                // The first driver's question goes with it: the new one asks again.
                if let Some((number, Question::Capabilities { .. })) = questions.front() {
                    let (question, permissions) = (*number, Vec::new());
                    let widget = widget.name.clone();
                    sink.emit(Event::WidgetPermissions { request, question, widget, permissions });
                }
                questions.retain(|(_, question)| !matches!(question, Question::Capabilities { .. }));
            }
            answer = answers.recv() => match answer {
                None => break,
                // A new document (a reload too): the old one's widget API is done, and what the new
                // one posts waits for its own.
                // Its questions go with it: withdrawn, so an answer meant for them can't answer
                // the new document's.
                Some(Answer::WidgetLoading) => {
                    driver = None;
                    announce_by = None;
                    grace_by = None;
                    handshake.reset();
                    early.clear();
                    if let Some((number, _)) = questions.front() {
                        let (question, permissions) = (*number, Vec::new());
                        sink.emit(Event::WidgetPermissions { request, question, widget: widget.name.clone(), permissions });
                    }
                    questions.clear();
                }
                // Each load starts the widget API afresh: the page starts over.
                Some(Answer::WidgetLoaded) => {
                    let announced = early.iter().any(|message| announces(&serde_json::from_str(message).unwrap_or(Value::Null)));
                    // A slow widget, reloaded for it, gets longer to announce itself.
                    let wait = if std::mem::take(&mut patient) { SLOW_ANNOUNCE } else { ANNOUNCE };
                    announce_by = (says_when_ready && !announced).then(|| tokio::time::Instant::now() + wait);
                    let started = Driver::start(&settings, &room);
                    for message in early.drain(..) {
                        started.handle.send(message);
                    }
                    driver = Some(started);
                }
                Some(Answer::WidgetMessage(message)) => {
                    let parsed: Value = serde_json::from_str(&message).unwrap_or(Value::Null);
                    if announces(&parsed) {
                        announce_by = None;
                    }
                    match handshake.heard(&parsed, &message, std::time::Instant::now()) {
                        // Once a run: a widget that misses its start again goes on with its driver.
                        Step::Reload if !reloaded => {
                            reloaded = true;
                            // It is slow: next time it gets longer to announce itself.
                            patient = true;
                            driver = None;
                            early.clear();
                            sink.emit(Event::WidgetPage { request, page: page.clone() });
                            continue;
                        }
                        Step::Hold => grace_by = Some(tokio::time::Instant::now() + ANSWER_GRACE),
                        Step::Nothing | Step::Restart(_) | Step::Reload => {}
                    }
                    if asks_identity(&parsed) {
                        match grants.answer(&[IDENTITY.to_owned()]) {
                            Some(granted) if granted.is_empty() => {
                                sink.emit(Event::WidgetMessage { request, message: refused(&parsed) });
                                continue;
                            }
                            Some(_) => {}
                            None => {
                                asked_count += 1;
                                let question = (asked_count, Question::Identity { message });
                                if questions.is_empty() {
                                    ask(&question);
                                }
                                questions.push_back(question);
                                continue;
                            }
                        }
                    }
                    match &driver {
                        Some(driver) => {
                            driver.handle.send(message);
                        }
                        None => early.push(message),
                    }
                }
                Some(Answer::WidgetPermissions { question: answered, granted }) => {
                    // Only the question showing: a late answer to a withdrawn one is dropped.
                    if questions.front().is_none_or(|(number, _)| *number != answered) {
                        continue;
                    }
                    let Some((_, question)) = questions.pop_front() else { continue };
                    let names = question.names();
                    let granted: Vec<String> = granted.into_iter().filter(|name| names.contains(name)).collect();
                    grants.remember(&names, &granted);
                    account.keep_grants(&key, &grants).await;
                    match question {
                        Question::Capabilities { reply, at, .. } if at.elapsed() < PATIENCE => {
                            let _ = reply.send(granted_capabilities(&granted));
                        }
                        // The driver gave up waiting, and the page answers the question only once: it
                        // loads again, and its new driver finds the answer kept.
                        Question::Capabilities { .. } => {
                            driver = None;
                            early.clear();
                            sink.emit(Event::WidgetPage { request, page: page.clone() });
                        }
                        Question::Identity { message } if granted.is_empty() => {
                            let parsed: Value = serde_json::from_str(&message).unwrap_or(Value::Null);
                            sink.emit(Event::WidgetMessage { request, message: refused(&parsed) });
                        }
                        Question::Identity { message } => match &driver {
                            Some(driver) => {
                                driver.handle.send(message);
                            }
                            None => early.push(message),
                        },
                    }
                    if let Some(next) = questions.front() {
                        ask(next);
                    }
                }
                Some(_) => {}
            },
        }
    }
    Ok(())
}
/// The capabilities named, as the driver takes them; names it doesn't know are left out.
fn granted_capabilities(names: &[String]) -> Capabilities {
    serde_json::from_value(Value::from(names.to_vec())).unwrap_or_default()
}
