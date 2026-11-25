//! Custom emoji and stickers: image packs (Matrix 1.19, from MSC2545), as Cinny, FluffyChat and
//! others keep them. A pack is a set of images with short codes, each usable as a custom emoji
//! in text, as a sticker, or both.
//!
//! - The user's own pack: global account data `im.ponies.user_emotes` (the spec has none; some
//!   clients wrote `m.image_pack`, read first).
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
//!
//! Editing a pack ([`PackEdit`]) uploads its pictures, then, one change at a time per account,
//! reads each copy of the pack as the server holds it now, changes it and writes it back under
//! its own name (a new room pack, or a new list of rooms enabled everywhere, under the spec's
//! name; a new user pack under `im.ponies.user_emotes`). The change is listed at once: a room's
//! pack state is read again from the server, and the user's own waits (briefly) for sync.

use std::time::Duration;

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState;
use matrix_sdk::ruma::events::{GlobalAccountDataEventType, StateEventType};
use matrix_sdk::ruma::serde::Raw;
use serde_json::{Value, json};

use crate::model::{ImagePack, NewPackImage, NewPackImageSource, PackEdit, PackImage, PackImageChange, PackPlace};
use crate::{Account, Error, MediaSize};

/// The user's own pack: the spec's name, then the one before it.
const USER_PACK: [&str; 2] = ["m.image_pack", "im.ponies.user_emotes"];
/// A room's packs.
const ROOM_PACK: [&str; 2] = ["m.room.image_pack", "im.ponies.room_emotes"];
/// Rooms whose packs apply everywhere.
const ENABLED_ROOMS: [&str; 2] = ["m.image_pack.rooms", "im.ponies.emote_rooms"];
/// The names new packs are written under. A room's packs and the rooms enabled everywhere take
/// the spec's names (Matrix 1.19); the user's own pack, which the spec has no name for, takes
/// the one other apps read and write.
const NEW_USER_PACK: &str = USER_PACK[1];
const NEW_ROOM_PACK: &str = ROOM_PACK[0];
const NEW_ENABLED_ROOMS: &str = ENABLED_ROOMS[0];
/// The longest a short code may be, in bytes.
const MAX_SHORTCODE: usize = 100;
/// The largest side a pack image is shown at, which its `info` gives other apps: stickers
/// are drawn at their stated size.
const SHOWN_SIDE: u32 = 256;
/// How long a change to the user's account data may take to come back through sync before it
/// is listed anyway.
#[cfg(not(test))]
const SYNC_WAIT: Duration = Duration::from_secs(5);
/// Tests' servers don't sync: nothing comes back to wait for.
#[cfg(test)]
const SYNC_WAIT: Duration = Duration::ZERO;

/// The packs usable in a room, and whether the account may add one to it.
#[derive(Debug, Default)]
pub struct Listed {
    /// The user's own first (always, empty or not), then the room's, then those the user
    /// enabled everywhere, each once.
    pub packs: Vec<ImagePack>,
    /// Whether the account may add packs to the room.
    pub may_add: bool,
}

impl Account {
    /// The image packs usable in `room_id` (or anywhere, for `None`).
    pub async fn image_packs(&self, room_id: Option<&str>) -> Result<Listed, Error> {
        let account = self.client().account();
        let me = self.user_id()?;
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
        let own = user
            .and_then(|raw| serde_json::from_str::<Value>(raw.json().get()).ok())
            .and_then(|content| pack("user".to_owned(), String::new(), PackPlace::User, &content))
            .unwrap_or_else(|| empty_pack("user".to_owned(), String::new(), PackPlace::User));
        let mut listed = Listed { packs: vec![ImagePack { editable: true, ..own }], may_add: false };
        let enabled = self.enabled_rooms().await;
        if let Some(room) = room_id.and_then(|id| matrix_sdk::ruma::RoomId::parse(id).ok())
            && let Some(room) = self.client().get_room(&room)
        {
            let power = room.power_levels().await.ok();
            listed.may_add =
                power.as_ref().is_some_and(|power| power.user_can_send_state(&me, StateEventType::from(NEW_ROOM_PACK)));
            let server = self.server_pack_state(&room).await;
            listed.packs.extend(room_packs(&room, None, &server, &me, &enabled).await);
        }
        for (room_id, keys) in enabled.iter().fold(Vec::<(&str, Vec<String>)>::new(), |mut rooms, (room, key)| {
            match rooms.iter_mut().find(|(known, _)| known == room) {
                Some((_, keys)) => keys.push(key.clone()),
                None => rooms.push((room, vec![key.clone()])),
            }
            rooms
        }) {
            let Some(room) = matrix_sdk::ruma::RoomId::parse(room_id).ok().and_then(|id| self.client().get_room(&id))
            else {
                continue;
            };
            let server = self.server_pack_state(&room).await;
            for found in room_packs(&room, Some(&keys), &server, &me, &enabled).await {
                if !listed.packs.iter().any(|known| known.id == found.id) {
                    listed.packs.push(found);
                }
            }
        }
        Ok(listed)
    }

    /// The room packs the user enabled everywhere, as room ID and state key, under either name.
    async fn enabled_rooms(&self) -> Vec<(String, String)> {
        let mut enabled = Vec::new();
        for name in ENABLED_ROOMS {
            let raw = self.client().account().account_data_raw(GlobalAccountDataEventType::from(name)).await;
            let Some(content) = raw.ok().flatten().and_then(|raw| serde_json::from_str::<Value>(raw.json().get()).ok())
            else {
                continue;
            };
            for (room_id, keys) in content["rooms"].as_object().into_iter().flatten() {
                for key in keys.as_object().into_iter().flat_map(|keys| keys.keys()) {
                    let pair = (room_id.clone(), key.clone());
                    if !enabled.contains(&pair) {
                        enabled.push(pair);
                    }
                }
            }
        }
        enabled
    }
}

impl Account {
    /// A room's pack state events as the server held them when last read
    /// ([`Account::extra_state`]): under sliding sync read once a run per room, otherwise only
    /// after this run changed the room's packs. Empty when the server can't be asked now (asked
    /// again next time).
    async fn server_pack_state(&self, room: &Room) -> Vec<Value> {
        let known = self.extra_state.lock().ok().and_then(|known| known.rooms.get(room.room_id().as_str()).cloned());
        let events = match known {
            Some(known) => known,
            None if self.sliding() => self.extra_state(room).await,
            None => Vec::new(),
        };
        events
            .into_iter()
            .filter(|event| event["type"].as_str().is_some_and(|kind| ROOM_PACK.contains(&kind)))
            .collect()
    }

