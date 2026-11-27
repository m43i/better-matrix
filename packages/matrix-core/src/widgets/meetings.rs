//! Meetings: video conferences pinned to a room as widgets, Jitsi and BigBlueButton, as Element
//! and Dimension write them. A meeting opens as itself (Jitsi's page, BigBlueButton's join
//! link) rather than the page Element wraps it in, which only works inside Element: the wrapper
//! asks Element for the user's name and an OpenID token, which the engine has itself.
//!
//! - Jitsi (`jitsi`, `m.jitsi`, Dimension's `dimension-jitsi`): the server and conference from
//!   the widget's `data` (`domain`, `conferenceId`), else from its URL (`conferenceDomain`,
//!   `domain`, `conferenceId`, `confId`; older widgets meant `jitsi.riot.im`). A server that
//!   checks who joins (`auth: openidtoken-jwt`, the Prosody module
//!   `mod_auth_matrix_user_verification`) gets a JWT carrying an OpenID token, as Element's
//!   wrapper makes it. The user's name and audio-only go in Jitsi's own URL settings.
//! - BigBlueButton: Dimension's widget (`bigbluebutton`) names its room's page
//!   (`conferenceUrl`); a meeting Dimension created (an `m.custom` widget on Dimension's
//!   `/widgets/bigbluebutton` page) is joined through Dimension's `getJoinUrl`.
//! - Starting one writes a Jitsi widget as Element does, on the server the homeserver's
//!   well-known file names (`io.element.jitsi`, then `im.vector.riot.jitsi`), and only there, so
//!   Element users see and join it too.

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{LEGACY_TYPE, Viewer, credential_http, encode, send_json, with_json};
use crate::{Account, Error};

/// A video meeting a widget holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum Meeting {
    /// A Jitsi Meet conference.
    Jitsi,
    /// A BigBlueButton meeting.
    BigBlueButton,
    /// A configured Microsoft Teams meeting.
    Teams,
    /// A configured Zoom meeting.
    Zoom,
}

/// The server older Jitsi widgets meant when they named none.
const OLD_JITSI: &str = "jitsi.riot.im";
/// A Jitsi server that checks who joins with an OpenID token.
const JITSI_AUTH: &str = "openidtoken-jwt";
/// The page Element shows Jitsi widgets in, written into new ones so Element users can join.
const ELEMENT_JITSI: &str = "https://app.element.io/jitsi.html";
/// How long a meeting's servers get to answer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Where a meeting is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// A Jitsi conference.
    Jitsi(Jitsi),
    /// A BigBlueButton meeting.
    BigBlueButton(BigBlueButton),
    /// A hosted meeting opened with its original join link.
    Hosted { meeting: Meeting, url: url::Url },
}

/// A Jitsi conference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Jitsi {
    /// Its server (`meet.example.org`).
    pub domain: String,
    /// Its conference (the room's name on the server).
    pub conference: String,
    /// Whether it starts without video.
    pub audio_only: bool,
    /// Whether the server checks who joins with an OpenID token.
    pub auth: bool,
}

/// A BigBlueButton meeting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BigBlueButton {
    /// A room's page (Greenlight's), joined there.
    Room(String),
    /// A meeting Dimension created, whose join link Dimension gives (at `api`, its origin).
    Dimension { api: String, meeting_id: String, password: String },
}

impl Place {
    /// What kind of meeting.
    pub(crate) fn meeting(&self) -> Meeting {
        match self {
            Self::Jitsi(_) => Meeting::Jitsi,
            Self::BigBlueButton(_) => Meeting::BigBlueButton,
            Self::Hosted { meeting, .. } => *meeting,
        }
    }

    /// The meeting's page in a browser, without what joining asks the server for; `None` when
    /// only joining knows it.
    pub(crate) fn browser_url(&self, viewer: &Viewer) -> Option<String> {
        match self {
            Self::Jitsi(jitsi) => jitsi.url(&viewer.display_name, None),
            Self::BigBlueButton(BigBlueButton::Room(url)) => Some(url.clone()),
            Self::BigBlueButton(BigBlueButton::Dimension { .. }) => None,
            Self::Hosted { url, .. } => Some(url.to_string()),
        }
    }
}

