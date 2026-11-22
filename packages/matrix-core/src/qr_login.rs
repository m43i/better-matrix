//! Signing in with a QR code (MSC4108): a new device signs in by a QR code exchanged with a
//! device where the account is signed in already, which approves the sign-in and passes on the
//! encryption secrets, so the new session starts verified.
//!
//! Either device may show the code and the other scan it. Whichever scanned shows a two-digit
//! check code, which the user types on the device that showed the code. The signed-in device
//! then approves the sign-in on the account page in the browser, and the new device signs in.
//! It takes an OAuth homeserver that runs a rendezvous service.
//!
//! The four directions share one flow: [`QrLoginStep`]s reported as [`Event::QrLogin`], the
//! check code answered with [`Command::Answer`].
//!
//! [`Event::QrLogin`]: crate::Event::QrLogin
//! [`Command::Answer`]: crate::Command::Answer

use std::future::IntoFuture;

use futures_util::{Stream, StreamExt};
use matrix_sdk::authentication::oauth::qrcode::{
    CheckCodeSender, ContinuationMessageSender, GeneratedQrProgress, GrantLoginProgress, LoginProgress,
    Msc4108IntentData, QrCodeData, QrCodeIntent, QrCodeIntentData, QrProgress,
};
use matrix_sdk::authentication::oauth::registration::{ApplicationType, ClientMetadata, Localized, OAuthGrantType};
use matrix_sdk::ruma::serde::Raw;
use tokio::sync::mpsc;

use crate::account::{Account, ClientIdentity, Holder, NewStore};
use crate::qr::QrImage;
use crate::session::Sessions;
use crate::store::StoreLayout;
use crate::{Answer, Error};

/// How a new device starts signing in with a QR code.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum QrSignIn {
    /// Show a code for the signed-in device to scan.
    Show {
        /// The resolved homeserver URL (from [`Homeserver::url`](crate::Homeserver::url)) whose
        /// rendezvous service carries the exchange; the account may be on another server.
        homeserver: String,
    },
    /// The signed-in device's code, as scanned (its raw bytes).
    Scanned(Vec<u8>),
}

/// Where signing in with a QR code stands, on either device.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum QrLoginStep {
    /// Starting: talking to the server.
    Starting,
    /// Show this code for the other device to scan.
    Show(QrImage),
    /// The other device scanned the code and shows two digits: type them here
    /// ([`Answer::CheckCode`]).
    EnterCheckCode,
    /// Type these two digits on the other device.
    CheckCode(u8),
    /// The signed-in device: approve the new sign-in on the account page, opened in the browser
    /// ([`Event::OpenUrl`](crate::Event::OpenUrl) carries the same URL).
    Approve {
        /// The account page's approval URL.
        url: String,
    },
    /// The new device: waiting for the signed-in device to approve; the account page shows this
    /// code to compare.
    Waiting {
        /// The code the account page shows, if the server gave one.
        code: Option<String>,
    },
    /// Passing the encryption secrets to the new device.
    Syncing,
    /// Signed in: on the new device, as `account` (saved, and about to be added); on the
    /// signed-in device, `None`.
    Done {
        /// The account the new device signed in as.
        account: Option<crate::AccountId>,
    },
}

/// How long the signed-in device waits for the server to create the new device once the
/// browser approved it: the user may take a while in the browser before the new device asks.
const DEVICE_CREATION_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// One progress update of any of the four directions, in the shape the flow cares about.
enum Progress {
    Quiet,
    Show(QrCodeData),
    Scanned(CheckCodeSender),
    CheckCode(u8),
    Approve(String, ContinuationMessageSender),
    Waiting(String),
    Syncing,
}

impl From<LoginProgress<QrProgress>> for Progress {
    fn from(progress: LoginProgress<QrProgress>) -> Self {
        match progress {
            LoginProgress::EstablishingSecureChannel(QrProgress { check_code }) => Self::CheckCode(check_code),
            other => login(other),
        }
    }
}

impl From<LoginProgress<GeneratedQrProgress>> for Progress {
    fn from(progress: LoginProgress<GeneratedQrProgress>) -> Self {
        match progress {
            LoginProgress::EstablishingSecureChannel(generated) => self::generated(generated),
            other => login(other),
        }
    }
}

impl From<GrantLoginProgress<QrProgress>> for Progress {
    fn from(progress: GrantLoginProgress<QrProgress>) -> Self {
        match progress {
            GrantLoginProgress::EstablishingSecureChannel(QrProgress { check_code }) => Self::CheckCode(check_code),
            other => grant(other),
        }
    }
}

