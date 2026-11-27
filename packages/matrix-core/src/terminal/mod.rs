//! Engine operations for clients that read history in pages and wait for server acknowledgements.

use matrix_sdk::Room;
use matrix_sdk::ruma::{EventId, RoomAliasId, RoomOrAliasId, ServerName, UserId};
use serde_json::{Value, json};

use crate::engine::ErrorKind;
use crate::{Account, Error, SearchUsage, TimelineKey};

mod history;
mod send;
pub use send::{MessageAction, NewMessage};

pub(crate) fn error_kind(error: &Error) -> ErrorKind {
    use matrix_sdk::ruma::api::error::ErrorKind as Matrix;
    let kind = match error {
        Error::Sdk(error) => error.client_api_error_kind(),
        Error::Http(error) => error.client_api_error_kind(),
        Error::UnknownAccount(_)
        | Error::PasswordRequired
        | Error::UnexpectedAccount { .. }
        | Error::UnexpectedDevice
        | Error::ResumeStore => return ErrorKind::Authentication,
        Error::NotAllowed => return ErrorKind::Permission,
        Error::TimedOut => return ErrorKind::Timeout,
        _ => None,
    };
    match kind {
        Some(Matrix::UnknownToken { .. } | Matrix::MissingToken | Matrix::Unauthorized) => ErrorKind::Authentication,
        Some(Matrix::Forbidden) => ErrorKind::Permission,
        Some(Matrix::LimitExceeded { .. }) => ErrorKind::RateLimit,
        _ => ErrorKind::Other,
    }
}

/// Membership state to include in a member list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Membership {
    /// Joined members.
    Joined,
    /// Invited users.
    Invited,
    /// Banned users.
    Banned,
    /// Users asking to join.
    Knocking,
}

/// One terminal operation. Existing engine operations still use their ordinary commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Resolve an alias to its canonical room ID.
    ResolveAlias(String),
    /// Report the local device and verification state.
    Session,
    /// Confirm that the homeserver still accepts the local session.
    ValidateSession,
    /// Read a Matrix user's public profile.
    Person(String),
    /// Read the account's ignored users.
    Ignored,
    /// Read and list the account's notification keywords.
    Keywords,
    /// Read the account default notification mode after sync.
    NotificationDefaults,
    /// Request admission to a room.
    Knock {
        /// Room ID or alias.
        room: String,
        /// Servers from the permalink.
        via: Vec<String>,
        /// Optional reason to share with moderators.
        reason: Option<String>,
    },
    /// Forget a room that has already been left.
    Forget(String),
    /// Decline a pending invitation.
    DeclineInvite(String),
    /// List members with the given membership state.
    Members(String, Membership),
    /// List direct children or descend through the space hierarchy.
    Children {
        /// The space's room ID.
        space: String,
        /// Include nested rooms and spaces.
        recursive: bool,
    },
    /// Read one page of messages and its continuation cursor.
    History {
        /// Where to read.
        key: TimelineKey,
        /// Maximum number of events in the server page.
        limit: u16,
        /// The opaque cursor returned by the last page.
        cursor: Option<String>,
    },
    /// Read one message by event ID.
    Message(String, String),
    /// Search locally stored messages, optionally in one room.
    Search(String, Option<String>),
    /// Scan a bounded amount of readable history for highlights.
    Mentions {
        /// Restrict the scan to one room.
        room: Option<String>,
        /// Earliest timestamp, in milliseconds since the Unix epoch.
        since_ms: u64,
        /// Maximum number of matches.
        limit: u16,
    },
    /// Follow new messages using the account's running sync.
    Watch {
        /// Restrict the watch to this room and optionally thread.
        key: Option<TimelineKey>,
        /// Emit only messages highlighted by the account's rules.
        highlights: bool,
    },
    /// Send an explicit read receipt up to the named event.
    ReadThrough(String, String),
    /// Send a message and await the homeserver's acknowledgement.
    Send(NewMessage),
    /// Change an existing message, awaiting the homeserver.
    ChangeMessage(MessageAction),
    /// Download the file attached to one message.
    Download(String, String, std::path::PathBuf),
}

