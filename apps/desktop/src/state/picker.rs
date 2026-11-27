//! The sticker and GIF picker (`Popover::Stickers`): one composer button, two tabs. Stickers
//! come from the room's image packs ([`super::packs`]), filtered as one types; GIFs come from
//! GIPHY with the user's own API key (Settings → General, or the tab itself), trending until
//! something is typed, searched once typing pauses: GIPHY's free keys allow few searches an
//! hour. A GIF goes as its GIPHY link, which this app plays and others show as a link.
//!
//! The key is kept in the system keychain ([`crate::platform::Shell::save_gif_key`]), read at
//! launch, and held in memory: once kept it is never shown again, only replaced or removed.

use std::time::{Duration, Instant};

use matrix_core::{Command, FoundGif, GifPage, Outgoing, RequestId};
use rune::{TextEdit, Tx};

use super::{ComposerTarget, Popover, Purpose, State};

/// How long typing must pause before GIPHY is asked.
pub const GIF_SEARCH_PAUSE: Duration = Duration::from_millis(500);
/// GIPHY's "Powered by GIPHY" mark, from its attribution marks, shown under the GIF grid.
const GIPHY_MARK: &[u8] = include_bytes!("../../assets/giphy-powered-by.png");
/// [`GIPHY_MARK`]'s width and height in pixels.
pub const GIPHY_MARK_SIZE: (u32, u32) = (641, 136);

/// The handle [`GIPHY_MARK`] is kept under in the media cache.
pub fn giphy_mark() -> matrix_core::MediaRef {
    matrix_core::MediaRef::opaque("app:giphy-powered-by")
}

/// Where a GIPHY API key is made.
pub const GIPHY_KEYS: &str = "https://developers.giphy.com/dashboard/";

/// The picker's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerTab {
    #[default]
    Stickers,
    Gifs,
}

/// The user's GIPHY API key, as the keychain holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum GifKey {
    /// Being read from the keychain at launch.
    #[default]
    Loading,
    /// None kept.
    Missing,
    /// The key kept.
    Kept(String),
    /// A key the keychain didn't take: it works until the app quits.
    Session(String),
}

/// The picker's state, kept while it is closed so it opens where it was left.
#[derive(Debug)]
pub struct Picker {
    pub tab: PickerTab,
    /// Narrows the stickers shown by short code or description.
    pub filter: TextEdit,
    pub gifs: GifSearch,
    /// The user's GIPHY API key.
    pub giphy: GifKey,
    /// The GIF tab's previews' players, apart from the timelines' GIFs.
    pub players: super::gifs::GifCache,
    /// A GIPHY API key being typed, here or in Settings: a first one, or one replacing the
    /// one kept.
    pub key: TextEdit,
    /// A new key is being typed to replace the one kept.
    pub replacing: bool,
}

impl Default for Picker {
    fn default() -> Self {
        Self {
            tab: PickerTab::default(),
            filter: TextEdit::default(),
            gifs: GifSearch::default(),
            giphy: GifKey::default(),
            players: super::gifs::GifCache::with_players(super::gifs::PICKER_PLAYERS),
            key: TextEdit::default(),
            replacing: false,
        }
    }
}

/// GIF search: what is typed, what was searched for and what it found.
#[derive(Debug, Default)]
pub struct GifSearch {
    pub field: TextEdit,
    /// The query the GIFs shown are for: `None` until the first search goes out, or after the
    /// key changed.
    searched: Option<String>,
    /// Text not searched for yet, and when it was first seen.
    resting: Option<(String, Instant)>,
    /// The search on its way (a first page, or the next one).
    pub request: Option<RequestId>,
    pub gifs: Vec<FoundGif>,
    /// Where the next page starts, when there is one.
    pub next: Option<u32>,
    /// The page asked for last (query and offset), to ask again after a failure.
    asked: Option<(String, u32)>,
    /// How many pages show.
    pages: usize,
    /// Why the last search failed.
    pub error: Option<String>,
}