impl Jitsi {
    /// The conference's page: the user's name filled in, video off for an audio-only one, and
    /// Jitsi's "open the app" page skipped on phones; `jwt` lets a checking server let them in.
    pub(crate) fn url(&self, display_name: &str, jwt: Option<&str>) -> Option<String> {
        let mut url = url::Url::parse(&format!("https://{}/", self.domain)).ok()?;
        url.path_segments_mut().ok()?.clear().push(&self.conference);
        if let Some(jwt) = jwt {
            url.query_pairs_mut().append_pair("jwt", jwt);
        }
        // Jitsi reads each setting as JSON.
        let mut settings = vec![format!("userInfo.displayName={}", encode(&Value::from(display_name).to_string()))];
        if self.audio_only {
            settings.push("config.startWithVideoMuted=true".to_owned());
        }
        settings.push("config.disableDeepLinking=true".to_owned());
        url.set_fragment(Some(&settings.join("&")));
        Some(url.to_string())
    }
}

/// The meeting a widget holds, from its type, URL template and `data`.
pub(crate) fn place(kind: &str, template: &str, data: &Value) -> Option<Place> {
    let params = params(template);
    let param = |names: &[&str]| {
        names.iter().find_map(|name| {
            params.iter().find(|(key, value)| key == name && !value.is_empty() && !value.starts_with('$'))
        })
    };
    let text = |key: &str| data.get(key).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty());
    if matches!(kind, "jitsi" | "m.jitsi" | "dimension-jitsi") {
        let domain = text("domain")
            .map(str::to_owned)
            .or_else(|| param(&["conferenceDomain", "domain"]).map(|(_, value)| value.clone()))
            .unwrap_or_else(|| OLD_JITSI.to_owned());
        let conference = text("conferenceId")
            .map(str::to_owned)
            .or_else(|| param(&["conferenceId", "confId"]).map(|(_, value)| value.clone()))?;
        let audio_only = match data.get("isAudioOnly") {
            Some(value) => value.as_bool().unwrap_or(false) || value.as_str() == Some("true"),
            None => param(&["isAudioOnly", "isAudioConf"]).is_some_and(|(_, value)| value == "true"),
        };
        let auth = text("auth") == Some(JITSI_AUTH) || param(&["auth"]).is_some_and(|(_, value)| value == JITSI_AUTH);
        // A server name only: a widget naming anything else isn't a Jitsi meeting this app opens.
        if domain.contains(['/', '?', '#', '@']) {
            return None;
        }
        return Some(Place::Jitsi(Jitsi { domain, conference, audio_only, auth }));
    }
    if let Some((meeting, url)) = hosted_meeting(template).or_else(|| {
        ["joinUrl", "conferenceUrl"].into_iter().filter_map(text).filter_map(hosted_meeting).find(|(meeting, _)| {
            match meeting {
                Meeting::Teams => matches!(kind, "teams" | "m.teams" | "microsoft-teams"),
                Meeting::Zoom => matches!(kind, "zoom" | "m.zoom"),
                _ => false,
            }
        })
    }) {
        return Some(Place::Hosted { meeting, url });
    }
    let web = |url: &str| {
        url::Url::parse(url).ok().filter(|url| matches!(url.scheme(), "https" | "http")).map(|url| url.to_string())
    };
    let room = text("conferenceUrl").and_then(web).or_else(|| param(&["conferenceUrl"]).and_then(|(_, url)| web(url)));
    if matches!(kind, "bigbluebutton" | "dimension-bigbluebutton") {
        return room.map(|url| Place::BigBlueButton(BigBlueButton::Room(url)));
    }
    // A meeting Dimension created: its own page, naming the meeting.
    let page = url::Url::parse(template).ok()?;
    let prefix = page.path().strip_suffix("/widgets/bigbluebutton")?;
    if let Some(url) = room {
        return Some(Place::BigBlueButton(BigBlueButton::Room(url)));
    }
    // Dimension is told who the user is: only over HTTPS.
    if page.scheme() != "https" {
        return None;
    }
    let (_, meeting_id) = param(&["meetingId"])?;
    let (_, password) = param(&["meetingPassword"])?;
    let api = format!("{}{prefix}", page.origin().ascii_serialization());
    Some(Place::BigBlueButton(BigBlueButton::Dimension {
        api,
        meeting_id: meeting_id.clone(),
        password: password.clone(),
    }))
}

/// Recognizes published provider join URLs without trusting a room-set widget type or name.
fn hosted_meeting(value: &str) -> Option<(Meeting, url::Url)> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return None;
    }
    let host = url.host_str()?;
    let path = url.path();
    let has_join_path = |prefixes: &[&str]| {
        prefixes.iter().any(|prefix| path.strip_prefix(prefix).is_some_and(|rest| !rest.is_empty()))
    };
    let meeting = if matches!(host, "teams.microsoft.com" | "teams.cloud.microsoft" | "teams.live.com")
        && has_join_path(&["/l/meetup-join/", "/meet/"])
    {
        Meeting::Teams
    } else if ["zoom.us", "zoom.com", "zoomgov.com"]
        .into_iter()
        .any(|base| host == base || host.strip_suffix(base).is_some_and(|left| left.ends_with('.')))
        && has_join_path(&["/j/", "/my/", "/wc/join/", "/w/"])
    {
        Meeting::Zoom
    } else {
        return None;
    };
    Some((meeting, url))
}

