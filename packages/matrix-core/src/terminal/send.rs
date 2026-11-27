use std::path::PathBuf;

use matrix_sdk::room::edit::EditedContent;
use matrix_sdk::room::reply::{EnforceThread, Reply};
use matrix_sdk::ruma::events::reaction::ReactionEventContent;
use matrix_sdk::ruma::events::relation::{Annotation, RelationType};
use matrix_sdk::ruma::events::room::message::{AddMentions, ReplyWithinThread, TextMessageEventContent};
use matrix_sdk::ruma::{EventId, OwnedEventId, UserId};
use matrix_sdk::{Room, attachment::AttachmentConfig};
use serde_json::{Value, json};

use crate::{Account, Error, Mentions, Outgoing, TimelineKey};

/// A message to send synchronously through the SDK, with the room's encryption and relations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewMessage {
    /// Text or Markdown that may reply to a message or thread.
    Text {
        /// Its timeline.
        key: TimelineKey,
        /// Its plain text and optional formatted body.
        message: Outgoing,
        /// An event the message answers.
        reply_to: Option<String>,
        /// A caller-supplied idempotency key.
        transaction_id: Option<String>,
    },
    /// Upload an attachment.
    Attachment {
        /// Its timeline.
        key: TimelineKey,
        /// The local file.
        path: PathBuf,
        /// The text accompanying the file.
        caption: Option<String>,
        /// A caller-supplied idempotency key for the message event.
        transaction_id: Option<String>,
    },
}

/// Changes to server events that finish once the server has acknowledged them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageAction {
    /// Replace the text or formatted text of one's own event.
    Edit {
        /// Its room ID.
        room: String,
        /// The original event ID.
        event: String,
        /// The new plain text.
        text: String,
        /// The new formatted text, when present.
        html: Option<String>,
    },
    /// Redact an event.
    Redact(String, String),
    /// Forward a message to another room.
    Forward(String, String, String),
    /// Add or remove the user's reaction.
    React {
        /// Its room ID.
        room: String,
        /// The event ID of the message.
        event: String,
        /// The reaction key.
        emoji: String,
        /// Whether to remove it.
        remove: bool,
    },
}

pub(super) async fn post(account: &Account, message: NewMessage) -> Result<Value, Error> {
    match message {
        NewMessage::Text { key, message, reply_to, transaction_id } => {
            let room = account.joined_room(&key.room_id)?;
            let content = crate::outgoing::room_message(&message).ok_or(Error::MessageUnavailable)?;
            if let Outgoing::Text { mentions, .. } = &message {
                for user in &mentions.user_ids {
                    UserId::parse(user)?;
                }
            }
            let content = match reply(&room, &key, reply_to.as_deref()).await? {
                Some(reply) => room.make_reply_event(content, reply).await.map_err(message_error)?,
                None => content.into(),
            };
            let send = room.send(content);
            let send = match transaction(transaction_id)? {
                Some(id) => send.with_transaction_id(id),
                None => send,
            };
            let sent = send.await?;
            finish_backup(&room).await;
            Ok(sent_event(&room, &sent.response.event_id))
        }
        NewMessage::Attachment { key, path, caption, transaction_id } => {
            let room = account.joined_room(&key.room_id)?;
            let data = tokio::fs::read(&path).await.map_err(|source| Error::ReadFile { path: path.clone(), source })?;
            let filename = path.file_name().ok_or(Error::MessageUnavailable)?.to_string_lossy().into_owned();
            let content_type = mime_guess::from_path(&path).first_or_octet_stream();
            let config = AttachmentConfig::new()
                .caption(caption.map(TextMessageEventContent::plain))
                .reply(reply(&room, &key, None).await?);
            let config = match transaction(transaction_id)? {
                Some(id) => config.txn_id(id),
                None => config,
            };
            let sent = room.send_attachment(filename, &content_type, data, config).await?;
            finish_backup(&room).await;
            Ok(sent_event(&room, &sent.event_id))
        }
    }
}

fn transaction(id: Option<String>) -> Result<Option<matrix_sdk::ruma::OwnedTransactionId>, Error> {
    match id {
        Some(id) if id.trim().is_empty() => Err(Error::Account("transaction ID is empty".into())),
        Some(id) => Ok(Some(id.into())),
        None => Ok(None),
    }
}

async fn finish_backup(room: &Room) {
    if !room.encryption_state().is_encrypted() {
        return;
    }
    let backups = room.client().encryption().backups();
    let backup = async {
        if backups.are_enabled().await {
            let _ = backups.wait_for_steady_state().await;
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), backup).await;
}

