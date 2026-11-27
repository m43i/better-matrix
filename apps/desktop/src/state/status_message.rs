//! The status message: what the user says they are up to ("🌴 On vacation"), shown to others
//! beside their presence. Written in the account menu, which shows the message set until the
//! user starts writing one for its account (`State::status_draft`, in `State::status_edit`).

use chrono::{DateTime, Local, LocalResult, NaiveDate, NaiveTime, TimeZone};
use matrix_core::{AccountId, Command};
use rune::{TextEdit, Tx};

use super::{Popover, Purpose, State};
use crate::i18n::t;

/// The longest status message the field takes, in characters.
pub const MAX_LENGTH: usize = 100;

/// When a draft status message should clear, relative to saving it or at a local clock time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Expiry {
    #[default]
    Never,
    HalfHour,
    Hour,
    FourHours,
    Day,
    Custom,
    At(u64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExpiryPicker {
    #[default]
    Closed,
    Choices,
    Custom,
}

#[derive(Debug, Default)]
pub struct ExpiryDraft {
    pub choice: Expiry,
    pub picker: ExpiryPicker,
    pub time: TextEdit,
    pub date: TextEdit,
}

impl ExpiryDraft {
    fn deadline(&self, now: &DateTime<Local>) -> Option<u64> {
        if self.picker == ExpiryPicker::Custom {
            return custom_deadline(now, &self.time.text, &self.date.text);
        }
        self.choice.deadline(now)
    }
}

impl Expiry {
    pub const OPTIONS: [Self; 6] = [Self::Never, Self::HalfHour, Self::Hour, Self::FourHours, Self::Day, Self::Custom];

    pub fn id(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::HalfHour => "half-hour",
            Self::Hour => "hour",
            Self::FourHours => "four-hours",
            Self::Day => "day",
            Self::Custom => "custom",
            Self::At(_) => "at",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Never => t!("status_message.expiry_never"),
            Self::HalfHour => t!("schedule.half_hour"),
            Self::Hour => t!("schedule.hour"),
            Self::FourHours => t!("status_message.expiry_four_hours"),
            Self::Day => t!("status_message.expiry_day"),
            Self::Custom | Self::At(_) => t!("status_message.expiry_custom"),
        }
    }

    pub fn current(self, now_ms: u64) -> Self {
        match self {
            Self::At(at) if at <= now_ms => Self::Never,
            _ => self,
        }
    }

    fn deadline(self, now: &DateTime<Local>) -> Option<u64> {
        let now = u64::try_from(now.timestamp_millis()).ok()?;
        let minutes = match self.current(now) {
            Self::Never | Self::Custom => return None,
            Self::At(at) => return Some(at),
            Self::HalfHour => 30,
            Self::Hour => 60,
            Self::FourHours => 240,
            Self::Day => 1440,
        };
        Some(now + minutes * 60_000)
    }
}

fn custom_deadline<T: TimeZone>(now: &DateTime<T>, time: &str, date: &str) -> Option<u64> {
    let time = NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    let date = date.trim();
    if date.is_empty() {
        let today = now.date_naive();
        return local_deadline(now, today, time).or_else(|| local_deadline(now, today.succ_opt()?, time));
    }
    local_deadline(now, NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?, time)
}