/// A URL's query and fragment parameters, decoded.
fn params(url: &str) -> Vec<(String, String)> {
    let after = |mark: char| url.split_once(mark).map(|(_, rest)| rest);
    let query = after('?').map(|rest| rest.split('#').next().unwrap_or_default());
    let fragment = after('#');
    query
        .into_iter()
        .chain(fragment)
        .flat_map(|part| url::form_urlencoded::parse(part.as_bytes()).into_owned().collect::<Vec<_>>())
        .collect()
}

impl Account {
    /// Where joining a widget's meeting goes: its page, with what its server needs to let the
    /// user in.
    /// `identity`: whether the user lets a checking Jitsi server (or Dimension) learn who they
    /// are; Dimension isn't asked without it.
    pub(crate) async fn join_url(&self, place: &Place, viewer: &Viewer, identity: bool) -> Result<String, Error> {
        // Dimension is told who the user is: it alone hears it.
        let http = credential_http()?;
        match place {
            Place::Jitsi(jitsi) => {
                let jwt = if jitsi.auth && identity { Some(self.jitsi_jwt(jitsi, viewer).await?) } else { None };
                jitsi.url(&viewer.display_name, jwt.as_deref()).ok_or_else(|| Error::Meeting(jitsi.domain.clone()))
            }
            Place::BigBlueButton(BigBlueButton::Room(url)) => Ok(url.clone()),
            Place::Hosted { url, .. } => Ok(url.to_string()),
            Place::BigBlueButton(BigBlueButton::Dimension { .. }) if !identity => Err(Error::Cancelled),
            Place::BigBlueButton(BigBlueButton::Dimension { api, meeting_id, password }) => {
                let body = json!({
                    "displayName": viewer.display_name,
                    "userId": viewer.user_id,
                    "avatarUrl": viewer.avatar_url,
                    "meetingId": meeting_id,
                    "meetingPassword": password,
                });
                let asked = http.post(format!("{api}/api/v1/dimension/bigbluebutton/getJoinUrl")).timeout(TIMEOUT);
                let (status, answer) =
                    send_json(with_json(asked, &body)).await.map_err(|error| Error::Meeting(error.to_string()))?;
                answer["url"]
                    .as_str()
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        Error::Meeting(answer["message"].as_str().map_or_else(|| status.to_string(), str::to_owned))
                    })
            }
        }
    }

    /// The JWT a checking Jitsi server lets the user in with: an OpenID token proving who they
    /// are, for the room the conference belongs to.
    async fn jitsi_jwt(&self, jitsi: &Jitsi, viewer: &Viewer) -> Result<String, Error> {
        use matrix_sdk::ruma::api::client::account::request_openid_token;
        let openid = self.client().send(request_openid_token::v3::Request::new(self.user_id()?)).await?;
        let payload = json!({
            "iss": jitsi.domain,
            "sub": jitsi.domain,
            "aud": format!("https://{}", jitsi.domain),
            "room": "*",
            "context": {
                "matrix": {
                    "token": openid.access_token,
                    "room_id": viewer.room_id,
                    "server_name": openid.matrix_server_name.to_string(),
                },
                "user": { "avatar": viewer.avatar_url, "name": viewer.display_name },
            },
        });
        Ok(jwt(&payload))
    }

    /// Pins a new Jitsi meeting under the widget ID reserved by the engine.
    pub(crate) async fn start_meeting(&self, room_id: &str, id: &str, video: bool) -> Result<(), Error> {
        let room = self.room(room_id)?;
        let http = self.client().http_client().clone();
        // Only where the homeserver names a Jitsi server (rooms offer meetings only there).
        let domain = match self.meeting_server() {
            Some(domain) => domain,
            None => named_server(&http, self.user_id()?.server_name().as_str())
                .await
                .ok_or(Error::Unsupported("meetings"))?,
        };
        let auth = jitsi_auth(&http, &domain).await;
        // A checking server needs the conference to name the room it belongs to.
        let conference = if auth { base32(room_id.as_bytes()) } else { conference_name() };
        let room_name = room.cached_display_name().map(|name| name.to_string()).unwrap_or_default();
        let mut url = format!(
            "{ELEMENT_JITSI}?confId={}#conferenceDomain=$domain&conferenceId=$conferenceId&isAudioOnly=$isAudioOnly\
             &startWithAudioMuted=$startWithAudioMuted&startWithVideoMuted=$startWithVideoMuted\
             &isVideoChannel=$isVideoChannel&displayName=$matrix_display_name&avatarUrl=$matrix_avatar_url\
             &userId=$matrix_user_id&roomId=$matrix_room_id&theme=$theme&roomName=$roomName\
             &supportsScreensharing=true&language=$org.matrix.msc2873.client_language",
            encode(&conference)
        );
        let mut data = json!({
            "conferenceId": conference,
            "roomName": room_name,
            "isAudioOnly": !video,
            "isVideoChannel": false,
            "domain": domain,
        });
        if auth {
            url.push_str(&format!("&auth={JITSI_AUTH}"));
            data["auth"] = JITSI_AUTH.into();
        }
        let content = json!({
            "id": id,
            "type": "jitsi",
            "name": "Jitsi",
            "url": url,
            "data": data,
            "creatorUserId": self.user_id()?.as_str(),
        });
        room.send_state_event_raw(LEGACY_TYPE, id, content).await?;
        Ok(())
    }
}

