//! What calls hear from sync: media keys, rings, one-to-one call events and changes to a room's
//! call memberships. Event handlers on each account's client turn them into [`Signal`]s on one
//! channel, which the engine routes to the call under way or turns into a ringing call.

use matrix_sdk::Client;
use matrix_sdk::deserialized_responses::EncryptionInfo;
use matrix_sdk::event_handler::EventHandlerHandle;
use matrix_sdk::ruma::events::{AnySyncStateEvent, AnySyncTimelineEvent, AnyToDeviceEvent};
use matrix_sdk::ruma::serde::Raw;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use super::keys::{self, KEYS_EVENT, ReceivedKey};
use crate::model::AccountId;

/// Ring notifications, stable and unstable; Element sends (and the app sends) the unstable one.
pub(crate) const RING_EVENT: &str = "org.matrix.msc4075.rtc.notification";
const RING_EVENTS: [&str; 2] = ["m.rtc.notification", RING_EVENT];
/// Call membership state: the type Element Call sends (and the app sends) first, then the stable one.
pub(crate) const MEMBER_EVENTS: [&str; 2] = ["org.matrix.msc3401.call.member", "m.call.member"];

/// Something sync delivered that a call cares about.
#[derive(Clone, Debug)]
pub(crate) enum Signal {
    /// Someone's media key.
    Key { account: AccountId, key: ReceivedKey },
    /// A MatrixRTC call rings in a room until `until` (milliseconds since the epoch).
    Ring { account: AccountId, room_id: String, caller: String, video: bool, until: u64 },
    /// An event of the one-to-one standard.
    Direct { account: AccountId, room_id: String, sender: String, sent: u64, event: DirectEvent },
    /// A room's call memberships changed.
    Members { account: AccountId, room_id: String },
    /// A ring's time ran out: the engine's own, numbered to tell rings in one room apart.
    RingLapsed { account: AccountId, room_id: String, ring: u64 },
    /// A ring's caller was looked up: the engine's own, with the ring's number.
    RingReady(Box<(super::IncomingCall, u64)>),
    /// The user is no longer in a room (left elsewhere, kicked, banned): its call ends.
    Left { account: AccountId, room_id: String },
}

/// An event of the one-to-one standard (MSC2746), reduced to what the call needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DirectEvent {
    Invite { call_id: String, party_id: String, sdp: String, lifetime: u64, invitee: Option<String> },
    Answer { call_id: String, party_id: String, sdp: String },
    Candidates { call_id: String, party_id: String, candidates: Vec<IceCandidate> },
    SelectAnswer { call_id: String, party_id: String, selected: String },
    Hangup { call_id: String, party_id: String },
    Reject { call_id: String, party_id: String },
}

impl DirectEvent {
    pub fn call_id(&self) -> &str {
        match self {
            Self::Invite { call_id, .. }
            | Self::Answer { call_id, .. }
            | Self::Candidates { call_id, .. }
            | Self::SelectAnswer { call_id, .. }
            | Self::Hangup { call_id, .. }
            | Self::Reject { call_id, .. } => call_id,
        }
    }
}

/// One ICE candidate, as `m.call.candidates` carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: String,
    pub sdp_m_line_index: i32,
}

