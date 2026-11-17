//! Read indicators use the SDK's positions and the room's existing member profiles.

use matrix_core::{AccountId, MediaRef, Person, ReadReceipt, TimelineKey};

use super::{AccountData, MemberSubmenu, Popover, State};

/// The most pictures shown in one message's read indicator.
pub const READER_FACES: usize = 4;

/// The member selected from a read indicator's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderMenu {
    pub user_id: String,
    pub submenu: Option<MemberSubmenu>,
}

impl AccountData {
    pub fn reader(&self, room_id: &str, user_id: &str) -> Person {
        if let Some(member) =
            self.members.get(room_id).and_then(|members| members.people.iter().find(|m| m.person.id == user_id))
        {
            return member.person.clone();
        }
        if let Some(message) = self
            .timeline(&TimelineKey::room(room_id))
            .and_then(|timeline| timeline.messages().rev().find(|m| m.sender_id == user_id))
        {
            return Person {
                id: user_id.to_owned(),
                name: message.sender_name.clone(),
                avatar: message.sender_avatar.clone(),
            };
        }
        Person { id: user_id.to_owned(), name: user_id.to_owned(), avatar: None }
    }

    pub fn readers(&self, key: &TimelineKey, event_id: &str) -> &[ReadReceipt] {
        if key.thread.is_some() {
            return &[];
        }
        self.timeline(key)
            .and_then(|timeline| timeline.messages().find(|m| m.event_id == event_id && !m.pending))
            .map_or(&[], |message| message.read_receipts.as_slice())
    }
}

impl State {
    pub fn toggle_readers(&mut self, key: &TimelineKey, event_id: &str) {
        let open = matches!(&self.popover, Some(Popover::ReadReceipts { key: shown, event_id: id, .. }) if shown == key && id == event_id);
        self.popover = None;
        if open || self.active_account().is_none_or(|account| account.readers(key, event_id).is_empty()) {
            return;
        }
        self.popover =
            Some(Popover::ReadReceipts { key: key.clone(), event_id: event_id.to_owned(), member: None, shown: 0..8 });
        if let Some(account) = self.nav.account.clone() {
            self.load_members(account, key.room_id.clone(), true);
        }
        self.request_avatars();
    }

    pub(super) fn refresh_readers(&mut self) {
        let Some(Popover::ReadReceipts { key, event_id, member, .. }) = &self.popover else {
            return;
        };
        let readers = self.active_account().map_or(&[][..], |account| account.readers(key, event_id));
        if readers.is_empty() {
            self.popover = None;
        } else if member.as_ref().is_some_and(|menu| readers.iter().all(|reader| reader.user_id != menu.user_id))
            && let Some(Popover::ReadReceipts { member, .. }) = &mut self.popover
        {
            *member = None;
        }
    }

    pub fn toggle_reader_menu(&mut self, user_id: &str) {
        if let Some(Popover::ReadReceipts { member, .. }) = &mut self.popover {
            *member = if member.as_ref().is_some_and(|m| m.user_id == user_id) {
                None
            } else {
                Some(ReaderMenu { user_id: user_id.to_owned(), submenu: None })
            };
        }
    }

    pub fn readers_shown(&mut self, range: std::ops::Range<usize>) -> bool {
        let Some(Popover::ReadReceipts { shown, .. }) = &mut self.popover else {
            return false;
        };
        if *shown == range {
            return false;
        }
        *shown = range;
        self.request_avatars();
        true
    }

    pub(super) fn reader_pictures(&self) -> Vec<(AccountId, MediaRef)> {
        let (Some(account), Some(Popover::ReadReceipts { key, event_id, shown, .. })) =
            (self.active_account(), &self.popover)
        else {
            return Vec::new();
        };
        account
            .readers(key, event_id)
            .iter()
            .skip(shown.start)
            .take(shown.len())
            .filter_map(|receipt| {
                Some((account.summary.id.clone(), account.reader(&key.room_id, &receipt.user_id).avatar?))
            })
            .collect()
    }
}
