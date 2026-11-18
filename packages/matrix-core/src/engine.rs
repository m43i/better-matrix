//! The engine: a background Tokio runtime that owns every signed-in [`Account`], runs their
//! sync loops and carries out [`Command`]s, reporting through an [`EventSink`].
//!
//! It is an actor. One loop owns the state; each command runs as its own task, so a slow login
//! never holds up a send on another account. Tasks report what they find with events directly;
//! anything that changes the engine's own state (an account signing in or out) comes back to
//! the loop as an [`Outcome`]. Every command ends with exactly one [`Event::Finished`] carrying
//! its [`RequestId`], and any command can be cancelled with [`Command::Cancel`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use matrix_sdk::config::SyncSettings;
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::{LoopCtrl, SessionChange};
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinSet};

use crate::account::{Account, ClientIdentity, LoginRequest};
use crate::discovery::{self, Homeserver};
use crate::model::{
    AccountId, AccountSummary, MediaRef, MediaSize, Message, Notification, NotificationMode, Outgoing, Password,
    RoomList, TimelineDiff, TimelineKey,
};
use crate::secrets::SecretStore;
use crate::session::Sessions;
use crate::store::StoreLayout;
use crate::{Error, convert};

/// Identifies one command, so its [`Event::Finished`] can be matched to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
pub struct RequestId(pub u64);

/// What an app asks the engine to do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Command {
    /// Resolve a server name or URL (through `.well-known`) and find out how it lets users sign
    /// in; answered by [`Event::HomeserverDiscovered`].
    Discover {
        /// A server name or homeserver URL, e.g. from [`SignInInput`](crate::SignInInput).
        server: String,
    },
    /// Sign in another account with a password.
    Login(LoginRequest),
    /// Sign in another account in the browser (OAuth 2.0). [`Event::OpenUrl`] asks the app to
    /// open the sign-in page; the command finishes when the browser comes back.
    LoginOAuth {
        /// The resolved homeserver URL, from [`Homeserver::url`].
        homeserver: String,
        /// A suspended account to sign in as again, taking back its store and device.
        resume: Option<AccountId>,
    },
    /// The browser came back to the app's own redirect URI ([`OAuthRedirect::App`]) for the
    /// sign-in `request` started: finishes it. Unknown or finished sign-ins answer cancelled.
    ///
    /// [`OAuthRedirect::App`]: crate::account::OAuthRedirect::App
    FinishOAuth {
        /// The [`Command::LoginOAuth`] it answers.
        request: RequestId,
        /// The full URL the app received, query included.
        url: String,
    },
    /// Sign an account out on the server and forget it locally, its data included. A
    /// suspended account is only forgotten.
    Logout(AccountId),
    /// Stop an account the server signed out softly ([`Event::SessionEnded`] with `kept`),
    /// keeping its data here; signing in as it again with `resume` carries on from it.
    /// [`Event::AccountRemoved`] follows.
    Suspend(AccountId),
    /// Send a message into a timeline: the room, or a thread (the reply joins the thread).
    Send {
        /// The sending account.
        account: AccountId,
        /// Where it goes.
        key: TimelineKey,
        /// The event ID of the message it answers, for a reply.
        reply_to: Option<String>,
        /// What to send.
        message: Outgoing,
    },
    /// Add the account's reaction to a message, or take it back when it is already there.
    React {
        /// The reacting account.
        account: AccountId,
        /// The timeline showing the message; it must be open.
        key: TimelineKey,
        /// The message's event ID.
        event_id: String,
        /// The emoji.
        emoji: String,
    },
    /// Change the text of a message the account sent.
    Edit {
        /// The account that sent it.
        account: AccountId,
        /// The timeline showing the message; it must be open.
        key: TimelineKey,
        /// The message's event ID.
        event_id: String,
        /// The new text.
        body: String,
        /// Its formatting as Matrix HTML, if any.
        html: Option<String>,
    },
    /// Delete a message the account sent, for everyone.
    Redact {
        /// The account that sent it.
        account: AccountId,
        /// The timeline showing the message; it must be open.
        key: TimelineKey,
        /// The message's event ID.
        event_id: String,
    },
    /// Forward a text or attachment message from an open timeline to another joined room.
    /// Preserves its latest content and formatting, but removes reply, thread and edit relations.
    Forward {
        /// The account forwarding the message (source and destination use this account).
        account: AccountId,
        /// The open source timeline.
        key: TimelineKey,
        /// The source message's server event ID.
        event_id: String,
        /// The destination room.
        room_id: String,
    },
    /// Pin or unpin a message for everyone in a room; requires the room's pin permission.
    /// Updated room summaries follow through sync.
    SetPinned {
        /// The account changing the pins.
        account: AccountId,
        /// The room containing the message.
        room_id: String,
        /// The message's server event ID.
        event_id: String,
        /// Pin when true, unpin when false.
        pinned: bool,
    },
    /// Fetch all pinned message previews, including messages outside loaded history.
    /// Answered by [`Event::PinnedMessages`]; inaccessible events retain a placeholder.
    ListPinnedMessages {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
    },
    /// Vote in a poll: `answers` replace the account's earlier vote; none takes it back.
    VotePoll {
        /// The voting account.
        account: AccountId,
        /// The timeline showing the poll; it must be open.
        key: TimelineKey,
        /// The poll's event ID.
        poll_id: String,
        /// The picked answers' IDs.
        answers: Vec<String>,
    },
    /// End a poll the account started: no more votes, results final.
    EndPoll {
        /// The account that started it.
        account: AccountId,
        /// The timeline showing the poll; it must be open.
        key: TimelineKey,
        /// The poll's event ID.
        poll_id: String,
    },
    /// Download the GIF behind a link (see [`links`](crate::links)); answered by
    /// [`Event::GifLoaded`].
    FetchGif {
        /// The link as written in the message.
        url: String,
    },
    /// A link's preview card, from the account's homeserver (see [`links`](crate::links));
    /// answered by [`Event::LinkPreview`].
    PreviewLink {
        /// The account whose homeserver fetches it.
        account: AccountId,
        /// The link as written in the message.
        url: String,
    },
    /// Download media; answered by [`Event::MediaLoaded`].
    FetchMedia {
        /// The account that can see the media.
        account: AccountId,
        /// The media.
        media: MediaRef,
        /// Which rendition.
        size: MediaSize,
    },
    /// Download media as uploaded and write it to `path` (a file the user chose to save).
    SaveMedia {
        /// The account that can see the media.
        account: AccountId,
        /// The media.
        media: MediaRef,
        /// Where to write it.
        path: PathBuf,
    },
    /// Set when a room notifies; an updated [`Event::RoomsUpdated`] follows.
    SetNotificationMode {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// The room's own mode, or `None` to follow the account's default.
        mode: Option<NotificationMode>,
    },
    /// Set when the account's rooms notify unless they set their own mode; an
    /// [`Event::AccountChanged`] and an [`Event::RoomsUpdated`] follow.
    SetDefaultNotificationMode {
        /// Which account.
        account: AccountId,
        /// The new default.
        mode: NotificationMode,
    },
    /// Set what others see of the user (online, away or hidden) on every session of this app;
    /// an [`Event::Status`] follows.
    SetStatus {
        /// Which account.
        account: AccountId,
        /// The new status.
        status: crate::Status,
    },
    /// Set the status message others see beside the user's presence, or clear it with `None`;
    /// an [`Event::Presence`] with the user's follows.
    SetStatusMessage {
        /// Which account.
        account: AccountId,
        /// What the user is up to ("🌴 On vacation"); blank clears it too.
        message: Option<String>,
    },
    /// Leave a room; an updated [`Event::RoomsUpdated`] follows without it.
    LeaveRoom {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// Also forget it on the server (deleting a direct chat).
        forget: bool,
    },
    /// Follow a timeline (a room's, or a thread's): [`Event::TimelineChanged`] starts with a
    /// reset and follows every change; at least [`HISTORY_PAGE`](crate::HISTORY_PAGE) messages are
    /// loaded when it has them.
    OpenTimeline {
        /// Whose room.
        account: AccountId,
        /// Which timeline.
        key: TimelineKey,
    },
    /// Record a room being shown to the user. Background timeline subscriptions do not count.
    VisitRoom {
        /// The account viewing it.
        account: AccountId,
        /// The room shown (opening a thread does not count again).
        room_id: String,
    },
    /// Stop following a timeline.
    CloseTimeline {
        /// Whose room.
        account: AccountId,
        /// Which timeline.
        key: TimelineKey,
    },
    /// Mark an open timeline read up to a displayed event with a read receipt, which the server
    /// counts unread messages from in every session. Does nothing when it is closed or already
    /// read.
    MarkRead {
        /// Whose room.
        account: AccountId,
        /// Which timeline.
        key: TimelineKey,
        /// The displayed event; `None` uses the newest event before the first viewport report.
        event_id: Option<String>,
        /// Also advance the cross-device fully-read marker after the unread boundary has
        /// been seen. Thread reads never advance the room-wide marker.
        fully_read: bool,
    },
    /// Tell a room whether the user is typing in it. Repeating `true` while typing is cheap: the
    /// SDK sends again only once the last notice is about to expire.
    Typing {
        /// Who is typing.
        account: AccountId,
        /// Where.
        room_id: String,
        /// Typing, or stopped.
        typing: bool,
    },
    /// Load the page of messages before the oldest shown, from the event cache or the server,
    /// filling gaps from time offline; [`Event::HistoryLoaded`] follows.
    LoadOlder {
        /// Whose room.
        account: AccountId,
        /// Which timeline; it must be open.
        key: TimelineKey,
    },
    /// List the account's signed-in sessions; answered by [`Event::Sessions`].
    ListSessions(AccountId),
    /// Rename a session; an updated [`Event::Sessions`] follows.
    RenameSession {
        /// Whose session.
        account: AccountId,
        /// The device ID.
        device_id: String,
        /// The new name.
        name: String,
    },
    /// Sign another session out. A password account may answer with [`Event::PasswordNeeded`]
    /// (send again with the password); an OAuth account answers with [`Event::OpenUrl`] for the
    /// server's page that ends it.
    EndSession {
        /// Whose session.
        account: AccountId,
        /// The device ID.
        device_id: String,
        /// The account password, when the server asked for it.
        password: Option<Password>,
    },
    /// Verify this session by restoring secrets with the recovery key.
    Recover {
        /// Which account.
        account: AccountId,
        /// The recovery key.
        key: Password,
    },
    /// Set up recovery; the new key arrives once in [`Event::RecoveryKey`].
    SetUpRecovery(AccountId),
    /// Replace the recovery key; the new key arrives once in [`Event::RecoveryKey`].
    ResetRecoveryKey(AccountId),
    /// Change the display name; an empty one removes it. [`Event::AccountChanged`] follows.
    SetDisplayName {
        /// Whose.
        account: AccountId,
        /// The new name.
        name: String,
    },
    /// Upload a picture as the profile picture, or with `None` remove it.
    /// [`Event::AccountChanged`] follows.
    SetAvatar {
        /// Whose.
        account: AccountId,
        /// The picture.
        file: Option<PathBuf>,
    },
    /// List the email addresses and phone numbers linked to the account; answered by
    /// [`Event::ContactDetails`].
    ListContactDetails(AccountId),
    /// Ask the server to confirm a new email address (it sends a link) or phone number (it
    /// texts a code); answered by [`Event::ContactCodeSent`].
    RequestContactCode {
        /// Whose.
        account: AccountId,
        /// Email or phone.
        kind: crate::ContactKind,
        /// The address or number.
        address: String,
        /// The ISO 3166 country a number without `+` is dialled from.
        country: String,
    },
    /// Send the code texted to a new phone number.
    ConfirmContactCode {
        /// Whose.
        account: AccountId,
        /// The number being added.
        pending: crate::PendingContact,
        /// The code.
        code: String,
    },
    /// Link a confirmed contact detail. The server usually asks for the password:
    /// [`Event::PasswordNeeded`] (send again with it). [`Event::ContactDetails`] follows.
    AddContact {
        /// Whose.
        account: AccountId,
        /// The confirmed detail.
        pending: crate::PendingContact,
        /// The account password, when the server asked for it.
        password: Option<Password>,
    },
    /// Unlink a contact detail; [`Event::ContactDetails`] follows.
    RemoveContact {
        /// Whose.
        account: AccountId,
        /// The detail.
        contact: crate::ContactDetail,
    },
    /// Open a page of the homeserver's account management (OAuth accounts); answered by
    /// [`Event::OpenUrl`].
    OpenAccountPage {
        /// Whose.
        account: AccountId,
        /// Which page.
        page: crate::AccountPage,
    },
    /// List a room's joined members and whether the user may invite; answered by
    /// [`Event::Members`].
    ListMembers {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// Everyone, fetching the full list from the server when this device lacks it. Without
        /// it, only the members this device knows (enough for a few faces), with no request.
        all: bool,
    },
    /// List a room's threads; answered by [`Event::Threads`].
    ListThreads {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
    },
    /// List the files shared in a room; answered by [`Event::Files`].
    ListFiles {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
    },
    /// Kick, ban or change the role of a room member; [`Event::Members`] follows.
    ModerateMember {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The member.
        user_id: String,
        /// What to do.
        action: crate::people::Moderation,
    },
    /// Find people in the homeserver's user directory; answered by [`Event::PeopleFound`].
    SearchPeople {
        /// Whose homeserver.
        account: AccountId,
        /// A name or user ID, or part of one.
        term: String,
    },
    /// Search the homeserver's public directory; answered by [`Event::DirectoryFound`].
    SearchDirectory {
        /// Whose homeserver to search.
        account: AccountId,
        /// Part of a name, alias or topic. Empty text returns no results.
        term: String,
        /// Rooms or spaces, with separate result limits.
        kind: crate::DirectoryKind,
    },
    /// Search this account's locally stored messages without a server request; answered by
    /// [`Event::MessagesFound`]. Empty text returns no hits; at most 30 are returned.
    SearchMessages {
        /// The account whose history to search.
        account: AccountId,
        /// Fuzzy search terms.
        term: String,
    },
    /// Suggest people to start a chat with; answered by [`Event::PeopleFound`].
    SuggestPeople(AccountId),
    /// List every public room under a space, ranked by member count.
    ListPublicRooms {
        /// Whose homeserver.
        account: AccountId,
        /// The space whose hierarchy to browse.
        space: String,
    },
    /// Join a room or space by its ID or an alias: a public room, an invitation to accept, a
    /// [`Permalink`](crate::Permalink). Answered by [`Event::RoomJoined`] once the account's
    /// room list has it.
    JoinRoom {
        /// The joining account.
        account: AccountId,
        /// The room ID (`!…`) or alias (`#…`).
        room_id: String,
        /// Servers to join through (advertised by a space, or from a link); ones that aren't
        /// server names are left out.
        via: Vec<String>,
    },
    /// Invite someone to a room; an updated [`Event::Members`] follows.
    Invite {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// Who.
        user_id: String,
    },
    /// Open the direct chat with someone, starting one when there is none; answered by
    /// [`Event::DirectChat`].
    OpenDirectChat {
        /// Whose.
        account: AccountId,
        /// Who with.
        user_id: String,
    },
    /// Look at a room the account may not have joined, by its ID or an alias (from a
    /// [`Permalink`](crate::Permalink)); answered by [`Event::RoomPreview`]. Fails when no
    /// server will describe the room.
    PreviewRoom {
        /// Whose.
        account: AccountId,
        /// The room ID (`!…`) or alias (`#…`).
        room: String,
        /// Servers to ask, from the link; ones that aren't server names are left out.
        via: Vec<String>,
    },
    /// Create a private, encrypted room, in a space when given; answered by
    /// [`Event::RoomCreated`].
    CreateRoom {
        /// Whose.
        account: AccountId,
        /// Its name.
        name: String,
        /// Its topic, if any.
        topic: Option<String>,
        /// The space it belongs in, if any.
        space: Option<String>,
    },
    /// Create a space; answered by [`Event::RoomCreated`].
    CreateSpace {
        /// Whose.
        account: AccountId,
        /// Its name.
        name: String,
    },
    /// Rename a room or space.
    RenameRoom {
        /// Whose.
        account: AccountId,
        /// The room or space.
        room_id: String,
        /// The new name.
        name: String,
    },
    /// Save how the user arranged the sidebar; it syncs to their other sessions.
    SetSidebarLayout {
        /// Whose.
        account: AccountId,
        /// The arrangement.
        layout: crate::SidebarLayout,
    },
    /// Replace the account's cryptographic identity when the recovery key and every verified
    /// session are lost. An OAuth account answers with [`Event::OpenUrl`] for the page that
    /// approves it, then waits for the approval; a password account may answer with
    /// [`Event::PasswordNeeded`] (send again with the password). Recovery is off afterwards.
    ResetIdentity {
        /// Which account.
        account: AccountId,
        /// The account password, when the server asked for it.
        password: Option<Password>,
    },
    /// Fetch the keys of the rooms on screen from the key backup, so messages that couldn't be
    /// decrypted can be: after this session was verified. Their timelines update by themselves.
    FetchRoomKeys(AccountId),
    /// Ask the user's other sessions to verify this one; [`Event::Verification`] follows.
    VerifyThisSession(AccountId),
    /// Accept a verification request from another of the user's sessions.
    AcceptVerification {
        /// Which account.
        account: AccountId,
        /// The verification.
        flow_id: String,
    },
    /// Say whether the emoji match the other session's.
    ConfirmVerification {
        /// Which account.
        account: AccountId,
        /// The verification.
        flow_id: String,
        /// They match.
        matches: bool,
    },
    /// Stop a verification.
    CancelVerification {
        /// Which account.
        account: AccountId,
        /// The verification.
        flow_id: String,
    },
    /// Stop an earlier command. It finishes with an error saying it was cancelled.
    Cancel(RequestId),
    /// Start a call in a room (ending any other): MatrixRTC where the room allows it, else
    /// one-to-one in a direct chat. Reported by [`Event::CallUpdated`] until
    /// [`Event::CallEnded`].
    StartCall {
        /// Whose.
        account: AccountId,
        /// Where.
        room_id: String,
        /// With the camera on.
        video: bool,
    },
    /// Answer the call ringing in a room ([`Event::IncomingCall`]).
    AnswerCall {
        /// Whose.
        account: AccountId,
        /// Where it rings.
        room_id: String,
        /// With the camera on.
        video: bool,
    },
    /// Decline the call ringing in a room.
    DeclineCall {
        /// Whose.
        account: AccountId,
        /// Where it rings.
        room_id: String,
    },
    /// Leave the call under way.
    HangUp,
    /// Turn the user's microphone, camera or screen sharing on or off in the call under way.
    SetCallMedia(crate::calls::MediaToggle),
    /// Send a video of the call to `sink`, from now on.
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    WatchVideo {
        /// Which video.
        video: crate::calls::VideoId,
        /// Where its frames go.
        sink: crate::calls::VideoSink,
    },
    /// Stop sending a video anywhere.
    UnwatchVideo(crate::calls::VideoId),
    /// List the microphones, speakers and cameras; answered by [`Event::CallDevices`].
    ListCallDevices,
    /// Use a microphone, speaker or camera for calls (`None`: the system's default), from the
    /// call under way on.
    ChooseCallDevice {
        /// Which kind.
        kind: crate::calls::DeviceKind,
        /// The device's name from [`Event::CallDevices`].
        id: Option<String>,
    },
    /// Send the account's pushes to this device through a push gateway (or update its
    /// pusher there): the homeserver sends only room and event IDs.
    RegisterPusher {
        /// Whose pushes.
        account: AccountId,
        /// The gateway, the device's token and how the pusher introduces itself.
        pusher: crate::model::Pusher,
    },
    /// Stop sending the account's pushes to this device.
    RemovePusher {
        /// Whose pushes.
        account: AccountId,
        /// The pusher's app ID, as registered.
        app_id: String,
        /// The device's token, as registered.
        push_key: String,
    },
    /// Fetch and decrypt what a push is about; answered by [`Event::PushContent`].
    FetchNotification {
        /// Whose push.
        account: AccountId,
        /// The room it names.
        room_id: String,
        /// The event it names.
        event_id: String,
    },
}

