//! The Matrix side of a MatrixRTC session, as Element Call runs it in its default
//! ("compatibility") mode: memberships are state events, and each member publishes on its own
//! homeserver's LiveKit service while subscribing to everyone else's (multi-SFU). Here: who is in
//! the room's call and where they publish, the tokens for those services, the user's own
//! membership (with a delayed leave so a crash still leaves the call), and the ring that starts
//! a call in a direct chat. Memberships are read from raw JSON, so one in a shape this code
//! doesn't know costs only that member, not the whole list.

use std::time::Duration;

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState;
use matrix_sdk::ruma::api::client::account::request_openid_token;
use matrix_sdk::ruma::api::client::delayed_events::{DelayParameters, delayed_state_event, update_delayed_event};
use matrix_sdk::ruma::events::StateEventType;
use matrix_sdk::ruma::events::call::member::CallMemberStateKey;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{MilliSecondsSinceUnixEpoch, OwnedUserId};
use serde::Deserialize;
use serde_json::Value;

use super::CallError;
use super::keys::default_member_id;

/// How long the server waits for a restart before it sends the user's delayed leave (Element
/// Call's default).
pub(crate) const LEAVE_DELAY: Duration = Duration::from_secs(18);
/// How often the delayed leave is put off while the call runs.
pub(crate) const RESTART_EVERY: Duration = Duration::from_secs(4);
/// How long putting off or sending the delayed leave may take: no retries, as the next restart
/// comes in seconds, and leaving shouldn't hold a hang-up up.
const QUICK: Duration = Duration::from_secs(5);

pub(crate) fn quick() -> matrix_sdk::config::RequestConfig {
    matrix_sdk::config::RequestConfig::new().disable_retry().timeout(QUICK)
}

/// How long a membership lasts unless renewed, in milliseconds.
const EXPIRES: u64 = 4 * 60 * 60 * 1000;
use super::signals::MEMBER_EVENTS;
use super::signals::RING_EVENT;

/// One device in the room's call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Member {
    pub user_id: String,
    pub device_id: String,
    /// Its member ID: the membership's own, else `user:device`.
    pub member_id: String,
    /// When the device joined, in milliseconds since the epoch.
    pub since: u64,
    /// The LiveKit service it prefers, where it publishes.
    pub service: Option<String>,
    /// Whether it publishes on its own service (multi-SFU) rather than the oldest member's.
    pub multi_sfu: bool,
}

/// The devices in the room's call at `now`: unexpired, non-empty memberships of the room-wide
/// `m.call`, from either membership event type.
pub(crate) async fn members(room: &Room) -> Vec<Member> {
    let now = MilliSecondsSinceUnixEpoch::now().get().into();
    let mut members = Vec::new();
    for kind in MEMBER_EVENTS {
        let Ok(events) = room.get_state_events(StateEventType::from(kind)).await else { continue };
        for raw in events {
            let RawAnySyncOrStrippedState::Sync(raw) = raw else { continue };
            let Ok(event) = raw.deserialize_as_unchecked::<Value>() else { continue };
            members.extend(parse_members(&event, now));
        }
    }
    members
}

/// The memberships one state event holds: one (session format), or a list (the oldest format).
fn parse_members(event: &Value, now: u64) -> Vec<Member> {
    let sender = event["sender"].as_str().unwrap_or_default();
    let sent = event["origin_server_ts"].as_u64().unwrap_or_default();
    let content = &event["content"];
    match content["memberships"].as_array() {
        Some(list) => list.iter().filter_map(|membership| parse_member(sender, sent, membership, now)).collect(),
        None => parse_member(sender, sent, content, now).into_iter().collect(),
    }
}

