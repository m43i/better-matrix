//! Making and naming rooms and spaces: a new room (in a space, or on its own), a new space, and
//! a new name for either.

use matrix_sdk::ruma::api::client::room::create_room::v3::{CreationContent, Request as CreateRoom, RoomPreset};
use matrix_sdk::ruma::events::InitialStateEvent;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::ruma::events::space::parent::SpaceParentEventContent;
use matrix_sdk::ruma::room::RoomType;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedRoomId, RoomId};

use crate::Error;
use crate::account::Account;

impl Account {
    /// Joins a room, accepting an invitation when one exists. Advertised servers let the
    /// homeserver find public rooms on other servers.
    pub async fn join_room(&self, room_id: &str, via: &[String]) -> Result<(), Error> {
        let room_id = matrix_sdk::ruma::RoomOrAliasId::parse(room_id)?;
        let via = via.iter().map(matrix_sdk::ruma::ServerName::parse).collect::<Result<Vec<_>, _>>()?;
        self.client().join_room_by_id_or_alias(&room_id, &via).await?;
        Ok(())
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
