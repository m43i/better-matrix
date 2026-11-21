//! The desktop app's state. Two ways in, both small and general:
//!
//! - [`State::apply`] folds every engine [`Event`] into the state: the only path by which
//!   Matrix data arrives.
//! - User actions call methods here, which change UI state and send [`Command`]s through
//!   [`State::dispatch`], tagging each with a [`Purpose`] so its answer is routed back.
//!
//! Side effects on the OS (dialogs, notifications) go through [`Shell`].

mod accounts;
mod attachments;
pub mod backdrop;
pub mod breath;
mod call;
mod call_glow;
mod call_layout;
mod composer;
mod drafts;
mod gifs;
mod link_previews;
mod links;
mod media;
mod message_actions;
mod navigation;
mod panes;
mod poll;
mod preview;
mod profile;
mod readers;
mod receipts;
mod retry;
mod room_discovery;
mod search;
mod sections;
mod settings;
mod sign_in;
mod status_message;
mod typing;
mod updates;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use matrix_core::{
    AccountId, AccountPage, CallEnd, Command, ContactDetail, ContactKind, EngineHandle, Event, Homeserver, MediaRef,
    MediaSize, MediaToggle, Message, MessageContent, NotificationMode, Password, PendingContact, RequestId,
    SignInInput, TimelineKey, VerificationStep,
};
use rune::{TextEdit, Tx};

pub use accounts::READ_MARKER_FADE;
pub use accounts::{
    AccountData, Accounts, EntryView, Landing, RoomTimeline, Scope, TimelineItem, continues_group, now_ms,
    wants_attention,
};
pub use attachments::{AttachmentAction, can_open_attachment};
pub use call::Call;
pub use call_layout::{CallLayout, CallTile};
pub use composer::{Composer, EditTarget, Format, ReplyTarget, VoiceMemo};
pub use gifs::GifCache;
pub use link_previews::LinkPreviews;
pub use links::{LinkDialog, LinkStep};
pub use media::MediaCache;
pub use navigation::{navigate, navigate_now};
pub use panes::{Pane, PaneLayout, PaneSize, PaneWidths};
pub use poll::{MAX_ANSWERS, PollForm};
pub use preview::{
    DocumentStatus, ImageStatus, Opens, Preview, PreviewContent, PreviewItem, VideoStatus, render_document,
};
pub use profile::{ContactStep, ProfileForm};
pub use readers::{READER_FACES, ReaderMenu};
pub use room_discovery::Directory;
pub use search::{PeopleSearch, SEARCH_PAUSE, SearchFilter, SearchStatus, matches};
pub use sections::{DIRECT, INVITATIONS, ROOMS, SectionView, is_builtin, ordered_spaces, sections};
pub use settings::{Appearance, PlaybackSpeed, Settings, SettingsSection, Zoom};
pub use sign_in::{SignInForm, SignInStep};
pub use status_message::{leading_emoji, suggestions as status_suggestions};
pub use updates::Update;

use crate::i18n::{Language, t, tf};
use crate::platform::{Shell, Toast};

/// Which screen the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Saved accounts are being signed back in, and none is ready yet.
    Restoring,
    SignIn,
    Chat,
}

/// A floating panel over the window. At most one is open; new kinds are new variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Popover {
    AccountSwitcher,
    /// The context menu of one room in the sidebar, with its notifications submenu open or not.
    RoomMenu {
        room_id: String,
        notifications: bool,
    },
    /// The emoji picker, for a reaction or for a composer.
    Emoji(EmojiFor),
    /// The actions available for one sent message, including its attachment.
    MessageMenu {
        key: TimelineKey,
        event_id: String,
        /// Window point for a context menu; absent when opened from More.
        at: Option<(i32, i32)>,
    },
    /// File actions over the currently open attachment preview.
    AttachmentMenu {
        at: (i32, i32),
    },
    /// What can be done with a member, from the member list: start a chat, call, invite to
    /// another room, and with the rights, kick, ban or change their role.
    MemberMenu {
        user_id: String,
        submenu: Option<MemberSubmenu>,
    },
    /// Readers at one message, with an optional member menu beside a reader.
    ReadReceipts {
        key: TimelineKey,
        event_id: String,
        member: Option<ReaderMenu>,
        shown: std::ops::Range<usize>,
    },
    /// The top bar's search results.
    Search,
    /// The menu of one of the user's sidebar sections.
    SectionMenu(String),
    /// Starting a direct chat: search for someone, from the sidebar's chat button.
    NewChat,
    /// What can be made here, from the sidebar's plus button.
    CreateMenu,
    /// The menu of a space on the rail.
    SpaceMenu(String),
    /// The devices to pick from in a call, from the chevron beside its microphone or camera.
    CallDevices(matrix_core::DeviceKind),
    /// Call layout choices, anchored to the stage's layout button.
    CallLayout,
    /// The options of a select in settings.
    Select(SelectFor),
}

/// Which setting a select chooses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectFor {
    Language,
    /// An account's default notifications.
    Notifications(AccountId),
    /// Ringing aloud or silently.
    Ring,
    /// A microphone, speaker or camera.
    Device(matrix_core::DeviceKind),
}

/// What the panel beside the open room shows (the thread shows there when none is open).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidePanel {
    /// The room's overview: name, topic, and the ways to its members, threads and files.
    Info,
    Members,
    Threads,
    Files,
    Pins,
    /// Someone's profile, from their name or picture on a message: as the message shows them,
    /// for when the member list doesn't know them.
    Person(matrix_core::Person),
}

/// A submenu of the member menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberSubmenu {
    /// The rooms to invite them to.
    Invite,
    /// The roles to give them.
    Role,
}

/// What the room form makes or changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomForm {
    /// A new room, inside `space` when given.
    NewRoom {
        space: Option<String>,
    },
    NewSpace,
    /// A new name for a room or space.
    Rename {
        room_id: String,
    },
}

/// Where a voice memo goes once its file is written: the room or thread it was recorded for,
/// and the message it answers.
#[derive(Clone, Debug)]
pub struct MemoFor {
    pub target: ComposerTarget,
    pub account: AccountId,
    pub key: matrix_core::TimelineKey,
    pub reply_to: Option<String>,
}

/// The size a link card's image is fetched at: twice its 72-point square, for sharp screens.
const LINK_THUMB: MediaSize = MediaSize::Thumbnail { width: 160, height: 160 };

/// Which composer: the room's, or the open thread's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerTarget {
    Room,
    Thread,
}

/// Pictures are fetched as thumbnails this big: sharp at 2x for the 36-point timeline avatars and
/// the rail, and small enough that hundreds fit the media budget.
const AVATAR_PIXELS: (u32, u32) = (128, 128);
const AVATAR_SIZE: MediaSize = MediaSize::Thumbnail { width: AVATAR_PIXELS.0, height: AVATAR_PIXELS.1 };

/// What the system file dialog was opened for; its answer goes back there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickFor {
    /// Files to attach to a composer.
    Attachments(ComposerTarget),
    /// A new profile picture.
    Avatar,
}

/// Something dragged in the sidebar or on the rail: a room, a whole section, or a space.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarDrag {
    pub item: Dragged,
    /// The pointer, in window points, for the ghost.
    pub point: [f32; 2],
    /// Where it would land.
    pub target: Option<DropTarget>,
}

/// What is dragged in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dragged {
    Room(String),
    Section(String),
    /// A space on the rail, by room ID.
    Space(String),
}

/// Where a dragged item would land.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// A room: into this section, at this place among its rooms.
    Room { section: String, index: usize },
    /// A section: at this place among the sections.
    Section { index: usize },
    /// A space: at this place among the spaces on the rail.
    Space { index: usize },
}

/// An audio attachment or voice memo playing (or downloading to play).
#[derive(Debug)]
pub struct AudioClip {
    /// Which time a clip was picked to play: a failure its player reports after the clip was
    /// picked again belongs to the earlier time, and is ignored.
    pub id: u64,
    pub media: MediaRef,
    /// `None` while the file downloads.
    pub player: Option<crate::platform::sound::Player>,
    /// Where to start once the file is here, as a share of the length: scrubbing a clip before
    /// it has downloaded.
    pub start_at: Option<f64>,
    /// Whether it is to play, apart from where it is: once its file opens, and when a drag along
    /// its waveform ends. Picking it or pressing play sets it; pausing it, or moving it to its
    /// very end, clears it.
    pub play: bool,
    /// Whether the pointer is down on its waveform: a file that comes meanwhile waits for it.
    pub held: bool,
    /// Where it was and whether it was to play when the pointer went down on its waveform, to go
    /// back to if the drag is cancelled; `None` when that press picked it.
    pub scrub_from: Option<(f64, bool)>,
}

/// A step of a press or drag along a clip's waveform (`State::scrub_audio`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scrub {
    Press,
    Drag,
    Release,
    /// Escape, or the window losing focus, mid-drag.
    Cancel,
}

impl AudioClip {
    /// How far it has played, from 0 to 1; 0 while it downloads.
    pub fn progress(&self) -> f64 {
        match &self.player {
            Some(player) => player.progress(),
            None => self.start_at.unwrap_or(0.0),
        }
    }
}

/// A clip's or video's file in the scratch folder.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScratchFile {
    Downloading,
    Saved(std::path::PathBuf),
}

/// What decoding a clip read from its file: kept for its card after another clip is current.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    pub media: MediaRef,
    pub waveform: Vec<u16>,
    pub duration: Option<f64>,
}

/// How many clips' [`DecodedAudio`] the state keeps.
const DECODED_AUDIO: usize = 64;

/// How many steps a playing clip's progress shows in: the view rebuilds as it passes each, so
/// the bars fill and the scrubber moves without redrawing at every frame.
pub const PROGRESS_STEPS: f64 = 120.0;
/// How far one arrow key moves a clip along, as a share of its length.
pub const AUDIO_STEP: f64 = 0.05;

/// Whether this platform plays audio: the app's own player covers every desktop.
pub fn audio_supported() -> bool {
    true
}

/// A field that takes focus as soon as it shows: the one to type in when a dialog, popover or
/// room opens.
fn focused(mut edit: TextEdit) -> TextEdit {
    edit.focused = true;
    edit
}

/// What a chosen emoji goes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmojiFor {
    /// A reaction to one message.
    Reaction { key: TimelineKey, event_id: String },
    /// The draft in a composer, at its caret.
    Composer(ComposerTarget),
    /// The status message being written in the account menu, which it starts.
    StatusMessage,
}

/// A dialog over the whole window, with the rest dimmed. At most one is open; new kinds are new
/// variants.
#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    /// The messages an account saved, from the account menu.
    Saved(AccountId),
    /// Choose a destination room for a forwarded message.
    Forward {
        account: AccountId,
        key: TimelineKey,
        event_id: String,
        search: TextEdit,
        sending: Option<RequestId>,
    },
    /// Confirm sending message text to Google Translate in the browser.
    Translate(String),
    Invitation {
        account: AccountId,
        room_id: String,
    },
    PublicRooms {
        account: AccountId,
        space: String,
    },
    Settings(SettingsSection),
    /// Signing in to another account over the current conversation.
    SignIn,
    /// Writing a poll (`State::poll_form`).
    Poll,
    /// "Are you sure?" before something that can't be undone.
    Confirm(Confirm),
    /// Verifying this session with another of the user's sessions. `flow_id` is `None` while
    /// the request is on its way.
    Verification {
        account: AccountId,
        flow_id: Option<String>,
        step: VerificationStep,
    },
    /// Entering the recovery key to verify this session.
    RecoveryKeyEntry {
        account: AccountId,
        edit: TextEdit,
        error: Option<String>,
        submitting: bool,
    },
    /// A new recovery key, shown once.
    NewRecoveryKey {
        key: String,
    },
    /// The account password, for an action the server guards with it.
    Password {
        account: AccountId,
        action: Guarded,
        edit: TextEdit,
        error: Option<String>,
        submitting: bool,
    },
    /// Replacing the account's cryptographic identity, when the recovery key and every verified
    /// session are lost.
    ResetIdentity {
        account: AccountId,
        step: ResetStep,
    },
    /// The server ended an account's session: it is signed out here; sign in again? When
    /// `kept`, its data stays on this device for that.
    SessionEnded {
        account: AccountId,
        kept: bool,
    },
    /// Naming a room or space: a new one, or a new name.
    RoomForm {
        form: RoomForm,
        name: Box<TextEdit>,
        topic: Box<TextEdit>,
    },
    /// Naming a sidebar section: a new one, or renaming `section`.
    SectionName {
        section: Option<String>,
        edit: TextEdit,
    },
    /// Inviting people to a room.
    Invite {
        account: AccountId,
        room_id: String,
        /// Who was invited while the dialog is open.
        invited: Vec<String>,
    },
    /// A permalink to a room the user hasn't joined (its preview, Join), or to a person.
    Link(LinkDialog),
    /// A new name for a session.
    RenameSession {
        account: AccountId,
        device_id: String,
        edit: TextEdit,
    },
}

/// An action that waits for the user's confirmation. New destructive actions are new variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    LeaveRoom {
        account: AccountId,
        room_id: String,
        name: String,
    },
    DeleteChat {
        account: AccountId,
        room_id: String,
        name: String,
    },
    /// Kick or ban a member, or raise them to the user's own level (which the user can't undo:
    /// equals can't demote each other).
    Moderate {
        account: AccountId,
        room_id: String,
        user_id: String,
        name: String,
        action: matrix_core::Moderation,
    },
    /// Delete one of the user's messages, for everyone.
    DeleteMessage {
        account: AccountId,
        key: TimelineKey,
        event_id: String,
    },
    /// End a poll: nobody can vote after, and it can't be opened again.
    EndPoll {
        account: AccountId,
        key: TimelineKey,
        poll_id: String,
        question: String,
    },
}

/// An action the server may guard with the account password ([`Event::PasswordNeeded`]). New
/// guarded actions are new variants; the password dialog and its retry are shared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Guarded {
    EndSession { device_id: String },
    ResetIdentity,
    AddContact { pending: PendingContact },
}

impl Guarded {
    fn command(&self, account: AccountId, password: Option<Password>) -> Command {
        match self {
            Self::EndSession { device_id } => Command::EndSession { account, device_id: device_id.clone(), password },
            Self::ResetIdentity => Command::ResetIdentity { account, password },
            Self::AddContact { pending } => Command::AddContact { account, pending: pending.clone(), password },
        }
    }
}

/// Where an identity reset stands. The password, when the server asks, is the shared
/// [`Modal::Password`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResetStep {
    /// What a reset keeps and what it costs; nothing has happened yet.
    Warning { error: Option<String> },
    /// Sent. An OAuth account approves it on its account page, opened in the browser at `url`.
    Resetting { request: RequestId, url: Option<String> },
    /// This session now holds the new identity; recovery is next.
    Done,
}

/// How many members' pictures the open member list loads.
const MEMBER_PICTURES: usize = 60;

/// Why a command was sent, so its [`Event::Finished`] lands in the right place.
#[derive(Debug, Clone, PartialEq)]
enum Purpose {
    Directory {
        account: AccountId,
        space: String,
    },
    Membership {
        account: AccountId,
        room_id: String,
        action: room_discovery::MembershipAction,
        scope: Scope,
    },
    /// Starting, answering or steering a call.
    Call,
    /// Watching or unwatching one of a call's videos.
    CallVideo,
    Discover,
    SignIn,
    Send,
    /// Editing one of the user's messages: what was sent, back in the field if it fails.
    Edit(Box<composer::Edit>),
    /// Deleting one of the user's messages.
    Delete,
    /// Message utilities whose failures become a notice.
    MessageAction,
    /// Listing the messages an account saved; a failure shows in the list.
    Saved(AccountId),
    /// Reading the draft the store keeps for a timeline; a failure lets the next visit ask again.
    Draft(drafts::Owner),
    /// Keeping a draft in the store; a failure is forgotten, as the draft stays in memory.
    KeepDraft,
    /// Forwarding from the destination picker, which stays open on failure.
    Forward,
    Logout,
    Media {
        media: MediaRef,
        dimensions: Option<(u32, u32)>,
    },
    NotificationMode,
    /// Setting the user's status to `chosen`; failing, the account shows its saved status again,
    /// unless another status was chosen or reported since.
    Status {
        account: AccountId,
        chosen: matrix_core::Status,
    },
    /// Setting the user's status message; once the latest one set finishes, the message shows as
    /// the engine reports it.
    StatusMessage {
        account: AccountId,
    },
    Leave,
    Recover,
    RecoveryKey,
    Guarded {
        account: AccountId,
        action: Guarded,
    },
    Security,
    /// Profile changes whose failure is a notice: removing a contact detail.
    Profile,
    ProfileName,
    Avatar,
    /// Asking for a contact detail's link or code.
    ContactCode(ContactKind),
    /// Sending a phone number's code.
    ContactConfirm(ContactKind),
    AccountPage,
    /// The full file behind the open preview, for the showing ([`Preview::visit`]) that asked.
    Preview(u64),
    /// Saving a previewed video in scratch to play it.
    Video {
        media: MediaRef,
        path: PathBuf,
    },
    /// Saving a file the user chose to download.
    Save,
    /// Fetching the original image for a clipboard copy.
    CopyImage,
    /// Saving an attachment in scratch before opening it with the default application.
    OpenFile(PathBuf),
    /// Reading a room's members.
    Members {
        account: AccountId,
        room_id: String,
        all: bool,
    },
    /// Reading a room's threads or files for its side panel.
    SidePanel(AccountId, String, SidePanel),
    /// A local message search.
    MessageSearch,
    /// A public room or space directory search; errors stay in the search panel.
    DirectorySearch,
    /// A user-directory search; failures leave the results as they were.
    People,
    /// Inviting someone.
    Invite,
    /// Opening a direct chat, from search or a member list; it opens if the account is still
    /// the one shown.
    DirectChat(AccountId),
    /// Kicking, banning or changing a member's role.
    Moderate,
    /// Voting in or ending a poll.
    Poll,
    /// A direct chat being opened (or started) to call its person.
    DirectCall {
        account: AccountId,
        video: bool,
    },
    /// Saving an account's sidebar arrangement. One save per account runs at a time, so the
    /// server never ends on an older arrangement.
    Layout(AccountId),
    /// Telling the server the user read a timeline; a failure waits for the next message.
    Read,
    /// Telling a room the user is typing; a failure is forgotten, as the notice would lapse.
    Typing,
    /// Making a room or space, which opens once made if the account is still the one shown.
    Create {
        account: AccountId,
        space: bool,
    },
    /// Renaming a room or space.
    Rename,
    /// Downloading audio to play from `path`.
    Audio {
        media: MediaRef,
        path: PathBuf,
    },
    Timeline {
        account: AccountId,
        key: TimelineKey,
    },
    History {
        account: AccountId,
        key: TimelineKey,
    },
    React,
    Gif(String),
    /// A link's preview card, from an account's homeserver.
    LinkPreview(link_previews::LinkKey),
    /// Looking up or joining what a permalink names; the link dialog's step holds the request.
    Permalink,
    Cancel,
}

/// Where the user is: the active account and, per account, the scope and open room.
#[derive(Debug, Default)]
pub struct Nav {
    pub account: Option<AccountId>,
    scopes: HashMap<AccountId, Scope>,
    rooms: HashMap<AccountId, String>,
    /// The thread open beside the room, by its root event, per account.
    threads: HashMap<AccountId, String>,
    /// The shown room whose visit has been recorded, independent of subscription retries.
    visited: Option<(AccountId, String)>,
}

