use matrix_core::{AccountId, EntryKind, Event, Message, MessageContent, TimelineDiff, TimelineEntry, TimelineKey};
use rune::{Env, Runtime};

use crate::{
    demo,
    state::{SettingsSection, State},
};

fn runtime(scene: impl FnOnce(&mut State)) -> Runtime<State> {
    let mut state = State::detached();
    demo::seed(&mut state);
    scene(&mut state);
    let mut rt = Runtime::new(state, super::view, (1200.0, 760.0), Env::new(true, 1.0));
    rt.poll = crate::spec().poll;
    rt.frame(0.0);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn press(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn switching_settings_sections_should_land_the_new_content_at_once() {
    let mut rt = runtime(State::open_settings);
    press(&mut rt, "settings-info");
    let (first, opacity) = rt.presented("app-version").expect("the Info section");
    assert_eq!(opacity, 1.0, "new settings must already be opaque");
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.presented("app-version").unwrap().0, first, "settings content must not morph into place");
    }
}

#[test]
fn switching_settings_sections_should_show_their_final_colours_on_the_first_frame() {
    for source in SettingsSection::ALL {
        for destination in SettingsSection::ALL {
            if source == destination {
                continue;
            }
            let mut rt = runtime(|s| {
                s.open_settings();
                s.show_settings_section(source);
            });
            press(&mut rt, &format!("settings-{}", destination.id()));
            let elements = rt.elements();
            let body = elements.iter().find(|e| e.key.as_deref() == Some("settings-body")).unwrap();
            let prefix = format!("{}/", body.path);
            let first: Vec<_> = elements.into_iter().filter(|e| e.path.starts_with(&prefix)).collect();
            rt.settle_all();
            rt.frame(1.0 / 60.0);
            let settled = rt.elements();
            for element in first {
                let final_element = settled.iter().find(|e| e.path == element.path).unwrap();
                assert_eq!(
                    (element.fill, element.text_color),
                    (final_element.fill, final_element.text_color),
                    "{source:?} -> {destination:?}: colours must not fade at {} ({:?})",
                    element.path,
                    element.text,
                );
            }
        }
    }
}

#[test]
fn switching_accounts_should_remove_the_previous_conversation_at_once() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    press(&mut rt, "account-button");
    press(&mut rt, "switch-@ada:hobby.chat");
    assert!(rt.presented("room-header").is_none(), "the previous account's conversation must not linger");
    assert_eq!(rt.presented("overview-hero").expect("the other account's overview").1, 1.0);
}

#[test]
fn switching_screens_should_remove_the_previous_content_at_once() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    press(&mut rt, "account-button");
    press(&mut rt, "menu-add-account");
    assert!(rt.presented("room-header").is_none(), "the conversation must not linger over sign-in");
}

#[test]
fn switching_chats_should_show_only_the_new_conversation_on_its_first_frame() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    let previous = rt.elements().into_iter().find_map(|e| e.key.filter(|key| key.starts_with("msg-"))).unwrap();
    press(&mut rt, "room-!dm-grace:example.org");
    assert!(rt.presented(&previous).is_none(), "the previous chat's messages must not linger");
    let first = rt.presented("room-name").unwrap();
    assert_eq!(first.1, 1.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.presented("room-name").unwrap(), first, "the new header must not morph into place");
    }
}

#[test]
fn showing_the_overview_should_remove_the_conversation_at_once() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    press(&mut rt, "sidebar-title");
    assert!(rt.presented("room-header").is_none());
    assert_eq!(rt.presented("overview-hero").unwrap().1, 1.0);
}

#[test]
fn switching_scopes_should_show_the_new_overview_at_once() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    press(&mut rt, &format!("scope-{}", demo::WORK_SPACE));
    assert!(rt.presented("room-header").is_none());
    assert_eq!(rt.presented("overview-hero").unwrap().1, 1.0);
}