/// Starts hearing an account's call signals; the handles stop it, and the task reading them in
/// order (which the account's teardown aborts, so nothing queued is heard after it).
pub(crate) fn listen(
    client: &Client,
    account: &AccountId,
    signals: &UnboundedSender<Signal>,
    sticky: &super::StickyMembers,
) -> (Vec<EventHandlerHandle>, tokio::task::AbortHandle) {
    let to_device = {
        let (account, signals) = (account.clone(), signals.clone());
        client.add_event_handler(move |raw: Raw<AnyToDeviceEvent>, encryption: Option<EncryptionInfo>| {
            let (account, signals) = (account.clone(), signals.clone());
            async move {
                // Most to-device events are something else (room keys): only keys are read whole.
                if raw.get_field::<String>("type").ok().flatten().as_deref() != Some(KEYS_EVENT) {
                    return;
                }
                let Ok(event) = raw.deserialize_as_unchecked::<Value>() else { return };
                if let Some(key) = to_device_key(&event, encryption.as_ref()) {
                    let _ = signals.send(Signal::Key { account, key });
                }
            }
        })
    };
    // Call events, and fresh events the SDK couldn't decrypt yet, in the order they came: one
    // queue per account, read beside the sync (which waits for its handlers).
    let (queue, queued) = tokio::sync::mpsc::unbounded_channel();
    let reader = tokio::spawn(read_in_order(account.clone(), signals.clone(), queued)).abort_handle();
    let timeline = {
        let sticky = sticky.clone();
        client.add_event_handler(
            move |raw: Raw<AnySyncTimelineEvent>, room: matrix_sdk::Room, encryption: Option<EncryptionInfo>| {
                let (queue, sticky) = (queue.clone(), sticky.clone());
                async move {
                    // Most events are none of these: only theirs are read whole.
                    let kind = raw.get_field::<String>("type").ok().flatten().unwrap_or_default();
                    if super::sticky::RTC_MEMBER_EVENTS.contains(&kind.as_str()) || kind == "m.room.redaction" {
                        let Ok(event) = raw.deserialize_as_unchecked::<Value>() else { return };
                        let trusted = encryption.is_some() || !room.encryption_state().is_encrypted();
                        sticky_heard(&room, &event, trusted, &sticky);
                        return;
                    }
                    let sent = raw.get_field::<u64>("origin_server_ts").ok().flatten().unwrap_or_default();
                    let undecrypted = kind == "m.room.encrypted" && awaits_key(sent, crate::engine::now_ms());
                    if concerns_calls(&kind) || undecrypted {
                        let (encrypted, arrived) = (encryption.is_some(), tokio::time::Instant::now());
                        let _ = queue.send(Queued { raw, room, encrypted, undecrypted, arrived });
                    }
                }
            },
        )
    };
    let state = {
        let (account, signals, sticky) = (account.clone(), signals.clone(), sticky.clone());
        let me = client.user_id().map(ToString::to_string).unwrap_or_default();
        client.add_event_handler(move |raw: Raw<AnySyncStateEvent>, room: matrix_sdk::Room| {
            let (account, signals, me, sticky) = (account.clone(), signals.clone(), me.clone(), sticky.clone());
            async move {
                let kind = raw.get_field::<String>("type").ok().flatten().unwrap_or_default();
                let room_id = room.room_id().to_string();
                if MEMBER_EVENTS.contains(&kind.as_str()) {
                    let _ = signals.send(Signal::Members { account, room_id });
                } else if kind == "m.room.member" && raw_left(&me, &raw) {
                    let _ = signals.send(Signal::Left { account, room_id });
                } else if kind == "m.room.member" {
                    // A matrix_2_0 member who left or was removed is out of the call, whatever
                    // their sticky membership still says (the store reports it).
                    // Large rooms' members arrive by the thousand: only a call member's is read.
                    let user = raw.get_field::<String>("state_key").ok().flatten().unwrap_or_default();
                    let now = crate::engine::now_ms();
                    if !sticky.members(&room_id, now).iter().any(|member| member.user_id == user) {
                        return;
                    }
                    let membership = raw
                        .deserialize_as_unchecked::<Value>()
                        .ok()
                        .and_then(|event| event["content"]["membership"].as_str().map(str::to_owned));
                    if membership.as_deref() != Some("join") {
                        sticky.left(&room_id, &user);
                    }
                }
            }
        })
    };
    (vec![to_device, timeline, state], reader)
}

/// A timeline event the calls read in order: a call event, or a fresh one still encrypted.
struct Queued {
    raw: Raw<AnySyncTimelineEvent>,
    room: matrix_sdk::Room,
    /// It came encrypted (and was decrypted).
    encrypted: bool,
    /// The SDK couldn't decrypt it yet: its key may still be on its way.
    undecrypted: bool,
    /// When sync delivered it: its wait for a key counts from then, so waits in a row don't
    /// add up.
    arrived: tokio::time::Instant,
}

