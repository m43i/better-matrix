//! People: a room's members and whether the user may invite more, finding people in the
//! homeserver's user directory, inviting them, and the direct chat with someone.
//!
//! Matrix is federated: the directory knows the people on the user's own server and those it has
//! seen in shared or public rooms, never everyone. So search and suggestions put people on the
//! user's own server first, and a full user ID typed from any server is looked up on its own.

use std::collections::HashSet;

use futures_util::StreamExt;
use matrix_sdk::RoomMemberships;
use matrix_sdk::ruma::api::client::profile::{AvatarUrl, DisplayName};
use matrix_sdk::ruma::events::presence::PresenceEvent;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{OwnedUserId, RoomId, UserId};

use crate::Error;
use crate::account::Account;
use crate::convert;
use crate::model::Person;

/// What someone may do in a room, by their power level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum MemberRole {
    /// Power level 100 or more, or the room's creator.
    Admin,
    /// Power level 50 or more.
    Moderator,
    /// Everyone else.
    #[default]
    Member,
}

/// Whether someone is around, as their homeserver last said (servers may not share presence).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum Presence {
    /// Active now.
    Online,
    /// Connected but away.
    Away,
    /// Not connected.
    Offline,
    /// Their server shares no presence.
    #[default]
    Unknown,
}

/// What someone shows others, as their homeserver last said: whether they are around, and the
/// status message they set (Matrix presence's `status_msg`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PersonPresence {
    /// Whose.
    pub user_id: String,
    /// Whether they are around.
    pub presence: Presence,
    /// What they say they are up to ("🌴 On vacation"); `None` when they set none.
    pub status_message: Option<String>,
}

/// One joined member, as the member list shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomMember {
    /// Who they are.
    pub person: Person,
    /// What they may do in the room.
    pub role: MemberRole,
    /// Whether they are around.
    pub presence: Presence,
    /// The status message they set, if any.
    pub status_message: Option<String>,
    /// Whether the user verified them (their cross-signing identity).
    pub verified: bool,
    /// Their power level; a room creator's counts as the highest.
    pub level: i64,
}

impl MemberRole {
    /// The power level the role is given.
    pub fn level(self) -> i64 {
        match self {
            Self::Admin => 100,
            Self::Moderator => 50,
            Self::Member => 0,
        }
    }
}

/// What a moderator does to a member, or to someone asking to join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Moderation {
    /// Remove them from the room; they may join again.
    Kick,
    /// Remove them and keep them out.
    Ban,
    /// Give them a role.
    Role(MemberRole),
    /// Give them a power level of the user's choosing.
    Level(i64),
    /// Let someone banned join again (after an invitation, or as the room's rule allows).
    Unban,
    /// Let someone asking to join in: they are invited.
    Admit,
    /// Turn down someone asking to join.
    Deny,
}

/// A room's joined members, as the member list shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Members {
    /// Everyone joined: those around first, then by role, then by name; only those this device
    /// knows unless `complete`.
    pub people: Vec<RoomMember>,
    /// Whether `people` is everyone.
    pub complete: bool,
    /// Whether the user may invite others to the room.
    pub can_invite: bool,
    /// The user's own power level: they act only on members below it.
    pub own_level: i64,
    /// Whether the user may kick members.
    pub can_kick: bool,
    /// Whether the user may ban members.
    pub can_ban: bool,
    /// Whether the user may change members' roles.
    pub can_set_roles: bool,
    /// Those asking to join (when the room lets people ask), by name.
    pub knocking: Vec<RoomMember>,
    /// Those banned, by name.
    pub banned: Vec<RoomMember>,
}

