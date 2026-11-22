//! Developer tools and widgets: an event's source, hidden events in timelines, and a room's
//! widgets (web apps pinned to it, opened in the browser).

use matrix_core::{AccountId, Command, EventSource, RequestId};
use rune::{TextEdit, Tx};

use super::{Confirm, Modal, Purpose, RoomForm, State, focused};
use crate::i18n::tf;

/// The source dialog: waiting for the event's JSON, then showing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceView {
    Loading(RequestId),
    Shown(Box<EventSource>),
}

impl State {
    /// Shows an event's source, asking the engine for it.
    pub fn view_source(&mut self, account: AccountId, room_id: String, event_id: String) {
        self.popover = None;
        let request = self.dispatch(Command::ViewSource { account, room_id, event_id }, Purpose::Source);
        self.modal = Some(Modal::Source(SourceView::Loading(request)));
    }

    /// The source arrived: shown if its dialog still waits for it.
    pub(super) fn source_loaded(&mut self, request: RequestId, source: EventSource) {
        if self.modal == Some(Modal::Source(SourceView::Loading(request))) {
            self.modal = Some(Modal::Source(SourceView::Shown(Box::new(source))));
        }
    }

    /// The source couldn't be read: its dialog closes and says why.
    pub(super) fn source_failed(&mut self, request: RequestId, error: &str) {
        if self.modal == Some(Modal::Source(SourceView::Loading(request))) {
            self.modal = None;
            self.notice = Some(tf!("notice.source", error = error));
        }
    }

    /// Shows the timeline's hidden events, or not.
    pub fn set_show_hidden_events(&mut self, show: bool) {
        self.settings.show_hidden_events = show;
        self.shell.save_settings(self.settings.clone());
        self.accounts.replan_timelines();
    }

    /// Opens a widget in the browser.
    pub fn open_widget(&mut self, url: &str, tx: &mut Tx) {
        if let Err(error) = tx.open_url(url) {
            self.notice = Some(tf!("notice.browser", error = error));
        }
    }

    /// Asks for a web page to pin to the room shown.
    pub fn start_add_widget(&mut self, room_id: &str) {
        self.popover = None;
        self.modal = Some(Modal::RoomForm {
            form: RoomForm::AddWidget { room_id: room_id.to_owned() },
            name: Box::new(focused(TextEdit::new(""))),
            topic: Box::new(TextEdit::new("")),
        });
    }

    /// Pins the page in the add-widget form. A blank address keeps the form open.
    pub(super) fn add_widget(&mut self, account: AccountId, room_id: String, name: Box<TextEdit>, url: Box<TextEdit>) {
        let address = url.text.trim().to_owned();
        if address.is_empty() {
            self.modal = Some(Modal::RoomForm { form: RoomForm::AddWidget { room_id }, name, topic: url });
            return;
        }
        let name = name.text.trim().to_owned();
        self.dispatch(Command::AddWidget { account, room_id, name, url: address }, Purpose::Widget);
    }

    /// Asks before taking a widget off the room shown.
    pub fn ask_to_remove_widget(&mut self, room_id: &str, widget_id: &str, name: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::Confirm(Confirm::RemoveWidget {
            account,
            room_id: room_id.to_owned(),
            widget_id: widget_id.to_owned(),
            name: name.to_owned(),
        }));
    }
}
