//! A room's widgets: web apps pinned to a room as state (`im.vector.modular.widgets`, the type
//! Element writes, and `m.widget`), keyed by the widget's ID.
//!
//! - Listing, adding and removing them (here), with the URL's template variables filled in
//!   (`$matrix_user_id`, …).
//! - Meetings ([`meetings`]): Jitsi and BigBlueButton widgets, which open their meeting itself
//!   rather than the page Element wraps it in, and starting a Jitsi meeting.
//! - Running one ([`session`], [`Command::RunWidget`](crate::Command::RunWidget)): the app shows
//!   the page in a web view and passes its messages to the SDK's widget driver (MSC2762), which
//!   reads and sends events for it within the permissions the user grants.
//! - Integration managers ([`integrations`]): the page that adds bots, bridges and widgets,
//!   talking to the app through Element's "scalar" messages.
//!
//! The room list's sliding sync asks for a fixed set of state that leaves widgets out, so a
//! room's widgets also come from its state on the server, read once a run per room (and again
//! whenever the list is asked for afresh); what sync brought since wins for the same widget.

pub mod integrations;
pub mod meetings;
pub mod session;

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState;
use matrix_sdk::ruma::events::StateEventType;
use serde_json::{Value, json};

use crate::Error;
use crate::account::Account;
pub use meetings::Meeting;

/// The state type Element writes widgets as, and the one this app writes.
pub(crate) const LEGACY_TYPE: &str = "im.vector.modular.widgets";
/// A proposed state type, read for compatibility.
pub(crate) const SPEC_TYPE: &str = "m.widget";
/// Both, as the room's state holds them.
pub(crate) const TYPES: [&str; 2] = [LEGACY_TYPE, SPEC_TYPE];

/// A widget pinned to a room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Widget {
    /// Its ID (the state key).
    pub id: String,
    /// Its name, or its kind when it has none.
    pub name: String,
    /// What it is: `m.custom`, `jitsi`, `m.etherpad`, `m.grafana`, …
    pub kind: String,
    /// Where it opens in a browser, its template variables filled in for this user. A meeting's
    /// is the meeting itself, without what joining adds (a Jitsi server's sign-in, a
    /// BigBlueButton meeting's join link): open it with [`Command::RunWidget`](crate::Command::RunWidget).
    pub url: String,
    /// The host it opens on, to show before opening.
    pub host: String,
    /// Who added it.
    pub creator: Option<String>,
    /// Whether the user's power level lets them remove it, under every type it is pinned as.
    pub can_remove: bool,
    /// The video meeting it holds, which the room offers to join.
    pub meeting: Option<Meeting>,
}

/// A room's widgets, and whether the user may add one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Widgets {
    /// By name.
    pub widgets: Vec<Widget>,
    /// Whether the user's power level lets them add a widget (as the type Element writes).
    pub can_add: bool,
    /// Matrix IDs explicitly marked as providing a service in this room.
    #[cfg_attr(feature = "ffi", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub services: Vec<String>,
    /// Connections advertised by bridges in room state.
    #[cfg_attr(feature = "ffi", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub bridges: Vec<RoomBridge>,
}

/// An external destination advertised in a room's bridge state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomBridge {
    /// The bridge's state key.
    pub id: String,
    /// The external service.
    pub protocol: String,
    /// The external channel or conversation.
    pub channel: String,
    /// The network or workspace, when supplied.
    pub network: Option<String>,
}

/// Who a widget's template variables describe.
#[derive(Debug, Default)]
pub(crate) struct Viewer {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: String,
    pub device_id: String,
    pub room_id: String,
    pub homeserver: String,
}

/// How fresh a room's widgets must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Freshness {
    /// From the server now: the list asked for, or a change just written.
    Server,
    /// What this device knows: the store, and the server's state read once this run.
    Known,
}