impl Members {
    /// Whether the user may do `action` to `member`: the room allows it and they rank above them.
    pub fn may(&self, action: Moderation, member: &RoomMember) -> bool {
        let allowed = match action {
            Moderation::Kick => self.can_kick,
            Moderation::Ban => self.can_ban,
            // By the exact level: someone at a custom 25 may be set back to Member (0).
            Moderation::Role(role) => {
                self.can_set_roles && role.level() <= self.own_level && role.level() != member.level
            }
            Moderation::Level(level) => self.can_set_roles && level <= self.own_level && level != member.level,
            // Lifting a ban is a kick from it: both rights (Matrix's authorization rules).
            Moderation::Unban => self.can_ban && self.can_kick,
            // An invitation needs no rank above the one invited.
            Moderation::Admit => return self.can_invite,
            Moderation::Deny => self.can_kick,
        };
        allowed && member.level < self.own_level
    }
}

/// Someone asking to join or banned, as the member list shows them: no presence or verification.
fn outsider(member: &matrix_sdk::room::RoomMember) -> RoomMember {
    use matrix_sdk::ruma::events::room::power_levels::UserPowerLevel;
    let level = match member.power_level() {
        UserPowerLevel::Int(level) => i64::from(level),
        _ => i64::MAX,
    };
    RoomMember {
        person: Person {
            id: member.user_id().to_string(),
            name: member.name().to_owned(),
            avatar: member.avatar_url().map(ToOwned::to_owned).and_then(convert::avatar),
        },
        role: MemberRole::Member,
        presence: Presence::Unknown,
        status_message: None,
        verified: false,
        level,
    }
}

/// Whether the store's view of a member already shows `action` done to them.
fn shows(member: &matrix_sdk::room::RoomMember, action: Moderation) -> bool {
    use matrix_sdk::ruma::events::room::member::MembershipState;
    use matrix_sdk::ruma::events::room::power_levels::UserPowerLevel;
    match action {
        Moderation::Kick => *member.membership() != MembershipState::Join,
        Moderation::Ban => *member.membership() == MembershipState::Ban,
        Moderation::Unban => *member.membership() != MembershipState::Ban,
        Moderation::Admit => *member.membership() == MembershipState::Invite,
        Moderation::Deny => *member.membership() != MembershipState::Knock,
        Moderation::Role(role) => {
            matches!(member.power_level(), UserPowerLevel::Int(level) if i64::from(level) == role.level())
        }
        Moderation::Level(wanted) => {
            matches!(member.power_level(), UserPowerLevel::Int(level) if i64::from(level) == wanted)
        }
    }
}

