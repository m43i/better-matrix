//! Finding things: rooms, spaces and direct chats on this device, and people in the homeserver's
//! user directory. The top bar's search and the invite dialog share [`PeopleSearch`].

use std::time::{Duration, Instant};

use matrix_core::{MessageHit, Person, RequestId};
use rune::TextEdit;

/// How long the text must rest before it is searched: a word typed letter by letter is one
/// search, not one per letter.
pub const SEARCH_PAUSE: Duration = Duration::from_millis(300);

/// The local history search can finish independently of the user directory.
#[derive(Debug, Default, PartialEq, Eq)]
pub enum MessageSearchStatus {
    #[default]
    Ready,
    Searching(RequestId),
    Failed,
}

/// A query for the user directory as it is typed. A search goes out once the text rests for
/// [`SEARCH_PAUSE`] (the state's `poll` sends it); only the answer to the latest counts.
#[derive(Debug)]
pub struct PeopleSearch {
    pub field: TextEdit,
    /// The text the results are for: `None` while the text has moved on and its search hasn't
    /// gone out yet.
    searched: Option<String>,
    /// Text not searched for yet, and when it was first seen.
    resting: Option<(String, Instant)>,
    /// The search on its way, if any.
    pub request: Option<RequestId>,
    pub people: Vec<Person>,
    pub messages: Vec<MessageHit>,
    pub message_status: MessageSearchStatus,
}

impl Default for PeopleSearch {
    fn default() -> Self {
        Self {
            field: TextEdit::new(""),
            searched: Some(String::new()),
            resting: None,
            request: None,
            people: Vec::new(),
            messages: Vec::new(),
            message_status: MessageSearchStatus::Ready,
        }
    }
}

impl PeopleSearch {
    /// The text to search for at `now`: when it changed since the last search and has rested
    /// since. Clearing the text clears the results at once; results for no text (suggestions)
    /// stay until something is typed.
    pub fn due(&mut self, now: Instant) -> Option<String> {
        let text = self.field.text.trim();
        if self.searched.as_deref() == Some(text) {
            self.resting = None;
            return None;
        }
        self.messages.clear();
        self.message_status = MessageSearchStatus::Ready;
        if text.is_empty() {
            self.searched = Some(String::new());
            self.resting = None;
            self.request = None;
            self.people.clear();
            return None;
        }
        match &self.resting {
            Some((resting, since)) if resting == text && now.duration_since(*since) >= SEARCH_PAUSE => {}
            Some((resting, _)) if resting == text => return None,
            _ => {
                // The text moved on: an answer to the last search no longer fits it, and going
                // back to that text searches again.
                self.request = None;
                self.searched = None;
                self.resting = Some((text.to_owned(), now));
                return None;
            }
        }
        self.resting = None;
        self.searched = Some(text.to_owned());
        Some(text.to_owned())
    }

    /// How long until the resting text is due, if any is waiting.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.resting.as_ref().map(|(_, since)| SEARCH_PAUSE.saturating_sub(now.duration_since(*since)))
    }

    /// The people found by `request`, if it is still the latest search.
    pub fn found(&mut self, request: RequestId, people: Vec<Person>) {
        if self.request == Some(request) {
            self.people = people;
        }
    }

    pub fn messages_found(&mut self, request: RequestId, messages: Vec<MessageHit>) {
        if self.message_request() == Some(request) && self.searched.as_deref() == Some(self.query()) {
            self.messages = messages;
        }
    }

    pub fn message_request(&self) -> Option<RequestId> {
        match self.message_status {
            MessageSearchStatus::Searching(request) => Some(request),
            _ => None,
        }
    }

    pub fn finished(&mut self, request: RequestId, failed: bool) {
        if self.message_request() == Some(request) {
            self.message_status = if failed { MessageSearchStatus::Failed } else { MessageSearchStatus::Ready };
        }
        if self.request == Some(request) {
            self.request = None;
        }
    }

    /// The trimmed query.
    pub fn query(&self) -> &str {
        self.field.text.trim()
    }
}

/// A message picked in search, waiting for its timeline and history to arrive.
#[derive(Debug)]
pub struct MessageJump {
    pub account: matrix_core::AccountId,
    pub key: matrix_core::TimelineKey,
    pub event_id: String,
}

impl super::State {
    /// Drop results and stop their local history scan when a search is left.
    pub fn reset_search(&mut self) {
        if let Some(request) = self.search.message_request() {
            self.engine.send(matrix_core::Command::Cancel(request));
        }
        self.search = PeopleSearch::default();
    }

    pub fn open_found_message(&mut self, hit: &MessageHit) {
        self.open_message(
            &matrix_core::TimelineKey { room_id: hit.room_id.clone(), thread: hit.thread.clone() },
            &hit.event_id,
        );
    }

    /// Open a message from search or pinned messages, loading history until it is on screen.
    pub fn open_message(&mut self, key: &matrix_core::TimelineKey, event_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.message_jump = Some(MessageJump { account, key: key.clone(), event_id: event_id.to_owned() });
        self.open_found_room(&key.room_id);
        if let Some(root) = &key.thread {
            self.open_thread(root);
        }
    }

