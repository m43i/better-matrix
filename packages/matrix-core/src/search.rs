//! Local message search and the fuzzy matcher used by search result highlights.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use matrix_sdk::cross_process_lock::MappedCrossProcessLockState;
use matrix_sdk::deserialized_responses::TimelineEvent;
use serde_json::Value;

use crate::{Account, Error};

mod usage;
pub use usage::SearchUsage;
pub(crate) use usage::Usage;

/// A message found in the account's stored history. Offsets count Unicode scalar values.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct MessageHit {
    /// The room containing the message.
    pub room_id: String,
    /// The message to open.
    pub event_id: String,
    /// The root to open first when this is a thread reply.
    pub thread: Option<String>,
    /// Who sent the message (the UI resolves a cached display name when available).
    pub sender: String,
    /// When it was sent, in milliseconds since the epoch.
    pub timestamp_ms: u64,
    /// A plain text excerpt around the match.
    pub body: String,
    /// Characters of `body` matched by the query, in ascending order.
    pub highlights: Vec<usize>,
}

/// A fuzzy match: character positions and a rank (smaller is better).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyMatch {
    /// Matched Unicode scalar positions, never byte offsets.
    pub positions: Vec<usize>,
    /// Prefer consecutive characters, word starts and earlier matches.
    pub rank: usize,
}

/// Match each query word as a case-insensitive subsequence, preferring a contiguous match.
/// Whitespace separates independent terms; all must match. The phones run the same vectors.
pub fn fuzzy_match(text: &str, query: &str) -> Option<FuzzyMatch> {
    let chars: Vec<char> = text.chars().collect();
    // Keep an original position for every folded character (lowercase can expand one scalar).
    let folded: Vec<(char, usize)> =
        chars.iter().enumerate().flat_map(|(i, c)| fold(*c).map(move |c| (c, i))).collect();
    let mut occurrences: HashMap<char, Vec<usize>> = HashMap::new();
    for (i, &(c, _)) in folded.iter().enumerate() {
        occurrences.entry(c).or_default().push(i);
    }
    let mut positions = BTreeSet::new();
    let mut rank = 0;
    for term in query.split_whitespace() {
        let needle: Vec<char> = term.chars().flat_map(fold).collect();
        let mut best: Option<(usize, Vec<usize>)> = None;
        if needle.iter().any(|c| !occurrences.contains_key(c)) {
            return None;
        }
        for &start in occurrences.get(&needle[0])? {
            let mut at = start;
            let mut found = vec![folded[start].1];
            for c in &needle[1..] {
                let candidates = occurrences.get(c)?;
                let next = candidates.partition_point(|&i| i <= at);
                let Some(&index) = candidates.get(next) else { break };
                at = index;
                found.push(folded[at].1);
            }
            if found.len() != needle.len() {
                continue;
            }
            let first = found[0];
            let last = found[found.len() - 1];
            let gaps = (last - first + 1).saturating_sub(needle.len());
            let boundary = first == 0 || !chars[first - 1].is_alphanumeric();
            let score = gaps * 100 + usize::from(!boundary) * 10 + first;
            if best.as_ref().is_none_or(|(score_before, _)| score < *score_before) {
                best = Some((score, found));
            }
            // A consecutive word-start match cannot improve enough to justify scanning on.
            if gaps == 0 && boundary {
                break;
            }
        }
        let (score, found) = best?;
        rank += score;
        positions.extend(found);
    }
    Some(FuzzyMatch { positions: positions.into_iter().collect(), rank })
}

fn fold(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().map(|c| if c == 'ς' { 'σ' } else { c })
}

impl Account {
    /// Search only joined rooms' locally stored, decrypted history; never fetch or paginate.
    /// Reads the SDK's encrypted-at-rest cache, including closed rooms and thread-only events.
    /// Results are bounded, ranked by local interaction frequency, then match quality and recency.
    pub async fn search_messages(&self, term: &str, usage: &SearchUsage) -> Result<Vec<MessageHit>, Error> {
        if term.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut found = Vec::new();
        for room in self.client().joined_rooms() {
            if room.is_space() {
                continue;
            }
            let events = {
                let lock = self.client().event_cache_store().lock().await.map_err(|e| Error::Account(e.to_string()))?;
                // Reading persistent events doesn't rely on the in-memory cache's generation.
                let (MappedCrossProcessLockState::Clean(store) | MappedCrossProcessLockState::Dirty(store)) = lock;
                store.get_room_events(room.room_id(), None, None).await.map_err(|e| Error::Account(e.to_string()))?
            };
            let query = term.to_owned();
            let room_id = room.room_id().to_string();
            // Parsing and fuzzy matching a large history must not occupy a runtime worker.
            let hits = tokio::task::spawn_blocking(move || search_events(&room_id, &events, &query))
                .await
                .map_err(|e| Error::Account(e.to_string()))?;
            found.extend(hits);
            found.sort_by(|(a, x), (b, y)| {
                usage
                    .message(&y.room_id, &y.sender)
                    .cmp(&usage.message(&x.room_id, &x.sender))
                    .then(a.cmp(b))
                    .then(y.timestamp_ms.cmp(&x.timestamp_ms))
                    .then(x.room_id.cmp(&y.room_id))
                    .then(x.event_id.cmp(&y.event_id))
            });
            found.truncate(30);
        }
        Ok(found.into_iter().map(|(_, hit)| hit).collect())
    }
}

