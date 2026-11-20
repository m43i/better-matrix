use matrix_core::{AccountId, EntryKind, Event, Message, MessageContent, TimelineDiff, TimelineEntry, TimelineKey};
use rune::{Env, Runtime};

use crate::{
    demo,
    state::{SettingsSection, SidePanel, State},
};

pub(super) fn runtime(scene: impl FnOnce(&mut State)) -> Runtime<State> {
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

pub(super) fn press(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn space_creation_should_follow_the_last_space_in_the_rail() {
    let rt = runtime(|_| {});
    let add = rt.presented("rail-create-space").expect("the new-space button").0;
    let account = rt.state.active_account().unwrap();
    let last = crate::state::ordered_spaces(&account.spaces, &account.sidebar).pop().unwrap();
    let space = rt.presented(&format!("scope-{}", last.room_id)).unwrap().0;
    assert!(add[1] >= space[1] + space[3], "the plus must follow the last space");
}

#[test]
fn space_creation_should_be_available_before_joining_any_spaces() {
    let rt = runtime(|s| {
        let account = s.nav.account.clone().unwrap();
        s.accounts.set_room_list(&account, matrix_core::RoomList::default());
    });
    assert!(rt.find("rail-create-space").is_some());
}

#[test]
fn space_creation_should_stay_reachable_with_many_spaces() {
    let mut rt = runtime(demo::many_spaces);
    let avatar = rt.presented("account-button").unwrap().0;
    rt.wheel(34.0, 300.0, 2000.0);
    rt.frame(1.0 / 60.0);
    let add = rt.presented("rail-create-space").unwrap().0;
    let first = rt.presented("scope-!space-0:example.org").unwrap().0;
    assert!(rt.presented("rail-space-blur").unwrap().1 > 0.99, "spaces must scroll behind the blur");
    assert!(rt.instances.iter().any(|instance| instance.params[1] == rune::render::KIND_BACKDROP_BLUR));
    rt.wheel(34.0, 300.0, -2000.0);
    rt.frame(1.0 / 60.0);
    assert!(rt.presented("scope-!space-0:example.org").unwrap().0[1] < first[1], "spaces must scroll");
    assert_eq!(rt.presented("rail-create-space").unwrap().0, add, "the plus must stay fixed");
    assert!(add[1] >= 0.0 && add[1] + add[3] <= avatar[1], "the plus must fit above the avatar");
    assert!((avatar[1] - add[1] - add[3] - 16.0).abs() < 0.5, "the actions need a clear gap");
    assert_eq!(rt.presented("account-button").unwrap().0, avatar, "the avatar must stay fixed");
    press(&mut rt, "rail-create-space");
    assert!(matches!(
        rt.state.modal,
        Some(crate::state::Modal::RoomForm { form: crate::state::RoomForm::NewSpace, .. })
    ));
}

#[test]
fn space_creation_should_require_an_active_account() {
    let rt = runtime(|s| s.nav.account = None);
    assert!(rt.find("rail-create-space").is_none());
}

#[test]
fn space_creation_should_open_the_same_form_from_home_and_a_space() {
    for scope in [crate::state::Scope::Home, crate::state::Scope::Space(demo::WORK_SPACE.into())] {
        let mut rt = runtime(|s| s.select_scope(scope));
        press(&mut rt, "rail-create-space");
        assert!(matches!(
            rt.state.modal,
            Some(crate::state::Modal::RoomForm { form: crate::state::RoomForm::NewSpace, .. })
        ));
    }
}

#[test]
fn space_creation_should_not_require_managing_an_existing_space() {
    let mut rt = runtime(|s| {
        let account = s.active_account().unwrap();
        let id = account.summary.id.clone();
        let mut spaces = account.spaces.clone();
        for space in &mut spaces {
            space.can_add_rooms = false;
            space.can_rename = false;
        }
        s.accounts.set_room_list(&id, matrix_core::RoomList { spaces, ..Default::default() });
        s.select_scope(crate::state::Scope::Space(demo::WORK_SPACE.into()));
    });
    press(&mut rt, "rail-create-space");
    assert!(matches!(
        rt.state.modal,
        Some(crate::state::Modal::RoomForm { form: crate::state::RoomForm::NewSpace, .. })
    ));
}

#[test]
fn space_creation_should_be_absent_from_the_sidebar_create_menu() {
    let mut rt = runtime(|s| s.select_scope(crate::state::Scope::Home));
    press(&mut rt, "sidebar-create");
    assert!(rt.find("create-menu").is_some());
    assert!(rt.find("create-space").is_none());
    assert!(rt.find("create-room").is_some());
}

#[test]
fn adding_an_account_should_show_a_modal_over_the_current_conversation() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    press(&mut rt, "account-button");
    press(&mut rt, "menu-add-account");

    assert!(rt.find("sign-in-modal").is_some() && rt.find("modal-backdrop").is_some());
    assert!(rt.find("room").is_some() && rt.find("backdrop").is_some());
    assert_eq!(rt.state.active_room_id(), Some(demo::DEMO_THREAD.0));
}

#[test]
fn the_sign_in_modal_should_fill_ninety_percent_of_the_window() {
    let rt = runtime(State::start_adding_account);
    let (rect, _) = rt.presented("sign-in-modal").expect("the sign-in modal");

    assert_eq!(rect, [60.0, 38.0, 1080.0, 684.0]);
}

#[test]
fn closing_the_add_account_modal_should_return_to_the_current_conversation() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.start_adding_account();
    });
    press(&mut rt, "cancel");

    assert!(rt.state.modal.is_none());
    assert_eq!(rt.state.active_room_id(), Some(demo::DEMO_THREAD.0));
}

#[test]
fn pressing_the_sign_in_backdrop_should_close_the_modal() {
    let mut rt = runtime(State::start_adding_account);
    rt.pointer_down(20.0, 20.0);
    rt.pointer_up(20.0, 20.0);
    rt.frame(1.0 / 60.0);

    assert!(rt.state.modal.is_none());
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
    rt.update(|s, tx| {
        let accounts: Vec<_> = s.accounts.iter().map(|a| a.summary.id.clone()).collect();
        for id in accounts {
            s.apply(Event::AccountRemoved(id), tx);
        }
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.presented("room-header").is_none(), "the conversation must not linger over sign-in");
    assert!(rt.presented("sign-in-card").is_some());
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

#[test]
fn right_clicking_message_text_should_open_the_full_message_menu() {
    let mut rt = runtime(|s| {
        demo::pinned_messages(s);
    });
    rt.update(|_, tx| tx.scroll_into_view_in("timeline", format!("text-{}", demo::DEMO_THREAD.1)));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find(&format!("text-{}", demo::DEMO_THREAD.1)).expect("message text");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.1);
    assert!(
        matches!(rt.state.popover, Some(crate::state::Popover::MessageMenu { .. })),
        "secondary press at {x}, {y}: {:?}",
        rt.state.popover
    );
    assert!(rt.find("message-copy-link").is_some());
    assert!(rt.find("message-forward").is_some());
    assert!(rt.find("message-pin").is_some());
    assert!(rt.find("message-edit").is_none());
}

#[test]
fn a_message_menu_should_keep_other_rows_from_hovering_underneath_it() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        let entries = (0..4)
            .map(|i| TimelineEntry {
                id: format!("menu-row-{i}"),
                kind: EntryKind::Message(Box::new(Message {
                    event_id: format!("$menu-{i}"),
                    sender_id: "@grace:example.org".into(),
                    sender_name: "Grace".into(),
                    timestamp_ms: demo::DEMO_DAY_MS + i * 600_000,
                    content: MessageContent::Text {
                        body: "A message\nwith several lines\nto hover over".into(),
                        html: None,
                    },
                    ..Message::default()
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
    let (x, y) = rt.find("text-$menu-2").unwrap();
    rt.update(|s, _| {
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: "$menu-0".into(),
            at: Some((x as i32 - 50, y as i32 - 50)),
        })
    });
    rt.frame(0.1);
    rt.pointer_move(x, y);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(rt.presented("actions-$menu-2").unwrap().1, 0.0);
}

#[test]
fn an_image_message_should_offer_file_actions_without_copying_its_filename() {
    let mut rt = runtime(demo::attachment_message);
    let (x, y) = rt.find("image-better-matrix.png").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.1);
    for key in ["attachment-copy-image", "attachment-open", "attachment-download", "message-forward"] {
        assert!(rt.find(key).is_some(), "missing {key}");
    }
    assert!(rt.find("message-copy").is_none());
    assert!(rt.find("message-translate").is_none());
    assert!(rt.state.preview.is_none());
    assert!(rt.state.active_thread().is_none());
}

#[test]
fn a_file_message_should_offer_download_and_open_without_copy_image() {
    let mut rt = runtime(|s| {
        demo::attachment_message(s);
        let key = TimelineKey::room(demo::DEMO_THREAD.0);
        let mut message = s.active_account().unwrap().timeline(&key).unwrap().entries[0].message().unwrap().clone();
        let MessageContent::Attachment(file) = &mut message.content else { panic!("attachment") };
        file.kind = matrix_core::AttachmentKind::File;
        file.name = "notes.pdf".into();
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key,
                diffs: vec![TimelineDiff::Reset(vec![TimelineEntry {
                    id: "file".into(),
                    kind: EntryKind::Message(Box::new(message)),
                }])],
            },
            &mut rune::Tx::default(),
        );
    });
    let (x, y) = rt.find("file-notes.pdf").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.1);
    assert!(rt.find("attachment-download").is_some());
    assert!(rt.find("attachment-open").is_some());
    assert!(rt.find("attachment-copy-image").is_none());
    assert!(rt.find("message-copy").is_none());
}