/// Allocates a widget ID before a meeting's run starts.
pub(crate) fn new_meeting_id() -> String {
    random_text(24, b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")
}

/// The Jitsi server the homeserver's well-known file names, if any.
pub(crate) async fn named_server(http: &matrix_sdk::reqwest::Client, server_name: &str) -> Option<String> {
    let url = format!("https://{server_name}/.well-known/matrix/client");
    let (status, known) = send_json(http.get(url).timeout(TIMEOUT)).await.ok()?;
    (200..300).contains(&status).then(|| named_jitsi(&known)).flatten()
}

/// The Jitsi server a well-known file names.
pub(crate) fn named_jitsi(known: &Value) -> Option<String> {
    ["io.element.jitsi", "im.vector.riot.jitsi"].iter().find_map(|key| {
        known[key]["preferredDomain"]
            .as_str()
            .map(str::trim)
            .filter(|domain| !domain.is_empty() && !domain.contains(['/', '?', '#', '@']))
            .map(str::to_owned)
    })
}

/// Whether a Jitsi server checks who joins (its `/.well-known/element/jitsi`).
async fn jitsi_auth(http: &matrix_sdk::reqwest::Client, domain: &str) -> bool {
    let url = format!("https://{domain}/.well-known/element/jitsi");
    send_json(http.get(url).timeout(TIMEOUT))
        .await
        .is_ok_and(|(status, known)| (200..300).contains(&status) && known["auth"] == JITSI_AUTH)
}

/// A new conference's name, as Element makes them: `Jitsi` and 24 letters.
fn conference_name() -> String {
    let letters = random_text(24, b"abcdefghijklmnopqrstuvwxyz");
    let mut chars = letters.chars();
    let first = chars.next().map(|first| first.to_ascii_uppercase()).unwrap_or('A');
    format!("Jitsi{first}{}", chars.as_str())
}

/// `length` characters from `alphabet`, at random.
pub(crate) fn random_text(length: usize, alphabet: &[u8]) -> String {
    let mut bytes = vec![0u8; length];
    // The OS's randomness; without it (never seen), the clock still makes the ID unlikely to clash.
    if getrandom::fill(&mut bytes).is_err() {
        let seed = crate::engine::now_ms().to_le_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = seed[index % seed.len()].wrapping_mul(31).wrapping_add(u8::try_from(index % 256).unwrap_or(0));
        }
    }
    bytes.iter().map(|byte| char::from(alphabet[usize::from(*byte) % alphabet.len()])).collect()
}

/// RFC 4648 base32, upper case and unpadded, as Element names a checking server's conferences.
pub(crate) fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
    }
    out
}

/// A JWT of `payload`, signed with HS256 as Element's wrapper signs it: the server reads the
/// OpenID token inside, not the signature.
pub(crate) fn jwt(payload: &Value) -> String {
    let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = base64.encode(json!({"alg": "HS256", "typ": "JWT"}).to_string());
    let body = base64.encode(payload.to_string());
    let signed = format!("{header}.{body}");
    let signature = base64.encode(hmac_sha256(b"notused", signed.as_bytes()));
    format!("{signed}.{signature}")
}

/// HMAC-SHA-256 (RFC 2104) with a key of at most one block.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    let key = if key.len() > 64 { Sha256::digest(key).to_vec() } else { key.to_vec() };
    block[..key.len()].copy_from_slice(&key);
    let pad = |with: u8| block.iter().map(|byte| byte ^ with).collect::<Vec<u8>>();
    let inner = Sha256::new().chain_update(pad(0x36)).chain_update(message).finalize();
    Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize().into()
}
