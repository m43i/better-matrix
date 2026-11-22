//! Administering rooms and spaces: what a new one is made with ([`NewRoom`]), a room's settings
//! as an admin sees them ([`RoomSettings`]) and the changes made to them ([`RoomSetting`]),
//! upgrading a room to a newer version, and arranging a space's rooms ([`SpaceChange`]).
//!
//! Every change waits (briefly) for sync to bring it into the store, so the settings, room
//! list and space read afterwards show it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::SyncOrStrippedState;
use matrix_sdk::ruma::api::client::room::Visibility;
use matrix_sdk::ruma::api::client::room::create_room::v3::{CreationContent, Request as CreateRoom, RoomPreset};
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::events::room::guest_access::{GuestAccess, RoomGuestAccessEventContent};
use matrix_sdk::ruma::events::room::join_rules::RoomJoinRulesEventContent;
use matrix_sdk::ruma::events::room::power_levels::{RoomPowerLevels, RoomPowerLevelsEventContent, UserPowerLevel};
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::ruma::events::space::parent::SpaceParentEventContent;
use matrix_sdk::ruma::events::{InitialStateEvent, StateEventType, TimelineEventType};
use matrix_sdk::ruma::room::{AllowRule, JoinRule as SdkJoinRule, Restricted, RoomType};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{Int, OwnedRoomAliasId, OwnedRoomId, RoomAliasId, RoomId, RoomVersionId, SpaceChildOrder};

use crate::Error;
use crate::account::Account;
use crate::model::{HistoryVisibility, MediaRef};
use crate::rooms::JoinRule;

/// How long a change may take to come back through sync before the command finishes anyway.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(10);
/// How often the store is looked at while waiting.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// What a new room or space is made with.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct NewRoom {
    /// Its name.
    pub name: String,
    /// Its topic, if any.
    pub topic: Option<String>,
    /// A space rather than a room.
    pub space: bool,
    /// The space it belongs in, if any: linked both ways.
    pub parent: Option<String>,
    /// Who may join. [`JoinRule::Restricted`] lets the members of `parent` in, so needs one.
    pub join_rule: JoinRule,
    /// Its main address's local part (`rust` for `#rust:example.org`), on the account's server.
    pub address: Option<String>,
    /// Listed in the server's public directory.
    pub published: bool,
    /// Messages are end-to-end encrypted (rooms only; it can't be turned off later).
    pub encrypted: bool,
}

impl NewRoom {
    /// A private room or space named `name`: invite only, encrypted when a room.
    pub fn private(name: impl Into<String>, space: bool) -> Self {
        Self {
            name: name.into(),
            topic: None,
            space,
            parent: None,
            join_rule: JoinRule::Invite,
            address: None,
            published: false,
            encrypted: !space,
        }
    }
}

/// Something an admin may allow from a chosen power level on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum Permission {
    /// The level everyone starts at: whoever is at least here may do what it allows.
    DefaultLevel,
    /// Send messages.
    SendMessages,
    /// Invite people.
    Invite,
    /// Remove people.
    Kick,
    /// Ban people.
    Ban,
    /// Delete other people's messages.
    RemoveMessages,
    /// Notify everyone with `@room`.
    MentionRoom,
    /// Change any setting not listed apart.
    ChangeSettings,
    /// Change the name.
    Name,
    /// Change the topic.
    Topic,
    /// Change the picture.
    Avatar,
    /// Change the addresses.
    Addresses,
    /// Change who may join, read history and join as a guest.
    Access,
    /// Pin messages.
    Pins,
    /// Change these permissions and people's roles.
    Permissions,
    /// Turn on encryption.
    Encryption,
    /// Upgrade the room.
    Upgrade,
    /// Add, remove and order a space's rooms.
    SpaceRooms,
}

/// The events a message is sent as: plain, or encrypted.
const MESSAGE_EVENTS: [matrix_sdk::ruma::events::MessageLikeEventType; 2] = [
    matrix_sdk::ruma::events::MessageLikeEventType::RoomMessage,
    matrix_sdk::ruma::events::MessageLikeEventType::RoomEncrypted,
];

/// The state events a permission stands for. Empty for the ones kept outside `events`.
fn state_events(permission: Permission) -> &'static [StateEventType] {
    match permission {
        Permission::Name => &[StateEventType::RoomName],
        Permission::Topic => &[StateEventType::RoomTopic],
        Permission::Avatar => &[StateEventType::RoomAvatar],
        Permission::Addresses => &[StateEventType::RoomCanonicalAlias],
        Permission::Access => {
            &[StateEventType::RoomJoinRules, StateEventType::RoomHistoryVisibility, StateEventType::RoomGuestAccess]
        }
        Permission::Pins => &[StateEventType::RoomPinnedEvents],
        Permission::Permissions => &[StateEventType::RoomPowerLevels],
        Permission::Encryption => &[StateEventType::RoomEncryption],
        Permission::Upgrade => &[StateEventType::RoomTombstone],
        Permission::SpaceRooms => &[StateEventType::SpaceChild],
        _ => &[],
    }
}

