//! Frequent rooms: how often the user opens each room on this device, so the jump list (the
//! Dock icon's menu on macOS) offers the rooms and direct chats used most. Each open adds to a
//! room's score and every other's fades a little, so rooms used lately and often come first, and
//! rooms no longer opened make way. Saved in this device's prefs, apart from its settings
//! ([`crate::platform::Shell::save_visits`]).

use std::collections::HashSet;

use matrix_core::{AccountId, RoomKind};

use super::State;
use crate::i18n::t;

/// How many rooms, and how many direct chats, are remembered; past it, the lowest scored of
/// that kind is forgotten. Apart, so many rooms opened can't push every direct chat out.
const REMEMBERED: usize = 32;
/// What one open adds to a room's score.
const OPEN: u32 = 1000;
/// The percentage of its score every other room keeps at each open.
const KEPT: u32 = 97;
/// How many rooms, and how many direct chats, the jump list offers.
const SHOWN: usize = 5;

/// One room's opens, as a score that fades as other rooms are opened.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Visit {
    pub account: String,
    pub room_id: String,
    /// A direct chat, rather than a room.
    pub direct: bool,
    pub score: u32,
}

/// The rooms opened on this device, the latest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Visits(Vec<Visit>);

impl Visits {
    /// Counts one more open of a room (a direct chat when `direct`), which becomes the latest;
    /// every other room's score fades.
    pub fn visit(&mut self, account: &AccountId, room_id: &str, direct: bool) {
        let at = self.0.iter().position(|visit| visit.account == account.as_str() && visit.room_id == room_id);
        let mut visit = match at {
            Some(at) => self.0.remove(at),
            None => Visit { account: account.as_str().to_owned(), room_id: room_id.to_owned(), direct, score: 0 },
        };
        for other in &mut self.0 {
            other.score = other.score / 100 * KEPT + other.score % 100 * KEPT / 100;
        }
        visit.direct = direct;
        visit.score = visit.score.saturating_add(OPEN);
        self.0.insert(0, visit);
        let kind = || self.0.iter().enumerate().skip(1).filter(|(_, visit)| visit.direct == direct);
        // The room just opened stays, so a new room can always come in.
        if kind().count() >= REMEMBERED
            && let Some(least) = kind().rev().min_by_key(|(_, visit)| visit.score).map(|(at, _)| at)
        {
            self.0.remove(least);
        }
    }

    /// The rooms scored highest, the latest first among equals.
    pub fn most(&self) -> impl Iterator<Item = &Visit> {
        let mut most: Vec<&Visit> = self.0.iter().collect();
        most.sort_by_key(|visit| std::cmp::Reverse(visit.score));
        most.into_iter()
    }
}

impl State {
    /// The jump list: the rooms and the direct chats opened most that are still joined, each
    /// opening through its link. With more than one account, each says whose it is. A link names
    /// no account, so a room two accounts share shows once, naming none: it opens like any link.
    pub fn jump_list(&self) -> rune::JumpList {
        let several = self.accounts.iter().nth(1).is_some();
        let (mut rooms, mut direct, mut listed) = (Vec::new(), Vec::new(), HashSet::new());
        for visit in self.visits.most() {
            let account = AccountId::new(visit.account.as_str());
            let Some(data) = self.accounts.get(&account) else { continue };
            let Some(room) = data.room(&visit.room_id) else { continue };
            if !listed.insert(room.room_id.as_str()) {
                continue;
            }
            let list = if room.kind == RoomKind::Direct { &mut direct } else { &mut rooms };
            if list.len() == SHOWN {
                continue;
            }
            let shared = self.accounts.iter().filter(|other| other.room(&room.room_id).is_some()).nth(1).is_some();
            let whose = (several && !shared).then_some(data.summary.display_name.as_str());
            let title = whose.map_or_else(|| room.name.clone(), |whose| format!("{} · {whose}", room.name));
            let link = matrix_core::permalinks::room_link(&room.room_id, account.as_str());
            list.push(rune::JumpItem::new(title, link).description(whose.unwrap_or_default()));
        }
        rune::JumpList::new().category(t!("sidebar.rooms"), rooms).category(t!("sidebar.direct"), direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visits(opens: &[&str]) -> Visits {
        let mut visits = Visits::default();
        for room in opens {
            visits.visit(&AccountId::new("@a:x"), room, false);
        }
        visits
    }

    fn rooms(visits: &Visits) -> Vec<&str> {
        visits.most().map(|visit| visit.room_id.as_str()).collect()
    }

    #[test]
    fn the_room_opened_most_should_come_first() {
        assert_eq!(rooms(&visits(&["!a", "!b", "!b", "!c"])), ["!b", "!c", "!a"]);
    }

    #[test]
    fn rooms_opened_as_often_should_come_latest_first() {
        assert_eq!(rooms(&visits(&["!a", "!b"])), ["!b", "!a"]);
    }

    #[test]
    fn a_room_opened_once_should_stay_even_when_every_other_was_opened_more() {
        let mut visits = Visits::default();
        for i in 0..REMEMBERED {
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), false);
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), false);
        }
        visits.visit(&AccountId::new("@a:x"), "!new", false);
        assert!(rooms(&visits).contains(&"!new"));
    }

    #[test]
    fn direct_chats_opened_lately_should_make_way_for_each_other_over_ones_no_longer_opened() {
        let mut visits = Visits::default();
        for i in 0..REMEMBERED {
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), true);
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), true);
        }
        for _ in 0..10 {
            visits.visit(&AccountId::new("@a:x"), "!dm-one", true);
            visits.visit(&AccountId::new("@a:x"), "!dm-two", true);
        }
        assert_eq!(rooms(&visits)[..2], ["!dm-two", "!dm-one"]);
    }

    #[test]
    fn many_rooms_opened_should_leave_the_direct_chats_remembered() {
        let mut visits = Visits::default();
        for i in 0..5 {
            visits.visit(&AccountId::new("@a:x"), &format!("!dm-{i}"), true);
        }
        for i in 0..64 {
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), false);
        }
        assert_eq!(visits.0.iter().filter(|visit| visit.direct).count(), 5);
    }

    #[test]
    fn a_room_used_long_ago_should_fall_behind_one_used_lately() {
        let mut visits = visits(&["!old", "!old", "!old"]);
        for i in 0..60 {
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), false);
        }
        visits.visit(&AccountId::new("@a:x"), "!new", false);
        assert!(
            rooms(&visits).iter().position(|room| *room == "!new")
                < rooms(&visits).iter().position(|room| *room == "!old")
        );
    }

    #[test]
    fn the_same_room_in_two_accounts_should_count_apart() {
        let mut visits = Visits::default();
        visits.visit(&AccountId::new("@a:x"), "!r", false);
        visits.visit(&AccountId::new("@b:x"), "!r", false);
        assert_eq!(visits.0.len(), 2);
    }

    #[test]
    fn past_the_bound_the_least_opened_room_should_be_forgotten() {
        let mut visits = visits(&["!kept", "!kept"]);
        for i in 0..REMEMBERED {
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"), false);
        }
        let remembered = rooms(&visits);
        assert_eq!(
            (remembered.len(), remembered.contains(&"!kept"), remembered.contains(&"!0")),
            (REMEMBERED, true, false)
        );
    }
}