#[test]
fn right_clicking_a_preview_should_offer_file_actions_above_the_viewer() {
    let mut rt = runtime(|s| {
        demo::attachment_preview(s);
        s.close_popover();
    });
    let (x, y) = rt.find("preview-image").unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.1);
    assert!(rt.find("attachment-copy-image").is_some());
    press(&mut rt, "attachment-copy-image");
    assert!(rt.state.popover.is_none());
    assert!(rt.state.preview.is_some());
}

#[test]
fn dismissing_a_preview_menu_should_keep_the_preview_open() {
    let mut rt = runtime(demo::attachment_preview);
    rt.pointer_down(30.0, 500.0);
    rt.pointer_up(30.0, 500.0);
    rt.frame(0.1);
    assert!(rt.state.popover.is_none());
    assert!(rt.state.preview.is_some());
}

#[test]
fn escape_should_close_the_preview_menu_before_the_preview() {
    let mut rt = runtime(demo::attachment_preview);
    rt.on_key = crate::spec().on_key;
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    rt.frame(0.1);
    assert!(rt.state.popover.is_none());
    assert!(rt.state.preview.is_some());
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    rt.frame(0.1);
    assert!(rt.state.preview.is_none());
}

#[test]
fn a_document_preview_should_offer_open_and_download_without_copy_image() {
    let mut rt = runtime(|s| {
        demo::attachment_preview(s);
        let preview = s.preview.as_mut().unwrap();
        preview.attachment.kind = matrix_core::AttachmentKind::File;
        preview.attachment.name = "notes.pdf".into();
        preview.content =
            crate::state::PreviewContent::Document { pages: vec![], status: crate::state::DocumentStatus::Done };
        s.close_popover();
    });
    rt.secondary_down(600.0, 300.0);
    rt.secondary_up(600.0, 300.0);
    rt.frame(0.1);
    assert!(rt.find("attachment-open").is_some());
    assert!(rt.find("attachment-download").is_some());
    assert!(rt.find("attachment-copy-image").is_none());
}

#[test]
fn an_executable_attachment_menu_should_offer_download_without_open() {
    let file = matrix_core::Attachment {
        kind: matrix_core::AttachmentKind::File,
        name: "invoice.pdf.exe".into(),
        media: Some(matrix_core::MediaRef::opaque("executable")),
        thumbnail: None,
        dimensions: None,
        audio: None,
        size: None,
    };
    let mut rt = Runtime::new(
        file,
        |file, _| {
            rune::col().w_full().h_full().children(super::attachments::actions(
                &AccountId::new(demo::ADA),
                file,
                &super::theme::palette(true),
            ))
        },
        (400.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);
    assert!(rt.find("attachment-download").is_some());
    assert!(rt.find("attachment-open").is_none());
}

#[test]
fn room_info_should_open_the_shared_pinned_messages() {
    let mut rt = runtime(|s| {
        demo::pinned_messages(s);
        s.show_side(SidePanel::Info);
    });
    press(&mut rt, "info-pins");
    assert!(rt.find("pins-panel").is_some());
    assert!(rt.find(&format!("pinned-open-{}", demo::DEMO_THREAD.1)).is_some());
}

#[test]
fn opening_a_pin_should_jump_to_its_message_in_the_conversation() {
    let mut rt = runtime(|s| {
        demo::pinned_messages(s);
        s.show_side(SidePanel::Pins);
    });
    press(&mut rt, &format!("pinned-open-{}", demo::DEMO_THREAD.1));
    assert_eq!(rt.state.active_room_id(), Some(demo::DEMO_THREAD.0));
    assert_eq!(rt.state.active_thread(), None);
    assert!(rt.state.side.is_none());
}

#[test]
fn right_clicking_formatted_message_text_should_open_message_actions() {
    let mut rt = runtime(|s| {
        demo::seed(s);
        s.select_room(demo::DEMO_THREAD.0);
    });
    let id = "$demo-@alan:example.org-2";
    rt.update(|_, tx| tx.scroll_into_view_in("timeline", format!("rich-{id}")));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find(&format!("rich-{id}-0")).expect("formatted paragraph");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.1);
    assert!(rt.find("message-copy-link").is_some());
}

#[test]
fn the_unpin_icon_should_not_open_the_message() {
    let mut rt = runtime(|s| {
        demo::pinned_messages(s);
        s.show_side(SidePanel::Pins);
    });
    press(&mut rt, &format!("pinned-unpin-{}", demo::DEMO_THREAD.1));
    assert_eq!(rt.state.side, Some(SidePanel::Pins));
    assert!(rt.state.message_jump.is_none());
}

#[test]
fn forwarding_should_separate_rooms_and_direct_messages() {
    let rt = runtime(|s| {
        s.start_forward(TimelineKey::room(demo::DEMO_THREAD.0), demo::DEMO_THREAD.1.into());
    });
    let rooms = rt.find("forward-rooms").expect("rooms section");
    let direct = rt.find("forward-direct").expect("direct messages section");
    assert!(rooms.1 < direct.1);
}

fn readers_runtime() -> Runtime<State> {
    runtime(demo::read_receipts)
}

fn open_readers(rt: &mut Runtime<State>) {
    press(rt, &format!("readers-{}-$read-demo", demo::DEMO_THREAD.0));
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_read_indicator_should_open_readers_without_opening_a_thread() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    assert!(rt.find("readers-panel").is_some());
    assert!(rt.state.active_thread().is_none());
    let panel = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("readers-panel")).unwrap();
    assert!(panel.rect[3] <= 380.0);
    assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some("Seen by 6 people")));
}

#[test]
fn selecting_a_reader_should_open_the_existing_member_actions() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    press(&mut rt, "reader-@kai:example.org");
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("member-menu-chat").is_some());
    assert!(rt.find("member-menu-kick").is_some());
    press(&mut rt, "member-menu-role");
    assert!(rt.find("member-role-menu").is_some());
}

#[test]
fn a_reader_outside_the_member_list_should_keep_chat_actions_without_moderation() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    press(&mut rt, "reader-@mira:example.org");
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("member-menu-chat").is_some());
    assert!(rt.find("member-menu-kick").is_none());
    assert!(rt.find("member-menu-role").is_none());
}

