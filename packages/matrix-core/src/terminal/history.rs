use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use futures_util::{StreamExt, TryStreamExt};
use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::TimelineEvent;
use matrix_sdk::event_cache::RedecryptorReport;
use matrix_sdk::room::{IncludeRelations, MessagesOptions, RelationsOptions};
use matrix_sdk::ruma::events::relation::RelationType;
use matrix_sdk::ruma::{EventId, OwnedEventId, OwnedRoomId, UInt, serde::Raw};
use serde_json::{Value, json};

use crate::{Account, Error, MediaSize, TimelineKey};

/// Exclude relation events from the message stream. A missing key remains visible in normal
/// history, so an encrypted timeline doesn't appear to have no messages at all.
fn message_row(room: &Room, raw: &Value, highlighted: bool) -> Option<Value> {
    let id = raw["event_id"].as_str()?;
    let sender = raw["sender"].as_str()?;
    let content = &raw["content"];
    let relation = &content["m.relates_to"];
    let event_type = raw["type"].as_str()?;
    if relation["rel_type"] == "m.replace"
        || !matches!(event_type, "m.room.message" | "m.sticker" | "m.room.encrypted" | "org.matrix.msc3381.poll.start")
    {
        return None;
    }
    let edit = &raw["unsigned"]["m.relations"]["m.replace"];
    let replacement = &edit["content"]["m.new_content"];
    let latest = if edit["sender"].as_str() == Some(sender) && replacement.is_object() { replacement } else { content };
    let body = latest["body"].as_str().map(|text| crate::html::shown_text(text, latest["formatted_body"].as_str()));
    let thread = (relation["rel_type"] == "m.thread").then(|| relation["event_id"].as_str()).flatten();
    Some(json!({
        "room_id": room.room_id(),
        "event_id": id,
        "sender_id": sender,
        "timestamp_ms": raw["origin_server_ts"],
        "body": body,
        "thread": thread,
        "highlighted": highlighted,
        "undecryptable": event_type == "m.room.encrypted",
        "redacted": !raw["unsigned"]["redacted_because"].is_null(),
        "type": event_type,
    }))
}

async fn highlight<T>(room: &Room, raw: &Raw<T>) -> bool {
    room.event_push_actions(raw)
        .await
        .ok()
        .flatten()
        .is_some_and(|actions| actions.iter().any(matrix_sdk::ruma::push::Action::is_highlight))
}

pub(super) async fn page(
    account: &Account,
    key: &TimelineKey,
    limit: u16,
    cursor: Option<String>,
) -> Result<Value, Error> {
    let room = account.joined_room(&key.room_id)?;
    let read = || read_page(&room, key.thread.as_deref(), limit, cursor.clone());
    let (mut events, mut next) = read().await?;
    if events.iter().any(is_undecrypted) && fetch_backup(account, &room).await {
        (events, next) = read().await?;
    }
    let mut messages = Vec::new();
    for event in events {
        let Ok(mut raw) = event.raw().deserialize_as_unchecked::<Value>() else { continue };
        decrypt_bundled_edit(&room, &mut raw).await;
        if let Some(row) = message_row(&room, &raw, highlight(&room, event.raw()).await) {
            messages.push(row);
        }
    }
    Ok(json!({ "messages": messages, "next_cursor": next }))
}

async fn decrypt_bundled_edit(room: &Room, event: &mut Value) {
    let edit = &event["unsigned"]["m.relations"]["m.replace"];
    if edit["type"] == "m.room.encrypted"
        && let Ok(raw) = Raw::new(edit)
        && let Ok(decrypted) = room.decrypt_event(&raw.cast_unchecked(), None).await
        && let Ok(edit) = decrypted.raw().deserialize_as_unchecked::<Value>()
        && edit["type"] == "m.room.message"
    {
        event["unsigned"]["m.relations"]["m.replace"] = edit;
    }
}

async fn read_page(
    room: &Room,
    thread: Option<&str>,
    limit: u16,
    cursor: Option<String>,
) -> Result<(Vec<TimelineEvent>, Option<String>), Error> {
    if let Some(thread) = thread {
        return relations(room, thread, RelationType::Thread, limit, cursor).await;
    }
    let mut options = MessagesOptions::backward();
    options.limit = UInt::from(limit);
    options.from = cursor;
    let page = room.messages(options).await?;
    Ok((page.chunk, page.end))
}

