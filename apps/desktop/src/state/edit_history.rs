//! A message's edit history: pressing "edited" under a message opens a dialog of every version
//! it went through, newest first (`Modal::EditHistory`), asked of the engine as it opens.

use matrix_core::{AccountId, Command, EditVersion};

use super::{Modal, Purpose, State};

/// The edit history dialog: whose message, and its versions once they came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditHistory {
    pub account: AccountId,
    pub room_id: String,
    pub event_id: String,
    /// Oldest first, as the engine sends them; `None` while they load.
    pub versions: Option<Vec<EditVersion>>,
    /// Why they couldn't be read.
    pub error: Option<String>,
}

impl State {
    /// Opens the edit history of a message in the room on screen.
    pub fn open_edit_history(&mut self, room_id: &str, event_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.modal = Some(Modal::EditHistory(Box::new(EditHistory {
            account: account.clone(),
            room_id: room_id.to_owned(),
            event_id: event_id.to_owned(),
            versions: None,
            error: None,
        })));
        let command = Command::ListEdits { account, room_id: room_id.to_owned(), event_id: event_id.to_owned() };
        self.dispatch(command, Purpose::EditHistory);
    }

    /// A message's versions came: the open dialog takes them if it is that message's.
    pub(super) fn edit_history_listed(&mut self, event_id: &str, versions: Vec<EditVersion>) {
        if let Some(Modal::EditHistory(history)) = &mut self.modal
            && history.event_id == event_id
        {
            history.versions = Some(versions);
            history.error = None;
        }
    }

    /// Reading the versions failed: the open dialog says why.
    pub(super) fn edit_history_failed(&mut self, error: String) {
        if let Some(Modal::EditHistory(history)) = &mut self.modal
            && history.versions.is_none()
        {
            history.error = Some(error);
        }
    }
}
