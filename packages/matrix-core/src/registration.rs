//! Creating an account. An OAuth homeserver creates it on its own page in the browser (a browser
//! sign-in that asks for registration, [`Command::LoginOAuth`]'s `create`); a password homeserver
//! through the client-server API, where the server asks for stages first (interactive auth).
//!
//! The engine takes the stages it can by itself (`m.login.dummy`), asks the app for a
//! registration token, agreement to the server's policies or an email address to confirm (the
//! server sends a link; servers such as Synapse have no page for that stage), and sends the user
//! to the server's own page for the rest (a captcha): Matrix's fallback pages. Flows with a phone
//! number aren't taken. Each question is a [`RegistrationStep`] in [`Event::Registration`], answered with
//! [`Command::Answer`].
//!
//! [`Command::LoginOAuth`]: crate::Command::LoginOAuth
//! [`Event::Registration`]: crate::Event::Registration
//! [`Command::Answer`]: crate::Command::Answer

use matrix_sdk::ruma::api::client::account::{register, request_registration_token_via_email};
use matrix_sdk::ruma::api::client::uiaa::{self, AuthType, UiaaInfo};
use tokio::sync::mpsc;

use crate::account::{Account, Holder, NewStore};
use crate::model::Password;
use crate::session::Sessions;
use crate::store::StoreLayout;
use crate::{Answer, DEVICE_DISPLAY_NAME, Error};

/// What creating an account on a password homeserver needs.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RegisterRequest {
    /// The resolved homeserver URL, from [`Homeserver::url`](crate::Homeserver::url).
    pub homeserver: String,
    /// The username (the user ID's localpart) to take.
    pub username: String,
    /// The new account's password.
    pub password: Password,
}

// Hand-written so the request reads without its password.
impl std::fmt::Debug for RegisterRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegisterRequest")
            .field("homeserver", &self.homeserver)
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// What the server asks before it creates the account.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum RegistrationStep {
    /// The server takes new accounts only with a token its admins hand out: answer
    /// [`Answer::RegistrationToken`].
    Token,
    /// Agree to these policies: answer [`Answer::Agree`].
    Terms(Vec<Policy>),
    /// The server wants an email address it confirms: answer [`Answer::Email`].
    Email,
    /// The server sent a link to `address`: answer [`Answer::Continue`] once it is opened.
    EmailSent {
        /// Where the link went.
        address: String,
        /// Asked again: the server hadn't seen the link opened yet.
        unconfirmed: bool,
    },
    /// Finish a stage on the server's page in the browser (a captcha, a confirmed email address);
    /// [`Event::OpenUrl`](crate::Event::OpenUrl) carries the same URL. Answer
    /// [`Answer::Continue`] once done there.
    Browser {
        /// The page.
        url: String,
    },
}

/// One of a server's policies to agree to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Policy {
    /// Its name, in English where the server offers it.
    pub name: String,
    /// Where to read it.
    pub url: String,
}

/// Which way a password homeserver creates accounts, found by asking it to start one.
pub(crate) async fn password_registration(client: &matrix_sdk::Client) -> bool {
    let mut request = register::v3::Request::new();
    request.inhibit_login = true;
    match client.send(request).await {
        Ok(_) => true,
        Err(error) => error.as_uiaa_response().is_some_and(|info| chosen_flow(info).is_some()),
    }
}

/// The flow to follow among those the app can take: the one with the fewest stages the user
/// must finish in the browser, then the shortest.
fn chosen_flow(info: &UiaaInfo) -> Option<&[AuthType]> {
    info.flows
        .iter()
        .filter(|flow| !flow.stages.contains(&AuthType::Msisdn))
        .min_by_key(|flow| (flow.stages.iter().filter(|stage| in_browser(stage)).count(), flow.stages.len()))
        .map(|flow| flow.stages.as_slice())
}

/// Whether the user finishes a stage on the server's page rather than in the app.
fn in_browser(stage: &AuthType) -> bool {
    !matches!(stage, AuthType::Dummy | AuthType::RegistrationToken | AuthType::Terms | AuthType::EmailIdentity)
}

/// The next stage of the chosen flow the server hasn't seen completed.
fn next_stage(info: &UiaaInfo) -> Option<AuthType> {
    chosen_flow(info)?.iter().find(|stage| !info.completed.contains(stage)).cloned()
}

