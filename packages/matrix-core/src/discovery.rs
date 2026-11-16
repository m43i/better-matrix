//! Finding a homeserver from what a user types, and how it lets them sign in.
//!
//! Users type a server name (`example.org`), a URL (`https://example.org`), or their full Matrix
//! ID (`@alice:example.org`). The server name is looked up through `.well-known/matrix/client`;
//! when that fails, the input is tried as a homeserver URL directly. Everything after discovery
//! uses the resolved homeserver URL, so nothing is discovered twice.

use matrix_sdk::Client;
use matrix_sdk::ruma::api::client::session::get_login_types::v3::LoginType;

use crate::Error;
use crate::model::LoginMethods;

/// What the user typed, split into the server to discover and a username, if they gave one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignInInput {
    /// A server name or URL to discover.
    pub server: String,
    /// The localpart, when the user typed a full Matrix ID.
    pub username: Option<String>,
}

impl SignInInput {
    /// Parses a homeserver field; `None` when it is blank or names no server.
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        let (server, username) = match input.strip_prefix('@').and_then(|id| id.split_once(':')) {
            Some((user, server)) => (server, Some(user.to_owned()).filter(|u| !u.is_empty())),
            None => (input, None),
        };
        let server = server.trim_end_matches('/');
        (!server.is_empty()).then(|| Self { server: server.to_owned(), username })
    }
}

/// A discovered homeserver.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Homeserver {
    /// The server name users belong to (`example.org`), when found through `.well-known`.
    pub server_name: Option<String>,
    /// The client API base URL every request goes to (`https://matrix-client.example.org/`).
    pub url: String,
    /// How it lets users sign in.
    pub methods: LoginMethods,
}

impl Homeserver {
    /// A short name for the UI: the server name, else the URL's host.
    pub fn display_name(&self) -> &str {
        self.server_name.as_deref().unwrap_or_else(|| {
            self.url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/')
        })
    }
}

/// Resolves `server` (a server name or URL) and asks it for its sign-in methods.
pub(crate) async fn discover(server: &str) -> Result<Homeserver, Error> {
    let client = crate::tls::sdk(Client::builder()).server_name_or_homeserver_url(server).build().await?;
    let oauth = client.oauth().server_metadata().await.is_ok();
    let password = match client.matrix_auth().get_login_types().await {
        Ok(response) => response.flows.iter().any(|flow| matches!(flow, LoginType::Password(_))),
        Err(_) => false,
    };
    Ok(Homeserver {
        server_name: client.server().and_then(|url| url.host_str().map(str::to_owned)),
        url: client.homeserver().to_string(),
        methods: LoginMethods { oauth, password },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_name_should_be_discovered_as_typed() {
        assert_eq!(
            SignInInput::parse(" example.org "),
            Some(SignInInput { server: "example.org".into(), username: None })
        );
    }

    #[test]
    fn a_url_should_lose_its_trailing_slash() {
        assert_eq!(SignInInput::parse("https://example.org/").map(|i| i.server), Some("https://example.org".into()));
    }

    #[test]
    fn a_matrix_id_should_give_the_server_and_the_username() {
        assert_eq!(
            SignInInput::parse("@alice:example.org"),
            Some(SignInInput { server: "example.org".into(), username: Some("alice".into()) })
        );
    }

    #[test]
    fn a_blank_field_should_name_no_server() {
        assert_eq!(SignInInput::parse("  "), None);
    }

    #[test]
    fn the_display_name_should_prefer_the_server_name() {
        let server = Homeserver {
            server_name: Some("example.org".into()),
            url: "https://matrix-client.example.org/".into(),
            methods: LoginMethods::default(),
        };
        assert_eq!(server.display_name(), "example.org");
    }

    #[test]
    fn the_display_name_should_fall_back_to_the_url_host() {
        let server = Homeserver {
            server_name: None,
            url: "https://matrix.example.org/".into(),
            methods: LoginMethods::default(),
        };
        assert_eq!(server.display_name(), "matrix.example.org");
    }
}
