//! Keeping drafts on this device: which draft the store holds for each timeline, as far as the
//! app knows, and when a composer's draft should be written there. A draft being typed is written
//! at most every `EVERY`; an emptied one (sent or cleared) and the draft of a timeline being left
//! at once, so a sent message never comes back as a draft after a restart.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use matrix_core::{AccountId, Command, Draft, TimelineKey};

use super::{ComposerTarget, Purpose, State};

/// How often a draft that keeps changing is written to the store.
pub const EVERY: Duration = Duration::from_millis(500);

/// A timeline written in.
pub type Owner = (AccountId, TimelineKey);

#[derive(Debug, Default)]
pub struct Kept {
    /// The draft the store holds per timeline, as far as the app knows; absent for none.
    stored: HashMap<Owner, Draft>,
    /// Timelines whose draft changed since it was last written, and since when.
    changed: HashMap<Owner, Instant>,
    /// Timelines whose draft the user changed this run: a draft read from the store after that
    /// doesn't replace what they did.
    touched: HashSet<Owner>,
    /// Timelines whose draft was written this run: the engine reads and writes in order, so a
    /// read answered after a write is older than what the store holds.
    wrote: HashSet<Owner>,
}

impl Kept {
    /// The store holds `draft` for `at`: it was just read from there.
    pub fn loaded(&mut self, at: &Owner, draft: Option<&Draft>) {
        match draft {
            Some(draft) => self.stored.insert(at.clone(), draft.clone()),
            None => self.stored.remove(at),
        };
        self.changed.remove(at);
    }

    /// The draft of `at` is now `draft`: what to write to the store now, if anything (`Some(None)`
    /// forgets the kept one). `leaving` writes a change at once.
    pub fn due(&mut self, at: &Owner, draft: Option<Draft>, now: Instant, leaving: bool) -> Option<Option<Draft>> {
        if self.stored.get(at) == draft.as_ref() {
            self.changed.remove(at);
            return None;
        }
        self.touched.insert(at.clone());
        let since = *self.changed.entry(at.clone()).or_insert(now);
        if !leaving && draft.is_some() && now.duration_since(since) < EVERY {
            return None;
        }
        self.loaded(at, draft.as_ref());
        self.wrote.insert(at.clone());
        Some(draft)
    }

    /// Whether the user changed the draft of `at` this run.
    pub fn touched(&self, at: &Owner) -> bool {
        self.touched.contains(at)
    }

    /// Whether the draft of `at` was written this run.
    pub fn wrote(&self, at: &Owner) -> bool {
        self.wrote.contains(at)
    }

    /// How long until a changed draft is due.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.changed.values().map(|since| EVERY.saturating_sub(now.duration_since(*since))).min()
    }

    /// Forgets an account's drafts, as its account goes.
    pub fn forget(&mut self, account: &AccountId) {
        self.stored.retain(|(owner, _), _| owner != account);
        self.changed.retain(|(owner, _), _| owner != account);
        self.touched.retain(|(owner, _)| owner != account);
        self.wrote.retain(|(owner, _)| owner != account);
    }
}

