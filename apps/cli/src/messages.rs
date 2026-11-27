use crate::text::t;
use std::io::Read;

use matrix_core::terminal::{Action, MessageAction, NewMessage};
use matrix_core::{AccountId, Command, Event, Mentions as MentionTargets, Outgoing, TimelineKey};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::args::{Body, EventTarget, Files, MentionAction, Mentions, Messages, Pins, Saved, TimelineTarget};
use crate::client::Client;
use crate::{CliError, Result};

pub(crate) async fn messages(client: &mut Client, account: AccountId, command: Messages) -> Result<Option<Value>> {
    let remove_reaction = matches!(command, Messages::Unreact(_));
    let action = match command {
        Messages::List { target, thread, limit, cursor } => {
            let key = key(client, &account, TimelineTarget { target, thread }).await?;
            Action::History { key, limit, cursor }
        }
        Messages::Show(event) => {
            let (room, event) = target(client, &account, event).await?;
            Action::Message(room, event)
        }
        Messages::Search { query, filter } => {
            let room = client.optional_room(&account, filter.room.as_deref()).await?;
            Action::Search(query.query, room)
        }
        Messages::Watch(timeline) => {
            let key = key(client, &account, timeline).await?;
            client.watch(&account, Some(key), false).await?;
            return Ok(None);
        }
        Messages::Send(args) => {
            let key = key(client, &account, args.timeline).await?;
            let (body, html) = body(args.body).await?;
            let message = Outgoing::Text {
                body,
                html,
                mentions: MentionTargets { user_ids: args.mention, room: args.mention_room },
            };
            Action::Send(NewMessage::Text { key, message, reply_to: args.reply_to, transaction_id: args.txn_id })
        }
        Messages::Edit { event, body: text } => {
            let (room, event) = target(client, &account, event).await?;
            let (text, html) = body(text).await?;
            Action::ChangeMessage(MessageAction::Edit { room, event, text, html })
        }
        Messages::Redact(event) => {
            let terminal = client.terminal;
            tokio::task::spawn_blocking(move || terminal.confirm(crate::text::t("Redact this message permanently?")))
                .await??;
            let (room, event) = target(client, &account, event).await?;
            Action::ChangeMessage(MessageAction::Redact(room, event))
        }
        Messages::Forward { event, destination } => {
            let (room, event) = target(client, &account, event).await?;
            let destination = client.resolve(&account, &destination).await?;
            Action::ChangeMessage(MessageAction::Forward(room, event, destination))
        }
        Messages::React(args) | Messages::Unreact(args) => {
            let (room, event) = target(client, &account, args.event).await?;
            Action::ChangeMessage(MessageAction::React { room, event, emoji: args.emoji, remove: remove_reaction })
        }
        Messages::Source(event) => {
            let (room_id, event_id) = target(client, &account, event).await?;
            let value = client
                .run(Command::ViewSource { account, room_id, event_id }, |id, event| match event {
                    Event::EventSource { request, source } if *request == id => Some(json!(source)),
                    _ => None,
                })
                .await?;
            return Ok(Some(value));
        }
    };
    Ok(Some(client.terminal_action(&account, action).await?))
}

pub(crate) async fn mentions(client: &mut Client, account: AccountId, command: Mentions) -> Result<Option<Value>> {
    match command.action {
        Some(MentionAction::Watch(filter)) => {
            let room = client.optional_room(&account, filter.room.as_deref()).await?;
            client.watch(&account, room.map(TimelineKey::room), true).await?;
            Ok(None)
        }
        Some(MentionAction::Count(filter)) => {
            let room = client.optional_room(&account, filter.room.as_deref()).await?;
            let rooms = client.rooms(&account).rooms;
            if let Some(room) = &room
                && !rooms.iter().any(|joined| joined.room_id == *room)
            {
                return Err(CliError::Input(tf!("{room} is not a joined room", room = room)));
            }
            let matches: Vec<_> = rooms
                .into_iter()
                .filter(|joined| room.as_ref().map_or(joined.mentions > 0, |room| *room == joined.room_id))
                .map(|joined| json!({ "room_id": joined.room_id, "name": joined.name, "mentions": joined.mentions }))
                .collect();
            Ok(Some(json!({
                "count": matches.iter().filter_map(|room| room["mentions"].as_u64()).sum::<u64>(),
                "rooms": matches,
            })))
        }
        None => {
            let room = client.optional_room(&account, command.filter.room.as_deref()).await?;
            let since = match command.since {
                Some(text) => OffsetDateTime::parse(&text, &Rfc3339)
                    .map_err(|_| CliError::Input(t("use an RFC 3339 timestamp for --since").into()))?,
                None => OffsetDateTime::now_utc() - time::Duration::days(1),
            };
            let since_ms = u64::try_from(since.unix_timestamp_nanos() / 1_000_000).unwrap_or(0);
            let value =
                client.terminal_action(&account, Action::Mentions { room, since_ms, limit: command.limit }).await?;
            Ok(Some(value))
        }
    }
}

