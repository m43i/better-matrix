use crate::text::t;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use matrix_core::engine::ErrorKind;
use matrix_core::terminal::Action;
use matrix_core::{
    AccountId, AccountSummary, ClientIdentity, Command, Engine, EngineConfig, EngineHandle, Event, FileSecrets,
    LinkTarget, OAuthRedirect, Permalink, RequestId, RoomList, StoreSharing, TimelineKey,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout_at};

use crate::terminal::{self, Terminal};
use crate::{CliError, Result};

#[derive(Debug, Default)]
struct Accounts {
    saved: HashSet<AccountId>,
    active: HashMap<AccountId, AccountSummary>,
    suspended: HashSet<AccountId>,
    ended: HashSet<AccountId>,
    errors: HashMap<AccountId, String>,
    synced: HashSet<AccountId>,
    rooms: HashMap<AccountId, RoomList>,
}

pub(crate) struct Client {
    engine: EngineHandle,
    receiver: mpsc::UnboundedReceiver<Event>,
    accounts: Accounts,
    deadline: Instant,
    pub terminal: Terminal,
    pub data_dir: PathBuf,
    default: Option<String>,
}

impl Client {
    pub fn start(
        data_dir: PathBuf,
        default: Option<String>,
        terminal: Terminal,
        duration: Duration,
        watch: bool,
    ) -> Result<Self> {
        let (tx, receiver) = mpsc::unbounded_channel();
        let engine = Engine::start_terminal(
            EngineConfig {
                secrets: Arc::new(FileSecrets::new(data_dir.join("secrets"))),
                data_dir: data_dir.clone(),
                identity: ClientIdentity {
                    name: "Better Matrix CLI".into(),
                    uri: url::Url::parse("https://github.com/m43i/better-matrix")
                        .map_err(|error| CliError::Input(error.to_string()))?,
                },
                oauth_redirect: OAuthRedirect::Loopback,
                stores: StoreSharing::Alone,
                scans_qr: false,
            },
            move |event: Event| {
                let _ = tx.send(event);
            },
            watch,
        )?;
        Ok(Self {
            engine,
            receiver,
            deadline: Instant::now() + duration,
            terminal,
            data_dir,
            default,
            accounts: Accounts::default(),
        })
    }

    pub async fn shutdown(self) {
        let stopped = self.engine.stopped();
        drop(self);
        let _ = tokio::task::spawn_blocking(move || stopped.wait(Duration::from_secs(4))).await;
    }

