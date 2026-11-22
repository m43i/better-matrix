//! A room's widgets: web apps pinned to a room as state (`im.vector.modular.widgets`, the type
//! Element writes, and `m.widget`), keyed by the widget's ID. Apps open them in a browser with
//! the URL's template variables filled in (`$matrix_user_id`, …); Jitsi widgets open their
//! meeting directly. Integration managers are not supported (they need an embedded web view).

use matrix_sdk::Room;
use matrix_sdk::ruma::events::StateEventType;
use serde_json::{Value, json};

use crate::Error;
use crate::account::Account;

/// The state type Element writes widgets as, and the one this app writes.
const LEGACY_TYPE: &str = "im.vector.modular.widgets";
/// The specced state type, read too.
const SPEC_TYPE: &str = "m.widget";

/// A widget pinned to a room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Widget {
    /// Its ID (the state key).
    pub id: String,
    /// Its name, or its kind when it has none.
    pub name: String,
    /// What it is: `m.custom`, `jitsi`, `m.etherpad`, `m.grafana`, …
    pub kind: String,
    /// Where it opens, its template variables filled in for this user.
    pub url: String,
    /// The host it opens on, to show before opening.
    pub host: String,
    /// Who added it.
    pub creator: Option<String>,
    /// Whether the user's power level lets them remove it, under every type it is pinned as.
    pub can_remove: bool,
}

/// A room's widgets, and whether the user may add one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Widgets {
    /// By name.
    pub widgets: Vec<Widget>,
    /// Whether the user's power level lets them add a widget (as the type Element writes).
    pub can_add: bool,
}

/// Who a widget's template variables describe.
#[derive(Debug)]
pub(crate) struct Viewer<'a> {
    pub user_id: &'a str,
    pub display_name: &'a str,
    pub avatar_url: &'a str,
    pub device_id: &'a str,
    pub room_id: &'a str,
    pub homeserver: &'a str,
}

impl Account {
    /// The room's widgets.
    pub async fn widgets(&self, room_id: &str) -> Result<Widgets, Error> {
        let room = self.room(room_id)?;
        let me = self.user_id()?;
        let member = room.get_member_no_sync(&me).await.ok().flatten();
        let display_name = member.as_ref().and_then(|m| m.display_name().map(str::to_owned)).unwrap_or_default();
        let avatar_url = member.as_ref().and_then(|m| m.avatar_url().map(ToString::to_string)).unwrap_or_default();
        let device_id = self.client().device_id().map(ToString::to_string).unwrap_or_default();
        let homeserver = self.client().homeserver().to_string();
        let viewer = Viewer {
            user_id: me.as_str(),
            display_name: &display_name,
            avatar_url: &avatar_url,
            device_id: &device_id,
            room_id,
            homeserver: homeserver.trim_end_matches('/'),
        };
        let state = widget_state(&room).await?;
        let levels = room.power_levels().await.ok();
        let may_send = |kind: &str| {
            levels.as_ref().is_some_and(|levels| levels.user_can_send_state(&me, StateEventType::from(kind)))
        };
        let mut found: Vec<(&str, &Value, Option<String>)> = Vec::new();
        for (_, state_key, content, sender) in &state {
            if !found.iter().any(|(id, ..)| *id == state_key.as_str()) {
                found.push((state_key, content, sender.clone()));
            }
        }
        let mut widgets: Vec<Widget> = found
            .into_iter()
            .filter_map(|(id, content, sender)| {
                let widget = widget(id, content, sender, &viewer)?;
                // Removing writes every type the widget is pinned as.
                let can_remove = state.iter().filter(|(_, key, ..)| key == id).all(|(kind, ..)| may_send(kind));
                Some(Widget { can_remove, ..widget })
            })
            .collect();
        widgets.sort_by_cached_key(|widget| widget.name.to_lowercase());
        Ok(Widgets { widgets, can_add: may_send(LEGACY_TYPE) })
    }

