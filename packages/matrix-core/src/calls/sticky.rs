//! Call memberships as Element Call's "matrix_2_0" mode writes them: sticky `m.rtc.member` events
//! (MSC4143 over MSC4354 sticky events) rather than state. Each member has a random member ID,
//! and its event stays current for at most an hour unless sent again. Every Element Call reads
//! both kinds of membership, so this app keeps writing state memberships and reads these too.
//!
//! The SDK keeps no sticky events, so each account keeps them ([`StickyMembers`]): those the
//! timeline brings as they are sent, and, once per room, those still sticky that a sync without a
//! token returns ([`StickyMembers::fetch`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use matrix_sdk::Room;
use serde_json::Value;

use super::Signal;
use super::rtc::{Member, Transport};
use crate::model::AccountId;

/// The sticky membership types: the unstable one Element Call sends, then the stable one.
pub(crate) const RTC_MEMBER_EVENTS: [&str; 2] = ["org.matrix.msc4143.rtc.member", "m.rtc.member"];
/// The room-wide call's slot: the only one this app joins, so the only one it counts.
const ROOM_CALL: &str = "m.call#ROOM";
/// The longest an event stays sticky (MSC4354).
const LONGEST: u64 = 60 * 60 * 1000;

/// One member's latest sticky event.
#[derive(Clone, Debug, PartialEq)]
struct Sticky {
    /// When it stops being sticky, by this device's clock.
    until: u64,
    /// When it stops being sticky by the sender's server's clock, which orders events (MSC4354):
    /// a snapshot that arrives late doesn't count as newer than what live sync brought.
    rank: u64,
    event_id: String,
    /// The member it describes; `None` once they left.
    member: Option<Member>,
}

/// What a sticky event is about: its type, sender and sticky key (MSC4354).
type Key = (String, String, String);

/// One account's sticky memberships, by room; and, by room, whether those from before this
/// session were fetched. Cheap to clone; it lives as long as its account.
#[derive(Clone, Debug, Default)]
pub(crate) struct StickyMembers {
    rooms: Arc<Mutex<HashMap<String, HashMap<Key, Sticky>>>>,
    /// Per room, held while its fetch runs (so a call waits for one under way) and true once one
    /// succeeded.
    fetched: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<bool>>>>>,
    /// Where changes are reported, as the engine's `Signal::Members` for the account.
    report: Arc<OnceLock<(AccountId, tokio::sync::mpsc::UnboundedSender<Signal>)>>,
}

impl StickyMembers {
    /// Reports every change to the account's rooms' members as `Signal::Members`: when a
    /// membership comes, goes or is redacted, and again when it lapses.
    pub(crate) fn report_to(&self, account: AccountId, signals: tokio::sync::mpsc::UnboundedSender<Signal>) {
        let _ = self.report.set((account, signals));
    }

    /// Asks again for every room's memberships from before: after the sync reconnects, which may
    /// have missed some.
    pub(crate) fn fetch_again(&self) {
        if let Ok(mut fetched) = self.fetched.lock() {
            fetched.clear();
        }
    }

    /// Reports that `room_id`'s members changed, and, with `until`, that they change again then.
    fn changed(&self, room_id: &str, until: Option<u64>) {
        let Some((account, signals)) = self.report.get() else { return };
        let _ = signals.send(Signal::Members { account: account.clone(), room_id: room_id.to_owned() });
        let (Some(until), Ok(runtime)) = (until, tokio::runtime::Handle::try_current()) else { return };
        let (account, room_id, signals) = (account.clone(), room_id.to_owned(), signals.clone());
        let wait = std::time::Duration::from_millis(until.saturating_sub(crate::engine::now_ms()).saturating_add(1));
        runtime.spawn(async move {
            tokio::time::sleep(wait).await;
            let _ = signals.send(Signal::Members { account, room_id });
        });
    }

    /// Keeps a sticky membership `event` of `room_id`, heard at `now`. `trusted`: it came
    /// encrypted, or the room isn't (in an encrypted room, anyone who can send could fake one in
    /// the clear). Returns when it stops being sticky, when it changed the room's members.
    pub(crate) fn heard(&self, room_id: &str, event: &Value, trusted: bool, now: u64) -> Option<u64> {
        if !trusted {
            return None;
        }
        let (key, sticky) = parse(event, now)?;
        let mut rooms = self.rooms.lock().ok()?;
        let room = rooms.entry(room_id.to_owned()).or_default();
        // The current event is the one sticky longest, then the highest event ID (MSC4354).
        let newer = room.get(&key).is_none_or(|kept| (sticky.rank, &sticky.event_id) > (kept.rank, &kept.event_id));
        let until = sticky.until;
        if !newer {
            return None;
        }
        room.insert(key, sticky);
        drop(rooms);
        self.changed(room_id, Some(until));
        Some(until)
    }

