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
                None,
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
fn a_room_with_pins_should_open_them_from_its_header() {
    let mut rt = runtime(demo::pinned_messages);
    press(&mut rt, super::room::PINS);
    assert_eq!(rt.state.side, Some(SidePanel::Pins));
    assert!(rt.find("pins-panel").is_some());
}

#[test]
fn the_header_pin_button_should_sit_between_the_members_and_the_info() {
    let rt = runtime(demo::pinned_messages);
    let members = rt.find(super::people::MEMBERS).expect("members button").0;
    let pins = rt.find(super::room::PINS).expect("pins button").0;
    let info = rt.find(super::people::INFO).expect("info button").0;
    assert!(members < pins && pins < info);
}

#[test]
fn a_room_without_pins_should_have_no_pin_button_in_its_header() {
    let rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    assert!(rt.find("room-header").is_some());
    assert!(rt.find(super::room::PINS).is_none());
}

#[test]
fn the_header_should_show_the_topic_beside_the_room_name() {
    let rt = runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    let topic = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("room-topic")).expect("topic");
    assert_eq!(topic.text.as_deref(), Some("Company-wide announcements and chatter"));
}

/// Gives the demo's #general `topic`.
fn general_topic(s: &mut State, topic: &str) {
    let account = AccountId::new(demo::ADA);
    let Some(data) = s.accounts.get(&account) else { return };
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    if let Some(room) = rooms.iter_mut().find(|r| r.room_id == demo::DEMO_THREAD.0) {
        room.topic = Some(topic.to_owned());
    }
    s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, spaces, invitations: Vec::new() });
    s.select_room(demo::DEMO_THREAD.0);
}

#[test]
fn a_markdown_topic_should_show_formatted_in_the_header() {
    let rt = runtime(|s| general_topic(s, "[Github](https://github.com/x) | **Office**\nSecond line"));
    let topic = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("room-topic")).expect("topic");
    assert_eq!(topic.text.as_deref(), Some("Github | Office"));
}

#[test]
fn a_markdown_topic_should_show_formatted_in_room_info() {
    let rt = runtime(|s| {
        general_topic(s, "[Github](https://github.com/x) | **Office**");
        s.show_side(SidePanel::Info);
    });
    let words = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("info-topic-0")).expect("topic");
    assert_eq!(words.text.as_deref(), Some("Github | Office"));
}

#[test]
fn a_room_without_a_topic_should_show_none_in_its_header_or_info() {
    let rt = runtime(|s| {
        s.select_room("!releases:example.org");
        s.show_side(SidePanel::Info);
    });
    assert!(rt.find("room-topic").is_none());
    assert!(rt.find("info-topic").is_none());
}

#[test]
fn a_multi_line_topic_should_show_only_its_first_line() {
    assert_eq!(super::room::topic_line("\n  Releases every Friday  \nAsk in #help"), Some("Releases every Friday"));
}

#[test]
fn a_blank_topic_should_show_nothing() {
    assert_eq!(super::room::topic_line(" \n\t"), None);
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
            sender: None,
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
                room_id: demo::DEMO_THREAD.0.into(),
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
                batch: Vec::new(),
                index: 0,
                visit: 0,
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
fn a_package_update_should_show_its_command_beside_the_rail() {
    let mut rt = runtime(|s| s.update_found(demo::package_update()));
    press(&mut rt, "rail-update");
    let panel = rt.presented("update-panel").unwrap().0;
    let button = rt.presented("rail-update").unwrap().0;
    assert!(panel[0] > button[0] + button[2]);
    assert!(rt.find("update-command").is_some());
}

#[test]
fn a_long_update_command_should_wrap_inside_its_panel() {
    let mut rt = runtime(|s| s.update_found(demo::package_update()));
    press(&mut rt, "rail-update");
    let panel = rt.presented("update-panel").unwrap().0;
    let command = rt.presented("update-command-text").unwrap().0;
    assert!(command[0] + command[2] <= panel[0] + panel[2]);
}

#[test]
fn a_long_update_command_should_keep_copy_reachable_in_the_smallest_window() {
    let image = "'/home/alexandra/Downloads/Better Matrix Releases/Better Matrix.AppImage'";
    let staged = "'/home/alexandra/Downloads/Better Matrix Releases/Better Matrix.AppImage.new'";
    let command = format!(
        "curl -fLo {staged} 'https://github.com/m43i/better-matrix/releases/download/v0.1.0/better-matrix-linux-x86_64.AppImage' && chmod +x {staged} && mv {staged} {image}"
    );
    let release = crate::platform::updates::Release {
        version: "0.1.0".into(),
        upgrade: crate::platform::updates::Upgrade::Command(command),
    };
    let mut rt = runtime(|s| s.update_found(release));
    // The smallest window, 720 by 480 points, at 175% zoom.
    rt.set_viewport((411.0, 274.0), 1.0);
    rt.frame(1.0 / 60.0);
    press(&mut rt, "rail-update");
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let copy = rt.presented("update-copy").expect("the copy button").0;
    assert!(copy[1] >= 0.0 && copy[1] + copy[3] <= 274.0, "copy is at {copy:?}");
    assert!(copy[0] >= 0.0 && copy[0] + copy[2] <= 411.0, "copy is at {copy:?}");
}

#[test]
fn hovering_a_code_block_should_reveal_a_button_that_copies_its_code() {
    let clipboard = rune::clipboard::memory();
    let mut rt = runtime(demo::message_features);
    rt.use_clipboard(&clipboard);
    let key = |rt: &Runtime<State>, prefix: &str| {
        rt.elements().into_iter().filter_map(|e| e.key).find(|key| key.starts_with(prefix)).expect(prefix)
    };
    let (copy, block) = (key(&rt, "rich-$code-copy-code-"), key(&rt, "rich-$code-code-"));
    assert!(rt.presented(&copy).expect("the button").1 < 0.01, "hidden at rest");
    let (x, y) = rt.find(&block).expect("the code block");
    rt.pointer_move(x, y);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.presented(&copy).expect("the button").1 > 0.99, "shown while the block is hovered");
    press(&mut rt, &copy);
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("fn retry(times: u32) -> bool {\n    // Once more, then give up.\n    times < 3\n}")
    );
}

#[test]
fn copying_the_update_command_should_put_it_on_the_clipboard() {
    let clipboard = rune::clipboard::memory();
    let mut rt = runtime(|s| {
        s.update_found(demo::package_update());
        s.install_update();
    });
    rt.use_clipboard(&clipboard);
    press(&mut rt, "update-copy");
    let Some(crate::platform::updates::Upgrade::Command(command)) =
        rt.state.update.release().map(|release| release.upgrade.clone())
    else {
        panic!("no command offered")
    };
    assert_eq!(clipboard.text().as_deref().map(String::as_str), Some(command.as_str()));
}

#[test]
fn copying_the_update_command_should_close_its_panel_with_a_notice() {
    let mut rt = runtime(|s| {
        s.update_found(demo::package_update());
        s.install_update();
    });
    press(&mut rt, "update-copy");
    assert!(rt.find("update-panel").is_none());
    assert_eq!(rt.state.notice.as_deref(), Some(crate::i18n::t!("update.copied")));
}

#[test]
fn an_unknown_install_should_offer_the_release_page() {
    let mut rt = runtime(|s| s.update_found(demo::page_update()));
    press(&mut rt, "rail-update");
    assert!(rt.find("update-page").is_some());
    assert!(rt.find("update-command").is_none());
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
fn the_top_bar_should_open_saved_messages() {
    let mut rt = runtime(|_| {});
    press(&mut rt, super::people::SAVED);
    assert!(rt.find("saved-dialog").is_some());
}

#[test]
fn saved_messages_should_open_at_once_while_searching_under_the_unverified_banner() {
    let mut rt = runtime(|s| {
        s.select_account(&AccountId::new("@ada:hobby.chat"));
        s.open_search();
    });
    assert!(rt.find("unverified-bar").is_some());
    press(&mut rt, super::people::SAVED);
    assert!(rt.find("saved-dialog").is_some());
}

#[test]
fn the_new_chat_search_should_offer_the_note_to_self_before_anyone() {
    let rt = runtime(|s| {
        s.open_new_chat();
        let grace = matrix_core::Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None };
        s.chat_search.people = vec![grace];
    });
    let note = rt.presented(super::sidebar::NOTE_TO_SELF).expect("the note to self").0;
    let first = rt.presented("new-chat-@grace:example.org").expect("a suggested person").0;
    assert!(note[1] < first[1], "the note to self comes first: {note:?} {first:?}");
}

#[test]
fn the_note_to_self_should_lead_homes_sidebar() {
    let rt = runtime(|_| {});
    let note = rt.presented(super::sidebar::NOTE_TO_SELF_ROW).expect("the note to self").0;
    let section = rt.presented("section-header-rooms").expect("a section").0;
    assert!(note[1] < section[1], "{note:?} above {section:?}");
}