fn is_undecrypted(event: &TimelineEvent) -> bool {
    event.raw().get_field::<String>("type").ok().flatten().as_deref() == Some("m.room.encrypted")
}

async fn fetch_backup(account: &Account, room: &Room) -> bool {
    let fetch = async {
        if !room.client().encryption().backups().are_enabled().await {
            return false;
        }
        account.fetch_room_keys(room.room_id().as_str()).await.is_ok()
    };
    tokio::time::timeout(Duration::from_secs(5), fetch).await.unwrap_or(false)
}

pub(super) async fn event(account: &Account, room: &Room, id: &EventId) -> Result<TimelineEvent, Error> {
    let event = room.load_or_fetch_event(id, None).await?;
    if is_undecrypted(&event) && fetch_backup(account, room).await {
        return Ok(room.event(id, None).await?);
    }
    Ok(event)
}

pub(super) async fn relations(
    room: &Room,
    event: &str,
    relation: RelationType,
    limit: u16,
    cursor: Option<String>,
) -> Result<(Vec<TimelineEvent>, Option<String>), Error> {
    let request = RelationsOptions {
        from: cursor,
        limit: Some(UInt::from(limit)),
        include_relations: IncludeRelations::RelationsOfType(relation),
        ..RelationsOptions::default()
    };
    let page = room.relations(EventId::parse(event)?, request).await?;
    Ok((page.chunk, page.next_batch_token))
}

pub(super) async fn message(account: &Account, room: &str, id: &str) -> Result<Value, Error> {
    let room = account.joined_room(room)?;
    let event_id = EventId::parse(id)?;
    let original = event(account, &room, &event_id).await?;
    let raw = original.raw().deserialize_as_unchecked::<Value>().map_err(matrix_sdk::Error::from)?;
    let mut row = message_row(&room, &raw, highlight(&room, original.raw()).await).ok_or(Error::MessageUnavailable)?;
    if room.encryption_state().is_encrypted()
        && raw["type"] == "m.room.message"
        && raw["unsigned"]["redacted_because"].is_null()
        && let Some(edit) = latest_edit(&room, &event_id, &raw).await?
    {
        let new = &edit["content"]["m.new_content"];
        if let Some(body) = new["body"].as_str() {
            row["body"] = json!(crate::html::shown_text(body, new["formatted_body"].as_str()));
        }
    }
    Ok(row)
}

pub(super) async fn latest_edit(room: &Room, id: &EventId, original: &Value) -> Result<Option<Value>, Error> {
    let (edits, _) = relations(room, id.as_str(), RelationType::Replacement, 30, None).await?;
    Ok(edits
        .into_iter()
        .filter_map(|edit| edit.raw().deserialize_as_unchecked::<Value>().ok())
        .filter(|edit| edit["sender"] == original["sender"] && edit["content"]["m.new_content"].is_object())
        .max_by_key(|edit| edit["origin_server_ts"].as_u64()))
}

pub(super) async fn mentions(account: &Account, room: Option<&str>, since_ms: u64, limit: u16) -> Result<Value, Error> {
    let rooms = match room {
        Some(room) => vec![account.joined_room(room)?],
        None => account.client().joined_rooms().into_iter().filter(|room| !room.is_space()).collect(),
    };
    let rooms_scanned = rooms.len();
    let pages: Vec<_> = futures_util::stream::iter(
        rooms
            .into_iter()
            .map(|room| async move { room_highlights(account, &room, since_ms, usize::from(limit)).await }),
    )
    .buffer_unordered(6)
    .try_collect()
    .await?;
    let mut incomplete = false;
    let mut undecryptable = 0;
    let mut matches = Vec::new();
    for (rows, more, encrypted) in pages {
        matches.extend(rows);
        incomplete |= more;
        undecryptable += encrypted;
    }
    matches.sort_by(|a, b| {
        b["timestamp_ms"]
            .as_u64()
            .cmp(&a["timestamp_ms"].as_u64())
            .then_with(|| a["event_id"].as_str().cmp(&b["event_id"].as_str()))
    });
    matches.truncate(usize::from(limit));
    Ok(json!({
        "messages": matches,
        "rooms_scanned": rooms_scanned,
        "scan_limit_reached": incomplete,
        "undecryptable": undecryptable,
    }))
}