#[derive(Debug)]
pub struct State {
    pub directories: HashMap<(AccountId, String), room_discovery::Directory>,
    pub accounts: Accounts,
    pub nav: Nav,
    pub sign_in: SignInForm,
    pub composer: Composer,
    pub thread_composer: Composer,
    /// The Profile page's form.
    pub profile: ProfileForm,
    /// Saved accounts still being signed back in at launch.
    pub restoring: Vec<AccountId>,
    /// The attachment shown full size over the window, if any.
    pub preview: Option<Preview>,
    /// The top bar's search.
    pub search: PeopleSearch,
    pub message_jump: Option<search::MessageJump>,
    /// The invite dialog's search.
    pub invite_search: PeopleSearch,
    /// The new-chat search.
    pub chat_search: PeopleSearch,
    /// The audio attachment or voice memo playing, if any.
    pub audio: Option<AudioClip>,
    /// What playing clips here read from their files, newest last, kept once another clip is
    /// current: the waveform and length of clips whose senders sent neither.
    pub decoded_audio: Vec<DecodedAudio>,
    /// Clips' and videos' files, downloading or saved to play: one download each, and a saved
    /// file is played again as it is, so one played again never writes its file over while it
    /// is read.
    scratch_files: HashMap<MediaRef, ScratchFile>,
    /// The [`AudioClip::id`] of the clip picked last.
    last_audio_id: u64,
    /// The [`Preview::visit`] of the item shown last.
    preview_visits: u64,
    /// The preview's item changed: the next `poll` scrolls the strip to its thumbnail.
    reveal_thumbnail: bool,
    /// Accounts whose sidebar layout changed while a save was under way.
    layout_unsaved: HashSet<AccountId>,
    /// What [`State::clock`] read when the view last showed it.
    clock_shown: Option<(Option<u64>, Option<u64>, Option<u64>)>,
    /// The entries of each open timeline its view shows, by ID; one not reported yet counts
    /// its newest entries.
    on_screen: HashMap<(AccountId, TimelineKey), accounts::Shown>,
    /// When a failed download may next be tried again.
    retry_at: Option<std::time::Instant>,
    /// The lists whose pictures were last asked for: the active account and
    /// [`Accounts::revision`] then. Their pictures are asked for again only once these change.
    avatars_listed: Option<(Option<AccountId>, u64)>,
    /// A fingerprint of the pictures those lists showed, so a list that changed but shows the
    /// same pictures, all of them here, asks for none.
    listed_pictures: Option<u64>,
    /// Accounts the server signed out softly, whose data waits on this device until the user
    /// signs in as them again or removes them.
    pub suspended: Vec<AccountId>,
    /// The sign-in screen's backdrop.
    pub backdrop: backdrop::Backdrop,
    /// The system asks for reduced motion, as the view last saw: the state's animations hold
    /// still.
    pub reduced_motion: std::cell::Cell<bool>,
    /// What the shown room was last told about the user's typing.
    typing: typing::Typing,
    receipts: receipts::Receipts,
    /// A room being dragged in the sidebar, and the section it is over.
    pub dragging: Option<SidebarDrag>,
    /// Where the sidebar's sections are, by ID, for dropping a dragged room.
    section_zones: Vec<(String, [f32; 4])>,
    /// Where the sidebar's room rows are, by room ID, for the place a dropped room takes.
    row_zones: HashMap<String, [f32; 4]>,
    /// Where the rail's space tiles are, by room ID, for the place a dropped space takes.
    space_zones: HashMap<String, [f32; 4]>,
    pub media: MediaCache,
    pub gifs: GifCache,
    /// Links' preview cards.
    pub link_previews: LinkPreviews,
    pub popover: Option<Popover>,
    pub modal: Option<Modal>,
    /// What the emoji picker searches for.
    pub emoji_search: TextEdit,
    /// The account whose status message is being written in the account menu, while one is.
    pub status_draft: Option<AccountId>,
    /// The status message being written.
    pub status_edit: TextEdit,
    /// The poll being written, while [`Modal::Poll`] shows.
    pub poll_form: PollForm,
    /// Edits set aside: refused by the server, or left by navigating away. Each comes back into
    /// its composer once its timeline shows with the composer free (no other edit, no memo).
    set_aside: Vec<composer::Edit>,
    /// Drafts of timelines not on screen, by timeline: each comes back into its composer when
    /// its timeline shows again (`State::follow_drafts`).
    drafts: HashMap<drafts::Owner, composer::Draft>,
    /// Timelines whose draft was asked of the store this run: it is read once, on the first
    /// visit, as the store holds what was left at the last run.
    drafts_asked: HashSet<drafts::Owner>,
    /// What the store holds of each draft, and which are due to be written.
    kept_drafts: drafts::Kept,
    /// The panel beside the open room.
    pub side: Option<SidePanel>,
    /// Widths shared across rooms and accounts for this window.
    pub panes: PaneWidths,
    /// The room and panel the side panel's list was last asked for.
    side_loaded: Option<(AccountId, String, SidePanel)>,
    /// What the member list is filtered by.
    pub member_search: TextEdit,
    /// The call under way, from any account.
    pub call: Option<Call>,
    /// Calls ringing for the user, oldest first.
    pub rings: Vec<matrix_core::IncomingCall>,
    /// The ring sounding while a call rings (unless the user silenced it).
    ring_sound: Option<crate::platform::sound::Ring>,
    /// The microphones, speakers and cameras there are, once listed.
    pub call_devices: Option<matrix_core::CallDevices>,
    pub settings: Settings,
    /// Whether Settings → Info shows the full third-party list.
    pub third_party_shown: bool,
    /// A newer desktop release, offered until the user chooses to install it.
    pub update: Update,
    /// A file drag is over the room, for the drop highlight.
    pub drop_hover: bool,
    /// Whether the window has keyboard focus; notifications for the open room are skipped then.
    pub window_focused: bool,
    /// The selected rail tile's halo: breathing a while after the user turns to the window.
    pub breath: breath::Breath,
    /// The latest error worth telling the user about; dismissable.
    pub notice: Option<String>,
    /// A permalink that came before any account was ready: it opens with the first room list.
    pending_link: Option<matrix_core::Permalink>,
    /// The notice on screen and since when, so it goes after a while.
    notice_shown: Option<(String, std::time::Instant)>,
    /// Where a dialog's text field types when its dialog has just closed.
    scratch: TextEdit,
    /// Set by a timeline's scroll handler when its view nears the oldest loaded message;
    /// [`State::poll`] then loads more. Shared because scroll handlers can't change state.
    pub near_top: Arc<AtomicBool>,
    /// Whether the main timeline is at the bottom, as the scroll handler last reported.
    pub at_bottom: Arc<AtomicBool>,
    /// The same for the thread panel.
    pub thread_near_top: Arc<AtomicBool>,
    /// The timelines the engine is following: the room on screen and its open thread.
    open_timelines: Vec<(AccountId, TimelineKey)>,
    /// Subscriptions that have delivered their first snapshot; warm UI rows can precede it.
    timelines_ready: HashSet<(AccountId, TimelineKey)>,
    engine: EngineHandle,
    shell: Shell,
    pending: HashMap<RequestId, Purpose>,
}

impl State {
    /// A state with `settings` applied (the language takes effect at once).
    pub fn new(engine: EngineHandle, shell: Shell, settings: Settings) -> Self {
        settings.language.apply();
        let mut state = Self {
            directories: HashMap::new(),
            accounts: Accounts::default(),
            nav: Nav { account: settings.last_account.clone(), ..Nav::default() },
            sign_in: SignInForm::default(),
            composer: Composer::default(),
            thread_composer: Composer::default(),
            profile: ProfileForm::default(),
            restoring: Vec::new(),
            preview: None,
            search: PeopleSearch::default(),
            message_jump: None,
            invite_search: PeopleSearch::default(),
            chat_search: PeopleSearch::default(),
            audio: None,
            decoded_audio: Vec::new(),
            scratch_files: HashMap::new(),
            last_audio_id: 0,
            preview_visits: 0,
            reveal_thumbnail: false,
            layout_unsaved: HashSet::new(),
            clock_shown: None,
            typing: typing::Typing::default(),
            receipts: receipts::Receipts::default(),
            suspended: Vec::new(),
            backdrop: backdrop::Backdrop::default(),
            reduced_motion: std::cell::Cell::new(false),
            on_screen: HashMap::new(),
            retry_at: None,
            avatars_listed: None,
            listed_pictures: None,
            dragging: None,
            section_zones: Vec::new(),
            space_zones: HashMap::new(),
            row_zones: HashMap::new(),
            media: MediaCache::default(),
            gifs: GifCache::default(),
            link_previews: LinkPreviews::default(),
            popover: None,
            modal: None,
            emoji_search: TextEdit::default(),
            status_draft: None,
            status_edit: TextEdit::default(),
            poll_form: PollForm::default(),
            set_aside: Vec::new(),
            drafts: HashMap::new(),
            drafts_asked: HashSet::new(),
            kept_drafts: drafts::Kept::default(),
            side: None,
            panes: PaneWidths::default(),
            side_loaded: None,
            member_search: TextEdit::default(),
            call: None,
            rings: Vec::new(),
            ring_sound: None,
            call_devices: None,
            third_party_shown: false,
            update: Update::default(),
            settings,
            drop_hover: false,
            window_focused: true,
            breath: breath::Breath::Asked,
            notice: None,
            pending_link: None,
            notice_shown: None,
            scratch: TextEdit::new(""),
            near_top: Arc::new(AtomicBool::new(false)),
            at_bottom: Arc::new(AtomicBool::new(false)),
            thread_near_top: Arc::new(AtomicBool::new(false)),
            open_timelines: Vec::new(),
            timelines_ready: HashSet::new(),
            engine,
            shell,
            pending: HashMap::new(),
        };
        // Calls use the devices picked on an earlier run.
        let devices = state.settings.devices.clone();
        for kind in
            [matrix_core::DeviceKind::Microphone, matrix_core::DeviceKind::Speaker, matrix_core::DeviceKind::Camera]
        {
            if let Some(name) = devices.get(kind) {
                let command = Command::ChooseCallDevice { kind, id: Some(name.to_owned()) };
                state.dispatch(command, Purpose::CallVideo);
            }
        }
        state
    }

    /// A state whose commands and OS calls go nowhere, in English: for headless renders and
    /// tests.
    pub fn detached() -> Self {
        Self::new(EngineHandle::detached().0, Shell::headless(), Settings::fixed_english())
    }

    // ----- reading -------------------------------------------------------------------------

    pub fn screen(&self) -> Screen {
        if self.active_account().is_some() {
            Screen::Chat
        } else if !self.restoring.is_empty() {
            Screen::Restoring
        } else {
            Screen::SignIn
        }
    }

    pub fn active_account(&self) -> Option<&AccountData> {
        self.nav.account.as_ref().and_then(|id| self.accounts.get(id))
    }

    pub fn active_scope(&self) -> Scope {
        self.nav.account.as_ref().and_then(|id| self.nav.scopes.get(id)).cloned().unwrap_or_default()
    }

    pub fn active_room_id(&self) -> Option<&str> {
        self.nav.account.as_ref().and_then(|id| self.nav.rooms.get(id)).map(String::as_str)
    }

    /// The open thread's root event ID, if a thread is open beside the room.
    pub fn active_thread(&self) -> Option<&str> {
        self.nav.account.as_ref().and_then(|id| self.nav.threads.get(id)).map(String::as_str)
    }

    /// The timeline a composer writes into.
    pub fn composer_key(&self, target: ComposerTarget) -> Option<TimelineKey> {
        let room = self.active_room_id()?;
        match target {
            ComposerTarget::Room => Some(TimelineKey::room(room)),
            ComposerTarget::Thread => Some(TimelineKey::thread(room, self.active_thread()?)),
        }
    }

    pub fn composer(&self, target: ComposerTarget) -> &Composer {
        match target {
            ComposerTarget::Room => &self.composer,
            ComposerTarget::Thread => &self.thread_composer,
        }
    }

    pub fn composer_mut(&mut self, target: ComposerTarget) -> &mut Composer {
        match target {
            ComposerTarget::Room => &mut self.composer,
            ComposerTarget::Thread => &mut self.thread_composer,
        }
    }

    /// Whether any account other than the active one has rooms wanting attention.
    pub fn other_accounts_unread(&self) -> bool {
        self.accounts.iter().any(|a| Some(&a.summary.id) != self.nav.account.as_ref() && a.has_unread(&Scope::Home))
    }