async fn reply(room: &Room, key: &TimelineKey, target: Option<&str>) -> Result<Option<Reply>, Error> {
    let event_id = match (key.thread.as_deref(), target) {
        (Some(root), Some(target)) => {
            let event = room.load_or_fetch_event(&EventId::parse(target)?, None).await?;
            if target != root
                && event.raw().deserialize_as_unchecked::<Value>().ok().is_none_or(|raw| {
                    raw["content"]["m.relates_to"]["rel_type"] != "m.thread"
                        || raw["content"]["m.relates_to"]["event_id"] != root
                })
            {
                return Err(Error::Account("the reply target is outside the specified thread".into()));
            }
            EventId::parse(target)?
        }
        (Some(root), None) => EventId::parse(root)?,
        (None, Some(target)) => EventId::parse(target)?,
        (None, None) => return Ok(None),
    };
    Ok(Some(Reply {
        event_id,
        enforce_thread: if key.thread.is_some() {
            EnforceThread::Threaded(if target.is_some() { ReplyWithinThread::Yes } else { ReplyWithinThread::No })
        } else {
            EnforceThread::Unthreaded
        },
        add_mentions: if target.is_some() { AddMentions::Yes } else { AddMentions::No },
    }))
}

pub(super) async fn change(account: &Account, action: MessageAction) -> Result<Value, Error> {
    match action {
        MessageAction::Edit { room, event, text, html } => {
            let room = account.joined_room(&room)?;
            let event = EventId::parse(event)?;
            let content = crate::outgoing::text(&text, html.as_deref(), &Mentions::default());
            let edit =
                room.make_edit_event(&event, EditedContent::RoomMessage(content)).await.map_err(message_error)?;
            let sent = room.send(edit).await?;
            finish_backup(&room).await;
            Ok(sent_event(&room, &sent.response.event_id))
        }
        MessageAction::Redact(room, event) => {
            let room = account.joined_room(&room)?;
            let event = EventId::parse(event)?;
            let sent = room.redact(&event, None, None).await?;
            Ok(sent_event(&room, &sent.event_id))
        }
        MessageAction::Forward(room, event, destination) => {
            let room = account.joined_room(&room)?;
            let destination = account.joined_room(&destination)?;
            let event = EventId::parse(event)?;
            let original = super::history::event(account, &room, &event).await?;
            let raw = original.raw().deserialize_as_unchecked::<Value>().map_err(matrix_sdk::Error::from)?;
            let latest = super::history::latest_edit(&room, &event, &raw).await?;
            let content =
                crate::history::forward_content(latest.as_ref().unwrap_or(&raw)).ok_or(Error::MessageUnavailable)?;
            let sent = destination.send(content).await?;
            finish_backup(&destination).await;
            Ok(sent_event(&destination, &sent.response.event_id))
        }
        MessageAction::React { room, event, emoji, remove } => {
            let room = account.joined_room(&room)?;
            react(&room, EventId::parse(event)?, emoji, remove).await
        }
    }
}

async fn react(room: &Room, event: OwnedEventId, emoji: String, remove: bool) -> Result<Value, Error> {
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    let mut own_reactions = Vec::new();
    loop {
        let (reactions, next) =
            super::history::relations(room, event.as_str(), RelationType::Annotation, 100, cursor).await?;
        for reaction in reactions {
            let Ok(raw) = reaction.raw().deserialize_as_unchecked::<Value>() else { continue };
            if raw["sender"].as_str() == Some(room.own_user_id().as_str())
                && raw["content"]["m.relates_to"]["key"].as_str() == Some(emoji.as_str())
                && let Some(id) = reaction.event_id()
            {
                if !remove {
                    return Ok(sent_event(room, id));
                }
                own_reactions.push(id.to_owned());
            }
        }
        let Some(next) = next else { break };
        if !seen.insert(next.clone()) {
            return Err(Error::RepeatedPaginationToken { resource: event.to_string() });
        }
        cursor = Some(next);
    }
    if remove {
        let mut redactions = Vec::new();
        for id in own_reactions {
            redactions.push(room.redact(&id, None, None).await?.event_id);
        }
        Ok(json!({ "room_id": room.room_id(), "event_id": event, "redactions": redactions }))
    } else {
        let sent = room.send(ReactionEventContent::new(Annotation::new(event, emoji))).await?;
        Ok(sent_event(room, &sent.response.event_id))
    }
}

fn sent_event(room: &Room, event: &EventId) -> Value {
    json!({ "room_id": room.room_id(), "event_id": event })
}

fn message_error(error: impl std::fmt::Display) -> Error {
    Error::Account(error.to_string())
}