fn parse_member(sender: &str, sent: u64, content: &Value, now: u64) -> Option<Member> {
    let application = content["application"].as_str().or_else(|| content["application"]["type"].as_str())?;
    let call_id = content["call_id"].as_str().unwrap_or_default();
    let scope = content["scope"].as_str().unwrap_or("m.room");
    if application != "m.call" || !call_id.is_empty() || scope != "m.room" || sender.is_empty() {
        return None;
    }
    let device_id = content["device_id"].as_str()?.to_owned();
    let since = content["created_ts"].as_u64().unwrap_or(sent);
    // Another client's numbers: one that overflows is no membership.
    let until = since.checked_add(content["expires"].as_u64().unwrap_or(EXPIRES))?;
    if now >= until {
        return None;
    }
    let service = content["foci_preferred"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|focus| (focus["type"] == "livekit").then(|| focus["livekit_service_url"].as_str()).flatten())
        .map(str::to_owned);
    Some(Member {
        member_id: content["membershipID"]
            .as_str()
            .map_or_else(|| default_member_id(sender, &device_id), str::to_owned),
        user_id: sender.to_owned(),
        device_id,
        since,
        service,
        multi_sfu: content["focus_active"]["focus_selection"] == "multi_sfu",
    })
}

/// The LiveKit services to subscribe to: the user's own first (where they publish), then each
/// multi-SFU member's own, and the oldest member's for those who join wherever it is.
pub(crate) fn subscriptions(members: &[Member], own: &str) -> Vec<String> {
    let oldest = members.iter().filter(|m| m.service.is_some()).min_by_key(|m| m.since).and_then(|m| m.service.clone());
    let mut services = vec![own.to_owned()];
    for member in members {
        let service = if member.multi_sfu { member.service.clone() } else { oldest.clone() };
        if let Some(service) = service.filter(|service| !services.contains(service)) {
            services.push(service);
        }
    }
    services
}

/// The SFU to connect to and the token for it.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Grant {
    pub url: String,
    pub jwt: String,
}

/// Asks a call service for a grant to the room's call: an OpenID token proves who the user is.
/// The older endpoint first (`user:device` identities, which every Element Call reads), then the
/// newer one. On another homeserver's service the grant only subscribes.
pub(crate) async fn grant(
    http: &matrix_sdk::reqwest::Client,
    room: &Room,
    service: &str,
    user_id: OwnedUserId,
    device_id: &str,
) -> Result<Grant, CallError> {
    let openid = room
        .client()
        .send(request_openid_token::v3::Request::new(user_id.clone()))
        .await
        .map_err(|error| CallError::Service(error.to_string()))?;
    let token = serde_json::json!({
        "access_token": openid.access_token,
        "token_type": openid.token_type.to_string(),
        "matrix_server_name": openid.matrix_server_name.to_string(),
        "expires_in": openid.expires_in.as_secs(),
    });
    let base = service.trim_end_matches('/');
    let legacy = serde_json::json!({ "room": room.room_id(), "openid_token": token, "device_id": device_id });
    let newer = serde_json::json!({
        "room_id": room.room_id(),
        "slot_id": "m.call#room",
        "openid_token": token,
        "member": {
            "id": default_member_id(user_id.as_str(), device_id),
            "claimed_user_id": user_id,
            "claimed_device_id": device_id,
        },
    });
    let mut failure = String::new();
    for (path, body) in [("sfu/get", legacy), ("get_token", newer)] {
        let response = http
            .post(format!("{base}/{path}"))
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&body).unwrap_or_default())
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {
                let bytes = response.bytes().await.map_err(|error| CallError::Service(error.to_string()))?;
                return serde_json::from_slice::<Grant>(&bytes).map_err(|error| CallError::Service(error.to_string()));
            }
            Ok(response) => failure = format!("{path}: {}", response.status()),
            Err(error) => failure = format!("{path}: {error}"),
        }
        tracing::debug!(%service, "call: no grant from {failure}");
    }
    Err(CallError::Service(failure))
}

fn state_key(user_id: OwnedUserId, device_id: &str) -> String {
    CallMemberStateKey::new(user_id, Some(format!("{device_id}_m.call")), true).as_ref().to_owned()
}