    pub fn dark(&self, system_dark: bool) -> bool {
        match self.settings.appearance {
            Appearance::System => system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }

    // ----- engine --------------------------------------------------------------------------

    fn dispatch(&mut self, command: Command, purpose: Purpose) -> RequestId {
        let reading = match &command {
            Command::MarkRead { account, key, .. } => Some((account.clone(), key.clone())),
            _ => None,
        };
        let request = self.engine.send(command);
        if let Some((account, key)) = reading {
            self.receipts.sent(&account, &key, request);
        }
        self.pending.insert(request, purpose);
        request
    }

    /// Folds one engine event into the state.
    pub fn apply(&mut self, event: Event, tx: &mut Tx) {
        let location = navigation::Location::of(self);
        match event {
            Event::Finished { request, result } => self.finish(request, result),
            Event::HomeserverDiscovered { request, homeserver } => {
                if self.sign_in.step == SignInStep::Discovering(request) {
                    self.continue_sign_in(homeserver);
                }
            }
            Event::OpenUrl { request, url } if self.pending.get(&request) == Some(&Purpose::AccountPage) => {
                if let Err(error) = tx.open_url(&url) {
                    self.notice = Some(tf!("notice.browser", error = error));
                }
            }
            Event::OpenUrl { request, url } if matches!(self.pending.get(&request), Some(Purpose::Guarded { .. })) => {
                if let Some(Modal::ResetIdentity {
                    step: ResetStep::Resetting { request: waiting, url: shown }, ..
                }) = &mut self.modal
                    && *waiting == request
                {
                    *shown = Some(url.clone());
                }
                if let Err(error) = tx.open_url(&url) {
                    self.notice = Some(tf!("notice.browser", error = error));
                }
            }
            Event::OpenUrl { request, url } => {
                if let SignInStep::Browser { request: Some(waiting), url: shown, .. } = &mut self.sign_in.step
                    && *waiting == request
                {
                    if let Err(error) = tx.open_url(&url) {
                        self.notice = Some(tf!("notice.browser", error = error));
                    }
                    *shown = Some(url);
                }
            }
            Event::Restoring(accounts) => {
                if self.nav.account.as_ref().is_some_and(|id| !accounts.contains(id)) {
                    self.nav.account = None;
                }
                self.restoring = accounts;
            }
            // Every refused request reports the ended session again: only the first counts.
            Event::SessionEnded { account, .. } if matches!(&self.modal, Some(Modal::SessionEnded { account: shown, .. }) if *shown == account) =>
                {}
            Event::SessionEnded { account, kept } => {
                self.restoring.retain(|id| *id != account);
                // Signed out softly, the account stops but its data stays for signing in again;
                // one suspended in an earlier run is already stopped.
                if !kept {
                    self.dispatch(Command::Logout(account.clone()), Purpose::Logout);
                } else {
                    if self.accounts.get(&account).is_some() {
                        self.dispatch(Command::Suspend(account.clone()), Purpose::Logout);
                    }
                    if !self.suspended.contains(&account) {
                        self.suspended.push(account.clone());
                    }
                }
                // One suspended in an earlier run is listed (account menu, sign-in card) rather
                // than announced at every launch.
                let running = self.accounts.get(&account).is_some();
                if !kept || running {
                    self.modal = Some(Modal::SessionEnded { account, kept });
                }
            }
            Event::AccountAdded(summary) => {
                self.restoring.retain(|id| *id != summary.id);
                self.suspended.retain(|id| *id != summary.id);
                // An incoming dialog can cover sign-in while its request continues. An
                // explicit close clears the form's request, so a cancelled sign-in won't
                // take over if it still completes.
                let signing_in =
                    self.sign_in.pending().and_then(|request| self.pending.get(&request)) == Some(&Purpose::SignIn);
                if self.nav.account.is_none() || signing_in {
                    self.nav.account = Some(summary.id.clone());
                }
                // Signed in again while it ran: the engine runs it afresh, so its open timelines
                // open again on the new session.
                let again = self.accounts.get(&summary.id).is_some();
                let id = summary.id.clone();
                self.accounts.upsert(summary);
                if again {
                    self.open_timelines.retain(|(account, _)| *account != id);
                    self.sync_timeline();
                }
                self.request_avatars();
            }
            Event::AccountChanged(summary) => {
                self.accounts.upsert(summary);
                self.request_avatars();
            }
            Event::AccountRemoved(id) => {
                self.directories.retain(|(account, _), _| *account != id);
                // A suspended account stays listed in `suspended`: removing it is what suspending
                // does here.
                self.accounts.remove(&id);
                // The full sign-in screen takes over without cancelling the form's request.
                if self.accounts.is_empty() && matches!(self.modal, Some(Modal::SignIn)) {
                    self.modal = None;
                }
                self.nav.rooms.remove(&id);
                self.nav.scopes.remove(&id);
                if self.nav.account.as_ref() == Some(&id) {
                    self.reset_search();
                    self.nav.account = self.accounts.iter().next().map(|account| account.summary.id.clone());
                }
                self.open_timelines.retain(|(account, _)| *account != id);
                self.sync_timeline();
                // After the sync, which sets an edit of its timelines aside: they all go with it,
                // and an edit still on its way doesn't come back if the account signs in again.
                self.set_aside.retain(|edit| edit.target.account != id);
                self.drafts.retain(|(account, _), _| *account != id);
                self.drafts_asked.retain(|(account, _)| *account != id);
                self.kept_drafts.forget(&id);
                if matches!(&self.modal, Some(Modal::Saved(account)) if *account == id) {
                    self.modal = None;
                }
                self.pending.retain(|_, purpose| !matches!(purpose, Purpose::Edit(edit) if edit.target.account == id));
            }
            Event::RoomsUpdated { account, list } => {
                let changed_counts =
                    self.accounts.get(&account).map(|data| data.member_counts_changed(&list.rooms)).unwrap_or_default();
                let pins_changed = self.nav.account.as_ref() == Some(&account)
                    && self.active_room_id().is_some_and(|id| {
                        let old = self.accounts.get(&account).and_then(|data| data.room_unindexed(id));
                        let new = list.rooms.iter().find(|room| room.room_id == id);
                        old.map(|r| &r.pinned) != new.map(|r| &r.pinned)
                    });
                self.accounts.set_room_list(&account, list);
                self.saved_changed(&account);
                if pins_changed && self.side == Some(SidePanel::Pins) {
                    self.side_loaded = None;
                    self.sync_side();
                }
                for room_id in changed_counts {
                    self.invalidate_members(&account, &room_id);
                }
                let scope_gone = matches!(self.nav.scopes.get(&account), Some(Scope::Space(id))
                    if self.accounts.get(&account).is_none_or(|data| data.space(id).is_none()));
                if scope_gone {
                    self.nav.scopes.remove(&account);
                    self.nav.threads.remove(&account);
                }
                self.directories.retain(|(owner, space), _| {
                    owner != &account || self.accounts.get(owner).is_some_and(|data| data.space(space).is_some())
                });
                self.request_avatars();
                // The open room is gone (left, deleted, kicked): back to the overview.
                let gone = self
                    .nav
                    .rooms
                    .get(&account)
                    .is_some_and(|id| self.accounts.get(&account).is_none_or(|a| a.room(id).is_none()));
                if gone || scope_gone {
                    self.nav.rooms.remove(&account);
                    self.nav.threads.remove(&account);
                    self.sync_timeline();
                }
                self.open_pending_link();
            }
            Event::PublicRooms { request, mut rooms } => {
                if let Some(Purpose::Directory { account, space }) = self.pending.get(&request)
                    && self.accounts.get(account).is_some_and(|data| data.space(space).is_some())
                {
                    rooms.sort_by(|a, b| {
                        b.member_count
                            .cmp(&a.member_count)
                            .then_with(|| a.name.cmp(&b.name))
                            .then_with(|| a.room_id.cmp(&b.room_id))
                    });
                    rooms.dedup_by(|a, b| a.room_id == b.room_id);
                    self.directories.insert((account.clone(), space.clone()), room_discovery::Directory::Ready(rooms));
                }
            }
            Event::LatestMessage { account, room_id, message } => {
                self.accounts.set_latest(&account, &room_id, *message);
            }
            Event::ReadBoundary { account, room_id, event_id, loaded } => {
                if let Some(timeline) = self.accounts.kept_timeline_mut(&account, &TimelineKey::room(room_id)) {
                    timeline.set_read_boundary(event_id, loaded);
                }
            }
            // Changes still in flight when a timeline closed are dropped: opening it again
            // starts from a fresh copy.
            Event::TimelineChanged { account, key, .. }
                if !self.open_timelines.contains(&(account.clone(), key.clone())) => {}
            Event::TimelineChanged { account, key, diffs } => {
                self.timelines_ready.insert((account.clone(), key.clone()));
                let old_end = self
                    .accounts
                    .get(&account)
                    .and_then(|data| data.timeline(&key))
                    .map_or(0, |timeline| timeline.entries.len().saturating_sub(1));
                let incoming_while_away = !self.window_focused && diffs.iter().any(|diff| {
                    let other = |entry: &matrix_core::TimelineEntry| matches!(&entry.kind, matrix_core::EntryKind::Message(message) if message.sender_id != account.as_str() && !message.pending);
                    match diff {
                        matrix_core::TimelineDiff::PushBack(entry) => other(entry),
                        matrix_core::TimelineDiff::Append(entries) => entries.iter().any(other),
                        matrix_core::TimelineDiff::Insert { index, entry } if *index >= old_end => other(entry),
                        _ => false,
                    }
                });
                let following_live = self.window_focused
                    && key.thread.is_none()
                    && self.at_bottom.load(Ordering::Relaxed)
                    && !diffs.iter().any(|diff| matches!(diff, matrix_core::TimelineDiff::Reset(_)))
                    && self
                        .accounts
                        .get(&account)
                        .and_then(|data| data.timeline(&key))
                        .is_some_and(RoomTimeline::may_mark_fully_read);
                // Only a change that takes entries away can take a GIF off the timelines.
                let only_added = diffs.iter().all(|diff| {
                    matches!(
                        diff,
                        matrix_core::TimelineDiff::PushBack(_)
                            | matrix_core::TimelineDiff::PushFront(_)
                            | matrix_core::TimelineDiff::Append(_)
                            | matrix_core::TimelineDiff::Insert { .. }
                    )
                });
                // A timeline's first content lands as it is, rather than easing in row by row,
                // and so does the history that fills it after: a room opened with a few cached
                // messages would otherwise build up from the bottom whenever the frame its page
                // arrives in animates (input in it, or the read marker's fade). The history
                // lands in its own timeline only; news, at the end, eases in.
                let reset = diffs.iter().any(|diff| matches!(diff, matrix_core::TimelineDiff::Reset(_)));
                if reset {
                    if key.thread.is_none() {
                        tx.settle();
                    } else {
                        // A thread snapshot must not repin the room's sticky viewport.
                        tx.instant();
                    }
                }
                if let Some(timeline) = self.accounts.timeline_mut(&account, &key) {
                    if reset || timeline.adds_history(&diffs) {
                        timeline.landing = Landing::Due;
                    }
                    timeline.apply(diffs);
                    if incoming_while_away {
                        timeline.show_read_marker();
                    }
                    if following_live {
                        timeline.see_current_read_marker();
                    }
                    if let Some(root) = &key.thread {
                        timeline.note_thread_root(root);
                    }
                }
                self.refresh_readers();
                if self.window_focused {
                    self.mark_timeline_read(account, key);
                }
                self.request_timeline_media();
                if only_added {
                    self.fetch_linked(std::time::Instant::now());
                } else {
                    self.request_gifs();
                }
                self.request_avatars();
            }
            Event::Typing { account, room_id, people }
                if self.open_timelines.iter().any(|(open, key)| *open == account && key.room_id == room_id) =>
            {
                self.accounts.set_typing(&account, room_id, people);
                self.request_avatars();
            }
            // The room was closed meanwhile: nothing shows its typing.
            Event::Typing { .. } => {}
            Event::GifLoaded { url, bytes } => {
                self.gifs.loaded(url, bytes);
                // It may have scrolled away while downloading: it plays only if still shown.
                self.fetch_linked(std::time::Instant::now());
            }
            Event::LinkPreview { account, url, preview } => {
                self.link_previews.loaded((account, url), preview.map(|preview| *preview));
                // Its image, if it has one, loads like the other media on screen.
                self.request_timeline_media();
            }
            Event::SecurityChanged { account, security } => {
                let before = self.accounts.get(&account).and_then(|a| a.security);
                self.accounts.set_security(&account, security);
                // Newly verified, or the backup key just arrived: the backup can now fill in the
                // keys of messages on screen that couldn't be decrypted.
                if security.session_verified && before != Some(security) {
                    self.dispatch(Command::FetchRoomKeys(account), Purpose::Security);
                }
            }
            Event::Sessions { account, sessions } => self.accounts.set_sessions(&account, sessions),
            Event::ContactDetails { account, contacts } => self.accounts.set_contacts(&account, contacts),
            Event::Status { account, status } => self.accounts.status_saved(&account, status),
            Event::Presence { account, people } => self.accounts.set_presence(&account, people),
            Event::SidebarLayout { account, layout } => {
                let newer = self
                    .accounts
                    .get(&account)
                    .is_none_or(|data| !data.sidebar_loaded || layout.changed_at >= data.sidebar.changed_at);
                if newer && !self.saving_layout(&account) {
                    self.accounts.set_sidebar(&account, layout);
                }
            }
            Event::Members { request, .. } if request.is_some_and(|request| !self.pending.contains_key(&request)) => {}
            Event::Members { account, room_id, members, .. } => {
                self.accounts.set_members(&account, room_id, members);
                self.request_timeline_media();
                self.request_avatars();
            }
            Event::MembersInvalidated { account, room_id } => {
                self.invalidate_members(&account, &room_id);
            }
            Event::SearchUsage { account, usage } => self.accounts.set_search_usage(&account, usage),
            Event::MessagesFound { request, messages } => {
                self.search.messages_found(request, messages);
            }
            Event::DirectoryFound { request, rooms } => {
                self.search.directory_found(request, rooms);
                self.request_avatars();
            }
            Event::PeopleFound { request, people } => {
                self.link_person_found(request, &people);
                self.search.found(request, people.clone());
                self.invite_search.found(request, people.clone());
                self.chat_search.found(request, people);
                self.request_avatars();
            }
            Event::DirectChat { request, room_id } => match self.pending.get(&request).cloned() {
                Some(Purpose::DirectChat(account)) if self.nav.account.as_ref() == Some(&account) => {
                    self.open_room(&account, &room_id);
                }
                // The chat is there: open it and call.
                Some(Purpose::DirectCall { account, video }) if self.nav.account.as_ref() == Some(&account) => {
                    self.open_room(&account, &room_id);
                    self.start_call(video);
                }
                _ => {}
            },
            Event::RoomPreview { request, preview } => self.link_previewed(request, *preview),
            Event::RoomJoined { request, room_id } => self.link_joined(request, &room_id),
            Event::RoomCreated { request, room_id } => {
                let Some(Purpose::Create { account, space }) = self.pending.get(&request).cloned() else { return };
                if self.nav.account.as_ref() != Some(&account) {
                    return;
                }
                let is_space = self.accounts.get(&account).is_some_and(|data| data.space(&room_id).is_some());
                if space || is_space {
                    self.select_scope(Scope::Space(room_id));
                } else {
                    self.open_room(&account, &room_id);
                }
            }
            Event::ContactCodeSent { request, pending } => {
                if let Some(form) = self.profile.requesting(request) {
                    form.step = ContactStep::Sent { pending, busy: false };
                    form.code = TextEdit::new("");
                    form.error = None;
                }
            }
            Event::RecoveryKey { key, .. } => self.modal = Some(Modal::NewRecoveryKey { key: key.0 }),
            // The request is answered by asking; its failure that follows is not shown.
            Event::PasswordNeeded { request } => {
                if let Some(Purpose::Guarded { account, action }) = self.pending.remove(&request) {
                    let edit = focused(TextEdit::new("").secure());
                    self.modal = Some(Modal::Password { account, action, edit, error: None, submitting: false });
                }
            }
            Event::Verification { account, flow_id, step } => self.verification_moved(account, flow_id, step),
            Event::HistoryLoaded { account, key, reached_start } => {
                if let Some(timeline) = self.accounts.kept_timeline_mut(&account, &key) {
                    timeline.loading = false;
                    timeline.reached_start |= reached_start;
                }
            }
            Event::HistoryFailed { account, key, error } => {
                if let Some(timeline) = self.accounts.kept_timeline_mut(&account, &key) {
                    timeline.history_failed(error);
                }
            }
            Event::MediaLoaded { request, media, bytes, .. } => match self.pending.get(&request) {
                Some(Purpose::Preview(visit)) => {
                    let visit = *visit;
                    self.preview_loaded(visit, bytes);
                }
                Some(Purpose::CopyImage) => self.shell.copy_image(bytes),
                Some(Purpose::Media { dimensions, .. }) => {
                    let dimensions = *dimensions;
                    self.media.loaded(media, bytes, dimensions);
                }
                _ => {}
            },
            Event::Notification(notification) => {
                let looking = self.window_focused
                    && self.nav.account.as_ref() == Some(&notification.account)
                    && self.active_room_id() == Some(notification.room_id.as_str());
                if !looking {
                    let account_count = self.accounts.iter().count();
                    let title = match self.accounts.get(&notification.account) {
                        Some(account) if account_count > 1 => {
                            format!(
                                "{} in {} · {}",
                                notification.sender_name, notification.room_name, account.summary.display_name
                            )
                        }
                        _ => format!("{} in {}", notification.sender_name, notification.room_name),
                    };
                    self.shell.notify(&Toast {
                        id: notification.event_id,
                        title,
                        body: notification.body,
                        account: notification.account.to_string(),
                        room_id: notification.room_id,
                    });
                }
            }
            Event::AccountError { account, error } => {
                // A saved account that couldn't be signed back in: the sign-in screen takes over.
                self.restoring.retain(|id| *id != account);
                if self.nav.account.as_ref() == Some(&account) && self.accounts.get(&account).is_none() {
                    self.nav.account = self.accounts.iter().next().map(|data| data.summary.id.clone());
                }
                self.notice = Some(tf!("notice.account", account = account, error = error));
            }
            Event::CallUpdated(info) => {
                self.call_updated(*info);
                self.request_avatars();
            }
            Event::CallEnded { call, reason, .. } => {
                // A call that another replaced ends quietly.
                let replaced = self.call.as_ref().is_some_and(|shown| shown.info.call != call);
                if !replaced {
                    self.call = None;
                    if self.popover == Some(Popover::CallLayout) {
                        self.popover = None;
                    }
                    self.notice = match reason {
                        CallEnd::HungUp | CallEnd::Left => None,
                        CallEnd::Declined => Some(t!("call.declined").to_owned()),
                        CallEnd::NoAnswer => Some(t!("call.no_answer").to_owned()),
                        CallEnd::AnsweredElsewhere => Some(t!("call.answered_elsewhere").to_owned()),
                        CallEnd::Failed(error) => Some(tf!("call.failed", error = error)),
                    }
                    .or(self.notice.take());
                }
            }
            // Only who speaks shows in the view; the glow reads the levels on its own.
            Event::CallLevels(levels) => {
                if !self.call.as_mut().is_some_and(|call| call.heard(levels, std::time::Instant::now())) {
                    tx.live_only();
                }
            }
            Event::CallMediaFailed(error) => self.notice = Some(tf!("call.media_failed", error = error)),
            Event::CallMediaStopped(media) => {
                self.notice = Some(
                    match media {
                        MediaToggle::Screen(_) => t!("call.share_stopped"),
                        MediaToggle::Camera(_) | MediaToggle::Microphone(_) => t!("call.camera_stopped"),
                    }
                    .to_owned(),
                );
            }
            Event::IncomingCall(ring) => {
                if !self.window_focused {
                    let kind = if ring.video { t!("call.incoming_video") } else { t!("call.incoming") };
                    self.shell.notify(&Toast {
                        id: format!("ring-{}-{}", ring.account, ring.room_id),
                        title: ring.caller.name.clone(),
                        body: kind.to_owned(),
                        account: ring.account.to_string(),
                        room_id: ring.room_id.clone(),
                    });
                }
                self.rings.retain(|known| !(known.account == ring.account && known.room_id == ring.room_id));
                self.rings.push(*ring);
                self.sound_ring();
                self.request_avatars();
            }
            Event::RingEnded { account, room_id } => {
                self.rings.retain(|ring| !(ring.account == account && ring.room_id == room_id));
                self.sound_ring();
            }
            Event::CallDevices(devices) => self.call_devices = Some(devices),
            Event::Threads { account, room_id, threads } => self.accounts.set_threads(&account, room_id, threads),
            Event::PinnedMessages { account, room_id, messages } => {
                self.accounts.set_pins(&account, room_id, messages);
                self.request_avatars();
            }
            Event::SavedMessages { account, messages } => {
                self.accounts.set_saved_messages(&account, messages);
                self.request_avatars();
            }
            Event::DraftLoaded { account, key, draft, .. } => self.draft_loaded((account, key), draft),
            Event::Files { account, room_id, files } => self.accounts.set_files(&account, room_id, files),
            // Pushes are for phones: the desktop syncs while it runs and never registers a pusher.
            Event::PushContent { .. } => {}
        }
        self.remember_account();
        if !location.is_current(self) {
            tx.settle();
        }
    }

    // ----- calls ---------------------------------------------------------------------------

    /// Takes the engine's view of the call (a newer call's replaces an older one's; an older
    /// one's is late and ignored), watching the videos that came while the stage shows and
    /// dropping the ones that went.
    fn call_updated(&mut self, info: matrix_core::CallInfo) {
        match self.call.as_ref().map(|call| call.info.call) {
            Some(shown) if info.call < shown => return,
            Some(shown) if info.call == shown => {}
            _ => self.call = Some(Call::new(info.clone())),
        }
        let Some(call) = self.call.as_mut() else { return };
        let changes = call.update(info);
        for video in &changes.removed {
            call.watched.remove(video);
        }
        for video in changes.removed {
            self.dispatch(Command::UnwatchVideo(video), Purpose::CallVideo);
        }
        self.sync_watching();
    }

    /// Has the engine stream `videos` into their images.
    fn watch_videos(&mut self, videos: &[matrix_core::VideoId]) {
        let Some(call) = self.call.as_ref() else { return };
        let found: Vec<_> =
            videos.iter().filter_map(|id| call.videos.get(id).map(|pixels| (*id, pixels.clone()))).collect();
        for (video, pixels) in found {
            let sink = matrix_core::VideoSink(Arc::new(move |width, height, rgba| pixels.set(width, height, rgba)));
            self.dispatch(Command::WatchVideo { video, sink }, Purpose::CallVideo);
        }
    }

    /// Diffs watches against the visible call plan, so hidden pages and hidden self-view
    /// convert no frames. A call outside the current room watches nothing.
    pub fn sync_watching(&mut self) {
        let layout_menu = self.call_here().is_some_and(|call| call.video() && !call.compact());
        if !layout_menu && self.popover == Some(Popover::CallLayout) {
            self.popover = None;
        }
        let shows = self.call_here().is_some();
        let Some(call) = self.call.as_mut() else { return };
        // Called every frame: the plan is worked out again only when what it reads changed.
        let key = (shows, call.videos_key());
        if call.watched_for == Some(key) {
            return;
        }
        call.watched_for = Some(key);
        let visible = if shows { call.visible_videos() } else { HashSet::new() };
        let added: Vec<_> = visible.difference(&call.watched).copied().collect();
        let removed: Vec<_> = call.watched.difference(&visible).copied().collect();
        call.watched = visible;
        self.watch_videos(&added);
        for video in removed {
            self.dispatch(Command::UnwatchVideo(video), Purpose::CallVideo);
        }
    }

    /// The call, when its room is the one shown.
    pub fn call_here(&self) -> Option<&Call> {
        self.call.as_ref().filter(|call| {
            self.nav.account.as_ref() == Some(&call.info.account)
                && self.active_room_id() == Some(call.info.room_id.as_str())
        })
    }

    /// Starts a call in the room shown, or joins the call under way there.
    pub fn start_call(&mut self, video: bool) {
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        self.dispatch(Command::StartCall { account, room_id, video }, Purpose::Call);
    }

    /// Answers a ringing call and opens its room.
    pub fn answer_call(&mut self, account: &AccountId, room_id: &str, video: bool) {
        self.rings.retain(|ring| !(ring.account == *account && ring.room_id == room_id));
        self.sound_ring();
        self.open_room(account, room_id);
        let command = Command::AnswerCall { account: account.clone(), room_id: room_id.to_owned(), video };
        self.dispatch(command, Purpose::Call);
    }

    pub fn decline_call(&mut self, account: &AccountId, room_id: &str) {
        self.rings.retain(|ring| !(ring.account == *account && ring.room_id == room_id));
        self.sound_ring();
        self.dispatch(Command::DeclineCall { account: account.clone(), room_id: room_id.to_owned() }, Purpose::Call);
    }

    pub fn hang_up(&mut self) {
        self.dispatch(Command::HangUp, Purpose::Call);
    }

    pub fn toggle_microphone(&mut self) {
        let Some(on) = self.call.as_ref().map(|call| !call.info.microphone) else { return };
        self.dispatch(Command::SetCallMedia(MediaToggle::Microphone(on)), Purpose::Call);
    }

    pub fn toggle_camera(&mut self) {
        let Some(on) = self.call.as_ref().map(|call| call.info.camera.is_none()) else { return };
        self.dispatch(Command::SetCallMedia(MediaToggle::Camera(on)), Purpose::Call);
    }

    pub fn toggle_screen(&mut self) {
        let Some(on) = self.call.as_ref().map(|call| call.info.screen.is_none()) else { return };
        self.dispatch(Command::SetCallMedia(MediaToggle::Screen(on)), Purpose::Call);
    }

    /// Rings aloud while a call rings (and the user wants the sound), silent otherwise.
    fn sound_ring(&mut self) {
        let wanted = self.settings.ring_sound && !self.rings.is_empty();
        if !wanted {
            self.ring_sound = None;
        } else if self.ring_sound.is_none() {
            self.ring_sound = self.shell.ring(self.settings.devices.speaker.clone());
        }
    }

    /// Opens (or closes) a select in settings; a device select lists the devices afresh.
    pub fn open_select(&mut self, of: SelectFor) {
        if matches!(of, SelectFor::Device(_)) && self.popover.as_ref() != Some(&Popover::Select(of.clone())) {
            self.list_devices();
        }
        self.toggle_popover(Popover::Select(of));
    }

    /// Opens the device menu beside a call control, listing the devices afresh.
    pub fn open_call_devices(&mut self, kind: matrix_core::DeviceKind) {
        self.list_devices();
        self.toggle_popover(Popover::CallDevices(kind));
    }

    /// Asks which microphones, speakers and cameras there are.
    pub fn list_devices(&mut self) {
        self.dispatch(Command::ListCallDevices, Purpose::CallVideo);
    }

    /// Picks a microphone, speaker or camera (`None`: the system's default) for calls and voice
    /// messages, from the call under way on, and remembers it.
    pub fn choose_device(&mut self, kind: matrix_core::DeviceKind, name: Option<String>) {
        self.settings.devices.set(kind, name.clone());
        self.shell.save_settings(self.settings.clone());
        self.popover = None;
        self.dispatch(Command::ChooseCallDevice { kind, id: name }, Purpose::CallVideo);
    }

    /// Rings aloud for incoming calls, or silently.
    pub fn set_ring_sound(&mut self, on: bool) {
        self.settings.ring_sound = on;
        self.shell.save_settings(self.settings.clone());
        self.sound_ring();
    }

    /// Shows or hides chat alongside the call stage.
    pub fn toggle_call_chat(&mut self) {
        if let Some(call) = self.call.as_mut() {
            call.chat = !call.chat;
        }
        self.sync_watching();
    }

    /// Goes back to the call: its room, showing the stage.
    pub fn open_call(&mut self) {
        let Some((account, room_id)) = self.call.as_mut().map(|call| {
            call.chat = false;
            (call.info.account.clone(), call.info.room_id.clone())
        }) else {
            return;
        };
        self.open_room(&account, &room_id);
    }

    fn finish(&mut self, request: RequestId, result: Result<(), String>) {
        self.search.finished(request, result.is_err());
        self.receipts.finished(request, result.is_ok(), std::time::Instant::now());
        let Some(purpose) = self.pending.remove(&request) else { return };
        let Err(error) = result else {
            match &purpose {
                Purpose::Forward if matches!(self.modal, Some(Modal::Forward { sending: Some(waiting), .. }) if waiting == request) => {
                    self.modal = None
                }
                Purpose::Membership { account, room_id, action, scope } => {
                    self.membership_finished(account, room_id, *action, scope)
                }
                Purpose::Recover if matches!(self.modal, Some(Modal::RecoveryKeyEntry { .. })) => self.modal = None,
                Purpose::Guarded { account, action: Guarded::ResetIdentity } if self.resetting_identity() => {
                    self.modal = Some(Modal::ResetIdentity { account: account.clone(), step: ResetStep::Done });
                }
                Purpose::Guarded { action: Guarded::AddContact { pending }, .. } => {
                    self.profile.contact_mut(pending.kind).reset();
                    if matches!(self.modal, Some(Modal::Password { .. })) {
                        self.modal = None;
                    }
                }
                Purpose::ProfileName => self.profile.saving_name = false,
                Purpose::StatusMessage { account } => {
                    self.accounts.status_message_saved(account, request);
                }
                Purpose::Audio { media, path } => {
                    self.scratch_files.insert(media.clone(), ScratchFile::Saved(path.clone()));
                    if self.audio.as_ref().is_some_and(|clip| clip.media == *media) {
                        self.open_audio(path.clone());
                    }
                }
                Purpose::Video { media, path } => {
                    self.scratch_files.insert(media.clone(), ScratchFile::Saved(path.clone()));
                    let (media, path) = (media.clone(), path.clone());
                    self.open_video(&media, path);
                }
                Purpose::Avatar => self.profile.saving_avatar = false,
                Purpose::OpenFile(path) => self.shell.open_file(path.clone()),
                Purpose::Layout(account) => self.save_layout_again(account.clone()),
                // Something saved while this list was on its way is listed now.
                Purpose::Saved(account) => self.saved_changed(&account.clone()),
                Purpose::ContactConfirm(kind) => {
                    let kind = *kind;
                    self.add_contact(kind);
                }
                Purpose::Guarded { .. } if matches!(self.modal, Some(Modal::Password { .. })) => self.modal = None,
                _ => {}
            }
            if purpose == Purpose::SignIn {
                self.sign_in = SignInForm::default();
                if matches!(self.modal, Some(Modal::SignIn)) {
                    self.modal = None;
                }
                self.sync_timeline();
            }
            return;
        };
        match purpose {
            Purpose::Directory { account, space } => {
                if self.accounts.get(&account).is_some_and(|data| data.space(&space).is_some()) {
                    self.directories.insert((account, space), room_discovery::Directory::Failed(error));
                }
            }
            Purpose::Membership { .. } => self.notice = Some(tf!("notice.membership", error = error)),
            Purpose::Discover | Purpose::SignIn => self.sign_in.fail(request, error),
            Purpose::Cancel => {}
            Purpose::Media { media, .. } => {
                self.media.failed(media, std::time::Instant::now());
                self.schedule_retry();
            }
            Purpose::Send => self.notice = Some(tf!("notice.not_sent", error = error)),
            Purpose::Edit(edit) => {
                self.notice = Some(tf!("notice.not_changed", error = error));
                // It comes back as the user left it: now, or once its timeline and composer are
                // free. (Removing an account forgets its edits on their way.)
                self.set_edit_aside(*edit);
                self.resume_edits();
            }
            Purpose::MessageAction => self.notice = Some(tf!("notice.message_action", error = error)),
            Purpose::Saved(account) => self.accounts.saved_failed(&account, error),
            Purpose::Draft(owner) => {
                self.drafts_asked.remove(&owner);
            }
            Purpose::KeepDraft => {}
            Purpose::Forward => {
                if let Some(Modal::Forward { sending, .. }) = &mut self.modal
                    && *sending == Some(request)
                {
                    *sending = None;
                }
                self.notice = Some(tf!("notice.message_action", error = error));
            }
            Purpose::Delete => self.notice = Some(tf!("notice.not_deleted", error = error)),
            Purpose::Logout => self.notice = Some(tf!("notice.sign_out", error = error)),
            Purpose::NotificationMode => self.notice = Some(tf!("notice.notification_setting", error = error)),
            Purpose::Status { account, chosen } => {
                if let Some(data) = self.accounts.get(&account)
                    && data.status == chosen
                {
                    let saved = data.saved_status;
                    self.accounts.set_status(&account, saved);
                }
                self.notice = Some(tf!("notice.status", error = error));
            }
            // A choice made since has the say.
            Purpose::StatusMessage { account } => {
                if self.accounts.status_message_saved(&account, request) {
                    self.notice = Some(tf!("notice.status_message", error = error));
                }
            }
            Purpose::Leave => self.notice = Some(tf!("notice.leave", error = error)),
            // History that couldn't load waits for the user to retry, instead of retrying on every
            // scroll; a timeline that couldn't open opens again the next time it is shown.
            Purpose::History { account, key } => {
                if let Some(timeline) = self.accounts.kept_timeline_mut(&account, &key) {
                    timeline.history_failed(error);
                }
            }
            Purpose::Timeline { account, key } => {
                if let Some(timeline) = self.accounts.kept_timeline_mut(&account, &key) {
                    timeline.history_failed(error);
                }
                self.open_timelines.retain(|open| *open != (account.clone(), key.clone()));
            }
            Purpose::React => self.notice = Some(tf!("notice.react", error = error)),
            Purpose::Recover => {
                if let Some(Modal::RecoveryKeyEntry { error: shown, submitting, .. }) = &mut self.modal {
                    *shown = Some(error);
                    *submitting = false;
                }
            }
            Purpose::Guarded { .. } => match &mut self.modal {
                Some(Modal::Password { error: shown, submitting, .. }) => {
                    *shown = Some(error);
                    *submitting = false;
                }
                Some(Modal::ResetIdentity { step, .. }) if matches!(step, ResetStep::Resetting { .. }) => {
                    *step = ResetStep::Warning { error: Some(error) };
                }
                _ => match purpose {
                    Purpose::Guarded { action: Guarded::AddContact { pending }, .. } => {
                        self.profile.contact_mut(pending.kind).fail(error);
                    }
                    _ => self.notice = Some(error),
                },
            },
            Purpose::ContactCode(kind) | Purpose::ContactConfirm(kind) => self.profile.contact_mut(kind).fail(error),
            Purpose::ProfileName => {
                self.profile.saving_name = false;
                self.notice = Some(error);
            }
            Purpose::Avatar => {
                self.profile.saving_avatar = false;
                self.notice = Some(error);
            }
            Purpose::Profile | Purpose::AccountPage => self.notice = Some(error),
            Purpose::Preview(visit) => {
                if let Some(preview) = self.preview.as_mut().filter(|preview| preview.visit == visit) {
                    match &mut preview.content {
                        PreviewContent::Document { status, .. } => *status = DocumentStatus::Failed(error),
                        PreviewContent::Image { status, .. } => *status = ImageStatus::Failed(error),
                        PreviewContent::Video { .. } | PreviewContent::Audio => {}
                    }
                }
            }
            Purpose::Video { media, .. } => {
                self.scratch_files.remove(&media);
                if let Some(Preview { content: PreviewContent::Video { status, .. }, .. }) =
                    self.preview.as_mut().filter(|preview| preview.media == media)
                {
                    *status = VideoStatus::Failed(error);
                }
            }
            Purpose::Save => self.notice = Some(tf!("notice.save", error = error)),
            Purpose::CopyImage => self.notice = Some(tf!("notice.copy_image", error = error)),
            Purpose::OpenFile(_) => self.notice = Some(tf!("notice.open_attachment", error = error)),
            Purpose::RecoveryKey | Purpose::Security => self.notice = Some(error),
            Purpose::Gif(url) => {
                self.gifs.failed(&url, std::time::Instant::now());
                self.schedule_retry();
            }
            Purpose::LinkPreview(key) => {
                self.link_previews.failed(key, std::time::Instant::now());
                self.schedule_retry();
            }
            Purpose::Members { .. }
            | Purpose::MessageSearch
            | Purpose::DirectorySearch
            | Purpose::People
            | Purpose::Read
            | Purpose::Typing
            | Purpose::CallVideo => {}
            Purpose::Call => self.notice = Some(tf!("call.failed", error = error)),
            Purpose::Layout(account) => {
                self.notice = Some(tf!("notice.layout", error = error));
                self.save_layout_again(account);
            }
            Purpose::Create { .. } => self.notice = Some(tf!("notice.create", error = error)),
            Purpose::Rename => self.notice = Some(tf!("notice.rename", error = error)),
            Purpose::Audio { media, .. } => {
                self.scratch_files.remove(&media);
                self.audio_failed(&media, None, error);
            }
            Purpose::Invite => self.notice = Some(tf!("notice.invite", error = error)),
            Purpose::Moderate => self.notice = Some(tf!("notice.moderate", error = error)),
            // The panel stops loading, and says why.
            Purpose::SidePanel(account, room_id, panel) => {
                match panel {
                    SidePanel::Threads => self.accounts.set_threads(&account, room_id, Vec::new()),
                    SidePanel::Files => self.accounts.set_files(&account, room_id, Vec::new()),
                    SidePanel::Pins => {
                        self.accounts.pins_failed(&account, room_id, error.clone());
                    }
                    SidePanel::Info | SidePanel::Members | SidePanel::Person(_) => {}
                }
                if panel != SidePanel::Pins {
                    self.notice = Some(tf!("notice.side", error = error));
                }
            }
            Purpose::Poll => self.notice = Some(tf!("notice.poll", error = error)),
            Purpose::Permalink => self.link_failed(request, error),
            Purpose::DirectChat(_) | Purpose::DirectCall { .. } => {
                self.notice = Some(tf!("notice.direct_chat", error = error));
            }
        }
    }

    /// Keeps the engine following exactly the timelines on screen (the room, and its thread when
    /// one is open): closes the ones no longer shown, opens the new ones. Call after anything that
    /// changes what is shown.
    fn sync_timeline(&mut self) {
        let mut shown = Vec::new();
        if let (Some(account), Some(room_id), Screen::Chat) = (&self.nav.account, self.active_room_id(), self.screen())
        {
            shown.push((account.clone(), TimelineKey::room(room_id)));
            if let Some(root) = self.active_thread() {
                shown.push((account.clone(), TimelineKey::thread(room_id, root)));
            }
        }
        let visiting = shown.first().map(|(account, key)| (account.clone(), key.room_id.clone()));
        if self.nav.visited != visiting {
            self.nav.visited = visiting.clone();
            if let Some((account, room_id)) = visiting {
                self.engine.send(Command::VisitRoom { account, room_id });
            }
        }
        if self.open_timelines.first() != shown.first() {
            self.side = None;
            self.side_loaded = None;
            self.member_search = TextEdit::default();
        }
        // A memo recording for a timeline no longer shown would go on unseen, the microphone
        // open: closing the thread, another room and the overview all drop it. An edit of a
        // message there is set aside, its draft back, and returns when the timeline does.
        // Kept edits come back only once each composer holds its new timeline's draft.
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            if self.composer(target).voice.as_ref().is_some_and(|memo| !shown.contains(&memo.timeline))
                && let Some(memo) = self.composer_mut(target).voice.take()
            {
                memo.recorder.cancel();
            }
            let editing = self.composer(target).editing.as_ref().map(|editing| &editing.target);
            if editing.is_some_and(|edited| !shown.iter().any(|(a, k)| *a == edited.account && *k == edited.key))
                && let Some(edit) = self.composer_mut(target).set_aside_edit()
            {
                self.set_edit_aside(edit);
            }
        }
        let moved = self.follow_drafts(&shown);
        self.resume_edits();
        // A draft or edit coming back with its timeline isn't typing.
        if !moved.is_empty() {
            self.settle_typing(&moved);
        }
        let (gone, kept): (Vec<_>, Vec<_>) = self.open_timelines.drain(..).partition(|open| !shown.contains(open));
        for (account, key) in gone {
            if key.thread.is_none() {
                self.near_top.store(false, Ordering::Relaxed);
                self.at_bottom.store(false, Ordering::Relaxed);
            } else {
                self.thread_near_top.store(false, Ordering::Relaxed);
            }
            self.accounts.timeline_closed(&account, &key);
            self.timelines_ready.remove(&(account.clone(), key.clone()));
            self.receipts.forget(&account, &key);
            self.on_screen.remove(&(account.clone(), key.clone()));
            self.engine.send(Command::CloseTimeline { account, key });
        }
        self.open_timelines = kept;
        for (account, key) in shown {
            if self.open_timelines.contains(&(account.clone(), key.clone())) {
                continue;
            }
            self.accounts.timeline_opened(&account, &key);
            if let Some(timeline) = self.accounts.timeline_mut(&account, &key) {
                timeline.loading = true;
                timeline.history_error = None;
            }
            let purpose = Purpose::Timeline { account: account.clone(), key: key.clone() };
            self.dispatch(Command::OpenTimeline { account: account.clone(), key: key.clone() }, purpose);
            if key.thread.is_none() {
                let complete = self
                    .accounts
                    .get(&account)
                    .and_then(|data| data.members.get(&key.room_id))
                    .map(|members| members.complete);
                if complete.is_none() || !self.accounts.members_fresh(&account, &key.room_id) {
                    self.load_members(account.clone(), key.room_id.clone(), complete.unwrap_or(false));
                }
            }
            self.open_timelines.push((account, key));
        }
        self.request_gifs();
        self.sync_side();
    }

