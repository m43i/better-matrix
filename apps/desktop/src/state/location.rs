//! Sharing a place: the location dialog (`Modal::Location`) finds a place by name on
//! OpenStreetMap (only when the user asks: Nominatim forbids searching as one types) or reads
//! typed coordinates, shows it on a map, and sends it as an `m.location` message.

use matrix_core::{Command, MediaSize, Outgoing, Place, RequestId};
use rune::{TextEdit, Tx};

use super::{ComposerTarget, Modal, Purpose, State};

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

    pub fn start_live_location(&mut self) {
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
}
