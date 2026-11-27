//! Sample accounts for headless renders (`--scene demo`) and UI tests. Fed through
//! [`State::apply`] like real engine events, so the demo exercises the same path.

use std::path::PathBuf;

use matrix_core::{
    AccountId, AccountSummary, Attachment, AttachmentKind, ContactDetail, ContactKind, Contacts, EntryKind, Event,
    HistoryVisibility, Homeserver, LoginMethods, Message, MessageContent, NotificationMode, OfflineDevice, Person,
    Policy, QrImage, QrLoginStep, Quote, Reaction, RecoveryStatus, RegistrationStep, RequestId, RoomChange, RoomEvent,
    RoomKind, RoomList, RoomSummary, SasEmoji, Security, Session, SignInMethod, SpaceSummary, ThreadSummary,
    TimelineDiff, TimelineEntry, TimelineKey,
};
use rune::Tx;

use crate::state::{Asked, Format, QrLogin, SignInStep, State};

pub const WORK_SPACE: &str = "!engines:example.org";

/// An overflowing rail, for checking spaces scrolling behind its bottom actions.
pub fn many_spaces(s: &mut State) {
    seed(s);
    let Some(account) = s.active_account() else { return };
    let id = account.summary.id.clone();
    let rooms = account.rooms.clone();
    let spaces = (0..20)
        .map(|index| SpaceSummary {
            room_id: format!("!space-{index}:example.org"),
            name: format!("Space {index:02}"),
            ..Default::default()
        })
        .collect();
    apply(s, Event::RoomsUpdated { account: id, list: RoomList { rooms, spaces, ..Default::default() } });
}

const GRACE: &str = "@grace:example.org";
const ALAN: &str = "@alan:example.org";
pub const ADA: &str = "@ada:example.org";

pub fn demo_attachment() -> Attachment {
    Attachment {
        kind: AttachmentKind::Image,
        name: "better-matrix.png".into(),
        size: Some(24_000),
        media: Some(matrix_core::MediaRef::opaque("demo-image")),
        thumbnail: None,
        dimensions: Some((256, 256)),
        audio: None,
        sticker: false,
    }
}

/// A sent image for exercising its file actions without an account or network.
pub fn attachment_message(s: &mut State) {
    image_message(s, demo_attachment());
}

/// `file`, a sent picture of the app's icon, as the room's only message.
pub fn image_message(s: &mut State, file: Attachment) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    apply(
        s,
        Event::TimelineChanged {
            account: AccountId::new(ADA),
            key: TimelineKey::room(DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(vec![TimelineEntry {
                id: "attachment".into(),
                kind: EntryKind::Message(Box::new(Message {
                    event_id: "$attachment".into(),
                    sender_id: GRACE.into(),
                    sender_name: "Grace Hopper".into(),
                    timestamp_ms: DEMO_DAY_MS,
                    content: MessageContent::Attachment(file.clone()),
                    ..Message::default()
                })),
            }])],
        },
    );
    s.media.attach(std::sync::Arc::new(|| {}));
    if let Some(media) = file.media {
        s.media.loaded(
            media,
            include_bytes!("../../../packaging/icons/better-matrix-256.png").as_slice().into(),
            file.dimensions,
        );
    }
}

/// The same image full size, with its right-click menu open over the viewer.
pub fn attachment_preview(s: &mut State) {
    attachment_message(s);
    let file = demo_attachment();
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), "$attachment", &file, &mut rune::Tx::default());
    let bytes = include_bytes!("../../../packaging/icons/better-matrix-256.png").as_slice().into();
    if let (Some(preview), Some(image)) = (&mut s.preview, s.media.decode(bytes)) {
        preview.content =
            crate::state::PreviewContent::Image { thumbnail: None, status: crate::state::ImageStatus::Decoding(image) };
    }
    s.popover = Some(crate::state::Popover::AttachmentMenu { at: (700, 350) });
}

/// The attachments Grace dropped into #general at once: a picture, a video, a recording and a
/// PDF, each its own message, as Matrix sends them.
pub fn batch_attachments() -> Vec<Attachment> {
    let file = |kind, name: &str, dimensions| Attachment {
        kind,
        name: name.into(),
        size: Some(1_200_000),
        media: Some(matrix_core::MediaRef::opaque(name)),
        thumbnail: None,
        dimensions,
        audio: None,
        sticker: false,
    };
    let mut recording = file(AttachmentKind::Audio, "standup.ogg", None);
    recording.audio = Some(matrix_core::AudioDetails {
        voice: false,
        duration_ms: Some(83_000),
        waveform: (0..100).map(|i| ((i * 37) % 100) as u16 * 10).collect(),
    });
    vec![
        file(AttachmentKind::Image, "better-matrix.png", Some((256, 256))),
        Attachment {
            thumbnail: Some(matrix_core::MediaRef::opaque("launch-poster")),
            ..file(AttachmentKind::Video, "launch.mp4", Some((1280, 720)))
        },
        recording,
        file(AttachmentKind::File, "Release notes.pdf", None),
    ]
}

/// [`batch_attachments`] as Grace's messages in #general, a minute apart, the picture and the
/// video's poster loaded.
pub fn batch_messages(s: &mut State) {
    post_batch(s, batch_attachments());
}

/// `files` as Grace's messages in #general, a minute apart, the first two previews loaded.
pub fn post_batch(s: &mut State, files: Vec<Attachment>) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    let entries = files
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, file)| TimelineEntry {
            id: format!("batch-{i}"),
            kind: EntryKind::Message(Box::new(Message {
                event_id: format!("$batch-{i}"),
                sender_id: GRACE.into(),
                sender_name: "Grace Hopper".into(),
                timestamp_ms: DEMO_DAY_MS + i as u64 * 60_000,
                content: MessageContent::Attachment(file),
                ..Message::default()
            })),
        })
        .collect();
    apply(
        s,
        Event::TimelineChanged {
            account: AccountId::new(ADA),
            key: TimelineKey::room(DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(entries)],
        },
    );
    s.media.attach(std::sync::Arc::new(|| {}));
    let icon: std::sync::Arc<[u8]> = include_bytes!("../../../packaging/icons/better-matrix-256.png").as_slice().into();
    for file in files.into_iter().take(2) {
        if let Some((media, _)) = file.preview() {
            s.media.loaded(media, icon.clone(), Some((256, 256)));
        }
    }
    if let Some(image) = s.media.decode(icon) {
        let _ = image.wait();
    }
    s.media.measure();
}

/// Opens the batch's `index`th attachment in the preview.
fn open_batch(s: &mut State, index: usize) {
    batch_messages(s);
    let file = batch_attachments().swap_remove(index);
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), &format!("$batch-{index}"), &file, &mut rune::Tx::default());
}

/// The batch's picture full size, the strip of the others under it.
pub fn preview_batch(s: &mut State) {
    open_batch(s, 0);
    let bytes = include_bytes!("../../../packaging/icons/better-matrix-256.png").as_slice().into();
    if let (Some(preview), Some(image)) = (&mut s.preview, s.media.decode(bytes)) {
        preview.content =
            crate::state::PreviewContent::Image { thumbnail: None, status: crate::state::ImageStatus::Decoding(image) };
    }
}

/// The batch's recording, opened with pictures and videos held back everywhere, stepped back
/// onto the held-back video: Show, and nothing fetched.
pub fn preview_held(s: &mut State) {
    batch_messages(s);
    let safety = matrix_core::Safety { media: matrix_core::ShowMedia::Never, ..matrix_core::Safety::default() };
    apply(s, Event::Safety { account: AccountId::new(ADA), safety });
    let file = batch_attachments().swap_remove(2);
    s.open_attachment(&TimelineKey::room(DEMO_THREAD.0), "$batch-2", &file, &mut rune::Tx::default());
    s.show_preview_item(1);
}

/// The batch's video, a third of the way through and paused, with its controls.
pub fn preview_video(s: &mut State) {
    open_batch(s, 1);
    let player = crate::platform::video::Detached::new(Some(94.0), Some((1280, 720)));
    let player = crate::platform::video::Player::Detached(player);
    player.seek(1.0 / 3.0);
    player.pause();
    if let Some(crate::state::Preview { content: crate::state::PreviewContent::Video { status, .. }, .. }) =
        &mut s.preview
    {
        *status = crate::state::VideoStatus::Open(player);
    }
}

/// The batch's recording, ready to play.
pub fn preview_audio(s: &mut State) {
    open_batch(s, 2);
}

/// A divider between the last few messages, for inspecting its fade in place.
pub fn unread(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    if let Some(timeline) = s.accounts.timeline_mut(&AccountId::new(ADA), &TimelineKey::room(DEMO_THREAD.0)) {
        timeline.apply(vec![TimelineDiff::Insert {
            index: timeline.entries.len().saturating_sub(2),
            entry: TimelineEntry { id: "new-marker".into(), kind: EntryKind::ReadMarker },
        }]);
    }
}

/// The user hidden from others: their status, as the engine reports it once saved.
pub fn hidden(s: &mut State) {
    apply(s, Event::Status { account: AccountId::new(ADA), status: matrix_core::Status::Hidden });
}

/// A received invitation, for checking its sidebar entry and preview.
pub fn invitation(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    apply(s, Event::SidebarLayout { account: account.clone(), layout: matrix_core::SidebarLayout::default() });
    let Some(data) = s.accounts.get(&account) else { return };
    let list = RoomList {
        rooms: data.rooms.iter().filter(|room| room.room_id != "!design:example.org").cloned().collect(),
        spaces: data.spaces.clone(),
        invitations: vec![matrix_core::Invitation {
            room_id: "!design:example.org".into(),
            name: "Design".into(),
            topic: Some("A place to share ideas, review designs, and build Better Matrix together.".into()),
            avatar: None,
            inviter: Some(Person { id: GRACE.into(), name: "Grace Hopper".into(), avatar: None }),
            space: false,
            kind: RoomKind::Group,
            stranger: false,
        }],
    };
    apply(s, Event::RoomsUpdated { account, list });
    s.show_invitation("!design:example.org");
}

