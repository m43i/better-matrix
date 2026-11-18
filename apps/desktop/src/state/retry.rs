//! Failed downloads and when each may be tried again: soon after the first failure, then less
//! and less often, so a server hiccup heals by itself and a file that is gone costs little.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

const FIRST_WAIT: Duration = Duration::from_secs(5);
const LONGEST_WAIT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug)]
pub struct Retries<K> {
    /// How often each failed in a row, and when it may be tried again.
    failed: HashMap<K, (u32, Instant)>,
}

impl<K> Default for Retries<K> {
    fn default() -> Self {
        Self { failed: HashMap::new() }
    }
}

/// The wait after the `attempts`th failure in a row: 5 s, doubling, up to 5 minutes.
fn wait(attempts: u32) -> Duration {
    FIRST_WAIT.saturating_mul(1 << attempts.saturating_sub(1).min(16)).min(LONGEST_WAIT)
}

impl<K: Hash + Eq> Retries<K> {
    pub fn failed(&mut self, key: K, now: Instant) {
        let attempts = self.failed.get(&key).map_or(0, |(attempts, _)| *attempts) + 1;
        self.failed.insert(key, (attempts, now + wait(attempts)));
    }

    pub fn succeeded(&mut self, key: &K) {
        self.failed.remove(key);
    }

    /// Whether `key` failed last time it was tried.
    pub fn has_failed(&self, key: &K) -> bool {
        self.failed.contains_key(key)
    }

    /// Whether `key` may be fetched at `now`: it never failed, or its wait is over.
    pub fn may_try(&self, key: &K, now: Instant) -> bool {
        self.failed.get(key).is_none_or(|(_, at)| *at <= now)
    }

    /// The first time after `now` a wait ends, for waking up then.
    pub fn next_after(&self, now: Instant) -> Option<Instant> {
        self.failed.values().map(|(_, at)| *at).filter(|at| *at > now).min()
    }

    /// How many keys are waiting to be tried again.
    pub fn len(&self) -> usize {
        self.failed.len()
    }

    /// Forgets the failures of keys not in `kept`.
    pub fn retain(&mut self, mut kept: impl FnMut(&K) -> bool) {
        self.failed.retain(|key, _| kept(key));
    }

    /// Forgets all but the `most` failures that may be tried again last.
    pub fn keep_latest(&mut self, most: usize) {
        if self.failed.len() <= most {
            return;
        }
        let mut times: Vec<Instant> = self.failed.values().map(|(_, at)| *at).collect();
        times.sort_unstable();
        match times.len().checked_sub(most).and_then(|at| times.get(at)).copied() {
            Some(cutoff) if most > 0 => self.failed.retain(|_, (_, at)| *at >= cutoff),
            _ => self.failed.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn past_the_limit_the_failures_tried_again_soonest_should_be_forgotten() {
        let (mut retries, now) = (Retries::default(), Instant::now());
        retries.failed("old", now);
        retries.failed("new", now + FIRST_WAIT);
        retries.keep_latest(1);
        assert_eq!((retries.has_failed(&"old"), retries.has_failed(&"new")), (false, true));
    }

    #[test]
    fn a_failed_download_should_wait_before_it_is_tried_again() {
        let (mut retries, now) = (Retries::default(), Instant::now());
        retries.failed("a", now);
        assert!(!retries.may_try(&"a", now));
        assert!(retries.may_try(&"a", now + FIRST_WAIT));
    }

    #[test]
    fn each_failure_in_a_row_should_wait_longer() {
        let (mut retries, now) = (Retries::default(), Instant::now());
        retries.failed("a", now);
        retries.failed("a", now);
        assert!(!retries.may_try(&"a", now + FIRST_WAIT));
        assert!(retries.may_try(&"a", now + FIRST_WAIT * 2));
    }

    #[test]
    fn waits_should_stop_growing_at_five_minutes() {
        assert_eq!(wait(40), LONGEST_WAIT);
    }

    #[test]
    fn a_success_should_forget_the_failures() {
        let (mut retries, now) = (Retries::default(), Instant::now());
        retries.failed("a", now);
        retries.succeeded(&"a");
        assert!(retries.may_try(&"a", now) && !retries.has_failed(&"a"));
    }

    #[test]
    fn the_next_wake_should_be_the_earliest_wait_still_ahead() {
        let (mut retries, now) = (Retries::default(), Instant::now());
        retries.failed("a", now);
        retries.failed("b", now);
        retries.failed("b", now);
        assert_eq!(retries.next_after(now), Some(now + FIRST_WAIT));
        assert_eq!(retries.next_after(now + FIRST_WAIT), Some(now + FIRST_WAIT * 2));
    }
}
