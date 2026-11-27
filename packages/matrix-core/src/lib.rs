//! Matrix client core shared by every Better Matrix app (desktop now, mobile later).
//!
//! The core is an [`engine`]: an app sends [`Command`]s through an [`EngineHandle`] and hears
//! back [`Event`]s through its [`EventSink`]. Everything the UIs render is a plain value from
//! [`model`]; no SDK type crosses that boundary, so a UI never blocks on the network and a new
//! front end (mobile, CLI, tests) only implements a sink.
//!
//! Adding a feature means adding a [`Command`] variant, the async work behind it in
//! [`account`], and the [`Event`]s it reports.

#![warn(missing_docs)]
// matrix-sdk futures nest deeply enough that proving `Send` for a spawned sync loop exceeds the default.
#![recursion_limit = "256"]

pub mod account;
pub mod admin;
pub mod calls;
pub mod discovery;
pub mod emoticons;
pub mod engine;
pub mod geo;
pub mod gif_search;
pub mod html;
pub mod markdown;
pub mod math;
pub mod model;
pub mod people;
pub mod permalinks;
pub mod profile;
pub mod push;
pub mod qr;
pub mod qr_login;
pub mod registration;
pub mod rooms;
pub mod safety;
pub mod search;
pub mod secrets;
pub mod security;
pub mod sidebar;
pub mod status;
pub mod store;

mod convert;
mod drafts;
mod edits;
mod history;
mod note_to_self;
mod offline_device;
mod outgoing;
pub mod packs;
mod polls;
mod saved;
mod scheduled;
mod session;
mod source;
#[cfg(test)]
mod test_server;
mod thumbnail;
pub use thumbnail::upload_thumbnail;
#[cfg(feature = "command-line")]
pub mod terminal;
mod timeline;
mod timeline_bootstrap;
mod tls;
pub mod widgets;

pub mod links;

pub use account::{Account, ClientIdentity, LoginRequest, OAuthRedirect};
pub use admin::{NewRoom, Permission, PermissionLevel, RoomRights, RoomSetting, RoomSettings, SpaceChange};
pub use calls::{
    CallDevices, CallEnd, CallInfo, CallParticipant, CallPhase, CallVia, DeviceChoice, DeviceKind, IncomingCall,
    MediaDevice, MediaToggle, RoomCall, VideoId, VideoSink,
};
pub use discovery::{Homeserver, SignInInput};
pub use engine::{
    Answer, Command, DetachedCommands, Engine, EngineConfig, EngineHandle, EngineStopped, Event, EventSink, RequestId,
    StoreSharing,
};
pub use model::{
    AccountId, AccountSummary, Attachment, AttachmentKind, AudioDetails, CallCard, EditVersion, EntryKind, FoundGif,
    GifPage, HiddenEvent, HistoryVisibility, ImagePack, Invitation, LiveDuration, Location, LoginMethods, MediaRef,
    MediaSize, Mentions, Message, MessageContent, NewPackImage, NewPackImageSource, Notification, NotificationMode,
    Outgoing, PackEdit, PackImage, PackImageChange, PackPlace, Password, Person, Place, Poll, PollAnswer, PublicRoom,
    PushContent, PushTarget, Pusher, Quote, Reaction, ReadReceipt, RoomChange, RoomEvent, RoomKind, RoomList,
    RoomPriority, RoomSummary, ScheduledAction, ScheduledMessage, SignInMethod, SpaceSummary, Successor, ThreadSummary,
    TimelineDiff, TimelineEntry, TimelineKey, Upload,
};