#[test]
fn the_note_to_self_info_should_lead_to_its_members_to_invite_someone() {
    let rt = runtime(|s| {
        let account = s.active_account().unwrap().summary.id.clone();
        s.open_room(&account, "!notes:example.org");
        s.toggle_side(SidePanel::Info);
    });
    assert!(rt.find("info-members").is_some());
}

#[test]
fn a_space_should_not_show_the_note_to_self() {
    let rt = runtime(|s| s.select_scope(crate::state::Scope::Space(demo::WORK_SPACE.into())));
    assert!(rt.find(super::sidebar::NOTE_TO_SELF_ROW).is_none());
    assert!(rt.find("room-!notes:example.org").is_none());
}

#[test]
fn an_account_without_a_note_to_self_should_show_none() {
    let rt = runtime(|s| s.select_account(&AccountId::new("@ada:hobby.chat")));
    assert!(rt.find(super::sidebar::NOTE_TO_SELF_ROW).is_none());
}

#[test]
fn the_new_chat_search_should_offer_the_note_to_self_for_its_name_and_the_users_id() {
    for (typed, offered) in [("note", true), ("@ada:example.org", true), ("grace", false)] {
        let rt = runtime(|s| {
            s.open_new_chat();
            s.chat_search.field = rune::TextEdit::new(typed);
        });
        assert_eq!(rt.find(super::sidebar::NOTE_TO_SELF).is_some(), offered, "{typed}");
    }
}

#[test]
fn choosing_the_note_to_self_should_close_the_new_chat_search() {
    let mut rt = runtime(State::open_new_chat);
    press(&mut rt, super::sidebar::NOTE_TO_SELF);
    assert!(rt.state.popover.is_none());
}

#[test]
fn the_note_to_self_should_say_only_the_user_sees_it() {
    let rt = runtime(|s| {
        let account = s.active_account().unwrap().summary.id.clone();
        let notes = s.active_account().unwrap().rooms.iter().find(|room| room.note_to_self).unwrap().room_id.clone();
        s.open_room(&account, &notes);
    });
    assert!(rt.find("room-private").is_some());
}

#[test]
fn the_account_menu_should_leave_saved_messages_to_the_top_bar() {
    let rt = runtime(|s| s.toggle_popover(crate::state::Popover::AccountSwitcher));
    assert!(rt.find("account-menu").is_some());
    assert!(rt.find("menu-saved").is_none());
}

#[test]
fn the_account_menu_should_end_the_accounts_with_adding_one() {
    let rt = runtime(|s| s.toggle_popover(crate::state::Popover::AccountSwitcher));
    let last = rt.state.accounts.iter().last().map(|account| account.summary.id.clone()).expect("an account");
    let last_account = rt.presented(&format!("switch-{last}")).expect("the last account").0;
    let add = rt.presented("menu-add-account").expect("add account").0;
    let profile = rt.presented("menu-profile").expect("profile").0;
    let account_end = last_account[1] + last_account[3];
    assert!(
        (add[1] - account_end).abs() < 0.5,
        "no divider between the accounts and adding one: {last_account:?} {add:?}"
    );
    assert!(profile[1] - (add[1] + add[3]) > 1.0, "a divider ends the accounts: {add:?} {profile:?}");
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
fn the_account_menu_should_set_signing_out_apart() {
    let rt = runtime(|s| s.toggle_popover(crate::state::Popover::AccountSwitcher));
    let settings = rt.presented("menu-settings").expect("settings").0;
    let sign_out = rt.presented("menu-sign-out").expect("sign out").0;
    assert!(sign_out[1] - (settings[1] + settings[3]) > 1.0, "a divider above signing out: {settings:?} {sign_out:?}");
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

/// Whether `entry`'s row shows whole inside the room's viewport on the first frame after
/// `diffs` bring it, in a window frame that asked for motion.
fn shown_whole_at_once(scene: impl FnOnce(&mut State), diffs: Vec<TimelineDiff>, entry: &str) -> bool {
    let mut rt = window_runtime(scene);
    rt.update(|_, tx| tx.animated());
    rt.update(|s, tx| {
        s.apply(Event::TimelineChanged { account: AccountId::new(demo::ADA), key: room_key(), diffs }, tx);
    });
    rt.frame(1.0 / 60.0);
    let (row, viewport) = (rt.presented(entry).expect(entry).0, rt.presented("timeline").expect("viewport").0);
    row[1] >= viewport[1] - 0.5 && row[1] + row[3] <= viewport[1] + viewport[3] + 0.5
}

#[test]
fn older_messages_in_a_short_room_should_show_whole_on_their_first_frame() {
    let page = (0..3).map(|i| TimelineDiff::PushFront(short_message(&format!("older-{i}"), 0))).collect();
    assert!(shown_whole_at_once(|s| short_room(s, 2), page, "msg-older-2"), "the viewport uncovered the page");
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
fn history_landing_while_the_new_marker_fades_should_keep_the_fade() {
    let mut rt = window_runtime(unread_timeline);
    rt.update(|s, _| s.focus_changed(true));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    let before = rt.presented("new-marker").expect("the divider").1;
    assert!(before > 0.1 && before < 0.95, "the fade should be under way: {before}");
    // In a frame that asked for motion, where the landing would otherwise take the fade along.
    rt.update(|_, tx| tx.animated());
    rt.update(|s, tx| marker_changed(s, tx, vec![TimelineDiff::PushFront(short_message("older", 0))]));
    rt.frame(1.0 / 60.0);
    let after = rt.presented("new-marker").expect("the divider").1;
    assert!(after > before - 0.2, "the fade jumped from {before} to {after}");
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

#[test]
fn the_presence_dots_overhang_should_open_the_profile() {
    let mut rt = runtime(|s| short_room(s, 1));
    let face = element_rect(&rt, "msg-short-0-sender-face");
    // One point past the 36 pt picture, on the part of the dot that reaches beyond it.
    let (x, y) = (face[0] + 37.0, face[1] + 32.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.active_thread().is_none());
    assert!(matches!(&rt.state.side, Some(SidePanel::Person(_))), "{:?}", rt.state.side);
}

#[test]
fn the_users_own_profile_should_show_a_saved_name_before_the_member_list_has_it() {
    let mut rt = runtime(|s| {
        one_message_from(s, demo::ADA, None);
        let mut summary = s.active_account().unwrap().summary.clone();
        summary.display_name = "Ada, renamed".into();
        s.apply(Event::AccountChanged(summary), &mut rune::Tx::default());
    });
    press(&mut rt, "msg-only-sender-name");
    let shown = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("person-name")).and_then(|e| e.text);
    assert_eq!(shown.as_deref(), Some("Ada, renamed"));
}

#[test]
fn switching_to_a_room_should_show_its_first_page_whole_on_its_first_frame() {
    let mut rt = window_runtime(|s| s.select_room(demo::DEMO_THREAD.0));
    press(&mut rt, "room-!dm-grace:example.org");
    let key = TimelineKey::room("!dm-grace:example.org");
    let cached = vec![TimelineDiff::Reset(vec![short_message("cached-0", 10), short_message("cached-1", 11)])];
    rt.update(|s, tx| {
        s.apply(Event::TimelineChanged { account: AccountId::new(demo::ADA), key: key.clone(), diffs: cached }, tx)
    });
    rt.frame(1.0 / 60.0);
    // The page arrives in a frame that asked for motion, as the New divider's fade does.
    rt.update(|_, tx| tx.animated());
    let page = (0..3).map(|i| TimelineDiff::PushFront(short_message(&format!("page-{i}"), 0))).collect();
    rt.update(|s, tx| s.apply(Event::TimelineChanged { account: AccountId::new(demo::ADA), key, diffs: page }, tx));
    rt.frame(1.0 / 60.0);
    let entries = ["msg-page-0", "msg-page-1", "msg-page-2"];
    let viewport = rt.presented("timeline").expect("viewport").0;
    let first: Vec<[f32; 4]> = entries.iter().map(|entry| rt.presented(entry).expect(entry).0).collect();
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
    for (entry, row) in entries.iter().zip(first) {
        let settled = rt.presented(entry).unwrap().0[3];
        assert!((row[3] - settled).abs() < 0.5, "{entry} eased in: {} of {settled}", row[3]);
        assert!(
            row[1] >= viewport[1] - 0.5 && row[1] + row[3] <= viewport[1] + viewport[3] + 0.5,
            "{entry} {row:?} outside {viewport:?}"
        );
    }
}

#[test]
fn held_back_media_should_show_a_placeholder_until_show_is_pressed() {
    let mut rt = runtime(demo::hidden_media);
    assert!(rt.find("hidden-media-$attachment").is_some());
    assert!(rt.find("image-better-matrix.png").is_none());
    press(&mut rt, "show-media-$attachment");
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("hidden-media-$attachment").is_none());
    assert!(rt.find("image-better-matrix.png").is_some());
}

#[test]
fn a_strangers_hidden_invitation_should_stay_out_of_the_sidebar_but_open_from_settings() {
    let mut rt = runtime(demo::safety);
    assert!(rt.find("invitation-!prizes:spam.example").is_none());
    press(&mut rt, "safety-hidden-!prizes:spam.example");
    assert!(
        matches!(&rt.state.modal, Some(crate::state::Modal::Invitation { room_id, .. }) if room_id == "!prizes:spam.example")
    );
}

#[test]
fn stop_ignoring_in_settings_should_remove_the_person_from_the_list() {
    // A window tall enough for the list at the bottom of the page.
    let mut state = State::detached();
    demo::safety(&mut state);
    let mut rt = Runtime::new(state, super::view, (1200.0, 1500.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    press(&mut rt, "safety-unignore-@troll:spam.example");
    assert!(rt.find("safety-ignored-@troll:spam.example").is_none());
    assert!(rt.find("safety-ignored-none").is_some());
}

#[test]
fn choosing_where_media_shows_should_select_it() {
    let mut rt = runtime(demo::safety);
    press(&mut rt, "safety-media-Never");
    assert_eq!(rt.state.active_account().unwrap().safety.media, matrix_core::ShowMedia::Never);
}

#[test]
fn someone_elses_message_should_offer_a_report_that_opens_the_dialog() {
    let mut rt = runtime(|s| {
        demo::attachment_message(s);
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: "$attachment".into(),
            at: Some((600, 200)),
        });
    });
    press(&mut rt, "message-report");
    assert!(rt.find("report-dialog").is_some());
}

#[test]
fn the_report_dialog_should_tick_also_ignore_when_pressed() {
    let mut rt = runtime(demo::report);
    press(&mut rt, "report-ignore");
    assert!(matches!(&rt.state.modal, Some(crate::state::Modal::Report(form)) if form.also_ignore));
}

#[test]
fn the_member_menu_should_offer_to_ignore_and_report() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.show_side(SidePanel::Person(matrix_core::Person {
            id: "@grace:example.org".into(),
            name: "Grace Hopper".into(),
            avatar: None,
        }));
        s.toggle_member_menu("@grace:example.org");
    });
    assert!(rt.find("member-menu-report").is_some());
    press(&mut rt, "member-menu-ignore");
    assert!(matches!(
        &rt.state.modal,
        Some(crate::state::Modal::Confirm(crate::state::Confirm::Ignore { user_id, .. })) if user_id == "@grace:example.org"
    ));
}