/// One widget state event that holds something: its type, state key, content and sender.
pub(crate) type WidgetState = (&'static str, String, Value, Option<String>);

impl Account {
    /// The room's widgets, as the server has them now.
    pub async fn widgets(&self, room_id: &str) -> Result<Widgets, Error> {
        self.room_widgets(room_id, Freshness::Server).await
    }

    /// The room's widgets, as fresh as asked.
    pub(crate) async fn room_widgets(&self, room_id: &str, freshness: Freshness) -> Result<Widgets, Error> {
        let room = self.room(room_id)?;
        let me = self.user_id()?;
        let viewer = self.viewer(&room).await;
        let state = self.widget_state(&room, freshness).await?;
        let levels = room.power_levels().await.ok();
        let may_send = |kind: &str| {
            levels.as_ref().is_some_and(|levels| levels.user_can_send_state(&me, StateEventType::from(kind)))
        };
        let mut found: Vec<(&str, &Value, Option<String>)> = Vec::new();
        for (_, state_key, content, sender) in &state {
            if !found.iter().any(|(id, ..)| *id == state_key.as_str()) {
                found.push((state_key, content, sender.clone()));
            }
        }
        let mut widgets: Vec<Widget> = found
            .into_iter()
            .filter_map(|(id, content, sender)| {
                let widget = widget(id, content, sender, &viewer)?;
                // Removing writes every type the widget is pinned as.
                let can_remove = state.iter().filter(|(_, key, ..)| key == id).all(|(kind, ..)| may_send(kind));
                Some(Widget { can_remove, ..widget })
            })
            .collect();
        widgets.sort_by_cached_key(|widget| widget.name.to_lowercase());
        let (services, bridges) = room_integrations(&self.extra_state(&room).await);
        Ok(Widgets { widgets, can_add: may_send(LEGACY_TYPE), services, bridges })
    }

    /// One of the room's widgets, its state content, and who its variables describe.
    pub(crate) async fn widget(
        &self,
        room_id: &str,
        widget_id: &str,
        freshness: Freshness,
    ) -> Result<(Widget, Value, Viewer), Error> {
        let room = self.room(room_id)?;
        let viewer = self.viewer(&room).await;
        let state = self.widget_state(&room, freshness).await?;
        let (_, id, content, sender) =
            state.into_iter().find(|(_, id, ..)| id == widget_id).ok_or(Error::WidgetGone)?;
        let found = widget(&id, &content, sender, &viewer).ok_or(Error::WidgetGone)?;
        Ok((found, content, viewer))
    }

    /// Who a widget's template variables describe in `room`.
    pub(crate) async fn viewer(&self, room: &Room) -> Viewer {
        let me = self.client().user_id().map(ToString::to_string).unwrap_or_default();
        let member = match self.client().user_id() {
            Some(me) => room.get_member_no_sync(me).await.ok().flatten(),
            None => None,
        };
        Viewer {
            display_name: member
                .as_ref()
                .and_then(|m| m.display_name().map(str::to_owned))
                .unwrap_or_else(|| me.clone()),
            avatar_url: member.as_ref().and_then(|m| m.avatar_url().map(ToString::to_string)).unwrap_or_default(),
            user_id: me,
            device_id: self.client().device_id().map(ToString::to_string).unwrap_or_default(),
            room_id: room.room_id().to_string(),
            homeserver: self.client().homeserver().to_string().trim_end_matches('/').to_owned(),
        }
    }

    /// The room's widgets after a change was written, read up to three times, a second apart:
    /// the change stands even when the list can't be read.
    pub(crate) async fn widgets_after_write(&self, room_id: &str) -> Option<Widgets> {
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            if let Ok(widgets) = self.widgets(room_id).await {
                return Some(widgets);
            }
        }
        None
    }

    /// Pins a web page to the room as a custom widget.
    pub async fn add_widget(&self, room_id: &str, name: &str, url: &str) -> Result<(), Error> {
        let room = self.room(room_id)?;
        let parsed = url::Url::parse(url.trim()).map_err(|_| Error::InvalidWidgetUrl)?;
        if !matches!(parsed.scheme(), "https" | "http") {
            return Err(Error::InvalidWidgetUrl);
        }
        let id = format!("{}_{}", self.user_id()?, crate::engine::now_ms());
        let name = if name.trim().is_empty() { parsed.host_str().unwrap_or_default() } else { name.trim() };
        let content = json!({
            "id": id,
            "type": "m.custom",
            "url": url.trim(),
            "name": name,
            "data": {},
            "creatorUserId": self.user_id()?.as_str(),
        });
        room.send_state_event_raw(LEGACY_TYPE, &id, content).await?;
        Ok(())
    }

    /// Takes a widget off the room, under every type it is pinned as.
    pub async fn remove_widget(&self, room_id: &str, widget_id: &str) -> Result<(), Error> {
        let room = self.room(room_id)?;
        for (kind, id, ..) in self.widget_state(&room, Freshness::Server).await? {
            if id == widget_id {
                room.send_state_event_raw(kind, widget_id, json!({})).await?;
            }
        }
        Ok(())
    }

    /// A room's widget state events that still hold something.
    pub(crate) async fn widget_state(&self, room: &Room, freshness: Freshness) -> Result<Vec<WidgetState>, Error> {
        let server = match freshness {
            // The server's state as it resolved it: no cached copy outranks it (senders' clocks
            // can put an old event after its removal).
            Freshness::Server => {
                let resolved = self.read_extra_state(room).await?;
                let mut widgets: Vec<_> = resolved
                    .iter()
                    .filter(|event| TYPES.iter().any(|kind| event["type"] == *kind))
                    .filter_map(read_state)
                    .collect();
                // Match the store's order when the same ID is pinned under both state types.
                widgets.sort_by_key(|(kind, ..)| *kind != LEGACY_TYPE);
                return Ok(widgets);
            }
            // Without sliding sync, the store holds every widget sync brought.
            Freshness::Known if !self.sliding() => Vec::new(),
            Freshness::Known => self.extra_state(room).await,
        };
        let mut local = Vec::new();
        for kind in TYPES {
            for event in room.get_state_events(StateEventType::from(kind)).await.unwrap_or_default() {
                let json: Option<Value> = match event {
                    RawAnySyncOrStrippedState::Sync(raw) => raw.deserialize_as_unchecked().ok(),
                    RawAnySyncOrStrippedState::Stripped(raw) => raw.deserialize_as_unchecked().ok(),
                };
                local.extend(json);
            }
        }
        let server = server.into_iter().filter(|event| TYPES.iter().any(|kind| event["type"] == *kind));
        Ok(newest(server.collect(), local).iter().filter_map(read_state).collect())
    }

    /// A room's state from the server that the room list's sliding sync leaves out (widgets,
    /// image packs), as read once this run; read now when it wasn't, and when that fails, none
    /// (asked again next time).
    pub(crate) async fn extra_state(&self, room: &Room) -> Vec<Value> {
        let room_id = room.room_id().to_string();
        if let Some(known) = self.extra_state.lock().ok().and_then(|known| known.rooms.get(&room_id).cloned()) {
            return known;
        }
        self.read_extra_state(room).await.unwrap_or_default()
    }

    /// Forgets every room's [`Account::extra_state`].
    pub(crate) fn forget_all_extra_state(&self) {
        if let Ok(mut known) = self.extra_state.lock() {
            known.rooms.clear();
            known.generation += 1;
        }
    }

    /// Forgets a room's [`Account::extra_state`]: it is read from the server when next needed.
    pub(crate) fn forget_extra_state(&self, room_id: &str) {
        if let Ok(mut known) = self.extra_state.lock() {
            known.rooms.remove(room_id);
            *known.forgotten.entry(room_id.to_owned()).or_default() += 1;
        }
    }

    /// Reads [`Account::extra_state`] from the server now, and keeps it for the run. An answer
    /// overtaken while it came (a later read answered first, or something was forgotten) gives
    /// way: to the later read's, or to a read again.
    pub(crate) async fn read_extra_state(&self, room: &Room) -> Result<Vec<Value>, Error> {
        use matrix_sdk::ruma::api::client::state::get_state_events;
        let room_id = room.room_id().to_string();
        let mut tries = 0;
        loop {
            tries += 1;
            // Numbered as asked: of overlapping reads of a room, only the latest asked counts.
            let (asked_at, number) = match self.extra_state.lock() {
                Ok(mut known) => {
                    known.asked += 1;
                    ((known.generation, known.forgotten.get(&room_id).copied().unwrap_or_default()), known.asked)
                }
                Err(_) => ((0, 0), 0),
            };
            let response = self.client().send(get_state_events::v3::Request::new(room.room_id().to_owned())).await?;
            let events: Vec<Value> = response
                .room_state
                .iter()
                .filter_map(|raw| raw.deserialize_as_unchecked::<Value>().ok())
                .filter(|event| event["type"].as_str().is_some_and(|kind| EXTRA_STATE.contains(&kind)))
                .collect();
            let Ok(mut known) = self.extra_state.lock() else { return Ok(events) };
            let later = known.written.get(&room_id).is_some_and(|written| *written > number);
            let forgotten = (known.generation, known.forgotten.get(&room_id).copied().unwrap_or_default());
            if later || forgotten != asked_at {
                if let Some(newer) = known.rooms.get(&room_id).filter(|_| later) {
                    return Ok(newer.clone());
                }
                // Forgotten meanwhile, with nothing newer kept: asked again, once; overtaken again,
                // it answers nothing rather than something stale.
                if tries < 2 {
                    continue;
                }
                return Err(Error::Account("the room's state changed while it was read".to_owned()));
            }
            known.rooms.insert(room_id.clone(), events.clone());
            known.written.insert(room_id, number);
            return Ok(events);
        }
    }
}

