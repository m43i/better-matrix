//! Permalinks: links that name a Matrix user, room or message rather than a web page.
//!
//! Two forms are read:
//!
//! - **matrix.to:** `https://matrix.to/#/<id>[/<event>][?via=…]`, where `<id>` is a user
//!   (`@alice:example.org`), a room alias (`#rust:matrix.org`) or a room ID (`!abc:example.org`),
//!   each URL-encoded or not, and `<event>` an event ID in that room.
//! - **`matrix:` URIs** ([MSC2312], in the spec since v1.2): `matrix:u/alice:example.org`,
//!   `matrix:r/rust:matrix.org`, `matrix:roomid/abc:example.org[/e/<event>]`, with the sigils left
//!   out and each segment URL-encoded, plus `?via=` and `?action=join|chat`.
//!
//! Search fields also take a **typed link** ([`Permalink::parse_typed`]): a permalink, one
//! without its `https://`, or a bare ID (`#rust:matrix.org`, `!abc:example.org`,
//! `@alice:example.org`), as people paste invitations and room addresses.
//!
//! The mobile app reads the same links in TypeScript (`apps/mobile/src/state/permalinks.ts`),
//! because it must route a link before the engine could answer. Both are tested against one set
//! of vectors, `permalinks.json` beside this file, so they can't drift apart.
//!
//! [MSC2312]: https://github.com/matrix-org/matrix-spec-proposals/pull/2312

/// What a permalink names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    /// A user, by their full ID (`@alice:example.org`).
    User(String),
    /// A room by its ID (`!abc:example.org`) or an alias (`#rust:matrix.org`), and one message in
    /// it when the link names one.
    Room {
        /// The room ID or alias.
        room: String,
        /// The message's event ID (`$…`).
        event: Option<String>,
    },
}

/// What a `matrix:` URI asks the client to do (`?action=`). matrix.to links never ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// Join the room.
    Join,
    /// Open a direct chat with the user.
    Chat,
}

/// A parsed permalink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permalink {
    /// What it names.
    pub target: LinkTarget,
    /// Servers to ask about the room (`?via=`), for one this account's server doesn't know yet.
    pub via: Vec<String>,
    /// What the link asks for, if it asks.
    pub action: Option<LinkAction>,
}

impl Permalink {
    /// Reads `link` as a matrix.to link or a `matrix:` URI; `None` for anything else, or for one
    /// whose IDs aren't Matrix IDs.
    ///
    /// ```
    /// use matrix_core::permalinks::{LinkTarget, Permalink};
    /// let link = Permalink::parse("https://matrix.to/#/%23rust%3Amatrix.org?via=matrix.org").unwrap();
    /// assert_eq!(link.target, LinkTarget::Room { room: "#rust:matrix.org".into(), event: None });
    /// assert_eq!(link.via, ["matrix.org"]);
    /// ```
    pub fn parse(link: &str) -> Option<Self> {
        // ASCII whitespace only, as the TypeScript trims it.
        let link = link.trim_matches(|c: char| c.is_ascii_whitespace());
        if let Some(rest) = strip_prefix_ignoring_case(link, "matrix:") {
            return matrix_uri(rest);
        }
        let rest =
            strip_prefix_ignoring_case(link, "https://").or_else(|| strip_prefix_ignoring_case(link, "http://"))?;
        let rest = strip_prefix_ignoring_case(rest, "matrix.to")?;
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let fragment = rest.strip_prefix('#')?;
        matrix_to(fragment.strip_prefix('/').unwrap_or(fragment))
    }

    /// Reads what someone typed or pasted into a search as a link: a permalink, a matrix.to link
    /// without its scheme, or a bare room alias, room ID or user ID. `None` for anything else, so
    /// a name being searched for never reads as one.
    ///
    /// ```
    /// use matrix_core::permalinks::{LinkTarget, Permalink};
    /// let link = Permalink::parse_typed("#rust:matrix.org").unwrap();
    /// assert_eq!(link.target, LinkTarget::Room { room: "#rust:matrix.org".into(), event: None });
    /// assert!(Permalink::parse_typed("rust").is_none());
    /// ```
    pub fn parse_typed(text: &str) -> Option<Self> {
        let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
        if let Some(link) = Self::parse(text) {
            return Some(link);
        }
        if strip_prefix_ignoring_case(text, "matrix.to").is_some() {
            return Self::parse(&format!("https://{text}"));
        }
        let target = if is_id(text, '@', true) {
            LinkTarget::User(text.to_owned())
        } else if is_id(text, '#', true) || is_id(text, '!', false) {
            LinkTarget::Room { room: text.to_owned(), event: None }
        } else {
            return None;
        };
        Some(Permalink { target, via: Vec::new(), action: None })
    }

