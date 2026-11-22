//! The user's profile for one account: display name and picture, the contact details (email
//! addresses and phone numbers) linked to the account, the password, deactivating the account,
//! and the homeserver's account page.

use std::path::Path;

use matrix_sdk::ruma::api::client::discovery::get_authorization_server_metadata::v1::AccountManagementActionData;
use matrix_sdk::ruma::thirdparty::Medium;
use matrix_sdk::ruma::{ClientSecret, OwnedClientSecret, OwnedSessionId, UInt};

use crate::Error;
use crate::account::Account;

/// The kind of a contact detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum ContactKind {
    /// An email address.
    Email,
    /// A phone number.
    Phone,
}

/// An email address or phone number linked to the account (a Matrix third-party identifier).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct ContactDetail {
    /// Email or phone.
    pub kind: ContactKind,
    /// The address, or the phone number in international form without the `+`.
    pub address: String,
}

/// The contact details linked to an account, and whether the app may change them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Contacts {
    /// The email addresses and phone numbers.
    pub details: Vec<ContactDetail>,
    /// Whether the server lets clients add and remove them (its `m.3pid_changes` capability). A
    /// server whose accounts an OAuth service manages (such as MAS) doesn't: the service's
    /// account page does it.
    pub changeable: bool,
}

/// A contact detail being added: the server sent a link (email) or a code (phone) to confirm it.
/// Hand it back to confirm and add it; never inspect it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PendingContact {
    /// Email or phone.
    pub kind: ContactKind,
    /// What the user entered.
    pub address: String,
    #[cfg_attr(feature = "ffi", ts(type = "string"))]
    secret: OwnedClientSecret,
    #[cfg_attr(feature = "ffi", ts(type = "string"))]
    sid: OwnedSessionId,
    /// Where a phone code goes; `None` when the server checks it itself.
    submit_url: Option<String>,
}

/// A page of the homeserver's account management, for OAuth accounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum AccountPage {
    /// Name, picture, contact details and password.
    Profile,
    /// Deactivating the account.
    Deactivate,
}

impl PendingContact {
    /// A detail that never reached a server: for demo scenes and tests.
    pub fn offline(kind: ContactKind, address: &str) -> Self {
        Self {
            kind,
            address: address.to_owned(),
            secret: ClientSecret::new(),
            sid: matrix_sdk::ruma::session_id!("offline").to_owned(),
            submit_url: None,
        }
    }
}

