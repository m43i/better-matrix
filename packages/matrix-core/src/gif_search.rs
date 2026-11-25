//! GIF search on GIPHY, for the GIF picker. Each user brings their own API key (the apps keep
//! it); GIPHY sees the search, the user's address and, when one is sent, which GIF.
//!
//! As GIPHY asks: its URLs are used exactly as it gives them, its order is kept, a sent GIF is
//! reported to it (`analytics.onsent`, with a random ID that lasts the run), and the apps show
//! "Powered by GIPHY" wherever its GIFs are browsed.

use std::sync::OnceLock;

use matrix_sdk::reqwest;
use serde::Deserialize;

use crate::Error;
use crate::model::{FoundGif, GifPage};

const API: &str = "https://api.giphy.com/v1/gifs";
/// GIFs a page holds.
pub const PAGE: u32 = 24;
/// The longest query GIPHY takes, in characters.
const MAX_QUERY: usize = 50;
/// The furthest offsets GIPHY serves: searches, then trending.
const MAX_SEARCH_OFFSET: u32 = 4999;
const MAX_TRENDING_OFFSET: u32 = 499;
/// The most GIPHY rates a GIF that may show.
const RATING: &str = "pg";

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    data: Vec<Gif>,
    #[serde(default)]
    pagination: Pagination,
}

#[derive(Deserialize, Default)]
struct Pagination {
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    count: u32,
    total_count: Option<u32>,
}

#[derive(Deserialize)]
struct Gif {
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    alt_text: Option<String>,
    #[serde(default)]
    images: Images,
    #[serde(default)]
    analytics: Option<Analytics>,
}

#[derive(Deserialize, Default)]
struct Images {
    fixed_width: Option<Rendition>,
    fixed_width_small: Option<Rendition>,
}

#[derive(Deserialize)]
struct Rendition {
    #[serde(default)]
    url: String,
    #[serde(default)]
    width: Option<String>,
    #[serde(default)]
    height: Option<String>,
}

#[derive(Deserialize)]
struct Analytics {
    onsent: Option<Ping>,
}

#[derive(Deserialize)]
struct Ping {
    url: String,
}

