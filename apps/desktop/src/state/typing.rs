//! Typing notices: telling the room shown whether the user is typing in it, from changes to its
//! composers' drafts. The SDK repeats a notice only once the last is about to expire, and the
//! server lets one lapse on its own, so this sends `true` as the draft changes and `false` only
//! where typing visibly ends: the draft emptied, or the user left the room.

use std::time::{Duration, Instant};

use matrix_core::AccountId;

/// How often a draft that keeps changing says so again: just under the SDK's notice lifetime.
const REPEAT: Duration = Duration::from_secs(3);

/// One notice to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub account: AccountId,
    pub room_id: String,
    pub typing: bool,
}

/// What the user is typing where, as last seen.
#[derive(Debug, Default)]
pub struct Typing {
    /// The room shown, and the drafts of its composers (the room's and the open thread's).
    room: Option<(AccountId, String)>,
    drafts: Vec<String>,
    /// When the room was last told the user is typing; `None` once told they stopped.
    told: Option<Instant>,
    /// Which composer the user typed in last (its place in `drafts`): typing stops when it empties
    /// or goes, whatever another composer holds.
    typing_in: Option<usize>,
    /// The composer typed in went on to write elsewhere (another thread): typing there ended.
    ended: bool,
}

impl Typing {
    /// The composer at `slot` now writes into another timeline: if it was the one typed in, the
    /// next update tells the room typing stopped.
    pub fn left(&mut self, slot: usize) {
        if self.typing_in == Some(slot) {
            self.typing_in = None;
            self.ended = true;
        }
    }

    /// The drafts of the composers at `slots` changed without the user typing (a draft came
    /// back): no notice for that. The other composers keep their last seen draft, so what the
    /// user did there still counts.
    pub fn seen(&mut self, drafts: &[&str], slots: &[usize]) {
        self.drafts = drafts
            .iter()
            .enumerate()
            .map(|(i, draft)| match self.drafts.get(i) {
                Some(old) if !slots.contains(&i) => old.clone(),
                _ => (*draft).to_owned(),
            })
            .collect();
    }

