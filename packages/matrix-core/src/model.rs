//! Plain data the UIs render and send. Built from SDK state, never holding SDK handles, so
//! views stay cheap to clone and trivial to test.

use std::fmt;
use std::path::PathBuf;

/// Identifies one signed-in account: its full Matrix user ID, e.g. `@alice:example.org`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct AccountId(String);

impl AccountId {
    /// Wraps a full Matrix user ID.
    pub fn new(user_id: impl Into<String>) -> Self {
        Self(user_id.into())
    }

    /// The full Matrix user ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An account as the account switcher shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct AccountSummary {
    /// Which account this is.
    pub id: AccountId,
    /// The user's display name, or their user ID when they have none.
    pub display_name: String,
    /// The homeserver the account lives on, e.g. `example.org`.
    pub homeserver: String,
    /// When rooms notify unless a room sets its own mode.
    pub default_notifications: NotificationMode,
    /// The user's profile picture.
    pub avatar: Option<MediaRef>,
    /// How the account signed in, which decides where its account details are managed.
    pub sign_in: SignInMethod,
}

/// How an account signed in. An OAuth account manages its email, password and sessions on the
/// homeserver's account page; a password account manages them in the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum SignInMethod {
    /// Through the homeserver's authorization server in the browser.
    OAuth,
    /// With a username and password.
    #[default]
    Password,
}

/// Whether a room is a group conversation or a direct chat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum RoomKind {
    /// A group room, listed under rooms.
    Group,
    /// A direct chat, listed under direct messages.
    Direct,
}

/// A joined room as the room list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomSummary {
    /// The room's ID, e.g. `!abc:example.org`.
    pub room_id: String,
    /// The computed display name.
    pub name: String,
    /// The room topic, if one is set.
    pub topic: Option<String>,
    /// Group room or direct chat.
    pub kind: RoomKind,
    /// Unread messages, as the server counts them.
    pub unread: u64,
    /// Unread mentions of the user.
    pub mentions: u64,
    /// For a direct chat, the other people's user IDs; empty for a group room and the note to
    /// self.
    pub dm_users: Vec<String>,
    /// Whether this is the user's note to self: the room the account data names for it, while
    /// nobody else is in it. Listed with the direct chats ([`RoomKind::Direct`]), with the
    /// user's own picture, and never offers a call.
    pub note_to_self: bool,
    /// The room's picture; for a direct chat without one, the other person's (the user's own for
    /// the note to self).
    pub avatar: Option<MediaRef>,
    /// How many people have joined.
    pub member_count: u64,
    /// How many have joined or are invited: who a call there would ring.
    pub active_member_count: u64,
    /// Who may read messages from before they joined.
    pub history: HistoryVisibility,
    /// When this room notifies, after the account's default applies.
    pub notifications: NotificationMode,
    /// Whether the room sets its own mode rather than following the account's default.
    pub custom_notifications: bool,
    /// How a call would start here and who is in its call now.
    pub call: crate::calls::RoomCall,
    /// When the room's latest event came, in milliseconds since the epoch, as the store keeps
    /// it across restarts (the room list's order, before any timeline opens). `None` until the
    /// SDK has worked it out.
    pub activity_ms: Option<u64>,
    /// Whether messages here are end-to-end encrypted.
    pub encrypted: bool,
    /// Whether anyone may join (the public join rule): strangers may speak, so media may wait
    /// for a press ([`Safety::shows_media`](crate::Safety::shows_media)).
    pub public: bool,
    /// Whether the user may rename the room.
    pub can_rename: bool,
    /// Event IDs pinned in this room, in server order.
    pub pinned: Vec<String>,
    /// Whether the user may change the room's pinned messages.
    pub can_pin: bool,
    /// Event IDs the user saved in this room, most recently saved first. Private to the user.
    pub saved: Vec<String>,
    /// Whether the user put the room among their favourites or their low priority rooms (room
    /// tags, which every client shares).
    pub priority: RoomPriority,
    /// Whether the user marked the room unread (MSC2867); reading it clears the mark.
    pub marked_unread: bool,
    /// Whether the user may change any of the room's settings: whether they are offered.
    pub can_change_settings: bool,
    /// How many people are asking to join, when the user may let them in.
    pub knocks: u64,
    /// The room that replaced this one, once it was upgraded.
    pub successor: Option<Successor>,
}