    /// Forgets the memberships of someone no longer in the room (left, removed, banned), whatever
    /// they still say.
    pub(crate) fn left(&self, room_id: &str, user_id: &str) {
        let Ok(mut rooms) = self.rooms.lock() else { return };
        let Some(room) = rooms.get_mut(room_id) else { return };
        let before = room.len();
        room.retain(|(_, sender, _), _| sender != user_id);
        let gone = room.len() != before;
        drop(rooms);
        if gone {
            self.changed(room_id, None);
        }
    }

    /// Forgets a membership that was redacted. True when one was.
    pub(crate) fn redacted(&self, room_id: &str, event_id: &str) -> bool {
        let Ok(mut rooms) = self.rooms.lock() else { return false };
        let Some(room) = rooms.get_mut(room_id) else { return false };
        let before = room.len();
        room.retain(|_, sticky| sticky.event_id != event_id);
        let redacted = room.len() != before;
        drop(rooms);
        if redacted {
            self.changed(room_id, None);
        }
        redacted
    }

    /// The members of `room_id` at `now` from sticky events: joined, still sticky.
    pub(crate) fn members(&self, room_id: &str, now: u64) -> Vec<Member> {
        let Ok(mut rooms) = self.rooms.lock() else { return Vec::new() };
        let Some(room) = rooms.get_mut(room_id) else { return Vec::new() };
        room.retain(|_, sticky| sticky.until > now);
        room.values().filter_map(|sticky| sticky.member.clone()).collect()
    }

    /// Fetches the room's still-sticky memberships, once per room where the server has sticky
    /// events: a sync without a token, for this room alone, that marks no one present. It isn't
    /// processed by the SDK, and its token is never used, so nothing it carries (to-device
    /// messages) counts as delivered. A fetch under way is waited for, not repeated; one that
    /// failed, or found events this device can't decrypt yet, runs again next time. What it
    /// changes is reported as any change is. True once the room's memberships are fetched (or
    /// there is nothing to fetch), false when it should be tried again.
    pub(crate) async fn fetch(&self, room: &Room, http: &matrix_sdk::reqwest::Client) -> bool {
        let client = room.client();
        let sticky = client
            .unstable_features()
            .await
            .is_ok_and(|features| features.iter().any(|feature| feature.as_str() == "org.matrix.msc4354"));
        let Some(token) = client.access_token().filter(|_| sticky) else { return true };
        let room_id = room.room_id().to_string();
        let Some(gate) =
            self.fetched.lock().ok().map(|mut fetched| fetched.entry(room_id.clone()).or_default().clone())
        else {
            return true;
        };
        let mut done = gate.lock().await;
        if *done {
            return true;
        }
        let Some(body) = sync_room(&client, http, &room_id, &token).await else { return false };
        let joined = &body["rooms"]["join"][&room_id];
        let events: Vec<&Value> = ["msc4354_sticky", "sticky"]
            .iter()
            .filter_map(|section| joined[section]["events"].as_array())
            .chain(joined["timeline"]["events"].as_array())
            .flatten()
            .filter(|event| matches!(event["type"].as_str(), Some(kind) if kind == "m.room.encrypted" || RTC_MEMBER_EVENTS.contains(&kind)))
            .collect();
        let encrypted_room = room.encryption_state().is_encrypted();
        let now = crate::engine::now_ms();
        let mut undecrypted = false;
        for event in events {
            let (event, came_encrypted) = match event["type"].as_str() {
                Some("m.room.encrypted") => match decrypted(room, event).await {
                    Some(event) => (event, true),
                    None => {
                        undecrypted = true;
                        continue;
                    }
                },
                _ => (event.clone(), false),
            };
            // Someone who left (or was removed) since doesn't come back with it.
            if still_joined(room, &event).await {
                self.heard(&room_id, &event, !encrypted_room || came_encrypted, now);
            }
        }
        *done = !undecrypted;
        *done
    }
}