/// The policies an `m.login.terms` stage lists, in English where offered, else the first language.
fn policies(info: &UiaaInfo) -> Vec<Policy> {
    let params = info.params.as_ref().and_then(|raw| serde_json::from_str::<serde_json::Value>(raw.get()).ok());
    let Some(listed) = params.as_ref().and_then(|p| p.pointer("/m.login.terms/policies")).and_then(|p| p.as_object())
    else {
        return Vec::new();
    };
    listed
        .values()
        .filter_map(|policy| {
            let languages = policy.as_object()?;
            let text = languages.get("en").or_else(|| languages.values().find(|v| v.is_object()))?;
            Some(Policy { name: text.get("name")?.as_str()?.to_owned(), url: text.get("url")?.as_str()?.to_owned() })
        })
        .collect()
}

/// The server's own page for a stage (Matrix's fallback for clients that can't do it).
fn fallback_url(homeserver: &url::Url, stage: &AuthType, session: &str) -> Result<String, Error> {
    let mut url = homeserver
        .join(&format!("_matrix/client/v3/auth/{}/fallback/web", stage.as_str()))
        .map_err(|e| Error::Account(e.to_string()))?;
    url.query_pairs_mut().append_pair("session", session);
    Ok(url.to_string())
}

/// Waits for the app's answer to a step; a closed channel means the command was cancelled.
async fn answer(answers: &mut mpsc::UnboundedReceiver<Answer>) -> Result<Answer, Error> {
    answers.recv().await.ok_or(Error::Cancelled)
}