/// What the engine reports. Delivered on the engine's threads; a sink forwards them to its UI.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Event {
    /// A command ended. `Err` holds a message fit to show the user.
    Finished {
        /// The command this answers.
        request: RequestId,
        /// How it ended.
        result: Result<(), String>,
    },
    /// A homeserver was found.
    HomeserverDiscovered {
        /// The [`Command::Discover`] this answers.
        request: RequestId,
        /// Where it is and how it signs users in.
        homeserver: Homeserver,
    },
    /// The app should open this URL in the user's browser.
    OpenUrl {
        /// The command that needs it.
        request: RequestId,
        /// The URL.
        url: String,
    },
    /// An account signed in, or was restored at startup.
    AccountAdded(AccountSummary),
    /// An account's summary changed (its default notification mode, say).
    AccountChanged(AccountSummary),
    /// The server no longer accepts this account's session: it was signed out elsewhere, or its
    /// refresh token was rejected. The account stops syncing. Send [`Command::Logout`] to forget
    /// it here, or, when `kept`, [`Command::Suspend`] to keep its data for signing in again.
    SessionEnded {
        /// Which account.
        account: AccountId,
        /// The server signed it out softly: signing in again on the same device keeps its
        /// history and keys. Also sent at start for an account suspended earlier.
        kept: bool,
    },
    /// At start: the saved accounts being signed back in. Each then arrives as
    /// [`Event::AccountAdded`], or fails with [`Event::AccountError`].
    Restoring(Vec<AccountId>),
    /// An account signed out and was removed.
    AccountRemoved(AccountId),
    /// An account's full room and space list, replacing the previous one.
    RoomsUpdated {
        /// Whose rooms.
        account: AccountId,
        /// Every joined room and space.
        list: RoomList,
    },
    /// The newest message of a room, from sync, for room lists and previews. What a room shows
    /// comes from its timeline instead.
    LatestMessage {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// The message.
        message: Box<Message>,
    },
    /// An open timeline changed; apply the diffs in order.
    TimelineChanged {
        /// Whose room.
        account: AccountId,
        /// Which timeline.
        key: TimelineKey,
        /// The changes.
        diffs: Vec<TimelineDiff>,
    },
    /// The room's cross-device read boundary, even when its event is outside loaded history.
    ReadBoundary {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// The fully-read event, falling back to the existing receipt on first open.
        /// `None` means the server has neither boundary.
        event_id: Option<String>,
        /// Whether that event is in this subscription's applied history.
        loaded: bool,
    },
    /// Who is typing in a room whose timeline is open, the user excepted; empty when nobody is.
    /// Threads share their room's.
    Typing {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// Everyone typing, in the order the server lists them.
        people: Vec<crate::Person>,
    },
    /// Older history finished loading.
    HistoryLoaded {
        /// Whose room.
        account: AccountId,
        /// Which timeline.
        key: TimelineKey,
        /// Nothing older exists, or this user may not see it (see [`HistoryVisibility`]).
        ///
        /// [`HistoryVisibility`]: crate::HistoryVisibility
        reached_start: bool,
    },
    /// A page of history could not be loaded; the timeline stays as it was.
    HistoryFailed {
        /// Whose timeline.
        account: AccountId,
        /// Which timeline.
        key: TimelineKey,
        /// Why.
        error: String,
    },
    /// How the account's encryption stands on this device; sent at start and on every change.
    SecurityChanged {
        /// Which account.
        account: AccountId,
        /// The state.
        security: crate::Security,
    },
    /// The account's signed-in sessions.
    Sessions {
        /// Which account.
        account: AccountId,
        /// This session first, then the most recently active.
        sessions: Vec<crate::Session>,
    },
    /// The email addresses and phone numbers linked to the account.
    ContactDetails {
        /// Whose.
        account: AccountId,
        /// The details, and whether the app may change them.
        contacts: crate::Contacts,
    },
    /// The server sent a link or code to confirm a new contact detail.
    ContactCodeSent {
        /// The [`Command::RequestContactCode`] it answers.
        request: RequestId,
        /// The detail being added, to confirm and add.
        pending: crate::PendingContact,
    },
    /// A room's joined members.
    Members {
        /// The command that requested this snapshot, if any.
        request: Option<RequestId>,
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The members, and whether the user may invite.
        members: crate::Members,
    },
    /// Sync changed membership or power levels; cached member snapshots must be refreshed.
    MembersInvalidated {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
    },
    /// This account's device-local interaction counts, loaded at start and updated on use.
    SearchUsage {
        /// The account whose search sections use these counts.
        account: AccountId,
        /// Open and send frequency, never synced to the homeserver.
        usage: crate::SearchUsage,
    },
    /// Locally stored messages found for [`Command::SearchMessages`].
    MessagesFound {
        /// The search being answered.
        request: RequestId,
        /// Ranked excerpts and their match highlights.
        messages: Vec<crate::MessageHit>,
    },
    /// People found for [`Command::SearchPeople`].
    PeopleFound {
        /// The search it answers.
        request: RequestId,
        /// Who matched.
        people: Vec<crate::Person>,
    },
    /// Public destinations found for [`Command::SearchDirectory`].
    DirectoryFound {
        /// The search being answered.
        request: RequestId,
        /// Up to 20 rooms or spaces in the directory's relevance order.
        rooms: Vec<crate::RoomPreview>,
    },
    /// Public rooms in a space, for [`Command::ListPublicRooms`].
    PublicRooms {
        /// The command this answers.
        request: RequestId,
        /// All public rooms, most members first.
        rooms: Vec<crate::PublicRoom>,
    },
    /// The direct chat for [`Command::OpenDirectChat`].
    DirectChat {
        /// The command it answers.
        request: RequestId,
        /// The chat's room ID.
        room_id: String,
    },
    /// A room as [`Command::PreviewRoom`] found it.
    RoomPreview {
        /// The command it answers.
        request: RequestId,
        /// What the room looks like from outside.
        preview: Box<crate::RoomPreview>,
    },
    /// The room [`Command::JoinRoom`] joined, now in the account's room list.
    RoomJoined {
        /// The command it answers.
        request: RequestId,
        /// Its ID, also when it was joined by an alias.
        room_id: String,
    },
    /// A room or space made by [`Command::CreateRoom`] or [`Command::CreateSpace`].
    RoomCreated {
        /// The command it answers.
        request: RequestId,
        /// Its ID.
        room_id: String,
    },
    /// How the user arranged the sidebar, as saved: sent once an account is ready.
    SidebarLayout {
        /// Whose.
        account: AccountId,
        /// The arrangement.
        layout: crate::SidebarLayout,
    },
    /// The status the user set for an account, as saved: sent once an account is ready and
    /// whenever it changes, here or on another session.
    Status {
        /// Whose.
        account: AccountId,
        /// The status.
        status: crate::Status,
    },
    /// What people show others (presence and status message), the user included: at start for
    /// the people in their direct chats, as this device stored it; then whoever's a sync brings.
    /// The user's own is read from the server, at start and when a sync brings a change, and
    /// reported after [`Command::SetStatusMessage`].
    Presence {
        /// Which account learned it.
        account: AccountId,
        /// One entry per person.
        people: Vec<crate::PersonPresence>,
    },
    /// A new recovery key, to show the user once.
    RecoveryKey {
        /// Which account.
        account: AccountId,
        /// The key.
        key: Password,
    },
    /// The command `request` needs the account password: send it again with one. Its
    /// [`Event::Finished`] follows with [`Error::PasswordRequired`](crate::Error::PasswordRequired).
    PasswordNeeded {
        /// The command that asked.
        request: RequestId,
    },
    /// A verification between this session and another of the user's moved on.
    Verification {
        /// Which account.
        account: AccountId,
        /// The verification.
        flow_id: String,
        /// Where it stands.
        step: crate::VerificationStep,
    },
    /// A linked GIF downloaded for [`Command::FetchGif`].
    GifLoaded {
        /// The link it was asked for with.
        url: String,
        /// The GIF file.
        #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
        bytes: Arc<[u8]>,
    },
    /// A link's preview card for [`Command::PreviewLink`]; `None` when the page has nothing to
    /// show.
    LinkPreview {
        /// The account whose homeserver described it.
        account: AccountId,
        /// The link it was asked for with.
        url: String,
        /// The card.
        preview: Option<Box<crate::links::LinkPreview>>,
    },
    /// Media downloaded for [`Command::FetchMedia`].
    MediaLoaded {
        /// The fetch it answers.
        request: RequestId,
        /// The media.
        media: MediaRef,
        /// The rendition.
        size: MediaSize,
        /// The file's bytes.
        #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
        bytes: Arc<[u8]>,
    },
    /// The server's push rules say the user should hear about this.
    Notification(Notification),
    /// Something went wrong with an account outside any command: a stopped sync, an expired
    /// session, a failed restore.
    AccountError {
        /// Which account.
        account: AccountId,
        /// A message fit to show the user.
        error: String,
    },
    /// The call under way changed: who is in it, their media, the user's own.
    CallUpdated(Box<crate::calls::CallInfo>),
    /// The call ended.
    CallEnded {
        /// Whose.
        account: AccountId,
        /// Its room.
        room_id: String,
        /// Which call ([`CallInfo::call`](crate::calls::CallInfo::call)).
        call: u64,
        /// Why.
        reason: crate::calls::CallEnd,
    },
    /// How loud each participant is now, 0 to 255, by participant key (the user's own under
    /// the empty key); every 100 ms during a MatrixRTC call.
    CallLevels(Vec<(String, u8)>),
    /// Turning on the camera or screen sharing failed: a message fit to show the user.
    CallMediaFailed(String),
    /// The camera (`MediaToggle::Camera`) or a shared screen (`MediaToggle::Screen`) stopped on
    /// its own (unplugged, taken by another app, stopped from the system), so the call stopped
    /// sending it.
    CallMediaStopped(crate::calls::MediaToggle),
    /// A call rings for the user.
    IncomingCall(Box<crate::calls::IncomingCall>),
    /// Pinned message previews for a room.
    PinnedMessages {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// Previews in pin order, including unavailable placeholders.
        messages: Vec<crate::PinnedMessage>,
    },
    /// A room's threads, for [`Command::ListThreads`].
    Threads {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// Its threads, most recently active first.
        threads: Vec<crate::ThreadInfo>,
    },
    /// The files shared in a room, for [`Command::ListFiles`].
    Files {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// Its files, newest first.
        files: Vec<crate::SharedFile>,
    },
    /// The devices calls can use, for [`Command::ListCallDevices`].
    CallDevices(crate::calls::CallDevices),
    /// The call ringing in a room stopped ringing: answered, declined, gone or lapsed.
    RingEnded {
        /// Whose.
        account: AccountId,
        /// Where it rang.
        room_id: String,
    },
    /// What a push is about, for [`Command::FetchNotification`].
    PushContent {
        /// The fetch it answers.
        request: RequestId,
        /// What to show.
        content: crate::model::PushContent,
    },
}

/// Receives the engine's events. Called from the engine's threads, so it must hand them to the
/// UI thread itself (a channel, a UI framework's update handle).
pub trait EventSink: Send + Sync + 'static {
    /// Handles one event.
    fn emit(&self, event: Event);
}

impl<F: Fn(Event) + Send + Sync + 'static> EventSink for F {
    fn emit(&self, event: Event) {
        self(event);
    }
}

