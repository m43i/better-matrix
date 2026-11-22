//! Keeping the user safe from people they don't want to hear from, which matters most in public
//! rooms: ignoring people, reporting messages, rooms and people to the homeserver's admins, and
//! hiding what strangers send (invitations, pictures in invitations, media in public rooms).
//!
//! Every choice lives in the account's data on the homeserver, so it follows the user to their
//! other sessions:
//! - ignored people in `m.ignored_user_list`: the server then sends nothing from them, and the
//!   SDK clears the rooms' caches so earlier messages go too;
//! - media and invitation pictures in MSC4278's `m.media_preview_config`, written under its
//!   unstable name as the SDK and Element do (and under the spec's where a client wrote it), so
//!   other clients follow the same choice;
//! - invitations from strangers in `chat.bettermatrix.invites`: no other client knows it, and
//!   hiding is up to the apps, which never decline for the user.

use std::collections::BTreeSet;

use matrix_sdk::ruma::api::client::reporting::report_user;
use matrix_sdk::ruma::events::{AnyGlobalAccountDataEvent, GlobalAccountDataEventType};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{EventId, OwnedUserId, RoomId, UserId};
use serde_json::{Value, json};

use crate::Error;
use crate::account::Account;

/// The ignored people's account data.
const IGNORED: &str = "m.ignored_user_list";
/// MSC4278's account data, as the spec names it.
const MEDIA: &str = "m.media_preview_config";
/// MSC4278's account data under its unstable name, which clients wrote first.
const MEDIA_UNSTABLE: &str = "io.element.msc4278.media_preview_config";
/// Whether invitations from strangers show.
const INVITES: &str = "chat.bettermatrix.invites";

/// What the user chose to keep away, for one account.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Safety {
    /// The people the user ignores, by user ID, sorted.
    pub ignored: Vec<String>,
    /// Where pictures and videos show without a press.
    pub media: ShowMedia,
    /// Whether invitations show their room's and inviter's pictures.
    pub invite_pictures: bool,
    /// Whether invitations from people the user shares no room with are hidden
    /// ([`Invitation::stranger`](crate::Invitation::stranger)).
    pub hide_stranger_invites: bool,
}

impl Default for Safety {
    fn default() -> Self {
        Self { ignored: Vec::new(), media: ShowMedia::Always, invite_pictures: true, hide_stranger_invites: false }
    }
}

impl Safety {
    /// Whether a picture or video in a room shows without a press. The user's own always do.
    pub fn shows_media(&self, public_room: bool, from_self: bool) -> bool {
        from_self
            || match self.media {
                ShowMedia::Always => true,
                ShowMedia::Private => !public_room,
                ShowMedia::Never => false,
            }
    }

    /// Whether an invitation is listed.
    pub fn shows_invitation(&self, invitation: &crate::Invitation) -> bool {
        !(self.hide_stranger_invites && invitation.stranger)
    }

    /// Whether the user ignores someone.
    pub fn ignores(&self, user_id: &str) -> bool {
        self.ignored.binary_search_by(|ignored| ignored.as_str().cmp(user_id)).is_ok()
    }
}

/// Where pictures, videos and stickers show at once (MSC4278's `media_previews`); elsewhere they
/// wait for a press. Avatars and the user's own media always show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum ShowMedia {
    /// Everywhere.
    #[default]
    Always,
    /// In private rooms (invite, knock or restricted), where strangers can't speak.
    Private,
    /// Nowhere.
    Never,
}

impl ShowMedia {
    fn as_str(self) -> &'static str {
        match self {
            Self::Always => "on",
            Self::Private => "private",
            Self::Never => "off",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "on" => Some(Self::Always),
            "private" => Some(Self::Private),
            "off" => Some(Self::Never),
            _ => None,
        }
    }
}

/// One change to [`Safety`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum SafetyChange {
    /// Ignore someone, or stop ignoring them.
    Ignore {
        /// Their user ID.
        user_id: String,
        /// Whether to ignore them.
        ignored: bool,
    },
    /// Where pictures and videos show at once.
    Media(ShowMedia),
    /// Whether invitations show pictures.
    InvitePictures(bool),
    /// Whether invitations from strangers are hidden.
    HideStrangerInvites(bool),
}