impl State {
    /// Each composer holds the draft of the timeline it writes into. Leaving a timeline keeps
    /// its draft (in memory, its text in the store too); arriving brings the timeline's back,
    /// from memory, or on the first visit this run from the store. `shown` are the timelines on
    /// screen, the room's first. Returns the composers (their typing slots) that moved to
    /// another timeline.
    pub(super) fn follow_drafts(&mut self, shown: &[Owner]) -> Vec<usize> {
        let now = Instant::now();
        let mut moved = Vec::new();
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            let thread = target == ComposerTarget::Thread;
            let owner = shown.iter().find(|(_, key)| key.thread.is_some() == thread).cloned();
            if self.composer(target).owner == owner {
                continue;
            }
            moved.push(usize::from(thread));
            self.typing.left(usize::from(thread));
            if let Some(left) = self.composer_mut(target).owner.take() {
                // An edit still in the field is set aside first: only the draft is the timeline's.
                if let Some(edit) = self.composer_mut(target).set_aside_edit() {
                    self.set_edit_aside(edit);
                }
                let draft = self.composer_mut(target).take_draft();
                if let Some(keep) = self.kept_drafts.due(&left, draft.stored(), now, true) {
                    let (account, key) = left.clone();
                    self.dispatch(Command::SaveDraft { account, key, draft: keep }, Purpose::KeepDraft);
                }
                if draft.is_empty() {
                    // A read of the store still on its way now finds the composer gone; the next
                    // visit asks again.
                    if self.pending.values().any(|purpose| *purpose == Purpose::Draft(left.clone())) {
                        self.drafts_asked.remove(&left);
                    }
                    self.drafts.remove(&left);
                } else {
                    self.drafts.insert(left, draft);
                }
            }
            if let Some(at) = &owner {
                if let Some(draft) = self.drafts.remove(at) {
                    self.composer_mut(target).put_draft(draft);
                } else if self.drafts_asked.insert(at.clone()) {
                    let (account, key) = at.clone();
                    self.dispatch(Command::LoadDraft { account, key }, Purpose::Draft(at.clone()));
                }
            }
            self.composer_mut(target).owner = owner;
        }
        // The caller settles typing for these once kept edits are back too. Typing in a composer
        // that moved ends (`Typing::left`).
        moved
    }

    /// The store's draft for `at`, read on the first visit: its text goes into the composer if
    /// that still writes there and the user hasn't changed the draft (beside a reply or files
    /// picked meanwhile, or under an edit started meanwhile). What the user did meanwhile wins,
    /// and replaces it in the store too (even text typed and cleared again, here or since left).
    pub(super) fn draft_loaded(&mut self, at: Owner, draft: Option<Draft>) {
        let Some(draft) = draft else { return };
        // The store already holds what was written since this was read.
        if self.kept_drafts.wrote(&at) {
            return;
        }
        let target = [ComposerTarget::Room, ComposerTarget::Thread]
            .into_iter()
            .find(|target| self.composer(*target).owner.as_ref() == Some(&at));
        if self.kept_drafts.touched(&at) {
            let left = match target {
                Some(target) => self.composer(target).stored_draft(),
                None => self.drafts.get(&at).and_then(super::composer::Draft::stored),
            };
            self.kept_drafts.loaded(&at, Some(&draft));
            if let Some(keep) = self.kept_drafts.due(&at, left, Instant::now(), true) {
                let (account, key) = at;
                self.dispatch(Command::SaveDraft { account, key, draft: keep }, Purpose::KeepDraft);
            }
            return;
        }
        let Some(target) = target else {
            // Left meanwhile with a reply or files but no text: the text joins them there.
            if let Some(parked) = self.drafts.get_mut(&at).filter(|parked| parked.stored().is_none()) {
                parked.restore(&draft);
                self.kept_drafts.loaded(&at, Some(&draft));
            }
            return;
        };
        if self.composer(target).stored_draft().is_some() {
            return;
        }
        self.kept_drafts.loaded(&at, Some(&draft));
        self.composer_mut(target).restore_draft(&draft);
        self.settle_typing(&[usize::from(target == ComposerTarget::Thread)]);
    }

    /// Writes the drafts on screen to the store as they change (`Kept::due`).
    pub(super) fn keep_drafts(&mut self, now: Instant, tx: &mut rune::Tx) {
        self.write_drafts(now, false);
        if let Some(wait) = self.kept_drafts.wait(now) {
            tx.wake_after(wait);
        }
    }

    /// Writes the drafts on screen that changed; `now_or_never` writes them without waiting, as
    /// the window loses focus (the app may be about to quit).
    pub(super) fn write_drafts(&mut self, now: Instant, now_or_never: bool) {
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            let composer = self.composer(target);
            let Some(owner) = composer.owner.clone() else { continue };
            let draft = composer.stored_draft();
            if let Some(keep) = self.kept_drafts.due(&owner, draft, now, now_or_never) {
                let (account, key) = owner;
                self.dispatch(Command::SaveDraft { account, key, draft: keep }, Purpose::KeepDraft);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> Owner {
        (AccountId::new("@me:example.org"), TimelineKey::room("!r:example.org"))
    }

    fn draft(body: &str) -> Option<Draft> {
        Some(Draft { body: body.into(), html: None })
    }

    #[test]
    fn an_empty_composer_in_a_timeline_without_a_draft_should_write_nothing() {
        let mut kept = Kept::default();
        assert_eq!(kept.due(&at(), None, Instant::now(), false), None);
        assert_eq!(kept.due(&at(), None, Instant::now(), true), None);
    }

    #[test]
    fn a_draft_being_typed_should_be_written_once_it_has_waited() {
        let mut kept = Kept::default();
        let start = Instant::now();
        assert_eq!(kept.due(&at(), draft("he"), start, false), None);
        assert_eq!(kept.wait(start), Some(EVERY));
        assert_eq!(kept.due(&at(), draft("hello"), start + EVERY, false), Some(draft("hello")));
        assert_eq!(kept.wait(start + EVERY), None);
    }

    #[test]
    fn leaving_a_timeline_should_write_its_draft_at_once() {
        let mut kept = Kept::default();
        let now = Instant::now();
        assert_eq!(kept.due(&at(), draft("hello"), now, true), Some(draft("hello")));
    }

    #[test]
    fn an_emptied_draft_should_be_forgotten_at_once() {
        let mut kept = Kept::default();
        let now = Instant::now();
        kept.loaded(&at(), draft("hello").as_ref());
        assert_eq!(kept.due(&at(), None, now, false), Some(None));
        assert_eq!(kept.due(&at(), None, now, false), None);
    }

    #[test]
    fn a_draft_read_from_the_store_should_not_be_written_back() {
        let mut kept = Kept::default();
        kept.loaded(&at(), draft("hello").as_ref());
        assert_eq!(kept.due(&at(), draft("hello"), Instant::now(), true), None);
    }

    #[test]
    fn a_removed_account_should_leave_no_draft_waiting() {
        let mut kept = Kept::default();
        let now = Instant::now();
        kept.due(&at(), draft("he"), now, false);
        kept.forget(&at().0);
        assert_eq!(kept.wait(now), None);
    }
}