impl GifSearch {
    /// The query to search for now: a blank one at once (what's trending), typed text once it
    /// has rested [`GIF_SEARCH_PAUSE`], each once.
    pub fn due(&mut self, now: Instant) -> Option<String> {
        let text = self.field.text.trim().to_owned();
        if self.searched.as_deref() == Some(text.as_str()) {
            self.resting = None;
            return None;
        }
        let rested = match &self.resting {
            _ if text.is_empty() => true,
            Some((resting, since)) if *resting == text => now.duration_since(*since) >= GIF_SEARCH_PAUSE,
            _ => {
                self.resting = Some((text.clone(), now));
                false
            }
        };
        rested.then(|| self.start(text))
    }

    /// The typed text, to search for at once (Enter); `None` when it is what shows.
    pub fn submit(&mut self) -> Option<String> {
        let text = self.field.text.trim().to_owned();
        (self.searched.as_deref() != Some(text.as_str())).then(|| self.start(text))
    }

    fn start(&mut self, text: String) -> String {
        self.resting = None;
        self.searched = Some(text.clone());
        self.gifs.clear();
        self.next = None;
        self.error = None;
        text
    }

    /// How long until the text typed is due.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.resting.as_ref().map(|(_, since)| GIF_SEARCH_PAUSE.saturating_sub(now.duration_since(*since)))
    }

    /// The query the GIFs shown are for.
    pub fn searched(&self) -> Option<&str> {
        self.searched.as_deref()
    }

    /// Whether a search is on its way.
    pub fn searching(&self) -> bool {
        self.request.is_some()
    }

    /// A page found by `request`, if it is still the latest search: the first replaces what
    /// shows, a later one adds to it.
    pub fn found(&mut self, request: RequestId, offset: u32, page: GifPage) {
        if self.request != Some(request) {
            return;
        }
        self.request = None;
        if offset == 0 {
            self.gifs = page.gifs;
            self.pages = 1;
        } else {
            self.pages += 1;
            let known: std::collections::HashSet<String> = self.gifs.iter().map(|gif| gif.id.clone()).collect();
            self.gifs.extend(page.gifs.into_iter().filter(|gif| !known.contains(&gif.id)));
        }
        self.next = page.next;
    }

    /// The page that failed, to ask for again, its error cleared; none without a failure.
    pub fn retry(&mut self) -> Option<(String, u32)> {
        self.error.take()?;
        self.asked.clone()
    }

    /// `request` failed: it says why, if it is still the latest.
    pub fn failed(&mut self, request: RequestId, error: String) {
        if self.request == Some(request) {
            self.request = None;
            self.error = Some(error);
        }
    }

    /// Forgets what was found, so the next look searches again (the key changed).
    pub fn reset(&mut self) {
        *self = Self { field: std::mem::take(&mut self.field), ..Self::default() };
    }
}

impl State {
    /// The picker just opened: a GIF tab without results searches.
    pub(super) fn picker_opened(&mut self) {
        match self.picker.tab {
            PickerTab::Stickers => self.picker.filter.focused = true,
            PickerTab::Gifs if self.asks_gif_key() => self.picker.key.focused = true,
            PickerTab::Gifs => self.picker.gifs.field.focused = true,
        }
        self.load_giphy_mark();
        // A search that failed (offline, say) is tried again as the picker opens.
        self.retry_gifs();
        self.search_gifs(Instant::now());
    }

    /// Decodes GIPHY's mark for the GIF tab, unless it is here.
    pub fn load_giphy_mark(&mut self) {
        if self.media.wants(&giphy_mark(), Instant::now()) {
            self.media.loaded(giphy_mark(), GIPHY_MARK.into(), Some(GIPHY_MARK_SIZE));
        }
    }

    /// Shows a tab of the picker.
    pub fn show_picker_tab(&mut self, tab: PickerTab) {
        if self.picker.tab == tab {
            return;
        }
        self.picker.tab = tab;
        // GIPHY's answers aren't kept between visits: back on the GIF tab, it searches afresh.
        self.picker.gifs.reset();
        self.picker_opened();
        // Leaving the GIF tab lets its previews go.
        self.request_gifs();
    }

    /// The user's GIPHY API key, if one is kept.
    fn gif_key(&self) -> Option<&str> {
        match &self.picker.giphy {
            GifKey::Kept(key) | GifKey::Session(key) => Some(key.as_str()),
            GifKey::Loading | GifKey::Missing => None,
        }
    }

    /// Whether the user gave a GIPHY API key.
    pub fn has_gif_key(&self) -> bool {
        self.gif_key().is_some()
    }