impl Safety {
    /// The choices after `change`.
    pub fn with(mut self, change: &SafetyChange) -> Self {
        match change {
            SafetyChange::Ignore { user_id, ignored } => {
                let mut set: BTreeSet<String> = self.ignored.into_iter().collect();
                if *ignored {
                    set.insert(user_id.clone());
                } else {
                    set.remove(user_id);
                }
                self.ignored = set.into_iter().collect();
            }
            SafetyChange::Media(media) => self.media = *media,
            SafetyChange::InvitePictures(shown) => self.invite_pictures = *shown,
            SafetyChange::HideStrangerInvites(hidden) => self.hide_stranger_invites = *hidden,
        }
        self
    }
}

/// What a report is about.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum ReportTarget {
    /// A message in a joined room.
    Message {
        /// Its room.
        room_id: String,
        /// The message's event ID.
        event_id: String,
    },
    /// A room, joined or not.
    Room {
        /// Its ID.
        room_id: String,
    },
    /// A person.
    Person {
        /// Their user ID.
        user_id: String,
    },
}

/// Whether a sync's account data changes the user's safety choices.
pub(crate) fn changed_in(account_data: &[Raw<AnyGlobalAccountDataEvent>]) -> bool {
    account_data.iter().any(|raw| carries(raw, &[IGNORED, MEDIA, MEDIA_UNSTABLE, INVITES]))
}

/// Whether an account data event is one of `types`.
fn carries(raw: &Raw<AnyGlobalAccountDataEvent>, types: &[&str]) -> bool {
    raw.get_field::<String>("type").ok().flatten().is_some_and(|kind| types.contains(&kind.as_str()))
}

/// `base` with the parts a sync carried taken from `synced`: what it didn't carry didn't change.
fn merge_synced(base: Safety, synced: Safety, account_data: &[Raw<AnyGlobalAccountDataEvent>]) -> Safety {
    let has = |types: &[&str]| account_data.iter().any(|raw| carries(raw, types));
    let mut merged = base;
    if has(&[IGNORED]) {
        merged.ignored = synced.ignored;
    }
    if has(&[MEDIA, MEDIA_UNSTABLE]) {
        merged.media = synced.media;
        merged.invite_pictures = synced.invite_pictures;
    }
    if has(&[INVITES]) {
        merged.hide_stranger_invites = synced.hide_stranger_invites;
    }
    merged
}

/// The choices in the three account data events' contents (absent ones are `Null`). An absent
/// MSC4278 field shows (the default); any other value this app doesn't know, `null` included,
/// hides, as MSC4278 asks.
fn read(ignored: &Value, media: &Value, invites: &Value) -> Safety {
    let mut ignored: Vec<String> =
        ignored["ignored_users"].as_object().map(|users| users.keys().cloned().collect()).unwrap_or_default();
    ignored.sort();
    let defaults = Safety::default();
    Safety {
        ignored,
        media: match media.get("media_previews") {
            None => defaults.media,
            Some(value) => value.as_str().and_then(ShowMedia::parse).unwrap_or(ShowMedia::Never),
        },
        invite_pictures: media.get("invite_avatars").is_none_or(|value| value.as_str() == Some("on")),
        hide_stranger_invites: invites["hide_strangers"].as_bool().unwrap_or(defaults.hide_stranger_invites),
    }
}

/// The ignored list's content with `user_id` in it or not, keeping every other field.
fn with_ignored(mut content: Value, user_id: &str, ignored: bool) -> Value {
    if !content.is_object() {
        content = json!({});
    }
    if !content["ignored_users"].is_object() {
        content["ignored_users"] = json!({});
    }
    if let Some(users) = content["ignored_users"].as_object_mut() {
        if ignored {
            users.entry(user_id).or_insert_with(|| json!({}));
        } else {
            users.remove(user_id);
        }
    }
    content
}

/// MSC4278's content with the user's choice set, keeping every other field.
fn with_media(mut content: Value, media: Option<ShowMedia>, invite_pictures: Option<bool>) -> Value {
    if !content.is_object() {
        content = json!({});
    }
    if let Some(media) = media {
        content["media_previews"] = json!(media.as_str());
    }
    if let Some(shown) = invite_pictures {
        content["invite_avatars"] = json!(if shown { "on" } else { "off" });
    }
    content
}

impl Account {
    /// The user's safety choices, from this device's copy of the account data: a sync brings
    /// each event the first time and whenever it changes.
    pub async fn safety(&self) -> Result<Safety, Error> {
        let ignored = self.local_account_data(IGNORED).await?;
        let media = match self.local_account_data(MEDIA).await? {
            Value::Null => self.local_account_data(MEDIA_UNSTABLE).await?,
            media => media,
        };
        let invites = self.local_account_data(INVITES).await?;
        Ok(read(&ignored, &media, &invites))
    }

