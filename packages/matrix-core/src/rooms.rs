//! Making, naming and joining rooms and spaces: a new room (in a space, or on its own), a new
//! space, a new name for either, and a look at a room the account hasn't joined (a
//! [`RoomPreview`]) before joining it by its ID or an alias.

use matrix_sdk::ruma::api::client::room::create_room::v3::{CreationContent, Request as CreateRoom, RoomPreset};
use matrix_sdk::ruma::events::InitialStateEvent;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::ruma::events::space::parent::SpaceParentEventContent;
use matrix_sdk::ruma::room::RoomType;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedRoomId, OwnedServerName, RoomId, RoomOrAliasId, ServerName};

use crate::Error;
use crate::account::Account;
use crate::model::MediaRef;

/// Who may join a room, as its preview tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum JoinRule {
    /// Anyone.
    Public,
    /// Members of certain other rooms or spaces, and anyone invited.
    Restricted,
    /// Anyone who asks and is let in, and anyone invited.
    Knock,
    /// Only those invited.
    Invite,
}

/// A room as someone who hasn't joined it may see it: what a permalink to it shows before the
/// user joins.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomPreview {
    /// Its ID, also when it was looked up by an alias.
    pub room_id: String,
    /// Its main alias (`#rust:matrix.org`), if it has one.
    pub alias: Option<String>,
    /// Its name, if it has one.
    pub name: Option<String>,
    /// Its topic, if it has one.
    pub topic: Option<String>,
    /// Its picture.
    pub avatar: Option<MediaRef>,
    /// How many people have joined.
    pub member_count: u64,
    /// Who may join, when the server says.
    pub join_rule: Option<JoinRule>,
    /// Whether it is a space.
    pub space: bool,
    /// Whether this account has joined it already.
    pub joined: bool,
    /// Whether this account is invited to it, so it may join whatever the rule.
    pub invited: bool,
}

impl RoomPreview {
    /// Whether joining may work: the account is invited, or the rule lets anyone (or members of
    /// another room, which only the server can check) in. Asking to join isn't offered.
    pub fn joinable(&self) -> bool {
        self.invited || matches!(self.join_rule, None | Some(JoinRule::Public | JoinRule::Restricted))
    }
}

/// Our preview of the SDK's.
fn preview_of(preview: matrix_sdk::room_preview::RoomPreview) -> RoomPreview {
    use matrix_sdk::RoomState;
    use matrix_sdk::ruma::room::JoinRuleSummary;
    let join_rule = preview.join_rule.map(|rule| match rule {
        JoinRuleSummary::Public => JoinRule::Public,
        JoinRuleSummary::Restricted(_) | JoinRuleSummary::KnockRestricted(_) => JoinRule::Restricted,
        JoinRuleSummary::Knock => JoinRule::Knock,
        // Private is reserved and unknown rules can't be followed: both as invite-only.
        _ => JoinRule::Invite,
    });
    RoomPreview {
        room_id: preview.room_id.to_string(),
        alias: preview.canonical_alias.map(|alias| alias.to_string()),
        name: preview.name.filter(|name| !name.trim().is_empty()),
        topic: preview.topic.filter(|topic| !topic.trim().is_empty()),
        avatar: preview.avatar_url.and_then(crate::convert::avatar),
        member_count: preview.num_joined_members,
        join_rule,
        space: preview.room_type == Some(RoomType::Space),
        joined: preview.state == Some(RoomState::Joined),
        invited: preview.state == Some(RoomState::Invited),
    }
}

/// The servers among `via` that are server names; the others are left out.
fn servers(via: &[String]) -> Vec<OwnedServerName> {
    via.iter().filter_map(|server| ServerName::parse(server).ok()).collect()
}

/// Which destinations to find in a homeserver's public directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum DirectoryKind {
    /// Public conversations.
    Rooms,
    /// Public spaces.
    Spaces,
}

