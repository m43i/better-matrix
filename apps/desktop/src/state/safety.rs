//! Safety (ADR 0011): what the user keeps away on each account, ignoring people, reporting
//! messages, rooms and people, and showing media the account's choice holds back.

use matrix_core::{AccountId, Command, Message, ReportTarget, RequestId, Safety, SafetyChange, TimelineKey};
use rune::TextEdit;

use super::{Confirm, Modal, Purpose, ReportForm, State, focused};
use crate::i18n::{t, tf};

impl State {
    /// The choices the engine reported: shown at once, and kept as the ones a failed change
    /// falls back to. Media they now show is fetched.
    pub(super) fn safety_reported(&mut self, account: &AccountId, safety: Safety) {
        self.accounts.safety_saved(account, safety);
        self.safety_changed();
    }

    /// Changes one of an account's safety choices, showing it at once.
    pub fn set_safety(&mut self, account: &AccountId, change: SafetyChange) {
        self.change_safety(account, change, None);
    }

    /// Changes a choice, and once it is saved declines the invitation `decline`, if any.
    pub(super) fn change_safety(&mut self, account: &AccountId, change: SafetyChange, decline: Option<String>) {
        let Some(data) = self.accounts.get(account) else { return };
        let changed = data.safety.clone().with(&change);
        let saved = data.saved_safety.clone().with(&change) == data.saved_safety;
        if changed == data.safety && (saved || decline.is_none()) {
            // Saved already: nothing to wait for. (Shown but still being saved, with nothing to
            // decline: the save on its way covers it.)
            if let Some(room_id) = decline {
                self.respond_to_invitation(account, &room_id, false);
            }
            return;
        }
        // Shown but not saved yet, with an invitation to decline: saved again, so the decline
        // waits for a save of its own.
        self.accounts.set_safety(account, changed);
        self.safety_changed();
        let command = Command::SetSafety { account: account.clone(), change };
        self.dispatch(command, Purpose::Safety { account: account.clone(), decline });
    }

    /// A change failed: the account shows the choices last saved again.
    pub(super) fn safety_failed(&mut self, account: &AccountId, error: &str) {
        self.notice = Some(tf!("notice.safety", error = error));
        if let Some(saved) = self.accounts.get(account).map(|data| data.saved_safety.clone()) {
            self.accounts.set_safety(account, saved);
        }
    }

    /// What shows may have changed: media no longer held back loads, and invitations' pictures.
    fn safety_changed(&mut self) {
        self.request_timeline_media();
        self.fetch_linked(std::time::Instant::now());
        self.request_avatars();
    }