/// The user's membership in the room's call, as Element Call writes it in compatibility mode:
/// publishing on `service` (their own homeserver's), subscribing to everyone's.
pub(crate) fn membership(
    user_id: &str,
    device_id: &str,
    room_id: &str,
    service: &str,
    video: bool,
    since: Option<u64>,
) -> Value {
    let mut content = serde_json::json!({
        "application": "m.call",
        "call_id": "",
        "scope": "m.room",
        "device_id": device_id,
        "membershipID": default_member_id(user_id, device_id),
        "expires": EXPIRES,
        "m.call.intent": if video { "video" } else { "audio" },
        "focus_active": { "type": "livekit", "focus_selection": "multi_sfu" },
        "foci_preferred": [{ "type": "livekit", "livekit_service_url": service, "livekit_alias": room_id }],
    });
    // A renewal keeps when the user joined, and lasts four hours from now, as Element's does.
    if let Some(since) = since {
        let now: u64 = MilliSecondsSinceUnixEpoch::now().get().into();
        content["created_ts"] = since.into();
        content["expires"] = now.saturating_sub(since).saturating_add(EXPIRES).into();
    }
    content
}

/// Joins the room's call: schedules the delayed leave first (when the server has delayed events),
/// then sends the membership. Returns the delayed leave's ID, to keep putting it off.
pub(crate) async fn join(
    room: &Room,
    user_id: OwnedUserId,
    device_id: &str,
    content: Value,
) -> Result<Option<String>, CallError> {
    let key = state_key(user_id, device_id);
    let delayed = match Raw::new(&serde_json::json!({})) {
        Ok(leave) => {
            let request = delayed_state_event::unstable::Request::new_raw(
                room.room_id().to_owned(),
                key.clone(),
                StateEventType::from(MEMBER_EVENTS[0]),
                DelayParameters::Timeout { timeout: LEAVE_DELAY },
                leave.cast_unchecked(),
            );
            room.client().send(request).await.ok().map(|response| response.delay_id)
        }
        Err(_) => None,
    };
    room.send_state_event_raw(MEMBER_EVENTS[0], &key, content)
        .await
        .map_err(|error| CallError::Service(error.to_string()))?;
    Ok(delayed)
}

/// Puts the delayed leave off again. False when the server refused: it may have sent it
/// already (a stall longer than the delay), so the membership needs sending again.
pub(crate) async fn restart(room: &Room, delay_id: &str) -> bool {
    let request = update_delayed_event::unstable_v1::Request::new(
        delay_id.to_owned(),
        update_delayed_event::UpdateAction::Restart,
    );
    room.client().send(request).with_request_config(quick()).await.is_ok()
}

/// Joins again: drops the old delayed leave (if any) and sends the membership with a new one.
/// After a stall that let the server send the leave, and every so often so it doesn't expire.
pub(crate) async fn rejoin(
    room: &Room,
    user_id: OwnedUserId,
    device_id: &str,
    content: Value,
    old_delay: Option<String>,
) -> Result<Option<String>, CallError> {
    if let Some(delay_id) = old_delay {
        let cancel =
            update_delayed_event::unstable_v1::Request::new(delay_id, update_delayed_event::UpdateAction::Cancel);
        let _ = room.client().send(cancel).await;
    }
    join(room, user_id, device_id, content).await
}

/// Leaves the room's call: the delayed leave now, else an empty membership. Each try is short
/// and unretried: offline, the delayed leave goes out on its own.
pub(crate) async fn leave(room: &Room, user_id: OwnedUserId, device_id: &str, delay_id: Option<&str>) {
    if let Some(delay_id) = delay_id {
        let request = update_delayed_event::unstable_v1::Request::new(
            delay_id.to_owned(),
            update_delayed_event::UpdateAction::Send,
        );
        if room.client().send(request).with_request_config(quick()).await.is_ok() {
            return;
        }
    }
    let key = state_key(user_id, device_id);
    let empty = room.send_state_event_raw(MEMBER_EVENTS[0], &key, serde_json::json!({}));
    let _ = tokio::time::timeout(QUICK, empty).await;
}