pub(crate) async fn run(
    account: Account,
    action: Action,
    usage: SearchUsage,
    report: impl Fn(Value) + Send + Sync,
    watch_ready: Option<std::sync::Arc<crate::engine::WatchSync>>,
) -> Result<(), Error> {
    let client = account.client();
    let result = match action {
        Action::ResolveAlias(alias) => {
            let response = client.resolve_room_alias(&RoomAliasId::parse(alias)?).await?;
            json!({ "room_id": response.room_id })
        }
        Action::Session => json!({
            "account": account.cached_summary().await,
            "device_id": client.device_id(),
            "security": account.security(),
        }),
        Action::ValidateSession => {
            client.whoami().await?;
            json!({ "ok": true })
        }
        Action::Person(user) => {
            let user = UserId::parse(user)?;
            let profile = client.account().fetch_user_profile_of(&user).await?;
            json!({ "user_id": user, "display_name": profile.get("displayname"), "avatar_url": profile.get("avatar_url") })
        }
        Action::Ignored => json!(account.safety().await?.ignored),
        Action::Keywords => json!(account.server_keywords().await?),
        Action::NotificationDefaults => json!({
            "account": account.id(), "default": account.cached_summary().await.default_notifications,
        }),
        Action::Knock { room, via, reason } => {
            let via = via.iter().map(ServerName::parse).collect::<Result<_, _>>()?;
            let room = client.knock(RoomOrAliasId::parse(room)?, reason, via).await?;
            json!({ "room_id": room.room_id() })
        }
        Action::Forget(room) => {
            account.room(&room)?.forget().await?;
            json!({ "room_id": room })
        }
        Action::DeclineInvite(room_id) => {
            let room = account.room(&room_id)?;
            if room.state() != matrix_sdk::RoomState::Invited {
                return Err(Error::Outdated);
            }
            room.leave().await?;
            json!({ "room_id": room_id })
        }
        Action::Members(room, membership) => members(&account.room(&room)?, membership).await?,
        Action::Children { space, recursive } => children(&account, &space, recursive).await?,
        Action::History { key, limit, cursor } => history::page(&account, &key, limit, cursor).await?,
        Action::Message(room, event) => history::message(&account, &room, &event).await?,
        Action::Search(query, room) => json!(account.search_messages_in(&query, &usage, room.as_deref()).await?),
        Action::Mentions { room, since_ms, limit } => {
            history::mentions(&account, room.as_deref(), since_ms, limit).await?
        }
        Action::Watch { key, highlights } => {
            let sync = watch_ready.ok_or(Error::Unsupported("watching without a terminal sync"))?;
            return history::watch(&account, key, highlights, report, &sync).await;
        }
        Action::ReadThrough(room_id, event_id) => {
            use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
            use matrix_sdk::ruma::events::receipt::ReceiptThread;
            let event_id = EventId::parse(event_id)?;
            let room = account.joined_room(&room_id)?;
            room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id.clone()).await?;
            account.set_unread(&room_id, false).await?;
            json!({ "room_id": room_id, "event_id": event_id })
        }
        Action::Send(message) => send::post(&account, message).await?,
        Action::ChangeMessage(action) => send::change(&account, action).await?,
        Action::Download(room, event, dest) => {
            history::download(&account, &room, &event, &dest).await?;
            json!({ "room_id": room, "event_id": event, "path": dest })
        }
    };
    report(result);
    Ok(())
}

async fn members(room: &Room, membership: Membership) -> Result<Value, Error> {
    use matrix_sdk::RoomMemberships as Sdk;
    let selection = match membership {
        Membership::Joined => Sdk::JOIN,
        Membership::Invited => Sdk::INVITE,
        Membership::Banned => Sdk::BAN,
        Membership::Knocking => Sdk::KNOCK,
    };
    let members = room.members(selection).await?;
    Ok(Value::Array(
        members
            .iter()
            .map(|member| {
                json!({
                    "user_id": member.user_id(),
                    "display_name": member.display_name(),
                    "avatar_url": member.avatar_url(),
                    "membership": member.membership(),
                    "power_level": match member.power_level() {
                        matrix_sdk::ruma::events::room::power_levels::UserPowerLevel::Int(value) => json!(value),
                        _ => json!("infinite"),
                    },
                })
            })
            .collect(),
    ))
}

async fn children(account: &Account, space: &str, recursive: bool) -> Result<Value, Error> {
    use matrix_sdk::ruma::{RoomId, UInt, api::client::space::get_hierarchy};
    use std::collections::HashSet;
    let root = RoomId::parse(space)?;
    let mut request = get_hierarchy::v1::Request::new(root.clone());
    request.max_depth = (!recursive).then(|| UInt::from(1_u16));
    let mut seen = HashSet::new();
    let mut rooms = Vec::new();
    loop {
        let page = account.client().send(request.clone()).await?;
        for room in page.rooms {
            if room.summary.room_id != root {
                rooms.push(json!(room));
            }
        }
        let Some(next) = page.next_batch else { return Ok(Value::Array(rooms)) };
        if !seen.insert(next.clone()) {
            return Err(Error::RepeatedPaginationToken { resource: space.to_owned() });
        }
        request.from = Some(next);
    }
}

#[cfg(test)]
mod tests;