/// Settings → Safety: someone ignored, media shown in private rooms only, and a stranger's
/// invitation hidden there rather than in the sidebar.
pub fn safety(s: &mut State) {
    invitation(s);
    s.close_modal();
    let account = AccountId::new(ADA);
    let Some(data) = s.accounts.get(&account) else { return };
    let mut list =
        RoomList { rooms: data.rooms.clone(), spaces: data.spaces.clone(), invitations: data.invitations.clone() };
    list.invitations.push(matrix_core::Invitation {
        room_id: "!prizes:spam.example".into(),
        name: "Claim your prize".into(),
        topic: None,
        avatar: None,
        inviter: Some(Person { id: "@winner:spam.example".into(), name: "Prize desk".into(), avatar: None }),
        space: false,
        kind: RoomKind::Group,
        stranger: true,
    });
    apply(s, Event::RoomsUpdated { account: account.clone(), list });
    let safety = matrix_core::Safety {
        ignored: vec!["@troll:spam.example".into()],
        media: matrix_core::ShowMedia::Private,
        invite_pictures: false,
        hide_stranger_invites: true,
    };
    apply(s, Event::Safety { account, safety });
    s.open_settings();
    s.show_settings_section(crate::state::SettingsSection::Safety);
}

/// Reporting someone else's message.
pub fn report(s: &mut State) {
    attachment_message(s);
    s.report_message(&TimelineKey::room(DEMO_THREAD.0), "$attachment");
}

/// A picture in a public room, held back by media shown in private rooms only.
pub fn hidden_media(s: &mut State) {
    attachment_message(s);
    let account = AccountId::new(ADA);
    let Some(data) = s.accounts.get(&account) else { return };
    let mut list =
        RoomList { rooms: data.rooms.clone(), spaces: data.spaces.clone(), invitations: data.invitations.clone() };
    for room in list.rooms.iter_mut().filter(|room| room.room_id == DEMO_THREAD.0) {
        room.public = true;
    }
    apply(s, Event::RoomsUpdated { account: account.clone(), list });
    let safety = matrix_core::Safety { media: matrix_core::ShowMedia::Private, ..matrix_core::Safety::default() };
    apply(s, Event::Safety { account, safety });
}

/// A space with public rooms to discover and join.
pub fn discovery(s: &mut State) {
    seed(s);
    s.select_scope(crate::state::Scope::Space(WORK_SPACE.into()));
    let rooms =
        [("announcements", 250), ("design", 120), ("help", 80), ("desktop", 60), ("testing", 40), ("off-topic", 20)]
            .into_iter()
            .map(|(name, member_count)| matrix_core::PublicRoom {
                room_id: format!("!public-{name}:example.org"),
                name: name.into(),
                topic: Some("Meet the community and share what you are working on.".into()),
                avatar: None,
                member_count,
                via: vec!["example.org".into()],
            })
            .collect();
    s.directories.insert((AccountId::new(ADA), WORK_SPACE.into()), crate::state::Directory::Ready(rooms));
}
/// A link with a preview card in the demo.
const RELEASE_NOTES: &str = "https://bettermatrix.example.org/blog/0.2";
const KAI: &str = "@kai:example.org";

/// The demo, organised: Grace's chat among the favourites, lab moved to low priority, Alan's muted
/// chat marked unread, and two notification keywords.
pub fn organised(s: &mut State) {
    seed(s);
    let ada = AccountId::new(ADA);
    s.accounts.set_priority(&ada, "!dm-grace:example.org", matrix_core::RoomPriority::Favourite);
    s.accounts.set_priority(&ada, "!lab:example.org", matrix_core::RoomPriority::Low);
    s.accounts.set_marked(&ada, "!dm-alan:example.org", true);
    s.accounts.set_keywords(&ada, vec!["deploy".into(), "release".into()]);
}

pub fn seed(s: &mut State) {
    add_account(s, ADA, "Ada Lovelace", "example.org", SignInMethod::Password, work());
    // The release notes' card, as the homeserver would describe the page.
    s.link_previews.loaded(
        (AccountId::new(ADA), RELEASE_NOTES.into()),
        Some(matrix_core::links::LinkPreview {
            site: Some("Better Matrix".into()),
            title: Some("Better Matrix 0.2: calls, polls and a member list".into()),
            description: Some(
                "Voice and video calls that work with Element Call, polls in every chat, and a member list \
                 with moderation."
                    .into(),
            ),
            image: Some((matrix_core::MediaRef::opaque("demo-release-notes"), Some((1200, 630)))),
        }),
    );
    add_account(s, "@ada:hobby.chat", "ada", "hobby.chat", SignInMethod::OAuth, hobby());
    let details = vec![
        ContactDetail { kind: ContactKind::Email, address: "ada@example.org".into() },
        ContactDetail { kind: ContactKind::Phone, address: "4915112345678".into() },
    ];
    apply(s, Event::ContactDetails { account: AccountId::new(ADA), contacts: Contacts { details, changeable: true } });
    // The hobby server's accounts are managed by an OAuth service, which owns contact details.
    let details = vec![ContactDetail { kind: ContactKind::Email, address: "ada@hobby.chat".into() }];
    let contacts = Contacts { details, changeable: false };
    apply(s, Event::ContactDetails { account: AccountId::new("@ada:hobby.chat"), contacts });
    use matrix_core::{MemberRole, Presence, RoomMember};
    let member = |id: &str, name: &str, role: MemberRole, presence: Presence, verified: bool| RoomMember {
        person: Person { id: id.into(), name: name.into(), avatar: None },
        role,
        presence,
        status_message: None,
        verified,
        level: role.level(),
    };
    let people = vec![
        member(ADA, "Ada Lovelace", MemberRole::Admin, Presence::Online, true),
        member("@grace:example.org", "Grace Hopper", MemberRole::Moderator, Presence::Online, true),
        member("@kai:example.org", "Kai", MemberRole::Member, Presence::Away, false),
        member("@alan:example.org", "Alan Turing", MemberRole::Member, Presence::Offline, false),
    ];
    let members = matrix_core::Members {
        people,
        complete: true,
        can_invite: true,
        own_level: 100,
        can_kick: true,
        can_ban: true,
        can_set_roles: true,
        ..Default::default()
    };
    apply(s, Event::Members { request: None, account: AccountId::new(ADA), room_id: DEMO_THREAD.0.into(), members });
    // What people show, as the engine reports it at start: Kai is away on vacation.
    let shown = |id: &str, presence: Presence, message: Option<&str>| matrix_core::PersonPresence {
        user_id: id.into(),
        presence,
        status_message: message.map(Into::into),
    };
    let people = vec![
        shown("@grace:example.org", Presence::Online, Some("🐛 Chasing the sync bug")),
        shown("@kai:example.org", Presence::Away, Some("🌴 On vacation until Monday")),
        shown("@alan:example.org", Presence::Offline, None),
    ];
    apply(s, Event::Presence { account: AccountId::new(ADA), people });
    let verified =
        Security { session_verified: true, recovery: RecoveryStatus::Enabled, offline_device: OfflineDevice::Off };
    apply(s, Event::SecurityChanged { account: AccountId::new(ADA), security: verified });
    let fresh =
        Security { session_verified: false, recovery: RecoveryStatus::Enabled, offline_device: OfflineDevice::Off };
    apply(s, Event::SecurityChanged { account: AccountId::new("@ada:hobby.chat"), security: fresh });
    apply(s, Event::Sessions { account: AccountId::new(ADA), sessions: sessions() });
    s.select_account(&AccountId::new(ADA));
}

/// Ada's sessions: this desktop, her phone, and an old laptop nobody verified.
fn sessions() -> Vec<Session> {
    let session = |id: &str, name: &str, days_ago: u64, ip: &str, current: bool, verified: bool| Session {
        device_id: id.into(),
        name: Some(name.into()),
        last_seen_ms: Some(DEMO_DAY_MS + 10 * 3_600_000 - days_ago * 86_400_000),
        last_seen_ip: Some(ip.into()),
        current,
        verified,
    };
    vec![
        session("BMDESKTOP", "Better Matrix on Linux", 0, "203.0.113.7", true, true),
        session("PHONE42", "Element X on Pixel", 1, "198.51.100.23", false, true),
        session("OLDLAPTOP", "Firefox on Windows", 40, "192.0.2.80", false, false),
    ]
}

/// The emoji a verification asks to compare.
pub fn verification_emojis() -> Vec<SasEmoji> {
    [
        ("🐶", "Dog"),
        ("🔑", "Key"),
        ("🎸", "Guitar"),
        ("🌍", "Globe"),
        ("🚀", "Rocket"),
        ("🍕", "Pizza"),
        ("⚓", "Anchor"),
    ]
    .iter()
    .map(|(symbol, description)| SasEmoji { symbol: (*symbol).into(), description: (*description).into() })
    .collect()
}

fn homeserver(oauth: bool, password: bool) -> Homeserver {
    Homeserver {
        server_name: Some("example.org".into()),
        url: "https://matrix.example.org/".into(),
        methods: LoginMethods { oauth, password, create_account: true, qr: oauth },
    }
}

/// A QR code the size of a real one (MSC4108 and verification codes carry about 150 bytes).
pub fn qr_code() -> QrImage {
    let data: Vec<u8> = b"MATRIX\x02\x03".iter().copied().chain((0..140u8).map(|i| i.wrapping_mul(37))).collect();
    QrImage::encode(&data).unwrap_or(QrImage { width: 0, dark: Vec::new() })
}

/// Verifying Grace from her profile, both sides ready: this side shows its code. After [`seed`].
pub fn verify_person_qr(s: &mut State) {
    s.select_room(DEMO_THREAD.0);
    let grace = Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None };
    s.verify_person(&grace);
    if let Some(crate::state::Modal::Verification {
        flow: crate::state::VerificationFlow::Requested(request), ..
    }) = &s.modal
    {
        let request = *request;
        apply(s, Event::VerificationRequested { request, account: AccountId::new(ADA), flow_id: "flow".into() });
    }
    let step = matrix_core::VerificationStep::Ready { qr: Some(qr_code()), scan: false };
    apply(s, Event::Verification { account: AccountId::new(ADA), flow_id: "flow".into(), step });
}

/// Grace's profile beside the room, with whether Ada verified her. After [`seed`].
pub fn person(s: &mut State, identity: Option<matrix_core::Identity>) {
    s.select_room(DEMO_THREAD.0);
    s.show_side(crate::state::SidePanel::Person(Person {
        id: "@grace:example.org".into(),
        name: "Grace Hopper".into(),
        avatar: None,
    }));
    apply(s, Event::Identity { account: AccountId::new(ADA), user_id: "@grace:example.org".into(), identity });
}

/// Ada's OAuth account, verified here, showing a code for a new device to scan. After [`seed`].
pub fn link_device(s: &mut State) {
    let hobby = AccountId::new("@ada:hobby.chat");
    let verified =
        Security { session_verified: true, recovery: RecoveryStatus::Enabled, offline_device: OfflineDevice::On };
    apply(s, Event::SecurityChanged { account: hobby.clone(), security: verified });
    s.select_account(&hobby);
    s.open_settings();
    s.show_settings_section(crate::state::SettingsSection::Sessions);
    s.link_device();
    if let Some(crate::state::Modal::LinkDevice { flow: QrLogin { request: Some(request), .. }, .. }) = &s.modal {
        let request = *request;
        apply(s, Event::QrLogin { request, step: QrLoginStep::Show(qr_code()) });
    }
}