/// How long a call event that came before its key waits for it. Under sliding sync the key
/// comes through the other connection (the to-device long poll), often a moment after the
/// room's events; a call's first events are usually the first of a new session.
const KEY_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
/// How often a waiting event tries again.
const KEY_RETRY: std::time::Duration = std::time::Duration::from_millis(250);
/// Only events younger than this wait for their key: older ones are past any call (an invite
/// lives a minute) and a catch-up sync's backlog mustn't hold the queue up.
const KEY_FRESH_MS: u64 = 2 * 60 * 1000;

/// Whether an undecryptable event sent at `sent` (ms since the epoch) is fresh enough to wait
/// for its key at `now`.
fn awaits_key(sent: u64, now: u64) -> bool {
    now.saturating_sub(sent) < KEY_FRESH_MS
}

/// Hands each room's queued events to that room's reader ([`read_room`]): a room waiting for a
/// key holds up only its own events.
async fn read_in_order(
    account: AccountId,
    signals: UnboundedSender<Signal>,
    queued: tokio::sync::mpsc::UnboundedReceiver<Queued>,
) {
    let room = |event: &Queued| event.room.room_id().to_owned();
    by_room(queued, room, |events| read_room(account.clone(), signals.clone(), events)).await;
}

/// Hands each of `queued` to the reader of its room (`room`), started with `read` on the
/// room's first. The readers end with this (their set is dropped when it's aborted, or when
/// `queued` closes).
async fn by_room<T, K, R, F>(
    mut queued: tokio::sync::mpsc::UnboundedReceiver<T>,
    room: impl Fn(&T) -> K,
    read: impl Fn(tokio::sync::mpsc::UnboundedReceiver<T>) -> R,
) where
    T: Send + 'static,
    K: std::hash::Hash + Eq,
    R: std::future::Future<Output = F> + Send + 'static,
    F: Send + 'static,
{
    let mut rooms: std::collections::HashMap<K, UnboundedSender<T>> = std::collections::HashMap::new();
    let mut readers = tokio::task::JoinSet::new();
    while let Some(event) = queued.recv().await {
        let reader = rooms.entry(room(&event)).or_insert_with(|| {
            let (reader, events) = tokio::sync::mpsc::unbounded_channel();
            readers.spawn(read(events));
            reader
        });
        let _ = reader.send(event);
    }
    // Closed: each reader finishes what it was handed.
    rooms.clear();
    while readers.join_next().await.is_some() {}
}

/// Turns one room's queued events into signals, in the order sync delivered them: one still
/// encrypted waits for its key first (until [`KEY_WAIT`] after it arrived), so an invite that
/// came before its key still rings, and its candidates and answer don't overtake it. Every event
/// waits at most that long after arriving, whatever waited before it: events of a session whose
/// key is late wait out the same few seconds together.
async fn read_room(
    account: AccountId,
    signals: UnboundedSender<Signal>,
    mut queued: tokio::sync::mpsc::UnboundedReceiver<Queued>,
) {
    while let Some(Queued { raw, room, encrypted, undecrypted, arrived }) = queued.recv().await {
        let (raw, encrypted) = if undecrypted {
            match decrypted_by(&room, &raw, arrived + KEY_WAIT).await {
                Some(decrypted) => (decrypted, true),
                None => {
                    tracing::debug!("call: an event in {} never got its key", room.room_id());
                    continue;
                }
            }
        } else {
            (raw, encrypted)
        };
        let kind = raw.get_field::<String>("type").ok().flatten().unwrap_or_default();
        if !concerns_calls(&kind) {
            continue;
        }
        let Ok(event) = raw.deserialize_as_unchecked::<Value>() else { continue };
        for signal in timeline_signals(&account, room.room_id().as_str(), &event, encrypted) {
            let _ = signals.send(signal);
        }
    }
}

/// An event the SDK couldn't decrypt, decrypted once its key came; `None` if it didn't come by
/// `deadline` (tried at least once, even past it).
async fn decrypted_by(
    room: &matrix_sdk::Room,
    raw: &Raw<AnySyncTimelineEvent>,
    deadline: tokio::time::Instant,
) -> Option<Raw<AnySyncTimelineEvent>> {
    tried_until(deadline, || async {
        // Without its key the SDK answers with the event still unreadable, not an error.
        let event = room.decrypt_event(raw.cast_ref_unchecked(), None).await.ok()?;
        event.encryption_info().is_some().then(|| event.into_raw())
    })
    .await
}