    /// The notices due now, for the room shown (if any) and its composers' drafts, the room's
    /// first. Opening a room or a thread with a draft already in it isn't typing; changing a
    /// draft is.
    pub fn update(&mut self, room: Option<(&AccountId, &str)>, drafts: &[&str], now: Instant) -> Vec<Notice> {
        let mut notices = Vec::new();
        let same_room = match (&self.room, room) {
            (Some((account, room_id)), Some((shown, shown_room))) => account == shown && room_id == shown_room,
            (None, None) => true,
            _ => false,
        };
        if !same_room {
            if let (Some((account, room_id)), Some(_)) = (self.room.take(), self.told.take()) {
                notices.push(Notice { account, room_id, typing: false });
            }
            self.room = room.map(|(account, room_id)| (account.clone(), room_id.to_owned()));
            self.drafts = drafts.iter().map(|draft| (*draft).to_owned()).collect();
            self.typing_in = None;
            self.ended = false;
            return notices;
        }
        let ended = std::mem::take(&mut self.ended);
        if !ended && self.drafts.iter().map(String::as_str).eq(drafts.iter().copied()) {
            return notices;
        }
        let blank = |i: usize| drafts.get(i).is_none_or(|draft| draft.trim().is_empty());
        // A composer is typed in when its own text changed; one that just showed isn't.
        let typed = (0..drafts.len()).find(|&i| self.drafts.get(i).is_some_and(|old| old != drafts[i]) && !blank(i));
        let stopped = ended || self.typing_in.is_none_or(blank);
        self.drafts = drafts.iter().map(|draft| (*draft).to_owned()).collect();
        let Some((account, room_id)) = &self.room else { return notices };
        if let Some(i) = typed {
            self.typing_in = Some(i);
            if self.told.is_none_or(|told| now.duration_since(told) >= REPEAT) {
                self.told = Some(now);
                notices.push(Notice { account: account.clone(), room_id: room_id.clone(), typing: true });
            }
        } else if stopped {
            self.typing_in = None;
            if self.told.take().is_some() {
                notices.push(Notice { account: account.clone(), room_id: room_id.clone(), typing: false });
            }
        }
        notices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountId {
        AccountId::new("@me:example.org")
    }

    fn typing(notices: &[Notice]) -> Vec<bool> {
        notices.iter().map(|notice| notice.typing).collect()
    }

    #[test]
    fn typing_in_a_room_should_tell_it() {
        let (mut state, now, me) = (Typing::default(), Instant::now(), account());
        state.update(Some((&me, "!a")), &[""], now);
        assert_eq!(typing(&state.update(Some((&me, "!a")), &["h"], now)), [true]);
    }

    #[test]
    fn typing_on_should_not_repeat_the_notice_at_once() {
        let (mut state, now, me) = (Typing::default(), Instant::now(), account());
        state.update(Some((&me, "!a")), &[""], now);
        state.update(Some((&me, "!a")), &["h"], now);
        assert!(state.update(Some((&me, "!a")), &["hi"], now + Duration::from_secs(1)).is_empty());
        assert_eq!(typing(&state.update(Some((&me, "!a")), &["hi!"], now + REPEAT)), [true]);
    }

    #[test]
    fn emptying_the_draft_should_say_typing_stopped() {
        let (mut state, now, me) = (Typing::default(), Instant::now(), account());
        state.update(Some((&me, "!a")), &[""], now);
        state.update(Some((&me, "!a")), &["h"], now);
        assert_eq!(typing(&state.update(Some((&me, "!a")), &[""], now)), [false]);
    }

    #[test]
    fn leaving_the_room_while_typing_should_tell_the_room_typing_stopped() {
        let (mut state, now, me) = (Typing::default(), Instant::now(), account());
        state.update(Some((&me, "!a")), &[""], now);
        state.update(Some((&me, "!a")), &["h"], now);
        let notices = state.update(Some((&me, "!b")), &[""], now);
        assert_eq!(notices, [Notice { account: me, room_id: "!a".into(), typing: false }]);
    }

    #[test]
    fn opening_a_room_with_a_draft_should_not_count_as_typing() {
        let (mut state, now, me) = (Typing::default(), Instant::now(), account());
        state.update(Some((&me, "!a")), &[""], now);
        assert!(state.update(Some((&me, "!b")), &["an old draft"], now).is_empty());
    }
}

#[cfg(test)]
mod composers {
    use super::*;

    const ROOM: &str = "!r:example.org";

    fn shown(typing: &mut Typing, drafts: &[&str], now: Instant) -> Vec<bool> {
        let account = AccountId::new("@me:example.org");
        typing.update(Some((&account, ROOM)), drafts, now).iter().map(|notice| notice.typing).collect()
    }

    #[test]
    fn opening_a_thread_beside_an_idle_room_draft_should_not_count_as_typing() {
        let (mut typing, now) = (Typing::default(), Instant::now());
        shown(&mut typing, &["room text"], now);
        assert_eq!(shown(&mut typing, &["room text", ""], now), Vec::<bool>::new());
    }

    #[test]
    fn closing_a_thread_typed_in_should_stop_typing_beside_an_idle_room_draft() {
        let (mut typing, now) = (Typing::default(), Instant::now());
        shown(&mut typing, &["room text", ""], now);
        assert_eq!(shown(&mut typing, &["room text", "hi"], now), [true]);
        assert_eq!(shown(&mut typing, &["room text"], now), [false]);
    }

    #[test]
    fn a_draft_coming_back_in_one_composer_should_not_swallow_a_stop_in_the_other() {
        let (mut typing, now) = (Typing::default(), Instant::now());
        shown(&mut typing, &[""], now);
        assert_eq!(shown(&mut typing, &["hi"], now), [true]);
        // The room's message is sent and a thread opens before the next look.
        typing.seen(&["", ""], &[1]);
        assert_eq!(shown(&mut typing, &["", ""], now), [false]);
    }
}