/// Signing in on a new device with a QR code, showing it for a signed-in device to scan.
pub fn sign_in_qr(s: &mut State) {
    let mut flow = QrLogin::started(RequestId(1));
    flow.moved(RequestId(1), QrLoginStep::Show(qr_code()));
    s.sign_in.step = SignInStep::Qr { server: homeserver(true, false), flow: Box::new(flow) };
}

/// Creating an account on a password server, which asks to agree to its policies.
pub fn registration_terms(s: &mut State) {
    let policies = vec![
        Policy { name: "Terms of Service".into(), url: "https://example.org/terms".into() },
        Policy { name: "Privacy Policy".into(), url: "https://example.org/privacy".into() },
    ];
    s.sign_in.step = SignInStep::Register {
        server: homeserver(false, true),
        request: Some(RequestId(1)),
        asked: Some(Asked { step: RegistrationStep::Terms(policies), answered: false }),
        error: None,
    };
}

/// Creating an account on a password server, which sent a link to confirm the email address
/// and hasn't seen it opened yet.
pub fn registration_email_sent(s: &mut State) {
    let step = RegistrationStep::EmailSent { address: "ada@hobby.chat".into(), unconfirmed: true };
    s.sign_in.step = SignInStep::Register {
        server: homeserver(false, true),
        request: Some(RequestId(1)),
        asked: Some(Asked { step, answered: false }),
        error: None,
    };
}

/// Creating an account on a password server, which asks for an email address to confirm.
pub fn registration_email(s: &mut State) {
    s.sign_in.step = SignInStep::Register {
        server: homeserver(false, true),
        request: Some(RequestId(1)),
        asked: Some(Asked { step: RegistrationStep::Email, answered: false }),
        error: None,
    };
    s.sign_in.email = rune::TextEdit::new("ada@hobby.chat");
}

/// Signing in again as a suspended account on an OAuth server: no QR code, no new account.
pub fn sign_in_resume(s: &mut State) {
    sign_in_browser(s);
    s.sign_in.resume = Some(AccountId::new("@ada:hobby.chat"));
}

/// The registration form on a password server.
pub fn registration_form(s: &mut State) {
    s.sign_in.step = SignInStep::Register { server: homeserver(false, true), request: None, asked: None, error: None };
    s.sign_in.username = rune::TextEdit::new("ada");
}

/// Sign-in step 2 on a password-only server.
pub fn sign_in_password(s: &mut State) {
    s.sign_in.step = SignInStep::Password { server: homeserver(false, true), submitting: None, error: None };
}

/// Sign-in step 2 on an OAuth server, waiting for the browser.
pub fn sign_in_browser(s: &mut State) {
    s.sign_in.step = SignInStep::Browser {
        server: homeserver(true, true),
        request: Some(matrix_core::RequestId(1)),
        url: Some("https://auth.example.org/authorize".into()),
        create: false,
        error: None,
    };
}

fn apply(s: &mut State, event: Event) {
    s.apply(event, &mut Tx::default());
}

/// The threads and files of #general, as the info panel lists them.
pub fn room_history(s: &mut State) {
    let room_id = "!general:example.org".to_owned();
    let thread = matrix_core::ThreadInfo {
        root: DEMO_THREAD.1.into(),
        sender_name: "Alan Turing".into(),
        sender_id: ALAN.into(),
        body: "Cross-signing verification passes on Linux, macOS and Windows.".into(),
        replies: 3,
        latest_at: Some(DEMO_DAY_MS + 11 * 3_600_000 + 36 * 60_000),
    };
    apply(s, Event::Threads { account: AccountId::new(ADA), room_id: room_id.clone(), threads: vec![thread] });
    let file = |name: &str, kind: AttachmentKind, (id, who): (&str, &str), minutes: u64| matrix_core::SharedFile {
        event_id: format!("$file-{name}"),
        sender_name: who.into(),
        sender_id: id.into(),
        timestamp: DEMO_DAY_MS + 10 * 3_600_000 + minutes * 60_000,
        attachment: Attachment {
            kind,
            name: name.into(),
            size: Some(1_450_000),
            media: None,
            thumbnail: None,
            dimensions: None,
            audio: None,
            sticker: false,
        },
    };
    let files = vec![
        file("release-notes-0.2.pdf", AttachmentKind::File, (ADA, "Ada Lovelace"), 33),
        file("screenshot-sidebar.png", AttachmentKind::Image, (GRACE, "Grace Hopper"), 12),
        file("standup.ogg", AttachmentKind::Audio, (KAI, "Kai"), 2),
    ];
    apply(s, Event::Files { account: AccountId::new(ADA), room_id, files });
}

/// Grace's poll on when to release: open, results showing, Ada's vote on Friday.
fn release_poll() -> MessageContent {
    let answer = |id: &str, text: &str, votes: u32, mine: bool| matrix_core::PollAnswer {
        id: id.into(),
        text: text.into(),
        votes,
        mine,
    };
    MessageContent::Poll(matrix_core::Poll {
        question: "When do we release 0.2?".into(),
        answers: vec![
            answer("0", "Friday", 2, true),
            answer("1", "Monday", 1, false),
            answer("2", "After the 0.3 fixes", 0, false),
        ],
        disclosed: true,
        max_selections: 1,
        voters: 3,
        ended: false,
    })
}

/// A call under way in #general: faces for a voice call, the video grid (with a shared screen
/// and painted frames standing in for cameras) for a video call.
pub fn call(s: &mut State, video: bool) {
    use matrix_core::{CallInfo, CallParticipant, CallPhase, CallVia, VideoId};
    seed(s);
    s.select_scope(crate::state::Scope::Space(WORK_SPACE.into()));
    s.select_room("!general:example.org");
    let person = |id: &str, name: &str| Person { id: id.into(), name: name.into(), avatar: None };
    let participant = |key: &str, who: Person, muted: bool, camera: Option<u64>, screen: Option<u64>| CallParticipant {
        key: key.into(),
        person: who,
        muted,
        camera: camera.map(VideoId),
        screen: screen.map(VideoId),
    };
    let participants = vec![
        participant("grace", person(GRACE, "Grace Hopper"), false, video.then_some(1), video.then_some(2)),
        participant("kai", person(KAI, "Kai Ito"), true, None, None),
        participant("alan", person(ALAN, "Alan Turing"), false, video.then_some(3), None),
    ];
    let info = CallInfo {
        call: 1,
        account: AccountId::new(ADA),
        room_id: "!general:example.org".into(),
        via: CallVia::Rtc,
        phase: CallPhase::Connected,
        video,
        microphone: true,
        camera: video.then_some(VideoId(4)),
        screen: None,
        participants,
    };
    apply(s, Event::CallUpdated(Box::new(info)));
    apply(s, Event::CallLevels(vec![("grace".into(), 190), (String::new(), 40)]));
    paint_call(s);
}

/// Camera-only calls, including a crowded gallery and a one-to-one self-view.
pub fn call_gallery(s: &mut State, people: usize) {
    call(s, true);
    let Some(mut info) = s.call.as_ref().map(|call| call.info.clone()) else { return };
    for person in &mut info.participants {
        person.screen = None;
    }
    info.participants.truncate(people.saturating_sub(1));
    while info.participants.len() + 1 < people {
        let n = info.participants.len();
        info.participants.push(matrix_core::CallParticipant {
            key: format!("guest-{n}"),
            person: Person { id: format!("@guest-{n}:example.org"), name: format!("Guest {n}"), avatar: None },
            muted: n % 3 == 0,
            camera: Some(matrix_core::VideoId(n as u64 + 10)),
            screen: None,
        });
    }
    apply(s, Event::CallUpdated(Box::new(info)));
    paint_call(s);
}

/// Two simultaneous shares, one portrait, available in Auto and Grid.
pub fn call_shares(s: &mut State) {
    call(s, true);
    let Some(mut info) = s.call.as_ref().map(|call| call.info.clone()) else { return };
    if let Some(person) = info.participants.last_mut() {
        person.screen = Some(matrix_core::VideoId(5));
    }
    apply(s, Event::CallUpdated(Box::new(info)));
    paint_call(s);
}

fn paint_call(s: &mut State) {
    if let Some(call) = s.call.as_mut() {
        // Painted frames in place of cameras: a soft gradient in each tile's own hue.
        for (id, pixels) in &call.videos {
            let (w, h) = match id.0 {
                2 => (640u32, 400u32),
                5 => (180, 320),
                _ => (320, 180),
            };
            let hue = (id.0 as f32 * 0.23).fract();
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    let (u, v) = (x as f32 / w as f32, y as f32 / h as f32);
                    let r = (0.25 + 0.5 * hue + 0.2 * u) * 255.0;
                    let g = (0.35 + 0.3 * v) * 255.0;
                    let b = (0.55 + 0.35 * (1.0 - hue) - 0.2 * v) * 255.0;
                    rgba.extend_from_slice(&[
                        r.clamp(0.0, 255.0) as u8,
                        g.clamp(0.0, 255.0) as u8,
                        b.clamp(0.0, 255.0) as u8,
                        255,
                    ]);
                }
            }
            pixels.set(w, h, rgba);
        }
        call.measure_videos();
    }
}

/// The devices a desk might have: a headset beside the built-in ones, the headset picked.
pub fn devices(s: &mut State) {
    let device = |name: &str| matrix_core::MediaDevice { name: name.into() };
    let devices = matrix_core::CallDevices {
        microphones: vec![device("Built-in Microphone"), device("USB Headset Microphone")],
        speakers: vec![device("Built-in Speakers"), device("USB Headset")],
        cameras: vec![device("Integrated Camera")],
    };
    apply(s, Event::CallDevices(devices));
    s.settings.devices.microphone = Some("USB Headset Microphone".into());
}

/// A video call ringing from Grace.
pub fn ringing(s: &mut State) {
    seed(s);
    let caller = Person { id: GRACE.into(), name: "Grace Hopper".into(), avatar: None };
    let ring = matrix_core::IncomingCall {
        account: AccountId::new(ADA),
        room_id: "!dm-grace:example.org".into(),
        caller,
        video: true,
        via: matrix_core::CallVia::Rtc,
    };
    apply(s, Event::IncomingCall(Box::new(ring)));
}