impl From<GrantLoginProgress<GeneratedQrProgress>> for Progress {
    fn from(progress: GrantLoginProgress<GeneratedQrProgress>) -> Self {
        match progress {
            GrantLoginProgress::EstablishingSecureChannel(generated) => self::generated(generated),
            other => grant(other),
        }
    }
}

fn generated(progress: GeneratedQrProgress) -> Progress {
    match progress {
        GeneratedQrProgress::QrReady(data) => Progress::Show(data),
        GeneratedQrProgress::QrScanned(sender) => Progress::Scanned(sender),
    }
}

fn login<Q>(progress: LoginProgress<Q>) -> Progress {
    match progress {
        LoginProgress::WaitingForToken { user_code } => Progress::Waiting(user_code),
        LoginProgress::SyncingSecrets => Progress::Syncing,
        LoginProgress::Starting | LoginProgress::EstablishingSecureChannel(_) | LoginProgress::Done => Progress::Quiet,
    }
}

fn grant<Q>(progress: GrantLoginProgress<Q>) -> Progress {
    match progress {
        GrantLoginProgress::WaitingForAuth { verification_uri, continuation_sender } => {
            Progress::Approve(verification_uri.to_string(), continuation_sender)
        }
        GrantLoginProgress::SyncingSecrets => Progress::Syncing,
        GrantLoginProgress::Starting | GrantLoginProgress::EstablishingSecureChannel(_) | GrantLoginProgress::Done => {
            Progress::Quiet
        }
    }
}

/// Runs one direction's future to its end, reporting its progress and handing it the check
/// code the user types.
async fn drive<P: Into<Progress>, E: std::fmt::Display>(
    future: impl IntoFuture<Output = Result<(), E>>,
    progress: impl Stream<Item = P>,
    report: &(dyn Fn(QrLoginStep) + Send + Sync),
    answers: &mut mpsc::UnboundedReceiver<Answer>,
) -> Result<(), Error> {
    let future = future.into_future();
    tokio::pin!(future);
    tokio::pin!(progress);
    let mut check = None;
    report(QrLoginStep::Starting);
    loop {
        tokio::select! {
            result = &mut future => {
                return result.map_err(|e| Error::QrLogin(e.to_string()));
            }
            Some(update) = progress.next() => match update.into() {
                Progress::Quiet => {}
                Progress::Show(data) => report(QrLoginStep::Show(QrImage::encode(&data.to_bytes())?)),
                Progress::Scanned(sender) => {
                    check = Some(sender);
                    report(QrLoginStep::EnterCheckCode);
                }
                Progress::CheckCode(code) => report(QrLoginStep::CheckCode(code)),
                Progress::Approve(url, continuation) => {
                    report(QrLoginStep::Approve { url });
                    // The sign-in carries on while the user approves in the browser: the new
                    // device waits for the server, and this one for the new device.
                    continuation.confirm().await.map_err(|e| Error::QrLogin(e.to_string()))?;
                }
                Progress::Waiting(code) => report(QrLoginStep::Waiting { code: Some(code) }),
                Progress::Syncing => report(QrLoginStep::Syncing),
            },
            Some(answer) = answers.recv() => {
                // The SDK takes one check code: a second answer is dropped, not an error.
                if let Answer::CheckCode(code) = answer
                    && let Some(sender) = check.take()
                {
                    sender.send(code).await.map_err(|e| Error::QrLogin(e.to_string()))?;
                }
            }
        }
    }
}

/// The code a device scanned, checked to be the kind `intent` expects: a signed-in device's
/// when signing in, a new device's when approving one.
fn read_code(bytes: &[u8], intent: QrCodeIntent) -> Result<QrCodeData, Error> {
    let data = QrCodeData::from_bytes(bytes).map_err(|_| Error::NotASignInCode)?;
    if data.intent() != intent {
        return Err(Error::WrongSignInCode);
    }
    Ok(data)
}

/// The homeserver a signed-in device's code names.
async fn homeserver_of(data: &QrCodeData) -> Result<String, Error> {
    match data.intent_data() {
        QrCodeIntentData::Msc4108 { data: Msc4108IntentData::Reciprocate { server_name }, .. } => {
            Ok(crate::discovery::discover(server_name).await?.url)
        }
        QrCodeIntentData::Msc4388 { base_url, .. } => Ok(base_url.to_string()),
        QrCodeIntentData::Msc4108 { .. } => Err(Error::WrongSignInCode),
    }
}

