//! Links in messages.
//!
//! - **Linked GIFs:** links to the popular GIF sites (Giphy, Tenor, Imgur) and direct `.gif`
//!   links are resolved to the GIF file and fetched with a size limit. Fetching contacts the
//!   site, which sees the user's address, so apps let the user turn it off.
//! - **Link previews:** any other link gets a card from its page's Open Graph tags (title,
//!   description, image), which the homeserver fetches and caches (`preview_url`), so the site
//!   never sees the user; the image is homeserver media. Apps let the user turn these off too.

use std::time::Duration;

use matrix_sdk::reqwest;

use crate::Error;
use crate::model::MediaRef;

/// The largest GIF fetched.
const MAX_GIF_BYTES: usize = 15 << 20;
/// The largest page read to find a GIF in (Tenor share pages).
const MAX_PAGE_BYTES: usize = 1 << 20;

/// Where a link's GIF lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GifSource {
    /// The GIF file itself.
    File(String),
    /// A page whose `og:image` is the GIF (Tenor share links).
    Page(String),
}

/// The GIF behind a link, if the link is one; `None` for anything else.
///
/// ```
/// use matrix_core::links::{gif_source, GifSource};
/// assert_eq!(
///     gif_source("https://giphy.com/gifs/cat-typing-JIX9t2j0ZTN9S"),
///     Some(GifSource::File("https://i.giphy.com/JIX9t2j0ZTN9S.gif".into()))
/// );
/// ```
pub fn gif_source(link: &str) -> Option<GifSource> {
    let rest = link.strip_prefix("https://").or_else(|| link.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split(['?', '#']).next().unwrap_or_default().trim_end_matches('/');
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let last = path.rsplit('/').next().unwrap_or_default();
    match host.as_str() {
        "giphy.com" if path.starts_with("gifs/") || path.starts_with("stickers/") => {
            let id = last.rsplit('-').next().filter(|id| !id.is_empty())?;
            Some(GifSource::File(format!("https://i.giphy.com/{id}.gif")))
        }
        "gph.is" => None,
        "tenor.com" if path.starts_with("view/") => Some(GifSource::Page(format!("https://tenor.com/{path}"))),
        "imgur.com" if !path.is_empty() && !path.contains('/') => {
            Some(GifSource::File(format!("https://i.imgur.com/{path}.gif")))
        }
        "i.imgur.com" if last.ends_with(".gifv") => {
            Some(GifSource::File(format!("https://i.imgur.com/{}.gif", last.trim_end_matches(".gifv"))))
        }
        _ if last.to_ascii_lowercase().ends_with(".gif") => {
            Some(GifSource::File(link.split(['?', '#']).next()?.to_owned()))
        }
        _ => None,
    }
}

/// The links in a message's text, in order, without the punctuation around them.
fn links(text: &str) -> impl Iterator<Item = &str> {
    link_ranges(text).map(move |range| &text[range])
}

/// Where the links in `text` are, as byte ranges, in order: web addresses (`http://`,
/// `https://`) without the punctuation around them.
///
/// ```
/// use matrix_core::links::link_ranges;
/// let text = "see (https://matrix.org/blog), then https://example.org.";
/// let found: Vec<&str> = link_ranges(text).map(|range| &text[range]).collect();
/// assert_eq!(found, ["https://matrix.org/blog", "https://example.org"]);
/// ```
pub fn link_ranges(text: &str) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    text.split_whitespace().filter_map(move |word| {
        let link = unwrap_link(word);
        let rest = link.strip_prefix("https://").or_else(|| link.strip_prefix("http://"));
        let is_link = rest.is_some_and(|rest| !rest.is_empty());
        // Both are slices of `text`: where they start is how far into it they are.
        let start = link.as_ptr() as usize - text.as_ptr() as usize;
        is_link.then(|| start..start + link.len())
    })
}

/// Whether `text` is one web address and nothing else: what a paste makes a link.
pub fn is_link(text: &str) -> bool {
    let text = text.trim();
    link_ranges(text).next().is_some_and(|range| range == (0..text.len()))
}

/// A word without the punctuation around it. A closing parenthesis stays when the link opened
/// one (`…/wiki/Rust_(programming_language)`).
fn unwrap_link(word: &str) -> &str {
    let mut word = word.trim_start_matches(['<', '(', '"', '\'']);
    loop {
        let trimmed = word.trim_end_matches(['>', '"', '\'', ',', '.', '!', '?', ';', ':']);
        let unbalanced = trimmed.ends_with(')') && trimmed.matches(')').count() > trimmed.matches('(').count();
        let next = if unbalanced { &trimmed[..trimmed.len() - 1] } else { trimmed };
        if next == word {
            return word;
        }
        word = next;
    }
}

