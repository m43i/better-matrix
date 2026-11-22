//! Scheduled messages (MSC4140): a composer's message handed to the server to send later,
//! from the menu beside the send button (`Popover::Schedule`), and the room's list of them
//! (`Popover::Scheduled`), where one can be sent now or cancelled. The server holds them, so
//! they go out whether or not the app runs; each account's list is asked for once a run and
//! comes back after every change.

use std::collections::{HashMap, HashSet};

use matrix_core::{AccountId, Command, Outgoing, ScheduledAction, ScheduledMessage};

use super::{ComposerTarget, Popover, Purpose, State};

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

    /// The moment it names, in milliseconds since the epoch, seen from `now_ms` with the local
    /// clock `offset_ms` ahead of UTC.
    pub fn at(self, now_ms: u64, offset_ms: i64) -> u64 {
        const MINUTE: u64 = 60_000;
        const DAY: u64 = 24 * 60 * MINUTE;
        let morning = |days_ahead: u64| {
            let local = now_ms.saturating_add_signed(offset_ms);
            let midnight = local - local % DAY;
            (midnight + days_ahead * DAY + 9 * 60 * MINUTE).saturating_add_signed(-offset_ms)
        };
        match self {
            When::HalfHour => now_ms + 30 * MINUTE,
            When::Hour => now_ms + 60 * MINUTE,
            When::TomorrowMorning => morning(1),
            When::MondayMorning => {
                // The epoch was a Thursday: day 0 is weekday 3, counting from Monday.
                let local = now_ms.saturating_add_signed(offset_ms);
                let weekday = (local / DAY + 3) % 7;
                morning(7 - weekday)
            }
        }
    }
}

/// Each account's scheduled messages, once listed.
#[derive(Debug, Default)]
pub struct Scheduled {
    listed: HashMap<AccountId, Vec<ScheduledMessage>>,
    asked: HashSet<AccountId>,
}

impl Scheduled {
    pub fn listed(&mut self, account: AccountId, messages: Vec<ScheduledMessage>) {
        self.listed.insert(account, messages);
    }

    pub fn forget(&mut self, account: &AccountId) {
        self.listed.remove(account);
        self.asked.remove(account);
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

    /// Asks for the account's scheduled messages once a run; called as a room opens.
    pub(super) fn want_scheduled(&mut self, account: &AccountId) {
        if self.scheduled.asked.insert(account.clone()) {
            self.dispatch(Command::ListScheduled { account: account.clone() }, Purpose::ListScheduled);
        }
    }

    /// Whether a composer's message can be scheduled: text only (uploads go when sent), and no
    /// edit under way.
    pub fn can_schedule(&self, target: ComposerTarget) -> bool {
        let composer = self.composer(target);
        composer.editing.is_none() && composer.attachments.is_empty() && !composer.edit.text.trim().is_empty()
    }

    /// Schedules a composer's message for `when` and empties the composer, as sending does.
    pub fn schedule_message(&mut self, target: ComposerTarget, when: When) {
        self.popover = None;
        if !self.can_schedule(target) {
            return;
        }
        let now = super::now_ms();
        let delay_ms = when.at(now, local_offset(now)).saturating_sub(now);
        let Some((reply_to, messages)) = self.take_messages(target) else { return };
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return };
        let mut reply_to = reply_to.map(|reply| reply.event_id);
        for message in messages.into_iter().filter(|message| !matches!(message, Outgoing::File(_))) {
            let command = Command::Schedule {
                account: account.clone(),
                key: key.clone(),
                reply_to: reply_to.take(),
                message,
                delay_ms,
            };
            self.dispatch(command, Purpose::Schedule);
        }
    }

    /// Cancels a scheduled message, or sends it now.
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
    fn half_an_hour_should_be_thirty_minutes_on() {
        assert_eq!(When::HalfHour.at(WEDNESDAY, 0) - WEDNESDAY, 30 * 60_000);
    }

    #[test]
    fn tomorrow_morning_should_be_nine_on_the_next_day() {
        // Thursday 2026-11-19, 09:00 UTC.
        assert_eq!(When::TomorrowMorning.at(WEDNESDAY, 0), 1_795_078_800_000);
    }

    #[test]
    fn tomorrow_morning_should_be_nine_on_the_local_clock() {
        // An hour ahead of UTC: 09:00 there is 08:00 UTC.
        assert_eq!(When::TomorrowMorning.at(WEDNESDAY, 3_600_000), 1_795_078_800_000 - 3_600_000);
    }

    #[test]
    fn monday_morning_should_be_nine_on_the_coming_monday() {
        // Monday 2026-11-23, 09:00 UTC.
        assert_eq!(When::MondayMorning.at(WEDNESDAY, 0), 1_795_424_400_000);
    }
}