/// What the engine needs from the app.
#[derive(Clone)]
pub struct EngineConfig {
    /// The per-user data directory; see [`StoreLayout`].
    pub data_dir: PathBuf,
    /// Where tokens and database passphrases are kept.
    pub secrets: Arc<dyn SecretStore>,
    /// How the app introduces itself to OAuth servers.
    pub identity: ClientIdentity,
    /// Where the browser returns after an OAuth sign-in.
    pub oauth_redirect: crate::account::OAuthRedirect,
    /// Whether anything besides the engine opens an account's store.
    pub stores: StoreSharing,
}

/// Whether anything besides the engine opens an account's store (ADR 0010).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreSharing {
    /// Only the engine does (the desktop): the stores lock within this process. The SDK's
    /// cross-process locks would write a lease to disk every 50 ms while held, as each sync
    /// holds the crypto store's for its whole long poll.
    Alone,
    /// A push handled outside the engine's sync may too (the phones' push extension and push
    /// task, and the engine's own push fetch): they take the SDK's cross-process locks.
    WithPush,
}

impl std::fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig")
            .field("data_dir", &self.data_dir)
            .field("identity", &self.identity)
            .field("oauth_redirect", &self.oauth_redirect)
            .field("stores", &self.stores)
            .finish()
    }
}

/// Sends commands to a running engine. Cheap to clone; the engine stops when every handle
/// is dropped.
#[derive(Clone, Debug)]
pub struct EngineHandle {
    commands: mpsc::UnboundedSender<Envelope>,
    next_request: Arc<AtomicU64>,
    stopped: EngineStopped,
}

/// Waits for an engine to stop, without keeping it running as a handle would.
#[derive(Clone, Debug, Default)]
pub struct EngineStopped(Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>);

impl EngineStopped {
    /// Waits until the engine stopped (once every handle is gone it leaves the call under way
    /// and stops), or `max` passed.
    pub fn wait(&self, max: std::time::Duration) {
        let deadline = std::time::Instant::now() + max;
        let Ok(mut thread) = self.0.lock() else { return };
        while thread.as_ref().is_some_and(|thread| !thread.is_finished()) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if thread.as_ref().is_some_and(std::thread::JoinHandle::is_finished)
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }
}

impl EngineHandle {
    /// What waits for the engine to stop.
    pub fn stopped(&self) -> EngineStopped {
        self.stopped.clone()
    }

    /// Queues a command and returns its request ID. A command sent after the engine stopped is
    /// dropped and never finishes.
    pub fn send(&self, command: Command) -> RequestId {
        let request = RequestId(self.next_request.fetch_add(1, Ordering::Relaxed));
        // An Err means the engine has stopped, which only happens when the app is shutting down.
        let _ = self.commands.send(Envelope { request, command });
        request
    }

    /// A handle with no engine behind it, and the receiving end of its commands: for UIs that
    /// run headless and for tests that assert what a UI asked for.
    pub fn detached() -> (Self, DetachedCommands) {
        let (commands, receiver) = mpsc::unbounded_channel();
        (
            Self { commands, next_request: Arc::new(AtomicU64::new(1)), stopped: EngineStopped::default() },
            DetachedCommands(receiver),
        )
    }
}

/// The commands sent to a [`EngineHandle::detached`] handle.
#[derive(Debug)]
pub struct DetachedCommands(mpsc::UnboundedReceiver<Envelope>);

impl DetachedCommands {
    /// The next queued command, if any, without waiting.
    pub fn try_next(&mut self) -> Option<(RequestId, Command)> {
        self.0.try_recv().ok().map(|envelope| (envelope.request, envelope.command))
    }
}

#[derive(Debug)]
struct Envelope {
    request: RequestId,
    command: Command,
}

/// What a finished task hands back to the loop: only what changes the engine's state.
enum Outcome {
    Done,
    /// Signed in, with what this device knows about the account: it shows before the command
    /// that signed in is answered.
    LoggedIn(Account, Box<AccountSummary>),
    LoggedOut(AccountId),
    TimelineOpened(Box<OpenTimeline>),
}

/// A timeline built and subscribed to, not yet shown: its first entries and its changes.
struct OpenTimeline {
    account: AccountId,
    key: TimelineKey,
    timeline: Arc<matrix_sdk_ui::timeline::Timeline>,
    reset: crate::TimelineDiff,
    changes: TimelineChanges,
    /// How many messages it holds.
    loaded: usize,
    me: matrix_sdk::ruma::OwnedUserId,
    first_page: Option<FirstPage>,
    read_boundary: Option<String>,
    items: eyeball_im::Vector<Arc<matrix_sdk_ui::timeline::TimelineItem>>,
}

type FirstPage = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<crate::TimelineEntry>, Error>> + Send>>;

type TimelineChanges = std::pin::Pin<
    Box<
        dyn futures_util::Stream<Item = Vec<eyeball_im::VectorDiff<Arc<matrix_sdk_ui::timeline::TimelineItem>>>> + Send,
    >,
>;

/// A finished task. `request` is `None` for the engine's own work (restoring sessions).
struct Completion {
    request: Option<RequestId>,
    result: Result<Outcome, Error>,
}

/// Passes `error` on, first telling the desktop when it means the command needs the password.
fn asked_for_password(error: Error, request: RequestId, sink: &dyn EventSink) -> Error {
    if matches!(error, Error::PasswordRequired) {
        sink.emit(Event::PasswordNeeded { request });
    }
    error
}

/// Distinguishes commands that share a handler.
enum Kind {
    Reset,
    Other,
}

/// Starts engines.
#[derive(Debug)]
pub struct Engine;

/// The most threads the engine's runtime runs tasks on. Its work is mostly waiting on the
/// network and the stores (which have their own blocking threads); one worker per core (24 on
/// a large desktop) only adds threads that wake to look for work.
const WORKERS: usize = 4;

impl Engine {
    /// Starts an engine on its own thread with its own Tokio runtime. Saved accounts are
    /// restored right away and reported with [`Event::AccountAdded`].
    pub fn start(config: EngineConfig, sink: impl EventSink) -> Result<EngineHandle, Error> {
        crate::tls::install_provider();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(WORKERS.min(std::thread::available_parallelism().map_or(WORKERS, usize::from)))
            .enable_all()
            .thread_name("matrix-engine")
            .build()?;
        let (commands, receiver) = mpsc::unbounded_channel();
        let (signals, signal_receiver) = mpsc::unbounded_channel();
        let state = State {
            layout: StoreLayout::new(&config.data_dir),
            sessions: Sessions::new(&config.data_dir, config.secrets),
            holder: crate::account::Holder::engine(config.stores),
            identity: config.identity,
            oauth_redirect: config.oauth_redirect,
            oauth_returns: HashMap::new(),
            sink: Arc::new(sink),
            accounts: HashMap::new(),
            tasks: JoinSet::new(),
            running: HashMap::new(),
            http: crate::links::http_client().map_err(|e| e.to_string()),
            signals,
            call: None,
            leaving: crate::calls::Leaving::default(),
            rings: HashMap::new(),
            next_ring: 0,
            status_stamp: 0,
            message_number: 0,
            devices: crate::calls::DeviceChoice::default(),
        };
        let thread = std::thread::Builder::new().name("matrix-engine".into()).spawn(move || {
            runtime.block_on(state.run(receiver, signal_receiver));
            // Blocking work still running (a secret store, a file) needn't hold the app up.
            runtime.shutdown_background();
        })?;
        let stopped = EngineStopped(Arc::new(std::sync::Mutex::new(Some(thread))));
        Ok(EngineHandle { commands, next_request: Arc::new(AtomicU64::new(1)), stopped })
    }
}

struct Running {
    account: Account,
    usage: crate::search::Usage,
    background: Vec<AbortHandle>,
    flows: crate::security::Flows,
    timelines: HashMap<TimelineKey, (Arc<matrix_sdk_ui::timeline::Timeline>, TimelineWatch)>,
    /// Timelines closed lately, oldest first, kept built so opening them again is instant.
    warm: Vec<(TimelineKey, Arc<matrix_sdk_ui::timeline::Timeline>)>,
    /// Timelines being opened, by the request opening each. Closing one forgets it, so an open
    /// that finishes after its close (or after a newer open) keeps the timeline warm instead.
    opening: HashMap<TimelineKey, RequestId>,
    /// Rooms whose keys were fetched from the backup this run; after verification, all again.
    keys_fetched: HashSet<String>,
    /// The account's room list: every re-read of rooms goes through it.
    feed: RoomFeed,
    /// The handlers hearing the account's call signals.
    call_handlers: Vec<matrix_sdk::event_handler::EventHandlerHandle>,
}

/// How many closed timelines an account keeps warm: the rooms and threads switched between.
const WARM_TIMELINES: usize = 8;

impl Running {
    /// Stops following a timeline and keeps it warm.
    fn close_timeline(&mut self, key: &TimelineKey) {
        self.opening.remove(key);
        let Some((timeline, forward)) = self.timelines.remove(key) else { return };
        forward.abort();
        self.keep_warm(key.clone(), timeline);
    }

    fn keep_warm(&mut self, key: TimelineKey, timeline: Arc<matrix_sdk_ui::timeline::Timeline>) {
        self.warm.retain(|(warm, _)| *warm != key);
        self.warm.push((key, timeline));
        if self.warm.len() > WARM_TIMELINES {
            self.warm.remove(0);
        }
    }

    /// A warm timeline for `key`, taken out of the warm set.
    fn take_warm(&mut self, key: &TimelineKey) -> Option<Arc<matrix_sdk_ui::timeline::Timeline>> {
        let index = self.warm.iter().position(|(warm, _)| warm == key)?;
        Some(self.warm.remove(index).1)
    }
}

/// A signed-in account and its summary from this device's store (no requests).
async fn signed_in(account: Account) -> Outcome {
    let cached = account.cached_summary().await;
    Outcome::LoggedIn(account, Box::new(cached))
}

/// Stops a task when the work that started it ends.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        for task in self.background.iter().chain(self.timelines.values().map(|(_, following)| &following.forward)) {
            task.abort();
        }
        for handler in self.call_handlers.drain(..) {
            self.account.client().remove_event_handler(handler);
        }
    }
}

/// How long quitting waits for the call under way to leave.
const QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// The longest a ring rings, whatever its event says.
const RING_CAP: u64 = 2 * 60 * 1000;

/// A call ringing for the user, until it is answered, declined or lapses.
#[derive(Clone, Debug)]
enum Ring {
    /// A MatrixRTC session's ring: answering joins the session.
    Rtc { video: bool, until: u64 },
    /// A one-to-one invite: answering answers it.
    Direct { invite: crate::calls::Invite, video: bool, until: u64 },
}

impl Ring {
    fn until(&self) -> u64 {
        match self {
            Ring::Rtc { until, .. } | Ring::Direct { until, .. } => *until,
        }
    }
}

/// Whether the user is in the room's call on another device. One entry per device: this
/// device's own (stale, after a crash) doesn't count.
fn joined_elsewhere(room: &matrix_sdk::Room) -> bool {
    let me = room.own_user_id();
    let mine = room.active_room_call_participants().iter().filter(|user| *user == me).count();
    let here = room.client().device_id().is_some_and(|device| room.is_device_in_active_room_call(me, device));
    mine > usize::from(here)
}

/// One of the account's rooms, by ID.
fn room_of(account: &Account, room_id: &str) -> Option<matrix_sdk::Room> {
    matrix_sdk::ruma::RoomId::parse(room_id).ok().and_then(|id| account.client().get_room(&id))
}

fn now_ms() -> u64 {
    matrix_sdk::ruma::MilliSecondsSinceUnixEpoch::now().get().into()
}

struct State {
    layout: StoreLayout,
    sessions: Sessions,
    /// How accounts' stores are opened: alone, or under the cross-process locks.
    holder: crate::account::Holder,
    identity: ClientIdentity,
    oauth_redirect: crate::account::OAuthRedirect,
    /// Browser sign-ins waiting for the app to hand their redirect back, by request.
    oauth_returns: HashMap<RequestId, tokio::sync::oneshot::Sender<String>>,
    sink: Arc<dyn EventSink>,
    accounts: HashMap<AccountId, Running>,
    tasks: JoinSet<Completion>,
    /// Command tasks still running, for [`Command::Cancel`].
    running: HashMap<RequestId, AbortHandle>,
    /// For linked files and call services; shared by every account, carries no credentials.
    http: Result<matrix_sdk::reqwest::Client, String>,
    /// Where the accounts' call signals go, for [`State::run`] to route.
    signals: mpsc::UnboundedSender<crate::calls::Signal>,
    /// The call under way.
    call: Option<crate::calls::CallHandle>,
    /// The call last hung up, until it has left: the next call, a sign-out and quitting wait
    /// for it.
    leaving: crate::calls::Leaving,
    /// Calls ringing for the user, by account and room, numbered so a lapse ends only its own.
    rings: HashMap<(AccountId, String), (u64, Ring)>,
    /// The number of the next ring.
    next_ring: u64,
    /// When the last status was chosen (`Command::SetStatus`), in ms since the epoch: each is
    /// stamped later than the one before, even within one millisecond.
    status_stamp: u64,
    /// The number of the last status message set (`Command::SetStatusMessage`).
    message_number: u64,
    /// The microphone, speaker and camera the user picked for calls.
    devices: crate::calls::DeviceChoice,
}

impl State {
    fn flows(&self, account: &AccountId) -> Result<crate::security::Flows, Error> {
        self.accounts
            .get(account)
            .map(|running| running.flows.clone())
            .ok_or_else(|| Error::UnknownAccount(account.clone()))
    }