    /// The room's widgets after a change was written, read up to three times, a second apart:
    /// the change stands even when the list can't be read.
    pub(crate) async fn widgets_after_write(&self, room_id: &str) -> Option<Widgets> {
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            if let Ok(widgets) = self.widgets(room_id).await {
                return Some(widgets);
            }
        }
        None
    }

    /// Pins a web page to the room as a custom widget.
    pub async fn add_widget(&self, room_id: &str, name: &str, url: &str) -> Result<(), Error> {
        let room = self.room(room_id)?;
        let parsed = url::Url::parse(url.trim()).map_err(|_| Error::InvalidWidgetUrl)?;
        if !matches!(parsed.scheme(), "https" | "http") {
            return Err(Error::InvalidWidgetUrl);
        }
        let id = format!("{}_{}", self.user_id()?, crate::engine::now_ms());
        let name = if name.trim().is_empty() { parsed.host_str().unwrap_or_default() } else { name.trim() };
        let content = json!({
            "id": id,
            "type": "m.custom",
            "url": url.trim(),
            "name": name,
            "data": {},
            "creatorUserId": self.user_id()?.as_str(),
        });
        room.send_state_event_raw(LEGACY_TYPE, &id, content).await?;
        Ok(())
    }

    /// Takes a widget off the room, under every type it is pinned as.
    pub async fn remove_widget(&self, room_id: &str, widget_id: &str) -> Result<(), Error> {
        let room = self.room(room_id)?;
        for (kind, id, ..) in widget_state(&room).await? {
            if id == widget_id {
                room.send_state_event_raw(kind, widget_id, json!({})).await?;
            }
        }
        Ok(())
    }
}

/// A room's widget state as the server has it now (the room list's sliding sync doesn't bring
/// widget state, and a change just sent may not have come back through sync yet): each event
/// that still holds something, as its type, state key, content and sender.
async fn widget_state(room: &Room) -> Result<Vec<(&'static str, String, Value, Option<String>)>, Error> {
    use matrix_sdk::ruma::api::client::state::get_state_events;
    let response = room.client().send(get_state_events::v3::Request::new(room.room_id().to_owned())).await?;
    Ok(response.room_state.iter().filter_map(|raw| read_state(raw.json().get())).collect())
}

/// One state event, if it is a widget's that still holds something.
fn read_state(json: &str) -> Option<(&'static str, String, Value, Option<String>)> {
    let event: Value = serde_json::from_str(json).ok()?;
    let kind = [LEGACY_TYPE, SPEC_TYPE].into_iter().find(|kind| event["type"] == *kind)?;
    let content = event.get("content")?.clone();
    if !content.as_object().is_some_and(|content| !content.is_empty()) {
        return None;
    }
    let state_key = event["state_key"].as_str()?.to_owned();
    let sender = event["sender"].as_str().map(str::to_owned);
    Some((kind, state_key, content, sender))
}

/// A widget from its state content, `None` when it has no usable URL.
pub(crate) fn widget(id: &str, content: &Value, sender: Option<String>, viewer: &Viewer<'_>) -> Option<Widget> {
    let kind = content.get("type").and_then(Value::as_str).unwrap_or("m.custom").to_owned();
    let data = content.get("data").cloned().unwrap_or_else(|| json!({}));
    let url = if is_jitsi(&kind) { jitsi_url(&data) } else { None };
    let url = match url {
        Some(url) => url,
        None => fill(content.get("url").and_then(Value::as_str)?, id, &data, viewer),
    };
    let parsed = url::Url::parse(&url).ok().filter(|url| matches!(url.scheme(), "https" | "http"))?;
    let name = content
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(|| kind.clone(), str::to_owned);
    let creator = content.get("creatorUserId").and_then(Value::as_str).map(str::to_owned).or(sender);
    let host = parsed.host_str().unwrap_or_default().to_owned();
    Some(Widget { id: id.to_owned(), name, kind, host, url, creator, can_remove: false })
}

fn is_jitsi(kind: &str) -> bool {
    matches!(kind, "jitsi" | "m.jitsi")
}

/// A Jitsi widget's meeting: its domain and conference, opened directly rather than through
/// Element's wrapper page, which only works inside Element.
fn jitsi_url(data: &Value) -> Option<String> {
    let domain = data.get("domain").and_then(Value::as_str)?;
    let conference = data.get("conferenceId").and_then(Value::as_str)?;
    let mut url = url::Url::parse(&format!("https://{domain}/")).ok()?;
    url.path_segments_mut().ok()?.push(conference);
    Some(url.to_string())
}

