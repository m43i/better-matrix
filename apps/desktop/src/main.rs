//! Better Matrix desktop.
//!
//! `bun run dev` opens the window. Headless:
//! `cargo run -p better-matrix-desktop -- --snap shot.png --scene demo`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod demo;
mod i18n;
mod platform;
mod state;
mod third_party;
mod ui;

use matrix_core::{ClientIdentity, Engine, EngineConfig};
use platform::Shell;
use rune::{App, AppSpec, Key, KeyEvent, Tx};
use state::{Appearance, Popover, Scope, SettingsSection, State};

/// Reverse-DNS app ID: data folders, keychain service, notification identity.
pub const APP_ID: &str = "chat.bettermatrix.desktop";

/// The app's public home page, sent to OAuth servers when the app registers with them. Servers
/// show it on the consent screen and require `https`. Placeholder until the project has a site.
const CLIENT_URI: &str = "https://example.org/better-matrix";

fn spec() -> AppSpec<State> {
    AppSpec {
        min_size: Some((720.0, 480.0)),
        scenes: &[
            ("demo", demo::seed),
            ("demo-invitation", demo::invitation),
            ("demo-discovery", demo::discovery),
            ("demo-public-rooms", |s| {
                demo::discovery(s);
                s.browse_public_rooms();
            }),
            ("demo-invitations-home", |s| {
                demo::invitation(s);
                s.close_modal();
            }),
            ("demo-invitations-collapsed", |s| {
                demo::invitation(s);
                s.close_modal();
                s.toggle_section(state::INVITATIONS);
            }),
            ("demo-light", |s| {
                demo::seed(s);
                s.settings.appearance = Appearance::Light;
            }),
            ("demo-dm", |s| {
                demo::seed(s);
                s.select_room("!dm-grace:example.org");
            }),
            ("demo-call-card", |s| {
                demo::seed(s);
                s.select_room("!dm-kai:example.org");
            }),
            ("demo-space", |s| {
                demo::seed(s);
                s.select_scope(Scope::Space(demo::WORK_SPACE.into()));
            }),
            ("demo-accounts", |s| {
                demo::seed(s);
                s.toggle_popover(Popover::AccountSwitcher);
            }),
            ("demo-compose", demo::compose),
            ("demo-add-account", |s| {
                demo::seed(s);
                s.start_adding_account();
            }),
            ("demo-sign-in-password", demo::sign_in_password),
            ("demo-sign-in-browser", demo::sign_in_browser),
            ("demo-settings", |s| {
                demo::seed(s);
                s.open_settings();
            }),
            ("demo-settings-info", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::Info);
            }),
            ("demo-settings-third-party", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::Info);
                s.toggle_third_party();
            }),
            ("demo-settings-notifications", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::Notifications);
            }),
            ("demo-de", |s| {
                demo::seed(s);
                s.set_language(i18n::Language::Locale("de"));
            }),
            ("demo-fr-settings", |s| {
                demo::seed(s);
                s.set_language(i18n::Language::Locale("fr"));
                s.open_settings();
            }),
            ("demo-room", |s| {
                demo::seed(s);
                s.select_room("!lab:example.org");
            }),
            ("demo-history-hidden", |s| {
                demo::seed(s);
                s.select_scope(Scope::Space(demo::WORK_SPACE.into()));
                s.select_room("!releases:example.org");
            }),
            ("demo-suspended", |s| {
                // The hobby account was signed out softly and put off: it waits in the menu.
                demo::seed(s);
                let account = matrix_core::AccountId::new("@ada:hobby.chat");
                let ended = matrix_core::Event::SessionEnded { account: account.clone(), kept: true };
                s.apply(ended, &mut rune::Tx::default());
                s.apply(matrix_core::Event::AccountRemoved(account), &mut rune::Tx::default());
                s.close_modal();
                s.toggle_popover(Popover::AccountSwitcher);
            }),
            ("suspended", |s| {
                // Every account was signed out softly in an earlier run: the sign-in card lists them.
                for id in ["@ada:hobby.chat", "@ada:example.org"] {
                    let account = matrix_core::AccountId::new(id);
                    s.apply(matrix_core::Event::SessionEnded { account, kept: true }, &mut rune::Tx::default());
                }
            }),
            ("restoring", |s| {
                s.restoring = vec![matrix_core::AccountId::new(demo::ADA)];
            }),
            ("demo-voice", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.start_voice_memo(state::ComposerTarget::Room);
            }),
            ("demo-typing", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                let person =
                    |id: &str, name: &str| matrix_core::Person { id: id.into(), name: name.into(), avatar: None };
                let people = vec![
                    person("@grace:example.org", "Grace Hopper"),
                    person("@alan:example.org", "Alan Turing"),
                    person("@kai:example.org", "Kai"),
                    person("@linus:example.org", "Linus"),
                ];
                s.accounts.set_typing(&matrix_core::AccountId::new(demo::ADA), demo::DEMO_THREAD.0.into(), people);
            }),
            ("demo-edit", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                let target = state::EditTarget {
                    account: matrix_core::AccountId::new(demo::ADA),
                    key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                    event_id: "$demo-edit".into(),
                };
                s.composer.start_edit(target, "Friday works for me, let's tag it then.", None);
            }),
            ("demo-thread", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.open_thread(demo::DEMO_THREAD.1);
            }),
            ("demo-react", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.toggle_popover(Popover::Emoji(state::EmojiFor::Reaction {
                    key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                    event_id: demo::DEMO_THREAD.1.into(),
                }));
            }),
            ("demo-room-files", |s| {
                demo::compose(s);
                s.composer = state::Composer::default();
                demo::room_history(s);
                s.toggle_side(state::SidePanel::Files);
                demo::room_history(s);
            }),
            ("demo-room-threads", |s| {
                demo::compose(s);
                s.composer = state::Composer::default();
                s.toggle_side(state::SidePanel::Threads);
                demo::room_history(s);
            }),
            ("demo-poll-form", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.open_poll_form(state::ComposerTarget::Room);
                s.poll_form.question.insert("Where do we meet?");
                s.poll_form.answers[0].insert("Office");
                s.poll_form.answers[1].insert("Café");
            }),
            ("demo-emoji-search", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.toggle_popover(Popover::Emoji(state::EmojiFor::Composer(state::ComposerTarget::Room)));
                s.emoji_search.insert("heart");
            }),
            ("demo-account", |s| {
                demo::seed(s);
                s.open_account_settings();
            }),
            ("demo-unverified", |s| {
                demo::seed(s);
                s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
                s.open_account_settings();
            }),
            ("demo-unverified-banner", |s| {
                demo::seed(s);
                s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
            }),
            ("demo-sessions", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::Sessions);
            }),
            ("demo-verify", |s| {
                demo::seed(s);
                s.modal = Some(state::Modal::Verification {
                    account: matrix_core::AccountId::new("@ada:hobby.chat"),
                    flow_id: Some("flow".into()),
                    step: matrix_core::VerificationStep::Compare(demo::verification_emojis()),
                });
            }),
            ("demo-recovery-key", |s| {
                demo::seed(s);
                s.modal = Some(state::Modal::NewRecoveryKey {
                    key: "EsTc 9Ruv YFbj QwpV Mvyp xWV8 yfr3 jMzW kx2Y 5c4Q".into(),
                });
            }),
            ("demo-reset", |s| {
                demo::seed(s);
                s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
                s.start_identity_reset();
            }),
            ("demo-reset-approve", |s| {
                demo::seed(s);
                s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
                s.start_identity_reset();
                s.confirm_identity_reset();
                if let Some(state::Modal::ResetIdentity { step: state::ResetStep::Resetting { url, .. }, .. }) =
                    &mut s.modal
                {
                    *url = Some("https://account.hobby.chat/reset".into());
                }
            }),
            ("demo-reset-done", |s| {
                demo::seed(s);
                s.modal = Some(state::Modal::ResetIdentity {
                    account: matrix_core::AccountId::new("@ada:hobby.chat"),
                    step: state::ResetStep::Done,
                });
            }),
            ("demo-profile", |s| {
                demo::seed(s);
                s.open_profile();
            }),
            ("demo-profile-adding", |s| {
                demo::seed(s);
                s.open_profile();
                let pending = matrix_core::PendingContact::offline(matrix_core::ContactKind::Phone, "+49 170 1234567");
                s.profile.phone.step = state::ContactStep::Sent { pending, busy: false };
                let pending = matrix_core::PendingContact::offline(matrix_core::ContactKind::Email, "ada@work.example");
                s.profile.email.step = state::ContactStep::Sent { pending, busy: false };
            }),
            ("demo-profile-oauth", |s| {
                demo::seed(s);
                s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
                s.open_settings();
                s.show_settings_section(SettingsSection::Profile);
            }),
            ("demo-search", |s| {
                demo::seed(s);
                s.search.field.insert("gr");
                let now = std::time::Instant::now();
                s.search_people(now);
                s.search_people(now + state::SEARCH_PAUSE);
            }),
            ("demo-members", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.toggle_members();
            }),
            ("demo-member-menu", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.toggle_members();
                s.toggle_member_menu("@kai:example.org");
                s.toggle_member_submenu(state::MemberSubmenu::Role);
            }),
            ("demo-room-info", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.toggle_side(state::SidePanel::Info);
            }),
            ("demo-invite", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.open_invite();
                s.invite_search.field.insert("@mira:example.org");
            }),
            ("demo-sections", |s| {
                demo::seed(s);
                let id = matrix_core::AccountId::new(demo::ADA);
                let mut layout = matrix_core::SidebarLayout::default();
                layout.scopes.insert(
                    "home".into(),
                    matrix_core::ScopeLayout {
                        collapsed: Vec::new(),
                        order: Vec::new(),
                        sections: vec![matrix_core::Section {
                            id: "section-1".into(),
                            name: "Close friends".into(),
                            rooms: vec!["!dm-grace:example.org".into()],
                        }],
                    },
                );
                s.apply(matrix_core::Event::SidebarLayout { account: id, layout }, &mut rune::Tx::default());
                s.dragging = Some(state::SidebarDrag {
                    item: state::Dragged::Room("!dm-kai:example.org".into()),
                    point: [180.0, 150.0],
                    target: Some(state::DropTarget::Room { section: "section-1".into(), index: 1 }),
                });
            }),
            ("demo-section-drag", |s| {
                demo::seed(s);
                s.dragging = Some(state::SidebarDrag {
                    item: state::Dragged::Section(state::DIRECT.into()),
                    point: [180.0, 140.0],
                    target: Some(state::DropTarget::Section { index: 0 }),
                });
            }),
            ("demo-call-voice", |s| demo::call(s, false)),
            ("demo-call-video", |s| demo::call(s, true)),
            ("demo-call-ringing", demo::ringing),
            ("demo-settings-voice-video", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::VoiceVideo);
                demo::devices(s);
            }),
            ("demo-settings-select", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::VoiceVideo);
                demo::devices(s);
                s.open_select(state::SelectFor::Device(matrix_core::DeviceKind::Microphone));
                demo::devices(s);
            }),
            ("demo-call-devices", |s| {
                demo::call(s, false);
                s.open_call_devices(matrix_core::DeviceKind::Microphone);
                demo::devices(s);
            }),
            ("demo-call-chat", |s| {
                demo::call(s, false);
                s.toggle_call_chat();
            }),
            ("demo-space-drag", |s| {
                demo::seed(s);
                s.dragging = Some(state::SidebarDrag {
                    item: state::Dragged::Space("!rust:example.org".into()),
                    point: [34.0, 92.0],
                    target: Some(state::DropTarget::Space { index: 0 }),
                });
            }),
            ("demo-create-menu", |s| {
                demo::seed(s);
                s.toggle_popover(Popover::CreateMenu);
            }),
            ("demo-new-chat", |s| {
                demo::seed(s);
                s.open_new_chat();
            }),
            ("demo-space-menu", |s| {
                demo::seed(s);
                s.toggle_popover(Popover::SpaceMenu(demo::WORK_SPACE.into()));
            }),
            ("demo-new-room", |s| {
                demo::seed(s);
                s.select_scope(Scope::Space(demo::WORK_SPACE.into()));
                s.open_room_form(state::RoomForm::NewRoom { space: Some(demo::WORK_SPACE.into()) });
            }),
            ("demo-stress", demo::stress),
            ("demo-toast", |s| {
                demo::seed(s);
                s.notice = Some("Could not send the message: the server is unreachable.".into());
            }),
            ("demo-room-menu", |s| {
                demo::seed(s);
                s.toggle_room_menu("!lab:example.org");
                s.show_room_notifications();
            }),
            ("demo-dm-menu", |s| {
                demo::seed(s);
                s.toggle_room_menu("!dm-grace:example.org");
            }),
            ("demo-leave", |s| {
                demo::seed(s);
                s.ask_to_leave("!lab:example.org");
            }),
        ],
        // Inter for text; the system's monospace font for code, recovery keys and the sign-in
        // backdrop (Rune's default mono slot is Inter too).
        families: ("Inter", "ui-monospace"),
        setup: Some(|s, waker| s.media.attach(waker)),
        poll: Some(|s, clock, tx| {
            let moved = s.animate(clock, tx);
            s.poll(tx) || moved
        }),
        on_key: Some(on_key),
        ..AppSpec::new("Better Matrix", (1200.0, 760.0), State::detached, ui::view)
    }
}

