//! Places: `geo:` URIs (RFC 5870), maps of them from OpenStreetMap, and finding places by name.
//!
//! - **Maps** are media like any other: a [`Location`](crate::model::Location) carries a
//!   [`MediaRef`] naming the place ([`map`]), and [`Command::FetchMedia`] draws it from
//!   OpenStreetMap's standard tiles (`tile.openstreetmap.org`), composed to the size asked for
//!   with a pin at the place, so apps fetch, cache and retry maps as they do pictures. Tiles are
//!   kept in memory (the last [`TILES_KEPT`]) and fetched two at a time, as OpenStreetMap's tile
//!   policy asks; requests name the app, never the user. Apps credit "© OpenStreetMap
//!   contributors" beside every map.
//! - **Search** goes to OpenStreetMap's Nominatim ([`search_places`]), only when the user asks
//!   (never as they type: its usage policy forbids autocomplete).
//!
//! [`Command::FetchMedia`]: crate::Command::FetchMedia

use std::sync::{Arc, Mutex, OnceLock};

use matrix_sdk::reqwest;

use crate::Error;
use crate::model::{MediaRef, MediaSize, Place};

/// What a map's [`MediaRef`] starts with: never a media source's JSON, which starts with `{`
/// or `"`.
const MAP_PREFIX: &str = "osm:";
/// How close maps are drawn: streets and their names.
pub const MAP_ZOOM: u8 = 15;
/// The size a map is drawn at when asked for the original.
const ORIGINAL_MAP: (u32, u32) = (640, 400);
/// A tile's side in pixels.
const TILE: u32 = 256;
/// How many decoded tiles stay in memory, shared by every map.
const TILES_KEPT: usize = 64;
/// Where tiles come from.
const TILE_SERVER: &str = "https://tile.openstreetmap.org";
/// Where places are looked up by name.
const SEARCH_SERVER: &str = "https://nominatim.openstreetmap.org";

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

/// The map of a place, to fetch like any media (any size: it is drawn to fit the box asked
/// for); `None` when the URI names no place.
pub fn map(geo_uri: &str) -> Option<MediaRef> {
    let (latitude, longitude) = coordinates(geo_uri)?;
    Some(MediaRef(format!("{MAP_PREFIX}{},{},{MAP_ZOOM}", trim_number(latitude), trim_number(longitude))))
}

/// The place on openstreetmap.org, to open in the browser.
///
/// ```
/// assert_eq!(
///     matrix_core::geo::web_link("geo:52.5163,13.3777").as_deref(),
///     Some("https://www.openstreetmap.org/?mlat=52.5163&mlon=13.3777#map=17/52.5163/13.3777")
/// );
/// ```
pub fn web_link(geo_uri: &str) -> Option<String> {
    let (latitude, longitude) = coordinates(geo_uri)?;
    let (latitude, longitude) = (trim_number(latitude), trim_number(longitude));
    Some(format!("https://www.openstreetmap.org/?mlat={latitude}&mlon={longitude}#map=17/{latitude}/{longitude}"))
}

/// A map's place and zoom, when `media` is a map.
fn map_of(media: &MediaRef) -> Option<(f64, f64, u8)> {
    let rest = media.0.strip_prefix(MAP_PREFIX)?;
    let mut parts = rest.split(',');
    let latitude = parts.next()?.parse().ok()?;
    let longitude = parts.next()?.parse().ok()?;
    let zoom = parts.next()?.parse().ok().filter(|zoom| *zoom <= 19)?;
    Some((latitude, longitude, zoom))
}

/// Whether `media` is a map, drawn by [`draw_map`] rather than fetched from a homeserver.
pub(crate) fn is_map(media: &MediaRef) -> bool {
    media.0.starts_with(MAP_PREFIX)
}

/// Where a place is in the world's pixels at `zoom`, Web Mercator as the tiles are drawn.
fn world_pixel(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    let size = f64::from(TILE) * f64::from(1u32 << zoom);
    let latitude = latitude.clamp(-85.051_128, 85.051_128).to_radians();
    let x = (longitude + 180.0) / 360.0 * size;
    let y = (1.0 - (latitude.tan() + 1.0 / latitude.cos()).ln() / std::f64::consts::PI) / 2.0 * size;
    (x, y)
}

