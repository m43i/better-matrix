//! A room beacon belongs to one share, even if another device starts a newer one.
use crate::{Answer, Error};
use matrix_sdk::{
    Room,
    ruma::events::{beacon::BeaconEventContent, beacon_info::BeaconInfoEventContent},
};
use std::time::Duration;
use tokio::sync::mpsc;

const DURATION: Duration = Duration::from_secs(15 * 60);

/// Read the original, decrypted reference events. The SDK timeline aggregate drops their sender.
pub(crate) async fn latest_fix(room: &Room, beacon: &str, owner: &str) -> Option<String> {
    use matrix_sdk::ruma::events::{
        AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent, relation::RelationType,
    };
    let beacon = matrix_sdk::ruma::EventId::parse(beacon).ok()?;
    let (cache, _guard) = room.event_cache().await.ok()?;
    let references = cache.find_event_relations(&beacon, Some(vec![RelationType::Reference])).await.ok()?;
    references
        .into_iter()
        .filter_map(|event| {
            let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::Beacon(beacon_event)) =
                event.raw().deserialize().ok()?
            else {
                return None;
            };
            let SyncMessageLikeEvent::Original(fix) = beacon_event else { return None };
            if fix.sender.as_str() != owner || fix.content.relates_to.event_id != beacon {
                return None;
            }
            let uri = fix.content.location.uri;
            super::coordinates(&uri)?;
            Some((fix.content.ts, uri))
        })
        .max_by_key(|(ts, _)| *ts)
        .map(|(_, uri)| uri)
}

pub(crate) fn permitted(
    power: &matrix_sdk::ruma::events::room::power_levels::RoomPowerLevels,
    user: &matrix_sdk::ruma::UserId,
    encrypted: bool,
) -> bool {
    use matrix_sdk::ruma::events::{MessageLikeEventType, StateEventType};
    power.user_can_send_state(user, StateEventType::BeaconInfo)
        && power.user_can_send_message(user, MessageLikeEventType::Beacon)
        && (!encrypted || power.user_can_send_message(user, MessageLikeEventType::RoomEncrypted))
}

pub(crate) async fn share(
    room: Room,
    first: String,
    mut answers: mpsc::UnboundedReceiver<Answer>,
    mut stopped: tokio::sync::watch::Receiver<()>,
    started: impl FnOnce(String),
) -> Result<(), Error> {
    super::coordinates(&first).ok_or(Error::MessageUnavailable)?;
    if !permitted(
        &room.power_levels().await.map_err(matrix_sdk::Error::from)?,
        room.own_user_id(),
        room.encryption_state().is_encrypted(),
    ) {
        return Err(Error::Location("Insufficient permission to share live location".into()));
    }
    let mut info = BeaconInfoEventContent::new(None, DURATION, true, None);
    let until = tokio::time::Instant::now() + DURATION;
    let before = beacon_id(&room).await?;
    if stopped.has_changed().unwrap_or(true) {
        return Ok(());
    }
    let id = room.send_state_event_for_key(room.own_user_id(), info.clone()).await?.event_id;
    let publish = |uri: String| room.send(BeaconEventContent::new(id.clone(), uri, None));
    let run = async {
        publish(first).await?;
        started(id.to_string());
        loop {
            let next = tokio::select! {
                biased;
                answer = answers.recv() => match answer {
                    Some(Answer::LocationFix(uri)) => uri,
                    None => break,
                    _ => continue,
                },
                result = next_desktop_fix() => result?,
            };
            super::coordinates(&next).ok_or(Error::MessageUnavailable)?;
            // Sync may already show an early stop or a replacement from another device.
            if !current(&room, &id, before.as_deref()).await? {
                return Ok(());
            }
            publish(next).await?;
        }
        Ok::<_, Error>(())
    };
    let result = tokio::select! {
        biased;
        _ = stopped.changed() => Ok(()),
        _ = tokio::time::sleep_until(until) => Ok(()),
        result = run => result,
    };
    if info.is_live() && owns_server_state(&room, &id, &info).await? {
        info.stop();
        room.send_state_event_for_key(room.own_user_id(), info).await?;
    }
    result
}

/// State writes share a key across devices. Check the server before stopping, since sync may
/// still show our start after another device took over. Older servers return just the content.
async fn owns_server_state(
    room: &Room,
    id: &matrix_sdk::ruma::EventId,
    info: &BeaconInfoEventContent,
) -> Result<bool, Error> {
    use matrix_sdk::ruma::{api::client::state::get_state_event_for_key::v3, events::StateEventType};
    let mut request =
        v3::Request::new(room.room_id().to_owned(), StateEventType::BeaconInfo, room.own_user_id().to_string());
    request.format = v3::StateEventFormat::Event;
    let reply = room.client().send(request).await?;
    let raw = reply.into_event();
    if let Some(current) = raw.get_field::<String>("event_id").ok().flatten() {
        return Ok(current == id.as_str());
    }
    Ok(raw
        .deserialize_as_unchecked::<BeaconInfoEventContent>()
        .is_ok_and(|current| current.live && current.ts == info.ts))
}

// The cache can still contain the preceding state event until sync echoes our new one.
async fn current(
    room: &Room,
    id: &matrix_sdk::ruma::EventId,
    before: Option<&matrix_sdk::ruma::EventId>,
) -> Result<bool, Error> {
    let cached = beacon_id(room).await?;
    Ok(cached.as_deref() == Some(id) || cached.as_deref() == before)
}