fn search_events(room: &str, events: &[TimelineEvent], term: &str) -> Vec<(usize, MessageHit)> {
    let events: Vec<Value> = events.iter().filter_map(|e| e.raw().deserialize_as_unchecked().ok()).collect();
    let mut redacted = HashSet::new();
    let mut edits: HashMap<&str, BTreeMap<(u64, &str), &Value>> = HashMap::new();
    for e in &events {
        if e["type"] == "m.room.redaction"
            && let Some(id) = e["content"]["redacts"].as_str().or_else(|| e["redacts"].as_str())
        {
            redacted.insert(id);
        }
        let relation = &e["content"]["m.relates_to"];
        if relation["rel_type"] == "m.replace"
            && let (Some(id), Some(edit_id)) = (relation["event_id"].as_str(), e["event_id"].as_str())
        {
            edits.entry(id).or_default().insert((e["origin_server_ts"].as_u64().unwrap_or(0), edit_id), e);
        }
    }
    let mut found = Vec::new();
    for e in &events {
        let Some(id) = e["event_id"].as_str() else { continue };
        let kind = e["type"].as_str().unwrap_or_default();
        if redacted.contains(id)
            || !e["unsigned"]["redacted_because"].is_null()
            || e["content"]["m.relates_to"]["rel_type"] == "m.replace"
        {
            continue;
        }
        let content = edits
            .get(id)
            .and_then(|edits| {
                edits.iter().rev().find_map(|((_, edit_id), edit)| {
                    (!redacted.contains(edit_id)
                        && edit["sender"] == e["sender"]
                        && edit["type"] == e["type"]
                        && message_body(kind, &edit["content"]["m.new_content"]).is_some())
                    .then_some(&edit["content"]["m.new_content"])
                })
            })
            .unwrap_or(&e["content"]);
        let Some(body) = message_body(kind, content) else { continue };
        let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
        let Some(matched) = fuzzy_match(&body, term) else { continue };
        let (body, highlights) = excerpt(&body, &matched.positions);
        found.push((
            matched.rank,
            MessageHit {
                room_id: room.to_owned(),
                event_id: id.to_owned(),
                thread: (e["content"]["m.relates_to"]["rel_type"] == "m.thread")
                    .then(|| e["content"]["m.relates_to"]["event_id"].as_str().map(str::to_owned))
                    .flatten(),
                sender: e["sender"].as_str().unwrap_or_default().to_owned(),
                timestamp_ms: e["origin_server_ts"].as_u64().unwrap_or(0),
                body,
                highlights,
            },
        ));
    }
    found
}

fn message_body<'a>(kind: &str, content: &'a Value) -> Option<&'a str> {
    match kind {
        "m.room.message" | "m.sticker" => content["body"].as_str(),
        "org.matrix.msc3381.poll.start" => content["org.matrix.msc1767.text"]
            .as_str()
            .or_else(|| content["org.matrix.msc3381.poll.start"]["question"]["org.matrix.msc1767.text"].as_str()),
        _ => None,
    }
}

fn excerpt(body: &str, positions: &[usize]) -> (String, Vec<usize>) {
    let chars: Vec<char> = body.chars().collect();
    let start = positions.first().copied().unwrap_or(0).saturating_sub(40);
    let end = (start + 220).min(chars.len());
    let prefix = usize::from(start > 0);
    let mut text = String::new();
    if start > 0 {
        text.push('…');
    }
    text.extend(&chars[start..end]);
    if end < chars.len() {
        text.push('…');
    }
    (text, positions.iter().filter(|&&i| i >= start && i < end).map(|i| i - start + prefix).collect())
}

#[cfg(test)]
mod tests;
