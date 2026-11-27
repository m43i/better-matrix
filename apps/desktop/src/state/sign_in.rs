//! Signing in, as a two-step state machine:
//!
//! 1. **Homeserver**: the user types a server name, URL or Matrix ID; the engine resolves it
//!    (`.well-known` first) and reports how it signs users in.
//! 2. **Sign in**: OAuth servers go straight to the browser; password servers show username and
//!    password. A server offering both starts in the browser and offers the password instead.
//!    Step 2 also offers to create an account (in the browser on OAuth servers, with a
//!    registration form on password servers) and, where the server allows it, to sign in with a
//!    QR code from a device already signed in. Signing in again as a suspended account offers
//!    neither: both start a new store, not the suspended one's.
//!
//! Used for the first account and every one after it.

use matrix_core::{
    AccountId, Homeserver, LoginRequest, Password, QrLoginStep, RegisterRequest, RegistrationStep, RequestId,
};
use rune::TextEdit;

use super::qr_login::QrLogin;
use crate::i18n::t;

/// Where the sign-in is. Each step holds only what is valid in it.
#[derive(Debug, Clone, PartialEq)]
pub enum SignInStep {
    /// Step 1: choosing a homeserver.
    Server { error: Option<String> },
    /// Step 1: resolving it.
    Discovering(RequestId),
    /// Step 2 on an OAuth server: the user finishes in the browser. `request` is `None` after a
    /// failure, until the user tries again.
    /// With `create`, the browser page creates the account first.
    Browser { server: Homeserver, request: Option<RequestId>, url: Option<String>, create: bool, error: Option<String> },
    /// Step 2 on a password server. `submitting` is the login in flight.
    Password { server: Homeserver, submitting: Option<RequestId>, error: Option<String> },
    /// Step 2, creating an account on a password server: the form, then whatever the server asks
    /// (`asked`) while `request` runs.
    Register { server: Homeserver, request: Option<RequestId>, asked: Option<Asked>, error: Option<String> },
    /// Step 2, signing in with a QR code shown here for a device already signed in to scan.
    Qr { server: Homeserver, flow: Box<QrLogin> },
}

/// What a password server asks before it creates an account, and whether it was answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub step: RegistrationStep,
    /// The answer went back: waiting for the server's next question or the new account.
    pub answered: bool,
}

#[derive(Debug)]
pub struct SignInForm {
    pub homeserver: TextEdit,
    pub username: TextEdit,
    pub password: TextEdit,
    /// The new password again, when creating an account.
    pub confirm: TextEdit,
    /// A registration token, when the server asks for one.
    pub token: TextEdit,
    /// The email address to confirm, when the server asks for one.
    pub email: TextEdit,
    pub step: SignInStep,
    /// The account signed out softly that this sign-in takes back, keeping its data.
    pub resume: Option<AccountId>,
}

impl Default for SignInForm {
    fn default() -> Self {
        Self {
            homeserver: TextEdit::new("matrix.org"),
            username: TextEdit::new(""),
            password: TextEdit::new("").secure(),
            confirm: TextEdit::new("").secure(),
            token: TextEdit::new(""),
            email: TextEdit::new(""),
            step: SignInStep::Server { error: None },
            resume: None,
        }
    }
}

impl SignInForm {
    /// The discovered homeserver, in step 2.
    pub fn server(&self) -> Option<&Homeserver> {
        match &self.step {
            SignInStep::Browser { server, .. }
            | SignInStep::Password { server, .. }
            | SignInStep::Register { server, .. }
            | SignInStep::Qr { server, .. } => Some(server),
            SignInStep::Server { .. } | SignInStep::Discovering(_) => None,
        }
    }

    /// The password login the form describes, once both fields are filled in.
    pub fn password_request(&self) -> Option<LoginRequest> {
        Some(LoginRequest {
            homeserver: self.server()?.url.clone(),
            username: Some(self.username.text.trim().to_owned()).filter(|text| !text.is_empty())?,
            password: Some(self.password.text.clone()).filter(|text| !text.is_empty())?,
            resume: self.resume.clone(),
        })
    }

