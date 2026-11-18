//! The status message: what the user says they are up to ("🌴 On vacation"), shown to others
//! beside their presence. Written in the account menu, which shows the message set until the
//! user starts writing one for its account (`State::status_draft`, in `State::status_edit`).

use matrix_core::{AccountId, Command};
use rune::{TextEdit, Tx};

use super::{Popover, Purpose, State};
use crate::i18n::t;

/// The longest status message the field takes, in characters.
pub const MAX_LENGTH: usize = 100;

/// The messages offered while writing one: a stable ID, an emoji and its words.
pub fn suggestions() -> [(&'static str, &'static str, &'static str); 4] {
    [
        ("vacation", "🌴", t!("status_message.vacation")),
        ("meeting", "📅", t!("status_message.meeting")),
        ("sick", "🤒", t!("status_message.sick")),
        ("remote", "🏠", t!("status_message.remote")),
    ]
}

/// The emoji a status message starts with, if any: what places with little room show of it.
pub fn leading_emoji(message: &str) -> Option<&str> {
    // An emoji is a few code points at most (a flag, a family, a skin tone): the longest
    // prefix the emoji list knows.
    let ends = message.char_indices().map(|(at, c)| at + c.len_utf8()).take(10);
    ends.filter(|&end| emojis::get(&message[..end]).is_some()).last().map(|end| &message[..end])
}

/// `message` starting with `emoji` instead of the emoji it started with, if any.
fn with_emoji(message: &str, emoji: &str) -> String {
    let message = message.trim_start();
    let rest = leading_emoji(message).map_or(message, |old| &message[old.len()..]).trim_start();
    if rest.is_empty() { emoji.to_owned() } else { format!("{emoji} {rest}") }
}

impl State {
    /// Starts writing the status message of the account on screen, from the one set.
    pub fn edit_status_message(&mut self) {
        let Some(account) = self.active_account() else { return };
        let (id, message) = (account.summary.id.clone(), account.status_message().unwrap_or_default().to_owned());
        self.status_edit = TextEdit::new(message);
        self.status_edit.focused = true;
        self.status_draft = Some(id);
    }

    /// Stops writing, keeping the message set.
    pub fn cancel_status_message(&mut self) {
        self.status_draft = None;
    }

    /// Escape in the field stops writing, leaving the account menu open.
    pub fn status_message_key(&mut self, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
        if key.key != rune::Key::Escape {
            return false;
        }
        self.cancel_status_message();
        tx.blur();
        true
    }

    /// Saves the message being written.
    pub fn save_status_message(&mut self) {
        let Some(account) = self.status_draft.take() else { return };
        let message = std::mem::take(&mut self.status_edit.text);
        self.set_status_message(&account, &message);
    }

    /// Sets the status message of `account` to `message`, trimmed and cut at [`MAX_LENGTH`]
    /// characters (blank clears it), showing it at once; closes the account menu.
    pub fn set_status_message(&mut self, account: &AccountId, message: &str) {
        self.status_draft = None;
        self.popover = None;
        let cut: String = message.trim().chars().take(MAX_LENGTH).collect();
        let message = Some(cut.trim_end().to_owned()).filter(|message| !message.is_empty());
        let Some(data) = self.accounts.get(account) else { return };
        if data.status_message() == message.as_deref() {
            return;
        }
        let command = Command::SetStatusMessage { account: account.clone(), message: message.clone() };
        let request = self.dispatch(command, Purpose::StatusMessage { account: account.clone() });
        self.accounts.status_message_chosen(account, message, request);
    }

    /// An emoji picked for the message being written: it starts the message, replacing the one
    /// it started with; back to the account menu, writing.
    pub(super) fn status_emoji_chosen(&mut self, emoji: &str) {
        self.popover = Some(Popover::AccountSwitcher);
        if self.status_draft.is_some() {
            self.status_edit = TextEdit::new(with_emoji(&self.status_edit.text, emoji));
            self.status_edit.focused = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_should_start_with_the_emoji_it_begins_with() {
        assert_eq!(leading_emoji("🌴 On vacation"), Some("🌴"));
    }

    #[test]
    fn an_emoji_of_several_code_points_should_count_whole() {
        assert_eq!(leading_emoji("👩‍💻 Coding"), Some("👩‍💻"));
    }

    #[test]
    fn a_message_starting_with_words_should_have_no_emoji() {
        assert_eq!(leading_emoji("Back at 3"), None);
    }

    #[test]
    fn a_picked_emoji_should_replace_the_one_the_message_started_with() {
        assert_eq!(with_emoji("🌴 On vacation", "🏝️"), "🏝️ On vacation");
    }

    #[test]
    fn a_picked_emoji_should_replace_an_emoji_after_spaces() {
        assert_eq!(with_emoji("  🌴 On vacation", "🏝️"), "🏝️ On vacation");
    }

    #[test]
    fn a_picked_emoji_should_start_a_message_without_one() {
        assert_eq!(with_emoji("Back at 3", "🕒"), "🕒 Back at 3");
    }
}
