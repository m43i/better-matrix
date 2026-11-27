//! What the Account and Sessions pages and the person panel start besides recovery: verifying
//! another person, the offline device, key export and import, changing the password,
//! deactivating the account, and linking a new device with a QR code (ADR 0004, ADR 0008).

use std::path::PathBuf;

use matrix_core::{
    AccountId, AccountPage, Answer, Command, KeyImport, Password, Person, QrLoginStep, RequestId, SignInMethod,
    VerificationStep,
};
use rune::{TextEdit, Tx};

use super::{Confirm, Guarded, Modal, PickFor, Purpose, QrLogin, State, VerificationFlow, focused, verification_ended};
use crate::i18n::{t, tf};

/// The name the save dialog suggests for a key export.
const KEY_FILE_NAME: &str = "message-keys.txt";

/// What the recovery key is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryKeyFor {
    /// Verifying this session.
    Verify,
    /// Turning the offline device on, which keeps its key in secret storage.
    OfflineDevice,
}

/// Which way message keys move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyTransfer {
    Export,
    Import,
}

/// Where a key export or import stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyFileStep {
    /// The passphrase is being typed.
    Entering,
    /// Sent as this request.
    Working(RequestId),
    /// Finished, with what to tell the user.
    Done(String),
}

/// A key export or import: the file, its passphrase, and how it went.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyFile {
    pub account: AccountId,
    pub transfer: KeyTransfer,
    pub path: PathBuf,
    pub passphrase: TextEdit,
    /// The passphrase again, for an export.
    pub confirm: TextEdit,
    pub error: Option<String>,
    pub step: KeyFileStep,
}

/// The change-password dialog of a password account.
#[derive(Clone, Debug, PartialEq)]
pub struct PasswordForm {
    pub account: AccountId,
    pub current: TextEdit,
    pub new: TextEdit,
    pub confirm: TextEdit,
    pub error: Option<String>,
    pub submitting: bool,
    /// Changed: the dialog says so, and that other sessions were signed out.
    pub done: bool,
}

impl PasswordForm {
    fn new(account: AccountId) -> Self {
        Self {
            account,
            current: focused(TextEdit::new("").secure()),
            new: TextEdit::new("").secure(),
            confirm: TextEdit::new("").secure(),
            error: None,
            submitting: false,
            done: false,
        }
    }
}

impl State {
    /// Whether the active account signs in through its server's account page.
    fn active_uses_oauth(&self) -> bool {
        self.active_account().is_some_and(|account| account.summary.sign_in == SignInMethod::OAuth)
    }

    // ----- verifying people ----------------------------------------------------------------

