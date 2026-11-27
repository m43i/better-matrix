//! Coalesces viewport progress while scrolling, independently for each open timeline. Closing a
//! timeline sends what it still held back: the engine takes it through the closed timeline while
//! it is kept warm, so leaving a room right after reading it keeps it read.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use matrix_core::{AccountId, Command, RequestId, TimelineKey};

const INTERVAL: Duration = Duration::from_millis(500);
const RETRY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Position {
    event_id: Option<String>,
    fully_read: bool,
}

#[derive(Debug)]
struct Sent {
    position: Position,
    next_send: Instant,
    in_flight: Option<RequestId>,
    closed: bool,
    pending: Option<Position>,
}

#[derive(Debug, Default)]
pub struct Receipts {
    timelines: HashMap<(AccountId, TimelineKey), Sent>,
}

fn command(account: AccountId, key: TimelineKey, position: Position) -> Command {
    Command::MarkRead { account, key, event_id: position.event_id, fully_read: position.fully_read }
}

impl Receipts {
    pub fn update(
        &mut self,
        account: AccountId,
        key: TimelineKey,
        event_id: Option<String>,
        fully_read: bool,
        now: Instant,
    ) -> Option<Command> {
        let position = Position { event_id, fully_read };
        let open = (account.clone(), key.clone());
        if let Some(sent) = self.timelines.get_mut(&open) {
            sent.closed = false;
            if sent.position == position {
                return None;
            }
            // Reaching the initial unread boundary deserves an immediate cross-device ack.
            let boundary_reached = fully_read && !sent.position.fully_read;
            if sent.in_flight.is_some() || now < sent.next_send && !boundary_reached {
                sent.pending = Some(position);
                return None;
            }
        }
        self.timelines.insert(
            open,
            Sent {
                position: position.clone(),
                next_send: now + INTERVAL,
                in_flight: None,
                closed: false,
                pending: None,
            },
        );
        Some(command(account, key, position))
    }

