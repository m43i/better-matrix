//! Message utilities shared by the hover menu and the right-click menu.

use matrix_core::{Command, TimelineKey};
use rune::TextEdit;

use super::{Modal, Purpose, State};

impl State {
    pub fn set_message_pinned(&mut self, key: &TimelineKey, event_id: &str, pinned: bool) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        self.dispatch(
            Command::SetPinned { account, room_id: key.room_id.clone(), event_id: event_id.to_owned(), pinned },
            Purpose::MessageAction,
        );
    }

    /// Saves a message for the user alone, or stops saving it. Room summaries follow through
    /// sync, and with them the menu and the saved list.
    pub fn set_message_saved(&mut self, room_id: &str, event_id: &str, saved: bool) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        let command = Command::SetSaved { account, room_id: room_id.to_owned(), event_id: event_id.to_owned(), saved };
        self.dispatch(command, Purpose::MessageAction);
    }

    /// Shows the messages the account on screen saved, listing them afresh.
    pub fn open_saved(&mut self) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::Saved(account.clone()));
        self.list_saved(account);
    }

    fn list_saved(&mut self, account: matrix_core::AccountId) {
        self.accounts.listing_saved(&account);
        if !self.pending.values().any(|purpose| *purpose == Purpose::Saved(account.clone())) {
            self.dispatch(Command::ListSavedMessages { account: account.clone() }, Purpose::Saved(account));
        }
    }

    /// Lists the saved messages again: after a failure.
    pub fn retry_saved(&mut self) {
        if let Some(Modal::Saved(account)) = &self.modal {
            self.list_saved(account.clone());
        }
    }

    /// An account's rooms changed: while its saved messages show, one saved meanwhile (here or
    /// on another session) is listed. One no longer saved leaves on its own
    /// (`AccountData::saved_messages`). After a failure, only Try again lists them, so syncs
    /// don't ask a failing server again and again.
    pub(super) fn saved_changed(&mut self, account: &matrix_core::AccountId) {
        if matches!(&self.modal, Some(Modal::Saved(shown)) if shown == account)
            && self.accounts.get(account).is_some_and(|data| data.saved_error.is_none() && data.saved_unlisted())
        {
            self.list_saved(account.clone());
        }
    }

    /// Opens a saved message where it was sent: its room, or its thread.
    pub fn open_saved_message(&mut self, room_id: &str, thread: Option<&str>, event_id: &str) {
        self.modal = None;
        let key = TimelineKey { room_id: room_id.to_owned(), thread: thread.map(str::to_owned) };
        self.open_message(&key, event_id);
    }

    pub fn start_forward(&mut self, key: TimelineKey, event_id: String) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::Forward { account, key, event_id, search: TextEdit::default(), sending: None });
    }

    pub fn forward_to(&mut self, room_id: String) {
        let Some(Modal::Forward { account, key, event_id, sending, .. }) = &mut self.modal else { return };
        if sending.is_some() {
            return;
        }
        let command =
            Command::Forward { account: account.clone(), key: key.clone(), event_id: event_id.clone(), room_id };
        let request = self.dispatch(command, Purpose::Forward);
        if let Some(Modal::Forward { sending, .. }) = &mut self.modal {
            *sending = Some(request);
        }
    }
}