impl Account {
    /// Changes the password of a password account, which signs its other sessions out. The
    /// server asks for the current `password` (else this fails with
    /// [`Error::PasswordRequired`]). An OAuth account changes it on its account page.
    pub async fn change_password(&self, new: &str, password: Option<&str>) -> Result<(), Error> {
        let account = self.client().account();
        match account.change_password(new, None).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let Some(info) = error.as_uiaa_response() else { return Err(error.into()) };
                let auth = self.password_auth(password.ok_or(Error::PasswordRequired)?, info.session.clone());
                account.change_password(new, Some(auth)).await?;
                Ok(())
            }
        }
    }

    /// Deactivates a password account for good: nobody can sign in to it again, and its user ID
    /// can't be taken again. With `erase`, the server also forgets the messages it sent, as far
    /// as it can. The server asks for the `password` (else this fails with
    /// [`Error::PasswordRequired`]). An OAuth account is deactivated on its account page.
    pub async fn deactivate(&self, erase: bool, password: Option<&str>) -> Result<(), Error> {
        let account = self.client().account();
        match account.deactivate(None, None, erase).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let Some(info) = error.as_uiaa_response() else { return Err(error.into()) };
                let auth = self.password_auth(password.ok_or(Error::PasswordRequired)?, info.session.clone());
                account.deactivate(None, Some(auth), erase).await?;
                Ok(())
            }
        }
    }

    /// Sets the display name; an empty name removes it.
    pub async fn set_display_name(&self, name: &str) -> Result<(), Error> {
        let name = name.trim();
        Ok(self.client().account().set_display_name((!name.is_empty()).then_some(name)).await?)
    }

    /// Uploads the picture at `path` and makes it the profile picture; `None` removes it.
    pub async fn set_avatar(&self, path: Option<&Path>) -> Result<(), Error> {
        let account = self.client().account();
        let Some(path) = path else { return Ok(account.set_avatar_url(None).await?) };
        let bytes = tokio::fs::read(path).await.map_err(|source| Error::ReadFile { path: path.to_owned(), source })?;
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        if mime.type_() != mime_guess::mime::IMAGE {
            return Err(Error::NotAnImage(path.to_owned()));
        }
        let url = account.upload_avatar(&mime, bytes).await?;
        Ok(account.set_avatar_url(Some(&url)).await?)
    }

    /// The email addresses and phone numbers linked to the account.
    pub async fn contact_details(&self) -> Result<Contacts, Error> {
        let changeable = self.client().homeserver_capabilities().can_change_thirdparty_ids().await?;
        let response = match self.client().account().get_3pids().await {
            Ok(response) => response,
            // A server that leaves them to its account page may not list them either.
            Err(_) if !changeable => return Ok(Contacts { details: Vec::new(), changeable }),
            Err(error) => return Err(error.into()),
        };
        let details = response
            .threepids
            .into_iter()
            .filter_map(|threepid| {
                let kind = match threepid.medium {
                    Medium::Email => ContactKind::Email,
                    Medium::Msisdn => ContactKind::Phone,
                    _ => return None,
                };
                Some(ContactDetail { kind, address: threepid.address })
            })
            .collect();
        Ok(Contacts { details, changeable })
    }

    /// Asks the server to confirm a new contact detail: it emails a link, or texts a code. A
    /// phone number without a leading `+` is read as dialled from `country` (ISO 3166 code).
    pub async fn request_contact_code(
        &self,
        kind: ContactKind,
        address: &str,
        country: &str,
    ) -> Result<PendingContact, Error> {
        let account = self.client().account();
        let secret = ClientSecret::new();
        let address = address.trim().to_owned();
        let attempt = UInt::MIN;
        let (sid, submit_url) = match kind {
            ContactKind::Email => {
                let response = account.request_3pid_email_token(&secret, &address, attempt).await?;
                (response.sid, response.submit_url)
            }
            ContactKind::Phone => {
                let number: String = address.chars().filter(|c| !c.is_whitespace()).collect();
                let response = account.request_3pid_msisdn_token(&secret, country, &number, attempt).await?;
                (response.sid, response.submit_url)
            }
        };
        Ok(PendingContact { kind, address, secret, sid, submit_url })
    }

    /// Sends the code the server texted for a new phone number.
    pub async fn confirm_contact_code(&self, pending: &PendingContact, code: &str) -> Result<(), Error> {
        let url = pending.submit_url.as_deref().ok_or(Error::NoCodeSubmission)?;
        let body = serde_json::json!({
            "sid": pending.sid,
            "client_secret": pending.secret,
            "token": code.trim(),
        });
        let request =
            self.client().http_client().post(url).header("content-type", "application/json").body(body.to_string());
        let response = request.send().await.map_err(Error::Code)?;
        let bytes = response.bytes().await.map_err(Error::Code)?;
        let accepted: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        match accepted.get("success").and_then(serde_json::Value::as_bool) {
            Some(true) => Ok(()),
            _ => Err(Error::WrongCode),
        }
    }

    /// Links a confirmed contact detail to the account. The server usually asks for the
    /// password; without it this fails with [`Error::PasswordRequired`].
    pub async fn add_contact(&self, pending: &PendingContact, password: Option<&str>) -> Result<(), Error> {
        let account = self.client().account();
        match account.add_3pid(&pending.secret, &pending.sid, None).await {
            Ok(_) => Ok(()),
            Err(error) => {
                let Some(info) = error.as_uiaa_response() else { return Err(error.into()) };
                let auth = self.password_auth(password.ok_or(Error::PasswordRequired)?, info.session.clone());
                account.add_3pid(&pending.secret, &pending.sid, Some(auth)).await?;
                Ok(())
            }
        }
    }

    /// Unlinks a contact detail from the account.
    pub async fn remove_contact(&self, contact: &ContactDetail) -> Result<(), Error> {
        let medium = match contact.kind {
            ContactKind::Email => Medium::Email,
            ContactKind::Phone => Medium::Msisdn,
        };
        self.client().account().delete_3pid(&contact.address, medium, None).await?;
        Ok(())
    }

    /// The URL of a page of the homeserver's account management (OAuth accounts).
    pub async fn account_page(&self, page: AccountPage) -> Result<String, Error> {
        let action = match page {
            AccountPage::Profile => AccountManagementActionData::Profile,
            AccountPage::Deactivate => AccountManagementActionData::AccountDeactivate,
        };
        self.account_page_url(action).await
    }

    pub(crate) async fn account_page_url(&self, action: AccountManagementActionData<'_>) -> Result<String, Error> {
        let metadata = self.client().oauth().server_metadata().await.map_err(|e| Error::Account(e.to_string()))?;
        let url = metadata.account_management_url_with_action(action).ok_or(Error::NoAccountPage)?;
        Ok(url.to_string())
    }
}