/// The demo with a formatted draft and two attachments in the composer.
pub fn compose(s: &mut State) {
    seed(s);
    s.select_scope(crate::state::Scope::Space(WORK_SPACE.into()));
    s.select_room("!general:example.org");
    let edit = &mut s.composer.edit;
    edit.insert("Release notes are ready, see the attached changelog.");
    edit.set_selection(0, 13);
    s.composer.format(Format::Bold);
    s.composer.edit.set_caret(s.composer.edit.text.len());
    s.composer.attach([PathBuf::from("CHANGELOG.md"), PathBuf::from("screenshot.png")]);
}

struct Fixture {
    rooms: Vec<(RoomSummary, Vec<Message>)>,
    spaces: Vec<SpaceSummary>,
    /// Threads: the room, the root message, and its replies.
    threads: Vec<(&'static str, Message, Vec<Message>)>,
}

/// The thread demo scenes open: Alan's cross-signing report in #general.
pub const DEMO_THREAD: (&str, &str) = ("!general:example.org", "$demo-@alan:example.org-5");

fn add_account(s: &mut State, user_id: &str, name: &str, server: &str, sign_in: SignInMethod, fixture: Fixture) {
    let account = AccountId::new(user_id);
    apply(
        s,
        Event::AccountAdded(AccountSummary {
            id: account.clone(),
            display_name: name.into(),
            homeserver: server.into(),
            default_notifications: NotificationMode::AllMessages,
            avatar: None,
            sign_in,
        }),
    );
    let list = RoomList {
        rooms: fixture.rooms.iter().map(|(room, _)| room.clone()).collect(),
        spaces: fixture.spaces,
        invitations: Vec::new(),
    };
    apply(s, Event::RoomsUpdated { account: account.clone(), list });
    for (room, messages) in fixture.rooms {
        if let Some(last) = messages.last() {
            apply(
                s,
                Event::LatestMessage {
                    account: account.clone(),
                    room_id: room.room_id.clone(),
                    message: Box::new(last.clone()),
                },
            );
        }
        seed_timeline(s, &account, &TimelineKey::room(&room.room_id), timeline(messages));
    }
    for (room_id, root, replies) in fixture.threads {
        let key = TimelineKey::thread(room_id, root.event_id.clone());
        let mut entries = vec![
            TimelineEntry { id: "start".into(), kind: EntryKind::Start },
            TimelineEntry { id: root.event_id.clone(), kind: EntryKind::Message(Box::new(root)) },
        ];
        entries.extend(
            replies
                .into_iter()
                .map(|m| TimelineEntry { id: m.event_id.clone(), kind: EntryKind::Message(Box::new(m)) }),
        );
        seed_timeline(s, &account, &key, TimelineDiff::Reset(entries));
    }
}

/// Fills a timeline as if it had been open: the state drops changes to timelines that aren't.
fn seed_timeline(s: &mut State, account: &AccountId, key: &TimelineKey, diff: TimelineDiff) {
    if let Some(timeline) = s.accounts.timeline_mut(account, key) {
        timeline.apply(vec![diff]);
    }
}

/// A whole timeline: its start, the day's divider, then the messages.
fn timeline(messages: Vec<Message>) -> TimelineDiff {
    let day = messages.first().map_or(0, |m| m.timestamp_ms - m.timestamp_ms % 86_400_000);
    let mut entries = vec![
        TimelineEntry { id: "start".into(), kind: EntryKind::Start },
        TimelineEntry { id: "day".into(), kind: EntryKind::DateDivider(day) },
    ];
    entries.extend(room_events());
    entries.extend(
        messages.into_iter().map(|m| TimelineEntry { id: m.event_id.clone(), kind: EntryKind::Message(Box::new(m)) }),
    );
    TimelineDiff::Reset(entries)
}

/// How a room begins: created, an invite, and a run of joins that reads as one line.
fn room_events() -> Vec<TimelineEntry> {
    let person = |id: &str, name: &str| Person { id: id.into(), name: name.into(), avatar: None };
    let ada = person(ADA, "Ada Lovelace");
    let event = |id: &str, sender: Person, change: RoomChange| TimelineEntry {
        id: id.into(),
        kind: EntryKind::RoomEvent(Box::new(RoomEvent { sender, change, timestamp_ms: DEMO_DAY_MS })),
    };
    let mut events = vec![
        event("ev-created", ada.clone(), RoomChange::Created),
        event("ev-invited", ada, RoomChange::Invited(person("@kai:example.org", "Kai"))),
    ];
    let joined = [
        ("@grace:example.org", "Grace Hopper"),
        ("@alan:example.org", "Alan Turing"),
        ("@kai:example.org", "Kai"),
        ("@mira:example.org", "Mira"),
        ("@ken:example.org", "Ken Thompson"),
        ("@barbara:example.org", "Barbara Liskov"),
    ];
    events.extend(
        joined.iter().map(|(id, name)| event(&format!("ev-joined-{id}"), person(id, name), RoomChange::Joined)),
    );
    events
}

/// #general with the changes of its setup: a day before the messages on which the room only got
/// its name, topic and picture (the room log keeps those, so that day shows nothing), and Grace
/// changing her picture among the joins (which still read as one line).
pub fn room_changes(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    let key = TimelineKey::room(DEMO_THREAD.0);
    let person = |id: &str, name: &str| Person { id: id.into(), name: name.into(), avatar: None };
    let event = |id: &str, sender: Person, change: RoomChange, timestamp_ms: u64| TimelineEntry {
        id: id.into(),
        kind: EntryKind::RoomEvent(Box::new(RoomEvent { sender, change, timestamp_ms })),
    };
    let ada = person(ADA, "Ada Lovelace");
    let setup = DEMO_DAY_MS - 86_400_000 + 9 * 3_600_000;
    let entries = [
        TimelineEntry { id: "day-setup".into(), kind: EntryKind::DateDivider(setup) },
        event("ev-named", ada.clone(), RoomChange::RoomName("general".into()), setup),
        event("ev-topic", ada.clone(), RoomChange::RoomTopic("Company-wide news".into()), setup + 60_000),
        event("ev-picture", ada, RoomChange::RoomPicture, setup + 120_000),
    ];
    // After the timeline's start.
    for (index, entry) in (1..).zip(entries) {
        seed_timeline(s, &account, &key, TimelineDiff::Insert { index, entry });
    }
    let grace = "@grace:example.org";
    let after_join = s
        .accounts
        .get(&account)
        .and_then(|data| data.timeline(&key))
        .and_then(|timeline| timeline.entries.iter().position(|entry| entry.id == format!("ev-joined-{grace}")))
        .map_or(0, |at| at + 1);
    let picture = event("ev-grace-picture", person(grace, "Grace Hopper"), RoomChange::ProfilePicture, DEMO_DAY_MS);
    seed_timeline(s, &account, &key, TimelineDiff::Insert { index: after_join, entry: picture });
    s.select_room(DEMO_THREAD.0);
}

fn room(id: &str, name: &str, topic: Option<&str>, unread: u64, mentions: u64) -> RoomSummary {
    RoomSummary {
        room_id: id.into(),
        name: name.into(),
        name_from_member: false,
        topic: topic.map(Into::into),
        kind: RoomKind::Group,
        unread,
        mentions,
        dm_users: Vec::new(),
        note_to_self: false,
        avatar: None,
        member_count: 4,
        active_member_count: 4,
        history: HistoryVisibility::Shared,
        notifications: NotificationMode::AllMessages,
        custom_notifications: false,
        // The demo's homeserver names a call service.
        call: matrix_core::RoomCall { via: Some(matrix_core::CallVia::Rtc), ..matrix_core::RoomCall::default() },
        activity_ms: None,
        encrypted: true,
        public: false,
        can_rename: true,
        pinned: Vec::new(),
        can_pin: true,
        can_start_meeting: true,
        can_share_live_location: true,
        saved: Vec::new(),
        priority: matrix_core::RoomPriority::Normal,
        marked_unread: false,
        can_change_settings: true,
        knocks: 0,
        successor: None,
    }
}

fn call_card(video: Option<bool>) -> MessageContent {
    MessageContent::Call(matrix_core::CallCard { video, ..matrix_core::CallCard::default() })
}

fn dm(id: &str, with: &str, name: &str, unread: u64) -> RoomSummary {
    RoomSummary {
        kind: RoomKind::Direct,
        name_from_member: true,
        dm_users: vec![with.into()],
        member_count: 2,
        active_member_count: 2,
        ..room(id, name, None, unread, unread)
    }
}

/// The user's note to self, as the engine reports one.
fn note_to_self(id: &str) -> RoomSummary {
    RoomSummary {
        kind: RoomKind::Direct,
        note_to_self: true,
        member_count: 1,
        active_member_count: 1,
        // Nobody to call.
        call: matrix_core::RoomCall::default(),
        ..room(id, "Note to self", None, 0, 0)
    }
}

fn space(id: &str, name: &str, children: &[&str], members: &[&str]) -> SpaceSummary {
    let strings = |xs: &[&str]| xs.iter().map(|x| (*x).to_owned()).collect();
    SpaceSummary {
        room_id: id.into(),
        name: name.into(),
        avatar: None,
        can_add_rooms: true,
        can_rename: true,
        can_change_settings: true,
        children: strings(children),
        members: strings(members),
    }
}

fn text(body: &str) -> MessageContent {
    MessageContent::Text { body: body.into(), html: None }
}

fn html(body: &str, html: &str) -> MessageContent {
    MessageContent::Text { body: body.into(), html: Some(html.into()) }
}

/// The demo's day: 12 November 2026, UTC.
pub const DEMO_DAY_MS: u64 = 20_769 * 86_400_000;

/// The voice memo Grace sends: 14 seconds of speech, with the waveform a client sends along.
pub const DEMO_VOICE_MEMO: &str = "demo-voice-memo";

fn voice_memo() -> MessageContent {
    // Words and the pauses between them: a swell every few bars, quieter at the breaths.
    let waveform = (0..100u16)
        .map(|i| {
            let x = f32::from(i);
            let words = ((x * 0.55).sin() * 0.5 + 0.5) * ((x * 0.13).sin() * 0.35 + 0.65);
            let breath = if (x * 0.09).sin() > 0.93 { 0.1 } else { 1.0 };
            (words * breath * 1000.0) as u16 + 24
        })
        .collect();
    MessageContent::Attachment(Attachment {
        kind: AttachmentKind::Audio,
        name: "voice-1765012345.wav".into(),
        size: Some(670_000),
        media: Some(matrix_core::MediaRef::opaque(DEMO_VOICE_MEMO)),
        thumbnail: None,
        dimensions: None,
        audio: Some(matrix_core::AudioDetails { voice: true, duration_ms: Some(14_000), waveform }),
        sticker: false,
    })
}

/// Messages `gap` minutes after the one before, from 09:12 UTC on the demo's day.
fn messages(lines: Vec<(&str, &str, MessageContent, u64)>) -> Vec<Message> {
    let mut at = DEMO_DAY_MS + (9 * 60 + 12) * 60_000;
    lines
        .into_iter()
        .enumerate()
        .map(|(i, (sender_id, sender_name, content, gap))| {
            at += gap * 60_000;
            Message {
                event_id: format!("$demo-{sender_id}-{i}"),
                sender_id: sender_id.into(),
                sender_name: sender_name.into(),
                content,
                timestamp_ms: at,
                ..Message::default()
            }
        })
        .collect()
}

fn work() -> Fixture {
    let general = messages(vec![
        (GRACE, "Grace Hopper", text("Morning all! The staging homeserver is back up."), 0),
        (GRACE, "Grace Hopper", text("Federation to the public servers works again too."), 1),
        (
            ALAN,
            "Alan Turing",
            html(
                "Encryption test plan:\nCross-signing\nKey backup\nRun cargo test -p matrix-core first.",
                "<p><b>Encryption test plan:</b></p><ul><li>Cross-signing</li><li>Key backup</li></ul>\
                 <p>Run <code>cargo test -p matrix-core</code> first.</p>",
            ),
            6,
        ),
        (
            ADA,
            "Ada Lovelace",
            MessageContent::Attachment(Attachment {
                kind: AttachmentKind::File,
                name: "release-notes-0.2.pdf".into(),
                size: Some(1_450_000),
                media: None,
                thumbnail: None,
                dimensions: None,
                audio: None,
                sticker: false,
            }),
            14,
        ),
        (GRACE, "Grace Hopper", text("Nice! Looks good to ship. https://giphy.com/gifs/ship-it-l0MYC0LajbaPoEADu"), 20),
        (ALAN, "Alan Turing", text("Cross-signing verification passes on Linux, macOS and Windows."), 22),
        (GRACE, "Grace Hopper", text("Great, then let's tag it."), 2),
        (GRACE, "Grace Hopper", release_poll(), 3),
        (ALAN, "Alan Turing", text(&format!("The release notes are up: {RELEASE_NOTES}")), 30),
    ]);
    let mut general = general;
    // Reactions on the test plan, one of them Ada's own.
    general[2].reactions =
        vec![Reaction { key: "👍".into(), count: 3, mine: true }, Reaction { key: "🎉".into(), count: 1, mine: false }];
    // A reply quoting the test plan.
    general[6].reply_to = Some(Quote {
        event_id: general[2].event_id.clone(),
        sender_name: Some("Alan Turing".into()),
        body: Some(general[2].content.plain().replace('\n', " ")),
        call: None,
    });
    // A thread on the cross-signing report.
    general[5].thread = Some(ThreadSummary {
        replies: 3,
        latest: Some(Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None }),
        latest_at: general[5].timestamp_ms.checked_add(21 * 60_000),
    });
    let root = general[5].clone();
    let thread = messages(vec![
        (GRACE, "Grace Hopper", text("Including recovery from the key backup?"), 80),
        (ALAN, "Alan Turing", text("Yes, restored on a fresh Windows install."), 3),
        (GRACE, "Grace Hopper", text("Perfect, thanks!"), 1),
    ])
    .into_iter()
    .enumerate()
    .map(|(i, m)| Message { event_id: format!("$thread-{i}"), ..m })
    .collect();
    Fixture {
        rooms: vec![
            (room("!general:example.org", "general", Some("Company-wide announcements and chatter"), 0, 0), general),
            (
                room("!design:example.org", "design", Some("Mockups, reviews and the component library"), 3, 0),
                messages(vec![(GRACE, "Grace Hopper", text("New sidebar spacing is ready for review."), 0)]),
            ),
            (
                RoomSummary {
                    history: HistoryVisibility::Joined,
                    ..room("!releases:example.org", "releases", None, 1, 1)
                },
                messages(vec![(ALAN, "Alan Turing", text("@ada can you sign off 0.2.0?"), 0)]),
            ),
            (
                RoomSummary {
                    notifications: NotificationMode::MentionsOnly,
                    custom_notifications: true,
                    ..room("!meetup:example.org", "meetup", Some("Monthly Rust meetup"), 4, 0)
                },
                Vec::new(),
            ),
            (room("!lab:example.org", "lab", Some("My own scratch room"), 0, 0), Vec::new()),
            (
                dm("!dm-grace:example.org", GRACE, "Grace Hopper", 2),
                messages(vec![
                    (ADA, "Ada Lovelace", call_card(Some(true)), 0),
                    (GRACE, "Grace Hopper", text("Do you have a minute to look at the sync loop?"), 1),
                    (GRACE, "Grace Hopper", text("It drops the room list after a reconnect."), 2),
                    (GRACE, "Grace Hopper", voice_memo(), 1),
                ]),
            ),
            (
                RoomSummary {
                    notifications: NotificationMode::Mute,
                    ..dm("!dm-alan:example.org", ALAN, "Alan Turing", 0)
                },
                Vec::new(),
            ),
            // Kai is calling: the card shows the call under way, with the way in.
            (
                RoomSummary {
                    call: matrix_core::RoomCall {
                        via: Some(matrix_core::CallVia::Rtc),
                        participants: vec![KAI.into()],
                    },
                    ..dm("!dm-kai:example.org", KAI, "Kai", 1)
                },
                messages(vec![(KAI, "Kai", call_card(Some(false)), 0)]),
            ),
            (
                note_to_self("!notes:example.org"),
                messages(vec![
                    (ADA, "Ada Lovelace", text("Ask Grace whether the reconnect fix made the release"), 0),
                    (ADA, "Ada Lovelace", text("Slides for the meetup: keep the engine diagram"), 0),
                ]),
            ),
        ],
        spaces: vec![
            space(
                WORK_SPACE,
                "Analytical Engines",
                &["!general:example.org", "!design:example.org", "!releases:example.org"],
                &[ADA, GRACE, ALAN],
            ),
            space("!rust:example.org", "Rust Meetup", &["!meetup:example.org"], &[ADA, KAI]),
        ],
        threads: vec![(DEMO_THREAD.0, root, thread)],
    }
}