/// The room that replaced an upgraded one.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Successor {
    /// Its ID.
    pub room_id: String,
    /// Why the room was replaced, if the upgrade said.
    pub reason: Option<String>,
    /// Servers to join it through.
    pub via: Vec<String>,
}

/// Where the user put a room: among their favourites, their low priority rooms, or neither.
/// Kept as the room tags `m.favourite` and `m.lowpriority`, so every client groups it the same.
/// A room tagged both counts as a favourite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum RoomPriority {
    /// A favourite (`m.favourite`).
    Favourite,
    /// Neither tag.
    #[default]
    Normal,
    /// Low priority (`m.lowpriority`).
    Low,
}

/// Who can read a room's messages from before they joined, by the room's settings. The server
/// enforces it: history a member may not see is never sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum HistoryVisibility {
    /// Anyone, even without joining.
    WorldReadable,
    /// Every member, all of it.
    #[default]
    Shared,
    /// Members, from the time they were invited.
    Invited,
    /// Members, from the time they joined.
    Joined,
}

impl HistoryVisibility {
    /// Whether a member sees nothing from before they were invited or joined.
    pub fn hides_earlier(self) -> bool {
        matches!(self, Self::Invited | Self::Joined)
    }
}

/// One entry of a room's timeline, as a timeline subscription delivers it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct TimelineEntry {
    /// Stable across updates of the same entry; use it as the UI key.
    pub id: String,
    /// What the entry is.
    pub kind: EntryKind,
}

/// What a timeline entry is. Entries the UI doesn't draw yet still take their place, so diff
/// indices stay aligned.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum EntryKind {
    /// Something that happened to the room.
    RoomEvent(Box<RoomEvent>),
    /// A message.
    Message(Box<Message>),
    /// The start of a day, as milliseconds since the Unix epoch.
    DateDivider(u64),
    /// Where the user stopped reading.
    ReadMarker,
    /// The start of what this user can see of the room.
    Start,
    /// An event timelines don't draw (unknown or unsupported state, events that didn't parse,
    /// membership events that changed nothing). Shown only with "Show hidden events".
    Hidden(Box<HiddenEvent>),
}

/// An event timelines don't draw, as developer tools show it: its type and who sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct HiddenEvent {
    /// Its event ID; `None` while it is being sent.
    pub event_id: Option<String>,
    /// Who sent it.
    pub sender: Person,
    /// Its type, e.g. `m.room.server_acl`.
    pub event_type: String,
    /// When it was sent, in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
}

/// One change to a room's timeline, in the order the timeline applies them. Mirrors the
/// standard vector diff, by index.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum TimelineDiff {
    /// Replace everything.
    Reset(Vec<TimelineEntry>),
    /// Add to the end.
    Append(Vec<TimelineEntry>),
    /// Remove everything.
    Clear,
    /// Add one at the start.
    PushFront(TimelineEntry),
    /// Add one at the end.
    PushBack(TimelineEntry),
    /// Remove the first.
    PopFront,
    /// Remove the last.
    PopBack,
    /// Insert at an index.
    Insert {
        /// Where.
        index: usize,
        /// What.
        entry: TimelineEntry,
    },
    /// Replace at an index.
    Set {
        /// Where.
        index: usize,
        /// The new entry.
        entry: TimelineEntry,
    },
    /// Remove at an index.
    Remove {
        /// Where.
        index: usize,
    },
    /// Keep only the first `length`.
    Truncate {
        /// How many remain.
        length: usize,
    },
}

/// When a room notifies. Stored as push rules on the server, so every client follows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum NotificationMode {
    /// Every new message.
    #[default]
    AllMessages,
    /// Only messages that mention the user or one of their keywords.
    MentionsOnly,
    /// Nothing.
    Mute,
}

/// Something the server's push rules say the user should be told about.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Notification {
    /// The account it is for.
    pub account: AccountId,
    /// The room it happened in.
    pub room_id: String,
    /// The room's display name.
    pub room_name: String,
    /// The event that notified.
    pub event_id: String,
    /// Who sent it.
    pub sender_name: String,
    /// A plain-text preview.
    pub body: String,
}