/// Rooms' state the room list's sliding sync leaves out, as read from the server this run
/// ([`Account::extra_state`]).
#[derive(Debug, Default)]
pub(crate) struct ExtraState {
    /// By room.
    pub rooms: std::collections::HashMap<String, Vec<Value>>,
    /// Bumped whenever every room's is forgotten, so a read that started before isn't kept.
    pub generation: u64,
    /// The same, per room, for one room's forgotten.
    pub forgotten: std::collections::HashMap<String, u64>,
    /// How many reads were asked for, numbering each.
    pub asked: u64,
    /// The number of the read each room's state comes from.
    pub written: std::collections::HashMap<String, u64>,
}

/// The state types the room list's sliding sync leaves out that the app reads: widgets, and a
/// room's image packs (`crate::packs`).
const BRIDGE_TYPES: [&str; 2] = ["m.bridge", "uk.half-shot.bridge"];
const MEMBER_HINT_TYPES: [&str; 2] = ["m.member_hints", "io.element.functional_members"];

pub(crate) const EXTRA_STATE: [&str; 8] = [
    LEGACY_TYPE,
    SPEC_TYPE,
    "m.room.image_pack",
    "im.ponies.room_emotes",
    BRIDGE_TYPES[0],
    BRIDGE_TYPES[1],
    MEMBER_HINT_TYPES[0],
    MEMBER_HINT_TYPES[1],
];