    /// Asks `person` to verify each other, in their direct chat; the Verification dialog follows.
    pub fn verify_person(&mut self, person: &Person) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.request_verification(account, Some(person.clone()));
    }

    /// Switches the open verification from the QR code to comparing emoji.
    pub fn compare_emoji(&mut self) {
        let Some((account, flow_id)) = self.open_verification() else { return };
        self.dispatch(Command::CompareEmoji { account, flow_id }, Purpose::Security);
    }

    /// A verification `request` couldn't start: its dialog says why.
    pub(super) fn verification_failed(&mut self, request: RequestId, error: String) {
        match &mut self.modal {
            Some(Modal::Verification { flow, step, .. })
                if *flow == VerificationFlow::Requested(request) && !verification_ended(step) =>
            {
                *step = VerificationStep::Cancelled(error);
            }
            _ => self.notice = Some(error),
        }
    }

    /// Asks whether the user verified `user_id` (not the user themself); [`Event::Identity`]
    /// answers.
    ///
    /// [`Event::Identity`]: matrix_core::Event::Identity
    pub(super) fn load_identity(&mut self, user_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        if account.as_str() != user_id {
            self.dispatch(Command::LoadIdentity { account, user_id: user_id.to_owned() }, Purpose::Identity);
        }
    }

    // ----- offline device ------------------------------------------------------------------

    /// Turns the offline device on (asking for the recovery key first) or off.
    pub fn set_offline_device(&mut self, on: bool) {
        if on {
            self.ask_recovery_key(RecoveryKeyFor::OfflineDevice);
            return;
        }
        let Some(account) = self.nav.account.clone() else { return };
        self.dispatch(Command::SetOfflineDevice { account, on: false, recovery_key: None }, Purpose::Security);
    }

    // ----- key export and import -----------------------------------------------------------

    /// Asks where to write the message keys; the passphrase dialog follows.
    pub fn export_room_keys(&mut self, tx: &mut Tx) {
        let Some(account) = self.nav.account.clone() else { return };
        self.shell.pick_save_path(tx, KEY_FILE_NAME, move |s, path| {
            if s.file_account_is_active(&account) {
                s.key_file_chosen(KeyTransfer::Export, path);
            }
        });
    }

    /// Asks for a key file to read; the passphrase dialog follows.
    pub fn import_room_keys(&mut self, tx: &mut Tx) {
        let Some(account) = self.nav.account.clone() else { return };
        self.shell.pick_files(tx, PickFor::KeyFile(account));
    }

    /// The file dialog's answer for a key export or import: asks for its passphrase.
    pub fn key_file_chosen(&mut self, transfer: KeyTransfer, path: PathBuf) {
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::KeyFile(Box::new(KeyFile {
            account,
            transfer,
            path,
            passphrase: focused(TextEdit::new("").secure()),
            confirm: TextEdit::new("").secure(),
            error: None,
            step: KeyFileStep::Entering,
        })));
    }

    /// Exports or imports with the passphrase typed in: an export's twice, the same.
    pub fn submit_key_file(&mut self) {
        let Some(Modal::KeyFile(file)) = &mut self.modal else { return };
        if file.step != KeyFileStep::Entering {
            return;
        }
        let passphrase = file.passphrase.text.clone();
        if passphrase.is_empty() {
            file.error = Some(t!("keys.passphrase_needed").to_owned());
            return;
        }
        if file.transfer == KeyTransfer::Export && file.confirm.text != passphrase {
            file.error = Some(t!("password.mismatch").to_owned());
            return;
        }
        let (account, path, passphrase) = (file.account.clone(), file.path.clone(), Password(passphrase));
        let command = match file.transfer {
            KeyTransfer::Export => Command::ExportRoomKeys { account, path, passphrase },
            KeyTransfer::Import => Command::ImportRoomKeys { account, path, passphrase },
        };
        let request = self.dispatch(command, Purpose::KeyFile);
        if let Some(Modal::KeyFile(file)) = &mut self.modal {
            file.error = None;
            file.step = KeyFileStep::Working(request);
        }
    }

    /// How many keys an import read.
    pub(super) fn keys_imported(&mut self, request: RequestId, import: KeyImport) {
        if let Some(Modal::KeyFile(file)) = &mut self.modal
            && file.step == KeyFileStep::Working(request)
        {
            file.step = KeyFileStep::Done(tf!("keys.imported", imported = import.imported, total = import.total));
        }
    }

    /// A key export or import finished; an import already said how many keys it read.
    pub(super) fn key_file_done(&mut self, request: RequestId) {
        if let Some(Modal::KeyFile(file)) = &mut self.modal
            && file.step == KeyFileStep::Working(request)
        {
            let done = match file.transfer {
                KeyTransfer::Export => tf!("keys.exported", path = file.path.display()),
                KeyTransfer::Import => t!("keys.imported_none").to_owned(),
            };
            file.step = KeyFileStep::Done(done);
        }
    }

    /// A key export or import failed: the passphrase dialog shows why, to try again.
    pub(super) fn key_file_failed(&mut self, request: RequestId, error: String) {
        match &mut self.modal {
            Some(Modal::KeyFile(file)) if file.step == KeyFileStep::Working(request) => {
                file.step = KeyFileStep::Entering;
                file.error = Some(error);
            }
            _ => self.notice = Some(error),
        }
    }

    // ----- password and deactivation -------------------------------------------------------

    /// Changes the password: in the app on a password account, on the account page otherwise.
    pub fn open_change_password(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        if self.active_uses_oauth() {
            self.dispatch(Command::OpenAccountPage { account, page: AccountPage::Profile }, Purpose::AccountPage);
        } else {
            self.modal = Some(Modal::ChangePassword(Box::new(PasswordForm::new(account))));
        }
    }

    /// Sends the new password with the current one, once all three fields agree.
    pub fn submit_password_change(&mut self) {
        let Some(Modal::ChangePassword(form)) = &mut self.modal else { return };
        if form.submitting || form.done {
            return;
        }
        if form.current.text.is_empty() || form.new.text.is_empty() {
            form.error = Some(t!("password.fill_in").to_owned());
            return;
        }
        if form.new.text != form.confirm.text {
            form.error = Some(t!("password.mismatch").to_owned());
            return;
        }
        form.error = None;
        form.submitting = true;
        let (account, current) = (form.account.clone(), Password(form.current.text.clone()));
        let action = Guarded::ChangePassword { new_password: Password(form.new.text.clone()) };
        let command = action.command(account.clone(), Some(current));
        self.dispatch(command, Purpose::Guarded { account, action });
    }

    /// The password changed: the dialog says so, whichever password dialog sent it.
    pub(super) fn password_changed(&mut self) {
        let account = match &self.modal {
            Some(Modal::ChangePassword(form)) => form.account.clone(),
            Some(Modal::Password { account, action: Guarded::ChangePassword { .. }, .. }) => account.clone(),
            _ => return,
        };
        self.modal = Some(Modal::ChangePassword(Box::new(PasswordForm { done: true, ..PasswordForm::new(account) })));
    }

    /// Deactivating the account: a password account confirms first (and may erase its
    /// messages); an OAuth account goes to its account page, which asks itself.
    pub fn ask_to_deactivate(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        if self.active_uses_oauth() {
            self.guarded(account, Guarded::Deactivate { erase: false });
        } else {
            self.modal = Some(Modal::Confirm(Confirm::Deactivate { account, erase: false }));
        }
    }

    /// Whether deactivating also erases the account's messages, in its confirmation.
    pub fn toggle_deactivate_erase(&mut self) {
        if let Some(Modal::Confirm(Confirm::Deactivate { erase, .. })) = &mut self.modal {
            *erase = !*erase;
        }
    }

    // ----- QR sign-in ----------------------------------------------------------------------

    /// Shows a QR code for a new device to scan, from this verified session of an OAuth account.
    pub fn link_device(&mut self) {
        let Some(account) = self.active_account() else { return };
        let verified = account.security.is_some_and(|security| security.session_verified);
        if !verified || account.summary.sign_in != SignInMethod::OAuth {
            return;
        }
        let account = account.summary.id.clone();
        let request =
            self.dispatch(Command::LinkDevice { account: account.clone(), scanned: None }, Purpose::LinkDevice);
        self.modal = Some(Modal::LinkDevice { account, flow: QrLogin::started(request) });
    }

    /// A QR sign-in moved on: the one linking a device here, or the one signing in.
    pub(super) fn qr_login_moved(&mut self, request: RequestId, step: QrLoginStep) {
        if let Some(Modal::LinkDevice { flow, .. }) = &mut self.modal
            && flow.moved(request, step.clone())
        {
            return;
        }
        self.sign_in.qr_moved(request, step);
    }

    /// Sends the check code typed in, for the QR sign-in that asked: the one linking a device
    /// here, or the one signing in.
    pub fn submit_check_code(&mut self) {
        let answer = match &mut self.modal {
            Some(Modal::LinkDevice { flow, .. }) => flow.answer(),
            _ => self.sign_in.qr().and_then(QrLogin::answer),
        };
        if let Some((request, code)) = answer {
            self.dispatch(Command::Answer { request, answer: Answer::CheckCode(code) }, Purpose::Security);
        }
    }

    /// The check code field of the QR sign-in that asks for it, for its input's `on_edit`.
    pub fn check_code_field(&mut self) -> &mut TextEdit {
        if let Some(Modal::LinkDevice { flow, .. }) = &mut self.modal {
            return &mut flow.code;
        }
        match &mut self.sign_in.step {
            super::SignInStep::Qr { flow, .. } => &mut flow.code,
            _ => &mut self.scratch,
        }
    }

    /// Opens the account page approving the new device again, for a tab the user closed.
    pub fn reopen_link_approval(&mut self, tx: &mut Tx) {
        if let Some(Modal::LinkDevice { flow: QrLogin { step: QrLoginStep::Approve { url }, .. }, .. }) = &self.modal
            && let Err(error) = tx.open_url(url)
        {
            self.notice = Some(tf!("notice.browser", error = error));
        }
    }
}

#[cfg(test)]
mod tests;