    /// Whether the GIF tab asks for a key: none is kept (once the keychain answered), or a new
    /// one is being typed to replace it.
    pub fn asks_gif_key(&self) -> bool {
        self.picker.giphy == GifKey::Missing || self.picker.replacing
    }

    /// Whether the picker shows its GIF tab now.
    pub fn picking_gifs(&self) -> bool {
        matches!(self.popover, Some(Popover::Stickers(_))) && self.picker.tab == PickerTab::Gifs
    }

    /// The previews the GIF tab shows now: they load and play while it is open, whatever the
    /// setting for GIFs in messages says.
    pub(super) fn picker_gif_previews(&self) -> impl Iterator<Item = &str> {
        self.picking_gifs()
            .then_some(self.picker.gifs.gifs.iter())
            .into_iter()
            .flatten()
            .map(|gif| gif.preview.as_str())
    }

    /// Sends the search the GIF tab is due, and wakes when typing will have paused. Called
    /// from `poll`.
    pub(super) fn search_gifs(&mut self, now: Instant) {
        if !self.picking_gifs() || !self.has_gif_key() || self.picker.replacing {
            return;
        }
        if let Some(query) = self.picker.gifs.due(now) {
            self.ask_gifs(query, 0);
        }
    }

    /// Searches for what the GIF field holds at once (Enter).
    pub fn submit_gif_search(&mut self) {
        if let Some(query) = self.picker.gifs.submit()
            && self.has_gif_key()
        {
            self.ask_gifs(query, 0);
        }
    }

    /// Whether the GIF tab offers more of what it shows: GIPHY has more, and the picker's
    /// players would hold every GIF shown (two pages; a narrower search finds the rest).
    pub fn more_gifs_offered(&self) -> bool {
        let at_most = (self.picker.gifs.pages + 1) * matrix_core::gif_search::PAGE as usize;
        self.picker.gifs.next.is_some() && at_most <= super::gifs::PICKER_PLAYERS
    }

    /// Asks for the next page of what the GIF tab shows.
    pub fn more_gifs(&mut self) {
        if !self.more_gifs_offered() {
            return;
        }
        let (Some(next), Some(query)) = (self.picker.gifs.next, self.picker.gifs.searched().map(str::to_owned)) else {
            return;
        };
        if !self.picker.gifs.searching() {
            self.ask_gifs(query, next);
        }
    }

    fn ask_gifs(&mut self, query: String, offset: u32) {
        let language = crate::i18n::I18N.locales()[self.settings.language.resolve()].id().to_owned();
        let key = self.gif_key().unwrap_or_default().to_owned();
        self.picker.gifs.asked = Some((query.clone(), offset));
        let command = Command::SearchGifs { key, query, offset, language: Some(language) };
        let request = self.dispatch(command, Purpose::GifSearch);
        self.picker.gifs.request = Some(request);
    }

    /// Asks again for the page whose search failed (Try again, or the picker opening again).
    pub fn retry_gifs(&mut self) {
        if !self.picking_gifs() || !self.has_gif_key() || self.picker.gifs.searching() {
            return;
        }
        if let Some((query, offset)) = self.picker.gifs.retry() {
            self.ask_gifs(query, offset);
        }
    }

    /// A page of GIFs found; their previews load.
    pub(super) fn gifs_found(&mut self, request: RequestId, offset: u32, page: GifPage) {
        self.picker.gifs.found(request, offset, page);
        self.request_gifs();
    }

    /// Sends a GIF from a composer's room or thread, answering what its draft answers.
    pub fn send_gif(&mut self, target: ComposerTarget, gif: FoundGif, tx: &mut Tx) {
        self.popover = None;
        self.request_gifs();
        let reply = self.composer_mut(target).reply_to.take();
        self.send_from(target, reply, vec![Outgoing::Gif(gif)], tx);
    }

    /// The keychain answered at launch with the key kept, if any.
    pub fn gif_key_loaded(&mut self, key: Option<String>) {
        if self.picker.giphy != GifKey::Loading {
            return;
        }
        self.picker.giphy = key.filter(|key| !key.trim().is_empty()).map_or(GifKey::Missing, GifKey::Kept);
        if self.picking_gifs() {
            self.picker_opened();
        }
    }