/// Where an account's pushes for this device go: the app's push gateway, which hands them to
/// Expo's push service, as the app registers it with [`Command::RegisterPusher`](crate::Command).
/// The homeserver sends only room and event IDs (`event_id_only`): no message content leaves it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Pusher {
    /// The gateway's ID for this app, e.g. `chat.bettermatrix.mobile`.
    pub app_id: String,
    /// The device's push token (an Expo push token, `ExponentPushToken[…]`).
    pub push_key: String,
    /// The gateway's notify URL, ending in `/_matrix/push/v1/notify`.
    pub gateway_url: String,
    /// The app's name, shown in the account's list of pushers.
    pub app_display_name: String,
    /// This device's name, shown there too.
    pub device_display_name: String,
    /// The language the user reads, e.g. `en`.
    pub lang: String,
    /// More of what the gateway puts in every push, as a JSON object (the fallback `alert` an
    /// iOS push shows until the device has read the event, say). The engine adds `account`,
    /// which tells the device whose push it is.
    pub payload: Option<String>,
}

/// What a push turned out to be once its event was fetched and decrypted
/// ([`Command::FetchNotification`](crate::Command), or a push handled without the engine).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum PushContent {
    /// A message to show, as the sync's notifications show it.
    Message(Notification),
    /// Someone invited the user to a room.
    Invite {
        /// The account invited.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The room's display name.
        room_name: String,
        /// The invite.
        event_id: String,
        /// Who invited them.
        sender_name: String,
    },
    /// Something notified that can't be shown: it couldn't be decrypted, or isn't a message.
    /// The app says something generic, in its room when the room is known.
    Hidden {
        /// The account it is for.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The room's display name, when this device knows the room.
        room_name: Option<String>,
        /// The event.
        event_id: String,
    },
    /// Nothing to show: the event is gone, redacted, or the push rules say it doesn't notify.
    Nothing,
}

/// What one push names: whose it is, and the event that notified.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PushTarget {
    /// The account, from the pusher's `default_payload`.
    pub account: AccountId,
    /// The room.
    pub room_id: String,
    /// The event.
    pub event_id: String,
    /// The account's unread count the server sent along, if it did.
    pub unread: Option<u64>,
}

/// How a homeserver lets users sign in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct LoginMethods {
    /// OAuth 2.0 in the browser (Matrix's next-generation auth).
    pub oauth: bool,
    /// Username and password in the app.
    pub password: bool,
    /// New accounts can be created: on the account page (OAuth), else in the app.
    pub create_account: bool,
    /// Signing in with a QR code from a signed-in device (MSC4108): an OAuth server that grants
    /// devices sign-ins.
    pub qr: bool,
}

/// A space: a room that groups other rooms. Never listed among [`RoomList::rooms`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct SpaceSummary {
    /// The space's room ID.
    pub room_id: String,
    /// The computed display name.
    pub name: String,
    /// The space's picture.
    pub avatar: Option<MediaRef>,
    /// Whether the user may add rooms to it.
    pub can_add_rooms: bool,
    /// Whether the user may rename it.
    pub can_rename: bool,
    /// Whether the user may change any of its settings: whether they are offered.
    pub can_change_settings: bool,
    /// IDs of the rooms and subspaces it contains, in the space's order.
    pub children: Vec<String>,
    /// User IDs of its joined members, for showing direct chats with people in the space.
    pub members: Vec<String>,
}

/// Everything an account has joined, split into rooms and spaces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomList {
    /// Joined rooms and direct chats, sorted by name.
    pub rooms: Vec<RoomSummary>,
    /// Joined spaces, sorted by name.
    pub spaces: Vec<SpaceSummary>,
    /// Rooms and spaces waiting for this account to accept or decline.
    pub invitations: Vec<Invitation>,
}

/// An invitation received by an account, from the room's stripped state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Invitation {
    /// The invited room or space.
    pub room_id: String,
    /// Its display name.
    pub name: String,
    /// Its topic, when shared with invitees.
    pub topic: Option<String>,
    /// Its picture.
    pub avatar: Option<MediaRef>,
    /// Who invited this account, when known.
    pub inviter: Option<Person>,
    /// Whether accepting opens a space's Overview.
    pub space: bool,
    /// A group room or direct chat, for its navigation label.
    pub kind: RoomKind,
    /// Whether the inviter shares no joined room with the account, as far as this device knows:
    /// someone the user may not know, whose invitation the user can hide
    /// ([`Safety::hide_stranger_invites`](crate::Safety::hide_stranger_invites)).
    pub stranger: bool,
}

