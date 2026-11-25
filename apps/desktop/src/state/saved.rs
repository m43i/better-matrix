//! The saved messages dialog's state: whose messages it lists, the search typed over them, and
//! the quick filters (when a message was sent, where it was saved) its filter menu picks.

use matrix_core::{AccountId, RoomKind, SavedMessage};
use rune::TextEdit;

use super::{AccountData, Modal, State, matches};

/// The open saved messages dialog (`Modal::Saved`).
#[derive(Clone, Debug, PartialEq)]
pub struct SavedList {
    /// Whose saved messages show; not always the account on screen (a call answered for another
    /// account switches it).
    pub account: AccountId,
    /// Words to find in a message's text, its sender, or the room or direct chat it was sent in.
    pub search: TextEdit,
    pub filter: SavedFilter,
    /// Whether the filter menu shows. The dialog owns it, so it goes with the dialog however
    /// that closes or is replaced.
    pub filters_open: bool,
}

impl SavedList {
    /// The list of `account`'s saved messages, unfiltered, ready to type a search into.
    pub fn new(account: AccountId) -> Self {
        let mut search = TextEdit::default();
        search.focused = true;
        Self { account, search, filter: SavedFilter::default(), filters_open: false }
    }

    /// The saved messages of `account` this list shows at `now_ms`, in the account's order
    /// (most recently saved first).
    pub fn shown<'a>(&self, account: &'a AccountData, now_ms: u64) -> Vec<&'a SavedMessage> {
        let since = self.filter.since;
        let query = self.search.text.trim();
        account
            .saved_messages()
            .filter(|saved| {
                since == SavedSince::AnyTime
                    || saved.preview.timestamp_ms.is_some_and(|sent| since.includes(sent, now_ms))
            })
            .filter(|saved| self.filter.place.includes(account, &saved.room_id))
            .filter(|saved| query.is_empty() || matches(&searchable(account, saved), query))
            .collect()
    }
}

/// The quick filters over saved messages; the default leaves them all in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedFilter {
    pub since: SavedSince,
    pub place: SavedIn,
}

impl SavedFilter {
    /// Whether any filter is chosen.
    pub fn active(&self) -> bool {
        *self != Self::default()
    }
}

/// How recently a saved message was sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SavedSince {
    #[default]
    AnyTime,
    /// Since midnight, on the user's clock.
    Today,
    /// The last 7 days.
    Week,
    /// The last 30 days.
    Month,
    /// The last 365 days.
    Year,
}

impl SavedSince {
    pub const ALL: [Self; 5] = [Self::AnyTime, Self::Today, Self::Week, Self::Month, Self::Year];

    /// Whether a message sent at `sent_ms` falls in this span at `now_ms`. Today compares
    /// calendar days on the user's clock, so a day that skips midnight (a summer time change)
    /// still starts where the date does.
    pub fn includes(self, sent_ms: u64, now_ms: u64) -> bool {
        const DAY: u64 = 24 * 60 * 60 * 1000;
        let days = match self {
            Self::AnyTime => return true,
            Self::Today => return local_day(sent_ms).is_some_and(|day| Some(day) == local_day(now_ms)),
            Self::Week => 7,
            Self::Month => 30,
            Self::Year => 365,
        };
        sent_ms >= now_ms.saturating_sub(days * DAY)
    }
}

/// The calendar day of a UTC time on the user's clock.
fn local_day(ms: u64) -> Option<chrono::NaiveDate> {
    use chrono::TimeZone;
    let ms = i64::try_from(ms).ok()?;
    chrono::Local.timestamp_millis_opt(ms).single().map(|at| at.date_naive())
}

/// Where a saved message was sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SavedIn {
    #[default]
    Anywhere,
    /// Rooms, not direct chats.
    Rooms,
    /// Direct chats.
    Direct,
    /// One room or direct chat.
    Room(String),
}

impl SavedIn {
    /// Whether a message saved in `room_id` of `account` is kept. A room the account has left
    /// counts as neither a room nor a direct chat.
    pub fn includes(&self, account: &AccountData, room_id: &str) -> bool {
        let kind = || account.room(room_id).map(|room| room.kind);
        match self {
            Self::Anywhere => true,
            Self::Rooms => kind() == Some(RoomKind::Group),
            Self::Direct => kind() == Some(RoomKind::Direct),
            Self::Room(chosen) => chosen == room_id,
        }
    }
}

/// A room as the saved list names it, as the sidebar does: `#general`, or a direct chat's
/// person; the room's ID when the account no longer has it.
pub fn saved_room_label(account: &AccountData, room_id: &str) -> String {
    match account.room(room_id) {
        Some(room) if room.kind == RoomKind::Group => format!("#{}", room.name),
        Some(room) => room.name.clone(),
        None => room_id.to_owned(),
    }
}

/// The rooms and direct chats `account` saved messages in, named, rooms first, each group by
/// name: what the filter menu offers to narrow the list to.
pub fn saved_rooms(account: &AccountData) -> Vec<(String, String)> {
    let mut rooms: Vec<(String, String)> = Vec::new();
    for saved in account.saved_messages() {
        if !rooms.iter().any(|(id, _)| *id == saved.room_id) {
            rooms.push((saved.room_id.clone(), saved_room_label(account, &saved.room_id)));
        }
    }
    let direct = |id: &str| account.room(id).is_some_and(|room| room.kind == RoomKind::Direct);
    rooms.sort_by_cached_key(|(id, label)| (direct(id), label.to_lowercase()));
    rooms
}

/// What a search over a saved message looks through: its sender, where it was sent, its text.
fn searchable(account: &AccountData, saved: &SavedMessage) -> String {
    let preview = &saved.preview;
    let room = saved_room_label(account, &saved.room_id);
    format!("{} {room} {}", preview.sender_name, preview.body.as_deref().unwrap_or_default())
}

impl State {
    /// The open saved messages dialog, if one shows.
    pub fn saved_list(&self) -> Option<&SavedList> {
        match &self.modal {
            Some(Modal::Saved(list)) => Some(list),
            _ => None,
        }
    }

    fn saved_list_mut(&mut self) -> Option<&mut SavedList> {
        match &mut self.modal {
            Some(Modal::Saved(list)) => Some(list),
            _ => None,
        }
    }

    /// Opens (or closes) the saved messages' filter menu.
    pub fn toggle_saved_filters(&mut self) {
        if let Some(list) = self.saved_list_mut() {
            list.filters_open = !list.filters_open;
        }
    }

    /// Closes the saved messages' filter menu, leaving the dialog open.
    pub fn close_saved_filters(&mut self) {
        if let Some(list) = self.saved_list_mut() {
            list.filters_open = false;
        }
    }

    /// Keeps the saved messages sent within `since`; the menu stays open for another choice.
    pub fn filter_saved_since(&mut self, since: SavedSince) {
        if let Some(list) = self.saved_list_mut() {
            list.filter.since = since;
        }
    }

    /// Keeps the saved messages sent in `place`; the menu stays open for another choice.
    pub fn filter_saved_in(&mut self, place: SavedIn) {
        if let Some(list) = self.saved_list_mut() {
            list.filter.place = place;
        }
    }

    /// Drops every filter, keeping the search.
    pub fn clear_saved_filters(&mut self) {
        if let Some(list) = self.saved_list_mut() {
            list.filter = SavedFilter::default();
        }
    }

    /// Shows every saved message again: no search, no filter.
    pub fn show_all_saved(&mut self) {
        if let Some(list) = self.saved_list_mut() {
            list.search.clear();
            list.filter = SavedFilter::default();
        }
    }
}

#[cfg(test)]
mod tests;
