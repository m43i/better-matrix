use rune::{Env, Key, KeyEvent, Mods, Runtime};

use super::*;
use crate::{demo, ui};

fn runtime(scene: fn(&mut State)) -> Runtime<State> {
    let mut state = State::detached();
    scene(&mut state);
    let mut rt = Runtime::new(state, ui::view, (1200.0, 760.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::on_key);
    rt.poll = crate::spec().poll;
    rt.frame(0.0);
    rt
}

fn click(rt: &mut Runtime<State>, key: &str) {
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn shortcut(rt: &mut Runtime<State>) {
    rt.key(KeyEvent {
        key: Key::Command('k'),
        mods: Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            ..Default::default()
        },
    });
    rt.frame(0.0);
}

#[test]
fn clicking_an_empty_search_should_show_filters_before_typing() {
    let mut rt = runtime(demo::seed);
    click(&mut rt, "top-search");
    assert!(
        rt.find("search-filters").is_some(),
        "focus: {}, popover: {:?}",
        rt.state.search.field.focused,
        rt.state.popover
    );
    click(&mut rt, "search-filter-spaces");
    assert_eq!(rt.state.search.filter, SearchFilter::Spaces);
    assert!(rt.find("search-active-filter").is_some());
    rt.paste("graphics");
    rt.frame(0.0);
    assert_eq!(rt.state.search.query(), "graphics");
}

#[test]
fn an_unfiltered_search_should_show_each_kind_in_its_section() {
    let rt = runtime(demo::search);
    for key in [
        "found-room-!dm-grace:example.org",
        "found-person-@grace:example.org",
        "found-public-!public-room:example.org",
        "found-public-!public-space:example.org",
    ] {
        assert!(rt.find(key).is_some(), "missing {key}");
    }
    assert!(rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|key| key.starts_with("found-message-"))));
}

#[test]
fn a_direct_chat_should_be_searchable_in_both_name_orders() {
    const ROOM: &str = "!dm-grace:example.org";
    let mut rt = runtime(|s| {
        demo::search(s);
        let id = matrix_core::AccountId::new(demo::ADA);
        let account = s.accounts.get(&id).unwrap();
        let (mut rooms, spaces) = (account.rooms.clone(), account.spaces.clone());
        rooms.iter_mut().find(|room| room.room_id == ROOM).unwrap().name = "Hopper, Grace".into();
        s.accounts.set_room_list(&id, matrix_core::RoomList { rooms, spaces, ..Default::default() });
        s.settings.reverse_display_names = true;
        s.search.field = rune::TextEdit::new("Grace, Hopper");
    });
    let key = format!("found-room-{ROOM}");
    assert!(rt.find(&key).is_some());
    rt.update(|s, _| s.search.field = rune::TextEdit::new("Hopper, Grace"));
    rt.frame(0.0);
    assert!(rt.find(&key).is_some());
}

#[test]
fn a_room_filter_should_hide_other_kinds_and_its_chip_should_clear_it() {
    let mut rt = runtime(|s| {
        demo::search(s);
        s.search.filter = SearchFilter::Rooms;
    });
    assert!(rt.find("found-public-!public-room:example.org").is_some());
    assert!(rt.find("found-public-!public-space:example.org").is_none());
    assert!(rt.find("found-person-@grace:example.org").is_none());
    assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|key| key.starts_with("found-message-"))));
    click(&mut rt, "search-clear-filter");
    assert_eq!(rt.state.search.filter, SearchFilter::All);
    assert!(rt.find("search-active-filter").is_none());
}

#[test]
fn clicking_the_selected_filter_should_restore_all_categories() {
    let mut rt = runtime(demo::search);
    click(&mut rt, "search-filter-people");
    click(&mut rt, "search-filter-people");
    assert_eq!(rt.state.search.filter, SearchFilter::All);
}

#[test]
fn the_filter_row_should_stay_above_scrolling_results() {
    let mut rt = runtime(demo::search);
    let before = rt.presented("search-filters").unwrap().0;
    let panel = rt.presented("search-results").unwrap().0;
    rt.wheel(panel[0] + panel[2] / 2.0, panel[1] + panel[3] - 20.0, 350.0);
    rt.frame(0.0);
    assert_eq!(rt.presented("search-filters").unwrap().0, before);
}

