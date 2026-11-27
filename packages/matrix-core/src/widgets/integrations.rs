//! Integration managers ([`Command::OpenIntegrations`](crate::Command::OpenIntegrations)): the
//! page that adds bots, bridges and widgets to a room (Element's, Dimension), run like a widget
//! ([`WidgetBridge::Integrations`]).
//!
//! - Which: the first the homeserver's well-known file names (`m.integrations`, its `api_url`
//!   and `ui_url`), else Element's (`scalar.vector.im`).
//! - Signing in: an OpenID token registered at the manager (`{api}/register?v=1.1`) gives a
//!   token, kept per account and manager; one the manager refuses is registered again. Its
//!   terms (`M_TERMS_NOT_SIGNED`, MSC2140) are shown ([`Event::IntegrationTerms`]) and agreed to
//!   ([`Answer::Agree`]).
//! - The page (`{ui}?scalar_token=…&room_id=…&room_name=…&theme=…`) asks the app with Element's
//!   "scalar" messages, each answered with the request and its `response`: a member's
//!   membership, inviting a bot, a bot's options and power level, the member count, join rules,
//!   bridges' plumbing state, the room's widgets (reading and setting them), whether the user may
//!   send an event, whether the room is encrypted, sending an event, closing. Each request must
//!   name the room the manager was opened for. Widgets it sets are listed again
//!   ([`Event::Widgets`]).

use std::sync::Arc;

use matrix_sdk::Room;
use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedState;
use matrix_sdk::ruma::events::{MessageLikeEventType, StateEventType};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::session::{WidgetAccess, WidgetAccessKind, WidgetBridge, WidgetPage, confirm_access};
use super::{credential_http, encode, send_json, with_json};
use crate::engine::{Answer, Event, EventSink, RequestId};
use crate::{Account, AccountId, Error, Policy};

/// Element's integration manager, used when the homeserver names none.
const ELEMENT_API: &str = "https://scalar.vector.im/api";
const ELEMENT_UI: &str = "https://scalar.vector.im/";
/// How long the manager gets to answer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// An integration manager: where it answers, and its page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Manager {
    pub api: String,
    pub ui: String,
}

/// The first manager a well-known file names.
pub(crate) fn named_manager(known: &Value) -> Option<Manager> {
    known["m.integrations"]["managers"].as_array()?.iter().find_map(|manager| {
        let web = |key: &str| {
            let url = url::Url::parse(manager[key].as_str()?).ok()?;
            (url.scheme() == "https" && url.username().is_empty() && url.password().is_none() && url.host().is_some())
                .then(|| url.to_string())
        };
        let api = web("api_url")?;
        let ui = if manager["ui_url"].is_null() { api.clone() } else { web("ui_url")? };
        Some(Manager { api: api.trim_end_matches('/').to_owned(), ui })
    })
}

/// What a manager's request asks the app to do after it is answered.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum After {
    /// Nothing.
    Nothing,
    /// List the room's widgets again: the manager changed them.
    Widgets,
    /// Close the manager.
    Close,
}

impl Account {
    /// The integration manager the homeserver names, else Element's.
    async fn manager(&self) -> Option<Manager> {
        let http = self.client().http_client().clone();
        let server = self.user_id().map(|id| id.server_name().to_string()).unwrap_or_default();
        let asked = http.get(format!("https://{server}/.well-known/matrix/client")).timeout(TIMEOUT);
        let known =
            send_json(asked).await.ok().filter(|(status, _)| (200..300).contains(status)).map(|(_, known)| known);
        known.as_ref().and_then(named_manager)
    }

