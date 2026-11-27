#[macro_use]
mod text;

use crate::text::t;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use matrix_core::terminal::Action;
use matrix_core::{AccountId, Command, Event, NotificationMode, ReportTarget, SafetyChange};
use serde_json::{Value, json};

mod args;
mod auth;
mod client;
mod messages;
mod rooms;
mod skill;
mod terminal;
#[cfg(test)]
mod tests;

use args::{Cli, DefaultMode, Keywords, Mode, Notifications, People, Profile, Report, RoomList, Shell, Top};
use client::Client;
use terminal::Terminal;

type Result<T> = std::result::Result<T, CliError>;

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    Command(String),
    #[error("{0}")]
    Authentication(String),
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    RateLimit(String),
    #[error("{}", t("the command timed out"))]
    Timeout,
    #[error("{}", t("the Matrix engine stopped"))]
    EngineStopped,
    #[error("{}", t("the Matrix engine did not return the requested result"))]
    NoResult,
    #[error(transparent)]
    Engine(#[from] matrix_core::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

impl CliError {
    fn code(&self) -> u8 {
        match self {
            Self::Input(_) => 2,
            Self::Authentication(_) => 3,
            Self::Permission(_) => 4,
            Self::RateLimit(_) => 5,
            Self::Timeout => 6,
            _ => 1,
        }
    }

    fn broken_pipe(&self) -> bool {
        match self {
            Self::Io(error) => error.kind() == std::io::ErrorKind::BrokenPipe,
            Self::Json(error) => error.io_error_kind() == Some(std::io::ErrorKind::BrokenPipe),
            _ => false,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.broken_pipe() => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bm: {}", terminal::safe(&error.to_string()));
            ExitCode::from(error.code())
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let output = Terminal { json: cli.json, no_input: cli.no_input, yes: cli.yes };
    match cli.command {
        Top::Skill { action } => return output.output(&skill::install(output, action)?),
        Top::Completions { shell } => {
            let shell = match shell {
                Shell::Bash => clap_complete::Shell::Bash,
                Shell::Zsh => clap_complete::Shell::Zsh,
                Shell::Fish => clap_complete::Shell::Fish,
            };
            clap_complete::generate(shell, &mut Cli::command(), "bm", &mut std::io::stdout());
            return Ok(());
        }
        _ => {}
    }
    let path = terminal::data_directory(cli.data_dir.clone())?;
    let _lock = terminal::lock(&path)?;
    let default = terminal::read_default(&path)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(run_matrix(cli, path, default, output))
}

async fn run_matrix(cli: Cli, path: PathBuf, default: Option<String>, output: Terminal) -> Result<()> {
    let timeout = cli.timeout.unwrap_or(match cli.command {
        Top::Auth(args::Auth::Login { .. } | args::Auth::Verify { .. }) => 600,
        _ => 45,
    });
    let watch = matches!(
        cli.command,
        Top::Messages(args::Messages::Watch(_))
            | Top::Mentions(args::Mentions { action: Some(args::MentionAction::Watch(_)), .. })
    );
    let mut client = Client::start(path, default, output, Duration::from_secs(timeout), watch)?;
    let result = async {
        client.restore().await?;
        if let Some(value) = execute(&mut client, cli.account.as_deref(), cli.command).await? {
            output.output(&value)?;
        }
        Ok(())
    }
    .await;
    client.shutdown().await;
    result
}

async fn execute(client: &mut Client, selected: Option<&str>, command: Top) -> Result<Option<Value>> {
    let command = match command {
        Top::Auth(command) => return Ok(Some(auth::run(client, selected, command).await?)),
        Top::Doctor => {
            let session = match client.selected(selected) {
                Ok(account) => client.terminal_action(&account, Action::Session).await?,
                Err(CliError::Authentication(_)) if selected.is_none() => Value::Null,
                Err(error) => return Err(error),
            };
            return Ok(Some(
                json!({ "version": env!("CARGO_PKG_VERSION"), "data_dir": client.data_dir, "session": session }),
            ));
        }
        command => command,
    };
    let account = client.selected(selected)?;
    client.synced(&account).await?;
    let value = match command {
        Top::Rooms(command) => rooms::rooms(client, account, command).await?,
        Top::Spaces(command) => rooms::spaces(client, account, command).await?,
        Top::Dms(command) => rooms::dms(client, account, command).await?,
        Top::Invites(command) => rooms::invites(client, account, command).await?,
        Top::Messages(command) => return messages::messages(client, account, command).await,
        Top::Mentions(command) => return messages::mentions(client, account, command).await,
        Top::Pins(command) => messages::pins(client, account, command).await?,
        Top::Saved(command) => messages::saved(client, account, command).await?,
        Top::Files(command) => messages::files(client, account, command).await?,
        Top::Threads(RoomList::List(room)) => {
            let room_id = client.resolve(&account, &room.room).await?;
            client
                .run(
                    Command::ListThreads { account: account.clone(), room_id: room_id.clone() },
                    |_, event| match event {
                        Event::Threads { account: owner, room_id: room, threads }
                            if *owner == account && *room == room_id =>
                        {
                            Some(json!(threads))
                        }
                        _ => None,
                    },
                )
                .await?
        }
        Top::Notifications(command) => notifications(client, account, command).await?,
        Top::People(command) => people(client, account, command).await?,
        Top::Profile(command) => profile(client, account, command).await?,
        Top::Report(command) => report(client, account, command).await?,
        Top::Sync => json!({ "account": account, "synced": true }),
        Top::Auth(_) | Top::Doctor | Top::Skill { .. } | Top::Completions { .. } => return Err(CliError::NoResult),
    };
    Ok(Some(value))
}

async fn notifications(client: &mut Client, account: AccountId, command: Notifications) -> Result<Value> {
    let command = match command {
        Notifications::Show { room: Some(room) } => {
            let room = client.resolve(&account, &room).await?;
            let list = client.rooms(&account);
            let room = list
                .rooms
                .iter()
                .find(|joined| joined.room_id == room)
                .ok_or_else(|| CliError::Input(tf!("{room} is not a joined room", room = room)))?;
            return Ok(json!({
                "room_id": room.room_id,
                "mode": room.notifications,
                "override": room.custom_notifications,
            }));
        }
        Notifications::Show { room: None } => {
            return client.terminal_action(&account, Action::NotificationDefaults).await;
        }
        Notifications::Default { mode } => {
            let mode = match mode {
                DefaultMode::All => NotificationMode::AllMessages,
                DefaultMode::Mentions => NotificationMode::MentionsOnly,
                DefaultMode::Mute => NotificationMode::Mute,
            };
            Command::SetDefaultNotificationMode { account, mode }
        }
        Notifications::Room { target, mode } => {
            let room_id = client.resolve(&account, &target.room).await?;
            let mode = match mode {
                Mode::All => Some(NotificationMode::AllMessages),
                Mode::Mentions => Some(NotificationMode::MentionsOnly),
                Mode::Mute => Some(NotificationMode::Mute),
                Mode::Default => None,
            };
            Command::SetNotificationMode { account, room_id, mode }
        }
        Notifications::Keywords(Keywords::List) => return client.terminal_action(&account, Action::Keywords).await,
        Notifications::Keywords(keywords) => {
            let enabled = matches!(keywords, Keywords::Add { .. });
            let keyword = match keywords {
                Keywords::Add { keyword } | Keywords::Remove { keyword } => keyword,
                Keywords::List => return Err(CliError::NoResult),
            };
            Command::SetKeyword { account, keyword, enabled }
        }
    };
    client.change(command).await
}

async fn people(client: &mut Client, account: AccountId, command: People) -> Result<Value> {
    let ignored = matches!(command, People::Ignore(_));
    match command {
        People::Search(query) => {
            client
                .run(Command::SearchPeople { account, term: query.query }, |id, event| match event {
                    Event::PeopleFound { request, people } if *request == id => Some(json!(people)),
                    _ => None,
                })
                .await
        }
        People::Show(user) => client.terminal_action(&account, Action::Person(user.user_id)).await,
        People::Ignored => client.terminal_action(&account, Action::Ignored).await,
        People::Ignore(user) | People::Unignore(user) => {
            client
                .change(Command::SetSafety { account, change: SafetyChange::Ignore { user_id: user.user_id, ignored } })
                .await
        }
    }
}

async fn profile(client: &mut Client, account: AccountId, command: Profile) -> Result<Value> {
    let command = match command {
        Profile::Show => return client.terminal_action(&account, Action::Person(account.to_string())).await,
        Profile::Name { name } => Command::SetDisplayName { account, name },
        Profile::Avatar { file } => Command::SetAvatar { account, file: Some(file) },
        Profile::Status { status } => {
            let status = match status {
                args::Status::Online => matrix_core::Status::Online,
                args::Status::Away => matrix_core::Status::Away,
                args::Status::Hidden => matrix_core::Status::Hidden,
            };
            Command::SetStatus { account, status }
        }
        Profile::StatusMessage { text, .. } => Command::SetStatusMessage { account, message: text, clear_at_ms: None },
    };
    client.change(command).await
}

async fn report(client: &mut Client, account: AccountId, command: Report) -> Result<Value> {
    let (target, reason) = match command {
        Report::Message { event, reason } => {
            let (room_id, event_id) = messages::target(client, &account, event).await?;
            (ReportTarget::Message { room_id, event_id }, reason)
        }
        Report::Room { target, reason } => {
            let room_id = client.resolve(&account, &target.room).await?;
            (ReportTarget::Room { room_id }, reason)
        }
        Report::User { user, reason } => (ReportTarget::Person { user_id: user.user_id }, reason),
    };
    client.change(Command::Report { account, target, reason: reason.unwrap_or_default() }).await
}
