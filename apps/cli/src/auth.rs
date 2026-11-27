use crate::text::t;
use matrix_core::terminal::Action;
use matrix_core::{AccountId, Command, Event, Homeserver, LoginRequest, Password, SignInInput, VerificationStep};
use serde_json::{Value, json};

use crate::args::{Auth, Keys, LoginMethod, Recovery, Sessions};
use crate::client::Client;
use crate::{CliError, Result};

pub(crate) async fn run(client: &mut Client, account: Option<&str>, command: Auth) -> Result<Value> {
    let command = match command {
        Auth::Discover { server } => return Ok(json!(discover(client, server).await?)),
        Auth::Login { user_id, server, method, password_stdin } => {
            return login(client, user_id, server, method, password_stdin).await;
        }
        Auth::List => return Ok(client.saved_accounts()),
        Auth::Use(user) => {
            let account = client.selected(Some(&user.user_id))?;
            client.set_default(&account).await?;
            return Ok(json!({ "account": account }));
        }
        command => command,
    };

    let account =
        if matches!(command, Auth::Logout) { client.saved_account(account)? } else { client.selected(account)? };
    match command {
        Auth::Whoami => client.terminal_action(&account, Action::Session).await,
        Auth::Logout => {
            let terminal = client.terminal;
            tokio::task::spawn_blocking(move || terminal.confirm(t("Sign out and delete this device's local keys?")))
                .await??;
            let result = client.change(Command::Logout(account.clone())).await?;
            client.remove_default_if(&account).await?;
            Ok(result)
        }
        Auth::Sessions(command) => sessions(client, account, command).await,
        Auth::Verify { user_id } => verify(client, account, user_id).await,
        Auth::Recovery(Recovery::Status) => {
            client.synced(&account).await?;
            let mut value = client.terminal_action(&account, Action::Session).await?;
            Ok(value["security"].take())
        }
        Auth::Recovery(Recovery::Restore { key_stdin }) => {
            let terminal = client.terminal;
            let key = tokio::task::spawn_blocking(move || terminal.secret(t("Recovery key: "), key_stdin)).await??;
            client.change(Command::Recover { account, key: Password(key) }).await
        }
        Auth::Keys(command) => {
            let (file, import) = match command {
                Keys::Import(file) => (file, true),
                Keys::Export(file) => (file, false),
            };
            let terminal = client.terminal;
            let stdin = file.passphrase_stdin;
            let passphrase = Password(
                tokio::task::spawn_blocking(move || terminal.secret(t("Key file passphrase: "), stdin)).await??,
            );
            if import {
                client
                    .run(Command::ImportRoomKeys { account, path: file.file, passphrase }, |id, event| match event {
                        Event::KeysImported { request, import } if *request == id => Some(json!(import)),
                        _ => None,
                    })
                    .await
            } else {
                let destination = file.file;
                let export = destination.clone();
                let temp = tokio::task::spawn_blocking(move || crate::terminal::temporary_output(&export)).await??;
                client.change(Command::ExportRoomKeys { account, path: temp.path().to_owned(), passphrase }).await?;
                tokio::task::spawn_blocking(move || temp.persist_noclobber(destination).map_err(|error| error.error))
                    .await??;
                Ok(json!({ "ok": true }))
            }
        }
        Auth::Discover { .. } | Auth::Login { .. } | Auth::List | Auth::Use(_) => Err(CliError::NoResult),
    }
}

async fn discover(client: &mut Client, server: String) -> Result<Homeserver> {
    let server = SignInInput::parse(&server)
        .ok_or_else(|| CliError::Input(t("enter a server name or Matrix ID").into()))?
        .server;
    client
        .run(Command::Discover { server }, |id, event| match event {
            Event::HomeserverDiscovered { request, homeserver } if *request == id => Some(homeserver.clone()),
            _ => None,
        })
        .await
}

