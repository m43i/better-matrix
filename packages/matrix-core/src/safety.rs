//! Keeping the user safe from people they don't want to hear from, which matters most in public
//! rooms: ignoring people, reporting messages, rooms and people to the homeserver's admins, and
//! hiding what strangers send (invitations, pictures in invitations, media in public rooms).
//!
//! Every choice lives in the account's data on the homeserver, so it follows the user to their
//! other sessions:
//! - ignored people in `m.ignored_user_list`: the server then sends nothing from them, and the
//!   SDK clears the rooms' caches so earlier messages go too;
//! - media and invitation pictures in MSC4278's `m.media_preview_config` (and its unstable name,
//!   which Element reads), so other clients follow the same choice;
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

/// Where pictures and videos show at once (MSC4278's `media_previews`); elsewhere they wait for a
/// press. Stickers, avatars and the user's own media always show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum ShowMedia {
    /// Everywhere.
    #[default]
    Always,
    /// In rooms that aren't public, where only invited people speak.
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
    account_data.iter().any(|raw| {
        matches!(
            raw.get_field::<String>("type").ok().flatten().as_deref(),
            Some(IGNORED | MEDIA | MEDIA_UNSTABLE | INVITES)
        )
    })
}

/// The choices in the three account data events' contents (absent ones are `Null`). The spec's
/// MSC4278 name wins over the unstable one; values a client doesn't know are left at the default.
fn read(ignored: &Value, media: &Value, invites: &Value) -> Safety {
    let mut ignored: Vec<String> =
        ignored["ignored_users"].as_object().map(|users| users.keys().cloned().collect()).unwrap_or_default();
    ignored.sort();
    let defaults = Safety::default();
    Safety {
        ignored,
        media: media["media_previews"].as_str().and_then(ShowMedia::parse).unwrap_or(defaults.media),
        invite_pictures: media["invite_avatars"].as_str().map_or(defaults.invite_pictures, |shown| shown != "off"),
        hide_stranger_invites: invites["hide_strangers"].as_bool().unwrap_or(defaults.hide_stranger_invites),
    }
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

    /// Makes one change and returns the choices that hold after it.
    pub async fn change_safety(&self, change: &SafetyChange) -> Result<Safety, Error> {
        let account = self.client().account();
        match change {
            SafetyChange::Ignore { user_id, ignored: true } => account.ignore_user(&UserId::parse(user_id)?).await?,
            SafetyChange::Ignore { user_id, ignored: false } => {
                account.unignore_user(&UserId::parse(user_id)?).await?;
            }
            SafetyChange::Media(media) => self.save_media_config(Some(*media), None).await?,
            SafetyChange::InvitePictures(shown) => self.save_media_config(None, Some(*shown)).await?,
            SafetyChange::HideStrangerInvites(hidden) => {
                let content = with_object(self.server_account_data(INVITES).await?, "hide_strangers", json!(hidden));
                self.save_account_data(INVITES, &content).await?;
            }
        }
        // This device's copy changes with the next sync; until then, the change applies to it.
        Ok(self.safety().await?.with(change))
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

    /// Sets MSC4278's fields under both its names: the spec's, which this app reads first, and
    /// the unstable one other clients still read.
    async fn save_media_config(&self, media: Option<ShowMedia>, invite_pictures: Option<bool>) -> Result<(), Error> {
        let current = match self.server_account_data(MEDIA).await? {
            Value::Null => self.server_account_data(MEDIA_UNSTABLE).await?,
            content => content,
        };
        let content = with_media(current, media, invite_pictures);
        self.save_account_data(MEDIA, &content).await?;
        self.save_account_data(MEDIA_UNSTABLE, &content).await
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
/// strangers' invitations and shares no room with them.
pub(crate) async fn hides_invite_from(client: &matrix_sdk::Client, inviter: &UserId) -> bool {
    let raw = client.account().account_data_raw(GlobalAccountDataEventType::from(INVITES)).await.ok().flatten();
    let hidden = raw
        .and_then(|raw| serde_json::from_str::<Value>(raw.json().get()).ok())
        .is_some_and(|content| content["hide_strangers"].as_bool() == Some(true));
    hidden && !shares_room(client, inviter).await
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