/// Window-wide keys the focused element left unhandled.
fn on_key(s: &mut State, key: &KeyEvent, _: &mut Tx) -> bool {
    match key.key {
        Key::Escape if s.preview.is_some() => {
            s.close_preview();
            true
        }
        Key::Escape if s.popover.is_some() => {
            s.close_popover();
            true
        }
        Key::Escape if s.modal.is_some() => {
            s.close_modal();
            true
        }
        _ => false,
    }
}

/// How long quitting waits for the engine to leave a call.
const QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(4);

fn main() {
    // The window reports this id (Wayland `app_id`, X11 `WM_CLASS`), so the desktop matches it
    // to the installed entry and its icon.
    rune::set_app_id(APP_ID);
    let app = App::new(spec()).on_focus(|s, focused, _| s.focus_changed(focused));
    // Renders, scripts and traces run on the detached demo state: no engine, no keychain, and
    // nothing they change reaches the user's saved settings.
    if rune::is_headless() {
        return app.run();
    }
    // The engine's and its libraries' logs go to stderr when `RUST_LOG` names what to show
    // (`RUST_LOG=matrix_core=debug,livekit=info`).
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        let _ = tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
    }
    let Some(dirs) = rune::dirs::AppDirs::new(APP_ID) else {
        eprintln!("better-matrix: no home folder to keep data in; running without an engine");
        return app.run();
    };
    let handle = app.handle();
    let config = match CLIENT_URI.parse() {
        Ok(uri) => EngineConfig {
            data_dir: dirs.data().to_path_buf(),
            secrets: platform::system_secrets(dirs.data()),
            identity: ClientIdentity { name: "Better Matrix".into(), uri },
        },
        Err(error) => {
            eprintln!("better-matrix: invalid client URI {CLIENT_URI}: {error}");
            return app.run();
        }
    };
    let sink = {
        let handle = handle.clone();
        move |event| handle.update(move |s: &mut State, tx| s.apply(event, tx))
    };
    match Engine::start(config, sink) {
        Ok(engine) => {
            let prefs = rune::prefs::Prefs::open(APP_ID);
            let settings = platform::load_settings(&prefs);
            let scratch = dirs.cache().join("scratch");
            platform::reset_scratch(&scratch);
            let shell_scratch = scratch.clone();
            let stopped = engine.stopped();
            app.init(move || State::new(engine, Shell::native(handle, prefs, shell_scratch), settings)).run();
            // The app's state, and with it the engine's handle, is gone: the engine leaves a call
            // under way and stops.
            stopped.wait(QUIT_WAIT);
            let _ = std::fs::remove_dir_all(&scratch);
        }
        Err(error) => {
            eprintln!("better-matrix: the Matrix engine failed to start: {error}");
            app.run();
        }
    }
}

