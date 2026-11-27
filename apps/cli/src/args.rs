use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "bm", version, about = crate::text::t("Better Matrix for the terminal"), arg_required_else_help = true)]
pub(crate) struct Cli {
    #[arg(help = crate::text::t("Use this signed-in Matrix account."))]
    #[arg(long, global = true)]
    pub account: Option<String>,
    #[arg(help = crate::text::t("Use this directory for CLI accounts and settings."))]
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
    #[arg(help = crate::text::t("Write JSON results, or one JSON object per line when watching."))]
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(help = crate::text::t("Fail when a command needs to prompt."))]
    #[arg(long, global = true)]
    pub no_input: bool,
    #[arg(help = crate::text::t("Confirm the requested change."))]
    #[arg(long, global = true)]
    pub yes: bool,
    #[arg(help = crate::text::t("Time limit in seconds for one-shot work."))]
    #[arg(long, global = true, value_parser = clap::value_parser!(u64).range(1..=86_400))]
    pub timeout: Option<u64>,
    #[command(subcommand)]
    pub command: Top,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Top {
    #[command(about = crate::text::t("Sign in, manage accounts and sessions, and verify this session."))]
    #[command(subcommand)]
    Auth(Auth),
    #[command(about = crate::text::t("Find and manage rooms."))]
    #[command(subcommand)]
    Rooms(Rooms),
    #[command(about = crate::text::t("Find and manage spaces and their children."))]
    #[command(subcommand)]
    Spaces(Spaces),
    #[command(about = crate::text::t("List or start direct chats."))]
    #[command(subcommand)]
    Dms(Dms),
    #[command(about = crate::text::t("Review or answer invitations."))]
    #[command(subcommand)]
    Invites(Invites),
    #[command(about = crate::text::t("Read or send messages."))]
    #[command(subcommand)]
    Messages(Messages),
    #[command(about = crate::text::t("List threads in a room."))]
    #[command(subcommand)]
    Threads(RoomList),
    #[command(about = crate::text::t("Manage a room's pinned messages."))]
    #[command(subcommand)]
    Pins(Pins),
    #[command(about = crate::text::t("Manage your saved messages."))]
    #[command(subcommand)]
    Saved(Saved),
    #[command(about = crate::text::t("List, upload, or download files."))]
    #[command(subcommand)]
    Files(Files),
    #[command(about = crate::text::t("List recent messages highlighted by your notification rules."))]
    Mentions(Mentions),
    #[command(about = crate::text::t("Manage notification modes and keywords."))]
    #[command(subcommand)]
    Notifications(Notifications),
    #[command(about = crate::text::t("Look up or ignore people."))]
    #[command(subcommand)]
    People(People),
    #[command(about = crate::text::t("View or change your profile."))]
    #[command(subcommand)]
    Profile(Profile),
    #[command(about = crate::text::t("Report something to the homeserver's administrators."))]
    #[command(subcommand)]
    Report(Report),
    #[command(about = crate::text::t("Wait for a sync with the selected homeserver."))]
    Sync,
    #[command(about = crate::text::t("Show the CLI data directory and check the selected session."))]
    Doctor,
    #[command(about = crate::text::t("Print a shell completion script."))]
    Completions { shell: Shell },
    #[command(about = crate::text::t("Install the bundled Better Matrix agent skill."))]
    Skill {
        #[command(subcommand)]
        action: Option<Skill>,
    },
}

#[derive(Args, Debug)]
pub(crate) struct Target {
    #[arg(help = crate::text::t("Matrix room ID, alias, or permalink."))]
    pub room: String,
}

#[derive(Args, Debug)]
pub(crate) struct EventTarget {
    #[command(flatten)]
    pub target: Target,
    #[arg(help = crate::text::t("Matrix event ID."))]
    pub event_id: String,
}

#[derive(Args, Debug)]
pub(crate) struct MemberTarget {
    #[command(flatten)]
    pub target: Target,
    #[arg(help = crate::text::t("Full Matrix user ID."))]
    pub user_id: String,
}

#[derive(Args, Debug)]
pub(crate) struct UserTarget {
    #[arg(help = crate::text::t("Full Matrix user ID."))]
    pub user_id: String,
}

#[derive(Args, Debug)]
pub(crate) struct Query {
    #[arg(help = crate::text::t("Search term."))]
    pub query: String,
}