    /// Asks before ignoring someone on the active account.
    pub fn ask_to_ignore(&mut self, user_id: &str, name: &str) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::Confirm(Confirm::Ignore {
            account,
            user_id: user_id.to_owned(),
            name: name.to_owned(),
            decline: None,
        }));
    }

    /// Asks before declining an invitation and ignoring whoever sent it.
    pub fn ask_to_ignore_inviter(&mut self, account: &AccountId, room_id: &str) {
        let Some(inviter) = self
            .accounts
            .get(account)
            .and_then(|data| data.invitations.iter().find(|invite| invite.room_id == room_id))
            .and_then(|invite| invite.inviter.clone())
        else {
            return;
        };
        self.modal = Some(Modal::Confirm(Confirm::Ignore {
            account: account.clone(),
            user_id: inviter.id,
            name: inviter.name,
            decline: Some(room_id.to_owned()),
        }));
    }

    /// Stops ignoring someone: nothing to confirm, as their messages come back.
    pub fn stop_ignoring(&mut self, account: &AccountId, user_id: &str) {
        self.popover = None;
        self.set_safety(account, SafetyChange::Ignore { user_id: user_id.to_owned(), ignored: false });
    }

    /// Opens the report dialog for a message in the active account's timeline.
    pub fn report_message(&mut self, key: &TimelineKey, event_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        let sender = self
            .accounts
            .get(&account)
            .and_then(|data| data.timeline(key))
            .and_then(|timeline| timeline.entries.iter().filter_map(|e| e.message()).find(|m| m.event_id == event_id))
            .map(|message| (message.sender_id.clone(), message.sender_name.clone()));
        let (person, name) = sender.map_or((None, String::new()), |(id, name)| (Some(id), name));
        let target = ReportTarget::Message { room_id: key.room_id.clone(), event_id: event_id.to_owned() };
        self.open_report(account, target, name, person);
    }

    /// Opens the report dialog for a room of the active account.
    pub fn report_room(&mut self, room_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        let name = self
            .accounts
            .get(&account)
            .and_then(|data| {
                data.room(room_id).map(|room| room.name.clone()).or_else(|| {
                    data.invitations.iter().find(|invite| invite.room_id == room_id).map(|invite| invite.name.clone())
                })
            })
            .unwrap_or_else(|| room_id.to_owned());
        self.open_report(account, ReportTarget::Room { room_id: room_id.to_owned() }, name, None);
    }

    /// Opens the report dialog for a person.
    pub fn report_person(&mut self, user_id: &str, name: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        let target = ReportTarget::Person { user_id: user_id.to_owned() };
        self.open_report(account, target, name.to_owned(), Some(user_id.to_owned()));
    }

    fn open_report(&mut self, account: AccountId, target: ReportTarget, name: String, person: Option<String>) {
        self.popover = None;
        // The user can't ignore themselves.
        let person = person.filter(|id| id != account.as_str());
        self.modal = Some(Modal::Report(Box::new(ReportForm {
            account,
            target,
            name,
            person,
            reason: focused(TextEdit::new("")),
            also_ignore: false,
            sending: None,
        })));
    }

    pub fn toggle_report_ignore(&mut self) {
        if let Some(Modal::Report(form)) = &mut self.modal
            && form.sending.is_none()
        {
            form.also_ignore = !form.also_ignore;
        }
    }

    /// Sends the report, and ignores them too when asked; the dialog waits for the report.
    pub fn send_report(&mut self) {
        let Some(Modal::Report(form)) = &self.modal else { return };
        if form.sending.is_some() {
            return;
        }
        let command = Command::Report {
            account: form.account.clone(),
            target: form.target.clone(),
            reason: form.reason.text.trim().to_owned(),
        };
        // They are ignored only once the report went through.
        let ignore =
            form.also_ignore.then(|| form.person.clone()).flatten().map(|user_id| (form.account.clone(), user_id));
        let request = self.dispatch(command, Purpose::Report { ignore });
        if let Some(Modal::Report(form)) = &mut self.modal {
            form.sending = Some(request);
        }
    }

    /// The report went through: its dialog closes, with a word that it did, and whoever it was
    /// about is ignored if the user asked.
    pub(super) fn report_sent(&mut self, request: RequestId, ignore: Option<(AccountId, String)>) {
        if matches!(&self.modal, Some(Modal::Report(form)) if form.sending == Some(request)) {
            self.modal = None;
        }
        self.notice = Some(t!("report.sent").to_owned());
        if let Some((account, user_id)) = ignore {
            self.set_safety(&account, SafetyChange::Ignore { user_id, ignored: true });
        }
    }

    /// The report failed: its dialog stays open to try again.
    pub(super) fn report_failed(&mut self, request: RequestId, error: &str) {
        if let Some(Modal::Report(form)) = &mut self.modal
            && form.sending == Some(request)
        {
            form.sending = None;
        }
        self.notice = Some(tf!("notice.report", error = error));
    }

    /// Whether a message's media waits for a press in the active account's timeline.
    pub fn hides_media(&self, account: &AccountId, room_id: &str, message: &Message) -> bool {
        self.accounts.get(account).is_some_and(|data| data.hides_media(room_id, message, &self.revealed_media))
    }

    /// Shows a message's held-back media, until the app restarts, and fetches it.
    pub fn reveal_media(&mut self, account: &AccountId, event_id: &str) {
        self.revealed_media.insert((account.clone(), event_id.to_owned()));
        self.request_timeline_media();
        self.fetch_linked(std::time::Instant::now());
    }
}

#[cfg(test)]
mod tests;