    pub async fn restore(&mut self) -> Result<()> {
        loop {
            if matches!(self.next().await?, Event::Restoring(_)) {
                break;
            }
        }
        while self.accounts.saved.iter().any(|id| {
            !self.accounts.active.contains_key(id)
                && !self.accounts.errors.contains_key(id)
                && !self.accounts.suspended.contains(id)
                && !self.accounts.ended.contains(id)
        }) {
            self.next().await?;
        }
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Event> {
        let event = timeout_at(self.deadline, self.receiver.recv())
            .await
            .map_err(|_| CliError::Timeout)?
            .ok_or(CliError::EngineStopped)?;
        self.observe(&event);
        Ok(event)
    }

    fn observe(&mut self, event: &Event) {
        let accounts = &mut self.accounts;
        match event {
            Event::Restoring(ids) => accounts.saved.extend(ids.iter().cloned()),
            Event::AccountAdded(account) => {
                accounts.saved.insert(account.id.clone());
                accounts.suspended.remove(&account.id);
                accounts.ended.remove(&account.id);
                accounts.errors.remove(&account.id);
                accounts.active.insert(account.id.clone(), account.clone());
            }
            Event::AccountChanged(account) => {
                if let Some(active) = accounts.active.get_mut(&account.id) {
                    *active = account.clone();
                }
            }
            Event::AccountRemoved(id) => {
                accounts.saved.remove(id);
                accounts.active.remove(id);
                accounts.suspended.remove(id);
                accounts.ended.remove(id);
                accounts.synced.remove(id);
                accounts.rooms.remove(id);
            }
            Event::SessionEnded { account, kept } => {
                accounts.active.remove(account);
                accounts.synced.remove(account);
                accounts.ended.insert(account.clone());
                if *kept {
                    accounts.suspended.insert(account.clone());
                }
            }
            Event::AccountError { account, error } => {
                accounts.errors.insert(account.clone(), error.clone());
            }
            Event::RoomsUpdated { account, list } => {
                accounts.rooms.insert(account.clone(), list.clone());
            }
            Event::SyncReady(account) => {
                accounts.synced.insert(account.clone());
            }
            _ => {}
        }
    }

    pub fn selected(&self, account: Option<&str>) -> Result<AccountId> {
        if let Some(account) = account.or(self.default.as_deref()) {
            let account = AccountId::new(account);
            if let Some(error) = self.accounts.errors.get(&account) {
                return Err(CliError::Command(error.clone()));
            }
            return if self.accounts.active.contains_key(&account) {
                Ok(account)
            } else {
                Err(CliError::Authentication(tf!("{account} needs to sign in. Run bm auth login.", account = account)))
            };
        }
        match self.accounts.active.keys().collect::<Vec<_>>().as_slice() {
            [only] => Ok((*only).clone()),
            [] => Err(CliError::Authentication(t("sign in with bm auth login").into())),
            _ => Err(CliError::Input(t("select an account with --account or bm auth use").into())),
        }
    }

    pub fn saved_account(&self, account: Option<&str>) -> Result<AccountId> {
        let Some(account) = account.or(self.default.as_deref()).map(AccountId::new) else {
            return self.selected(None);
        };
        if self.accounts.saved.contains(&account) { Ok(account) } else { self.selected(Some(account.as_str())) }
    }

    pub fn summary(&self, account: &AccountId) -> Result<&AccountSummary> {
        self.accounts.active.get(account).ok_or_else(|| {
            CliError::Authentication(tf!("{account} needs to sign in. Run bm auth login.", account = account))
        })
    }

    pub fn saved_accounts(&self) -> Value {
        let mut ids: Vec<_> = self.accounts.saved.union(&self.accounts.suspended).collect();
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        json!(
            ids.iter()
                .map(|id| json!({
                    "id": id,
                    "display_name": self.accounts.active.get(*id).map(|account| &account.display_name),
                    "default": self.default.as_deref() == Some(id.as_str()),
                    "signed_in": self.accounts.active.contains_key(*id),
                    "suspended": self.accounts.suspended.contains(*id),
                    "error": self.accounts.errors.get(*id),
                }))
                .collect::<Vec<_>>()
        )
    }

    pub fn has_session(&self, account: &AccountId) -> bool {
        self.accounts.saved.contains(account)
    }

    pub async fn session_ended(&mut self, account: &AccountId) -> Result<()> {
        while self.accounts.active.contains_key(account) {
            self.next().await?;
        }
        Ok(())
    }

    pub fn is_suspended(&self, id: &AccountId) -> bool {
        self.accounts.suspended.contains(id)
    }

    pub async fn set_default(&mut self, account: &AccountId) -> Result<()> {
        let path = self.data_dir.clone();
        let id = account.to_string();
        tokio::task::spawn_blocking(move || terminal::set_default(&path, &id)).await??;
        self.default = Some(account.to_string());
        Ok(())
    }

    pub fn has_default(&self) -> bool {
        self.default.is_some()
    }

    pub async fn remove_default_if(&mut self, account: &AccountId) -> Result<()> {
        if self.default.as_deref() == Some(account.as_str()) {
            let path = self.data_dir.clone();
            tokio::task::spawn_blocking(move || terminal::clear_default(&path)).await??;
            self.default = None;
        }
        Ok(())
    }

    pub async fn synced(&mut self, account: &AccountId) -> Result<()> {
        while !self.accounts.synced.contains(account) {
            self.summary(account)?;
            self.next().await?;
        }
        self.summary(account)?;
        Ok(())
    }

    pub fn rooms(&self, account: &AccountId) -> RoomList {
        self.accounts.rooms.get(account).cloned().unwrap_or_default()
    }

    pub fn send(&self, command: Command) -> RequestId {
        self.engine.send(command)
    }

    pub async fn change(&mut self, command: Command) -> Result<Value> {
        let result = self
            .collect(command, |id, event| match event {
                Event::OpenUrl { request, url } if *request == id => Some(json!({ "url": url })),
                _ => None,
            })
            .await?;
        Ok(result.unwrap_or_else(|| json!({ "ok": true })))
    }

    pub async fn terminal_action(&mut self, account: &AccountId, action: Action) -> Result<Value> {
        self.run(engine_action(account, action), |id, event| match event {
            Event::TerminalResult(request, result) if *request == id => Some(result.clone()),
            _ => None,
        })
        .await
    }

    pub async fn run<T>(&mut self, command: Command, output: impl FnMut(RequestId, &Event) -> Option<T>) -> Result<T> {
        self.collect(command, output).await?.ok_or(CliError::NoResult)
    }

    async fn collect<T>(
        &mut self,
        command: Command,
        mut output: impl FnMut(RequestId, &Event) -> Option<T>,
    ) -> Result<Option<T>> {
        let request = self.send(command);
        let mut category = ErrorKind::Other;
        let mut value = None;
        loop {
            let event = match self.next().await {
                Ok(event) => event,
                Err(error) => {
                    self.send(Command::Cancel(request));
                    return Err(error);
                }
            };
            match &event {
                Event::Finished { request: finished, result } if *finished == request => {
                    result.clone().map_err(|message| command_error(category, message))?;
                    return Ok(value);
                }
                Event::CommandError(failed, kind) if *failed == request => category = *kind,
                Event::OpenUrl { request: opened, url } if *opened == request => {
                    eprintln!("{}", terminal::safe(url));
                }
                Event::PasswordNeeded { request: asked } if *asked == request => category = ErrorKind::Authentication,
                _ => {}
            }
            if let Some(picked) = output(request, &event) {
                value = Some(picked);
            }
        }
    }

    pub async fn watch(&mut self, account: &AccountId, key: Option<TimelineKey>, highlights: bool) -> Result<()> {
        let request = self.send(engine_action(account, Action::Watch { key, highlights }));
        let mut category = ErrorKind::Other;
        let interrupted = tokio::signal::ctrl_c();
        tokio::pin!(interrupted);
        loop {
            tokio::select! {
                result = &mut interrupted => {
                    self.send(Command::Cancel(request));
                    return Ok(result?);
                }
                event = self.receiver.recv() => {
                    let event = event.ok_or(CliError::EngineStopped)?;
                    match event {
                        Event::TerminalResult(id, value) if id == request => self.terminal.output(&value)?,
                        Event::SessionEnded { account: ended, .. } if ended == *account => {
                            return Err(CliError::Authentication(tf!("{account} was signed out", account = account)));
                        }
                        Event::CommandError(id, kind) if id == request => category = kind,
                        Event::Finished { request: id, result } if id == request => {
                            return result.map_err(|message| command_error(category, message));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub async fn resolve(&mut self, account: &AccountId, target: &str) -> Result<String> {
        let (room, _) = room_target(target)?;
        if room.starts_with('#') {
            let resolved = self.terminal_action(account, Action::ResolveAlias(room)).await?;
            return resolved["room_id"].as_str().map(str::to_owned).ok_or(CliError::NoResult);
        }
        Ok(room)
    }

    pub async fn optional_room(&mut self, account: &AccountId, target: Option<&str>) -> Result<Option<String>> {
        match target {
            Some(target) => Ok(Some(self.resolve(account, target).await?)),
            None => Ok(None),
        }
    }
}

pub(crate) fn engine_action(account: &AccountId, action: Action) -> Command {
    Command::Terminal { account: account.clone(), action: Box::new(action) }
}

pub(crate) fn room_target(target: &str) -> Result<(String, Vec<String>)> {
    let Some(Permalink { target: LinkTarget::Room { room, .. }, via, .. }) = Permalink::parse_typed(target) else {
        return Err(CliError::Input(tf!("expected a Matrix room ID, alias, or permalink: {target}", target = target)));
    };
    Ok((room, via))
}

fn command_error(category: ErrorKind, message: String) -> CliError {
    match category {
        ErrorKind::Authentication => CliError::Authentication(message),
        ErrorKind::Permission => CliError::Permission(message),
        ErrorKind::RateLimit => CliError::RateLimit(message),
        ErrorKind::Timeout => CliError::Timeout,
        ErrorKind::Other => CliError::Command(message),
    }
}