#[derive(Args, Debug)]
pub(crate) struct RoomFilter {
    #[arg(help = crate::text::t("Include only this room."))]
    #[arg(long)]
    pub room: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Auth {
    #[command(about = crate::text::t("Look up sign-in methods for a homeserver."))]
    Discover { server: String },
    #[command(about = crate::text::t("Sign an account in and create a CLI session."))]
    Login {
        user_id: String,
        #[arg(long)]
        server: Option<String>,
        #[arg(long, value_enum)]
        method: Option<LoginMethod>,
        #[arg(long)]
        password_stdin: bool,
    },
    #[command(about = crate::text::t("List accounts saved in this CLI store."))]
    List,
    #[command(about = crate::text::t("Set the default account."))]
    Use(UserTarget),
    #[command(about = crate::text::t("Show the selected account and device."))]
    Whoami,
    #[command(about = crate::text::t("Sign out and remove the local session."))]
    Logout,
    #[command(about = crate::text::t("Manage the account's devices."))]
    #[command(subcommand)]
    Sessions(Sessions),
    #[command(about = crate::text::t("Compare emoji with another device or user."))]
    Verify { user_id: Option<String> },
    #[command(about = crate::text::t("Show recovery status or restore with a recovery key."))]
    #[command(subcommand)]
    Recovery(Recovery),
    #[command(about = crate::text::t("Import or export encrypted room keys."))]
    #[command(subcommand)]
    Keys(Keys),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum LoginMethod {
    Password,
    Oauth,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Sessions {
    #[command(about = crate::text::t("List sessions signed in to this account."))]
    List,
    #[command(about = crate::text::t("Change a session's name."))]
    Rename { device_id: String, name: String },
    #[command(about = crate::text::t("Sign another session out."))]
    End { device_id: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum Recovery {
    #[command(about = crate::text::t("Show this session's recovery and verification status."))]
    Status,
    #[command(about = crate::text::t("Verify this session using the account's recovery key."))]
    Restore {
        #[arg(long)]
        key_stdin: bool,
    },
}

#[derive(Args, Debug)]
pub(crate) struct KeyFile {
    #[arg(help = crate::text::t("Path to the encrypted key file."))]
    pub file: PathBuf,
    #[arg(help = crate::text::t("Read the file passphrase from stdin."))]
    #[arg(long)]
    pub passphrase_stdin: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Keys {
    #[command(about = crate::text::t("Read room keys from an encrypted file."))]
    Import(KeyFile),
    #[command(about = crate::text::t("Write room keys to an encrypted file."))]
    Export(KeyFile),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Rooms {
    #[command(about = crate::text::t("List joined group rooms."))]
    List {
        #[arg(long)]
        unread: bool,
        #[arg(long)]
        space: Option<String>,
    },
    #[command(about = crate::text::t("Show a joined room or preview a room."))]
    Show(Target),
    #[command(about = crate::text::t("Search the public room directory."))]
    Discover(Query),
    #[command(about = crate::text::t("Create a room, private and encrypted by default."))]
    Create(CreateRoom),
    #[command(about = crate::text::t("Join a room by ID, alias, or permalink."))]
    Join(Join),
    #[command(about = crate::text::t("Ask to join a room."))]
    Knock {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        reason: Option<String>,
    },
    #[command(about = crate::text::t("Leave a room."))]
    Leave(Target),
    #[command(about = crate::text::t("Forget a room you have already left."))]
    Forget(Target),
    #[command(about = crate::text::t("List members by membership state."))]
    Members {
        #[command(flatten)]
        target: Target,
        #[arg(long, value_enum, default_value = "joined")]
        membership: Membership,
    },
    #[command(about = crate::text::t("Invite someone to a room."))]
    Invite(MemberTarget),
    #[command(about = crate::text::t("Remove someone from a room."))]
    Kick(MemberTarget),
    #[command(about = crate::text::t("Ban someone from a room."))]
    Ban(MemberTarget),
    #[command(about = crate::text::t("Lift a room ban."))]
    Unban(MemberTarget),
    #[command(about = crate::text::t("Approve someone's request to join."))]
    Approve(MemberTarget),
    #[command(about = crate::text::t("Deny someone's request to join."))]
    Deny(MemberTarget),
    #[command(about = crate::text::t("Set a room member's power level."))]
    PowerLevel {
        #[command(flatten)]
        member: MemberTarget,
        level: i64,
    },
    #[command(about = crate::text::t("Send a read receipt and clear the unread mark."))]
    MarkRead {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        through: Option<String>,
    },
    #[command(about = crate::text::t("Set or clear a manual unread mark."))]
    MarkUnread {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        clear: bool,
    },
    #[command(about = crate::text::t("Set the room's priority."))]
    Priority {
        #[command(flatten)]
        target: Target,
        #[arg(value_enum)]
        priority: Priority,
    },
    #[command(about = crate::text::t("Read or change room or space settings."))]
    #[command(subcommand)]
    Settings(Settings),
    #[command(about = crate::text::t("Replace the room with one using a newer Matrix room version."))]
    Upgrade {
        #[command(flatten)]
        target: Target,
        version: String,
    },
}

#[derive(Args, Debug)]
pub(crate) struct CreationOptions {
    pub name: String,
    #[arg(long)]
    pub topic: Option<String>,
    #[arg(long)]
    pub alias: Option<String>,
    #[arg(long, value_enum, default_value = "private")]
    pub join_rule: JoinRule,
    #[arg(long)]
    pub publish: bool,
}

#[derive(Args, Debug)]
pub(crate) struct CreateRoom {
    #[command(flatten)]
    pub create: CreationOptions,
    #[arg(long)]
    pub space: Option<String>,
    #[arg(long)]
    pub unencrypted: bool,
}

#[derive(Args, Debug)]
pub(crate) struct Join {
    #[command(flatten)]
    pub target: Target,
    #[arg(help = crate::text::t("Ask this server to help join. May be repeated."))]
    #[arg(long)]
    pub via: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum JoinRule {
    Private,
    Public,
    Knock,
    Restricted,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Membership {
    Joined,
    Invited,
    Banned,
    Knocking,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Priority {
    Favourite,
    Low,
    Normal,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Settings {
    #[command(about = crate::text::t("Show settings, aliases, and permissions."))]
    Show(Target),
    #[command(about = crate::text::t("Set a name, topic, avatar, join rule, history, guest access, or directory listing."))]
    Set {
        #[command(flatten)]
        target: Target,
        #[arg(value_enum)]
        setting: Setting,
        value: String,
    },
    #[command(about = crate::text::t("Manage the room's aliases."))]
    #[command(subcommand)]
    Alias(Aliases),
    #[command(about = crate::text::t("Set the power level required for a permission."))]
    Permission {
        #[command(flatten)]
        target: Target,
        permission: String,
        level: i64,
    },
    #[command(about = crate::text::t("Enable end-to-end encryption permanently."))]
    Encrypt(Target),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Setting {
    Name,
    Topic,
    Avatar,
    JoinRule,
    History,
    GuestAccess,
    Directory,
}

#[derive(Args, Debug)]
pub(crate) struct AliasTarget {
    #[command(flatten)]
    pub target: Target,
    pub alias: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Aliases {
    #[command(about = crate::text::t("Add an alias."))]
    Add(AliasTarget),
    #[command(about = crate::text::t("Remove an alias."))]
    Remove(AliasTarget),
    #[command(about = crate::text::t("Make an alias primary."))]
    Primary(AliasTarget),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Spaces {
    #[command(about = crate::text::t("List joined spaces."))]
    List,
    #[command(about = crate::text::t("Show a space and its child IDs."))]
    Show(Target),
    #[command(about = crate::text::t("Search the public space directory."))]
    Discover(Query),
    #[command(about = crate::text::t("Create a space, private by default."))]
    Create {
        #[command(flatten)]
        create: CreationOptions,
        #[arg(long)]
        parent: Option<String>,
    },
    #[command(about = crate::text::t("Join a space."))]
    Join(Join),
    #[command(about = crate::text::t("Leave a space. Its children stay joined."))]
    Leave(Target),
    #[command(about = crate::text::t("List or arrange a space's rooms and subspaces."))]
    #[command(subcommand)]
    Children(Children),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Children {
    #[command(about = crate::text::t("List the children visible to this account."))]
    List {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        recursive: bool,
    },
    #[command(about = crate::text::t("Add a joined room or space."))]
    Add(SpaceChild),
    #[command(about = crate::text::t("Remove a room or space from the space."))]
    Remove(SpaceChild),
    #[command(about = crate::text::t("Set the order, supplying every direct child ID."))]
    Order {
        #[command(flatten)]
        target: Target,
        #[arg(required = true)]
        child_ids: Vec<String>,
    },
}

#[derive(Args, Debug)]
pub(crate) struct SpaceChild {
    #[command(flatten)]
    pub target: Target,
    pub child: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Dms {
    #[command(about = crate::text::t("List joined direct chats."))]
    List {
        #[arg(long)]
        unread: bool,
    },
    #[command(about = crate::text::t("Find or create the direct chat with a user."))]
    Open(UserTarget),
    #[command(about = crate::text::t("Leave a direct chat by room ID."))]
    Leave(Target),
    #[command(about = crate::text::t("Find or create your private note to self."))]
    NoteToSelf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Invites {
    #[command(about = crate::text::t("List pending invitations."))]
    List,
    #[command(about = crate::text::t("Show an invitation."))]
    Show(Target),
    #[command(about = crate::text::t("Accept an invitation."))]
    Accept(Target),
    #[command(about = crate::text::t("Decline an invitation."))]
    Decline(Target),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Messages {
    #[command(about = crate::text::t("Read a page of history, newest first."))]
    List {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        thread: Option<String>,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u16).range(1..=100))]
        limit: u16,
        #[arg(long)]
        cursor: Option<String>,
    },
    #[command(about = crate::text::t("Read one message by event ID."))]
    Show(EventTarget),
    #[command(about = crate::text::t("Search the session's locally available message history."))]
    Search {
        #[command(flatten)]
        query: Query,
        #[command(flatten)]
        filter: RoomFilter,
    },
    #[command(about = crate::text::t("Stream new messages until interrupted."))]
    Watch(TimelineTarget),
    #[command(about = crate::text::t("Send a message and wait for its server event ID."))]
    Send(Send),
    #[command(about = crate::text::t("Edit one of your messages."))]
    Edit {
        #[command(flatten)]
        event: EventTarget,
        #[command(flatten)]
        body: Body,
    },
    #[command(about = crate::text::t("Redact a message."))]
    Redact(EventTarget),
    #[command(about = crate::text::t("Forward a message into another joined room."))]
    Forward {
        #[command(flatten)]
        event: EventTarget,
        destination: String,
    },
    #[command(about = crate::text::t("Add a reaction."))]
    React(Reaction),
    #[command(about = crate::text::t("Remove your reaction."))]
    Unreact(Reaction),
    #[command(about = crate::text::t("Show a message's Matrix event source."))]
    Source(EventTarget),
}

#[derive(Args, Debug)]
pub(crate) struct Body {
    #[arg(required_unless_present = "stdin", conflicts_with = "stdin")]
    pub text: Option<String>,
    #[arg(help = crate::text::t("Read the message text from stdin."))]
    #[arg(long)]
    pub stdin: bool,
    #[arg(help = crate::text::t("Render the text as Markdown."))]
    #[arg(long)]
    pub markdown: bool,
}

#[derive(Args, Debug)]
pub(crate) struct TimelineTarget {
    #[command(flatten)]
    pub target: Target,
    #[arg(help = crate::text::t("Thread root event ID."))]
    #[arg(long)]
    pub thread: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct Send {
    #[command(flatten)]
    pub timeline: TimelineTarget,
    #[command(flatten)]
    pub body: Body,
    #[arg(help = crate::text::t("Event to reply to."))]
    #[arg(long)]
    pub reply_to: Option<String>,
    #[arg(help = crate::text::t("Mention a user. May be repeated."))]
    #[arg(long)]
    pub mention: Vec<String>,
    #[arg(help = crate::text::t("Notify the room when permitted."))]
    #[arg(long)]
    pub mention_room: bool,
    #[arg(help = crate::text::t("Reuse the same ID when retrying the same message."))]
    #[arg(long)]
    pub txn_id: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct Reaction {
    #[command(flatten)]
    pub event: EventTarget,
    pub emoji: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RoomList {
    #[command(about = crate::text::t("List this room's entries."))]
    List(Target),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Pins {
    #[command(about = crate::text::t("List pinned messages."))]
    List(Target),
    #[command(about = crate::text::t("Pin a message for everyone in the room."))]
    Add(EventTarget),
    #[command(about = crate::text::t("Unpin a message."))]
    Remove(EventTarget),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Saved {
    #[command(about = crate::text::t("List privately saved messages."))]
    List(RoomFilter),
    #[command(about = crate::text::t("Save a message privately."))]
    Add(EventTarget),
    #[command(about = crate::text::t("Remove a saved message."))]
    Remove(EventTarget),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Files {
    #[command(about = crate::text::t("List files shared in a room."))]
    List(Target),
    #[command(about = crate::text::t("Upload a file and wait for its server event ID."))]
    Upload {
        #[command(flatten)]
        timeline: TimelineTarget,
        file: PathBuf,
        #[arg(long)]
        caption: Option<String>,
        #[arg(long)]
        txn_id: Option<String>,
    },
    #[command(about = crate::text::t("Download the attachment from a message."))]
    Download {
        #[command(flatten)]
        event: EventTarget,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct Mentions {
    #[command(flatten)]
    pub filter: RoomFilter,
    #[arg(help = crate::text::t("Earliest time to include, as an RFC 3339 timestamp."))]
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=100))]
    pub limit: u16,
    #[command(subcommand)]
    pub action: Option<MentionAction>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MentionAction {
    #[command(about = crate::text::t("Show synced unread mention totals."))]
    Count(RoomFilter),
    #[command(about = crate::text::t("Stream newly highlighted messages until interrupted."))]
    Watch(RoomFilter),
}

#[derive(Debug, Subcommand)]
pub(crate) enum Notifications {
    #[command(about = crate::text::t("Show account defaults or a room's effective mode."))]
    Show { room: Option<String> },
    #[command(about = crate::text::t("Set the account's default mode."))]
    Default {
        #[arg(value_enum)]
        mode: DefaultMode,
    },
    #[command(about = crate::text::t("Set or clear a room's notification override."))]
    Room {
        #[command(flatten)]
        target: Target,
        #[arg(value_enum)]
        mode: Mode,
    },
    #[command(about = crate::text::t("Manage words that trigger notifications."))]
    #[command(subcommand)]
    Keywords(Keywords),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum DefaultMode {
    All,
    Mentions,
    Mute,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Mode {
    All,
    Mentions,
    Mute,
    Default,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Keywords {
    #[command(about = crate::text::t("List notification keywords."))]
    List,
    #[command(about = crate::text::t("Add a notification keyword."))]
    Add { keyword: String },
    #[command(about = crate::text::t("Remove a notification keyword."))]
    Remove { keyword: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum People {
    #[command(about = crate::text::t("Search the homeserver's user directory."))]
    Search(Query),
    #[command(about = crate::text::t("Show a Matrix user's profile."))]
    Show(UserTarget),
    #[command(about = crate::text::t("Ignore a user across your sessions."))]
    Ignore(UserTarget),
    #[command(about = crate::text::t("Stop ignoring a user."))]
    Unignore(UserTarget),
    #[command(about = crate::text::t("List ignored users."))]
    Ignored,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Profile {
    #[command(about = crate::text::t("Show the selected account's profile."))]
    Show,
    #[command(about = crate::text::t("Set your display name."))]
    Name { name: String },
    #[command(about = crate::text::t("Set your avatar from an image file."))]
    Avatar { file: PathBuf },
    #[command(about = crate::text::t("Set your presence status."))]
    Status {
        #[arg(value_enum)]
        status: Status,
    },
    #[command(about = crate::text::t("Set or clear your status message."))]
    StatusMessage {
        #[arg(conflicts_with = "clear", required_unless_present = "clear")]
        text: Option<String>,
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Status {
    Online,
    Away,
    Hidden,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Report {
    #[command(about = crate::text::t("Report a message."))]
    Message {
        #[command(flatten)]
        event: EventTarget,
        #[arg(long)]
        reason: Option<String>,
    },
    #[command(about = crate::text::t("Report a room."))]
    Room {
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        reason: Option<String>,
    },
    #[command(about = crate::text::t("Report a Matrix user."))]
    User {
        #[command(flatten)]
        user: UserTarget,
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Shell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Skill {
    #[command(about = crate::text::t("Install the skill and its command references offline."))]
    Install {
        #[arg(help = crate::text::t("Use the current directory or your user's home directory."))]
        #[arg(long, value_enum)]
        scope: Option<Scope>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Scope {
    Local,
    Global,
}
