//! Frequent rooms: how often the user opens each room on this device, so the jump list (a
//! right-click on the taskbar button on Windows, the Dock icon's menu on macOS) offers the rooms
//! and direct chats used most. Kept with this device's [`super::Settings`].

use matrix_core::{AccountId, RoomKind};

use super::State;
use crate::i18n::t;

/// How many rooms are remembered; past it, the least opened is forgotten.
const REMEMBERED: usize = 64;
/// How many rooms, and how many direct chats, the jump list offers.
const SHOWN: usize = 5;

/// One room's opens.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Visit {
    pub account: String,
    pub room_id: String,
    pub count: u32,
}

/// The rooms opened on this device, the latest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Visits(Vec<Visit>);

impl Visits {
    /// Counts one more open of a room, which becomes the latest.
    pub fn visit(&mut self, account: &AccountId, room_id: &str) {
        let at = self.0.iter().position(|visit| visit.account == account.as_str() && visit.room_id == room_id);
        let mut visit = match at {
            Some(at) => self.0.remove(at),
            None => Visit { account: account.as_str().to_owned(), room_id: room_id.to_owned(), count: 0 },
        };
        visit.count = visit.count.saturating_add(1);
        self.0.insert(0, visit);
        if self.0.len() > REMEMBERED
            // The room just opened stays, so a new room can always come in.
            && let Some(least) = self.0.iter().enumerate().skip(1).rev().min_by_key(|(_, visit)| visit.count).map(|(at, _)| at)
        {
            self.0.remove(least);
        }
    }

    /// The rooms opened most, the latest first among equals.
    pub fn most(&self) -> impl Iterator<Item = &Visit> {
        let mut most: Vec<&Visit> = self.0.iter().collect();
        most.sort_by_key(|visit| std::cmp::Reverse(visit.count));
        most.into_iter()
    }
}

impl State {
    /// The jump list: the rooms and the direct chats opened most that are still joined, each
    /// opening through its link. With more than one account, each says whose it is.
    pub fn jump_list(&self) -> rune::JumpList {
        let several = self.accounts.iter().nth(1).is_some();
        let (mut rooms, mut direct) = (Vec::new(), Vec::new());
        for visit in self.settings.visits.most() {
            let account = AccountId::new(visit.account.as_str());
            let Some(data) = self.accounts.get(&account) else { continue };
            let Some(room) = data.room(&visit.room_id) else { continue };
            let list = if room.kind == RoomKind::Direct { &mut direct } else { &mut rooms };
            if list.len() == SHOWN {
                continue;
            }
            let title =
                if several { format!("{} · {}", room.name, data.summary.display_name) } else { room.name.clone() };
            let link = matrix_core::permalinks::room_link(&room.room_id, account.as_str());
            list.push(rune::JumpItem::new(title, link).description(data.summary.display_name.as_str()));
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
            visits.visit(&AccountId::new("@a:x"), room);
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
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"));
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"));
        }
        visits.visit(&AccountId::new("@a:x"), "!new");
        assert!(rooms(&visits).contains(&"!new"));
    }

    #[test]
    fn the_same_room_in_two_accounts_should_count_apart() {
        let mut visits = Visits::default();
        visits.visit(&AccountId::new("@a:x"), "!r");
        visits.visit(&AccountId::new("@b:x"), "!r");
        assert_eq!(visits.0.len(), 2);
    }

    #[test]
    fn past_the_bound_the_least_opened_room_should_be_forgotten() {
        let mut visits = visits(&["!kept", "!kept"]);
        for i in 0..REMEMBERED {
            visits.visit(&AccountId::new("@a:x"), &format!("!{i}"));
        }
        let remembered = rooms(&visits);
        assert_eq!(
            (remembered.len(), remembered.contains(&"!kept"), remembered.contains(&"!0")),
            (REMEMBERED, true, false)
        );
    }
}