/// The tiles a box `width`×`height` centred on a place covers, by column and row, with where
/// each goes in the box; columns wrap round the world, rows off it are left out.
fn tiles_for(latitude: f64, longitude: f64, zoom: u8, width: u32, height: u32) -> Vec<((u32, u32), (i64, i64))> {
    let (x, y) = world_pixel(latitude, longitude, zoom);
    #[expect(clippy::cast_possible_truncation, reason = "world pixels fit an i64 at every zoom")]
    let (left, top) = ((x - f64::from(width) / 2.0).floor() as i64, (y - f64::from(height) / 2.0).floor() as i64);
    let tile = i64::from(TILE);
    let count = 1i64 << zoom;
    let mut tiles = Vec::new();
    for row in top.div_euclid(tile)..=(top + i64::from(height) - 1).div_euclid(tile) {
        if !(0..count).contains(&row) {
            continue;
        }
        for column in left.div_euclid(tile)..=(left + i64::from(width) - 1).div_euclid(tile) {
            let wrapped = column.rem_euclid(count);
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "within 0..2^19")]
            let key = (wrapped as u32, row as u32);
            tiles.push((key, (column * tile - left, row * tile - top)));
        }
    }
    tiles
}

type Tile = Arc<image::RgbaImage>;

/// Decoded tiles by zoom, column and row, the newest last.
fn tile_cache() -> &'static Mutex<Vec<((u8, u32, u32), Tile)>> {
    static CACHE: OnceLock<Mutex<Vec<((u8, u32, u32), Tile)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

async fn tile(http: &reqwest::Client, zoom: u8, column: u32, row: u32) -> Result<Tile, String> {
    let key = (zoom, column, row);
    if let Ok(mut cache) = tile_cache().lock()
        && let Some(index) = cache.iter().position(|(k, _)| *k == key)
    {
        let entry = cache.remove(index);
        let tile = Arc::clone(&entry.1);
        cache.push(entry);
        return Ok(tile);
    }
    let url = format!("{TILE_SERVER}/{zoom}/{column}/{row}.png");
    let response = http.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("the map server answered {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let decoded = tokio::task::spawn_blocking(move || image::load_from_memory(&bytes).map(|image| image.into_rgba8()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let tile = Arc::new(decoded);
    if let Ok(mut cache) = tile_cache().lock() {
        cache.push((key, Arc::clone(&tile)));
        let excess = cache.len().saturating_sub(TILES_KEPT);
        cache.drain(..excess);
    }
    Ok(tile)
}

/// Draws the map `media` names as a PNG the size `size` asks for, a pin at its place.
pub(crate) async fn draw_map(http: &reqwest::Client, media: &MediaRef, size: MediaSize) -> Result<Vec<u8>, Error> {
    let (latitude, longitude, zoom) = map_of(media).ok_or(Error::MessageUnavailable)?;
    let (width, height) = match size {
        MediaSize::Original => ORIGINAL_MAP,
        MediaSize::Thumbnail { width, height } => (width.clamp(32, 2048), height.clamp(32, 2048)),
    };
    let placed = tiles_for(latitude, longitude, zoom, width, height);
    let mut fetched = Vec::with_capacity(placed.len());
    // Two at a time, as the tile policy asks.
    for pair in placed.chunks(2) {
        let tiles =
            futures_util::future::join_all(pair.iter().map(|((column, row), _)| tile(http, zoom, *column, *row)));
        for (result, (_, at)) in tiles.await.into_iter().zip(pair) {
            fetched.push((result.map_err(|e| Error::Storage(std::io::Error::other(e)))?, *at));
        }
    }
    tokio::task::spawn_blocking(move || compose(width, height, &fetched))
        .await
        .map_err(|e| Error::Storage(std::io::Error::other(e)))?
}

/// The tiles laid out in a box, the pin at its middle, as a PNG.
fn compose(width: u32, height: u32, tiles: &[(Tile, (i64, i64))]) -> Result<Vec<u8>, Error> {
    let mut map = image::RgbaImage::from_pixel(width, height, image::Rgba([224, 224, 224, 255]));
    for (tile, (x, y)) in tiles {
        image::imageops::overlay(&mut map, tile.as_ref(), *x, *y);
    }
    draw_pin(&mut map);
    let mut png = Vec::new();
    map.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| Error::Storage(std::io::Error::other(e)))?;
    Ok(png)
}