/// Fills a widget URL's template variables: the viewer's, then the widget's own `data`, each
/// URL-encoded (as Element does). Unknown variables stay as they are.
pub(crate) fn fill(template: &str, widget_id: &str, data: &Value, viewer: &Viewer<'_>) -> String {
    let mut values: Vec<(String, String)> = vec![
        ("matrix_user_id".into(), viewer.user_id.into()),
        ("matrix_room_id".into(), viewer.room_id.into()),
        ("matrix_display_name".into(), viewer.display_name.into()),
        ("matrix_avatar_url".into(), viewer.avatar_url.into()),
        ("matrix_widget_id".into(), widget_id.into()),
        ("org.matrix.msc2873.client_id".into(), "chat.bettermatrix".into()),
        ("org.matrix.msc2873.client_theme".into(), "light".into()),
        ("org.matrix.msc2873.client_language".into(), "en".into()),
        ("org.matrix.msc3819.matrix_device_id".into(), viewer.device_id.into()),
        ("org.matrix.msc4039.matrix_base_url".into(), viewer.homeserver.into()),
    ];
    if let Some(data) = data.as_object() {
        for (key, value) in data {
            let value = match value {
                Value::String(text) => text.clone(),
                Value::Number(_) | Value::Bool(_) => value.to_string(),
                _ => continue,
            };
            values.push((key.clone(), value));
        }
    }
    // Longest names first, so `$matrix_user_id` isn't read as a shorter variable's prefix.
    values.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    let mut url = template.to_owned();
    for (name, value) in values {
        let encoded: String = url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
        url = url.replace(&format!("${name}"), &encoded.replace('+', "%20"));
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer() -> Viewer<'static> {
        Viewer {
            user_id: "@ada:example.org",
            display_name: "Ada Lovelace",
            avatar_url: "",
            device_id: "DEVICE",
            room_id: "!room:example.org",
            homeserver: "https://example.org",
        }
    }

    #[test]
    fn a_widget_url_should_name_the_viewer_and_room() {
        let url = fill(
            "https://w.example/?u=$matrix_user_id&r=$matrix_room_id&n=$matrix_display_name",
            "w1",
            &json!({}),
            &viewer(),
        );
        assert_eq!(url, "https://w.example/?u=%40ada%3Aexample.org&r=%21room%3Aexample.org&n=Ada%20Lovelace");
    }

    #[test]
    fn a_widget_url_should_take_values_from_the_widgets_data() {
        let url =
            fill("https://pad.example/p/$padName?w=$matrix_widget_id", "w1", &json!({"padName": "notes"}), &viewer());
        assert_eq!(url, "https://pad.example/p/notes?w=w1");
    }

    #[test]
    fn a_jitsi_widget_should_open_its_meeting_directly() {
        let content = json!({
            "type": "jitsi",
            "url": "https://app.element.io/jitsi.html#conferenceDomain=$domain",
            "name": "Standup",
            "data": {"domain": "meet.example.org", "conferenceId": "TeamStandup"}
        });
        let widget = widget("j1", &content, None, &viewer()).expect("widget");
        assert_eq!(widget.url, "https://meet.example.org/TeamStandup");
        assert_eq!(widget.host, "meet.example.org");
    }

    #[test]
    fn a_widget_without_a_web_url_should_be_left_out() {
        assert!(widget("w", &json!({"type": "m.custom", "url": "javascript:alert(1)"}), None, &viewer()).is_none());
        assert!(widget("w", &json!({"type": "m.custom"}), None, &viewer()).is_none());
    }

    #[test]
    fn widget_state_should_skip_other_state_and_removed_widgets() {
        let widget =
            r#"{"type":"im.vector.modular.widgets","state_key":"w1","sender":"@g:x","content":{"url":"https://w"}}"#;
        let removed = r#"{"type":"m.widget","state_key":"w2","sender":"@g:x","content":{}}"#;
        let name = r#"{"type":"m.room.name","state_key":"","sender":"@g:x","content":{"name":"Room"}}"#;
        assert_eq!(read_state(widget).map(|(kind, id, ..)| (kind, id)), Some((LEGACY_TYPE, "w1".to_owned())));
        assert!(read_state(removed).is_none() && read_state(name).is_none());
    }

    #[test]
    fn a_nameless_widget_should_go_by_its_kind() {
        let widget = widget("w", &json!({"type": "m.grafana", "url": "https://g.example"}), None, &viewer());
        assert_eq!(widget.map(|widget| widget.name), Some("m.grafana".to_owned()));
    }
}