    /// Asks for the open side panel's list (threads or files) of the room shown, once per room.
    fn sync_side(&mut self) {
        let (Some(account), Some(room_id), Some(panel @ (SidePanel::Threads | SidePanel::Files | SidePanel::Pins))) =
            (self.nav.account.clone(), self.active_room_id().map(str::to_owned), self.side.clone())
        else {
            return;
        };
        let wanted = (account.clone(), room_id.clone(), panel.clone());
        if self.side_loaded.as_ref() == Some(&wanted) {
            return;
        }
        self.side_loaded = Some(wanted);
        let purpose = Purpose::SidePanel(account.clone(), room_id.clone(), panel.clone());
        let command = match panel {
            SidePanel::Threads => Command::ListThreads { account, room_id },
            SidePanel::Pins => Command::ListPinnedMessages { account, room_id },
            _ => Command::ListFiles { account, room_id },
        };
        self.dispatch(command, purpose);
    }

    /// The window gained or lost focus. Coming back marks what is on screen read: messages that
    /// arrived while away count as unread until the user is back.
    pub fn focus_changed(&mut self, focused: bool) {
        self.window_focused = focused;
        // Linked GIFs play only while someone can see them.
        self.fetch_linked(std::time::Instant::now());
        if !focused {
            self.write_drafts(std::time::Instant::now(), true);
            return;
        }
        self.breath.ask();
        for (account, key) in self.open_timelines.clone() {
            self.accounts.mark_read(&account, &key.room_id);
            let marker = self
                .accounts
                .get(&account)
                .and_then(|data| data.timeline(&key))
                .and_then(|timeline| timeline.visible_read_marker.clone());
            if !marker.is_some_and(|id| self.read_marker_reached(&account, &key, &id)) {
                self.mark_timeline_read(account, key);
            }
        }
    }

    /// Called every frame: loads the page of history before the oldest message of a timeline
    /// scrolled near its top. True when it changed state.
    pub fn poll(&mut self, tx: &mut Tx) -> bool {
        let mut changed = self.expire_notice(tx);
        changed |= self.accounts.poll_timelines(tx, self.reduced_motion.get());
        let now = std::time::Instant::now();
        for command in self.receipts.due(now) {
            self.dispatch(command, Purpose::Read);
        }
        if let Some(wait) = self.receipts.wait(now) {
            tx.wake_after(wait);
        }
        // Images decoded since the last frame count at their exact size from now on.
        changed |= self.media.measure();
        changed |= self.preview.as_mut().is_some_and(|preview| preview.content.poll());
        // However the preview moved (a key, an arrow, a thumbnail, opening), its thumbnail shows.
        if std::mem::take(&mut self.reveal_thumbnail)
            && let Some(preview) = self.preview.as_ref().filter(|preview| preview.batch.len() > 1)
        {
            tx.scroll_into_view_in("preview-strip", format!("preview-thumb-{}", preview.index));
        }
        changed |= self.call.as_mut().is_some_and(Call::measure_videos);
        self.retry_due(now, tx);
        changed |= self.search_people(now);
        changed |= self.position_message(tx);
        // Searches go out for the active account; with none, nothing waits.
        if let Some(wait) = [&self.search, &self.invite_search, &self.chat_search]
            .iter()
            .filter_map(|search| search.wait(now))
            .min()
            .filter(|_| self.nav.account.is_some())
        {
            tx.wake_after(wait);
        }
        self.send_typing();
        self.keep_drafts(now, tx);
        // A voice memo's timer counts seconds, and a playing clip's progress moves in steps:
        // a tick a little faster than either is enough, and the view rebuilds only when what
        // shows changes.
        let clock = self.clock();
        if let Some((_, audio, video)) = clock {
            let playing = audio.is_some() || video.is_some();
            tx.wake_after(std::time::Duration::from_millis(if playing { 50 } else { 250 }));
        }
        if clock != self.clock_shown {
            self.clock_shown = clock;
            changed = true;
        }
        for (flag, thread) in [(Arc::clone(&self.near_top), false), (Arc::clone(&self.thread_near_top), true)] {
            if !flag.swap(false, Ordering::Relaxed) {
                continue;
            }
            let Some((account, key)) =
                self.open_timelines.iter().find(|(_, key)| key.thread.is_some() == thread).cloned()
            else {
                continue;
            };
            // Checked before asking to change it: a change moves the revision and replans rows.
            let idle = self
                .accounts
                .get(&account)
                .and_then(|data| data.timeline(&key))
                .is_some_and(|t| !t.loading && !t.reached_start && t.history_error.is_none());
            if !idle {
                continue;
            }
            let Some(timeline) = self.accounts.kept_timeline_mut(&account, &key) else { continue };
            timeline.loading = true;
            let purpose = Purpose::History { account: account.clone(), key: key.clone() };
            self.dispatch(Command::LoadOlder { account, key }, purpose);
            changed = true;
        }
        changed
    }