/// Rings the other members: Element's ring notification, for 30 seconds.
pub(crate) async fn ring(room: &Room, video: bool) {
    let content = serde_json::json!({
        "notification_type": "ring",
        "lifetime": 30_000,
        "sender_ts": MilliSecondsSinceUnixEpoch::now(),
        "m.mentions": { "room": true },
        "m.call.intent": if video { "video" } else { "audio" },
    });
    let _ = room.send_raw(RING_EVENT, content).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(since: u64, service: &str, multi_sfu: bool) -> Member {
        Member {
            user_id: "@a:x".into(),
            device_id: "D".into(),
            member_id: "@a:x:D".into(),
            since,
            service: Some(service.into()),
            multi_sfu,
        }
    }

    #[test]
    fn the_user_should_subscribe_to_their_own_service_and_each_multi_sfu_members() {
        let members = [member(10, "https://a", true), member(20, "https://b", true), member(30, "https://a", true)];
        assert_eq!(subscriptions(&members, "https://own"), ["https://own", "https://a", "https://b"]);
    }

    #[test]
    fn a_member_joining_the_oldest_service_should_be_found_there() {
        let members = [member(10, "https://oldest", true), member(20, "https://theirs", false)];
        assert_eq!(subscriptions(&members, "https://own"), ["https://own", "https://oldest"]);
    }

    #[test]
    fn element_calls_membership_should_read_with_its_member_id_and_service() {
        let event = serde_json::json!({
            "sender": "@ada:x", "origin_server_ts": 1000,
            "content": {
                "application": "m.call", "call_id": "", "scope": "m.room", "device_id": "DEV",
                "membershipID": "@ada:x:DEV", "expires": 14400000,
                "focus_active": { "type": "livekit", "focus_selection": "multi_sfu" },
                "foci_preferred": [{ "type": "livekit", "livekit_service_url": "https://sfu", "livekit_alias": "!r" }]
            }
        });
        let found = parse_members(&event, 2000);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].service.as_deref(), found[0].multi_sfu), (Some("https://sfu"), true));
    }

    #[test]
    fn a_renewed_membership_should_keep_when_the_user_joined() {
        let event = serde_json::json!({
            "sender": "@me:x", "origin_server_ts": 5000,
            "content": membership("@me:x", "DEV", "!r", "https://own", false, Some(1000)),
        });
        assert_eq!(parse_members(&event, 6000)[0].since, 1000);
    }

    #[test]
    fn a_membership_with_numbers_that_overflow_should_not_count() {
        let event = serde_json::json!({
            "sender": "@ada:x", "origin_server_ts": 1,
            "content": { "application": "m.call", "call_id": "", "device_id": "D", "expires": u64::MAX }
        });
        assert!(parse_members(&event, 2).is_empty());
    }

    #[test]
    fn an_expired_or_empty_membership_should_not_count() {
        let expired = serde_json::json!({
            "sender": "@ada:x", "origin_server_ts": 0,
            "content": { "application": "m.call", "call_id": "", "device_id": "D", "expires": 10 }
        });
        let empty = serde_json::json!({ "sender": "@ada:x", "origin_server_ts": 0, "content": {} });
        assert!(parse_members(&expired, 100).is_empty() && parse_members(&empty, 100).is_empty());
    }

    #[test]
    fn the_users_membership_should_publish_on_their_own_service() {
        let content = membership("@me:x", "DEV", "!r", "https://own", true, None);
        assert_eq!(content["foci_preferred"][0]["livekit_service_url"], "https://own");
        assert_eq!(
            (content["membershipID"].as_str(), content["m.call.intent"].as_str()),
            (Some("@me:x:DEV"), Some("video"))
        );
    }
}