/// How many messages a timeline page holds: what a room shows when it opens, and what each
/// scroll to the top loads. About a screen and a half at the desktop's density.
pub const HISTORY_PAGE: u16 = 30;
pub use drafts::Draft;
pub use history::{MessagePreview, SharedFile, ThreadInfo};
pub use people::{MemberRole, Members, Moderation, PersonPresence, Presence, RoomMember};
pub use permalinks::{LinkAction, LinkTarget, Permalink};
pub use profile::{AccountPage, ContactDetail, ContactKind, Contacts, PendingContact};
pub use qr::QrImage;
pub use qr_login::{QrLoginStep, QrSignIn};
pub use registration::{Policy, RegisterRequest, RegistrationStep};
pub use rooms::{DirectoryKind, JoinRule, RoomPreview};
pub use safety::{ReportTarget, Safety, SafetyChange, ShowMedia};
pub use saved::SavedMessage;
pub use scheduled::NOT_POSTED;
pub use search::{MessageHit, SearchUsage};
pub use secrets::{FileSecrets, MemorySecrets, SecretError, SecretStore};
pub use security::{Identity, KeyImport, OfflineDevice, RecoveryStatus, SasEmoji, Security, Session, VerificationStep};
pub use sidebar::{ScopeLayout, Section, SidebarLayout};
pub use source::EventSource;
pub use status::Status;
pub use store::StoreLayout;
pub use widgets::meetings::Meeting;
pub use widgets::session::{
    PermissionKind, WidgetAccess, WidgetAccessKind, WidgetBridge, WidgetPage, WidgetPermission, WidgetUrlField,
};
pub use widgets::{RoomBridge, Widget, Widgets};

/// Shown to other devices and in the server's session list.
pub const DEVICE_DISPLAY_NAME: &str = "Better Matrix";