    fn open_timeline(
        &self,
        account: &AccountId,
        key: &TimelineKey,
    ) -> Result<Arc<matrix_sdk_ui::timeline::Timeline>, Error> {
        self.accounts
            .get(account)
            .and_then(|running| running.timelines.get(key))
            .map(|(timeline, _)| Arc::clone(timeline))
            .ok_or_else(|| Error::TimelineClosed(key.room_id.clone()))
    }

    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<Envelope>,
        mut signals: mpsc::UnboundedReceiver<crate::calls::Signal>,
    ) {
        self.restore_saved();
        loop {
            tokio::select! {
                envelope = commands.recv() => match envelope {
                    Some(envelope) => self.dispatch(envelope),
                    None => break,
                },
                Some(signal) = signals.recv() => self.signal(signal),
                Some(joined) = self.tasks.join_next() => match joined {
                    Ok(completion) => self.complete(completion),
                    // Cancelled tasks were answered when cancelled; a panic loses its answer.
                    Err(error) if error.is_panic() => eprintln!("matrix-engine: a task panicked: {error}"),
                    Err(_) => {}
                },
            }
        }
        // The app is quitting: the call under way (or just hung up) leaves, so nobody sees the
        // user in it for the seconds until the delayed leave would.
        self.hang_up();
        if let Some(task) = self.leaving.any() {
            let _ = tokio::time::timeout(QUIT_WAIT, task).await;
        }
    }

    fn restore_saved(&mut self) {
        let (suspended, saved): (Vec<_>, Vec<_>) =
            self.sessions.list().into_iter().partition(|stored| stored.signed_out);
        self.sink.emit(Event::Restoring(saved.iter().map(|stored| AccountId::new(&stored.id)).collect()));
        // Signed out softly in an earlier run: they wait for the user to sign in again.
        for stored in suspended {
            self.sink.emit(Event::SessionEnded { account: AccountId::new(&stored.id), kept: true });
        }
        for stored in saved {
            let (sessions, holder, sink) = (self.sessions.clone(), self.holder, Arc::clone(&self.sink));
            self.tasks.spawn(async move {
                let result = Account::restore(&sessions, holder, &stored).await;
                if let Err(error) = &result {
                    let error = format!("could not restore the session: {error}");
                    sink.emit(Event::AccountError { account: AccountId::new(&stored.id), error });
                }
                let result = match result {
                    Ok(account) => Ok(signed_in(account).await),
                    Err(error) => Err(error),
                };
                Completion { request: None, result }
            });
        }
    }

    fn feed(&self, id: &AccountId) -> Result<RoomFeed, Error> {
        self.accounts.get(id).map(|running| running.feed.clone()).ok_or_else(|| Error::UnknownAccount(id.clone()))
    }

    /// The call under way, unless it has ended.
    fn live_call(&mut self) -> Option<&crate::calls::CallHandle> {
        if self.call.as_ref().is_some_and(crate::calls::CallHandle::is_over) {
            self.call = None;
        }
        self.call.as_ref()
    }

    /// Ends the call under way, if any, keeping hold of it until it has left.
    fn hang_up(&mut self) {
        if let Some(call) = self.call.take() {
            let account = call.account.clone();
            let room_id = call.room_id.clone();
            self.leaving.hung_up(account, room_id, call.hang_up());
        }
    }

    /// Starts a call, which first waits for the one under way (or the one just hung up) to
    /// leave.
    fn start_call(&mut self, account: Account, room_id: String, start: crate::calls::Start) {
        self.hang_up();
        let previous = self.leaving.any();
        let http = self.http.as_ref().ok().cloned();
        let devices = self.devices.clone();
        let sink = Arc::clone(&self.sink);
        self.call = Some(crate::calls::start(account, room_id, start, devices, sink, http, previous));
    }

    fn end_ring(&mut self, account: &AccountId, room_id: &str) -> Option<Ring> {
        let (_, ring) = self.rings.remove(&(account.clone(), room_id.to_owned()))?;
        self.sink.emit(Event::RingEnded { account: account.clone(), room_id: room_id.to_owned() });
        Some(ring)
    }

    /// The user is leaving (or is out of) a room: its call ends and its ring stops.
    fn leave_calls_in(&mut self, account: &AccountId, room_id: &str) {
        if self.call.as_ref().is_some_and(|call| call.account == *account && call.room_id == room_id) {
            self.hang_up();
        }
        self.end_ring(account, room_id);
    }

    /// An account went: its call ends and its rings stop.
    fn forget_calls(&mut self, account: &AccountId) {
        if self.call.as_ref().is_some_and(|call| call.account == *account) {
            self.hang_up();
        }
        let rooms: Vec<String> =
            self.rings.keys().filter(|(owner, _)| owner == account).map(|(_, room_id)| room_id.clone()).collect();
        for room_id in rooms {
            self.end_ring(account, &room_id);
        }
    }

    /// Routes what sync delivered for calls: to the call under way, or as a call ringing.
    fn signal(&mut self, signal: crate::calls::Signal) {
        use crate::calls::{DirectEvent, Signal};
        if self.call.as_ref().is_some_and(crate::calls::CallHandle::is_over) {
            self.call = None;
        }
        let call = self.call.as_ref();
        let ours =
            |account: &AccountId, room_id: &str| call.is_some_and(|c| c.account == *account && c.room_id == room_id);
        match signal {
            Signal::Key { ref account, .. } => {
                if let Some(call) = call.filter(|c| c.account == *account) {
                    call.send(crate::calls::Control::Signal(signal));
                }
            }
            Signal::Members { ref account, ref room_id } => {
                if ours(account, room_id)
                    && let Some(call) = call
                {
                    call.send(crate::calls::Control::Signal(signal.clone()));
                }
                // A ring is over once its session emptied, or once the user joined it from
                // another device.
                if matches!(self.rings.get(&(account.clone(), room_id.clone())), Some((_, Ring::Rtc { .. }))) {
                    let room = self.accounts.get(account).and_then(|r| room_of(&r.account, room_id));
                    let over = room.is_some_and(|room| !room.has_active_room_call() || joined_elsewhere(&room));
                    if over {
                        let (account, room_id) = (account.clone(), room_id.clone());
                        self.end_ring(&account, &room_id);
                    }
                }
            }
            Signal::Ring { account, room_id, caller, video, until } => {
                // Only an account that can join the session hears it ring.
                let Some(found) = self.accounts.get(&account).map(|r| &r.account) else { return };
                let me = found.id().to_string();
                // A ring whose caller already left (it came late, in a catch-up sync) is over.
                let live = room_of(found, &room_id).is_some_and(|room| room.has_active_room_call());
                if until <= now_ms()
                    || me == caller
                    || ours(&account, &room_id)
                    || found.rtc_service().is_none()
                    || !live
                {
                    return;
                }
                self.ring(account, room_id, caller, Ring::Rtc { video, until });
            }
            Signal::Left { account, room_id } => self.leave_calls_in(&account, &room_id),
            Signal::RingLapsed { account, room_id, ring } => {
                if self.ringing(&account, &room_id, ring) {
                    self.end_ring(&account, &room_id);
                }
            }
            // Shown only if it still rings: it may have ended while its caller was looked up.
            Signal::RingReady(incoming) => {
                let (account, room_id, ring) = (&incoming.0.account, &incoming.0.room_id, incoming.1);
                if self.ringing(account, room_id, ring) {
                    self.sink.emit(Event::IncomingCall(Box::new(incoming.0)));
                }
            }
            Signal::Direct { ref account, ref room_id, ref sender, sent, ref event } => {
                // Only the call's own room and account reach it.
                if let Some(call) =
                    call.filter(|c| c.account == *account && c.room_id == *room_id && c.call_id == event.call_id())
                {
                    call.send(crate::calls::Control::Signal(signal.clone()));
                    return;
                }
                let key = (account.clone(), room_id.clone());
                let me = self.accounts.get(account).map(|r| r.account.id().to_string()).unwrap_or_default();
                match event {
                    DirectEvent::Invite { call_id, party_id, sdp, lifetime, invitee } => {
                        let fresh = sent.saturating_add(*lifetime) > now_ms();
                        // An invite to nobody in particular rings only in a direct chat.
                        let direct = self
                            .accounts
                            .get(account)
                            .and_then(|r| room_of(&r.account, room_id))
                            .is_some_and(|room| !room.direct_targets().is_empty());
                        let for_me = invitee.as_deref().map_or(direct, |invitee| invitee == me);
                        if fresh && for_me && *sender != me && !ours(account, room_id) {
                            let invite = crate::calls::Invite {
                                call_id: call_id.clone(),
                                party_id: party_id.clone(),
                                sdp: sdp.clone(),
                                caller: sender.clone(),
                                candidates: Vec::new(),
                            };
                            let video = crate::calls::has_video(sdp);
                            self.ring(
                                account.clone(),
                                room_id.clone(),
                                sender.clone(),
                                Ring::Direct { invite, video, until: sent.saturating_add(*lifetime) },
                            );
                        }
                    }
                    // While it rings: the caller's candidates (kept for answering), the caller hanging
                    // up, or the user answering or declining on another device.
                    _ => {
                        let Some((_, Ring::Direct { invite, .. })) = self.rings.get_mut(&key) else { return };
                        match crate::calls::hear_ringing(invite, &me, sender, event) {
                            crate::calls::RingHeard::Candidates => {
                                if let DirectEvent::Candidates { candidates, .. } = event {
                                    invite.candidates.extend(candidates.iter().cloned());
                                }
                            }
                            crate::calls::RingHeard::Over => {
                                self.end_ring(account, room_id);
                            }
                            crate::calls::RingHeard::Nothing => {}
                        }
                    }
                }
            }
        }
    }

    /// Reports a call ringing, with who calls, and ends the ring when it lapses.
    fn ring(&mut self, account: AccountId, room_id: String, caller: String, ring: Ring) {
        let Some(running) = self.accounts.get(&account) else { return };
        let (video, via) = match &ring {
            Ring::Rtc { video, .. } => (*video, crate::calls::CallVia::Rtc),
            Ring::Direct { video, .. } => (*video, crate::calls::CallVia::Direct),
        };
        let lapse = ring.until().saturating_sub(now_ms()).min(RING_CAP);
        let id = self.next_ring;
        self.next_ring += 1;
        self.rings.insert((account.clone(), room_id.clone()), (id, ring));
        let (room, signals) = (room_of(&running.account, &room_id), self.signals.clone());
        // It lapses on time however long the caller takes to look up.
        {
            let (account, room_id, signals) = (account.clone(), room_id.clone(), signals.clone());
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(lapse)).await;
                let _ = signals.send(crate::calls::Signal::RingLapsed { account, room_id, ring: id });
            });
        }
        tokio::spawn(async move {
            let caller = match room {
                Some(room) => crate::calls::person(&room, &caller).await,
                None => crate::Person { id: caller.clone(), name: caller, avatar: None },
            };
            let incoming = crate::calls::IncomingCall { account, room_id, caller, video, via };
            let _ = signals.send(crate::calls::Signal::RingReady(Box::new((incoming, id))));
        });
    }

    /// Whether ring number `ring` still rings in the room.
    fn ringing(&self, account: &AccountId, room_id: &str, ring: u64) -> bool {
        self.rings.get(&(account.clone(), room_id.to_owned())).is_some_and(|(id, _)| *id == ring)
    }

    fn account(&self, id: &AccountId) -> Result<Account, Error> {
        self.accounts.get(id).map(|running| running.account.clone()).ok_or_else(|| Error::UnknownAccount(id.clone()))
    }

    /// Starts the task for one command. Runs on the loop, so it must not await.
    fn dispatch(&mut self, Envelope { request, command }: Envelope) {
        let sink = Arc::clone(&self.sink);
        let command_kind = if matches!(command, Command::ResetRecoveryKey(_)) { Kind::Reset } else { Kind::Other };
        let task: std::pin::Pin<Box<dyn Future<Output = Result<Outcome, Error>> + Send>> = match command {
            Command::FinishOAuth { request: sign_in, url } => {
                let result = match self.oauth_returns.remove(&sign_in) {
                    Some(waiting) => waiting.send(url).map_err(|_| Error::Cancelled.to_string()),
                    None => Err(Error::Cancelled.to_string()),
                };
                self.sink.emit(Event::Finished { request, result });
                return;
            }
            Command::Cancel(target) => {
                if let Some(handle) = self.running.remove(&target) {
                    handle.abort();
                    self.sink.emit(Event::Finished { request: target, result: Err(Error::Cancelled.to_string()) });
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::StartCall { account, room_id, video } => {
                let result = self.account(&account).map(|found| {
                    self.end_ring(&account, &room_id);
                    self.start_call(found, room_id, crate::calls::Start::New { video });
                });
                self.sink.emit(Event::Finished { request, result: result.map_err(|e| e.to_string()) });
                return;
            }
            Command::AnswerCall { account, room_id, video } => {
                let ring = self.end_ring(&account, &room_id).filter(|ring| ring.until() > now_ms());
                let result = match (self.account(&account), ring) {
                    (Ok(found), Some(Ring::Rtc { .. })) => {
                        self.start_call(found, room_id, crate::calls::Start::Rtc { video });
                        Ok(())
                    }
                    (Ok(found), Some(Ring::Direct { invite, .. })) => {
                        self.start_call(found, room_id, crate::calls::Start::Answer { video, invite });
                        Ok(())
                    }
                    (Ok(_), None) => Err(crate::calls::CallError::NoCall.to_string()),
                    (Err(error), _) => Err(error.to_string()),
                };
                self.sink.emit(Event::Finished { request, result });
                return;
            }
            Command::DeclineCall { account, room_id } => {
                if let (Some(Ring::Direct { invite, .. }), Ok(found)) =
                    (self.end_ring(&account, &room_id), self.account(&account))
                {
                    tokio::spawn(async move {
                        let room =
                            matrix_sdk::ruma::RoomId::parse(&room_id).ok().and_then(|id| found.client().get_room(&id));
                        let party = found.client().device_id().map(ToString::to_string).unwrap_or_default();
                        if let Some(room) = room {
                            crate::calls::reject(&room, &invite.call_id, &party).await;
                        }
                    });
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::HangUp => {
                self.hang_up();
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::SetCallMedia(toggle) => {
                let result = match self.live_call() {
                    Some(call) => {
                        call.send(crate::calls::Control::Toggle(toggle));
                        Ok(())
                    }
                    None => Err(crate::calls::CallError::NoCall.to_string()),
                };
                self.sink.emit(Event::Finished { request, result });
                return;
            }
            Command::WatchVideo { video, sink } => {
                if let Some(call) = self.live_call() {
                    call.send(crate::calls::Control::Watch(video, sink));
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::ListCallDevices => {
                let sink = Arc::clone(&self.sink);
                tokio::task::spawn_blocking(move || {
                    sink.emit(Event::CallDevices(crate::calls::list_devices()));
                    sink.emit(Event::Finished { request, result: Ok(()) });
                });
                return;
            }
            Command::ChooseCallDevice { kind, id } => {
                self.devices.set(kind, id.clone());
                if let Some(call) = self.live_call() {
                    call.send(crate::calls::Control::Device(kind, id));
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::UnwatchVideo(video) => {
                if let Some(call) = self.live_call() {
                    call.send(crate::calls::Control::Unwatch(video));
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::CloseTimeline { account, key } => {
                if let Some(running) = self.accounts.get_mut(&account) {
                    running.close_timeline(&key);
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::VisitRoom { account, room_id } => {
                let usage = self.accounts.get(&account).map(|running| running.usage.clone());
                let account = self.account(&account);
                Box::pin(async move {
                    let account = account?;
                    if let Some(usage) = usage {
                        usage.room(&account, &room_id, 1, None);
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::OpenTimeline { account: id, key } => {
                let account = self.account(&id);
                let (warm, fetch_keys) = self.accounts.get_mut(&id).map_or((None, false), |running| {
                    running.opening.insert(key.clone(), request);
                    let fetch_keys = key.thread.is_none() && running.keys_fetched.insert(key.room_id.clone());
                    (running.take_warm(&key), fetch_keys)
                });
                Box::pin(async move {
                    Ok(Outcome::TimelineOpened(Box::new(open_timeline(account?, key, warm, fetch_keys).await?)))
                })
            }
            Command::MarkRead { account, key, event_id, fully_read } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    if let Ok(timeline) = timeline {
                        mark_timeline_read(&timeline, event_id.as_deref(), fully_read && key.thread.is_none()).await?;
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::LoadOlder { account: id, key } => {
                let history = self
                    .accounts
                    .get(&id)
                    .and_then(|running| running.timelines.get(&key))
                    .map(|(timeline, following)| (Arc::clone(timeline), following.catchup.clone()))
                    .ok_or_else(|| Error::TimelineClosed(key.room_id.clone()));
                Box::pin(async move {
                    let (timeline, catchup) = history?;
                    load_history(timeline, catchup, crate::HISTORY_PAGE, sink, id, key).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::React { account, key, event_id, emoji } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    let event_id = matrix_sdk::ruma::EventId::parse(&event_id)?;
                    let item = matrix_sdk_ui::timeline::TimelineEventItemId::EventId(event_id);
                    timeline?.toggle_reaction(&item, &emoji).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::Edit { account, key, event_id, body, html } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    let item = matrix_sdk_ui::timeline::TimelineEventItemId::EventId(matrix_sdk::ruma::EventId::parse(
                        &event_id,
                    )?);
                    let content = text_content(&body, html.as_deref());
                    timeline?.edit(&item, matrix_sdk::room::edit::EditedContent::RoomMessage(content)).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::Redact { account, key, event_id } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    let item = matrix_sdk_ui::timeline::TimelineEventItemId::EventId(matrix_sdk::ruma::EventId::parse(
                        &event_id,
                    )?);
                    timeline?.redact(&item, None).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::Forward { account, key, event_id, room_id } => {
                let timeline = self.open_timeline(&account, &key);
                let account = self.account(&account);
                Box::pin(async move {
                    let event_id = matrix_sdk::ruma::EventId::parse(event_id)?;
                    let item = timeline?.item_by_event_id(&event_id).await.ok_or(Error::MessageUnavailable)?;
                    if !matches!(
                        item.content(),
                        matrix_sdk_ui::timeline::TimelineItemContent::MsgLike(
                            matrix_sdk_ui::timeline::MsgLikeContent {
                                kind: matrix_sdk_ui::timeline::MsgLikeKind::Message(_),
                                ..
                            }
                        )
                    ) {
                        return Err(Error::MessageUnavailable);
                    }
                    let raw = item.latest_json().ok_or(Error::MessageUnavailable)?;
                    let event: serde_json::Value =
                        raw.deserialize_as_unchecked().map_err(|_| Error::MessageUnavailable)?;
                    let content = crate::history::forward_content(&event).ok_or(Error::MessageUnavailable)?;
                    account?.forward_message(&room_id, content).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::SetPinned { account, room_id, event_id, pinned } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.set_pinned(&room_id, &event_id, pinned).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ListPinnedMessages { account: id, room_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let messages = account?.pinned_messages(&room_id).await?;
                    sink.emit(Event::PinnedMessages { account: id, room_id, messages });
                    Ok(Outcome::Done)
                })
            }
            Command::VotePoll { account, key, poll_id, answers } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    use matrix_sdk::ruma::events::poll::unstable_response::UnstablePollResponseEventContent;
                    let poll = matrix_sdk::ruma::EventId::parse(&poll_id)?;
                    timeline?.send(UnstablePollResponseEventContent::new(answers, poll).into()).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::EndPoll { account, key, poll_id } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    use matrix_sdk::ruma::events::poll::unstable_end::UnstablePollEndEventContent;
                    let poll = matrix_sdk::ruma::EventId::parse(&poll_id)?;
                    timeline?.send(UnstablePollEndEventContent::new("The poll has ended.", poll).into()).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ListSessions(id) => {
                let account = self.account(&id);
                Box::pin(async move {
                    let sessions = account?.sessions().await?;
                    sink.emit(Event::Sessions { account: id, sessions });
                    Ok(Outcome::Done)
                })
            }
            Command::RenameSession { account: id, device_id, name } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.rename_session(&device_id, &name).await?;
                    sink.emit(Event::Sessions { account: id, sessions: account.sessions().await? });
                    Ok(Outcome::Done)
                })
            }
            Command::EndSession { account: id, device_id, password } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    match account.end_session(&device_id, password.as_ref().map(|p| p.0.as_str())).await {
                        Ok(Some(url)) => sink.emit(Event::OpenUrl { request, url }),
                        Ok(None) => sink.emit(Event::Sessions { account: id, sessions: account.sessions().await? }),
                        Err(error) => return Err(asked_for_password(error, request, &*sink)),
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::SetDisplayName { account, name } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let account = account?;
                    account.set_display_name(&name).await?;
                    sink.emit(Event::AccountChanged(account.summary().await));
                    Ok(Outcome::Done)
                })
            }
            Command::SetAvatar { account, file } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let account = account?;
                    account.set_avatar(file.as_deref()).await?;
                    sink.emit(Event::AccountChanged(account.summary().await));
                    Ok(Outcome::Done)
                })
            }
            Command::ListContactDetails(id) => {
                let account = self.account(&id);
                Box::pin(async move {
                    let contacts = account?.contact_details().await?;
                    sink.emit(Event::ContactDetails { account: id, contacts });
                    Ok(Outcome::Done)
                })
            }
            Command::RequestContactCode { account, kind, address, country } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let pending = account?.request_contact_code(kind, &address, &country).await?;
                    sink.emit(Event::ContactCodeSent { request, pending });
                    Ok(Outcome::Done)
                })
            }
            Command::ConfirmContactCode { account, pending, code } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.confirm_contact_code(&pending, &code).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::AddContact { account: id, pending, password } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    let added = account.add_contact(&pending, password.as_ref().map(|p| p.0.as_str())).await;
                    added.map_err(|error| asked_for_password(error, request, &*sink))?;
                    sink.emit(Event::ContactDetails { account: id, contacts: account.contact_details().await? });
                    Ok(Outcome::Done)
                })
            }
            Command::RemoveContact { account: id, contact } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.remove_contact(&contact).await?;
                    sink.emit(Event::ContactDetails { account: id, contacts: account.contact_details().await? });
                    Ok(Outcome::Done)
                })
            }
            Command::OpenAccountPage { account, page } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let url = account?.account_page(page).await?;
                    sink.emit(Event::OpenUrl { request, url });
                    Ok(Outcome::Done)
                })
            }
            Command::FetchRoomKeys(id) => {
                let account = self.account(&id);
                // The backup may have just become readable: every room's keys are worth a look.
                if let Some(running) = self.accounts.get_mut(&id) {
                    running.keys_fetched.clear();
                }
                let mut rooms: Vec<String> = self
                    .accounts
                    .get(&id)
                    .map(|running| running.timelines.keys().map(|key| key.room_id.clone()).collect())
                    .unwrap_or_default();
                rooms.sort();
                rooms.dedup();
                Box::pin(async move {
                    let account = account?;
                    for room_id in rooms {
                        account.fetch_room_keys(&room_id).await?;
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::ListMembers { account: id, room_id, all } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let members = account?.members(&room_id, all).await?;
                    sink.emit(Event::Members { request: Some(request), account: id, room_id, members });
                    Ok(Outcome::Done)
                })
            }
            Command::ListThreads { account: id, room_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let threads = account?.threads(&room_id).await?;
                    sink.emit(Event::Threads { account: id, room_id, threads });
                    Ok(Outcome::Done)
                })
            }
            Command::ListFiles { account: id, room_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let files = account?.files(&room_id).await?;
                    sink.emit(Event::Files { account: id, room_id, files });
                    Ok(Outcome::Done)
                })
            }
            Command::ModerateMember { account: id, room_id, user_id, action } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.moderate(&room_id, &user_id, action).await?;
                    let members = account.members(&room_id, true).await?;
                    sink.emit(Event::Members { request: Some(request), account: id, room_id, members });
                    Ok(Outcome::Done)
                })
            }
            Command::SearchPeople { account, term } => {
                let usage = self.accounts.get(&account).map(|running| running.usage.clone());
                let account = self.account(&account);
                Box::pin(async move {
                    let usage = match usage {
                        Some(usage) => usage.snapshot().await,
                        None => crate::SearchUsage::default(),
                    };
                    let people = account?.search_people_ranked(&term, &usage).await?;
                    sink.emit(Event::PeopleFound { request, people });
                    Ok(Outcome::Done)
                })
            }
            Command::SearchDirectory { account, term, kind } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let rooms = account?.search_directory(&term, kind).await?;
                    sink.emit(Event::DirectoryFound { request, rooms });
                    Ok(Outcome::Done)
                })
            }
            Command::SearchMessages { account, term } => {
                let usage = self.accounts.get(&account).map(|running| running.usage.clone());
                let account = self.account(&account);
                Box::pin(async move {
                    let usage = match usage {
                        Some(usage) => usage.snapshot().await,
                        None => crate::SearchUsage::default(),
                    };
                    let messages = account?.search_messages(&term, &usage).await?;
                    sink.emit(Event::MessagesFound { request, messages });
                    Ok(Outcome::Done)
                })
            }
            Command::SuggestPeople(id) => {
                let account = self.account(&id);
                Box::pin(async move {
                    let people = account?.suggest_people().await?;
                    sink.emit(Event::PeopleFound { request, people });
                    Ok(Outcome::Done)
                })
            }
            Command::ListPublicRooms { account, space } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let rooms = account?.public_rooms(&space).await?;
                    sink.emit(Event::PublicRooms { request, rooms });
                    Ok(Outcome::Done)
                })
            }
            Command::Invite { account: id, room_id, user_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.invite(&room_id, &user_id).await?;
                    let members = account.members(&room_id, false).await?;
                    sink.emit(Event::Members { request: Some(request), account: id, room_id, members });
                    Ok(Outcome::Done)
                })
            }
            Command::OpenDirectChat { account, user_id } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let room_id = account?.direct_chat(&user_id).await?;
                    sink.emit(Event::DirectChat { request, room_id });
                    Ok(Outcome::Done)
                })
            }
            Command::PreviewRoom { account, room, via } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let preview = account?.preview_room(&room, &via).await?;
                    sink.emit(Event::RoomPreview { request, preview: Box::new(preview) });
                    Ok(Outcome::Done)
                })
            }
            Command::JoinRoom { account: id, room_id: room, via } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                Box::pin(async move {
                    let (account, feed) = (account?, feed?);
                    let room_id = account.join_room(&room, &via).await?;
                    // The app opens the room at once: the list it reads must have it already.
                    feed.refresh(Some(&HashSet::from([room_id.clone()]))).await;
                    sink.emit(Event::RoomJoined { request, room_id });
                    Ok(Outcome::Done)
                })
            }
            Command::CreateRoom { account, name, topic, space } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let room_id = account?.create_room(&name, topic.as_deref(), space.as_deref()).await?;
                    sink.emit(Event::RoomCreated { request, room_id });
                    Ok(Outcome::Done)
                })
            }
            Command::CreateSpace { account, name } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let room_id = account?.create_space(&name).await?;
                    sink.emit(Event::RoomCreated { request, room_id });
                    Ok(Outcome::Done)
                })
            }
            Command::RenameRoom { account: id, room_id, name } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                Box::pin(async move {
                    account?.rename_room(&room_id, &name).await?;
                    feed?.refresh(Some(&HashSet::from([room_id]))).await;
                    Ok(Outcome::Done)
                })
            }
            Command::SetSidebarLayout { account, layout } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.set_sidebar_layout(&layout).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ResetIdentity { account, password } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let approve = |url| sink.emit(Event::OpenUrl { request, url });
                    match account?.reset_identity(password.as_ref().map(|p| p.0.as_str()), approve).await {
                        Ok(()) => Ok(Outcome::Done),
                        Err(error) => Err(asked_for_password(error, request, &*sink)),
                    }
                })
            }
            Command::Recover { account, key } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.recover(&key.0).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::SetUpRecovery(id) | Command::ResetRecoveryKey(id) => {
                let reset = matches!(command_kind, Kind::Reset);
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    let key =
                        if reset { account.reset_recovery_key().await? } else { account.set_up_recovery().await? };
                    sink.emit(Event::RecoveryKey { account: id, key: Password(key) });
                    Ok(Outcome::Done)
                })
            }
            Command::VerifyThisSession(id) => {
                let (account, flows) = (self.account(&id), self.flows(&id));
                Box::pin(async move {
                    account?.verify_with_other_device(&flows?, sink).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::AcceptVerification { account, flow_id } => {
                let flows = self.flows(&account);
                Box::pin(async move {
                    flows?.accept(&flow_id).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ConfirmVerification { account, flow_id, matches } => {
                let flows = self.flows(&account);
                Box::pin(async move {
                    flows?.confirm(&flow_id, matches).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::CancelVerification { account, flow_id } => {
                let flows = self.flows(&account);
                Box::pin(async move {
                    flows?.cancel(&flow_id).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::FetchGif { url } => {
                let http = self.http.clone();
                Box::pin(async move {
                    let http = http.map_err(|reason| Error::Link { url: url.clone(), reason })?;
                    let bytes = crate::links::fetch_gif(&http, &url).await?;
                    sink.emit(Event::GifLoaded { url, bytes: bytes.into() });
                    Ok(Outcome::Done)
                })
            }
            Command::PreviewLink { account: id, url } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let preview = account?.preview_link(&url).await?;
                    sink.emit(Event::LinkPreview { account: id, url, preview: preview.map(Box::new) });
                    Ok(Outcome::Done)
                })
            }
            Command::Discover { server } => Box::pin(async move {
                let homeserver = discovery::discover(&server).await?;
                sink.emit(Event::HomeserverDiscovered { request, homeserver });
                Ok(Outcome::Done)
            }),
            Command::Login(login) => {
                let (layout, sessions, holder) = (self.layout.clone(), self.sessions.clone(), self.holder);
                Box::pin(async move {
                    Ok(signed_in(Account::login_password(&layout, &sessions, holder, &login).await?).await)
                })
            }
            Command::LoginOAuth { homeserver, resume } => {
                let (layout, sessions, identity) = (self.layout.clone(), self.sessions.clone(), self.identity.clone());
                let holder = self.holder;
                self.oauth_returns.retain(|_, waiting| !waiting.is_closed());
                let app_redirect = match &self.oauth_redirect {
                    crate::account::OAuthRedirect::Loopback => None,
                    crate::account::OAuthRedirect::App(uri) => {
                        let (sender, returned) = tokio::sync::oneshot::channel();
                        self.oauth_returns.insert(request, sender);
                        Some((uri.clone(), returned))
                    }
                };
                Box::pin(async move {
                    let pending = Account::start_oauth(
                        &layout,
                        &sessions,
                        holder,
                        &homeserver,
                        &identity,
                        resume.as_ref(),
                        app_redirect,
                    )
                    .await?;
                    sink.emit(Event::OpenUrl { request, url: pending.url.to_string() });
                    Ok(signed_in(pending.finish(&sessions).await?).await)
                })
            }
            Command::Logout(id) => {
                let (account, sessions) = (self.account(&id).ok(), self.sessions.clone());
                // The account's call leaves while its token still works.
                self.forget_calls(&id);
                let leaving = self.leaving.of_account(&id);
                Box::pin(async move {
                    if let Some(task) = leaving {
                        let _ = tokio::time::timeout(QUIT_WAIT, task).await;
                    }
                    match account {
                        Some(account) => account.logout(&sessions).await?,
                        None => crate::account::forget(&sessions, &id).await?,
                    }
                    Ok(Outcome::LoggedOut(id))
                })
            }
            Command::Suspend(id) => {
                let sessions = self.sessions.clone();
                Box::pin(async move {
                    crate::account::set_signed_out(&sessions, &id).await?;
                    Ok(Outcome::LoggedOut(id))
                })
            }
            Command::Typing { account, room_id, typing } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.typing(&room_id, typing).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::Send { account, key, reply_to, message } => {
                let usage = self.accounts.get(&account).map(|running| running.usage.clone());
                let (account, timeline) = (self.account(&account), self.open_timeline(&account, &key));
                Box::pin(async move {
                    let account = account?;
                    let replied_to = reply_to.clone();
                    match timeline {
                        // Through the timeline: replies and thread relations come out right, and
                        // the message shows at once as pending.
                        Ok(timeline) => send_in(&timeline, reply_to, &message).await?,
                        Err(_) if key.thread.is_none() && reply_to.is_none() => {
                            account.send(&key.room_id, &message).await?;
                        }
                        Err(error) => return Err(error),
                    }
                    if let Some(usage) = usage {
                        usage.room(&account, &key.room_id, 3, replied_to);
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::FetchMedia { account, media, size } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let bytes = account?.fetch_media(&media, size).await?;
                    sink.emit(Event::MediaLoaded { request, media, size, bytes: bytes.into() });
                    Ok(Outcome::Done)
                })
            }
            Command::SaveMedia { account, media, path } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let bytes = account?.fetch_media(&media, MediaSize::Original).await?;
                    tokio::fs::write(&path, bytes).await.map_err(Error::Storage)?;
                    Ok(Outcome::Done)
                })
            }
            Command::LeaveRoom { account: id, room_id, forget } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                // The room's call leaves first, while the user can still write to the room.
                // Whether it was under way or already hung up: the room is left after it.
                self.leave_calls_in(&id, &room_id);
                let leaving = self.leaving.of_room(&id, &room_id);
                Box::pin(async move {
                    if let Some(task) = leaving {
                        let _ = tokio::time::timeout(QUIT_WAIT, task).await;
                    }
                    account?.leave_room(&room_id, forget).await?;
                    feed?.refresh(Some(&HashSet::from([room_id]))).await;
                    Ok(Outcome::Done)
                })
            }
            Command::SetDefaultNotificationMode { account: id, mode } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                Box::pin(async move {
                    let account = account?;
                    account.set_default_notification_mode(mode).await?;
                    sink.emit(Event::AccountChanged(account.summary().await));
                    feed?.refresh(None).await;
                    Ok(Outcome::Done)
                })
            }
            Command::SetStatus { account: id, status } => {
                let account = self.account(&id);
                // Stamped here, as commands arrive: choices settle in the order they were made,
                // whichever task runs first.
                let changed_at = now_ms().max(self.status_stamp + 1);
                self.status_stamp = changed_at;
                Box::pin(async move {
                    let report = |status| sink.emit(Event::Status { account: id.clone(), status });
                    account?.set_status(status, changed_at, report).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::SetStatusMessage { account: id, message } => {
                let account = self.account(&id);
                // Numbered here, as commands arrive: messages settle in the order they were set.
                self.message_number += 1;
                let number = self.message_number;
                Box::pin(async move {
                    let report = |me| sink.emit(Event::Presence { account: id.clone(), people: vec![me] });
                    account?.set_status_message(message.as_deref(), number, report).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::SetNotificationMode { account: id, room_id, mode } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                Box::pin(async move {
                    account?.set_notification_mode(&room_id, mode).await?;
                    feed?.refresh(Some(&HashSet::from([room_id]))).await;
                    Ok(Outcome::Done)
                })
            }
            Command::RegisterPusher { account, pusher } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.register_pusher(&pusher).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::RemovePusher { account, app_id, push_key } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.remove_pusher(&app_id, &push_key).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::FetchNotification { account, room_id, event_id } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let account = account?;
                    let content = crate::push::fetch(account.client(), account.id(), &room_id, &event_id).await?;
                    sink.emit(Event::PushContent { request, content });
                    Ok(Outcome::Done)
                })
            }
        };
        let handle = self.tasks.spawn(async move { Completion { request: Some(request), result: task.await } });
        self.running.insert(request, handle);
    }

    /// Applies a finished task's outcome to the state, then answers its request.
    fn complete(&mut self, Completion { request, result }: Completion) {
        if let Some(request) = request {
            self.running.remove(&request);
        }
        let result = match result {
            Ok(Outcome::Done) => Ok(()),
            Ok(Outcome::LoggedIn(account, cached)) => {
                // Shown now, before the sign-in's `Finished`: the app selects an account it is
                // adding when it arrives.
                self.sink.emit(Event::AccountAdded((*cached).clone()));
                // Saving refreshed tokens comes first: the summary's requests may refresh them.
                let tokens = tokio::spawn(watch_session(account.clone(), Arc::clone(&self.sink)));
                let flows = crate::security::Flows::default();
                let feed = RoomFeed { account: account.clone(), sink: Arc::clone(&self.sink), cache: Arc::default() };
                let call_handlers = crate::calls::listen(account.client(), account.id(), &self.signals);
                // The rest waits on the server, so it runs beside the engine rather than holding
                // up every account's commands. The account shows before anything about it.
                let started = {
                    let (account, flows, sink, feed) =
                        (account.clone(), flows.clone(), Arc::clone(&self.sink), feed.clone());
                    tokio::spawn(async move {
                        // What this device knew showed at once, even offline; the server's
                        // profile follows.
                        let profile = async {
                            sink.emit(Event::AccountChanged(account.refreshed_summary(&cached).await));
                        };
                        let layout = async {
                            // Until the saved layout is read, the app doesn't save over it.
                            let mut retry = std::time::Duration::from_secs(2);
                            let layout = loop {
                                match account.sidebar_layout().await {
                                    Ok(layout) => break layout,
                                    Err(error) => {
                                        eprintln!("matrix-engine: could not read the sidebar layout: {error}")
                                    }
                                }
                                tokio::time::sleep(retry).await;
                                retry = (retry * 2).min(std::time::Duration::from_secs(60));
                            };
                            sink.emit(Event::SidebarLayout { account: account.id().clone(), layout });
                        };
                        let status = account
                            .report_status(|status| sink.emit(Event::Status { account: account.id().clone(), status }));
                        // Once the homeserver names a call service, rooms offer MatrixRTC calls.
                        let calls = {
                            let (account, feed) = (account.clone(), feed.clone());
                            async move {
                                if account.discover_rtc().await {
                                    feed.refresh(None).await;
                                }
                            }
                        };
                        // What people show as this device stored it, before syncing starts: a
                        // sync's newer report must not come first and be undone by it.
                        let people = account.known_presence().await;
                        if !people.is_empty() {
                            sink.emit(Event::Presence { account: account.id().clone(), people });
                        }
                        // The user's own presence, read from the server at start and again when a
                        // sync asks; asks made meanwhile wait as one. It ends with the account.
                        let own_refresh = Arc::new(tokio::sync::Notify::new());
                        let own = async {
                            let report =
                                |me| sink.emit(Event::Presence { account: account.id().clone(), people: vec![me] });
                            loop {
                                account.report_own_presence(report).await;
                                own_refresh.notified().await;
                            }
                        };
                        tokio::join!(
                            profile,
                            layout,
                            status,
                            own,
                            calls,
                            crate::security::watch(account.clone(), flows, Arc::clone(&sink)),
                            sync_loop(account.clone(), Arc::clone(&sink), feed, Arc::clone(&own_refresh)),
                        );
                    })
                };
                let (usage, usage_worker) = crate::search::Usage::start(account.clone(), Arc::clone(&self.sink));
                let background = vec![tokens.abort_handle(), started.abort_handle(), usage_worker];
                let running = Running {
                    account,
                    usage,
                    background,
                    flows,
                    timelines: HashMap::new(),
                    warm: Vec::new(),
                    opening: HashMap::new(),
                    keys_fetched: HashSet::new(),
                    feed,
                    call_handlers,
                };
                self.accounts.insert(running.account.id().clone(), running);
                Ok(())
            }
            Ok(Outcome::TimelineOpened(open)) => {
                if let Some(running) = self.accounts.get_mut(&open.account) {
                    let wanted = request.is_some() && running.opening.get(&open.key) == request.as_ref();
                    if wanted {
                        running.opening.remove(&open.key);
                        let (key, timeline) = (open.key.clone(), Arc::clone(&open.timeline));
                        let forward = start_timeline(*open, &self.sink);
                        if let Some((_, old)) = running.timelines.insert(key, (timeline, forward)) {
                            old.abort();
                        }
                    } else if !running.timelines.contains_key(&open.key) && !running.opening.contains_key(&open.key) {
                        // Closed while opening: nothing shows it, but switching back is instant.
                        running.keep_warm(open.key, open.timeline);
                    }
                }
                Ok(())
            }
            Ok(Outcome::LoggedOut(id)) => {
                self.forget_calls(&id);
                self.accounts.remove(&id);
                self.sink.emit(Event::AccountRemoved(id));
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        };
        if let Some(request) = request {
            self.sink.emit(Event::Finished { request, result });
        }
    }
}

/// Syncs one account until it fails or is aborted, reporting rooms, new messages and
/// notifications. The first response only catches up, so it notifies about nothing.
/// `own_refresh` asks for the user's own presence to be read again.
async fn sync_loop(account: Account, sink: Arc<dyn EventSink>, feed: RoomFeed, own_refresh: Arc<tokio::sync::Notify>) {
    // Waits between failed syncs: from a few seconds, doubling, to a minute.
    const FIRST_RETRY: std::time::Duration = std::time::Duration::from_secs(2);
    const LAST_RETRY: std::time::Duration = std::time::Duration::from_secs(60);
    let caught_up = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut retry = FIRST_RETRY;
    loop {
        let started = std::time::Instant::now();
        let Err(error) = sync_until_error(&account, &sink, &feed, &caught_up, &own_refresh).await else { return };
        // The session is over. The SDK broadcasts every ending as `UnknownToken`, with whether
        // it was soft, and `watch_session` reports that: the error here may hide it (a rejected
        // refresh), so reporting from here too could call a soft ending hard.
        if crate::account::session_ended(&error) {
            return;
        }
        // A sync that ran a while before failing was healthy: start the waits over.
        if started.elapsed() > LAST_RETRY {
            retry = FIRST_RETRY;
        }
        eprintln!("matrix-engine: sync of {} failed, retrying in {retry:?}: {error}", account.id());
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(LAST_RETRY);
    }
}

/// Syncs until the first error.
async fn sync_until_error(
    account: &Account,
    sink: &Arc<dyn EventSink>,
    feed: &RoomFeed,
    caught_up: &Arc<std::sync::atomic::AtomicBool>,
    own_refresh: &Arc<tokio::sync::Notify>,
) -> Result<(), matrix_sdk::Error> {
    // Members load as rooms need them (lazy loading): a first sync doesn't download every member
    // of every room.
    let filter = matrix_sdk::ruma::api::client::filter::FilterDefinition::with_lazy_loading();
    let settings = SyncSettings::default()
        .timeout(std::time::Duration::from_secs(30))
        .filter(matrix_sdk::ruma::api::client::sync::sync_events::v3::Filter::FilterDefinition(filter));
    // Room changes the SDK works out on its own (unread counts, each room's latest event) are
    // heard from before the first read: a latest event worked out for a room the read below
    // registers isn't missed. Missed ones (the channel overflowing) re-read every room.
    let mut updates = account.client().room_info_notable_update_receiver();
    // What is on disk shows before the first sync answers.
    feed.refresh(None).await;
    // A space's people come from its full member list, which lazy loading leaves out: each is
    // fetched once, one after another, beside the sync rather than in its way. Spaces arrive from
    // the store now and from syncs later (all of them, after a fresh sign-in).
    let (wanted_members, mut spaces) = tokio::sync::mpsc::unbounded_channel::<matrix_sdk::ruma::OwnedRoomId>();
    let members = {
        let feed = feed.clone();
        tokio::spawn(async move {
            let mut tried = HashSet::new();
            while let Some(first) = spaces.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = spaces.try_recv() {
                    batch.push(more);
                }
                let mut synced = HashSet::new();
                for room_id in batch {
                    let Some(space) = feed.account.client().get_room(&room_id) else { continue };
                    if tried.insert(room_id.clone())
                        && !space.are_members_synced()
                        && space.sync_members().await.is_ok()
                    {
                        synced.insert(room_id.to_string());
                    }
                }
                if !synced.is_empty() {
                    feed.refresh(Some(&synced)).await;
                }
            }
        })
    };
    let _members = AbortOnDrop(members.abort_handle());
    let want_members = |room: &matrix_sdk::Room| {
        if room.is_space() && !room.are_members_synced() {
            let _ = wanted_members.send(room.room_id().to_owned());
        }
    };
    for room in account.client().joined_rooms() {
        want_members(&room);
    }
    // Unread counts and each room's latest event are worked out after a sync's callback runs,
    // and receipts sent from here change them too: the SDK says which rooms, and those are read
    // again.
    let unread = {
        use matrix_sdk_base::RoomInfoNotableUpdateReasons as Reasons;
        let feed = feed.clone();
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::{RecvError, TryRecvError};
            // Names and heroes too: a sync re-reads only rooms whose events changed
            // (`summary_may_change`), and the SDK can change a name from the room's summary.
            let counts = Reasons::READ_RECEIPT
                | Reasons::UNREAD_MARKER
                | Reasons::LATEST_EVENT
                | Reasons::DISPLAY_NAME
                | Reasons::HEROES;
            loop {
                let mut changed = HashSet::new();
                // Missed updates: any room may have changed.
                let mut all = false;
                match updates.recv().await {
                    Ok(update) if update.reasons.intersects(counts) => {
                        changed.insert(update.room_id.to_string());
                    }
                    Ok(_) => continue,
                    Err(RecvError::Lagged(_)) => all = true,
                    Err(RecvError::Closed) => return,
                }
                // Several at once (a sync touching many rooms) make one update.
                loop {
                    match updates.try_recv() {
                        Ok(update) if update.reasons.intersects(counts) => {
                            changed.insert(update.room_id.to_string());
                        }
                        Ok(_) => {}
                        Err(TryRecvError::Lagged(_)) => all = true,
                        Err(_) => break,
                    }
                }
                feed.refresh((!all).then_some(&changed)).await;
            }
        })
    };
    let _unread = AbortOnDrop(unread.abort_handle());
    let on_response = |response: matrix_sdk::sync::SyncResponse, sent: PresenceState| {
        let (account, sink, caught_up, feed) = (account.clone(), Arc::clone(sink), Arc::clone(caught_up), feed.clone());
        let want_members = &want_members;
        async move {
            for (room_id, update) in &response.rooms.joined {
                let Some(room) = account.client().get_room(room_id) else { continue };
                if members_changed(update) {
                    sink.emit(Event::MembersInvalidated {
                        account: account.id().clone(),
                        room_id: room_id.to_string(),
                    });
                }
                want_members(&room);
                if let Some(message) = convert::messages(&room, &update.timeline.events).await.pop() {
                    sink.emit(Event::LatestMessage {
                        account: account.id().clone(),
                        room_id: room_id.to_string(),
                        message: Box::new(message),
                    });
                }
            }
            if caught_up.swap(true, Ordering::Relaxed) {
                for (room_id, notifications) in &response.notifications {
                    let Some(room) = account.client().get_room(room_id) else { continue };
                    for notification in notifications {
                        if let Some(n) = convert::notification(account.id(), &room, notification).await {
                            sink.emit(Event::Notification(n));
                        }
                    }
                }
            }
            // Only the rooms this sync changed are read again; direct chats and push rules can
            // change any room, so they read them all.
            let rooms = &response.rooms;
            let changed: HashSet<String> = rooms
                .joined
                .iter()
                .filter(|(_, update)| summary_may_change(update))
                .map(|(room_id, _)| room_id)
                .chain(rooms.left.keys())
                .chain(rooms.invited.keys())
                .map(ToString::to_string)
                .collect();
            let all = response.account_data.iter().any(|event| {
                matches!(event.get_field::<String>("type").ok().flatten().as_deref(), Some("m.direct" | "m.push_rules"))
            });
            if crate::sidebar::changed_in(&response.account_data)
                && let Ok(layout) = account.sidebar_layout().await
            {
                sink.emit(Event::SidebarLayout { account: account.id().clone(), layout });
            }
            // The user's own, as the server says now: the sync may predate a status message saved
            // meanwhile.
            let me = account.client().user_id().map(ToString::to_string);
            let (own, people): (Vec<_>, Vec<_>) = crate::people::synced_presence(&response.presence)
                .into_iter()
                .partition(|shown| me.as_deref() == Some(shown.user_id.as_str()));
            if !people.is_empty() {
                sink.emit(Event::Presence { account: account.id().clone(), people });
            }
            // Read aside, so the next sync doesn't wait for it.
            if !own.is_empty() {
                own_refresh.notify_one();
            }
            let report = |status| sink.emit(Event::Status { account: account.id().clone(), status });
            account.follow_status(&response.account_data, &sent, report).await;
            feed.refresh((!all).then_some(&changed)).await;
            LoopCtrl::Continue
        }
    };
    // The SDK's sync loop, with each sync under the crypto store's lock (`sync_once_locked`):
    // the next sync continues from the last one's token, the first error ends the loop for
    // `sync_loop` to retry, and syncs answered at once are spaced a second apart.
    let mut settings = settings;
    let mut last_sync: Option<std::time::Instant> = None;
    loop {
        if last_sync.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(1)) {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        last_sync = Some(std::time::Instant::now());
        // Each sync announces the status as last synced, so one set elsewhere holds here too.
        let sent = crate::status::sync_presence(account.status().await, caught_up.load(Ordering::Relaxed));
        settings = settings.set_presence(sent.clone());
        let response = match account.sync_once_locked(settings.clone()).await {
            Ok(response) => response,
            Err(error) => {
                account.sync_failed(&sent).await;
                return Err(error);
            }
        };
        settings = settings.token(response.next_batch.clone());
        if on_response(response, sent).await == LoopCtrl::Break {
            return Ok(());
        }
    }
}

/// Whether a room's sync update may change its summary: anything but typing notices, which a
/// busy room sends many of. Read receipts count; the unread counts they change are also
/// reported apart (`RoomInfoNotableUpdate`).
fn summary_may_change(update: &matrix_sdk_base::sync::JoinedRoomUpdate) -> bool {
    let state = match &update.state {
        matrix_sdk_base::sync::State::Before(events) | matrix_sdk_base::sync::State::After(events) => events,
    };
    let typing = |event: &matrix_sdk::ruma::serde::Raw<_>| {
        event.get_field::<String>("type").ok().flatten().as_deref() == Some("m.typing")
    };
    !update.timeline.events.is_empty()
        || update.timeline.limited
        || !state.is_empty()
        || !update.account_data.is_empty()
        || !update.ambiguity_changes.is_empty()
        || update.avatar_changes.is_some()
        || !update.ephemeral.iter().all(typing)
}

fn members_changed(update: &matrix_sdk_base::sync::JoinedRoomUpdate) -> bool {
    let relevant = |kind: Option<String>| matches!(kind.as_deref(), Some("m.room.member" | "m.room.power_levels"));
    let state = match &update.state {
        matrix_sdk_base::sync::State::Before(events) | matrix_sdk_base::sync::State::After(events) => events,
    };
    state.iter().any(|event| relevant(event.get_field("type").ok().flatten()))
        || matches!(update.state, matrix_sdk_base::sync::State::Before(_))
            && update.timeline.events.iter().any(|event| relevant(event.raw().get_field("type").ok().flatten()))
}

/// One sync loop's room list, as last reported. The sync, the unread listener and the member
/// fetch all re-read rooms through it, one at a time, so the list reported last is always the
/// newest.
#[derive(Clone)]
struct RoomFeed {
    account: Account,
    sink: Arc<dyn EventSink>,
    cache: Arc<tokio::sync::Mutex<crate::account::RoomCache>>,
}

impl RoomFeed {
    /// Re-reads the rooms in `changed` (all when `None`) and reports the list if it changed:
    /// most syncs change no room's summary (receipts, presence).
    async fn refresh(&self, changed: Option<&HashSet<String>>) {
        let mut cache = self.cache.lock().await;
        if let Some(list) = self.account.update_room_list(&mut cache, changed).await {
            self.sink.emit(Event::RoomsUpdated { account: self.account.id().clone(), list });
        }
    }
}

async fn mark_timeline_read(
    timeline: &matrix_sdk_ui::timeline::Timeline,
    event_id: Option<&str>,
    fully_read: bool,
) -> Result<(), Error> {
    use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
    if fully_read {
        let event_id = match event_id {
            Some(id) => Some(matrix_sdk::ruma::EventId::parse(id)?),
            None => timeline.latest_event_id().await,
        };
        if let Some(event_id) = event_id {
            let receipts =
                matrix_sdk::room::Receipts::new().fully_read_marker(event_id.clone()).public_read_receipt(event_id);
            timeline.send_multiple_receipts(receipts).await?;
        }
    } else if let Some(event_id) = event_id {
        timeline.send_single_receipt(ReceiptType::Read, matrix_sdk::ruma::EventId::parse(event_id)?).await?;
    } else {
        timeline.mark_as_read(ReceiptType::Read).await?;
    }
    Ok(())
}

/// Builds a timeline (or takes a warm one) and subscribes to it. Nothing reaches the app yet:
/// [`start_timeline`] does that once the engine knows the timeline is still wanted.
async fn open_timeline(
    account: Account,
    key: TimelineKey,
    warm: Option<Arc<matrix_sdk_ui::timeline::Timeline>>,
    fetch_keys: bool,
) -> Result<OpenTimeline, Error> {
    let me = account.user_id()?;
    // Keys missing from this device may wait in the backup; fetching them doesn't hold up
    // opening, and the timeline decrypts again once they are stored.
    if fetch_keys {
        let (account, room_id) = (account.clone(), key.room_id.clone());
        tokio::spawn(async move {
            if let Err(error) = account.fetch_room_keys(&room_id).await {
                eprintln!("matrix-engine: could not fetch room keys for {room_id}: {error}");
            }
        });
    }
    let timeline = match warm {
        Some(timeline) => timeline,
        None => Arc::new(account.timeline(&key).await?),
    };
    let (items, changes) = timeline.subscribe().await;
    let read_boundary = if key.thread.is_none() {
        match timeline.room().fully_read_event_id() {
            Some(id) => Some(id.to_string()),
            None => timeline.latest_user_read_receipt(&me).await.map(|(id, _)| id.to_string()),
        }
    } else {
        None
    };
    let loaded = crate::timeline::message_count(&items);
    let reset = crate::timeline::diff(eyeball_im::VectorDiff::Reset { values: items.clone() }, &me);
    let first_page: Option<FirstPage> = if key.thread.is_some() && loaded <= 1 {
        let (account, key) = (account.clone(), key.clone());
        Some(Box::pin(async move { account.thread_first_page(&key).await }))
    } else {
        None
    };
    Ok(OpenTimeline {
        account: account.id().clone(),
        key,
        timeline,
        reset,
        changes: Box::pin(changes),
        loaded,
        me,
        first_page,
        read_boundary,
        items,
    })
}

/// A followed timeline's forwarding task and its SDK catch-up progress.
struct TimelineWatch {
    forward: AbortHandle,
    catchup: tokio::sync::watch::Receiver<crate::timeline_bootstrap::Progress>,
}

impl TimelineWatch {
    fn abort(&self) {
        self.forward.abort();
    }
}

/// Shows cached entries, fills history beside them and forwards later subscription changes.
fn start_timeline(open: OpenTimeline, sink: &Arc<dyn EventSink>) -> TimelineWatch {
    use futures_util::StreamExt;

    let OpenTimeline {
        account: id,
        key,
        timeline,
        reset,
        mut changes,
        loaded,
        me,
        mut first_page,
        read_boundary,
        mut items,
    } = open;
    let mut bootstrap = first_page
        .as_ref()
        .and(key.thread.as_deref())
        .map(|root| crate::timeline_bootstrap::Bootstrap::new(&reset, root));
    let (catchup, catching_up) = tokio::sync::watch::channel(
        bootstrap
            .as_ref()
            .map_or(crate::timeline_bootstrap::Progress::Complete, crate::timeline_bootstrap::Bootstrap::progress),
    );
    let boundary_loaded = |items: &eyeball_im::Vector<Arc<matrix_sdk_ui::timeline::TimelineItem>>,
                           read_boundary: &Option<String>| {
        read_boundary.as_ref().is_none_or(|id| {
            items.iter().any(|item| {
                item.as_event().and_then(|event| event.event_id()).is_some_and(|event| event.as_str() == id)
            })
        })
    };
    let mut read_boundary_loaded = boundary_loaded(&items, &read_boundary);
    if key.thread.is_none() {
        sink.emit(Event::ReadBoundary {
            account: id.clone(),
            room_id: key.room_id.clone(),
            event_id: read_boundary.clone(),
            loaded: read_boundary_loaded,
        });
    }
    sink.emit(Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs: vec![reset] });
    let forward = {
        let (sink, id, key) = (Arc::clone(sink), id.clone(), key.clone());
        // Typing belongs to the room: a thread's timeline doesn't watch it again.
        let room = key.thread.is_none().then(|| timeline.room().clone());
        tokio::spawn(async move {
            let (_watching, mut typing) = match &room {
                Some(room) => {
                    let (guard, receiver) = room.subscribe_to_typing_notifications();
                    (Some(guard), Some(receiver))
                }
                None => (None, None),
            };
            loop {
                tokio::select! {
                    biased;
                    batch = changes.next() => {
                        let Some(batch) = batch else { break };
                        if key.thread.is_none() {
                            for diff in &batch {
                                diff.clone().apply(&mut items);
                            }
                            let loaded = boundary_loaded(&items, &read_boundary);
                            if loaded != read_boundary_loaded {
                                read_boundary_loaded = loaded;
                                sink.emit(Event::ReadBoundary { account: id.clone(), room_id: key.room_id.clone(), event_id: read_boundary.clone(), loaded });
                            }
                        }
                        let diffs = batch.into_iter().map(|diff| crate::timeline::diff(diff, &me)).collect();
                        let diffs = match &mut bootstrap { Some(bootstrap) => bootstrap.update(diffs), None => diffs };
                        if let Some(bootstrap) = &bootstrap {
                            catchup.send_replace(bootstrap.progress());
                        }
                        sink.emit(Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs });
                    }
                    page = async { match &mut first_page { Some(page) => page.await, None => std::future::pending().await } } => {
                        first_page = None;
                        if let Ok(entries) = page {
                            if let Some(diff) = bootstrap.as_mut().and_then(|bootstrap| bootstrap.show(entries)) {
                                sink.emit(Event::TimelineChanged { account: id.clone(), key: key.clone(), diffs: vec![diff] });
                            }
                        } else if let Some(bootstrap) = &mut bootstrap {
                            bootstrap.cancel();
                        }
                        if let Some(bootstrap) = &bootstrap {
                            catchup.send_replace(bootstrap.progress());
                        }
                    }
                    users = next_typing(&mut typing) => {
                        let Some(room) = &room else { continue };
                        let people = typing_people(room, users).await;
                        sink.emit(Event::Typing { account: id.clone(), room_id: key.room_id.clone(), people });
                    }
                }
            }
        })
        .abort_handle()
    };

    // The first page of history loads beside the open timeline, not before it: what is cached
    // shows at once, and a slow server (a thread waits seconds for a pagination token) holds up
    // nothing. Either way ends in `HistoryLoaded` or `HistoryFailed`; a failure leaves the
    // timeline open with what it has, and the app offers to try again.
    if loaded < usize::from(crate::HISTORY_PAGE) {
        let needed = crate::HISTORY_PAGE.saturating_sub(u16::try_from(loaded).unwrap_or(u16::MAX));
        let sink = Arc::clone(sink);
        let progress = catching_up.clone();
        tokio::spawn(async move {
            if let Err(error) =
                load_history(timeline, progress, needed, Arc::clone(&sink), id.clone(), key.clone()).await
            {
                sink.emit(Event::HistoryFailed { account: id, key, error: error.to_string() });
            }
        });
    } else {
        sink.emit(Event::HistoryLoaded { account: id, key, reached_start: false });
    }
    TimelineWatch { forward, catchup: catching_up }
}

