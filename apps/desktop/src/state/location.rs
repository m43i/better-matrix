//! Sharing a place: the location dialog (`Modal::Location`) finds a place by name on
//! OpenStreetMap (only when the user asks: Nominatim forbids searching as one types) or reads
//! typed coordinates, shows it on a map, and sends it as an `m.location` message.

use matrix_core::{Command, MediaSize, Outgoing, Place, RequestId};
use rune::{TextEdit, Tx};

use super::{ComposerTarget, Modal, Purpose, State};

/// The size the dialog's map and the timeline's location cards are drawn at.
pub const MAP_SIZE: MediaSize = MediaSize::Thumbnail { width: 480, height: 240 };
/// [`MAP_SIZE`] in pixels.
pub const MAP_PIXELS: (u32, u32) = (480, 240);

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
    /// Opens the location dialog for a composer.
    pub fn open_location(&mut self, target: ComposerTarget) {
        self.popover = None;
        let mut search = TextEdit::default();
        search.focused = true;
        self.location = LocationForm { target: Some(target), search, ..LocationForm::default() };
        self.modal = Some(Modal::Location);
    }

    /// Looks up what the search field says: coordinates are taken at once, a name is searched
    /// for on OpenStreetMap.
    pub fn search_places(&mut self) {
        let query = self.location.search.text.trim().to_owned();
        if query.is_empty() {
            return;
        }
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
        if let (Some(account), Some(map)) = (self.nav.account.clone(), place.map.clone()) {
            self.request_media(&account, vec![(map, MAP_SIZE, Some(MAP_PIXELS))]);
        }
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
