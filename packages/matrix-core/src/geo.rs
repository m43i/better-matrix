//! Shared places, GeoJS IP estimates and on-submit Nominatim searches.

use crate::Error;
use crate::model::{MediaRef, Place};
use matrix_sdk::reqwest;
use std::sync::OnceLock;

#[cfg(all(feature = "native-media", not(any(target_os = "android", target_os = "ios"))))]
pub(crate) mod desktop;
pub(crate) mod live;

const MAP_PREFIX: &str = "map:";
/// Street-level detail for a shared place.
pub const MAP_ZOOM: u8 = 15;
/// Where places are looked up by name.
const SEARCH_SERVER: &str = "https://nominatim.openstreetmap.org";
/// How many searches' answers are kept, so asking again asks no one.
const SEARCHES_KEPT: usize = 32;
/// The least time between two searches the app sends.
const SEARCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// The latitude and longitude a `geo:` URI names, in degrees; `None` when it names none
/// (another coordinate system, out of range, not a geo URI).
///
/// ```
/// assert_eq!(matrix_core::geo::coordinates("geo:51.5008,0.1247;u=35"), Some((51.5008, 0.1247)));
/// assert_eq!(matrix_core::geo::coordinates("https://example.org"), None);
/// ```
pub fn coordinates(geo_uri: &str) -> Option<(f64, f64)> {
    let rest = geo_uri.trim();
    let rest = rest.get(..4).filter(|scheme| scheme.eq_ignore_ascii_case("geo:")).map(|_| &rest[4..])?;
    let (point, params) = rest.split_once(';').unwrap_or((rest, ""));
    // Only WGS-84, the default, maps onto OpenStreetMap.
    let crs = params.split(';').find_map(|param| param.strip_prefix("crs=").or_else(|| param.strip_prefix("CRS=")));
    if crs.is_some_and(|crs| !crs.eq_ignore_ascii_case("wgs84")) {
        return None;
    }
    let mut parts = point.split(',');
    let latitude: f64 = parts.next()?.trim().parse().ok()?;
    let longitude: f64 = parts.next()?.trim().parse().ok()?;
    ((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) && latitude.is_finite())
        .then_some((latitude, longitude))
}

/// A `geo:` URI for a place, to six decimals (about 10 cm).
///
/// ```
/// assert_eq!(matrix_core::geo::uri(52.516275, 13.377704), "geo:52.516275,13.377704");
/// ```
pub fn uri(latitude: f64, longitude: f64) -> String {
    format!("geo:{},{}", trim_number(latitude), trim_number(longitude))
}

/// `value` to six decimals, without trailing zeros.
fn trim_number(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_owned() } else { text.to_owned() }
}

/// What a place without a name is called in a message's plain body: its coordinates.
///
/// ```
/// assert_eq!(matrix_core::geo::describe("geo:52.5163,13.3777"), "Location: 52.5163, 13.3777");
/// ```
pub fn describe(geo_uri: &str) -> String {
    match coordinates(geo_uri) {
        Some((latitude, longitude)) => format!("Location: {}, {}", trim_number(latitude), trim_number(longitude)),
        None => format!("Location: {geo_uri}"),
    }
}

/// A light street map of a geo URI. The UI chooses the style and zoom for display.
pub fn map(geo_uri: &str) -> Option<MediaRef> {
    styled_map(geo_uri, false, MAP_ZOOM)
}

/// The style and zoom are part of the preview cache key.
pub fn styled_map(geo_uri: &str, dark: bool, zoom: u8) -> Option<MediaRef> {
    let (latitude, longitude) = coordinates(geo_uri)?;
    Some(MediaRef(format!(
        "{MAP_PREFIX}{},{},{zoom},{}",
        trim_number(latitude),
        trim_number(longitude),
        if dark { "dark" } else { "light" }
    )))
}