fn room_integrations(events: &[Value]) -> (Vec<String>, Vec<RoomBridge>) {
    use matrix_sdk::ruma::UserId;
    let mut services = std::collections::BTreeSet::new();
    let mut bridges = std::collections::BTreeMap::new();
    for event in events {
        let Some(kind) = event["type"].as_str() else { continue };
        let content = &event["content"];
        if MEMBER_HINT_TYPES.contains(&kind) && event["state_key"] == "" {
            if let Some(members) = content["service_members"].as_array() {
                services.extend(members.iter().filter_map(Value::as_str).filter(|id| <&UserId>::try_from(*id).is_ok()));
            }
        } else if BRIDGE_TYPES.contains(&kind)
            && let Some(id) = event["state_key"].as_str()
            && let Some(protocol) = bridge_name(&content["protocol"])
            && let Some(channel) = bridge_name(&content["channel"])
        {
            if let Some(bot) = content["bridgebot"].as_str().filter(|id| <&UserId>::try_from(*id).is_ok()) {
                services.insert(bot);
            }
            let network = bridge_name(&content["network"]).map(str::to_owned);
            let bridge =
                RoomBridge { id: id.to_owned(), protocol: protocol.to_owned(), channel: channel.to_owned(), network };
            if kind == BRIDGE_TYPES[0] || !bridges.contains_key(id) {
                bridges.insert(id, bridge);
            }
        }
    }
    (services.into_iter().map(str::to_owned).collect(), bridges.into_values().collect())
}