/// A public room discovered in a space hierarchy.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PublicRoom {
    /// Its Matrix room ID.
    pub room_id: String,
    /// Its name or alias.
    pub name: String,
    /// Its topic, when set.
    pub topic: Option<String>,
    /// Its picture.
    pub avatar: Option<MediaRef>,
    /// Joined members, used to rank suggestions.
    pub member_count: u64,
    /// Servers advertised by its parent space to help a federated join.
    pub via: Vec<String>,
}

/// One message in a room timeline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Message {
    /// The event ID, unique within the room.
    pub event_id: String,
    /// The sender's full Matrix user ID.
    pub sender_id: String,
    /// The sender's display name in this room, or their user ID.
    pub sender_name: String,
    /// The sender's profile picture in this room.
    pub sender_avatar: Option<MediaRef>,
    /// What the message carries.
    pub content: MessageContent,
    /// Edited since it was sent.
    pub edited: bool,
    /// The user's own plain text message, which they may edit (emotes, notices and files keep
    /// what they are, so they aren't offered).
    pub editable: bool,
    /// Still on its way: queued or sending (a local echo).
    pub pending: bool,
    /// Milliseconds since the Unix epoch, as the server reports `origin_server_ts`.
    pub timestamp_ms: u64,
    /// Reactions, in the order they were first used.
    pub reactions: Vec<Reaction>,
    /// Readers at this message's read position, excluding this account. Empty when no
    /// receipts are exposed. These are positions, not a cumulative list of earlier reads.
    #[cfg_attr(feature = "ffi", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub read_receipts: Vec<ReadReceipt>,
    /// The message this one answers, when it is a reply.
    pub reply_to: Option<Quote>,
    /// The thread started from this message, if it has replies.
    pub thread: Option<ThreadSummary>,
    /// It mentions the account (by name, `@room` or the account's keywords), as the
    /// account's push rules decide.
    #[cfg_attr(feature = "ffi", serde(default))]
    pub highlighted: bool,
}

/// A reader's latest position in a room timeline, as exposed by the SDK.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct ReadReceipt {
    /// The reader's full Matrix user ID.
    pub user_id: String,
    /// The receipt's time, when supplied; never substitute the message's send time.
    pub timestamp_ms: Option<u64>,
}

/// Someone as a timeline shows them: ID, name in the room, and picture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Person {
    /// The full Matrix user ID.
    pub id: String,
    /// The display name in the room, or the user ID.
    pub name: String,
    /// The profile picture.
    pub avatar: Option<MediaRef>,
}

/// Something that happened to a room rather than something someone wrote: a member came or
/// went, the room's settings changed. Timelines show it as a small line.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RoomEvent {
    /// Who did it.
    pub sender: Person,
    /// What changed.
    pub change: RoomChange,
}

/// What a [`RoomEvent`] changed. The member a membership change is about is named; the sender
/// is the one who made it (inviting, removing, banning).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum RoomChange {
    /// The room was created.
    Created,
    /// The sender joined.
    Joined,
    /// The sender left.
    Left,
    /// The sender invited someone.
    Invited(Person),
    /// The sender took back someone's invite.
    InviteRevoked(Person),
    /// The sender declined their invite.
    InviteDeclined,
    /// The sender removed someone.
    Removed(Person),
    /// The sender banned someone.
    Banned(Person),
    /// The sender lifted someone's ban.
    Unbanned(Person),
    /// The sender asked to join.
    Knocked,
    /// The sender changed their display name; `None` removed it.
    DisplayName(Option<String>),
    /// The sender changed their profile picture.
    ProfilePicture,
    /// The sender renamed the room.
    RoomName(String),
    /// The sender changed the topic.
    RoomTopic(String),
    /// The sender changed the room's picture.
    RoomPicture,
    /// The sender turned on encryption.
    Encrypted,
    /// The sender changed who may read history from before they joined.
    HistoryVisibility(HistoryVisibility),
    /// The sender changed who may join.
    JoinRules,
    /// The sender changed who may do what.
    Permissions,
    /// The sender pinned or unpinned messages.
    PinnedMessages,
    /// The sender upgraded the room: it goes on in a new one.
    Upgraded,
    /// The sender changed the room's addresses.
    Addresses,
    /// The sender changed whether guests may join.
    GuestAccess,
    /// The sender changed which servers may take part.
    ServerAccess,
    /// The sender added or removed rooms of this space.
    SpaceRooms,
    /// The sender changed which spaces the room belongs to.
    Spaces,
    /// The sender invited someone by email or phone, named as the invite shows them.
    InvitedByContact(String),
    /// The sender changed the room's moderation rules (a policy list).
    ModerationRules,
    /// The sender started sharing their live location.
    LiveLocation,
}