async fn room_highlights(
    account: &Account,
    room: &Room,
    since_ms: u64,
    limit: usize,
) -> Result<(Vec<Value>, bool, usize), Error> {
    const MAX_PAGES: usize = 3;
    let mut matches = Vec::new();
    let mut undecryptable = 0;
    let mut from = None;
    let mut checked_backup = false;
    for _ in 0..MAX_PAGES {
        let (mut events, mut next) = read_page(room, None, 100, from.clone()).await?;
        if !checked_backup && events.iter().any(is_undecrypted) {
            checked_backup = true;
            if fetch_backup(account, room).await {
                (events, next) = read_page(room, None, 100, from.clone()).await?;
            }
        }
        let mut older = false;
        for event in events {
            let Ok(raw) = event.raw().deserialize_as_unchecked::<Value>() else { continue };
            if raw["origin_server_ts"].as_u64().is_some_and(|ts| ts < since_ms) {
                older = true;
                continue;
            }
            if raw["type"] == "m.room.encrypted" {
                undecryptable += 1;
            }
            if highlight(room, event.raw()).await
                && let Some(row) = message_row(room, &raw, true)
            {
                matches.push(row);
            }
        }
        if older || next.is_none() || matches.len() >= limit {
            return Ok((matches, false, undecryptable));
        }
        from = next;
    }
    Ok((matches, true, undecryptable))
}

pub(super) async fn watch(
    account: &Account,
    key: Option<TimelineKey>,
    highlights: bool,
    report: impl Fn(Value) + Send + Sync,
    sync: &crate::engine::WatchSync,
) -> Result<(), Error> {
    let client = account.client();
    if let Some(key) = &key {
        account.joined_room(&key.room_id)?;
        if let Some(thread) = &key.thread {
            EventId::parse(thread)?;
        }
    }
    let mut updates = sync.updates.subscribe();
    let cache = client.event_cache();
    let reports = cache.subscribe_to_decryption_reports();
    tokio::pin!(reports);
    let mut watch = Watch::new(account, key, highlights);
    if let Ok(mut checkpoints) = sync.checkpoints.lock() {
        watch.latest.extend(checkpoints.drain());
    }
    sync.ready.notify_one();

    loop {
        tokio::select! {
            batch = updates.recv() => {
                let batch = batch.map_err(|error| Error::Account(format!("watch lost a sync update: {error}")))?;
                watch.receive(account, &batch, &report).await?;
            }
            Some(result) = reports.next() => {
                if let Ok(RedecryptorReport::ResolvedUtds { events, .. }) = result {
                    for id in events {
                        let Some(room) = watch.pending.remove(&id) else { continue };
                        let Ok(event) = room.load_or_fetch_event(&id, None).await else { continue };
                        watch.report(&room, &event, &report).await;
                    }
                }
            }
        }
    }
}

struct Watch {
    key: Option<TimelineKey>,
    highlights: bool,
    latest: HashMap<OwnedRoomId, OwnedEventId>,
    pending: HashMap<OwnedEventId, Room>,
    ids: HashSet<OwnedEventId>,
    order: VecDeque<OwnedEventId>,
}

impl Watch {
    fn new(account: &Account, key: Option<TimelineKey>, highlights: bool) -> Self {
        use matrix_sdk_base::latest_event::LatestEventValue;
        let latest = account
            .client()
            .joined_rooms()
            .into_iter()
            .filter_map(|room| {
                let LatestEventValue::Remote(event) = room.latest_event() else { return None };
                Some((room.room_id().to_owned(), event.event_id()?.to_owned()))
            })
            .collect();
        Self { key, highlights, latest, pending: HashMap::new(), ids: HashSet::new(), order: VecDeque::new() }
    }

    async fn receive(
        &mut self,
        account: &Account,
        batch: &matrix_sdk::sync::RoomUpdates,
        report: &impl Fn(Value),
    ) -> Result<(), Error> {
        for (room_id, update) in &batch.joined {
            if self.key.as_ref().is_some_and(|key| key.room_id != room_id.as_str()) {
                continue;
            }
            let Some(room) = account.client().get_room(room_id) else { continue };
            if room.is_space() {
                continue;
            }
            let timeline = &update.timeline;
            let mut live = timeline.events.as_slice();
            let mut gap = Vec::new();
            // A room first seen during a watch begins with its first observed timeline.
            if timeline.limited
                && let Some(boundary) = self.latest.get(room_id)
            {
                if let Some(position) = live.iter().position(|event| event.event_id() == Some(boundary)) {
                    live = &live[position + 1..];
                } else {
                    let cursor = timeline.prev_batch.as_deref().ok_or_else(|| missing_gap(&room))?;
                    gap = tokio::time::timeout(Duration::from_secs(15), catch_up(account, &room, cursor, boundary))
                        .await
                        .map_err(|_| missing_gap(&room))??;
                }
            }
            for event in gap.iter().chain(live) {
                if let Some(id) = event.event_id()
                    && self.latest.get(room_id).is_none_or(|known| known != id)
                    && self.remember(id)
                {
                    self.latest.insert(room_id.clone(), id.to_owned());
                    if is_undecrypted(event) {
                        self.pending.insert(id.to_owned(), room.clone());
                    } else {
                        self.report(&room, event, report).await;
                    }
                }
            }
        }
        Ok(())
    }