fn clear_demo_readers(rt: &mut Runtime<State>) {
    rt.update(|s, tx| {
        let key = TimelineKey::room(demo::DEMO_THREAD.0);
        let account = s.nav.account.clone().unwrap();
        let timeline = s.active_account().unwrap().timeline(&key).unwrap();
        let index = timeline
            .entries
            .iter()
            .position(|entry| entry.message().is_some_and(|m| m.event_id == "$read-demo"))
            .unwrap();
        let entry = &timeline.entries[index];
        let mut message = entry.message().unwrap().clone();
        message.read_receipts.clear();
        let entry = TimelineEntry { id: entry.id.clone(), kind: EntryKind::Message(Box::new(message)) };
        s.apply(Event::TimelineChanged { account, key, diffs: vec![TimelineDiff::Set { index, entry }] }, tx);
    });
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

#[test]
fn a_read_indicator_should_not_make_its_message_taller() {
    let mut rt = readers_runtime();
    let height = |rt: &Runtime<State>| rt.presented("msg-read-demo").expect("the message row").0[3];
    let with_readers = height(&rt);
    clear_demo_readers(&mut rt);
    assert_eq!(height(&rt), with_readers);
}

fn plain(body: &str) -> MessageContent {
    MessageContent::Text { body: body.to_owned(), html: None }
}

/// Sets a message of `content` from the demo message's sender after it, so its row continues the
/// sender's run (no name line), read by `readers` people; gives back the row's height.
fn set_continued_message(rt: &mut Runtime<State>, content: MessageContent, readers: usize) -> f32 {
    rt.update(|s, tx| {
        let account = s.nav.account.clone().unwrap();
        let key = TimelineKey::room(demo::DEMO_THREAD.0);
        let entries = &s.active_account().unwrap().timeline(&key).unwrap().entries;
        let existing = entries.iter().position(|entry| entry.id == "continued");
        let read_receipts = (0..readers)
            .map(|i| matrix_core::ReadReceipt { user_id: format!("@reader-{i}:example.org"), timestamp_ms: None })
            .collect();
        let message = Message {
            event_id: "$continued".into(),
            sender_id: demo::ADA.into(),
            sender_name: "Ada Lovelace".into(),
            timestamp_ms: demo::DEMO_DAY_MS + 13 * 3_600_000 + 60_000,
            content,
            read_receipts,
            ..Message::default()
        };
        let entry = TimelineEntry { id: "continued".into(), kind: EntryKind::Message(Box::new(message)) };
        let diff = match existing {
            Some(index) => TimelineDiff::Set { index, entry },
            None => TimelineDiff::PushBack(entry),
        };
        s.apply(Event::TimelineChanged { account, key, diffs: vec![diff] }, tx);
    });
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    rt.presented("msg-continued").expect("the continued message's row").0[3]
}

#[test]
fn a_read_indicator_should_not_rewrap_a_long_message() {
    let mut rt = readers_runtime();
    let body = "Ready to try. ".repeat(40);
    let width = |rt: &Runtime<State>| rt.presented("text-$continued").expect("the message's text").0[2];
    let with_reader = set_continued_message(&mut rt, plain(&body), 1);
    let text_with_reader = width(&rt);
    assert!(rt.find(&format!("readers-{}-$continued", demo::DEMO_THREAD.0)).is_some());
    assert_eq!(set_continued_message(&mut rt, plain(&body), 0), with_reader);
    assert_eq!(width(&rt), text_with_reader, "the text must keep its width");
}

#[test]
fn a_read_indicator_should_not_make_content_shorter_than_a_line_taller() {
    let mut rt = readers_runtime();
    let tiny = || {
        MessageContent::Attachment(matrix_core::Attachment {
            kind: matrix_core::AttachmentKind::Image,
            name: "dot.png".into(),
            size: None,
            media: Some(matrix_core::MediaRef::opaque("dot.png")),
            thumbnail: None,
            dimensions: Some((8, 8)),
            audio: None,
        })
    };
    let unread = set_continued_message(&mut rt, tiny(), 0);
    assert_eq!(set_continued_message(&mut rt, tiny(), 1), unread);
}

#[test]
fn a_read_indicator_on_a_one_line_message_should_open_readers_not_its_actions() {
    let mut rt = readers_runtime();
    set_continued_message(&mut rt, plain("Ready."), 1);
    let (x, y) = rt.find(&format!("readers-{}-$continued", demo::DEMO_THREAD.0)).unwrap();
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    press(&mut rt, &format!("readers-{}-$continued", demo::DEMO_THREAD.0));
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("readers-panel").is_some());
    assert!(rt.find("message-menu").is_none());
}

#[test]
fn a_large_reader_count_should_stay_inside_the_readers_column() {
    let mut rt = readers_runtime();
    let indicator = format!("readers-{}-$continued", demo::DEMO_THREAD.0);
    for (viewport, faces) in
        [((1200.0, 760.0), crate::state::READER_FACES), ((760.0, 760.0), super::readers::NARROW_FACES)]
    {
        rt.set_viewport(viewport, 1.0);
        rt.frame(1.0 / 60.0);
        set_continued_message(&mut rt, plain("Ready."), 150);
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some("99+")));
        let width = rt.presented(&indicator).expect("the read indicator").0[2];
        assert!(width <= super::readers::column_width(faces), "{width} must fit {faces} faces' column");
    }
}

/// Where the readers' column starts in the room's timeline, in a window of its narrowest size.
fn readers_column_start(rt: &mut Runtime<State>) -> f32 {
    readers_column_start_at(rt, crate::spec().min_size.unwrap())
}

