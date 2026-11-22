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
pub mod calls;
pub mod discovery;
pub mod engine;
pub mod model;
pub mod people;
pub mod permalinks;
pub mod profile;
pub mod push;
pub mod rooms;
pub mod search;
pub mod secrets;
pub mod security;
pub mod sidebar;
pub mod status;
pub mod store;

mod convert;
mod drafts;
mod history;
mod polls;
mod saved;
mod session;
mod source;
#[cfg(test)]
mod test_server;
mod thumbnail;
mod timeline;
mod timeline_bootstrap;
mod tls;
pub mod widgets;

pub mod links;

pub use account::{Account, ClientIdentity, LoginRequest, OAuthRedirect};
pub use calls::{
    CallDevices, CallEnd, CallInfo, CallParticipant, CallPhase, CallVia, DeviceChoice, DeviceKind, IncomingCall,
    MediaDevice, MediaToggle, RoomCall, VideoId, VideoSink,
};
pub use discovery::{Homeserver, SignInInput};
pub use engine::{
    Command, DetachedCommands, Engine, EngineConfig, EngineHandle, EngineStopped, Event, EventSink, RequestId,
    StoreSharing,
};
pub use model::{
    AccountId, AccountSummary, Attachment, AttachmentKind, AudioDetails, CallCard, EntryKind, HiddenEvent,
    HistoryVisibility, Invitation, LoginMethods, MediaRef, MediaSize, Message, MessageContent, Notification,
    NotificationMode, Outgoing, Password, Person, Poll, PollAnswer, PublicRoom, PushContent, PushTarget, Pusher, Quote,
    Reaction, ReadReceipt, RoomChange, RoomEvent, RoomKind, RoomList, RoomSummary, SignInMethod, SpaceSummary,
    ThreadSummary, TimelineDiff, TimelineEntry, TimelineKey, Upload,
};

/// How many messages a timeline page holds: what a room shows when it opens, and what each
/// scroll to the top loads. About a screen and a half at the desktop's density.
pub const HISTORY_PAGE: u16 = 30;
pub use drafts::Draft;
pub use history::{MessagePreview, SharedFile, ThreadInfo};
pub use people::{MemberRole, Members, Moderation, PersonPresence, Presence, RoomMember};
pub use permalinks::{LinkAction, LinkTarget, Permalink};
pub use profile::{AccountPage, ContactDetail, ContactKind, Contacts, PendingContact};
pub use rooms::{DirectoryKind, JoinRule, RoomPreview};
pub use saved::SavedMessage;
pub use search::{MessageHit, SearchUsage};
pub use secrets::{FileSecrets, MemorySecrets, SecretError, SecretStore};
pub use security::{RecoveryStatus, SasEmoji, Security, Session, VerificationStep};
pub use sidebar::{ScopeLayout, Section, SidebarLayout};
pub use source::EventSource;
pub use status::Status;
pub use store::StoreLayout;
pub use widgets::{Widget, Widgets};

/// Shown to other devices and in the server's session list.
pub const DEVICE_DISPLAY_NAME: &str = "Better Matrix";

/// Everything that can go wrong in the core.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
    /// A widget's address isn't a web page (`http` or `https`).
    #[error("a widget needs a web address")]
    InvalidWidgetUrl,
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
    /// A command needs a room's timeline open first.
    #[error("the timeline of {0} is not open")]
    TimelineClosed(String),
    /// The command was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// A push couldn't be registered or read.
    #[error("push notifications: {0}")]
    Push(String),
    /// The server accepted a login but the client reports no user.
    #[error("the server accepted the login but reported no user ID")]
    NoUserId,
}

/// `bytes` random bytes from the OS, as lowercase hex.
pub(crate) fn random_hex(bytes: usize) -> Result<String, Error> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| Error::Storage(std::io::Error::other(e.to_string())))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}