/// A street map `width` × `height` points around `latitude`, `longitude`, for a map the user
/// moves: the size is part of the reference, so each size is its own picture.
///
/// ```
/// use matrix_core::geo::framed_map;
/// assert_ne!(framed_map(52.5, 13.4, 12, true, 800, 600), framed_map(52.5, 13.4, 12, true, 640, 400));
/// ```
pub fn framed_map(latitude: f64, longitude: f64, zoom: u8, dark: bool, width: u32, height: u32) -> MediaRef {
    MediaRef(format!(
        "{MAP_PREFIX}{},{},{zoom},{},{width}x{height}",
        trim_number(latitude),
        trim_number(longitude),
        if dark { "dark" } else { "light" }
    ))
}

/// The side of the world map at zoom 0, in points: MapLibre's tile size.
const WORLD: f64 = 512.0;

/// Where a place lies on the Web Mercator world map at `zoom`, in points from its top left
/// corner, as MapLibre draws it.
pub fn project(latitude: f64, longitude: f64, zoom: f64) -> (f64, f64) {
    let size = WORLD * zoom.exp2();
    let latitude = latitude.clamp(-85.051_128, 85.051_128).to_radians();
    let x = (longitude + 180.0) / 360.0 * size;
    let y = (1.0 - (latitude.tan() + 1.0 / latitude.cos()).ln() / std::f64::consts::PI) / 2.0 * size;
    (x, y)
}

/// The latitude and longitude at a point of the world map at `zoom`: [`project`] undone.
pub fn unproject(x: f64, y: f64, zoom: f64) -> (f64, f64) {
    let size = WORLD * zoom.exp2();
    let longitude = (x / size * 360.0 - 180.0).rem_euclid(360.0);
    let longitude = if longitude > 180.0 { longitude - 360.0 } else { longitude };
    let n = std::f64::consts::PI * (1.0 - 2.0 * y.clamp(0.0, size) / size);
    (n.sinh().atan().to_degrees(), longitude)
}

/// Open this place in Google Maps.
pub fn web_link(geo_uri: &str) -> Option<String> {
    let (lat, lon) = coordinates(geo_uri)?;
    Some(format!("https://www.google.com/maps/search/?api=1&query={},{}", trim_number(lat), trim_number(lon)))
}

pub(crate) async fn estimate(http: &reqwest::Client) -> Result<String, Error> {
    #[derive(serde::Deserialize)]
    struct Estimate {
        latitude: String,
        longitude: String,
    }
    let failed = |e: reqwest::Error| Error::Location(e.without_url().to_string());
    let body = http
        .get("https://get.geojs.io/v1/ip/geo.json")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .map_err(failed)?
        .error_for_status()
        .map_err(failed)?
        .bytes()
        .await
        .map_err(failed)?;
    let answer: Estimate = serde_json::from_slice(&body).map_err(|e| Error::Location(e.to_string()))?;
    let (lat, lon) =
        coordinates(&format!("geo:{},{}", answer.latitude, answer.longitude)).ok_or(Error::MessageUnavailable)?;
    Ok(uri(lat, lon))
}

/// Places whose name matches `query`, best first, up to eight, named in `language` (a BCP 47
/// tag) where OpenStreetMap knows the name in it.
pub(crate) async fn search_places(
    http: &reqwest::Client,
    query: &str,
    language: Option<&str>,
) -> Result<Vec<Place>, Error> {
    let asked = (query.trim().to_lowercase(), language.map(str::to_owned));
    // Held across the request: searches go one at a time, a second apart.
    let mut searches = searches().lock().await;
    if let Some((_, places)) = searches.answers.iter().find(|(known, _)| *known == asked) {
        return Ok(places.clone());
    }
    if let Some(last) = searches.last {
        tokio::time::sleep_until(last + SEARCH_INTERVAL).await;
    }
    searches.last = Some(tokio::time::Instant::now());
    let places = search_nominatim(http, query, language).await?;
    searches.answers.push((asked, places.clone()));
    let excess = searches.answers.len().saturating_sub(SEARCHES_KEPT);
    searches.answers.drain(..excess);
    Ok(places)
}

/// The searches sent: when the last went, and the answers kept by query and language.
#[derive(Default)]
struct Searches {
    last: Option<tokio::time::Instant>,
    answers: Vec<(Asked, Vec<Place>)>,
}

