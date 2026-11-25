//! Message utilities shared by the hover menu and the right-click menu.

use matrix_core::{AccountId, Command, TimelineKey};
use rune::TextEdit;

use super::{Modal, Purpose, State};

/// A message in one timeline: the room's, or a thread's.
pub type MessageAt = (TimelineKey, String);

/// Which message's hover toolbar holds the pointer. A toolbar rises out of its row over the
/// message above, so the pointer can be over two messages at once: a shown toolbar the pointer
/// is on stays shown, and hides the toolbar (and hover) of the row it lies over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageHover {
    toolbar: Option<MessageAt>,
}

impl MessageHover {
    /// The pointer entered or left `message`'s toolbar, which hears it only while shown.
    pub fn toolbar(&mut self, message: &MessageAt, inside: bool) {
        if inside {
            self.toolbar = Some(message.clone());
        } else if self.toolbar.as_ref() == Some(message) {
            self.toolbar = None;
        }
    }

    /// Whether the pointer holds `message`'s toolbar.
    pub fn holds(&self, message: &MessageAt) -> bool {
        self.toolbar.as_ref() == Some(message)
    }

    /// The message whose toolbar holds the pointer, when it's another in `message`'s timeline:
    /// where that toolbar lies over `message`'s row, the row shows no hover and no toolbar.
    pub fn other_held(&self, message: &MessageAt) -> Option<&str> {
        self.toolbar.as_ref().filter(|held| held.0 == message.0 && held.1 != message.1).map(|held| held.1.as_str())
    }
}

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
    pub fn set_message_saved(&mut self, account: &AccountId, room_id: &str, event_id: &str, saved: bool) {
        self.popover = None;
        let account = account.clone();
        let command = Command::SetSaved { account, room_id: room_id.to_owned(), event_id: event_id.to_owned(), saved };
        self.dispatch(command, Purpose::MessageAction);
    }

    /// Shows the messages the account on screen saved, listing them afresh, with no search or
    /// filter.
    pub fn open_saved(&mut self) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::Saved(Box::new(super::SavedList::new(account.clone()))));
        self.list_saved(account);
    }

    fn list_saved(&mut self, account: AccountId) {
        self.accounts.listing_saved(&account);
        if !self.pending.values().any(|purpose| *purpose == Purpose::Saved(account.clone())) {
            self.dispatch(Command::ListSavedMessages { account: account.clone() }, Purpose::Saved(account));
        }
    }

    /// Lists the saved messages again: after a failure.
    pub fn retry_saved(&mut self) {
        if let Some(list) = self.saved_list() {
            self.list_saved(list.account.clone());
        }
    }

    /// An account's rooms changed: while its saved messages show, one saved meanwhile (here or
    /// on another session) is listed. One no longer saved leaves on its own
    /// (`AccountData::saved_messages`). After a failure, only Try again lists them, so syncs
    /// don't ask a failing server again and again.
    pub(super) fn saved_changed(&mut self, account: &AccountId) {
        if self.saved_list().is_some_and(|list| list.account == *account)
            && self.accounts.get(account).is_some_and(|data| data.saved_error.is_none() && data.saved_unlisted())
        {
            self.list_saved(account.clone());
        }
    }

    /// Opens a saved message of `account` where it was sent: its room, or its thread.
    pub fn open_saved_message(&mut self, account: &AccountId, room_id: &str, thread: Option<&str>, event_id: &str) {
        self.close_modal();
        if self.nav.account.as_ref() != Some(account) {
            self.select_account(account);
        }
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