/// A page of GIFs: those matching `query` from `offset`, or trending ones for a blank query,
/// rated for everyone but young children. `language` (ISO 639-1) helps GIPHY read the query.
pub(crate) async fn search(
    http: &reqwest::Client,
    key: &str,
    query: &str,
    offset: u32,
    language: Option<&str>,
) -> Result<GifPage, Error> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::NoGifKey);
    }
    let query: String = query.trim().chars().take(MAX_QUERY).collect();
    let (endpoint, max_offset) =
        if query.is_empty() { ("trending", MAX_TRENDING_OFFSET) } else { ("search", MAX_SEARCH_OFFSET) };
    let failed = |reason: String| Error::GifSearch(reason);
    let mut url = url::Url::parse(&format!("{API}/{endpoint}")).map_err(|e| failed(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("api_key", key)
        .append_pair("limit", &PAGE.to_string())
        .append_pair("offset", &offset.min(max_offset).to_string())
        .append_pair("rating", RATING);
    if !query.is_empty() {
        url.query_pairs_mut().append_pair("q", &query);
        if let Some(language) = language.and_then(|tag| tag.split(['-', '_']).next()).filter(|tag| tag.len() == 2) {
            url.query_pairs_mut().append_pair("lang", language);
        }
    }
    let response = http.get(url).send().await.map_err(|e| failed(e.without_url().to_string()))?;
    match response.status().as_u16() {
        401 | 403 => return Err(Error::GifKeyRefused),
        429 => return Err(failed("too many searches; try again later".into())),
        _ => {}
    }
    let response = response.error_for_status().map_err(|e| failed(e.without_url().to_string()))?;
    let body = response.bytes().await.map_err(|e| failed(e.without_url().to_string()))?;
    page(&body, max_offset)
}

/// A page from GIPHY's answer.
fn page(body: &[u8], max_offset: u32) -> Result<GifPage, Error> {
    let answer: Answer = serde_json::from_slice(body).map_err(|e| Error::GifSearch(e.to_string()))?;
    let gifs: Vec<FoundGif> = answer.data.into_iter().filter_map(found).collect();
    let Pagination { offset, count, total_count } = answer.pagination;
    let next = offset + count;
    let more = count > 0 && next <= max_offset && total_count.is_none_or(|total| next < total);
    Ok(GifPage { gifs, next: more.then_some(next) })
}

/// A GIF the picker can show and send: one with a page and a preview.
fn found(gif: Gif) -> Option<FoundGif> {
    let preview = gif.images.fixed_width.or(gif.images.fixed_width_small).filter(|preview| !preview.url.is_empty())?;
    if gif.url.is_empty() {
        return None;
    }
    let side =
        |side: &Option<String>| side.as_deref().and_then(|side| side.parse::<u32>().ok()).filter(|side| *side > 0);
    let title = gif.alt_text.filter(|alt| !alt.trim().is_empty()).unwrap_or(gif.title);
    Some(FoundGif {
        id: gif.id,
        title: title.trim().to_owned(),
        url: gif.url,
        preview_size: side(&preview.width).zip(side(&preview.height)),
        preview: preview.url,
        sent_ping: gif.analytics.and_then(|analytics| analytics.onsent).map(|ping| ping.url),
    })
}

/// Tells GIPHY a GIF was sent, as it asks; a failure changes nothing.
pub(crate) async fn report_sent(http: &reqwest::Client, gif: &FoundGif) {
    let Some(ping) = gif.sent_ping.as_deref() else { return };
    let Ok(mut url) = url::Url::parse(ping) else { return };
    if url.scheme() != "https" {
        return;
    }
    url.query_pairs_mut()
        .append_pair("ts", &crate::engine::now_ms().to_string())
        .append_pair("customer_id", customer_id());
    let _ = http.get(url).send().await;
}

/// The random ID GIPHY's reports carry, new each run: it ties a run's reports together, not
/// the user's runs.
fn customer_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let mut bytes = [0u8; 16];
        let _ = getrandom::fill(&mut bytes);
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "url": format!("https://giphy.com/gifs/cat-{id}"), "title": "Cat GIF", "alt_text": "A cat waves",
            "images": { "fixed_width": { "url": format!("https://media.giphy.com/media/{id}/200w.gif?cid=x&ct=g"), "width": "200", "height": "150" } },
            "analytics": { "onsent": { "url": "https://giphy-analytics.giphy.com/v2/pingback_simple?analytics_response_payload=p&action_type=SENT" } }
        })
    }

    #[test]
    fn a_found_gif_should_keep_giphys_urls_as_given() {
        let body =
            serde_json::json!({ "data": [gif("a1")], "pagination": { "offset": 0, "count": 1, "total_count": 1 } });
        let page = page(body.to_string().as_bytes(), MAX_SEARCH_OFFSET).expect("a page");
        let found = &page.gifs[0];
        assert_eq!(
            (found.url.as_str(), found.preview.as_str(), found.preview_size, found.title.as_str()),
            (
                "https://giphy.com/gifs/cat-a1",
                "https://media.giphy.com/media/a1/200w.gif?cid=x&ct=g",
                Some((200, 150)),
                "A cat waves"
            )
        );
    }

    #[test]
    fn a_page_should_say_where_the_next_starts_while_there_are_more() {
        let body = serde_json::json!({ "data": [gif("a"), gif("b")], "pagination": { "offset": 24, "count": 2, "total_count": 100 } });
        assert_eq!(page(body.to_string().as_bytes(), MAX_SEARCH_OFFSET).expect("a page").next, Some(26));
    }

    #[test]
    fn the_last_page_should_name_no_next() {
        let body =
            serde_json::json!({ "data": [gif("a")], "pagination": { "offset": 9, "count": 1, "total_count": 10 } });
        assert_eq!(page(body.to_string().as_bytes(), MAX_SEARCH_OFFSET).expect("a page").next, None);
    }

    #[test]
    fn a_gif_without_a_preview_should_be_left_out() {
        let mut bare = gif("a");
        bare["images"] = serde_json::json!({});
        let body = serde_json::json!({ "data": [bare, gif("b")], "pagination": { "offset": 0, "count": 2 } });
        let page = page(body.to_string().as_bytes(), MAX_SEARCH_OFFSET).expect("a page");
        assert_eq!(page.gifs.iter().map(|gif| gif.id.as_str()).collect::<Vec<_>>(), ["b"]);
    }

    #[tokio::test]
    async fn a_search_without_a_key_should_ask_for_one() {
        let http = reqwest::Client::new();
        assert!(matches!(search(&http, " ", "cat", 0, None).await, Err(Error::NoGifKey)));
    }

    /// Searches GIPHY with the key in `GIPHY_API_KEY` (from `.env` at the repository root).
    #[tokio::test]
    #[ignore = "live network: needs GIPHY_API_KEY"]
    async fn a_live_search_should_find_gifs() {
        let key = std::env::var("GIPHY_API_KEY").expect("GIPHY_API_KEY");
        let http = crate::links::http_client().expect("http");
        let found = search(&http, &key, "cat", 0, Some("en")).await.expect("a page");
        assert!(!found.gifs.is_empty() && found.next.is_some());
        let trending = search(&http, &key, "", 0, None).await.expect("a page");
        assert!(!trending.gifs.is_empty());
    }
}