impl RoomChange {
    /// Whether consecutive events of this kind read as one line: several people joining.
    pub fn groups(&self) -> bool {
        matches!(self, Self::Joined | Self::Left)
    }
}

/// One reaction on a message: an emoji (or other key) and who used it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Reaction {
    /// The emoji.
    pub key: String,
    /// How many people reacted with it.
    pub count: u32,
    /// Whether the account's user is one of them.
    pub mine: bool,
}

/// The message a reply answers, as shown above the reply.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Quote {
    /// The answered message's event ID.
    pub event_id: String,
    /// Who wrote it, once known.
    pub sender_name: Option<String>,
    /// A plain-text excerpt, once known.
    pub body: Option<String>,
    /// The call it answers, when it is a call's card: the app names it.
    pub call: Option<CallCard>,
}

/// The replies in a thread, as its root message shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct ThreadSummary {
    /// How many replies it has.
    pub replies: u32,
    /// Who wrote the latest reply, once known.
    pub latest: Option<Person>,
    /// When the latest reply was sent, in milliseconds since the Unix epoch, once known.
    pub latest_at: Option<u64>,
}

/// Which timeline of a room: the room itself, or one thread in it (by its root event).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct TimelineKey {
    /// The room's ID.
    pub room_id: String,
    /// The thread's root event ID; `None` for the room's main timeline.
    pub thread: Option<String>,
}

impl TimelineKey {
    /// A room's main timeline, without thread replies.
    pub fn room(room_id: impl Into<String>) -> Self {
        Self { room_id: room_id.into(), thread: None }
    }

    /// One thread of a room.
    pub fn thread(room_id: impl Into<String>, root: impl Into<String>) -> Self {
        Self { room_id: room_id.into(), thread: Some(root.into()) }
    }
}

/// What a message carries. New message types become new variants here.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum MessageContent {
    /// Text, notices and emotes.
    Text {
        /// The plain-text body, always present.
        body: String,
        /// The sender's formatting as Matrix HTML (`org.matrix.custom.html`), if any. Untrusted:
        /// render it through a sanitising parser.
        html: Option<String>,
    },
    /// An uploaded file, image, video or audio clip.
    Attachment(Attachment),
    /// A message its sender or a moderator deleted.
    Redacted,
    /// An encrypted message whose keys this device doesn't have (yet).
    Undecryptable,
    /// A poll (MSC3381) with its votes so far.
    Poll(Poll),
    /// A call started in the room: MatrixRTC's ring, or a one-to-one invite.
    Call(CallCard),
    /// An emote (`/me waves`): the action, which the app shows after the sender's name.
    Emote {
        /// The plain-text action, always present.
        body: String,
        /// The sender's formatting as Matrix HTML, if any. Untrusted, as for text.
        html: Option<String>,
    },
    /// A place on a map (`m.location`).
    Location(Location),
}

/// A place a message shares.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Location {
    /// Where, as an RFC 5870 `geo:` URI (`geo:51.5008,0.1247;u=35`); read it with
    /// [`geo::coordinates`](crate::geo::coordinates).
    pub geo_uri: String,
    /// What the sender said about it, when it says more than the coordinates.
    pub description: Option<String>,
    /// A map of it from OpenStreetMap, to fetch like any media; `None` when the URI doesn't
    /// parse.
    pub map: Option<MediaRef>,
}

