//! Sharing a place: the location dialog (`Modal::Location`) finds a place by name on
//! OpenStreetMap (only when the user asks: Nominatim forbids searching as one types) or reads
//! typed coordinates, shows it on a map, and sends it as an `m.location` message.

use matrix_core::{
    AccountId, Command, LiveDuration, Location, MediaRef, MediaSize, Message, MessageContent, Outgoing, Place,
    RequestId, TimelineKey,
};
use rune::{TextEdit, Tx};

use super::{ComposerTarget, Modal, Purpose, State};

/// How long a place stays on the map viewer after it was shared, unless it is still live.
const RECENT_MS: u64 = 24 * 60 * 60 * 1000;
/// How far out and in the map viewer goes.
pub const MAP_ZOOMS: std::ops::RangeInclusive<u8> = 2..=19;
/// Wheel travel, in points, that makes one zoom step on a trackpad.
const WHEEL_STEP: f32 = 80.0;

/// The map viewer (`Modal::LocationMap`): every recent place shared in one conversation, on one
/// map the user drags and zooms.
#[derive(Clone, Debug, PartialEq)]
pub struct MapViewer {
    pub account: AccountId,
    /// The conversation (room or thread) whose places it shows.
    pub key: TimelineKey,
    /// The card it opened from: its place shows however old.
    pub opened: String,
    /// Whom it is about, chosen on the map since (a sender's user ID); the opened card's sender
    /// until then.
    pub chosen: Option<String>,
    pub camera: Camera,
    pub zoom: u8,
    /// How far the map is being dragged, in points; zero between drags.
    pub drag: (f32, f32),
    /// Trackpad wheel travel not yet a whole zoom step.
    pub wheel: f32,
    /// The last picture of the map drawn: shown moved and scaled into place until the camera's
    /// own arrives, so the map never goes blank while it moves.
    pub drawn: Option<MapFrame>,
}

/// Where the map viewer centres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Camera {
    /// On the focused place, wherever a live share moves it.
    Following,
    /// Where the user moved the map.
    Free { latitude: f64, longitude: f64 },
}

/// One picture of the map: its centre, zoom and size in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapFrame {
    pub latitude: f64,
    pub longitude: f64,
    pub zoom: u8,
    pub width: u32,
    pub height: u32,
}

impl MapFrame {
    /// The picture, in the light or dark style.
    pub fn media(&self, dark: bool) -> MediaRef {
        matrix_core::geo::framed_map(self.latitude, self.longitude, self.zoom, dark, self.width, self.height)
    }

    /// A key for the element that draws or fetches this picture.
    pub fn key(&self, dark: bool) -> String {
        let Self { latitude, longitude, zoom, width, height } = self;
        format!("{latitude},{longitude},{zoom},{width}x{height},{dark}")
    }
}

/// A place on the map viewer: the message that shared it, and where it is.
#[derive(Clone, Copy, Debug)]
pub struct MapPin<'a> {
    pub message: &'a Message,
    pub location: &'a Location,
    /// Latitude and longitude.
    pub at: (f64, f64),
}

impl MapPin<'_> {
    /// Still sharing live at `now`.
    pub fn live(&self, now: u64) -> bool {
        self.location.live_until.is_some_and(|until| until > now)
    }
}

/// The places a map viewer shows, newest first: each sender's newest place in the conversation
/// that is a live share still running or a place shared within a day; and the focused message's
/// place in place of its sender's, unless it is a live share that has ended (then their newest
/// stands, or it shows alone when they have none).
pub fn map_pins<'a>(messages: impl IntoIterator<Item = &'a Message>, focus: &str, now: u64) -> Vec<MapPin<'a>> {
    let mut pins: Vec<MapPin<'a>> = Vec::new();
    let mut focused = None;
    for message in messages {
        let MessageContent::Location(location) = &message.content else { continue };
        let Some(at) = matrix_core::geo::coordinates(&location.geo_uri) else { continue };
        let pin = MapPin { message, location, at };
        if message.event_id == focus {
            focused = Some(pin);
        }
        // A live share stays while it runs and goes once it ends: its last fix is no longer
        // where they are.
        let recent = match location.live_until {
            Some(_) => pin.live(now),
            None => message.timestamp_ms.saturating_add(RECENT_MS) >= now,
        };
        if !recent {
            continue;
        }
        match pins.iter_mut().find(|known| known.message.sender_id == message.sender_id) {
            Some(known) if message.timestamp_ms > known.message.timestamp_ms => *known = pin,
            Some(_) => {}
            None => pins.push(pin),
        }
    }
    if let Some(pin) = focused {
        let ended = pin.location.live_until.is_some() && !pin.live(now);
        match pins.iter_mut().find(|known| known.message.sender_id == pin.message.sender_id) {
            Some(known) if !ended => *known = pin,
            Some(_) => {}
            None => pins.push(pin),
        }
    }
    pins.sort_by_key(|pin| std::cmp::Reverse(pin.message.timestamp_ms));
    pins
}