    /// The choices after a sync that carried some of their events, or `None` while a change is
    /// being saved (its answer, read from the server, reports them). Only the parts the sync
    /// carried change: a sync answered before a save finished can't bring back an older choice
    /// for a part it didn't carry.
    pub(crate) async fn synced_safety(
        &self,
        account_data: &[Raw<AnyGlobalAccountDataEvent>],
    ) -> Result<Option<Safety>, Error> {
        let Ok(mut confirmed) = self.safety_changes.try_lock() else { return Ok(None) };
        let synced = self.safety().await?;
        let merged = match confirmed.take() {
            Some(base) => merge_synced(base, synced, account_data),
            None => synced,
        };
        *confirmed = Some(merged.clone());
        Ok(Some(merged))
    }

    /// Makes one change and returns the choices that hold after it, as the server has them.
    /// Changes run one at a time, each on the server's latest copy, so two made before a sync
    /// brings the first both hold.
    pub async fn change_safety(&self, change: &SafetyChange) -> Result<Safety, Error> {
        let mut confirmed = self.safety_changes.lock().await;
        match change {
            SafetyChange::Ignore { user_id, ignored } => {
                let user_id = UserId::parse(user_id)?;
                if self.client().user_id() == Some(&*user_id) {
                    return Err(matrix_sdk::Error::CantIgnoreLoggedInUser.into());
                }
                let content = with_ignored(self.server_account_data(IGNORED).await?, user_id.as_str(), *ignored);
                self.save_account_data(IGNORED, &content).await?;
            }
            SafetyChange::Media(media) => self.save_media_config(Some(*media), None).await?,
            SafetyChange::InvitePictures(shown) => self.save_media_config(None, Some(*shown)).await?,
            SafetyChange::HideStrangerInvites(hidden) => {
                let content = with_object(self.server_account_data(INVITES).await?, "hide_strangers", json!(hidden));
                self.save_account_data(INVITES, &content).await?;
            }
        }
        // The change is saved: a failure to read the rest back falls back on the choices the
        // server last confirmed (this device's copy before any), with the change applied.
        let safety = match self.server_safety().await {
            Ok(safety) => safety,
            Err(error) => {
                eprintln!("matrix-engine: could not read the safety choices back: {error}");
                let known = match confirmed.take() {
                    Some(known) => known,
                    None => self.safety().await?,
                };
                known.with(change)
            }
        };
        *confirmed = Some(safety.clone());
        Ok(safety)
    }

    /// The choices as the server has them now.
    async fn server_safety(&self) -> Result<Safety, Error> {
        let ignored = self.server_account_data(IGNORED).await?;
        let media = match self.server_account_data(MEDIA).await? {
            Value::Null => self.server_account_data(MEDIA_UNSTABLE).await?,
            media => media,
        };
        let invites = self.server_account_data(INVITES).await?;
        Ok(read(&ignored, &media, &invites))
    }

    /// Reports a message, a room or a person to the homeserver's admins, with the user's reason
    /// (which may be empty).
    pub async fn report(&self, target: &ReportTarget, reason: &str) -> Result<(), Error> {
        let reason = reason.trim().to_owned();
        match target {
            ReportTarget::Message { room_id, event_id } => {
                let room = self
                    .client()
                    .get_room(&RoomId::parse(room_id)?)
                    .ok_or_else(|| Error::UnknownRoom { account: self.id().clone(), room_id: room_id.clone() })?;
                room.report_content(EventId::parse(event_id)?, (!reason.is_empty()).then_some(reason)).await?;
            }
            ReportTarget::Room { room_id } => {
                let room_id = RoomId::parse(room_id)?;
                let request = matrix_sdk::ruma::api::client::room::report_room::v3::Request::new(room_id, reason);
                self.client().send(request).await.map_err(matrix_sdk::Error::from)?;
            }
            ReportTarget::Person { user_id } => {
                let user_id: OwnedUserId = UserId::parse(user_id)?;
                self.client()
                    .send(report_user::v3::Request::new(user_id, reason))
                    .await
                    .map_err(matrix_sdk::Error::from)?;
            }
        }
        Ok(())
    }