/// Tries `attempt` every [`KEY_RETRY`] until it gives something or `deadline` passes; the last
/// try comes at the deadline itself.
async fn tried_until<T, F: std::future::Future<Output = Option<T>>>(
    deadline: tokio::time::Instant,
    mut attempt: impl FnMut() -> F,
) -> Option<T> {
    loop {
        if let Some(found) = attempt().await {
            return Some(found);
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return None;
        }
        tokio::time::sleep_until((now + KEY_RETRY).min(deadline)).await;
    }
}

/// A matrix_2_0 membership (or the redaction of one) heard from the timeline: kept, and the
/// change reported (`StickyMembers::report_to`).
fn sticky_heard(room: &matrix_sdk::Room, event: &Value, trusted: bool, sticky: &super::StickyMembers) {
    let room_id = room.room_id().to_string();
    if event["type"] == "m.room.redaction" {
        let redacts = event["redacts"].as_str().or_else(|| event["content"]["redacts"].as_str()).unwrap_or_default();
        sticky.redacted(&room_id, redacts);
    } else {
        sticky.heard(&room_id, event, trusted, crate::engine::now_ms());
    }
}

/// [`left`] for an event as it arrives. A large room's members arrive by the thousand: only the
/// user's own is read whole.
fn raw_left(me: &str, raw: &Raw<AnySyncStateEvent>) -> bool {
    raw.get_field::<String>("state_key").ok().flatten().as_deref() == Some(me)
        && raw.deserialize_as_unchecked::<Value>().is_ok_and(|event| left(me, &event))
}

/// Whether a room membership event takes the user `me` out of the room.
fn left(me: &str, event: &Value) -> bool {
    event["state_key"] == me && matches!(event["content"]["membership"].as_str(), Some("leave" | "ban"))
}

/// The key a to-device event carries, if it came encrypted from the device it claims: a key in
/// the clear, or one naming another device, could come from anyone.
fn to_device_key(event: &Value, encryption: Option<&EncryptionInfo>) -> Option<ReceivedKey> {
    let encryption = encryption?;
    let sender = event["sender"].as_str().unwrap_or_default();
    let key = keys::from_to_device(sender, &event["content"])?;
    let from_device = encryption.sender_device.as_ref().is_none_or(|device| device.as_str() == key.device_id);
    (encryption.sender.as_str() == sender && from_device).then_some(key)
}

/// Whether a timeline event of type `kind` may carry a call signal.
fn concerns_calls(kind: &str) -> bool {
    kind == KEYS_EVENT || RING_EVENTS.contains(&kind) || kind.starts_with("m.call.")
}

/// The signals in one timeline event of `room_id`; `encrypted`: it arrived encrypted. Membership changes come from the state
/// handler, which sees them wherever they arrive.
fn timeline_signals(account: &AccountId, room_id: &str, event: &Value, encrypted: bool) -> Vec<Signal> {
    let kind = event["type"].as_str().unwrap_or_default();
    let sender = event["sender"].as_str().unwrap_or_default().to_owned();
    let sent = event["origin_server_ts"].as_u64().unwrap_or_default();
    let content = &event["content"];
    let (account, room_id) = (account.clone(), room_id.to_owned());
    // A media key in the clear could come from anyone who can write to the room.
    if kind == KEYS_EVENT {
        if !encrypted {
            return Vec::new();
        }
        return keys::from_room_event(&sender, content)
            .into_iter()
            .map(|key| Signal::Key {
                account: account.clone(),
                key: ReceivedKey { room_id: Some(room_id.clone()), ..key },
            })
            .collect();
    }
    if RING_EVENTS.contains(&kind) {
        if content["notification_type"] != "ring" {
            return Vec::new();
        }
        let lifetime = content["lifetime"].as_u64().unwrap_or(30_000);
        let video = content["m.call.intent"] == "video" || content["call_intent"] == "video";
        return vec![Signal::Ring { account, room_id, caller: sender, video, until: sent.saturating_add(lifetime) }];
    }
    direct_event(kind, content)
        .map(|event| Signal::Direct { account, room_id, sender, sent, event })
        .into_iter()
        .collect()
}