    /// Whether `link` is a permalink: what decides where a link in a message opens.
    pub fn is_permalink(link: &str) -> bool {
        Self::parse(link).is_some()
    }
}

/// A matrix.to fragment after `#/`: `<id>[/<event>][?query]`.
fn matrix_to(fragment: &str) -> Option<Permalink> {
    let (path, query) = fragment.split_once('?').unwrap_or((fragment, ""));
    let mut segments = path.split('/').map(decode);
    let id = segments.next()??;
    let event = match segments.next() {
        None => None,
        Some(event) => Some(event.filter(|event| is_id(event, '$', false))?),
    };
    if segments.next().is_some() {
        return None;
    }
    let target = if is_id(&id, '@', true) && event.is_none() {
        LinkTarget::User(id)
    } else if is_id(&id, '#', true) || is_id(&id, '!', false) {
        LinkTarget::Room { room: id, event }
    } else {
        return None;
    };
    let (via, action) = read_query(query)?;
    Some(Permalink { target, via, action })
}

/// A `matrix:` URI after the scheme: `<kind>/<id>[/e/<event>][?query][#fragment]`.
fn matrix_uri(rest: &str) -> Option<Permalink> {
    // The authority is reserved; nothing uses it yet.
    if rest.starts_with("//") {
        return None;
    }
    let rest = rest.split_once('#').map_or(rest, |(before, _)| before);
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let segments = path.split('/').map(decode).collect::<Option<Vec<_>>>()?;
    let target = match segments.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["u", user] => LinkTarget::User(sigiled('@', user, true)?),
        ["r", alias] => LinkTarget::Room { room: sigiled('#', alias, true)?, event: None },
        ["roomid", room] => LinkTarget::Room { room: sigiled('!', room, false)?, event: None },
        ["r", alias, "e", event] => {
            LinkTarget::Room { room: sigiled('#', alias, true)?, event: Some(sigiled('$', event, false)?) }
        }
        ["roomid", room, "e", event] => {
            LinkTarget::Room { room: sigiled('!', room, false)?, event: Some(sigiled('$', event, false)?) }
        }
        _ => return None,
    };
    let (via, action) = read_query(query)?;
    Some(Permalink { target, via, action })
}