/// Where the readers' column starts in the room's (narrow) timeline, in a window of `size`.
fn readers_column_start_at(rt: &mut Runtime<State>, size: (f32, f32)) -> f32 {
    rt.set_viewport(size, 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let frame = rt.presented("timeline-frame").expect("the timeline").0;
    frame[0] + frame[2] - 20.0 - super::readers::column_width(super::readers::NARROW_FACES)
}

#[test]
fn an_image_should_leave_the_readers_column_clear_in_the_narrowest_window() {
    let mut rt = runtime(demo::attachment_message);
    let column = readers_column_start(&mut rt);
    let image = rt.presented("image-better-matrix.png").expect("the image").0;
    assert!(image[0] + image[2] <= column, "the image ends at {}, the column starts at {column}", image[0] + image[2]);
}

#[test]
fn an_image_without_its_dimensions_should_take_the_shape_of_its_picture() {
    let file = matrix_core::Attachment { dimensions: None, ..demo::demo_attachment() };
    let mut rt = runtime(move |s| demo::image_message(s, file));
    rt.set_viewport((1280.0, 800.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let image = rt.presented("image-better-matrix.png").expect("the image").0;
    assert_eq!((image[2], image[3]), (256.0, 256.0), "the square picture shows in a square");
}

#[test]
fn a_linked_gif_should_leave_the_readers_column_clear_in_the_narrowest_window() {
    let mut rt = readers_runtime();
    set_continued_message(&mut rt, plain("https://x.org/a.gif"), 0);
    let column = readers_column_start(&mut rt);
    let gif = rt.presented("gif-https://x.org/a.gif").expect("the GIF").0;
    assert!(gif[0] + gif[2] <= column, "the GIF ends at {}, the column starts at {column}", gif[0] + gif[2]);
}

#[test]
fn a_file_card_should_leave_the_readers_column_clear_in_the_narrowest_window() {
    let mut rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    let column = readers_column_start(&mut rt);
    let names = rt.elements().into_iter().filter(|e| e.text.as_deref() == Some("release-notes-0.2.pdf"));
    for e in names.chain(rt.elements().into_iter().filter(|e| e.key.as_deref() == Some("file-release-notes-0.2.pdf"))) {
        assert!(
            e.rect[0] + e.rect[2] <= column,
            "{:?} ends at {}, the column starts at {column}",
            e.key,
            e.rect[0] + e.rect[2]
        );
    }
}

#[test]
fn a_long_link_should_wrap_clear_of_the_readers_column() {
    let mut rt = readers_runtime();
    let column = readers_column_start_at(&mut rt, (760.0, 1200.0));
    let link = rt
        .elements()
        .into_iter()
        .find(|e| e.text.as_deref().is_some_and(|t| t.contains("bettermatrix.example.org/blog")))
        .expect("the message with the link");
    assert!(
        link.rect[0] + link.rect[2] <= column,
        "the link ends at {}, the column starts at {column}",
        link.rect[0] + link.rect[2]
    );
}

#[test]
fn long_names_words_and_reactions_should_stay_clear_of_the_readers_column_in_the_narrowest_window() {
    const LONG: &str = "3f1c9a7e5b2d4f6a8c0e1b3d5f7a9c2e4b6d8f0a";
    let mut rt = readers_runtime();
    rt.update(|s, tx| {
        let account = s.nav.account.clone().unwrap();
        let read = vec![matrix_core::ReadReceipt { user_id: "@kai:example.org".into(), timestamp_ms: None }];
        let message = |id: &str, content: MessageContent| {
            let message = Message {
                event_id: format!("${id}"),
                sender_id: "@alexandra:example.org".into(),
                sender_name: "Alexandra Montgomery-Wellington".into(),
                timestamp_ms: demo::DEMO_DAY_MS + 13 * 3_600_000 + 120_000,
                content,
                read_receipts: read.clone(),
                ..Message::default()
            };
            TimelineEntry { id: id.into(), kind: EntryKind::Message(Box::new(message)) }
        };
        let mut text = message("long", plain(&format!("Fixed in {LONG}")));
        if let EntryKind::Message(message) = &mut text.kind {
            message.reactions =
                vec![matrix_core::Reaction { key: ":party_parrot_celebrate_forever:".into(), count: 12, mine: false }];
            message.reply_to = Some(matrix_core::Quote {
                event_id: "$read-demo".into(),
                sender_name: Some("Bartholomew Fitzgerald-Kensington".into()),
                body: Some(LONG.into()),
                call: None,
            });
        }
        let answer = |id: &str| matrix_core::PollAnswer { id: id.into(), text: LONG.into(), votes: 0, mine: false };
        let poll = matrix_core::Poll {
            question: format!("Which {LONG}?"),
            answers: vec![answer("a"), answer("b")],
            max_selections: 1,
            ..matrix_core::Poll::default()
        };
        let poll = message("long-poll", MessageContent::Poll(poll));
        let key = TimelineKey::room(demo::DEMO_THREAD.0);
        s.apply(
            Event::TimelineChanged {
                account,
                key,
                diffs: vec![TimelineDiff::PushBack(text), TimelineDiff::PushBack(poll)],
            },
            tx,
        );
    });
    let column = readers_column_start(&mut rt);
    let frame = rt.presented("timeline-frame").expect("the timeline").0;
    for key in ["msg-long", "msg-long-poll"] {
        let row = rt.presented(key).expect("the message").0;
        let inside = rt.elements().into_iter().filter(|e| {
            e.rect[1] >= row[1].max(frame[1])
                && e.rect[1] + e.rect[3] <= row[1] + row[3]
                && e.rect[0] < column
                && e.key.as_deref() != Some(key)
        });
        for e in inside {
            assert!(
                e.rect[0] + e.rect[2] <= column,
                "{:?} ends at {}, the column starts at {column}",
                e.text,
                e.rect[0] + e.rect[2]
            );
        }
    }
}

#[test]
fn a_timeline_too_narrow_for_the_readers_column_should_show_indicators_under_their_messages() {
    let mut rt = readers_runtime();
    rt.update(|s, _| s.open_thread(demo::DEMO_THREAD.1));
    // 780 points at 125% zoom, beside a thread.
    rt.set_viewport((624.0, 1280.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let text = rt.presented("text-$read-demo").expect("the message's text").0;
    let indicator = rt.presented(&format!("readers-{}-$read-demo", demo::DEMO_THREAD.0)).expect("the indicator").0;
    assert!(indicator[1] >= text[1] + text[3], "the indicator takes a line under the message");
    let answer = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Friday")).expect("a poll answer");
    assert!(answer.rect[3] < 30.0, "a poll answer keeps to a line: {}", answer.rect[3]);
    let site = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Better Matrix")).expect("the site's name");
    assert!(site.rect[2] >= 40.0, "the link card's words keep room: {}", site.rect[2]);
}

#[test]
fn a_call_card_at_the_narrowest_column_width_should_keep_its_words_readable() {
    let mut rt = runtime(|s| s.select_room("!dm-kai:example.org"));
    // Just wide enough to keep the readers' column.
    let column = readers_column_start_at(&mut rt, (694.0, 900.0));
    let frame = rt.presented("timeline-frame").expect("the timeline").0;
    let content = column - 10.0 - (frame[0] + 20.0 + 36.0 + 10.0);
    assert!(content >= super::readers::MIN_CONTENT, "the column stays at {content}");
    let join = rt.elements().into_iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("call-card-join-")));
    let join = join.expect("the call card's join button").rect;
    assert!(join[0] + join[2] <= column, "join ends at {}, the column starts at {column}", join[0] + join[2]);
    let title = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Voice call")).expect("the call's title");
    assert!(title.rect[2] >= 60.0, "the call's title keeps room: {}", title.rect[2]);
}

#[test]
fn a_call_card_in_a_cramped_timeline_should_keep_its_words_beside_its_picture_and_inside_it() {
    let mut rt = runtime(|s| s.select_room("!dm-kai:example.org"));
    // 720 points at 150% zoom.
    rt.set_viewport((480.0, 900.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let card = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("call-card-") && !k.starts_with("call-card-join-")));
    let card = card.expect("the call card").rect;
    let title = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Voice call")).expect("the title").rect;
    let join = rt.elements().into_iter().find(|e| e.key.as_deref().is_some_and(|k| k.starts_with("call-card-join-")));
    let join = join.expect("the join button").rect;
    for (what, rect) in [("title", title), ("join", join)] {
        assert!(
            rect[0] + rect[2] <= card[0] + card[2],
            "the {what} ends at {}, the card at {}",
            rect[0] + rect[2],
            card[0] + card[2]
        );
    }
    assert!(title[1] < card[1] + 12.0 + 40.0, "the title sits beside the picture, at {}", title[1]);
}

#[test]
fn a_voice_memo_in_a_cramped_timeline_should_keep_its_controls_inside_it() {
    let memo = "$demo-@grace:example.org-3";
    let mut rt = runtime(|s| {
        s.select_room("!dm-grace:example.org");
        let media = matrix_core::MediaRef::opaque(crate::demo::DEMO_VOICE_MEMO);
        s.audio = Some(crate::state::AudioClip {
            id: 0,
            media,
            player: None,
            start_at: None,
            play: true,
            held: false,
            scrub_from: None,
        });
        // The widest speed label.
        s.settings.playback_speed = crate::state::PlaybackSpeed::Faster;
    });
    // The smallest window, 720 by 480 points, at 150% and 175% zoom; and a compact card.
    for size in [(480.0, 320.0), (411.0, 274.0), (454.0, 390.0)] {
        rt.set_viewport(size, 1.0);
        rt.frame(1.0 / 60.0);
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        let timeline = rt.presented("timeline").expect("the timeline").0;
        let card = rt.presented(&format!("audio-{memo}")).unwrap_or_else(|| panic!("no card")).0;
        assert!(card[0] + card[2] <= timeline[0] + timeline[2], "at {size:?}, the card ends past the timeline");
        for key in [format!("audio-wave-{memo}"), format!("audio-speed-{memo}")] {
            let rect = rt.presented(&key).unwrap_or_else(|| panic!("no {key}")).0;
            assert!(
                rect[0] + rect[2] <= card[0] + card[2],
                "at {size:?}, {key} ends at {}, the card at {}",
                rect[0] + rect[2],
                card[0] + card[2]
            );
        }
    }
}

#[test]
fn a_thread_link_should_leave_the_readers_column_clear_in_a_narrow_window() {
    let mut rt = readers_runtime();
    rt.set_viewport((760.0, 1200.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let frame = rt.presented("timeline-frame").expect("the timeline").0;
    let column = frame[0] + frame[2] - 20.0 - super::readers::column_width(super::readers::NARROW_FACES);
    let link = format!("thread-link-{}", demo::DEMO_THREAD.1);
    let link = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(link.as_str())).expect("the thread link");
    let texts = rt.elements().into_iter().filter(|e| {
        e.text.is_some() && e.rect[1] >= link.rect[1] && e.rect[1] + e.rect[3] <= link.rect[1] + link.rect[3]
    });
    for e in texts {
        assert!(
            e.rect[0] + e.rect[2] <= column,
            "{:?} ends at {}, the column starts at {column}",
            e.text,
            e.rect[0] + e.rect[2]
        );
    }
}

#[test]
fn a_narrow_timeline_should_show_fewer_reader_pictures() {
    let overflow = |rt: &Runtime<State>| rt.elements().into_iter().any(|e| e.text.as_deref() == Some("+4"));
    let mut rt = readers_runtime();
    assert!(!overflow(&rt), "a wide timeline shows four of the six readers");
    rt.set_viewport((760.0, 760.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
    assert!(overflow(&rt), "a narrow timeline shows two of the six readers");
}

#[test]
fn moving_all_readers_away_should_remove_an_open_read_indicator() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    clear_demo_readers(&mut rt);
    assert!(rt.find("readers-panel").is_none());
    assert!(rt.find(&format!("readers-{}-$read-demo", demo::DEMO_THREAD.0)).is_none());
}

#[test]
fn messages_without_exposed_receipts_should_have_no_read_indicator() {
    let rt = runtime(|_| {});
    assert!(rt.elements().iter().all(|e| !e.key.as_deref().is_some_and(|key| key.starts_with("readers-"))));
}

#[test]
fn reader_lists_should_not_invent_a_timestamp_for_a_receipt_without_one() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    assert!(rt.find("reader-@sam:example.org").is_some());
    assert!(rt.find("reader-time-@sam:example.org").is_none());
}

#[test]
fn a_large_reader_list_should_scroll_to_every_reader() {
    let mut rt = runtime(|s| {
        demo::read_receipts(s);
        let key = TimelineKey::room(demo::DEMO_THREAD.0);
        let timeline = s.accounts.timeline_mut(&matrix_core::AccountId::new(demo::ADA), &key).unwrap();
        let crate::state::EntryView::Message(item) = &mut timeline.entries.last_mut().unwrap().kind else {
            panic!("last message")
        };
        item.message.read_receipts = (0..100)
            .map(|i| matrix_core::ReadReceipt { user_id: format!("@reader-{i}:example.org"), timestamp_ms: None })
            .collect();
    });
    open_readers(&mut rt);
    let panel = rt.presented("readers-panel").unwrap().0;
    assert!(panel[3] <= 380.0);
    rt.wheel(panel[0] + 100.0, panel[1] + 100.0, -6000.0);
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("reader-@reader-99:example.org").is_some());
}

#[test]
fn reader_actions_should_close_when_the_open_room_changes() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    press(&mut rt, "reader-@kai:example.org");
    rt.update(|s, _| s.select_room("!design:example.org"));
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("readers-panel").is_none());
    assert!(rt.find("member-menu-kick").is_none());
}

#[test]
fn a_reader_timestamp_should_include_its_localized_day() {
    let mut rt = readers_runtime();
    open_readers(&mut rt);
    let at = rt.state.active_account().unwrap().readers(&TimelineKey::room(demo::DEMO_THREAD.0), "$read-demo")[0]
        .timestamp_ms
        .unwrap();
    let label = super::components::when(at);
    assert!(label.contains(", "));
    assert!(rt.elements().iter().any(|element| element.text.as_deref() == Some(label.as_str())));
}

#[test]
fn every_demo_dialog_should_close_on_escape_even_when_a_field_is_focused() {
    let mut checked = 0;
    for (name, scene) in crate::spec().scenes {
        if crate::LANGUAGE_SCENES.contains(name) {
            continue;
        }
        let mut state = State::detached();
        scene(&mut state);
        // Another scene that switches the language would race every test reading English text.
        assert_eq!(crate::i18n::I18N.locale(), 0, "{name} switches the language: list it in LANGUAGE_SCENES");
        if state.modal.is_none() {
            continue;
        }
        state.popover = None;
        state.preview = None;
        let mut rt = Runtime::new(state, super::view, (1200.0, 760.0), Env::new(true, 1.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
        rt.frame(0.0);
        assert!(rt.state.modal.is_none(), "{name} should close on Escape");
        checked += 1;
    }
    assert!(checked >= 20, "exercise the app’s different dialog kinds");
}

#[test]
fn escape_should_close_a_settings_select_before_its_dialog() {
    let mut rt = runtime(|s| s.show_settings_section(SettingsSection::General));
    rt.on_key = Some(crate::on_key);
    press(&mut rt, "select-language");
    assert!(rt.state.popover.is_some());
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    rt.frame(0.0);
    assert!(rt.state.popover.is_none());
    assert!(rt.state.modal.is_some());
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    rt.frame(0.0);
    assert!(rt.state.modal.is_none());
}

#[test]
fn escape_should_close_an_attachment_preview_before_the_dialog_beneath_it() {
    use crate::state::{DocumentStatus, ImageStatus, Preview, PreviewContent};
    use matrix_core::{Attachment, AttachmentKind, MediaRef};

    for document in [false, true] {
        let mut rt = runtime(|s| {
            s.show_settings_section(SettingsSection::General);
            s.preview = Some(Preview {
                account: AccountId::new(demo::ADA),
                attachment: Attachment {
                    kind: if document { AttachmentKind::File } else { AttachmentKind::Image },
                    name: if document { "notes.pdf" } else { "photo.png" }.into(),
                    size: None,
                    media: None,
                    thumbnail: None,
                    dimensions: None,
                    audio: None,
                },
                media: MediaRef::opaque("preview"),
                content: if document {
                    PreviewContent::Document { pages: Vec::new(), status: DocumentStatus::Downloading }
                } else {
                    PreviewContent::Image { thumbnail: None, status: ImageStatus::Downloading }
                },
            });
        });
        rt.on_key = Some(crate::on_key);
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
        rt.frame(0.0);
        assert!(rt.state.preview.is_none());
        assert!(rt.state.modal.is_some());
    }
}

/// How long building the view takes for a large room whose readers are people the app knows
/// nothing about yet (no loaded member, no message of theirs): `cargo test -p
/// better-matrix-desktop --release -- --ignored --nocapture large_room_view_build`.
#[test]
#[ignore = "a timing, not a check"]
fn large_room_view_build_timing() {
    let mut rt = runtime(|s| {
        demo::read_receipts(s);
        let account = AccountId::new(demo::ADA);
        let room_id = demo::DEMO_THREAD.0.to_owned();
        let member = |id: String| matrix_core::RoomMember {
            person: matrix_core::Person { name: id.clone(), id, avatar: None },
            role: matrix_core::MemberRole::Member,
            presence: matrix_core::Presence::Unknown,
            status_message: None,
            verified: false,
            level: 0,
        };
        let members = matrix_core::Members {
            people: (0..3000).map(|i| member(format!("@member{i}:example.org"))).collect(),
            complete: true,
            ..Default::default()
        };
        s.apply(
            Event::Members { request: None, account: account.clone(), room_id: room_id.clone(), members },
            &mut rune::Tx::default(),
        );
        let entries = (0..400)
            .map(|i| {
                let message = Message {
                    event_id: format!("$bench{i}"),
                    sender_id: format!("@sender{}:example.org", i % 20),
                    sender_name: format!("Sender {}", i % 20),
                    timestamp_ms: 1_700_000_000_000 + i * 60_000,
                    content: MessageContent::Text { body: format!("message {i}"), html: None },
                    read_receipts: (0..5)
                        .map(|r| matrix_core::ReadReceipt {
                            user_id: format!("@lurker{i}-{r}:example.org"),
                            timestamp_ms: None,
                        })
                        .collect(),
                    ..Message::default()
                };
                TimelineEntry { id: format!("bench{i}"), kind: EntryKind::Message(Box::new(message)) }
            })
            .collect();
        s.apply(
            Event::TimelineChanged {
                account,
                key: TimelineKey::room(&room_id),
                diffs: vec![TimelineDiff::Append(entries)],
            },
            &mut rune::Tx::default(),
        );
    });
    rt.frame(1.0 / 60.0);
    // The median of ten batches of a hundred rebuilds, so one busy moment doesn't decide it.
    let mut batches: Vec<f64> = (0..10)
        .map(|_| {
            let started = std::time::Instant::now();
            for _ in 0..100 {
                rt.update(|s, _| s.drop_hover = !s.drop_hover);
                rt.frame(1.0 / 60.0);
            }
            started.elapsed().as_secs_f64() * 1000.0 / 100.0
        })
        .collect();
    batches.sort_by(f64::total_cmp);
    eprintln!("view build + frame: median {:.3} ms, best {:.3} ms", batches[5], batches[0]);
}

#[test]
fn the_update_icon_should_be_absent_without_an_available_release() {
    let rt = runtime(|_| {});
    assert!(rt.find("rail-update").is_none());
}

#[test]
fn an_update_should_appear_directly_above_the_avatar() {
    let rt = runtime(|s| s.update_found(demo::update()));
    let update = rt.presented("rail-update").unwrap().0;
    let avatar = rt.presented("account-button").unwrap().0;
    assert!((avatar[1] - update[1] - update[3] - 16.0).abs() < 0.5);
}

#[test]
fn a_long_rail_should_keep_plus_update_and_avatar_in_order() {
    let mut rt = runtime(|s| {
        demo::many_spaces(s);
        s.update_found(demo::update());
    });
    let add = rt.presented("rail-create-space").unwrap().0;
    let update = rt.presented("rail-update").unwrap().0;
    let avatar = rt.presented("account-button").unwrap().0;
    assert!((update[1] - add[1] - add[3] - 16.0).abs() < 0.5);
    assert!((avatar[1] - update[1] - update[3] - 16.0).abs() < 0.5);
    rt.wheel(34.0, 300.0, -2000.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.presented("rail-create-space").unwrap().0, add);
    assert_eq!(rt.presented("rail-update").unwrap().0, update);
    assert_eq!(rt.presented("account-button").unwrap().0, avatar);
}

#[test]
fn clicking_the_rail_update_should_start_installation() {
    let mut rt = runtime(|s| s.update_found(demo::update()));
    press(&mut rt, "rail-update");
    assert!(matches!(rt.state.update, crate::state::Update::Installing(_)));
}

#[test]
fn an_update_failure_should_keep_the_rail_action_available() {
    let mut rt = runtime(|s| {
        s.update_found(demo::update());
        s.install_update();
        s.update_failed("offline".into());
    });
    press(&mut rt, "rail-update");
    assert!(matches!(rt.state.update, crate::state::Update::Installing(_)));
}

#[test]
fn pressing_the_status_message_should_open_its_field_with_suggestions() {
    let mut rt = runtime(|_| {});
    press(&mut rt, "account-button");
    press(&mut rt, "status-message");
    assert!(rt.find("status-message-field").is_some() && rt.find("status-suggestion-vacation").is_some());
    assert!(rt.find("status-online").is_none(), "writing, the menu is only the status message");
}

#[test]
fn picking_a_suggested_status_message_should_set_it() {
    let mut rt = runtime(|_| {});
    press(&mut rt, "account-button");
    press(&mut rt, "status-message");
    press(&mut rt, "status-suggestion-vacation");
    assert_eq!(rt.state.active_account().and_then(|a| a.status_message()), Some("🌴 On vacation"));
    assert!(rt.find("account-menu").is_none());
}

#[test]
fn a_set_status_message_should_have_a_button_clearing_it() {
    let mut rt = runtime(|s| s.set_status_message(&AccountId::new(demo::ADA), "🌴 Away"));
    press(&mut rt, "account-button");
    press(&mut rt, "status-message-clear");
    assert_eq!(rt.state.active_account().and_then(|a| a.status_message()), None);
}

#[test]
fn a_direct_chat_should_show_the_emoji_its_persons_status_message_starts_with() {
    let rt = runtime(|_| {});
    assert!(rt.find("status-emoji-!dm-kai:example.org").is_some());
}

#[test]
fn a_direct_chats_header_should_say_what_its_person_shows() {
    let rt = runtime(|s| s.select_room("!dm-kai:example.org"));
    assert!(rt.find("room-presence").is_some());
}

#[test]
fn a_room_header_should_not_show_presence() {
    let rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    assert!(rt.find("room-presence").is_none());
}

#[test]
fn a_section_should_count_its_rooms_mentions_while_collapsed() {
    let rt = runtime(|s| {
        let ada = AccountId::new(demo::ADA);
        s.accounts.set_sidebar(&ada, matrix_core::SidebarLayout::default());
        s.toggle_section(crate::state::DIRECT);
    });
    // Grace's two and Kai's one.
    assert!(rt.find("section-mentions-direct").is_some());
    let elements = rt.elements();
    let badge = elements.iter().position(|element| element.key.as_deref() == Some("section-mentions-direct"));
    assert!(badge.is_some(), "the section's mention count");
    assert!(elements.iter().any(|element| element.text.as_deref() == Some("3") && element.parent == badge));
    assert!(rt.find("room-!dm-grace:example.org").is_none(), "collapsed, its rooms are hidden");
}

#[test]
fn escape_while_writing_a_status_message_should_go_back_to_the_menu() {
    let mut rt = runtime(|_| {});
    rt.on_key = crate::spec().on_key;
    press(&mut rt, "account-button");
    press(&mut rt, "status-message");
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
    rt.frame(0.1);
    assert!(rt.state.status_draft.is_none());
    assert!(rt.find("status-online").is_some(), "the account menu stays open");
}

#[test]
fn a_direct_chats_row_should_follow_its_persons_presence() {
    let mut rt = runtime(|_| {});
    assert!(rt.find("status-emoji-!dm-kai:example.org").is_some());
    let back = matrix_core::PersonPresence {
        user_id: "@kai:example.org".into(),
        presence: matrix_core::Presence::Online,
        status_message: None,
    };
    rt.update(|s, tx| s.apply(Event::Presence { account: AccountId::new(demo::ADA), people: vec![back] }, tx));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("status-emoji-!dm-kai:example.org").is_none());
}

#[test]
fn a_direct_chat_more_people_joined_should_show_no_presence() {
    let rt = runtime(|s| {
        let ada = AccountId::new(demo::ADA);
        let Some(account) = s.active_account() else { return };
        let mut list = matrix_core::RoomList {
            rooms: account.rooms.clone(),
            spaces: account.spaces.clone(),
            invitations: Vec::new(),
        };
        // Two joined, a third invited.
        for room in list.rooms.iter_mut().filter(|room| room.room_id == "!dm-kai:example.org") {
            room.member_count = 2;
            room.active_member_count = 3;
        }
        s.apply(Event::RoomsUpdated { account: ada, list }, &mut rune::Tx::default());
    });
    assert!(rt.find("status-emoji-!dm-kai:example.org").is_none());
}

#[test]
fn choosing_away_in_the_account_menu_should_set_the_status() {
    let mut rt = runtime(|_| {});
    press(&mut rt, "account-button");
    assert!(rt.find("status-online").is_some() && rt.find("status-hidden").is_some());
    press(&mut rt, "status-away");
    assert_eq!(rt.state.active_account().map(|a| a.status), Some(matrix_core::Status::Away));
    assert!(rt.find("account-menu").is_none());
}

#[test]
fn the_account_menu_should_open_saved_messages() {
    let mut rt = runtime(|s| s.toggle_popover(crate::state::Popover::AccountSwitcher));
    press(&mut rt, "menu-saved");
    assert!(rt.find("saved-dialog").is_some());
}

#[test]
fn the_message_menu_should_offer_to_save_a_message() {
    let rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: demo::DEMO_THREAD.1.into(),
            at: Some((600, 280)),
        });
    });
    assert!(rt.find("message-save").is_some());
}