#[test]
fn the_files_panel_should_leave_out_files_of_people_the_user_ignores() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_side(SidePanel::Files);
        demo::room_history(s);
        let safety =
            matrix_core::Safety { ignored: vec!["@grace:example.org".into()], ..matrix_core::Safety::default() };
        s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, &mut rune::Tx::default());
    });
    rt.frame(1.0 / 60.0);
    assert!(rt.find("listed-file-$file-screenshot-sidebar.png").is_none());
    assert!(rt.find("listed-file-$file-standup.ogg").is_some());
}

#[test]
fn a_held_preview_should_offer_no_file_actions() {
    let mut rt = runtime(demo::preview_held);
    assert!(rt.find("preview-download").is_none());
    let (x, y) = rt.find("preview-held").expect("the held card");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.popover.is_none());
}

/// Where a right-click at `at` puts an attachment message's menu, the longest one there is, in
/// the 1200 x 760 window.
fn message_menu_at(at: (i32, i32)) -> [f32; 4] {
    let rt = runtime(|s| {
        demo::attachment_message(s);
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: "$attachment".into(),
            at: Some(at),
        });
    });
    rt.presented("message-menu").expect("the menu").0
}

#[test]
fn a_message_menu_opened_near_the_bottom_should_open_upwards_inside_the_window() {
    let [x, y, w, h] = message_menu_at((600, 740));
    assert!(y >= 0.0 && y + h <= 760.0, "inside the window: {:?}", [x, y, w, h]);
    assert!((y + h - 740.0).abs() <= 1.0, "its bottom at the pointer: {:?}", [x, y, w, h]);
    assert!((x - 600.0).abs() <= 1.0, "beside the pointer: {:?}", [x, y, w, h]);
}

#[test]
fn a_message_menu_opened_near_the_top_should_open_downwards_from_the_pointer() {
    let [_, y, _, h] = message_menu_at((600, 60));
    assert!((y - 60.0).abs() <= 1.0 && y + h <= 760.0, "{:?}", (y, h));
}

#[test]
fn a_message_menu_with_no_room_above_or_below_should_shift_to_stay_inside_the_window() {
    let [_, y, _, h] = message_menu_at((600, 380));
    assert!(y >= 0.0 && y + h <= 760.0, "{:?}", (y, h));
}

#[test]
fn a_message_menu_opened_at_the_right_edge_should_open_to_the_pointers_left() {
    let [x, _, w, _] = message_menu_at((1190, 300));
    assert!((x + w - 1190.0).abs() <= 1.0, "{:?}", (x, w));
}

#[test]
fn a_message_menu_with_no_room_beside_the_pointer_should_shift_to_stay_inside_the_window() {
    // As narrow as a small window zoomed in: 300 points fit on neither side of the pointer.
    let mut state = State::detached();
    demo::seed(&mut state);
    demo::attachment_message(&mut state);
    state.popover = Some(crate::state::Popover::MessageMenu {
        key: TimelineKey::room(demo::DEMO_THREAD.0),
        event_id: "$attachment".into(),
        at: Some((200, 300)),
    });
    let mut rt = Runtime::new(state, super::view, (420.0, 760.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    let [x, _, w, _] = rt.presented("message-menu").expect("the menu").0;
    assert!(x >= 0.0 && x + w <= 420.0, "{:?}", (x, w));
}

#[test]
fn a_message_menu_opened_where_the_window_no_longer_reaches_should_stay_inside_it() {
    // Opened at x 1170 in a wider window that narrowed to 720 points since.
    let mut state = State::detached();
    demo::seed(&mut state);
    demo::attachment_message(&mut state);
    state.popover = Some(crate::state::Popover::MessageMenu {
        key: TimelineKey::room(demo::DEMO_THREAD.0),
        event_id: "$attachment".into(),
        at: Some((1170, 300)),
    });
    let mut rt = Runtime::new(state, super::view, (720.0, 760.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    let [x, _, w, _] = rt.presented("message-menu").expect("the menu").0;
    assert!(x >= 0.0 && x + w <= 720.0, "{:?}", (x, w));
}

#[test]
fn an_attachment_menu_opened_at_the_bottom_corner_should_stay_inside_the_window() {
    let rt = runtime(|s| {
        demo::attachment_preview(s);
        s.popover = Some(crate::state::Popover::AttachmentMenu { at: (1195, 755) });
    });
    let [x, y, w, h] = rt.presented("attachment-menu").expect("the menu").0;
    assert!(x >= 0.0 && y >= 0.0 && x + w <= 1200.0 && y + h <= 760.0, "{:?}", [x, y, w, h]);
}

#[test]
fn a_held_back_pictures_message_menu_should_offer_no_file_actions() {
    let menu = |s: &mut State| {
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: "$attachment".into(),
            at: Some((600, 200)),
        });
    };
    let rt = runtime(|s| {
        demo::hidden_media(s);
        menu(s);
    });
    assert!(rt.find("message-menu").is_some());
    for key in ["attachment-copy-image", "attachment-open", "attachment-download"] {
        assert!(rt.find(key).is_none(), "{key} offered while held");
    }
    let rt = runtime(|s| {
        demo::hidden_media(s);
        s.revealed_media.insert((AccountId::new(demo::ADA), "$attachment".into()));
        menu(s);
    });
    assert!(rt.find("attachment-download").is_some());
}

#[test]
fn a_kept_timeline_should_draw_no_messages_from_someone_ignored_since() {
    let mut rt = runtime(demo::attachment_message);
    assert!(rt.find("msg-attachment").is_some());
    let safety = matrix_core::Safety { ignored: vec!["@grace:example.org".into()], ..matrix_core::Safety::default() };
    rt.update(|s, tx| s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, tx));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("msg-attachment").is_none());
    // The stored entry stays, for when they are no longer ignored.
    let key = TimelineKey::room(demo::DEMO_THREAD.0);
    assert_eq!(rt.state.active_account().unwrap().timeline(&key).map(|t| t.entries.len()), Some(1));
}