pub(crate) async fn files(client: &mut Client, account: AccountId, command: Files) -> Result<Value> {
    match command {
        Files::List(room) => {
            let room_id = client.resolve(&account, &room.room).await?;
            client
                .run(
                    Command::ListFiles { account: account.clone(), room_id: room_id.clone() },
                    |_, event| match event {
                        Event::Files { account: owner, room_id: room, files }
                            if *owner == account && *room == room_id =>
                        {
                            Some(json!(files))
                        }
                        _ => None,
                    },
                )
                .await
        }
        Files::Upload { timeline, file, caption, txn_id } => {
            let key = key(client, &account, timeline).await?;
            client
                .terminal_action(
                    &account,
                    Action::Send(NewMessage::Attachment { key, path: file, caption, transaction_id: txn_id }),
                )
                .await
        }
        Files::Download { event, output } => {
            let (room, event) = target(client, &account, event).await?;
            client.terminal_action(&account, Action::Download(room, event, output)).await
        }
    }
}

pub(crate) async fn pins(client: &mut Client, account: AccountId, command: Pins) -> Result<Value> {
    let pinned = matches!(command, Pins::Add(_));
    match command {
        Pins::List(room) => {
            let room_id = client.resolve(&account, &room.room).await?;
            client
                .run(Command::ListPinnedMessages { account: account.clone(), room_id: room_id.clone() }, |_, event| {
                    match event {
                        Event::PinnedMessages { account: owner, room_id: room, messages }
                            if *owner == account && *room == room_id =>
                        {
                            Some(json!(messages))
                        }
                        _ => None,
                    }
                })
                .await
        }
        Pins::Add(event) | Pins::Remove(event) => {
            let (room_id, event_id) = target(client, &account, event).await?;
            client.change(Command::SetPinned { account, room_id, event_id, pinned }).await
        }
    }
}

pub(crate) async fn saved(client: &mut Client, account: AccountId, command: Saved) -> Result<Value> {
    let saved = matches!(command, Saved::Add(_));
    match command {
        Saved::List(filter) => {
            let room = client.optional_room(&account, filter.room.as_deref()).await?;
            client
                .run(Command::ListSavedMessages { account: account.clone() }, |_, event| match event {
                    Event::SavedMessages { account: owner, messages } if *owner == account => Some(json!(
                        messages
                            .iter()
                            .filter(|message| { room.as_ref().is_none_or(|room| *room == message.room_id) })
                            .collect::<Vec<_>>()
                    )),
                    _ => None,
                })
                .await
        }
        Saved::Add(event) | Saved::Remove(event) => {
            let (room_id, event_id) = target(client, &account, event).await?;
            client.change(Command::SetSaved { account, room_id, event_id, saved }).await
        }
    }
}

async fn body(body: Body) -> Result<(String, Option<String>)> {
    let text = if body.stdin {
        tokio::task::spawn_blocking(|| {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            Ok::<_, std::io::Error>(text)
        })
        .await??
    } else {
        body.text.unwrap_or_default()
    };
    if text.trim().is_empty() {
        return Err(CliError::Input(t("the message is empty").into()));
    }
    let html = body.markdown.then(|| matrix_core::markdown::to_html(&text)).flatten();
    Ok((text, html))
}

pub(crate) async fn target(client: &mut Client, account: &AccountId, args: EventTarget) -> Result<(String, String)> {
    let room = client.resolve(account, &args.target.room).await?;
    Ok((room, args.event_id))
}

async fn key(client: &mut Client, account: &AccountId, args: TimelineTarget) -> Result<TimelineKey> {
    let room = client.resolve(account, &args.target.room).await?;
    Ok(TimelineKey { room_id: room, thread: args.thread })
}