impl Permission {
    /// The permissions a room's (or with `space`, a space's) editor lists, in its order.
    pub fn listed(space: bool) -> &'static [Permission] {
        use Permission::*;
        if space {
            &[
                DefaultLevel,
                Invite,
                Kick,
                Ban,
                ChangeSettings,
                Name,
                Topic,
                Avatar,
                Addresses,
                Access,
                SpaceRooms,
                Permissions,
            ]
        } else {
            &[
                DefaultLevel,
                SendMessages,
                Invite,
                Kick,
                Ban,
                RemoveMessages,
                MentionRoom,
                Pins,
                ChangeSettings,
                Name,
                Topic,
                Avatar,
                Addresses,
                Access,
                Encryption,
                Permissions,
                Upgrade,
            ]
        }
    }

    /// The level `power` asks for it.
    pub(crate) fn level(self, power: &RoomPowerLevels) -> i64 {
        let level = match self {
            Self::DefaultLevel => power.users_default,
            // What a message needs: its own event's level where the room sets one (plain or
            // encrypted, the higher), else the default.
            Self::SendMessages => MESSAGE_EVENTS
                .iter()
                .map(|event| {
                    power.events.get(&TimelineEventType::from(event.clone())).copied().unwrap_or(power.events_default)
                })
                .max()
                .unwrap_or(power.events_default),
            Self::Invite => power.invite,
            Self::Kick => power.kick,
            Self::Ban => power.ban,
            Self::RemoveMessages => power.redact,
            Self::MentionRoom => power.notifications.room,
            Self::ChangeSettings => power.state_default,
            // The highest of its events: what one must reach to change them all.
            permission => state_events(permission)
                .iter()
                .map(|event| {
                    power.events.get(&TimelineEventType::from(event.clone())).copied().unwrap_or(power.state_default)
                })
                .max()
                .unwrap_or(power.state_default),
        };
        level.into()
    }

    /// The highest level [`Permission::set`] would change: what the user must reach to change it
    /// (Send messages shows what a message needs, but also writes the default for events).
    pub(crate) fn highest(self, power: &RoomPowerLevels) -> i64 {
        match self {
            Self::SendMessages => MESSAGE_EVENTS
                .iter()
                .filter_map(|event| power.events.get(&TimelineEventType::from(event.clone())).copied())
                .chain([power.events_default])
                .max()
                .map_or(0, i64::from),
            permission => permission.level(power),
        }
    }

    /// Whether every level [`Permission::set`] writes is `level` in `power`: what a written
    /// change looks like once sync brings it (the highest alone may have been `level` before).
    pub(crate) fn is_set(self, power: &RoomPowerLevels, level: i64) -> bool {
        let level = Int::new_saturating(level);
        match self {
            Self::SendMessages => {
                power.events_default == level
                    && MESSAGE_EVENTS.iter().all(|event| {
                        power.events.get(&TimelineEventType::from(event.clone())).is_none_or(|set| *set == level)
                    })
            }
            permission if !state_events(permission).is_empty() => state_events(permission)
                .iter()
                .all(|event| power.events.get(&TimelineEventType::from(event.clone())) == Some(&level)),
            permission => Int::new_saturating(permission.level(power)) == level,
        }
    }

    /// `power` with it asked for at `level`.
    pub(crate) fn set(self, power: &mut RoomPowerLevels, level: Int) {
        match self {
            Self::DefaultLevel => power.users_default = level,
            // The default for events (messages, polls, reactions), as Element has it; a message
            // level the room sets of its own would otherwise still rule, so it follows.
            Self::SendMessages => {
                power.events_default = level;
                for event in MESSAGE_EVENTS {
                    if let Some(set) = power.events.get_mut(&TimelineEventType::from(event.clone())) {
                        *set = level;
                    }
                }
            }
            Self::Invite => power.invite = level,
            Self::Kick => power.kick = level,
            Self::Ban => power.ban = level,
            Self::RemoveMessages => power.redact = level,
            Self::MentionRoom => power.notifications.room = level,
            Self::ChangeSettings => power.state_default = level,
            permission => {
                for event in state_events(permission) {
                    power.events.insert(TimelineEventType::from(event.clone()), level);
                }
            }
        }
    }
}

/// One permission and the level it asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PermissionLevel {
    /// Which.
    pub permission: Permission,
    /// From which power level on.
    pub level: i64,
    /// Whether the user may change it: every level its change writes is no higher than their
    /// own, and the room wasn't replaced (an upgrade raised its levels to close it).
    pub editable: bool,
}

/// What the user may change in a room's settings. The settings screen shows only these.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomRights {
    /// Its name.
    pub name: bool,
    /// Its topic.
    pub topic: bool,
    /// Its picture.
    pub avatar: bool,
    /// Who may join.
    pub join_rule: bool,
    /// Who may read history.
    pub history: bool,
    /// Whether guests may join.
    pub guest_access: bool,
    /// Its addresses, and whether it is in the server's directory.
    pub addresses: bool,
    /// Turning on encryption (never when it is on).
    pub encryption: bool,
    /// The permission levels.
    pub permissions: bool,
    /// Upgrading it.
    pub upgrade: bool,
    /// Its rooms, for a space.
    pub space_rooms: bool,
}

impl RoomRights {
    /// What the user (`me`) may change, by `power`.
    pub(crate) fn of(power: &RoomPowerLevels, me: &matrix_sdk::ruma::UserId, encrypted: bool, space: bool) -> Self {
        let may = |event: StateEventType| power.user_can_send_state(me, event);
        Self {
            name: may(StateEventType::RoomName),
            topic: may(StateEventType::RoomTopic),
            avatar: may(StateEventType::RoomAvatar),
            join_rule: may(StateEventType::RoomJoinRules),
            history: may(StateEventType::RoomHistoryVisibility),
            guest_access: may(StateEventType::RoomGuestAccess),
            addresses: may(StateEventType::RoomCanonicalAlias),
            encryption: !encrypted && !space && may(StateEventType::RoomEncryption),
            permissions: may(StateEventType::RoomPowerLevels),
            // Following a space's successor isn't offered, so neither is upgrading one.
            upgrade: !space && may(StateEventType::RoomTombstone),
            space_rooms: space && may(StateEventType::SpaceChild),
        }
    }

