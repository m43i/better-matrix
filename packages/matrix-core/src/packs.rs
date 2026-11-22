//! Custom emoji and stickers: MSC2545 image packs, as Element, Cinny, FluffyChat and others
//! keep them. A pack is a set of images with short codes, each usable as a custom emoji in
//! text, as a sticker, or both.
//!
//! - The user's own pack: global account data `m.image_pack`.
//! - A room's packs: its `m.room.image_pack` state, one pack per state key.
//! - Packs the user enabled everywhere: `m.image_pack.rooms` names rooms and state keys whose
//!   packs apply in every room.
//!
//! Each is read under its name in the spec, then the `im.ponies.*` one clients used before it
//! (`user_emotes`, `room_emotes`, `emote_rooms`); a room's pack under both names shows once.
//!
//! Packs are read from this device's store (sync keeps them), the user's own falling back to
//! the server when the store has none yet. A room's packs, under sliding sync, which asks for
//! a fixed set of state that leaves packs out, also come from the room's state on the server,
//! read once a run per room; what sync brought since wins for the same pack.

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState;
use matrix_sdk::ruma::events::{GlobalAccountDataEventType, StateEventType};
use serde_json::Value;

use crate::model::{ImagePack, PackImage};
use crate::{Account, Error};

/// The user's own pack: the spec's name, then the one before it.
const USER_PACK: [&str; 2] = ["m.image_pack", "im.ponies.user_emotes"];
/// A room's packs.
const ROOM_PACK: [&str; 2] = ["m.room.image_pack", "im.ponies.room_emotes"];
/// Rooms whose packs apply everywhere.
const ENABLED_ROOMS: [&str; 2] = ["m.image_pack.rooms", "im.ponies.emote_rooms"];

impl Account {
    /// The image packs usable in `room_id` (or anywhere, for `None`): the user's own first,
    /// then the room's, then those the user enabled everywhere, each once.
    pub async fn image_packs(&self, room_id: Option<&str>) -> Result<Vec<ImagePack>, Error> {
        let account = self.client().account();
        let mut packs = Vec::new();
        let mut user = None;
        for name in USER_PACK {
            user = match account.account_data_raw(GlobalAccountDataEventType::from(name)).await? {
                Some(raw) => Some(raw),
                None => account.fetch_account_data(GlobalAccountDataEventType::from(name)).await.ok().flatten(),
            };
            if user.is_some() {
                break;
            }
        }
        if let Some(raw) = user
            && let Ok(content) = serde_json::from_str::<Value>(raw.json().get())
            && let Some(pack) = pack("user".to_owned(), String::new(), &content)
        {
            packs.push(pack);
        }
        if let Some(room) = room_id.and_then(|id| matrix_sdk::ruma::RoomId::parse(id).ok())
            && let Some(room) = self.client().get_room(&room)
        {
            packs.extend(room_packs(&room, None, &self.server_pack_state(&room).await).await);
        }
        for name in ENABLED_ROOMS {
            let Some(raw) = account.account_data_raw(GlobalAccountDataEventType::from(name)).await.ok().flatten()
            else {
                continue;
            };
            let Ok(content) = serde_json::from_str::<Value>(raw.json().get()) else { continue };
            let Some(rooms) = content["rooms"].as_object() else { continue };
            for (room_id, keys) in rooms {
                let Some(room) =
                    matrix_sdk::ruma::RoomId::parse(room_id).ok().and_then(|id| self.client().get_room(&id))
                else {
                    continue;
                };
                let keys: Vec<String> = keys.as_object().map(|keys| keys.keys().cloned().collect()).unwrap_or_default();
                let server = self.server_pack_state(&room).await;
                for found in room_packs(&room, Some(&keys), &server).await {
                    if !packs.iter().any(|known| known.id == found.id) {
                        packs.push(found);
                    }
                }
            }
        }
        Ok(packs)
    }
}

impl Account {
    /// A room's pack state events as the server holds them, under sliding sync (read once a run
    /// per room); none otherwise, or when the server can't be asked now (asked again next time).
    async fn server_pack_state(&self, room: &Room) -> Vec<Value> {
        if !self.sliding() {
            return Vec::new();
        }
        let room_id = room.room_id().to_string();
        if let Some(known) = self.pack_state.lock().ok().and_then(|known| known.get(&room_id).cloned()) {
            return known;
        }
        let request =
            matrix_sdk::ruma::api::client::state::get_state_events::v3::Request::new(room.room_id().to_owned());
        let Ok(response) = self.client().send(request).await else { return Vec::new() };
        let events: Vec<Value> = response
            .room_state
            .iter()
            .filter_map(|raw| raw.deserialize_as_unchecked::<Value>().ok())
            .filter(|event| event["type"].as_str().is_some_and(|kind| ROOM_PACK.contains(&kind)))
            .collect();
        if let Ok(mut known) = self.pack_state.lock() {
            known.insert(room_id, events.clone());
        }
        events
    }
}

