//! Scheduled messages (MSC4140): a composer's message handed to the server to send later,
//! from the menu beside the send button (`Popover::Schedule`), and the room's list of them
//! (`Popover::Scheduled`), where one can be sent now or cancelled. The server holds them, so
//! they go out whether or not the app runs. Each account's list is asked for once a run, as
//! the list opens, and when one of its messages comes due; it comes back after every change.
//!
//! Only messages are scheduled: text, and the commands that make one (`/me`, `/plain`,
//! `/spoiler`, the faces); another command waits for the user to send it. The composer is
//! emptied as sending does, and what it held comes back if the server refuses.

use std::collections::{HashMap, HashSet};

use matrix_core::{AccountId, Command, Outgoing, ScheduledAction, ScheduledMessage};

use super::{ComposerTarget, Popover, Purpose, State};
use crate::i18n::tf;

/// A time a message can be scheduled for, as the menu offers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Half an hour from now.
    HalfHour,
    /// An hour from now.
    Hour,
    /// Tomorrow at 9:00, local time.
    TomorrowMorning,
    /// Next Monday at 9:00, local time.
    MondayMorning,
}

impl When {
    pub const ALL: [When; 4] = [When::HalfHour, When::Hour, When::TomorrowMorning, When::MondayMorning];

    /// Stable, for keys.
    pub fn id(self) -> &'static str {
        match self {
            When::HalfHour => "half-hour",
            When::Hour => "hour",
            When::TomorrowMorning => "tomorrow",
            When::MondayMorning => "monday",
        }
    }

    /// The moment it names, in milliseconds since the epoch, seen from `now_ms`; `offset` is
    /// how far the local clock is ahead of UTC at a moment (it changes with summer time, so
    /// 09:00 is read on the clock of that day, not today's).
    pub fn at(self, now_ms: u64, offset: impl Fn(u64) -> i64) -> u64 {
        const MINUTE: u64 = 60_000;
        const DAY: u64 = 24 * 60 * MINUTE;
        let local = now_ms.saturating_add_signed(offset(now_ms));
        let morning = |days_ahead: u64| {
            let wanted = local - local % DAY + days_ahead * DAY + 9 * 60 * MINUTE;
            let guess = wanted.saturating_add_signed(-offset(now_ms));
            wanted.saturating_add_signed(-offset(guess))
        };
        match self {
            When::HalfHour => now_ms + 30 * MINUTE,
            When::Hour => now_ms + 60 * MINUTE,
            When::TomorrowMorning => morning(1),
            // The epoch was a Thursday: day 0 is weekday 3, counting from Monday.
            When::MondayMorning => morning(7 - (local / DAY + 3) % 7),
        }
    }
}

/// Each account's scheduled messages, once listed.
#[derive(Debug, Default)]
pub struct Scheduled {
    listed: HashMap<AccountId, Vec<ScheduledMessage>>,
    asked: HashSet<AccountId>,
    /// When the list was last asked for because a message's time came: while the server still
    /// lists one (it is sending it, or the answer was lost), it is asked again after a while.
    due_asked: Option<std::time::Instant>,
}

impl Scheduled {
    /// The account's list, as the engine answered; returns why the first message it newly
    /// reports as failed couldn't be sent, if one is.
    pub fn listed(&mut self, account: AccountId, messages: Vec<ScheduledMessage>) -> Option<String> {
        let before = self.listed.get(&account);
        let new = messages
            .iter()
            .filter(|message| {
                before.is_none_or(|before| {
                    !before.iter().any(|known| known.delay_id == message.delay_id && known.failed.is_some())
                })
            })
            .find_map(|message| message.failed.clone());
        self.listed.insert(account, messages);
        new
    }

    pub fn forget(&mut self, account: &AccountId) {
        self.listed.remove(account);
        self.asked.remove(account);
    }
}

/// How long a message whose time came may still be listed before the list is asked for again.
const DUE_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

/// Why a scheduled message wasn't sent, in words: the server's own, or ours for a code the
/// engine gives ([`matrix_core::NOT_POSTED`]).
pub fn failure_reason(reason: &str) -> String {
    match reason {
        matrix_core::NOT_POSTED => crate::i18n::t!("schedule.not_posted").to_owned(),
        reason => reason.to_owned(),
    }
}