    /// The account the registration form describes, or what is missing from it.
    pub fn registration_request(&self) -> Result<RegisterRequest, &'static str> {
        let server = self.server().ok_or(t!("sign_in.fill_in_new"))?;
        let username = self.username.text.trim().trim_start_matches('@');
        if username.is_empty() || self.password.text.is_empty() {
            return Err(t!("sign_in.fill_in_new"));
        }
        if self.password.text != self.confirm.text {
            return Err(t!("password.mismatch"));
        }
        Ok(RegisterRequest {
            homeserver: server.url.clone(),
            username: username.to_owned(),
            password: Password(self.password.text.clone()),
        })
    }

    /// Whether step 2 may start a new store: sign in with a QR code or create an account. Not
    /// when signing in again as a suspended account (`resume`), which takes back its own store.
    pub fn starts_afresh(&self) -> bool {
        self.resume.is_none()
    }

    /// The email address typed in, or why it can't be one.
    pub fn email_address(&self) -> Result<String, &'static str> {
        let address = self.email.text.trim();
        if address.is_empty() || !address.contains('@') {
            return Err(t!("register.email_invalid"));
        }
        Ok(address.to_owned())
    }

    /// The QR sign-in under way, in step 2.
    pub fn qr(&mut self) -> Option<&mut QrLogin> {
        match &mut self.step {
            SignInStep::Qr { flow, .. } => Some(flow.as_mut()),
            _ => None,
        }
    }

    /// Takes a QR sign-in step for `request`, if it is this form's.
    pub fn qr_moved(&mut self, request: RequestId, step: QrLoginStep) {
        if let Some(flow) = self.qr() {
            flow.moved(request, step);
        }
    }

    /// Takes what the server asks for the registration `request`, if it is this form's.
    pub fn registration_asked(&mut self, request: RequestId, step: RegistrationStep) {
        if let SignInStep::Register { request: Some(running), asked, error, .. } = &mut self.step
            && *running == request
        {
            match step {
                RegistrationStep::Token => self.token = super::focused(TextEdit::new("")),
                RegistrationStep::Email => self.email = super::focused(TextEdit::new("")),
                _ => {}
            }
            *asked = Some(Asked { step, answered: false });
            *error = None;
        }
    }

    /// The request the form is waiting on, if any.
    pub fn pending(&self) -> Option<RequestId> {
        match &self.step {
            SignInStep::Discovering(request) => Some(*request),
            SignInStep::Browser { request, .. } => *request,
            SignInStep::Password { submitting, .. } => *submitting,
            SignInStep::Register { request, .. } => *request,
            SignInStep::Qr { flow, .. } => flow.request,
            SignInStep::Server { .. } => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match &self.step {
            SignInStep::Server { error }
            | SignInStep::Browser { error, .. }
            | SignInStep::Password { error, .. }
            | SignInStep::Register { error, .. } => error.as_deref(),
            SignInStep::Qr { flow, .. } => flow.error.as_deref(),
            SignInStep::Discovering(_) => None,
        }
    }

    /// Records a failed request: discovery returns to step 1, a sign-in stays on step 2.
    pub fn fail(&mut self, request: RequestId, message: String) {
        match &mut self.step {
            SignInStep::Discovering(r) if *r == request => self.step = SignInStep::Server { error: Some(message) },
            SignInStep::Browser { request: r @ Some(_), error, .. } if *r == Some(request) => {
                *r = None;
                *error = Some(message);
            }
            SignInStep::Password { submitting: r @ Some(_), error, .. } if *r == Some(request) => {
                *r = None;
                *error = Some(message);
            }
            // The form comes back as it was, to correct and send again.
            SignInStep::Register { request: r @ Some(_), asked, error, .. } if *r == Some(request) => {
                *r = None;
                *asked = None;
                *error = Some(message);
            }
            SignInStep::Qr { flow, .. } => {
                flow.fail(request, message);
            }
            _ => {}
        }
    }
}