/// Who a message mentions on purpose (`m.mentions`): notified whatever its text says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Mentions {
    /// The people mentioned, by full user ID.
    #[cfg_attr(feature = "ffi", serde(default))]
    pub user_ids: Vec<String>,
    /// Everyone in the room (`@room`).
    #[cfg_attr(feature = "ffi", serde(default))]
    pub room: bool,
}

/// A custom emoji or sticker pack (MSC2545 image packs): the user's own, or a room's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct ImagePack {
    /// Stable for the pack: `user`, or the room ID and state key it was read from.
    pub id: String,
    /// What the pack calls itself, or the room's name.
    pub name: String,
    /// The pack's picture, if it has one.
    pub avatar: Option<MediaRef>,
    /// Its images, in the pack's order.
    pub images: Vec<PackImage>,
}

/// One image of an [`ImagePack`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PackImage {
    /// The short code it is typed as, without colons (`blobcat`).
    pub shortcode: String,
    /// What it shows, for a sticker's body; the short code when the pack says nothing.
    pub body: String,
    /// The image's `mxc://` URI, which messages carry.
    pub url: String,
    /// The image, to fetch like any media.
    pub media: MediaRef,
    /// May be used as a custom emoji in text.
    pub emoticon: bool,
    /// May be sent as a sticker.
    pub sticker: bool,
    /// Width and height in pixels, when the pack says.
    pub dimensions: Option<(u32, u32)>,
    /// Its media type, when the pack says.
    pub mimetype: Option<String>,
}

/// One version of an edited message, for its edit history.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct EditVersion {
    /// The version's own event ID (the original's for the first).
    pub event_id: String,
    /// What it said.
    pub body: String,
    /// Its formatting as Matrix HTML, if any. Untrusted, as for text.
    pub html: Option<String>,
    /// When it was sent, in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
}

/// A message the server holds to send later (MSC4140 delayed events).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct ScheduledMessage {
    /// The server's ID for it, to cancel it or send it now.
    pub delay_id: String,
    /// The room it goes to.
    pub room_id: String,
    /// When it goes, in milliseconds since the Unix epoch.
    pub send_at_ms: u64,
    /// What it says, as far as this device can read it: `None` for content it can't (another
    /// device's encrypted message), else the plain-text body.
    pub body: Option<String>,
    /// Why the server couldn't send it when its time came (its error message, or code); `None`
    /// while it waits. Only this device's own messages are followed this far, and one that
    /// failed stays listed until it is cancelled, which only lets it go.
    pub failed: Option<String>,
}

/// What to do with a [`ScheduledMessage`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum ScheduledAction {
    /// Drop it unsent.
    Cancel,
    /// Send it at once.
    SendNow,
}

/// A place found by name, for sharing a location.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Place {
    /// Its name, as the search service gives it (`Brandenburger Tor, Pariser Platz, Berlin`).
    pub name: String,
    /// Where it is, as a `geo:` URI.
    pub geo_uri: String,
    /// A map of it, to fetch like any media.
    pub map: Option<MediaRef>,
}

/// A call's card in the timeline. Whether the call is still under way is the room's
/// ([`RoomSummary::call`]), not the card's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct CallCard {
    /// Video or voice, when the caller said.
    pub video: Option<bool>,
    /// A one-to-one call (`m.call.invite`) rather than MatrixRTC's.
    pub direct: bool,
    /// Who declined it, by user ID.
    pub declined_by: Vec<String>,
}

/// A poll and where its votes stand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Poll {
    /// What it asks.
    pub question: String,
    /// The choices, in the sender's order.
    pub answers: Vec<PollAnswer>,
    /// Whether votes show while it is open; otherwise only once it ended.
    pub disclosed: bool,
    /// How many choices one vote may pick.
    pub max_selections: u32,
    /// How many people voted.
    pub voters: u32,
    /// Whether it ended: no more votes, results final.
    pub ended: bool,
}

/// One choice of a poll.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct PollAnswer {
    /// Stable within the poll; votes name it.
    pub id: String,
    /// What it says.
    pub text: String,
    /// How many picked it.
    pub votes: u32,
    /// Whether the user picked it.
    pub mine: bool,
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::Text { body: String::new(), html: None }
    }
}