    /// Sets MSC4278's fields as the SDK and Element do, under the unstable name, and under the
    /// spec's name only where a client already wrote it, so this app never makes a copy that
    /// shadows the other. Only the field that changes is written; each copy keeps every other
    /// field as it was, values this app doesn't know included.
    async fn save_media_config(&self, media: Option<ShowMedia>, invite_pictures: Option<bool>) -> Result<(), Error> {
        let stable = self.server_account_data(MEDIA).await?;
        let unstable = self.server_account_data(MEDIA_UNSTABLE).await?;
        let stable_existed = !stable.is_null();
        self.save_account_data(MEDIA_UNSTABLE, &with_media(unstable, media, invite_pictures)).await?;
        if stable_existed {
            self.save_account_data(MEDIA, &with_media(stable, media, invite_pictures)).await?;
        }
        Ok(())
    }

    async fn local_account_data(&self, event_type: &str) -> Result<Value, Error> {
        let raw = self.client().account().account_data_raw(GlobalAccountDataEventType::from(event_type)).await?;
        Ok(raw.and_then(|raw| serde_json::from_str(raw.json().get()).ok()).unwrap_or(Value::Null))
    }

    /// The server's copy, so a change keeps fields another session set since the last sync.
    async fn server_account_data(&self, event_type: &str) -> Result<Value, Error> {
        let raw = self.client().account().fetch_account_data(GlobalAccountDataEventType::from(event_type)).await?;
        Ok(raw.and_then(|raw| serde_json::from_str(raw.json().get()).ok()).unwrap_or(Value::Null))
    }

    async fn save_account_data(&self, event_type: &str, content: &Value) -> Result<(), Error> {
        let raw = Raw::new(content).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(event_type), raw).await?;
        Ok(())
    }
}

/// `content` as an object with `key` set, keeping every other field.
fn with_object(mut content: Value, key: &str, value: Value) -> Value {
    if !content.is_object() {
        content = json!({});
    }
    content[key] = value;
    content
}

/// Whether an invitation from `inviter` stays out of sight, push included: the user hides
/// strangers' invitations and shares no room with them. The choice is read from the server, as a
/// push may come while this device's copy is old; this device's copy stands in when it can't be.
pub(crate) async fn hides_invite_from(client: &matrix_sdk::Client, inviter: &UserId) -> bool {
    hides_stranger_invites(client).await && !shares_room(client, inviter).await
}

/// How long a push waits for the server's copy of the choice before deciding on this device's:
/// a push has seconds to show.
const PUSH_CHOICE_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Whether the user hides strangers' invitations, as the server says; this device's copy stands in
/// when the server can't be asked in time.
async fn hides_stranger_invites(client: &matrix_sdk::Client) -> bool {
    let event_type = GlobalAccountDataEventType::from(INVITES);
    let fetched = tokio::time::timeout(PUSH_CHOICE_WAIT, client.account().fetch_account_data(event_type.clone())).await;
    let raw = match fetched {
        Ok(Ok(raw)) => raw,
        _ => client.account().account_data_raw(event_type).await.ok().flatten(),
    };
    raw.and_then(|raw| serde_json::from_str::<Value>(raw.json().get()).ok())
        .is_some_and(|content| content["hide_strangers"].as_bool() == Some(true))
}

/// Whether `room_id` is an invitation this device knows that stays out of sight
/// ([`hides_invite_from`]): for a push whose event couldn't be fetched.
pub(crate) async fn hides_invitation(client: &matrix_sdk::Client, room_id: &RoomId) -> bool {
    let Some(room) = client.get_room(room_id) else { return false };
    if room.state() != matrix_sdk::RoomState::Invited {
        return false;
    }
    match room.invite_details().await {
        Ok(invite) => hides_invite_from(client, &invite.inviter_id).await,
        // An unknown inviter is a stranger.
        Err(_) => hides_stranger_invites(client).await,
    }
}

/// Whether the user shares a joined room with someone, as this device knows its members: a
/// direct chat with them, or a room where they joined. Lazy loading leaves out members who never
/// spoke in a large room, so someone quiet there still counts as a stranger.
pub(crate) async fn shares_room(client: &matrix_sdk::Client, user_id: &UserId) -> bool {
    for room in client.joined_rooms() {
        if room.direct_targets().iter().any(|target| target.as_user_id() == Some(user_id)) {
            return true;
        }
        if let Ok(Some(member)) = room.get_member_no_sync(user_id).await
            && matches!(member.membership(), matrix_sdk::ruma::events::room::member::MembershipState::Join)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