impl Account {
    /// Signs a new account in with a QR code exchanged with a device where it is signed in
    /// already, and saves the session. The session starts verified, with the account's secrets.
    pub(crate) async fn sign_in_with_qr(
        layout: &StoreLayout,
        sessions: &Sessions,
        holder: Holder,
        identity: &ClientIdentity,
        start: QrSignIn,
        report: &(dyn Fn(QrLoginStep) + Send + Sync),
        mut answers: mpsc::UnboundedReceiver<Answer>,
    ) -> Result<Self, Error> {
        let (homeserver, code) = match start {
            QrSignIn::Show { homeserver } => (homeserver, None),
            QrSignIn::Scanned(bytes) => {
                let data = read_code(&bytes, QrCodeIntent::Reciprocate)?;
                (homeserver_of(&data).await?, Some(data))
            }
        };
        let (store, passphrase) = (NewStore::new(layout)?, crate::session::new_passphrase()?);
        let client = crate::account::build_client(&homeserver, &store.path(), &passphrase, sessions, holder).await?;
        let mut metadata = ClientMetadata::new(
            ApplicationType::Native,
            vec![OAuthGrantType::DeviceCode],
            Localized::new(identity.uri.clone(), []),
        );
        metadata.client_name = Some(Localized::new(identity.name.clone(), []));
        let registration = Raw::new(&metadata).map_err(|e| Error::Storage(e.into()))?.into();
        let oauth = client.oauth();
        let result = match &code {
            Some(data) => {
                let login = oauth.login_with_qr_code(Some(&registration)).scan(data);
                let progress = login.subscribe_to_progress();
                drive(login, progress, report, &mut answers).await
            }
            None => {
                let login = oauth.login_with_qr_code(Some(&registration)).generate();
                let progress = login.subscribe_to_progress();
                drive(login, progress, report, &mut answers).await
            }
        };
        result?;
        // Saving would replace the store of an account already here (a running one under it,
        // or a suspended one's history and keys): the new device signs out again instead, and a
        // suspended account signs in again from its entry, which keeps its store.
        let user = client.user_id().map(ToString::to_string).ok_or(Error::NoUserId)?;
        let listed = user.clone();
        let saved = crate::account::blocking(sessions, move |s| Ok(s.list())).await?;
        if saved.iter().any(|stored| stored.id == listed) {
            let _ = client.logout().await;
            return Err(Error::AlreadySignedIn(user));
        }
        let account = store.signed_in(client, sessions, holder, passphrase).await?;
        // Only now: an app that leaves the flow on `Done` can't cancel the saving.
        report(QrLoginStep::Done { account: Some(account.id().clone()) });
        Ok(account)
    }

    /// Approves a new device's sign-in with a QR code, from this signed-in session: shows a code
    /// for the new device to scan (`scanned` is `None`), or takes the new device's code. Passes
    /// on the account's secrets, so this session must be verified.
    pub(crate) async fn link_device(
        &self,
        scanned: Option<Vec<u8>>,
        report: &(dyn Fn(QrLoginStep) + Send + Sync),
        mut answers: mpsc::UnboundedReceiver<Answer>,
    ) -> Result<(), Error> {
        let oauth = self.client().oauth();
        let grant = oauth.grant_login_with_qr_code().device_creation_timeout(DEVICE_CREATION_WAIT);
        match scanned {
            Some(bytes) => {
                let data = read_code(&bytes, QrCodeIntent::Login)?;
                let grant = grant.scan(&data);
                let progress = grant.subscribe_to_progress();
                drive(grant, progress, report, &mut answers).await?;
            }
            None => {
                let grant = grant.generate();
                let progress = grant.subscribe_to_progress();
                drive(grant, progress, report, &mut answers).await?;
            }
        }
        report(QrLoginStep::Done { account: None });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_no_sign_in_code_should_be_refused() {
        assert!(matches!(read_code(b"not a code", QrCodeIntent::Login), Err(Error::NotASignInCode)));
    }

    #[test]
    fn a_new_devices_code_should_be_refused_when_signing_in() {
        let key = vodozemac_key();
        let url = url::Url::parse("https://rendezvous.example.org/abc").expect("url");
        let data = QrCodeData::new_msc4108(key, url, Msc4108IntentData::Login);
        assert!(matches!(read_code(&data.to_bytes(), QrCodeIntent::Reciprocate), Err(Error::WrongSignInCode)));
    }

    #[test]
    fn a_signed_in_devices_code_should_be_taken_when_signing_in() {
        let key = vodozemac_key();
        let url = url::Url::parse("https://rendezvous.example.org/abc").expect("url");
        let intent = Msc4108IntentData::Reciprocate { server_name: "example.org".into() };
        let data = QrCodeData::new_msc4108(key, url, intent);
        assert!(read_code(&data.to_bytes(), QrCodeIntent::Reciprocate).is_ok());
    }

    fn vodozemac_key() -> matrix_sdk_base::crypto::vodozemac::Curve25519PublicKey {
        matrix_sdk_base::crypto::vodozemac::Curve25519PublicKey::from_bytes([7; 32])
    }
}