/// How far the local clock is ahead of UTC at `now_ms`, in milliseconds.
pub fn local_offset(now_ms: u64) -> i64 {
    use chrono::{Offset, TimeZone};
    i64::try_from(now_ms)
        .ok()
        .and_then(|ms| chrono::Local.timestamp_millis_opt(ms).single())
        .map_or(0, |at| i64::from(at.offset().fix().local_minus_utc()) * 1000)
}

impl State {
    /// The scheduled messages of the room on screen, soonest first.
    pub fn scheduled_here(&self) -> Vec<&ScheduledMessage> {
        let (Some(account), Some(room_id)) = (self.nav.account.as_ref(), self.active_room_id()) else {
            return Vec::new();
        };
        self.scheduled.listed.get(account).into_iter().flatten().filter(|message| message.room_id == room_id).collect()
    }

    /// Asks for the account's scheduled messages once a run, and again when one of them comes
    /// due (the server sent it), every [`DUE_RETRY`] while the server still lists one; wakes
    /// when the next one does. Called from `poll`.
    pub(super) fn want_scheduled(&mut self, account: &AccountId, tx: &mut rune::Tx) {
        let (now, at) = (super::now_ms(), std::time::Instant::now());
        let messages = self.scheduled.listed.get(account).map(Vec::as_slice).unwrap_or_default();
        // One that failed is done: only those still waiting come due.
        let waiting = || messages.iter().filter(|message| message.failed.is_none());
        let due = waiting().any(|message| message.send_at_ms <= now);
        if let Some(next) = waiting().map(|message| message.send_at_ms).filter(|at| *at > now).min() {
            tx.wake_after(std::time::Duration::from_millis(next - now));
        }
        let retry = due && self.scheduled.due_asked.is_none_or(|asked| at.duration_since(asked) >= DUE_RETRY);
        if due {
            tx.wake_after(DUE_RETRY);
        }
        if self.scheduled.asked.insert(account.clone()) || retry {
            if retry {
                self.scheduled.due_asked = Some(at);
            }
            self.dispatch(Command::ListScheduled { account: account.clone() }, Purpose::ListScheduled);
        }
    }

    /// Opens or closes the room's list of scheduled messages, asking for it again as it opens.
    pub fn toggle_scheduled(&mut self) {
        self.toggle_popover(Popover::Scheduled);
        if self.popover == Some(Popover::Scheduled)
            && let Some(account) = self.nav.account.clone()
        {
            self.dispatch(Command::ListScheduled { account }, Purpose::ListScheduled);
        }
    }

    /// Whether a composer's message can be scheduled: text only (uploads go when sent), and no
    /// edit under way.
    pub fn can_schedule(&self, target: ComposerTarget) -> bool {
        let composer = self.composer(target);
        composer.editing.is_none() && composer.attachments.is_empty() && !composer.edit.text.trim().is_empty()
    }