impl Account {
    /// The room's joined members: from the store, or with `all`, everyone (fetched from the
    /// server once per room).
    pub async fn members(&self, room_id: &str, all: bool) -> Result<Members, Error> {
        let room = self
            .client()
            .get_room(&RoomId::parse(room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.to_owned() })?;
        let members = if all {
            room.members(RoomMemberships::JOIN).await?
        } else {
            room.members_no_sync(RoomMemberships::JOIN).await?
        };
        let complete = all || room.are_members_synced();
        /// Verification is looked up for this many members at most: one crypto-store read each.
        const VERIFIED_UP_TO: usize = 1000;
        /// How many members' verification is read at once.
        const LOOKUPS_AT_ONCE: usize = 32;
        let check_verified = members.len() <= VERIFIED_UP_TO;
        // Everyone's presence in one store read.
        let ids: Vec<OwnedUserId> = members.iter().map(|member| member.user_id().to_owned()).collect();
        let mut presence: std::collections::HashMap<OwnedUserId, PersonPresence> =
            match self.client().state_store().get_presence_events(&ids).await {
                Ok(events) => events
                    .iter()
                    .filter_map(|raw| raw.deserialize().ok())
                    .map(|event| (event.sender.clone(), person_presence(&event)))
                    .collect(),
                Err(_) => std::collections::HashMap::new(),
            };
        // One verification read per member: many at a time, as a large room has thousands.
        let client = self.client().clone();
        let mut people: Vec<RoomMember> = futures_util::stream::iter(members.iter().cloned())
            .map(move |member| {
                let client = client.clone();
                let shown = presence.remove(member.user_id());
                async move {
                    let encryption = client.encryption();
                    use matrix_sdk::ruma::events::room::power_levels::UserPowerLevel;
                    // A room's creators (v12) rank above every level.
                    let level = match member.power_level() {
                        UserPowerLevel::Int(level) => i64::from(level),
                        _ => i64::MAX,
                    };
                    // By the level itself (a room's top level may be above 100), as the
                    // moderation menu compares levels.
                    let role = match level {
                        100.. => MemberRole::Admin,
                        50.. => MemberRole::Moderator,
                        _ => MemberRole::Member,
                    };
                    let verified = check_verified
                        && encryption
                            .get_user_identity(member.user_id())
                            .await
                            .ok()
                            .flatten()
                            .is_some_and(|id| id.is_verified());
                    RoomMember {
                        person: Person {
                            id: member.user_id().to_string(),
                            name: member.name().to_owned(),
                            avatar: member.avatar_url().map(ToOwned::to_owned).and_then(convert::avatar),
                        },
                        role,
                        presence: shown.as_ref().map_or(Presence::Unknown, |shown| shown.presence),
                        status_message: shown.and_then(|shown| shown.status_message),
                        verified,
                        level,
                    }
                }
            })
            .buffered(LOOKUPS_AT_ONCE)
            .collect()
            .await;
        sort_members(&mut people);
        let me = self.user_id()?;
        let own = room.get_member_no_sync(&me).await?;
        let can_invite = own.as_ref().is_some_and(|member| member.can_invite());
        let own_level = own.as_ref().map_or(0, |member| match member.power_level() {
            matrix_sdk::ruma::events::room::power_levels::UserPowerLevel::Int(level) => i64::from(level),
            _ => i64::MAX,
        });
        let power = room.power_levels().await.ok();
        let may = |check: &dyn Fn(&matrix_sdk::ruma::events::room::power_levels::RoomPowerLevels) -> bool| {
            power.as_ref().is_some_and(check)
        };
        // Those asking to join and those banned: few, so as the store has them, by name.
        let outside = |membership: RoomMemberships| {
            let room = room.clone();
            async move {
                let mut people: Vec<RoomMember> =
                    room.members_no_sync(membership).await.unwrap_or_default().iter().map(outsider).collect();
                people.sort_by_cached_key(|member| member.person.name.to_lowercase());
                people
            }
        };
        let knocking = outside(RoomMemberships::KNOCK).await;
        let banned = outside(RoomMemberships::BAN).await;
        Ok(Members {
            people,
            complete,
            knocking,
            banned,
            can_invite,
            own_level,
            can_kick: may(&|p| p.user_can_kick(&me)),
            can_ban: may(&|p| p.user_can_ban(&me)),
            can_set_roles: may(&|p| {
                p.user_can_send_state(&me, matrix_sdk::ruma::events::StateEventType::RoomPowerLevels)
            }),
        })
    }

    /// Kicks, bans, unbans or changes the power level of a member of a room, or answers someone
    /// asking to join.
    pub async fn moderate(&self, room_id: &str, user_id: &str, action: Moderation) -> Result<(), Error> {
        let room = self
            .client()
            .get_room(&RoomId::parse(room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.to_owned() })?;
        let user = UserId::parse(user_id)?;
        // An answer to a request to join, or an unban, only while it still stands: denying
        // someone another moderator let in meanwhile would remove a member.
        use matrix_sdk::ruma::events::room::member::MembershipState;
        let needs = match action {
            Moderation::Admit | Moderation::Deny => Some(MembershipState::Knock),
            Moderation::Unban => Some(MembershipState::Ban),
            _ => None,
        };
        if let Some(needs) = needs
            && self.membership(&room, &user).await?.as_ref() != Some(&needs)
        {
            // The store missed the change: the member list after this reads them afresh.
            room.mark_members_missing();
            return Err(Error::Outdated);
        }
        match action {
            Moderation::Kick | Moderation::Deny => room.kick_user(&user, None).await?,
            Moderation::Ban => room.ban_user(&user, None).await?,
            Moderation::Unban => room.unban_user(&user, None).await?,
            Moderation::Admit => room.invite_user_by_id(&user).await?,
            Moderation::Role(role) => {
                let level = matrix_sdk::ruma::Int::new(role.level()).unwrap_or_default();
                room.update_power_levels(vec![(&user, level)]).await?;
            }
            Moderation::Level(level) => {
                room.update_power_levels(vec![(&user, matrix_sdk::ruma::Int::new_saturating(level))]).await?;
            }
        }
        // The member list reads the store, which the change reaches with the next sync: wait
        // for it (briefly), so the list then shows it.
        const SETTLE: std::time::Duration = std::time::Duration::from_secs(10);
        const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(250);
        let settled = async {
            loop {
                let member = room.get_member_no_sync(&user).await.ok().flatten();
                if member.is_none_or(|member| shows(&member, action)) {
                    return;
                }
                tokio::time::sleep(LOOK_EVERY).await;
            }
        };
        let _ = tokio::time::timeout(SETTLE, settled).await;
        Ok(())
    }