#[test]
fn a_held_back_video_without_a_poster_should_offer_show_on_its_file_card() {
    let video = matrix_core::Attachment {
        kind: matrix_core::AttachmentKind::Video,
        name: "clip.mp4".into(),
        size: None,
        media: Some(matrix_core::MediaRef::opaque("clip")),
        thumbnail: None,
        dimensions: None,
        audio: None,
    };
    let mut rt = runtime(move |s| {
        demo::image_message(s, video);
        let safety = matrix_core::Safety { media: matrix_core::ShowMedia::Never, ..matrix_core::Safety::default() };
        s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, &mut rune::Tx::default());
    });
    assert!(rt.find("show-media-$attachment").is_some());
    // The card itself does nothing while held: neither shows nor opens it.
    let (x, y) = rt.find("file-clip.mp4").expect("the file card");
    rt.pointer_down(x - 100.0, y);
    rt.pointer_up(x - 100.0, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.preview.is_none() && rt.state.revealed_media.is_empty());
    press(&mut rt, "show-media-$attachment");
    assert!(rt.find("show-media-$attachment").is_none());
    press(&mut rt, "file-clip.mp4");
    assert!(rt.state.preview.is_some());
}

#[test]
fn the_threads_panel_should_leave_out_threads_of_people_the_user_ignores() {
    let thread = format!("listed-thread-{}", demo::DEMO_THREAD.1);
    let open = |s: &mut State| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_side(SidePanel::Threads);
        demo::room_history(s);
    };
    let rt = runtime(open);
    assert!(rt.find(&thread).is_some());
    let rt = runtime(move |s| {
        open(s);
        let safety =
            matrix_core::Safety { ignored: vec!["@alan:example.org".into()], ..matrix_core::Safety::default() };
        s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, &mut rune::Tx::default());
    });
    assert!(rt.find(&thread).is_none());
}

/// The hidden-media scene with Grace's message linking a page whose card has a picture.
fn held_link_card(s: &mut State) {
    demo::hidden_media(s);
    let account = AccountId::new(demo::ADA);
    let message = Message {
        event_id: "$link".into(),
        sender_id: "@grace:example.org".into(),
        sender_name: "Grace Hopper".into(),
        timestamp_ms: 1,
        content: MessageContent::Text { body: "Look: https://x.org/a".into(), html: None },
        ..Message::default()
    };
    let entry = TimelineEntry { id: "link".into(), kind: EntryKind::Message(Box::new(message)) };
    s.apply(
        Event::TimelineChanged {
            account: account.clone(),
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(vec![entry])],
        },
        &mut rune::Tx::default(),
    );
    let preview = matrix_core::links::LinkPreview {
        title: Some("A page".into()),
        image: Some((matrix_core::MediaRef::opaque("page-image"), None)),
        ..Default::default()
    };
    let event = Event::LinkPreview { account, url: "https://x.org/a".into(), preview: Some(Box::new(preview)) };
    s.apply(event, &mut rune::Tx::default());
}

#[test]
fn a_held_back_link_card_image_should_say_show_and_reveal_without_opening_the_link() {
    let mut rt = runtime(held_link_card);
    let thumb = rt.presented("link-$link-hidden").expect("the held-back image").0;
    let inside = |rect: [f32; 4]| {
        rect[0] >= thumb[0] && rect[1] >= thumb[1] && rect[0] <= thumb[0] + thumb[2] && rect[1] <= thumb[1] + thumb[3]
    };
    assert!(
        rt.elements().iter().any(|element| element.text.as_deref() == Some("Show") && inside(element.rect)),
        "the placeholder says Show"
    );
    rt.take_url_requests();
    press(&mut rt, "link-$link-hidden");
    assert!(rt.state.revealed_media.contains(&(AccountId::new(demo::ADA), "$link".to_owned())));
    assert!(rt.take_url_requests().is_empty(), "showing the image doesn't open the link");
}

#[test]
fn search_should_leave_out_messages_of_people_the_user_ignores_found_before_too() {
    let found = |rt: &Runtime<State>, sender: &str| {
        rt.state
            .search
            .messages
            .iter()
            .filter(|hit| hit.sender == sender)
            .any(|hit| rt.find(&format!("found-message-{}-{}", hit.room_id, hit.event_id)).is_some())
    };
    let rt = runtime(demo::search);
    let sender = rt.state.search.messages.first().map(|hit| hit.sender.clone()).expect("a message hit");
    assert!(found(&rt, &sender));
    let mut rt = runtime(demo::search);
    let safety = matrix_core::Safety { ignored: vec![sender.clone()], ..matrix_core::Safety::default() };
    rt.update(|s, tx| s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, tx));
    rt.frame(1.0 / 60.0);
    assert!(!rt.state.search.messages.is_empty(), "the results held are kept");
    assert!(!found(&rt, &sender));
}

#[test]
fn hidden_events_should_show_only_while_turned_on() {
    let shown = runtime(demo::hidden_events);
    assert!(shown.find("hidden-acl").is_some(), "a line for the server ACL change");
    let left_out = runtime(|s| {
        demo::hidden_events(s);
        s.set_show_hidden_events(false);
    });
    assert!(left_out.find("hidden-acl").is_none());
}

#[test]
fn a_hidden_events_source_button_should_ask_for_its_source() {
    let mut rt = runtime(demo::hidden_events);
    press(&mut rt, "hidden-acl-source");
    assert!(matches!(rt.state.modal, Some(crate::state::Modal::Source(crate::state::SourceView::Loading(_)))));
}

#[test]
fn the_message_menu_should_offer_the_messages_source() {
    let mut rt = runtime(|s| {
        demo::pinned_messages(s);
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: demo::DEMO_THREAD.1.into(),
            at: Some((600, 280)),
        });
    });
    press(&mut rt, "message-source");
    assert!(matches!(rt.state.modal, Some(crate::state::Modal::Source(_))));
}

#[test]
fn adding_a_widget_should_ask_for_its_name_and_address() {
    let mut rt = runtime(demo::widgets);
    press(&mut rt, "widget-add");
    assert!(rt.find("widget-form-name").is_some() && rt.find("widget-form-url").is_some());
}

#[test]
fn the_widgets_panel_should_list_widgets_and_ask_before_removing_one() {
    let mut rt = runtime(demo::widgets);
    assert!(rt.find("widget-open-board").is_some());
    assert!(rt.find("widget-open-standup").is_some());
    press(&mut rt, "widget-remove-board");
    assert!(matches!(rt.state.modal, Some(crate::state::Modal::Confirm(crate::state::Confirm::RemoveWidget { .. }))));
}

#[test]
fn the_source_dialog_should_show_both_the_decrypted_and_the_sent_event() {
    let rt = runtime(demo::event_source);
    assert!(rt.find("source-dialog").is_some());
    assert!(rt.find("source-copy").is_some());
}

#[test]
fn choosing_chat_bubbles_in_the_appearance_settings_should_lay_messages_out_in_bubbles() {
    let mut rt = runtime(|s| {
        s.open_settings();
        s.show_settings_section(SettingsSection::Appearance);
    });
    press(&mut rt, "message-layout-bubbles");
    assert_eq!(rt.state.settings.message_layout, crate::state::MessageLayout::Bubbles);
}

#[test]
fn choosing_typed_emoticons_in_the_appearance_settings_should_keep_them_as_typed() {
    let mut rt = runtime(|s| {
        s.open_settings();
        s.show_settings_section(SettingsSection::Appearance);
    });
    press(&mut rt, "emoticons-typed");
    assert_eq!(rt.state.settings.emoticons, crate::state::Emoticons::AsTyped);
}

#[test]
fn choosing_monochrome_in_the_emoticons_settings_should_change_emoji_style() {
    let mut rt = runtime(|s| {
        s.open_settings();
        s.show_settings_section(SettingsSection::Appearance);
    });
    rt.update(|_, tx| tx.scroll_to_instant("settings-body", 0.0, 200.0));
    rt.frame(1.0 / 60.0);
    press(&mut rt, "emoji-style-monochrome");
    assert_eq!(rt.state.settings.emoji_style, crate::state::EmojiStyle::Monochrome);
}

#[test]
fn a_group_tab_in_the_emoji_picker_should_scroll_to_that_group() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_popover(crate::state::Popover::Emoji(crate::state::EmojiFor::Composer(
            crate::state::ComposerTarget::Room,
        )));
    });
    assert!(rt.find("emoji-group-flags").is_none(), "the flags come last");
    press(&mut rt, "emoji-tab-flags");
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    let top = |key: &str| rt.presented(key).map(|(rect, _)| rect[1]).unwrap_or_else(|| panic!("no {key}"));
    assert!((top("emoji-group-flags") - top("emoji-grid")).abs() < 1.0, "the flags' title at the grid's top");
}

#[test]
fn custom_emoji_arriving_in_an_open_picker_should_show_at_its_top() {
    let mut rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_popover(crate::state::Popover::Emoji(crate::state::EmojiFor::Composer(
            crate::state::ComposerTarget::Room,
        )));
    });
    rt.update(|s, _| demo::image_packs(s));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    let top = |key: &str| rt.presented(key).map(|(rect, _)| rect[1]).unwrap_or_else(|| panic!("no {key}"));
    assert!((top("emoji-custom-title") - top("emoji-grid-custom")).abs() < 1.0, "the custom emoji lead");
}