#[test]
fn the_search_shortcut_should_move_focus_from_a_draft_and_leave_it_intact() {
    let mut rt = runtime(|s| {
        demo::seed(s);
        s.select_room(demo::DEMO_THREAD.0);
    });
    rt.paste("An unfinished draft");
    rt.frame(0.0);
    shortcut(&mut rt);
    assert!(rt.state.search.field.focused);
    assert!(rt.find("search-filters").is_some());
    assert_eq!(rt.state.composer.edit.text, "An unfinished draft");
    rt.paste("graphics");
    rt.frame(0.0);
    assert_eq!(rt.state.search.query(), "graphics");
}

#[test]
fn escape_should_dismiss_search_without_immediately_reopening_it() {
    let mut rt = runtime(demo::seed);
    shortcut(&mut rt);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    assert!(rt.find("search-results").is_none());
}

#[test]
fn escape_during_the_search_pause_should_keep_search_closed() {
    let mut rt = runtime(demo::seed);
    shortcut(&mut rt);
    rt.paste("ru");
    rt.frame(0.0);
    let now = std::time::Instant::now();
    rt.update(move |s, _| {
        s.search_people(now);
    });
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    rt.update(move |s, _| {
        s.search_people(now + crate::state::SEARCH_PAUSE);
    });
    rt.frame(0.0);
    assert!(rt.state.popover.is_none());
    assert!(rt.find("search-results").is_none());
}

#[test]
fn clicking_outside_during_the_search_pause_should_keep_search_closed() {
    let mut rt = runtime(demo::seed);
    shortcut(&mut rt);
    rt.paste("ru");
    rt.frame(0.0);
    let now = std::time::Instant::now();
    rt.update(move |s, _| {
        s.search_people(now);
    });
    click(&mut rt, "scrim");
    rt.update(move |s, _| {
        s.search_people(now + crate::state::SEARCH_PAUSE);
    });
    rt.frame(0.0);
    assert!(rt.state.popover.is_none());
    assert!(rt.find("search-results").is_none());
}

#[test]
fn escape_should_close_each_popover_that_has_a_focused_search_field() {
    let openers: [fn(&mut State); 3] = [State::open_search, State::open_new_chat, |s| {
        s.toggle_popover(crate::state::Popover::Emoji(crate::state::EmojiFor::Composer(
            crate::state::ComposerTarget::Room,
        )))
    }];
    for open in openers {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
        });
        rt.update(move |s, _| open(s));
        rt.frame(0.0);
        assert!(rt.state.popover.is_some());
        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        rt.frame(0.0);
        assert!(rt.state.popover.is_none());
    }
}

#[test]
fn a_public_result_should_open_its_join_preview() {
    let mut rt = runtime(demo::search);
    click(&mut rt, "found-public-!public-space:example.org");
    assert!(matches!(&rt.state.modal, Some(crate::state::Modal::Link(dialog)) if
        matches!(&dialog.step, crate::state::LinkStep::Preview(room) if room.space)));
}

/// Types `query` into the top bar's search of the account on screen.
fn searching(rt: &mut Runtime<State>, query: &str) {
    click(rt, "top-search");
    rt.paste(query);
    rt.settle_all();
    rt.frame(0.0);
}

#[test]
fn searching_its_name_should_offer_to_make_the_note_to_self() {
    let mut rt = runtime(|s| {
        demo::seed(s);
        s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
    });
    searching(&mut rt, "note to");
    click(&mut rt, NOTE_TO_SELF);
    // The search closes for the chat, which opens once the engine has made it.
    assert!(rt.state.popover.is_none());
}

#[test]
fn searching_an_existing_note_to_self_should_find_it_rather_than_offer_another() {
    let mut rt = runtime(demo::seed);
    searching(&mut rt, "note to");
    assert!(rt.find("found-room-!notes:example.org").is_some());
    assert!(rt.find(NOTE_TO_SELF).is_none());
}