/// A search: its query (trimmed, lower case) and language.
type Asked = (String, Option<String>);

fn searches() -> &'static tokio::sync::Mutex<Searches> {
    static SEARCHES: OnceLock<tokio::sync::Mutex<Searches>> = OnceLock::new();
    SEARCHES.get_or_init(Default::default)
}

/// Places matching `query`, as Nominatim answers.
async fn search_nominatim(http: &reqwest::Client, query: &str, language: Option<&str>) -> Result<Vec<Place>, Error> {
    #[derive(serde::Deserialize)]
    struct Found {
        display_name: String,
        lat: String,
        lon: String,
    }
    let mut url =
        url::Url::parse(&format!("{SEARCH_SERVER}/search")).map_err(|e| Error::Storage(std::io::Error::other(e)))?;
    url.query_pairs_mut().append_pair("format", "jsonv2").append_pair("limit", "8").append_pair("q", query.trim());
    if let Some(language) = language {
        url.query_pairs_mut().append_pair("accept-language", language);
    }
    let failed = |e: reqwest::Error| Error::Storage(std::io::Error::other(e));
    let response = http.get(url).send().await.map_err(failed)?.error_for_status().map_err(failed)?;
    let body = response.bytes().await.map_err(failed)?;
    let found: Vec<Found> = serde_json::from_slice(&body).map_err(|e| Error::Storage(e.into()))?;
    Ok(found
        .into_iter()
        .filter_map(|place| {
            let (latitude, longitude) = (place.lat.parse().ok()?, place.lon.parse().ok()?);
            let geo_uri = uri(latitude, longitude);
            Some(Place { name: place.display_name, map: map(&geo_uri), geo_uri })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_geo_uri_with_an_altitude_should_still_name_its_place() {
        assert_eq!(coordinates("geo:-33.8688,151.2093,58"), Some((-33.8688, 151.2093)));
    }

    #[test]
    fn a_framed_map_should_name_its_size_after_its_style() {
        assert_eq!(framed_map(52.5, 13.4, 12, true, 800, 600).0, "map:52.5,13.4,12,dark,800x600");
    }

    #[test]
    fn a_projected_place_should_unproject_to_itself() {
        let (x, y) = project(52.516275, 13.377704, 15.0);
        let (latitude, longitude) = unproject(x, y, 15.0);
        assert!((latitude - 52.516275).abs() < 1e-9 && (longitude - 13.377704).abs() < 1e-9);
    }

    #[test]
    fn a_zoom_step_should_double_the_distance_between_places() {
        let distance = |zoom| project(52.5, 13.4, zoom).0 - project(52.5, 13.3, zoom).0;
        assert!((distance(11.0) - 2.0 * distance(10.0)).abs() < 1e-9);
    }

    #[test]
    fn the_equator_and_prime_meridian_should_lie_at_the_world_maps_centre() {
        assert_eq!(project(0.0, 0.0, 0.0), (256.0, 256.0));
    }

    #[test]
    fn a_geo_uri_out_of_range_should_name_no_place() {
        assert_eq!(coordinates("geo:91,0"), None);
    }

    #[test]
    fn a_geo_uri_in_another_coordinate_system_should_name_no_place() {
        assert_eq!(coordinates("geo:1,2;crs=mars2000"), None);
    }

    /// Searches OpenStreetMap: needs the network.
    #[tokio::test]
    #[ignore = "contacts OpenStreetMap"]
    async fn places_should_be_found_by_name() {
        let http = crate::links::http_client().expect("client");
        let places = search_places(&http, "Brandenburger Tor", Some("de")).await.expect("found");
        assert!(places.iter().any(|place| place.name.contains("Brandenburger Tor")), "{places:?}");
    }
}

#[cfg(test)]
mod estimate_tests {
    #[tokio::test]
    #[ignore = "contacts GeoJS for this connection's approximate location"]
    async fn an_ip_estimate_should_be_a_valid_location() {
        let estimate = super::estimate(&matrix_sdk::reqwest::Client::new()).await.unwrap();
        assert!(super::coordinates(&estimate).is_some());
    }
}