    /// Someone's membership of a room as the homeserver has it now (the store may have missed a
    /// change in a gap); `None` when they never had one.
    async fn membership(
        &self,
        room: &matrix_sdk::Room,
        user: &UserId,
    ) -> Result<Option<matrix_sdk::ruma::events::room::member::MembershipState>, Error> {
        use matrix_sdk::ruma::api::client::state::get_state_event_for_key::v3::Request;
        use matrix_sdk::ruma::api::error::ErrorKind;
        use matrix_sdk::ruma::events::StateEventType;
        use matrix_sdk::ruma::events::room::member::RoomMemberEventContent;
        let request = Request::new(room.room_id().to_owned(), StateEventType::RoomMember, user.to_string());
        match self.client().send(request).await {
            Ok(response) => Ok(response
                .into_content()
                .deserialize_as_unchecked::<RoomMemberEventContent>()
                .ok()
                .map(|content| content.membership)),
            Err(error) if error.client_api_error_kind() == Some(&ErrorKind::NotFound) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// People matching `term`, by name or ID: the homeserver's user directory, and a typed user
    /// ID from any server (even when the directory doesn't know it). Ranked by [`rank`].
    pub async fn search_people(&self, term: &str) -> Result<Vec<Person>, Error> {
        self.search_people_ranked(term, &crate::SearchUsage::default()).await
    }

    pub(crate) async fn search_people_ranked(
        &self,
        term: &str,
        usage: &crate::SearchUsage,
    ) -> Result<Vec<Person>, Error> {
        /// Enough to pick from without scrolling far.
        const LIMIT: usize = 20;
        /// Asked of the directory: more than shown, so that a page of people on other servers
        /// can't push those on the user's own server out of the list.
        const ASKED: u64 = 50;
        let term = term.trim();
        let typed = UserId::parse(term).ok();
        let mut people: Vec<Person> = match self.client().search_users(term, ASKED).await {
            Ok(response) => response
                .results
                .into_iter()
                .map(|user| Person {
                    name: user.display_name.unwrap_or_else(|| user.user_id.to_string()),
                    avatar: user.avatar_url.and_then(convert::avatar),
                    id: user.user_id.to_string(),
                })
                .collect(),
            // A typed user ID still stands on its own.
            Err(_) if typed.is_some() => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        if let Some(user) = typed
            && !people.iter().any(|person| person.id == user.as_str())
        {
            people.push(self.profile_of(&user).await);
        }
        rank(&mut people, term, self.user_id()?.server_name().as_str());
        people
            .sort_by_key(|person| (!person.id.eq_ignore_ascii_case(term), std::cmp::Reverse(usage.person(&person.id))));
        people.truncate(LIMIT);
        Ok(people)
    }

    /// Someone's public name and picture, asked of their server; just the ID when it can't say.
    async fn profile_of(&self, user: &UserId) -> Person {
        let profile = self.client().account().fetch_user_profile_of(user).await.ok();
        let name = profile.as_ref().and_then(|p| p.get_static::<DisplayName>().ok().flatten());
        let avatar = profile.and_then(|p| p.get_static::<AvatarUrl>().ok().flatten()).and_then(convert::avatar);
        Person { id: user.to_string(), name: name.unwrap_or_else(|| user.to_string()), avatar }
    }

    /// People to start a chat with: those the user shares the most rooms with, among the
    /// members this device already knows, leaving out anyone they already have a direct chat with.
    pub async fn suggest_people(&self) -> Result<Vec<Person>, Error> {
        /// A short list to pick from at a glance.
        const LIMIT: usize = 12;
        let me = self.user_id()?;
        let mut seen: std::collections::HashMap<String, (Person, usize)> = std::collections::HashMap::new();
        // Everyone with a direct chat, from one pass over the rooms rather than one per member:
        // `Client::get_dm_room`'s rule (the SDK's default, spec definition), a room whose only
        // direct target they are.
        let mut chatting: HashSet<OwnedUserId> = HashSet::new();
        for room in self.client().joined_rooms() {
            let targets = room.direct_targets();
            if targets.len() == 1
                && let Some(user) = targets.iter().next().and_then(|target| target.as_user_id())
            {
                chatting.insert(user.to_owned());
            }
        }
        for room in self.client().joined_rooms() {
            if room.is_space() || room.is_direct().await.unwrap_or(false) {
                continue;
            }
            for member in room.members_no_sync(RoomMemberships::JOIN).await.unwrap_or_default() {
                let user = member.user_id();
                if user == me || chatting.contains(user) {
                    continue;
                }
                let entry = seen.entry(user.to_string()).or_insert_with(|| {
                    let person = Person {
                        id: user.to_string(),
                        name: member.name().to_owned(),
                        avatar: member.avatar_url().map(ToOwned::to_owned).and_then(convert::avatar),
                    };
                    (person, 0)
                });
                entry.1 += 1;
            }
        }
        let home = me.server_name().as_str();
        let mut ranked: Vec<(Person, usize)> = seen.into_values().collect();
        ranked.sort_by(|(a, shared_a), (b, shared_b)| {
            on_server(&b.id, home)
                .cmp(&on_server(&a.id, home))
                .then_with(|| shared_b.cmp(shared_a))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(ranked.into_iter().take(LIMIT).map(|(person, _)| person).collect())
    }

    /// Invites someone to a room.
    pub async fn invite(&self, room_id: &str, user_id: &str) -> Result<(), Error> {
        let room = self
            .client()
            .get_room(&RoomId::parse(room_id)?)
            .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.to_owned() })?;
        Ok(room.invite_user_by_id(&UserId::parse(user_id)?).await?)
    }

    /// The direct chat with someone: the existing one with them alone (a third person who
    /// joined one would hear its calls), or a new one inviting them.
    pub async fn direct_chat(&self, user_id: &str) -> Result<String, Error> {
        let user = UserId::parse(user_id)?;
        if let Some(room) = self.client().get_dm_rooms(&user).find(|room| room.active_members_count() <= 2) {
            return Ok(room.room_id().to_string());
        }
        Ok(self.client().create_dm(&user).await?.room_id().to_string())
    }
}

/// The member list's order: those around first, then by role, then by name.
fn sort_members(people: &mut [RoomMember]) {
    let around = |presence: Presence| match presence {
        Presence::Online => 0,
        Presence::Away => 1,
        Presence::Offline | Presence::Unknown => 2,
    };
    people.sort_by_cached_key(|member| (around(member.presence), member.role, member.person.name.to_lowercase()));
}

impl Account {
    /// What the people in the user's direct chats show, as this device stored it: what the apps
    /// show until a sync brings changes. The user's own is read from the server instead
    /// ([`Account::report_own_presence`]).
    pub(crate) async fn known_presence(&self) -> Vec<PersonPresence> {
        let me = self.user_id().ok();
        let mut people: Vec<OwnedUserId> = Vec::new();
        for room in self.client().joined_rooms() {
            for target in room.direct_targets() {
                if let Some(user) = target.as_user_id()
                    && me.as_deref() != Some(user)
                    && !people.iter().any(|known| known == user)
                {
                    people.push(user.to_owned());
                }
            }
        }
        match self.client().state_store().get_presence_events(&people).await {
            Ok(events) => events.iter().filter_map(|raw| raw.deserialize().ok()).map(|e| person_presence(&e)).collect(),
            Err(error) => {
                eprintln!("matrix-engine: could not read stored presence for {}: {error}", self.id());
                Vec::new()
            }
        }
    }
}

/// What a sync's presence events say people show now.
pub(crate) fn synced_presence(events: &[Raw<PresenceEvent>]) -> Vec<PersonPresence> {
    events.iter().filter_map(|raw| raw.deserialize().ok()).map(|event| person_presence(&event)).collect()
}

/// What an event says someone shows. A presence the apps don't know shows nothing, its message
/// included.
fn person_presence(event: &PresenceEvent) -> PersonPresence {
    let presence = presence(&event.content.presence);
    let status_message = match presence {
        Presence::Unknown => None,
        _ => status_message(event.content.status_msg.as_deref()),
    };
    PersonPresence { user_id: event.sender.to_string(), presence, status_message }
}

/// A status message as shown: trimmed, and none when only blank.
pub(crate) fn status_message(message: Option<&str>) -> Option<String> {
    message.map(str::trim).filter(|message| !message.is_empty()).map(ToOwned::to_owned)
}

pub(crate) fn presence(state: &matrix_sdk::ruma::presence::PresenceState) -> Presence {
    use matrix_sdk::ruma::presence::PresenceState;
    match state {
        PresenceState::Online => Presence::Online,
        PresenceState::Unavailable => Presence::Away,
        PresenceState::Offline => Presence::Offline,
        _ => Presence::Unknown,
    }
}

/// Orders people found for `term`: the person whose ID was typed, then those on the user's own
/// server (`home`), then everyone else, each group keeping the directory's order of relevance.
fn rank(people: &mut [Person], term: &str, home: &str) {
    people.sort_by_key(|person| (!person.id.eq_ignore_ascii_case(term), !on_server(&person.id, home)));
}

/// Whether the user ID `id` lives on the server `home`.
fn on_server(id: &str, home: &str) -> bool {
    id.split_once(':').is_some_and(|(_, server)| server.eq_ignore_ascii_case(home))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sync with a group room of @a (the user), @b and @c, a direct chat with @b, and @b online.
    fn people_response(request: &str) -> serde_json::Value {
        use serde_json::json;
        if !request.contains("/sync?") {
            return crate::test_server::thread_response(request);
        }
        let room = |members: &[&str]| {
            let mut events = vec![json!({"type":"m.room.create", "state_key":"", "sender":"@a:example.org",
                "event_id":"$create", "origin_server_ts":0, "content":{"room_version":"10"}})];
            events.extend(members.iter().map(|user| {
                json!({"type":"m.room.member", "state_key":user, "sender":user, "event_id":format!("$join-{user}"),
                    "origin_server_ts":0, "content":{"membership":"join"}})
            }));
            json!({"state":{"events":events}, "timeline":{"events":[], "limited":false}})
        };
        json!({"next_batch":"sync",
        "account_data":{"events":[{"type":"m.direct", "content":{"@b:example.org":["!dm:example.org"]}}]},
        "presence":{"events":[{"type":"m.presence", "sender":"@b:example.org", "content":{"presence":"online"}}]},
        "rooms":{"join":{
            "!group:example.org": room(&["@a:example.org", "@b:example.org", "@c:example.org"]),
            "!dm:example.org": room(&["@a:example.org", "@b:example.org"]),
        }}})
    }

    async fn synced_people() -> (crate::test_server::Server, Account) {
        let server = crate::test_server::Server::new(people_response);
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync rooms");
        (server, account)
    }

    #[tokio::test]
    async fn suggestions_should_leave_out_people_with_a_direct_chat() {
        let (_server, account) = synced_people().await;
        let suggested = account.suggest_people().await.expect("suggestions");
        assert_eq!(suggested.iter().map(|person| person.id.as_str()).collect::<Vec<_>>(), ["@c:example.org"]);
    }

    #[tokio::test]
    async fn a_member_list_should_show_each_members_stored_presence() {
        let (_server, account) = synced_people().await;
        let members = account.members("!group:example.org", false).await.expect("members");
        let shown: Vec<(&str, Presence)> =
            members.people.iter().map(|member| (member.person.id.as_str(), member.presence)).collect();
        assert_eq!(
            shown,
            [
                ("@b:example.org", Presence::Online),
                ("@a:example.org", Presence::Unknown),
                ("@c:example.org", Presence::Unknown)
            ]
        );
    }

    fn presence_event(content: serde_json::Value) -> Raw<PresenceEvent> {
        serde_json::from_value(serde_json::json!({ "type": "m.presence", "sender": "@kai:x.org", "content": content }))
            .unwrap()
    }

    #[test]
    fn a_synced_presence_should_carry_the_status_message() {
        let events = [presence_event(serde_json::json!({ "presence": "unavailable", "status_msg": " 🌴 Away " }))];
        let shown = synced_presence(&events);
        assert_eq!(
            shown,
            vec![PersonPresence {
                user_id: "@kai:x.org".into(),
                presence: Presence::Away,
                status_message: Some("🌴 Away".into()),
            }]
        );
    }

    #[test]
    fn an_unknown_presence_should_carry_no_status_message() {
        let events = [presence_event(serde_json::json!({ "presence": "busy", "status_msg": "Deep work" }))];
        let shown = synced_presence(&events);
        assert_eq!((shown[0].presence, shown[0].status_message.as_deref()), (Presence::Unknown, None));
    }

    #[test]
    fn a_blank_status_message_should_count_as_none() {
        let events = [presence_event(serde_json::json!({ "presence": "online", "status_msg": "  " }))];
        assert_eq!(synced_presence(&events)[0].status_message, None);
    }

    fn people(ids: &[&str]) -> Vec<Person> {
        ids.iter().map(|id| Person { id: (*id).to_owned(), name: (*id).to_owned(), avatar: None }).collect()
    }

    fn ids(people: &[Person]) -> Vec<&str> {
        people.iter().map(|person| person.id.as_str()).collect()
    }

    fn member(name: &str, presence: Presence, role: MemberRole) -> RoomMember {
        RoomMember {
            person: Person { id: name.into(), name: name.into(), avatar: None },
            role,
            presence,
            status_message: None,
            verified: false,
            level: role.level(),
        }
    }

    #[test]
    fn the_member_list_should_put_those_around_first_then_roles_then_names() {
        let mut list = vec![
            member("zoe", Presence::Online, MemberRole::Member),
            member("amy", Presence::Offline, MemberRole::Admin),
            member("bob", Presence::Online, MemberRole::Admin),
            member("cy", Presence::Away, MemberRole::Member),
        ];
        sort_members(&mut list);
        let names: Vec<&str> = list.iter().map(|m| m.person.name.as_str()).collect();
        assert_eq!(names, ["bob", "zoe", "cy", "amy"]);
    }

    #[test]
    fn a_moderator_should_act_only_on_those_below_them() {
        let members =
            Members { own_level: 50, can_kick: true, can_ban: true, can_set_roles: true, ..Members::default() };
        let admin = member("a", Presence::Online, MemberRole::Admin);
        let plain = member("b", Presence::Online, MemberRole::Member);
        assert!(!members.may(Moderation::Kick, &admin) && members.may(Moderation::Kick, &plain));
        assert!(members.may(Moderation::Role(MemberRole::Moderator), &plain));
        assert!(!members.may(Moderation::Role(MemberRole::Admin), &plain));
    }

    #[test]
    fn answering_join_requests_should_follow_the_invite_and_kick_rights() {
        let asking = member("k", Presence::Unknown, MemberRole::Member);
        let inviter = Members { own_level: 50, can_invite: true, ..Members::default() };
        assert!(inviter.may(Moderation::Admit, &asking) && !inviter.may(Moderation::Deny, &asking));
        let kicker = Members { own_level: 50, can_kick: true, ..Members::default() };
        assert!(kicker.may(Moderation::Deny, &asking) && !kicker.may(Moderation::Admit, &asking));
    }

    #[tokio::test]
    async fn denying_someone_who_joined_meanwhile_should_not_remove_them() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let kicked = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&kicked);
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/kick") {
                seen.store(true, Ordering::Relaxed);
            }
            // The homeserver has them joined, whatever the store says.
            if request.contains("/state/m.room.member/") {
                return serde_json::json!({"membership": "join"});
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                // The store still has them asking (a sync gap hid their join).
                let joined = serde_json::json!({"type":"m.room.member","state_key":"@b:example.org","sender":"@b:example.org","event_id":"$b","origin_server_ts":1,"content":{"membership":"knock"}});
                if let Some(events) =
                    response.pointer_mut("/rooms/join/!room:example.org/state/events").and_then(|e| e.as_array_mut())
                {
                    events.push(joined);
                }
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        let settings = matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO);
        account.client().sync_once(settings).await.unwrap();
        let denied = account.moderate("!room:example.org", "@b:example.org", Moderation::Deny).await;
        assert!(matches!(denied, Err(Error::Outdated)));
        assert!(!kicked.load(Ordering::Relaxed));
        // The member list after it reads them afresh, not the store that missed the join.
        assert!(!account.room("!room:example.org").unwrap().are_members_synced());
    }