/// The first GIF link in a message's text, if any.
pub fn find_gif_link(text: &str) -> Option<&str> {
    links(text).find(|word| gif_source(word).is_some())
}

/// The first link in a message's text that gets a preview card: one that isn't a GIF (those
/// show as the GIF).
///
/// ```
/// use matrix_core::links::find_preview_link;
/// assert_eq!(find_preview_link("see https://matrix.org/blog."), Some("https://matrix.org/blog"));
/// assert_eq!(find_preview_link("https://giphy.com/gifs/cat-JIX9t2j0ZTN9S"), None);
/// ```
pub fn find_preview_link(text: &str) -> Option<&str> {
    links(text).find(|word| gif_source(word).is_none())
}

/// A link's preview card: what its page says about itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct LinkPreview {
    /// The site's name (`og:site_name`).
    pub site: Option<String>,
    /// The page's title.
    pub title: Option<String>,
    /// Its description, cut short.
    pub description: Option<String>,
    /// Its image, as homeserver media, and the image's size when the page gives it.
    pub image: Option<(MediaRef, Option<(u32, u32)>)>,
}

/// The longest description a card shows.
const MAX_DESCRIPTION: usize = 300;

/// Reads the homeserver's Open Graph data for a link; `None` when it says nothing worth a card.
pub fn parse_preview(data: &serde_json::Value) -> Option<LinkPreview> {
    let text = |key: &str| {
        data[key]
            .as_str()
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|text| !text.is_empty())
    };
    let number = |key: &str| {
        data[key]
            .as_u64()
            .or_else(|| data[key].as_str().and_then(|n| n.parse().ok()))
            .and_then(|n| u32::try_from(n).ok())
    };
    let description = text("og:description").map(|text| match text.char_indices().nth(MAX_DESCRIPTION) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text,
    });
    let image = data["og:image"]
        .as_str()
        .filter(|uri| uri.starts_with("mxc://"))
        .and_then(|uri| crate::convert::avatar(uri.into()))
        .map(|media| (media, number("og:image:width").zip(number("og:image:height"))));
    let preview = LinkPreview { site: text("og:site_name"), title: text("og:title"), description, image };
    (preview.title.is_some() || preview.description.is_some()).then_some(preview)
}

/// How long the homeserver may take to describe a page; it isn't asked twice in one go (a page
/// it can't fetch shouldn't hold a request open for minutes).
const PREVIEW_WAIT: Duration = Duration::from_secs(15);

impl crate::Account {
    /// A link's preview card, from the homeserver (the authenticated endpoint, else the older
    /// one where it doesn't know that); `None` when the page has nothing to show, or the server
    /// won't describe it (it couldn't fetch it, or previews are off). Only a failure worth
    /// trying again later (no answer, too many requests) is an error.
    pub async fn preview_link(&self, url: &str) -> Result<Option<LinkPreview>, Error> {
        use matrix_sdk::ruma::api::client::{authenticated_media, media};
        let config = matrix_sdk::config::RequestConfig::new().disable_retry().timeout(PREVIEW_WAIT);
        let request = authenticated_media::get_media_preview::v1::Request::new(url.to_owned());
        let data = match self.client().send(request).with_request_config(config).await {
            Ok(response) => response.data,
            Err(error) if unknown_endpoint(&error) => {
                #[expect(deprecated, reason = "for homeservers without authenticated media")]
                let request = media::get_media_preview::v3::Request::new(url.to_owned());
                match self.client().send(request).with_request_config(config).await {
                    Ok(response) => response.data,
                    Err(error) => return refused(url, &error),
                }
            }
            Err(error) => return refused(url, &error),
        };
        Ok(data.and_then(|raw| serde_json::from_str(raw.get()).ok()).and_then(|data| parse_preview(&data)))
    }
}

/// The homeserver doesn't have the endpoint asked.
fn unknown_endpoint(error: &matrix_sdk::HttpError) -> bool {
    use matrix_sdk::ruma::api::error::ErrorKind;
    error
        .as_client_api_error()
        .is_some_and(|api| api.status_code.as_u16() == 404 || matches!(api.error_kind(), Some(ErrorKind::Unrecognized)))
}

/// A failed preview: no card when the request was refused (any 4xx but "too many requests",
/// however it's worded: a proxy blocking previews answers with its own page) or the homeserver
/// itself said it couldn't describe the page (a Matrix error); an error, to try again later,
/// when nothing answered, it asked to slow down, or something in front of the homeserver failed
/// (a proxy's 502 while it restarts).
fn refused(url: &str, error: &matrix_sdk::HttpError) -> Result<Option<LinkPreview>, Error> {
    use matrix_sdk::ruma::api::error::ErrorBody;
    let answer =
        error.as_client_api_error().map(|api| (api.status_code.as_u16(), matches!(api.body, ErrorBody::Standard(_))));
    if gets_no_card(answer) { Ok(None) } else { Err(Error::Link { url: url.to_owned(), reason: error.to_string() }) }
}

