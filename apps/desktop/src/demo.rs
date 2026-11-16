//! Sample accounts for headless renders (`--scene demo`) and UI tests. Fed through
//! [`State::apply`] like real engine events, so the demo exercises the same path.

use std::path::PathBuf;

use matrix_core::{
    AccountId, AccountSummary, Attachment, AttachmentKind, ContactDetail, ContactKind, Contacts, EntryKind, Event,
    HistoryVisibility, Homeserver, LoginMethods, Message, MessageContent, NotificationMode, Person, Quote, Reaction,
    RecoveryStatus, RoomChange, RoomEvent, RoomKind, RoomList, RoomSummary, SasEmoji, Security, Session, SignInMethod,
    SpaceSummary, ThreadSummary, TimelineDiff, TimelineEntry, TimelineKey,
};
use rune::Tx;

use crate::state::{Format, SignInStep, State};

pub const WORK_SPACE: &str = "!engines:example.org";

const GRACE: &str = "@grace:example.org";
const ALAN: &str = "@alan:example.org";
pub const ADA: &str = "@ada:example.org";

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
        }],
    };
    apply(s, Event::RoomsUpdated { account, list });
    s.show_invitation("!design:example.org");
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
    };
    apply(s, Event::Members { request: None, account: AccountId::new(ADA), room_id: DEMO_THREAD.0.into(), members });
    let verified = Security { session_verified: true, recovery: RecoveryStatus::Enabled };
    apply(s, Event::SecurityChanged { account: AccountId::new(ADA), security: verified });
    let fresh = Security { session_verified: false, recovery: RecoveryStatus::Enabled };
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
        methods: LoginMethods { oauth, password },
    }
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
        body: "Cross-signing verification passes on Linux, macOS and Windows.".into(),
        replies: 3,
        latest_at: Some(DEMO_DAY_MS + 11 * 3_600_000 + 36 * 60_000),
    };
    apply(s, Event::Threads { account: AccountId::new(ADA), room_id: room_id.clone(), threads: vec![thread] });
    let file = |name: &str, kind: AttachmentKind, who: &str, minutes: u64| matrix_core::SharedFile {
        event_id: format!("$file-{name}"),
        sender_name: who.into(),
        timestamp: DEMO_DAY_MS + 10 * 3_600_000 + minutes * 60_000,
        attachment: Attachment {
            kind,
            name: name.into(),
            size: Some(1_450_000),
            media: None,
            thumbnail: None,
            dimensions: None,
        },
    };
    let files = vec![
        file("release-notes-0.2.pdf", AttachmentKind::File, "Ada Lovelace", 33),
        file("screenshot-sidebar.png", AttachmentKind::Image, "Grace Hopper", 12),
        file("standup.ogg", AttachmentKind::Audio, "Kai", 2),
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
        kind: EntryKind::RoomEvent(Box::new(RoomEvent { sender, change })),
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

fn room(id: &str, name: &str, topic: Option<&str>, unread: u64, mentions: u64) -> RoomSummary {
    RoomSummary {
        room_id: id.into(),
        name: name.into(),
        topic: topic.map(Into::into),
        kind: RoomKind::Group,
        unread,
        mentions,
        dm_users: Vec::new(),
        avatar: None,
        member_count: 4,
        active_member_count: 4,
        history: HistoryVisibility::Shared,
        notifications: NotificationMode::AllMessages,
        custom_notifications: false,
        // The demo's homeserver names a call service.
        call: matrix_core::RoomCall { via: Some(matrix_core::CallVia::Rtc), ..matrix_core::RoomCall::default() },
        activity_ms: None,
    }
}

fn call_card(video: Option<bool>) -> MessageContent {
    MessageContent::Call(matrix_core::CallCard { video, ..matrix_core::CallCard::default() })
}

fn dm(id: &str, with: &str, name: &str, unread: u64) -> RoomSummary {
    RoomSummary {
        kind: RoomKind::Direct,
        dm_users: vec![with.into()],
        member_count: 2,
        active_member_count: 2,
        ..room(id, name, None, unread, unread)
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