impl MessageContent {
    /// Plain text for previews, notifications, quotes, copying and accessibility: a hidden
    /// spoiler's words never, whatever the plain body says ([`crate::html::shown_text`]).
    pub fn plain(&self) -> std::borrow::Cow<'_, str> {
        use std::borrow::Cow;
        match self {
            Self::Text { body, html } | Self::Emote { body, html } => match html.as_deref() {
                Some(html) if crate::html::holds_spoiler(html) => Cow::Owned(crate::html::shown_text(body, Some(html))),
                _ => Cow::Borrowed(body),
            },
            Self::Attachment(attachment) => Cow::Borrowed(&attachment.name),
            Self::Poll(poll) => Cow::Borrowed(&poll.question),
            Self::Location(location) => Cow::Borrowed(location.description.as_deref().unwrap_or(&location.geo_uri)),
            Self::Redacted | Self::Undecryptable | Self::Call(_) => Cow::Borrowed(""),
        }
    }
}

/// A file attached to a message.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Attachment {
    /// How the file is presented.
    pub kind: AttachmentKind,
    /// The file name.
    pub name: String,
    /// The size in bytes, when the sender said.
    pub size: Option<u64>,
    /// The file itself, for [`Command::FetchMedia`](crate::Command::FetchMedia).
    pub media: Option<MediaRef>,
    /// A thumbnail the sender uploaded, if any; otherwise ask the server for one of `media`.
    pub thumbnail: Option<MediaRef>,
    /// Width and height in pixels for images and videos, when the sender said.
    pub dimensions: Option<(u32, u32)>,
    /// What the sender said about an audio clip: whether it is a voice memo, its length and its
    /// waveform. `None` for other kinds, and for audio sent without any of it.
    pub audio: Option<AudioDetails>,
}

/// What the sender said about an audio clip (`m.audio` with MSC3245's voice and MSC1767's audio
/// blocks).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct AudioDetails {
    /// Whether it was sent as a voice memo.
    pub voice: bool,
    /// How long it plays, in milliseconds, when the sender said.
    pub duration_ms: Option<u64>,
    /// The loudness along the clip, each from 0 to [`AudioDetails::WAVEFORM_MAX`]; empty when
    /// the sender sent none.
    pub waveform: Vec<u16>,
}

impl AudioDetails {
    /// The loudest a waveform amplitude can be (MSC3246's unit).
    pub const WAVEFORM_MAX: u16 = 1024;
}

impl Attachment {
    /// The media to fetch for a preview and the size to ask for, if the attachment has one: an
    /// image's thumbnail (the sender's, else the server's), or the poster a video's sender
    /// uploaded (servers don't make them).
    pub fn preview(&self) -> Option<(MediaRef, MediaSize)> {
        match (self.kind, &self.thumbnail, &self.media) {
            (AttachmentKind::Image | AttachmentKind::Video, Some(thumbnail), _) => {
                Some((thumbnail.clone(), MediaSize::Original))
            }
            (AttachmentKind::Image, None, Some(media)) => Some((media.clone(), MediaSize::PREVIEW)),
            _ => None,
        }
    }
}

/// An opaque handle to a piece of Matrix media (plain or encrypted), as the engine needs it to
/// download. Compare and hash it to key caches; never parse it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct MediaRef(pub(crate) String);

impl MediaRef {
    /// A handle that names nothing on any server: for demo scenes and tests.
    pub fn opaque(id: &str) -> Self {
        Self(id.to_owned())
    }
}

/// Which rendition of media to fetch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum MediaSize {
    /// The file as uploaded.
    Original,
    /// A server-made thumbnail scaled to fit the box.
    Thumbnail {
        /// Box width in pixels.
        width: u32,
        /// Box height in pixels.
        height: u32,
    },
}

impl MediaSize {
    /// The thumbnail size timelines ask for.
    pub const PREVIEW: Self = Self::Thumbnail { width: 800, height: 600 };
}

/// How an attachment is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub enum AttachmentKind {
    /// A picture.
    Image,
    /// A video clip.
    Video,
    /// An audio clip.
    Audio,
    /// Any other file.
    File,
}