/// Whether a failed preview means "no card" rather than "try again": by the answer's status and
/// whether it was a Matrix error from the homeserver (`None`: nothing answered).
fn gets_no_card(answer: Option<(u16, bool)>) -> bool {
    match answer {
        Some((429, _)) | None => false,
        Some((400..=499, _)) => true,
        Some((_, from_homeserver)) => from_homeserver,
    }
}

/// `text` without the link `url`, its spaces tidied: the words around a GIF that shows on its
/// own. Empty when the text was only the link.
pub fn without_link(text: &str, url: &str) -> String {
    text.split(' ')
        .map(|word| if word.contains(url) { word.replace(url, "") } else { word.to_owned() })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

/// Matrix HTML without the link `url`: the anchor around it, and the bare address.
pub fn html_without_link(html: &str, url: &str) -> String {
    let escaped = url.replace('&', "&amp;");
    let mut out = html.to_owned();
    for href in [url, escaped.as_str()] {
        while let Some(at) = out.find(&format!("href=\"{href}\"")) {
            let (Some(open), Some(close)) = (out[..at].rfind("<a"), out[at..].find("</a>")) else { break };
            out.replace_range(open..at + close + "</a>".len(), "");
        }
    }
    out.replace(&escaped, "").replace(url, "").trim().to_owned()
}

/// The `og:image` of an HTML page, if it has one.
fn og_image(html: &str) -> Option<&str> {
    html.split("<meta").skip(1).find_map(|tag| {
        let tag = tag.split('>').next()?;
        let is_og_image = tag.contains("property=\"og:image\"") || tag.contains("property='og:image'");
        if !is_og_image {
            return None;
        }
        let start = tag.find("content=\"")? + "content=\"".len();
        tag[start..].split('"').next()
    })
}

/// Fetches the GIF behind a link.
pub(crate) async fn fetch_gif(http: &reqwest::Client, link: &str) -> Result<Vec<u8>, Error> {
    let fail = |reason: String| Error::Link { url: link.to_owned(), reason };
    let file = match gif_source(link).ok_or_else(|| fail("not a GIF link".into()))? {
        GifSource::File(url) => url,
        GifSource::Page(url) => {
            let page = fetch(http, &url, MAX_PAGE_BYTES).await.map_err(fail)?;
            let html = String::from_utf8_lossy(&page);
            og_image(&html).ok_or_else(|| fail("the page names no GIF".into()))?.replace("&amp;", "&")
        }
    };
    fetch(http, &file, MAX_GIF_BYTES).await.map_err(fail)
}

async fn fetch(http: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut response =
        http.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > limit {
            return Err(format!("larger than {} MB", limit >> 20));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// An HTTP client for linked files: no cookies, a short timeout.
pub(crate) fn http_client() -> Result<reqwest::Client, Error> {
    crate::tls::http(reqwest::Client::builder())
        .timeout(Duration::from_secs(20))
        .user_agent("BetterMatrix")
        .build()
        .map_err(|e| Error::Link { url: String::new(), reason: e.to_string() })
}

/// Opens Google Translate with this text and target language. Apps ask before sharing text.
pub fn translation_url(body: &str, language: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("sl", "auto")
        .append_pair("tl", language)
        .append_pair("text", body)
        .append_pair("op", "translate")
        .finish();
    format!("https://translate.google.com/?{query}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_should_be_found_by_where_they_are_after_other_letters() {
        let text = "Grüße: https://a.example/ü und mehr";
        let found: Vec<&str> = link_ranges(text).map(|range| &text[range]).collect();
        assert_eq!(found, ["https://a.example/ü"]);
    }

    #[test]
    fn a_scheme_alone_should_not_be_a_link() {
        assert_eq!(link_ranges("https:// http://").count(), 0);
    }

    #[test]
    fn only_a_lone_address_should_count_as_a_link_to_paste() {
        assert!(is_link("  https://matrix.org/blog \n"));
        assert!(!is_link("see https://matrix.org"));
        assert!(!is_link("matrix.org"));
    }

    #[test]
    fn a_refused_preview_should_get_no_card_and_a_failed_one_should_be_tried_again() {
        // Refused: a proxy blocking previews, the homeserver saying it couldn't fetch the page.
        assert!(gets_no_card(Some((404, false))) && gets_no_card(Some((403, true))));
        assert!(gets_no_card(Some((502, true))));
        // Tried again: no answer, too many requests, a proxy's error page while it restarts.
        assert!(!gets_no_card(None) && !gets_no_card(Some((429, true))) && !gets_no_card(Some((502, false))));
    }

    #[test]
    fn a_link_ending_in_its_own_parenthesis_should_keep_it() {
        let text = "see https://en.wikipedia.org/wiki/Rust_(programming_language).";
        assert_eq!(find_preview_link(text), Some("https://en.wikipedia.org/wiki/Rust_(programming_language)"));
        assert_eq!(find_preview_link("(https://matrix.org/blog)"), Some("https://matrix.org/blog"));
    }

    #[test]
    fn a_page_with_open_graph_tags_should_preview_as_a_card() {
        let data = serde_json::json!({
            "og:site_name": "Matrix.org", "og:title": "Matrix 2.0", "og:description": "Faster  sync.",
            "og:image": "mxc://matrix.org/abc", "og:image:width": 1200, "og:image:height": "630"
        });
        let preview = parse_preview(&data).unwrap_or_default();
        assert_eq!(
            (preview.title.as_deref(), preview.description.as_deref()),
            (Some("Matrix 2.0"), Some("Faster sync."))
        );
        assert_eq!(preview.image.and_then(|(_, size)| size), Some((1200, 630)));
    }

    #[test]
    fn a_page_without_a_title_or_description_should_get_no_card() {
        assert!(parse_preview(&serde_json::json!({ "og:image": "mxc://x/y" })).is_none());
    }

    #[test]
    fn a_long_description_should_be_cut_short() {
        let data = serde_json::json!({ "og:title": "T", "og:description": "word ".repeat(200) });
        let description = parse_preview(&data).and_then(|preview| preview.description).unwrap_or_default();
        assert!(description.chars().count() <= MAX_DESCRIPTION + 1 && description.ends_with('…'));
    }

    #[test]
    fn a_gif_link_should_be_cut_from_the_words_around_it() {
        let url = "https://media.tenor.com/x/kiwi.gif";
        assert_eq!(without_link(&format!("look {url} at this"), url), "look at this");
    }

    #[test]
    fn a_message_that_is_only_a_gif_link_should_leave_nothing() {
        let url = "https://media.tenor.com/x/kiwi.gif";
        assert_eq!(without_link(url, url), "");
    }

    #[test]
    fn a_gif_link_should_be_cut_from_html_with_its_anchor() {
        let url = "https://giphy.com/gifs/ship-it?a=1&b=2";
        let html = "<b>Ship</b> <a href=\"https://giphy.com/gifs/ship-it?a=1&amp;b=2\">link</a> now";
        assert_eq!(html_without_link(html, url), "<b>Ship</b>  now");
    }

    #[test]
    fn a_tenor_share_link_should_be_read_from_its_page() {
        assert_eq!(
            gif_source("https://tenor.com/view/happy-cat-12345?x=1"),
            Some(GifSource::Page("https://tenor.com/view/happy-cat-12345".into()))
        );
    }

    #[test]
    fn an_imgur_page_should_point_at_its_gif() {
        assert_eq!(
            gif_source("https://imgur.com/AbC123"),
            Some(GifSource::File("https://i.imgur.com/AbC123.gif".into()))
        );
    }

    #[test]
    fn an_imgur_gifv_should_become_a_gif() {
        assert_eq!(
            gif_source("https://i.imgur.com/AbC123.gifv"),
            Some(GifSource::File("https://i.imgur.com/AbC123.gif".into()))
        );
    }

    #[test]
    fn a_direct_gif_link_should_be_fetched_as_is() {
        let link = "https://media.tenor.com/abc/cat.gif?width=200";
        assert_eq!(gif_source(link), Some(GifSource::File("https://media.tenor.com/abc/cat.gif".into())));
    }

    #[test]
    fn other_links_should_not_count() {
        assert_eq!(gif_source("https://example.org/page"), None);
    }

    #[test]
    fn the_first_gif_link_in_a_message_should_be_found() {
        assert_eq!(
            find_gif_link("look (https://giphy.com/gifs/x-abc) and https://imgur.com/q"),
            Some("https://giphy.com/gifs/x-abc")
        );
    }

    /// Talks to Giphy; run with `cargo test -p matrix-core -- --ignored`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn a_giphy_link_should_fetch_a_gif() {
        let bytes = fetch_gif(&http_client().unwrap(), "https://giphy.com/gifs/cat-funny-JIX9t2j0ZTN9S").await.unwrap();
        assert!(bytes.starts_with(b"GIF8"), "{} bytes", bytes.len());
    }

    #[test]
    fn og_image_should_be_read_from_a_page() {
        let html = r#"<head><meta property="og:image" content="https://media.tenor.com/a/b.gif"></head>"#;
        assert_eq!(og_image(html), Some("https://media.tenor.com/a/b.gif"));
    }
}