#[test]
fn classic_rows_should_hold_messages_in_no_bubble() {
    let rt = runtime(|s| short_room(s, 1));
    assert!(rt.find("msg-short-0-bubble").is_none());
    assert!(rt.find("msg-short-0-sender-face").is_some());
}

/// [`short_room`] in chat bubbles, with Grace's message followed by one of Ada's own.
fn bubble_room(s: &mut State) {
    s.settings.message_layout = crate::state::MessageLayout::Bubbles;
    s.select_room(demo::DEMO_THREAD.0);
    let mut own = short_message("own", 1);
    if let EntryKind::Message(message) = &mut own.kind {
        message.sender_id = demo::ADA.into();
        message.sender_name = "Ada Lovelace".into();
    }
    s.apply(
        Event::TimelineChanged {
            account: AccountId::new(demo::ADA),
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(vec![short_message("short-0", 0), own])],
        },
        &mut rune::Tx::default(),
    );
}

#[test]
fn in_bubbles_other_peoples_messages_should_sit_at_the_left_beside_their_picture() {
    let rt = runtime(bubble_room);
    let face = element_rect(&rt, "msg-short-0-sender-face");
    let bubble = element_rect(&rt, "msg-short-0-bubble");
    let own = element_rect(&rt, "msg-own-bubble");
    assert!(bubble[0] > face[0] + face[2] - 1.0, "the bubble follows the picture");
    assert!(bubble[0] + bubble[2] < own[0] + own[2], "it ends short of the user's own");
}

#[test]
fn in_bubbles_the_users_own_messages_should_sit_at_the_right_without_a_picture() {
    let rt = runtime(bubble_room);
    let other = element_rect(&rt, "msg-short-0-bubble");
    let own = element_rect(&rt, "msg-own-bubble");
    let row = element_rect(&rt, "msg-own");
    assert!(rt.find("msg-own-sender-face").is_none());
    assert!(rt.find("msg-own-sender-name").is_none());
    assert!(own[0] > other[0] + other[2] / 2.0, "the user's own bubble starts right of the other's");
    assert!(own[0] > row[0] + row[2] / 2.0, "and sits in the timeline's right half");
}

#[test]
fn pressing_a_bubble_beside_its_text_should_open_its_thread() {
    let mut rt = runtime(bubble_room);
    // In the bubble's padding, under the words and left of the time: the text keeps its own
    // presses for selecting.
    let bubble = element_rect(&rt, "msg-own-bubble");
    let (x, y) = (bubble[0] + 4.0, bubble[1] + bubble[3] - 3.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.active_thread(), Some("$own"));
}

#[test]
fn in_bubbles_the_toolbar_of_the_users_own_message_should_sit_just_left_of_its_bubble() {
    let rt = runtime(bubble_room);
    let toolbar = element_rect(&rt, "actions-$own");
    let own = element_rect(&rt, "msg-own-bubble");
    let gap = own[0] - (toolbar[0] + toolbar[2]);
    assert!((gap - super::timeline::TOOLBAR_BESIDE).abs() < 1.0, "beside the bubble, {gap} points off");
    assert!(toolbar[1] >= own[1] - 1.0, "level with its top, not above it");
}

#[test]
fn in_bubbles_the_toolbar_of_someone_elses_message_should_sit_just_right_of_its_bubble() {
    let rt = runtime(bubble_room);
    let toolbar = element_rect(&rt, "actions-$short-0");
    let bubble = element_rect(&rt, "msg-short-0-bubble");
    let gap = toolbar[0] - (bubble[0] + bubble[2]);
    assert!((gap - super::timeline::TOOLBAR_BESIDE).abs() < 1.0, "beside the bubble, {gap} points off");
}

#[test]
fn a_wide_bubble_toolbar_should_keep_every_button() {
    use super::timeline::{ToolbarPlace, beside_toolbar};
    assert_eq!(beside_toolbar(400.0), ToolbarPlace::Beside { kept: super::timeline::FIXED_BUTTONS });
}

#[test]
fn a_narrower_bubble_toolbar_should_lose_buttons_before_more() {
    use super::timeline::{TOOLBAR_BESIDE, ToolbarPlace, beside_toolbar, toolbar_width};
    let room = |buttons| TOOLBAR_BESIDE + toolbar_width(buttons);
    assert_eq!(beside_toolbar(room(3)), ToolbarPlace::Beside { kept: 2 });
    assert_eq!(beside_toolbar(room(2)), ToolbarPlace::Beside { kept: 1 });
    assert_eq!(beside_toolbar(room(1)), ToolbarPlace::Beside { kept: 0 });
}

#[test]
fn a_bubble_toolbar_without_room_beside_should_keep_only_more_at_the_rows_corner() {
    use super::timeline::{TOOLBAR_BESIDE, ToolbarPlace, beside_toolbar, toolbar_width};
    assert_eq!(beside_toolbar(TOOLBAR_BESIDE + toolbar_width(1) - 1.0), ToolbarPlace::Row { kept: 0 });
}

/// Windows where the room beside [`bubble_room`]'s first bubble holds React, Reply and More, and
/// where the bubble leaves no room beside it.
const NARROW_BUBBLES: f32 = 640.0;
const COMPACT_BUBBLES: f32 = 400.0;

/// [`bubble_room`] in a window `width` points wide.
fn narrow_bubble_room(width: f32) -> Runtime<State> {
    let mut state = State::detached();
    demo::seed(&mut state);
    bubble_room(&mut state);
    let mut rt = Runtime::new(state, super::view, (width, 760.0), Env::new(true, 1.0));
    rt.frame(0.0);
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

#[test]
fn in_a_narrow_timeline_a_bubbles_toolbar_should_drop_thread_first() {
    let rt = narrow_bubble_room(NARROW_BUBBLES);
    assert!(rt.find("thread-$short-0").is_none(), "no room for Thread");
    assert!(rt.find("reply-$short-0").is_some() && rt.find("more-$short-0").is_some(), "Reply and More stay");
}

#[test]
fn in_a_compact_timeline_a_bubbles_toolbar_should_hold_only_more() {
    let rt = narrow_bubble_room(COMPACT_BUBBLES);
    for button in ["react", "reply", "thread"] {
        assert!(rt.find(&format!("{button}-$short-0")).is_none(), "no {button}");
    }
    assert!(rt.find("more-$short-0").is_some(), "More stays");
}

#[test]
fn adding_a_reaction_from_the_menu_should_open_the_picker_at_more_when_react_is_gone() {
    let mut rt = narrow_bubble_room(COMPACT_BUBBLES);
    rt.update(|s, _| {
        s.popover = Some(crate::state::Popover::MessageMenu {
            key: TimelineKey::room(demo::DEMO_THREAD.0),
            event_id: "$short-0".into(),
            at: None,
        })
    });
    rt.frame(1.0 / 60.0);
    press(&mut rt, "message-react");
    assert!(matches!(
        &rt.state.popover,
        Some(crate::state::Popover::Emoji(crate::state::EmojiFor::Reaction { event_id, .. })) if event_id == "$short-0"
    ));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("emoji-picker").is_some(), "the picker shows");
}

#[test]
fn in_bubbles_the_users_own_voice_message_should_sit_at_the_right() {
    use matrix_core::{Attachment, AttachmentKind, AudioDetails, MediaRef};

    let rt = runtime(|s| {
        bubble_room(s);
        let mut voice = short_message("voice", 2);
        if let EntryKind::Message(message) = &mut voice.kind {
            message.sender_id = demo::ADA.into();
            message.content = MessageContent::Attachment(Attachment {
                kind: AttachmentKind::Audio,
                name: "memo.ogg".into(),
                size: None,
                media: Some(MediaRef::opaque("memo")),
                thumbnail: None,
                dimensions: None,
                audio: Some(AudioDetails { voice: true, duration_ms: Some(4_000), waveform: vec![500; 40] }),
            });
        }
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::PushBack(voice)],
            },
            &mut rune::Tx::default(),
        );
    });
    let card = element_rect(&rt, "audio-$voice");
    let text = element_rect(&rt, "msg-own-bubble");
    assert!((card[0] + card[2] - (text[0] + text[2])).abs() < 1.0, "the card ends where the user's bubbles do");
}

#[test]
fn bubbles_should_name_senders_in_a_direct_chat_a_third_person_joined() {
    let mut room = demo_dm();
    room.member_count = 3;
    assert!(super::timeline::names_sender(&room, "@grace:example.org"));
}

#[test]
fn bubbles_should_not_name_the_other_person_of_a_one_to_one_chat() {
    assert!(!super::timeline::names_sender(&demo_dm(), "@grace:example.org"));
}

#[test]
fn bubbles_should_name_someone_who_left_a_direct_chat() {
    assert!(super::timeline::names_sender(&demo_dm(), "@alan:example.org"));
}