/// `?via=…&action=…`: every non-empty `via` in order, and the action when it is one this app
/// knows (an unknown one is left out, as the spec asks). Other parameters are ignored.
fn read_query(query: &str) -> Option<(Vec<String>, Option<LinkAction>)> {
    let mut via = Vec::new();
    let mut action = None;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decode(value)?;
        match key {
            "via" if !value.is_empty() => via.push(value),
            "action" => {
                action = match value.as_str() {
                    "join" => Some(LinkAction::Join),
                    "chat" => Some(LinkAction::Chat),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    Some((via, action))
}

/// `id` with its sigil put in front, when that makes a Matrix ID.
fn sigiled(sigil: char, id: &str, needs_server: bool) -> Option<String> {
    let id = format!("{sigil}{id}");
    is_id(&id, sigil, needs_server).then_some(id)
}

/// Whether `id` is a Matrix ID with `sigil`: something after it, no spaces or control
/// characters, and, when `needs_server`, a `localpart:server` (room IDs and event IDs of newer
/// room versions have no server).
fn is_id(id: &str, sigil: char, needs_server: bool) -> bool {
    let Some(rest) = id.strip_prefix(sigil) else { return false };
    if rest.is_empty() || rest.chars().any(|c| c.is_ascii_control() || c == ' ') {
        return false;
    }
    !needs_server || rest.split_once(':').is_some_and(|(local, server)| !local.is_empty() && !server.is_empty())
}

/// Percent-decodes `text` (`+` stays a plus); `None` when an escape is broken or the bytes
/// aren't UTF-8, as JavaScript's `decodeURIComponent` refuses them.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let hex = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok().filter(|_| hex.bytes().all(|b| b.is_ascii_hexdigit()))?);
            i += 3;
        } else {
            out.push(byte);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// A shareable message link, with a joined member's server as a routing hint (also for v12 IDs).
pub fn message_link(room_id: &str, event_id: &str, account: &str) -> String {
    format!("https://matrix.to/#/{}/{}{}", encode(room_id), encode(event_id), via(account))
}

/// A link to a room by its ID, with a joined member's server as a routing hint.
pub fn room_link(room_id: &str, account: &str) -> String {
    format!("https://matrix.to/#/{}{}", encode(room_id), via(account))
}

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// `?via=` the server of `account`, a member's user ID; nothing for one without a server.
fn via(account: &str) -> String {
    account.split_once(':').map(|(_, server)| format!("?via={}", encode(server))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    /// The vectors the mobile app's parser is tested against too.
    const VECTORS: &str = include_str!("permalinks.json");

    /// A parsed link in the vectors' shape.
    fn shape(link: &Permalink) -> Value {
        let (kind, id, event) = match &link.target {
            LinkTarget::User(id) => ("user", id.clone(), None),
            LinkTarget::Room { room, event } => ("room", room.clone(), event.clone()),
        };
        let action = link.action.map(|action| match action {
            LinkAction::Join => "join",
            LinkAction::Chat => "chat",
        });
        json!({ "kind": kind, "id": id, "event": event, "via": link.via, "action": action })
    }

    #[test]
    fn message_links_should_match_the_shared_mobile_vectors() {
        let vectors: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("message_links.json")).expect("vectors");
        for vector in vectors {
            assert_eq!(
                message_link(
                    vector["room"].as_str().unwrap(),
                    vector["event"].as_str().unwrap(),
                    vector["account"].as_str().unwrap()
                ),
                vector["link"]
            );
            assert_eq!(
                crate::links::translation_url(vector["body"].as_str().unwrap(), vector["language"].as_str().unwrap()),
                vector["translation"]
            );
        }
    }

    #[test]
    fn a_room_link_should_read_back_as_its_room() {
        let link = Permalink::parse(&room_link("!abc:example.org", "@me:example.org"));
        let target = LinkTarget::Room { room: "!abc:example.org".into(), event: None };
        assert_eq!(link.map(|link| (link.target, link.via)), Some((target, vec!["example.org".to_owned()])));
    }

    #[test]
    fn every_shared_vector_should_parse_as_written() {
        let vectors: Vec<Value> = serde_json::from_str(VECTORS).expect("the vectors are JSON");
        assert!(vectors.len() > 30);
        for vector in vectors {
            let link = vector["link"].as_str().expect("a link");
            let parsed = Permalink::parse(link).as_ref().map(shape).unwrap_or(Value::Null);
            assert_eq!(parsed, vector["parsed"], "{link}");
        }
    }

    /// What search fields read as links, shared with the mobile app's parser too.
    const TYPED: &str = include_str!("typed_links.json");

    #[test]
    fn every_shared_typed_vector_should_parse_as_written() {
        let vectors: Vec<Value> = serde_json::from_str(TYPED).expect("the vectors are JSON");
        for vector in vectors {
            let text = vector["text"].as_str().expect("a text");
            let parsed = Permalink::parse_typed(text).as_ref().map(shape).unwrap_or(Value::Null);
            assert_eq!(parsed, vector["parsed"], "{text}");
        }
    }

    #[test]
    fn a_web_page_should_not_be_a_permalink() {
        assert!(!Permalink::is_permalink("https://example.org/#/@alice:example.org"));
    }

    #[test]
    fn a_broken_escape_should_not_parse() {
        assert_eq!(decode("%4"), None);
        assert_eq!(decode("%zz"), None);
        assert_eq!(decode("%+1"), None);
    }
}