async fn login(
    client: &mut Client,
    user_id: String,
    server: Option<String>,
    method: Option<LoginMethod>,
    password_stdin: bool,
) -> Result<Value> {
    let account = AccountId::new(&user_id);
    let input = SignInInput::parse(&user_id)
        .filter(|input| input.username.is_some())
        .ok_or_else(|| CliError::Input(t("use a full Matrix user ID, such as @alice:example.org").into()))?;
    if client.summary(&account).is_ok() {
        match client.terminal_action(&account, Action::ValidateSession).await {
            Ok(_) => {
                return Err(CliError::Input(tf!(
                    "{account} already has a CLI session in this data directory",
                    account = account
                )));
            }
            Err(CliError::Authentication(_)) => client.session_ended(&account).await?,
            Err(error) => return Err(error),
        }
    }
    let suspended = client.is_suspended(&account);
    if !suspended && client.has_session(&account) {
        return Err(CliError::Authentication(tf!(
            "{account} has a saved session. Use bm auth logout to remove it before signing in.",
            account = account
        )));
    }
    let homeserver = discover(client, server.unwrap_or(input.server)).await?;
    if suspended {
        // Close the ended client before opening the same SQLite stores again.
        client.change(Command::Suspend(account.clone())).await?;
    }
    let resume = suspended.then(|| account.clone());
    let method = method.unwrap_or(if homeserver.methods.oauth && !password_stdin {
        LoginMethod::Oauth
    } else {
        LoginMethod::Password
    });
    let command = match method {
        LoginMethod::Password if homeserver.methods.password => {
            let terminal = client.terminal;
            let password =
                tokio::task::spawn_blocking(move || terminal.secret(t("Matrix password: "), password_stdin)).await??;
            Command::Login(LoginRequest { homeserver: homeserver.url, username: user_id, password, resume })
        }
        LoginMethod::Oauth if homeserver.methods.oauth && !password_stdin => {
            eprintln!(
                "{}",
                t("Open the sign-in URL in a browser. Over SSH, forward the loopback redirect port shown in the URL.")
            );
            Command::LoginOAuthAs { homeserver: homeserver.url, account, resume: suspended }
        }
        _ => return Err(CliError::Authentication(t("the requested sign-in method is unavailable").into())),
    };
    let signed_in = client
        .run(command, |id, event| match event {
            Event::SignedIn { request, account } if *request == id => Some(account.clone()),
            _ => None,
        })
        .await
        .map_err(|error| match error {
            CliError::Permission(message) => CliError::Authentication(message),
            error => error,
        })?;
    if !client.has_default() {
        client.set_default(&signed_in).await?;
    }
    client.terminal_action(&signed_in, Action::Session).await
}

async fn sessions(client: &mut Client, account: AccountId, command: Sessions) -> Result<Value> {
    match command {
        Sessions::List => {
            client
                .run(Command::ListSessions(account.clone()), |_, event| match event {
                    Event::Sessions { account: owner, sessions } if *owner == account => Some(json!(sessions)),
                    _ => None,
                })
                .await
        }
        Sessions::Rename { device_id, name } => {
            client.change(Command::RenameSession { account, device_id, name }).await
        }
        Sessions::End { device_id } => {
            let terminal = client.terminal;
            tokio::task::spawn_blocking(move || terminal.confirm(t("End the selected session?"))).await??;
            let command =
                Command::EndSession { account: account.clone(), device_id: device_id.clone(), password: None };
            match client.change(command).await {
                Err(CliError::Authentication(_)) if client.terminal.interactive() => {
                    let terminal = client.terminal;
                    let password =
                        tokio::task::spawn_blocking(move || terminal.secret(t("Matrix password: "), false)).await??;
                    client.change(Command::EndSession { account, device_id, password: Some(Password(password)) }).await
                }
                result => result,
            }
        }
    }
}

async fn verify(client: &mut Client, account: AccountId, user_id: Option<String>) -> Result<Value> {
    if !client.terminal.interactive() {
        return Err(CliError::Input(t("emoji verification requires an interactive terminal").into()));
    }
    client.synced(&account).await?;
    let start = match user_id {
        Some(user_id) => Command::VerifyUser { account: account.clone(), user_id },
        None => Command::VerifyThisSession(account.clone()),
    };
    let request = client.send(start);
    let mut flow = None;
    loop {
        match client.next().await {
            Ok(Event::Finished { request: id, result: Err(error) }) if id == request => {
                return Err(CliError::Command(error));
            }
            Ok(Event::VerificationRequested { request: id, flow_id, .. }) if id == request => {
                flow = Some(flow_id);
                eprintln!("{}", t("Accept the verification on the other device."));
            }
            Ok(Event::Verification { account: owner, flow_id, step })
                if owner == account && flow.as_deref() == Some(&flow_id) =>
            {
                match step {
                    VerificationStep::Ready { .. } => {
                        // Keep consuming events while the emoji negotiation starts.
                        client.send(Command::CompareEmoji { account: account.clone(), flow_id });
                    }
                    VerificationStep::Compare(emoji) => {
                        let emoji = emoji
                            .iter()
                            .map(|emoji| format!("{} {}", emoji.symbol, emoji.description))
                            .collect::<Vec<_>>()
                            .join("  ");
                        eprintln!("{emoji}");
                        let terminal = client.terminal;
                        let matches = tokio::task::spawn_blocking(move || terminal.ask(t("Do the emoji match? [y/N]")))
                            .await??
                            .eq_ignore_ascii_case("y");
                        client.send(Command::ConfirmVerification { account: account.clone(), flow_id, matches });
                    }
                    VerificationStep::Done => return Ok(json!({ "verified": true })),
                    VerificationStep::Cancelled(reason) => return Err(CliError::Command(reason)),
                    _ => {}
                }
            }
            Err(error) => {
                if let Some(flow_id) = flow {
                    client.send(Command::CancelVerification { account, flow_id });
                }
                return Err(error);
            }
            _ => {}
        }
    }
}
