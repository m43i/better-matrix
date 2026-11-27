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
    /// A request for paginated data or server-confirmed actions.
    #[cfg(feature = "command-line")]
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    Terminal {
        /// The account answering the request.
        account: AccountId,
        /// The operation to perform.
        action: Box<crate::terminal::Action>,
    },
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
        /// Create a new account on the server's page first (when
        /// [`LoginMethods::create_account`](crate::LoginMethods::create_account)).
        create: bool,
    },
    /// Sign in as an explicitly selected account using the browser.
    #[cfg(feature = "command-line")]
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    LoginOAuthAs {
        /// The resolved homeserver URL.
        homeserver: String,
        /// The only Matrix user ID whose session this login may save.
        account: AccountId,
        /// Resume the suspended device and its stores.
        resume: bool,
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
    /// Create an account on a password homeserver and sign it in. The server may ask first:
    /// [`Event::Registration`] says what, [`Command::Answer`] replies. An OAuth homeserver
    /// creates accounts in the browser instead ([`Command::LoginOAuth`] with `create`).
    Register(crate::RegisterRequest),
    /// Sign another account in with a QR code exchanged with a device where it is signed in
    /// (MSC4108). [`Event::QrLogin`] reports each step; the check code goes back with
    /// [`Command::Answer`]. The new session starts verified.
    SignInWithQr(crate::QrSignIn),
    /// Approve a new device's sign-in with a QR code from this verified session: show a code
    /// for it to scan (`scanned` is `None`), or take the code it shows. [`Event::QrLogin`]
    /// reports each step, [`Event::OpenUrl`] opens the account page to approve it on, and the
    /// check code goes back with [`Command::Answer`].
    LinkDevice {
        /// The signed-in account.
        account: AccountId,
        /// The new device's code, as scanned.
        scanned: Option<Vec<u8>>,
    },
    /// Answer a question a running command asked (a check code, a registration step).
    /// Answers to finished or unknown commands are dropped.
    Answer {
        /// The command that asked.
        request: RequestId,
        /// The answer.
        answer: Answer,
    },
    /// Read the QR code in a picture (a camera frame); answered by [`Event::QrCodeRead`].
    ReadQrCode {
        /// The picture, a PNG or JPEG file.
        picture: PathBuf,
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
        /// Who the new text mentions on purpose.
        #[cfg_attr(feature = "ffi", serde(default))]
        mentions: crate::Mentions,
    },
    /// Every version of an edited message, oldest first; answered by [`Event::EditHistory`].
    ListEdits {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The message's event ID.
        event_id: String,
    },
    /// Find which timeline shows a message: the room's, or the thread it is in; answered by
    /// [`Event::MessageLocated`], for opening a link to it.
    LocateMessage {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The message's event ID.
        event_id: String,
    },
    /// Ask the server to send a message later (MSC4140), encrypted where the room is; an
    /// updated [`Event::ScheduledMessages`] follows. Uploads can't be scheduled.
    Schedule {
        /// The sending account.
        account: AccountId,
        /// Where it goes.
        key: TimelineKey,
        /// The event ID of the message it answers, for a reply.
        reply_to: Option<String>,
        /// What to send.
        message: Outgoing,
        /// How long to wait, in milliseconds (at most 30 days; a server may allow less).
        delay_ms: u64,
    },
    /// List the account's scheduled messages; answered by [`Event::ScheduledMessages`].
    ListScheduled {
        /// Whose.
        account: AccountId,
    },
    /// Cancel a scheduled message or send it now; an updated [`Event::ScheduledMessages`]
    /// follows.
    UpdateScheduled {
        /// Whose.
        account: AccountId,
        /// Which ([`ScheduledMessage::delay_id`](crate::ScheduledMessage::delay_id)).
        delay_id: String,
        /// What to do.
        action: crate::ScheduledAction,
    },
    /// The custom emoji and sticker packs usable in a room (or anywhere); answered by
    /// [`Event::ImagePacks`].
    ListImagePacks {
        /// Whose.
        account: AccountId,
        /// The room the packs are for, adding its own packs to the user's.
        room_id: Option<String>,
    },
    /// Change an image pack: the user's own, or a room's (with the right to); answered by
    /// [`Event::ImagePackEdited`]. Lists of packs asked for after it show the change.
    EditImagePack {
        /// Whose.
        account: AccountId,
        /// Which pack.
        pack: crate::PackPlace,
        /// What to change.
        edit: crate::PackEdit,
    },
    /// Find GIFs on GIPHY, or those trending for a blank query; answered by
    /// [`Event::GifsFound`]. Fails with [`Error::NoGifKey`](crate::Error::NoGifKey) without a key.
    SearchGifs {
        /// The user's GIPHY API key.
        key: String,
        /// What to look for; blank for what's trending.
        query: String,
        /// Where the page starts ([`GifPage::next`](crate::GifPage::next) of the one before).
        offset: u32,
        /// The app's language (a BCP 47 tag), to read the query in.
        language: Option<String>,
    },
    /// Find a location for the composer. Desktop uses an existing device grant first;
    /// phones supply their own device fix and use this command only for the IP estimate.
    Locate {
        /// Explicitly request access to the desktop device position, without an IP fallback.
        request_access: bool,
    },
    /// Share a fresh device fix in the room for 15 minutes. This request stays open for
    /// further fixes and an early stop. Beacon state events always belong to the room.
    ShareLiveLocation {
        /// Sending account.
        account: AccountId,
        /// Destination room.
        room_id: String,
        /// The first device fix.
        geo_uri: String,
    },
    /// Find places by name on OpenStreetMap, for sharing a location; answered by
    /// [`Event::PlacesFound`]. Send it when the user asks, never as they type.
    SearchPlaces {
        /// What to look for.
        query: String,
        /// The app's language (a BCP 47 tag), for the places' names.
        language: Option<String>,
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
    /// Save a message for the user alone, or stop saving it (MSC2437 tagged events in the
    /// room's account data). Updated room summaries (`RoomSummary::saved`) follow through sync.
    SetSaved {
        /// The account saving the message.
        account: AccountId,
        /// The room containing the message.
        room_id: String,
        /// The message's server event ID.
        event_id: String,
        /// Save when true, stop saving when false.
        saved: bool,
    },
    /// Fetch every message the account saved, in all its joined rooms, most recently saved
    /// first. Answered by [`Event::SavedMessages`]; unreadable events keep a placeholder.
    ListSavedMessages {
        /// Whose saved messages.
        account: AccountId,
    },
    /// Keep what the user is writing in a room or thread on this device, replacing what was
    /// kept there; `None` (or blank text) forgets it.
    SaveDraft {
        /// The account writing.
        account: AccountId,
        /// The room or thread written in.
        key: TimelineKey,
        /// The text and its formatting.
        draft: Option<crate::Draft>,
    },
    /// Read the draft kept for a room or thread. Answered by [`Event::DraftLoaded`].
    LoadDraft {
        /// The account writing.
        account: AccountId,
        /// The room or thread written in.
        key: TimelineKey,
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
    /// Add a word that notifies the user wherever it is written, or remove one; an
    /// [`Event::Keywords`] follows.
    SetKeyword {
        /// Which account.
        account: AccountId,
        /// The word.
        keyword: String,
        /// Add when true, remove when false.
        enabled: bool,
    },
    /// Put a room among the favourites or the low priority rooms, or neither (room tags); an
    /// updated [`Event::RoomsUpdated`] follows.
    SetRoomPriority {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// Where it goes.
        priority: crate::RoomPriority,
    },
    /// Mark a room unread (MSC2867's flag), or take the mark away; nothing is read. An updated
    /// [`Event::RoomsUpdated`] follows.
    SetUnread {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
        /// Mark unread when true, take the mark away when false.
        unread: bool,
    },
    /// Read a room without opening it: a read receipt for its latest message when it has unread
    /// ones, and its unread mark taken away. Fails when it has unread messages but none to mark
    /// read yet. An updated [`Event::RoomsUpdated`] follows.
    MarkRoomRead {
        /// Whose room.
        account: AccountId,
        /// The room's ID.
        room_id: String,
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
        /// Optional time this device should clear the message, in Unix milliseconds.
        #[cfg_attr(feature = "ffi", serde(default), ts(optional = nullable))]
        clear_at_ms: Option<u64>,
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
    /// An event's JSON, for developer tools; answered by [`Event::EventSource`].
    ViewSource {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The event.
        event_id: String,
    },
    /// List a room's widgets; answered by [`Event::Widgets`].
    ListWidgets {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
    },
    /// Pin a web page to a room as a widget. The room's widgets are listed again after.
    AddWidget {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// What to call it; the page's host when blank.
        name: String,
        /// The page, `http` or `https`. It may hold template variables (`$matrix_user_id`, …).
        url: String,
    },
    /// Take a widget off a room. The room's widgets are listed again after.
    RemoveWidget {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The widget (`Widget::id`).
        widget_id: String,
    },
    /// Run one of a room's widgets ([`crate::widgets::session`]): in the browser (`browser`),
    /// answered by [`Event::OpenUrl`]; else in the app's web view, which shows
    /// [`Event::WidgetPage`] and passes its messages ([`Event::WidgetMessage`],
    /// [`Answer::WidgetMessage`]), until the app closes it with [`Command::Cancel`]. A widget
    /// asking for permissions waits for [`Event::WidgetPermissions`]'s answer
    /// ([`Answer::WidgetPermissions`]). A meeting opens as itself, joined as its server asks.
    RunWidget {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The widget (`Widget::id`).
        widget_id: String,
        /// Whether to open it in the browser rather than the app.
        browser: bool,
    },
    /// Start a Jitsi meeting in a room: pin it as Element does (the room's widgets are listed
    /// again), then run it as [`Command::RunWidget`] does.
    StartMeeting {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// Whether it starts with video, rather than audio only.
        video: bool,
        /// Whether to open it in the browser rather than the app.
        browser: bool,
    },
    /// Open the integration manager for a room ([`crate::widgets::integrations`]): bots,
    /// bridges and widgets. Runs like [`Command::RunWidget`] in the app's web view; its terms,
    /// when not yet agreed to, come first ([`Event::IntegrationTerms`], [`Answer::Agree`]). It
    /// ends when the manager closes itself or the app cancels it.
    OpenIntegrations {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
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
    /// Change what the user keeps away (ignored people, media, invitations) on every session;
    /// an [`Event::Safety`] with the choices after it follows.
    SetSafety {
        /// Whose.
        account: AccountId,
        /// The change.
        change: crate::SafetyChange,
    },
    /// Report a message, a room or a person to the homeserver's admins.
    Report {
        /// Who reports.
        account: AccountId,
        /// What is reported.
        target: crate::ReportTarget,
        /// Why, in the user's words; may be empty.
        reason: String,
    },
    /// Kick, ban, unban or change the power level of a room member, or answer someone asking
    /// to join; [`Event::Members`] follows.
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
    /// Open the user's note to self (a room only they are in, for their own notes), making one
    /// named `name` when there is none; answered by [`Event::DirectChat`] once the room list
    /// shows it.
    OpenNoteToSelf {
        /// Whose.
        account: AccountId,
        /// A new one's name, in the user's language: what other clients show.
        name: String,
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
    /// Make a room or space as `room` describes; answered by [`Event::RoomCreated`].
    CreateRoom {
        /// Whose.
        account: AccountId,
        /// What to make.
        room: crate::NewRoom,
    },
    /// Read a room's or space's settings; answered by [`Event::RoomSettings`].
    LoadRoomSettings {
        /// Whose.
        account: AccountId,
        /// The room or space.
        room_id: String,
    },
    /// Change one of a room's or space's settings (its name, say). Once sync brings the change,
    /// [`Event::RoomSettings`] and an updated [`Event::RoomsUpdated`] follow.
    ChangeRoom {
        /// Whose.
        account: AccountId,
        /// The room or space.
        room_id: String,
        /// What to change.
        setting: crate::RoomSetting,
    },
    /// Upgrade a room to a newer room version: the server makes a new room and closes the old
    /// one, pointing to it. Spaces the user may arrange list the new room instead. Answered by
    /// [`Event::RoomCreated`] with the new room.
    UpgradeRoom {
        /// Whose.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The version, from [`RoomSettings::upgrade_to`](crate::RoomSettings::upgrade_to).
        version: String,
    },
    /// Add, remove or order a space's rooms; an updated [`Event::RoomsUpdated`] follows.
    ChangeSpace {
        /// Whose.
        account: AccountId,
        /// The space.
        space: String,
        /// What to change.
        change: crate::SpaceChange,
    },
    /// Leave a space and, first, those of its rooms named; an updated [`Event::RoomsUpdated`]
    /// follows without them.
    LeaveSpace {
        /// Whose.
        account: AccountId,
        /// The space.
        space: String,
        /// Its rooms (and subspaces) to leave too.
        rooms: Vec<String>,
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
    /// Ask another person to verify each other, in the direct chat both have joined (else it
    /// fails with [`Error::NoDirectChat`](crate::Error::NoDirectChat)); [`Event::Verification`]
    /// follows, then [`Event::Identity`] once it ends.
    VerifyUser {
        /// Which account.
        account: AccountId,
        /// Whom.
        user_id: String,
    },
    /// Compare emoji instead of scanning a code, in a verification both sides are ready for.
    CompareEmoji {
        /// Which account.
        account: AccountId,
        /// The verification.
        flow_id: String,
    },
    /// The code the other side of a verification shows, as scanned.
    ScanVerificationCode {
        /// Which account.
        account: AccountId,
        /// The verification.
        flow_id: String,
        /// The code's raw bytes.
        code: Vec<u8>,
    },
    /// Find out whether the user verified someone; answered by [`Event::Identity`].
    LoadIdentity {
        /// Which account.
        account: AccountId,
        /// Whom.
        user_id: String,
    },
    /// Write every message key this session holds to a file, encrypted with a passphrase.
    ExportRoomKeys {
        /// Whose keys.
        account: AccountId,
        /// The file to write.
        path: PathBuf,
        /// What the file is encrypted with.
        passphrase: Password,
    },
    /// Read message keys from a file another session or client exported; answered by
    /// [`Event::KeysImported`]. Messages they unlock decrypt by themselves.
    ImportRoomKeys {
        /// Which account.
        account: AccountId,
        /// The file.
        path: PathBuf,
        /// What it was encrypted with.
        passphrase: Password,
    },
    /// Turn the offline device on (with the recovery key) or off on this session; an
    /// [`Event::SecurityChanged`] follows.
    SetOfflineDevice {
        /// Which account.
        account: AccountId,
        /// On or off.
        on: bool,
        /// The recovery key, to turn it on.
        recovery_key: Option<Password>,
    },
    /// Change a password account's password, which signs its other sessions out. The server
    /// asks for the current one: [`Event::PasswordNeeded`] (send again with it). An OAuth
    /// account changes it on its account page ([`Command::OpenAccountPage`]).
    ChangePassword {
        /// Which account.
        account: AccountId,
        /// The new password.
        new_password: Password,
        /// The current password, when the server asked for it.
        password: Option<Password>,
    },
    /// Deactivate an account for good, then forget it here ([`Event::AccountRemoved`]). A
    /// password account may answer with [`Event::PasswordNeeded`] (send again with it); an OAuth
    /// account answers with [`Event::OpenUrl`] for its account page, where it is deactivated
    /// (its session then ends).
    DeactivateAccount {
        /// Which account.
        account: AccountId,
        /// Also ask the server to forget the messages it sent, as far as it can.
        erase: bool,
        /// The account password, when the server asked for it.
        password: Option<Password>,
    },
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

/// The app's answer to a question a running command asked ([`Command::Answer`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, serde::Deserialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Answer {
    /// A new device fix from the phone for its running live location share.
    LocationFix(String),
    /// The two digits the other device shows ([`QrLoginStep::EnterCheckCode`](crate::QrLoginStep::EnterCheckCode)).
    CheckCode(u8),
    /// The registration token ([`RegistrationStep::Token`](crate::RegistrationStep::Token)).
    RegistrationToken(String),
    /// The user agrees to the policies ([`RegistrationStep::Terms`](crate::RegistrationStep::Terms)).
    Agree,
    /// The email address to confirm ([`RegistrationStep::Email`](crate::RegistrationStep::Email)).
    Email(String),
    /// A message a running widget's page posted ([`Command::RunWidget`]), as JSON.
    WidgetMessage(String),
    /// The permissions the user grants a running widget, by capability
    /// ([`Event::WidgetPermissions`]); those left out are refused.
    WidgetPermissions {
        /// The question answered ([`Event::WidgetPermissions::question`]); an answer to another
        /// is dropped.
        question: u64,
        /// The capabilities granted.
        granted: Vec<String>,
    },
    /// A running widget's page started loading a new document: what it posts waits for that
    /// document's widget API ([`Answer::WidgetLoaded`]).
    WidgetLoading,
    /// A running widget's page finished loading: the widget API starts talking to it.
    WidgetLoaded,
    /// The user finished the step in the browser ([`RegistrationStep::Browser`](crate::RegistrationStep::Browser)),
    /// or opened the emailed link ([`RegistrationStep::EmailSent`](crate::RegistrationStep::EmailSent)).
    Continue,
}

/// Exit categories preserved across the engine boundary for one-shot clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Sign in or supply the account password.
    Authentication,
    /// The account lacks permission.
    Permission,
    /// The homeserver is limiting requests.
    RateLimit,
    /// The operation ran out of time.
    Timeout,
    /// Any other failure.
    Other,
}

/// What the engine reports. Delivered on the engine's threads; a sink forwards them to its UI.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(serde::Serialize, ts_rs::TS))]
#[cfg_attr(feature = "ffi", serde(tag = "type", content = "data"))]
pub enum Event {
    /// Structured error category for a terminal command, before its completion.
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    CommandError(RequestId, ErrorKind),
    /// Result of a terminal operation, or a message in a running watch.
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    TerminalResult(RequestId, serde_json::Value),
    /// The first successful sync with full room state has been applied to the room list.
    #[cfg_attr(feature = "ffi", serde(skip), ts(skip))]
    SyncReady(AccountId),
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
    /// Which account a sign-in command (`Login`, `LoginOAuth`, `Register`, `SignInWithQr`)
    /// signed in, right after its [`Event::AccountAdded`] and before its [`Event::Finished`].
    SignedIn {
        /// The command it answers.
        request: RequestId,
        /// The account.
        account: AccountId,
    },
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
    /// At start: the accounts being restored. Each then arrives as
    /// [`Event::AccountAdded`], [`Event::AccountError`], or (for suspended terminal sessions)
    /// [`Event::SessionEnded`].
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
    /// An event's source, for [`Command::ViewSource`].
    EventSource {
        /// The request it answers.
        request: RequestId,
        /// The event's JSON.
        source: crate::EventSource,
    },
    /// A room's widgets, for [`Command::ListWidgets`] and after adding or removing one.
    Widgets {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// Its widgets.
        widgets: crate::Widgets,
    },
    /// Show a running widget's page ([`Command::RunWidget`], [`Command::StartMeeting`],
    /// [`Command::OpenIntegrations`]). Again for the same run: load it again (the widget starts
    /// over, as after a permission answered too late for the page's first question).
    WidgetPage {
        /// The run.
        request: RequestId,
        /// The page.
        page: crate::WidgetPage,
    },
    /// Post a message into a running widget's page, as its bridge says.
    WidgetMessage {
        /// The run.
        request: RequestId,
        /// The message, as JSON.
        message: String,
    },
    /// A running widget asks for permissions; the user answers with
    /// [`Answer::WidgetPermissions`]. Until then it waits. No permissions withdraws the question
    /// showing (the page started over): the app stops asking.
    WidgetPermissions {
        /// The run.
        request: RequestId,
        /// The question's number within the run, which the answer names.
        question: u64,
        /// The widget's name.
        widget: String,
        /// What it asks for.
        permissions: Vec<crate::WidgetPermission>,
    },
    /// The integration manager's terms, to agree to ([`Answer::Agree`]) before it opens.
    IntegrationTerms {
        /// The run.
        request: RequestId,
        /// Its policies.
        policies: Vec<crate::Policy>,
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
    /// The direct chat for [`Command::OpenDirectChat`], or the note to self for
    /// [`Command::OpenNoteToSelf`].
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
    /// A room or space made by [`Command::CreateRoom`], or the new room of
    /// [`Command::UpgradeRoom`].
    RoomCreated {
        /// The command it answers.
        request: RequestId,
        /// Its ID.
        room_id: String,
    },
    /// A room's or space's settings, for [`Command::LoadRoomSettings`] and after
    /// [`Command::ChangeRoom`].
    RoomSettings {
        /// Whose.
        account: AccountId,
        /// The settings.
        settings: Box<crate::RoomSettings>,
    },
    /// How the user arranged the sidebar, as saved: sent once an account is ready.
    SidebarLayout {
        /// Whose.
        account: AccountId,
        /// The arrangement.
        layout: crate::SidebarLayout,
    },
    /// What the user keeps away on an account (ignored people, media, invitations): sent once
    /// an account is ready, when a sync changes it (here or on another session), and after
    /// [`Command::SetSafety`].
    Safety {
        /// Whose.
        account: AccountId,
        /// The choices.
        safety: crate::Safety,
    },
    /// The account's notification keywords, sorted: sent once an account is ready and whenever
    /// they change, here or on another session.
    Keywords {
        /// Whose.
        account: AccountId,
        /// The words.
        keywords: Vec<String>,
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
    /// The expiry kept for this account on this device, reported at start and after changes.
    StatusMessageExpiry {
        /// Whose.
        account: AccountId,
        /// The scheduled clear, if one is set.
        expiry: Option<crate::status::StatusMessageExpiry>,
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
    /// Where signing in with a QR code stands, for [`Command::SignInWithQr`] or
    /// [`Command::LinkDevice`].
    QrLogin {
        /// The command it reports on.
        request: RequestId,
        /// Where it stands.
        step: crate::QrLoginStep,
    },
    /// What the server asks before creating an account, for [`Command::Register`]: answer with
    /// [`Command::Answer`].
    Registration {
        /// The command it asks for.
        request: RequestId,
        /// What it asks.
        step: crate::RegistrationStep,
    },
    /// The QR code in a picture, for [`Command::ReadQrCode`]; `None` when none reads.
    QrCodeRead {
        /// The command it answers.
        request: RequestId,
        /// The code's raw bytes.
        code: Option<Vec<u8>>,
    },
    /// Whether the user verified someone: for [`Command::LoadIdentity`], and once a
    /// verification with them ends. `None` when they have no identity (no encryption) yet.
    Identity {
        /// Which account.
        account: AccountId,
        /// Whom.
        user_id: String,
        /// What the user knows of their identity.
        identity: Option<crate::Identity>,
    },
    /// Message keys read from a file, for [`Command::ImportRoomKeys`].
    KeysImported {
        /// The command it answers.
        request: RequestId,
        /// How many were new.
        import: crate::KeyImport,
    },
    /// The verification [`Command::VerifyThisSession`] or [`Command::VerifyUser`] started: its
    /// [`Event::Verification`]s carry this `flow_id`, and come after this.
    VerificationRequested {
        /// The command it answers.
        request: RequestId,
        /// Which account.
        account: AccountId,
        /// The new verification.
        flow_id: String,
    },
    /// A verification moved on: between this session and another of the user's, or with
    /// another person.
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
        messages: Vec<crate::MessagePreview>,
    },
    /// Every message the account saved, for [`Command::ListSavedMessages`].
    SavedMessages {
        /// Whose saved messages.
        account: AccountId,
        /// Most recently saved first, including unavailable placeholders.
        messages: Vec<crate::SavedMessage>,
    },
    /// The draft kept for a room or thread, for [`Command::LoadDraft`]; `None` when there is
    /// none.
    DraftLoaded {
        /// The read it answers.
        request: RequestId,
        /// The account writing.
        account: AccountId,
        /// The room or thread written in.
        key: TimelineKey,
        /// The text and its formatting.
        draft: Option<crate::Draft>,
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
    /// The versions of an edited message, for [`Command::ListEdits`].
    EditHistory {
        /// Whose room.
        account: AccountId,
        /// The room.
        room_id: String,
        /// The message.
        event_id: String,
        /// Its versions, the original first.
        versions: Vec<crate::EditVersion>,
    },
    /// Where a message shows, for [`Command::LocateMessage`].
    MessageLocated {
        /// The command it answers.
        request: RequestId,
        /// Whose room.
        account: AccountId,
        /// The room's timeline, or the message's thread.
        key: TimelineKey,
        /// The message: the one an edit edits, when asked for an edit.
        event_id: String,
    },
    /// The account's scheduled messages, soonest first.
    ScheduledMessages {
        /// Whose.
        account: AccountId,
        /// What the server holds.
        messages: Vec<crate::ScheduledMessage>,
    },
    /// Custom emoji and sticker packs, for [`Command::ListImagePacks`].
    ImagePacks {
        /// Whose.
        account: AccountId,
        /// The room they were listed for.
        room_id: Option<String>,
        /// The packs: the user's own first (always, empty or not), then the room's, then those
        /// the user uses everywhere.
        packs: Vec<crate::ImagePack>,
        /// Whether the account may add packs to the room.
        #[cfg_attr(feature = "ffi", serde(default))]
        may_add: bool,
    },
    /// An image pack changed for [`Command::EditImagePack`]: lists of packs asked for from now
    /// on show it.
    ImagePackEdited {
        /// The change.
        request: RequestId,
        /// Whose.
        account: AccountId,
        /// The pack changed: a new room pack's place, for [`PackEdit::Create`](crate::PackEdit::Create).
        pack: crate::PackPlace,
    },
    /// GIFs found for [`Command::SearchGifs`].
    GifsFound {
        /// The search.
        request: RequestId,
        /// What was searched for, as asked.
        query: String,
        /// Where the page starts, as asked.
        offset: u32,
        /// The GIFs.
        page: crate::GifPage,
    },
    /// Initial position for a location composer.
    PositionFound {
        /// The locate request.
        request: RequestId,
        /// A validated geo URI.
        geo_uri: String,
        /// Whether the position was estimated from the public IP.
        estimated: bool,
    },
    /// The first fix has reached the room. Phones can now deliver subsequent fixes.
    LiveLocationStarted {
        /// The running share.
        request: RequestId,
        /// The beacon state event that owns subsequent fixes.
        event_id: String,
    },
    /// Places found for [`Command::SearchPlaces`].
    PlacesFound {
        /// The search it answers.
        request: RequestId,
        /// Best first.
        places: Vec<crate::Place>,
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
    /// Whether the app can scan QR codes (it has a camera to, and sends what it scans): it then
    /// offers to scan the other side's code in verifications.
    pub scans_qr: bool,
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
            .field("scans_qr", &self.scans_qr)
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
    LoggedIn(Box<Account>, Box<AccountSummary>),
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
        Self::start_with_sync(config, sink, true, false)
    }

    /// Starts the engine with ordinary sync, which can report a complete first room list.
    #[cfg(feature = "command-line")]
    pub fn start_terminal(config: EngineConfig, sink: impl EventSink, watch: bool) -> Result<EngineHandle, Error> {
        Self::start_with_sync(config, sink, false, watch)
    }

    fn start_with_sync(
        config: EngineConfig,
        sink: impl EventSink,
        sliding_sync: bool,
        pause_for_watch: bool,
    ) -> Result<EngineHandle, Error> {
        crate::tls::install_provider();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(WORKERS.min(std::thread::available_parallelism().map_or(WORKERS, usize::from)))
            .enable_all()
            .thread_name("matrix-engine")
            .build()?;
        let (commands, receiver) = mpsc::unbounded_channel();
        let (_signals, signal_receiver) = mpsc::unbounded_channel();
        let state = State {
            layout: StoreLayout::new(&config.data_dir),
            sessions: Sessions::new(&config.data_dir, config.secrets),
            holder: crate::account::Holder::engine(config.stores),
            identity: config.identity,
            oauth_redirect: config.oauth_redirect,
            oauth_returns: HashMap::new(),
            answers: HashMap::new(),
            scans_qr: config.scans_qr,
            sliding_sync,
            pause_for_watch,
            sink: Arc::new(sink),
            accounts: HashMap::new(),
            tasks: JoinSet::new(),
            running: HashMap::new(),
            widget_runs: HashMap::new(),
            live_shares: HashMap::new(),
            http: crate::links::http_client().map_err(|e| e.to_string()),
            #[cfg(feature = "native-media")]
            signals: _signals,
            #[cfg(feature = "native-media")]
            call: None,
            #[cfg(feature = "native-media")]
            leaving: crate::calls::Leaving::default(),
            #[cfg(feature = "native-media")]
            rings: HashMap::new(),
            #[cfg(feature = "native-media")]
            next_ring: 0,
            status_stamp: 0,
            message_number: 0,
            #[cfg(feature = "native-media")]
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
    #[cfg(feature = "command-line")]
    watch_ready: Option<Arc<WatchSync>>,
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
    handlers: Vec<matrix_sdk::event_handler::EventHandlerHandle>,
    /// By room, the last message sent: it lets go once it has gone (an attachment) or is in the
    /// room's send queue (anything else), or failed, and the next one waits for that, so they go
    /// in the order they were sent.
    sends: HashMap<String, tokio::sync::oneshot::Receiver<()>>,
}

/// How many closed timelines an account keeps warm: the rooms and threads switched between.
const WARM_TIMELINES: usize = 8;

impl Running {
    /// Stops following a timeline and keeps it warm.
    fn close_timeline(&mut self, key: &TimelineKey) {
        self.opening.remove(key);
        // The room's last open timeline (a room's and its threads' share it), shown or still
        // opening. The set changes now, so a later open's follow comes after it.
        if !self.timelines.keys().chain(self.opening.keys()).any(|open| open != key && open.room_id == key.room_id) {
            self.account.unfollow_room(&key.room_id);
        }
        let Some((timeline, forward)) = self.timelines.remove(key) else { return };
        forward.abort();
        self.keep_warm(key.clone(), timeline);
    }

    /// Forgets the open `request` was making, once it failed or was cancelled: nothing will
    /// show its timeline.
    fn forget_opening(&mut self, request: RequestId) {
        let failed: Vec<TimelineKey> =
            self.opening.iter().filter(|(_, opening)| **opening == request).map(|(key, _)| key.clone()).collect();
        self.opening.retain(|_, opening| *opening != request);
        // An open that failed followed its room for nothing, unless another timeline shows it.
        for key in failed {
            if !self.timelines.keys().chain(self.opening.keys()).any(|open| open.room_id == key.room_id) {
                self.account.unfollow_room(&key.room_id);
            }
        }
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
    Outcome::LoggedIn(Box::new(account), Box::new(cached))
}

/// Stops a task when the work that started it ends.
struct AbortOnDrop(AbortHandle);

/// Clears a flag when dropped: a loop that ends, however it ends.
struct ClearOnDrop(Arc<std::sync::atomic::AtomicBool>);

impl Drop for ClearOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

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
        for handler in self.handlers.drain(..) {
            self.account.client().remove_event_handler(handler);
        }
        self.flows.stop();
    }
}

/// How long quitting waits for the call under way to leave.
const QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// The longest a ring rings, whatever its event says.
#[cfg(feature = "native-media")]
const RING_CAP: u64 = 2 * 60 * 1000;

/// A call ringing for the user, until it is answered, declined or lapses.
#[derive(Clone, Debug)]
#[cfg(feature = "native-media")]
enum Ring {
    /// A MatrixRTC session's ring: answering joins the session.
    Rtc { video: bool, until: u64 },
    /// A one-to-one invite: answering answers it.
    Direct { invite: crate::calls::Invite, video: bool, until: u64 },
}

#[cfg(feature = "native-media")]
impl Ring {
    fn until(&self) -> u64 {
        match self {
            Ring::Rtc { until, .. } | Ring::Direct { until, .. } => *until,
        }
    }
}

/// Whether the user is in the room's call on another device. One entry per device: this
/// device's own (stale, after a crash) doesn't count.
#[cfg(feature = "native-media")]
fn joined_elsewhere(room: &matrix_sdk::Room, sticky: &crate::calls::StickyMembers) -> bool {
    let me = room.own_user_id();
    let mine = room.active_room_call_participants().iter().filter(|user| *user == me).count();
    let here = room.client().device_id().is_some_and(|device| room.is_device_in_active_room_call(me, device));
    let device = room.client().device_id().map(ToString::to_string).unwrap_or_default();
    // A matrix_2_0 membership of the user's on another device.
    let elsewhere = sticky
        .members(room.room_id().as_str(), now_ms())
        .iter()
        .any(|member| member.user_id == me.as_str() && member.device_id != device);
    mine > usize::from(here) || elsewhere
}

/// Whether anyone is in the room's call: a membership of either standard.
#[cfg(feature = "native-media")]
fn call_under_way(room: &matrix_sdk::Room, sticky: &crate::calls::StickyMembers) -> bool {
    room.has_active_room_call() || !sticky.members(room.room_id().as_str(), now_ms()).is_empty()
}

/// One of the account's rooms, by ID.
#[cfg(feature = "native-media")]
fn room_of(account: &Account, room_id: &str) -> Option<matrix_sdk::Room> {
    matrix_sdk::ruma::RoomId::parse(room_id).ok().and_then(|id| account.client().get_room(&id))
}

pub(crate) fn now_ms() -> u64 {
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
    /// Running commands that ask the app questions, by request: where its answers go.
    answers: HashMap<RequestId, mpsc::UnboundedSender<Answer>>,
    /// Whether the app scans QR codes ([`EngineConfig::scans_qr`]).
    scans_qr: bool,
    sliding_sync: bool,
    pause_for_watch: bool,
    sink: Arc<dyn EventSink>,
    accounts: HashMap<AccountId, Running>,
    tasks: JoinSet<Completion>,
    /// Command tasks still running, for [`Command::Cancel`].
    running: HashMap<RequestId, AbortHandle>,
    /// Local live shares, scoped to the destination, including starts still in flight.
    live_shares: HashMap<(AccountId, String), (RequestId, tokio::sync::watch::Sender<()>)>,
    /// Widget runs ([`Command::RunWidget`] and the like), by the account they act for: signing
    /// it out ends them.
    widget_runs: HashMap<RequestId, AccountId>,
    /// For linked files and call services; shared by every account, carries no credentials.
    http: Result<matrix_sdk::reqwest::Client, String>,
    /// Where the accounts' call signals go, for [`State::run`] to route.
    #[cfg(feature = "native-media")]
    signals: mpsc::UnboundedSender<crate::calls::Signal>,
    /// The call under way.
    #[cfg(feature = "native-media")]
    call: Option<crate::calls::CallHandle>,
    /// The call last hung up, until it has left: the next call, a sign-out and quitting wait
    /// for it.
    #[cfg(feature = "native-media")]
    leaving: crate::calls::Leaving,
    /// Calls ringing for the user, by account and room, numbered so a lapse ends only its own.
    #[cfg(feature = "native-media")]
    rings: HashMap<(AccountId, String), (u64, Ring)>,
    /// The number of the next ring.
    #[cfg(feature = "native-media")]
    next_ring: u64,
    /// When the last status was chosen (`Command::SetStatus`), in ms since the epoch: each is
    /// stamped later than the one before, even within one millisecond.
    status_stamp: u64,
    /// The number of the last status message set (`Command::SetStatusMessage`).
    message_number: u64,
    /// The microphone, speaker and camera the user picked for calls.
    #[cfg(feature = "native-media")]
    devices: crate::calls::DeviceChoice,
}

impl State {
    /// Where the app's answers to `request`'s questions go ([`Command::Answer`]).
    fn answers_for(&mut self, request: RequestId) -> mpsc::UnboundedReceiver<Answer> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.answers.insert(request, sender);
        receiver
    }

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
                Some(_signal) = signals.recv() => {
                    #[cfg(feature = "native-media")]
                    self.signal(_signal);
                },
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
        #[cfg(feature = "native-media")]
        self.hang_up();
        self.end_locations(|_, _| true).await;
        #[cfg(feature = "native-media")]
        if let Some(task) = self.leaving.any() {
            let _ = tokio::time::timeout(QUIT_WAIT, task).await;
        }
    }

    fn restore_saved(&mut self) {
        let (suspended, saved): (Vec<_>, Vec<_>) =
            self.sessions.list().into_iter().partition(|stored| stored.signed_out);
        let mut restoring: Vec<_> = saved.iter().map(|stored| AccountId::new(&stored.id)).collect();
        if !self.sliding_sync {
            restoring.extend(suspended.iter().map(|stored| AccountId::new(&stored.id)));
        }
        self.sink.emit(Event::Restoring(restoring));
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
    #[cfg(feature = "native-media")]
    fn live_call(&mut self) -> Option<&crate::calls::CallHandle> {
        if self.call.as_ref().is_some_and(crate::calls::CallHandle::is_over) {
            self.call = None;
        }
        self.call.as_ref()
    }

    /// Ends the call under way, if any, keeping hold of it until it has left.
    #[cfg(feature = "native-media")]
    fn hang_up(&mut self) {
        if let Some(call) = self.call.take() {
            let account = call.account.clone();
            let room_id = call.room_id.clone();
            self.leaving.hung_up(account, room_id, call.hang_up());
        }
    }

    /// Starts a call, which first waits for the one under way (or the one just hung up) to
    /// leave.
    #[cfg(feature = "native-media")]
    fn start_call(&mut self, account: Account, room_id: String, start: crate::calls::Start) {
        self.hang_up();
        let previous = self.leaving.any();
        let http = self.http.as_ref().ok().cloned();
        let devices = self.devices.clone();
        let sink = Arc::clone(&self.sink);
        self.call = Some(crate::calls::start(account, room_id, start, devices, sink, http, previous));
    }

    #[cfg(feature = "native-media")]
    fn end_ring(&mut self, account: &AccountId, room_id: &str) -> Option<Ring> {
        let (_, ring) = self.rings.remove(&(account.clone(), room_id.to_owned()))?;
        self.sink.emit(Event::RingEnded { account: account.clone(), room_id: room_id.to_owned() });
        Some(ring)
    }

    /// The user is leaving (or is out of) a room: its call ends and its ring stops.
    #[cfg(feature = "native-media")]
    fn leave_calls_in(&mut self, account: &AccountId, room_id: &str) {
        if self.call.as_ref().is_some_and(|call| call.account == *account && call.room_id == room_id) {
            self.hang_up();
        }
        self.end_ring(account, room_id);
    }

    /// Signal the selected shares now. Before giving up the room or its token, let each
    /// share send its final beacon state while the account still has permission.
    fn end_locations(
        &self,
        matches: impl Fn(&AccountId, &str) -> bool,
    ) -> futures_util::future::BoxFuture<'static, ()> {
        let mut stopping = Vec::new();
        for ((account, room), (_, stop)) in &self.live_shares {
            if matches(account, room) {
                let _ = stop.send(());
                stopping.push(stop.clone());
            }
        }
        Box::pin(async move {
            let finished = futures_util::future::join_all(stopping.iter().map(tokio::sync::watch::Sender::closed));
            let _ = tokio::time::timeout(QUIT_WAIT, finished).await;
        })
    }

    /// Stops a running command; it finishes as cancelled.
    fn cancel(&mut self, target: RequestId) {
        if let Some((_, stop)) = self.live_shares.values().find(|(request, _)| *request == target) {
            let _ = stop.send(());
            return;
        }
        self.answers.remove(&target);
        self.widget_runs.remove(&target);
        if let Some(handle) = self.running.remove(&target) {
            handle.abort();
            self.forget_opening(target);
            self.sink.emit(Event::Finished { request: target, result: Err(Error::Cancelled.to_string()) });
        }
    }

    /// Ends the widget runs acting for an account that is going.
    fn end_widget_runs(&mut self, account: &AccountId) {
        let runs: Vec<RequestId> =
            self.widget_runs.iter().filter(|(_, owner)| *owner == account).map(|(request, _)| *request).collect();
        for request in runs {
            self.cancel(request);
        }
    }

    #[cfg(feature = "native-media")]
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
    #[cfg(feature = "native-media")]
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
                // Its summary names who is in its call: matrix_2_0 members are heard apart from
                // the sync's room updates.
                if let Some(running) = self.accounts.get(account) {
                    let (feed, changed) = (running.feed.clone(), HashSet::from([room_id.clone()]));
                    tokio::spawn(async move { feed.refresh(Some(&changed)).await });
                }
                if ours(account, room_id)
                    && let Some(call) = call
                {
                    call.send(crate::calls::Control::Signal(signal.clone()));
                }
                // A ring is over once its session emptied, or once the user joined it from
                // another device.
                if matches!(self.rings.get(&(account.clone(), room_id.clone())), Some((_, Ring::Rtc { .. }))) {
                    let found = self.accounts.get(account).map(|r| &r.account);
                    let over =
                        found.and_then(|found| room_of(found, room_id).map(|room| (room, &found.sticky))).is_some_and(
                            |(room, sticky)| !call_under_way(&room, sticky) || joined_elsewhere(&room, sticky),
                        );
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
                let live = room_of(found, &room_id).is_some_and(|room| call_under_way(&room, &found.sticky));
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
            Signal::Left { account, room_id } => {
                self.leave_calls_in(&account, &room_id);
                drop(self.end_locations(|owner, room| owner == &account && room == room_id));
            }
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
    #[cfg(feature = "native-media")]
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
    #[cfg(feature = "native-media")]
    fn ringing(&self, account: &AccountId, room_id: &str, ring: u64) -> bool {
        self.rings.get(&(account.clone(), room_id.to_owned())).is_some_and(|(id, _)| *id == ring)
    }

    fn account(&self, id: &AccountId) -> Result<Account, Error> {
        self.accounts.get(id).map(|running| running.account.clone()).ok_or_else(|| Error::UnknownAccount(id.clone()))
    }

    fn login_oauth(
        &mut self,
        request: RequestId,
        homeserver: String,
        resume: Option<AccountId>,
        create: bool,
        expected: Option<AccountId>,
    ) -> futures_util::future::BoxFuture<'static, Result<Outcome, Error>> {
        let (layout, sessions, identity) = (self.layout.clone(), self.sessions.clone(), self.identity.clone());
        let (holder, sink) = (self.holder, Arc::clone(&self.sink));
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
            let target =
                crate::account::OAuthTarget { homeserver: &homeserver, resume: resume.as_ref(), create, expected };
            let pending = Account::start_oauth(&layout, &sessions, holder, &identity, target, app_redirect).await?;
            sink.emit(Event::OpenUrl { request, url: pending.url.to_string() });
            Ok(signed_in(pending.finish(&sessions).await?).await)
        })
    }

    /// Starts the task for one command. Runs on the loop, so it must not await.
    fn dispatch(&mut self, Envelope { request, command }: Envelope) {
        let sink = Arc::clone(&self.sink);
        let command_kind = if matches!(command, Command::ResetRecoveryKey(_)) { Kind::Reset } else { Kind::Other };
        let task: std::pin::Pin<Box<dyn Future<Output = Result<Outcome, Error>> + Send>> = match command {
            #[cfg(feature = "command-line")]
            Command::Terminal { account, action } => {
                let found = self.account(&account);
                let running = self.accounts.get(&account);
                let usage = running.map(|running| running.usage.clone());
                let watch_ready = running.and_then(|running| running.watch_ready.clone());
                Box::pin(async move {
                    let report = |value| sink.emit(Event::TerminalResult(request, value));
                    let usage = match usage {
                        Some(usage) => usage.snapshot().await,
                        None => crate::SearchUsage::default(),
                    };
                    crate::terminal::run(found?, *action, usage, report, watch_ready).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::FinishOAuth { request: sign_in, url } => {
                let result = match self.oauth_returns.remove(&sign_in) {
                    Some(waiting) => waiting.send(url).map_err(|_| Error::Cancelled.to_string()),
                    None => Err(Error::Cancelled.to_string()),
                };
                self.sink.emit(Event::Finished { request, result });
                return;
            }
            Command::Answer { request: asking, answer } => {
                if let Some(answers) = self.answers.get(&asking) {
                    let _ = answers.send(answer);
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            Command::Cancel(target) => {
                self.cancel(target);
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            #[cfg(feature = "native-media")]
            Command::StartCall { account, room_id, video } => {
                let result = self.account(&account).map(|found| {
                    self.end_ring(&account, &room_id);
                    self.start_call(found, room_id, crate::calls::Start::New { video });
                });
                self.sink.emit(Event::Finished { request, result: result.map_err(|e| e.to_string()) });
                return;
            }
            #[cfg(feature = "native-media")]
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
            #[cfg(feature = "native-media")]
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
            #[cfg(feature = "native-media")]
            Command::HangUp => {
                self.hang_up();
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            #[cfg(feature = "native-media")]
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
            #[cfg(feature = "native-media")]
            Command::WatchVideo { video, sink } => {
                if let Some(call) = self.live_call() {
                    call.send(crate::calls::Control::Watch(video, sink));
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            #[cfg(feature = "native-media")]
            Command::ListCallDevices => {
                let sink = Arc::clone(&self.sink);
                tokio::task::spawn_blocking(move || {
                    sink.emit(Event::CallDevices(crate::calls::list_devices()));
                    sink.emit(Event::Finished { request, result: Ok(()) });
                });
                return;
            }
            #[cfg(feature = "native-media")]
            Command::ChooseCallDevice { kind, id } => {
                self.devices.set(kind, id.clone());
                if let Some(call) = self.live_call() {
                    call.send(crate::calls::Control::Device(kind, id));
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            #[cfg(feature = "native-media")]
            Command::UnwatchVideo(video) => {
                if let Some(call) = self.live_call() {
                    call.send(crate::calls::Control::Unwatch(video));
                }
                self.sink.emit(Event::Finished { request, result: Ok(()) });
                return;
            }
            #[cfg(not(feature = "native-media"))]
            Command::StartCall { .. }
            | Command::AnswerCall { .. }
            | Command::DeclineCall { .. }
            | Command::HangUp
            | Command::SetCallMedia(_)
            | Command::WatchVideo { .. }
            | Command::ListCallDevices
            | Command::ChooseCallDevice { .. }
            | Command::UnwatchVideo(_) => Box::pin(async { Err(Error::Unsupported("calls")) }),
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
                    // Whether a matrix_2_0 call is under way here, from members who joined before
                    // this session heard them (once per room until a reconnect; nothing where the
                    // server has no sticky events).
                    #[cfg(feature = "native-media")]
                    if let Some(room) = room_of(&running.account, &key.room_id) {
                        let account = running.account.clone();
                        tokio::spawn(async move {
                            let http = room.client().http_client().clone();
                            account.sticky.fetch(&room, &http).await;
                        });
                    }
                    // The room's widgets, so it can offer its meetings (read from the server once
                    // a run under sliding sync, which leaves them out).
                    if key.thread.is_none() {
                        let (account, sink, room_id) =
                            (running.account.clone(), Arc::clone(&sink), key.room_id.clone());
                        tokio::spawn(async move {
                            let freshness = crate::widgets::Freshness::Known;
                            if let Ok(widgets) = account.room_widgets(&room_id, freshness).await {
                                sink.emit(Event::Widgets { account: account.id().clone(), room_id, widgets });
                            }
                        });
                    }
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
            Command::Edit { account, key, event_id, body, html, mentions } => {
                let timeline = self.open_timeline(&account, &key);
                Box::pin(async move {
                    let item = matrix_sdk_ui::timeline::TimelineEventItemId::EventId(matrix_sdk::ruma::EventId::parse(
                        &event_id,
                    )?);
                    let content = crate::outgoing::text(&body, html.as_deref(), &mentions);
                    timeline?.edit(&item, matrix_sdk::room::edit::EditedContent::RoomMessage(content)).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ListEdits { account: id, room_id, event_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let versions = account?.edit_history(&room_id, &event_id).await?;
                    sink.emit(Event::EditHistory { account: id, room_id, event_id, versions });
                    Ok(Outcome::Done)
                })
            }
            Command::LocateMessage { account: id, room_id, event_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let (key, event_id) = account?.locate_message(&room_id, &event_id).await?;
                    sink.emit(Event::MessageLocated { request, account: id, key, event_id });
                    Ok(Outcome::Done)
                })
            }
            Command::Schedule { account: id, key, reply_to, message, delay_ms } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    let delay = std::time::Duration::from_millis(delay_ms);
                    account.schedule(&key, reply_to.as_deref(), &message, delay).await?;
                    // A server that sends nothing back for the list still scheduled it: what this
                    // device knows goes instead.
                    let _order = account.listing_scheduled.lock().await;
                    let (Ok(messages) | Err((_, messages))) = account.scheduled_or_known().await;
                    sink.emit(Event::ScheduledMessages { account: id, messages });
                    Ok(Outcome::Done)
                })
            }
            Command::ListScheduled { account: id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    // In the order asked: an older list never covers a newer.
                    let _order = account.listing_scheduled.lock().await;
                    // Unlisted, what this device knows shows, and the failure says why.
                    match account.scheduled_or_known().await {
                        Ok(messages) => {
                            sink.emit(Event::ScheduledMessages { account: id, messages });
                            Ok(Outcome::Done)
                        }
                        Err((error, messages)) => {
                            sink.emit(Event::ScheduledMessages { account: id, messages });
                            Err(error)
                        }
                    }
                })
            }
            Command::UpdateScheduled { account: id, delay_id, action } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.update_scheduled(&delay_id, action).await?;
                    let _order = account.listing_scheduled.lock().await;
                    let (Ok(messages) | Err((_, messages))) = account.scheduled_or_known().await;
                    sink.emit(Event::ScheduledMessages { account: id, messages });
                    Ok(Outcome::Done)
                })
            }
            Command::ListImagePacks { account: id, room_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let listed = account?.image_packs(room_id.as_deref()).await?;
                    sink.emit(Event::ImagePacks { account: id, room_id, packs: listed.packs, may_add: listed.may_add });
                    Ok(Outcome::Done)
                })
            }
            Command::EditImagePack { account: id, pack, edit } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let pack = account?.edit_image_pack(&pack, edit).await?;
                    sink.emit(Event::ImagePackEdited { request, account: id, pack });
                    Ok(Outcome::Done)
                })
            }
            Command::SearchGifs { key, query, offset, language } => {
                let http = self.http.clone();
                Box::pin(async move {
                    let http = http.map_err(Error::GifSearch)?;
                    let page = crate::gif_search::search(&http, &key, &query, offset, language.as_deref()).await?;
                    sink.emit(Event::GifsFound { request, query, offset, page });
                    Ok(Outcome::Done)
                })
            }
            Command::Locate { request_access } => {
                let http = self.http.clone();
                Box::pin(async move {
                    #[cfg(all(feature = "native-media", not(any(target_os = "android", target_os = "ios"))))]
                    {
                        let position = crate::geo::desktop::position(request_access).await;
                        if position.is_ok() || request_access {
                            sink.emit(Event::PositionFound { request, geo_uri: position?, estimated: false });
                            return Ok(Outcome::Done);
                        }
                    }
                    let _ = request_access;
                    let geo_uri = crate::geo::estimate(&http.map_err(Error::Location)?).await?;
                    sink.emit(Event::PositionFound { request, geo_uri, estimated: true });
                    Ok(Outcome::Done)
                })
            }
            Command::ShareLiveLocation { account, room_id, geo_uri } => {
                let room = self.account(&account).and_then(|owner| owner.joined_room(&room_id));
                let destination = (account, room_id);
                if let std::collections::hash_map::Entry::Vacant(entry) = self.live_shares.entry(destination) {
                    let (stop, stopped) = tokio::sync::watch::channel(());
                    entry.insert((request, stop));
                    let answers = self.answers_for(request);
                    Box::pin(async move {
                        crate::geo::live::share(room?, geo_uri, answers, stopped, |event_id| {
                            sink.emit(Event::LiveLocationStarted { request, event_id })
                        })
                        .await?;
                        Ok(Outcome::Done)
                    })
                } else {
                    Box::pin(async { Err(Error::Location("A live location is already being shared here".into())) })
                }
            }
            Command::SearchPlaces { query, language } => {
                let http = self.http.clone();
                Box::pin(async move {
                    let http = http.map_err(|reason| Error::Link { url: String::new(), reason })?;
                    let places = crate::geo::search_places(&http, &query, language.as_deref()).await?;
                    sink.emit(Event::PlacesFound { request, places });
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
            Command::SetSaved { account, room_id, event_id, saved } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.set_saved(&room_id, &event_id, saved).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ListSavedMessages { account: id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let messages = account?.saved_messages().await?;
                    sink.emit(Event::SavedMessages { account: id, messages });
                    Ok(Outcome::Done)
                })
            }
            // Queued now, in the order the commands came: drafts change in that order.
            Command::SaveDraft { account, key, draft } => {
                let saved = self.account(&account).map(|account| {
                    let queue = account.drafts.clone();
                    queue.run(async move { account.save_draft(&key, draft).await })
                });
                Box::pin(async move {
                    saved?.await?;
                    Ok(Outcome::Done)
                })
            }
            Command::LoadDraft { account: id, key } => {
                let loaded = self.account(&id).map(|account| {
                    let (queue, key) = (account.drafts.clone(), key.clone());
                    queue.run(async move { account.load_draft(&key).await })
                });
                Box::pin(async move {
                    let draft = loaded?.await?;
                    sink.emit(Event::DraftLoaded { request, account: id, key, draft });
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
            Command::ViewSource { account: id, room_id, event_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let source = account?.event_source(&room_id, &event_id).await?;
                    sink.emit(Event::EventSource { request, source });
                    Ok(Outcome::Done)
                })
            }
            Command::ListWidgets { account: id, room_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let widgets = account?.widgets(&room_id).await?;
                    sink.emit(Event::Widgets { account: id, room_id, widgets });
                    Ok(Outcome::Done)
                })
            }
            Command::AddWidget { account: id, room_id, name, url } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.add_widget(&room_id, &name, &url).await?;
                    // Added, even when the list can't be read again: retrying would add another.
                    if let Some(widgets) = account.widgets_after_write(&room_id).await {
                        sink.emit(Event::Widgets { account: id, room_id, widgets });
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::RemoveWidget { account: id, room_id, widget_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let account = account?;
                    account.remove_widget(&room_id, &widget_id).await?;
                    if let Some(widgets) = account.widgets_after_write(&room_id).await {
                        sink.emit(Event::Widgets { account: id, room_id, widgets });
                    }
                    Ok(Outcome::Done)
                })
            }
            Command::RunWidget { account: id, room_id, widget_id, browser } => {
                self.widget_runs.insert(request, id.clone());
                let account = self.account(&id);
                let answers = self.answers_for(request);
                Box::pin(async move {
                    // From the server: one removed since sync last said opens no more.
                    let fresh = crate::widgets::Freshness::Server;
                    crate::widgets::session::run(account?, room_id, widget_id, browser, fresh, request, sink, answers)
                        .await?;
                    Ok(Outcome::Done)
                })
            }
            Command::StartMeeting { account: id, room_id, video, browser } => {
                self.widget_runs.insert(request, id.clone());
                let account = self.account(&id);
                let answers = self.answers_for(request);
                Box::pin(async move {
                    let account = account?;
                    let widget_id = account.start_meeting(&room_id, video).await?;
                    // Started, even when the list can't be read again: retrying would start another.
                    if let Some(widgets) = account.widgets_after_write(&room_id).await {
                        sink.emit(Event::Widgets { account: id, room_id: room_id.clone(), widgets });
                    }
                    // Read from the server: sync may not have brought it yet.
                    let fresh = crate::widgets::Freshness::Server;
                    crate::widgets::session::run(account, room_id, widget_id, browser, fresh, request, sink, answers)
                        .await?;
                    Ok(Outcome::Done)
                })
            }
            Command::OpenIntegrations { account: id, room_id } => {
                self.widget_runs.insert(request, id.clone());
                let account = self.account(&id);
                let answers = self.answers_for(request);
                Box::pin(async move {
                    crate::widgets::integrations::run(account?, id, room_id, request, sink, answers).await?;
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
            // Queued now, in the order the commands came: a role change rewrites the power levels
            // as room settings do.
            Command::ModerateMember { account: id, room_id, user_id, action } => {
                let account = self.account(&id);
                let moderated = account.as_ref().ok().map(|account| {
                    let (account, room_id, queue) = (account.clone(), room_id.clone(), account.admin_changes.clone());
                    queue.run(async move { account.moderate(&room_id, &user_id, action).await })
                });
                Box::pin(async move {
                    let account = account?;
                    let done = match moderated {
                        Some(moderated) => moderated.await,
                        None => Ok(()),
                    };
                    // Also after a failure: a request answered elsewhere shows as it is now.
                    if done.is_ok() || matches!(done, Err(Error::Outdated)) {
                        let members = account.members(&room_id, true).await?;
                        sink.emit(Event::Members { request: Some(request), account: id, room_id, members });
                    }
                    done?;
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
            // Queued with the admin changes: opening it twice at once makes one room.
            Command::OpenNoteToSelf { account: id, name } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                let opening = account.as_ref().ok().map(|account| {
                    let account = account.clone();
                    account.admin_changes.clone().run(async move { account.note_to_self(&name).await })
                });
                Box::pin(async move {
                    let (_, feed) = (account?, feed?);
                    let Some(opening) = opening else { return Ok(Outcome::Done) };
                    let room_id = opening.await?;
                    // The app opens the room at once: the list it reads must have it already.
                    feed.refresh(Some(&HashSet::from([room_id.clone()]))).await;
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
            Command::CreateRoom { account, room } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let room_id = account?.create_room(&room).await?;
                    sink.emit(Event::RoomCreated { request, room_id });
                    Ok(Outcome::Done)
                })
            }
            // Queued with the changes: a read started before a change can't report after it.
            Command::LoadRoomSettings { account: id, room_id } => {
                let account = self.account(&id);
                let loading = account.as_ref().ok().map(|account| {
                    let (account, queue, sink) = (account.clone(), account.admin_changes.clone(), Arc::clone(&sink));
                    queue.run(async move {
                        let settings = account.room_settings(&room_id).await?;
                        sink.emit(Event::RoomSettings { account: id, settings: Box::new(settings) });
                        Ok(())
                    })
                });
                Box::pin(async move {
                    account?;
                    if let Some(loading) = loading {
                        loading.await?;
                    }
                    Ok(Outcome::Done)
                })
            }
            // Queued now, in the order the commands came: each change reads what the last left.
            Command::ChangeRoom { account: id, room_id, setting } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                let changing = account.as_ref().ok().map(|account| {
                    let (account, room_id, queue) = (account.clone(), room_id.clone(), account.admin_changes.clone());
                    let (sink, id) = (Arc::clone(&sink), id.clone());
                    queue.run(async move {
                        let changed = account.change_room(&room_id, setting).await;
                        // Read and reported in the queue too, also after a failure (what the
                        // server kept shows as it is): an earlier change's report can't arrive
                        // after a later one's.
                        if let Ok(settings) = account.room_settings(&room_id).await {
                            sink.emit(Event::RoomSettings { account: id, settings: Box::new(settings) });
                        }
                        Ok(changed)
                    })
                });
                Box::pin(async move {
                    account?;
                    let changed = match changing {
                        Some(changing) => changing.await?,
                        None => Ok(()),
                    };
                    feed?.refresh(Some(&HashSet::from([room_id.clone()]))).await;
                    changed?;
                    Ok(Outcome::Done)
                })
            }
            // Queued with the other admin changes: it rewrites spaces' children as a reorder does.
            Command::UpgradeRoom { account: id, room_id, version } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                let upgrading = account.as_ref().ok().map(|account| {
                    let (account, queue) = (account.clone(), account.admin_changes.clone());
                    let old = room_id.clone();
                    let upgraded = queue.run(async move { account.upgrade_room(&room_id, &version).await });
                    (upgraded, old)
                });
                Box::pin(async move {
                    let account = account?;
                    let Some((upgrading, old)) = upgrading else { return Ok(Outcome::Done) };
                    let new = upgrading.await?;
                    // The members follow by invitation, outside the queue: rate limits may make it
                    // slow, and nothing else should wait for it.
                    let (inviting, successor) = (account.clone(), new.clone());
                    tokio::spawn(async move { inviting.invite_to_successor(&old, &successor).await });
                    feed?.refresh(None).await;
                    sink.emit(Event::RoomCreated { request, room_id: new });
                    Ok(Outcome::Done)
                })
            }
            // Queued now, in the order the commands came: two quick moves keep their order.
            Command::ChangeSpace { account: id, space, change } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                let changing = account.as_ref().ok().map(|account| {
                    let (account, space, change, queue) =
                        (account.clone(), space.clone(), change.clone(), account.admin_changes.clone());
                    queue.run(async move { account.change_space(&space, &change).await })
                });
                Box::pin(async move {
                    account?;
                    let result = match changing {
                        Some(changing) => changing.await,
                        None => Ok(()),
                    };
                    // Also after a failure: what the server kept shows as it is.
                    let mut changed = HashSet::from([space]);
                    if let crate::SpaceChange::Add(room) | crate::SpaceChange::Remove(room) = change {
                        changed.insert(room);
                    }
                    feed?.refresh(Some(&changed)).await;
                    result?;
                    Ok(Outcome::Done)
                })
            }
            Command::LeaveSpace { account: id, space, rooms } => {
                let (account, feed) = (self.account(&id), self.feed(&id));
                #[cfg(feature = "native-media")]
                for room_id in &rooms {
                    self.leave_calls_in(&id, room_id);
                }
                let locations = self
                    .end_locations(|owner, room| owner == &id && (room == space || rooms.iter().any(|r| r == room)));
                Box::pin(async move {
                    locations.await;
                    let left = account?.leave_space(&space, &rooms).await;
                    feed?.refresh(None).await;
                    left?;
                    Ok(Outcome::Done)
                })
            }
            // Queued now, in the order the commands came: two quick choices end on the later.
            Command::SetSafety { account: id, change } => {
                let changed = self.account(&id).map(|account| {
                    let queue = account.safety_queue.clone();
                    queue.run(async move { account.change_safety(&change).await })
                });
                Box::pin(async move {
                    let safety = changed?.await?;
                    sink.emit(Event::Safety { account: id, safety });
                    Ok(Outcome::Done)
                })
            }
            Command::Report { account, target, reason } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.report(&target, &reason).await?;
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
                let (account, flows, scans_qr) = (self.account(&id), self.flows(&id), self.scans_qr);
                Box::pin(async move {
                    let started = |flow_id| sink.emit(Event::VerificationRequested { request, account: id, flow_id });
                    account?.request_verification(None, &flows?, scans_qr, Arc::clone(&sink), started).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::VerifyUser { account: id, user_id } => {
                let (account, flows, scans_qr) = (self.account(&id), self.flows(&id), self.scans_qr);
                Box::pin(async move {
                    let started = |flow_id| sink.emit(Event::VerificationRequested { request, account: id, flow_id });
                    account?
                        .request_verification(Some(&user_id), &flows?, scans_qr, Arc::clone(&sink), started)
                        .await?;
                    Ok(Outcome::Done)
                })
            }
            Command::AcceptVerification { account, flow_id } => {
                let (flows, scans_qr) = (self.flows(&account), self.scans_qr);
                Box::pin(async move {
                    flows?.accept(&flow_id, scans_qr).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::CompareEmoji { account, flow_id } => {
                let flows = self.flows(&account);
                Box::pin(async move {
                    flows?.compare_emoji(&flow_id).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ScanVerificationCode { account, flow_id, code } => {
                let flows = self.flows(&account);
                Box::pin(async move {
                    flows?.scan(&flow_id, &code).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::LoadIdentity { account: id, user_id } => {
                let account = self.account(&id);
                Box::pin(async move {
                    let identity = account?.identity(&user_id).await?;
                    sink.emit(Event::Identity { account: id, user_id, identity });
                    Ok(Outcome::Done)
                })
            }
            Command::ExportRoomKeys { account, path, passphrase } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.export_room_keys(&path, &passphrase.0).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ImportRoomKeys { account, path, passphrase } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let import = account?.import_room_keys(&path, &passphrase.0).await?;
                    sink.emit(Event::KeysImported { request, import });
                    Ok(Outcome::Done)
                })
            }
            Command::SetOfflineDevice { account, on, recovery_key } => {
                let account = self.account(&account);
                Box::pin(async move {
                    account?.set_offline_device(on, recovery_key.as_ref().map(|k| k.0.as_str())).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ChangePassword { account, new_password, password } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let password = password.as_ref().map(|p| p.0.as_str());
                    match account?.change_password(&new_password.0, password).await {
                        Ok(()) => Ok(Outcome::Done),
                        Err(error) => Err(asked_for_password(error, request, &*sink)),
                    }
                })
            }
            Command::DeactivateAccount { account: id, erase, password } => {
                let (account, sessions) = (self.account(&id), self.sessions.clone());
                Box::pin(async move {
                    let account = account?;
                    if account.client().oauth().client_id().is_some() {
                        let url = account.account_page(crate::AccountPage::Deactivate).await?;
                        sink.emit(Event::OpenUrl { request, url });
                        return Ok(Outcome::Done);
                    }
                    match account.deactivate(erase, password.as_ref().map(|p| p.0.as_str())).await {
                        Ok(()) => {
                            // The account is gone on the server whatever happens here: it stops
                            // either way, and data left behind only costs disk.
                            if let Err(error) = crate::account::forget(&sessions, &id).await {
                                eprintln!("matrix-engine: could not forget the deactivated account: {error}");
                            }
                            Ok(Outcome::LoggedOut(id))
                        }
                        Err(error) => Err(asked_for_password(error, request, &*sink)),
                    }
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
                let strict = !self.sliding_sync;
                Box::pin(async move {
                    Ok(signed_in(Account::login_password(&layout, &sessions, holder, &login, strict).await?).await)
                })
            }
            Command::Register(registration) => {
                let (layout, sessions, holder) = (self.layout.clone(), self.sessions.clone(), self.holder);
                let answers = self.answers_for(request);
                Box::pin(async move {
                    let ask = |step: crate::RegistrationStep| {
                        if let crate::RegistrationStep::Browser { url } = &step {
                            sink.emit(Event::OpenUrl { request, url: url.clone() });
                        }
                        sink.emit(Event::Registration { request, step });
                    };
                    let account = Account::register(&layout, &sessions, holder, &registration, &ask, answers).await?;
                    Ok(signed_in(account).await)
                })
            }
            Command::SignInWithQr(start) => {
                let (layout, sessions, holder) = (self.layout.clone(), self.sessions.clone(), self.holder);
                let identity = self.identity.clone();
                let answers = self.answers_for(request);
                Box::pin(async move {
                    let report = |step: crate::QrLoginStep| sink.emit(Event::QrLogin { request, step });
                    let account =
                        Account::sign_in_with_qr(&layout, &sessions, holder, &identity, start, &report, answers)
                            .await?;
                    Ok(signed_in(account).await)
                })
            }
            Command::LinkDevice { account, scanned } => {
                let account = self.account(&account);
                let answers = self.answers_for(request);
                Box::pin(async move {
                    let report = |step: crate::QrLoginStep| {
                        if let crate::QrLoginStep::Approve { url } = &step {
                            sink.emit(Event::OpenUrl { request, url: url.clone() });
                        }
                        sink.emit(Event::QrLogin { request, step });
                    };
                    account?.link_device(scanned, &report, answers).await?;
                    Ok(Outcome::Done)
                })
            }
            Command::ReadQrCode { picture } => Box::pin(async move {
                let code = tokio::task::spawn_blocking(move || crate::qr::read_picture(&picture))
                    .await
                    .map_err(|e| Error::QrCode(e.to_string()))??;
                sink.emit(Event::QrCodeRead { request, code });
                Ok(Outcome::Done)
            }),
            Command::LoginOAuth { homeserver, resume, create } => {
                self.login_oauth(request, homeserver, resume, create, None)
            }
            #[cfg(feature = "command-line")]
            Command::LoginOAuthAs { homeserver, account, resume } => {
                self.login_oauth(request, homeserver, resume.then(|| account.clone()), false, Some(account))
            }
            Command::Logout(id) => {
                let (account, sessions) = (self.account(&id).ok(), self.sessions.clone());
                // The account's call leaves while its token still works.
                #[cfg(feature = "native-media")]
                self.forget_calls(&id);
                #[cfg(feature = "native-media")]
                let leaving = self.leaving.of_account(&id);
                let locations = self.end_locations(|owner, _| owner == &id);
                Box::pin(async move {
                    locations.await;
                    #[cfg(feature = "native-media")]
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
                let locations = self.end_locations(|owner, _| owner == &id);
                let sessions = self.sessions.clone();
                Box::pin(async move {
                    locations.await;
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
                // Held until this send is done: dropping it lets the room's next one go.
                let (sending, next) = tokio::sync::oneshot::channel::<()>();
                let previous =
                    self.accounts.get_mut(&account).and_then(|running| running.sends.insert(key.room_id.clone(), next));
                let (account, timeline) = (self.account(&account), self.open_timeline(&account, &key));
                let http = self.http.clone().ok();
                Box::pin(async move {
                    // The words typed with attachments go first, then the attachments in order.
                    let sending = sending;
                    if let Some(previous) = previous {
                        let _ = previous.await;
                    }
                    let account = account?;
                    let replied_to = reply_to.clone();
                    match timeline {
                        // Through the timeline: replies and thread relations come out right, and
                        // the message shows at once as pending.
                        Ok(timeline) => send_in(&timeline, key.thread.as_deref(), reply_to, &message).await?,
                        Err(_) if key.thread.is_none() && reply_to.is_none() => {
                            account.send(&key.room_id, &message).await?;
                        }
                        Err(error) => return Err(error),
                    }
                    drop(sending);
                    if let Some(usage) = usage {
                        usage.room(&account, &key.room_id, 3, replied_to);
                    }
                    if let (Outgoing::Gif(gif), Some(http)) = (&message, http) {
                        crate::gif_search::report_sent(&http, gif).await;
                    }
                    Ok(Outcome::Done)
                })
            }
            #[cfg(all(feature = "native-media", not(any(target_os = "android", target_os = "ios"))))]
            Command::FetchMedia { media, size, .. } if crate::geo::desktop::is_map(&media) => {
                let cache = self.layout.map_tiles();
                Box::pin(async move {
                    let bytes = crate::geo::desktop::draw(&cache, &media, size).await?;
                    sink.emit(Event::MediaLoaded { request, media, size, bytes: bytes.into() });
                    Ok(Outcome::Done)
                })
            }
            Command::FetchMedia { account, media, size } => {
                let account = self.account(&account);
                Box::pin(async move {
                    let mut bytes = account?.fetch_media(&media, size).await?;
                    // A thumbnail larger than its box is scaled to fit, off the async threads.
                    if let MediaSize::Thumbnail { width, height } = size {
                        bytes = crate::thumbnail::fit_off_thread(bytes, width, height).await;
                    }
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
                #[cfg(feature = "native-media")]
                self.leave_calls_in(&id, &room_id);
                #[cfg(feature = "native-media")]
                let leaving = self.leaving.of_room(&id, &room_id);
                let locations = self.end_locations(|owner, room| owner == &id && room == room_id);
                Box::pin(async move {
                    locations.await;
                    #[cfg(feature = "native-media")]
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
            Command::SetStatusMessage { account: id, message, clear_at_ms } => {
                let account = self.account(&id);
                // Numbered here, as commands arrive: messages settle in the order they were set.
                self.message_number += 1;
                let number = self.message_number;
                Box::pin(async move {
                    let report = |me, expiry| {
                        sink.emit(Event::Presence { account: id.clone(), people: vec![me] });
                        sink.emit(Event::StatusMessageExpiry { account: id.clone(), expiry });
                    };
                    account?.set_status_message(message.as_deref(), clear_at_ms, number, report).await?;
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
            // Queued now, in the order the commands came, like the room changes below.
            Command::SetKeyword { account: id, keyword, enabled } => {
                let (sink, id) = (Arc::clone(&sink), id);
                let changed = self.account(&id).map(|account| {
                    let queue = account.organising.clone();
                    // Reported in the queue too, so a slower report never lands after a later one.
                    queue.run(async move {
                        let result = account.set_keyword(&keyword, enabled).await;
                        // As the change left them; after a failure, as the server has them (or,
                        // unreachable, as last synced).
                        let keywords = match &result {
                            Ok(keywords) => keywords.clone(),
                            Err(_) => match account.server_keywords().await {
                                Ok(keywords) => keywords,
                                Err(_) => account.keywords().await,
                            },
                        };
                        sink.emit(Event::Keywords { account: id, keywords });
                        result.map(drop)
                    })
                });
                Box::pin(async move {
                    changed?.await?;
                    Ok(Outcome::Done)
                })
            }
            // Queued now, in the order the commands came: a quick change back lands last.
            Command::SetRoomPriority { account: id, room_id, priority } => {
                let feed = self.feed(&id);
                let changed = self.account(&id).map(|account| {
                    let (queue, room_id) = (account.organising.clone(), room_id.clone());
                    queue.run(async move { account.set_room_priority(&room_id, priority).await })
                });
                Box::pin(async move {
                    changed?.await?;
                    feed?.refresh(Some(&HashSet::from([room_id]))).await;
                    Ok(Outcome::Done)
                })
            }
            Command::SetUnread { account: id, room_id, unread } => {
                let feed = self.feed(&id);
                let changed = self.account(&id).map(|account| {
                    let (queue, room_id) = (account.organising.clone(), room_id.clone());
                    queue.run(async move { account.set_unread(&room_id, unread).await })
                });
                Box::pin(async move {
                    changed?.await?;
                    feed?.refresh(Some(&HashSet::from([room_id]))).await;
                    Ok(Outcome::Done)
                })
            }
            Command::MarkRoomRead { account: id, room_id } => {
                let feed = self.feed(&id);
                let changed = self.account(&id).map(|account| {
                    let (queue, room_id) = (account.organising.clone(), room_id.clone());
                    queue.run(async move { account.mark_room_read(&room_id).await })
                });
                Box::pin(async move {
                    changed?.await?;
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

    /// A timeline `request` was opening, if any, is no longer being opened.
    fn forget_opening(&mut self, request: RequestId) {
        for running in self.accounts.values_mut() {
            running.forget_opening(request);
        }
    }

    /// Applies a finished task's outcome to the state, then answers its request.
    fn complete(&mut self, Completion { request, result }: Completion) {
        if let Some(request) = request {
            self.running.remove(&request);
            self.answers.remove(&request);
            self.widget_runs.remove(&request);
            self.live_shares.retain(|_, (active, _)| *active != request);
        }
        let result = match result {
            Ok(Outcome::Done) => Ok(()),
            Ok(Outcome::LoggedIn(account, cached)) => {
                let account = *account;
                // Shown now, before the sign-in's `Finished`: the app selects an account it is
                // adding when it arrives.
                self.sink.emit(Event::AccountAdded((*cached).clone()));
                if let Some(request) = request {
                    self.sink.emit(Event::SignedIn { request, account: account.id().clone() });
                }
                // Saving refreshed tokens comes first: the summary's requests may refresh them.
                let terminal_sessions = (!self.sliding_sync).then(|| self.sessions.clone());
                let tokens = tokio::spawn(watch_session(account.clone(), Arc::clone(&self.sink), terminal_sessions));
                let flows = crate::security::Flows::default();
                let feed = RoomFeed { account: account.clone(), sink: Arc::clone(&self.sink), cache: Arc::default() };
                #[cfg(feature = "native-media")]
                account.sticky.report_to(account.id().clone(), self.signals.clone());
                #[cfg(feature = "native-media")]
                let mut handlers = crate::calls::listen(account.client(), account.id(), &self.signals, &account.sticky);
                #[cfg(not(feature = "native-media"))]
                let mut handlers = Vec::new();
                handlers.extend(crate::security::listen(&account, &flows, self.scans_qr, &self.sink));
                // The rest waits on the server, so it runs beside the engine rather than holding
                // up every account's commands. The account shows before anything about it.
                let sliding_sync = self.sliding_sync;
                let watch_ready = self.pause_for_watch.then(|| Arc::new(WatchSync::new()));
                let started = {
                    let watch_ready = watch_ready.clone();
                    let (account, sink, feed) = (account.clone(), Arc::clone(&self.sink), feed.clone());
                    tokio::spawn(async move {
                        // The safety choices as this device stored them, before any room or
                        // invitation shows (the room list follows from the sync loop below): an
                        // app hides what they hide from the first frame. The server's follow, as
                        // a save may not have synced here before the app quit.
                        match account.safety().await {
                            Ok(safety) => sink.emit(Event::Safety { account: account.id().clone(), safety }),
                            Err(error) => eprintln!("matrix-engine: could not read the safety choices: {error}"),
                        }
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
                        let safety = async {
                            if let Some(safety) = account.synced_safety().await {
                                sink.emit(Event::Safety { account: account.id().clone(), safety });
                            }
                        };
                        let keywords = async {
                            sink.emit(Event::Keywords {
                                account: account.id().clone(),
                                keywords: account.keywords().await,
                            });
                        };
                        let status = account
                            .report_status(|status| sink.emit(Event::Status { account: account.id().clone(), status }));
                        // Once the homeserver names a call service, rooms offer MatrixRTC calls.
                        #[cfg(feature = "native-media")]
                        let calls = {
                            let (account, feed) = (account.clone(), feed.clone());
                            async move {
                                if account.discover_rtc().await {
                                    feed.refresh(None).await;
                                }
                            }
                        };
                        #[cfg(not(feature = "native-media"))]
                        let calls = std::future::ready(());
                        // Once the homeserver names a Jitsi server, rooms offer meetings.
                        let meetings = {
                            let (account, feed) = (account.clone(), feed.clone());
                            async move {
                                if account.discover_meetings().await {
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
                            safety,
                            keywords,
                            status,
                            own,
                            account.watch_status_message(
                                |expiry| sink
                                    .emit(Event::StatusMessageExpiry { account: account.id().clone(), expiry }),
                                |me| sink.emit(Event::Presence { account: account.id().clone(), people: vec![me] }),
                            ),
                            calls,
                            meetings,
                            crate::security::watch(account.clone(), Arc::clone(&sink)),
                            sync_loop(
                                account.clone(),
                                Arc::clone(&sink),
                                feed,
                                Arc::clone(&own_refresh),
                                sliding_sync,
                                watch_ready.clone()
                            ),
                        );
                    })
                };
                let (usage, usage_worker) = crate::search::Usage::start(account.clone(), Arc::clone(&self.sink));
                let background = vec![tokens.abort_handle(), started.abort_handle(), usage_worker];
                let running = Running {
                    account,
                    #[cfg(feature = "command-line")]
                    watch_ready,
                    usage,
                    background,
                    flows,
                    timelines: HashMap::new(),
                    warm: Vec::new(),
                    opening: HashMap::new(),
                    keys_fetched: HashSet::new(),
                    feed,
                    handlers,
                    sends: HashMap::new(),
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
                        // Its follow came after the close let the room go, so it goes again.
                        if !running
                            .timelines
                            .keys()
                            .chain(running.opening.keys())
                            .any(|key| key.room_id == open.key.room_id)
                        {
                            running.account.unfollow_room(&open.key.room_id);
                        }
                        running.keep_warm(open.key, open.timeline);
                    }
                }
                Ok(())
            }
            Ok(Outcome::LoggedOut(id)) => {
                self.end_widget_runs(&id);
                #[cfg(feature = "native-media")]
                self.forget_calls(&id);
                self.accounts.remove(&id);
                self.sink.emit(Event::AccountRemoved(id));
                Ok(())
            }
            Err(error) => {
                if let Some(request) = request {
                    self.forget_opening(request);
                    #[cfg(feature = "command-line")]
                    self.sink.emit(Event::CommandError(request, crate::terminal::error_kind(&error)));
                }
                Err(error.to_string())
            }
        };
        if let Some(request) = request {
            self.sink.emit(Event::Finished { request, result });
        }
    }
}

/// Lists a room's widgets again, from the server, whenever sync brings a change to them, so the
/// room offers the meetings pinned to it as they come and go (under sliding sync the store
/// keeps no widget state from timelines).
fn follow_widgets(account: Account, sink: Arc<dyn EventSink>) -> AbortHandle {
    let mut synced = account.client().subscribe_to_all_room_updates();
    tokio::spawn(async move {
        loop {
            let rooms = match synced.recv().await {
                Ok(updates) => {
                    // A redaction may have removed a widget: the rooms on screen are read again
                    // (others when next opened).
                    let (mut rooms, redacted) = crate::widgets::changed_rooms(&updates);
                    let followed = account.followed_rooms();
                    for room in redacted {
                        if followed.contains(&room) {
                            rooms.push(room);
                        } else {
                            // Read from the server when next opened.
                            account.forget_extra_state(&room);
                        }
                    }
                    rooms
                }
                // Missed some: the rooms on screen are read again.
                // Missed some (a redaction elsewhere among them): every room's cached state is
                // read again when it opens, the rooms on screen at once.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    account.forget_all_extra_state();
                    account.followed_rooms()
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            for room_id in rooms {
                let freshness = crate::widgets::Freshness::Server;
                if let Ok(widgets) = account.room_widgets(&room_id, freshness).await {
                    sink.emit(Event::Widgets { account: account.id().clone(), room_id, widgets });
                }
            }
        }
    })
    .abort_handle()
}

/// Syncs one account until it fails or is aborted, reporting rooms, new messages and
/// notifications. The first response only catches up, so it notifies about nothing.
/// `own_refresh` asks for the user's own presence to be read again.
///
/// Where the server offers sliding sync (MSC4186), rooms come through the SDK's
/// `RoomListService`, a few at a time and newest first, so a big account shows its rooms at
/// once; a long-poll sync that asks for no rooms carries what sliding sync can't: to-device
/// messages and keys, presence and account data. Elsewhere one long-poll sync brings it all.
async fn sync_loop(
    account: Account,
    sink: Arc<dyn EventSink>,
    feed: RoomFeed,
    own_refresh: Arc<tokio::sync::Notify>,
    sliding_sync: bool,
    watch_ready: Option<Arc<WatchSync>>,
) {
    // Waits between failed syncs: from a few seconds, doubling, to a minute.
    const FIRST_RETRY: std::time::Duration = std::time::Duration::from_secs(2);
    const LAST_RETRY: std::time::Duration = std::time::Duration::from_secs(60);
    let progress = Arc::new(SyncProgress { watch_ready, ..SyncProgress::default() });
    // Room changes the SDK works out on its own (unread counts, each room's latest event) are
    // heard from before the first read: a latest event worked out for a room the read below
    // registers isn't missed. Missed ones (the channel overflowing) re-read every room. The
    // listener hears through failed syncs and the waits between them (a message sent offline
    // changes its room's latest event), so a retry needn't read every room again.
    let updates = account.client().room_info_notable_update_receiver();
    // What is on disk shows before the first sync answers.
    feed.refresh(None).await;
    let (wanted_members, members) = fetch_space_members(feed.clone());
    let _members = AbortOnDrop(members);
    let _unread = AbortOnDrop(follow_notable_updates(feed.clone(), updates));
    let _widgets = AbortOnDrop(follow_widgets(account.clone(), Arc::clone(&sink)));
    // Until the first long-poll sync has caught up, the room list's sliding sync marks no one
    // present either (`status::sync_presence`).
    let _ = account.client().set_presence(PresenceState::Offline, None, false).await;
    let sliding = Arc::new(std::sync::atomic::AtomicBool::new(false));
    notify_from_sliding_sync(&account, &sink, &sliding).await;
    // Once the loop ends (the account signed out), notifications still waiting for their key
    // stay unshown.
    let _stopped = ClearOnDrop(Arc::clone(&sliding));
    // A server that names sliding sync but whose endpoint refuses it (a proxy in front, a
    // rollout half done) gets the long-poll sync for every room, until the app starts again.
    let mut refused_sliding = false;
    // Once sliding sync was refused, the first long-poll sync for every room brings each room's
    // whole state: the room-less sync before moved the token past changes sliding sync never
    // delivered.
    let mut full_rooms = if sliding_sync { SyncedRooms::All } else { SyncedRooms::Terminal };
    let mut retry = FIRST_RETRY;
    loop {
        // Spaces arrive from the store now and from syncs later (all of them, after a fresh
        // sign-in); asking again after a failure retries those whose fetch failed.
        for room in account.client().joined_rooms() {
            want_members(&wanted_members, &room);
        }
        let started = std::time::Instant::now();
        // Asked again after every failure: a server may gain (or lose) sliding sync.
        let rooms_sliding = sliding_sync && !refused_sliding && supports_sliding_sync(account.client()).await;
        sliding.store(rooms_sliding, Ordering::Relaxed);
        let mut rooms = if rooms_sliding { SyncedRooms::None } else { full_rooms };
        let long_poll = sync_until_error(&account, &sink, &feed, &progress, &own_refresh, &wanted_members, &mut rooms);
        let result = if rooms_sliding {
            tokio::select! {
                result = long_poll => result,
                result = sync_room_list(&account, &sink, &feed, &wanted_members) => {
                    refused_sliding = result.as_ref().is_err_and(endpoint_refused);
                    result
                }
            }
        } else {
            long_poll.await
        };
        full_rooms = match (rooms_sliding, refused_sliding) {
            (true, true) => SyncedRooms::Reconcile,
            (true, false) => SyncedRooms::All,
            // Still `Reconcile` until its first response came.
            (false, _) => rooms,
        };
        account.set_room_list(None).await;
        // Sticky memberships sent meanwhile may have come only in the sync that failed: the
        // rooms on screen (a call's among them) ask again now, the others when next opened.
        #[cfg(feature = "native-media")]
        account.sticky.fetch_again();
        #[cfg(feature = "native-media")]
        for room_id in account.followed_rooms() {
            if let Some(room) = room_of(&account, &room_id) {
                let sticky = account.sticky.clone();
                tokio::spawn(async move {
                    // The connection may still be down: tried again until it answers.
                    let http = room.client().http_client().clone();
                    let mut wait = std::time::Duration::from_secs(5);
                    while !sticky.fetch(&room, &http).await && wait < std::time::Duration::from_secs(600) {
                        tokio::time::sleep(wait).await;
                        wait *= 2;
                    }
                });
            }
        }
        let Err(error) = result else {
            return;
        };
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

type WantedMembers = tokio::sync::mpsc::UnboundedSender<matrix_sdk::ruma::OwnedRoomId>;

/// Asks for a space's full member list, unless it is loaded already.
fn want_members(wanted: &WantedMembers, room: &matrix_sdk::Room) {
    if room.is_space() && !room.are_members_synced() {
        let _ = wanted.send(room.room_id().to_owned());
    }
}

/// When failed fetches may be tried again: not before a wait that starts at
/// [`Backoff::FIRST_WAIT`] and doubles with each failure in a row, up to [`Backoff::LAST_WAIT`].
/// A success forgets the failures.
#[derive(Debug)]
struct Backoff<K> {
    /// How often each failed in a row, and when it may be tried again.
    failed: HashMap<K, (u32, std::time::Instant)>,
}

impl<K: std::hash::Hash + Eq> Backoff<K> {
    const FIRST_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
    const LAST_WAIT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

    fn new() -> Self {
        Self { failed: HashMap::new() }
    }

    /// Whether `key` may be tried at `now`: it never failed, or its wait is over.
    fn may_try(&self, key: &K, now: std::time::Instant) -> bool {
        self.failed.get(key).is_none_or(|(_, at)| *at <= now)
    }

    fn failed(&mut self, key: K, now: std::time::Instant) {
        let attempts = self.failed.get(&key).map_or(0, |(attempts, _)| *attempts).saturating_add(1);
        let wait = Self::FIRST_WAIT.saturating_mul(1 << (attempts - 1).min(16)).min(Self::LAST_WAIT);
        self.failed.insert(key, (attempts, now + wait));
    }

    fn succeeded(&mut self, key: &K) {
        self.failed.remove(key);
    }
}

/// A space's people come from its full member list, which lazy loading leaves out: each is
/// fetched once, one after another, beside the sync rather than in its way. A fetch that fails
/// (a space that forbids it, a server error) is tried again when the space is next asked for,
/// but not before its [`Backoff`] wait: 30 s, doubling to an hour.
fn fetch_space_members(feed: RoomFeed) -> (WantedMembers, AbortHandle) {
    let (wanted, mut spaces) = tokio::sync::mpsc::unbounded_channel::<matrix_sdk::ruma::OwnedRoomId>();
    let task = tokio::spawn(async move {
        let mut fetched = HashSet::new();
        let mut backoff = Backoff::new();
        while let Some(first) = spaces.recv().await {
            let mut batch = vec![first];
            while let Ok(more) = spaces.try_recv() {
                if !batch.contains(&more) {
                    batch.push(more);
                }
            }
            let mut synced = HashSet::new();
            for room_id in batch {
                if fetched.contains(&room_id) || !backoff.may_try(&room_id, std::time::Instant::now()) {
                    continue;
                }
                let Some(space) = feed.account.client().get_room(&room_id) else { continue };
                if space.are_members_synced() {
                    continue;
                }
                if space.sync_members().await.is_ok() {
                    backoff.succeeded(&room_id);
                    synced.insert(room_id.to_string());
                    fetched.insert(room_id);
                } else {
                    backoff.failed(room_id, std::time::Instant::now());
                }
            }
            if !synced.is_empty() {
                feed.refresh(Some(&synced)).await;
            }
        }
    });
    (wanted, task.abort_handle())
}

/// Unread counts and each room's latest event are worked out after a sync's callback runs,
/// and receipts sent from here change them too: the SDK says which rooms, and those are read
/// again.
fn follow_notable_updates(
    feed: RoomFeed,
    mut updates: tokio::sync::broadcast::Receiver<matrix_sdk_base::RoomInfoNotableUpdate>,
) -> AbortHandle {
    use matrix_sdk_base::RoomInfoNotableUpdateReasons as Reasons;
    let task = tokio::spawn(async move {
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
    });
    task.abort_handle()
}

/// Syncs until the first error.
#[derive(Debug)]
pub(crate) struct WatchSync {
    pub ready: tokio::sync::Notify,
    pub updates: tokio::sync::broadcast::Sender<Arc<matrix_sdk::sync::RoomUpdates>>,
    pub checkpoints: std::sync::Mutex<HashMap<matrix_sdk::ruma::OwnedRoomId, matrix_sdk::ruma::OwnedEventId>>,
}

impl WatchSync {
    fn new() -> Self {
        Self {
            ready: tokio::sync::Notify::new(),
            updates: tokio::sync::broadcast::channel(32).0,
            checkpoints: std::sync::Mutex::default(),
        }
    }
}

#[derive(Default)]
struct SyncProgress {
    caught_up: std::sync::atomic::AtomicBool,
    terminal_ready: std::sync::atomic::AtomicBool,
    watch_ready: Option<Arc<WatchSync>>,
}

async fn sync_until_error(
    account: &Account,
    sink: &Arc<dyn EventSink>,
    feed: &RoomFeed,
    progress: &Arc<SyncProgress>,
    own_refresh: &Arc<tokio::sync::Notify>,
    wanted_members: &WantedMembers,
    rooms: &mut SyncedRooms,
) -> Result<(), matrix_sdk::Error> {
    // Members load as rooms need them (lazy loading): a first sync doesn't download every member
    // of every room.
    let mut filter = matrix_sdk::ruma::api::client::filter::FilterDefinition::with_lazy_loading();
    if *rooms == SyncedRooms::None {
        filter.room.rooms = Some(Vec::new());
    }
    let settings = SyncSettings::default()
        .timeout(std::time::Duration::from_secs(30))
        .filter(matrix_sdk::ruma::api::client::sync::sync_events::v3::Filter::FilterDefinition(filter));
    let terminal = *rooms == SyncedRooms::Terminal;
    let on_response = |response: matrix_sdk::sync::SyncResponse, sent: PresenceState| {
        let (account, sink, progress, feed) = (account.clone(), Arc::clone(sink), Arc::clone(progress), feed.clone());
        async move {
            let changed = report_room_updates(&account, &sink, wanted_members, &response.rooms).await;
            let already_synced = progress.caught_up.swap(true, Ordering::Relaxed);
            if already_synced {
                for (room_id, notifications) in &response.notifications {
                    let Some(room) = account.client().get_room(room_id) else { continue };
                    for notification in notifications {
                        if let Some(n) = convert::notification(account.id(), &room, notification).await {
                            sink.emit(Event::Notification(n));
                        }
                    }
                }
            }
            // Only the rooms this sync changed are read again; direct chats, push rules and the
            // note to self's mark can change any room, so they read them all.
            let all = crate::note_to_self::changed_in(&response.account_data)
                || response.account_data.iter().any(|event| {
                    matches!(
                        event.get_field::<String>("type").ok().flatten().as_deref(),
                        Some("m.direct" | "m.push_rules")
                    )
                });
            // Keywords are push rules too, changed here or on another session.
            if response
                .account_data
                .iter()
                .any(|event| event.get_field::<String>("type").ok().flatten().as_deref() == Some("m.push_rules"))
            {
                sink.emit(Event::Keywords { account: account.id().clone(), keywords: account.keywords().await });
            }
            if crate::sidebar::changed_in(&response.account_data)
                && let Ok(layout) = account.sidebar_layout().await
            {
                sink.emit(Event::SidebarLayout { account: account.id().clone(), layout });
            }
            if crate::safety::changed_in(&response.account_data)
                && let Some(safety) = account.synced_safety().await
            {
                sink.emit(Event::Safety { account: account.id().clone(), safety });
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
            account.follow_status(&response.account_data, (!terminal).then_some(&sent), report).await;
            account.follow_offline_device(&response.account_data).await;
            feed.refresh((!all).then_some(&changed)).await;
            if let Some(watch) = &progress.watch_ready {
                if progress.terminal_ready.load(Ordering::Relaxed) {
                    let _ = watch.updates.send(Arc::new(response.rooms));
                } else if let Ok(mut checkpoints) = watch.checkpoints.lock() {
                    // A limited first live batch can backfill to the exact event seen during
                    // catch-up, even when the SDK is still computing the room's latest event.
                    for (room, update) in &response.rooms.joined {
                        if let Some(event) = update.timeline.events.iter().rev().find_map(|event| event.event_id()) {
                            checkpoints.insert(room.clone(), event.to_owned());
                        }
                    }
                }
            }
            if terminal && response.to_device.is_empty() && !progress.terminal_ready.swap(true, Ordering::Relaxed) {
                // Let a watcher resolve its room and register its receiver before another sync.
                sink.emit(Event::SyncReady(account.id().clone()));
                if let Some(watch_ready) = &progress.watch_ready {
                    watch_ready.ready.notified().await;
                }
            }
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
        let sent = if terminal {
            PresenceState::Offline
        } else {
            crate::status::sync_presence(account.status().await, progress.caught_up.load(Ordering::Relaxed))
        };
        settings = settings.set_presence(sent.clone());
        // The room list's sliding sync announces the same.
        let _ = account.client().set_presence(sent.clone(), None, false).await;
        let whole_state = *rooms == SyncedRooms::Reconcile;
        let response = match account
            .sync_once_locked(
                if terminal && !progress.terminal_ready.load(Ordering::Relaxed) {
                    settings.clone().timeout(std::time::Duration::ZERO)
                } else {
                    settings.clone()
                }
                .full_state(whole_state),
            )
            .await
        {
            Ok(response) => response,
            Err(error) => {
                account.sync_failed(&sent).await;
                return Err(error);
            }
        };
        if whole_state {
            *rooms = SyncedRooms::All;
        }
        settings = settings.token(response.next_batch.clone());
        if on_response(response, sent).await == LoopCtrl::Break {
            return Ok(());
        }
    }
}

/// Which rooms the long-poll sync asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncedRooms {
    /// Every room: the server has no sliding sync.
    All,
    /// All rooms, with a zero-timeout first request for one-shot clients.
    Terminal,
    /// None: the room list's sliding sync brings them.
    None,
    /// Every room, the first response with each room's whole state: after the room list's
    /// sliding sync was refused, as the room-less sync moved the token past what it missed.
    Reconcile,
}

/// Whether the server offers sliding sync (MSC4186), which the SDK then uses for the room list.
async fn supports_sliding_sync(client: &matrix_sdk::Client) -> bool {
    use matrix_sdk::sliding_sync::Version;
    let native =
        client.available_sliding_sync_versions().await.into_iter().any(|version| matches!(version, Version::Native));
    client.set_sliding_sync_version(if native { Version::Native } else { Version::None });
    native
}

/// What a sync brought for rooms, reported: members to read again, spaces whose members to
/// fetch, each room's latest message. Returns the rooms whose summary may have changed.
async fn report_room_updates(
    account: &Account,
    sink: &Arc<dyn EventSink>,
    wanted_members: &WantedMembers,
    rooms: &matrix_sdk::sync::RoomUpdates,
) -> HashSet<String> {
    for (room_id, update) in &rooms.joined {
        let Some(room) = account.client().get_room(room_id) else { continue };
        if members_changed(update) {
            sink.emit(Event::MembersInvalidated { account: account.id().clone(), room_id: room_id.to_string() });
        }
        want_members(wanted_members, &room);
        if let Some(message) = convert::latest_message(&room, &update.timeline.events).await {
            sink.emit(Event::LatestMessage {
                account: account.id().clone(),
                room_id: room_id.to_string(),
                message: Box::new(message),
            });
        }
    }
    rooms
        .joined
        .iter()
        .filter(|(_, update)| summary_may_change(update))
        .map(|(room_id, _)| room_id)
        .chain(rooms.left.keys())
        .chain(rooms.invited.keys())
        .map(ToString::to_string)
        .collect()
}

/// Marks as left the rooms this device holds as joined that the server no longer lists: left
/// on another device while the room list's sliding sync couldn't say. Only rooms held as
/// joined before asking, and still when marked, count: one joined meanwhile stays, and one
/// whose membership changed meanwhile (an invitation back) keeps it. The rooms marked left,
/// `None` when the server couldn't say.
async fn leave_rooms_left_elsewhere(account: &Account) -> Option<HashSet<String>> {
    use matrix_sdk::ruma::api::client::membership::joined_rooms;
    use matrix_sdk_base::RoomInfoNotableUpdateReasons as Reasons;
    // What syncs bring while the server is asked is newer than its answer: a room they report
    // as joined (a rejoin) or whose membership they change (an invitation back) keeps that.
    let mut updates = account.client().room_info_notable_update_receiver();
    let mut synced = account.client().subscribe_to_all_room_updates();
    let held: Vec<matrix_sdk::Room> = account.client().joined_rooms();
    let joined = account.client().send(joined_rooms::v3::Request::new()).await.ok()?.joined_rooms;
    let mut changed = HashSet::new();
    loop {
        match synced.try_recv() {
            Ok(rooms) => changed.extend(rooms.joined.into_keys()),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => return None,
            Err(_) => break,
        }
    }
    loop {
        match updates.try_recv() {
            Ok(update) if update.reasons.contains(Reasons::MEMBERSHIP) => {
                changed.insert(update.room_id);
            }
            Ok(_) => {}
            // Missed some: any room may have changed, so ask again later.
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => return None,
            Err(_) => break,
        }
    }
    let mut left = HashSet::new();
    let stale = held
        .into_iter()
        .filter(|room| !joined.iter().any(|id| id == room.room_id()) && !changed.contains(room.room_id()));
    for room in stale {
        let marked = room
            .update_and_save_room_info(|mut info| {
                if info.state() != matrix_sdk_base::RoomState::Joined {
                    return (info, Reasons::NONE);
                }
                info.mark_as_left();
                (info, Reasons::MEMBERSHIP)
            })
            .await;
        if marked.is_ok() && room.state() == matrix_sdk_base::RoomState::Left {
            left.insert(room.room_id().to_string());
        }
    }
    Some(left)
}

/// Whether the server refused an endpoint it named: it doesn't know it (`M_UNRECOGNIZED`, 404).
fn endpoint_refused(error: &matrix_sdk::Error) -> bool {
    use matrix_sdk::ruma::api::error::ErrorKind;
    let matrix_sdk::Error::Http(http) = error else { return false };
    matches!(http.client_api_error_kind(), Some(ErrorKind::Unrecognized))
        || http.as_client_api_error().is_some_and(|api| api.status_code.as_u16() == 404)
}

/// How many of each room's latest events the room list's sliding sync brings.
const ROOM_LIST_EVENTS: u32 = 10;

/// Runs the room list's sliding sync until it fails, reporting the rooms each response
/// changed. It only reads rooms into the store: the room list itself is read from there.
async fn sync_room_list(
    account: &Account,
    sink: &Arc<dyn EventSink>,
    feed: &RoomFeed,
    wanted_members: &WantedMembers,
) -> Result<(), matrix_sdk::Error> {
    use futures_util::StreamExt;
    use tokio::sync::broadcast::error::RecvError;
    // Each room comes with its last few events, as many as a long-poll sync's: a burst between
    // two responses (a call invite, then a message) arrives whole, for the call handlers and
    // notifications, rather than as the last event and a gap.
    let service =
        matrix_sdk_ui::RoomListService::new_with(account.client().clone(), true, "room-list", ROOM_LIST_EVENTS)
            .await
            .map_err(room_list_error)?;
    let service = Arc::new(service);
    // Heard from before the first request, so no response goes unread.
    let mut updates = account.client().subscribe_to_all_room_updates();
    account.set_room_list(Some(Arc::clone(&service))).await;
    // A sliding sync that starts afresh (the first after an upgrade, or once the server forgot
    // its position) lists only the rooms joined now, never the ones left meanwhile.
    let reconcile = {
        let feed = feed.clone();
        tokio::spawn(async move {
            // Asked again while the sync runs until the server answers (a network handoff).
            let mut wait = std::time::Duration::from_secs(5);
            let left = loop {
                if let Some(left) = leave_rooms_left_elsewhere(&feed.account).await {
                    break left;
                }
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(std::time::Duration::from_secs(300));
            };
            if !left.is_empty() {
                feed.refresh(Some(&left)).await;
            }
        })
    };
    let _reconcile = AbortOnDrop(reconcile.abort_handle());
    let report = async {
        loop {
            match updates.recv().await {
                Ok(rooms) => {
                    let changed = report_room_updates(account, sink, wanted_members, &rooms).await;
                    if !changed.is_empty() {
                        feed.refresh(Some(&changed)).await;
                    }
                }
                // Missed updates: any room may have changed.
                Err(RecvError::Lagged(_)) => feed.refresh(None).await,
                Err(RecvError::Closed) => std::future::pending().await,
            }
        }
    };
    let sync = async {
        let stream = service.sync();
        futures_util::pin_mut!(stream);
        while let Some(result) = stream.next().await {
            result.map_err(room_list_error)?;
        }
        Ok(())
    };
    tokio::select! {
        result = sync => result,
        () = report => Ok(()),
    }
}

fn room_list_error(error: matrix_sdk_ui::room_list_service::Error) -> matrix_sdk::Error {
    match error {
        matrix_sdk_ui::room_list_service::Error::SlidingSync(error) => error,
        other => matrix_sdk::Error::UnknownError(Box::new(other)),
    }
}

/// Notifications from the room list's sliding sync, while it brings the rooms (`sliding`).
/// Only for events that came after syncing started, once each: a room the list reaches late,
/// or one opened (which asks for its recent events again), brings events already seen.
async fn notify_from_sliding_sync(
    account: &Account,
    sink: &Arc<dyn EventSink>,
    sliding: &Arc<std::sync::atomic::AtomicBool>,
) {
    let seen = Arc::new(std::sync::Mutex::new(Notified::since(now_ms())));
    let (account_id, sink, sliding) = (account.id().clone(), Arc::clone(sink), Arc::clone(sliding));
    account
        .client()
        .register_notification_handler(move |notification, room, _| {
            let (account_id, sink, sliding, seen) =
                (account_id.clone(), Arc::clone(&sink), Arc::clone(&sliding), Arc::clone(&seen));
            async move {
                if !sliding.load(Ordering::Relaxed) {
                    return;
                }
                // Beside the sync, which waits for each handler: an encrypted one may wait for
                // its key.
                tokio::spawn(async move {
                    let Some(notification) = decrypted_soon(&room, notification).await else { return };
                    let Some(shown) = convert::notification(&account_id, &room, &notification).await else { return };
                    let matrix_sdk::deserialized_responses::RawAnySyncOrStrippedTimelineEvent::Sync(raw) =
                        &notification.event
                    else {
                        return;
                    };
                    let sent = raw.get_field::<u64>("origin_server_ts").ok().flatten().unwrap_or(0);
                    // Not once the account stopped syncing (signed out) while this waited.
                    if sliding.load(Ordering::Relaxed)
                        && seen.lock().is_ok_and(|mut seen| seen.first_time(&shown.event_id, sent))
                    {
                        sink.emit(Event::Notification(shown));
                    }
                });
            }
        })
        .await;
}

/// A notification as it reads decrypted. Its key may still be on its way, through the long-poll
/// sync beside the room list's: tried again for a few seconds, and its push rules worked out
/// again for what it says. `None` when it stays unreadable.
async fn decrypted_soon(
    room: &matrix_sdk::Room,
    notification: matrix_sdk::sync::Notification,
) -> Option<matrix_sdk::sync::Notification> {
    use matrix_sdk::deserialized_responses::RawAnySyncOrStrippedTimelineEvent as Raw;
    let Raw::Sync(raw) = &notification.event else { return Some(notification) };
    if raw.get_field::<String>("type").ok().flatten().as_deref() != Some("m.room.encrypted") {
        return Some(notification);
    }
    for attempt in 0..10 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        // Without its key the SDK answers with the event still unreadable, not an error.
        if let Ok(event) = room.decrypt_event(raw.cast_ref_unchecked(), None).await
            && event.encryption_info().is_some()
        {
            let decrypted = event.into_raw();
            let actions = match room.push_context().await {
                Ok(Some(context)) => context.for_event(&decrypted).await,
                _ => notification.actions.clone(),
            };
            return Some(matrix_sdk::sync::Notification { actions, event: Raw::Sync(decrypted) });
        }
    }
    None
}

/// Which events have notified already.
#[derive(Debug)]
struct Notified {
    /// When syncing started: anything sent before is old news.
    since_ms: u64,
    /// The latest notified, oldest first.
    recent: std::collections::VecDeque<String>,
}

impl Notified {
    /// How many notified events are remembered: far more than one room's catch-up brings.
    const KEPT: usize = 256;

    fn since(since_ms: u64) -> Self {
        Self { since_ms, recent: std::collections::VecDeque::new() }
    }

    /// Whether an event sent at `sent_ms` should notify, remembering that it did.
    fn first_time(&mut self, event_id: &str, sent_ms: u64) -> bool {
        if sent_ms < self.since_ms || self.recent.iter().any(|seen| seen == event_id) {
            return false;
        }
        if self.recent.len() == Self::KEPT {
            self.recent.pop_front();
        }
        self.recent.push_back(event_id.to_owned());
        true
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
    account.follow_room(&key.room_id).await;
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
    let reset =
        crate::timeline::diff(eyeball_im::VectorDiff::Reset { values: items.clone() }, &me, timeline.room()).await;
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
        items,
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
    // A room's timeline mirrors its items to tell whether its read boundary is among them; a
    // thread's has no read boundary, and nothing to mirror.
    let mut boundary = read_boundary.clone().map(|id| {
        BoundaryMirror::new(items, move |item: &Arc<matrix_sdk_ui::timeline::TimelineItem>| {
            item.as_event().and_then(|event| event.event_id()).is_some_and(|event| event.as_str() == id)
        })
    });
    let mut read_boundary_loaded = boundary.as_ref().is_none_or(BoundaryMirror::holds);
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
        let timeline_room = timeline.room().clone();
        let room = key.thread.is_none().then(|| timeline_room.clone());
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
                        if let Some(boundary) = &mut boundary {
                            for diff in &batch {
                                boundary.apply(diff.clone());
                            }
                            let loaded = boundary.holds();
                            if loaded != read_boundary_loaded {
                                read_boundary_loaded = loaded;
                                sink.emit(Event::ReadBoundary { account: id.clone(), room_id: key.room_id.clone(), event_id: read_boundary.clone(), loaded });
                            }
                        }
                        let mut diffs = Vec::with_capacity(batch.len());
                        for diff in batch {
                            diffs.push(crate::timeline::diff(diff, &me, &timeline_room).await);
                        }
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

/// A timeline's items as its diffs leave them, and whether one of them is the item `is` looks
/// for. Kept diff by diff: only the items a diff adds or takes are looked at, and all of them
/// again only when the one looked for goes.
struct BoundaryMirror<T, F> {
    items: eyeball_im::Vector<T>,
    is: F,
    holds: bool,
}

impl<T: Clone, F: Fn(&T) -> bool> BoundaryMirror<T, F> {
    fn new(items: eyeball_im::Vector<T>, is: F) -> Self {
        let holds = items.iter().any(&is);
        Self { items, is, holds }
    }

    /// Whether the items hold the one looked for.
    fn holds(&self) -> bool {
        self.holds
    }

    fn apply(&mut self, diff: eyeball_im::VectorDiff<T>) {
        use eyeball_im::VectorDiff;
        let is = &self.is;
        // Whether a diff that takes items takes the one looked for: then the rest are looked at.
        let takes = |taken: Option<&T>| taken.is_some_and(is);
        let rescan = match &diff {
            VectorDiff::Append { values } => {
                self.holds |= values.iter().any(is);
                false
            }
            VectorDiff::Reset { values } => {
                self.holds = values.iter().any(is);
                false
            }
            VectorDiff::PushFront { value } | VectorDiff::PushBack { value } | VectorDiff::Insert { value, .. } => {
                self.holds |= is(value);
                false
            }
            VectorDiff::Set { index, value } => {
                let taken = self.holds && takes(self.items.get(*index));
                self.holds |= is(value);
                taken && !is(value)
            }
            VectorDiff::Clear => {
                self.holds = false;
                false
            }
            VectorDiff::PopFront => self.holds && takes(self.items.front()),
            VectorDiff::PopBack => self.holds && takes(self.items.back()),
            VectorDiff::Remove { index } => self.holds && takes(self.items.get(*index)),
            VectorDiff::Truncate { length } => self.holds && self.items.iter().skip(*length).any(is),
        };
        diff.apply(&mut self.items);
        if rescan {
            self.holds = self.items.iter().any(&self.is);
        }
    }
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

/// `content` in `thread` and answering `reply_to`, as the SDK relates the room messages it
/// sends: it relates no other kind (stickers, polls) itself. In a thread without a reply,
/// `latest` (the thread's latest message) is what clients without threads see it answer.
fn related(
    content: matrix_sdk::ruma::events::AnyMessageLikeEventContent,
    thread: Option<&str>,
    reply_to: Option<&str>,
    latest: Option<&str>,
) -> Result<matrix_sdk::ruma::events::AnyMessageLikeEventContent, Error> {
    use matrix_sdk::ruma::events::{AnyMessageLikeEventContent, EventContentFromType, MessageLikeEventContent};
    if thread.is_none() && reply_to.is_none() {
        return Ok(content);
    }
    let event_type = content.event_type().to_string();
    let mut json = serde_json::to_value(&content).map_err(|e| Error::Storage(e.into()))?;
    crate::scheduled::relate(&mut json, thread, reply_to);
    if let (Some(_), None, Some(latest)) = (thread, reply_to, latest) {
        json["m.relates_to"]["m.in_reply_to"]["event_id"] = latest.into();
    }
    let raw = serde_json::value::to_raw_value(&json).map_err(|e| Error::Storage(e.into()))?;
    AnyMessageLikeEventContent::from_parts(&event_type, &raw).map_err(|e| Error::Storage(e.into()))
}

/// Sends through an open timeline: as a reply when `reply_to` is set, into the thread when the
/// timeline is `thread`'s.
async fn send_in(
    timeline: &matrix_sdk_ui::timeline::Timeline,
    thread: Option<&str>,
    reply_to: Option<String>,
    message: &Outgoing,
) -> Result<(), Error> {
    if let Some(content) = crate::outgoing::room_message(message) {
        match reply_to {
            Some(event_id) => {
                timeline.send_reply(content, matrix_sdk::ruma::EventId::parse(&event_id)?).await?;
            }
            None => {
                let content: matrix_sdk::ruma::events::room::message::RoomMessageEventContent = content.into();
                timeline.send(content.into()).await?;
            }
        }
        return Ok(());
    }
    match message {
        Outgoing::Poll { .. } | Outgoing::Sticker(_) => {
            if let Some(content) = crate::outgoing::event(message)? {
                // In a thread without a reply, the fallback for clients without threads answers
                // the thread's latest message, as the SDK's own thread messages do.
                let latest = match (thread, &reply_to) {
                    (Some(_), None) => timeline.latest_event_id().await.map(|id| id.to_string()),
                    _ => None,
                };
                timeline.send(related(content, thread, reply_to.as_deref(), latest.as_deref())?).await?;
            }
        }
        Outgoing::Text { .. } | Outgoing::Emote { .. } | Outgoing::Location { .. } | Outgoing::Gif(_) => {}
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
            // An attachment goes straight to the server, past the room's send queue: the words
            // queued before it go out first.
            queue_sent(timeline.room()).await;
            timeline.send_attachment(source, upload.mime(), config).await?;
        }
    }
    Ok(())
}

/// Waits until the room's send queue has sent what it holds, as long as it can: a queue the SDK
/// stopped (after a failure) or one held up by a message it couldn't send isn't waited for. It
/// looks again on every change to the queue, and every second, as the look and the listening
/// start apart.
async fn queue_sent(room: &matrix_sdk::Room) {
    use matrix_sdk::send_queue::LocalEchoContent;
    use tokio::sync::broadcast::error::RecvError;

    let queue = room.send_queue();
    loop {
        let Ok((echoes, mut changes)) = queue.subscribe().await else { return };
        // A message or redaction the SDK couldn't send holds the whole queue up: a wedge.
        let wedged = |echo: &matrix_sdk::send_queue::LocalEcho| match &echo.content {
            LocalEchoContent::Event { send_error, .. } | LocalEchoContent::Redaction { send_error, .. } => {
                send_error.is_some()
            }
            LocalEchoContent::React { .. } => false,
        };
        let stuck = echoes.iter().any(wedged);
        let waiting =
            echoes.iter().any(|echo| matches!(echo.content, LocalEchoContent::Event { send_error: None, .. }));
        if !waiting || stuck || !queue.is_enabled() || !room.client().send_queue().is_enabled() {
            return;
        }
        if let Ok(Err(RecvError::Closed)) =
            tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv()).await
        {
            return;
        }
    }
}

/// Reports a session the server no longer accepts. Refreshed tokens are saved by the client's
/// session callback inside the refresh's cross-process lock (`account::build_client`), and only
/// there: a later save from here could overwrite tokens another process refreshed since.
async fn watch_session(account: Account, sink: Arc<dyn EventSink>, terminal_sessions: Option<Sessions>) {
    use tokio::sync::broadcast::error::RecvError;

    let mut changes = account.session_changes();
    loop {
        match changes.recv().await {
            Ok(SessionChange::UnknownToken(token)) => {
                let mut kept = token.soft_logout;
                if kept
                    && let Some(sessions) = &terminal_sessions
                    && let Err(error) = crate::account::set_signed_out(sessions, account.id()).await
                {
                    kept = false;
                    sink.emit(Event::AccountError { account: account.id().clone(), error: error.to_string() });
                }
                sink.emit(Event::SessionEnded { account: account.id().clone(), kept });
                if terminal_sessions.is_some() {
                    return;
                }
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
    fn a_failed_fetch_should_wait_before_it_is_tried_again() {
        let (mut backoff, now) = (Backoff::new(), std::time::Instant::now());
        backoff.failed("!space", now);
        let wait = Backoff::<&str>::FIRST_WAIT;
        let tries = (backoff.may_try(&"!space", now + wait / 2), backoff.may_try(&"!space", now + wait));
        assert_eq!(tries, (false, true));
    }

    #[test]
    fn each_failure_in_a_row_should_double_the_wait() {
        let (mut backoff, now) = (Backoff::new(), std::time::Instant::now());
        backoff.failed("!space", now);
        backoff.failed("!space", now);
        let wait = Backoff::<&str>::FIRST_WAIT * 2;
        let tries = (
            backoff.may_try(&"!space", now + wait - Backoff::<&str>::FIRST_WAIT / 2),
            backoff.may_try(&"!space", now + wait),
        );
        assert_eq!(tries, (false, true));
    }

    #[test]
    fn the_wait_should_stop_growing_at_an_hour() {
        let (mut backoff, now) = (Backoff::new(), std::time::Instant::now());
        for _ in 0..40 {
            backoff.failed("!space", now);
        }
        assert!(backoff.may_try(&"!space", now + Backoff::<&str>::LAST_WAIT));
    }

    #[test]
    fn a_success_should_forget_the_failures() {
        let (mut backoff, now) = (Backoff::new(), std::time::Instant::now());
        backoff.failed("!space", now);
        backoff.failed("!space", now);
        backoff.succeeded(&"!space");
        backoff.failed("!space", now);
        assert!(backoff.may_try(&"!space", now + Backoff::<&str>::FIRST_WAIT));
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

    fn sticker_json(thread: Option<&str>, reply_to: Option<&str>) -> serde_json::Value {
        let image = crate::PackImage {
            shortcode: "wave".into(),
            body: "wave".into(),
            url: "mxc://x.org/wave".into(),
            media: crate::MediaRef::opaque("mxc://x.org/wave"),
            emoticon: false,
            sticker: true,
            dimensions: Some((256, 256)),
            mimetype: Some("image/png".into()),
        };
        let content = crate::outgoing::event(&Outgoing::Sticker(image)).expect("content").expect("a sticker");
        serde_json::to_value(related(content, thread, reply_to, Some("$latest")).expect("related")).expect("json")
    }

    #[test]
    fn a_sticker_sent_as_a_reply_should_answer_its_message() {
        let json = sticker_json(None, Some("$q"));
        assert_eq!(json["m.relates_to"]["m.in_reply_to"]["event_id"], "$q");
        assert_eq!(json["url"], "mxc://x.org/wave");
    }

    #[test]
    fn a_sticker_sent_in_a_thread_should_join_it() {
        let json = sticker_json(Some("$root"), None);
        assert_eq!(json["m.relates_to"]["rel_type"], "m.thread");
        assert_eq!(json["m.relates_to"]["event_id"], "$root");
        // Clients without threads see it answer the thread's latest message.
        assert_eq!(json["m.relates_to"]["is_falling_back"], true);
        assert_eq!(json["m.relates_to"]["m.in_reply_to"]["event_id"], "$latest");
    }

    fn mirror(items: &[u32]) -> BoundaryMirror<u32, impl Fn(&u32) -> bool> {
        BoundaryMirror::new(items.iter().copied().collect(), |item: &u32| *item == 7)
    }

    #[test]
    fn a_mirror_should_find_a_boundary_an_append_brings() {
        let mut mirror = mirror(&[1, 2]);
        mirror.apply(eyeball_im::VectorDiff::Append { values: [3, 7].into_iter().collect() });
        assert!(mirror.holds());
    }

    #[test]
    fn a_mirror_should_lose_the_boundary_its_item_takes_with_it() {
        let mut mirror = mirror(&[1, 7, 2]);
        mirror.apply(eyeball_im::VectorDiff::Remove { index: 1 });
        assert!(!mirror.holds());
    }

    #[test]
    fn a_mirror_should_keep_the_boundary_while_another_item_shows_it() {
        let mut mirror = mirror(&[7, 1, 7]);
        mirror.apply(eyeball_im::VectorDiff::Set { index: 0, value: 2 });
        assert!(mirror.holds());
    }

    #[test]
    fn a_mirror_should_agree_with_a_full_scan_after_every_diff() {
        use eyeball_im::VectorDiff;
        let mut mirror = mirror(&[]);
        // A fixed pseudo-random walk through every kind of diff, the boundary (7) among few values.
        let mut seed = 0x2545_f491_u64;
        let mut next = |below: usize| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(seed >> 33).unwrap() % below.max(1)
        };
        for step in 0..5_000 {
            let len = mirror.items.len();
            let value = u32::try_from(next(9)).unwrap();
            let diff = match next(11) {
                0 => VectorDiff::Append { values: (0..next(3)).map(|_| value).collect() },
                1 if step % 50 == 0 => VectorDiff::Clear,
                2 => VectorDiff::PushFront { value },
                3 => VectorDiff::PushBack { value },
                4 if len > 0 => VectorDiff::PopFront,
                5 if len > 0 => VectorDiff::PopBack,
                6 => VectorDiff::Insert { index: next(len + 1), value },
                7 if len > 0 => VectorDiff::Set { index: next(len), value },
                8 if len > 0 => VectorDiff::Remove { index: next(len) },
                9 => VectorDiff::Truncate { length: next(len + 1) },
                10 if step % 100 == 0 => {
                    VectorDiff::Reset { values: (0..next(6)).map(|_| u32::try_from(next(9)).unwrap()).collect() }
                }
                _ => VectorDiff::PushBack { value },
            };
            mirror.apply(diff.clone());
            assert_eq!(mirror.holds(), mirror.items.contains(&7), "after {diff:?} at step {step}");
        }
    }

    #[tokio::test]
    async fn a_failed_open_should_no_longer_count_as_opening() {
        let account = Account::for_test("https://example.org").await;
        let sink: Arc<dyn EventSink> = Arc::new(|_| {});
        let (usage, worker) = crate::search::Usage::start(account.clone(), Arc::clone(&sink));
        let feed = RoomFeed { account: account.clone(), sink, cache: Arc::default() };
        let mut running = Running {
            #[cfg(feature = "command-line")]
            watch_ready: None,
            account,
            usage,
            background: vec![worker],
            flows: crate::security::Flows::default(),
            timelines: HashMap::new(),
            warm: Vec::new(),
            opening: HashMap::from([(TimelineKey::room("!room:example.org"), RequestId(3))]),
            keys_fetched: HashSet::new(),
            feed,
            handlers: Vec::new(),
            sends: HashMap::new(),
        };
        running.forget_opening(RequestId(3));
        assert!(running.opening.is_empty());
    }

    #[test]
    fn an_event_sent_before_syncing_started_should_not_notify() {
        let mut notified = Notified::since(1_000);
        assert!(!notified.first_time("$old", 999));
    }

    #[test]
    fn an_event_should_notify_only_the_first_time_it_arrives() {
        let mut notified = Notified::since(1_000);
        assert!(notified.first_time("$new", 1_500));
        assert!(!notified.first_time("$new", 1_500));
    }

    #[test]
    fn the_oldest_notified_event_should_be_forgotten_once_too_many_are_kept() {
        let mut notified = Notified::since(0);
        for n in 0..=Notified::KEPT {
            assert!(notified.first_time(&format!("${n}"), 1));
        }
        assert!(notified.first_time("$0", 1), "the first is forgotten");
        assert!(!notified.first_time(&format!("${}", Notified::KEPT), 1), "the latest is kept");
    }

    /// A homeserver with sliding sync that knows one room, `!slide:example.org`.
    fn sliding_server(requests: std::sync::mpsc::Sender<String>) -> crate::test_server::Server {
        crate::test_server::Server::new(move |request| {
            let line = request.lines().next().unwrap_or_default().to_owned();
            let _ = requests.send(line.clone());
            if line.contains("simplified_msc3575/sync") {
                return serde_json::json!({
                    "pos": "1",
                    "lists": {"all_rooms": {"count": 1}},
                    "rooms": {"!slide:example.org": {
                        "name": "Sliding",
                        "initial": true,
                        "required_state": [
                            {"type":"m.room.create","state_key":"","sender":"@a:example.org","event_id":"$create","origin_server_ts":0,"content":{"creator":"@a:example.org","room_version":"10"}},
                            {"type":"m.room.member","state_key":"@a:example.org","sender":"@a:example.org","event_id":"$join","origin_server_ts":0,"content":{"membership":"join"}}
                        ],
                        "timeline": [],
                        "joined_count": 1
                    }},
                    "extensions": {}
                });
            }
            if line.contains("/sync?") {
                return serde_json::json!({"next_batch": "s1"});
            }
            serde_json::json!({})
        })
    }

    #[tokio::test]
    async fn rooms_should_come_from_the_room_lists_sliding_sync() {
        let (requests, _) = std::sync::mpsc::channel();
        let server = sliding_server(requests);
        let account = Account::for_test(&server.url()).await;
        account.client().set_sliding_sync_version(matrix_sdk::sliding_sync::Version::Native);
        let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
        let sink: Arc<dyn EventSink> = Arc::new(move |event| {
            let _ = events.send(event);
        });
        let feed = RoomFeed { account: account.clone(), sink: Arc::clone(&sink), cache: Arc::default() };
        let (wanted, members) = fetch_space_members(feed.clone());
        let _members = AbortOnDrop(members);
        let sync = {
            let (account, sink, feed) = (account.clone(), Arc::clone(&sink), feed.clone());
            tokio::spawn(async move { sync_room_list(&account, &sink, &feed, &wanted).await })
        };
        let listed = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(event) = received.recv().await {
                if let Event::RoomsUpdated { list, .. } = event
                    && list.rooms.iter().any(|room| room.room_id == "!slide:example.org")
                {
                    return true;
                }
            }
            false
        })
        .await;
        sync.abort();
        assert_eq!(listed, Ok(true), "the room the sliding sync brought is listed");
    }

    #[tokio::test]
    async fn a_room_the_server_no_longer_lists_as_joined_should_be_left() {
        let server = crate::test_server::Server::new(|request| {
            if request.contains("/joined_rooms") {
                return serde_json::json!({"joined_rooms": []});
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        assert_eq!(account.client().joined_rooms().len(), 1);
        let left = leave_rooms_left_elsewhere(&account).await;
        assert_eq!(left, Some(HashSet::from(["!room:example.org".to_owned()])));
        assert!(account.client().joined_rooms().is_empty());
    }

    #[tokio::test]
    async fn a_room_whose_membership_changed_while_asking_should_keep_it() {
        // The server answers `/joined_rooms` only once the test let it, after a rejoin landed.
        let (asked, was_asked) = std::sync::mpsc::channel();
        let (answer, may_answer) = std::sync::mpsc::channel::<()>();
        let may_answer = std::sync::Mutex::new(may_answer);
        let server = crate::test_server::Server::new(move |request| {
            if request.contains("/joined_rooms") {
                let _ = asked.send(());
                let _ = may_answer.lock().map(|may| may.recv());
                return serde_json::json!({"joined_rooms": []});
            }
            crate::test_server::thread_response(request)
        });
        let account = Account::for_test(&server.url()).await;
        account.client().sync_once(SyncSettings::default().timeout(Duration::ZERO)).await.expect("sync room");
        let room = account.client().joined_rooms().pop().expect("the room");
        let reconcile = {
            let account = account.clone();
            tokio::spawn(async move { leave_rooms_left_elsewhere(&account).await })
        };
        tokio::task::spawn_blocking(move || was_asked.recv()).await.expect("asked").expect("asked");
        // A rejoin elsewhere, which the room list's sync applied while the server was asked.
        room.update_and_save_room_info(|info| (info, matrix_sdk_base::RoomInfoNotableUpdateReasons::MEMBERSHIP))
            .await
            .expect("rejoined");
        answer.send(()).expect("answer");
        assert_eq!(reconcile.await.expect("reconciled"), Some(HashSet::new()));
        assert_eq!(room.state(), matrix_sdk_base::RoomState::Joined);
    }

    #[tokio::test]
    async fn the_long_poll_beside_sliding_sync_should_ask_for_no_rooms() {
        let (requests, received) = std::sync::mpsc::channel();
        let server = sliding_server(requests);
        let account = Account::for_test(&server.url()).await;
        let sink: Arc<dyn EventSink> = Arc::new(|_| {});
        let feed = RoomFeed { account: account.clone(), sink: Arc::clone(&sink), cache: Arc::default() };
        let (wanted, members) = fetch_space_members(feed.clone());
        let _members = AbortOnDrop(members);
        let sync = {
            let account = account.clone();
            tokio::spawn(async move {
                let (caught_up, own) = (Arc::default(), Arc::default());
                sync_until_error(&account, &sink, &feed, &caught_up, &own, &wanted, &mut SyncedRooms::None).await
            })
        };
        let line = tokio::task::spawn_blocking(move || {
            received.iter().find(|line| line.contains("/sync?")).expect("a long-poll sync")
        })
        .await
        .expect("request");
        sync.abort();
        let decoded = url::form_urlencoded::parse(line.as_bytes())
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        assert!(decoded.contains(r#""rooms":[]"#), "{decoded}");
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
        start_as("engine", false)
    }

    /// An engine with a data directory of its own (`name`), scanning QR codes or not.
    fn start_as(name: &str, scans_qr: bool) -> (EngineHandle, std_mpsc::Receiver<Event>) {
        let (events, received) = std_mpsc::channel();
        let sink = move |event| {
            let _ = events.send(event);
        };
        let data_dir = std::env::temp_dir().join(format!("matrix-core-{name}-{}", std::process::id()));
        let config = EngineConfig {
            data_dir,
            secrets: Arc::new(MemorySecrets::default()),
            identity: ClientIdentity { name: "Test".into(), uri: "https://example.org".parse().unwrap() },
            oauth_redirect: crate::OAuthRedirect::Loopback,
            stores: StoreSharing::Alone,
            scans_qr,
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
    fn an_answer_to_a_command_that_asked_nothing_should_be_dropped() {
        let (engine, events) = start();
        let request = engine.send(Command::Answer { request: RequestId(9_999), answer: Answer::Agree });
        assert_eq!(finished(&events, request), Ok(()));
    }

    #[test]
    fn a_picture_of_a_qr_code_should_read_back_its_bytes() {
        let code = crate::QrImage::encode(&[0, 159, 255, 7]).expect("encodes");
        let side = (code.width + 8) * 4;
        let picture = image::GrayImage::from_fn(side, side, |x, y| {
            let dark = code.is_dark((x / 4).wrapping_sub(4), (y / 4).wrapping_sub(4));
            image::Luma([if dark { 0 } else { 255 }])
        });
        let path = std::env::temp_dir().join(format!("matrix-core-qr-{}.png", std::process::id()));
        picture.save(&path).expect("saves");
        let (engine, events) = start();
        let request = engine.send(Command::ReadQrCode { picture: path.clone() });
        let code = loop {
            match events.recv_timeout(std::time::Duration::from_secs(10)).expect("an event") {
                Event::QrCodeRead { request: read, code } if read == request => break code,
                _ => {}
            }
        };
        let _ = std::fs::remove_file(path);
        assert_eq!(code, Some(vec![0, 159, 255, 7]));
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

    /// Sends `command` and collects the events until it finishes.
    fn run(
        engine: &EngineHandle,
        events: &std_mpsc::Receiver<Event>,
        command: Command,
    ) -> (Result<(), String>, Vec<Event>) {
        let request = engine.send(command);
        let mut seen = Vec::new();
        loop {
            match events.recv_timeout(Duration::from_secs(60)).expect("an event") {
                Event::Finished { request: r, result } if r == request => return (result, seen),
                event => seen.push(event),
            }
        }
    }

    /// An account's life on a real homeserver with open registration (a local Synapse with
    /// `enable_registration_without_verification` and `experimental_features.msc3814_enabled`):
    /// `BM_LIVE_HOMESERVER=http://127.0.0.1:8008 cargo test -p matrix-core -- --ignored live_account`.
    #[test]
    #[ignore = "needs a homeserver with open registration"]
    fn live_account_should_register_move_keys_keep_an_offline_device_and_close() {
        let Ok(server) = std::env::var("BM_LIVE_HOMESERVER") else { return };
        let (engine, events) = start();
        let (found, seen) = run(&engine, &events, Command::Discover { server });
        found.expect("discovered");
        let homeserver = seen
            .into_iter()
            .find_map(|e| match e {
                Event::HomeserverDiscovered { homeserver, .. } => Some(homeserver),
                _ => None,
            })
            .expect("a homeserver");
        assert!(homeserver.methods.create_account, "{homeserver:?}");

        let username = format!("live{}", now_ms());
        let password = Password("first password".into());
        let registration = crate::RegisterRequest { homeserver: homeserver.url, username: username.clone(), password };
        let (registered, seen) = run(&engine, &events, Command::Register(registration));
        registered.expect("registered");
        let account = seen
            .iter()
            .find_map(|e| match e {
                Event::SignedIn { account, .. } => Some(account.clone()),
                _ => None,
            })
            .expect("signed in");
        assert!(account.as_str().starts_with(&format!("@{username}:")));

        let (set_up, seen) = run(&engine, &events, Command::SetUpRecovery(account.clone()));
        set_up.expect("recovery set up");
        let key = seen
            .into_iter()
            .find_map(|e| match e {
                Event::RecoveryKey { key, .. } => Some(key),
                _ => None,
            })
            .expect("a recovery key");

        let file = std::env::temp_dir().join(format!("matrix-core-live-keys-{}.txt", std::process::id()));
        let passphrase = Password("file passphrase".into());
        let export =
            Command::ExportRoomKeys { account: account.clone(), path: file.clone(), passphrase: passphrase.clone() };
        run(&engine, &events, export).0.expect("exported");
        assert!(std::fs::read_to_string(&file).expect("a key file").contains("BEGIN MEGOLM SESSION DATA"));
        let wrong =
            Command::ImportRoomKeys { account: account.clone(), path: file.clone(), passphrase: Password("no".into()) };
        assert!(run(&engine, &events, wrong).0.is_err(), "a wrong passphrase is refused");
        let import = Command::ImportRoomKeys { account: account.clone(), path: file.clone(), passphrase };
        let (imported, seen) = run(&engine, &events, import);
        imported.expect("imported");
        assert!(seen.iter().any(|e| matches!(e, Event::KeysImported { .. })));
        let _ = std::fs::remove_file(&file);

        let on = Command::SetOfflineDevice { account: account.clone(), on: true, recovery_key: Some(key) };
        run(&engine, &events, on).0.expect("offline device on");
        let off = Command::SetOfflineDevice { account: account.clone(), on: false, recovery_key: None };
        run(&engine, &events, off).0.expect("offline device off");

        let change = Command::ChangePassword {
            account: account.clone(),
            new_password: Password("second password".into()),
            password: Some(Password("first password".into())),
        };
        run(&engine, &events, change).0.expect("password changed");

        let close = Command::DeactivateAccount {
            account: account.clone(),
            erase: false,
            password: Some(Password("second password".into())),
        };
        let (closed, seen) = run(&engine, &events, close);
        closed.expect("deactivated");
        assert!(seen.iter().any(|e| matches!(e, Event::AccountRemoved(gone) if *gone == account)));
    }

    /// A message's words reach the room before the files sent with it, every time, as the
    /// composer sends them: one command each, straight after one another.
    /// `BM_LIVE_HOMESERVER=http://127.0.0.1:8008 cargo test -p matrix-core -- --ignored live_send_order`.
    #[test]
    #[ignore = "needs a homeserver with open registration"]
    fn live_send_order_should_put_the_words_before_the_files_sent_with_them() {
        let Ok(server) = std::env::var("BM_LIVE_HOMESERVER") else { return };
        let (engine, events) = start_as("live-order", false);
        let (found, seen) = run(&engine, &events, Command::Discover { server });
        found.expect("discovered");
        let homeserver = seen
            .into_iter()
            .find_map(|e| match e {
                Event::HomeserverDiscovered { homeserver, .. } => Some(homeserver.url),
                _ => None,
            })
            .expect("a homeserver");
        let ada = register_live(&engine, &events, &homeserver, "order");
        let room = crate::NewRoom { encrypted: false, ..crate::NewRoom::private("order", false) };
        let (created, seen) = run(&engine, &events, Command::CreateRoom { account: ada.clone(), room });
        created.expect("created");
        let room_id = seen
            .into_iter()
            .find_map(|e| match e {
                Event::RoomCreated { room_id, .. } => Some(room_id),
                _ => None,
            })
            .expect("a room");
        let key = TimelineKey::room(&room_id);
        run(&engine, &events, Command::OpenTimeline { account: ada.clone(), key: key.clone() }).0.expect("opened");

        let mut sent = Vec::new();
        let mut requests = Vec::new();
        for round in 0..5 {
            let words = format!("words {round}");
            let files = [format!("file-{round}a.txt"), format!("file-{round}b.txt")];
            let text = Outgoing::Text { body: words.clone(), html: None, mentions: crate::Mentions::default() };
            let uploads =
                files.clone().map(|name| Outgoing::File(crate::Upload::Bytes { name, bytes: Arc::from(&b"x"[..]) }));
            for message in std::iter::once(text).chain(uploads) {
                let send = Command::Send { account: ada.clone(), key: key.clone(), reply_to: None, message };
                requests.push(engine.send(send));
            }
            sent.push(words);
            sent.extend(files);
        }
        while !requests.is_empty() {
            match events.recv_timeout(Duration::from_secs(60)).expect("an event") {
                Event::Finished { request, result } if requests.contains(&request) => {
                    result.expect("sent");
                    requests.retain(|r| *r != request);
                }
                _ => {}
            }
        }

        // The room as the server has it, read as Ada from a fresh login.
        let bodies = tokio::runtime::Runtime::new().expect("a runtime").block_on(async {
            let homeserver = homeserver.trim_end_matches('/');
            let http = matrix_sdk::reqwest::Client::new();
            let user = ada.as_str().trim_start_matches('@').split(':').next().expect("a user");
            let login = serde_json::json!({
                "type": "m.login.password", "identifier": {"type": "m.id.user", "user": user}, "password": "a password"
            });
            let token = http
                .post(format!("{homeserver}/_matrix/client/v3/login"))
                .header("Content-Type", "application/json")
                .body(login.to_string())
                .send()
                .await
                .expect("logged in")
                .text()
                .await
                .expect("a token");
            let token: serde_json::Value = serde_json::from_str(&token).expect("a token");
            let token = token["access_token"].as_str().expect("an access token").to_owned();
            for _ in 0..30 {
                let page = http
                    .get(format!("{homeserver}/_matrix/client/v3/rooms/{room_id}/messages?dir=f&limit=100"))
                    .bearer_auth(&token)
                    .send()
                    .await
                    .expect("messages")
                    .text()
                    .await
                    .expect("a page");
                let page: serde_json::Value = serde_json::from_str(&page).expect("a page");
                let bodies: Vec<String> = page["chunk"]
                    .as_array()
                    .expect("a chunk")
                    .iter()
                    .filter(|event| event["type"] == "m.room.message")
                    .filter_map(|event| event["content"]["body"].as_str().map(str::to_owned))
                    .collect();
                if bodies.len() >= sent.len() {
                    return bodies;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            panic!("the messages never all arrived");
        });
        assert_eq!(bodies, sent);
    }

    /// Registers a fresh account on `homeserver` (a resolved URL) and returns it.
    fn register_live(
        engine: &EngineHandle,
        events: &std_mpsc::Receiver<Event>,
        homeserver: &str,
        name: &str,
    ) -> AccountId {
        let username = format!("{name}{}", now_ms());
        let registration = crate::RegisterRequest {
            homeserver: homeserver.to_owned(),
            username,
            password: Password("a password".into()),
        };
        let (registered, seen) = run(engine, events, Command::Register(registration));
        registered.expect("registered");
        seen.into_iter()
            .find_map(|e| match e {
                Event::SignedIn { account, .. } => Some(account),
                _ => None,
            })
            .expect("signed in")
    }

    #[test]
    #[ignore = "needs a homeserver with open registration"]
    fn live_location_should_deliver_a_second_fix_to_the_room_and_stop() {
        let Ok(server) = std::env::var("BM_LIVE_HOMESERVER") else { return };
        for encrypted in [false, true] {
            share_and_receive_live_location(&server, encrypted);
        }
    }

    fn share_and_receive_live_location(server: &str, encrypted: bool) {
        let (engine, events) = start_as(if encrypted { "live-location-encrypted" } else { "live-location" }, false);
        let account = register_live(&engine, &events, server, "beacon");
        let room = crate::NewRoom { encrypted, ..crate::NewRoom::private("Live location", false) };
        let (result, seen) = run(&engine, &events, Command::CreateRoom { account: account.clone(), room });
        result.unwrap();
        let room_id = seen
            .iter()
            .find_map(|event| match event {
                Event::RoomCreated { room_id, .. } => Some(room_id.clone()),
                _ => None,
            })
            .unwrap();
        let key = TimelineKey::room(&room_id);
        let (opened, opening) = run(&engine, &events, Command::OpenTimeline { account: account.clone(), key });
        opened.unwrap();
        // The picker becomes available when the new room's power levels reach the room list.
        let allowed = |event: &Event| {
            matches!(event, Event::RoomsUpdated { list, .. }
            if list.rooms.iter().any(|room| room.room_id == room_id && room.can_share_live_location))
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        if !seen.iter().chain(&opening).any(allowed) {
            while !allowed(&events.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).unwrap())
            {
            }
        }
        let request = engine.send(Command::ShareLiveLocation { account, room_id, geo_uri: "geo:1,2".into() });
        let mut beacon = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        loop {
            match events.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).unwrap() {
                Event::Finished { request: id, result } if id == request => panic!("early finish: {result:?}"),
                Event::LiveLocationStarted { request: id, event_id } if id == request => {
                    beacon = Some(event_id);
                    engine.send(Command::Answer { request, answer: Answer::LocationFix("geo:3,4".into()) });
                }
                Event::TimelineChanged { diffs, .. } => {
                    let updated = diffs.iter().any(|diff| {
                        let entries: &[_] = match diff {
                            crate::TimelineDiff::Reset(entries) | crate::TimelineDiff::Append(entries) => entries,
                            crate::TimelineDiff::PushBack(entry) | crate::TimelineDiff::PushFront(entry)
                            | crate::TimelineDiff::Set { entry, .. } | crate::TimelineDiff::Insert { entry, .. } => std::slice::from_ref(entry),
                            _ => &[],
                        };
                        entries.iter().any(|entry| matches!(&entry.kind, crate::EntryKind::Message(m)
                            if Some(&m.event_id) == beacon.as_ref() && matches!(&m.content, crate::MessageContent::Location(place) if place.geo_uri == "geo:3,4")))
                    });
                    if updated {
                        break;
                    }
                }
                _ => {}
            }
        }
        engine.send(Command::Cancel(request));
        finished(&events, request).unwrap();
    }

    /// The bytes a phone's camera would read from a code drawn on a screen.
    fn photographed(code: &crate::QrImage) -> Vec<u8> {
        let side = (code.width + 8) * 4;
        let picture = image::GrayImage::from_fn(side, side, |x, y| {
            let dark = code.is_dark((x / 4).wrapping_sub(4), (y / 4).wrapping_sub(4));
            image::Luma([if dark { 0 } else { 255 }])
        });
        crate::qr::read_luma(&picture).expect("the code reads")
    }

    /// Two people verify each other on a real homeserver: a desktop shows its code, a phone
    /// scans it, the desktop confirms, and the desktop then sees the phone's user as verified.
    /// `BM_LIVE_HOMESERVER=http://127.0.0.1:8008 cargo test -p matrix-core -- --ignored live_people`.
    #[test]
    #[ignore = "needs a homeserver with open registration"]
    fn live_people_should_verify_each_other_by_qr_code() {
        let Ok(server) = std::env::var("BM_LIVE_HOMESERVER") else { return };
        let (desktop, desktop_events) = start_as("live-desktop", false);
        let (phone, phone_events) = start_as("live-phone", true);
        let ada = register_live(&desktop, &desktop_events, &server, "ada");
        let grace = register_live(&phone, &phone_events, &server, "grace");
        // Each sets up encryption's identity: cross-signing comes with recovery.
        run(&desktop, &desktop_events, Command::SetUpRecovery(ada.clone())).0.expect("ada's recovery");
        run(&phone, &phone_events, Command::SetUpRecovery(grace.clone())).0.expect("grace's recovery");

        // Not before they share a chat: Grace's client would drop a request from Ada's devices,
        // which it doesn't know yet.
        let early = Command::VerifyUser { account: ada.clone(), user_id: grace.to_string() };
        let refused = run(&desktop, &desktop_events, early).0.expect_err("refused without a chat");
        assert!(refused.contains("direct chat"), "{refused}");
        // They already chat: verification starts in their direct chat, where each side knows the
        // other's devices.
        run(&desktop, &desktop_events, Command::OpenDirectChat { account: ada.clone(), user_id: grace.to_string() })
            .0
            .expect("a direct chat");
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        let mut joined = false;
        while !joined {
            assert!(std::time::Instant::now() < deadline, "the invitation never came");
            if let Ok(Event::RoomsUpdated { list, .. }) = phone_events.recv_timeout(Duration::from_secs(1))
                && let Some(invite) = list.invitations.first()
            {
                let join =
                    Command::JoinRoom { account: grace.clone(), room_id: invite.room_id.clone(), via: Vec::new() };
                run(&phone, &phone_events, join).0.expect("joined");
                joined = true;
            }
        }
        // Each side's sync learns the other's devices.
        std::thread::sleep(Duration::from_secs(3));
        while desktop_events.try_recv().is_ok() {}
        desktop.send(Command::VerifyUser { account: ada.clone(), user_id: grace.to_string() });
        let (mut scanned, mut verified) = (false, false);
        let mut ada_code = None;
        while !verified {
            assert!(std::time::Instant::now() < deadline, "the verification didn't finish in time");
            while let Ok(event) = phone_events.try_recv() {
                if std::env::var("BM_LIVE_TRACE").is_ok()
                    && matches!(
                        event,
                        Event::Verification { .. }
                            | Event::Finished { .. }
                            | Event::RoomsUpdated { .. }
                            | Event::RoomJoined { .. }
                    )
                {
                    eprintln!("phone: {event:?}");
                }
                match event {
                    Event::Verification { flow_id, step: crate::VerificationStep::Incoming { user, .. }, .. } => {
                        assert_eq!(user.map(|p| p.id), Some(ada.to_string()), "the request names Ada");
                        phone.send(Command::AcceptVerification { account: grace.clone(), flow_id });
                    }
                    Event::Verification {
                        flow_id, step: crate::VerificationStep::Ready { scan: true, .. }, ..
                    } => {
                        while ada_code.is_none() {
                            assert!(std::time::Instant::now() < deadline, "Ada never showed her code");
                            if let Ok(Event::Verification {
                                step: crate::VerificationStep::Ready { qr: Some(qr), .. },
                                ..
                            }) = desktop_events.recv_timeout(Duration::from_secs(1))
                            {
                                ada_code = Some(photographed(&qr));
                            }
                        }
                        let code = ada_code.clone().unwrap_or_default();
                        phone.send(Command::ScanVerificationCode { account: grace.clone(), flow_id, code });
                        scanned = true;
                    }
                    Event::Verification { step: crate::VerificationStep::Cancelled(why), .. } => {
                        panic!("cancelled: {why}")
                    }
                    _ => {}
                }
            }
            while let Ok(event) = desktop_events.try_recv() {
                if std::env::var("BM_LIVE_TRACE").is_ok()
                    && matches!(
                        event,
                        Event::Verification { .. }
                            | Event::VerificationRequested { .. }
                            | Event::Finished { result: Err(_), .. }
                    )
                {
                    eprintln!("desktop: {event:?}");
                }
                match event {
                    Event::Verification { step: crate::VerificationStep::Ready { qr: Some(qr), .. }, .. } => {
                        ada_code = Some(photographed(&qr));
                    }
                    Event::Verification { flow_id, step: crate::VerificationStep::Scanned, .. } => {
                        desktop.send(Command::ConfirmVerification { account: ada.clone(), flow_id, matches: true });
                    }
                    Event::Verification { step: crate::VerificationStep::Cancelled(why), .. } => {
                        panic!("cancelled: {why}")
                    }
                    Event::Identity { user_id, identity, .. } if user_id == grace.as_str() => {
                        assert_eq!(identity, Some(crate::Identity { verified: true, changed: false }));
                        verified = true;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(scanned, "the phone scanned the desktop's code");
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