/// Both first loads and explicit retries fill any outstanding bootstrap coverage.
async fn load_history(
    timeline: Arc<matrix_sdk_ui::timeline::Timeline>,
    mut catching_up: tokio::sync::watch::Receiver<crate::timeline_bootstrap::Progress>,
    mut count: u16,
    sink: Arc<dyn EventSink>,
    account: AccountId,
    key: TimelineKey,
) -> Result<(), Error> {
    use crate::timeline_bootstrap::Progress;
    loop {
        let reached_start = timeline.paginate_backwards(count).await?;
        sink.emit(Event::HistoryLoaded { account: account.clone(), key: key.clone(), reached_start });
        // The early root is a display anchor, not proof of SDK pagination coverage.
        loop {
            if catching_up.has_changed().is_err() || reached_start {
                return Ok(());
            }
            let progress = *catching_up.borrow_and_update();
            match progress {
                Progress::Complete => return Ok(()),
                Progress::CatchingUp => break,
                Progress::Waiting => {
                    if catching_up.changed().await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
        count = crate::HISTORY_PAGE;
        tokio::task::yield_now().await;
    }
}

/// The next list of people typing; never ready without a subscription, or once it closed. A
/// lagged subscription skips to the newest list, which is all that matters.
async fn next_typing(
    receiver: &mut Option<tokio::sync::broadcast::Receiver<Vec<matrix_sdk::ruma::OwnedUserId>>>,
) -> Vec<matrix_sdk::ruma::OwnedUserId> {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        let Some(open) = receiver.as_mut() else { return std::future::pending().await };
        match open.recv().await {
            Ok(users) => return users,
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => *receiver = None,
        }
    }
}

/// The people behind typing user IDs, by their names and pictures in the room as the store
/// knows them; someone it doesn't know shows by their user ID.
async fn typing_people(room: &matrix_sdk::Room, users: Vec<matrix_sdk::ruma::OwnedUserId>) -> Vec<crate::Person> {
    let mut people = Vec::with_capacity(users.len());
    for user in users {
        let member = room.get_member_no_sync(&user).await.ok().flatten();
        people.push(crate::Person {
            id: user.to_string(),
            name: member.as_ref().map_or_else(|| user.to_string(), |member| member.name().to_owned()),
            avatar: member
                .and_then(|member| member.avatar_url().map(ToOwned::to_owned))
                .and_then(crate::convert::avatar),
        });
    }
    people
}

/// A text message's content: formatted when it has HTML.
fn text_content(
    body: &str,
    html: Option<&str>,
) -> matrix_sdk::ruma::events::room::message::RoomMessageEventContentWithoutRelation {
    use matrix_sdk::ruma::events::room::message::RoomMessageEventContentWithoutRelation as Content;
    match html {
        Some(html) => Content::text_html(body, html),
        None => Content::text_plain(body),
    }
}

/// Sends through an open timeline: as a reply when `reply_to` is set, into the thread when the
/// timeline is one.
async fn send_in(
    timeline: &matrix_sdk_ui::timeline::Timeline,
    reply_to: Option<String>,
    message: &Outgoing,
) -> Result<(), Error> {
    match message {
        Outgoing::Text { body, html } => {
            let content = text_content(body, html.as_deref());
            match reply_to {
                Some(event_id) => {
                    timeline.send_reply(content, matrix_sdk::ruma::EventId::parse(&event_id)?).await?;
                }
                None => {
                    let content: matrix_sdk::ruma::events::room::message::RoomMessageEventContent = content.into();
                    timeline.send(content.into()).await?;
                }
            }
        }
        Outgoing::Poll { question, answers, disclosed } => {
            timeline.send(crate::polls::start(question, answers, *disclosed)?).await?;
        }
        Outgoing::File(upload) | Outgoing::Voice { upload, .. } => {
            use matrix_sdk_ui::timeline::AttachmentSource;
            let source = match upload {
                crate::Upload::Path(path) => AttachmentSource::File(path.clone()),
                crate::Upload::Bytes { name, bytes } => {
                    AttachmentSource::Data { bytes: bytes.to_vec(), filename: name.clone() }
                }
            };
            let config = matrix_sdk_ui::timeline::AttachmentConfig {
                info: crate::account::voice_info(message),
                ..Default::default()
            };
            timeline.send_attachment(source, upload.mime(), config).await?;
        }
    }
    Ok(())
}

/// Reports a session the server no longer accepts. Refreshed tokens are saved by the client's
/// session callback inside the refresh's cross-process lock (`account::build_client`), and only
/// there: a later save from here could overwrite tokens another process refreshed since.
async fn watch_session(account: Account, sink: Arc<dyn EventSink>) {
    use tokio::sync::broadcast::error::RecvError;

    let mut changes = account.session_changes();
    loop {
        match changes.recv().await {
            Ok(SessionChange::UnknownToken(token)) => {
                sink.emit(Event::SessionEnded { account: account.id().clone(), kept: token.soft_logout });
            }
            Ok(SessionChange::TokensRefreshed) | Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    use super::*;
    use crate::secrets::MemorySecrets;

    fn ephemeral(kind: &str) -> matrix_sdk::ruma::serde::Raw<matrix_sdk::ruma::events::AnySyncEphemeralRoomEvent> {
        matrix_sdk::ruma::serde::Raw::new(&serde_json::json!({ "type": kind, "content": {} }))
            .expect("event")
            .cast_unchecked()
    }

    #[test]
    fn a_room_update_with_only_typing_notices_should_leave_its_summary_alone() {
        let update =
            matrix_sdk_base::sync::JoinedRoomUpdate { ephemeral: vec![ephemeral("m.typing")], ..Default::default() };
        assert!(!summary_may_change(&update));
    }

    #[test]
    fn a_room_update_with_a_read_receipt_should_read_its_summary_again() {
        let update = matrix_sdk_base::sync::JoinedRoomUpdate {
            ephemeral: vec![ephemeral("m.typing"), ephemeral("m.receipt")],
            ..Default::default()
        };
        assert!(summary_may_change(&update));
    }

    #[tokio::test]
    async fn an_unchanged_room_list_should_not_be_built_again() {
        let account = Account::for_test("https://example.org").await;
        let mut cache = crate::account::RoomCache::default();
        assert!(account.update_room_list(&mut cache, None).await.is_some(), "the first list is reported, even empty");
        assert!(account.update_room_list(&mut cache, None).await.is_none());
    }

    async fn next_thread_reply(received: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) -> bool {
        while let Some(event) = received.recv().await {
            if let Event::TimelineChanged { diffs, .. } = event {
                for diff in diffs {
                    let entries = match diff {
                        crate::TimelineDiff::Reset(entries) | crate::TimelineDiff::Append(entries) => entries,
                        crate::TimelineDiff::PushBack(entry)
                        | crate::TimelineDiff::PushFront(entry)
                        | crate::TimelineDiff::Insert { entry, .. }
                        | crate::TimelineDiff::Set { entry, .. } => vec![entry],
                        _ => Vec::new(),
                    };
                    if entries.iter().any(|entry| matches!(&entry.kind, crate::EntryKind::Message(message) if message.event_id == "$reply")) { return true; }
                }
            }
        }
        false
    }

    #[tokio::test]
    async fn a_cold_thread_should_show_replies_before_the_sdk_token_wait() {
        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO))
            .await
            .expect("sync room");
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(move |event| {
            let _ = events.send(event);
        });
        let mut baseline =
            open_timeline(account.clone(), TimelineKey::thread("!room:example.org", "$root"), None, false)
                .await
                .expect("cold SDK thread");
        baseline.first_page = None;
        let baseline = start_timeline(baseline, &sink);
        let still_waiting = tokio::time::timeout(Duration::from_millis(100), next_thread_reply(&mut received)).await;
        baseline.abort();
        assert!(still_waiting.is_err(), "the SDK alone should still be waiting for its pagination token");
        let started = std::time::Instant::now();
        let open = open_timeline(account, TimelineKey::thread("!room:example.org", "$root"), None, false)
            .await
            .expect("open thread");
        let forward = start_timeline(open, &sink);
        let found = tokio::time::timeout(Duration::from_millis(500), next_thread_reply(&mut received)).await;
        forward.abort();
        assert!(
            found == Ok(true) && started.elapsed() < Duration::from_millis(500),
            "cold thread elapsed: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn the_immediate_thread_page_should_keep_topological_order_with_skewed_clocks() {
        let server = crate::test_server::Server::new(|request| {
            if request.contains("/relations/") {
                let reply = |id: &str, time: u64| serde_json::json!({"type":"m.room.message", "room_id":"!room:example.org", "event_id":id, "sender":"@b:example.org", "origin_server_ts":time, "content":{"msgtype":"m.text", "body":id, "m.relates_to":{"rel_type":"m.thread", "event_id":"$root"}}});
                return serde_json::json!({"chunk":[reply("$second", 2), reply("$first", 30)]});
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        let entries =
            account.thread_first_page(&TimelineKey::thread("!room:example.org", "$root")).await.expect("first page");
        let ids: Vec<&str> = entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                crate::EntryKind::Message(message) => Some(message.event_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["$root", "$first", "$second"]);
    }

    async fn assert_sdk_catches_up(fail_first_page: bool) {
        use serde_json::json;
        let (requests, received_requests) = std::sync::mpsc::channel();
        let fail_once = std::sync::atomic::AtomicBool::new(fail_first_page);
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/relations/") {
                requests.send(request.to_owned()).expect("capture relations");
                let reply = |id: &str, time: u64| json!({"type":"m.room.message", "room_id":"!room:example.org", "event_id":id, "sender":"@b:example.org", "origin_server_ts":time, "content":{"msgtype":"m.text", "body":id, "m.relates_to":{"rel_type":"m.thread", "event_id":"$root"}}});
                if request.contains("from=older") {
                    let reaction = json!({"type":"m.reaction", "room_id":"!room:example.org", "event_id":"$reaction", "sender":"@b:example.org", "origin_server_ts":4, "content":{"m.relates_to":{"rel_type":"m.annotation", "event_id":"$old", "key":"like"}}});
                    return json!({"chunk":[reaction, reply("$old", 2)]});
                }
                if request.contains("recurse=true") {
                    if fail_once.swap(false, Ordering::Relaxed) {
                        return json!({"invalid":"simulated failed page"});
                    }
                    return json!({"chunk":[reply("$new", 3)], "next_batch":"older"});
                }
                return json!({"chunk":[reply("$new", 3), reply("$old", 2)]});
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        let open = open_timeline(account, TimelineKey::thread("!room:example.org", "$root"), None, false)
            .await
            .expect("open thread");
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(move |event| {
            let _ = events.send(event);
        });
        let (timeline, account, key) = (Arc::clone(&open.timeline), open.account.clone(), open.key.clone());
        let forward = start_timeline(open, &sink);
        let mut retried = false;
        let caught_up = tokio::time::timeout(Duration::from_secs(8), async {
            while let Some(event) = received.recv().await {
                if let Event::HistoryFailed { .. } = &event {
                    assert!(fail_first_page && !retried, "unexpected page failure: {event:?}");
                    retried = true;
                    load_history(Arc::clone(&timeline), forward.catchup.clone(), crate::HISTORY_PAGE, Arc::clone(&sink), account.clone(), key.clone()).await.expect("retry history");
                }
                if let Event::TimelineChanged { diffs, .. } = event {
                    for diff in diffs {
                        let entries = match diff {
                            TimelineDiff::Reset(entries) | TimelineDiff::Append(entries) => entries,
                            TimelineDiff::PushFront(entry) | TimelineDiff::PushBack(entry) | TimelineDiff::Insert { entry, .. } | TimelineDiff::Set { entry, .. } => vec![entry],
                            _ => Vec::new(),
                        };
                        if entries.iter().any(|entry| matches!(&entry.kind, crate::EntryKind::Message(message) if message.event_id == "$old" && !message.reactions.is_empty())) { return true; }
                    }
                }
            }
            false
        }).await;
        forward.abort();
        let requests: Vec<_> = received_requests.try_iter().collect();
        assert!(
            requests.iter().any(|request| request.contains("from=older")),
            "SDK must catch up beyond the first recursive page: {requests:?}"
        );
        assert_eq!(caught_up, Ok(true), "the early old reply must receive SDK reactions");
        assert_eq!(retried, fail_first_page);
    }

    #[tokio::test]
    async fn sdk_catchup_should_fetch_more_recursive_pages_even_when_the_early_page_has_the_root() {
        assert_sdk_catches_up(false).await;
    }

    #[tokio::test]
    async fn retrying_a_failed_sdk_page_should_resume_the_remaining_thread_catchup() {
        assert_sdk_catches_up(true).await;
    }

    #[tokio::test]
    async fn opening_a_room_should_report_a_read_boundary_outside_loaded_history() {
        let server = crate::test_server::Server::new(|request| {
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["account_data"] =
                    serde_json::json!({"events":[{"type":"m.fully_read", "content":{"event_id":"$unloaded"}}]});
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        let open =
            open_timeline(account, TimelineKey::room("!room:example.org"), None, false).await.expect("open room");
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(move |event| {
            let _ = events.send(event);
        });
        let forward = start_timeline(open, &sink);
        assert!(
            matches!(received.recv().await, Some(Event::ReadBoundary { event_id: Some(id), loaded: false, .. }) if id == "$unloaded")
        );
        forward.abort();
    }

    #[tokio::test]
    async fn member_state_changes_should_invalidate_snapshots_without_a_count_change() {
        let server = crate::test_server::Server::new(crate::test_server::thread_response);
        let account = Account::for_test(&server.url()).await;
        let mut response =
            account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        let update = response.rooms.joined.values_mut().next().expect("joined room");
        assert!(members_changed(update));
        update.state = matrix_sdk_base::sync::State::Before(Vec::new());
        assert!(!members_changed(update), "ordinary messages must not invalidate the member list");
    }

    #[tokio::test]
    async fn marking_a_room_fully_read_should_send_both_cross_device_and_public_markers() {
        let (requests, received) = std::sync::mpsc::channel();
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/read_markers") {
                requests.send(request.to_owned()).expect("capture read markers");
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account
            .client()
            .sync_once(matrix_sdk::config::SyncSettings::default().timeout(Duration::ZERO))
            .await
            .expect("sync room");
        let timeline = account.timeline(&TimelineKey::room("!room:example.org")).await.expect("timeline");
        mark_timeline_read(&timeline, None, true).await.expect("mark fully read");
        let request = received.recv_timeout(Duration::from_secs(1)).expect("read markers request");
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").expect("request body").1).expect("read markers JSON");
        assert_eq!(body, serde_json::json!({"m.fully_read":"$root","m.read":"$root"}));
    }

    #[tokio::test]
    async fn viewport_receipts_should_not_advance_to_a_newer_unseen_message() {
        let (requests, received) = std::sync::mpsc::channel();
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/read_markers") {
                requests.send(request.to_owned()).expect("capture markers");
            }
            let mut response = crate::test_server::thread_response(request);
            if request.contains("/sync?") {
                response["rooms"]["join"]["!room:example.org"]["timeline"]["events"].as_array_mut().expect("events").push(
                    serde_json::json!({"type":"m.room.message", "event_id":"$unseen", "sender":"@b:example.org", "origin_server_ts":3, "content":{"msgtype":"m.text", "body":"Below the viewport"}})
                );
            }
            response
        });
        let account = Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        let timeline = account.timeline(&TimelineKey::room("!room:example.org")).await.expect("timeline");
        assert_eq!(timeline.latest_event_id().await.as_deref().map(|id| id.as_str()), Some("$unseen"));
        mark_timeline_read(&timeline, Some("$root"), true).await.expect("mark viewport read");
        let request = received.recv_timeout(Duration::from_secs(1)).expect("markers request");
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").expect("HTTP body").1).expect("JSON body");
        assert_eq!(body, serde_json::json!({"m.fully_read":"$root","m.read":"$root"}));
    }

    fn start() -> (EngineHandle, std_mpsc::Receiver<Event>) {
        let (events, received) = std_mpsc::channel();
        let sink = move |event| {
            let _ = events.send(event);
        };
        let data_dir = std::env::temp_dir().join(format!("matrix-core-engine-{}", std::process::id()));
        let config = EngineConfig {
            data_dir,
            secrets: Arc::new(MemorySecrets::default()),
            identity: ClientIdentity { name: "Test".into(), uri: "https://example.org".parse().unwrap() },
            oauth_redirect: crate::OAuthRedirect::Loopback,
            stores: StoreSharing::Alone,
        };
        (Engine::start(config, sink).expect("engine starts"), received)
    }

    fn finished(events: &std_mpsc::Receiver<Event>, request: RequestId) -> Result<(), String> {
        loop {
            match events.recv_timeout(Duration::from_secs(30)).expect("an event") {
                Event::Finished { request: r, result } if r == request => return result,
                _ => {}
            }
        }
    }

    #[test]
    fn a_password_request_should_tell_the_app_before_failing() {
        let (events, received) = std_mpsc::channel();
        let sink = move |event| {
            let _ = events.send(event);
        };
        let error = asked_for_password(Error::PasswordRequired, RequestId(7), &sink);
        assert!(matches!(error, Error::PasswordRequired));
        assert!(matches!(received.try_recv(), Ok(Event::PasswordNeeded { request: RequestId(7) })));
    }

    #[test]
    fn other_failures_should_not_ask_for_the_password() {
        let (events, received) = std_mpsc::channel();
        let sink = move |event| {
            let _ = events.send(event);
        };
        let _ = asked_for_password(Error::WrongCode, RequestId(7), &sink);
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn a_command_for_an_unknown_account_should_finish_with_an_error() {
        let (engine, events) = start();
        let request = engine.send(Command::ListSessions(AccountId::new("@nobody:example.org")));
        assert_eq!(finished(&events, request), Err("no signed-in account @nobody:example.org".into()));
    }

    #[test]
    fn a_login_to_an_unreachable_homeserver_should_finish_with_an_error() {
        let (engine, events) = start();
        let request = engine.send(Command::Login(LoginRequest {
            homeserver: "http://127.0.0.1:9".into(),
            username: "alice".into(),
            password: "secret".into(),
            resume: None,
        }));
        assert!(finished(&events, request).is_err());
    }

    #[test]
    fn a_room_preview_for_an_unknown_account_should_finish_with_an_error() {
        let (engine, events) = start();
        let request = engine.send(Command::PreviewRoom {
            account: AccountId::new("@nobody:example.org"),
            room: "#rust:matrix.org".into(),
            via: vec!["matrix.org".into()],
        });
        assert_eq!(finished(&events, request), Err("no signed-in account @nobody:example.org".into()));
    }

    #[test]
    fn a_join_for_an_unknown_account_should_finish_with_an_error() {
        let (engine, events) = start();
        let request = engine.send(Command::JoinRoom {
            account: AccountId::new("@nobody:example.org"),
            room_id: "!abc:example.org".into(),
            via: Vec::new(),
        });
        assert_eq!(finished(&events, request), Err("no signed-in account @nobody:example.org".into()));
    }

    #[test]
    fn a_cancelled_command_should_finish_as_cancelled() {
        let (engine, events) = start();
        // A routable but unanswering address keeps discovery waiting until it is cancelled.
        let request = engine.send(Command::Discover { server: "http://10.255.255.1".into() });
        engine.send(Command::Cancel(request));
        assert_eq!(finished(&events, request), Err("cancelled".into()));
    }

    #[test]
    fn a_redirect_for_no_pending_sign_in_should_finish_as_cancelled() {
        let (engine, events) = start();
        let request =
            engine.send(Command::FinishOAuth { request: RequestId(999), url: "com.example.app:/oauth?code=x".into() });
        assert_eq!(finished(&events, request), Err("cancelled".into()));
    }

    /// Talks to matrix.org; run with `cargo test -p matrix-core -- --ignored`.
    #[test]
    #[ignore = "needs the network"]
    fn matrix_org_should_offer_browser_sign_in() {
        let (engine, events) = start();
        let request = engine.send(Command::Discover { server: "matrix.org".into() });
        let homeserver = loop {
            match events.recv_timeout(Duration::from_secs(30)).expect("an event") {
                Event::HomeserverDiscovered { request: r, homeserver } if r == request => break homeserver,
                Event::Finished { request: r, result: Err(error) } if r == request => panic!("{error}"),
                _ => {}
            }
        };
        assert!(homeserver.methods.oauth && homeserver.url.contains("matrix-client"), "{homeserver:?}");
    }

    #[test]
    fn each_command_should_get_its_own_request_id() {
        let (engine, mut commands) = EngineHandle::detached();
        let first = engine.send(Command::Logout(AccountId::new("@a:x")));
        let second = engine.send(Command::Logout(AccountId::new("@a:x")));
        assert_ne!(first, second);
        assert!(commands.try_next().is_some());
    }

    #[test]
    fn a_login_request_should_not_reveal_its_password_when_debug_printed() {
        let request =
            LoginRequest { homeserver: "x".into(), username: "y".into(), password: "hunter2".into(), resume: None };
        assert!(!format!("{request:?}").contains("hunter2"));
    }
}

#[cfg(test)]
#[path = "read_receipts_tests.rs"]
mod read_receipts_tests;