#[cfg(test)]
mod tests {
    use rune::{Env, Runtime};

    use super::*;

    fn runtime(scene: fn(&mut State)) -> Runtime<State> {
        let mut s = State::detached();
        scene(&mut s);
        let mut rt = Runtime::new(s, ui::view, (1200.0, 760.0), Env { system_dark: true, scale: 1.0 });
        rt.frame(0.0);
        rt
    }

    fn unread_timeline(s: &mut State, messages: usize) {
        demo::seed(s);
        s.select_room(demo::DEMO_THREAD.0);
        let mut entries =
            vec![matrix_core::TimelineEntry { id: "new-marker".into(), kind: matrix_core::EntryKind::ReadMarker }];
        entries.extend((0..messages).map(|i| matrix_core::TimelineEntry {
            id: format!("unread-{i}"),
            kind: matrix_core::EntryKind::Message(Box::new(matrix_core::Message {
                event_id: format!("$unread-{i}"),
                sender_id: "@grace:example.org".into(),
                sender_name: "Grace".into(),
                content: matrix_core::MessageContent::Text { body: "A new message".into(), html: None },
                ..Default::default()
            })),
        }));
        s.apply(
            matrix_core::Event::TimelineChanged {
                account: matrix_core::AccountId::new(demo::ADA),
                key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![matrix_core::TimelineDiff::Reset(entries)],
            },
            &mut rune::Tx::default(),
        );
    }