/// A message to send. New kinds of outgoing content become new variants here.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Outgoing {
    /// Text, optionally formatted.
    Text {
        /// The plain-text body every client can show.
        body: String,
        /// The same text as Matrix HTML, when it carries formatting.
        html: Option<String>,
        /// Who it mentions on purpose; sent as `m.mentions` (empty: nobody).
        #[cfg_attr(feature = "ffi", serde(default))]
        mentions: Mentions,
    },
    /// An emote (`/me waves`), optionally formatted.
    Emote {
        /// The action, without the sender's name.
        body: String,
        /// The same as Matrix HTML, when it carries formatting.
        html: Option<String>,
        /// Who it mentions on purpose.
        #[cfg_attr(feature = "ffi", serde(default))]
        mentions: Mentions,
    },
    /// A sticker from an image pack.
    Sticker(PackImage),
    /// A place on a map.
    Location {
        /// Where, as a `geo:` URI ([`geo::uri`](crate::geo::uri) makes one).
        geo_uri: String,
        /// What to call it (a place's name), if anything.
        description: Option<String>,
    },
    /// A file, uploaded and sent as an image, video, audio clip or file by its type.
    File(Upload),
    /// A recorded voice memo, sent as a voice message.
    Voice {
        /// The recording.
        upload: Upload,
        /// How long it plays, in milliseconds.
        duration_ms: u64,
        /// The loudness along the memo, each from 0 to [`AudioDetails::WAVEFORM_MAX`], for the
        /// bars clients draw; empty to send none.
        waveform: Vec<u16>,
    },
    /// A poll.
    Poll {
        /// What it asks.
        question: String,
        /// The choices, two or more.
        answers: Vec<String>,
        /// Whether votes show while it is open.
        disclosed: bool,
    },
}

/// A file to send: one on disk, or bytes already in memory (a pasted screenshot).
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Upload {
    /// A file on disk, read when it is sent.
    Path(PathBuf),
    /// Bytes in memory, with the file name to send them under.
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    Bytes {
        /// The file name; its extension decides the type.
        name: String,
        /// The file's contents.
        bytes: std::sync::Arc<[u8]>,
    },
}

impl Upload {
    /// The file name it is sent under.
    pub fn name(&self) -> String {
        match self {
            Self::Path(path) => {
                path.file_name().map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned())
            }
            Self::Bytes { name, .. } => name.clone(),
        }
    }

    /// The media type, from the file name.
    pub fn mime(&self) -> mime_guess::Mime {
        mime_guess::from_path(self.name()).first_or_octet_stream()
    }
}

impl std::fmt::Debug for Upload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(path) => f.debug_tuple("Path").field(path).finish(),
            Self::Bytes { name, bytes } => {
                f.debug_struct("Bytes").field("name", name).field("len", &bytes.len()).finish()
            }
        }
    }
}

/// Letters for an avatar without an image. From a display name: the first letter of each of its
/// first two words, so one letter for a single word. From a Matrix ID (`@ada:example.org`, used
/// when there is no display name): the first letter of its local part, never the server's.
/// `?` when there is nothing to use.
///
/// ```
/// assert_eq!(matrix_core::model::initials("Ada Lovelace"), "AL");
/// assert_eq!(matrix_core::model::initials("@ada:example.org"), "A");
/// ```
pub fn initials(name: &str) -> String {
    let name = match name.strip_prefix(['@', '#', '!']) {
        Some(id) => id.split(':').next().unwrap_or(id),
        None => name,
    };
    let letters: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() { "?".into() } else { letters }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_should_use_one_letter_for_a_single_word() {
        assert_eq!(initials("ada"), "A");
    }

    #[test]
    fn initials_should_take_the_first_letters_of_the_first_two_words() {
        assert_eq!(initials("Grace Brewster Hopper"), "GB");
    }

    #[test]
    fn initials_should_use_the_local_part_of_a_user_id() {
        assert_eq!(initials("@ada:example.org"), "A");
    }

    #[test]
    fn initials_should_ignore_the_server_of_a_user_id() {
        assert_eq!(initials("@1:Example Org"), "1");
    }

    #[test]
    fn initials_should_skip_symbols_at_the_start_of_a_word() {
        assert_eq!(initials("lab (old)"), "LO");
    }

    #[test]
    fn initials_should_fall_back_to_a_question_mark_when_blank() {
        assert_eq!(initials("  "), "?");
    }
}

/// A password or key on its way to the engine. `Debug` never shows it.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct Password(pub String);

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(…)")
    }
}