async fn beacon_id(room: &Room) -> Result<Option<matrix_sdk::ruma::OwnedEventId>, Error> {
    let state = room.get_state_event_static_for_key::<BeaconInfoEventContent, _>(room.own_user_id()).await?;
    Ok(state.and_then(|event| event.deserialize().ok()?.event_id().map(ToOwned::to_owned)))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
async fn next_desktop_fix() -> Result<String, Error> {
    tokio::time::sleep(Duration::from_secs(15)).await;
    super::desktop::position(false).await
}

#[cfg(any(target_os = "android", target_os = "ios"))]
async fn next_desktop_fix() -> Result<String, Error> {
    std::future::pending().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use matrix_sdk::config::SyncSettings;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn an_element_beacon_should_show_its_latest_fix_on_the_original_timeline_card() {
        let now = matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now();
        let server = crate::test_server::Server::new(move |request| {
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                let events =
                    response["rooms"]["join"]["!room:example.org"]["timeline"]["events"].as_array_mut().unwrap();
                events.push(json!({
                    "type":"org.matrix.msc3672.beacon_info","state_key":"@b:example.org",
                    "event_id":"$session","sender":"@b:example.org","origin_server_ts":now,
                    "content":{"live":true,"timeout":900000,"org.matrix.msc3488.ts":now,"org.matrix.msc3488.asset":{"type":"m.self"}}
                }));
                for (id, uri, ts) in [("$first", "geo:1,2", 10), ("$next", "geo:3,4", 11)] {
                    events.push(json!({
                        "type":"org.matrix.msc3672.beacon","event_id":id,"sender":"@b:example.org","origin_server_ts":now,
                        "content":{"m.relates_to":{"rel_type":"m.reference","event_id":"$session"},
                        "org.matrix.msc3488.ts":ts,"org.matrix.msc3488.location":{"uri":uri}}
                    }));
                }
            }
            response
        });
        let account = crate::Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.unwrap();
        let timeline = account.timeline(&crate::TimelineKey::room("!room:example.org")).await.unwrap();
        let (items, _) = timeline.subscribe().await;
        let me = account.user_id().unwrap();
        let crate::TimelineDiff::Reset(entries) =
            crate::timeline::diff(eyeball_im::VectorDiff::Reset { values: items }, &me, timeline.room()).await
        else {
            panic!("timeline reset")
        };
        let card = entries
            .into_iter()
            .find_map(|entry| match entry.kind {
                crate::EntryKind::Message(msg) if msg.event_id == "$session" => Some(msg),
                _ => None,
            })
            .unwrap();
        let crate::MessageContent::Location(place) = card.content else { panic!("location card") };
        assert_eq!(place.geo_uri, "geo:3,4");
        assert_eq!(latest_fix(timeline.room(), "$session", "@other:example.org").await, None);
        assert_eq!(place.live_until, Some(u64::from(now.0) + 900000));
    }

    #[tokio::test]
    async fn a_share_should_publish_fixes_related_to_its_own_beacon_and_stop_early() {
        let (sent, mut requests) = mpsc::unbounded_channel();
        let server = crate::test_server::Server::new(move |request| {
            if request.starts_with("GET ") && request.contains("beacon_info") {
                return json!({"event_id":"$session"});
            }
            if request.starts_with("GET ") && request.contains("/state/m.room.encryption") {
                return json!({"errcode":"M_NOT_FOUND","error":"Room is unencrypted"});
            }
            if request.starts_with("PUT ") && request.contains("org.matrix.msc3672.beacon") {
                let (headers, body) = request.split_once("\r\n\r\n").unwrap();
                let state = headers.lines().next().unwrap().contains("/state/");
                sent.send((state, serde_json::from_str::<Value>(body).unwrap())).unwrap();
                return json!({"event_id":if state {"$session"} else {"$fix"}});
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().unwrap().push(json!({
                    "type":"m.room.power_levels","event_id":"$permissions","state_key":"","sender":"@a:example.org",
                    "origin_server_ts":0,"content":{"events":{"org.matrix.msc3672.beacon_info":0}}
                }));
            }
            response
        });
        let account = crate::Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.unwrap();
        let room = account.joined_room("!room:example.org").unwrap();
        let (answers, rx) = mpsc::unbounded_channel();
        let (stop, stopped) = tokio::sync::watch::channel(());
        let task = tokio::spawn(share(room, "geo:1,2".into(), rx, stopped, |_| {}));
        let (state, start) = tokio::time::timeout(Duration::from_secs(10), requests.recv()).await.unwrap().unwrap();
        assert!(state && start["live"] == true);
        assert_eq!(start["timeout"], 900000);
        for geo in ["geo:1,2", "geo:3,4"] {
            if geo == "geo:3,4" {
                answers.send(Answer::LocationFix(geo.into())).unwrap();
            }
            let (state, fix) = tokio::time::timeout(Duration::from_secs(10), requests.recv()).await.unwrap().unwrap();
            if state {
                panic!("share ended before {geo}: {:?}", task.await);
            }
            assert_eq!(fix["m.relates_to"]["event_id"], "$session");
            assert_eq!(fix["org.matrix.msc3488.location"]["uri"], geo);
        }
        stop.send(()).unwrap();
        task.await.unwrap().unwrap();
        let (state, stop) = tokio::time::timeout(Duration::from_secs(10), requests.recv()).await.unwrap().unwrap();
        assert!(state && stop["live"] == false);
        assert_eq!(start["org.matrix.msc3488.ts"], stop["org.matrix.msc3488.ts"]);
    }
}