    /// Keeps the GIPHY API key typed, in the keychain; the GIF tab searches again with it.
    pub fn save_gif_key(&mut self) {
        let key = self.picker.key.text.trim().to_owned();
        if key.is_empty() {
            return;
        }
        let previous = self.gif_key().map(str::to_owned);
        self.shell.save_gif_key(Some(key.clone()), previous);
        self.gif_key_changed(GifKey::Kept(key));
    }

    /// The keychain didn't take a change to `key` (`None`: removing it). A new key still works
    /// until the app quits, if it is still the one in use; a key that couldn't be removed is
    /// still there, so it shows as kept again.
    pub fn gif_key_not_kept(&mut self, key: Option<String>, previous: Option<String>, error: &str) {
        let Some(key) = key else {
            self.notice = Some(super::Notice::problem(crate::i18n::tf!("notice.gif_key_remove", error = error)));
            if let (GifKey::Missing, Some(previous)) = (&self.picker.giphy, previous) {
                self.gif_key_changed(GifKey::Kept(previous));
            }
            return;
        };
        self.notice = Some(super::Notice::problem(crate::i18n::tf!("notice.gif_key", error = error)));
        // Not kept: it works until the app quits, and Settings says so; a newer key is left be.
        if self.picker.giphy == GifKey::Kept(key.clone()) {
            self.picker.giphy = GifKey::Session(key);
        }
    }

    /// Forgets the GIPHY API key, here and in the keychain.
    pub fn remove_gif_key(&mut self) {
        let previous = self.gif_key().map(str::to_owned);
        self.shell.save_gif_key(None, previous);
        self.gif_key_changed(GifKey::Missing);
    }

    /// Starts typing a new key to replace the one kept, which stays until the new one is saved.
    pub fn replace_gif_key(&mut self) {
        self.picker.replacing = true;
        self.picker.key = TextEdit::default();
        self.picker.key.focused = true;
    }

    /// Stops replacing the key: the one kept stays.
    pub fn cancel_gif_key(&mut self) {
        self.picker.replacing = false;
        self.picker.key = TextEdit::default();
        if self.picking_gifs() {
            self.picker_opened();
        }
    }

    fn gif_key_changed(&mut self, key: GifKey) {
        self.picker.giphy = key;
        self.picker.replacing = false;
        self.picker.key = TextEdit::default();
        self.picker.gifs.reset();
        if self.picking_gifs() {
            self.picker_opened();
        }
        self.request_gifs();
    }