/// The first message the saved-messages demo lists, and its room.
fn first_saved(rt: &Runtime<State>) -> (String, String) {
    let account = rt.state.active_account().expect("account");
    let saved = account.saved_messages().next().expect("a saved message");
    (saved.room_id.clone(), saved.preview.event_id.clone())
}

#[test]
fn opening_a_saved_message_should_close_the_list_and_show_its_room() {
    let mut rt = runtime(demo::saved_messages);
    let (room_id, event_id) = first_saved(&rt);
    press(&mut rt, &format!("saved-open-{room_id}-{event_id}"));
    assert!(rt.state.modal.is_none());
    assert_eq!(rt.state.active_room_id(), Some(room_id.as_str()));
}

#[test]
fn the_bookmark_button_should_not_open_the_saved_message() {
    let mut rt = runtime(demo::saved_messages);
    let (room_id, event_id) = first_saved(&rt);
    press(&mut rt, &format!("saved-remove-{room_id}-{event_id}"));
    assert!(rt.find("saved-dialog").is_some());
    assert!(rt.state.message_jump.is_none());
}

#[test]
fn the_account_menu_should_fit_the_smallest_window() {
    let mut state = State::detached();
    demo::seed(&mut state);
    state.set_status_message(&AccountId::new(demo::ADA), "🌴 On vacation until Monday");
    state.toggle_popover(crate::state::Popover::AccountSwitcher);
    let mut rt = Runtime::new(state, super::view, (720.0, 480.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let menu = rt.presented("account-menu").expect("the account menu").0;
    assert!(menu[1] >= 0.0 && menu[1] + menu[3] <= 480.0, "the menu stays inside the window: {menu:?}");
    rt.update(|_, tx| tx.scroll_into_view("menu-sign-out"));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    let sign_out = rt.presented("menu-sign-out").expect("sign out").0;
    assert!(sign_out[1] + sign_out[3] <= 480.0, "sign out can be reached: {sign_out:?}");
}

#[test]
fn scrolling_past_messages_should_follow_reading_without_rebuilding_the_view() {
    let mut rt = runtime(demo::stress);
    rt.settle_all();
    rt.update(|_, tx| tx.scroll_to_instant("timeline", 0.0, 20_000.0));
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    let (account, key) = (AccountId::new(demo::ADA), TimelineKey::room("!big:example.org"));
    let read_up_to = |rt: &Runtime<State>| {
        rt.state.accounts.get(&account).and_then(|data| data.timeline(&key)).and_then(|t| t.read_up_to.clone())
    };
    // As if the timeline had opened scrolled up here.
    rt.update(|s, _| {
        if let Some(timeline) = s.accounts.kept_timeline_mut(&AccountId::new(demo::ADA), &key) {
            timeline.read_up_to = None;
        }
    });
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    let (x, y) = rt.find("timeline").expect("the timeline");
    let (before, commits) = (read_up_to(&rt), rt.stats.commits);
    // Within the rows already mounted, so Rune itself has nothing to build.
    rt.wheel(x, y, 60.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert_ne!(read_up_to(&rt), before, "reading moved on");
    assert_eq!(rt.stats.commits, commits, "following it is bookkeeping");
}

#[test]
fn changing_the_member_search_should_change_the_list_shown() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_members();
    });
    assert!(rt.find("member-@kai:example.org").is_some());
    rt.update(|s, _| s.member_search.insert("grace"));
    rt.frame(1.0 / 60.0);
    assert!(rt.find("member-@grace:example.org").is_some() && rt.find("member-@kai:example.org").is_none());
    rt.update(|s, _| s.member_search = rune::TextEdit::default());
    rt.frame(1.0 / 60.0);
    assert!(rt.find("member-@kai:example.org").is_some());
}