    /// Changes a pack, as [`PackEdit`] says, and writes it back; the pack changed (a new room
    /// pack's place, for [`PackEdit::Create`]).
    ///
    /// Pictures are uploaded first; then the account's pack changes go one at a time, each
    /// reading the pack as the server holds it now, so two changes never write over each
    /// other. A pack kept under both names has each copy changed as it stands.
    pub async fn edit_image_pack(&self, place: &PackPlace, edit: PackEdit) -> Result<PackPlace, Error> {
        let mut added = Vec::new();
        if let PackEdit::AddImages(images) = &edit {
            for image in images {
                let (url, info, name) = self.pack_picture(image.source.clone()).await?;
                added.push((image.clone(), url, info, name));
            }
        }
        let _one_at_a_time = self.pack_edits.lock().await;
        match edit {
            PackEdit::UseEverywhere(on) => {
                self.use_everywhere(place, on).await?;
                return Ok(place.clone());
            }
            PackEdit::Create(name) => return self.create_room_pack(place, name.trim()).await,
            _ => {}
        }
        let mut copies = self.pack_copies(place).await?;
        if copies.is_empty() {
            // The user's pack is made by its first change; a room's only by `Create`: one deleted
            // while a picture uploaded stays deleted.
            if *place != PackPlace::User {
                return Err(Error::MessageUnavailable);
            }
            copies.push((NEW_USER_PACK, json!({ "images": {} })));
        }
        let codes = new_shortcodes(&copies, &added);
        // An image changed or removed is the one the apps show: the first copy's. In another
        // copy, the same short code naming a different picture is left alone.
        let shown = match &edit {
            PackEdit::ChangeImage { shortcode, .. } | PackEdit::RemoveImage(shortcode) => copies
                .first()
                .and_then(|(_, content)| content["images"][shortcode.as_str()]["url"].as_str().map(str::to_owned)),
            _ => None,
        };
        let mut changed = Vec::new();
        let mut missing = None;
        for (kind, mut content) in copies {
            if let PackEdit::ChangeImage { shortcode, .. } | PackEdit::RemoveImage(shortcode) = &edit
                && content["images"][shortcode.as_str()]["url"].as_str() != shown.as_deref()
            {
                missing = Some(Error::MessageUnavailable);
                continue;
            }
            let before = content.clone();
            match change(&mut content, &edit, &added, &codes).map(|()| widen_pack_usage(&mut content)) {
                Ok(()) if content != before => changed.push((kind, content)),
                Ok(()) => {}
                // An image one copy lacks is left to the copies that have it.
                Err(Error::MessageUnavailable) => missing = Some(Error::MessageUnavailable),
                // Anything else a copy refuses (a short code taken there) refuses the change.
                Err(error) => return Err(error),
            }
        }
        if changed.is_empty()
            && let Some(error) = missing
        {
            return Err(error);
        }
        // The copy the apps show goes last: if another copy fails, the change still shows as
        // not done, and doing it again reaches every copy.
        changed.reverse();
        self.write_pack(place, changed).await?;
        Ok(place.clone())
    }

    /// Makes a room pack named `name` under a state key of its own, never one used before
    /// (another pack's, or a deleted one's that people may still use everywhere).
    async fn create_room_pack(&self, place: &PackPlace, name: &str) -> Result<PackPlace, Error> {
        let PackPlace::Room { room_id, .. } = place else { return Err(Error::MessageUnavailable) };
        let room = self.joined_room(room_id)?;
        let state = self.refresh_pack_state(&room).await?;
        let taken: Vec<&str> = state.iter().filter_map(|event| event["state_key"].as_str()).collect();
        let state_key = new_state_key(name, &taken);
        let mut content = json!({ "images": {} });
        set_pack_field(&mut content, "display_name", name);
        let created = PackPlace::Room { room_id: room_id.clone(), state_key };
        self.write_pack(&created, vec![(NEW_ROOM_PACK, content)]).await?;
        Ok(created)
    }

    /// A pack's copies as the server holds them now, by the name each is kept under; none for
    /// a pack not made yet (or deleted).
    async fn pack_copies(&self, place: &PackPlace) -> Result<Vec<(&'static str, Value)>, Error> {
        let mut copies = Vec::new();
        match place {
            PackPlace::User => {
                let account = self.client().account();
                for name in USER_PACK {
                    let Some(raw) = account.fetch_account_data(GlobalAccountDataEventType::from(name)).await? else {
                        continue;
                    };
                    if let Ok(found) = serde_json::from_str::<Value>(raw.json().get())
                        && found.as_object().is_some_and(|found| !found.is_empty())
                    {
                        copies.push((name, found));
                    }
                }
            }
            PackPlace::Room { room_id, state_key } => {
                let room = self.joined_room(room_id)?;
                for name in ROOM_PACK {
                    let request = matrix_sdk::ruma::api::client::state::get_state_event_for_key::v3::Request::new(
                        room.room_id().to_owned(),
                        StateEventType::from(name),
                        state_key.clone(),
                    );
                    let found = match self.client().send(request).await {
                        Ok(response) => serde_json::from_str::<Value>(response.event_or_content.get())
                            .map_err(|e| Error::Storage(e.into()))?,
                        Err(error)
                            if error.client_api_error_kind()
                                == Some(&matrix_sdk::ruma::api::error::ErrorKind::NotFound) =>
                        {
                            continue;
                        }
                        Err(error) => return Err(matrix_sdk::Error::from(error).into()),
                    };
                    if found.as_object().is_some_and(|found| !found.is_empty()) {
                        copies.push((name, found));
                    }
                }
            }
        }
        Ok(copies)
    }