    pub fn due(&mut self, now: Instant) -> Vec<Command> {
        let mut commands = Vec::new();
        for ((account, key), sent) in &mut self.timelines {
            if sent.in_flight.is_none()
                && now >= sent.next_send
                && let Some(position) = sent.pending.take()
            {
                sent.position = position.clone();
                sent.next_send = now + INTERVAL;
                commands.push(command(account.clone(), key.clone(), position));
            }
        }
        commands
    }

    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.timelines
            .values()
            .filter(|sent| sent.pending.is_some() && sent.in_flight.is_none())
            .map(|sent| sent.next_send.saturating_duration_since(now))
            .min()
    }

    pub fn sent(&mut self, account: &AccountId, key: &TimelineKey, request: RequestId) {
        if let Some(sent) = self.timelines.get_mut(&(account.clone(), key.clone())) {
            sent.in_flight = Some(request);
        }
    }

    pub fn finished(&mut self, request: RequestId, succeeded: bool, now: Instant) {
        let Some(open) =
            self.timelines.iter().find_map(|(open, sent)| (sent.in_flight == Some(request)).then(|| open.clone()))
        else {
            return;
        };
        let Some(sent) = self.timelines.get_mut(&open) else { return };
        sent.in_flight = None;
        // A closed timeline sends what it held back at once, whatever became of the receipt
        // before it, and retries nothing: the room may be gone, and opening it reads it again.
        if sent.closed {
            if sent.pending.is_some() {
                sent.next_send = now;
            } else {
                self.timelines.remove(&open);
            }
            return;
        }
        if !succeeded {
            sent.pending.get_or_insert_with(|| sent.position.clone());
            sent.next_send = now + RETRY;
        } else if sent.pending.as_ref().is_some_and(|position| position.fully_read && !sent.position.fully_read) {
            sent.next_send = now;
        }
    }

    /// The app is quitting: every position held back goes now, even beside a receipt in flight
    /// (the server keeps the later of two receipts). The engine waits a moment for them.
    pub fn flush(&mut self) -> Vec<Command> {
        self.timelines
            .drain()
            .filter_map(|((account, key), sent)| sent.pending.map(|position| command(account, key, position)))
            .collect()
    }

    /// The timeline closed. A position held back goes now (dispatch it before closing), or once
    /// the receipt in flight finishes; nothing is sent for it after that.
    pub fn forget(&mut self, account: &AccountId, key: &TimelineKey) -> Option<Command> {
        let open = (account.clone(), key.clone());
        let sent = self.timelines.get_mut(&open)?;
        sent.closed = true;
        if sent.in_flight.is_some() {
            return None;
        }
        let Some(position) = sent.pending.take() else {
            self.timelines.remove(&open);
            return None;
        };
        sent.position = position.clone();
        Some(command(account.clone(), key.clone(), position))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_scrolling_should_send_the_first_and_last_positions() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        assert!(receipts.update(account.clone(), key.clone(), Some("$0".into()), true, now).is_some());
        for i in 1..200 {
            assert!(
                receipts
                    .update(account.clone(), key.clone(), Some(format!("${i}")), true, now + Duration::from_millis(i))
                    .is_none()
            );
        }
        assert!(receipts.due(now + INTERVAL - Duration::from_millis(1)).is_empty());
        assert_eq!(
            receipts.due(now + INTERVAL),
            [Command::MarkRead { account, key, event_id: Some("$199".into()), fully_read: true }]
        );
        assert!(receipts.wait(now + INTERVAL).is_none());
    }

    #[test]
    fn reaching_the_unread_boundary_should_not_wait_for_the_scroll_interval() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), false, now);
        assert!(receipts.update(account, key, Some("$1".into()), true, now).is_some());
    }

    #[test]
    fn closing_a_timeline_should_send_its_queued_receipt_at_once() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.update(account.clone(), key.clone(), Some("$2".into()), true, now);
        assert_eq!(
            receipts.forget(&account, &key),
            Some(Command::MarkRead {
                account: account.clone(),
                key: key.clone(),
                event_id: Some("$2".into()),
                fully_read: true
            })
        );
        receipts.sent(&account, &key, RequestId(1));
        receipts.finished(RequestId(1), true, now);
        assert!(receipts.due(now + INTERVAL).is_empty());
        assert!(receipts.timelines.is_empty());
    }

    #[test]
    fn quitting_should_send_every_queued_receipt() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.update(account.clone(), key.clone(), Some("$2".into()), true, now);
        assert_eq!(
            receipts.flush(),
            [Command::MarkRead { account, key, event_id: Some("$2".into()), fully_read: true }]
        );
    }

    #[test]
    fn closing_a_timeline_with_nothing_queued_should_send_nothing() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        assert!(receipts.forget(&account, &key).is_none());
        assert!(receipts.timelines.is_empty());
    }

    #[test]
    fn closing_a_timeline_should_send_its_queued_receipt_once_the_one_in_flight_finishes() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.update(account.clone(), key.clone(), Some("$2".into()), true, now);
        assert!(receipts.forget(&account, &key).is_none());
        receipts.finished(RequestId(1), true, now);
        assert_eq!(
            receipts.due(now),
            [Command::MarkRead {
                account: account.clone(),
                key: key.clone(),
                event_id: Some("$2".into()),
                fully_read: true
            }]
        );
        receipts.sent(&account, &key, RequestId(2));
        receipts.finished(RequestId(2), true, now);
        assert!(receipts.timelines.is_empty());
    }

    #[test]
    fn a_closed_timeline_should_still_send_its_queued_receipt_after_a_failure() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.update(account.clone(), key.clone(), Some("$2".into()), true, now);
        receipts.forget(&account, &key);
        receipts.finished(RequestId(1), false, now);
        assert_eq!(
            receipts.due(now),
            [Command::MarkRead { account, key, event_id: Some("$2".into()), fully_read: true }]
        );
    }

    #[test]
    fn a_closed_timeline_should_not_retry_a_failed_receipt() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.forget(&account, &key);
        receipts.finished(RequestId(1), false, now);
        assert!(receipts.due(now + RETRY).is_empty());
        assert!(receipts.timelines.is_empty());
    }

    #[test]
    fn a_slow_receipt_should_finish_before_the_next_one_is_sent() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.update(account, key, Some("$2".into()), true, now);
        assert!(receipts.due(now + INTERVAL).is_empty());
        assert!(receipts.wait(now + INTERVAL).is_none());
        receipts.finished(RequestId(1), true, now + INTERVAL);
        assert_eq!(receipts.due(now + INTERVAL).len(), 1);
    }

    #[test]
    fn a_failed_receipt_should_retry_the_latest_position() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.update(account.clone(), key.clone(), Some("$2".into()), true, now);
        receipts.finished(RequestId(1), false, now);
        assert!(receipts.due(now + INTERVAL).is_empty());
        assert_eq!(
            receipts.due(now + RETRY),
            [Command::MarkRead { account, key, event_id: Some("$2".into()), fully_read: true }]
        );
    }

    #[test]
    fn reopening_a_timeline_should_wait_for_its_previous_receipt_to_finish() {
        let (mut receipts, now) = (Receipts::default(), Instant::now());
        let account = AccountId::new("@a:example.org");
        let key = TimelineKey::room("!room:example.org");
        receipts.update(account.clone(), key.clone(), Some("$1".into()), true, now);
        receipts.sent(&account, &key, RequestId(1));
        receipts.forget(&account, &key);
        assert!(receipts.update(account, key, Some("$2".into()), true, now + INTERVAL).is_none());
        receipts.finished(RequestId(1), true, now + INTERVAL);
        assert_eq!(receipts.due(now + INTERVAL).len(), 1);
    }
}