    /// Whether any setting may be changed: whether the settings are offered at all.
    pub fn any(&self) -> bool {
        let Self {
            name,
            topic,
            avatar,
            join_rule,
            history,
            guest_access,
            addresses,
            encryption,
            permissions,
            upgrade,
            space_rooms,
        } = *self;
        name || topic
            || avatar
            || join_rule
            || history
            || guest_access
            || addresses
            || encryption
            || permissions
            || upgrade
            || space_rooms
    }
}

/// A room's or space's settings, as its settings screen shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomSettings {
    /// Its ID.
    pub room_id: String,
    /// Whether it is a space.
    pub space: bool,
    /// The name it was given, if any.
    pub name: Option<String>,
    /// Its topic, if any.
    pub topic: Option<String>,
    /// Its picture.
    pub avatar: Option<MediaRef>,
    /// Who may join; `None` for a rule the app doesn't know.
    pub join_rule: Option<JoinRule>,
    /// Whether it lies in a space whose members [`JoinRule::Restricted`] would let in, and its
    /// version allows that rule.
    pub can_restrict: bool,
    /// Whether [`JoinRule::Knock`] may be chosen: a room (not a space) whose version allows it.
    pub can_knock: bool,
    /// Who may read history from before they joined.
    pub history: HistoryVisibility,
    /// Whether guests may join.
    pub guest_access: bool,
    /// Whether messages are end-to-end encrypted.
    pub encrypted: bool,
    /// Its main address (`#rust:example.org`).
    pub main_address: Option<String>,
    /// Its other published addresses.
    pub addresses: Vec<String>,
    /// Whether it is listed in the account's server's public directory; `None` when the
    /// server didn't say.
    pub published: Option<bool>,
    /// The account's server, where new addresses are made (`example.org`).
    pub server: String,
    /// Its room version.
    pub version: String,
    /// The version an upgrade would bring it to: the server's default, when newer and the user
    /// may upgrade.
    pub upgrade_to: Option<String>,
    /// Each listed permission's level, in [`Permission::listed`] order.
    pub permissions: Vec<PermissionLevel>,
    /// The user's own power level: they may only grant up to it.
    pub own_level: i64,
    /// What the user may change.
    pub rights: RoomRights,
}

/// One change to a room's or space's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum RoomSetting {
    /// A new name.
    Name(String),
    /// A new topic; blank removes it.
    Topic(String),
    /// Upload a picture, or with `None` remove it.
    Avatar(Option<PathBuf>),
    /// Who may join. [`JoinRule::Restricted`] lets in the members of the spaces it is in.
    JoinRule(JoinRule),
    /// Who may read history.
    History(HistoryVisibility),
    /// Whether guests may join.
    GuestAccess(bool),
    /// List it in the account's server's public directory, or take it out.
    Published(bool),
    /// Make an address (`#rust:example.org`, or `rust` on the account's server) and publish
    /// it; the first becomes the main one.
    AddAddress(String),
    /// Unpublish an address, and delete it when it is on the account's server.
    RemoveAddress(String),
    /// Make a published address the main one.
    MainAddress(String),
    /// Turn on encryption. It can't be turned off.
    Encrypt,
    /// Ask for a permission from a level on.
    Permission {
        /// Which.
        permission: Permission,
        /// From which level.
        level: i64,
    },
}

/// One change to a space's rooms.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum SpaceChange {
    /// Add a joined room or space, after the others; it names the space as its parent where
    /// the user may say so.
    Add(String),
    /// Take a room or space out.
    Remove(String),
    /// Put the space's rooms in this order (every child, by ID).
    Order(Vec<String>),
}

/// A child's place in its space, as `m.space.child` gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChildPlace {
    /// The child's room ID.
    pub room_id: String,
    /// Its `order`, when valid (printable ASCII, at most 50 characters).
    pub order: Option<String>,
    /// When the child event was sent.
    pub sent_ms: u64,
    /// Whether the space suggests it to its members.
    pub suggested: bool,
}