/// The size the dialog's map and the timeline's location cards are drawn at.
const MAP_SIZE: MediaSize = MediaSize::Thumbnail { width: 480, height: 284 };
/// [`MAP_SIZE`] in pixels.
const MAP_PIXELS: (u32, u32) = (480, 284);

/// The location dialog's state.
#[derive(Debug, Default)]
pub struct LocationForm {
    /// The composer the place goes from.
    pub target: Option<ComposerTarget>,
    /// What to look for: a place's name, or coordinates.
    pub search: TextEdit,
    /// The search on its way.
    pub searching: Option<RequestId>,
    /// What the last search found; `None` before any.
    pub places: Option<Vec<Place>>,
    /// The place chosen to send.
    pub chosen: Option<Place>,
    /// A position lookup belongs only to the dialog that launched it.
    pub locating: Option<RequestId>,
    /// Whether the chosen place is a device fix or an IP estimate.
    pub estimated: Option<bool>,
    #[cfg(target_os = "windows")]
    consent: Option<windows_future::IAsyncOperation<windows::Devices::Geolocation::GeolocationAccessStatus>>,
    /// Why the last search failed.
    pub error: Option<String>,
}

/// A place from typed coordinates (`52.5163, 13.3777`, or a `geo:` URI).
pub fn typed_place(text: &str) -> Option<Place> {
    let text = text.trim();
    let uri = if text.get(..4).is_some_and(|scheme| scheme.eq_ignore_ascii_case("geo:")) {
        text.to_owned()
    } else {
        format!("geo:{}", text.split_whitespace().collect::<String>())
    };
    let (latitude, longitude) = matrix_core::geo::coordinates(&uri)?;
    let geo_uri = matrix_core::geo::uri(latitude, longitude);
    Some(Place { name: matrix_core::geo::describe(&geo_uri), map: matrix_core::geo::map(&geo_uri), geo_uri })
}

impl State {
    pub fn watch_location_expiry(&mut self, until: u64, tx: &mut Tx) {
        self.location_expiries.insert(until);
        tx.wake_after(std::time::Duration::from_millis(until.saturating_sub(super::now_ms())));
        tx.live_only();
    }

    pub(super) fn expire_locations(&mut self, now: u64, tx: &mut Tx) -> bool {
        let mut changed = false;
        while let Some(&until) = self.location_expiries.first() {
            if until > now {
                tx.wake_after(std::time::Duration::from_millis(until - now));
                break;
            }
            self.location_expiries.pop_first();
            changed = true;
        }
        changed
    }

    /// Opens the location dialog for a composer.
    pub fn open_location(&mut self, target: ComposerTarget) {
        self.popover = None;
        let mut search = TextEdit::default();
        search.focused = true;
        self.location = LocationForm { target: Some(target), search, ..LocationForm::default() };
        self.modal = Some(Modal::Location);
        self.locate_position(false);
    }

    pub fn requesting_position(&self) -> bool {
        #[cfg(target_os = "windows")]
        if self.location.consent.is_some() {
            return true;
        }
        self.location.locating.is_some()
    }