/// A fall-back day can have two occurrences of the same local time.
fn local_deadline<T: TimeZone>(now: &DateTime<T>, date: NaiveDate, time: NaiveTime) -> Option<u64> {
    let at = match now.timezone().from_local_datetime(&date.and_time(time)) {
        LocalResult::Single(at) => Some(at).filter(|at| at > now),
        LocalResult::Ambiguous(early, late) => [early, late].into_iter().find(|at| at > now),
        LocalResult::None => None,
    }?;
    u64::try_from(at.timestamp_millis()).ok()
}

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
        self.status_expiry = ExpiryDraft {
            choice: account.status_message_clear_at().map_or(Expiry::Never, Expiry::At).current(super::now_ms()),
            ..ExpiryDraft::default()
        };
        self.status_edit = TextEdit::new(message);
        self.status_edit.focused = true;
        self.status_draft = Some(id);
    }

    /// Stops writing, keeping the message set.
    pub fn cancel_status_message(&mut self) {
        self.status_draft = None;
    }

    /// Escape closes the deadline choices first, then stops writing.
    pub fn status_message_key(&mut self, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
        if key.key != rune::Key::Escape {
            return false;
        }
        if self.status_expiry.picker != ExpiryPicker::Closed {
            self.close_status_expiry();
            return true;
        }
        self.cancel_status_message();
        tx.blur();
        true
    }

    /// Saves the message being written.
    pub fn save_status_message(&mut self) {
        let expiry = &self.status_expiry;
        let clear_at = expiry.deadline(&Local::now());
        if expiry.picker == ExpiryPicker::Custom && clear_at.is_none() {
            return;
        }
        let Some(account) = self.status_draft.take() else { return };
        let message = std::mem::take(&mut self.status_edit.text);
        self.write_status_message(&account, &message, clear_at);
    }

    pub fn status_message_ready(&self) -> bool {
        let expiry = &self.status_expiry;
        expiry.picker != ExpiryPicker::Custom || expiry.deadline(&Local::now()).is_some()
    }

    pub fn toggle_status_expiry(&mut self) {
        if self.status_expiry.picker == ExpiryPicker::Closed {
            self.status_expiry.picker = ExpiryPicker::Choices;
        } else {
            self.close_status_expiry();
        }
    }

    pub fn close_status_expiry(&mut self) {
        let expiry = &mut self.status_expiry;
        if expiry.picker == ExpiryPicker::Custom
            && let Some(at) = expiry.deadline(&Local::now())
        {
            expiry.choice = Expiry::At(at);
        }
        expiry.picker = ExpiryPicker::Closed;
        self.status_edit.focused = true;
    }

    pub fn choose_status_expiry(&mut self, expiry: Expiry) {
        if expiry == Expiry::Custom {
            self.status_expiry.picker = ExpiryPicker::Custom;
            self.status_expiry.time.focused = true;
        } else {
            self.status_expiry.choice = expiry;
            self.close_status_expiry();
        }
    }

    pub fn choose_status_suggestion(&mut self, message: &str) {
        self.status_edit = TextEdit::new(message);
        self.status_edit.focused = true;
    }

    /// Sets the status message of `account` to `message`, trimmed and cut at [`MAX_LENGTH`]
    /// characters (blank clears it), showing it at once; closes the account menu.
    pub fn set_status_message(&mut self, account: &AccountId, message: &str) {
        self.write_status_message(account, message, None);
    }

    fn write_status_message(&mut self, account: &AccountId, message: &str, clear_at_ms: Option<u64>) {
        self.status_draft = None;
        self.popover = None;
        let cut: String = message.trim().chars().take(MAX_LENGTH).collect();
        let message = Some(cut.trim_end().to_owned()).filter(|message| !message.is_empty());
        let clear_at_ms = clear_at_ms.filter(|_| message.is_some());
        let Some(data) = self.accounts.get(account) else { return };
        if data.saving_status_message.is_none()
            && data.status_message() == message.as_deref()
            && data.status_message_clear_at() == clear_at_ms
        {
            return;
        }
        let command = Command::SetStatusMessage { account: account.clone(), message: message.clone(), clear_at_ms };
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
    fn a_custom_time_that_already_passed_should_clear_tomorrow() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 11, 27, 14, 0, 0).unwrap();
        let tomorrow = chrono::Utc.with_ymd_and_hms(2026, 11, 28, 9, 30, 0).unwrap();
        assert_eq!(custom_deadline(&now, "09:30", ""), Some(tomorrow.timestamp_millis() as u64));
    }

    #[test]
    fn a_custom_expiry_should_allow_future_dates() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 11, 27, 14, 0, 0).unwrap();
        let at = chrono::Utc.with_ymd_and_hms(2026, 12, 3, 9, 30, 0).unwrap();
        assert_eq!(custom_deadline(&now, "09:30", "2026-12-03"), Some(at.timestamp_millis() as u64));
    }

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
