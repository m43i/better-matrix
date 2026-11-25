use matrix_core::{EngineHandle, MessagePreview, RoomList};

use super::*;
use crate::demo::{ADA, seed};
use crate::platform::Shell;
use crate::state::Settings;

const GENERAL: &str = "!general:example.org";
const GRACE: &str = "!dm-grace:example.org";
const DAY: u64 = 24 * 60 * 60 * 1000;
/// A fixed "now", far from any day's edge in every time zone that matters here.
const NOW: u64 = 1_780_000_000_000;

fn ada() -> AccountId {
    AccountId::new(ADA)
}

fn message(room_id: &str, event_id: &str, sender: &str, body: &str, sent_ms: u64) -> SavedMessage {
    SavedMessage {
        room_id: room_id.into(),
        saved_at_ms: Some(sent_ms),
        preview: MessagePreview {
            event_id: event_id.into(),
            sender_name: sender.into(),
            sender_id: format!("@{}:example.org", sender.to_lowercase()),
            sender_avatar: None,
            timestamp_ms: Some(sent_ms),
            body: Some(body.into()),
            thread: None,
        },
    }
}

/// Ada's demo account with the saved messages open: one in #general a year ago, one in
/// #general last week, one from Grace's direct chat an hour ago.
fn saved_state() -> State {
    let (engine, _) = EngineHandle::detached();
    let mut s = State::new(engine, Shell::headless(), Settings::fixed_english());
    seed(&mut s);
    let messages = vec![
        message(GRACE, "$recent", "Grace", "The sync loop is fixed", NOW - 60 * 60 * 1000),
        message(GENERAL, "$week", "Alan", "Release notes are up", NOW - 5 * DAY),
        message(GENERAL, "$old", "Alan", "Welcome to the team", NOW - 300 * DAY),
    ];
    let data = s.accounts.get(&ada()).expect("account");
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    for room in &mut rooms {
        room.saved = messages
            .iter()
            .filter(|saved| saved.room_id == room.room_id)
            .map(|saved| saved.preview.event_id.clone())
            .collect();
    }
    s.accounts.set_room_list(&ada(), RoomList { rooms, spaces, invitations: Vec::new() });
    s.open_saved();
    s.accounts.set_saved_messages(&ada(), messages);
    s
}

fn shown(s: &State) -> Vec<String> {
    let list = s.saved_list().expect("saved messages open");
    let account = s.accounts.get(&list.account).expect("account");
    list.shown(account, NOW).into_iter().map(|saved| saved.preview.event_id.clone()).collect()
}

fn search(s: &mut State, query: &str) {
    s.modal_field().replace_text(query);
}

#[test]
fn an_unfiltered_list_should_show_every_saved_message_in_order() {
    let s = saved_state();
    assert_eq!(shown(&s), ["$recent", "$week", "$old"]);
}

#[test]
fn opening_saved_messages_should_focus_the_search() {
    let s = saved_state();
    assert!(s.saved_list().expect("open").search.focused);
}

#[test]
fn a_search_should_find_a_message_by_its_text() {
    let mut s = saved_state();
    search(&mut s, "release");
    assert_eq!(shown(&s), ["$week"]);
}

#[test]
fn a_search_should_find_messages_by_their_room() {
    let mut s = saved_state();
    search(&mut s, "general");
    assert_eq!(shown(&s), ["$week", "$old"]);
}

#[test]
fn a_search_should_find_messages_by_their_rooms_name_as_shown() {
    let mut s = saved_state();
    search(&mut s, "#general");
    assert_eq!(shown(&s), ["$week", "$old"]);
}

#[test]
fn a_search_should_find_messages_by_the_direct_chat_they_were_saved_in() {
    let mut s = saved_state();
    search(&mut s, "hopper");
    assert_eq!(shown(&s), ["$recent"]);
}

#[test]
fn a_search_should_find_messages_by_their_sender() {
    let mut s = saved_state();
    search(&mut s, "alan");
    assert_eq!(shown(&s), ["$week", "$old"]);
}

#[test]
fn every_word_of_a_search_should_have_to_match() {
    let mut s = saved_state();
    search(&mut s, "alan welcome");
    assert_eq!(shown(&s), ["$old"]);
}

#[test]
fn a_time_span_should_keep_the_messages_sent_within_it() {
    let mut s = saved_state();
    s.filter_saved_since(SavedSince::Week);
    assert_eq!(shown(&s), ["$recent", "$week"]);
    s.filter_saved_since(SavedSince::Year);
    assert_eq!(shown(&s), ["$recent", "$week", "$old"]);
}

#[test]
fn today_should_keep_a_message_sent_now_and_leave_out_one_from_two_days_ago() {
    assert!(SavedSince::Today.includes(NOW, NOW));
    assert!(!SavedSince::Today.includes(NOW - 2 * DAY, NOW));
}