/// A room's packs: all of them, or those under `keys`; a pack under both names once. `server`
/// is pack state read from the server, for packs the store doesn't hold.
async fn room_packs(room: &Room, keys: Option<&[String]>, server: &[Value]) -> Vec<ImagePack> {
    let mut packs: Vec<ImagePack> = Vec::new();
    for name in ROOM_PACK {
        for found in room_packs_named(room, name, keys, server).await {
            if !packs.iter().any(|known| known.id == found.id) {
                packs.push(found);
            }
        }
    }
    packs
}

/// A room's packs under one state event type.
async fn room_packs_named(room: &Room, name: &str, keys: Option<&[String]>, server: &[Value]) -> Vec<ImagePack> {
    let events = room.get_state_events(StateEventType::from(name)).await.unwrap_or_default();
    let room_name =
        room.cached_display_name().map(|name| name.to_string()).unwrap_or_else(|| room.room_id().to_string());
    let local: Vec<Value> = events
        .iter()
        .filter_map(|event| match event {
            RawAnySyncOrStrippedState::Sync(raw) => raw.deserialize_as_unchecked().ok(),
            RawAnySyncOrStrippedState::Stripped(raw) => raw.deserialize_as_unchecked().ok(),
        })
        .collect();
    // Of the store's and the server's copies of a pack, the newer counts: the store's may be
    // older (kept from before sliding sync, which brings no pack changes) or newer (a change
    // in a timeline since the server's state was read).
    let at = |event: &Value| event["origin_server_ts"].as_u64().unwrap_or_default();
    let ours = server.iter().filter(|event| event["type"].as_str() == Some(name));
    let mut newest: Vec<Value> = Vec::new();
    for event in local.iter().chain(ours) {
        match newest.iter_mut().find(|known| known["state_key"] == event["state_key"]) {
            Some(known) if at(event) > at(known) => *known = event.clone(),
            Some(_) => {}
            None => newest.push(event.clone()),
        }
    }
    let mut packs = Vec::new();
    for json in newest {
        let state_key = json["state_key"].as_str().unwrap_or_default().to_owned();
        if keys.is_some_and(|keys| !keys.contains(&state_key)) {
            continue;
        }
        if let Some(found) = pack(format!("{}/{state_key}", room.room_id()), room_name.clone(), &json["content"]) {
            packs.push(found);
        }
    }
    packs
}