    pub fn request_device_position(&mut self, tx: &mut Tx) {
        #[cfg(target_os = "windows")]
        match windows::Devices::Geolocation::Geolocator::RequestAccessAsync() {
            Ok(op) => {
                self.location.consent = Some(op);
                tx.wake_after(std::time::Duration::ZERO);
            }
            Err(error) => self.location.error = Some(error.to_string()),
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = tx;
            self.locate_position(true);
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn poll_location_consent(&mut self, tx: &mut Tx) -> bool {
        let Some(op) = &self.location.consent else { return false };
        if op.Status() == Ok(windows_future::AsyncStatus::Started) {
            tx.wake_after(std::time::Duration::from_millis(100));
            return false;
        }
        let granted = op.GetResults() == Ok(windows::Devices::Geolocation::GeolocationAccessStatus::Allowed);
        self.location.consent = None;
        if matches!(self.modal, Some(Modal::Location)) {
            if granted {
                self.locate_position(true);
            } else {
                self.location.error = Some(crate::i18n::t!("location.no_fix").into());
            }
        }
        true
    }

    fn locate_position(&mut self, request_access: bool) {
        self.location.error = None;
        self.location.searching = None;
        self.location.locating = Some(self.dispatch(Command::Locate { request_access }, Purpose::Position));
    }

    pub(super) fn position_found(&mut self, request: RequestId, geo_uri: String, estimated: bool) {
        if self.location.locating != Some(request) || !matches!(self.modal, Some(Modal::Location)) {
            return;
        }
        let name = if estimated { crate::i18n::t!("location.estimated") } else { crate::i18n::t!("location.current") };
        self.choose_place(Place { name: name.into(), map: matrix_core::geo::map(&geo_uri), geo_uri });
        self.location.estimated = Some(estimated);
    }

    pub(super) fn position_failed(&mut self, request: RequestId, error: String) {
        if self.location.locating == Some(request) {
            self.location.locating = None;
            self.location.error = Some(error);
        }
    }

    pub fn live_share(
        &self,
        account: &matrix_core::AccountId,
        room_id: &str,
        event_id: Option<&str>,
    ) -> Option<RequestId> {
        self.pending.iter().find_map(|(id, purpose)| {
            matches!(purpose, Purpose::LiveShare(owner, room, beacon) if owner == account && room == room_id && event_id.is_none_or(|id| beacon.as_deref() == Some(id))).then_some(*id)
        })
    }

    /// Opens the map viewer on a shared place's message.
    pub fn open_location_map(&mut self, account: AccountId, key: TimelineKey, event_id: String) {
        self.modal = Some(Modal::LocationMap(Box::new(MapViewer {
            account,
            key,
            opened: event_id,
            chosen: None,
            camera: Camera::Following,
            zoom: matrix_core::geo::MAP_ZOOM,
            drag: (0.0, 0.0),
            wheel: 0.0,
            drawn: None,
        })));
    }

    pub fn map_viewer(&self) -> Option<&MapViewer> {
        match &self.modal {
            Some(Modal::LocationMap(viewer)) => Some(viewer),
            _ => None,
        }
    }

    fn map_viewer_mut(&mut self) -> Option<&mut MapViewer> {
        match &mut self.modal {
            Some(Modal::LocationMap(viewer)) => Some(viewer),
            _ => None,
        }
    }

    /// The places the map viewer shows, none from someone the user ignores.
    pub fn map_pins(&self, viewer: &MapViewer, now: u64) -> Vec<MapPin<'_>> {
        let Some(data) = self.accounts.get(&viewer.account) else { return Vec::new() };
        let Some(timeline) = data.timeline(&viewer.key) else { return Vec::new() };
        map_pins(timeline.messages().filter(|message| !data.ignores(&message.sender_id)), &viewer.opened, now)
    }

    /// Whom the map viewer is about: the person chosen on it, else the opened card's sender.
    /// Their pin is the focused one, following them from an ended live share to a newer place.
    pub fn map_focus<'s>(&'s self, viewer: &'s MapViewer) -> Option<&'s str> {
        if let Some(chosen) = &viewer.chosen {
            return Some(chosen);
        }
        let timeline = self.accounts.get(&viewer.account)?.timeline(&viewer.key)?;
        timeline.messages().find(|message| message.event_id == viewer.opened).map(|message| message.sender_id.as_str())
    }

    /// Where the map viewer centres (latitude, longitude): on the focused person, or, once their
    /// place has left the map (a live share ended), on the newest place still on it; `None` while
    /// none is.
    pub fn map_center(&self, viewer: &MapViewer) -> Option<(f64, f64)> {
        match viewer.camera {
            Camera::Free { latitude, longitude } => Some((latitude, longitude)),
            Camera::Following => {
                let focus = self.map_focus(viewer);
                let pins = self.map_pins(viewer, super::now_ms());
                let focused = pins.iter().find(|pin| Some(pin.message.sender_id.as_str()) == focus);
                focused.or(pins.first()).map(|pin| pin.at)
            }
        }
    }

    /// Turns the map viewer to someone (a sender's user ID): centred on their place, at street
    /// level.
    pub fn focus_map_pin(&mut self, sender: String) {
        if let Some(viewer) = self.map_viewer_mut() {
            viewer.chosen = Some(sender);
            viewer.camera = Camera::Following;
            viewer.zoom = matrix_core::geo::MAP_ZOOM;
        }
    }

    /// Centres the map viewer on its focused place again, at the zoom it has.
    pub fn recenter_map(&mut self) {
        if let Some(viewer) = self.map_viewer_mut() {
            viewer.camera = Camera::Following;
        }
    }

    /// Zooms the map viewer by `step`, keeping the point `toward` (in points from the map's centre)
    /// where it is, as a wheel over the map does; the centre when `toward` is zero.
    pub fn zoom_map(&mut self, step: i32, toward: (f32, f32)) {
        let Some(viewer) = self.map_viewer() else { return };
        let (low, high) = (i32::from(*MAP_ZOOMS.start()), i32::from(*MAP_ZOOMS.end()));
        let zoom = u8::try_from((i32::from(viewer.zoom) + step).clamp(low, high)).unwrap_or(viewer.zoom);
        let camera = match self.map_center(viewer) {
            Some((latitude, longitude)) if toward != (0.0, 0.0) => {
                let (dx, dy) = (f64::from(toward.0), f64::from(toward.1));
                let (x, y) = matrix_core::geo::project(latitude, longitude, f64::from(viewer.zoom));
                let (lat, lon) = matrix_core::geo::unproject(x + dx, y + dy, f64::from(viewer.zoom));
                let (x, y) = matrix_core::geo::project(lat, lon, f64::from(zoom));
                let (latitude, longitude) = matrix_core::geo::unproject(x - dx, y - dy, f64::from(zoom));
                Camera::Free { latitude, longitude }
            }
            _ => viewer.camera,
        };
        if let Some(viewer) = self.map_viewer_mut() {
            viewer.zoom = zoom;
            viewer.camera = camera;
        }
    }

    /// A wheel over the map viewer: a notch is a zoom step; a trackpad's travel adds up to one.
    /// `toward` is the pointer, in points from the map's centre.
    pub fn wheel_map(&mut self, dy: f32, precise: bool, toward: (f32, f32)) -> bool {
        let Some(viewer) = self.map_viewer_mut() else { return false };
        let step = if precise {
            viewer.wheel += dy;
            let step = (viewer.wheel / WHEEL_STEP).trunc();
            viewer.wheel -= step * WHEEL_STEP;
            step as i32
        } else {
            dy.signum() as i32
        };
        if step != 0 {
            self.zoom_map(step, toward);
        }
        true
    }

    /// A drag on the map viewer: the map follows the pointer, and stays where it is let go.
    pub fn drag_map(&mut self, phase: rune::Phase, dx: f32, dy: f32) {
        let center = self.map_viewer().and_then(|viewer| Some((self.map_center(viewer)?, viewer.zoom)));
        let Some(viewer) = self.map_viewer_mut() else { return };
        match phase {
            rune::Phase::Down => viewer.drag = (0.0, 0.0),
            rune::Phase::Move => viewer.drag = (dx, dy),
            rune::Phase::Up => {
                viewer.drag = (0.0, 0.0);
                if let Some(((latitude, longitude), zoom)) = center
                    && (dx, dy) != (0.0, 0.0)
                {
                    let (x, y) = matrix_core::geo::project(latitude, longitude, f64::from(zoom));
                    let (latitude, longitude) =
                        matrix_core::geo::unproject(x - f64::from(dx), y - f64::from(dy), f64::from(zoom));
                    viewer.camera = Camera::Free { latitude, longitude };
                }
            }
        }
    }

    /// The map viewer drew `frame`: it stands in for the next picture until that arrives.
    pub fn map_drawn(&mut self, frame: MapFrame) {
        if let Some(viewer) = self.map_viewer_mut() {
            viewer.drawn = Some(frame);
        }
    }

    pub fn start_live_location(&mut self, duration: LiveDuration) {
        let (Some(account), Some(target), Some(place)) =
            (self.nav.account.clone(), self.location.target, self.location.chosen.as_ref())
        else {
            return;
        };
        let Some(key) = self.composer_key(target) else { return };
        if key.thread.is_some()
            || self.location.estimated != Some(false)
            || self.live_share(&account, &key.room_id, None).is_some()
            || !self
                .accounts
                .get(&account)
                .and_then(|data| data.room(&key.room_id))
                .is_some_and(|room| room.can_share_live_location)
        {
            return;
        }
        self.dispatch(
            Command::ShareLiveLocation {
                account: account.clone(),
                room_id: key.room_id.clone(),
                geo_uri: place.geo_uri.clone(),
                duration,
            },
            Purpose::LiveShare(account, key.room_id, None),
        );
        self.close_modal();
    }

    pub fn stop_live_location(&mut self, request: RequestId) {
        self.dispatch(Command::Cancel(request), Purpose::Cancel);
    }

    pub fn request_map(&mut self, account: &matrix_core::AccountId, map: matrix_core::MediaRef) {
        if self.settings.show_maps {
            self.request_media(account, vec![(map, MAP_SIZE, Some(MAP_PIXELS))]);
        }
    }

    /// Fetches one picture of the map viewer's map.
    pub fn request_map_frame(&mut self, account: &matrix_core::AccountId, frame: MapFrame, dark: bool) {
        if self.settings.show_maps {
            let (width, height) = (frame.width, frame.height);
            self.request_media(
                account,
                vec![(frame.media(dark), MediaSize::Thumbnail { width, height }, Some((width, height)))],
            );
        }
    }

    /// Looks up what the search field says: coordinates are taken at once, a name is searched
    /// for on OpenStreetMap.
    pub fn search_places(&mut self) {
        let query = self.location.search.text.trim().to_owned();
        if query.is_empty() {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            self.location.consent = None;
        }
        self.location.locating = None;
        if let Some(place) = typed_place(&query) {
            self.location.places = Some(vec![place.clone()]);
            self.choose_place(place);
            return;
        }
        let language = crate::i18n::I18N.locales()[self.settings.language.resolve()].id().to_owned();
        let request = self.dispatch(Command::SearchPlaces { query, language: Some(language) }, Purpose::Places);
        self.location.searching = Some(request);
        self.location.error = None;
    }

    /// Places found for the dialog's search.
    pub(super) fn places_found(&mut self, request: RequestId, places: Vec<Place>) {
        if self.location.searching != Some(request) {
            return;
        }
        self.location.searching = None;
        let first = places.first().cloned();
        self.location.places = Some(places);
        if let Some(place) = first {
            self.choose_place(place);
        }
    }

    /// The dialog's search failed.
    pub(super) fn places_failed(&mut self, request: RequestId, error: String) {
        if self.location.searching == Some(request) {
            self.location.searching = None;
            self.location.error = Some(error);
        }
    }

    /// Picks the place to send, and fetches its map.
    pub fn choose_place(&mut self, place: Place) {
        #[cfg(target_os = "windows")]
        {
            self.location.consent = None;
        }
        self.location.locating = None;
        self.location.searching = None;
        self.location.estimated = None;
        self.location.error = None;
        self.location.chosen = Some(place);
    }

    /// Sends the chosen place from the dialog's composer and closes the dialog.
    pub fn send_location(&mut self, tx: &mut Tx) {
        let (Some(target), Some(place)) = (self.location.target, self.location.chosen.clone()) else { return };
        self.modal = None;
        self.location = LocationForm::default();
        // A place found by coordinates is named by them, which the message says anyway.
        let description = (matrix_core::geo::describe(&place.geo_uri) != place.name).then_some(place.name);
        let reply = self.composer_mut(target).reply_to.take();
        self.send_from(target, reply, vec![Outgoing::Location { geo_uri: place.geo_uri, description }], tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_coordinates_should_be_a_place() {
        let place = typed_place("52.5163, 13.3777").expect("a place");
        assert_eq!(place.geo_uri, "geo:52.5163,13.3777");
    }

    #[test]
    fn a_typed_geo_uri_should_be_a_place() {
        assert!(typed_place("geo:52.5163,13.3777;u=20").is_some());
    }

    #[test]
    fn a_name_should_not_read_as_coordinates() {
        assert!(typed_place("Brandenburger Tor").is_none());
    }

    const NOW: u64 = 10 * RECENT_MS;
    const MINUTE: u64 = 60_000;

    fn shared(id: &str, sender: &str, ago: u64, live_until: Option<u64>) -> Message {
        Message {
            event_id: id.into(),
            sender_id: sender.into(),
            timestamp_ms: NOW - ago,
            content: MessageContent::Location(Location {
                live_until,
                geo_uri: "geo:52.5,13.4".into(),
                description: None,
                map: None,
            }),
            ..Message::default()
        }
    }

    fn ids(pins: &[MapPin<'_>]) -> Vec<String> {
        pins.iter().map(|pin| pin.message.event_id.clone()).collect()
    }

    #[test]
    fn the_map_should_show_only_each_senders_newest_place_newest_first() {
        let messages = [
            shared("$a-old", "@a:x", 30 * MINUTE, None),
            shared("$b", "@b:x", 20 * MINUTE, None),
            shared("$a", "@a:x", 5 * MINUTE, None),
        ];
        assert_eq!(ids(&map_pins(&messages, "$b", NOW)), ["$a", "$b"]);
    }

    #[test]
    fn a_place_shared_over_a_day_ago_should_leave_the_map() {
        let messages = [shared("$old", "@a:x", RECENT_MS + MINUTE, None), shared("$b", "@b:x", MINUTE, None)];
        assert_eq!(ids(&map_pins(&messages, "$b", NOW)), ["$b"]);
    }

    #[test]
    fn a_live_share_should_stay_on_the_map_however_long_ago_it_began() {
        let messages = [shared("$live", "@a:x", 2 * RECENT_MS, Some(NOW + MINUTE)), shared("$b", "@b:x", MINUTE, None)];
        assert_eq!(ids(&map_pins(&messages, "$b", NOW)), ["$b", "$live"]);
    }

    #[test]
    fn a_live_share_that_ended_should_leave_the_map_at_once() {
        let messages = [
            shared("$expired", "@a:x", 2 * 60 * MINUTE, Some(NOW - 60 * MINUTE)),
            shared("$stopped", "@c:x", 10 * MINUTE, Some(0)),
            shared("$b", "@b:x", MINUTE, None),
        ];
        assert_eq!(ids(&map_pins(&messages, "$b", NOW)), ["$b"]);
    }

    #[test]
    fn an_ended_live_share_should_give_way_to_its_senders_newer_place() {
        let messages = [
            shared("$first", "@a:x", 60 * MINUTE, Some(NOW - MINUTE)),
            shared("$renewed", "@a:x", 2 * MINUTE, Some(NOW + MINUTE)),
        ];
        assert_eq!(ids(&map_pins(&messages, "$first", NOW)), ["$renewed"]);
    }

    #[test]
    fn an_ended_live_share_should_still_show_when_its_sender_has_nothing_newer() {
        let messages = [shared("$ended", "@a:x", 60 * MINUTE, Some(NOW - MINUTE))];
        assert_eq!(ids(&map_pins(&messages, "$ended", NOW)), ["$ended"]);
    }

    #[test]
    fn the_focused_place_should_show_in_place_of_its_senders_newer_one() {
        let messages = [shared("$focus", "@a:x", 3 * RECENT_MS, None), shared("$newer", "@a:x", MINUTE, None)];
        assert_eq!(ids(&map_pins(&messages, "$focus", NOW)), ["$focus"]);
    }

    #[test]
    fn a_message_without_coordinates_should_not_be_a_pin() {
        let mut message = shared("$a", "@a:x", MINUTE, None);
        if let MessageContent::Location(location) = &mut message.content {
            location.geo_uri = "geo:1,2;crs=mars2000".into();
        }
        assert!(map_pins([&message], "$a", NOW).is_empty());
    }
}