/// The demo room holding only `count` short messages from Grace, all on screen.
fn short_room(s: &mut State, count: u64) {
    s.select_room(demo::DEMO_THREAD.0);
    let entries = (0..count).map(|i| short_message(&format!("short-{i}"), i)).collect();
    s.apply(
        Event::TimelineChanged {
            account: AccountId::new(demo::ADA),
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(entries)],
        },
        &mut rune::Tx::default(),
    );
}

fn short_message(id: &str, minute: u64) -> TimelineEntry {
    TimelineEntry {
        id: id.to_owned(),
        kind: EntryKind::Message(Box::new(Message {
            event_id: format!("${id}"),
            sender_id: "@grace:example.org".into(),
            sender_name: "Grace Hopper".into(),
            timestamp_ms: demo::DEMO_DAY_MS + minute * 600_000,
            content: MessageContent::Text { body: "Short".into(), html: None },
            ..Message::default()
        })),
    }
}

fn element_rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect
}

#[test]
fn a_press_just_left_of_message_text_should_not_open_the_thread() {
    let mut rt = runtime(|s| short_room(s, 1));
    // The words start where the sender's name does.
    let words = element_rect(&rt, "msg-short-0-sender-name")[0];
    let text = element_rect(&rt, "text-$short-0");
    rt.pointer_down(words - 5.0, text[1] + text[3] / 2.0);
    rt.pointer_up(words - 5.0, text[1] + text[3] / 2.0);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.active_thread().is_none());
}