/// The demo's one-to-one chat with Grace.
fn demo_dm() -> matrix_core::RoomSummary {
    let mut s = State::detached();
    demo::seed(&mut s);
    s.accounts.get(&AccountId::new(demo::ADA)).and_then(|a| a.room("!dm-grace:example.org")).unwrap().clone()
}

#[test]
fn in_bubbles_the_caption_of_the_users_own_gif_should_keep_to_the_gifs_side() {
    let rt = runtime(|s| {
        bubble_room(s);
        let mut gif = short_message("gif", 2);
        if let EntryKind::Message(message) = &mut gif.kind {
            message.sender_id = demo::ADA.into();
            message.content = MessageContent::Text { body: "Ha https://x.org/a.gif".into(), html: None };
        }
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::PushBack(gif)],
            },
            &mut rune::Tx::default(),
        );
    });
    let gif = element_rect(&rt, "gif-https://x.org/a.gif");
    let caption = element_rect(&rt, "text-$gif");
    // The words' box reaches `REACH` past them on either side.
    let words_end = caption[0] + caption[2] - super::message_text::REACH;
    assert!((words_end - (gif[0] + gif[2])).abs() < 1.0, "the caption ends where the GIF does");
}

#[test]
fn in_bubbles_a_formatted_caption_of_the_users_own_gif_should_keep_to_the_gifs_side() {
    let rt = runtime(|s| {
        bubble_room(s);
        let mut gif = short_message("gif", 2);
        if let EntryKind::Message(message) = &mut gif.kind {
            message.sender_id = demo::ADA.into();
            message.content = MessageContent::Text {
                body: "Ha https://x.org/a.gif".into(),
                html: Some("<b>Ha</b> https://x.org/a.gif".into()),
            };
        }
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::PushBack(gif)],
            },
            &mut rune::Tx::default(),
        );
    });
    let gif = element_rect(&rt, "gif-https://x.org/a.gif");
    let caption = element_rect(&rt, "rich-$gif");
    assert!(
        caption[0] > gif[0],
        "the caption starts within the GIF's span, not at the row's left: {caption:?} {gif:?}"
    );
}

#[test]
fn in_bubbles_the_users_own_card_should_be_no_wider_than_a_classic_rows() {
    let rt = runtime(|s| {
        bubble_room(s);
        let mut poll = short_message("poll", 2);
        if let EntryKind::Message(message) = &mut poll.kind {
            message.sender_id = demo::ADA.into();
            message.content = MessageContent::Poll(matrix_core::Poll {
                question: "Lunch?".into(),
                answers: vec![
                    matrix_core::PollAnswer { id: "a".into(), text: "Yes".into(), votes: 0, mine: false },
                    matrix_core::PollAnswer { id: "b".into(), text: "No".into(), votes: 0, mine: false },
                ],
                ..matrix_core::Poll::default()
            });
        }
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::PushBack(poll)],
            },
            &mut rune::Tx::default(),
        );
    });
    let own = element_rect(&rt, "msg-poll-bubble");
    let face = element_rect(&rt, "msg-short-0-sender-face");
    assert!(own[0] >= face[0] + face[2] - 1.0, "the card leaves the picture's gutter free");
}

#[test]
fn in_bubbles_the_picture_of_an_unnamed_sender_should_take_keyboard_focus() {
    let rt = runtime(|s| {
        s.settings.message_layout = crate::state::MessageLayout::Bubbles;
        s.select_room("!dm-grace:example.org");
    });
    let faces: Vec<_> = rt
        .elements()
        .into_iter()
        .filter(|e| e.key.as_deref().is_some_and(|key| key.ends_with("-sender-face")))
        .collect();
    assert!(!faces.is_empty() && faces.iter().all(|face| face.focusable));
}

#[test]
fn every_settings_section_should_be_reachable_in_the_smallest_window_at_a_large_size() {
    // The smallest window, 720 by 480 points, at 125% zoom.
    let mut rt = runtime(State::open_settings);
    rt.set_viewport((576.0, 384.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    let nav = rt.presented("settings-nav").expect("the sections").0;
    rt.wheel(nav[0] + nav[2] / 2.0, nav[1] + nav[3] / 2.0, -400.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    let info = rt.presented("settings-info").expect("Info").0;
    let modal = rt.presented("settings-modal").expect("the dialog").0;
    assert!(
        info[1] + info[3] <= nav[1] + nav[3] + 0.5,
        "Info scrolls into the dialog: info {info:?} nav {nav:?} modal {modal:?}"
    );
    press(&mut rt, "settings-info");
    assert_eq!(rt.state.modal, Some(crate::state::Modal::Settings(SettingsSection::Info)));
}

#[test]
fn in_bubbles_a_compact_timeline_should_keep_the_time_inside_the_bubble() {
    // The smallest window, 720 by 480 points, at 150% zoom: a compact timeline.
    let mut rt = runtime(|s| {
        s.settings.message_layout = crate::state::MessageLayout::Bubbles;
        s.select_room("!dm-grace:example.org");
    });
    rt.set_viewport((480.0, 320.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let timeline = rt.presented("timeline").expect("the timeline").0;
    let bubbles: Vec<_> = rt
        .elements()
        .into_iter()
        .filter_map(|e| Some((e.key.clone()?, e.rect)))
        .filter(|(key, _)| key.ends_with("-bubble"))
        .collect();
    assert!(!bubbles.is_empty(), "the timeline shows bubbles");
    for (key, bubble) in bubbles {
        let message = key.trim_end_matches("-bubble");
        let Some(time) = rt.presented(&format!("{message}-time")) else { continue };
        let time = time.0;
        assert!(
            time[0] >= bubble[0] - 0.5 && time[0] + time[2] <= bubble[0] + bubble[2] + 0.5,
            "{message}: the time {time:?} leaves its bubble {bubble:?} in a timeline {timeline:?}"
        );
    }
}

#[test]
fn in_bubbles_a_thread_link_should_stay_within_a_narrow_timeline() {
    let mut rt = runtime(|s| {
        s.settings.message_layout = crate::state::MessageLayout::Bubbles;
        s.select_room(demo::DEMO_THREAD.0);
    });
    rt.set_viewport((640.0, 480.0), 1.0);
    rt.frame(1.0 / 60.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let timeline = rt.presented("timeline").expect("the timeline").0;
    let link = rt.presented(&format!("thread-link-{}", demo::DEMO_THREAD.1)).expect("the thread link").0;
    assert!(link[0] + link[2] <= timeline[0] + timeline[2], "the link {link:?} ends past the timeline {timeline:?}");
}

#[test]
fn in_bubbles_wrapped_reactions_on_the_users_own_message_should_keep_to_the_right() {
    let rt = runtime(|s| {
        s.settings.message_layout = crate::state::MessageLayout::Bubbles;
        s.select_room(demo::DEMO_THREAD.0);
        let mut own = short_message("own", 1);
        if let EntryKind::Message(message) = &mut own.kind {
            message.sender_id = demo::ADA.into();
            message.reactions =
                (0..24).map(|i| matrix_core::Reaction { key: format!(":r{i}:"), count: 1, mine: false }).collect();
        }
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::Reset(vec![own])],
            },
            &mut rune::Tx::default(),
        );
    });
    let bubble = element_rect(&rt, "msg-own-bubble");
    let last = element_rect(&rt, "reaction-$own-:r23:");
    let first = element_rect(&rt, "reaction-$own-:r0:");
    assert!(last[1] > first[1], "the chips wrap");
    assert!((last[0] + last[2] - (bubble[0] + bubble[2])).abs() < 1.0, "the last line ends where the bubble does");
}

#[test]
fn stop_ignoring_should_show_their_messages_only_once_saved() {
    let mut rt = runtime(|s| {
        demo::attachment_message(s);
        // Grace's message is the room's newest: its preview, once shown.
        let message = Message {
            event_id: "$late".into(),
            sender_id: "@grace:example.org".into(),
            sender_name: "Grace Hopper".into(),
            timestamp_ms: u64::MAX / 2,
            content: MessageContent::Text { body: "Still here".into(), html: None },
            ..Message::default()
        };
        let entry = TimelineEntry { id: "late".into(), kind: EntryKind::Message(Box::new(message)) };
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::Reset(vec![entry])],
            },
            &mut rune::Tx::default(),
        );
        let safety =
            matrix_core::Safety { ignored: vec!["@grace:example.org".into()], ..matrix_core::Safety::default() };
        s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, &mut rune::Tx::default());
    });
    let preview = |rt: &Runtime<State>| {
        rt.state.active_account().unwrap().last_message(demo::DEMO_THREAD.0).map(|m| m.event_id.clone())
    };
    let settle = |rt: &mut Runtime<State>| {
        for _ in 0..10 {
            rt.frame(1.0 / 60.0);
        }
    };
    assert!(rt.find("msg-late").is_none());
    let ada = AccountId::new(demo::ADA);
    let owner = ada.clone();
    rt.update(move |s, _| s.stop_ignoring(&owner, "@grace:example.org"));
    settle(&mut rt);
    assert!(rt.find("msg-late").is_none(), "still hidden while Stop ignoring saves");
    assert_ne!(preview(&rt).as_deref(), Some("$late"));
    // Saved: the engine reports the list without them.
    rt.update(move |s, tx| s.apply(Event::Safety { account: ada, safety: matrix_core::Safety::default() }, tx));
    settle(&mut rt);
    assert!(rt.find("msg-late").is_some());
    assert_eq!(preview(&rt).as_deref(), Some("$late"));
}