    #[test]
    fn someone_at_a_custom_level_should_be_settable_back_to_member() {
        let members = Members { own_level: 100, can_set_roles: true, ..Members::default() };
        let custom = RoomMember { level: 25, ..member("c", Presence::Online, MemberRole::Member) };
        assert!(members.may(Moderation::Role(MemberRole::Member), &custom));
    }

    #[test]
    fn approving_should_not_need_a_rank_above_the_one_asking() {
        let asking = Members { own_level: 0, can_invite: true, ..Members::default() };
        assert!(asking.may(Moderation::Admit, &member("k", Presence::Unknown, MemberRole::Member)));
    }

    #[test]
    fn unbanning_should_need_the_ban_and_kick_rights() {
        let banned = member("b", Presence::Unknown, MemberRole::Member);
        let both = Members { own_level: 50, can_ban: true, can_kick: true, ..Members::default() };
        assert!(both.may(Moderation::Unban, &banned));
        assert!(!Members { own_level: 50, can_ban: true, ..Members::default() }.may(Moderation::Unban, &banned));
        assert!(!Members { own_level: 50, can_kick: true, ..Members::default() }.may(Moderation::Unban, &banned));
    }

    #[test]
    fn a_custom_level_should_go_no_higher_than_the_users_own() {
        let members = Members { own_level: 50, can_set_roles: true, ..Members::default() };
        let plain = member("p", Presence::Online, MemberRole::Member);
        assert!(members.may(Moderation::Level(25), &plain) && members.may(Moderation::Level(50), &plain));
        assert!(!members.may(Moderation::Level(51), &plain) && !members.may(Moderation::Level(0), &plain));
    }

    #[test]
    fn people_on_the_users_own_server_should_come_first() {
        let mut found = people(&["@ada:matrix.org", "@bob:home.org", "@cy:other.net", "@dee:home.org"]);
        rank(&mut found, "a", "home.org");
        assert_eq!(ids(&found), ["@bob:home.org", "@dee:home.org", "@ada:matrix.org", "@cy:other.net"]);
    }

    #[test]
    fn a_typed_user_id_should_come_before_everyone_else() {
        let mut found = people(&["@bob:home.org", "@ada:matrix.org"]);
        rank(&mut found, "@ada:matrix.org", "home.org");
        assert_eq!(ids(&found), ["@ada:matrix.org", "@bob:home.org"]);
    }

    #[test]
    fn a_server_with_a_port_should_not_count_as_the_home_server() {
        assert!(on_server("@ada:Home.org", "home.org") && !on_server("@ada:home.org:8448", "home.org"));
    }
}