    pub fn position_message(&mut self, tx: &mut rune::Tx) -> bool {
        let Some(jump) = &self.message_jump else { return false };
        if self.nav.account.as_ref() != Some(&jump.account)
            || self.active_room_id() != Some(jump.key.room_id.as_str())
            || !self.open_timelines.contains(&(jump.account.clone(), jump.key.clone()))
        {
            self.message_jump = None;
            return false;
        }
        if !self.timelines_ready.contains(&(jump.account.clone(), jump.key.clone())) {
            return false;
        }
        let Some(timeline) = self.accounts.get(&jump.account).and_then(|a| a.timeline(&jump.key)) else { return false };
        if let Some(entry) = timeline.entries.iter().find(|entry| {
            matches!(&entry.kind,
            super::EntryView::Message(item) if item.message.event_id == jump.event_id)
        }) {
            let prefix = if jump.key.thread.is_some() { "thread-msg" } else { "msg" };
            tx.scroll_into_view(format!("{prefix}-{}", entry.id));
            self.message_jump = None;
            return true;
        }
        if timeline.loading || timeline.history_error.is_some() {
            return false;
        }
        if timeline.reached_start {
            self.message_jump = None;
            self.notice = Some(crate::i18n::t!("search.message_unavailable").to_owned());
            return true;
        }
        let (account, key) = (jump.account.clone(), jump.key.clone());
        if let Some(timeline) = self.accounts.timeline_mut(&account, &key) {
            timeline.loading = true;
        }
        self.dispatch(
            matrix_core::Command::LoadOlder { account: account.clone(), key: key.clone() },
            super::Purpose::History { account, key },
        );
        true
    }
}

/// Whether `name` matches a typed query: every word of it starts a word of the name, ignoring
/// case ("gr ho" finds "Grace Hopper").
pub fn matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    query.to_lowercase().split_whitespace().all(|part| words.iter().any(|word| word.starts_with(part)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_should_match_the_starts_of_words() {
        assert!(matches("Grace Hopper", "gr ho"));
        assert!(matches("#release-notes", "notes"));
        assert!(!matches("Grace Hopper", "race"));
    }

    /// Types `text` and lets it rest, returning what is due then.
    fn typed(search: &mut PeopleSearch, text: &str, at: Instant) -> Option<String> {
        search.field = TextEdit::new(text);
        search.due(at);
        search.due(at + SEARCH_PAUSE)
    }

    #[test]
    fn a_changed_query_should_be_searched_once() {
        let (mut search, now) = (PeopleSearch::default(), Instant::now());
        assert_eq!(typed(&mut search, "ada", now).as_deref(), Some("ada"));
        assert_eq!(search.due(now + SEARCH_PAUSE * 2), None);
    }

    #[test]
    fn a_query_still_being_typed_should_not_be_searched() {
        let (mut search, now) = (PeopleSearch::default(), Instant::now());
        search.field = TextEdit::new("a");
        search.due(now);
        search.field = TextEdit::new("ad");
        assert_eq!(search.due(now + SEARCH_PAUSE), None);
        assert_eq!(search.due(now + SEARCH_PAUSE * 2).as_deref(), Some("ad"));
    }

    #[test]
    fn clearing_the_query_should_clear_the_results() {
        let (mut search, now) = (PeopleSearch::default(), Instant::now());
        typed(&mut search, "ada", now);
        search.people.push(Person::default());
        search.field = TextEdit::new("  ");
        assert_eq!(search.due(now), None);
        assert!(search.people.is_empty());
    }

    #[test]
    fn suggestions_should_stay_while_nothing_is_typed() {
        let mut search = PeopleSearch { request: Some(RequestId(1)), ..PeopleSearch::default() };
        search.found(RequestId(1), vec![Person::default()]);
        assert_eq!(search.due(Instant::now()), None);
        assert_eq!(search.people.len(), 1);
    }

    #[test]
    fn an_answer_to_an_old_search_should_be_dropped() {
        let mut search = PeopleSearch { request: Some(RequestId(2)), ..PeopleSearch::default() };
        search.found(RequestId(1), vec![Person::default()]);
        assert!(search.people.is_empty());
    }

    #[test]
    fn an_answer_arriving_after_the_text_changed_should_be_dropped() {
        let (mut search, now) = (PeopleSearch::default(), Instant::now());
        typed(&mut search, "ada", now);
        search.request = Some(RequestId(1));
        search.field = TextEdit::new("grace");
        search.due(now + SEARCH_PAUSE * 2);
        search.found(RequestId(1), vec![Person::default()]);
        assert!(search.people.is_empty());
    }

    #[test]
    fn going_back_to_a_query_whose_search_was_dropped_should_search_it_again() {
        let (mut search, now) = (PeopleSearch::default(), Instant::now());
        typed(&mut search, "ada", now);
        search.field = TextEdit::new("ad");
        search.due(now + SEARCH_PAUSE);
        assert_eq!(typed(&mut search, "ada", now + SEARCH_PAUSE * 2).as_deref(), Some("ada"));
    }
}