#[test]
fn a_fading_new_marker_should_stay_between_messages() {
    let mut rt = runtime(unread_timeline);
    rt.update(|s, _| s.focus_changed(true));
    let mut fading_frames = 0;
    for frame in 0..240 {
        rt.frame(1.0 / 60.0);
        if let Some((marker, opacity)) = rt.presented("new-marker")
            && opacity > 0.01
        {
            let before = rt.presented("msg-before").unwrap().0;
            let after = rt.presented("msg-after").unwrap().0;
            assert!(before[1] + before[3] <= marker[1] + 1.0, "frame {frame}: {before:?} overlaps {marker:?}");
            assert!(marker[1] + marker[3] <= after[1] + 1.0, "frame {frame}: {marker:?} overlaps {after:?}");
            fading_frames += usize::from(opacity < 1.0);
        }
    }
    assert!(fading_frames > 0, "the test must exercise the fade");
    assert!(rt.presented("new-marker").is_none(), "the marker must finish leaving");
}

#[test]
fn reduced_motion_should_close_the_new_markers_space_after_the_short_fade() {
    let mut rt = runtime(unread_timeline);
    rt.set_system_reduced_motion(true);
    rt.frame(1.0 / 60.0);
    rt.update(|s, _| s.focus_changed(true));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.presented("new-marker").is_none(), "reduced motion should not reserve the row for 2.5 seconds");
}

#[test]
fn a_receipt_removing_the_new_marker_should_keep_its_fade_between_messages() {
    let mut rt = fading_timeline();
    rt.update(|s, tx| marker_changed(s, tx, vec![TimelineDiff::Remove { index: 1 }]));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
        assert_marker_between_messages(&rt);
        assert!(rt.presented("new-marker").unwrap().1 > 0.01, "the receipt must not cut the fade short");
    }
}

#[test]
fn a_receipt_moving_the_new_marker_should_keep_its_original_boundary() {
    let mut rt = fading_timeline();
    rt.update(|s, tx| {
        marker_changed(
            s,
            tx,
            vec![
                TimelineDiff::Remove { index: 1 },
                TimelineDiff::PushBack(TimelineEntry { id: "new-marker".into(), kind: EntryKind::ReadMarker }),
            ],
        );
    });
    rt.frame(1.0 / 60.0);
    assert_marker_between_messages(&rt);
}

#[test]
fn calling_a_member_in_an_existing_chat_should_land_the_chat_at_once() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_side(crate::state::SidePanel::Members);
        s.toggle_member_menu("@grace:example.org");
    });
    let viewport = rt.presented("timeline").unwrap().0;
    let previous = rt
        .elements()
        .into_iter()
        .find_map(|e| {
            (e.rect[1] + e.rect[3] > viewport[1] && e.rect[1] < viewport[1] + viewport[3])
                .then(|| e.key.filter(|key| key.starts_with("msg-")))
                .flatten()
        })
        .expect("a visible message in the previous chat");
    press(&mut rt, "member-menu-call");
    assert_eq!(rt.state.active_room_id(), Some("!dm-grace:example.org"));
    assert!(rt.presented(&previous).is_none(), "the previous chat's messages must not linger");
}

fn marker_changed(s: &mut State, tx: &mut rune::Tx, diffs: Vec<TimelineDiff>) {
    s.apply(
        Event::TimelineChanged {
            account: AccountId::new(demo::ADA),
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            diffs,
        },
        tx,
    );
}

fn fading_timeline() -> Runtime<State> {
    let mut rt = runtime(unread_timeline);
    rt.update(|s, _| s.focus_changed(true));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn assert_marker_between_messages(rt: &Runtime<State>) {
    let (marker, _) = rt.presented("new-marker").expect("the divider should still be fading");
    let before = rt.presented("msg-before").unwrap().0;
    let after = rt.presented("msg-after").unwrap().0;
    assert!(before[1] + before[3] <= marker[1] + 1.0, "{before:?} overlaps {marker:?}");
    assert!(marker[1] + marker[3] <= after[1] + 1.0, "{marker:?} overlaps {after:?}");
}

fn unread_timeline(s: &mut State) {
    s.select_room(demo::DEMO_THREAD.0);
    s.focus_changed(false);
    let message = |id: &str| TimelineEntry {
        id: id.into(),
        kind: EntryKind::Message(Box::new(Message {
            event_id: format!("${id}"),
            sender_id: "@grace:example.org".into(),
            sender_name: "Grace".into(),
            content: MessageContent::Text { body: "A message beside the boundary".into(), html: None },
            ..Default::default()
        })),
    };
    s.apply(
        Event::TimelineChanged {
            account: AccountId::new(demo::ADA),
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(vec![
                message("before"),
                TimelineEntry { id: "new-marker".into(), kind: EntryKind::ReadMarker },
                message("after"),
            ])],
        },
        &mut rune::Tx::default(),
    );
}