fn hobby() -> Fixture {
    const MIRA: &str = "@mira:hobby.chat";
    Fixture {
        rooms: vec![
            (
                room("!synths:hobby.chat", "synths", Some("Modular, analog and everything with knobs"), 5, 0),
                messages(vec![(MIRA, "mira", text("Finally finished the eurorack case build."), 0)]),
            ),
            (
                RoomSummary {
                    notifications: NotificationMode::Mute,
                    custom_notifications: true,
                    ..room("!bikes:hobby.chat", "bikes", None, 7, 0)
                },
                Vec::new(),
            ),
            (dm("!dm-mira:hobby.chat", MIRA, "mira", 1), Vec::new()),
        ],
        spaces: vec![space("!synth-club:hobby.chat", "Synth Club", &["!synths:hobby.chat"], &[MIRA])],
        threads: Vec::new(),
    }
}

/// A large account for measuring: hundreds of rooms, and a room with thousands of messages.
pub fn stress(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    let mut rooms: Vec<RoomSummary> =
        (0..400).map(|n| room(&format!("!stress-{n}:example.org"), &format!("room {n}"), None, n % 7, 0)).collect();
    rooms.push(room("!big:example.org", "big", Some("Thousands of messages"), 0, 0));
    apply(
        s,
        Event::RoomsUpdated {
            account: account.clone(),
            list: RoomList { rooms, spaces: Vec::new(), invitations: Vec::new() },
        },
    );
    let start = 1_763_000_000_000u64;
    let messages: Vec<Message> = (0..2000)
        .map(|n| Message {
            event_id: format!("$big-{n}"),
            sender_id: format!("@user{}:example.org", n % 9),
            sender_name: format!("User {}", n % 9),
            content: text(&format!(
                "Message number {n}: a line of text long enough to wrap now and then in a narrow window."
            )),
            timestamp_ms: start + n * 45_000,
            ..Message::default()
        })
        .collect();
    seed_timeline(s, &account, &TimelineKey::room("!big:example.org"), timeline(messages));
    s.select_room("!big:example.org");
}

/// The link dialog for a public room the demo accounts haven't joined, its preview in.
pub fn link_preview(s: &mut State) {
    seed(s);
    s.open_permalink("https://matrix.to/#/%23rust%3Amatrix.org?via=matrix.org");
    if let Some(crate::state::Modal::Link(crate::state::LinkDialog {
        step: crate::state::LinkStep::Previewing(request),
        ..
    })) = &s.modal
    {
        let preview = matrix_core::RoomPreview {
            room_id: "!rustlang:matrix.org".into(),
            alias: Some("#rust:matrix.org".into()),
            name: Some("Rust".into()),
            topic: Some(
                "The Rust programming language: questions, news and help. Be kind; the code of conduct applies.".into(),
            ),
            avatar: None,
            member_count: 12_480,
            join_rule: Some(matrix_core::JoinRule::Public),
            space: false,
            joined: false,
            invited: false,
        };
        let event = matrix_core::Event::RoomPreview { request: *request, preview: Box::new(preview) };
        s.apply(event, &mut Tx::default());
    }
}

/// The link dialog for a person, their profile in.
pub fn link_person(s: &mut State) {
    seed(s);
    s.open_permalink("matrix:u/grace:example.org?action=chat");
    if let Some(crate::state::Modal::Link(crate::state::LinkDialog {
        step: crate::state::LinkStep::Person { request: Some(request), .. },
        ..
    })) = &s.modal
    {
        let grace = matrix_core::Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None };
        let event = matrix_core::Event::PeopleFound { request: *request, people: vec![grace] };
        s.apply(event, &mut Tx::default());
    }
}