    /// Writes a pack's changed copies, each under its name, then makes sure listing packs
    /// shows them: a room's state is read again from the server (its own times, not this
    /// device's), the user's account data waits for sync.
    async fn write_pack(&self, place: &PackPlace, copies: Vec<(&'static str, Value)>) -> Result<(), Error> {
        match place {
            PackPlace::User => {
                let account = self.client().account();
                let mut written = Vec::new();
                let mut failed = Ok(());
                for (kind, content) in &copies {
                    let raw = Raw::new(content).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
                    if let Err(error) = account.set_account_data_raw(GlobalAccountDataEventType::from(*kind), raw).await
                    {
                        failed = Err(error);
                        break;
                    }
                    written.push((kind, content));
                }
                // What was written shows when the packs are listed next, even if a later copy
                // failed.
                for (kind, content) in written {
                    self.await_account_data(kind, content).await;
                }
                failed?;
            }
            PackPlace::Room { room_id, state_key } => {
                let room = self.joined_room(room_id)?;
                let mut written = Ok(());
                for (kind, content) in copies {
                    if let Err(error) = room.send_state_event_raw(kind, state_key, content).await {
                        written = Err(error);
                        break;
                    }
                }
                // What was written shows (all, or the copies before one that failed): the room's
                // pack state is read again, or if it can't be, read again next time rather than
                // the snapshot from before standing for it.
                if self.refresh_pack_state(&room).await.is_err() {
                    self.forget_extra_state(room_id);
                }
                written?;
            }
        }
        Ok(())
    }

    /// Reads a room's state from the server and keeps it beside the store's
    /// ([`Account::read_extra_state`]), returning its packs (deleted ones included, as empty
    /// content), for listing and for new packs' state keys.
    async fn refresh_pack_state(&self, room: &Room) -> Result<Vec<Value>, Error> {
        let events = self.read_extra_state(room).await?;
        Ok(events
            .into_iter()
            .filter(|event| event["type"].as_str().is_some_and(|kind| ROOM_PACK.contains(&kind)))
            .collect())
    }

    /// Waits, a few seconds at most, until sync brings the account data just written.
    async fn await_account_data(&self, kind: &str, written: &Value) {
        let account = self.client().account();
        let deadline = tokio::time::Instant::now() + SYNC_WAIT;
        while tokio::time::Instant::now() < deadline {
            let stored = account.account_data_raw(GlobalAccountDataEventType::from(kind)).await.ok().flatten();
            if stored.and_then(|raw| serde_json::from_str::<Value>(raw.json().get()).ok()).as_ref() == Some(written) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Adds a room's pack to those the user uses everywhere, or takes it off under both names.
    /// The reference goes under the list name that matches each name the pack is kept under
    /// (`m.image_pack.rooms` names `m.room.image_pack` packs, `im.ponies.emote_rooms` the older
    /// ones), so the apps reading either find it.
    async fn use_everywhere(&self, place: &PackPlace, on: bool) -> Result<(), Error> {
        let PackPlace::Room { room_id, state_key } = place else { return Ok(()) };
        let kept_under: Vec<&str> =
            if on { self.pack_copies(place).await?.into_iter().map(|(kind, _)| kind).collect() } else { Vec::new() };
        let account = self.client().account();
        for (name, pack_name) in ENABLED_ROOMS.into_iter().zip(ROOM_PACK) {
            // A pack kept under neither name (not read yet) is referenced the spec's way.
            let referenced = kept_under.contains(&pack_name) || (kept_under.is_empty() && name == NEW_ENABLED_ROOMS);
            if on && !referenced {
                continue;
            }
            let raw = account.fetch_account_data(GlobalAccountDataEventType::from(name)).await?;
            let content = raw.and_then(|raw| serde_json::from_str::<Value>(raw.json().get()).ok());
            let mut content = match content {
                Some(content) if content.is_object() => content,
                // Nothing to take a pack off; a new list to put one on.
                _ if !on => continue,
                _ => json!({}),
            };
            let changed = if on {
                enable_room(&mut content, room_id, state_key)
            } else {
                disable_room(&mut content, room_id, state_key)
            };
            if changed {
                let raw = Raw::new(&content).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
                account.set_account_data_raw(GlobalAccountDataEventType::from(name), raw).await?;
                self.await_account_data(name, &content).await;
            }
        }
        Ok(())
    }

    /// A new image's picture on the server, unencrypted, with its `info` and a name to make
    /// its short code from.
    async fn pack_picture(&self, source: NewPackImageSource) -> Result<(String, Value, String), Error> {
        match source {
            NewPackImageSource::File(path) => {
                let bytes =
                    tokio::fs::read(&path).await.map_err(|source| Error::ReadFile { path: path.clone(), source })?;
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                if mime.type_() != mime_guess::mime::IMAGE {
                    return Err(Error::NotAnImage(path));
                }
                let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
                let info = image_info(&bytes, mime.essence_str(), picture_size(&bytes));
                let url = self.client().media().upload(&mime, bytes, None).await?.content_uri;
                Ok((url.to_string(), info, name))
            }
            NewPackImageSource::Media { media, name, dimensions } => {
                let name = std::path::Path::new(&name)
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or(name);
                if let matrix_sdk::ruma::events::room::MediaSource::Plain(url) = crate::convert::media_source(&media)? {
                    let mut info = json!({});
                    if let Some((w, h)) = dimensions.map(shown_size) {
                        info = json!({ "w": w, "h": h });
                    }
                    return Ok((url.to_string(), info, name));
                }
                let bytes = self.fetch_media(&media, MediaSize::Original).await?;
                let mime = image::guess_format(&bytes)
                    .ok()
                    .map(|format| format.to_mime_type())
                    .ok_or_else(|| Error::NotAnImage(name.clone().into()))?;
                let info = image_info(&bytes, mime, picture_size(&bytes).or(dimensions));
                let mime: mime_guess::mime::Mime = mime.parse().map_err(|_| Error::NotAnImage(name.clone().into()))?;
                let url = self.client().media().upload(&mime, bytes, None).await?.content_uri;
                Ok((url.to_string(), info, name))
            }
        }
    }
}

/// A picture's width and height in pixels, read from its header.
fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
}

/// `size` scaled to fit [`SHOWN_SIDE`], never up.
fn shown_size((w, h): (u32, u32)) -> (u32, u32) {
    let side = w.max(h).max(1);
    if side <= SHOWN_SIDE {
        return (w, h);
    }
    let scale = |x: u32| u32::try_from(u64::from(x) * u64::from(SHOWN_SIDE) / u64::from(side)).unwrap_or(1).max(1);
    (scale(w), scale(h))
}

/// A pack image's `info`: the size it shows at, its type and its length.
fn image_info(bytes: &[u8], mime: &str, size: Option<(u32, u32)>) -> Value {
    let mut info = json!({ "mimetype": mime, "size": bytes.len() });
    if let Some((w, h)) = size.map(shown_size) {
        info["w"] = json!(w);
        info["h"] = json!(h);
    }
    info
}

/// Applies an edit to one copy of a pack; `added` holds the pictures uploaded for
/// [`PackEdit::AddImages`].
fn change(
    content: &mut Value,
    edit: &PackEdit,
    added: &[(NewPackImage, String, Value, String)],
    codes: &[String],
) -> Result<(), Error> {
    match edit {
        PackEdit::Rename(name) => set_pack_field(content, "display_name", name.trim()),
        PackEdit::SetAvatar(url) => set_pack_field(content, "avatar_url", url.as_deref().unwrap_or_default()),
        PackEdit::AddImages(_) => {
            for ((image, url, info, _), code) in added.iter().zip(codes) {
                add_image(content, image, url.clone(), info.clone(), code);
            }
        }
        PackEdit::ChangeImage { shortcode, image } => change_image(content, shortcode, image)?,
        PackEdit::RemoveImage(shortcode) => remove_image(content, shortcode),
        PackEdit::Delete => *content = json!({}),
        PackEdit::Create(_) | PackEdit::UseEverywhere(_) => {}
    }
    Ok(())
}

/// A state key for a new room pack named `name`: made from the name, with a random part so no
/// key is ever used twice (two people making packs at once, or a deleted pack's key that people
/// still use everywhere), free among `taken`.
fn new_state_key(name: &str, taken: &[&str]) -> String {
    loop {
        let mut bytes = [0u8; 3];
        let _ = getrandom::fill(&mut bytes);
        let tail: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let key = format!("{}-{tail}", shortcode_from(name));
        if !taken.contains(&key.as_str()) {
            return key;
        }
    }
}

/// Widens a pack's own `usage`, when it has one, to every use its images name: the spec's apps
/// go by the pack's alone, so a sticker added to an emoji pack would be missing from their
/// sticker pickers. A pack naming no use is every use already.
fn widen_pack_usage(content: &mut Value) {
    let Some(declared) = content["pack"]["usage"].as_array().filter(|usage| !usage.is_empty()) else { return };
    let mut uses: Vec<Value> = declared.clone();
    for image in content["images"].as_object().into_iter().flat_map(|images| images.values()) {
        for kind in image["usage"].as_array().into_iter().flatten() {
            if !uses.contains(kind) {
                uses.push(kind.clone());
            }
        }
    }
    if uses.len() != declared.len() {
        content["pack"]["usage"] = Value::Array(uses);
    }
}

/// Sets a field of the pack's own `pack` object; blank removes it.
fn set_pack_field(content: &mut Value, field: &str, value: &str) {
    if !content.is_object() {
        *content = json!({});
    }
    if !content["pack"].is_object() {
        content["pack"] = json!({});
    }
    if let Some(pack) = content["pack"].as_object_mut() {
        if value.is_empty() {
            pack.remove(field);
        } else {
            pack.insert(field.to_owned(), json!(value));
        }
    }
}

/// Runs `change` on the pack's images, made an object if they weren't one.
fn with_images<R>(content: &mut Value, change: impl FnOnce(&mut serde_json::Map<String, Value>) -> R) -> R {
    if !content.is_object() {
        *content = json!({});
    }
    let mut images = match content.get_mut("images").map(Value::take) {
        Some(Value::Object(images)) => images,
        _ => serde_json::Map::new(),
    };
    let result = change(&mut images);
    content["images"] = Value::Object(images);
    result
}

/// `usage` as written: what the image may be used as.
fn usage(emoticon: bool, sticker: bool) -> Value {
    let mut usage = Vec::new();
    if emoticon {
        usage.push("emoticon");
    }
    if sticker {
        usage.push("sticker");
    }
    json!(usage)
}

/// Whether `shortcode` may name a new or changed image, as the spec's grammar has it: 1 to 100
/// ASCII letters, digits, `-` and `_`. Images already named otherwise still show.
pub fn valid_shortcode(shortcode: &str) -> bool {
    (1..=MAX_SHORTCODE).contains(&shortcode.len())
        && shortcode.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// A short code made from a name: ASCII letters and digits kept (lower case), anything else
/// `_`, short enough to take a number after it.
///
/// ```
/// assert_eq!(matrix_core::packs::shortcode_from("Happy Cat!"), "happy_cat");
/// assert_eq!(matrix_core::packs::shortcode_from("Grüße"), "gr_e");
/// assert_eq!(matrix_core::packs::shortcode_from("…"), "sticker");
/// ```
pub fn shortcode_from(name: &str) -> String {
    let made: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let mut joined = String::new();
    for part in made.split('_').filter(|part| !part.is_empty()) {
        if !joined.is_empty() {
            joined.push('_');
        }
        joined.push_str(part);
    }
    joined.truncate(MAX_SHORTCODE - 12);
    if joined.is_empty() { "sticker".to_owned() } else { joined }
}

/// `wanted`, or with a number after it when an image has it already.
fn free_shortcode(taken: &std::collections::HashSet<String>, wanted: &str) -> String {
    if !taken.contains(wanted) {
        return wanted.to_owned();
    }
    (2..).map(|n| format!("{wanted}_{n}")).find(|code| !taken.contains(code)).unwrap_or_default()
}

/// The short code each added image takes: the one asked for (or made from its name), with a
/// number when an image in any copy of the pack has it, so every copy files it under the same.
fn new_shortcodes(copies: &[(&str, Value)], added: &[(NewPackImage, String, Value, String)]) -> Vec<String> {
    let mut taken: std::collections::HashSet<String> = copies
        .iter()
        .flat_map(|(_, content)| content["images"].as_object().into_iter().flat_map(|images| images.keys().cloned()))
        .collect();
    added
        .iter()
        .map(|(image, _, _, name)| {
            let wanted = match image.shortcode.as_deref().map(str::trim).filter(|code| valid_shortcode(code)) {
                Some(code) => code.to_owned(),
                None => shortcode_from(name),
            };
            let code = free_shortcode(&taken, &wanted);
            taken.insert(code.clone());
            code
        })
        .collect()
}

/// Adds an image whose picture is at `url`, under `shortcode`.
fn add_image(content: &mut Value, image: &NewPackImage, url: String, info: Value, shortcode: &str) {
    let mut entry = json!({ "url": url, "info": info });
    if image.emoticon || image.sticker {
        entry["usage"] = usage(image.emoticon, image.sticker);
    }
    if let Some(body) = image.body.as_deref().map(str::trim).filter(|body| !body.is_empty()) {
        entry["body"] = json!(body);
    }
    with_images(content, |images| {
        images.insert(shortcode.to_owned(), entry);
    });
}

/// Changes an image's short code, description and use, keeping the rest of it.
fn change_image(content: &mut Value, shortcode: &str, change: &PackImageChange) -> Result<(), Error> {
    let new = change.shortcode.trim();
    if !valid_shortcode(new) {
        return Err(Error::InvalidShortcode(new.to_owned()));
    }
    with_images(content, |images| {
        if new != shortcode && images.contains_key(new) {
            return Err(Error::ShortcodeTaken(new.to_owned()));
        }
        let Some(mut entry) = images.remove(shortcode) else { return Err(Error::MessageUnavailable) };
        match change.body.trim() {
            "" => {
                if let Some(entry) = entry.as_object_mut() {
                    entry.remove("body");
                }
            }
            body => entry["body"] = json!(body),
        }
        if change.emoticon || change.sticker {
            entry["usage"] = usage(change.emoticon, change.sticker);
        }
        images.insert(new.to_owned(), entry);
        Ok(())
    })
}

/// Takes an image out, and off the pack's picture if it was that.
fn remove_image(content: &mut Value, shortcode: &str) {
    let removed = with_images(content, |images| images.remove(shortcode));
    let url = removed.as_ref().and_then(|image| image["url"].as_str());
    if url.is_some() && content["pack"]["avatar_url"].as_str() == url {
        set_pack_field(content, "avatar_url", "");
    }
}

/// Adds a room's pack to an enabled-rooms list; whether that changed it.
fn enable_room(content: &mut Value, room_id: &str, state_key: &str) -> bool {
    if !content["rooms"].is_object() {
        content["rooms"] = json!({});
    }
    if !content["rooms"][room_id].is_object() {
        content["rooms"][room_id] = json!({});
    }
    if content["rooms"][room_id].get(state_key).is_some() {
        return false;
    }
    content["rooms"][room_id][state_key] = json!({});
    true
}

/// Takes a room's pack off an enabled-rooms list, and the room once it names none; whether
/// that changed it.
fn disable_room(content: &mut Value, room_id: &str, state_key: &str) -> bool {
    let Some(rooms) = content["rooms"].as_object_mut() else { return false };
    let Some(keys) = rooms.get_mut(room_id).and_then(Value::as_object_mut) else { return false };
    if keys.remove(state_key).is_none() {
        return false;
    }
    if keys.is_empty() {
        rooms.remove(room_id);
    }
    true
}

/// A room's packs: all of them, or those under `keys`; a pack under both names once. `server`
/// is pack state read from the server, for packs the store doesn't hold.
async fn room_packs(
    room: &Room,
    keys: Option<&[String]>,
    server: &[Value],
    me: &matrix_sdk::ruma::UserId,
    enabled: &[(String, String)],
) -> Vec<ImagePack> {
    let power = room.power_levels().await.ok();
    let room_name =
        room.cached_display_name().map(|name| name.to_string()).unwrap_or_else(|| room.room_id().to_string());
    let room_id = room.room_id().to_string();
    let mut packs: Vec<ImagePack> = Vec::new();
    for name in ROOM_PACK {
        let may_write = power.as_ref().is_some_and(|power| power.user_can_send_state(me, StateEventType::from(name)));
        for json in newest_pack_events(room, name, server).await {
            let state_key = json["state_key"].as_str().unwrap_or_default().to_owned();
            if keys.is_some_and(|keys| !keys.contains(&state_key)) {
                continue;
            }
            let everywhere = enabled.iter().any(|(room, key)| *room == room_id && *key == state_key);
            let place = PackPlace::Room { room_id: room_id.clone(), state_key: state_key.clone() };
            let Some(found) = pack(format!("{room_id}/{state_key}"), room_name.clone(), place, &json["content"]) else {
                continue;
            };
            match packs.iter_mut().find(|known| known.id == found.id) {
                // Kept under both names: changing it writes both.
                Some(known) => known.editable &= may_write,
                None => packs.push(ImagePack { editable: may_write, everywhere, ..found }),
            }
        }
    }
    packs
}

/// A room's pack events under one state event type, the newer of the store's and the
/// server's copy of each.
async fn newest_pack_events(room: &Room, name: &str, server: &[Value]) -> Vec<Value> {
    let events = room.get_state_events(StateEventType::from(name)).await.unwrap_or_default();
    let local: Vec<Value> = events
        .iter()
        .filter_map(|event| match event {
            RawAnySyncOrStrippedState::Sync(raw) => raw.deserialize_as_unchecked().ok(),
            RawAnySyncOrStrippedState::Stripped(raw) => raw.deserialize_as_unchecked().ok(),
        })
        .collect();
    // Of the store's and the server's copies of a pack, the newer counts.
    let fetched = server.iter().filter(|event| event["type"].as_str() == Some(name)).cloned().collect();
    crate::widgets::newest(fetched, local)
}

/// A pack with nothing in it.
fn empty_pack(id: String, name: String, place: PackPlace) -> ImagePack {
    ImagePack {
        id,
        name,
        avatar: None,
        avatar_url: None,
        images: Vec::new(),
        place,
        editable: false,
        everywhere: false,
    }
}

/// A pack from its event content, named `fallback_name` when it names itself nothing; `None`
/// when the content is empty (a deleted pack). Not editable nor used everywhere until the
/// caller says.
pub(crate) fn pack(id: String, fallback_name: String, place: PackPlace, content: &Value) -> Option<ImagePack> {
    if content.as_object().is_none_or(serde_json::Map::is_empty) {
        return None;
    }
    let usage = |value: &Value| -> Option<(bool, bool)> {
        let list = value.as_array().filter(|list| !list.is_empty())?;
        let has = |kind: &str| list.iter().any(|usage| usage.as_str() == Some(kind));
        Some((has("emoticon"), has("sticker")))
    };
    // A pack that says nothing is both.
    let pack_usage = usage(&content["pack"]["usage"]).unwrap_or((true, true));
    let images: Vec<PackImage> = content["images"]
        .as_object()
        .into_iter()
        .flatten()
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
    let name = content["pack"]["display_name"].as_str().filter(|name| !name.trim().is_empty());
    let avatar_url =
        content["pack"]["avatar_url"].as_str().map(matrix_sdk::ruma::OwnedMxcUri::from).filter(|mxc| mxc.is_valid());
    let avatar = avatar_url.clone().and_then(crate::convert::avatar);
    Some(ImagePack {
        name: name.map_or(fallback_name, str::to_owned),
        avatar,
        avatar_url: avatar_url.map(|url| url.to_string()),
        images,
        ..empty_pack(id, String::new(), place)
    })
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
        let found = pack("user".into(), String::new(), PackPlace::User, &content).expect("a pack");
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
        let found = pack("room".into(), "Room".into(), PackPlace::User, &content).expect("a pack");
        let usage: Vec<_> =
            found.images.iter().map(|image| (image.shortcode.as_str(), image.emoticon, image.sticker)).collect();
        assert_eq!(usage, [("blob", true, false), ("wave", false, true)]);
    }

    #[test]
    fn a_pack_saying_nothing_of_its_usage_should_be_both() {
        let content = json!({ "images": { "wave": { "url": "mxc://x.org/wave" } } });
        let image = &pack("room".into(), "Room".into(), PackPlace::User, &content).expect("a pack").images[0];
        assert!(image.emoticon && image.sticker);
    }

    #[test]
    fn a_pack_without_a_name_should_take_its_rooms() {
        let content = json!({ "images": { "wave": { "url": "mxc://x.org/wave" } } });
        assert_eq!(pack("room".into(), "Design".into(), PackPlace::User, &content).expect("a pack").name, "Design");
    }

    #[test]
    fn images_without_a_valid_address_should_be_left_out() {
        let content =
            json!({ "images": { "bad": { "url": "https://x.org/a.png" }, "good": { "url": "mxc://x.org/a" } } });
        let found = pack("room".into(), "Room".into(), PackPlace::User, &content).expect("a pack");
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
        let packs = account.image_packs(Some("!room:example.org")).await.expect("packs").packs;
        let found: Vec<_> =
            packs[1..].iter().map(|pack| (pack.name.as_str(), pack.images[0].shortcode.as_str())).collect();
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
        let mut stale = pack_event("cats", "stalecat");
        stale["origin_server_ts"] = json!(0);
        let packs = room_packs(&room, None, &[stale, pack_event("birds", "bird")], &me(), &[]).await;
        let mut found: Vec<_> =
            packs.iter().map(|pack| (pack.name.as_str(), pack.images[0].shortcode.as_str())).collect();
        found.sort_unstable();
        assert_eq!(found, [("birds", "bird"), ("cats", "blobcat")]);
        // The same event redacted on the server since (its time unchanged): no pack.
        let mut redacted = pack_event("cats", "blobcat");
        redacted["content"] = json!({});
        assert!(room_packs(&room, None, &[redacted], &me(), &[]).await.is_empty());
        // A newer copy on the server (changed since the store's) counts instead.
        let mut changed = pack_event("cats", "newcat");
        changed["origin_server_ts"] = json!(2);
        let packs = room_packs(&room, None, &[changed], &me(), &[]).await;
        assert_eq!(packs[0].images[0].shortcode, "newcat");
    }

    fn me() -> matrix_sdk::ruma::OwnedUserId {
        matrix_sdk::ruma::owned_user_id!("@me:example.org")
    }

    #[test]
    fn an_empty_pack_should_still_be_a_pack_to_add_to() {
        let found = pack("room".into(), "Room".into(), PackPlace::User, &json!({ "images": {} })).expect("a pack");
        assert!(found.images.is_empty());
    }

    #[test]
    fn a_deleted_pack_should_be_none() {
        assert!(pack("room".into(), "Room".into(), PackPlace::User, &json!({})).is_none());
    }

    fn new_image(shortcode: Option<&str>) -> NewPackImage {
        NewPackImage {
            source: NewPackImageSource::File("cat.png".into()),
            shortcode: shortcode.map(str::to_owned),
            body: None,
            emoticon: false,
            sticker: true,
        }
    }

    #[test]
    fn an_added_image_should_take_a_free_short_code_from_its_name() {
        let mut content = json!({ "images": { "happy_cat": { "url": "mxc://x.org/a" } } });
        let added = [(new_image(None), "mxc://x.org/b".to_owned(), json!({}), "Happy Cat".to_owned())];
        let codes = new_shortcodes(&[("im.ponies.user_emotes", content.clone())], &added);
        add_image(&mut content, &new_image(None), "mxc://x.org/b".into(), json!({}), &codes[0]);
        assert_eq!(content["images"]["happy_cat_2"]["url"], "mxc://x.org/b");
        assert_eq!(content["images"]["happy_cat_2"]["usage"], json!(["sticker"]));
    }

    #[test]
    fn an_image_added_to_nothing_should_make_the_pack() {
        let mut content = json!({});
        add_image(&mut content, &new_image(Some("wave")), "mxc://x.org/w".into(), json!({ "w": 64 }), "wave");
        let found = pack("user".into(), String::new(), PackPlace::User, &content).expect("a pack");
        assert_eq!((found.images[0].shortcode.as_str(), found.images[0].sticker), ("wave", true));
    }

    #[test]
    fn changing_an_image_should_keep_its_picture_under_the_new_code() {
        let mut content = json!({ "images": { "cat": { "url": "mxc://x.org/a", "info": { "w": 64 } } } });
        let change =
            PackImageChange { shortcode: "kitty".into(), body: "A cat".into(), emoticon: true, sticker: false };
        change_image(&mut content, "cat", &change).expect("changed");
        assert_eq!(
            content["images"],
            json!({ "kitty": { "url": "mxc://x.org/a", "info": { "w": 64 }, "body": "A cat", "usage": ["emoticon"] } })
        );
    }

    #[test]
    fn an_image_should_not_take_another_images_code() {
        let mut content = json!({ "images": { "a": { "url": "mxc://x.org/a" }, "b": { "url": "mxc://x.org/b" } } });
        let change = PackImageChange { shortcode: "b".into(), body: String::new(), emoticon: true, sticker: true };
        assert!(matches!(change_image(&mut content, "a", &change), Err(Error::ShortcodeTaken(_))));
    }

    #[test]
    fn a_short_code_with_a_space_should_be_refused() {
        let mut content = json!({ "images": { "a": { "url": "mxc://x.org/a" } } });
        let change = PackImageChange { shortcode: "a b".into(), body: String::new(), emoticon: true, sticker: true };
        assert!(matches!(change_image(&mut content, "a", &change), Err(Error::InvalidShortcode(_))));
    }

    #[test]
    fn removing_the_packs_picture_should_leave_it_without_one() {
        let mut content = json!({
            "pack": { "display_name": "Cats", "avatar_url": "mxc://x.org/a" },
            "images": { "a": { "url": "mxc://x.org/a" } }
        });
        remove_image(&mut content, "a");
        assert_eq!(content, json!({ "pack": { "display_name": "Cats" }, "images": {} }));
    }

    #[test]
    fn a_pack_used_everywhere_should_be_listed_once_and_taken_off_with_its_room() {
        let mut content = json!({ "rooms": { "!other:x.org": { "": {} } } });
        assert!(enable_room(&mut content, "!r:x.org", "cats"));
        assert!(!enable_room(&mut content, "!r:x.org", "cats"));
        assert!(disable_room(&mut content, "!r:x.org", "cats"));
        assert_eq!(content, json!({ "rooms": { "!other:x.org": { "": {} } } }));
    }

    #[test]
    fn a_large_picture_should_be_shown_at_most_256_pixels_a_side() {
        assert_eq!((shown_size((1024, 512)), shown_size((100, 80))), ((256, 128), (100, 80)));
    }

    /// The body of a request the test server took.
    fn body(request: &str) -> serde_json::Value {
        request.split_once("\r\n\r\n").and_then(|(_, body)| serde_json::from_str(body).ok()).unwrap_or_default()
    }

    /// The path of a request the test server took.
    fn path(request: &str) -> String {
        let path = request.split_whitespace().nth(1).unwrap_or_default();
        path.replace("%21", "!").replace("%3A", ":").replace("%40", "@")
    }

    fn from_message(url: &str, name: &str) -> NewPackImage {
        NewPackImage {
            source: NewPackImageSource::Media {
                media: crate::MediaRef::from_mxc(url).expect("plain media"),
                name: name.into(),
                dimensions: Some((64, 64)),
            },
            shortcode: None,
            body: None,
            emoticon: false,
            sticker: true,
        }
    }

    #[tokio::test]
    async fn two_changes_at_once_should_both_land_in_the_users_pack() {
        let kept = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<String, Value>::new()));
        let store = std::sync::Arc::clone(&kept);
        let server = crate::test_server::Server::new(move |request| {
            let path = path(request);
            let Some((_, kind)) = path.split_once("/account_data/") else {
                return crate::test_server::thread_response(request);
            };
            let mut kept = store.lock().expect("kept");
            if request.starts_with("PUT") {
                kept.insert(kind.to_owned(), body(request));
                return json!({});
            }
            kept.get(kind).cloned().unwrap_or_else(|| json!({ "errcode": "M_NOT_FOUND", "error": "none" }))
        });
        let account = Account::for_test(&server.url()).await;
        let (a, b) = tokio::join!(
            account.edit_image_pack(&PackPlace::User, PackEdit::AddImages(vec![from_message("mxc://x.org/a", "cat")])),
            account.edit_image_pack(&PackPlace::User, PackEdit::AddImages(vec![from_message("mxc://x.org/b", "dog")])),
        );
        a.expect("first change");
        b.expect("second change");
        let pack = kept.lock().expect("kept").get(NEW_USER_PACK).cloned().expect("the pack written");
        let mut codes: Vec<&String> = pack["images"].as_object().expect("images").keys().collect();
        codes.sort();
        assert_eq!(codes, ["cat", "dog"]);
    }