#[test]
fn held_back_media_in_bubbles_should_show_a_placeholder_until_show_is_pressed() {
    let mut rt = runtime(|s| {
        demo::hidden_media(s);
        s.settings.message_layout = crate::state::MessageLayout::Bubbles;
    });
    assert!(rt.find("hidden-media-$attachment").is_some());
    assert!(rt.find("image-better-matrix.png").is_none());
    press(&mut rt, "show-media-$attachment");
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("hidden-media-$attachment").is_none());
    assert!(rt.find("image-better-matrix.png").is_some());
}

#[test]
fn bubbles_should_draw_no_messages_from_someone_ignored() {
    let mut rt = runtime(|s| {
        demo::attachment_message(s);
        s.settings.message_layout = crate::state::MessageLayout::Bubbles;
    });
    assert!(rt.find("msg-attachment").is_some());
    let safety = matrix_core::Safety { ignored: vec!["@grace:example.org".into()], ..matrix_core::Safety::default() };
    rt.update(|s, tx| s.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, tx));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rt.find("msg-attachment").is_none());
}

#[test]
fn a_narrow_link_card_should_keep_its_held_back_picture_under_the_words() {
    let mut state = State::detached();
    demo::seed(&mut state);
    held_link_card(&mut state);
    state.settings.message_layout = crate::state::MessageLayout::Bubbles;
    let safety = matrix_core::Safety { media: matrix_core::ShowMedia::Never, ..matrix_core::Safety::default() };
    state.apply(Event::Safety { account: AccountId::new(demo::ADA), safety }, &mut rune::Tx::default());
    let mut rt = Runtime::new(state, super::view, (620.0, 760.0), Env::new(true, 1.0));
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    let card = rt.presented("link-$link").expect("the card").0;
    let thumb = rt.presented("link-$link-hidden").expect("Show, even when narrow").0;
    assert!(thumb[1] > card[1] + 20.0, "under the words, not beside them");
    press(&mut rt, "link-$link-hidden");
    assert!(rt.state.revealed_media.contains(&(AccountId::new(demo::ADA), "$link".to_owned())));
}

#[test]
fn a_held_back_link_card_image_that_failed_should_still_offer_show_and_try_again() {
    let page = matrix_core::MediaRef::opaque("page-image");
    let failed = page.clone();
    let mut rt = runtime(move |s| {
        held_link_card(s);
        s.media.failed(failed, std::time::Instant::now());
    });
    assert!(rt.state.media.has_failed(&page));
    assert!(rt.find("link-$link-hidden").is_some(), "Show stays");
    press(&mut rt, "link-$link-hidden");
    assert!(rt.state.revealed_media.contains(&(AccountId::new(demo::ADA), "$link".to_owned())));
    assert!(!rt.state.media.has_failed(&page), "tried again");
}

#[test]
fn a_held_back_gif_that_failed_should_still_offer_show_and_try_again() {
    let url = "https://x.org/party.gif";
    let mut rt = runtime(move |s| {
        demo::hidden_media(s);
        let message = Message {
            event_id: "$gif".into(),
            sender_id: "@grace:example.org".into(),
            sender_name: "Grace Hopper".into(),
            timestamp_ms: 1,
            content: MessageContent::Text { body: url.into(), html: None },
            ..Message::default()
        };
        let entry = TimelineEntry { id: "gif".into(), kind: EntryKind::Message(Box::new(message)) };
        s.apply(
            Event::TimelineChanged {
                account: AccountId::new(demo::ADA),
                key: TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![TimelineDiff::Reset(vec![entry])],
            },
            &mut rune::Tx::default(),
        );
        s.gifs.failed(url, std::time::Instant::now());
    });
    assert!(rt.state.gifs.has_failed(url));
    assert!(rt.find("hidden-media-$gif").is_some(), "Show stays");
    press(&mut rt, "show-media-$gif");
    assert!(rt.state.revealed_media.contains(&(AccountId::new(demo::ADA), "$gif".to_owned())));
    assert!(!rt.state.gifs.has_failed(url), "tried again");
}

// ----- verification, account actions, linking devices, signing up ------------------------

fn grace_identity(verified: bool, changed: bool) -> Option<matrix_core::Identity> {
    Some(matrix_core::Identity { verified, changed })
}

#[test]
fn a_verification_ready_for_a_code_should_show_it_to_scan() {
    let rt = runtime(demo::verify_person_qr);
    assert!(rt.find("verify-qr").is_some() && rt.find("verify-emoji").is_some());
}

#[test]
fn the_verification_code_should_be_square_and_fit_its_dialog() {
    let rt = runtime(demo::verify_person_qr);
    let code = rt.presented("verify-qr").unwrap().0;
    let dialog = rt.presented("verify-dialog").unwrap().0;
    assert!((code[2] - code[3]).abs() < 0.5, "{code:?}");
    assert!(code[0] >= dialog[0] && code[0] + code[2] <= dialog[0] + dialog[2], "{code:?} in {dialog:?}");
}

#[test]
fn a_verified_person_should_show_the_badge_and_no_verify_button() {
    let rt = runtime(|s| demo::person(s, grace_identity(true, false)));
    let shown = rt.elements().into_iter().filter_map(|e| e.text).any(|text| text == "Verified");
    assert!(shown && rt.find("person-verify").is_none());
}

#[test]
fn an_unverified_person_should_offer_to_verify_them() {
    let mut rt = runtime(|s| demo::person(s, grace_identity(false, false)));
    press(&mut rt, "person-verify");
    assert!(matches!(
        &rt.state.modal,
        Some(crate::state::Modal::Verification { with: Some(person), .. }) if person.id == "@grace:example.org"
    ));
}

#[test]
fn a_person_whose_identity_changed_should_be_warned_about() {
    let rt = runtime(|s| demo::person(s, grace_identity(true, true)));
    assert!(rt.find("person-identity").is_some() && rt.find("person-verify").is_some());
}

#[test]
fn a_person_without_encryption_should_say_there_is_nothing_to_verify() {
    let rt = runtime(|s| demo::person(s, None));
    assert!(rt.find("person-cannot-verify").is_some() && rt.find("person-verify").is_none());
}

#[test]
fn the_account_page_should_offer_the_offline_device_keys_password_and_deactivation() {
    let rt = runtime(|s| s.open_account_settings());
    for key in ["offline-device-on", "keys-export", "keys-import", "password-change", "deactivate-account"] {
        assert!(rt.find(key).is_some(), "no {key}");
    }
}

#[test]
fn an_offline_device_the_server_does_not_keep_should_not_show() {
    let rt = runtime(|s| {
        let security = matrix_core::Security {
            session_verified: true,
            recovery: matrix_core::RecoveryStatus::Enabled,
            offline_device: matrix_core::OfflineDevice::Unsupported,
        };
        s.apply(Event::SecurityChanged { account: AccountId::new(demo::ADA), security }, &mut rune::Tx::default());
        s.open_account_settings();
    });
    assert!(rt.find("offline-device-on").is_none() && rt.find("offline-device-off").is_none());
}

#[test]
fn turning_the_offline_device_on_should_ask_for_the_recovery_key() {
    let mut rt = runtime(|s| s.open_account_settings());
    press(&mut rt, "offline-device-on");
    assert!(rt.find("recovery-key").is_some() && rt.find("recovery-forgot").is_none());
}

#[test]
fn deactivating_should_confirm_with_a_choice_to_erase_messages() {
    let mut rt = runtime(|s| {
        s.open_account_settings();
        s.ask_to_deactivate();
    });
    press(&mut rt, "deactivate-erase");
    assert!(matches!(
        rt.state.modal,
        Some(crate::state::Modal::Confirm(crate::state::Confirm::Deactivate { erase: true, .. }))
    ));
}

#[test]
fn an_empty_new_password_should_show_why_it_was_not_sent() {
    let mut rt = runtime(|s| {
        s.open_account_settings();
        s.open_change_password();
    });
    press(&mut rt, "password-change-submit");
    assert!(rt.find("dialog-error").is_some());
}

#[test]
fn the_sessions_page_of_a_verified_oauth_account_should_link_a_new_device() {
    let mut rt = runtime(|s| {
        demo::link_device(s);
        s.modal = Some(crate::state::Modal::Settings(SettingsSection::Sessions));
    });
    press(&mut rt, "link-device");
    assert!(matches!(rt.state.modal, Some(crate::state::Modal::LinkDevice { .. })));
}