    /// Opens GIPHY's page for making an API key in the browser.
    pub fn open_giphy_keys(&mut self, tx: &mut Tx) {
        self.open_link(GIPHY_KEYS, tx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(search: &mut GifSearch, text: &str) {
        search.field = TextEdit::new(text);
    }

    #[test]
    fn a_kept_key_should_be_replaced_only_once_a_new_one_is_saved() {
        let mut s = State::detached();
        s.gif_key_changed(GifKey::Kept("old".into()));
        s.replace_gif_key();
        assert!(s.asks_gif_key() && s.has_gif_key() && s.picker.key.text.is_empty());
        s.picker.key = TextEdit::new(" new ");
        s.save_gif_key();
        assert_eq!((&s.picker.giphy, s.picker.replacing), (&GifKey::Kept("new".into()), false));
    }

    #[test]
    fn cancelling_a_replacement_should_keep_the_key() {
        let mut s = State::detached();
        s.gif_key_changed(GifKey::Kept("old".into()));
        s.replace_gif_key();
        s.cancel_gif_key();
        assert!(!s.asks_gif_key() && s.has_gif_key());
    }

    #[test]
    fn a_removed_key_should_be_asked_for_again() {
        let mut s = State::detached();
        s.gif_key_changed(GifKey::Kept("old".into()));
        s.remove_gif_key();
        assert!(s.asks_gif_key() && !s.has_gif_key());
    }

    #[test]
    fn a_key_the_keychain_could_not_remove_should_show_as_kept_again() {
        let mut s = State::detached();
        s.gif_key_changed(GifKey::Kept("old".into()));
        s.remove_gif_key();
        s.gif_key_not_kept(None, Some("old".into()), "locked");
        assert_eq!(s.picker.giphy, GifKey::Kept("old".into()));
        assert!(s.notice.is_some());
    }

    #[test]
    fn a_key_the_keychain_refused_should_last_the_run_and_say_so() {
        let mut s = State::detached();
        s.picker.key = TextEdit::new("new");
        s.save_gif_key();
        s.gif_key_not_kept(Some("new".into()), None, "locked");
        assert_eq!(s.picker.giphy, GifKey::Session("new".into()));
        assert!(s.has_gif_key());
    }

    #[test]
    fn an_older_keys_failure_should_leave_a_newer_key_be() {
        let mut s = State::detached();
        s.gif_key_changed(GifKey::Kept("newer".into()));
        s.gif_key_not_kept(Some("older".into()), None, "locked");
        assert_eq!(s.picker.giphy, GifKey::Kept("newer".into()));
    }

    #[test]
    fn the_gif_tab_should_offer_no_more_pages_than_its_players_hold() {
        let mut s = State::detached();
        s.picker.gifs.request = Some(RequestId(1));
        s.picker.gifs.found(RequestId(1), 0, GifPage { gifs: vec![gif("a")], next: Some(24) });
        assert!(s.more_gifs_offered());
        // A second page, short of its 24 (duplicates dropped), still makes two.
        s.picker.gifs.request = Some(RequestId(2));
        s.picker.gifs.found(RequestId(2), 24, GifPage { gifs: vec![gif("a"), gif("b")], next: Some(48) });
        assert!(!s.more_gifs_offered());
    }

    #[test]
    fn a_failed_page_should_be_asked_for_again() {
        let mut search =
            GifSearch { asked: Some(("cat".into(), 24)), request: Some(RequestId(4)), ..GifSearch::default() };
        search.failed(RequestId(4), "offline".into());
        assert_eq!(search.retry(), Some(("cat".into(), 24)));
        assert!(search.error.is_none() && search.retry().is_none());
    }

    #[test]
    fn a_blank_key_from_the_keychain_should_count_as_none() {
        let mut s = State::detached();
        s.picker.giphy = GifKey::Loading;
        s.gif_key_loaded(Some("  ".into()));
        assert_eq!(s.picker.giphy, GifKey::Missing);
    }

    #[test]
    fn a_blank_search_should_ask_for_trending_gifs_at_once() {
        let mut search = GifSearch::default();
        assert_eq!(search.due(Instant::now()).as_deref(), Some(""));
    }

    #[test]
    fn typed_text_should_be_searched_only_once_typing_pauses() {
        let (mut search, now) = (GifSearch::default(), Instant::now());
        search.due(now);
        typed(&mut search, "cat");
        assert_eq!(search.due(now), None);
        assert_eq!(search.due(now + GIF_SEARCH_PAUSE).as_deref(), Some("cat"));
        assert_eq!(search.due(now + GIF_SEARCH_PAUSE * 2), None);
    }

    #[test]
    fn enter_should_search_at_once() {
        let mut search = GifSearch::default();
        typed(&mut search, "dog");
        assert_eq!(search.submit().as_deref(), Some("dog"));
        assert_eq!(search.submit(), None);
    }

    #[test]
    fn an_answer_to_an_older_search_should_be_dropped() {
        let mut search = GifSearch { request: Some(RequestId(2)), ..GifSearch::default() };
        let page = GifPage { gifs: vec![gif("a")], next: None };
        search.found(RequestId(1), 0, page);
        assert!(search.gifs.is_empty());
    }

    #[test]
    fn a_next_page_should_add_to_the_gifs_shown_once_each() {
        let mut search = GifSearch { gifs: vec![gif("a")], request: Some(RequestId(3)), ..GifSearch::default() };
        search.found(RequestId(3), 24, GifPage { gifs: vec![gif("a"), gif("b")], next: Some(48) });
        let ids: Vec<&str> = search.gifs.iter().map(|gif| gif.id.as_str()).collect();
        assert_eq!((ids, search.next), (vec!["a", "b"], Some(48)));
    }

    fn gif(id: &str) -> FoundGif {
        FoundGif {
            id: id.into(),
            title: String::new(),
            url: format!("https://giphy.com/gifs/{id}"),
            preview: format!("https://media.giphy.com/media/{id}/200w.gif"),
            preview_size: Some((200, 150)),
            sent_ping: None,
        }
    }
}