/// Everything that can go wrong in the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A map or location service failed.
    #[error("{0}")]
    Location(String),
    /// A message is gone, unreadable, or unsupported for this action.
    #[error("the message is no longer available for this action")]
    MessageUnavailable,
    /// The client could not be configured, e.g. an unreachable or invalid homeserver.
    #[error("could not set up the Matrix client: {0}")]
    Build(#[from] matrix_sdk::ClientBuildError),
    /// The SDK or the server rejected a request.
    #[error(transparent)]
    Sdk(#[from] matrix_sdk::Error),
    /// A Matrix identifier did not parse.
    #[error("invalid Matrix identifier: {0}")]
    Id(#[from] matrix_sdk::IdParseError),
    /// The homeserver doesn't offer what was asked of it.
    #[error("your homeserver doesn't support {0}")]
    Unsupported(&'static str),
    /// A widget's address isn't a web page (`http` or `https`).
    #[error("a widget needs a web address")]
    InvalidWidgetUrl,
    /// The widget is no longer pinned to the room.
    #[error("that widget is no longer in the room")]
    WidgetGone,
    /// A meeting's server couldn't be reached or wouldn't let the user join.
    #[error("could not join the meeting: {0}")]
    Meeting(String),
    /// The integration manager couldn't be reached or refused the account.
    #[error("could not open the integration manager: {0}")]
    Integrations(String),
    /// A pack image's short code must be something without spaces or colons.
    #[error("\"{0}\" can't be a short code: use no spaces or colons")]
    InvalidShortcode(String),
    /// Another image in the pack has that short code.
    #[error("another image in the pack is called \"{0}\"")]
    ShortcodeTaken(String),
    /// GIF search needs the user's GIPHY API key. (The phone app tells this and
    /// [`Error::GifKeyRefused`] apart by their text: change both together.)
    #[error("GIF search needs a GIPHY API key")]
    NoGifKey,
    /// GIPHY refused the key.
    #[error("GIPHY refused the API key")]
    GifKeyRefused,
    /// GIPHY couldn't be asked, or answered with an error.
    #[error("GIPHY didn't answer: {0}")]
    GifSearch(String),
    /// A poll needs a question and two to twenty answers.
    #[error("a poll needs two to twenty answers")]
    InvalidPoll,
    /// A call failed.
    #[error(transparent)]
    Call(#[from] calls::CallError),
    /// A file to send could not be read.
    #[error("could not read {path}: {source}")]
    ReadFile {
        /// The file.
        path: std::path::PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The engine's background runtime could not start.
    #[error("could not start the engine runtime: {0}")]
    Runtime(#[from] std::io::Error),
    /// A command named an account that is not signed in.
    #[error("no signed-in account {0}")]
    UnknownAccount(AccountId),
    /// A command named a room the account has not joined.
    #[error("account {account} has not joined room {room_id}")]
    UnknownRoom {
        /// The account the command named.
        account: AccountId,
        /// The room the command named.
        room_id: String,
    },
    /// The OAuth 2.0 flow failed.
    #[error(transparent)]
    OAuth(#[from] matrix_sdk::authentication::oauth::OAuthError),
    /// The browser sign-in did not come back.
    #[error("browser sign-in failed: {0}")]
    BrowserLogin(String),
    /// Changing notification settings failed.
    #[error(transparent)]
    NotificationSettings(#[from] matrix_sdk::NotificationSettingsError),
    /// Saving or reading local data failed.
    #[error("could not save local data: {0}")]
    Storage(std::io::Error),
    /// A paginated server response repeated its continuation token.
    #[error("the server repeated a pagination token for {resource}")]
    RepeatedPaginationToken {
        /// The resource being paginated.
        resource: String,
    },
    /// The secret store failed.
    #[error("could not use the secret store: {0}")]
    Secret(#[from] SecretError),
    /// The room timeline failed.
    #[error(transparent)]
    Timeline(#[from] matrix_sdk_ui::timeline::Error),
    /// Queueing a message for sending failed.
    #[error(transparent)]
    SendQueue(#[from] matrix_sdk::send_queue::RoomSendQueueError),
    /// The event cache failed.
    #[error(transparent)]
    EventCache(#[from] matrix_sdk::event_cache::EventCacheError),
    /// Fetching a linked file failed.
    #[error("could not load {url}: {reason}")]
    Link {
        /// The link.
        url: String,
        /// Why.
        reason: String,
    },
    /// Recovery failed, e.g. a wrong recovery key.
    #[error(transparent)]
    Recovery(#[from] matrix_sdk::encryption::recovery::RecoveryError),
    /// The local crypto store failed.
    #[error(transparent)]
    CryptoStore(#[from] matrix_sdk::encryption::CryptoStoreError),
    /// A server request failed.
    #[error(transparent)]
    Http(#[from] matrix_sdk::HttpError),
    /// The server wants the account password for this; send the command again with it.
    #[error("the server asks for your password")]
    PasswordRequired,
    /// A profile picture must be an image.
    #[error("{} is not an image", .0.display())]
    NotAnImage(std::path::PathBuf),
    /// The server checks phone codes itself, so the app can't send one.
    #[error("this server doesn't take confirmation codes from the app")]
    NoCodeSubmission,
    /// Sending a confirmation code failed.
    #[error("could not send the code: {0}")]
    Code(matrix_sdk::reqwest::Error),
    /// The server rejected a confirmation code.
    #[error("that code is not right")]
    WrongCode,
    /// The server did not answer in time.
    #[error("the server took too long to answer")]
    TimedOut,
    /// The browser approval of an identity reset did not arrive in time.
    #[error("the reset was not approved in time; start it again")]
    ApprovalTimedOut,
    /// The server offers no account page to end sessions on.
    #[error("the server has no account page to end sessions on")]
    NoAccountPage,
    /// The verification ended or is unknown.
    #[error("that verification is no longer running")]
    VerificationGone,
    /// The account has no cross-signing identity, so no other session can verify this one.
    #[error("this account has no cross-signing identity yet")]
    NoIdentity,
    /// Something about the account itself failed.
    #[error("{0}")]
    Account(String),
    /// A room has unread messages but none this device can mark read yet.
    #[error("there is no message to mark read yet")]
    NothingToRead,
    /// A command needs a room's timeline open first.
    #[error("the timeline of {0} is not open")]
    TimelineClosed(String),
    /// The command was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// A push couldn't be registered or read.
    #[error("push notifications: {0}")]
    Push(String),
    /// Only a room in a space can let the space's members join.
    #[error("only a room in a space can let the space's members join")]
    NoParentSpace,
    /// The user's power level in the room doesn't allow it.
    #[error("you aren't allowed to change that in this room")]
    NotAllowed,
    /// Someone's request to join or ban changed (another moderator answered it) before the
    /// user's answer went out.
    #[error("that changed meanwhile; the list shows how it is now")]
    Outdated,
    /// The room was upgraded already: its successor is where it goes on.
    #[error("this room was replaced already")]
    AlreadyReplaced,
    /// A space's rooms changed while a new order for them was on its way.
    #[error("the space's rooms changed meanwhile; try again")]
    SpaceChanged,
    /// Another room has that address.
    #[error("{0} belongs to another room")]
    AddressTaken(String),
    /// The server accepted a login but the client reports no user.
    #[error("the server accepted the login but reported no user ID")]
    NoUserId,
    /// A terminal sign-in completed as a different account.
    #[error("expected to sign in as {expected}, but the server signed in {actual}")]
    UnexpectedAccount {
        /// The selected user.
        expected: AccountId,
        /// The user the server authorized.
        actual: String,
    },
    /// The server returned a new device when the old one was being resumed.
    #[error("the server returned a different device while resuming this session")]
    UnexpectedDevice,
    /// A suspended account could not take back its store and device.
    #[error("could not resume the saved device on this homeserver")]
    ResumeStore,
    /// A QR code couldn't be drawn or read.
    #[error("could not read or draw the QR code: {0}")]
    QrCode(String),
    /// Signing in with a QR code failed.
    #[error("signing in with the QR code failed: {0}")]
    QrLogin(String),
    /// The scanned code isn't one for signing in.
    #[error("that QR code isn't for signing in")]
    NotASignInCode,
    /// The scanned sign-in code is from the wrong side: a new device's where a signed-in
    /// device's was needed, or the other way round.
    #[error("scan this code from the other device instead")]
    WrongSignInCode,
    /// The scanned code isn't the other side's verification code.
    #[error("that QR code isn't the other side's verification code")]
    NotAVerificationCode,
    /// Verifying someone takes a direct chat both have joined.
    #[error("start a direct chat with them first, and verify once they've joined it")]
    NoDirectChat,
    /// The other person hasn't set up encryption, so they can't be verified.
    #[error("they haven't set up encryption yet, so they can't be verified")]
    NoTheirIdentity,
    /// Importing message keys failed, e.g. a wrong passphrase.
    #[error("could not import the keys: {0}")]
    KeyImport(String),
    /// The offline device failed.
    #[error("offline device: {0}")]
    OfflineDevice(String),
    /// The offline device's key doesn't open the device on the server (replaced elsewhere).
    #[error("the offline device was replaced with another key")]
    StaleOfflineKey,
    /// Turning the offline device on needs the recovery key.
    #[error("the recovery key is needed for this")]
    RecoveryKeyRequired,
    /// The recovery key doesn't unlock the account's secrets.
    #[error("that recovery key is not right")]
    WrongRecoveryKey,
    /// A QR sign-in was for an account this device has already (signed in, or suspended: sign
    /// in again from its entry, which keeps its data).
    #[error("{0} is already on this device; sign in again from its entry instead")]
    AlreadySignedIn(String),
    /// The server asks for a registration step no client can take.
    #[error("this server doesn't let the app create accounts")]
    RegistrationUnsupported,
}

/// `bytes` random bytes from the OS, as lowercase hex.
pub(crate) fn random_hex(bytes: usize) -> Result<String, Error> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| Error::Storage(std::io::Error::other(e.to_string())))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}