    /// Schedules a composer's message for `when` and empties the composer, as sending does; a
    /// command that doesn't make a message stays, with a notice.
    pub fn schedule_message(&mut self, target: ComposerTarget, when: When) {
        self.popover = None;
        if !self.can_schedule(target) {
            return;
        }
        if !self.composer(target).starts_in_pill()
            && let super::slash::Parsed::Command { command, .. } = super::slash::parse(&self.composer(target).edit.text)
            && !command.sends_message()
        {
            self.notice = Some(tf!("slash.not_scheduled", command = command.name()));
            return;
        }
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return };
        let now = super::now_ms();
        let delay_ms = when.at(now, local_offset).saturating_sub(now);
        let kept = Box::new(self.composer(target).copy_draft());
        let Some((reply_to, messages)) = self.take_messages(target) else { return };
        let mut reply_to = reply_to.map(|reply| reply.event_id);
        for message in messages.into_iter().filter(|message| !matches!(message, Outgoing::File(_))) {
            let command = Command::Schedule {
                account: account.clone(),
                key: key.clone(),
                reply_to: reply_to.take(),
                message,
                delay_ms,
            };
            let purpose = Purpose::Scheduling { owner: (account.clone(), key.clone()), draft: kept.clone() };
            self.dispatch(command, purpose);
        }
    }

    /// The server refused a scheduled message: what its composer held comes back, unless the
    /// user wrote something there since.
    pub(super) fn scheduling_failed(&mut self, owner: super::drafts::Owner, draft: super::composer::Draft) {
        let here = [ComposerTarget::Room, ComposerTarget::Thread]
            .into_iter()
            .find(|target| self.composer(*target).owner.as_ref() == Some(&owner));
        match here {
            Some(target) if self.composer(target).is_empty() && self.composer(target).editing.is_none() => {
                self.composer_mut(target).put_draft(draft);
            }
            // Something was written there since: the refused draft waits for the composer to
            // be free again (`resume_refused`), as a refused edit does.
            Some(_) => self.refused.push((owner, draft)),
            None => match self.drafts.entry(owner) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(draft);
                }
                std::collections::hash_map::Entry::Occupied(entry) => self.refused.push((entry.key().clone(), draft)),
            },
        }
    }

    /// Brings refused scheduled drafts back into their composers once these are free: showing
    /// their timeline, empty, no edit or memo under way. Called from `poll`.
    pub(super) fn resume_refused(&mut self) {
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            let composer = self.composer(target);
            if !composer.is_empty()
                || composer.editing.is_some()
                || composer.voice.is_some()
                || composer.reply_to.is_some()
            {
                continue;
            }
            let Some(owner) = composer.owner.clone() else { continue };
            if let Some(index) = self.refused.iter().position(|(kept, _)| *kept == owner) {
                let (_, draft) = self.refused.remove(index);
                self.composer_mut(target).put_draft(draft);
            }
        }
    }

    /// Cancels a scheduled message, or sends it now; one that failed is let go.
    pub fn update_scheduled(&mut self, delay_id: &str, action: ScheduledAction) {
        let Some(account) = self.nav.account.clone() else { return };
        let command = Command::UpdateScheduled { account, delay_id: delay_id.to_owned(), action };
        self.dispatch(command, Purpose::Schedule);
        if self.scheduled_here().len() <= 1 && self.popover == Some(Popover::Scheduled) {
            self.popover = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wednesday 2026-11-18, 14:30 UTC.
    const WEDNESDAY: u64 = 1_795_012_200_000;

    #[test]
    fn the_engines_not_posted_code_should_read_as_words() {
        assert_eq!(failure_reason(matrix_core::NOT_POSTED), crate::i18n::t!("schedule.not_posted"));
        assert_eq!(failure_reason("You may not post here"), "You may not post here");
    }

    #[test]
    fn half_an_hour_should_be_thirty_minutes_on() {
        assert_eq!(When::HalfHour.at(WEDNESDAY, |_| 0) - WEDNESDAY, 30 * 60_000);
    }

    #[test]
    fn tomorrow_morning_should_be_nine_on_the_next_day() {
        // Thursday 2026-11-19, 09:00 UTC.
        assert_eq!(When::TomorrowMorning.at(WEDNESDAY, |_| 0), 1_795_078_800_000);
    }

    #[test]
    fn tomorrow_morning_should_be_nine_on_the_local_clock() {
        // An hour ahead of UTC: 09:00 there is 08:00 UTC.
        assert_eq!(When::TomorrowMorning.at(WEDNESDAY, |_| 3_600_000), 1_795_078_800_000 - 3_600_000);
    }

    #[test]
    fn a_morning_after_the_clocks_change_should_be_nine_on_that_days_clock() {
        // An hour ahead of UTC until Wednesday night, two hours ahead from then on.
        let change = WEDNESDAY + 12 * 3_600_000;
        let offset = |at: u64| if at < change { 3_600_000 } else { 7_200_000 };
        // 09:00 at two hours ahead is 07:00 UTC.
        assert_eq!(When::TomorrowMorning.at(WEDNESDAY, offset), 1_795_078_800_000 - 7_200_000);
    }

    #[test]
    fn monday_morning_should_be_nine_on_the_coming_monday() {
        // Monday 2026-11-23, 09:00 UTC.
        assert_eq!(When::MondayMorning.at(WEDNESDAY, |_| 0), 1_795_424_400_000);
    }
}
