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
use state::{Appearance, EmojiStyle, Popover, Scope, SettingsSection, State};
use std::sync::LazyLock;

/// Reverse-DNS app ID: data folders, keychain service, notification identity.
pub const APP_ID: &str = "chat.bettermatrix.desktop";

/// The app's public home page, sent to OAuth servers when the app registers with them. Servers
/// show it on the consent screen and require `https`. Placeholder until the project has a site.
const CLIENT_URI: &str = "https://example.org/better-matrix";

const NOTO_COLOR_EMOJI: &[u8] = include_bytes!("../../../assets/emoji/NotoColorEmoji.ttf");
const NOTO_EMOJI: &[u8] = include_bytes!("../../../assets/emoji/BetterMatrixNotoEmoji.ttf");

/// The demo scenes that switch the app's language. The language is global, so a test running
/// alongside one would read it: tests that run every scene leave these out.
#[cfg(test)]
const LANGUAGE_SCENES: &[&str] = &["demo-de", "demo-fr-settings"];

fn spec() -> AppSpec<State> {
    static FONTS: LazyLock<Vec<&'static [u8]>> =
        LazyLock::new(|| rune::text::DEFAULT_FONTS.iter().copied().chain([NOTO_COLOR_EMOJI, NOTO_EMOJI]).collect());
    AppSpec {
        min_size: Some((720.0, 480.0)),
        scenes: &[
            ("demo", demo::seed),
            ("demo-attachment-menu", |s| {
                demo::attachment_message(s);
                s.popover = Some(Popover::MessageMenu {
                    key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                    event_id: "$attachment".into(),
                    // Near the window's bottom edge: the menu opens upwards.
                    at: Some((700, 700)),
                });
            }),
            ("demo-attachment-preview", demo::attachment_preview),
            ("demo-preview-batch", demo::preview_batch),
            ("demo-preview-video", demo::preview_video),
            ("demo-preview-audio", demo::preview_audio),
            ("demo-preview-held", demo::preview_held),
            ("demo-pinned-message", |s| {
                demo::pinned_messages(s);
                s.open_thread(demo::DEMO_THREAD.1);
            }),
            ("demo-pins", |s| {
                demo::pinned_messages(s);
                s.show_side(state::SidePanel::Pins);
            }),
            ("demo-message-menu", |s| {
                demo::pinned_messages(s);
                s.popover = Some(state::Popover::MessageMenu {
                    key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                    event_id: demo::DEMO_THREAD.1.into(),
                    at: Some((600, 280)),
                });
            }),
            ("demo-saved", demo::saved_messages),
            ("demo-messages", demo::message_features),
            ("demo-mentions", |s| demo::typing(s, "Thanks @gr")),
            ("demo-commands", |s| demo::typing(s, "/")),
            ("demo-emoji-suggestions", |s| demo::typing(s, "Great :pa")),
            ("demo-pills", demo::pills_draft),
            ("demo-location", demo::location),
            ("demo-edit-history", demo::edit_history),
            ("demo-stickers", |s| {
                demo::typing(s, "");
                s.popover = Some(Popover::Stickers(state::ComposerTarget::Room));
            }),
            ("demo-schedule", |s| {
                demo::scheduled(s);
                s.popover = Some(Popover::Schedule(state::ComposerTarget::Room));
            }),
            ("demo-scheduled", |s| {
                demo::scheduled(s);
                s.popover = Some(Popover::Scheduled);
            }),
            ("demo-hidden-events", demo::hidden_events),
            ("demo-widgets", demo::widgets),
            ("demo-source", demo::event_source),
            ("demo-saved-menu", |s| {
                demo::saved_messages(s);
                s.close_modal();
                let event_id = s
                    .active_account()
                    .and_then(|account| account.room(demo::DEMO_THREAD.0))
                    .and_then(|room| room.saved.first().cloned())
                    .unwrap_or_default();
                s.popover = Some(state::Popover::MessageMenu {
                    key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                    event_id,
                    at: Some((600, 280)),
                });
            }),
            ("demo-forward", |s| {
                demo::seed(s);
                s.start_forward(matrix_core::TimelineKey::room(demo::DEMO_THREAD.0), demo::DEMO_THREAD.1.into());
            }),
            ("demo-unread", demo::unread),
            ("demo-invitation", demo::invitation),
            ("demo-discovery", demo::discovery),
            ("demo-safety", demo::safety),
            ("demo-report", demo::report),
            ("demo-hidden-media", demo::hidden_media),
            ("demo-hidden-media-bubbles", |s| {
                demo::hidden_media(s);
                s.settings.message_layout = state::MessageLayout::Bubbles;
            }),
            ("demo-public-rooms", |s| {
                demo::discovery(s);
                s.browse_public_rooms();
            }),
            ("demo-invitations-home", |s| {
                demo::invitation(s);
                s.close_modal();
            }),
            ("demo-collapsed-unread", |s| {
                demo::seed(s);
                s.settings.appearance = Appearance::Light;
                let ada = matrix_core::AccountId::new(demo::ADA);
                s.accounts.set_sidebar(&ada, matrix_core::SidebarLayout::default());
                s.toggle_section(state::DIRECT);
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
            ("demo-bubbles", |s| {
                demo::seed(s);
                s.settings.message_layout = state::MessageLayout::Bubbles;
                s.select_room(demo::DEMO_THREAD.0);
            }),
            ("demo-bubbles-light", |s| {
                demo::seed(s);
                s.settings.appearance = Appearance::Light;
                s.settings.message_layout = state::MessageLayout::Bubbles;
                s.select_room(demo::DEMO_THREAD.0);
            }),
            ("demo-bubbles-dm", |s| {
                demo::seed(s);
                s.settings.message_layout = state::MessageLayout::Bubbles;
                s.select_room("!dm-grace:example.org");
            }),
            ("demo-settings-appearance", |s| {
                demo::seed(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::Appearance);
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
            ("demo-status-hidden", |s| {
                demo::seed(s);
                demo::hidden(s);
                s.toggle_popover(Popover::AccountSwitcher);
            }),
            ("demo-status-message", |s| {
                demo::seed(s);
                s.set_status_message(&matrix_core::AccountId::new(demo::ADA), "🌴 On vacation until Monday");
                s.toggle_popover(Popover::AccountSwitcher);
            }),
            ("demo-status-message-editing", |s| {
                demo::seed(s);
                s.toggle_popover(Popover::AccountSwitcher);
                s.edit_status_message();
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
            // A voice memo dragged a third of the way along while it downloads to play.
            ("demo-voice-memo", |s| {
                demo::seed(s);
                s.select_room("!dm-grace:example.org");
                let media = matrix_core::MediaRef::opaque(demo::DEMO_VOICE_MEMO);
                s.audio = Some(state::AudioClip {
                    id: 0,
                    media,
                    player: None,
                    start_at: Some(0.35),
                    play: true,
                    held: false,
                    scrub_from: None,
                    sender: None,
                });
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
            ("demo-emoji", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                demo::image_packs(s);
                s.toggle_popover(Popover::Emoji(state::EmojiFor::Composer(state::ComposerTarget::Room)));
            }),
            ("demo-emoji-monochrome", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.set_emoji_style(EmojiStyle::Monochrome);
                s.toggle_popover(Popover::Emoji(state::EmojiFor::Composer(state::ComposerTarget::Room)));
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
                    flow: state::VerificationFlow::Following("flow".into()),
                    step: matrix_core::VerificationStep::Compare(demo::verification_emojis()),
                    with: None,
                });
            }),
            ("demo-verify-qr", |s| {
                demo::seed(s);
                demo::verify_person_qr(s);
            }),
            ("demo-verify-scanned", |s| {
                demo::seed(s);
                demo::verify_person_qr(s);
                let step = matrix_core::VerificationStep::Scanned;
                let event = matrix_core::Event::Verification {
                    account: matrix_core::AccountId::new(demo::ADA),
                    flow_id: "flow".into(),
                    step,
                };
                s.apply(event, &mut rune::Tx::default());
            }),
            ("demo-verify-incoming-person", |s| {
                demo::seed(s);
                let user =
                    matrix_core::Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None };
                let step =
                    matrix_core::VerificationStep::Incoming { other_device: "GRACEPHONE".into(), user: Some(user) };
                let event = matrix_core::Event::Verification {
                    account: matrix_core::AccountId::new(demo::ADA),
                    flow_id: "flow".into(),
                    step,
                };
                s.apply(event, &mut rune::Tx::default());
            }),
            ("demo-person-verified", |s| {
                demo::seed(s);
                demo::person(s, Some(matrix_core::Identity { verified: true, changed: false }));
            }),
            ("demo-person-unverified", |s| {
                demo::seed(s);
                demo::person(s, Some(matrix_core::Identity { verified: false, changed: false }));
            }),
            ("demo-person-changed", |s| {
                demo::seed(s);
                demo::person(s, Some(matrix_core::Identity { verified: true, changed: true }));
            }),
            ("demo-person-no-identity", |s| {
                demo::seed(s);
                demo::person(s, None);
            }),
            ("demo-link-device", |s| {
                demo::seed(s);
                demo::link_device(s);
            }),
            ("demo-sessions-oauth", |s| {
                demo::seed(s);
                demo::link_device(s);
                s.modal = Some(state::Modal::Settings(SettingsSection::Sessions));
            }),
            ("demo-link-device-code", |s| {
                demo::seed(s);
                demo::link_device(s);
                if let Some(state::Modal::LinkDevice { flow, .. }) = &mut s.modal
                    && let Some(request) = flow.request
                {
                    flow.moved(request, matrix_core::QrLoginStep::EnterCheckCode);
                    flow.code.insert("4");
                }
            }),
            ("demo-key-export", |s| {
                demo::seed(s);
                s.open_account_settings();
                s.key_file_chosen(state::KeyTransfer::Export, "/home/ada/message-keys.txt".into());
            }),
            ("demo-key-import-done", |s| {
                demo::seed(s);
                s.open_account_settings();
                s.key_file_chosen(state::KeyTransfer::Import, "/home/ada/message-keys.txt".into());
                if let Some(state::Modal::KeyFile(file)) = &mut s.modal {
                    file.step = state::KeyFileStep::Done(i18n::tf!("keys.imported", imported = 412, total = 530));
                }
            }),
            ("demo-change-password", |s| {
                demo::seed(s);
                s.open_account_settings();
                s.open_change_password();
            }),
            ("demo-deactivate", |s| {
                demo::seed(s);
                s.open_account_settings();
                s.ask_to_deactivate();
                s.toggle_deactivate_erase();
            }),
            ("demo-offline-device-key", |s| {
                demo::seed(s);
                s.open_account_settings();
                s.set_offline_device(true);
            }),
            ("demo-sign-in-qr", demo::sign_in_qr),
            ("demo-register", demo::registration_form),
            ("demo-register-terms", demo::registration_terms),
            ("demo-register-email", demo::registration_email),
            ("demo-register-email-sent", demo::registration_email_sent),
            ("demo-sign-in-resume", demo::sign_in_resume),
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
            ("demo-search", demo::search),
            ("demo-search-empty", |s| {
                demo::seed(s);
                s.open_search();
            }),
            ("demo-search-rooms", |s| {
                demo::search(s);
                s.search.filter = state::SearchFilter::Rooms;
            }),
            ("demo-search-ranked", |s| {
                demo::search(s);
                s.apply(
                    matrix_core::Event::SearchUsage {
                        account: matrix_core::AccountId::new(demo::ADA),
                        usage: matrix_core::SearchUsage {
                            rooms: std::collections::BTreeMap::from([("!design:example.org".into(), 20)]),
                            ..matrix_core::SearchUsage::default()
                        },
                    },
                    &mut rune::Tx::default(),
                );
            }),
            ("demo-read-receipts", demo::read_receipts),
            ("demo-readers", |s| {
                demo::read_receipts(s);
                s.toggle_readers(&matrix_core::TimelineKey::room(demo::DEMO_THREAD.0), "$read-demo");
            }),
            ("demo-reader-menu", |s| {
                demo::read_receipts(s);
                s.toggle_readers(&matrix_core::TimelineKey::room(demo::DEMO_THREAD.0), "$read-demo");
                s.toggle_reader_menu("@kai:example.org");
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
            ("demo-person", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.show_side(state::SidePanel::Person(matrix_core::Person {
                    id: "@grace:example.org".into(),
                    name: "Grace Hopper".into(),
                    avatar: None,
                }));
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
            ("demo-call-gallery", |s| demo::call_gallery(s, 6)),
            ("demo-call-many", |s| demo::call_gallery(s, 30)),
            ("demo-call-direct", |s| demo::call_gallery(s, 2)),
            ("demo-call-no-sidebar", |s| {
                demo::call(s, true);
                s.toggle_call_sidebar();
            }),
            ("demo-call-layout-menu", |s| {
                demo::call(s, true);
                s.open_call_layout();
            }),
            ("demo-call-shares", demo::call_shares),
            ("demo-call-focus", |s| {
                demo::call_gallery(s, 6);
                s.set_call_layout(state::CallLayout::Focus(None));
            }),
            ("demo-call-video-chat", |s| {
                demo::call(s, true);
                s.toggle_call_chat();
            }),
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
            ("demo-many-spaces", demo::many_spaces),
            ("demo-update", |s| {
                demo::seed(s);
                s.update_found(demo::update());
            }),
            ("demo-update-many-spaces", |s| {
                demo::many_spaces(s);
                s.update_found(demo::update());
            }),
            ("demo-update-command", |s| {
                demo::seed(s);
                s.update_found(demo::package_update());
                s.install_update();
            }),
            ("demo-update-page", |s| {
                demo::seed(s);
                s.update_found(demo::page_update());
                s.install_update();
            }),
            ("demo-updating", |s| {
                demo::many_spaces(s);
                s.update_found(demo::update());
                s.install_update();
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
            ("demo-note-to-self", |s| {
                demo::seed(s);
                s.open_room(&matrix_core::AccountId::new(demo::ADA), "!notes:example.org");
            }),
            // An account without one: typing its name offers to make it.
            ("demo-note-to-self-search", |s| {
                demo::seed(s);
                s.select_account(&matrix_core::AccountId::new("@ada:hobby.chat"));
                s.search.field = rune::TextEdit::new("note");
                s.open_search();
            }),
            ("demo-new-room", |s| {
                demo::seed(s);
                s.select_scope(Scope::Space(demo::WORK_SPACE.into()));
                s.open_room_form(state::RoomForm::NewRoom { space: Some(demo::WORK_SPACE.into()) });
            }),
            ("demo-new-room-public", |s| {
                demo::seed(s);
                s.select_scope(Scope::Space(demo::WORK_SPACE.into()));
                s.open_room_form(state::RoomForm::NewRoom { space: Some(demo::WORK_SPACE.into()) });
                s.room_form_field(state::RoomFormField::Name).insert("release-notes");
                s.set_room_form_rule(matrix_core::JoinRule::Public);
                s.room_form_field(state::RoomFormField::Address).insert("release-notes");
                s.toggle_room_form_published();
            }),
            ("demo-new-space", |s| {
                demo::seed(s);
                s.open_room_form(state::RoomForm::NewSpace);
                s.room_form_field(state::RoomFormField::Name).insert("Difference Engines");
                s.set_room_form_rule(matrix_core::JoinRule::Knock);
            }),
            ("demo-room-settings", |s| demo::room_settings(s, state::RoomSettingsSection::General)),
            ("demo-room-settings-access", |s| demo::room_settings(s, state::RoomSettingsSection::Access)),
            ("demo-room-settings-addresses", |s| demo::room_settings(s, state::RoomSettingsSection::Addresses)),
            ("demo-room-settings-permissions", |s| {
                demo::room_settings(s, state::RoomSettingsSection::Permissions);
                s.edit_custom_level(matrix_core::Permission::Pins);
            }),
            ("demo-room-settings-advanced", |s| demo::room_settings(s, state::RoomSettingsSection::Advanced)),
            ("demo-room-settings-encrypt", |s| {
                demo::room_settings(s, state::RoomSettingsSection::Security);
                s.ask_room_settings(state::SettingsConfirm::Encrypt);
            }),
            ("demo-join-requests", |s| {
                demo::join_requests(s);
                s.toggle_members();
            }),
            ("demo-join-requests-info", |s| {
                demo::join_requests(s);
                s.toggle_side(state::SidePanel::Info);
            }),
            ("demo-member-level", |s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
                s.toggle_members();
                s.open_member_level("@kai:example.org");
            }),
            ("demo-tombstone", demo::tombstone),
            ("demo-space-rooms", |s| {
                demo::seed(s);
                s.open_space_rooms(demo::WORK_SPACE);
            }),
            ("demo-leave-space", |s| {
                demo::seed(s);
                s.ask_to_leave_space(demo::WORK_SPACE);
                s.toggle_leave_with("!design:example.org");
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
            ("demo-organised", demo::organised),
            ("demo-organised-menu", |s| {
                demo::organised(s);
                s.toggle_room_menu("!dm-grace:example.org");
            }),
            ("demo-settings-keywords", |s| {
                demo::organised(s);
                s.open_settings();
                s.show_settings_section(SettingsSection::Notifications);
            }),
            ("demo-dm-menu", |s| {
                demo::seed(s);
                s.toggle_room_menu("!dm-grace:example.org");
            }),
            ("demo-leave", |s| {
                demo::seed(s);
                s.ask_to_leave("!lab:example.org");
            }),
            ("demo-link-preview", demo::link_preview),
            ("demo-link-person", demo::link_person),
        ],
        // Inter for text; the system's monospace font for code, recovery keys and the sign-in
        // backdrop (Rune's default mono slot is Inter too).
        families: ("Inter", "ui-monospace"),
        fonts: &FONTS,
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
fn on_key(s: &mut State, key: &KeyEvent, tx: &mut Tx) -> bool {
    if rune::Keybind::new("mod+k").matches_tx(key, tx)
        && s.screen() == state::Screen::Chat
        && s.nav.account.is_some()
        && s.modal.is_none()
        && s.preview.is_none()
    {
        s.open_search();
        return true;
    }
    match key.key {
        Key::Escape if matches!(s.popover, Some(Popover::Select(_) | Popover::AttachmentMenu { .. })) => {
            s.close_popover();
            true
        }
        Key::Escape if s.preview.is_some() => {
            s.close_preview();
            true
        }
        // The preview's batch, round from either end; a focused scrubber took them already.
        Key::Left | Key::Right if s.popover.is_none() && s.preview.as_ref().is_some_and(|p| p.batch.len() > 1) => {
            s.step_preview(if key.key == Key::Left { -1 } else { 1 });
            true
        }
        Key::Text(ref typed) if typed == " " && s.popover.is_none() && s.preview.is_some() => {
            s.toggle_preview_playback();
            true
        }
        Key::Escape if s.modal.is_some() => {
            s.close_modal();
            true
        }
        Key::Escape if s.popover.is_some() => {
            s.close_popover();
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
    EmojiStyle::default().apply();
    let app = App::new(spec())
        .on_focus(|s, focused, _| s.focus_changed(focused))
        // Permalinks the system opens with the app: every link on macOS, `scheme://` ones from a
        // later launch on Linux and Windows.
        .on_open_url(|s, url, tx| state::navigate_now(s, tx, |s| s.open_permalink(url)))
        // A later launch on Linux or Windows: the links it handed on open, and Rune brings the
        // window forward.
        .on_reopen(|s, _, tx| {
            state::navigate_now(s, tx, |s| {
                s.take_handed_links();
            });
            false
        });
    #[cfg(target_os = "windows")]
    let app = app.on_window(platform::window_icon::set);
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
    // One process per user: a later launch leaves its permalinks for the running one and goes
    // (`platform::launch`). Before the engine starts, so two never share the stores.
    let handoff = dirs.cache().join("links");
    let launched = platform::launch::links(std::env::args().skip(1));
    if !launched.is_empty()
        && let Err(error) = platform::launch::hand_on(&handoff, &launched)
    {
        eprintln!("better-matrix: could not hand the link on: {error}");
    }
    if !rune::single_instance(APP_ID) {
        return;
    }
    // This is the app's process: what this launch (or one racing it) left is its own to open.
    let mut links = platform::launch::take(&handoff);
    for link in launched {
        if !links.contains(&link) {
            links.push(link);
        }
    }
    let handle = app.handle();
    let config = match CLIENT_URI.parse() {
        Ok(uri) => EngineConfig {
            data_dir: dirs.data().to_path_buf(),
            secrets: platform::system_secrets(dirs.data()),
            identity: ClientIdentity { name: "Better Matrix".into(), uri },
            oauth_redirect: matrix_core::OAuthRedirect::Loopback,
            // One instance at a time, and no push opens a store without it.
            stores: matrix_core::StoreSharing::Alone,
            // No camera to scan with: the desktop shows its code, or compares emoji.
            scans_qr: false,
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
            settings.emoji_style.apply();
            let scratch = dirs.cache().join("scratch");
            platform::reset_scratch(&scratch);
            let shell_scratch = scratch.clone();
            let stopped = engine.stopped();
            app.init(move || {
                let shell = Shell::native(handle, prefs, shell_scratch).with_handoff(handoff);
                let mut state = State::new(engine, shell, settings);
                // They wait for the first account's rooms.
                links.iter().for_each(|link| state.open_permalink(link));
                state
            })
            .run();
            // The app's state, and with it the engine's handle, is gone: the engine leaves a call
            // under way and stops.
            stopped.wait(QUIT_WAIT);
            let _ = std::fs::remove_dir_all(&scratch);
            // The stores are closed: a launch waiting for this process can take over now.
            rune::release_instance();
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

    #[test]
    fn switching_emoji_style_should_shape_and_draw_with_the_selected_noto_font() {
        use rune::text::parley::PositionedLayoutItem;

        let spec = spec();
        EmojiStyle::Color.apply();
        let mut text = rune::text::TextSystem::for_window(spec.fonts, spec.families);
        for (style, font, color) in
            [(EmojiStyle::Color, NOTO_COLOR_EMOJI, true), (EmojiStyle::Monochrome, NOTO_EMOJI, false)]
        {
            style.apply();
            text.follow_chosen();
            let layout = text.layout_rich("🙂", &[], rune::TextStyle::new(20.0, 400.0), 1.0, None);
            let font_matches = layout.lines().flat_map(|line| line.items()).find_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => Some(run.run().font().data.as_ref() == font),
                _ => None,
            });
            assert_eq!(font_matches, Some(true), "{style:?}");
            let mut quads = Vec::new();
            text.glyphs_for_layout(&layout, 1.0, [0.0, 0.0], false, &mut quads);
            assert_eq!(quads.iter().any(|quad| quad.color != 0), color, "{style:?}");
        }
    }

    #[test]
    fn an_emoji_without_noto_outlines_should_draw_in_grayscale_in_monochrome_mode() {
        let spec = spec();
        EmojiStyle::Monochrome.apply();
        let mut text = rune::text::TextSystem::for_window(spec.fonts, spec.families);
        let layout = text.layout_rich("🫩", &[], rune::TextStyle::new(24.0, 400.0), 1.0, None);
        let mut quads = Vec::new();
        text.glyphs_for_layout(&layout, 1.0, [0.0, 0.0], false, &mut quads);
        let bitmap = quads.iter().find(|quad| quad.color != 0).expect("grayscale bitmap");
        let pixels = text.color_glyph(bitmap.color).rgba();
        assert!(!pixels.is_empty() && pixels.chunks_exact(4).all(|rgba| rgba[0] == rgba[1] && rgba[1] == rgba[2]));
    }

    fn runtime(scene: fn(&mut State)) -> Runtime<State> {
        let mut s = State::detached();
        scene(&mut s);
        let mut rt = Runtime::new(s, ui::view, (1200.0, 760.0), Env { system_dark: true, scale: 1.0 });
        rt.poll = spec().poll;
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
    fn empty_space_beside_message_text_should_open_its_thread() {
        let mut rt = runtime(|s| {
            posted_message(s, matrix_core::MessageContent::Text { body: "A new message".into(), html: None })
        });
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        let words = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("A new message")).unwrap();
        let room = rt.presented("room").unwrap().0;
        let (x, y) = (room[0] + room[2] - 40.0, words.rect[1] + words.rect[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), rune::Cursor::Pointer, "blank space beside the text");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.active_thread().is_some());
    }

    fn posted_message(s: &mut State, content: matrix_core::MessageContent) {
        demo::seed(s);
        s.select_room(demo::DEMO_THREAD.0);
        s.apply(
            matrix_core::Event::TimelineChanged {
                account: matrix_core::AccountId::new(demo::ADA),
                key: matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
                diffs: vec![matrix_core::TimelineDiff::Reset(vec![matrix_core::TimelineEntry {
                    id: "hit-test".into(),
                    kind: matrix_core::EntryKind::Message(Box::new(matrix_core::Message {
                        event_id: "$hit-test".into(),
                        sender_id: "@grace:example.org".into(),
                        sender_name: "Grace".into(),
                        content,
                        ..Default::default()
                    })),
                }])],
            },
            &mut rune::Tx::default(),
        );
    }

    #[test]
    fn empty_space_beside_formatted_message_text_should_open_its_thread() {
        let mut rt = runtime(|s| {
            posted_message(
                s,
                matrix_core::MessageContent::Text {
                    body: "A new message".into(),
                    html: Some("A <strong>new message</strong>".into()),
                },
            )
        });
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        let words = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("A new message")).unwrap();
        let room = rt.presented("room").unwrap().0;
        let (x, y) = (room[0] + room[2] - 40.0, words.rect[1] + words.rect[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), rune::Cursor::Pointer);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.active_thread(), Some("$hit-test"));
    }

    #[test]
    fn selecting_message_text_should_keep_the_text_cursor_without_opening_a_thread() {
        let mut rt = runtime(|s| {
            posted_message(s, matrix_core::MessageContent::Text { body: "A new message".into(), html: None })
        });
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        let words = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("A new message")).unwrap();
        let (x, y) = (words.rect[0] + 1.0, words.rect[1] + words.rect[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), rune::Cursor::Text);
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_move(x + 70.0, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(x + 70.0, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.selected_text().is_some_and(|selected| !selected.is_empty()));
        assert!(rt.state.active_thread().is_none());
    }

    #[test]
    fn clicking_a_message_link_should_open_the_link_instead_of_a_thread() {
        let mut rt = runtime(|s| {
            posted_message(
                s,
                matrix_core::MessageContent::Text {
                    body: "A link".into(),
                    html: Some("A <a href='https://matrix.org'>link</a>".into()),
                },
            )
        });
        let (x, y) = rt.find("https://matrix.org").expect("the message link");
        rt.pointer_move(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.cursor(), rune::Cursor::Pointer);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.take_url_requests(), ["https://matrix.org"]);
        assert!(rt.state.active_thread().is_none());
    }

    #[test]
    fn clicking_message_media_should_keep_its_preview_action() {
        use matrix_core::{Attachment, AttachmentKind, MediaRef, MessageContent};
        for (name, kind, key) in [
            ("photo.png", AttachmentKind::Image, "image-photo.png"),
            ("notes.pdf", AttachmentKind::File, "file-notes.pdf"),
        ] {
            let mut rt = runtime(|_| {});
            rt.update(|s, _| {
                posted_message(
                    s,
                    MessageContent::Attachment(Attachment {
                        kind,
                        name: name.into(),
                        size: None,
                        media: Some(MediaRef::opaque(name)),
                        thumbnail: None,
                        dimensions: Some((100, 100)),
                        audio: None,
                    }),
                )
            });
            rt.frame(1.0 / 60.0);
            press(&mut rt, key);
            assert!(rt.state.preview.is_some(), "{name} should open its preview");
            assert!(rt.state.active_thread().is_none());
        }
    }

    #[test]
    fn opening_a_thread_should_keep_the_rooms_reading_position() {
        let mut rt = runtime(demo::stress);
        rt.poll = Some(|s, _, tx| s.poll(tx));
        rt.settle_all();
        rt.update(|_, tx| tx.scroll_to_instant("timeline", 0.0, 20_000.0));
        for _ in 0..120 {
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.scroll_at_bottom("timeline"), Some(false));
        let viewport = rt.presented("timeline").unwrap().0;
        let anchor = rt
            .elements()
            .into_iter()
            .find(|e| {
                e.key.as_deref().is_some_and(|key| key.starts_with("msg-"))
                    && e.rect[1] + e.rect[3] > viewport[1]
                    && e.rect[1] < viewport[1] + viewport[3]
            })
            .expect("a visible message");
        let (x, y) = (anchor.rect[0] + 5.0, anchor.rect[1].max(viewport[1]) + 5.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..120 {
            rt.frame(1.0 / 60.0);
        }
        assert!(rt.state.active_thread().is_some());
        let after = rt.presented(anchor.key.as_deref().unwrap()).expect("the same reading anchor").0;
        assert!((anchor.rect[1] - after[1]).abs() < 1.0, "anchor moved: {:?} -> {after:?}", anchor.rect);
        assert_eq!(rt.scroll_at_bottom("timeline"), Some(false));
        let root = rt.state.active_thread().expect("the opened thread").to_owned();
        let account = matrix_core::AccountId::new(demo::ADA);
        let key = matrix_core::TimelineKey::thread("!big:example.org", &root);
        let root = rt
            .state
            .accounts
            .get(&account)
            .and_then(|data| data.timeline(&key))
            .and_then(|timeline| timeline.messages().next())
            .expect("the seeded root")
            .clone();
        // The real engine sends a first snapshot, then may replace it during SDK catch-up.
        for snapshot in ["sdk-first", "sdk-catchup"] {
            rt.update(|s, tx| {
                s.apply(
                    matrix_core::Event::TimelineChanged {
                        account: account.clone(),
                        key: key.clone(),
                        diffs: vec![matrix_core::TimelineDiff::Reset(vec![matrix_core::TimelineEntry {
                            id: snapshot.into(),
                            kind: matrix_core::EntryKind::Message(Box::new(root.clone())),
                        }])],
                    },
                    tx,
                )
            });
            for _ in 0..120 {
                rt.frame(1.0 / 60.0);
            }
            assert_eq!(rt.scroll_at_bottom("timeline"), Some(false), "after {snapshot}");
            let after = rt.presented(anchor.key.as_deref().unwrap()).expect("the same reading anchor").0;
            assert!((anchor.rect[1] - after[1]).abs() < 1.0, "anchor moved after {snapshot}");
        }
    }

    #[test]
    fn long_drafts_should_scroll_inside_capped_room_and_thread_composers() {
        for (target, field, scroll) in [
            (state::ComposerTarget::Room, "composer", "composer-scroll"),
            (state::ComposerTarget::Thread, "thread-composer", "thread-composer-scroll"),
        ] {
            let mut rt = runtime(|s| {
                demo::seed(s);
                s.select_room(demo::DEMO_THREAD.0);
            });
            if target == state::ComposerTarget::Thread {
                rt.update(|s, _| s.open_thread(demo::DEMO_THREAD.1));
            }
            rt.settle_all();
            rt.frame(1.0 / 60.0);
            rt.paste("Another line\n".repeat(40));
            rt.settle_all();
            rt.frame(1.0 / 60.0);
            assert!(rt.state.composer(target).edit.text.lines().count() >= 40);
            let viewport = rt.presented(scroll).expect("the editor scroll viewport").0;
            assert!(viewport[3] <= 176.5, "the editor stops at eight lines: {viewport:?}");
            let input = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(field)).unwrap();
            let caret = input.caret.expect("the last line's caret");
            assert!(
                caret[1] >= viewport[1] - 1.0 && caret[1] + caret[3] <= viewport[1] + viewport[3] + 1.0,
                "the newest line stays visible: {caret:?}, viewport {viewport:?}"
            );
            let (x, y) = rt.find(scroll).unwrap();
            rt.wheel(x, y, 120.0);
            for _ in 0..60 {
                rt.frame(1.0 / 60.0);
            }
            let moved = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(field)).unwrap();
            assert!(moved.scrolled[1] < input.scrolled[1], "scrolling reaches earlier draft lines");
            assert_eq!(rt.scroll_at_bottom("timeline"), Some(true), "the chat must not scroll");
            rt.update(|s, _| {
                let edit = &mut s.composer_mut(target).edit;
                edit.set_selection(0, edit.text.len());
                edit.insert("Short draft");
            });
            rt.settle_all();
            rt.frame(1.0 / 60.0);
            let short = rt.presented(scroll).unwrap().0;
            assert!(short[3] < viewport[3], "removing lines shrinks the editor again");
        }
    }

    #[test]
    fn clicking_composer_padding_should_focus_its_editor() {
        let mut rt = runtime(|s| {
            demo::seed(s);
            s.select_room(demo::DEMO_THREAD.0);
            s.open_thread(demo::DEMO_THREAD.1);
        });
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        for (target, scroll) in [
            (state::ComposerTarget::Room, "composer-scroll"),
            (state::ComposerTarget::Thread, "thread-composer-scroll"),
        ] {
            let viewport = rt.presented(scroll).unwrap().0;
            let (x, y) = (viewport[0] + 8.0, viewport[1] + viewport[3] / 2.0);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(1.0 / 60.0);
            rt.paste("Typed after clicking padding");
            rt.frame(1.0 / 60.0);
            assert_eq!(rt.state.composer(target).edit.text, "Typed after clicking padding");
        }
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
    fn every_sent_message_should_have_a_more_menu() {
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
        assert!(rt.find("more-$demo-@grace:example.org-1").is_some());
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
    fn a_linked_room_the_user_can_join_should_offer_join() {
        let rt = runtime(demo::link_preview);
        assert!(rt.find("link-join").is_some() && rt.find("link-account-@ada:hobby.chat").is_some());
    }

    #[test]
    fn a_linked_person_should_offer_to_start_a_chat() {
        let rt = runtime(demo::link_person);
        assert!(rt.find("link-chat").is_some());
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
    fn recording_a_voice_memo_should_light_a_glow_around_the_composer() {
        let rt = runtime(recording);
        assert!(rt.find("voice-glow").is_some());
    }

    /// Grace's voice memo in the demo's chat with her.
    const MEMO: &str = "$demo-@grace:example.org-3";

    fn voice_memo_chat(s: &mut State) {
        demo::seed(s);
        s.select_room("!dm-grace:example.org");
    }

    #[test]
    fn a_voice_memo_should_show_its_waveform_and_length_rather_than_its_file_name() {
        let rt = runtime(voice_memo_chat);
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        assert!(rt.find(&format!("audio-wave-{MEMO}")).is_some());
        assert!(texts.iter().any(|t| t == "0:14") && !texts.iter().any(|t| t.ends_with(".wav")));
    }

    #[test]
    fn pressing_along_a_voice_memo_should_play_it_from_there() {
        let mut rt = runtime(voice_memo_chat);
        let wave = rt.presented(&format!("audio-wave-{MEMO}")).unwrap_or_else(|| panic!("no waveform")).0;
        let (x, y) = (wave[0] + wave[2] * 0.75, wave[1] + wave[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        let start = rt.state.audio.as_ref().and_then(|clip| clip.start_at).unwrap_or_default();
        assert!((start - 0.75).abs() < 0.02, "started at {start}");
    }

    #[test]
    fn pressing_the_outer_half_of_the_scrubber_dot_should_scrub_rather_than_open_the_thread() {
        let mut rt = runtime(voice_memo_chat);
        // The bars start half a dot into the slider; the dot at the start reaches as far back.
        let wave = rt.presented(&format!("audio-wave-{MEMO}")).unwrap_or_else(|| panic!("no waveform")).0;
        let (x, y) = (wave[0] + 2.0, wave[1] + wave[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        let start = rt.state.audio.as_ref().and_then(|clip| clip.start_at);
        assert_eq!((start, rt.state.active_thread().is_some()), (Some(0.0), false));
    }

    #[test]
    fn an_arrow_key_after_pressing_a_voice_memo_s_waveform_should_step_it() {
        let mut rt = runtime(voice_memo_chat);
        let wave = rt.presented(&format!("audio-wave-{MEMO}")).unwrap_or_else(|| panic!("no waveform")).0;
        let (x, y) = (wave[0] + wave[2] / 2.0, wave[1] + wave[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        let pressed = rt.state.audio.as_ref().map_or(0.0, state::AudioClip::progress);
        rt.key(rune::KeyEvent { key: rune::Key::Right, mods: rune::Mods::default() });
        rt.frame(1.0 / 60.0);
        let stepped = rt.state.audio.as_ref().map_or(0.0, state::AudioClip::progress);
        assert!((stepped - pressed - state::AUDIO_STEP).abs() < 1e-6, "{pressed} then {stepped}");
    }

    #[test]
    fn a_voice_memo_should_offer_its_speed_once_it_plays() {
        let mut rt = runtime(voice_memo_chat);
        assert!(rt.find(&format!("audio-speed-{MEMO}")).is_none());
        press(&mut rt, &format!("audio-play-{MEMO}"));
        press(&mut rt, &format!("audio-speed-{MEMO}"));
        assert_eq!(rt.state.settings.playback_speed, state::PlaybackSpeed::Faster);
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
    fn the_glow_should_leave_the_composer_pressable() {
        let mut rt = runtime(recording);
        press(&mut rt, "voice-cancel");
        assert!(rt.state.composer.voice.is_none() && rt.find("voice-glow").is_none());
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
    fn showing_chat_should_keep_the_call_stage_and_controls_available() {
        let mut rt = runtime(|s| demo::call(s, false));
        press(&mut rt, "call-chat");
        assert!(rt.find("timeline").is_some() && rt.find("call-stage").is_some() && rt.find("call-hang-up").is_some());
        press(&mut rt, "call-back");
        assert!(rt.find("call-stage").is_some());
    }

    #[test]
    fn selecting_a_call_tile_should_pin_that_surface() {
        let mut rt = runtime(|s| demo::call(s, true));
        press(&mut rt, "call-tile-alan");
        assert_eq!(
            rt.state.call.as_ref().unwrap().layout,
            state::CallLayout::Focus(Some(state::CallTile::Camera("alan".into())))
        );
    }

    #[test]
    fn choosing_automatic_layout_should_release_a_pin() {
        let mut rt = runtime(|s| demo::call(s, true));
        press(&mut rt, "call-tile-alan");
        press(&mut rt, "call-layout");
        press(&mut rt, "call-layout-auto");
        assert_eq!(rt.state.call.as_ref().unwrap().layout, state::CallLayout::Auto);
    }

    #[test]
    fn choosing_a_layout_from_the_dropdown_should_close_it() {
        let mut rt = runtime(|s| demo::call(s, true));
        assert!(rt.find("call-layout-grid").is_none());
        press(&mut rt, "call-layout");
        assert!(rt.find("call-layout-menu").is_some());
        press(&mut rt, "call-layout-grid");
        assert!(
            rt.state.call.as_ref().unwrap().layout == state::CallLayout::Grid && rt.find("call-layout-menu").is_none()
        );
    }

    #[test]
    fn the_call_layout_button_should_only_show_on_the_call_stage() {
        let mut rt = runtime(|s| demo::call(s, true));
        assert!(rt.find("call-layout").is_some());
        rt.update(|s, _| s.select_room("!design:example.org"));
        rt.frame(1.0 / 60.0);
        assert!(rt.find("call-layout").is_none() && rt.find("call-sidebar").is_none());
    }

    #[test]
    fn ordinary_rooms_should_offer_no_call_layout_or_sidebar_toggle() {
        let rt = runtime(|s| {
            demo::seed(s);
            s.select_room("!general:example.org");
        });
        assert!(rt.find("call-layout").is_none() && rt.find("call-sidebar").is_none());
    }

    #[test]
    fn voice_calls_should_offer_the_sidebar_toggle() {
        let rt = runtime(|s| demo::call(s, false));
        assert!(rt.find("call-sidebar").is_some());
    }

    #[test]
    fn the_sidebar_toggle_should_sit_before_the_channel_name() {
        let rt = runtime(|s| demo::call(s, true));
        let elements = rt.elements();
        let toggle = elements.iter().find(|el| el.key.as_deref() == Some("call-sidebar")).unwrap().rect;
        let name = elements.iter().find(|el| el.key.as_deref() == Some("room-name")).unwrap().rect;
        assert!(toggle[0] + toggle[2] <= name[0]);
    }

    #[test]
    fn showing_the_full_video_should_not_pin_the_tile() {
        let mut rt = runtime(|s| demo::call(s, true));
        press(&mut rt, "call-tile-alan-fit");
        let call = rt.state.call.as_ref().unwrap();
        assert!(
            call.layout == state::CallLayout::Auto
                && call.full_video.contains_key(&state::CallTile::Camera("alan".into()))
        );
    }

    #[test]
    fn a_wide_call_should_leave_room_for_chat_beside_it() {
        let mut rt = runtime(|s| {
            demo::call(s, true);
            s.toggle_call_chat();
        });
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
        }
        let elements = rt.elements();
        let stage = elements.iter().find(|el| el.key.as_deref() == Some("call-stage")).unwrap().rect;
        let chat = elements.iter().find(|el| el.key.as_deref() == Some("room-chat")).unwrap().rect;
        assert!(stage[0] + stage[2] <= chat[0] + 1.0 && chat[0] + chat[2] <= 1201.0 && chat[2] >= 300.0);
    }

    #[test]
    fn a_narrow_call_should_leave_chat_and_hang_up_in_view() {
        let mut rt = runtime(|s| {
            demo::call(s, true);
            s.toggle_call_chat();
        });
        rt.set_viewport((720.0, 480.0), 1.0);
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
        }
        let elements = rt.elements();
        for key in ["room-chat", "call-hang-up"] {
            let el = elements.iter().find(|el| el.key.as_deref() == Some(key)).unwrap();
            assert!(
                el.rect[2] > 0.0
                    && el.rect[3] > 0.0
                    && el.rect[0] + el.rect[2] <= 721.0
                    && el.rect[1] + el.rect[3] <= 481.0,
                "{key}: {:?}",
                el.rect
            );
        }
        let viewport = elements.iter().find(|el| el.key.as_deref() == Some("call-viewport")).unwrap();
        assert!(viewport.rect[3] >= 90.0, "the compact call should leave a readable video preview");
    }

    #[test]
    fn compact_video_chat_should_allow_selecting_the_next_surface() {
        let mut rt = runtime(|s| {
            demo::call_gallery(s, 4);
            s.toggle_call_chat();
        });
        rt.set_viewport((720.0, 480.0), 1.0);
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
        }
        let before = rt.state.call.as_ref().unwrap().plan().tiles[0].key.clone();
        press(&mut rt, "call-page-next");
        let call = rt.state.call.as_ref().unwrap();
        assert!(rt.find("room-chat").is_some() && call.chat && call.plan().tiles[0].key != before);
    }

    #[test]
    fn the_call_header_should_hide_and_restore_the_sidebar() {
        let mut rt = runtime(|s| demo::call(s, true));
        press(&mut rt, "call-sidebar");
        assert!(rt.find("sidebar-header").is_none() && rt.find("call-sidebar").is_some());
        press(&mut rt, "call-sidebar");
        assert!(rt.find("sidebar-header").is_some());
    }

    #[test]
    fn leaving_the_call_room_should_restore_the_sidebar() {
        let mut rt = runtime(|s| demo::call(s, true));
        press(&mut rt, "call-sidebar");
        rt.update(|s, _| s.select_room("!design:example.org"));
        rt.frame(1.0 / 60.0);
        assert!(rt.find("sidebar-header").is_some() && rt.state.call.is_some());
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
        assert!(rt.find("sign-in-modal").is_some());
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

    fn open_room_menu(rt: &mut Runtime<State>, room_id: &str) {
        let (x, y) = rt.find(&format!("room-{room_id}")).unwrap();
        rt.secondary_down(x, y);
        rt.secondary_up(x, y);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn adding_a_room_to_favourites_from_its_menu_should_list_it_under_favourites() {
        let mut rt = runtime(demo::seed);
        assert!(rt.find("section-favourites").is_none());
        open_room_menu(&mut rt, "!lab:example.org");
        press(&mut rt, "room-menu-favourite");
        assert!(rt.find("section-header-favourites").is_some());
        let room = rt.state.active_account().and_then(|a| a.room("!lab:example.org")).map(|r| r.priority);
        assert_eq!(room, Some(matrix_core::RoomPriority::Favourite));
    }

    #[test]
    fn moving_a_room_to_low_priority_from_its_menu_should_list_it_under_low_priority() {
        let mut rt = runtime(demo::seed);
        open_room_menu(&mut rt, "!lab:example.org");
        press(&mut rt, "room-menu-low-priority");
        assert!(rt.find("section-header-low_priority").is_some());
    }

    #[test]
    fn marking_a_quiet_room_unread_from_its_menu_should_draw_attention_to_it() {
        let mut rt = runtime(demo::seed);
        open_room_menu(&mut rt, "!lab:example.org");
        press(&mut rt, "room-menu-unread");
        let room = rt.state.active_account().and_then(|a| a.room("!lab:example.org")).cloned();
        assert!(room.is_some_and(|room| state::wants_attention(&room)));
    }

    #[test]
    fn keywords_in_settings_should_be_listed_with_a_way_to_add_more() {
        let mut rt = runtime(|s| {
            demo::organised(s);
            s.open_settings();
            s.show_settings_section(SettingsSection::Notifications);
        });
        assert!(rt.find("keyword-@ada:example.org-deploy").is_some());
        assert!(rt.find("keyword-new-@ada:example.org").is_some());
        press(&mut rt, "keyword-remove-@ada:example.org-deploy");
        assert!(rt.find("keyword-@ada:example.org-deploy").is_none());
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