/// Search results with the same match in a room, person and locally stored message.
pub fn search(s: &mut State) {
    seed(s);
    s.search.field = rune::TextEdit::new("gr");
    s.open_search();
    let now = std::time::Instant::now();
    s.search_people(now);
    s.search_people(now + crate::state::SEARCH_PAUSE);
    s.search.people =
        vec![matrix_core::Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None }];
    let mut hits: Vec<_> = work()
        .rooms
        .into_iter()
        .flat_map(|(room, messages)| {
            messages.into_iter().filter_map(move |message| {
                let body = message.content.plain().split_whitespace().collect::<Vec<_>>().join(" ");
                let matched = matrix_core::search::fuzzy_match(&body, "gr")?;
                Some((
                    matched.rank,
                    matrix_core::MessageHit {
                        room_id: room.room_id.clone(),
                        event_id: message.event_id,
                        thread: None,
                        sender: message.sender_name,
                        timestamp_ms: message.timestamp_ms,
                        body,
                        highlights: matched.positions,
                    },
                ))
            })
        })
        .collect();
    hits.sort_by(|(a, x), (b, y)| a.cmp(b).then(y.timestamp_ms.cmp(&x.timestamp_ms)));
    s.search.messages = hits.into_iter().take(30).map(|(_, hit)| hit).collect();
    s.search.message_status = crate::state::SearchStatus::Ready;
    s.search.request = None;
    for space in [false, true] {
        let kind = if space { matrix_core::DirectoryKind::Spaces } else { matrix_core::DirectoryKind::Rooms };
        let suffix = if space { "space" } else { "room" };
        let source = s.search.directory_mut(kind);
        source.status = crate::state::SearchStatus::Ready;
        source.rooms = vec![matrix_core::RoomPreview {
            room_id: format!("!public-{suffix}:example.org"),
            alias: Some(format!("#graphics-{suffix}:example.org")),
            name: Some(if space { "Graphics community" } else { "Graphics and design" }.into()),
            topic: Some("A place to share graphics and ideas".into()),
            avatar: None,
            member_count: if space { 1200 } else { 340 },
            join_rule: Some(matrix_core::JoinRule::Public),
            space,
            joined: false,
            invited: false,
        }];
    }
}

/// Pinned messages with both a timeline marker and a shared preview list.
pub fn pinned_messages(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    let Some(data) = s.accounts.get(&account) else { return };
    let ids = [DEMO_THREAD.1, "$demo-@alan:example.org-2"];
    let pins = data
        .timeline(&TimelineKey::room(DEMO_THREAD.0))
        .into_iter()
        .flat_map(|timeline| &timeline.entries)
        .filter_map(|entry| entry.message())
        .filter(|message| ids.contains(&message.event_id.as_str()))
        .map(|message| matrix_core::MessagePreview {
            event_id: message.event_id.clone(),
            sender_name: message.sender_name.clone(),
            sender_id: message.sender_id.clone(),
            sender_avatar: message.sender_avatar.clone(),
            timestamp_ms: Some(message.timestamp_ms),
            body: Some(message.content.plain().into_owned()),
            thread: None,
        })
        .collect();
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    if let Some(room) = rooms.iter_mut().find(|r| r.room_id == DEMO_THREAD.0) {
        room.pinned = ids.iter().map(|id| (*id).to_owned()).collect();
    }
    s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, spaces, invitations: Vec::new() });
    s.accounts.set_pins(&account, DEMO_THREAD.0.into(), pins);
    s.select_room(DEMO_THREAD.0);
}

/// Messages Ada saved in two rooms, listed from the account menu.
pub fn saved_messages(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    let Some(data) = s.accounts.get(&account) else { return };
    let mut saved = Vec::new();
    let (mut rooms, spaces) = (data.rooms.clone(), data.spaces.clone());
    for (room_id, picked) in [(DEMO_THREAD.0, 2), ("!dm-grace:example.org", 1)] {
        let messages: Vec<_> = data
            .timeline(&TimelineKey::room(room_id))
            .into_iter()
            .flat_map(|timeline| &timeline.entries)
            .filter_map(|entry| entry.message())
            .filter(|message| matches!(message.content, matrix_core::MessageContent::Text { .. }))
            .take(picked)
            .collect();
        for message in &messages {
            saved.push(matrix_core::SavedMessage {
                room_id: room_id.into(),
                saved_at_ms: Some(message.timestamp_ms),
                preview: matrix_core::MessagePreview {
                    event_id: message.event_id.clone(),
                    sender_name: message.sender_name.clone(),
                    sender_id: message.sender_id.clone(),
                    sender_avatar: message.sender_avatar.clone(),
                    timestamp_ms: Some(message.timestamp_ms),
                    body: Some(message.content.plain().into_owned()),
                    thread: None,
                },
            });
        }
        if let Some(room) = rooms.iter_mut().find(|r| r.room_id == room_id) {
            room.saved = messages.iter().map(|message| message.event_id.clone()).collect();
        }
    }
    saved.sort_by_key(|m| std::cmp::Reverse(m.saved_at_ms));
    s.accounts.set_room_list(&account, matrix_core::RoomList { rooms, spaces, invitations: Vec::new() });
    s.select_room(DEMO_THREAD.0);
    s.open_saved();
    s.accounts.set_saved_messages(&account, saved);
}

/// Read positions with overflow, missing timestamps and the existing member menu.
pub fn read_receipts(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    let account = AccountId::new(ADA);
    let key = matrix_core::TimelineKey::room(DEMO_THREAD.0);
    let message = Message {
        event_id: "$read-demo".into(),
        sender_id: ADA.into(),
        sender_name: "Ada Lovelace".into(),
        timestamp_ms: DEMO_DAY_MS + 13 * 3_600_000,
        content: text("The new build is ready. Let me know what you think!"),
        read_receipts: [
            "@kai:example.org",
            "@grace:example.org",
            "@alan:example.org",
            "@mira:example.org",
            "@lin:example.org",
            "@sam:example.org",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| matrix_core::ReadReceipt {
            user_id: id.into(),
            timestamp_ms: (index != 5).then_some(DEMO_DAY_MS + 13 * 3_600_000 - index as u64 * 60_000),
        })
        .collect(),
        ..Message::default()
    };
    apply(
        s,
        Event::TimelineChanged {
            account,
            key,
            diffs: vec![matrix_core::TimelineDiff::PushBack(matrix_core::TimelineEntry {
                id: "read-demo".into(),
                kind: matrix_core::EntryKind::Message(Box::new(message)),
            })],
        },
    );
}

/// A newer release offered in the rail, without network or installation in demo scenes.
pub fn update() -> crate::platform::updates::Release {
    crate::platform::updates::Release {
        version: "0.1.0".into(),
        upgrade: crate::platform::updates::Upgrade::Archive {
            asset_id: 42,
            digest: format!("sha256:{}", "a".repeat(64)),
        },
    }
}

/// A newer release for a Linux package install: the command that updates it.
pub fn package_update() -> crate::platform::updates::Release {
    crate::platform::updates::Release {
        version: "0.1.0".into(),
        upgrade: crate::platform::updates::Upgrade::Command(
            "mkdir -p ~/.cache/better-matrix && curl -fLo ~/.cache/better-matrix/better-matrix.deb 'https://github.com/m43i/better-matrix/releases/download/v0.1.0/better-matrix-linux-x86_64.deb' && sudo apt install -y ~/.cache/better-matrix/better-matrix.deb".into(),
        ),
    }
}

/// A newer release for a Linux install of unknown origin: its page.
pub fn page_update() -> crate::platform::updates::Release {
    crate::platform::updates::Release {
        version: "0.1.0".into(),
        upgrade: crate::platform::updates::Upgrade::Page(
            "https://github.com/m43i/better-matrix/releases/tag/v0.1.0".into(),
        ),
    }
}

/// The custom emoji the demo's messages and packs use.
pub const DEMO_EMOJI: &str = "mxc://example.org/party";

/// Messages showing what the composer can send: a mention of the user and a room, an emote, a
/// spoiler, formulas, a custom emoji, a place, and an edited reply.
pub fn message_features(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    let at = |minutes: u64| DEMO_DAY_MS + (14 * 60 + minutes) * 60_000;
    let message = |id: &str, sender: &str, name: &str, minutes: u64, content: MessageContent| Message {
        event_id: id.into(),
        sender_id: sender.into(),
        sender_name: name.into(),
        timestamp_ms: at(minutes),
        content,
        ..Message::default()
    };
    let mention = Message {
        highlighted: true,
        ..message(
            "$mention",
            GRACE,
            "Grace Hopper",
            0,
            html(
                "Ada Lovelace: can you check #design before the release?",
                "<a href=\"https://matrix.to/#/@ada:example.org\">Ada Lovelace</a>: can you check \
                 <a href=\"https://matrix.to/#/#design:example.org\">#design</a> before the release?",
            ),
        )
    };
    let messages = vec![
        mention,
        message(
            "$emote",
            ALAN,
            "Alan Turing",
            2,
            MessageContent::Emote { body: "waves from the train".into(), html: None },
        ),
        message(
            "$spoiler",
            KAI,
            "Kai",
            4,
            html("The ending: [Spoiler]", "The ending: <span data-mx-spoiler>the butler did it</span>"),
        ),
        message(
            "$math",
            GRACE,
            "Grace Hopper",
            6,
            html(
                "Euler: e^{i\\pi} + 1 = 0, and the sum: \\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}",
                "Euler: <span data-mx-maths=\"e^{i\\pi} + 1 = 0\"><code>e^{i\\pi} + 1 = 0</code></span>, and the sum: \
                 <span data-mx-maths=\"\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}\"><code>…</code></span>",
            ),
        ),
        message(
            "$emoji",
            ALAN,
            "Alan Turing",
            8,
            html(
                "Shipped :party: nice work",
                &format!("Shipped <img data-mx-emoticon src=\"{DEMO_EMOJI}\" alt=\":party:\" height=\"32\"> nice work"),
            ),
        ),
        message(
            "$place",
            KAI,
            "Kai",
            10,
            MessageContent::Location(matrix_core::Location {
                live_until: None,
                geo_uri: "geo:52.516275,13.377704".into(),
                description: Some("Brandenburger Tor, Berlin".into()),
                map: matrix_core::geo::map("geo:52.516275,13.377704"),
            }),
        ),
        message(
            "$code",
            ALAN,
            "Alan Turing",
            11,
            html(
                "The fix:\nfn retry(times: u32) -> bool {\n    // Once more, then give up.\n    times < 3\n}",
                "<p>The fix:</p><pre><code class=\"language-rust\">fn retry(times: u32) -&gt; bool {\n    \
                 // Once more, then give up.\n    times &lt; 3\n}</code></pre>",
            ),
        ),
        Message {
            edited: true,
            editable: true,
            reply_to: Some(Quote {
                event_id: "$mention".into(),
                sender_name: Some("Grace Hopper".into()),
                body: Some("Ada Lovelace: can you check #design before the release?".into()),
                call: None,
            }),
            ..message("$reply", ADA, "Ada Lovelace", 12, text("Sure, I'll look at it this afternoon."))
        },
    ];
    let entries = messages
        .into_iter()
        .map(|message| TimelineEntry { id: message.event_id.clone(), kind: EntryKind::Message(Box::new(message)) })
        .collect();
    apply(
        s,
        Event::TimelineChanged {
            account: AccountId::new(ADA),
            key: TimelineKey::room(DEMO_THREAD.0),
            diffs: vec![TimelineDiff::Reset(entries)],
        },
    );
    demo_pack_pictures(s);
}

