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
