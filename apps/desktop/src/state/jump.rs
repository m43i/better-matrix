//! Jumping to a message: from a link to it, or from a reply's quote. A message the timeline on
//! screen holds is scrolled to at once; any other is first located by the engine (its room's
//! timeline or the thread it is in, `Command::LocateMessage`), then opened there, loading
//! history until it shows ([`State::open_message`]). The message jumped to flashes a moment.

use std::time::{Duration, Instant};

use matrix_core::{AccountId, Command, TimelineKey};

use super::{Purpose, State};

/// How long the message jumped to stays lit.
pub const FLASH: Duration = Duration::from_millis(2400);

/// The message jumped to, lit until its time is up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flash {
    pub event_id: String,
    pub since: Instant,
}

impl State {
    /// Goes to the message `event_id` in `room_id` of `account`: in the timeline on screen when
    /// it holds it, else wherever the engine finds it.
    pub fn jump_to_message(&mut self, account: &AccountId, room_id: &str, event_id: &str) {
        let shown = self.open_timelines.iter().find(|(open, key)| open == account && key.room_id == room_id).cloned();
        let held = shown.as_ref().is_some_and(|(account, key)| {
            self.accounts
                .get(account)
                .and_then(|data| data.timeline(key))
                .is_some_and(|timeline| timeline.messages().any(|message| message.event_id == event_id))
        });
        if let (true, Some((_, key))) = (held, shown) {
            self.locating = None;
            self.open_message(&key, event_id);
            return;
        }
        let command = Command::LocateMessage {
            account: account.clone(),
            room_id: room_id.to_owned(),
            event_id: event_id.to_owned(),
        };
        self.locating = Some(self.dispatch(command, Purpose::Locate));
    }

    /// The engine found where a message shows: open it there, if its account and room are
    /// still the ones on screen (the user may have gone elsewhere meanwhile).
    pub(super) fn message_located(
        &mut self,
        request: matrix_core::RequestId,
        account: &AccountId,
        key: &TimelineKey,
        event_id: &str,
    ) {
        // Only the latest jump goes: an earlier one answering late would undo it.
        if self.locating != Some(request) {
            return;
        }
        self.locating = None;
        if self.nav.account.as_ref() != Some(account) || self.active_room_id() != Some(key.room_id.as_str()) {
            return;
        }
        self.open_message(key, event_id);
    }

    /// Lights the message jumped to; `poll` lets it go once its time is up.
    pub(super) fn flash(&mut self, event_id: &str) {
        self.flash = Some(Flash { event_id: event_id.to_owned(), since: Instant::now() });
    }

    /// Lets the flash go once its time is up; true when it went.
    pub(super) fn expire_flash(&mut self, now: Instant, tx: &mut rune::Tx) -> bool {
        let Some(flash) = &self.flash else { return false };
        let until = flash.since + FLASH;
        if now >= until {
            self.flash = None;
            return true;
        }
        tx.wake_after(until - now);
        false
    }
}