/// Children in their space's order (spec): those with an `order` first, by it; then the rest
/// by when they were added; ties by room ID.
pub(crate) fn sort_children(children: &mut [ChildPlace]) {
    children.sort_by(|a, b| match (&a.order, &b.order) {
        (Some(x), Some(y)) => x.cmp(y).then_with(|| a.sent_ms.cmp(&b.sent_ms)).then_with(|| a.room_id.cmp(&b.room_id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.sent_ms.cmp(&b.sent_ms).then_with(|| a.room_id.cmp(&b.room_id)),
    });
}

/// The `order` each child takes for `wanted`, for those whose order must change: one fixed-width
/// key per position, so only children out of place are written.
pub(crate) fn reorder(current: &[ChildPlace], wanted: &[String]) -> Vec<(String, String)> {
    wanted
        .iter()
        .enumerate()
        .filter_map(|(index, room_id)| {
            let order = format!("{:06}", (index + 1) * 10);
            let place = current.iter().find(|child| &child.room_id == room_id)?;
            (place.order.as_deref() != Some(order.as_str())).then(|| (room_id.clone(), order))
        })
        .collect()
}

/// The order keys that put `new` where `old` was among `children`: none when `old`'s own key keeps
/// it there; else keys for every child out of place, as a new child without one (or sharing one)
/// would sort elsewhere.
pub(crate) fn successor_keys(mut children: Vec<ChildPlace>, old: &str, new: &str) -> Vec<(String, String)> {
    let Some(place) = children.iter().find(|child| child.room_id == old).cloned() else { return Vec::new() };
    let shared = children.iter().filter(|child| child.order == place.order).count() > 1;
    if place.order.is_some() && !shared {
        return Vec::new();
    }
    sort_children(&mut children);
    for child in &mut children {
        if child.room_id == old {
            child.room_id = new.to_owned();
            child.order = None;
        }
    }
    let wanted: Vec<String> = children.iter().map(|child| child.room_id.clone()).collect();
    reorder(&children, &wanted)
}

/// The valid `order` of a child event.
fn valid_order(order: Option<&str>) -> Option<String> {
    order.filter(|order| order.len() <= 50 && order.bytes().all(|b| (0x20..=0x7e).contains(&b))).map(ToOwned::to_owned)
}

/// Gives each child named its `order` key, keeping the rest of its child event (its servers,
/// whether it is suggested).
async fn write_orders(space: &Room, keys: &[(String, String)], me: &matrix_sdk::ruma::UserId) -> Result<(), Error> {
    for (room_id, order) in keys {
        let child = RoomId::parse(room_id.as_str())?;
        let mut content = SpaceChildEventContent::new(vec![me.server_name().to_owned()]);
        if let Ok(Some(raw)) = space.get_state_event_static_for_key::<SpaceChildEventContent, _>(&child).await
            && let Ok(SyncOrStrippedState::Sync(event)) = raw.deserialize()
            && let Some(event) = event.as_original()
        {
            content = event.content.clone();
        }
        content.order = Some(SpaceChildOrder::parse(order.as_str())?);
        space.send_state_event_for_key(&child, content).await?;
    }
    Ok(())
}

/// A space's children with their places, in no order: removed ones (empty `via`) left out.
pub(crate) async fn children(space: &Room) -> Vec<ChildPlace> {
    space
        .get_state_events_static::<SpaceChildEventContent>()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|raw| match raw.deserialize().ok()? {
            SyncOrStrippedState::Sync(event) => {
                event.as_original().filter(|e| !e.content.via.is_empty()).map(|e| ChildPlace {
                    room_id: e.state_key.to_string(),
                    order: valid_order(e.content.order.as_ref().map(|order| order.as_str())),
                    sent_ms: e.origin_server_ts.get().into(),
                    suggested: e.content.suggested,
                })
            }
            SyncOrStrippedState::Stripped(_) => None,
        })
        .collect()
}

/// An address as typed (`#rust:example.org`, `#rust`, `rust`) on `server` unless it names one.
pub(crate) fn address_of(typed: &str, server: &str) -> Result<OwnedRoomAliasId, Error> {
    let typed = typed.trim();
    let local = typed.strip_prefix('#').unwrap_or(typed);
    let full = if local.contains(':') { format!("#{local}") } else { format!("#{local}:{server}") };
    Ok(RoomAliasId::parse(full)?)
}

/// The local part of a new address: no `#`, no server, no spaces.
pub(crate) fn local_part(typed: &str) -> Option<String> {
    let typed = typed.trim();
    let local = typed.strip_prefix('#').unwrap_or(typed);
    let local = local.split(':').next().unwrap_or_default().trim();
    (!local.is_empty()).then(|| local.to_owned())
}

/// Our join rule of the SDK's.
pub(crate) fn join_rule_of(rule: &SdkJoinRule) -> Option<JoinRule> {
    match rule {
        SdkJoinRule::Public => Some(JoinRule::Public),
        SdkJoinRule::Invite => Some(JoinRule::Invite),
        SdkJoinRule::Knock => Some(JoinRule::Knock),
        SdkJoinRule::Restricted(_) => Some(JoinRule::Restricted),
        // Asking and space members at once: a rule of its own the app doesn't offer, so no
        // choice shows as in force and any choice is written.
        _ => None,
    }
}

fn history_of(history: HistoryVisibility) -> matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility {
    use matrix_sdk::ruma::events::room::history_visibility::HistoryVisibility as Sdk;
    match history {
        HistoryVisibility::WorldReadable => Sdk::WorldReadable,
        HistoryVisibility::Shared => Sdk::Shared,
        HistoryVisibility::Invited => Sdk::Invited,
        HistoryVisibility::Joined => Sdk::Joined,
    }
}

/// Whether room version `to` is newer than `from`: by number, for the numbered versions.
pub(crate) fn newer(to: &str, from: &str) -> bool {
    match (to.parse::<u32>(), from.parse::<u32>()) {
        (Ok(to), Ok(from)) => to > from,
        _ => false,
    }
}

fn storage(error: impl std::fmt::Display) -> Error {
    Error::Storage(std::io::Error::other(error.to_string()))
}

impl Account {
    /// Makes a room or space as `new` describes; returns its ID.
    pub async fn create_room(&self, new: &NewRoom) -> Result<String, Error> {
        let mut request = CreateRoom::new();
        request.name = Some(new.name.trim().to_owned());
        request.topic = new.topic.as_deref().map(str::trim).filter(|topic| !topic.is_empty()).map(ToOwned::to_owned);
        request.preset =
            Some(if new.join_rule == JoinRule::Public { RoomPreset::PublicChat } else { RoomPreset::PrivateChat });
        request.room_alias_name = new.address.as_deref().and_then(local_part);
        if new.published {
            request.visibility = Visibility::Public;
        }
        let parent = new.parent.as_deref().map(RoomId::parse).transpose()?;
        let mut initial = Vec::new();
        match new.join_rule {
            JoinRule::Restricted => {
                let Some(parent) = &parent else { return Err(Error::NoParentSpace) };
                let rule = SdkJoinRule::Restricted(Restricted::new(vec![AllowRule::room_membership(parent.clone())]));
                let event = InitialStateEvent::with_empty_state_key(RoomJoinRulesEventContent::new(rule));
                initial.push(Raw::new(&event).map_err(storage)?.cast_unchecked());
            }
            JoinRule::Knock => {
                let event = InitialStateEvent::with_empty_state_key(RoomJoinRulesEventContent::new(SdkJoinRule::Knock));
                initial.push(Raw::new(&event).map_err(storage)?.cast_unchecked());
            }
            JoinRule::Public | JoinRule::Invite => {}
        }
        if new.encrypted && !new.space {
            let event =
                InitialStateEvent::with_empty_state_key(RoomEncryptionEventContent::with_recommended_defaults());
            initial.push(Raw::new(&event).map_err(storage)?.cast_unchecked());
        }
        request.initial_state = initial;
        if new.space {
            let mut creation = CreationContent::new();
            creation.room_type = Some(RoomType::Space);
            request.creation_content = Some(Raw::new(&creation).map_err(storage)?);
        }
        let room = self.client().create_room(request).await?;
        if let Some(parent) = parent {
            let via = vec![self.user_id()?.server_name().to_owned()];
            let space = self.room(parent.as_str())?;
            space.send_state_event_for_key(room.room_id(), SpaceChildEventContent::new(via.clone())).await?;
            room.send_state_event_for_key(&parent, SpaceParentEventContent::new(via)).await?;
        }
        Ok(room.room_id().to_string())
    }

    /// The spaces linked with a room both ways: joined ones that list it as their child, and that
    /// it names as its parent. Neither link alone is trusted: a room's `m.space.parent` may name
    /// any space, and any space may list any room, and their members would then get in.
    async fn parent_spaces(&self, room: &Room) -> Vec<OwnedRoomId> {
        // The spaces the room names itself, as its own admins said.
        let named: Vec<OwnedRoomId> = room
            .get_state_events_static::<SpaceParentEventContent>()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|raw| match raw.deserialize().ok()? {
                SyncOrStrippedState::Sync(event) => {
                    event.as_original().filter(|e| !e.content.via.is_empty()).map(|e| e.state_key.clone())
                }
                SyncOrStrippedState::Stripped(_) => None,
            })
            .collect();
        let mut parents = Vec::new();
        for space in self.client().joined_space_rooms() {
            if named.iter().any(|id| id == space.room_id())
                && children(&space).await.iter().any(|child| child.room_id == room.room_id().as_str())
            {
                parents.push(space.room_id().to_owned());
            }
        }
        parents
    }

    /// Whether guests may join, as the server has it: sliding sync doesn't bring
    /// `m.room.guest_access`, so the store may lack it (and then reads as forbidden). `None`
    /// when the server didn't say.
    async fn guest_access(&self, room: &Room) -> Option<bool> {
        use matrix_sdk::ruma::api::client::state::get_state_event_for_key::v3::Request;
        use matrix_sdk::ruma::api::error::ErrorKind;
        let request = Request::new(room.room_id().to_owned(), StateEventType::RoomGuestAccess, String::new());
        match self.client().send(request).await {
            Ok(response) => response
                .into_content()
                .deserialize_as_unchecked::<RoomGuestAccessEventContent>()
                .ok()
                .map(|content| content.guest_access == GuestAccess::CanJoin),
            // Never set: guests are forbidden.
            Err(error) if error.client_api_error_kind() == Some(&ErrorKind::NotFound) => Some(false),
            Err(_) => None,
        }
    }

    /// A room's or space's settings, as the user may see and change them.
    pub async fn room_settings(&self, room_id: &str) -> Result<RoomSettings, Error> {
        let room = self.room(room_id)?;
        let me = self.user_id()?;
        let power = room.power_levels().await.map_err(matrix_sdk::Error::from)?;
        let space = room.is_space();
        let encrypted = room.encryption_state().is_encrypted();
        let mut rights = RoomRights::of(&power, &me, encrypted, space);
        // A room already replaced isn't upgraded again: its members follow the successor it names.
        rights.upgrade &= !room.is_tombstoned();
        let rules = room.clone_info().room_version_rules_or_default();
        let version = room.clone_info().room_version().map_or_else(|| "1".to_owned(), ToString::to_string);
        let upgrade_to = if rights.upgrade {
            match self.client().homeserver_capabilities().room_versions().await {
                Ok(versions) => Some(versions.default.to_string()).filter(|default| newer(default, &version)),
                Err(_) => None,
            }
        } else {
            None
        };
        // Unknown when the server won't say: then the directory switch isn't offered.
        let published = room.privacy_settings().get_room_visibility().await.ok().map(|v| v == Visibility::Public);
        let own_level = match power.for_user(&me) {
            UserPowerLevel::Int(level) => level.into(),
            _ => i64::MAX,
        };
        Ok(RoomSettings {
            room_id: room_id.to_owned(),
            space,
            name: room.name().filter(|name| !name.trim().is_empty()),
            topic: room.topic().filter(|topic| !topic.trim().is_empty()),
            avatar: room.avatar_url().and_then(crate::convert::avatar),
            join_rule: room.join_rule().as_ref().and_then(join_rule_of),
            can_restrict: rules.authorization.restricted_join_rule && !self.parent_spaces(&room).await.is_empty(),
            can_knock: rules.authorization.knocking && !space,
            history: crate::convert::history_visibility(room.history_visibility_or_default()),
            guest_access: self.guest_access(&room).await.unwrap_or(room.guest_access() == GuestAccess::CanJoin),
            encrypted,
            main_address: room.canonical_alias().map(|alias| alias.to_string()),
            addresses: room.alt_aliases().iter().map(ToString::to_string).collect(),
            published,
            server: me.server_name().to_string(),
            version,
            upgrade_to,
            permissions: Permission::listed(space)
                .iter()
                .map(|&permission| PermissionLevel {
                    permission,
                    level: permission.level(&power),
                    editable: rights.permissions && !room.is_tombstoned() && permission.highest(&power) <= own_level,
                })
                .collect(),
            own_level,
            rights,
        })
    }

    /// Changes one of a room's or space's settings, then waits for sync to bring it. The engine
    /// runs these through [`Account::admin_changes`], one at a time in the order asked.
    pub async fn change_room(&self, room_id: &str, setting: RoomSetting) -> Result<(), Error> {
        let room = self.room(room_id)?;
        let avatar_before = room.avatar_url();
        let server = self.user_id()?.server_name().to_string();
        let privacy = room.privacy_settings();
        // The rule in force changes nothing: writing it again would rebuild a restricted rule's
        // allow list from the spaces of now (a screen may show a rule changed since).
        if let RoomSetting::JoinRule(rule) = &setting
            && room.join_rule().as_ref().and_then(join_rule_of) == Some(*rule)
        {
            return Ok(());
        }
        match &setting {
            RoomSetting::Name(name) => {
                room.set_name(name.trim().to_owned()).await?;
            }
            RoomSetting::Topic(topic) => {
                room.set_room_topic(topic.trim()).await?;
            }
            RoomSetting::Avatar(None) => {
                room.remove_avatar().await?;
            }
            RoomSetting::Avatar(Some(path)) => {
                let bytes =
                    tokio::fs::read(path).await.map_err(|source| Error::ReadFile { path: path.clone(), source })?;
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                if mime.type_() != mime_guess::mime::IMAGE {
                    return Err(Error::NotAnImage(path.clone()));
                }
                room.upload_avatar(&mime, bytes, None).await?;
            }
            RoomSetting::JoinRule(rule) => {
                let rule = match rule {
                    JoinRule::Public => SdkJoinRule::Public,
                    JoinRule::Invite => SdkJoinRule::Invite,
                    JoinRule::Knock => SdkJoinRule::Knock,
                    JoinRule::Restricted => {
                        let parents = self.parent_spaces(&room).await;
                        if parents.is_empty() {
                            return Err(Error::NoParentSpace);
                        }
                        SdkJoinRule::Restricted(Restricted::new(
                            parents.into_iter().map(AllowRule::room_membership).collect(),
                        ))
                    }
                };
                privacy.update_join_rule(rule).await?;
            }
            RoomSetting::History(history) => privacy.update_room_history_visibility(history_of(*history)).await?,
            RoomSetting::GuestAccess(allowed) => {
                let access = if *allowed { GuestAccess::CanJoin } else { GuestAccess::Forbidden };
                room.send_state_event(RoomGuestAccessEventContent::new(access)).await?;
            }
            RoomSetting::Published(published) => {
                let visibility = if *published { Visibility::Public } else { Visibility::Private };
                privacy.update_room_visibility(visibility).await?;
            }
            RoomSetting::AddAddress(typed) => {
                let alias = address_of(typed, &server)?;
                // An address the room can't list would be left behind, leading nowhere visible.
                let me = self.user_id()?;
                let may = room
                    .power_levels()
                    .await
                    .is_ok_and(|power| power.user_can_send_state(&me, StateEventType::RoomCanonicalAlias));
                if !may {
                    return Err(Error::NotAllowed);
                }
                let made = privacy.publish_room_alias_in_room_directory(&alias).await?;
                if !made {
                    let target = self.client().resolve_room_alias(&alias).await?.room_id;
                    if target != room.room_id() {
                        return Err(Error::AddressTaken(alias.to_string()));
                    }
                }
                let (main, mut alt) = (room.canonical_alias(), room.alt_aliases());
                if main.as_ref() == Some(&alias) || alt.contains(&alias) {
                    return Ok(());
                }
                let main = match main {
                    Some(main) => {
                        alt.push(alias.clone());
                        Some(main)
                    }
                    None => Some(alias.clone()),
                };
                if let Err(error) = privacy.update_canonical_alias(main, alt).await {
                    // Not listed: the address made for it goes again.
                    if made {
                        let _ = self.client().remove_room_alias(&alias).await;
                    }
                    return Err(error.into());
                }
            }
            RoomSetting::RemoveAddress(typed) => {
                use matrix_sdk::ruma::api::error::ErrorKind;
                let alias = address_of(typed, &server)?;
                // Only one on the account's server can be deleted, and only while it still leads
                // here: it may have moved to another room since. Looked up first, so a failed
                // lookup fails the removal; another server's is only unlisted.
                let local = alias.server_name().as_str() == server;
                let here = local
                    && match self.client().resolve_room_alias(&alias).await {
                        Ok(found) => found.room_id == room.room_id(),
                        Err(error) if error.client_api_error_kind() == Some(&ErrorKind::NotFound) => false,
                        Err(error) => return Err(error.into()),
                    };
                let (main, alt) = (room.canonical_alias(), room.alt_aliases());
                if main.as_ref() == Some(&alias) || alt.contains(&alias) {
                    let main = main.filter(|main| main != &alias);
                    let alt = alt.into_iter().filter(|other| other != &alias).collect();
                    privacy.update_canonical_alias(main, alt).await?;
                }
                if here {
                    self.client().remove_room_alias(&alias).await?;
                }
            }
            RoomSetting::MainAddress(typed) => {
                let alias = address_of(typed, &server)?;
                let mut alt: Vec<_> = room.alt_aliases().into_iter().filter(|other| other != &alias).collect();
                if let Some(old) = room.canonical_alias().filter(|old| old != &alias) {
                    alt.insert(0, old);
                }
                privacy.update_canonical_alias(Some(alias), alt).await?;
            }
            RoomSetting::Encrypt => room.enable_encryption().await?,
            RoomSetting::Permission { permission, level } => {
                // A replaced room's levels were raised to close it: they stay so.
                if room.is_tombstoned() {
                    return Err(Error::AlreadyReplaced);
                }
                let mut power = room.power_levels().await.map_err(matrix_sdk::Error::from)?;
                permission.set(&mut power, Int::new_saturating(*level));
                let content = RoomPowerLevelsEventContent::try_from(power).map_err(storage)?;
                room.send_state_event(content).await?;
            }
        }
        let settled = async {
            while !self.shows(&room, &setting, &server, avatar_before.as_deref()).await {
                tokio::time::sleep(LOOK_EVERY).await;
            }
        };
        let _ = tokio::time::timeout(SETTLE, settled).await;
        Ok(())
    }

    /// Whether the store shows `setting` done.
    async fn shows(
        &self,
        room: &Room,
        setting: &RoomSetting,
        server: &str,
        avatar_before: Option<&matrix_sdk::ruma::MxcUri>,
    ) -> bool {
        let alias = |typed: &str| address_of(typed, server).ok();
        match setting {
            RoomSetting::Name(name) => room.name().as_deref() == Some(name.trim()),
            RoomSetting::Topic(topic) => room.topic().unwrap_or_default() == topic.trim(),
            RoomSetting::Avatar(None) => room.avatar_url().is_none(),
            // A new picture: one other than before.
            RoomSetting::Avatar(Some(_)) => room.avatar_url().is_some_and(|url| Some(url.as_ref()) != avatar_before),
            RoomSetting::JoinRule(rule) => room.join_rule().as_ref().and_then(join_rule_of) == Some(*rule),
            RoomSetting::History(history) => {
                crate::convert::history_visibility(room.history_visibility_or_default()) == *history
            }
            RoomSetting::GuestAccess(allowed) => self.guest_access(room).await == Some(*allowed),
            RoomSetting::Published(_) => true,
            RoomSetting::AddAddress(typed) => alias(typed).is_none_or(|alias| {
                room.canonical_alias().as_ref() == Some(&alias) || room.alt_aliases().contains(&alias)
            }),
            RoomSetting::RemoveAddress(typed) => alias(typed).is_none_or(|alias| {
                room.canonical_alias().as_ref() != Some(&alias) && !room.alt_aliases().contains(&alias)
            }),
            RoomSetting::MainAddress(typed) => alias(typed).is_none_or(|alias| room.canonical_alias() == Some(alias)),
            RoomSetting::Encrypt => room.encryption_state().is_encrypted(),
            RoomSetting::Permission { permission, level } => {
                room.power_levels().await.is_ok_and(|power| permission.is_set(&power, *level))
            }
        }
    }

    /// Upgrades a room to `version`; returns the new room's ID. Spaces the user may arrange
    /// list the new room in the old one's place.
    pub async fn upgrade_room(&self, room_id: &str, version: &str) -> Result<String, Error> {
        use matrix_sdk::ruma::api::client::room::upgrade_room::v3::Request;
        let room = self.room(room_id)?;
        if room.is_tombstoned() {
            return Err(Error::AlreadyReplaced);
        }
        let version = RoomVersionId::try_from(version)?;
        let response = self.client().send(Request::new(room.room_id().to_owned(), version)).await?;
        let new = response.replacement_room;
        let me = self.user_id()?;
        let mut moved = Vec::new();
        for space in self.client().joined_space_rooms() {
            let current = children(&space).await;
            let Some(place) = current.iter().find(|child| child.room_id == room_id).cloned() else {
                continue;
            };
            let may = space
                .power_levels()
                .await
                .is_ok_and(|power| power.user_can_send_state(&me, StateEventType::SpaceChild));
            if !may {
                continue;
            }
            // The new room takes the old one's place (`successor_keys`).
            let keys = successor_keys(current, room_id, new.as_str());
            let new_key = keys.iter().find(|(id, _)| id == new.as_str()).map(|(_, key)| key.clone());
            let mut child = SpaceChildEventContent::new(vec![me.server_name().to_owned()]);
            child.order =
                new_key.or(place.order.clone()).as_deref().and_then(|order| SpaceChildOrder::parse(order).ok());
            child.suggested = place.suggested;
            // The old room leaves the space only once the new one is in it.
            if space.send_state_event_for_key(&new, child).await.is_err() {
                continue;
            }
            let others: Vec<_> = keys.into_iter().filter(|(id, _)| id != new.as_str()).collect();
            let _ = write_orders(&space, &others, &me).await;
            let _ = space.send_state_event_for_key(room.room_id(), SpaceChildEventContent::new(Vec::new())).await;
            moved.push(space.clone());
            // The server doesn't carry `m.space.parent` over: the new room names the spaces the
            // old one still named (a link it cleared, with no servers, stays cleared), so Space
            // members can still be chosen there.
            let named = matches!(
                room.get_state_event_static_for_key::<SpaceParentEventContent, _>(space.room_id()).await,
                Ok(Some(raw)) if matches!(
                    raw.deserialize(),
                    Ok(SyncOrStrippedState::Sync(event))
                        if event.as_original().is_some_and(|event| !event.content.via.is_empty())
                )
            );
            if named {
                use matrix_sdk::ruma::api::client::state::send_state_event::v3::Request as SendState;
                let parent = SpaceParentEventContent::new(vec![me.server_name().to_owned()]);
                if let Ok(request) = SendState::new(new.clone(), &space.room_id().to_owned(), &parent) {
                    let _ = self.client().send(request).await;
                }
            }
        }
        // The server joined the user to the new room, and the spaces list it: wait for sync to
        // bring both, so the next queued change reads them.
        let listed = async {
            loop {
                let mut spaces_done = true;
                for space in &moved {
                    let now = children(space).await;
                    spaces_done &= now.iter().any(|child| child.room_id == new.as_str())
                        && !now.iter().any(|child| child.room_id == room_id);
                }
                if spaces_done && self.client().get_room(&new).is_some() {
                    return;
                }
                tokio::time::sleep(LOOK_EVERY).await;
            }
        };
        let _ = tokio::time::timeout(SETTLE, listed).await;
        Ok(new.to_string())
    }

    /// Invites the old room's joined members to the room that replaced it, unless anyone may
    /// join it: an invitation lets them follow whatever the rule (one who joined a Space members
    /// room by invitation, or left the space, may not be let in otherwise). Best effort, and
    /// outside [`Account::admin_changes`]: a big room's invitations are rate limited.
    pub async fn invite_to_successor(&self, room_id: &str, new: &str) {
        let (Ok(room), Ok(me)) = (self.room(room_id), self.user_id()) else { return };
        if matches!(room.join_rule(), Some(SdkJoinRule::Public)) {
            return;
        }
        let Ok(successor) = self.room(new) else { return };
        let members = room.members(matrix_sdk::RoomMemberships::JOIN).await.unwrap_or_default();
        for member in members.iter().filter(|member| member.user_id() != me) {
            if let Err(error) = successor.invite_user_by_id(member.user_id()).await {
                tracing::warn!(%error, "could not invite a member to the upgraded room");
            }
        }
    }

    /// Adds, removes or orders a space's rooms, then waits for sync to bring it. The engine runs
    /// these through [`Account::admin_changes`], so two quick moves can't be worked out from the
    /// same old order.
    pub async fn change_space(&self, space_id: &str, change: &SpaceChange) -> Result<(), Error> {
        let space = self.room(space_id)?;
        let me = self.user_id()?;
        let via = vec![me.server_name().to_owned()];
        let current = children(&space).await;
        let wanted: BTreeMap<String, Option<String>> = match change {
            SpaceChange::Add(room_id) => {
                let child = RoomId::parse(room_id.as_str())?;
                space.send_state_event_for_key(&child, SpaceChildEventContent::new(via.clone())).await?;
                // The room names the space too, where the user may say so there.
                if let Some(room) = self.client().get_room(&child) {
                    let may = room
                        .power_levels()
                        .await
                        .is_ok_and(|power| power.user_can_send_state(&me, StateEventType::SpaceParent));
                    if may {
                        let _ = room.send_state_event_for_key(space.room_id(), SpaceParentEventContent::new(via)).await;
                    }
                }
                current.iter().map(|c| (c.room_id.clone(), c.order.clone())).chain([(room_id.clone(), None)]).collect()
            }
            SpaceChange::Remove(room_id) => {
                let child = RoomId::parse(room_id.as_str())?;
                space.send_state_event_for_key(&child, SpaceChildEventContent::new(Vec::new())).await?;
                if let Some(room) = self.client().get_room(&child) {
                    let names = room
                        .get_state_event_static_for_key::<SpaceParentEventContent, _>(space.room_id())
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                    let may = room
                        .power_levels()
                        .await
                        .is_ok_and(|power| power.user_can_send_state(&me, StateEventType::SpaceParent));
                    if names && may {
                        let _ = room
                            .send_state_event_for_key(space.room_id(), SpaceParentEventContent::new(Vec::new()))
                            .await;
                    }
                }
                current.iter().filter(|c| &c.room_id != room_id).map(|c| (c.room_id.clone(), c.order.clone())).collect()
            }
            SpaceChange::Order(order) => {
                // An order of other rooms than the space has (one replaced or removed meanwhile)
                // would move the wrong ones: refused, and the list shows the space as it is.
                let listed: std::collections::BTreeSet<&str> = current.iter().map(|c| c.room_id.as_str()).collect();
                let asked: std::collections::BTreeSet<&str> = order.iter().map(String::as_str).collect();
                if listed != asked {
                    return Err(Error::SpaceChanged);
                }
                let changes = reorder(&current, order);
                write_orders(&space, &changes, &me).await?;
                let mut wanted: BTreeMap<_, _> = current.iter().map(|c| (c.room_id.clone(), c.order.clone())).collect();
                for (room_id, order) in changes {
                    wanted.insert(room_id, Some(order));
                }
                wanted
            }
        };
        let settled = async {
            loop {
                let now: BTreeMap<_, _> = children(&space).await.into_iter().map(|c| (c.room_id, c.order)).collect();
                let same = now.len() == wanted.len()
                    && wanted.iter().all(|(id, order)| now.get(id).is_some_and(|now| order.is_none() || now == order));
                if same {
                    return;
                }
                tokio::time::sleep(LOOK_EVERY).await;
            }
        };
        let _ = tokio::time::timeout(SETTLE, settled).await;
        Ok(())
    }

    /// Leaves a space, and first the rooms of it named in `rooms` (each that fails to leave
    /// stops it, so nothing is left behind half done).
    pub async fn leave_space(&self, space_id: &str, rooms: &[String]) -> Result<(), Error> {
        for room_id in rooms {
            if let Ok(room) = self.room(room_id) {
                room.leave().await?;
            }
        }
        self.room(space_id)?.leave().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