    #[tokio::test]
    async fn a_pack_kept_under_both_names_should_have_each_copy_changed_as_it_stands() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, Value)>::new()));
        let writes = std::sync::Arc::clone(&written);
        let server = crate::test_server::Server::new(move |request| {
            let path = path(request);
            if request.starts_with("PUT") && path.contains("/state/") {
                writes.lock().expect("writes").push((path, body(request)));
                return json!({ "event_id": "$written" });
            }
            if path.ends_with("/state/m.room.image_pack/cats") {
                return json!({ "images": { "new": { "url": "mxc://x.org/n" } } });
            }
            if path.ends_with("/state/im.ponies.room_emotes/cats") {
                return json!({ "images": { "old": { "url": "mxc://x.org/o" } }, "attribution": "Ada" });
            }
            if path.ends_with("/state") {
                return json!([]);
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let place = PackPlace::Room { room_id: "!room:example.org".into(), state_key: "cats".into() };
        account.edit_image_pack(&place, PackEdit::Rename("Cats".into())).await.expect("renamed");
        let mut written = written.lock().expect("writes").clone();
        // The copy the apps show is written last.
        assert!(written.last().is_some_and(|(path, _)| path.contains("m.room.image_pack")));
        written.sort_by(|a, b| a.0.cmp(&b.0));
        let kept: Vec<(bool, Value)> =
            written.iter().map(|(path, body)| (path.contains("im.ponies"), body["images"].clone())).collect();
        assert_eq!(
            kept,
            [
                (true, json!({ "old": { "url": "mxc://x.org/o" } })),
                (false, json!({ "new": { "url": "mxc://x.org/n" } }))
            ]
        );
        assert!(written.iter().all(|(_, body)| body["pack"]["display_name"] == "Cats"));
        assert_eq!(written[0].1["attribution"], "Ada");
    }

    #[test]
    fn a_new_room_pack_should_take_a_key_no_pack_had() {
        let key = new_state_key("Cats!", &["cats", ""]);
        assert!(key.starts_with("cats-") && key.len() > "cats-".len());
    }

    #[test]
    fn short_codes_should_follow_the_specs_grammar() {
        assert!(valid_shortcode("cat_wave-2"));
        assert!(!valid_shortcode("cat.v2") && !valid_shortcode("cat/blue") && !valid_shortcode("grüße"));
        assert!(!valid_shortcode(&"a".repeat(101)) && !valid_shortcode(""));
    }

    #[tokio::test]
    async fn a_change_to_a_deleted_room_pack_should_not_make_it_again() {
        let writes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let written = std::sync::Arc::clone(&writes);
        let server = crate::test_server::Server::new(move |request| {
            let path = path(request);
            if request.starts_with("PUT") && path.contains("/state/") {
                written.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return json!({ "event_id": "$written" });
            }
            if path.contains("/state/m.room.image_pack/cats") {
                return json!({});
            }
            if path.contains("/state/im.ponies.room_emotes/cats") {
                return json!({ "errcode": "M_NOT_FOUND", "error": "none" });
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let place = PackPlace::Room { room_id: "!room:example.org".into(), state_key: "cats".into() };
        let added =
            account.edit_image_pack(&place, PackEdit::AddImages(vec![from_message("mxc://x.org/a", "cat")])).await;
        assert!(matches!(added, Err(Error::MessageUnavailable)));
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_short_code_taken_in_either_copy_should_refuse_the_change() {
        let writes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let written = std::sync::Arc::clone(&writes);
        let server = crate::test_server::Server::new(move |request| {
            let path = path(request);
            if request.starts_with("PUT") && path.contains("/state/") {
                written.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return json!({ "event_id": "$written" });
            }
            if path.ends_with("/state/m.room.image_pack/cats") {
                return json!({ "images": { "cat": { "url": "mxc://x.org/c" }, "dog": { "url": "mxc://x.org/d" } } });
            }
            if path.ends_with("/state/im.ponies.room_emotes/cats") {
                return json!({ "images": { "cat": { "url": "mxc://x.org/c" } } });
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let place = PackPlace::Room { room_id: "!room:example.org".into(), state_key: "cats".into() };
        let image = PackImageChange { shortcode: "dog".into(), body: String::new(), emoticon: true, sticker: true };
        let changed = account.edit_image_pack(&place, PackEdit::ChangeImage { shortcode: "cat".into(), image }).await;
        assert!(matches!(changed, Err(Error::ShortcodeTaken(_))));
        assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn an_older_room_pack_used_everywhere_should_be_named_in_the_older_list() {
        let kept = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<String, Value>::new()));
        let store = std::sync::Arc::clone(&kept);
        let server = crate::test_server::Server::new(move |request| {
            let path = path(request);
            if path.ends_with("/state/m.room.image_pack/cats") {
                return json!({ "errcode": "M_NOT_FOUND", "error": "none" });
            }
            if path.ends_with("/state/im.ponies.room_emotes/cats") {
                return json!({ "images": { "cat": { "url": "mxc://x.org/c" } } });
            }
            let Some((_, kind)) = path.split_once("/account_data/") else {
                return crate::test_server::thread_response(request);
            };
            let mut kept = store.lock().expect("kept");
            if request.starts_with("PUT") {
                kept.insert(kind.to_owned(), body(request));
                return json!({});
            }
            kept.get(kind).cloned().unwrap_or_else(|| json!({ "errcode": "M_NOT_FOUND", "error": "none" }))
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let place = PackPlace::Room { room_id: "!room:example.org".into(), state_key: "cats".into() };
        account.edit_image_pack(&place, PackEdit::UseEverywhere(true)).await.expect("used everywhere");
        let kept = kept.lock().expect("kept");
        assert_eq!(
            kept.get("im.ponies.emote_rooms"),
            Some(&json!({ "rooms": { "!room:example.org": { "cats": {} } } }))
        );
        assert!(!kept.contains_key("m.image_pack.rooms"));
    }

    #[test]
    fn an_added_image_should_take_a_code_free_in_every_copy() {
        let copies = [
            ("m.room.image_pack", json!({ "images": { "cat": { "url": "mxc://x.org/a" } } })),
            ("im.ponies.room_emotes", json!({ "images": { "cat_2": { "url": "mxc://x.org/b" } } })),
        ];
        let added = [(new_image(None), "mxc://x.org/c".to_owned(), json!({}), "Cat".to_owned())];
        assert_eq!(new_shortcodes(&copies, &added), ["cat_3"]);
    }

    #[tokio::test]
    async fn removing_an_image_should_leave_another_picture_under_its_code_in_the_other_copy() {
        let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(String, Value)>::new()));
        let writes = std::sync::Arc::clone(&written);
        let server = crate::test_server::Server::new(move |request| {
            let path = path(request);
            if request.starts_with("PUT") && path.contains("/state/") {
                writes.lock().expect("writes").push((path, body(request)));
                return json!({ "event_id": "$written" });
            }
            if path.ends_with("/state/m.room.image_pack/cats") {
                return json!({ "images": { "cat": { "url": "mxc://x.org/shown" } } });
            }
            if path.ends_with("/state/im.ponies.room_emotes/cats") {
                return json!({ "images": { "cat": { "url": "mxc://x.org/other" } } });
            }
            if path.ends_with("/state") {
                return json!([]);
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(std::time::Duration::ZERO))
            .await
            .expect("sync");
        let place = PackPlace::Room { room_id: "!room:example.org".into(), state_key: "cats".into() };
        account.edit_image_pack(&place, PackEdit::RemoveImage("cat".into())).await.expect("removed");
        let written = written.lock().expect("writes").clone();
        assert_eq!(written.len(), 1, "only the copy whose `cat` is the picture shown");
        assert!(written[0].0.contains("m.room.image_pack") && written[0].1["images"] == json!({}));
    }

    #[test]
    fn a_sticker_in_an_emoji_pack_should_widen_the_packs_use() {
        let mut content = json!({ "pack": { "usage": ["emoticon"] }, "images": { "a": { "url": "mxc://x.org/a" } } });
        add_image(&mut content, &new_image(Some("wave")), "mxc://x.org/w".into(), json!({}), "wave");
        widen_pack_usage(&mut content);
        assert_eq!(content["pack"]["usage"], json!(["emoticon", "sticker"]));
    }

    #[test]
    fn a_pack_naming_no_use_should_stay_so() {
        let mut content = json!({ "images": { "a": { "url": "mxc://x.org/a", "usage": ["sticker"] } } });
        widen_pack_usage(&mut content);
        assert!(content["pack"]["usage"].is_null());
    }
}