/// A pack from its event content, named `fallback_name` when it names itself nothing; `None`
/// when it has no usable image.
pub(crate) fn pack(id: String, fallback_name: String, content: &Value) -> Option<ImagePack> {
    let usage = |value: &Value| -> Option<(bool, bool)> {
        let list = value.as_array().filter(|list| !list.is_empty())?;
        let has = |kind: &str| list.iter().any(|usage| usage.as_str() == Some(kind));
        Some((has("emoticon"), has("sticker")))
    };
    // A pack that says nothing is both.
    let pack_usage = usage(&content["pack"]["usage"]).unwrap_or((true, true));
    let images: Vec<PackImage> = content["images"]
        .as_object()?
        .iter()
        .filter_map(|(shortcode, image)| {
            let url = image["url"].as_str()?;
            let mxc = matrix_sdk::ruma::OwnedMxcUri::from(url);
            if !mxc.is_valid() || shortcode.is_empty() || shortcode.contains(char::is_whitespace) {
                return None;
            }
            let (emoticon, sticker) = usage(&image["usage"]).unwrap_or(pack_usage);
            let info = &image["info"];
            let dimension = |key: &str| info[key].as_u64().and_then(|value| u32::try_from(value).ok());
            Some(PackImage {
                shortcode: shortcode.clone(),
                body: image["body"].as_str().filter(|body| !body.trim().is_empty()).unwrap_or(shortcode).to_owned(),
                url: url.to_owned(),
                media: crate::convert::avatar(mxc)?,
                emoticon,
                sticker,
                dimensions: dimension("w").zip(dimension("h")),
                mimetype: info["mimetype"].as_str().map(str::to_owned),
            })
        })
        .collect();
    if images.is_empty() {
        return None;
    }
    let name = content["pack"]["display_name"].as_str().filter(|name| !name.trim().is_empty());
    let avatar = content["pack"]["avatar_url"]
        .as_str()
        .map(matrix_sdk::ruma::OwnedMxcUri::from)
        .filter(|mxc| mxc.is_valid())
        .and_then(crate::convert::avatar);
    Some(ImagePack { id, name: name.map_or(fallback_name, str::to_owned), avatar, images })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_pack_should_keep_each_image_with_its_short_code_and_size() {
        let content = json!({
            "pack": { "display_name": "Cats" },
            "images": { "blobcat": { "url": "mxc://x.org/cat", "info": { "w": 64, "h": 64, "mimetype": "image/png" } } }
        });
        let found = pack("user".into(), String::new(), &content).expect("a pack");
        assert_eq!(found.name, "Cats");
        let image = &found.images[0];
        assert_eq!(
            (image.shortcode.as_str(), image.url.as_str(), image.dimensions),
            ("blobcat", "mxc://x.org/cat", Some((64, 64)))
        );
    }

    #[test]
    fn an_image_should_follow_the_packs_usage_unless_it_has_its_own() {
        let content = json!({
            "pack": { "usage": ["sticker"] },
            "images": {
                "wave": { "url": "mxc://x.org/wave" },
                "blob": { "url": "mxc://x.org/blob", "usage": ["emoticon"] }
            }
        });
        let found = pack("room".into(), "Room".into(), &content).expect("a pack");
        let usage: Vec<_> =
            found.images.iter().map(|image| (image.shortcode.as_str(), image.emoticon, image.sticker)).collect();
        assert_eq!(usage, [("blob", true, false), ("wave", false, true)]);
    }

    #[test]
    fn a_pack_saying_nothing_of_its_usage_should_be_both() {
        let content = json!({ "images": { "wave": { "url": "mxc://x.org/wave" } } });
        let image = &pack("room".into(), "Room".into(), &content).expect("a pack").images[0];
        assert!(image.emoticon && image.sticker);
    }

    #[test]
    fn a_pack_without_a_name_should_take_its_rooms() {
        let content = json!({ "images": { "wave": { "url": "mxc://x.org/wave" } } });
        assert_eq!(pack("room".into(), "Design".into(), &content).expect("a pack").name, "Design");
    }

    #[test]
    fn images_without_a_valid_address_should_be_left_out() {
        let content =
            json!({ "images": { "bad": { "url": "https://x.org/a.png" }, "good": { "url": "mxc://x.org/a" } } });
        let found = pack("room".into(), "Room".into(), &content).expect("a pack");
        assert_eq!(found.images.len(), 1);
    }

    #[tokio::test]
    async fn a_rooms_packs_should_be_read_under_both_names_once_each() {
        let pack_event = |kind: &str, key: &str, shortcode: &str| {
            json!({
                "type": kind, "state_key": key, "sender": "@a:example.org", "event_id": format!("${kind}{key}"),
                "origin_server_ts": 1,
                "content": { "pack": { "display_name": key }, "images": { shortcode: { "url": "mxc://x.org/i" } } }
            })
        };
        let server = crate::test_server::Server::new(move |request| {
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                let state =
                    response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().expect("state");
                state.push(pack_event("m.room.image_pack", "cats", "blobcat"));
                state.push(pack_event("im.ponies.room_emotes", "cats", "oldcat"));
                state.push(pack_event("im.ponies.room_emotes", "dogs", "dog"));
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let packs = account.image_packs(Some("!room:example.org")).await.expect("packs");
        let found: Vec<_> = packs.iter().map(|pack| (pack.name.as_str(), pack.images[0].shortcode.as_str())).collect();
        assert_eq!(found, [("cats", "blobcat"), ("dogs", "dog")]);
    }

    #[tokio::test]
    async fn of_a_packs_two_copies_the_newer_should_count() {
        let pack_event = |key: &str, shortcode: &str| {
            json!({
                "type": "m.room.image_pack", "state_key": key, "sender": "@a:example.org",
                "event_id": format!("${key}{shortcode}"), "origin_server_ts": 1,
                "content": { "pack": { "display_name": key }, "images": { shortcode: { "url": "mxc://x.org/i" } } }
            })
        };
        let synced = pack_event("cats", "blobcat");
        let server = crate::test_server::Server::new(move |request| {
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                let state =
                    response["rooms"]["join"]["!room:example.org"]["state"]["events"].as_array_mut().expect("state");
                state.push(synced.clone());
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let room = account.client().get_room(matrix_sdk::ruma::room_id!("!room:example.org")).expect("room");
        // The server's copy of `cats` is older than the one sync brought; `birds` sync never did.
        let from_server = [pack_event("cats", "stalecat"), pack_event("birds", "bird")];
        let packs = room_packs(&room, None, &from_server).await;
        let found: Vec<_> = packs.iter().map(|pack| (pack.name.as_str(), pack.images[0].shortcode.as_str())).collect();
        assert_eq!(found, [("cats", "blobcat"), ("birds", "bird")]);
        // A newer copy on the server (changed since the store's) counts instead.
        let mut changed = pack_event("cats", "newcat");
        changed["origin_server_ts"] = json!(2);
        let packs = room_packs(&room, None, &[changed]).await;
        assert_eq!(packs[0].images[0].shortcode, "newcat");
    }

    #[test]
    fn a_pack_without_images_should_be_none() {
        assert!(pack("room".into(), "Room".into(), &json!({ "images": {} })).is_none());
    }
}