    /// Moves animations driven by the clock (seconds since start, virtual when headless) on:
    /// the backdrop of the sign-in and launch screens, while it shows in a focused window. True
    /// when that changed what shows.
    pub fn animate(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let (focused, still) = (self.window_focused, self.reduced_motion.get());
        // A memo whose microphone failed ends now, with why, rather than on send.
        let mut ended = false;
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            if let Some(error) = self.composer(target).voice.as_ref().and_then(|memo| memo.recorder.failed()) {
                self.cancel_voice_memo(target);
                self.notice = Some(tf!("notice.voice", error = error));
                ended = true;
            }
        }
        // The selected rail tile breathes a while in a focused window, then rests.
        if focused {
            let (rested, wait) = self.breath.advance(clock);
            ended |= rested;
            if let Some(wait) = wait {
                tx.wake_after(wait);
            }
        }
        // The call's videos stream while its stage shows; its glow moves while it shows in a
        // focused window.
        self.sync_watching();
        let stage = self.call_here().is_some();
        if let Some(call) = self.call.as_mut() {
            if !stage || !focused || still {
                call.glow.pause();
            } else {
                let levels = call.glow_levels();
                call.glow.advance(clock, &levels);
                tx.wake_after(call.glow.wait(clock));
            }
        }
        if !matches!(self.screen(), Screen::SignIn | Screen::Restoring) && !matches!(self.modal, Some(Modal::SignIn)) {
            self.backdrop.hidden();
            return ended;
        }
        if !focused {
            return ended;
        }
        let (moved, wait) = self.backdrop.advance_to(clock, still);
        if let Some(wait) = wait {
            tx.wake_after(wait);
        }
        moved || ended
    }

    /// The text in the composers on screen, as typing notices follow it: the room's, then the
    /// thread's while one is open (how many of the two count). Checked every frame, so nothing
    /// is allocated.
    fn typed(&self) -> ([&str; 2], usize) {
        let shown = if self.active_thread().is_some() { 2 } else { 1 };
        ([self.composer.edit.text.as_str(), self.thread_composer.edit.text.as_str()], shown)
    }

    /// A draft came back into a composer: that isn't typing.
    /// `slots` are the composers (0 the room's, 1 the thread's) whose draft came back.
    fn settle_typing(&mut self, slots: &[usize]) {
        let mut typing = std::mem::take(&mut self.typing);
        let (drafts, shown) = self.typed();
        typing.seen(drafts.get(..shown).unwrap_or_default(), slots);
        self.typing = typing;
    }

    /// Tells the room shown whether the user is typing in it, from its composers' drafts.
    fn send_typing(&mut self) {
        let mut typing = std::mem::take(&mut self.typing);
        let room = match (&self.nav.account, self.active_room_id(), self.screen()) {
            (Some(account), Some(room_id), Screen::Chat) => Some((account, room_id)),
            _ => None,
        };
        let (drafts, shown) = self.typed();
        let notices = typing.update(room, drafts.get(..shown).unwrap_or_default(), std::time::Instant::now());
        self.typing = typing;
        for typing::Notice { account, room_id, typing } in notices {
            self.dispatch(Command::Typing { account, room_id, typing }, Purpose::Typing);
        }
    }

    /// Tries loading older history again after it failed.
    pub fn retry_history(&mut self, key: &TimelineKey) {
        let Some(account) = self.nav.account.clone() else { return };
        if !self.open_timelines.contains(&(account.clone(), key.clone())) {
            // The timeline itself never opened: open it again.
            if let Some(timeline) = self.accounts.kept_timeline_mut(&account, key) {
                timeline.history_error = None;
            }
            self.sync_timeline();
            return;
        }
        let Some(timeline) = self.accounts.kept_timeline_mut(&account, key) else { return };
        timeline.history_error = None;
        timeline.loading = true;
        let purpose = Purpose::History { account: account.clone(), key: key.clone() };
        self.dispatch(Command::LoadOlder { account, key: key.clone() }, purpose);
    }

    /// The rows of a timeline the view shows (with a margin either side), by their entries'
    /// IDs: their pictures, previews and GIFs load, and nothing scrolled past does.
    pub fn rows_shown(&mut self, account: &AccountId, key: &TimelineKey, entries: std::ops::Range<usize>) {
        let Some(timeline) = self.accounts.get(account).and_then(|data| data.timeline(key)) else { return };
        let ids: HashSet<String> =
            timeline.entries.get(entries.clone()).unwrap_or_default().iter().map(|entry| entry.id.clone()).collect();
        let shown = accounts::Shown { revision: timeline.revision, range: entries, ids };
        let open = (account.clone(), key.clone());
        let same_rows = self.on_screen.get(&open).is_some_and(|known| known.ids == shown.ids);
        self.on_screen.insert(open, shown);
        if same_rows {
            return;
        }
        let now = std::time::Instant::now();
        self.request_timeline_media_at(now);
        self.fetch_linked(now);
    }

    /// The "New" marker itself entered the viewport, without the media prefetch margin.
    pub fn read_marker_reached(&mut self, account: &AccountId, key: &TimelineKey, id: &str) -> bool {
        if !self.window_focused {
            return false;
        }
        if !self.accounts.dismiss_read_marker(account, key, id) {
            return false;
        }
        self.mark_timeline_read(account.clone(), key.clone());
        true
    }

    pub fn read_marker_visible(&mut self, account: &AccountId, key: &TimelineKey, marker: Option<String>) -> bool {
        self.accounts.set_visible_read_marker(account, key, marker.clone());
        marker.is_some_and(|id| self.read_marker_reached(account, key, &id))
    }

    fn mark_timeline_read(&mut self, account: AccountId, key: TimelineKey) {
        if self.message_jump.is_some() {
            return;
        }
        if !self.timelines_ready.contains(&(account.clone(), key.clone())) {
            return;
        }
        let event_id = self
            .accounts
            .get(&account)
            .and_then(|data| data.timeline(&key))
            .and_then(|timeline| timeline.read_up_to.clone());
        let fully_read = key.thread.is_none()
            && self
                .accounts
                .get(&account)
                .and_then(|data| data.timeline(&key))
                .is_some_and(RoomTimeline::may_mark_fully_read);
        if let Some(command) = self.receipts.update(account, key, event_id, fully_read, std::time::Instant::now()) {
            self.dispatch(command, Purpose::Read);
        }
    }

    pub fn read_position_shown(&mut self, account: &AccountId, key: &TimelineKey, event_id: String) -> bool {
        if !self.accounts.set_read_position(account, key, event_id) {
            return false;
        }
        if self.window_focused {
            self.mark_timeline_read(account.clone(), key.clone());
        }
        true
    }

    /// The entries of the open timelines on screen, with their account and timeline.
    fn entries_on_screen(&self) -> impl Iterator<Item = ((&AccountId, &TimelineKey), &accounts::Entry)> {
        self.open_timelines.iter().flat_map(move |open| {
            let (account, key) = open;
            let shown = self.on_screen.get(open);
            self.accounts
                .get(account)
                .and_then(|data| data.timeline(key))
                .into_iter()
                .flat_map(move |timeline| timeline.on_screen(shown).map(move |entry| ((account, key), entry)))
        })
    }

    /// Lets go of the GIFs of timelines no longer open (all of them, with linked GIFs off),
    /// then fetches those on screen. Call when what is open changes; scrolling only fetches.
    fn request_gifs(&mut self) {
        // GIFs of timelines no longer shown let go of their frames; those scrolled past stay
        // loaded (paused), so scrolling back shows them at once.
        let kept: HashSet<String> = if self.settings.show_link_gifs {
            self.open_timelines
                .iter()
                .filter_map(|(account, key)| self.accounts.get(account)?.timeline(key))
                .flat_map(RoomTimeline::gif_links)
                .map(str::to_owned)
                .collect()
        } else {
            HashSet::new()
        };
        self.gifs.keep_only(&kept);
        self.fetch_linked(std::time::Instant::now());
    }

    /// Fetches what the links on screen show and isn't here, as far as it may be tried at `now`:
    /// their cards and their GIFs. The GIFs on screen play while the window is focused; the rest
    /// hold their frame.
    fn fetch_linked(&mut self, now: std::time::Instant) {
        self.fetch_link_previews(now);
        if !self.settings.show_link_gifs {
            return;
        }
        let shown: HashSet<String> =
            self.entries_on_screen().filter_map(|(_, entry)| entry.gif_link()).map(str::to_owned).collect();
        let mut wanted: Vec<String> = shown.iter().filter(|url| self.gifs.wants(url, now)).cloned().collect();
        self.gifs.show(shown, self.window_focused);
        wanted.sort_unstable();
        for url in wanted {
            self.gifs.loading(&url);
            self.dispatch(Command::FetchGif { url: url.clone() }, Purpose::Gif(url));
        }
    }

    /// Asks for the preview cards of the links on screen that aren't here and may be asked about
    /// at `now`.
    fn fetch_link_previews(&mut self, now: std::time::Instant) {
        if !self.settings.show_link_previews {
            return;
        }
        let gifs = self.settings.show_link_gifs;
        let shown: HashSet<link_previews::LinkKey> = self
            .entries_on_screen()
            .filter_map(|((account, _), entry)| Some((account.clone(), entry.preview_link(gifs)?.to_owned())))
            .collect();
        self.link_previews.trim(&shown);
        let mut wanted: Vec<_> = shown.into_iter().filter(|key| self.link_previews.wants(key, now)).collect();
        wanted.sort_unstable();
        for key in wanted {
            self.link_previews.loading(key.clone());
            let (account, url) = key.clone();
            self.dispatch(Command::PreviewLink { account, url }, Purpose::LinkPreview(key));
        }
    }

    /// Fetches what the timeline rows on screen show and isn't loaded yet: image previews, and
    /// the pictures of senders and of the people room events name; and the previews of the
    /// preview strip's batch.
    fn request_timeline_media(&mut self) {
        self.request_timeline_media_at(std::time::Instant::now());
    }

    fn request_timeline_media_at(&mut self, now: std::time::Instant) {
        let mut seen = HashSet::new();
        let mut wanted = Vec::new();
        for ((account, key), entry) in self.entries_on_screen() {
            if let Some(attachment) = entry.image()
                && let Some((media, size)) = attachment.preview()
            {
                let dimensions = media::preview_size(size, attachment.dimensions);
                wanted.push((account.clone(), media, size, dimensions));
            }
            if key.thread.is_none()
                && let Some(message) = entry.message().filter(|m| !m.pending)
            {
                for receipt in message.read_receipts.iter().take(READER_FACES) {
                    if let Some(picture) = self
                        .accounts
                        .get(account)
                        .map(|data| data.reader(&key.room_id, &receipt.user_id))
                        .and_then(|person| person.avatar)
                    {
                        wanted.push((account.clone(), picture, AVATAR_SIZE, Some(AVATAR_PIXELS)));
                    }
                }
            }
            for media in entry.avatars() {
                wanted.push((account.clone(), media.clone(), AVATAR_SIZE, Some(AVATAR_PIXELS)));
            }
            // A link card's image.
            if self.settings.show_link_previews
                && let Some((media, size)) = entry
                    .preview_link(self.settings.show_link_gifs)
                    .and_then(|url| self.link_previews.get(account, url))
                    .and_then(|card| card.image.as_ref())
            {
                let dimensions = media::preview_size(LINK_THUMB, *size);
                wanted.push((account.clone(), media.clone(), LINK_THUMB, dimensions));
            }
        }
        // The preview's strip shows its batch's previews, rows on screen or not.
        if let Some(preview) = self.preview.as_ref().filter(|preview| preview.batch.len() > 1) {
            for attachment in preview.batch.iter().map(|item| &item.attachment) {
                if let Some((media, size)) = attachment.preview() {
                    let dimensions = media::preview_size(size, attachment.dimensions);
                    wanted.push((preview.account.clone(), media, size, dimensions));
                }
            }
        }
        // What shows counts as just used, so making room evicts what was scrolled past first.
        self.media.touch(wanted.iter().map(|(_, media, _, _)| media));
        wanted.retain(|(_, media, _, _)| self.media.wants(media, now) && seen.insert(media.clone()));
        for (account, media, size, dimensions) in wanted {
            self.media.loading(media.clone());
            let command = Command::FetchMedia { account, media: media.clone(), size };
            self.dispatch(command, Purpose::Media { media, dimensions });
        }
    }

    /// Tries failed downloads again once their wait is over, if they are still on screen.
    pub(crate) fn retry_due(&mut self, now: std::time::Instant, tx: &mut Tx) {
        let Some(at) = self.retry_at else { return };
        if at > now {
            tx.wake_after(at - now);
            return;
        }
        self.retry_at = None;
        self.request_timeline_media_at(now);
        self.fetch_linked(now);
        // The lists' pictures that failed are asked for again too.
        self.avatars_listed = None;
        self.listed_pictures = None;
        self.request_avatars();
        self.schedule_retry_after(now);
        if let Some(at) = self.retry_at {
            tx.wake_after(at.saturating_duration_since(now));
        }
    }

    /// Notes when the next failed download may be tried again.
    fn schedule_retry(&mut self) {
        self.schedule_retry_after(std::time::Instant::now());
    }

    fn schedule_retry_after(&mut self, now: std::time::Instant) {
        self.retry_at = [self.media.next_retry(now), self.gifs.next_retry(now), self.link_previews.next_retry(now)]
            .into_iter()
            .flatten()
            .min();
    }

    /// Fetches the pictures on screen that aren't loaded yet, but for the timelines' (see
    /// [`State::request_timeline_media`]): every account's, and the active account's spaces,
    /// rooms, members, pinned-message senders, people typing and people found.
    fn request_avatars(&mut self) {
        let now = std::time::Instant::now();
        self.request_listed_avatars(now);
        let mut wanted: Vec<(AccountId, MediaRef)> = Vec::new();
        if let Some(account) = self.active_account() {
            let id = &account.summary.id;
            // The header shows three faces; the open list, those near its top. A room of
            // thousands doesn't ask for thousands of pictures.
            let shown = if self.side == Some(SidePanel::Members) { MEMBER_PICTURES } else { 3 };
            let members = self.active_room_id().and_then(|room_id| account.members.get(room_id)).into_iter().flat_map(
                move |members| members.people.iter().take(shown).filter_map(|member| member.person.avatar.clone()),
            );
            let pins = self
                .active_room_id()
                .filter(|_| self.side == Some(SidePanel::Pins))
                .and_then(|room| account.pins.get(room))
                .into_iter()
                .flat_map(|pins| pins.iter().take(MEMBER_PICTURES).filter_map(|pin| pin.sender_avatar.clone()));
            // The open person panel's picture: the member list's, else the message's.
            let person = match (&self.side, self.active_room_id()) {
                (Some(SidePanel::Person(shown_as)), Some(room_id)) => account
                    .member(room_id, &shown_as.id)
                    .map_or_else(|| shown_as.avatar.clone(), |member| member.person.avatar.clone()),
                _ => None,
            };
            let typing = self
                .active_room_id()
                .and_then(|room_id| account.typing.get(room_id))
                .into_iter()
                .flat_map(|people| people.iter().filter_map(|person| person.avatar.clone()));
            let found = [&self.search, &self.invite_search, &self.chat_search]
                .into_iter()
                .flat_map(|search| search.people.iter().filter_map(|person| person.avatar.clone()));
            let public = [&self.search.public_rooms, &self.search.public_spaces]
                .into_iter()
                .filter(|_| self.popover == Some(Popover::Search))
                .flat_map(|source| {
                    source
                        .rooms
                        .iter()
                        .filter(|room| account.room(&room.room_id).is_none() && account.space(&room.room_id).is_none())
                        .take(6)
                        .filter_map(|room| room.avatar.clone())
                });
            wanted.extend(
                members
                    .chain(person)
                    .chain(pins)
                    .chain(typing)
                    .chain(found)
                    .chain(public)
                    .map(|media| (id.clone(), media)),
            );
        }
        // The faces of the call and of whoever rings, from any account.
        if let Some(call) = &self.call {
            let faces = call.info.participants.iter().filter_map(|participant| participant.person.avatar.clone());
            wanted.extend(faces.map(|media| (call.info.account.clone(), media)));
        }
        wanted.extend(self.rings.iter().filter_map(|ring| Some((ring.account.clone(), ring.caller.avatar.clone()?))));
        wanted.extend(self.reader_pictures());
        wanted.extend(self.link_pictures());
        // Pictures on screen count as just used, so making room for previews keeps them.
        self.media.touch(wanted.iter().map(|(_, media)| media));
        self.fetch_avatars(wanted, now);
    }

    /// Fetches the pictures the lists show (every account's own, and the active account's
    /// rooms, spaces and invitations) once those lists change: the media cache keeps them
    /// outside its budget (`MediaCache::keep`), so nothing needs to touch them in between.
    /// When the lists change, any of their pictures not here (a failed one whose wait is over)
    /// is asked for again, even if the lists show the same pictures.
    fn request_listed_avatars(&mut self, now: std::time::Instant) {
        if !self.media.can_load() {
            return;
        }
        let listed = (self.nav.account.clone(), self.accounts.revision());
        if self.avatars_listed.as_ref() == Some(&listed) {
            return;
        }
        self.avatars_listed = Some(listed);
        let pictures = || {
            let own = self
                .accounts
                .iter()
                .filter_map(|account| Some((&account.summary.id, account.summary.avatar.as_ref()?)));
            let active = self.active_account().into_iter().flat_map(|account| {
                let id = &account.summary.id;
                let rooms = account.rooms.iter().filter_map(|room| room.avatar.as_ref());
                let spaces = account.spaces.iter().filter_map(|space| space.avatar.as_ref());
                let invitations = account.invitations.iter().filter_map(|invite| invite.avatar.as_ref());
                rooms.chain(spaces).chain(invitations).map(move |media| (id, media))
            });
            own.chain(active)
        };
        let (fingerprint, missing) = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            let mut missing = false;
            for picture in pictures() {
                picture.hash(&mut hasher);
                missing = missing || self.media.wants(picture.1, now);
            }
            (hasher.finish(), missing)
        };
        let same = self.listed_pictures == Some(fingerprint);
        if same && !missing {
            return;
        }
        let wanted: Vec<(AccountId, MediaRef)> =
            pictures().map(|(account, media)| (account.clone(), media.clone())).collect();
        if !same {
            self.listed_pictures = Some(fingerprint);
            self.media.keep(wanted.iter().map(|(_, media)| media.clone()).collect());
        }
        self.fetch_avatars(wanted, now);
    }

    /// Asks for each picture of `wanted` that isn't here and may be asked for at `now`, once.
    fn fetch_avatars(&mut self, mut wanted: Vec<(AccountId, MediaRef)>, now: std::time::Instant) {
        let mut seen = HashSet::new();
        wanted.retain(|(_, media)| self.media.wants(media, now) && seen.insert(media.clone()));
        for (account, media) in wanted {
            self.media.loading(media.clone());
            let command = Command::FetchMedia { account, media: media.clone(), size: AVATAR_SIZE };
            self.dispatch(command, Purpose::Media { media, dimensions: Some(AVATAR_PIXELS) });
        }
    }

    // ----- making rooms and spaces ----------------------------------------------------------

    /// Opens the new-chat search from the sidebar's chat button.
    pub fn open_new_chat(&mut self) {
        self.chat_search = PeopleSearch::default();
        self.chat_search.field.focused = true;
        self.toggle_popover(Popover::NewChat);
        // Until something is typed, the search suggests the people the user shares rooms with.
        if let Some(account) = self.nav.account.clone() {
            let request = self.dispatch(Command::SuggestPeople(account), Purpose::People);
            self.chat_search.request = Some(request);
        }
    }

    /// Opens the room form.
    pub fn open_room_form(&mut self, form: RoomForm) {
        let name = match &form {
            RoomForm::Rename { room_id } => self.active_account().and_then(|account| {
                account
                    .room(room_id)
                    .map(|room| room.name.clone())
                    .or_else(|| account.space(room_id).map(|s| s.name.clone()))
            }),
            _ => None,
        };
        self.popover = None;
        let name = Box::new(focused(TextEdit::new(name.unwrap_or_default())));
        self.modal = Some(Modal::RoomForm { form, name, topic: Box::new(TextEdit::new("")) });
    }

    /// A field of the room form: its name, or with `topic` its topic.
    pub fn room_form_field(&mut self, topic: bool) -> &mut TextEdit {
        match &mut self.modal {
            Some(Modal::RoomForm { name, topic: field, .. }) => {
                if topic {
                    field.as_mut()
                } else {
                    name.as_mut()
                }
            }
            _ => &mut self.scratch,
        }
    }

    pub fn submit_room_form(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        let Some(Modal::RoomForm { form, name, topic }) = self.modal.take() else { return };
        let name = name.text.trim().to_owned();
        if name.is_empty() {
            self.modal = Some(Modal::RoomForm { form, name: Box::new(TextEdit::new("")), topic });
            return;
        }
        match form {
            RoomForm::NewRoom { space } => {
                let topic = Some(topic.text.trim().to_owned()).filter(|t| !t.is_empty());
                let purpose = Purpose::Create { account: account.clone(), space: false };
                self.dispatch(Command::CreateRoom { account, name, topic, space }, purpose);
            }
            RoomForm::NewSpace => {
                let purpose = Purpose::Create { account: account.clone(), space: true };
                self.dispatch(Command::CreateSpace { account, name }, purpose);
            }
            RoomForm::Rename { room_id } => {
                self.dispatch(Command::RenameRoom { account, room_id, name }, Purpose::Rename);
            }
        }
    }

    /// Asks before leaving a space (leaving it removes it from the rail).
    pub fn confirm_leave_space(&mut self, space_id: &str) {
        let (Some(account), Some(name)) = (
            self.nav.account.clone(),
            self.active_account().and_then(|a| a.space(space_id)).map(|space| space.name.clone()),
        ) else {
            return;
        };
        self.popover = None;
        self.modal = Some(Modal::Confirm(Confirm::LeaveRoom { account, room_id: space_id.to_owned(), name }));
    }

    // ----- sidebar sections --------------------------------------------------------------------

    /// Changes the active account's sidebar layout for the current scope, and saves it.
    fn change_layout(&mut self, change: impl FnOnce(&mut matrix_core::SidebarLayout, &Scope)) {
        let Some(id) = self.nav.account.clone() else { return };
        let scope = self.active_scope();
        let Some(account) = self.accounts.get(&id) else { return };
        // Saving before the saved layout is read would replace it on every device.
        if !account.sidebar_loaded {
            self.notice = Some(t!("notice.layout_loading").to_owned());
            return;
        }
        let mut layout = account.sidebar.clone();
        change(&mut layout, &scope);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        // Later than the layout it changes, even when this device's clock is behind.
        layout.changed_at = u64::try_from(now).unwrap_or(u64::MAX).max(layout.changed_at + 1);
        self.accounts.set_sidebar(&id, layout.clone());
        self.save_layout(id, layout);
    }

    fn save_layout(&mut self, account: AccountId, layout: matrix_core::SidebarLayout) {
        if self.saving_layout(&account) {
            // Saved once the save under way finishes, with whatever the layout is by then.
            self.layout_unsaved.insert(account);
            return;
        }
        let purpose = Purpose::Layout(account.clone());
        self.dispatch(Command::SetSidebarLayout { account, layout }, purpose);
    }

    /// A save just finished: saves the changes made meanwhile.
    fn save_layout_again(&mut self, account: AccountId) {
        if !self.layout_unsaved.remove(&account) {
            return;
        }
        if let Some(layout) = self.accounts.get(&account).map(|data| data.sidebar.clone()) {
            self.save_layout(account, layout);
        }
    }

    /// Whether this session has layout changes the server may not have yet. A layout arriving
    /// from sync meanwhile is older than what shows.
    fn saving_layout(&self, account: &AccountId) -> bool {
        self.layout_unsaved.contains(account)
            || self.pending.values().any(|purpose| matches!(purpose, Purpose::Layout(id) if id == account))
    }

    pub fn toggle_section(&mut self, id: &str) {
        self.change_layout(|layout, scope| sections::toggle_collapsed(layout, scope, id));
    }

    pub fn section_collapsed(&self, id: &str) -> bool {
        self.active_account()
            .and_then(|account| account.sidebar.scopes.get(&sections::scope_key(&self.active_scope())))
            .is_some_and(|scope| scope.collapsed.iter().any(|section| section == id))
    }

    /// Asks for a new section's name.
    pub fn new_section(&mut self) {
        self.popover = None;
        self.modal = Some(Modal::SectionName { section: None, edit: focused(TextEdit::new("")) });
    }

    /// Asks for a section's new name.
    pub fn rename_section(&mut self, id: &str) {
        let scope = self.active_scope();
        let name = self.active_account().and_then(|account| {
            let layout = account.sidebar.scopes.get(&sections::scope_key(&scope))?;
            layout.sections.iter().find(|s| s.id == id).map(|s| s.name.clone())
        });
        self.popover = None;
        let edit = focused(TextEdit::new(name.unwrap_or_default()));
        self.modal = Some(Modal::SectionName { section: Some(id.to_owned()), edit });
    }

    pub fn submit_section_name(&mut self) {
        let Some(Modal::SectionName { section, edit }) = self.modal.take() else { return };
        let name = edit.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        self.change_layout(|layout, scope| match &section {
            Some(id) => sections::rename_section(layout, scope, id, &name),
            None => {
                sections::add_section(layout, scope, &name);
            }
        });
    }

    pub fn delete_section(&mut self, id: &str) {
        self.popover = None;
        self.change_layout(|layout, scope| sections::delete_section(layout, scope, id));
    }

    /// Where a section is drawn, for dropping rooms on it.
    pub fn section_zone(&mut self, id: &str, rect: [f32; 4]) {
        match self.section_zones.iter_mut().find(|(known, _)| known == id) {
            Some((_, zone)) => *zone = rect,
            None => self.section_zones.push((id.to_owned(), rect)),
        }
    }

    /// Where a sidebar room row is drawn, for placing a dropped room among its neighbours.
    pub fn row_zone(&mut self, room_id: &str, rect: [f32; 4]) {
        match self.row_zones.get_mut(room_id) {
            Some(zone) => *zone = rect,
            None => {
                self.row_zones.insert(room_id.to_owned(), rect);
            }
        }
    }

    /// Where a space's tile is drawn on the rail, for placing a dropped space among the others.
    pub fn space_zone(&mut self, room_id: &str, rect: [f32; 4]) {
        match self.space_zones.get_mut(room_id) {
            Some(zone) => *zone = rect,
            None => {
                self.space_zones.insert(room_id.to_owned(), rect);
            }
        }
    }

    /// The spaces on the rail now, in order.
    fn shown_spaces(&self) -> Vec<String> {
        let Some(account) = self.active_account() else { return Vec::new() };
        sections::ordered_spaces(&account.spaces, &account.sidebar)
            .into_iter()
            .map(|space| space.room_id.clone())
            .collect()
    }

    /// The sections as shown now, in order.
    fn shown_sections(&self) -> Vec<String> {
        let Some(account) = self.active_account() else { return Vec::new() };
        sections(account, &account.sidebar, &self.active_scope()).into_iter().map(|view| view.id).collect()
    }

    /// Where something dragged to `point` would land.
    fn drop_target(&self, item: &Dragged, [x, y]: [f32; 2]) -> Option<DropTarget> {
        // Only sections on screen count: zones from another scope linger until redrawn.
        let shown = self.shown_sections();
        let zone_of = |id: &str| self.section_zones.iter().find(|(known, _)| known == id).map(|(_, zone)| *zone);
        match item {
            // Above the first other section whose middle is below the pointer.
            Dragged::Section(id) => {
                let index = shown
                    .iter()
                    .filter(|shown| *shown != id)
                    .take_while(|shown| zone_of(shown).is_some_and(|[_, zy, _, zh]| zy + zh / 2.0 < y))
                    .count();
                Some(DropTarget::Section { index })
            }
            // Above the first other space whose middle is below the pointer.
            Dragged::Space(id) => {
                let index = self
                    .shown_spaces()
                    .iter()
                    .filter(|shown| *shown != id)
                    .take_while(|shown| self.space_zones.get(*shown).is_some_and(|[_, zy, _, zh]| zy + zh / 2.0 < y))
                    .count();
                Some(DropTarget::Space { index })
            }
            // Into the section under the pointer, above the first row whose middle is below it.
            Dragged::Room(room_id) => {
                let (section, _) = self.section_zones.iter().find(|(id, [zx, zy, zw, zh])| {
                    shown.contains(id) && x >= *zx && x <= zx + zw && y >= *zy && y <= zy + zh
                })?;
                let account = self.active_account()?;
                let view = sections(account, &account.sidebar, &self.active_scope());
                let rooms = &view.iter().find(|s| s.id == *section)?.rooms;
                let index = rooms
                    .iter()
                    .filter(|room| room.room_id != *room_id)
                    .take_while(|room| {
                        self.row_zones.get(&room.room_id).is_some_and(|[_, ry, _, rh]| ry + rh / 2.0 < y)
                    })
                    .count();
                Some(DropTarget::Room { section: section.clone(), index })
            }
        }
    }

    /// A room row, section header or rail space being dragged: it follows the pointer, shows
    /// where it would land, and lands there when released. A press that never moves stays a press.
    pub fn drag_in_sidebar(&mut self, item: Dragged, drag: &rune::Drag) {
        /// How far the pointer travels before a press becomes a drag.
        const THRESHOLD: f32 = 4.0;
        let point = drag.window_point();
        match drag.phase {
            rune::Phase::Down => {}
            rune::Phase::Move => {
                if self.dragging.is_some() || drag.dx.abs().max(drag.dy.abs()) > THRESHOLD {
                    let target = self.drop_target(&item, point);
                    self.dragging = Some(SidebarDrag { item, point, target });
                }
            }
            rune::Phase::Up => {
                let Some(SidebarDrag { item, target: Some(target), .. }) = self.dragging.take() else { return };
                let (shown, spaces) = (self.shown_sections(), self.shown_spaces());
                self.change_layout(|layout, scope| match (item, target) {
                    (Dragged::Room(room_id), DropTarget::Room { section, index }) => {
                        sections::move_room(layout, scope, &room_id, &section, index);
                    }
                    (Dragged::Section(id), DropTarget::Section { index }) => {
                        sections::move_section(layout, scope, &shown, &id, index);
                    }
                    (Dragged::Space(id), DropTarget::Space { index }) => {
                        sections::move_space(layout, &spaces, &id, index);
                    }
                    _ => {}
                });
            }
        }
    }

    // ----- people ----------------------------------------------------------------------------

    /// Sends the user-directory searches (the top bar's, the invite dialog's, the new chat's)
    /// whose text has rested by `now`. True when that changed what shows: the top bar's
    /// results opened or closed.
    pub fn search_people(&mut self, now: std::time::Instant) -> bool {
        let Some(account) = self.nav.account.clone() else { return false };
        let changed = self.search_destinations(now, &account);
        if let Some(term) = self.invite_search.due(now) {
            let request = self.dispatch(Command::SearchPeople { account: account.clone(), term }, Purpose::People);
            self.invite_search.request = Some(request);
        }
        if let Some(term) = self.chat_search.due(now) {
            let request = self.dispatch(Command::SearchPeople { account, term }, Purpose::People);
            self.chat_search.request = Some(request);
        }
        changed
    }

    /// Opens the invite dialog for the open room.
    pub fn open_invite(&mut self) {
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        self.popover = None;
        self.invite_search = PeopleSearch::default();
        self.invite_search.field.focused = true;
        self.modal = Some(Modal::Invite { account, room_id, invited: Vec::new() });
        // Who is already in the room shows as such in the results.
        self.list_all_members();
    }

    /// Invites someone from the invite dialog's search results.
    pub fn invite(&mut self, user_id: &str) {
        let Some(Modal::Invite { account, room_id, invited }) = &mut self.modal else { return };
        if invited.iter().any(|known| known == user_id) {
            return;
        }
        invited.push(user_id.to_owned());
        let command =
            Command::Invite { account: account.clone(), room_id: room_id.clone(), user_id: user_id.to_owned() };
        self.dispatch(command, Purpose::Invite);
    }

    /// Opens the direct chat with someone, starting one if there is none.
    pub fn open_direct_chat(&mut self, user_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.reset_search();
        self.chat_search = PeopleSearch::default();
        let purpose = Purpose::DirectChat(account.clone());
        self.dispatch(Command::OpenDirectChat { account, user_id: user_id.to_owned() }, purpose);
    }

    /// A room or direct chat picked in the top bar's search.
    pub fn open_found_room(&mut self, room_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.reset_search();
        self.open_room(&account, room_id);
    }

    /// A space picked in the top bar's search.
    pub fn open_found_space(&mut self, space_id: &str) {
        self.popover = None;
        self.reset_search();
        self.select_scope(Scope::Space(space_id.to_owned()));
    }

    // ----- audio -----------------------------------------------------------------------------

    /// Plays an audio attachment or voice memo, or pauses it when it is the one playing. The
    /// file downloads once, to a temporary file the player opens.
    pub fn toggle_audio(&mut self, attachment: &matrix_core::Attachment) {
        let Some(media) = attachment.media.clone() else { return };
        if let Some(clip) = self.audio.as_mut().filter(|clip| clip.media == media) {
            match &clip.player {
                Some(player) if player.is_playing() => {
                    player.pause();
                    clip.play = false;
                }
                Some(player) => {
                    player.play();
                    clip.play = true;
                }
                // Still downloading: a press queues playing it once here (from the start if it
                // was moved to the end meanwhile), and another takes that back.
                None => {
                    clip.play = !clip.play;
                    if clip.play {
                        clip.start_at = clip.start_at.filter(|share| *share < 1.0);
                    }
                    clip.held = false;
                }
            }
            return;
        }
        self.load_audio(attachment, None, false);
    }

    /// Moves a clip to `share` of the way through (0 the start, 1 the end), as dragging along
    /// its waveform does. A clip that isn't the current one starts playing from there. Moved to
    /// the very end, a clip waits there until play starts it over, rather than ending and
    /// rewinding at once.
    pub fn seek_audio(&mut self, attachment: &matrix_core::Attachment, share: f64) {
        let Some(media) = attachment.media.clone() else { return };
        let share = share.clamp(0.0, 1.0);
        let Some(clip) = self.audio.as_mut().filter(|clip| clip.media == media) else {
            return self.load_audio(attachment, Some(share), false);
        };
        // Mid-drag, the release decides whether it plays on.
        if share >= 1.0 && !clip.held {
            clip.play = false;
        }
        match &clip.player {
            Some(player) => {
                if share >= 1.0 {
                    player.pause();
                }
                player.seek(share);
            }
            None => clip.start_at = Some(share),
        }
    }

    /// A press or drag along a clip's waveform, at `share` of the way: it pauses while the
    /// pointer is down and goes on from where it is let go if it is to play (it was playing, or
    /// the press picked it), unless it was let go at the very end; there it waits at the end for
    /// play. A cancelled drag puts the clip back where it was, playing if it was, and unpicks a
    /// clip the press picked.
    pub fn scrub_audio(&mut self, attachment: &matrix_core::Attachment, share: f64, scrub: Scrub) {
        let share = share.clamp(0.0, 1.0);
        match scrub {
            Scrub::Press => match self.current_audio(attachment) {
                Some(clip) => {
                    if let Some(player) = &clip.player {
                        clip.play = player.is_playing();
                        player.pause();
                    }
                    clip.scrub_from = Some((clip.progress(), clip.play));
                    clip.held = true;
                }
                None => return self.load_audio(attachment, Some(share), true),
            },
            // Only a scrub under way goes on: a clip that failed meanwhile isn't picked again.
            _ if !self.current_audio(attachment).is_some_and(|clip| clip.held) => return,
            Scrub::Cancel => return self.cancel_scrub(attachment),
            Scrub::Drag | Scrub::Release => {}
        }
        self.seek_audio(attachment, share);
        let Some(clip) = self.current_audio(attachment) else { return };
        if scrub == Scrub::Release {
            clip.held = false;
            clip.scrub_from = None;
            // Let go at the very end, it waits there for play.
            clip.play &= share < 1.0;
            if let Some(player) = clip.player.as_ref().filter(|_| clip.play) {
                player.play();
            }
        }
    }

    /// A drag along a clip's waveform cancelled: see [`State::scrub_audio`].
    fn cancel_scrub(&mut self, attachment: &matrix_core::Attachment) {
        let Some(clip) = self.current_audio(attachment) else { return };
        clip.held = false;
        let Some((from, play)) = clip.scrub_from.take() else {
            self.audio = None;
            return;
        };
        clip.play = play;
        match &clip.player {
            Some(player) => {
                player.seek(from);
                if play {
                    player.play();
                }
            }
            None => clip.start_at = Some(from),
        }
    }

    /// The clip playing, if it is `attachment`'s.
    fn current_audio(&mut self, attachment: &matrix_core::Attachment) -> Option<&mut AudioClip> {
        self.audio.as_mut().filter(|clip| attachment.media.as_ref() == Some(&clip.media))
    }

    /// Moves a clip `steps` twentieths along (back when negative), as the arrow keys do on its
    /// waveform.
    pub fn step_audio(&mut self, attachment: &matrix_core::Attachment, steps: i32) {
        let at = self
            .audio
            .as_ref()
            .filter(|clip| attachment.media.as_ref() == Some(&clip.media))
            .map_or(0.0, AudioClip::progress);
        self.seek_audio(attachment, at + f64::from(steps) * AUDIO_STEP);
    }

    /// Steps the playback speed (1×, 1.5×, 2×), for the clip playing and every one after.
    pub fn cycle_playback_speed(&mut self) {
        self.settings.playback_speed = self.settings.playback_speed.next();
        self.shell.save_settings(self.settings.clone());
        if let Some(player) = self.audio.as_ref().and_then(|clip| clip.player.as_ref()) {
            player.set_rate(self.settings.playback_speed.rate());
        }
    }

    /// Picks a clip to play, from `start_at` of the way through when given: at once from its
    /// file when that is saved already, else once it has downloaded. `held` when a press on its
    /// waveform picks it: it waits to be let go.
    fn load_audio(&mut self, attachment: &matrix_core::Attachment, start_at: Option<f64>, held: bool) {
        let (Some(account), Some(media)) = (self.nav.account.clone(), attachment.media.clone()) else { return };
        self.remember_decoded_audio();
        self.last_audio_id += 1;
        let id = self.last_audio_id;
        // Picked by a press on its waveform, the release decides; else, picked at its very end,
        // it waits there for play.
        let play = held || start_at.is_none_or(|share| share < 1.0);
        self.audio = Some(AudioClip { id, media: media.clone(), player: None, start_at, play, held, scrub_from: None });
        match self.scratch_files.get(&media).cloned() {
            Some(ScratchFile::Saved(path)) => self.open_audio(path),
            Some(ScratchFile::Downloading) => {}
            None => {
                let path = self.scratch_path(attachment, &media, "audio");
                self.scratch_files.insert(media.clone(), ScratchFile::Downloading);
                let command = Command::SaveMedia { account, media: media.clone(), path: path.clone() };
                self.dispatch(command, Purpose::Audio { media, path });
            }
        }
    }

    /// Where `media`'s file goes in the scratch folder: named by a hash of it, keeping the
    /// attachment's extension so players recognise the format.
    fn scratch_path(&self, attachment: &matrix_core::Attachment, media: &MediaRef, kind: &str) -> PathBuf {
        let extension = std::path::Path::new(&attachment.name).extension().and_then(|e| e.to_str()).unwrap_or(kind);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(media, &mut hasher);
        self.shell.scratch().join(format!("{kind}-{:x}.{extension}", std::hash::Hasher::finish(&hasher)))
    }

    /// Opens the current clip's saved file in a player, unless it has one: at the speed chosen,
    /// from where it was scrubbed to, playing if it is to play; while the pointer is still down
    /// on its waveform, it waits to be let go.
    fn open_audio(&mut self, path: std::path::PathBuf) {
        let speaker = self.settings.devices.speaker.clone();
        let rate = self.settings.playback_speed.rate();
        let Some(clip) = self.audio.as_mut().filter(|clip| clip.player.is_none()) else { return };
        let player = self.shell.player(path, speaker, clip.media.clone(), clip.id);
        player.set_rate(rate);
        let share = clip.start_at.take();
        if let Some(share) = share {
            player.seek(share);
        }
        if clip.play && !clip.held {
            player.play();
        }
        clip.player = Some(player);
    }

    /// Keeps what the current clip's file said (its waveform and length) for its card, before
    /// another clip takes its place; the oldest goes past [`DECODED_AUDIO`].
    fn remember_decoded_audio(&mut self) {
        let Some(AudioClip { media, player: Some(player), .. }) = &self.audio else { return };
        let waveform = player.waveform();
        if waveform.is_empty() {
            return;
        }
        self.decoded_audio.retain(|decoded| decoded.media != *media);
        if self.decoded_audio.len() >= DECODED_AUDIO {
            self.decoded_audio.remove(0);
        }
        self.decoded_audio.push(DecodedAudio { media: media.clone(), waveform, duration: player.duration() });
    }

    // ----- preview ---------------------------------------------------------------------------

    /// Clicking an attachment in `key`'s timeline: an image, PDF, video or audio clip opens
    /// full size, with the rest of its batch to step through; any other file saves.
    pub fn open_attachment(&mut self, key: &TimelineKey, event_id: &str, attachment: &matrix_core::Attachment) {
        let (Some(account), Some(media)) = (self.nav.account.clone(), attachment.media.clone()) else { return };
        if !Opens::previews(attachment) {
            self.download(account, media, &attachment.name);
            return;
        }
        let entries = self.accounts.get(&account).and_then(|data| data.timeline(key)).map(|t| t.entries.as_slice());
        let mut batch = preview::batch(entries.unwrap_or_default(), event_id);
        let index = match batch.iter().position(|item| item.event_id == event_id) {
            Some(index) => index,
            None => {
                batch = vec![PreviewItem { event_id: event_id.to_owned(), attachment: attachment.clone() }];
                0
            }
        };
        self.close_preview();
        let content = self.preview_content(attachment);
        let (attachment, item) = (attachment.clone(), batch[index].attachment.clone());
        self.preview_visits += 1;
        self.reveal_thumbnail = true;
        let visit = self.preview_visits;
        self.preview = Some(Preview { account, attachment, media, content, batch, index, visit });
        self.load_preview_item(&item);
        self.request_timeline_media();
    }

    /// Moves the preview `steps` along its batch (back when negative), round from either end to
    /// the other.
    pub fn step_preview(&mut self, steps: i32) {
        let Some(preview) = &self.preview else { return };
        let len = preview.batch.len() as i64;
        if len < 2 {
            return;
        }
        let index = (preview.index as i64 + i64::from(steps)).rem_euclid(len) as usize;
        self.show_preview_item(index);
    }

    /// Shows the batch's `index`th attachment in the preview, letting go of what showed: a PDF's
    /// render stops, a video closes, and a clip played here pauses.
    pub fn show_preview_item(&mut self, index: usize) {
        let Some(preview) = &self.preview else { return };
        let Some(item) = preview.batch.get(index).filter(|_| index != preview.index).cloned() else { return };
        let Some(media) = item.attachment.media.clone() else { return };
        self.leave_preview_item();
        self.popover = None;
        let content = self.preview_content(&item.attachment);
        self.preview_visits += 1;
        self.reveal_thumbnail = true;
        let Some(preview) = self.preview.as_mut() else { return };
        preview.visit = self.preview_visits;
        preview.index = index;
        preview.attachment = item.attachment.clone();
        preview.media = media;
        preview.content = content;
        self.load_preview_item(&item.attachment);
    }

    /// What the preview shows of `attachment` before its file is here: the timeline's thumbnail
    /// for a picture or a video.
    fn preview_content(&self, attachment: &matrix_core::Attachment) -> PreviewContent {
        let thumbnail = || attachment.preview().and_then(|(media, _)| self.media.image(&media).cloned());
        match Opens::of(attachment) {
            Opens::Image => PreviewContent::Image { thumbnail: thumbnail(), status: ImageStatus::Downloading },
            Opens::Document => PreviewContent::Document { pages: Vec::new(), status: DocumentStatus::Downloading },
            Opens::Video => PreviewContent::Video { thumbnail: thumbnail(), status: VideoStatus::Downloading },
            Opens::Audio | Opens::Download => PreviewContent::Audio,
        }
    }

    /// Fetches what the preview's current attachment needs: a picture's or PDF's file, or a
    /// video's saved to play (once a run, like a clip's). A clip waits for play.
    fn load_preview_item(&mut self, attachment: &matrix_core::Attachment) {
        let Some(preview) = &self.preview else { return };
        let (account, media, visit) = (preview.account.clone(), preview.media.clone(), preview.visit);
        match preview.content {
            PreviewContent::Image { .. } | PreviewContent::Document { .. } => {
                let command = Command::FetchMedia { account, media, size: MediaSize::Original };
                self.dispatch(command, Purpose::Preview(visit));
            }
            PreviewContent::Video { .. } => match self.scratch_files.get(&media).cloned() {
                Some(ScratchFile::Saved(path)) => self.open_video(&media, path),
                Some(ScratchFile::Downloading) => {}
                None => {
                    let path = self.scratch_path(attachment, &media, "video");
                    self.scratch_files.insert(media.clone(), ScratchFile::Downloading);
                    let command = Command::SaveMedia { account, media: media.clone(), path: path.clone() };
                    self.dispatch(command, Purpose::Video { media, path });
                }
            },
            PreviewContent::Audio => {}
        }
    }

    /// Lets go of the preview's current attachment: stops its PDF render, drops its video and
    /// pauses its clip if it plays.
    fn leave_preview_item(&mut self) {
        self.shell.stop_document();
        let Some(preview) = &mut self.preview else { return };
        if matches!(preview.content, PreviewContent::Audio)
            && let Some(clip) = self.audio.as_mut().filter(|clip| clip.media == preview.media)
        {
            clip.play = false;
            if let Some(player) = &clip.player {
                player.pause();
            }
        }
        preview.content = PreviewContent::Audio;
    }

    /// A previewed video's file is saved: it plays, if it is still the one shown.
    fn open_video(&mut self, media: &MediaRef, path: PathBuf) {
        let Some(preview) = self.preview.as_mut().filter(|preview| preview.media == *media) else { return };
        let PreviewContent::Video { status, .. } = &mut preview.content else { return };
        if !matches!(status, VideoStatus::Downloading) {
            return;
        }
        let speaker = self.settings.devices.speaker.as_deref();
        *status = VideoStatus::Open(self.shell.video_player(path, speaker, preview.attachment.dimensions));
    }

    /// The previewed video's player, once its file is here.
    fn preview_video(&self) -> Option<&crate::platform::video::Player> {
        match &self.preview.as_ref()?.content {
            PreviewContent::Video { status: VideoStatus::Open(player), .. } => Some(player),
            _ => None,
        }
    }

    /// Plays or pauses the previewed video, or its clip; play after the end starts over.
    pub fn toggle_preview_playback(&mut self) {
        let Some(preview) = &self.preview else { return };
        if matches!(preview.content, PreviewContent::Audio) {
            let attachment = preview.attachment.clone();
            return self.toggle_audio(&attachment);
        }
        if let Some(player) = self.preview_video() {
            if player.is_playing() { player.pause() } else { player.play() }
        }
    }

    /// Moves the previewed video to `share` of the way through.
    pub fn seek_video(&mut self, share: f64) {
        if let Some(player) = self.preview_video() {
            player.seek(share);
        }
    }

    /// Moves the previewed video `steps` twentieths along, as the arrow keys do on its bar.
    pub fn step_video(&mut self, steps: i32) {
        if let Some(player) = self.preview_video() {
            player.seek(player.progress() + f64::from(steps) * AUDIO_STEP);
        }
    }

    /// Mutes the previewed video, or lets it be heard again.
    pub fn toggle_video_sound(&mut self) {
        if let Some(player) = self.preview_video() {
            player.set_muted(!player.muted());
        }
    }

    /// The previewed file is here, for the showing `visit`: a picture decodes, a PDF renders.
    /// What an earlier showing of the same file asked for is dropped, as is a second answer.
    fn preview_loaded(&mut self, visit: u64, bytes: std::sync::Arc<[u8]>) {
        let Some(preview) = self.preview.as_mut().filter(|preview| preview.visit == visit) else { return };
        match &mut preview.content {
            PreviewContent::Image { status: status @ ImageStatus::Downloading, .. } => {
                if let Some(image) = self.media.decode(bytes) {
                    *status = ImageStatus::Decoding(image);
                }
            }
            PreviewContent::Document { status: status @ DocumentStatus::Downloading, .. } => {
                *status = DocumentStatus::Rendering(self.shell.render_document(bytes));
            }
            _ => {}
        }
    }

    /// The previewed PDF's render `render`, if it still is, while it renders.
    fn rendering(&mut self, render: u64) -> Option<(&mut preview::Pages, &mut DocumentStatus)> {
        match &mut self.preview.as_mut()?.content {
            PreviewContent::Document { pages, status } if *status == DocumentStatus::Rendering(render) => {
                Some((pages, status))
            }
            _ => None,
        }
    }

    /// One more page of the previewed PDF, from the renderer.
    pub fn document_page(&mut self, render: u64, page: rune::Pixels, size: (u32, u32)) {
        if let Some((pages, _)) = self.rendering(render) {
            pages.push((page, size));
        }
    }

    /// The renderer finished the previewed PDF, or failed.
    pub fn document_rendered(&mut self, render: u64, result: Result<(), String>) {
        if let Some((_, status)) = self.rendering(render) {
            *status = match result {
                Ok(()) => DocumentStatus::Done,
                Err(error) => DocumentStatus::Failed(error),
            };
        }
    }

    pub fn close_preview(&mut self) {
        if matches!(self.popover, Some(Popover::AttachmentMenu { .. })) {
            self.popover = None;
        }
        self.leave_preview_item();
        self.preview = None;
    }

    /// Saves the previewed file where the user picks.
    pub fn download_preview(&mut self) {
        let Some(preview) = &self.preview else { return };
        let (account, file) = (preview.account.clone(), preview.attachment.clone());
        self.act_on_attachment(account, &file, AttachmentAction::Download);
    }

    /// Asks where to save a file, then saves it there.
    fn download(&mut self, account: AccountId, media: MediaRef, name: &str) {
        self.shell.pick_save_path(name, move |s: &mut State, path| {
            s.dispatch(Command::SaveMedia { account, media, path }, Purpose::Save);
        });
    }

    // ----- navigation ----------------------------------------------------------------------

    fn remember_account(&mut self) {
        // A remembered account may still be restoring. Other accounts arriving first must
        // neither replace it on screen nor overwrite the saved choice.
        if self.nav.account.as_ref().is_some_and(|id| self.accounts.get(id).is_none()) {
            return;
        }
        if self.settings.last_account != self.nav.account {
            self.settings.last_account.clone_from(&self.nav.account);
            self.shell.save_settings(self.settings.clone());
        }
    }

    pub fn select_account(&mut self, id: &AccountId) {
        if self.accounts.get(id).is_some() {
            if self.nav.account.as_ref() != Some(id) {
                self.reset_search();
            }
            self.nav.account = Some(id.clone());
            self.remember_account();
            if matches!(self.modal, Some(Modal::SignIn)) {
                self.close_modal();
            }
            self.sync_timeline();
            self.request_timeline_media();
            self.request_avatars();
            self.breath.ask();
        }
        self.popover = None;
        self.discover_scope();
    }

    /// Shows a scope's overview: no room selected until the user picks one.
    pub fn select_scope(&mut self, scope: Scope) {
        let Some(account) = self.nav.account.clone() else { return };
        self.nav.scopes.insert(account.clone(), scope);
        self.nav.rooms.remove(&account);
        self.sync_timeline();
        self.discover_scope();
        self.breath.ask();
    }

    /// Back to the current scope's overview.
    pub fn show_overview(&mut self) {
        if let Some(account) = &self.nav.account {
            self.nav.rooms.remove(account);
        }
        self.sync_timeline();
        self.discover_scope();
    }

    fn invalidate_members(&mut self, account: &AccountId, room_id: &str) {
        let cached = self.accounts.invalidate_members(account, room_id);
        let mut refresh = cached.is_some();
        let mut all = cached.unwrap_or(false);
        let mut retired = Vec::new();
        self.pending.retain(|request, purpose| {
            if let Purpose::Members { account: owner, room_id: room, all: requested_all } = purpose
                && owner == account
                && room == room_id
            {
                refresh = true;
                all |= *requested_all;
                retired.push(*request);
                return false;
            }
            true
        });
        for request in retired {
            self.engine.send(Command::Cancel(request));
        }
        let visible = self.nav.account.as_ref() == Some(account) && self.active_room_id() == Some(room_id)
            || matches!(&self.modal, Some(Modal::Invite { account: owner, room_id: room, .. }) if owner == account && room == room_id);
        if refresh && visible {
            self.load_members(account.clone(), room_id.to_owned(), all);
        }
    }

    fn load_members(&mut self, account: AccountId, room_id: String, all: bool) {
        if self.pending.values().any(|purpose| {
            matches!(purpose,
            Purpose::Members { account: owner, room_id: room, all: requested_all }
            if owner == &account && room == &room_id && (*requested_all || !all))
        }) {
            return;
        }
        let purpose = Purpose::Members { account: account.clone(), room_id: room_id.clone(), all };
        self.dispatch(Command::ListMembers { account, room_id, all }, purpose);
    }

    pub fn select_room(&mut self, room_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.accounts.mark_read(&account, room_id);
        if self.nav.rooms.get(&account).map(String::as_str) != Some(room_id) {
            self.nav.threads.remove(&account);
        }
        self.nav.rooms.insert(account.clone(), room_id.to_owned());
        // The new room is ready to type into.
        self.composer.edit.focused = true;
        self.sync_timeline();
        self.request_timeline_media();
        // The header's faces need only the members this device knows; the open list fetches
        // everyone.
        let all = self.side == Some(SidePanel::Members);
        self.member_search = TextEdit::default();
        self.load_members(account, room_id.to_owned(), all);
    }

    /// Shows a room of any account in a scope whose sidebar lists it.
    pub fn open_room(&mut self, account: &AccountId, room_id: &str) {
        let Some(data) = self.accounts.get(account) else { return };
        let current = self.nav.scopes.get(account).cloned().unwrap_or(Scope::Home);
        let lists = |scope: &Scope| {
            [matrix_core::RoomKind::Group, matrix_core::RoomKind::Direct]
                .into_iter()
                .any(|kind| data.rooms_in(scope, kind).any(|room| room.room_id == room_id))
        };
        let scope = if lists(&current) {
            current
        } else if lists(&Scope::Home) {
            Scope::Home
        } else {
            data.spaces.iter().map(|space| Scope::Space(space.room_id.clone())).find(lists).unwrap_or(Scope::Home)
        };
        self.select_account(account);
        self.nav.scopes.insert(account.clone(), scope);
        self.select_room(room_id);
    }

    pub fn toggle_popover(&mut self, popover: Popover) {
        self.popover = if self.popover.as_ref() == Some(&popover) { None } else { Some(popover) };
        // The account menu opens showing the status message set, not one left half written.
        if self.popover == Some(Popover::AccountSwitcher) {
            self.status_draft = None;
        }
        // The emoji picker opens empty, ready to type a search into.
        if matches!(self.popover, Some(Popover::Emoji(_))) {
            self.emoji_search = TextEdit::default();
            self.emoji_search.focused = true;
        }
    }

    /// Shows or hides the member list beside the open room; it takes the thread's place.
    pub fn toggle_members(&mut self) {
        self.toggle_side(SidePanel::Members);
    }

    /// Shows or hides a panel beside the open room.
    pub fn toggle_side(&mut self, panel: SidePanel) {
        if self.side.as_ref() == Some(&panel) {
            self.side = None;
        } else {
            self.show_side(panel);
        }
    }

    /// Shows a panel beside the open room, in the thread's place, fetching what it lists.
    pub fn show_side(&mut self, panel: SidePanel) {
        self.popover = None;
        self.close_thread();
        self.side = Some(panel);
        match &self.side {
            Some(SidePanel::Members) => {
                self.member_search = TextEdit::default();
                self.member_search.focused = true;
                self.list_all_members();
            }
            // Opened again, its list is fresh.
            Some(SidePanel::Threads | SidePanel::Files | SidePanel::Pins) => {
                self.side_loaded = None;
                self.sync_side();
            }
            // Their role and whether they are verified come from the member list, as complete
            // as the readers' list asks for; their picture may be one no row has fetched.
            Some(SidePanel::Person(_)) => {
                self.list_all_members();
                self.request_avatars();
            }
            Some(SidePanel::Info) | None => {}
        }
    }

    /// Opens a thread from the thread list.
    pub fn open_listed_thread(&mut self, root: &str) {
        self.side = None;
        self.open_thread(root);
    }

    /// Opens (or closes) what can be done with a member of the open room.
    pub fn toggle_member_menu(&mut self, user_id: &str) {
        let open = matches!(&self.popover, Some(Popover::MemberMenu { user_id: shown, .. }) if shown == user_id);
        self.popover = (!open).then(|| Popover::MemberMenu { user_id: user_id.to_owned(), submenu: None });
    }

    /// Opens one of the member menu's submenus, or closes it when it's open.
    pub fn toggle_member_submenu(&mut self, which: MemberSubmenu) {
        if let Some(Popover::MemberMenu { submenu, .. }) = &mut self.popover {
            *submenu = if *submenu == Some(which) { None } else { Some(which) };
        } else if let Some(Popover::ReadReceipts { member: Some(menu), .. }) = &mut self.popover {
            menu.submenu = if menu.submenu == Some(which) { None } else { Some(which) };
        }
    }

    /// Kicks or bans a member of the open room after the user confirms; gives a role at once.
    pub fn moderate(&mut self, user_id: &str, action: matrix_core::Moderation) {
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        self.popover = None;
        let members = self.accounts.get(&account).and_then(|data| data.members.get(&room_id));
        let name = members
            .and_then(|members| members.people.iter().find(|m| m.person.id == user_id))
            .map_or_else(|| user_id.to_owned(), |m| m.person.name.clone());
        let own_level = members.map_or(0, |members| members.own_level);
        let user_id = user_id.to_owned();
        let asks = match action {
            matrix_core::Moderation::Kick | matrix_core::Moderation::Ban => true,
            matrix_core::Moderation::Role(role) => role.level() >= own_level,
        };
        if asks {
            self.modal = Some(Modal::Confirm(Confirm::Moderate { account, room_id, user_id, name, action }));
        } else {
            self.dispatch(Command::ModerateMember { account, room_id, user_id, action }, Purpose::Moderate);
        }
    }

    /// Calls someone in the direct chat with them, starting that chat if there is none.
    pub fn call_person(&mut self, user_id: &str, video: bool) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        let existing = self
            .accounts
            .get(&account)
            .and_then(|data| data.direct_chat_with(user_id))
            .map(|room| room.room_id.clone());
        match existing {
            Some(room_id) => {
                self.open_room(&account, &room_id);
                self.start_call(video);
            }
            None => {
                let purpose = Purpose::DirectCall { account: account.clone(), video };
                self.dispatch(Command::OpenDirectChat { account, user_id: user_id.to_owned() }, purpose);
            }
        }
    }

    /// Invites someone to another of the account's rooms.
    pub fn invite_member_to(&mut self, room_id: &str, user_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        let command = Command::Invite { account, room_id: room_id.to_owned(), user_id: user_id.to_owned() };
        self.dispatch(command, Purpose::Invite);
    }

    /// Fetches everyone in the open room, unless the list already has them all.
    fn list_all_members(&mut self) {
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        let complete = self
            .accounts
            .get(&account)
            .and_then(|data| data.members.get(&room_id))
            .is_some_and(|members| members.complete);
        if !complete || !self.accounts.members_fresh(&account, &room_id) {
            self.load_members(account, room_id, true);
        }
    }

    pub fn close_popover(&mut self) {
        self.popover = None;
    }

    /// A popover's field must dismiss before the editor consumes Escape to blur itself.
    pub fn popover_input_key(&mut self, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
        if key.key == rune::Key::Escape && self.popover.is_some() {
            self.close_popover();
            tx.blur();
            true
        } else {
            false
        }
    }

    /// Opens a room's context menu, or closes it when it is already open.
    pub fn toggle_room_menu(&mut self, room_id: &str) {
        let open = matches!(&self.popover, Some(Popover::RoomMenu { room_id: id, .. }) if id == room_id);
        self.popover = (!open).then(|| Popover::RoomMenu { room_id: room_id.to_owned(), notifications: false });
    }

    /// Opens the notifications submenu of the open room menu.
    pub fn show_room_notifications(&mut self) {
        if let Some(Popover::RoomMenu { notifications, .. }) = &mut self.popover {
            *notifications = true;
        }
    }

    /// Asks to leave a room, or for a direct chat to delete it; [`State::confirm`] carries it out.
    pub fn ask_to_leave(&mut self, room_id: &str) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        let Some(room) = self.accounts.get(&account).and_then(|a| a.room(room_id)) else { return };
        let (room_id, name) = (room.room_id.clone(), room.name.clone());
        self.modal = Some(Modal::Confirm(match room.kind {
            matrix_core::RoomKind::Group => Confirm::LeaveRoom { account, room_id, name },
            matrix_core::RoomKind::Direct => Confirm::DeleteChat { account, room_id, name },
        }));
    }

    /// Carries out the action the open confirmation asks about.
    pub fn confirm(&mut self) {
        let Some(Modal::Confirm(confirm)) = self.modal.take() else { return };
        let (command, purpose) = match confirm {
            Confirm::LeaveRoom { account, room_id, .. } => {
                (Command::LeaveRoom { account, room_id, forget: false }, Purpose::Leave)
            }
            Confirm::DeleteChat { account, room_id, .. } => {
                (Command::LeaveRoom { account, room_id, forget: true }, Purpose::Leave)
            }
            Confirm::Moderate { account, room_id, user_id, action, .. } => {
                (Command::ModerateMember { account, room_id, user_id, action }, Purpose::Moderate)
            }
            Confirm::EndPoll { account, key, poll_id, .. } => {
                (Command::EndPoll { account, key, poll_id }, Purpose::Poll)
            }
            Confirm::DeleteMessage { account, key, event_id } => {
                (Command::Redact { account, key, event_id }, Purpose::Delete)
            }
        };
        self.dispatch(command, purpose);
    }

    /// Sets a room's own notification mode, or with `None` makes it follow the account default.
    pub fn set_notification_mode(&mut self, room_id: &str, mode: Option<NotificationMode>) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        self.accounts.set_notifications(&account, room_id, mode);
        let command = Command::SetNotificationMode { account, room_id: room_id.to_owned(), mode };
        self.dispatch(command, Purpose::NotificationMode);
    }

    /// Sets when an account's rooms notify unless they set their own mode.
    pub fn set_default_notifications(&mut self, account: &AccountId, mode: NotificationMode) {
        self.accounts.set_default_notifications(account, mode);
        let command = Command::SetDefaultNotificationMode { account: account.clone(), mode };
        self.dispatch(command, Purpose::NotificationMode);
    }

    /// Sets what others see of the user on an account, showing it at once, and closes the
    /// account menu.
    pub fn set_status(&mut self, account: &AccountId, status: matrix_core::Status) {
        self.popover = None;
        if self.accounts.get(account).is_none_or(|data| data.status == status) {
            return;
        }
        self.accounts.set_status(account, status);
        let command = Command::SetStatus { account: account.clone(), status };
        self.dispatch(command, Purpose::Status { account: account.clone(), chosen: status });
    }

    // ----- encryption and sessions ---------------------------------------------------------

    /// The account the Account and Sessions settings show: the active one.
    pub fn settings_account(&self) -> Option<&AccountData> {
        self.active_account()
    }

    /// Asks the user's other sessions to verify this one.
    pub fn verify_with_other_device(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        self.modal =
            Some(Modal::Verification { account: account.clone(), flow_id: None, step: VerificationStep::Waiting });
        self.dispatch(Command::VerifyThisSession(account), Purpose::Security);
    }

    /// Follows a verification: opens the dialog for a new incoming request, updates an open one.
    fn verification_moved(&mut self, account: AccountId, flow_id: String, step: VerificationStep) {
        match &mut self.modal {
            Some(Modal::Verification { account: shown, flow_id: open, step: current })
                if *shown == account && open.as_ref().is_none_or(|open| *open == flow_id) =>
            {
                *open = Some(flow_id);
                *current = step;
            }
            _ if matches!(step, VerificationStep::Incoming { .. }) => {
                self.modal = Some(Modal::Verification { account, flow_id: Some(flow_id), step });
            }
            _ => {}
        }
    }

    fn open_verification(&self) -> Option<(AccountId, String)> {
        match &self.modal {
            Some(Modal::Verification { account, flow_id: Some(flow_id), .. }) => {
                Some((account.clone(), flow_id.clone()))
            }
            _ => None,
        }
    }

    pub fn accept_verification(&mut self) {
        let Some((account, flow_id)) = self.open_verification() else { return };
        self.dispatch(Command::AcceptVerification { account, flow_id }, Purpose::Security);
    }

    pub fn confirm_verification(&mut self, matches: bool) {
        let Some((account, flow_id)) = self.open_verification() else { return };
        self.dispatch(Command::ConfirmVerification { account, flow_id, matches }, Purpose::Security);
    }

    /// Stops the open verification and closes its dialog.
    pub fn cancel_verification(&mut self) {
        if let Some((account, flow_id)) = self.open_verification()
            && let Some(Modal::Verification { step, .. }) = &self.modal
            && !matches!(step, VerificationStep::Done | VerificationStep::Cancelled(_))
        {
            self.dispatch(Command::CancelVerification { account, flow_id }, Purpose::Cancel);
        }
        self.modal = None;
    }

    /// The text field of the open dialog, for its input's `on_edit`.
    pub fn modal_field(&mut self) -> &mut TextEdit {
        match &mut self.modal {
            Some(
                Modal::Forward { search: edit, .. }
                | Modal::RecoveryKeyEntry { edit, .. }
                | Modal::Password { edit, .. }
                | Modal::RenameSession { edit, .. }
                | Modal::SectionName { edit, .. },
            ) => edit,
            _ => &mut self.scratch,
        }
    }

    pub fn open_recovery_key_entry(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::RecoveryKeyEntry {
            account,
            edit: focused(TextEdit::new("").secure()),
            error: None,
            submitting: false,
        });
    }

    pub fn submit_recovery_key(&mut self) {
        let Some(Modal::RecoveryKeyEntry { account, edit, error, submitting }) = &mut self.modal else { return };
        if *submitting || edit.text.trim().is_empty() {
            return;
        }
        let command = Command::Recover { account: account.clone(), key: Password(edit.text.clone()) };
        *error = None;
        *submitting = true;
        self.dispatch(command, Purpose::Recover);
    }

    /// Sets up recovery, or with `reset` replaces the key; the new key shows once.
    pub fn set_up_recovery(&mut self, reset: bool) {
        let Some(account) = self.nav.account.clone() else { return };
        let command = if reset { Command::ResetRecoveryKey(account) } else { Command::SetUpRecovery(account) };
        self.dispatch(command, Purpose::RecoveryKey);
    }

    pub fn copy_recovery_key(&mut self, tx: &mut Tx) {
        if let Some(Modal::NewRecoveryKey { key }) = &self.modal {
            tx.copy(key.clone());
        }
    }

    pub fn load_sessions(&mut self) {
        if let Some(account) = self.nav.account.clone() {
            self.dispatch(Command::ListSessions(account), Purpose::Security);
        }
    }

    /// Starts an action the server may guard with the password; if it does, the password dialog
    /// opens and [`State::confirm_with_password`] sends the action again.
    fn guarded(&mut self, account: AccountId, action: Guarded) -> RequestId {
        let command = action.command(account.clone(), None);
        self.dispatch(command, Purpose::Guarded { account, action })
    }

    /// Signs out another session; the server may ask for the password first.
    pub fn end_session(&mut self, device_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.guarded(account, Guarded::EndSession { device_id: device_id.to_owned() });
    }

    /// Sends the guarded action of the password dialog again, with the password.
    pub fn confirm_with_password(&mut self) {
        let Some(Modal::Password { account, action, edit, error, submitting }) = &mut self.modal else { return };
        if *submitting || edit.text.is_empty() {
            return;
        }
        let (account, action) = (account.clone(), action.clone());
        let command = action.command(account.clone(), Some(Password(edit.text.clone())));
        *error = None;
        *submitting = true;
        self.dispatch(command, Purpose::Guarded { account, action });
    }

    /// Opens the identity reset at its warning, from the Account page or the recovery key dialog.
    pub fn start_identity_reset(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        self.modal = Some(Modal::ResetIdentity { account, step: ResetStep::Warning { error: None } });
    }

    /// Resets the identity after the warning.
    pub fn confirm_identity_reset(&mut self) {
        let Some(Modal::ResetIdentity { account, step: ResetStep::Warning { .. } }) = &self.modal else { return };
        let account = account.clone();
        let request = self.guarded(account, Guarded::ResetIdentity);
        // The password dialog may already have replaced the modal.
        if let Some(Modal::ResetIdentity { step, .. }) = &mut self.modal {
            *step = ResetStep::Resetting { request, url: None };
        }
    }

    fn resetting_identity(&self) -> bool {
        match &self.modal {
            Some(Modal::ResetIdentity { step, .. }) => matches!(step, ResetStep::Resetting { .. }),
            Some(Modal::Password { action, .. }) => *action == Guarded::ResetIdentity,
            _ => false,
        }
    }

    /// Opens the approval page of a running identity reset again.
    pub fn reopen_reset_approval(&mut self, tx: &mut Tx) {
        if let Some(Modal::ResetIdentity { step: ResetStep::Resetting { url: Some(url), .. }, .. }) = &self.modal
            && let Err(error) = tx.open_url(url)
        {
            self.notice = Some(tf!("notice.browser", error = error));
        }
    }

    /// From the finished reset straight into setting up recovery.
    pub fn set_up_recovery_after_reset(&mut self) {
        self.modal = None;
        self.set_up_recovery(false);
    }

    pub fn start_renaming_session(&mut self, device_id: &str, name: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        let edit = focused(TextEdit::new(name));
        self.modal = Some(Modal::RenameSession { account, device_id: device_id.to_owned(), edit });
    }

    pub fn submit_session_name(&mut self) {
        let Some(Modal::RenameSession { account, device_id, edit }) = self.modal.take() else { return };
        let name = edit.text.trim().to_owned();
        if !name.is_empty() {
            self.dispatch(Command::RenameSession { account, device_id, name }, Purpose::Security);
        }
    }

    // ----- settings ------------------------------------------------------------------------

    pub fn open_settings(&mut self) {
        self.popover = None;
        self.modal = Some(Modal::Settings(SettingsSection::default()));
        self.third_party_shown = false;
    }

    /// Shows or hides the full third-party list in Settings → Info.
    pub fn toggle_third_party(&mut self) {
        self.third_party_shown = !self.third_party_shown;
    }

    pub fn show_settings_section(&mut self, section: SettingsSection) {
        self.modal = Some(Modal::Settings(section));
        match section {
            SettingsSection::Sessions => self.load_sessions(),
            SettingsSection::VoiceVideo => self.list_devices(),
            SettingsSection::Profile => self.load_profile(),
            _ => {}
        }
    }

    // ----- profile ---------------------------------------------------------------------------

    /// The avatar menu's Profile entry: the Profile page, for every account. Name and picture
    /// change here everywhere; contact details too where the server allows it.
    pub fn open_profile(&mut self) {
        if self.active_account().is_none() {
            return;
        }
        self.popover = None;
        self.modal = Some(Modal::Settings(SettingsSection::Profile));
        self.load_profile();
    }

    /// The account page, from the Profile page of an account whose server leaves contact details
    /// to it.
    pub fn open_account_page(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        self.dispatch(Command::OpenAccountPage { account, page: AccountPage::Profile }, Purpose::AccountPage);
    }

    /// Fills the Profile page's form for the active account, and lists its contact details
    /// where the app manages them.
    fn load_profile(&mut self) {
        let Some(summary) = self.active_account().map(|account| account.summary.clone()) else { return };
        if self.profile.account.as_ref() != Some(&summary.id) {
            // Without a display name the summary shows the user ID; the field stays empty then.
            let name = if summary.display_name == summary.id.as_str() { "" } else { &summary.display_name };
            self.profile = ProfileForm::open(summary.id.clone(), name);
        }
        self.dispatch(Command::ListContactDetails(summary.id), Purpose::Profile);
    }

    /// Whether the name field differs from the saved display name.
    pub fn display_name_changed(&self) -> bool {
        let Some(account) = self.active_account() else { return false };
        let name = self.profile.name.text.trim();
        !name.is_empty() && name != account.summary.display_name
    }

    pub fn save_display_name(&mut self) {
        if self.profile.saving_name || !self.display_name_changed() {
            return;
        }
        let Some(account) = self.nav.account.clone() else { return };
        let name = self.profile.name.text.trim().to_owned();
        self.profile.saving_name = true;
        self.dispatch(Command::SetDisplayName { account, name }, Purpose::ProfileName);
    }

    /// Asks for a new profile picture; the dialog's answer uploads it.
    pub fn pick_avatar(&mut self) {
        self.shell.pick_files(PickFor::Avatar);
    }

    /// Uploads `file` as the profile picture, or with `None` removes it.
    pub fn set_avatar(&mut self, file: Option<PathBuf>) {
        let Some(account) = self.nav.account.clone() else { return };
        self.profile.saving_avatar = true;
        self.dispatch(Command::SetAvatar { account, file }, Purpose::Avatar);
    }

    /// Asks the server to send a link (email) or code (phone) for the address typed in.
    pub fn request_contact_code(&mut self, kind: ContactKind) {
        let Some(account) = self.nav.account.clone() else { return };
        let form = self.profile.contact(kind);
        let address = form.address.text.trim().to_owned();
        if address.is_empty() || form.step != profile::ContactStep::Entering {
            return;
        }
        let country = crate::i18n::system_region().unwrap_or_default();
        let request =
            self.dispatch(Command::RequestContactCode { account, kind, address, country }, Purpose::ContactCode(kind));
        let form = self.profile.contact_mut(kind);
        form.step = ContactStep::Requesting(request);
        form.error = None;
    }

    /// Confirms the link was opened (email) or sends the code (phone), then adds the detail.
    pub fn confirm_contact(&mut self, kind: ContactKind) {
        let Some(account) = self.nav.account.clone() else { return };
        let form = self.profile.contact_mut(kind);
        let ContactStep::Sent { pending, busy: false } = &form.step else { return };
        let pending = pending.clone();
        match kind {
            ContactKind::Email => self.add_contact(kind),
            ContactKind::Phone => {
                let code = form.code.text.trim().to_owned();
                if code.is_empty() {
                    return;
                }
                form.step = ContactStep::Sent { pending: pending.clone(), busy: true };
                form.error = None;
                self.dispatch(Command::ConfirmContactCode { account, pending, code }, Purpose::ContactConfirm(kind));
            }
        }
    }

    /// Links the confirmed detail; the server may ask for the password first.
    fn add_contact(&mut self, kind: ContactKind) {
        let Some(account) = self.nav.account.clone() else { return };
        let form = self.profile.contact_mut(kind);
        let Some(pending) = form.pending().cloned() else { return };
        form.step = ContactStep::Sent { pending: pending.clone(), busy: true };
        form.error = None;
        self.guarded(account, Guarded::AddContact { pending });
    }

    /// Gives up adding a contact detail.
    pub fn cancel_contact(&mut self, kind: ContactKind) {
        self.profile.contact_mut(kind).reset();
    }

    pub fn remove_contact(&mut self, contact: ContactDetail) {
        let Some(account) = self.nav.account.clone() else { return };
        self.dispatch(Command::RemoveContact { account, contact }, Purpose::Profile);
    }

    /// Opens settings on the Account page, from the unverified-session banner.
    pub fn open_account_settings(&mut self) {
        self.popover = None;
        self.show_settings_section(SettingsSection::Account);
    }

    /// Closes the open dialog, stopping an identity reset that is waiting for its approval.
    pub fn close_modal(&mut self) {
        match self.modal.take() {
            Some(Modal::SignIn) => self.change_server(),
            Some(Modal::ResetIdentity { step: ResetStep::Resetting { request, .. }, .. }) => {
                self.dispatch(Command::Cancel(request), Purpose::Cancel);
            }
            // Giving up on the password leaves the contact detail waiting to be confirmed.
            Some(Modal::Password { action: Guarded::AddContact { pending }, .. }) => {
                if let ContactStep::Sent { busy, .. } = &mut self.profile.contact_mut(pending.kind).step {
                    *busy = false;
                }
            }
            _ => {}
        }
    }

    pub fn set_language(&mut self, language: Language) {
        self.settings.language = language;
        language.apply();
        self.shell.save_settings(self.settings.clone());
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.settings.appearance = appearance;
        self.shell.save_settings(self.settings.clone());
    }

    // ----- accounts and signing in -----------------------------------------------------------

    /// Signs an account back in after the server ended its session: the user ID finds the
    /// server (and fills in the username where a password is asked for). One signed out softly
    /// takes its data back.
    pub fn sign_in_again(&mut self) {
        let Some(Modal::SessionEnded { account, .. }) = self.modal.take() else { return };
        self.sign_in_as(&account);
    }

    /// Starts signing in as `account`: its user ID finds the server. A suspended account takes
    /// its data back.
    pub fn sign_in_as(&mut self, account: &AccountId) {
        // A sign-in under way (another server's browser step) gives way to this one.
        self.change_server();
        self.start_adding_account();
        self.sign_in.homeserver = TextEdit::new(account.as_str());
        self.submit_server();
    }

    /// Forgets an account signed out softly, deleting what it kept on this device.
    pub fn remove_signed_out(&mut self) {
        let Some(Modal::SessionEnded { account, kept: true }) = self.modal.take() else { return };
        self.forget_suspended(&account);
    }

    /// Forgets a suspended account and what it kept on this device.
    pub fn forget_suspended(&mut self, account: &AccountId) {
        self.suspended.retain(|known| known != account);
        self.dispatch(Command::Logout(account.clone()), Purpose::Logout);
    }

    pub fn start_adding_account(&mut self) {
        self.sign_in.resume = None;
        if !self.accounts.is_empty() {
            self.modal = Some(Modal::SignIn);
        }
        self.popover = None;
    }

    /// Step 1: resolve what the user typed. A full Matrix ID also fills in the username.
    pub fn submit_server(&mut self) {
        if self.sign_in.pending().is_some() {
            return;
        }
        let Some(input) = SignInInput::parse(&self.sign_in.homeserver.text) else {
            self.sign_in.step = SignInStep::Server { error: Some(t!("sign_in.enter_server").into()) };
            return;
        };
        if let Some(username) = input.username {
            self.sign_in.username = rune::TextEdit::new(username);
        }
        // Signing in as a suspended account takes back its store and device, however the
        // sign-in started.
        let typed = self.sign_in.homeserver.text.trim();
        self.sign_in.resume =
            self.suspended.iter().find(|account| account.as_str().eq_ignore_ascii_case(typed)).cloned();
        let request = self.dispatch(Command::Discover { server: input.server }, Purpose::Discover);
        self.sign_in.step = SignInStep::Discovering(request);
    }

    /// Step 2, chosen by what the server offers: the browser for OAuth, else the password form.
    fn continue_sign_in(&mut self, server: Homeserver) {
        if server.methods.oauth {
            self.start_browser(server);
        } else if server.methods.password {
            self.sign_in.step = SignInStep::Password { server, submitting: None, error: None };
        } else {
            let error = tf!("sign_in.no_method", server = server.display_name());
            self.sign_in.step = SignInStep::Server { error: Some(error) };
        }
    }

    fn start_browser(&mut self, server: Homeserver) {
        let command = Command::LoginOAuth { homeserver: server.url.clone(), resume: self.sign_in.resume.clone() };
        let request = self.dispatch(command, Purpose::SignIn);
        self.sign_in.step = SignInStep::Browser { server, request: Some(request), url: None, error: None };
    }

    /// Starts the browser sign-in again, after a failure or from the password form.
    pub fn retry_browser(&mut self) {
        let Some(server) = self.sign_in.server().filter(|s| s.methods.oauth).cloned() else { return };
        self.cancel_pending();
        self.start_browser(server);
    }

    /// Switches step 2 to the password form, on a server that offers both.
    pub fn use_password(&mut self) {
        let Some(server) = self.sign_in.server().filter(|s| s.methods.password).cloned() else { return };
        self.cancel_pending();
        self.sign_in.step = SignInStep::Password { server, submitting: None, error: None };
    }

    /// Back to step 1.
    pub fn change_server(&mut self) {
        self.cancel_pending();
        // Another server is another account: nothing suspended carries over.
        self.sign_in.resume = None;
        self.sign_in.step = SignInStep::Server { error: None };
    }

    pub fn submit_password(&mut self) {
        if self.sign_in.pending().is_some() {
            return;
        }
        let login = self.sign_in.password_request();
        let SignInStep::Password { submitting, error, .. } = &mut self.sign_in.step else { return };
        let Some(login) = login else {
            *error = Some(t!("sign_in.fill_in").into());
            return;
        };
        *error = None;
        let request = self.engine.send(Command::Login(login));
        *submitting = Some(request);
        self.pending.insert(request, Purpose::SignIn);
    }

    /// Opens the sign-in page again, for a browser tab the user closed.
    pub fn reopen_browser(&mut self, tx: &mut Tx) {
        if let SignInStep::Browser { url: Some(url), .. } = &self.sign_in.step
            && let Err(error) = tx.open_url(url)
        {
            self.notice = Some(tf!("notice.browser", error = error));
        }
    }

    /// Cancels whatever the sign-in is waiting on, leaving the step as it is.
    fn cancel_pending(&mut self) {
        if let Some(request) = self.sign_in.pending() {
            self.dispatch(Command::Cancel(request), Purpose::Cancel);
        }
    }

    pub fn sign_out(&mut self, id: &AccountId) {
        self.dispatch(Command::Logout(id.clone()), Purpose::Logout);
        self.popover = None;
    }

    // ----- threads, replies and reactions --------------------------------------------------

    /// Opens a message's thread beside the room; a message without one starts it.
    pub fn open_thread(&mut self, root_event_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        if self
            .active_room_id()
            .and_then(|room| self.accounts.get(&account)?.timeline(&TimelineKey::room(room)))
            .is_some_and(|timeline| {
                timeline.messages().any(|message| {
                    message.event_id == root_event_id && matches!(message.content, MessageContent::Call(_))
                })
            })
        {
            return;
        }
        // A memo recording in a thread being left goes with it; an edit and the draft there
        // are set aside, to come back with the thread (`sync_timeline`).
        let key = self.active_room_id().map(|room_id| TimelineKey::thread(room_id, root_event_id));
        if let Some(key) = key {
            self.accounts.seed_thread(&account, &key);
        }
        self.nav.threads.insert(account, root_event_id.to_owned());
        // The thread takes the side panel's place beside the room.
        self.side = None;
        // Keep the room's reading anchor: a window-wide settle would snap it to the bottom.
        self.composer.edit.focused = false;
        self.thread_composer.edit.focused = true;
        self.sync_timeline();
    }

    pub fn close_thread(&mut self) {
        if let Some(account) = &self.nav.account {
            self.nav.threads.remove(account);
        }
        self.sync_timeline();
    }

    /// Starts a reply to `message` in the given composer.
    pub fn start_reply(&mut self, target: ComposerTarget, message: &Message, tx: &mut Tx) {
        if matches!(message.content, MessageContent::Call(_)) {
            return;
        }
        let excerpt: String = match &message.content {
            MessageContent::Call(card) => call_title(card).to_owned(),
            content => content.plain().chars().take(140).collect(),
        };
        // Replying ends an edit: its draft comes back, and the reply goes with that.
        let composer = self.composer_mut(target);
        composer.cancel_edit();
        composer.reply_to =
            Some(ReplyTarget { event_id: message.event_id.clone(), sender_name: message.sender_name.clone(), excerpt });
        tx.focus(composer_field(target), false);
    }

    pub fn cancel_reply(&mut self, target: ComposerTarget) {
        self.composer_mut(target).reply_to = None;
    }

    /// Starts editing one of the user's text messages in a composer.
    pub fn start_edit(&mut self, target: ComposerTarget, message: &Message, tx: &mut Tx) {
        let (MessageContent::Text { body, html }, true) = (&message.content, message.editable) else { return };
        // A voice memo recording has the composer: it is sent or dropped first.
        if message.pending || self.composer(target).voice.is_some() {
            return;
        }
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return };
        self.popover = None;
        let edited = composer::EditTarget { account, key, event_id: message.event_id.clone() };
        // An edit of another message under way is kept, to come back after this one, when it
        // changed anything.
        if let Some(other) = self.composer_mut(target).take_edit() {
            self.set_edit_aside(other);
        }
        // What was kept of an earlier edit of this message comes back as the user left it, when
        // it was made here; from another timeline, it is dropped for a fresh edit.
        let kept = self.set_aside.iter().position(|kept| kept.target == edited).map(|at| self.set_aside.remove(at));
        self.set_aside.retain(|kept| !kept.target.same_message(&edited));
        match kept {
            Some(kept) => self.composer_mut(target).resume_edit(kept),
            None => self.composer_mut(target).start_edit(edited, body, html.as_deref()),
        }
        tx.focus(composer_field(target), false);
    }

    /// Keeps an edit for later, in place of any earlier one of the same message.
    fn set_edit_aside(&mut self, edit: composer::Edit) {
        self.set_aside.retain(|kept| !kept.target.same_message(&edit.target));
        self.set_aside.push(edit);
    }

    /// Edits set aside come back into the composers of the timelines on screen that are free.
    fn resume_edits(&mut self) {
        if self.screen() != Screen::Chat {
            return;
        }
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { continue };
            let composer = self.composer(target);
            if composer.editing.is_some() || composer.voice.is_some() {
                continue;
            }
            // The newest first: the edit left last comes back first.
            let at = self.set_aside.iter().rposition(|edit| edit.target.account == account && edit.target.key == key);
            if let Some(edit) = at.map(|at| self.set_aside.remove(at)) {
                self.composer_mut(target).resume_edit(edit);
            }
        }
    }

    /// Edits the user's newest text message in a composer's timeline: the Up arrow in an empty
    /// field. False when there is none to edit.
    pub fn edit_last(&mut self, target: ComposerTarget, tx: &mut Tx) -> bool {
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return false };
        let Some(timeline) = self.accounts.get(&account).and_then(|data| data.timeline(&key)) else { return false };
        let last = timeline.entries.iter().rev().find_map(|entry| match &entry.kind {
            EntryView::Message(item) if item.message.editable && !item.message.pending => Some(item.message.clone()),
            _ => None,
        });
        let Some(message) = last else { return false };
        self.start_edit(target, &message, tx);
        true
    }

    pub fn cancel_edit(&mut self, target: ComposerTarget) {
        self.composer_mut(target).cancel_edit();
        self.resume_edits();
    }

    /// Asks before deleting one of the user's messages.
    pub fn delete_message(&mut self, key: TimelineKey, event_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.modal = Some(Modal::Confirm(Confirm::DeleteMessage { account, key, event_id: event_id.to_owned() }));
    }

    /// An emoji picked in the picker: a reaction, or text for a draft.
    pub fn choose_emoji(&mut self, chosen_for: EmojiFor, emoji: &str) {
        match chosen_for {
            EmojiFor::Reaction { key, event_id } => self.react(key, &event_id, emoji),
            EmojiFor::Composer(target) => {
                self.popover = None;
                let edit = &mut self.composer_mut(target).edit;
                edit.insert(emoji);
                // Back to writing, the search having had the keyboard.
                edit.focused = true;
            }
            EmojiFor::StatusMessage => self.status_emoji_chosen(emoji),
        }
    }

    /// Opens the poll form for a composer.
    pub fn open_poll_form(&mut self, target: ComposerTarget) {
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return };
        self.popover = None;
        self.poll_form = PollForm::new(target, account, key);
        self.modal = Some(Modal::Poll);
    }

    /// Sends the poll being written to the timeline it was opened from, wherever the user is now.
    pub fn send_poll(&mut self, tx: &mut Tx) {
        let Some(message) = self.poll_form.outgoing() else { return };
        self.modal = None;
        let (account, key) = (self.poll_form.account.clone(), self.poll_form.key.clone());
        self.dispatch(Command::Send { account, key, reply_to: None, message }, Purpose::Send);
        tx.scroll_to_bottom(timeline_view(self.poll_form.target));
    }

    /// Votes for (or takes back) an answer of a poll.
    pub fn vote(&mut self, key: TimelineKey, poll: &matrix_core::Poll, poll_id: &str, answer: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        if poll.ended {
            return;
        }
        let answers = poll::next_vote(poll, answer);
        self.dispatch(Command::VotePoll { account, key, poll_id: poll_id.to_owned(), answers }, Purpose::Poll);
    }

    /// Asks to end a poll the user started; [`State::confirm`] ends it.
    pub fn end_poll(&mut self, key: TimelineKey, poll_id: &str, question: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        let (poll_id, question) = (poll_id.to_owned(), question.to_owned());
        self.modal = Some(Modal::Confirm(Confirm::EndPoll { account, key, poll_id, question }));
    }

    /// Adds the user's reaction, or takes it back.
    pub fn react(&mut self, key: TimelineKey, event_id: &str, emoji: &str) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        if self.accounts.get(&account).and_then(|data| data.timeline(&key)).is_some_and(|timeline| {
            timeline
                .messages()
                .any(|message| message.event_id == event_id && matches!(message.content, MessageContent::Call(_)))
        }) {
            return;
        }
        let command = Command::React { account, key, event_id: event_id.to_owned(), emoji: emoji.to_owned() };
        self.dispatch(command, Purpose::React);
    }

    // ----- composing -----------------------------------------------------------------------

    pub fn send_message(&mut self, target: ComposerTarget, tx: &mut Tx) {
        if self.composer(target).editing.is_some() {
            // Unchanged, nothing is sent; either way the draft comes back.
            if let Some(edit) = self.composer_mut(target).take_edit() {
                // This edit supersedes one of the same message kept from before, or under way in
                // the other composer (a thread's root shows in both).
                self.set_aside.retain(|kept| !kept.target.same_message(&edit.target));
                let other = match target {
                    ComposerTarget::Room => ComposerTarget::Thread,
                    ComposerTarget::Thread => ComposerTarget::Room,
                };
                if self.composer(other).editing.as_ref().is_some_and(|live| live.target.same_message(&edit.target)) {
                    self.composer_mut(other).cancel_edit();
                }
                let composer::EditTarget { account, key, event_id } = edit.target.clone();
                let (body, html) = (edit.body.clone(), edit.html.clone());
                self.dispatch(Command::Edit { account, key, event_id, body, html }, Purpose::Edit(Box::new(edit)));
            }
            self.resume_edits();
            return;
        }
        let (reply_to, messages) = self.composer_mut(target).take_outgoing();
        self.send_from(target, reply_to, messages, tx);
    }

    /// Sends messages from a composer's room or thread; the first answers `reply_to`, if any.
    fn send_from(
        &mut self,
        target: ComposerTarget,
        mut reply_to: Option<ReplyTarget>,
        messages: Vec<matrix_core::Outgoing>,
        tx: &mut Tx,
    ) {
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return };
        for message in messages {
            let reply_to = reply_to.take().map(|target| target.event_id);
            let command = Command::Send { account: account.clone(), key: key.clone(), reply_to, message };
            self.dispatch(command, Purpose::Send);
        }
        tx.scroll_to_bottom(timeline_view(target));
    }

    /// Starts recording a voice memo in a composer.
    pub fn start_voice_memo(&mut self, target: ComposerTarget) {
        let (Some(account), Some(key)) = (self.nav.account.clone(), self.composer_key(target)) else { return };
        if self.composer(target).voice.is_some() {
            return;
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
        let path = self.shell.scratch().join(format!("voice-{stamp}.m4a"));
        let recorder = self.shell.recorder(path, self.settings.devices.microphone.clone());
        recorder.start();
        self.composer_mut(target).voice = Some(VoiceMemo { recorder, timeline: (account, key) });
    }

    /// Drops the voice memo being recorded.
    pub fn cancel_voice_memo(&mut self, target: ComposerTarget) {
        if let Some(memo) = self.composer_mut(target).voice.take() {
            memo.recorder.cancel();
        }
        self.resume_edits();
    }

    /// Stops recording and sends the memo as a voice message, to the room or thread it was
    /// recorded for, answering what the draft answered then.
    pub fn send_voice_memo(&mut self, target: ComposerTarget, tx: &mut Tx) {
        let Some(memo) = self.composer_mut(target).voice.take() else { return };
        let (account, key) = memo.timeline;
        let reply_to = self.composer_mut(target).reply_to.take().map(|reply| reply.event_id);
        let memo_for = MemoFor { target, account, key, reply_to };
        if let Some(result) = self.shell.finish_recording(memo.recorder, memo_for.clone()) {
            self.voice_memo_recorded(memo_for, result, tx);
        }
        self.resume_edits();
    }

    /// A voice memo's file is written: it goes where it was recorded for.
    pub fn voice_memo_recorded(
        &mut self,
        memo: MemoFor,
        result: Result<crate::platform::sound::Recording, String>,
        tx: &mut Tx,
    ) {
        match result {
            Ok(recording) => {
                let message = matrix_core::Outgoing::Voice {
                    upload: matrix_core::Upload::Path(recording.path),
                    duration_ms: (recording.duration * 1000.0).round() as u64,
                    waveform: recording.waveform,
                };
                let command = Command::Send { account: memo.account, key: memo.key, reply_to: memo.reply_to, message };
                self.dispatch(command, Purpose::Send);
                tx.scroll_to_bottom(timeline_view(memo.target));
            }
            Err(error) => self.notice = Some(tf!("notice.voice", error = error)),
        }
    }

    /// A clip couldn't be fetched or played: it stops showing as playing, and the user hears why.
    /// `clip` is the [`AudioClip::id`] whose player failed, or `None` when the download did; a
    /// player picked before the clip was picked again fails unheard.
    pub fn audio_failed(&mut self, media: &matrix_core::MediaRef, clip: Option<u64>, error: String) {
        let current = |c: &AudioClip| c.media == *media && clip.is_none_or(|id| c.id == id);
        if self.audio.as_ref().is_some_and(current) {
            self.audio = None;
        } else if clip.is_some() {
            return;
        }
        self.notice = Some(tf!("notice.audio", error = error));
    }

    /// The second a voice memo's timer shows, and a playing clip's and previewed video's second
    /// and progress step, while any is counting: the view rebuilds when it changes.
    fn clock(&self) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
        let recording = [&self.composer, &self.thread_composer]
            .iter()
            .filter_map(|composer| composer.voice.as_ref())
            .map(|memo| memo.recorder.elapsed().max(0.0) as u64)
            .max();
        // The second shown and the step of the progress, together.
        let step = |time: f64, progress: f64| (time.max(0.0) as u64) << 16 | (progress * PROGRESS_STEPS) as u64;
        let playing = || {
            let clip = self.audio.as_ref()?;
            let player = clip.player.as_ref().filter(|player| player.is_playing())?;
            Some(step(player.time(), clip.progress()))
        };
        let video = self.preview_video().filter(|player| player.is_playing()).map(|p| step(p.time(), p.progress()));
        let playing = playing();
        (recording.is_some() || playing.is_some() || video.is_some()).then_some((recording, playing, video))
    }

    /// Asks for files with the system dialog for a composer; they arrive through
    /// [`State::files_picked`].
    pub fn pick_attachments(&mut self, target: ComposerTarget) {
        self.shell.pick_files(PickFor::Attachments(target));
    }

    /// The system file dialog's answer.
    pub fn files_picked(&mut self, picked_for: PickFor, paths: &[PathBuf]) {
        match picked_for {
            PickFor::Attachments(target) => self.attach_files_to(target, paths),
            PickFor::Avatar => {
                if let Some(path) = paths.first() {
                    self.set_avatar(Some(path.clone()));
                }
            }
        }
    }

    /// A paste into a composer: an image or files become attachments, and a lone web address a
    /// link (true); anything else is left to the editor to paste (false).
    pub fn paste_into_composer(&mut self, target: ComposerTarget, content: &rune::clipboard::ClipboardContent) -> bool {
        let composer = self.composer_mut(target);
        // An edit changes the text only: nothing attaches to it.
        if composer.editing.is_none() {
            if let Some(image) = content.image() {
                composer.attach_pasted_image(image.png());
                return true;
            }
            if !content.files().is_empty() {
                composer.attach(content.files().iter().cloned());
                return true;
            }
        }
        // In a code block links can't be, so the address pastes as text.
        if matrix_core::links::is_link(content.text()) && composer.edit.active_block() != rune::Block::Code {
            composer.paste_link(content.text().trim());
            return true;
        }
        false
    }

    /// Attaches files to a composer: dropped onto its pane. Not to an edit.
    pub fn attach_files_to(&mut self, target: ComposerTarget, paths: &[PathBuf]) {
        let composer = self.composer_mut(target);
        if composer.editing.is_none() {
            composer.attach(paths.iter().cloned());
        }
        self.drop_hover = false;
    }

    pub fn set_zoom(&mut self, zoom: Zoom) {
        self.settings.zoom = zoom;
        self.shell.save_settings(self.settings.clone());
    }

    /// Opens a link: a permalink in the app, anything else in the browser.
    pub fn open_link(&mut self, url: &str, tx: &mut Tx) {
        if matrix_core::Permalink::is_permalink(url) {
            navigation::navigate_now(self, tx, |s| s.open_permalink(url));
        } else if let Err(error) = tx.open_url(url) {
            self.notice = Some(tf!("notice.browser", error = error));
        }
    }

    /// Shows links' preview cards, or not.
    pub fn set_show_link_previews(&mut self, show: bool) {
        self.settings.show_link_previews = show;
        self.shell.save_settings(self.settings.clone());
        self.fetch_link_previews(std::time::Instant::now());
        self.request_timeline_media();
    }

    pub fn set_show_link_gifs(&mut self, show: bool) {
        self.settings.show_link_gifs = show;
        self.shell.save_settings(self.settings.clone());
        self.request_gifs();
        // A GIF link gets a card while GIFs don't play.
        self.fetch_link_previews(std::time::Instant::now());
    }

    /// Clears the notice once it has been on screen a while; true when it went.
    fn expire_notice(&mut self, tx: &mut Tx) -> bool {
        /// Long enough to read a sentence or two.
        const SHOWN_FOR: std::time::Duration = std::time::Duration::from_secs(6);
        let Some(message) = &self.notice else {
            self.notice_shown = None;
            return false;
        };
        match &self.notice_shown {
            Some((shown, since)) if shown == message => {
                if since.elapsed() >= SHOWN_FOR {
                    self.notice = None;
                    self.notice_shown = None;
                    return true;
                }
            }
            _ => self.notice_shown = Some((message.clone(), std::time::Instant::now())),
        }
        tx.wake_after(std::time::Duration::from_millis(500));
        false
    }

    pub fn dismiss_notice(&mut self) {
        self.notice = None;
    }
}

/// The name of a kind of call: video, voice, or just a call when the caller didn't say.
pub fn call_title(card: &matrix_core::CallCard) -> &'static str {
    match card.video {
        Some(true) => t!("call.card.video"),
        Some(false) => t!("call.card.voice"),
        None => t!("call.card.call"),
    }
}

#[cfg(test)]
mod tests;

/// The element key of a composer's text field.
pub fn composer_field(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Room => "composer",
        ComposerTarget::Thread => "thread-composer",
    }
}

/// The element key of the scroll view a composer's messages appear in.
pub fn timeline_view(target: ComposerTarget) -> &'static str {
    match target {
        ComposerTarget::Room => "timeline",
        ComposerTarget::Thread => "thread-timeline",
    }
}