impl Account {
    /// Creates an account on a password homeserver and signs it in, asking `ask` for what the
    /// server wants first and waiting on `answers` for the replies.
    pub(crate) async fn register(
        layout: &StoreLayout,
        sessions: &Sessions,
        holder: Holder,
        request: &RegisterRequest,
        ask: &(dyn Fn(RegistrationStep) + Send + Sync),
        mut answers: mpsc::UnboundedReceiver<Answer>,
    ) -> Result<Self, Error> {
        let (store, passphrase) = (NewStore::new(layout)?, crate::session::new_passphrase()?);
        let client = async {
            let (client, crypto) =
                crate::account::build_client(&request.homeserver, &store.path(), &passphrase, sessions, holder).await?;
            let mut auth = None;
            // The address being confirmed, and the credentials the server sent the link under.
            let mut email: Option<(String, uiaa::ThirdpartyIdCredentials)> = None;
            loop {
                let mut register = register::v3::Request::new();
                register.username = Some(request.username.trim().to_owned());
                register.password = Some(request.password.0.clone());
                register.initial_device_display_name = Some(DEVICE_DISPLAY_NAME.to_owned());
                register.auth = auth.take();
                let error = match client.matrix_auth().register(register).await {
                    Ok(_) if client.user_id().is_some() => return Ok((client, crypto)),
                    Ok(_) => return Err(Error::NoUserId),
                    Err(error) => error,
                };
                let Some(info) = error.as_uiaa_response() else { return Err(error.into()) };
                let session = info.session.clone();
                let stage = next_stage(info).ok_or(Error::RegistrationUnsupported)?;
                match (&info.auth_error, &stage, &email) {
                    // The link wasn't opened yet: ask again, the same link still counts.
                    (Some(_), AuthType::EmailIdentity, Some(_)) => {}
                    (Some(refused), ..) => return Err(Error::Account(refused.message.clone())),
                    (None, ..) => {}
                }
                auth = Some(match stage {
                    AuthType::Dummy => {
                        let mut stage = uiaa::Dummy::new();
                        stage.session = session;
                        uiaa::AuthData::Dummy(stage)
                    }
                    AuthType::RegistrationToken => {
                        ask(RegistrationStep::Token);
                        let Answer::RegistrationToken(token) = answer(&mut answers).await? else {
                            return Err(Error::Cancelled);
                        };
                        let mut stage = uiaa::RegistrationToken::new(token.trim().to_owned());
                        stage.session = session;
                        uiaa::AuthData::RegistrationToken(stage)
                    }
                    AuthType::Terms => {
                        ask(RegistrationStep::Terms(policies(info)));
                        let Answer::Agree = answer(&mut answers).await? else { return Err(Error::Cancelled) };
                        let mut stage = uiaa::Terms::new();
                        stage.session = session;
                        uiaa::AuthData::Terms(stage)
                    }
                    AuthType::EmailIdentity => {
                        let unconfirmed = email.is_some();
                        if email.is_none() {
                            ask(RegistrationStep::Email);
                            let Answer::Email(address) = answer(&mut answers).await? else {
                                return Err(Error::Cancelled);
                            };
                            let address = address.trim().to_owned();
                            let secret = matrix_sdk::ruma::ClientSecret::new();
                            let request = request_registration_token_via_email::v3::Request::new(
                                secret.clone(),
                                address.clone(),
                                matrix_sdk::ruma::uint!(1),
                            );
                            let sent = client.send(request).await?;
                            email = Some((address, uiaa::ThirdpartyIdCredentials::new(sent.sid, secret)));
                        }
                        let Some((address, credentials)) = &email else { return Err(Error::Cancelled) };
                        ask(RegistrationStep::EmailSent { address: address.clone(), unconfirmed });
                        let Answer::Continue = answer(&mut answers).await? else { return Err(Error::Cancelled) };
                        // ruma offers no constructor for it.
                        let stage = serde_json::json!({
                            "type": "m.login.email.identity",
                            "threepid_creds": credentials,
                            "session": session,
                        });
                        uiaa::AuthData::EmailIdentity(
                            serde_json::from_value(stage).map_err(|e| Error::Account(e.to_string()))?,
                        )
                    }
                    stage => {
                        let session = session.ok_or(Error::RegistrationUnsupported)?;
                        ask(RegistrationStep::Browser { url: fallback_url(&client.homeserver(), &stage, &session)? });
                        let Answer::Continue = answer(&mut answers).await? else { return Err(Error::Cancelled) };
                        uiaa::AuthData::FallbackAcknowledgement(uiaa::FallbackAcknowledgement::new(session))
                    }
                });
            }
        }
        .await?;
        let (client, crypto) = client;
        store.signed_in(client, crypto, sessions, holder, passphrase).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(json: serde_json::Value) -> UiaaInfo {
        serde_json::from_value(json).expect("uiaa info")
    }

    #[test]
    fn the_flow_with_fewest_browser_stages_should_be_chosen() {
        let info = info(serde_json::json!({
            "flows": [
                { "stages": ["m.login.recaptcha", "m.login.terms", "m.login.dummy"] },
                { "stages": ["m.login.registration_token", "m.login.terms"] },
            ],
            "session": "s",
        }));
        assert_eq!(next_stage(&info), Some(AuthType::RegistrationToken));
    }

    #[test]
    fn a_flow_asking_for_a_phone_number_should_not_be_chosen() {
        let info = info(serde_json::json!({
            "flows": [
                { "stages": ["m.login.msisdn"] },
                { "stages": ["m.login.email.identity"] },
            ],
            "session": "s",
        }));
        assert_eq!(next_stage(&info), Some(AuthType::EmailIdentity));
    }

    #[test]
    fn an_email_stage_should_be_preferred_over_a_browser_stage() {
        let info = info(serde_json::json!({
            "flows": [
                { "stages": ["m.login.recaptcha"] },
                { "stages": ["m.login.email.identity"] },
            ],
            "session": "s",
        }));
        assert_eq!(next_stage(&info), Some(AuthType::EmailIdentity));
    }

    #[test]
    fn a_server_asking_only_for_phone_numbers_should_offer_no_flow() {
        let info = info(serde_json::json!({ "flows": [{ "stages": ["m.login.msisdn"] }], "session": "s" }));
        assert_eq!(chosen_flow(&info), None);
    }

    #[test]
    fn completed_stages_should_be_skipped() {
        let info = info(serde_json::json!({
            "flows": [{ "stages": ["m.login.registration_token", "m.login.terms"] }],
            "completed": ["m.login.registration_token"],
            "session": "s",
        }));
        assert_eq!(next_stage(&info), Some(AuthType::Terms));
    }

    #[test]
    fn policies_should_be_read_in_english() {
        let info = info(serde_json::json!({
            "flows": [{ "stages": ["m.login.terms"] }],
            "params": { "m.login.terms": { "policies": { "privacy": {
                "version": "1",
                "de": { "name": "Datenschutz", "url": "https://example.org/de" },
                "en": { "name": "Privacy", "url": "https://example.org/en" },
            } } } },
        }));
        assert_eq!(policies(&info), vec![Policy { name: "Privacy".into(), url: "https://example.org/en".into() }]);
    }

    #[test]
    fn a_stage_the_app_cant_do_should_open_the_servers_page() {
        let homeserver = url::Url::parse("https://matrix.example.org/").expect("url");
        let url = fallback_url(&homeserver, &AuthType::ReCaptcha, "abc").expect("url");
        assert_eq!(url, "https://matrix.example.org/_matrix/client/v3/auth/m.login.recaptcha/fallback/web?session=abc");
    }
}