/// The room's slice of a sync without a token, as JSON.
async fn sync_room(
    client: &matrix_sdk::Client,
    http: &matrix_sdk::reqwest::Client,
    room_id: &str,
    token: &str,
) -> Option<Value> {
    let filter = serde_json::json!({
        "room": {
            "rooms": [room_id],
            "timeline": { "limit": 1 },
            "state": { "types": [] },
            "ephemeral": { "types": [] },
            "account_data": { "types": [] },
        },
        "presence": { "types": [] },
        "account_data": { "types": [] },
    });
    let mut url = client.homeserver();
    url.set_path("/_matrix/client/v3/sync");
    url.query_pairs_mut()
        .append_pair("timeout", "0")
        .append_pair("set_presence", "offline")
        .append_pair("filter", &filter.to_string());
    let response = http.get(url).bearer_auth(token).timeout(std::time::Duration::from_secs(20)).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    serde_json::from_slice(&response.bytes().await.ok()?).ok()
}

/// A sticky membership event's key and what it says, `None` when it isn't one: not sticky, not
/// the room-wide call's slot, no sticky key, or a member naming someone else.
fn parse(event: &Value, now: u64) -> Option<(Key, Sticky)> {
    let kind = event["type"].as_str()?;
    if !RTC_MEMBER_EVENTS.contains(&kind) {
        return None;
    }
    let sender = event["sender"].as_str()?;
    let event_id = event["event_id"].as_str()?.to_owned();
    let sent = event["origin_server_ts"].as_u64().unwrap_or_default();
    let content = &event["content"];
    let sticky_key = content["msc4354_sticky_key"].as_str().or_else(|| content["sticky_key"].as_str())?;
    if let (Some(unstable), Some(stable)) = (content["msc4354_sticky_key"].as_str(), content["sticky_key"].as_str())
        && unstable != stable
    {
        return None;
    }
    if content["slot_id"].as_str()? != ROOM_CALL {
        return None;
    }
    // How long it stays sticky: what the server says is left, else from when it was sent, as
    // long as it was sent sticky at all.
    let duration = event["msc4354_sticky"]["duration_ms"].as_u64().map(|duration| duration.min(LONGEST));
    let until = match (event["unsigned"]["msc4354_sticky_duration_ttl_ms"].as_u64(), duration) {
        (Some(left), _) => now.saturating_add(left.min(LONGEST)),
        (None, Some(duration)) => sent.saturating_add(duration),
        (None, None) => return None,
    };
    let rank = sent.saturating_add(duration.unwrap_or(LONGEST));
    let member = member(sender, sent, content);
    let key = (kind.to_owned(), sender.to_owned(), sticky_key.to_owned());
    Some((key, Sticky { until, rank, event_id, member }))
}

/// The member a joined membership describes; `None` for a leave.
fn member(sender: &str, sent: u64, content: &Value) -> Option<Member> {
    let member = &content["member"];
    let joined = content["application"]["type"] == "m.call" && member["membership"].as_str() != Some("leave");
    let id = member["id"].as_str()?;
    if !joined || member["user_id"].as_str().is_some_and(|user| user != sender) {
        return None;
    }
    let device_id = member["device_id"].as_str().or_else(|| member["claimed_device_id"].as_str())?;
    let published = content["transports"]["published"].as_array().or_else(|| content["rtc_transports"].as_array());
    let transport = published.into_iter().flatten().find_map(|focus| Transport::read(focus, sender));
    Some(Member {
        user_id: sender.to_owned(),
        device_id: device_id.to_owned(),
        member_id: id.to_owned(),
        since: sent,
        transport,
        multi_sfu: true,
    })
}

/// Whether an event's sender is in the room now (asked for when lazy loading left them out).
async fn still_joined(room: &Room, event: &Value) -> bool {
    use matrix_sdk::ruma::events::room::member::MembershipState;
    let Some(user) = event["sender"].as_str().and_then(|sender| matrix_sdk::ruma::UserId::parse(sender).ok()) else {
        return false;
    };
    let known = match room.get_member_no_sync(&user).await.ok().flatten() {
        Some(known) => Some(known),
        None => room.get_member(&user).await.ok().flatten(),
    };
    known.is_some_and(|member| *member.membership() == MembershipState::Join)
}