/// The picture behind the demo's custom emoji and stickers: the app's icon.
fn demo_pack_pictures(s: &mut State) {
    s.media.attach(std::sync::Arc::new(|| {}));
    let icon = include_bytes!("../../../packaging/icons/better-matrix-256.png");
    for url in [DEMO_EMOJI, "mxc://example.org/wave", "mxc://example.org/ship"] {
        if let Some(media) = matrix_core::MediaRef::from_mxc(url) {
            s.media.loaded(media, icon.as_slice().into(), Some((256, 256)));
        }
    }
}

/// The room's image pack: a custom emoji and two stickers.
pub fn image_packs(s: &mut State) {
    let image = |shortcode: &str, url: &str, emoticon: bool, sticker: bool| matrix_core::PackImage {
        shortcode: shortcode.into(),
        body: shortcode.replace('_', " "),
        url: url.into(),
        media: matrix_core::MediaRef::from_mxc(url).unwrap_or_else(|| matrix_core::MediaRef::opaque(url)),
        emoticon,
        sticker,
        dimensions: Some((256, 256)),
        mimetype: Some("image/png".into()),
    };
    let own = matrix_core::ImagePack {
        id: "user".into(),
        name: String::new(),
        avatar: None,
        avatar_url: None,
        images: vec![image("wave", "mxc://example.org/wave", false, true)],
        place: matrix_core::PackPlace::User,
        editable: true,
        everywhere: false,
    };
    let room = matrix_core::ImagePack {
        id: format!("{}/", DEMO_THREAD.0),
        name: "Better Matrix".into(),
        avatar: None,
        avatar_url: None,
        images: vec![image("party", DEMO_EMOJI, true, false), image("ship_it", "mxc://example.org/ship", false, true)],
        place: matrix_core::PackPlace::Room { room_id: DEMO_THREAD.0.into(), state_key: String::new() },
        editable: true,
        everywhere: false,
    };
    apply(
        s,
        Event::ImagePacks {
            account: AccountId::new(ADA),
            room_id: Some(DEMO_THREAD.0.into()),
            packs: vec![own, room],
            may_add: true,
        },
    );
    demo_pack_pictures(s);
}

/// The room with `typed` in its composer, the caret at its end.
pub fn typing(s: &mut State, typed: &str) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    image_packs(s);
    s.composer.edit.insert(typed);
    s.composer.edit.focused = true;
}

/// A draft with files to send: a pasted screenshot and a picked picture show as thumbnails, a
/// PDF as a chip, all decoded before the scene draws.
pub fn upload_thumbnails(s: &mut State) {
    typing(s, "Here are the release assets");
    s.media.attach(std::sync::Arc::new(|| {}));
    let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packaging/icons/better-matrix-256.png");
    let composer = &mut s.composer;
    composer.attach_pasted_image(include_bytes!("../../../packaging/icons/better-matrix-256.png"));
    composer.attach([std::path::PathBuf::from(icon), std::path::PathBuf::from("Release notes.pdf")]);
    s.upload_thumbnails.sync(&s.media, s.composer.attachments.iter());
    s.upload_thumbnails.wait(&s.media);
}

/// A draft holding a mention, a spoiler and a formula as pills.
pub fn pills_draft(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    let edit = &mut s.composer.edit;
    edit.insert("Thanks ");
    edit.insert_pill("user:@grace:example.org", "Grace Hopper");
    edit.insert(", the twist is ");
    edit.insert_pill("spoiler", "the butler did it");
    edit.insert(" and the bound is ");
    edit.insert_pill("math", "O(n \\log n)");
    edit.focused = true;
    s.composer.formatting = true;
}

/// Seed the same MapLibre snapshots that the media worker normally supplies.
fn berlin_map(s: &mut State) {
    s.media.attach(std::sync::Arc::new(|| {}));
    for (dark, png) in [
        (false, include_bytes!("../assets/demo/location-light.png").as_slice()),
        (true, include_bytes!("../assets/demo/location-dark.png").as_slice()),
    ] {
        if let Some(source) = matrix_core::geo::styled_map("geo:52.516275,13.377704", dark, matrix_core::geo::MAP_ZOOM)
        {
            let bytes: std::sync::Arc<[u8]> = png.into();
            s.media.loaded(source, bytes.clone(), Some((480, 280)));
            if let Some(image) = s.media.decode(bytes) {
                let _ = image.wait();
            }
        }
    }
    s.media.measure();
}

pub fn location_map(s: &mut State) {
    message_features(s);
    berlin_map(s);
    s.modal = Some(crate::state::Modal::LocationMap {
        account: AccountId::new(ADA),
        key: TimelineKey::room(DEMO_THREAD.0),
        event_id: "$place".into(),
        zoom: matrix_core::geo::MAP_ZOOM,
    });
}

/// The location dialog with a place found and chosen.
pub fn location(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    s.open_location(crate::state::ComposerTarget::Room);
    s.location.search = rune::TextEdit::new("Brandenburger Tor");
    let place = |name: &str, uri: &str| matrix_core::Place {
        name: name.into(),
        geo_uri: uri.into(),
        map: matrix_core::geo::map(uri),
    };
    s.location.places = Some(vec![
        place("Brandenburger Tor, Pariser Platz, Mitte, Berlin, 10117, Deutschland", "geo:52.516275,13.377704"),
        place("Brandenburger Tor, Potsdam, Brandenburg, Deutschland", "geo:52.399929,13.047917"),
    ]);
    s.location.chosen = s.location.places.as_ref().and_then(|places| places.first().cloned());
    berlin_map(s);
}

/// The edit history of the user's edited reply.
pub fn edit_history(s: &mut State) {
    message_features(s);
    s.open_edit_history(DEMO_THREAD.0, "$reply");
    let version = |id: &str, body: &str, minutes: u64| matrix_core::EditVersion {
        event_id: id.into(),
        body: body.into(),
        html: None,
        timestamp_ms: DEMO_DAY_MS + (14 * 60 + minutes) * 60_000,
    };
    apply(
        s,
        Event::EditHistory {
            account: AccountId::new(ADA),
            room_id: DEMO_THREAD.0.into(),
            event_id: "$reply".into(),
            versions: vec![
                version("$reply", "Sure, I'll look at it", 12),
                version("$reply-2", "Sure, I'll look at it later", 13),
                version("$reply-3", "Sure, I'll look at it this afternoon.", 15),
            ],
        },
    );
}

/// A message being written, two already scheduled in the room.
pub fn scheduled(s: &mut State) {
    typing(s, "Reminder: release notes are due at noon.");
    let message = |id: &str, hours: u64, body: Option<&str>| matrix_core::ScheduledMessage {
        delay_id: id.into(),
        room_id: DEMO_THREAD.0.into(),
        send_at_ms: crate::state::now_ms() + hours * 3_600_000,
        body: body.map(Into::into),
        failed: None,
    };
    let failed = matrix_core::ScheduledMessage {
        failed: Some("You don't have permission to post here".into()),
        send_at_ms: crate::state::now_ms() - 3_600_000,
        ..message("d0", 0, Some("Office closed on Friday."))
    };
    apply(
        s,
        Event::ScheduledMessages {
            account: AccountId::new(ADA),
            messages: vec![
                message("d1", 2, Some("Stand-up moved to 10:30 today.")),
                message("d2", 26, Some("Happy release day, everyone!")),
                failed,
            ],
        },
    );
}

/// The general room with hidden events showing: room state the timeline otherwise leaves out.
pub fn hidden_events(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    let hidden = |id: &str, kind: &str, name: &str, at: u64| matrix_core::TimelineEntry {
        id: id.into(),
        kind: matrix_core::EntryKind::Hidden(Box::new(matrix_core::HiddenEvent {
            event_id: Some(format!("${id}")),
            sender: matrix_core::Person {
                id: format!("@{}:example.org", name.to_lowercase()),
                name: name.into(),
                avatar: None,
            },
            event_type: kind.into(),
            timestamp_ms: at,
        })),
    };
    let key = TimelineKey::room(DEMO_THREAD.0);
    for (id, kind, name, at) in [
        ("hidden-acl", "m.room.server_acl", "Grace", 1_700_000_000_000),
        ("hidden-widget", "im.vector.modular.widgets", "Alan", 1_700_000_100_000),
    ] {
        seed_timeline(s, &account, &key, TimelineDiff::PushBack(hidden(id, kind, name, at)));
    }
    s.set_show_hidden_events(true);
    s.select_room(DEMO_THREAD.0);
}

/// The general room's widgets: a board, a dashboard, and two meetings (Jitsi, BigBlueButton).
fn room_widgets() -> matrix_core::Widgets {
    let widget = |id: &str, name: &str, kind: &str, url: &str, meeting: Option<matrix_core::Meeting>| {
        let host = url.split('/').nth(2).unwrap_or_default().to_owned();
        matrix_core::Widget {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            url: url.into(),
            host,
            creator: Some("@grace:example.org".into()),
            can_remove: true,
            meeting,
        }
    };
    matrix_core::Widgets {
        widgets: vec![
            widget("board", "Sprint board", "m.custom", "https://board.example.org/team", None),
            widget("grafana", "Server dashboard", "m.grafana", "https://grafana.example.org/d/1", None),
            widget(
                "review",
                "Design review",
                "bigbluebutton",
                "https://bbb.example.org/b/design-review",
                Some(matrix_core::Meeting::BigBlueButton),
            ),
            widget(
                "standup",
                "Standup",
                "jitsi",
                "https://meet.example.org/TeamStandup",
                Some(matrix_core::Meeting::Jitsi),
            ),
        ],
        can_add: true,
    }
}

/// The general room's widgets panel.
pub fn widgets(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    s.show_side(crate::state::SidePanel::Widgets);
    s.accounts.set_widgets(&AccountId::new(ADA), DEMO_THREAD.0.into(), room_widgets());
}

/// The general room with a meeting just started: the header's video button with its chevron
/// (the call, joining the meeting, starting another), and the line that says Grace started it.
pub fn meeting(s: &mut State) {
    seed(s);
    let account = AccountId::new(ADA);
    let started = TimelineEntry {
        id: "ev-meeting".into(),
        kind: EntryKind::RoomEvent(Box::new(RoomEvent {
            sender: Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None },
            change: RoomChange::WidgetAdded { name: "Standup".into(), meeting: Some(matrix_core::Meeting::Jitsi) },
            timestamp_ms: DEMO_DAY_MS,
        })),
    };
    seed_timeline(s, &account, &TimelineKey::room(DEMO_THREAD.0), TimelineDiff::PushBack(started));
    let mut widgets = room_widgets();
    widgets.widgets.retain(|widget| widget.meeting != Some(matrix_core::Meeting::BigBlueButton));
    s.accounts.set_widgets(&account, DEMO_THREAD.0.into(), widgets);
    s.select_room(DEMO_THREAD.0);
}