/// An `m.call.*` event's content, read leniently: version 0 events lack party IDs.
fn direct_event(kind: &str, content: &Value) -> Option<DirectEvent> {
    let text = |key: &str| content[key].as_str().unwrap_or_default().to_owned();
    let (call_id, party_id) = (text("call_id"), text("party_id"));
    if call_id.is_empty() {
        return None;
    }
    Some(match kind {
        "m.call.invite" => DirectEvent::Invite {
            sdp: content["offer"]["sdp"].as_str()?.to_owned(),
            lifetime: content["lifetime"].as_u64().unwrap_or(60_000),
            invitee: content["invitee"].as_str().map(str::to_owned),
            call_id,
            party_id,
        },
        "m.call.answer" => {
            DirectEvent::Answer { sdp: content["answer"]["sdp"].as_str()?.to_owned(), call_id, party_id }
        }
        "m.call.candidates" => DirectEvent::Candidates {
            candidates: content["candidates"]
                .as_array()?
                .iter()
                .filter_map(|candidate| {
                    Some(IceCandidate {
                        candidate: candidate["candidate"].as_str()?.to_owned(),
                        sdp_mid: candidate["sdpMid"].as_str().unwrap_or_default().to_owned(),
                        sdp_m_line_index: candidate["sdpMLineIndex"].as_i64().and_then(|i| i32::try_from(i).ok())?,
                    })
                })
                .collect(),
            call_id,
            party_id,
        },
        "m.call.select_answer" => DirectEvent::SelectAnswer { selected: text("selected_party_id"), call_id, party_id },
        "m.call.hangup" => DirectEvent::Hangup { call_id, party_id },
        "m.call.reject" => DirectEvent::Reject { call_id, party_id },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_key_coming_just_before_the_deadline_should_still_count() {
        let start = tokio::time::Instant::now();
        // The key comes 9.9 s in: between the last regular try and the deadline.
        let key_at = start + std::time::Duration::from_millis(9_900);
        let found = tried_until(start + KEY_WAIT, || async { (tokio::time::Instant::now() >= key_at).then_some(()) });
        assert_eq!(found.await, Some(()));
        let late = start + KEY_WAIT + std::time::Duration::from_millis(1);
        let missed = tried_until(start + KEY_WAIT, || async { (tokio::time::Instant::now() >= late).then_some(()) });
        assert_eq!(missed.await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_room_waiting_for_a_key_should_hold_up_only_its_own_events() {
        // Each event: its room, and how long its reader takes over it (a wait for its key).
        let (queue, queued) = tokio::sync::mpsc::unbounded_channel::<(&'static str, u64, &'static str)>();
        let (heard, mut order) = tokio::sync::mpsc::unbounded_channel();
        let read = move |mut events: tokio::sync::mpsc::UnboundedReceiver<(&'static str, u64, &'static str)>| {
            let heard = heard.clone();
            async move {
                while let Some((_, wait, name)) = events.recv().await {
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    let _ = heard.send((name, tokio::time::Instant::now()));
                }
            }
        };
        let start = tokio::time::Instant::now();
        let reading = tokio::spawn(by_room(queued, |event| event.0, read));
        for event in [("!a", 10, "a's message"), ("!b", 0, "b's answer"), ("!a", 0, "a's next")] {
            let _ = queue.send(event);
        }
        drop(queue);
        let mut heard = Vec::new();
        while let Some((name, at)) = order.recv().await {
            heard.push((name, (at - start).as_secs()));
            if heard.len() == 3 {
                break;
            }
        }
        let _ = reading.await;
        // Room b's answer isn't held up by room a's wait; room a keeps its own order.
        assert_eq!(heard, [("b's answer", 0), ("a's message", 10), ("a's next", 10)]);
    }

    #[test]
    fn only_a_fresh_undecryptable_event_should_wait_for_its_key() {
        let now = 10_000_000;
        assert!(awaits_key(now - 1_000, now));
        assert!(!awaits_key(now - KEY_FRESH_MS, now));
        // A sender's clock running ahead still counts as fresh.
        assert!(awaits_key(now + 5_000, now));
    }

    fn account() -> AccountId {
        AccountId::new("@me:example.org")
    }

    #[test]
    fn a_ring_should_say_who_calls_and_until_when() {
        let event = serde_json::json!({
            "type": "org.matrix.msc4075.rtc.notification", "sender": "@ada:example.org", "origin_server_ts": 1000,
            "content": { "notification_type": "ring", "lifetime": 30000, "m.call.intent": "video" }
        });
        let signals = timeline_signals(&account(), "!r", &event, true);
        assert!(
            matches!(&signals[..], [Signal::Ring { caller, video: true, until: 31_000, .. }] if caller == "@ada:example.org")
        );
    }

    #[test]
    fn an_older_clients_room_key_should_belong_to_its_room() {
        let event = serde_json::json!({
            "type": KEYS_EVENT, "sender": "@ada:example.org", "origin_server_ts": 1,
            "content": { "keys": [{ "index": 0, "key": "AAAAAAAAAAAAAAAAAAAAAA" }], "device_id": "D", "call_id": "" }
        });
        let signals = timeline_signals(&account(), "!r", &event, true);
        assert!(matches!(&signals[..], [Signal::Key { key, .. }] if key.room_id.as_deref() == Some("!r")));
    }

    #[test]
    fn an_older_clients_room_key_in_the_clear_should_be_ignored() {
        let event = serde_json::json!({
            "type": KEYS_EVENT, "sender": "@ada:example.org", "origin_server_ts": 1,
            "content": { "keys": [{ "index": 0, "key": "AAAAAAAAAAAAAAAAAAAAAA" }], "device_id": "D", "call_id": "" }
        });
        assert!(timeline_signals(&account(), "!r", &event, false).is_empty());
    }

    #[test]
    fn a_key_in_the_clear_should_be_ignored() {
        let event = serde_json::json!({
            "type": KEYS_EVENT, "sender": "@ada:example.org",
            "content": { "room_id": "!r", "media_key": { "index": 0, "key": "AAAAAAAAAAAAAAAAAAAAAA" }, "member": { "claimed_device_id": "D" } }
        });
        assert!(to_device_key(&event, None).is_none());
    }

    #[test]
    fn being_kicked_should_count_as_leaving_and_someone_else_leaving_should_not() {
        let kicked =
            serde_json::json!({ "type": "m.room.member", "state_key": "@me:x", "content": { "membership": "leave" } });
        let other =
            serde_json::json!({ "type": "m.room.member", "state_key": "@ada:x", "content": { "membership": "ban" } });
        assert!(left("@me:x", &kicked) && !left("@me:x", &other));
    }

    #[test]
    fn being_kicked_should_count_as_leaving_as_the_event_arrives() {
        let kicked: Raw<AnySyncStateEvent> = serde_json::from_value(serde_json::json!({
            "type": "m.room.member", "state_key": "@me:x", "content": { "membership": "leave" }
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(raw_left("@me:x", &kicked));
    }

    #[test]
    fn a_plain_notification_should_not_ring() {
        let event = serde_json::json!({
            "type": "m.rtc.notification", "sender": "@ada:example.org", "origin_server_ts": 1,
            "content": { "notification_type": "notification", "lifetime": 30000 }
        });
        assert!(timeline_signals(&account(), "!r", &event, true).is_empty());
    }

    #[test]
    fn an_invite_should_carry_its_offer_and_lifetime() {
        let content = serde_json::json!({
            "call_id": "c1", "party_id": "p1", "version": "1", "lifetime": 60000,
            "offer": { "type": "offer", "sdp": "v=0" }
        });
        let event = direct_event("m.call.invite", &content);
        assert!(matches!(event, Some(DirectEvent::Invite { ref sdp, lifetime: 60_000, .. }) if sdp == "v=0"));
    }

    #[test]
    fn candidates_should_keep_each_ones_media_line() {
        let content = serde_json::json!({
            "call_id": "c1", "party_id": "p1", "version": "1",
            "candidates": [{ "candidate": "candidate:1", "sdpMid": "0", "sdpMLineIndex": 0 }]
        });
        let Some(DirectEvent::Candidates { candidates, .. }) = direct_event("m.call.candidates", &content) else {
            panic!("not candidates");
        };
        assert_eq!(candidates[0].sdp_m_line_index, 0);
    }
}