impl Account {
    /// Search the account's homeserver directory, keeping at most 20 public destinations.
    /// Separate room-type requests keep popular rooms from crowding out spaces. Read at most
    /// six pages when a server ignores the type filter; never enumerate the whole directory.
    pub async fn search_directory(&self, term: &str, kind: DirectoryKind) -> Result<Vec<RoomPreview>, Error> {
        use matrix_sdk::ruma::api::client::directory::get_public_rooms_filtered::v3::Request;
        use matrix_sdk::ruma::directory::RoomTypeFilter;
        use matrix_sdk::ruma::room::JoinRuleKind;
        use std::collections::HashSet;

        let term = term.trim();
        if term.is_empty() {
            return Ok(Vec::new());
        }
        let mut since = None;
        let mut tokens = HashSet::new();
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for _ in 0..6 {
            let mut request = Request::new();
            request.limit = Some(50u16.into());
            request.since = since;
            request.filter.generic_search_term = Some(term.to_owned());
            request.filter.room_types = vec![match kind {
                DirectoryKind::Rooms => RoomTypeFilter::Default,
                DirectoryKind::Spaces => RoomTypeFilter::Space,
            }];
            let response = self.client().send(request).await?;
            for room in response.chunk {
                let space = room.room_type == Some(RoomType::Space);
                if space != (kind == DirectoryKind::Spaces)
                    || room.join_rule != JoinRuleKind::Public
                    || !seen.insert(room.room_id.clone())
                {
                    continue;
                }
                let state = self.client().get_room(&room.room_id).map(|room| room.state());
                found.push(RoomPreview {
                    room_id: room.room_id.to_string(),
                    alias: room.canonical_alias.map(|alias| alias.to_string()),
                    name: room.name.filter(|name| !name.trim().is_empty()),
                    topic: room.topic.filter(|topic| !topic.trim().is_empty()),
                    avatar: room.avatar_url.and_then(crate::convert::avatar),
                    member_count: room.num_joined_members.into(),
                    join_rule: Some(JoinRule::Public),
                    space,
                    joined: state == Some(matrix_sdk::RoomState::Joined),
                    invited: state == Some(matrix_sdk::RoomState::Invited),
                });
                if found.len() == 20 {
                    return Ok(found);
                }
            }
            since = response.next_batch;
            let Some(token) = &since else { break };
            if !tokens.insert(token.clone()) {
                return Err(Error::RepeatedPaginationToken { resource: "public directory".into() });
            }
        }
        Ok(found)
    }