/// An encrypted event, decrypted with what this device holds, with its outer fields.
async fn decrypted(room: &Room, event: &Value) -> Option<Value> {
    let raw = matrix_sdk::ruma::serde::Raw::<matrix_sdk::ruma::events::AnySyncTimelineEvent>::from_json(
        serde_json::value::to_raw_value(event).ok()?,
    );
    // Without its key the SDK answers with the event still unreadable, not an error.
    let decrypted =
        room.decrypt_event(&raw.cast_unchecked(), None).await.ok().filter(|event| event.encryption_info().is_some())?;
    let mut inner: Value = serde_json::from_str(decrypted.raw().json().get()).ok()?;
    // Keys the decrypted payload lacks come from the envelope (sender, timestamps, sticky).
    for key in ["sender", "event_id", "origin_server_ts", "msc4354_sticky", "unsigned"] {
        if inner.get(key).is_none()
            && let Some(value) = event.get(key)
        {
            inner[key] = value.clone();
        }
    }
    Some(inner)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn joined(event_id: &str, sent: u64, member_id: &str) -> Value {
        json!({
            "type": "org.matrix.msc4143.rtc.member",
            "sender": "@ada:x",
            "event_id": event_id,
            "origin_server_ts": sent,
            "msc4354_sticky": { "duration_ms": 3_600_000 },
            "content": {
                "slot_id": "m.call#ROOM",
                "application": { "type": "m.call", "m.call.intent": "video" },
                "transports": { "published": [
                    { "type": "livekit", "livekit_service_url": "https://jwt.x", "livekit_alias": "!r:x" }
                ], "can_subscribe": ["livekit"] },
                "member": { "id": member_id, "device_id": "DEV", "user_id": "@ada:x", "membership": "join" },
                "msc4354_sticky_key": member_id,
            }
        })
    }

    #[test]
    fn a_sticky_membership_should_read_as_a_member_with_its_own_id() {
        let sticky = StickyMembers::default();
        assert!(sticky.heard("!a", &joined("$1", 1_000, "uuid-1"), true, 2_000).is_some());
        let members = sticky.members("!a", 2_000);
        assert_eq!(members.len(), 1);
        assert_eq!((members[0].member_id.as_str(), members[0].device_id.as_str()), ("uuid-1", "DEV"));
        assert_eq!(members[0].transport, Some(Transport::Service("https://jwt.x".into())));
    }

    #[test]
    fn a_membership_should_end_when_it_stops_being_sticky() {
        let sticky = StickyMembers::default();
        assert_eq!(sticky.heard("!b", &joined("$1", 1_000, "uuid-1"), true, 1_000), Some(1_000 + LONGEST));
        assert!(sticky.members("!b", 1_000 + LONGEST + 1).is_empty());
    }

    #[test]
    fn a_leave_should_replace_the_members_join() {
        let sticky = StickyMembers::default();
        sticky.heard("!c", &joined("$1", 1_000, "uuid-1"), true, 1_000);
        let leave = json!({
            "type": "org.matrix.msc4143.rtc.member", "sender": "@ada:x", "event_id": "$2", "origin_server_ts": 2_000,
            "msc4354_sticky": { "duration_ms": 3_600_000 },
            "content": { "slot_id": "m.call#ROOM", "member": { "id": "uuid-1", "membership": "leave" }, "msc4354_sticky_key": "uuid-1" }
        });
        assert!(sticky.heard("!c", &leave, true, 2_000).is_some());
        assert!(sticky.members("!c", 2_000).is_empty());
    }

    #[test]
    fn a_late_snapshot_of_an_older_join_should_not_undo_a_newer_leave() {
        let sticky = StickyMembers::default();
        let leave = json!({
            "type": "org.matrix.msc4143.rtc.member", "sender": "@ada:x", "event_id": "$2", "origin_server_ts": 2_000,
            "msc4354_sticky": { "duration_ms": 3_600_000 },
            "content": { "slot_id": "m.call#ROOM", "member": { "id": "uuid-1", "membership": "leave" }, "msc4354_sticky_key": "uuid-1" }
        });
        sticky.heard("!m", &leave, true, 2_000);
        let mut join = joined("$1", 1_000, "uuid-1");
        join["unsigned"] = json!({ "msc4354_sticky_duration_ttl_ms": 3_599_000 });
        assert_eq!(sticky.heard("!m", &join, true, 9_000), None, "the snapshot came later, but the join is older");
        assert!(sticky.members("!m", 9_000).is_empty());
    }

    #[test]
    fn an_older_event_should_not_replace_a_newer_one() {
        let sticky = StickyMembers::default();
        sticky.heard("!d", &joined("$2", 2_000, "uuid-1"), true, 2_000);
        assert_eq!(sticky.heard("!d", &joined("$1", 1_000, "uuid-1"), true, 2_000), None);
    }

    #[test]
    fn a_membership_naming_someone_else_should_not_count() {
        let sticky = StickyMembers::default();
        let mut event = joined("$1", 1_000, "uuid-1");
        event["content"]["member"]["user_id"] = json!("@mallory:x");
        sticky.heard("!e", &event, true, 1_000);
        assert!(sticky.members("!e", 1_000).is_empty());
    }

    #[test]
    fn a_membership_in_the_clear_in_an_encrypted_room_should_not_count() {
        let sticky = StickyMembers::default();
        assert_eq!(sticky.heard("!f", &joined("$1", 1_000, "uuid-1"), false, 1_000), None);
        assert!(sticky.members("!f", 1_000).is_empty());
    }

    #[test]
    fn an_event_that_was_never_sticky_should_not_count() {
        let sticky = StickyMembers::default();
        let mut event = joined("$1", 1_000, "uuid-1");
        event.as_object_mut().map(|event| event.remove("msc4354_sticky"));
        assert_eq!(sticky.heard("!g", &event, true, 1_000), None);
    }

    #[test]
    fn a_membership_of_another_slot_should_not_count() {
        let sticky = StickyMembers::default();
        let mut event = joined("$1", 1_000, "uuid-1");
        event["content"]["slot_id"] = json!("m.call#other");
        assert_eq!(sticky.heard("!h", &event, true, 1_000), None);
    }

    #[tokio::test]
    async fn a_membership_should_be_reported_when_it_comes_and_when_it_lapses() {
        let sticky = StickyMembers::default();
        let (signals, mut heard) = tokio::sync::mpsc::unbounded_channel();
        sticky.report_to(AccountId::new("@me:x"), signals);
        let now = crate::engine::now_ms();
        let mut event = joined("$1", now, "uuid-1");
        event["msc4354_sticky"]["duration_ms"] = json!(50);
        sticky.heard("!l", &event, true, now);
        for _ in 0..2 {
            let signal = tokio::time::timeout(std::time::Duration::from_secs(5), heard.recv()).await;
            assert!(matches!(signal, Ok(Some(Signal::Members { ref room_id, .. })) if room_id == "!l"), "{signal:?}");
        }
        assert!(sticky.members("!l", crate::engine::now_ms()).is_empty());
    }

    #[test]
    fn someone_leaving_the_room_should_take_their_memberships_with_them() {
        let sticky = StickyMembers::default();
        sticky.heard("!n", &joined("$1", 1_000, "uuid-1"), true, 1_000);
        sticky.left("!n", "@ada:x");
        assert!(sticky.members("!n", 1_000).is_empty());
    }

    #[test]
    fn a_redacted_membership_should_be_forgotten() {
        let sticky = StickyMembers::default();
        sticky.heard("!i", &joined("$1", 1_000, "uuid-1"), true, 1_000);
        assert!(sticky.redacted("!i", "$1"));
        assert!(sticky.members("!i", 1_000).is_empty());
    }

    #[test]
    fn a_member_on_a_homeserver_vouched_sfu_should_name_it_and_its_server() {
        let sticky = StickyMembers::default();
        let mut event = joined("$1", 1_000, "uuid-1");
        event["content"]["transports"]["published"] =
            json!([{ "type": "livekit", "url": "wss://sfu.y", "server_name": "y" }]);
        sticky.heard("!j", &event, true, 1_000);
        assert_eq!(
            sticky.members("!j", 1_000)[0].transport,
            Some(Transport::Sfu { url: "wss://sfu.y".into(), server_name: "y".into() })
        );
    }

    #[test]
    fn a_vouched_sfu_without_a_server_should_be_asked_at_the_publishers() {
        let sticky = StickyMembers::default();
        let mut event = joined("$1", 1_000, "uuid-1");
        event["content"]["transports"]["published"] = json!([{ "type": "livekit", "url": "wss://sfu.x" }]);
        sticky.heard("!k", &event, true, 1_000);
        assert_eq!(
            sticky.members("!k", 1_000)[0].transport,
            Some(Transport::Sfu { url: "wss://sfu.x".into(), server_name: "x".into() })
        );
    }
}