/// A map pin whose point is the box's middle: a red head with a white dot, over a soft shadow.
fn draw_pin(map: &mut image::RgbaImage) {
    let (cx, cy) = (f64::from(map.width()) / 2.0, f64::from(map.height()) / 2.0);
    let (head_y, radius) = (cy - 18.0, 10.0);
    let blend = |pixel: &mut image::Rgba<u8>, colour: [u8; 3], alpha: f64| {
        for (channel, value) in pixel.0.iter_mut().zip(colour) {
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a blend of two bytes")]
            let mixed = (f64::from(*channel) * (1.0 - alpha) + f64::from(value) * alpha).round() as u8;
            *channel = mixed;
        }
    };
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "pixel bounds of the box")]
    let (left, right, top, bottom) =
        ((cx - 16.0).max(0.0) as u32, (cx + 16.0) as u32, (head_y - 16.0).max(0.0) as u32, (cy + 6.0) as u32);
    for y in top..bottom.min(map.height()) {
        for x in left..right.min(map.width()) {
            let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let pixel = map.get_pixel_mut(x, y);
            // The shadow: a flat ellipse under the point.
            let shadow = ((px - cx) / 7.0).powi(2) + ((py - cy) / 2.5).powi(2);
            if shadow < 1.0 {
                blend(pixel, [0, 0, 0], 0.25 * (1.0 - shadow));
            }
            // The head, and the tail narrowing from it down to the point.
            let head = ((px - cx).powi(2) + (py - head_y).powi(2)).sqrt() - radius;
            let tail = if (head_y..=cy).contains(&py) {
                (px - cx).abs() - radius * 0.75 * (cy - py) / (cy - head_y)
            } else {
                f64::INFINITY
            };
            let edge = head.min(tail);
            if edge < 0.5 {
                let coverage = (0.5 - edge).clamp(0.0, 1.0);
                blend(pixel, [229, 72, 77], coverage);
                let dot = ((px - cx).powi(2) + (py - head_y).powi(2)).sqrt() - 3.5;
                if dot < 0.5 {
                    blend(pixel, [255, 255, 255], (0.5 - dot).clamp(0.0, 1.0));
                }
            }
        }
    }
}

/// Places whose name matches `query`, best first, up to eight, named in `language` (a BCP 47
/// tag) where OpenStreetMap knows the name in it.
pub(crate) async fn search_places(
    http: &reqwest::Client,
    query: &str,
    language: Option<&str>,
) -> Result<Vec<Place>, Error> {
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
            Some(Place { name: place.display_name, geo_uri: uri(latitude, longitude) })
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
    fn a_geo_uri_out_of_range_should_name_no_place() {
        assert_eq!(coordinates("geo:91,0"), None);
    }

    #[test]
    fn a_geo_uri_in_another_coordinate_system_should_name_no_place() {
        assert_eq!(coordinates("geo:1,2;crs=mars2000"), None);
    }

    #[test]
    fn a_map_should_name_its_place_and_zoom() {
        let media = map("geo:52.5163,13.3777;u=20").expect("a map");
        assert_eq!(map_of(&media), Some((52.5163, 13.3777, MAP_ZOOM)));
    }

    #[test]
    fn a_media_source_should_not_read_as_a_map() {
        assert!(!is_map(&MediaRef("{\"url\":\"mxc://x/y\"}".into())));
    }

    #[test]
    fn a_map_box_should_be_covered_by_the_tiles_around_its_place() {
        let tiles = tiles_for(0.0, 0.0, 1, 256, 256);
        // At zoom 1 the null island is where all four tiles meet.
        let keys: Vec<_> = tiles.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, [(0, 0), (1, 0), (0, 1), (1, 1)]);
        assert_eq!(tiles[0].1, (-128, -128));
    }

    #[test]
    fn tiles_past_the_date_line_should_wrap_round_the_world() {
        let tiles = tiles_for(0.0, 179.99, 2, 300, 10);
        assert!(tiles.iter().any(|((column, _), _)| *column == 0), "{tiles:?}");
    }

    #[test]
    fn a_composed_map_should_be_the_size_asked_for_with_the_pin_at_its_middle() {
        let tile = Arc::new(image::RgbaImage::from_pixel(256, 256, image::Rgba([200, 220, 200, 255])));
        let png = compose(120, 80, &[(tile, (-60, -60))]).expect("a map");
        let map = image::load_from_memory(&png).expect("a PNG").into_rgba8();
        assert_eq!(map.dimensions(), (120, 80));
        // The pin's head is red, above the middle.
        let head = map.get_pixel(60, 40 - 26);
        assert!(head.0[0] > 200 && head.0[1] < 120, "{head:?}");
    }

    /// Fetches a real map: needs the network.
    #[tokio::test]
    #[ignore = "contacts OpenStreetMap"]
    async fn a_map_should_be_drawn_from_openstreetmap() {
        let http = crate::links::http_client().expect("client");
        let media = map("geo:52.5163,13.3777").expect("a map");
        let png = draw_map(&http, &media, MediaSize::Thumbnail { width: 400, height: 240 }).await.expect("drawn");
        assert_eq!(image::load_from_memory(&png).expect("a PNG").into_rgba8().dimensions(), (400, 240));
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