    /// Public rooms anywhere under a space, with all hierarchy pages included before ranking.
    pub async fn public_rooms(&self, space: &str) -> Result<Vec<crate::PublicRoom>, Error> {
        use matrix_sdk::ruma::api::client::space::get_hierarchy::v1::Request;
        use matrix_sdk::ruma::room::{JoinRuleSummary, RoomType};
        use std::collections::{HashMap, HashSet};

        let space_id = RoomId::parse(space)?;
        let mut from = None;
        let mut tokens = HashSet::new();
        let mut rooms = HashMap::new();
        let mut servers: HashMap<String, Vec<String>> = HashMap::new();
        loop {
            let mut request = Request::new(space_id.clone());
            request.from = from;
            request.limit = Some(100u16.into());
            let response = self.client().send(request).await?;
            for chunk in response.rooms {
                for child in chunk.children_state {
                    if let Ok(child) = child.deserialize() {
                        servers
                            .entry(child.state_key.to_string())
                            .or_default()
                            .extend(child.content.via.iter().map(ToString::to_string));
                    }
                }
                let summary = chunk.summary;
                if summary.room_id == space_id
                    || summary.room_type == Some(RoomType::Space)
                    || summary.join_rule != JoinRuleSummary::Public
                {
                    continue;
                }
                let room_id = summary.room_id.to_string();
                let name = summary
                    .name
                    .or_else(|| summary.canonical_alias.map(|alias| alias.to_string()))
                    .unwrap_or_else(|| room_id.clone());
                rooms.insert(
                    room_id.clone(),
                    crate::PublicRoom {
                        room_id,
                        name,
                        topic: summary.topic,
                        avatar: summary.avatar_url.and_then(crate::convert::avatar),
                        member_count: summary.num_joined_members.into(),
                        via: Vec::new(),
                    },
                );
            }
            from = response.next_batch;
            let Some(token) = &from else { break };
            if !tokens.insert(token.clone()) {
                return Err(Error::RepeatedPaginationToken { resource: format!("space {space_id}") });
            }
        }
        let mut rooms: Vec<_> = rooms.into_values().collect();
        for room in &mut rooms {
            room.via = servers.remove(&room.room_id).unwrap_or_default();
            if room.via.is_empty() {
                let room_id = RoomId::parse(&room.room_id)?;
                let server = space_id
                    .server_name()
                    .or_else(|| room_id.server_name())
                    .map(ToOwned::to_owned)
                    .unwrap_or(self.user_id()?.server_name().to_owned());
                room.via.push(server.to_string());
            }
            let mut seen = HashSet::new();
            room.via.retain(|server| seen.insert(server.clone()));
        }
        rooms.sort_by(|a, b| {
            b.member_count
                .cmp(&a.member_count)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.room_id.cmp(&b.room_id))
        });
        Ok(rooms)
    }

    /// Creates a private, encrypted room, inside `space` when given (the room lists it as its
    /// parent and the space as its child); returns its ID.
    pub async fn create_room(&self, name: &str, topic: Option<&str>, space: Option<&str>) -> Result<String, Error> {
        let mut request = CreateRoom::new();
        request.name = Some(name.trim().to_owned());
        request.topic = topic.map(str::trim).filter(|topic| !topic.is_empty()).map(ToOwned::to_owned);
        request.preset = Some(RoomPreset::PrivateChat);
        let encryption =
            InitialStateEvent::with_empty_state_key(RoomEncryptionEventContent::with_recommended_defaults());
        request.initial_state = vec![Raw::new(&encryption).map_err(|e| Error::Storage(e.into()))?.cast_unchecked()];
        let room = self.client().create_room(request).await?;
        if let Some(space) = space {
            let space_id = RoomId::parse(space)?;
            let via = vec![self.user_id()?.server_name().to_owned()];
            let space_room = self
                .client()
                .get_room(&space_id)
                .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: space.to_owned() })?;
            space_room.send_state_event_for_key(room.room_id(), SpaceChildEventContent::new(via.clone())).await?;
            let parent: OwnedRoomId = space_id;
            room.send_state_event_for_key(&parent, SpaceParentEventContent::new(via)).await?;
        }
        Ok(room.room_id().to_string())
    }

    /// Creates a space; returns its ID.
    pub async fn create_space(&self, name: &str) -> Result<String, Error> {
        let mut request = CreateRoom::new();
        request.name = Some(name.trim().to_owned());
        request.preset = Some(RoomPreset::PrivateChat);
        let mut creation = CreationContent::new();
        creation.room_type = Some(RoomType::Space);
        request.creation_content = Some(Raw::new(&creation).map_err(|e| Error::Storage(e.into()))?);
        Ok(self.client().create_room(request).await?.room_id().to_string())
    }

    /// What `room` (an ID or an alias) looks like from outside, asking the `via` servers when
    /// this account's server doesn't know it. Fails when no server will describe it (a private
    /// room the account isn't in).
    pub async fn preview_room(&self, room: &str, via: &[String]) -> Result<RoomPreview, Error> {
        let room = RoomOrAliasId::parse(room)?;
        Ok(preview_of(self.client().get_room_preview(&room, servers(via)).await?))
    }

    /// Joins `room` (an ID or an alias) through the `via` servers, accepting an invitation when
    /// one exists; returns its ID. Advertised servers let the homeserver find rooms elsewhere.
    pub async fn join_room(&self, room: &str, via: &[String]) -> Result<String, Error> {
        let room = RoomOrAliasId::parse(room)?;
        Ok(self.client().join_room_by_id_or_alias(&room, &servers(via)).await?.room_id().to_string())
    }

    /// Gives a room or space a new name.
    pub async fn rename_room(&self, room_id: &str, name: &str) -> Result<(), Error> {
        let room = self
            .client()
            .get_room(&RoomId::parse(room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.to_owned() })?;
        room.set_name(name.trim().to_owned()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