#[test]
fn a_drag_from_beside_message_text_should_select_it() {
    let mut rt = runtime(|s| short_room(s, 3));
    let text = element_rect(&rt, "text-$short-2");
    let y = text[1] + text[3] / 2.0;
    // Past the last glyph: the box ends `REACH` beyond the words.
    let beside = text[0] + text[2] - super::message_text::REACH + 4.0;
    rt.pointer_move(beside, y);
    rt.pointer_down(beside, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_move(text[0] - 20.0, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(text[0] - 20.0, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.selected_text().as_deref(), Some("Short"));
    assert!(rt.state.active_thread().is_none());
}

#[test]
fn pressing_a_message_beside_its_text_should_still_open_the_thread() {
    let mut rt = runtime(|s| short_room(s, 3));
    let text = element_rect(&rt, "text-$short-2");
    rt.pointer_down(text[0] + text[2] + 40.0, text[1] + text[3] / 2.0);
    rt.pointer_up(text[0] + text[2] + 40.0, text[1] + text[3] / 2.0);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.active_thread(), Some("$short-2"));
}

#[test]
fn pressing_a_senders_name_should_open_their_profile_rather_than_the_thread() {
    let mut rt = runtime(|s| short_room(s, 1));
    press(&mut rt, "msg-short-0-sender-name");
    assert!(rt.state.active_thread().is_none());
    assert!(matches!(&rt.state.side, Some(SidePanel::Person(person)) if person.id == "@grace:example.org"));
    assert!(rt.find("person-panel").is_some());
}

#[test]
fn pressing_a_senders_picture_should_open_their_profile_rather_than_the_thread() {
    let mut rt = runtime(|s| short_room(s, 1));
    press(&mut rt, "msg-short-0-sender-face");
    assert!(rt.state.active_thread().is_none());
    assert!(matches!(&rt.state.side, Some(SidePanel::Person(person)) if person.id == "@grace:example.org"));
}

#[test]
fn a_profile_should_offer_the_member_menu_beside_its_more_button() {
    let mut rt = runtime(|s| short_room(s, 1));
    press(&mut rt, "msg-short-0-sender-name");
    press(&mut rt, "person-more");
    assert!(rt.find("member-menu").is_some());
    assert!(rt.find("member-menu-invite").is_some());
}

/// As [`runtime`], built the way a window builds it: a change no input in its frame caused
/// lands at once unless something in that frame asks for motion.
fn window_runtime(scene: impl FnOnce(&mut State)) -> Runtime<State> {
    let mut state = State::detached();
    demo::seed(&mut state);
    scene(&mut state);
    let mut rt =
        rune::App::new(crate::spec()).init(move || state).headless_runtime((1200.0, 760.0), Env::new(true, 1.0));
    rt.frame(0.0);
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

/// The heights `entries` show at in a window on the first frame after `diffs` bring them,
/// each beside its settled height, in a frame that asked for motion (as the read marker's
/// fade does when a room switch shows it). `also` runs in that frame too.
fn arriving_in_a_window(
    scene: impl FnOnce(&mut State),
    key: TimelineKey,
    diffs: Vec<TimelineDiff>,
    also: impl FnOnce(&mut State, &mut rune::Tx),
    entries: &[&str],
) -> Vec<(f32, f32)> {
    let mut rt = window_runtime(scene);
    rt.update(|_, tx| tx.animated());
    rt.update(|s, tx| {
        s.apply(Event::TimelineChanged { account: AccountId::new(demo::ADA), key, diffs }, tx);
        also(s, tx);
    });
    rt.frame(1.0 / 60.0);
    let first: Vec<f32> = entries.iter().map(|entry| rt.presented(entry).expect(entry).0[3]).collect();
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    entries.iter().zip(first).map(|(entry, first)| (first, rt.presented(entry).unwrap().0[3])).collect()
}

fn room_key() -> TimelineKey {
    TimelineKey::room(demo::DEMO_THREAD.0)
}

#[test]
fn a_page_of_older_messages_in_a_window_should_land_at_once() {
    let page = (0..3).map(|i| TimelineDiff::PushFront(short_message(&format!("older-{i}"), 0))).collect();
    for (arriving, settled) in arriving_in_a_window(
        |s| short_room(s, 2),
        room_key(),
        page,
        |_, _| {},
        &["msg-older-0", "msg-older-1", "msg-older-2"],
    ) {
        assert!((arriving - settled).abs() < 0.5, "it eased in: {arriving} of {settled}");
    }
}

#[test]
fn older_messages_should_land_at_once_when_their_page_ends_in_the_same_frame() {
    let page = vec![TimelineDiff::PushFront(short_message("older", 0))];
    let loaded = |s: &mut State, tx: &mut rune::Tx| {
        s.apply(Event::HistoryLoaded { account: AccountId::new(demo::ADA), key: room_key(), reached_start: false }, tx);
    };
    let [(arriving, settled)] =
        arriving_in_a_window(|s| short_room(s, 2), room_key(), page, loaded, &["msg-older"])[..]
    else {
        unreachable!()
    };
    assert!((arriving - settled).abs() < 0.5, "it eased in: {arriving} of {settled}");
}

#[test]
fn a_new_message_in_a_window_frame_that_asked_for_motion_should_ease_in() {
    let news = vec![TimelineDiff::PushBack(short_message("new", 30))];
    let [(arriving, settled)] =
        arriving_in_a_window(|s| short_room(s, 2), room_key(), news, |_, _| {}, &["msg-new"])[..]
    else {
        unreachable!()
    };
    assert!(arriving < settled - 10.0, "it landed at once: {arriving} of {settled}");
}

#[test]
fn a_threads_history_should_leave_news_in_the_room_easing_in() {
    let scene = |s: &mut State| {
        short_room(s, 2);
        s.open_thread("$short-1");
    };
    let thread = TimelineKey::thread(demo::DEMO_THREAD.0, "$short-1");
    let page = vec![TimelineDiff::PushFront(short_message("reply", 0))];
    let news = |s: &mut State, tx: &mut rune::Tx| {
        let diffs = vec![TimelineDiff::PushBack(short_message("new", 30))];
        s.apply(Event::TimelineChanged { account: AccountId::new(demo::ADA), key: room_key(), diffs }, tx);
    };
    let [(arriving, settled)] = arriving_in_a_window(scene, thread, page, news, &["msg-new"])[..] else {
        unreachable!()
    };
    assert!(arriving < settled - 10.0, "the room's news landed with the thread's history: {arriving} of {settled}");
}

/// The demo room holding one message, from `sender`, with `html` formatting when given.
fn one_message_from(s: &mut State, sender: &str, html: Option<&str>) {
    s.select_room(demo::DEMO_THREAD.0);
    let mut entry = short_message("only", 0);
    if let EntryKind::Message(message) = &mut entry.kind {
        message.sender_id = sender.to_owned();
        message.sender_name = sender.to_owned();
        message.content = MessageContent::Text { body: "Short".into(), html: html.map(str::to_owned) };
    }
    s.apply(
        Event::TimelineChanged {
            account: AccountId::new(demo::ADA),
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(vec![entry])],
        },
        &mut rune::Tx::default(),
    );
}

#[test]
fn the_right_edge_of_a_senders_picture_should_open_their_profile() {
    let mut rt = runtime(|s| short_room(s, 1));
    let face = element_rect(&rt, "msg-short-0-sender-face");
    let (x, y) = (face[0] + face[2] - 1.0, face[1] + face[3] - 4.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(matches!(&rt.state.side, Some(SidePanel::Person(_))), "{:?}", rt.state.side);
}

#[test]
fn a_press_just_left_of_formatted_text_should_not_open_the_thread() {
    let mut rt = runtime(|s| one_message_from(s, "@grace:example.org", Some("<b>Short</b>")));
    let words = element_rect(&rt, "msg-only-sender-name")[0];
    let text = element_rect(&rt, "rich-$only-0");
    rt.pointer_down(words - 5.0, text[1] + text[3] / 2.0);
    rt.pointer_up(words - 5.0, text[1] + text[3] / 2.0);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.active_thread().is_none());
}

#[test]
fn the_users_own_profile_should_offer_editing_instead_of_a_chat() {
    let mut rt = runtime(|s| one_message_from(s, demo::ADA, None));
    press(&mut rt, "msg-only-sender-name");
    assert!(rt.find("person-edit").is_some());
    assert!(rt.find("person-chat").is_none());
    assert!(rt.find("person-more").is_none());
}

#[test]
fn the_profile_of_someone_the_member_list_lacks_should_still_offer_the_member_menu() {
    let mut rt = runtime(|s| one_message_from(s, "@stranger:example.org", None));
    press(&mut rt, "msg-only-sender-name");
    press(&mut rt, "person-more");
    assert!(rt.find("member-menu-chat").is_some());
}

#[test]
fn history_landing_as_the_new_marker_starts_fading_should_keep_the_fade() {
    let mut rt = window_runtime(unread_timeline);
    rt.update(|s, tx| {
        s.focus_changed(true);
        marker_changed(s, tx, vec![TimelineDiff::PushFront(short_message("older", 0))]);
    });
    rt.frame(1.0 / 60.0);
    let older = rt.presented("msg-older").expect("the older message").0[3];
    let mut seen = Vec::new();
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
        seen.push(rt.presented("new-marker").expect("the divider should still be fading").1);
    }
    let settled = rt.presented("msg-older").unwrap().0[3];
    assert!((older - settled).abs() < 0.5, "the history eased in: {older} of {settled}");
    assert!(seen.iter().any(|opacity| *opacity > 0.05 && *opacity < 0.95), "the fade was cut: {seen:?}");
}