#[test]
fn a_pasted_address_should_be_offered_in_the_search_and_open_its_link_dialog() {
    let mut rt = runtime(|s| {
        s.search.field = rune::TextEdit::new("#rust:matrix.org");
        s.popover = Some(crate::state::Popover::Search);
    });
    press(&mut rt, "found-link");
    assert!(matches!(rt.state.modal, Some(crate::state::Modal::Link(_))));
    assert_eq!(rt.state.search.query(), "");
}

#[test]
fn search_should_list_messages_below_people_and_destinations() {
    let rt = runtime(demo::search);
    let keys: Vec<_> = rt.elements().into_iter().filter_map(|e| e.key).collect();
    let person = keys.iter().position(|key| key == "found-person-@grace:example.org").unwrap();
    let message = keys.iter().position(|key| key.starts_with("found-message-")).unwrap();
    assert!(message > person);
}

#[test]
fn pressing_a_message_hit_should_open_its_room_and_clear_search() {
    let mut rt = runtime(demo::search);
    let hit = rt.state.search.messages[0].clone();
    press(&mut rt, &format!("found-message-{}-{}", hit.room_id, hit.event_id));
    assert_eq!(rt.state.active_room_id(), Some(hit.room_id.as_str()));
    assert!(rt.state.search.query().is_empty());
}

#[test]
fn a_frequently_used_room_should_survive_the_search_result_limit() {
    let rt = runtime(|s| {
        demo::search(s);
        let account = s.accounts.get(&matrix_core::AccountId::new(demo::ADA)).unwrap();
        let template = account.rooms[0].clone();
        let rooms = (0..8)
            .map(|i| matrix_core::RoomSummary {
                room_id: format!("!result-{i}:example.org"),
                name: format!("Group {i}"),
                ..template.clone()
            })
            .collect();
        s.apply(
            matrix_core::Event::RoomsUpdated {
                account: matrix_core::AccountId::new(demo::ADA),
                list: matrix_core::RoomList { rooms, spaces: Vec::new(), invitations: Vec::new() },
            },
            &mut rune::Tx::default(),
        );
        s.apply(
            matrix_core::Event::SearchUsage {
                account: matrix_core::AccountId::new(demo::ADA),
                usage: matrix_core::SearchUsage {
                    rooms: std::collections::BTreeMap::from([("!result-7:example.org".into(), 20)]),
                    ..matrix_core::SearchUsage::default()
                },
            },
            &mut rune::Tx::default(),
        );
    });
    let room_keys: Vec<_> =
        rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("found-room-")).collect();
    assert_eq!(room_keys[0], "found-room-!result-7:example.org");
}

#[test]
fn a_message_hit_should_scroll_an_offscreen_row_into_view() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        let entries = (0..80)
            .map(|i| TimelineEntry {
                id: format!("search-row-{i}"),
                kind: EntryKind::Message(Box::new(matrix_core::Message {
                    event_id: format!("$search-{i}"),
                    sender_id: "@grace:example.org".into(),
                    sender_name: "Grace".into(),
                    content: matrix_core::MessageContent::Text { body: format!("Message {i}"), html: None },
                    timestamp_ms: demo::DEMO_DAY_MS + i * 600_000,
                    ..matrix_core::Message::default()
                })),
            })
            .collect();
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::Reset(entries)],
            },
            &mut rune::Tx::default(),
        );
    });
    rt.update(|s, _| {
        s.open_found_message(&matrix_core::MessageHit {
            room_id: demo::DEMO_THREAD.0.into(),
            event_id: "$search-0".into(),
            thread: None,
            sender: "@grace:example.org".into(),
            timestamp_ms: demo::DEMO_DAY_MS,
            body: "Message 0".into(),
            highlights: vec![0],
        })
    });
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    let viewport = rt.presented("timeline").unwrap().0;
    let row = rt.presented("msg-search-row-0").expect("the matching row should be mounted").0;
    assert!(row[1] + row[3] > viewport[1] && row[1] < viewport[1] + viewport[3]);
}