impl super::State {
    /// Creates an account on the discovered server: on its page for OAuth servers, with the
    /// registration form on password servers. Not when signing in again as a suspended account.
    pub fn start_registration(&mut self) {
        if !self.sign_in.starts_afresh() {
            return;
        }
        let Some(server) = self.sign_in.server().filter(|s| s.methods.create_account).cloned() else { return };
        self.cancel_pending();
        if server.methods.oauth {
            self.start_browser(server, true);
        } else {
            self.sign_in.password = TextEdit::new("").secure();
            self.sign_in.confirm = TextEdit::new("").secure();
            self.sign_in.step = SignInStep::Register { server, request: None, asked: None, error: None };
        }
    }

    /// Sends the registration form, once it is complete and both passwords match.
    pub fn submit_registration(&mut self) {
        if self.sign_in.pending().is_some() {
            return;
        }
        let registration = self.sign_in.registration_request();
        let SignInStep::Register { error, .. } = &mut self.sign_in.step else { return };
        let registration = match registration {
            Ok(registration) => registration,
            Err(message) => {
                *error = Some(message.to_owned());
                return;
            }
        };
        *error = None;
        let request = self.dispatch(matrix_core::Command::Register(registration), super::Purpose::SignIn);
        if let SignInStep::Register { request: running, .. } = &mut self.sign_in.step {
            *running = Some(request);
        }
    }

    /// Answers what the server asked before creating the account; once per question.
    fn answer_registration(&mut self, answer: matrix_core::Answer) {
        let SignInStep::Register { request: Some(request), asked: Some(asked), .. } = &mut self.sign_in.step else {
            return;
        };
        if asked.answered {
            return;
        }
        asked.answered = true;
        let request = *request;
        self.dispatch(matrix_core::Command::Answer { request, answer }, super::Purpose::Security);
    }

    /// Sends the registration token typed in.
    pub fn submit_registration_token(&mut self) {
        let token = self.sign_in.token.text.trim().to_owned();
        if !token.is_empty() {
            self.answer_registration(matrix_core::Answer::RegistrationToken(token));
        }
    }

    /// Sends the email address typed in, for the server to send a link to; says what is wrong
    /// with one that can't be an address.
    pub fn submit_registration_email(&mut self) {
        let address = self.sign_in.email_address();
        let SignInStep::Register { asked: Some(Asked { step: RegistrationStep::Email, .. }), error, .. } =
            &mut self.sign_in.step
        else {
            return;
        };
        match address {
            Ok(address) => {
                *error = None;
                self.answer_registration(matrix_core::Answer::Email(address));
            }
            Err(message) => *error = Some(message.to_owned()),
        }
    }

    /// Agrees to the server's policies.
    pub fn agree_to_terms(&mut self) {
        self.answer_registration(matrix_core::Answer::Agree);
    }

    /// Goes on after the stage on the server's page, or after opening the emailed link.
    pub fn continue_registration(&mut self) {
        self.answer_registration(matrix_core::Answer::Continue);
    }

    /// Opens the server's page for the registration stage again, for a tab the user closed.
    pub fn reopen_registration_page(&mut self, tx: &mut rune::Tx) {
        if let SignInStep::Register { asked: Some(Asked { step: RegistrationStep::Browser { url }, .. }), .. } =
            &self.sign_in.step
            && let Err(error) = tx.open_url(url)
        {
            self.notice = Some(super::Notice::problem(crate::i18n::tf!("notice.browser", error = error)));
        }
    }

    /// Shows a QR code for a device already signed in to scan, on a server that allows it; not
    /// when signing in again as a suspended account (QR sign-in always starts a new store).
    pub fn start_qr_sign_in(&mut self) {
        if !self.sign_in.starts_afresh() {
            return;
        }
        let Some(server) = self.sign_in.server().filter(|s| s.methods.qr).cloned() else { return };
        self.cancel_pending();
        let start = matrix_core::QrSignIn::Show { homeserver: server.url.clone() };
        let request = self.dispatch(matrix_core::Command::SignInWithQr(start), super::Purpose::SignIn);
        self.sign_in.step = SignInStep::Qr { server, flow: Box::new(QrLogin::started(request)) };
    }

    /// From creating an account or the QR code back to the server's usual way of signing in.
    pub fn sign_in_instead(&mut self) {
        let Some(server) = self.sign_in.server().cloned() else { return };
        self.cancel_pending();
        self.continue_sign_in(server);
    }
}

#[cfg(test)]
mod tests;