/// The general room with a meeting pinned, the menu of ways to meet open beside its video
/// button: joining the meeting, starting another.
pub fn video_options(s: &mut State) {
    meeting(s);
    s.toggle_video_options();
}

/// The general room with a meeting pinned where the server allows no calls: the video button
/// joins the meeting.
pub fn meeting_without_calls(s: &mut State) {
    meeting(s);
    change_room(s, DEMO_THREAD.0, |room| room.call.via = None);
}

/// The sprint board, opened in its window, asks what it may do; one permission is unticked.
pub fn widget_permissions(s: &mut State) {
    widgets(s);
    let Some(request) = s.run_widget("board", false) else { return };
    let permission = |capability: &str, kind: matrix_core::PermissionKind, subject: Option<&str>| {
        matrix_core::WidgetPermission { capability: capability.into(), kind, subject: subject.map(Into::into) }
    };
    use matrix_core::PermissionKind as Kind;
    let permissions = vec![
        permission("org.matrix.msc2762.receive.event:m.room.message", Kind::ReadEvents, Some("m.room.message")),
        permission("org.matrix.msc2762.send.event:m.room.message", Kind::SendEvents, Some("m.room.message")),
        permission("org.matrix.msc2762.receive.state_event:m.room.member", Kind::ReadState, Some("m.room.member")),
        permission("org.matrix.msc4039.download_file", Kind::DownloadFiles, None),
        permission("io.element.requires_client", Kind::StayInApp, None),
        permission("org.example.board.sync", Kind::Other, None),
    ];
    let event = Event::WidgetPermissions { request, question: 1, widget: "Sprint board".into(), permissions };
    s.apply(event, &mut Tx::default());
    if let Some(key) = s.widget_prompt().map(crate::state::WidgetPrompt::key) {
        s.toggle_widget_permission(key, 5);
    }
}

/// The integration manager's terms, before it opens.
pub fn integration_terms(s: &mut State) {
    widgets(s);
    let Some(request) = s.open_integrations() else { return };
    let policy = |name: &str, url: &str| Policy { name: name.into(), url: url.into() };
    let policies = vec![
        policy("Terms of Service", "https://integrations.example.org/terms"),
        policy("Privacy Policy", "https://integrations.example.org/privacy"),
    ];
    s.apply(Event::IntegrationTerms { request, policies }, &mut Tx::default());
}

/// The source of an encrypted message: as this device reads it, and as it was sent.
pub fn event_source(s: &mut State) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    let source = matrix_core::EventSource {
        room_id: DEMO_THREAD.0.into(),
        event_id: DEMO_THREAD.1.into(),
        json: format!(
            r#"{{
  "type": "m.room.message",
  "sender": "@alan:example.org",
  "event_id": "{event}",
  "origin_server_ts": 1700000000000,
  "content": {{
    "msgtype": "m.text",
    "body": "Has anyone tried the new build?"
  }}
}}"#,
            event = DEMO_THREAD.1
        ),
        encrypted: Some(format!(
            r#"{{
  "type": "m.room.encrypted",
  "sender": "@alan:example.org",
  "event_id": "{event}",
  "origin_server_ts": 1700000000000,
  "content": {{
    "algorithm": "m.megolm.v1.aes-sha2",
    "ciphertext": "AwgAEpABqOCzJ2Tp…",
    "device_id": "ALANLAPTOP",
    "sender_key": "Hc1gJ8i3y1cCi0ZmRp3K7XcfuF0pB6wd1Sx4nT2EYfQ",
    "session_id": "3yv1Q3wN1cZ9u2lO0d5XG4WeV8m7n2Ck1lZ6aQpTb3Y"
  }}
}}"#,
            event = DEMO_THREAD.1
        )),
    };
    s.modal = Some(crate::state::Modal::Source(crate::state::SourceView::Shown(Box::new(source))));
}

/// Changes one of Ada's rooms as the next room list would bring it.
fn change_room(s: &mut State, room_id: &str, change: impl FnOnce(&mut RoomSummary)) {
    let Some(account) = s.accounts.get(&AccountId::new(ADA)) else { return };
    let mut list = RoomList {
        rooms: account.rooms.clone(),
        spaces: account.spaces.clone(),
        invitations: account.invitations.clone(),
    };
    if let Some(room) = list.rooms.iter_mut().find(|room| room.room_id == room_id) {
        change(room);
    }
    apply(s, Event::RoomsUpdated { account: AccountId::new(ADA), list });
}

/// The settings of #general as Ada, its admin, reads them: everything hers to change.
pub fn general_settings() -> matrix_core::RoomSettings {
    use matrix_core::{Permission, PermissionLevel, RoomRights};
    let level = |permission, level| PermissionLevel { permission, level, editable: true };
    matrix_core::RoomSettings {
        room_id: DEMO_THREAD.0.into(),
        space: false,
        name: Some("general".into()),
        topic: Some("Company-wide announcements and chatter".into()),
        avatar: None,
        join_rule: Some(matrix_core::JoinRule::Restricted),
        can_restrict: true,
        can_knock: true,
        history: HistoryVisibility::Shared,
        guest_access: false,
        encrypted: false,
        main_address: Some("#general:example.org".into()),
        addresses: vec!["#engines-general:example.org".into()],
        published: Some(false),
        server: "example.org".into(),
        version: "10".into(),
        upgrade_to: Some("11".into()),
        permissions: vec![
            level(Permission::DefaultLevel, 0),
            level(Permission::SendMessages, 0),
            level(Permission::Invite, 0),
            level(Permission::Kick, 50),
            level(Permission::Ban, 50),
            level(Permission::RemoveMessages, 50),
            level(Permission::MentionRoom, 50),
            level(Permission::Pins, 25),
            level(Permission::ChangeSettings, 50),
            level(Permission::Permissions, 100),
            level(Permission::Upgrade, 100),
        ],
        own_level: 100,
        rights: RoomRights {
            name: true,
            topic: true,
            avatar: true,
            join_rule: true,
            history: true,
            guest_access: true,
            addresses: true,
            encryption: true,
            permissions: true,
            upgrade: true,
            space_rooms: false,
        },
    }
}

/// #general's settings open at `section`.
pub fn room_settings(s: &mut State, section: crate::state::RoomSettingsSection) {
    seed(s);
    s.select_room(DEMO_THREAD.0);
    s.open_room_settings(DEMO_THREAD.0);
    apply(s, Event::RoomSettings { account: AccountId::new(ADA), settings: Box::new(general_settings()) });
    s.show_room_settings_section(section);
}

/// #general's member list with two people asking to join and one banned.
pub fn join_requests(s: &mut State) {
    use matrix_core::{MemberRole, Presence, RoomMember};
    seed(s);
    s.select_room(DEMO_THREAD.0);
    let outsider = |id: &str, name: &str| RoomMember {
        person: Person { id: id.into(), name: name.into(), avatar: None },
        role: MemberRole::Member,
        presence: Presence::Unknown,
        status_message: None,
        verified: false,
        level: 0,
    };
    let Some(mut members) =
        s.accounts.get(&AccountId::new(ADA)).and_then(|account| account.members.get(DEMO_THREAD.0)).cloned()
    else {
        return;
    };
    members.knocking = vec![outsider("@lin:example.org", "Lin Okafor"), outsider("@sam:other.net", "Sam")];
    members.banned = vec![outsider("@spam:junk.example", "Totally Real Prizes")];
    apply(s, Event::Members { request: None, account: AccountId::new(ADA), room_id: DEMO_THREAD.0.into(), members });
    change_room(s, DEMO_THREAD.0, |room| room.knocks = 2);
}

/// #lab, upgraded: its banner leads to the room that replaced it.
pub fn tombstone(s: &mut State) {
    seed(s);
    change_room(s, "!lab:example.org", |room| {
        room.successor = Some(matrix_core::Successor {
            room_id: "!lab-v11:example.org".into(),
            reason: Some("This room moved to room version 11.".into()),
            via: vec!["example.org".into()],
        });
    });
    s.select_room("!lab:example.org");
}

/// The picker's GIF tab with GIFs found (their previews not loaded: the demo goes nowhere).
pub fn gif_picker(s: &mut State) {
    typing(s, "");
    s.picker.giphy = crate::state::GifKey::Kept("demo".into());
    s.picker.tab = crate::state::PickerTab::Gifs;
    s.popover = Some(crate::state::Popover::Stickers(crate::state::ComposerTarget::Room));
    // What's trending counts as searched for, so the view asks GIPHY for nothing.
    s.picker.gifs.due(std::time::Instant::now());
    let sizes = [(200, 150), (200, 112), (200, 200), (200, 260), (200, 140), (200, 180)];
    s.picker.gifs.gifs = sizes
        .iter()
        .enumerate()
        .map(|(i, size)| matrix_core::FoundGif {
            id: format!("demo{i}"),
            title: format!("Demo GIF {i}"),
            url: format!("https://giphy.com/gifs/demo{i}"),
            preview: format!("https://media.giphy.com/media/demo{i}/200w.gif"),
            preview_size: Some(*size),
            sent_ping: None,
        })
        .collect();
    s.picker.gifs.next = Some(24);
    s.load_giphy_mark();
}

/// The picker's GIF tab before the user gave a GIPHY key.
pub fn gif_key(s: &mut State) {
    typing(s, "");
    s.picker.giphy = crate::state::GifKey::Missing;
    s.picker.tab = crate::state::PickerTab::Gifs;
    s.popover = Some(crate::state::Popover::Stickers(crate::state::ComposerTarget::Room));
}

/// The sticker manager on the room's pack, a sticker's details open when `open`.
pub fn pack_manager(s: &mut State, open: bool) {
    typing(s, "");
    s.open_pack_manager();
    s.select_pack(&format!("{}/", DEMO_THREAD.0));
    if open {
        s.open_pack_image("ship_it");
    }
}

/// A sticker Grace sent.
pub fn sticker_message(s: &mut State) {
    post_batch(
        s,
        vec![Attachment {
            kind: matrix_core::AttachmentKind::Image,
            name: "ship it".into(),
            size: None,
            media: matrix_core::MediaRef::from_mxc("mxc://example.org/ship"),
            thumbnail: None,
            dimensions: Some((256, 256)),
            audio: None,
            sticker: true,
        }],
    );
    demo_pack_pictures(s);
}