#[test]
fn today_should_keep_the_messages_of_the_users_calendar_day() {
    use chrono::TimeZone;
    let now = chrono::Local.timestamp_millis_opt(NOW as i64).single().expect("a local time");
    let day = now.date_naive();
    // Whatever the user's zone does to the clock that day, a message sent at any moment that
    // is still the same date counts as today, and one a moment before it does not.
    let first = (0..24 * 60)
        .map(|minutes| NOW - minutes * 60_000)
        .take_while(|&at| chrono::Local.timestamp_millis_opt(at as i64).single().map(|t| t.date_naive()) == Some(day))
        .last()
        .expect("now is today");
    assert!(SavedSince::Today.includes(first, NOW));
    assert!(!SavedSince::Today.includes(first - 60_000, NOW));
}

#[test]
fn a_time_span_should_leave_out_a_message_without_a_send_time() {
    let mut s = saved_state();
    let mut undated = message(GENERAL, "$undated", "Alan", "Unreadable", NOW);
    undated.preview.timestamp_ms = None;
    let mut listed: Vec<_> = s.accounts.get(&ada()).and_then(|a| a.saved.clone()).expect("listed");
    listed.push(undated);
    s.accounts.set_saved_messages(&ada(), listed);
    let data = s.accounts.get(&ada()).expect("account");
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    rooms.iter_mut().find(|room| room.room_id == GENERAL).expect("room").saved.push("$undated".into());
    s.accounts.set_room_list(&ada(), RoomList { rooms, spaces, invitations: Vec::new() });
    assert!(shown(&s).contains(&"$undated".to_owned()));
    s.filter_saved_since(SavedSince::Month);
    assert!(!shown(&s).contains(&"$undated".to_owned()));
}

#[test]
fn the_rooms_filter_should_leave_out_direct_chats() {
    let mut s = saved_state();
    s.filter_saved_in(SavedIn::Rooms);
    assert_eq!(shown(&s), ["$week", "$old"]);
}

#[test]
fn the_direct_chats_filter_should_leave_out_rooms() {
    let mut s = saved_state();
    s.filter_saved_in(SavedIn::Direct);
    assert_eq!(shown(&s), ["$recent"]);
}

#[test]
fn a_room_filter_should_keep_only_that_rooms_messages() {
    let mut s = saved_state();
    s.filter_saved_in(SavedIn::Room(GRACE.into()));
    assert_eq!(shown(&s), ["$recent"]);
}

#[test]
fn filters_and_a_search_should_apply_together() {
    let mut s = saved_state();
    s.filter_saved_in(SavedIn::Room(GENERAL.into()));
    s.filter_saved_since(SavedSince::Month);
    assert_eq!(shown(&s), ["$week"]);
    search(&mut s, "welcome");
    assert!(shown(&s).is_empty());
}

#[test]
fn clearing_the_filters_should_keep_the_search() {
    let mut s = saved_state();
    search(&mut s, "alan");
    s.filter_saved_since(SavedSince::Week);
    s.clear_saved_filters();
    assert!(!s.saved_list().expect("open").filter.active());
    assert_eq!(shown(&s), ["$week", "$old"]);
}

#[test]
fn showing_all_should_clear_the_search_and_the_filters() {
    let mut s = saved_state();
    search(&mut s, "alan");
    s.filter_saved_in(SavedIn::Direct);
    s.show_all_saved();
    assert_eq!(s.saved_list().map(|list| (list.search.text.as_str(), list.filter.active())), Some(("", false)));
    assert_eq!(shown(&s), ["$recent", "$week", "$old"]);
}

#[test]
fn reopening_saved_messages_should_start_unfiltered() {
    let mut s = saved_state();
    search(&mut s, "alan");
    s.filter_saved_since(SavedSince::Today);
    s.close_modal();
    s.open_saved();
    assert_eq!(s.saved_list().map(|list| (list.search.text.as_str(), list.filter.active())), Some(("", false)));
}

#[test]
fn the_filter_menu_should_open_and_close_inside_its_dialog() {
    let mut s = saved_state();
    s.toggle_saved_filters();
    assert!(s.saved_list().is_some_and(|list| list.filters_open));
    s.close_saved_filters();
    assert!(s.saved_list().is_some_and(|list| !list.filters_open));
}

#[test]
fn a_dialog_replacing_saved_messages_should_take_the_filter_menu_with_it() {
    let mut s = saved_state();
    s.toggle_saved_filters();
    s.modal = Some(Modal::Translate("hello".into()));
    s.close_modal();
    assert_eq!(s.popover, None);
    s.open_saved();
    assert!(s.saved_list().is_some_and(|list| !list.filters_open));
}

#[test]
fn the_filter_menu_should_offer_rooms_before_direct_chats() {
    let s = saved_state();
    let account = s.accounts.get(&ada()).expect("account");
    let rooms: Vec<_> = saved_rooms(account).into_iter().map(|(_, label)| label).collect();
    assert_eq!(rooms, ["#general", "Grace Hopper"]);
}
