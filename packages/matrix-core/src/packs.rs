//! Custom emoji and stickers: MSC2545 image packs, as Element, Cinny, FluffyChat and others
//! keep them. A pack is a set of images with short codes, each usable as a custom emoji in
//! text, as a sticker, or both.
//!
//! - The user's own pack: global account data `im.ponies.user_emotes`.
//! - A room's packs: its `im.ponies.room_emotes` state, one pack per state key.
//! - Packs the user enabled everywhere: `im.ponies.emote_rooms` names rooms and state keys
//!   whose packs apply in every room.
//!
//! Packs are read from this device's store (sync keeps them), the user's own falling back to
//! the server when the store has none yet.

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState;
use matrix_sdk::ruma::events::{GlobalAccountDataEventType, StateEventType};
use serde_json::Value;

use crate::model::{ImagePack, PackImage};
use crate::{Account, Error};

const USER_PACK: &str = "im.ponies.user_emotes";
const ROOM_PACK: &str = "im.ponies.room_emotes";
const ENABLED_ROOMS: &str = "im.ponies.emote_rooms";

impl Account {
    /// The image packs usable in `room_id` (or anywhere, for `None`): the user's own first,
    /// then the room's, then those the user enabled everywhere, each once.
    pub async fn image_packs(&self, room_id: Option<&str>) -> Result<Vec<ImagePack>, Error> {
        let account = self.client().account();
        let mut packs = Vec::new();
        let user = match account.account_data_raw(GlobalAccountDataEventType::from(USER_PACK)).await? {
            Some(raw) => Some(raw),
            None => account.fetch_account_data(GlobalAccountDataEventType::from(USER_PACK)).await.ok().flatten(),
        };
        if let Some(raw) = user
            && let Ok(content) = serde_json::from_str::<Value>(raw.json().get())
            && let Some(pack) = pack("user".to_owned(), String::new(), &content)
        {
            packs.push(pack);
        }
        if let Some(room) = room_id.and_then(|id| matrix_sdk::ruma::RoomId::parse(id).ok())
            && let Some(room) = self.client().get_room(&room)
        {
            packs.extend(room_packs(&room, None).await);
        }
        let enabled = account.account_data_raw(GlobalAccountDataEventType::from(ENABLED_ROOMS)).await.ok().flatten();
        if let Some(raw) = enabled
            && let Ok(content) = serde_json::from_str::<Value>(raw.json().get())
            && let Some(rooms) = content["rooms"].as_object()
        {
            for (room_id, keys) in rooms {
                let Some(room) =
                    matrix_sdk::ruma::RoomId::parse(room_id).ok().and_then(|id| self.client().get_room(&id))
                else {
                    continue;
                };
                let keys: Vec<String> = keys.as_object().map(|keys| keys.keys().cloned().collect()).unwrap_or_default();
                for found in room_packs(&room, Some(&keys)).await {
                    if !packs.iter().any(|known| known.id == found.id) {
                        packs.push(found);
                    }
                }
            }
        }
        Ok(packs)
    }
}

/// A room's packs: all of them, or those under `keys`.
async fn room_packs(room: &Room, keys: Option<&[String]>) -> Vec<ImagePack> {
    let Ok(events) = room.get_state_events(StateEventType::from(ROOM_PACK)).await else { return Vec::new() };
    let room_name =
        room.cached_display_name().map(|name| name.to_string()).unwrap_or_else(|| room.room_id().to_string());
    let mut packs = Vec::new();
    for event in events {
        let json: Option<Value> = match &event {
            RawAnySyncOrStrippedState::Sync(raw) => raw.deserialize_as_unchecked().ok(),
            RawAnySyncOrStrippedState::Stripped(raw) => raw.deserialize_as_unchecked().ok(),
        };
        let Some(json) = json else { continue };
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

    #[test]
    fn a_pack_without_images_should_be_none() {
        assert!(pack("room".into(), "Room".into(), &json!({ "images": {} })).is_none());
    }
}