fn bridge_name(value: &Value) -> Option<&str> {
    value["displayname"]
        .as_str()
        .filter(|name| !name.is_empty())
        .or_else(|| value["id"].as_str().filter(|id| !id.is_empty()))
}

/// Of the server's and the store's copies of each state event (by type and state key), the
/// newer: the store's may be older (kept from before sliding sync, which brings no such state)
/// or newer (a change in a timeline since the server's state was read). Alike in time, the
/// server's does: the same event, redacted since, keeps its time.
pub(crate) fn newest(server: Vec<Value>, local: Vec<Value>) -> Vec<Value> {
    let at = |event: &Value| event["origin_server_ts"].as_u64().unwrap_or_default();
    let mut newest: Vec<Value> = Vec::new();
    for event in server.into_iter().chain(local) {
        match newest.iter_mut().find(|known| known["type"] == event["type"] && known["state_key"] == event["state_key"])
        {
            Some(known) if at(&event) > at(known) => *known = event,
            Some(_) => {}
            None => newest.push(event),
        }
    }
    newest
}

/// One state event, if it is a widget's that still holds something.
fn read_state(event: &Value) -> Option<WidgetState> {
    let kind = TYPES.into_iter().find(|kind| event["type"] == *kind)?;
    let content = event.get("content")?.clone();
    if !content.as_object().is_some_and(|content| !content.is_empty()) {
        return None;
    }
    let state_key = event["state_key"].as_str()?.to_owned();
    let sender = event["sender"].as_str().map(str::to_owned);
    Some((kind, state_key, content, sender))
}

/// A widget from its state content, `None` when it has no usable URL.
pub(crate) fn widget(id: &str, content: &Value, sender: Option<String>, viewer: &Viewer) -> Option<Widget> {
    let kind = content.get("type").and_then(Value::as_str).unwrap_or("m.custom").to_owned();
    let data = content.get("data").cloned().unwrap_or_else(|| json!({}));
    let template = content.get("url").and_then(Value::as_str).unwrap_or_default();
    let place = meetings::place(&kind, template, &data);
    let url = match place.as_ref().and_then(|place| place.browser_url(viewer)) {
        Some(url) => url,
        None if template.is_empty() => return None,
        None => fill(template, id, &data, viewer),
    };
    let parsed = url::Url::parse(&url).ok().filter(|url| matches!(url.scheme(), "https" | "http"))?;
    let name = content
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| kind.clone(), str::to_owned);
    let creator = content.get("creatorUserId").and_then(Value::as_str).map(str::to_owned).or(sender);
    let host = parsed.host_str().unwrap_or_default().to_owned();
    let meeting = place.map(|place| place.meeting());
    Some(Widget { id: id.to_owned(), name, kind, host, url, creator, can_remove: false, meeting })
}

/// Fills a widget URL's template variables: the viewer's, then the widget's own `data`, each
/// URL-encoded (as Element does). Unknown variables stay as they are.
pub(crate) fn fill(template: &str, widget_id: &str, data: &Value, viewer: &Viewer) -> String {
    let mut values: Vec<(String, String)> = vec![
        ("matrix_user_id".into(), viewer.user_id.clone()),
        ("matrix_room_id".into(), viewer.room_id.clone()),
        ("matrix_display_name".into(), viewer.display_name.clone()),
        ("matrix_avatar_url".into(), viewer.avatar_url.clone()),
        ("matrix_widget_id".into(), widget_id.into()),
        ("org.matrix.msc2873.client_id".into(), "chat.bettermatrix".into()),
        ("org.matrix.msc2873.client_theme".into(), "light".into()),
        ("org.matrix.msc2873.client_language".into(), "en".into()),
        ("org.matrix.msc3819.matrix_device_id".into(), viewer.device_id.clone()),
        ("org.matrix.msc4039.matrix_base_url".into(), viewer.homeserver.clone()),
    ];
    if let Some(data) = data.as_object() {
        for (key, value) in data {
            let value = match value {
                Value::String(text) => text.clone(),
                Value::Number(_) | Value::Bool(_) => value.to_string(),
                _ => continue,
            };
            values.push((key.clone(), value));
        }
    }
    // Longest names first, so `$matrix_user_id` isn't read as a shorter variable's prefix.
    values.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    let mut url = template.to_owned();
    for (name, value) in values {
        url = url.replace(&format!("${name}"), &encode(&value));
    }
    url
}