    #[test]
    fn a_new_marker_visible_on_open_should_fade_away() {
        let mut rt = runtime(|s| unread_timeline(s, 2));
        assert!(rt.presented("new-marker").is_some());
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.presented("new-marker").is_none());
    }

    #[test]
    fn a_new_marker_should_fade_slowly_after_it_is_seen() {
        let mut rt = runtime(|s| unread_timeline(s, 2));
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
        let (_, opacity) = rt.presented("new-marker").expect("the marker is still fading");
        assert!(opacity > 0.1 && opacity < 1.0, "opacity: {opacity}");
    }

    #[test]
    fn scrolling_up_to_the_new_marker_should_dismiss_it() {
        let mut rt = runtime(|s| unread_timeline(s, 80));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        rt.update(|_, tx| tx.scroll_to_instant("timeline", 0.0, 0.0));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.presented("new-marker").is_none());
    }

    #[test]
    fn a_new_marker_outside_the_viewport_should_wait_to_be_seen() {
        let mut rt = runtime(|s| unread_timeline(s, 80));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        let account = matrix_core::AccountId::new(demo::ADA);
        let key = matrix_core::TimelineKey::room(demo::DEMO_THREAD.0);
        let timeline = rt.state.accounts.get(&account).and_then(|data| data.timeline(&key)).expect("the timeline");
        assert!(!timeline.read_marker_seen("new-marker"));
    }

    fn incoming_boundary(s: &mut State, tx: &mut rune::Tx) {
        use matrix_core::{EntryKind, Event, Message, TimelineDiff, TimelineEntry};
        s.apply(
            Event::TimelineChanged {
                account: matrix_core::AccountId::new(demo::ADA),
                key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![
                    TimelineDiff::Remove { index: 0 },
                    TimelineDiff::PushBack(TimelineEntry { id: "incoming-marker".into(), kind: EntryKind::ReadMarker }),
                    TimelineDiff::PushBack(TimelineEntry {
                        id: "incoming".into(),
                        kind: EntryKind::Message(Box::new(Message {
                            event_id: "$incoming".into(),
                            sender_id: "@grace:example.org".into(),
                            sender_name: "Grace".into(),
                            ..Default::default()
                        })),
                    }),
                ],
            },
            tx,
        );
    }

    #[test]
    fn following_live_at_the_bottom_should_not_flash_another_new_marker() {
        let mut rt = runtime(|s| unread_timeline(s, 2));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.state.at_bottom.load(std::sync::atomic::Ordering::Relaxed));
        rt.update(incoming_boundary);
        rt.frame(1.0 / 60.0);
        assert!(rt.presented("incoming-marker").is_none(), "a live boundary should never be mounted");
    }

    #[test]
    fn an_acknowledged_line_should_stay_hidden_when_receipts_advance_while_scrolled_up() {
        let mut rt = runtime(|s| unread_timeline(s, 2));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        rt.state.at_bottom.store(false, std::sync::atomic::Ordering::Relaxed);
        rt.update(incoming_boundary);
        let account = matrix_core::AccountId::new(demo::ADA);
        let key = matrix_core::TimelineKey::room(demo::DEMO_THREAD.0);
        assert!(rt.state.accounts.get(&account).unwrap().timeline(&key).unwrap().read_marker_seen("incoming-marker"));
        rt.frame(1.0 / 60.0);
        assert!(rt.presented("incoming-marker").is_none());
    }

    #[test]
    fn an_incoming_marker_while_unfocused_should_wait_until_focus_returns() {
        let mut rt = runtime(|s| unread_timeline(s, 2));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        rt.update(|s, _| s.focus_changed(false));
        rt.update(incoming_boundary);
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        let (_, opacity) = rt.presented("incoming-marker").expect("unread while away");
        assert_eq!(opacity, 1.0);
        rt.update(|s, _| s.focus_changed(true));
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.presented("incoming-marker").is_none());
    }

    #[test]
    fn loaded_members_should_correct_a_stale_zero_in_the_room_header() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
            let id = matrix_core::AccountId::new(demo::ADA);
            let data = s.accounts.get(&id).expect("account");
            let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
            for room in &mut rooms {
                room.member_count = 0;
            }
            s.accounts.set_room_list(&id, matrix_core::RoomList { rooms, spaces, ..Default::default() });
        });
        let button =
            rt.elements().into_iter().find(|e| e.key.as_deref() == Some("room-members")).expect("members button");
        assert_eq!(button.label.as_deref(), Some("4 members"));
    }

    #[test]
    fn refreshing_members_should_keep_the_list_and_search_visible() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
            s.show_side(state::SidePanel::Members);
            s.member_search = rune::TextEdit::new("Grace");
        });
        assert!(rt.find("member-@grace:example.org").is_some());
        rt.update(|s, tx| {
            s.apply(
                matrix_core::Event::MembersInvalidated {
                    account: matrix_core::AccountId::new(demo::ADA),
                    room_id: demo::DEMO_THREAD.0.into(),
                },
                tx,
            )
        });
        rt.frame(1.0 / 60.0);
        assert!(rt.find("member-@grace:example.org").is_some());
        assert_eq!(rt.state.member_search.text, "Grace");
        assert!(!rt.elements().iter().any(|element| element.text.as_deref() == Some("Loading members…")));
    }

    #[test]
    fn a_direct_chats_info_should_omit_the_people_link() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-grace:example.org");
            s.show_side(state::SidePanel::Info);
        });
        assert!(rt.find("info-members").is_none());
    }

    #[test]
    fn an_invitation_preview_should_offer_accept_and_decline() {
        let rt = runtime(demo::invitation);
        assert!(rt.find("invitation-accept").is_some() && rt.find("invitation-decline").is_some());
    }

    #[test]
    fn collapsing_invitations_should_keep_the_counter_and_hide_its_rooms() {
        let mut rt = runtime(|s| {
            demo::invitation(s);
            s.close_modal();
        });
        press(&mut rt, "section-header-invitations");
        let header = rt
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some("section-header-invitations"))
            .expect("invitation section");
        assert!(
            header.label.as_deref() == Some("Invitations (1)") && rt.find("invitation-!design:example.org").is_none()
        );
    }

    #[test]
    fn the_home_overview_should_show_invitations_outside_spaces() {
        let rt = runtime(|s| {
            demo::invitation(s);
            s.close_modal();
        });
        assert!(rt.find("overview-invitation-!design:example.org").is_some());
    }

    #[test]
    fn invitation_names_should_align_with_normal_room_names() {
        let rt = runtime(|s| {
            demo::invitation(s);
            s.close_modal();
        });
        let elements = rt.elements();
        let invite = elements
            .iter()
            .find(|element| element.key.as_deref() == Some("invitation-!design:example.org"))
            .expect("invite");
        let room = elements
            .iter()
            .find(|element| element.key.as_deref() == Some("room-!lab:example.org"))
            .expect("normal room");
        let invite_name = elements
            .iter()
            .find(|element| element.text.as_deref() == Some("Design") && element.path.starts_with(&invite.path))
            .expect("invitation name");
        let room_name = elements
            .iter()
            .find(|element| element.text.as_deref() == Some("lab") && element.path.starts_with(&room.path))
            .expect("room name");
        assert_eq!((invite_name.rect[0], invite.rect[3]), (room_name.rect[0], room.rect[3]));
    }

    #[test]
    fn a_spaces_more_button_should_open_all_public_rooms() {
        let mut rt = runtime(demo::discovery);
        press(&mut rt, "public-rooms-more");
        assert!(rt.find("public-rooms").is_some());
    }

    #[test]
    fn a_call_card_should_have_no_message_actions() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-kai:example.org");
        });
        assert!(rt.find("actions-$demo-@kai:example.org-0").is_none());
    }

    #[test]
    fn a_dialog_in_a_short_window_should_keep_its_buttons_in_view() {
        let mut s = State::detached();
        demo::seed(&mut s);
        s.select_room(demo::DEMO_THREAD.0);
        s.open_poll_form(state::ComposerTarget::Room);
        for _ in 0..4 {
            s.poll_form.add_answer();
        }
        let mut rt = Runtime::new(s, ui::view, (720.0, 380.0), Env { system_dark: true, scale: 1.0 });
        // The dialog's bound comes from the window's size, read from the first layout.
        rt.frame(0.0);
        rt.frame(0.1);
        let (_, create) = rt.find("poll-create").unwrap_or((0.0, f32::MAX));
        assert!(create < 380.0);
    }

    #[test]
    fn ending_a_poll_with_a_long_question_should_keep_the_buttons_in_view() {
        let mut s = State::detached();
        demo::seed(&mut s);
        s.select_room(demo::DEMO_THREAD.0);
        let question = "Which of these very long options should we pick for the release? ".repeat(30);
        s.end_poll(matrix_core::TimelineKey::room(demo::DEMO_THREAD.0), "$poll", &question);
        let mut rt = Runtime::new(s, ui::view, (720.0, 380.0), Env { system_dark: true, scale: 1.0 });
        rt.frame(0.0);
        rt.frame(0.1);
        let (_, ok) = rt.find("confirm-ok").unwrap_or((0.0, f32::MAX));
        assert!(ok < 380.0);
    }

    #[test]
    fn a_call_under_way_should_offer_to_join_from_its_card() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-kai:example.org");
        });
        assert!(rt.find("call-card-join-$demo-@kai:example.org-0").is_some());
    }

    #[test]
    fn a_call_card_should_offer_no_join_where_the_room_allows_no_calls() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-kai:example.org");
            let account = matrix_core::AccountId::new(demo::ADA);
            let Some(data) = s.accounts.get(&account) else { return };
            let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
            for room in rooms.iter_mut().filter(|room| room.room_id == "!dm-kai:example.org") {
                room.call.via = None;
            }
            s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, spaces, invitations: Vec::new() });
        });
        assert!(rt.find("call-card-join-$demo-@kai:example.org-0").is_none());
    }

    #[test]
    fn only_the_newest_call_card_should_offer_to_join() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-kai:example.org");
            let card = matrix_core::Message {
                event_id: "$newer".into(),
                sender_id: "@kai:example.org".into(),
                sender_name: "Kai".into(),
                content: matrix_core::MessageContent::Call(matrix_core::CallCard::default()),
                ..matrix_core::Message::default()
            };
            let entry = matrix_core::TimelineEntry {
                id: "$newer".into(),
                kind: matrix_core::EntryKind::Message(Box::new(card)),
            };
            let (account, key) =
                (matrix_core::AccountId::new(demo::ADA), matrix_core::TimelineKey::room("!dm-kai:example.org"));
            if let Some(timeline) = s.accounts.timeline_mut(&account, &key) {
                timeline.apply(vec![matrix_core::TimelineDiff::PushBack(entry)]);
            }
        });
        assert!(rt.find("call-card-join-$newer").is_some());
        assert!(rt.find("call-card-join-$demo-@kai:example.org-0").is_none());
    }

    #[test]
    fn only_the_users_own_messages_should_have_a_more_menu() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-grace:example.org");
            let account = matrix_core::AccountId::new(demo::ADA);
            let key = matrix_core::TimelineKey::room("!dm-grace:example.org");
            s.accounts.timeline_mut(&account, &key).expect("timeline").apply(vec![
                matrix_core::TimelineDiff::PushBack(matrix_core::TimelineEntry {
                    id: "own-text".into(),
                    kind: matrix_core::EntryKind::Message(Box::new(matrix_core::Message {
                        event_id: "$own-text".into(),
                        sender_id: demo::ADA.into(),
                        sender_name: "Ada".into(),
                        content: matrix_core::MessageContent::Text { body: "Hello".into(), html: None },
                        ..Default::default()
                    })),
                }),
            ]);
        });
        assert!(rt.find("more-$own-text").is_some());
        assert!(rt.find("more-$demo-@grace:example.org-1").is_none());
    }

    #[test]
    fn the_users_own_call_card_should_offer_no_more_menu() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!dm-grace:example.org");
        });
        assert!(rt.find("more-$demo-@ada:example.org-0").is_none());
    }

    #[test]
    fn without_accounts_the_window_should_show_sign_in() {
        let rt = runtime(|_| {});
        assert!(rt.find("sign-in-card").is_some());
    }

    #[test]
    fn every_sign_in_step_should_show_the_app_logo_and_name() {
        for scene in [|_: &mut State| {}, demo::sign_in_password, demo::sign_in_browser] {
            assert!(runtime(scene).find("brand").is_some());
        }
    }

    fn recording(s: &mut State) {
        demo::seed(s);
        s.select_room(demo::DEMO_THREAD.0);
        s.start_voice_memo(state::ComposerTarget::Room);
    }

    #[test]
    fn recording_a_voice_memo_should_show_water_along_the_bottom_of_the_room() {
        let rt = runtime(recording);
        assert!(rt.find("room-voice-water").is_some());
    }

    #[test]
    fn a_recording_memo_should_offer_one_send_button() {
        let rt = runtime(recording);
        assert!(rt.find("voice-send").is_some() && rt.find("send").is_none());
    }

    #[test]
    fn leaving_the_room_should_stop_its_voice_memo() {
        let mut rt = runtime(recording);
        rt.update(|s, _| s.select_room("!lab:example.org"));
        rt.frame(1.0 / 60.0);
        assert!(rt.state.composer.voice.is_none());
    }

    #[test]
    fn opening_another_thread_should_stop_the_memo_recording_in_the_last() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
            s.open_thread(demo::DEMO_THREAD.1);
            s.start_voice_memo(state::ComposerTarget::Thread);
        });
        rt.update(|s, _| s.open_thread("$another-root"));
        rt.frame(1.0 / 60.0);
        assert!(rt.state.thread_composer.voice.is_none());
    }

    #[test]
    fn the_water_should_leave_the_composer_pressable() {
        let mut rt = runtime(recording);
        press(&mut rt, "voice-cancel");
        assert!(rt.state.composer.voice.is_none() && rt.find("room-voice-water").is_none());
    }

    #[test]
    fn pressing_a_room_should_open_it() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, "room-!lab:example.org");
        assert_eq!(rt.state.active_room_id(), Some("!lab:example.org"));
    }

    #[test]
    fn contact_details_the_server_keeps_to_its_account_page_should_show_read_only() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
            s.open_profile();
        });
        assert!(rt.find("contact-email-ada@hobby.chat").is_some() && rt.find("account-page").is_some());
        assert!(rt.find("contact-remove-email-ada@hobby.chat").is_none() && rt.find("contact-new-email").is_none());
    }

    #[test]
    fn dragging_a_space_above_another_should_reorder_the_rail() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            let event = matrix_core::Event::SidebarLayout {
                account: matrix_core::AccountId::new(demo::ADA),
                layout: matrix_core::SidebarLayout::default(),
            };
            s.apply(event, &mut rune::Tx::default());
        });
        let (_, top) = rt.find("scope-!engines:example.org").unwrap_or_else(|| panic!("no first space"));
        let (x, y) = rt.find("scope-!rust:example.org").unwrap_or_else(|| panic!("no second space"));
        rt.pointer_down(x, y);
        for to in [y - 10.0, top - 10.0] {
            rt.pointer_move(x, to);
            rt.frame(1.0 / 60.0);
        }
        rt.pointer_up(x, top - 10.0);
        rt.frame(1.0 / 60.0);
        let spaces = rt.state.active_account().map(|account| account.sidebar.spaces.clone()).unwrap_or_default();
        assert_eq!(spaces, ["!rust:example.org", "!engines:example.org"]);
    }

    #[test]
    fn a_room_whose_server_offers_calls_should_show_the_call_buttons() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!general:example.org");
        });
        assert!(rt.find("call-start").is_some() && rt.find("call-start-video").is_some());
    }

    #[test]
    fn the_rooms_call_should_take_the_place_of_its_timeline() {
        let rt = runtime(|s| demo::call(s, false));
        assert!(rt.find("call-stage").is_some() && rt.find("timeline").is_none());
        assert!(rt.find("call-start").is_none() && rt.find("call-bar").is_none());
    }

    #[test]
    fn showing_the_chat_during_a_call_should_offer_the_way_back() {
        let mut rt = runtime(|s| demo::call(s, false));
        press(&mut rt, "call-chat");
        assert!(rt.find("timeline").is_some() && rt.find("call-back").is_some() && rt.find("call-bar").is_some());
        press(&mut rt, "call-back");
        assert!(rt.find("call-stage").is_some());
    }

    #[test]
    fn a_long_caller_name_should_leave_the_ring_controls_in_view() {
        let rt = runtime(|s| {
            demo::ringing(s);
            s.rings[0].caller.name = "Grace Brewster Murray Hopper of the United States Navy Reserve".repeat(3);
        });
        let (x, _) = rt.find("ring-answer-@ada:example.org-!dm-grace:example.org").unwrap_or((f32::MAX, 0.0));
        assert!(x < 1200.0 - 20.0);
    }

    #[test]
    fn a_ringing_call_should_show_who_calls_until_answered() {
        let mut rt = runtime(demo::ringing);
        press(&mut rt, "ring-answer-@ada:example.org-!dm-grace:example.org");
        assert!(rt.state.rings.is_empty() && rt.find("call-rings").is_none());
    }

    #[test]
    fn the_chevron_beside_the_microphone_should_list_the_devices() {
        let mut rt = runtime(|s| {
            demo::call(s, false);
            demo::devices(s);
        });
        press(&mut rt, "call-devices-Microphone");
        assert!(rt.find("call-device-Microphone-USB Headset Microphone").is_some());
        press(&mut rt, "call-device-Speaker-USB Headset");
        assert_eq!(rt.state.settings.devices.speaker.as_deref(), Some("USB Headset"));
    }

    #[test]
    fn the_info_panel_should_lead_to_the_member_list() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
        });
        press(&mut rt, "room-info");
        assert!(rt.find("info-panel").is_some());
        press(&mut rt, "info-members");
        assert!(rt.find("members-panel").is_some() && rt.find("member-search").is_some());
    }

    #[test]
    fn pressing_a_member_should_offer_what_the_user_may_do() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
            s.toggle_members();
        });
        press(&mut rt, "member-@kai:example.org");
        assert!(rt.find("member-menu-chat").is_some() && rt.find("member-menu-kick").is_some());
    }

    #[test]
    fn searching_the_member_list_should_narrow_it() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
            s.toggle_members();
            s.member_search.insert("grace");
        });
        rt.frame(1.0 / 60.0);
        assert!(rt.find("member-@grace:example.org").is_some() && rt.find("member-@kai:example.org").is_none());
    }

    #[test]
    fn the_third_party_button_should_reveal_the_list() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.open_settings();
            s.show_settings_section(SettingsSection::Info);
        });
        assert!(rt.find("component-matrix-sdk-0.19.1").is_none() && rt.find("third-party").is_none());
        press(&mut rt, "third-party-toggle");
        assert!(rt.find("third-party").is_some());
    }

    fn press(rt: &mut Runtime<State>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn switching_accounts_should_go_through_the_account_menu() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, "account-button");
        press(&mut rt, "switch-@ada:hobby.chat");
        assert_eq!(rt.state.nav.account.as_ref().map(|id| id.as_str()), Some("@ada:hobby.chat"));
    }

    #[test]
    fn add_account_in_the_account_menu_should_open_sign_in() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, "account-button");
        press(&mut rt, "menu-add-account");
        assert!(rt.find("sign-in-card").is_some());
    }

    #[test]
    fn pressing_outside_the_account_menu_should_close_it() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, "account-button");
        press(&mut rt, "scrim");
        assert_eq!(rt.state.popover, None);
    }

    #[test]
    fn pressing_a_space_should_list_its_rooms() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, &format!("scope-{}", demo::WORK_SPACE));
        assert!(rt.find("room-!design:example.org").is_some());
    }

    #[test]
    fn right_clicking_a_room_should_open_its_menu() {
        let mut rt = runtime(demo::seed);
        let (x, y) = rt.find("room-!lab:example.org").unwrap();
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.find("room-menu").is_some());
    }

    #[test]
    fn choosing_mute_in_the_room_menu_should_mute_the_room() {
        let mut rt = runtime(demo::seed);
        let (x, y) = rt.find("room-!lab:example.org").unwrap();
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
        rt.frame(1.0 / 60.0);
        press(&mut rt, "room-menu-notifications");
        press(&mut rt, "notify-mute");
        let room = rt.state.active_account().and_then(|a| a.room("!lab:example.org")).map(|r| r.notifications);
        assert_eq!(room, Some(matrix_core::NotificationMode::Mute));
    }

    #[test]
    fn settings_in_the_account_menu_should_open_settings() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, "account-button");
        press(&mut rt, "menu-settings");
        assert!(rt.find("settings-modal").is_some());
    }

    #[test]
    fn choosing_a_default_in_settings_should_apply_to_rooms_without_their_own_mode() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.open_settings();
            s.show_settings_section(SettingsSection::Notifications);
        });
        press(&mut rt, "select-default-@ada:example.org");
        press(&mut rt, "default-@ada:example.org-mentions");
        let lab = rt.state.active_account().and_then(|a| a.room("!lab:example.org")).map(|r| r.notifications);
        assert_eq!(lab, Some(matrix_core::NotificationMode::MentionsOnly));
    }

    #[test]
    fn pressing_the_backdrop_should_close_settings() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.open_settings();
        });
        let (x, y) = (5.0, 5.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.modal.is_none());
    }

    #[test]
    fn leave_room_should_ask_before_leaving() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.toggle_room_menu("!lab:example.org");
        });
        press(&mut rt, "room-menu-leave");
        assert!(rt.find("confirm-dialog").is_some());
    }

    #[test]
    fn cancelling_the_confirmation_should_keep_the_room() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.ask_to_leave("!lab:example.org");
        });
        press(&mut rt, "confirm-cancel");
        assert!(rt.state.modal.is_none() && rt.find("room-!lab:example.org").is_some());
    }

    #[test]
    fn opening_the_app_should_show_the_overview() {
        let rt = runtime(demo::seed);
        assert!(rt.find("overview").is_some() && rt.state.active_room_id().is_none());
    }

    #[test]
    fn an_unread_room_in_the_overview_should_open_it() {
        let mut rt = runtime(demo::seed);
        press(&mut rt, "overview-unread-!dm-grace:example.org");
        assert_eq!(rt.state.active_room_id(), Some("!dm-grace:example.org"));
    }

    #[test]
    fn the_sidebar_header_should_return_to_the_overview() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!lab:example.org");
        });
        press(&mut rt, "sidebar-header");
        assert!(rt.find("overview").is_some());
    }

    #[test]
    fn the_unverified_banner_should_open_the_account_page() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
        });
        press(&mut rt, "unverified-verify");
        assert_eq!(rt.state.modal, Some(state::Modal::Settings(SettingsSection::Account)));
    }

    #[test]
    fn a_verified_account_should_show_no_banner() {
        let rt = runtime(demo::seed);
        assert!(rt.find("unverified-bar").is_none());
    }

    #[test]
    fn the_bold_button_should_toggle_bold() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!lab:example.org");
        });
        open_formatting(&mut rt);
        press(&mut rt, "format-bold");
        assert!(rt.state.composer.is_active(state::Format::Bold));
    }

    /// Opens the formatting bar and lets the composer settle at its new height.
    fn open_formatting(rt: &mut Runtime<State>) {
        press(rt, "formatting");
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
    }

    #[test]
    fn the_bold_button_should_switch_bold_off_again() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!lab:example.org");
        });
        open_formatting(&mut rt);
        press(&mut rt, "format-bold");
        press(&mut rt, "format-bold");
        assert!(!rt.state.composer.is_active(state::Format::Bold));
    }

    #[test]
    fn the_formatting_bar_should_stay_hidden_until_aa_is_pressed() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!lab:example.org");
        });
        assert!(rt.find("format-bold").is_none());
        press(&mut rt, "formatting");
        assert!(rt.find("format-bold").is_some());
    }
}