#[test]
fn a_password_account_should_not_offer_to_link_a_device() {
    let rt = runtime(|s| {
        s.open_settings();
        s.show_settings_section(SettingsSection::Sessions);
    });
    assert!(rt.find("link-device").is_none());
}

#[test]
fn linking_a_device_should_show_the_code_to_scan() {
    let rt = runtime(demo::link_device);
    assert!(rt.find("qr-login-code").is_some());
}

#[test]
fn the_approval_page_of_a_device_link_should_open_again_on_request() {
    let mut rt = runtime(demo::link_device);
    let url = "https://account.hobby.chat/device".to_owned();
    rt.update(|s, _| {
        if let Some(crate::state::Modal::LinkDevice { flow, .. }) = &mut s.modal
            && let Some(request) = flow.request
        {
            flow.moved(request, matrix_core::QrLoginStep::Approve { url: url.clone() });
        }
    });
    rt.frame(1.0 / 60.0);
    press(&mut rt, "qr-login-reopen");
    assert_eq!(rt.take_url_requests(), [url]);
}

#[test]
fn signing_in_with_a_qr_code_should_show_the_code_to_scan() {
    let rt = runtime(|s| {
        s.nav.account = None;
        s.accounts = crate::state::Accounts::default();
        demo::sign_in_qr(s);
    });
    assert!(rt.find("qr-login-code").is_some());
}

#[test]
fn signing_in_again_as_a_suspended_account_should_not_offer_a_qr_code_or_a_new_account() {
    let rt = runtime(|s| {
        s.start_adding_account();
        demo::sign_in_resume(s);
    });
    assert!(rt.find("use-qr").is_none() && rt.find("create-account").is_none());
}

#[test]
fn signing_in_to_a_new_account_should_offer_a_qr_code_and_a_new_account() {
    let rt = runtime(|s| {
        s.start_adding_account();
        demo::sign_in_browser(s);
    });
    assert!(rt.find("use-qr").is_some() && rt.find("create-account").is_some());
}

#[test]
fn an_email_address_should_answer_the_registration() {
    let mut rt = runtime(|s| {
        s.start_adding_account();
        demo::registration_email(s);
    });
    press(&mut rt, "registration-email-submit");
    assert!(matches!(
        rt.state.sign_in.step,
        crate::state::SignInStep::Register { asked: Some(crate::state::Asked { answered: true, .. }), .. }
    ));
}

#[test]
fn a_link_not_yet_opened_should_say_so() {
    let rt = runtime(|s| {
        s.start_adding_account();
        demo::registration_email_sent(s);
    });
    assert!(rt.find("registration-email-unconfirmed").is_some() && rt.find("registration-continue").is_some());
}

#[test]
fn a_policy_to_agree_to_should_open_in_the_browser() {
    let mut rt = runtime(|s| {
        s.start_adding_account();
        demo::registration_terms(s);
    });
    press(&mut rt, "policy-0");
    assert_eq!(rt.take_url_requests(), ["https://example.org/terms"]);
}

#[test]
fn agreeing_to_the_policies_should_answer_the_registration() {
    let mut rt = runtime(|s| {
        s.start_adding_account();
        demo::registration_terms(s);
    });
    press(&mut rt, "registration-agree");
    assert!(matches!(
        rt.state.sign_in.step,
        crate::state::SignInStep::Register { asked: Some(crate::state::Asked { answered: true, .. }), .. }
    ));
}

// ----- administering rooms and spaces ------------------------------------------------------------

/// Changes Ada's room `room_id` and space `space_id` before the view is built.
fn without_settings_rights(s: &mut State) {
    let account = s.active_account().unwrap();
    let id = account.summary.id.clone();
    let mut rooms = account.rooms.clone();
    let mut spaces = account.spaces.clone();
    rooms.iter_mut().for_each(|room| room.can_change_settings = false);
    spaces.iter_mut().for_each(|space| space.can_change_settings = false);
    s.accounts.set_room_list(&id, matrix_core::RoomList { rooms, spaces, ..Default::default() });
}

#[test]
fn room_settings_should_be_offered_in_the_room_info_with_the_right() {
    let rt = runtime(|s| {
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_side(SidePanel::Info);
    });
    assert!(rt.find("info-settings").is_some());
}

#[test]
fn room_settings_should_not_be_offered_without_the_right() {
    let rt = runtime(|s| {
        without_settings_rights(s);
        s.select_room(demo::DEMO_THREAD.0);
        s.toggle_side(SidePanel::Info);
    });
    assert!(rt.find("info-panel").is_some() && rt.find("info-settings").is_none());
}

#[test]
fn the_room_menu_should_offer_settings_only_with_the_right() {
    let rt = runtime(|s| s.toggle_room_menu(demo::DEMO_THREAD.0));
    assert!(rt.find("room-menu-settings").is_some());
    let rt = runtime(|s| {
        without_settings_rights(s);
        s.toggle_room_menu(demo::DEMO_THREAD.0);
    });
    assert!(rt.find("room-menu").is_some() && rt.find("room-menu-settings").is_none());
}

#[test]
fn the_space_menu_should_offer_settings_only_with_the_right() {
    let rt = runtime(|s| s.toggle_popover(crate::state::Popover::SpaceMenu(demo::WORK_SPACE.into())));
    assert!(rt.find("space-settings").is_some() && rt.find("space-rooms").is_some());
    let rt = runtime(|s| {
        without_settings_rights(s);
        s.toggle_popover(crate::state::Popover::SpaceMenu(demo::WORK_SPACE.into()));
    });
    assert!(rt.find("space-leave").is_some() && rt.find("space-settings").is_none());
}

#[test]
fn room_settings_should_show_only_the_parts_the_user_may_change() {
    let rt = runtime(|s| {
        demo::room_settings(s, crate::state::RoomSettingsSection::General);
        let settings = matrix_core::RoomSettings {
            rights: matrix_core::RoomRights { name: true, ..Default::default() },
            ..demo::general_settings()
        };
        let account = matrix_core::AccountId::new(demo::ADA);
        s.apply(Event::RoomSettings { account, settings: Box::new(settings) }, &mut rune::Tx::default());
    });
    assert!(rt.find("room-settings-name").is_some());
    for hidden in [
        "room-settings-topic",
        "room-settings-picture",
        "room-settings-access",
        "room-settings-addresses",
        "room-settings-security",
        "room-settings-permissions",
        "room-settings-advanced",
    ] {
        assert!(rt.find(hidden).is_none(), "{hidden} must not show without its right");
    }
}

#[test]
fn a_permission_above_the_users_level_should_not_open() {
    let mut rt = runtime(|s| {
        demo::room_settings(s, crate::state::RoomSettingsSection::Permissions);
        let mut settings = matrix_core::RoomSettings { own_level: 25, ..demo::general_settings() };
        // As the engine reads it at level 25: only what asks for no more is changeable.
        for shown in &mut settings.permissions {
            shown.editable = shown.level <= 25;
        }
        let account = matrix_core::AccountId::new(demo::ADA);
        s.apply(Event::RoomSettings { account, settings: Box::new(settings) }, &mut rune::Tx::default());
    });
    // Locked, it takes no press: pressed where it is drawn, nothing opens.
    let locked = rt.presented("permission-Kick").expect("the locked select").0;
    let (x, y) = (locked[0] + locked[2] / 2.0, locked[1] + locked[3] / 2.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.popover.is_none());
    press(&mut rt, "permission-Invite");
    assert!(rt.find("permission-level-0").is_some() && rt.find("permission-level-50").is_none());
}

#[test]
fn join_requests_should_show_with_their_answers() {
    let rt = runtime(|s| {
        demo::join_requests(s);
        s.toggle_members();
    });
    assert!(rt.find("knock-admit-@lin:example.org").is_some());
    assert!(rt.find("knock-deny-@lin:example.org").is_some());
    assert!(rt.find("unban-@spam:junk.example").is_some());
}

#[test]
fn a_replaced_room_should_show_the_way_to_the_new_one_instead_of_the_composer() {
    let rt = runtime(demo::tombstone);
    assert!(rt.find("tombstone-follow").is_some());
    assert!(rt.find("tombstone-reason").is_some());
}

#[test]
fn the_new_room_form_should_offer_an_address_only_for_a_public_room() {
    let mut rt = runtime(|s| s.open_room_form(crate::state::RoomForm::NewRoom { space: None }));
    assert!(rt.find("room-form-address").is_none() && rt.find("room-form-rule-space-members").is_none());
    press(&mut rt, "room-form-rule-public");
    assert!(rt.find("room-form-address").is_some() && rt.find("room-form-publish").is_some());
}

#[test]
fn a_press_on_a_dialogs_empty_space_should_keep_it_open() {
    let mut rt = runtime(|s| s.open_settings());
    let r = rt.presented("settings-body").unwrap().0;
    let (x, y) = (r[0] + r[2] - 10.0, r[1] + r[3] - 10.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.modal.is_some());
}