    /// The token the manager knows the user by: the one kept, else a new one.
    async fn manager_token(
        &self,
        manager: &Manager,
        terms: &mut (dyn FnMut(Vec<Policy>) -> bool + Send),
        agreed: &mut mpsc::UnboundedReceiver<Answer>,
    ) -> Result<String, Error> {
        let key = format!("bettermatrix.integrations_token:{}", manager.api).into_bytes();
        let store = self.client().state_store();
        let kept = store.get_custom_value(&key).await.ok().flatten().and_then(|bytes| String::from_utf8(bytes).ok());
        if let Some(token) = kept {
            match self.check_token(manager, &token).await {
                Ok(()) => return Ok(token),
                Err(Refused::Terms) => {
                    self.agree_to_terms(manager, &token, terms, agreed).await?;
                    return Ok(token);
                }
                Err(Refused::Token) => {}
                Err(Refused::Failed(error)) => return Err(error),
            }
        }
        let token = self.register_at(manager).await?;
        match self.check_token(manager, &token).await {
            Ok(()) => {}
            Err(Refused::Terms) => self.agree_to_terms(manager, &token, terms, agreed).await?,
            Err(Refused::Token) => return Err(Error::Integrations("the manager refused the account".to_owned())),
            Err(Refused::Failed(error)) => return Err(error),
        }
        let _ = store.set_custom_value_no_read(&key, token.clone().into_bytes()).await;
        Ok(token)
    }