    fn remember(&mut self, id: &EventId) -> bool {
        const REMEMBERED: usize = 4096;
        if !self.ids.insert(id.to_owned()) {
            return false;
        }
        self.order.push_back(id.to_owned());
        if self.order.len() > REMEMBERED
            && let Some(old) = self.order.pop_front()
        {
            self.ids.remove(&old);
            self.pending.remove(&old);
        }
        true
    }

    async fn report(&self, room: &Room, event: &TimelineEvent, report: &impl Fn(Value)) {
        if let Ok(raw) = event.raw().deserialize_as_unchecked::<Value>()
            && let Some(row) =
                watch_row(room, &raw, highlight(room, event.raw()).await, self.key.as_ref(), self.highlights)
        {
            report(row);
        }
    }
}

async fn catch_up(
    account: &Account,
    room: &Room,
    cursor: &str,
    boundary: &EventId,
) -> Result<Vec<TimelineEvent>, Error> {
    const MAX_PAGES: usize = 10;
    let mut cursor = cursor.to_owned();
    let mut gap = Vec::new();
    for _ in 0..MAX_PAGES {
        let (events, next) = read_page(room, None, 100, Some(cursor.clone())).await?;
        for event in events {
            if event.event_id() == Some(boundary) {
                decrypt_gap(account, room, &mut gap).await?;
                gap.reverse();
                return Ok(gap);
            }
            gap.push(event);
        }
        let Some(next) = next.filter(|next| *next != cursor) else { break };
        cursor = next;
    }
    Err(missing_gap(room))
}

async fn decrypt_gap(account: &Account, room: &Room, gap: &mut [TimelineEvent]) -> Result<(), Error> {
    if gap.iter().any(is_undecrypted) {
        // /messages events are outside the event cache. Retry them directly with the keys now
        // on disk, and account for the whole recovered span before reporting any of its events.
        fetch_backup(account, room).await;
        for event in gap.iter_mut().filter(|event| is_undecrypted(event)) {
            let decrypted = room.decrypt_event(event.raw().cast_ref_unchecked(), None).await?;
            if is_undecrypted(&decrypted) {
                return Err(missing_gap(room));
            }
            *event = decrypted;
        }
    }
    Ok(())
}

fn missing_gap(room: &Room) -> Error {
    Error::Account(format!("could not recover a gap in the watch for {}; check message history", room.room_id()))
}

fn watch_row(
    room: &Room,
    raw: &Value,
    highlighted: bool,
    key: Option<&TimelineKey>,
    highlights: bool,
) -> Option<Value> {
    if highlights && !highlighted {
        return None;
    }
    let row = message_row(room, raw, highlighted)?;
    let thread = key.and_then(|key| key.thread.as_deref());
    thread
        .is_none_or(|root| row["thread"].as_str() == Some(root) || row["event_id"].as_str() == Some(root))
        .then_some(row)
}

pub(super) async fn download(account: &Account, room: &str, event: &str, path: &Path) -> Result<(), Error> {
    let room = account.joined_room(room)?;
    let event = self::event(account, &room, &EventId::parse(event)?).await?;
    let raw = event.raw().deserialize_as_unchecked::<Value>().map_err(matrix_sdk::Error::from)?;
    let attachment = crate::history::attachment(&raw).ok_or(Error::MessageUnavailable)?;
    let media = attachment.media.ok_or(Error::MessageUnavailable)?;
    let bytes = account.fetch_media(&media, MediaSize::Original).await?;
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(Error::Storage)?;
        file.write_all(&bytes).map_err(Error::Storage)?;
        file.persist_noclobber(path).map_err(|error| Error::Storage(error.error))?;
        Ok(())
    })
    .await
    .map_err(|error| Error::Account(error.to_string()))?
}