/// A value URL-encoded as a URI component (spaces as `%20`).
pub(crate) fn encode(value: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
    encoded.replace('+', "%20")
}

/// The timeline's line for a widget state event's content: pinned (or changed), or removed.
pub(crate) fn change(content: &Value) -> crate::RoomChange {
    if !content.as_object().is_some_and(|content| !content.is_empty()) {
        return crate::RoomChange::WidgetRemoved;
    }
    let kind = content.get("type").and_then(Value::as_str).unwrap_or("m.custom");
    let template = content.get("url").and_then(Value::as_str).unwrap_or_default();
    let data = content.get("data").cloned().unwrap_or(Value::Null);
    let name = content
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(kind)
        .to_owned();
    let meeting = meetings::place(kind, template, &data).map(|place| place.meeting());
    crate::RoomChange::WidgetAdded { name, meeting }
}

/// An HTTP client for requests carrying the user's credentials (OpenID tokens, a manager's
/// token): it follows no redirect, so they reach only the server asked.
pub(crate) fn credential_http() -> Result<matrix_sdk::reqwest::Client, Error> {
    matrix_sdk::reqwest::Client::builder()
        .redirect(matrix_sdk::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| Error::Integrations(error.to_string()))
}

/// A request with a JSON body (the SDK's HTTP client has no JSON support of its own).
pub(crate) fn with_json(
    builder: matrix_sdk::reqwest::RequestBuilder,
    body: &Value,
) -> matrix_sdk::reqwest::RequestBuilder {
    builder.header("content-type", "application/json").body(body.to_string())
}

/// Sends a request and reads its answer as JSON (`null` when it isn't), with its status. Its
/// errors leave out the URL, which may carry a token.
pub(crate) async fn send_json(
    builder: matrix_sdk::reqwest::RequestBuilder,
) -> Result<(u16, Value), matrix_sdk::reqwest::Error> {
    let response = builder.send().await.map_err(matrix_sdk::reqwest::Error::without_url)?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.map_err(matrix_sdk::reqwest::Error::without_url)?;
    Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
}

/// The rooms whose widgets a sync's updates changed (a widget state event), and the rooms where
/// something was redacted, which may have been a widget.
pub(crate) fn changed_rooms(updates: &matrix_sdk::sync::RoomUpdates) -> (Vec<String>, Vec<String>) {
    #[derive(serde::Deserialize)]
    struct Typed<'a> {
        #[serde(rename = "type", borrow)]
        kind: std::borrow::Cow<'a, str>,
    }
    let kind = |json: &str| serde_json::from_str::<Typed<'_>>(json).map(|event| event.kind.into_owned()).ok();
    let is_widget = |json: &str| {
        kind(json).is_some_and(|kind| {
            let kind = kind.as_str();
            TYPES.contains(&kind) || BRIDGE_TYPES.contains(&kind) || MEMBER_HINT_TYPES.contains(&kind)
        })
    };
    let (mut widgets, mut redacted) = (Vec::new(), Vec::new());
    for (room_id, update) in &updates.joined {
        let state = match &update.state {
            matrix_sdk::sync::State::Before(events) | matrix_sdk::sync::State::After(events) => events,
        };
        let in_state = state.iter().any(|raw| is_widget(raw.json().get()));
        let in_timeline = update.timeline.events.iter().any(|event| is_widget(event.raw().json().get()));
        if in_state || in_timeline {
            widgets.push(room_id.to_string());
        } else if update
            .timeline
            .events
            .iter()
            .any(|event| kind(event.raw().json().get()).as_deref() == Some("m.room.redaction"))
        {
            redacted.push(room_id.to_string());
        }
    }
    (widgets, redacted)
}

#[cfg(test)]
mod tests;