    /// Registers the user at the manager with an OpenID token proving who they are.
    async fn register_at(&self, manager: &Manager) -> Result<String, Error> {
        use matrix_sdk::ruma::api::client::account::request_openid_token;
        let openid = self.client().send(request_openid_token::v3::Request::new(self.user_id()?)).await?;
        let body = json!({
            "access_token": openid.access_token,
            "token_type": openid.token_type.to_string(),
            "matrix_server_name": openid.matrix_server_name.to_string(),
            "expires_in": openid.expires_in.as_secs(),
        });
        let asked = credential_http()?.post(format!("{}/register?v=1.1", manager.api)).timeout(TIMEOUT);
        let (_, answer) =
            send_json(with_json(asked, &body)).await.map_err(|error| Error::Integrations(error.to_string()))?;
        answer["scalar_token"]
            .as_str()
            .or_else(|| answer["token"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| Error::Integrations(answer["error"].as_str().unwrap_or("no token").to_owned()))
    }

    /// Whether the manager takes the token.
    async fn check_token(&self, manager: &Manager, token: &str) -> Result<(), Refused> {
        let url = format!("{}/account?v=1.1&scalar_token={}", manager.api, encode(token));
        let http = credential_http().map_err(Refused::Failed)?;
        let asked = http.get(url).timeout(TIMEOUT);
        let (status, answer) =
            send_json(asked).await.map_err(|error| Refused::Failed(Error::Integrations(error.to_string())))?;
        if (200..300).contains(&status) {
            return Ok(());
        }
        if answer["errcode"] == "M_TERMS_NOT_SIGNED" {
            return Err(Refused::Terms);
        }
        if matches!(status, 401 | 403) {
            return Err(Refused::Token);
        }
        Err(Refused::Failed(Error::Integrations(format!("status {status}"))))
    }

    /// Shows the manager's terms and, once the user agrees, agrees to them.
    async fn agree_to_terms(
        &self,
        manager: &Manager,
        token: &str,
        terms: &mut (dyn FnMut(Vec<Policy>) -> bool + Send),
        agreed: &mut mpsc::UnboundedReceiver<Answer>,
    ) -> Result<(), Error> {
        let http = credential_http()?;
        let url = terms_url(&manager.api).ok_or_else(|| Error::Integrations(manager.api.clone()))?;
        let failed = |error: matrix_sdk::reqwest::Error| Error::Integrations(error.to_string());
        let (status, answer) = send_json(http.get(&url).bearer_auth(token).timeout(TIMEOUT)).await.map_err(failed)?;
        if !(200..300).contains(&status) {
            return Err(Error::Integrations(format!("status {status}")));
        }
        let policies = policies(&answer);
        if !policies.is_empty() && terms(policies.clone()) {
            loop {
                match agreed.recv().await {
                    Some(Answer::Agree) => break,
                    Some(_) => {}
                    None => return Err(Error::Cancelled),
                }
            }
        }
        let accepts: Vec<&str> = policies.iter().map(|policy| policy.url.as_str()).collect();
        let agree = with_json(http.post(&url).bearer_auth(token).timeout(TIMEOUT), &json!({ "user_accepts": accepts }));
        let (status, _) = send_json(agree).await.map_err(failed)?;
        if !(200..300).contains(&status) {
            return Err(Error::Integrations(format!("status {status}")));
        }
        Ok(())
    }
}

/// Why the manager refused a token.
enum Refused {
    /// The user hasn't agreed to its terms.
    Terms,
    /// It doesn't know the token.
    Token,
    /// It couldn't be asked.
    Failed(Error),
}

/// A manager's terms (MSC2140): each policy's name and link, in English where it offers it.
pub(crate) fn policies(answer: &Value) -> Vec<Policy> {
    let Some(listed) = answer["policies"].as_object() else { return Vec::new() };
    listed
        .values()
        .filter_map(|policy| {
            let languages = policy.as_object()?;
            let text = languages.get("en").or_else(|| languages.values().find(|value| value.is_object()))?;
            Some(Policy { name: text["name"].as_str()?.to_owned(), url: text["url"].as_str()?.to_owned() })
        })
        .collect()
}

/// Opens the integration manager for a room and answers its page until it closes.
pub(crate) async fn run(
    account: Account,
    id: AccountId,
    room_id: String,
    request: RequestId,
    sink: Arc<dyn EventSink>,
    mut answers: mpsc::UnboundedReceiver<Answer>,
) -> Result<(), Error> {
    let room = account.room(&room_id)?;
    let configured = account.manager().await;
    let hosted = configured.is_none();
    let manager = configured.unwrap_or_else(|| Manager { api: ELEMENT_API.to_owned(), ui: ELEMENT_UI.to_owned() });
    let mut url = url::Url::parse(&manager.ui).map_err(|error| Error::Integrations(error.to_string()))?;
    let api = url::Url::parse(&manager.api).map_err(|error| Error::Integrations(error.to_string()))?;
    let mut origin = api.origin().ascii_serialization();
    if api.origin() != url.origin() {
        origin = format!("{origin} · {}", url.origin().ascii_serialization());
    }
    let room_name = room.cached_display_name().map_or_else(|| room_id.clone(), |name| name.to_string());
    let access = WidgetAccess {
        name: String::new(),
        origin,
        user_id: account.user_id()?.to_string(),
        room: room_name.clone(),
        kind: WidgetAccessKind::Manager { hosted },
    };
    confirm_access(&*sink, request, access, &mut answers).await?;
    let mut terms = |policies: Vec<Policy>| {
        sink.emit(Event::IntegrationTerms { request, policies });
        true
    };
    let token = account.manager_token(&manager, &mut terms, &mut answers).await?;
    url.query_pairs_mut()
        .append_pair("scalar_token", &token)
        .append_pair("room_id", &room_id)
        .append_pair("room_name", &room_name)
        .append_pair("theme", "light");
    let page = WidgetPage { url: url.to_string(), title: String::new(), bridge: WidgetBridge::Integrations };
    sink.emit(Event::WidgetPage { request, page });
    while let Some(answer) = answers.recv().await {
        let Answer::WidgetMessage(message) = answer else { continue };
        let Ok(asked) = serde_json::from_str::<Value>(&message) else { continue };
        // Only the page's requests: a reply (even an empty one) is never answered again.
        if asked.get("response").is_some() || asked.get("action").is_none() {
            continue;
        }
        let (response, after) = answer_request(&account, &room, &asked).await;
        let mut reply = asked;
        reply["response"] = response;
        sink.emit(Event::WidgetMessage { request, message: reply.to_string() });
        match after {
            After::Nothing => {}
            After::Widgets => {
                if let Some(widgets) = account.widgets_after_write(&room_id).await {
                    sink.emit(Event::Widgets { account: id.clone(), room_id: room_id.clone(), widgets });
                }
            }
            After::Close => break,
        }
    }
    Ok(())
}

/// An error answer, as Element gives them.
fn error(message: impl Into<String>) -> Value {
    json!({ "error": { "message": message.into() } })
}

/// The answer to one of the manager's requests, and what follows it.
pub(crate) async fn answer_request(account: &Account, room: &Room, asked: &Value) -> (Value, After) {
    let action = asked["action"].as_str().unwrap_or_default();
    if action == "close_scalar" {
        return (json!({ "success": true }), After::Close);
    }
    // Only the room it was opened for.
    if asked["room_id"].as_str() != Some(room.room_id().as_str()) {
        return (error("Room not visible"), After::Nothing);
    }
    match act(account, room, action, asked).await {
        Ok(answer) => answer,
        Err(failure) => (error(failure.to_string()), After::Nothing),
    }
}

/// Does what one request asks.
async fn act(account: &Account, room: &Room, action: &str, asked: &Value) -> Result<(Value, After), Error> {
    use matrix_sdk::ruma::events::room::member::MembershipState;
    use matrix_sdk::ruma::{Int, UserId};
    let user = || -> Result<&UserId, Error> {
        let user = asked["user_id"].as_str().ok_or_else(|| Error::Integrations("missing user_id".to_owned()))?;
        Ok(<&UserId>::try_from(user)?)
    };
    let done = (json!({ "success": true }), After::Nothing);
    Ok(match action {
        "membership_state" => (state_content(room, "m.room.member", user()?.as_str()).await, After::Nothing),
        "invite" => {
            let user = user()?;
            if !room
                .get_member(user)
                .await?
                .is_some_and(|member| matches!(member.membership(), MembershipState::Join | MembershipState::Invite))
            {
                room.invite_user_by_id(user).await?;
            }
            done
        }
        "kick" => {
            let user = user()?;
            if room.get_member(user).await?.is_some_and(|member| {
                matches!(member.membership(), MembershipState::Join | MembershipState::Invite | MembershipState::Knock)
            }) {
                room.kick_user(user, asked["reason"].as_str()).await?;
            }
            done
        }
        "bot_options" => (state_content(room, "m.room.bot.options", &format!("_{}", user()?)).await, After::Nothing),
        "set_bot_options" => {
            let key = format!("_{}", user()?);
            room.send_state_event_raw("m.room.bot.options", &key, asked["content"].clone()).await?;
            done
        }
        "set_bot_power" => {
            let level = asked["level"].as_i64().ok_or_else(|| Error::Integrations("missing level".to_owned()))?;
            let user = user()?;
            let level = Int::new_saturating(level);
            let keeps_level = asked["ignoreIfGreater"] == true && room.get_user_power_level(user).await? >= level;
            if !keeps_level {
                room.update_power_levels(vec![(user, level)]).await?;
            }
            done
        }
        "get_membership_count" => (json!(room.joined_members_count()), After::Nothing),
        "join_rules_state" => (state_content(room, "m.room.join_rules", "").await, After::Nothing),
        "set_plumbing_state" => {
            let status = asked["status"].clone();
            room.send_state_event_raw("m.room.plumbing", "", json!({ "status": status })).await?;
            done
        }
        "get_widgets" => (json!(widget_events(account, room).await), After::Nothing),
        "set_widget" => {
            if asked["userWidget"].as_bool() == Some(true) {
                return Err(Error::Integrations("account widgets aren't supported".to_owned()));
            }
            let id = asked["widget_id"].as_str().ok_or_else(|| Error::Integrations("missing widget_id".to_owned()))?;
            let content = widget_content(asked)?;
            if asked["url"].is_null() {
                account.remove_widget(room.room_id().as_str(), id).await?;
            } else {
                room.send_state_event_raw(super::LEGACY_TYPE, id, content).await?;
            }
            (json!({ "success": true }), After::Widgets)
        }
        "can_send_event" => {
            let kind = asked["event_type"].as_str().unwrap_or_default();
            let me = room.own_user_id();
            let levels = room.power_levels().await.ok();
            let may = levels.is_some_and(|levels| {
                if asked["is_state"].as_bool() == Some(true) {
                    levels.user_can_send_state(me, StateEventType::from(kind))
                } else {
                    levels.user_can_send_message(me, MessageLikeEventType::from(kind))
                }
            });
            if may {
                (json!(true), After::Nothing)
            } else {
                (error("You do not have permission to send this event"), After::Nothing)
            }
        }
        "get_room_enc_state" => (json!(room.encryption_state().is_encrypted()), After::Nothing),
        "send_event" => {
            // Only the state integrations keep, never messages or the room's own settings.
            let kind = asked["type"].as_str().unwrap_or_default();
            let key = asked["state_key"].as_str();
            let (Some(key), true) = (key, SENDABLE.contains(&kind)) else {
                return Ok((error("This event can't be sent"), After::Nothing));
            };
            if !asked["content"].is_object() {
                return Ok((error("missing content"), After::Nothing));
            }
            let event_id = room.send_state_event_raw(kind, key, asked["content"].clone()).await?.event_id;
            let after = if super::TYPES.contains(&kind) { After::Widgets } else { After::Nothing };
            (json!({ "room_id": room.room_id(), "event_id": event_id }), after)
        }
        _ => (error("Unknown action"), After::Nothing),
    })
}

/// The state a manager may send (`send_event`): widgets, their layout, bots' options and
/// bridges' plumbing.
pub(crate) const SENDABLE: [&str; 5] =
    [super::LEGACY_TYPE, super::SPEC_TYPE, "io.element.widgets.layout", "m.room.bot.options", "m.room.plumbing"];

/// Where a manager keeps its terms (MSC2140): at its origin, beside its API.
pub(crate) fn terms_url(api: &str) -> Option<String> {
    let api = url::Url::parse(api).ok()?;
    Some(format!("{}/_matrix/integrations/v1/terms", api.origin().ascii_serialization()))
}

/// A widget's state content from a manager; only an explicit null removes it.
pub(crate) fn widget_content(asked: &Value) -> Result<Value, Error> {
    if asked.get("url").is_some_and(Value::is_null) {
        return Ok(json!({}));
    }
    let url = asked["url"]
        .as_str()
        .filter(|url| url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")))
        .ok_or(Error::InvalidWidgetUrl)?;
    let kind = asked["type"].as_str().ok_or_else(|| Error::Integrations("missing widget type".to_owned()))?;
    let mut content = json!({
        "type": kind,
        "url": url,
        "name": asked["name"],
        "data": if asked["data"].is_object() { asked["data"].clone() } else { json!({}) },
    });
    if let Some(avatar) = asked["avatar_url"].as_str() {
        content["avatar_url"] = avatar.into();
    }
    Ok(content)
}

/// A state event's content, `null` when the room has none.
async fn state_content(room: &Room, kind: &str, key: &str) -> Value {
    let event = room.get_state_event(StateEventType::from(kind), key).await.ok().flatten();
    let json: Option<Value> = match event {
        Some(RawAnySyncOrStrippedState::Sync(raw)) => raw.deserialize_as_unchecked().ok(),
        Some(RawAnySyncOrStrippedState::Stripped(raw)) => raw.deserialize_as_unchecked().ok(),
        None => None,
    };
    json.map(|event| event["content"].clone()).filter(|content| !content.is_null()).unwrap_or(Value::Null)
}

/// The room's widget state events, as a manager reads them.
async fn widget_events(account: &Account, room: &Room) -> Vec<Value> {
    let state = account.widget_state(room, super::Freshness::Known).await.unwrap_or_default();
    state
        .into_iter()
        .map(|(kind, key, content, sender)| {
            json!({ "type": kind, "state_key": key, "sender": sender, "room_id": room.room_id(), "content": content })
        })
        .collect()
}
