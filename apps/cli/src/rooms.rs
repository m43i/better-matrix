use crate::text::t;
use std::collections::HashSet;

use clap::ValueEnum;
use matrix_core::terminal::{Action, Membership};
use matrix_core::{
    AccountId, Command, DirectoryKind, Event, HistoryVisibility, Moderation, NewRoom, RoomKind, RoomPriority,
    RoomSetting, RoomSummary, SpaceChange,
};
use serde_json::{Value, json};

use crate::args::{
    self, Aliases, Children, CreationOptions, Dms, Invites, Join, Rooms, Setting, Settings, Spaces, Target,
};
use crate::client::{Client, room_target};
use crate::{CliError, Result};

pub(crate) async fn rooms(client: &mut Client, account: AccountId, command: Rooms) -> Result<Value> {
    let list = client.rooms(&account);
    match command {
        Rooms::List { unread, space } => {
            let rooms = if let Some(space) = space {
                let space = client.resolve(&account, &space).await?;
                let ids = space_rooms(&space, &list.spaces)?;
                room_rows(list.rooms.into_iter().filter(|room| ids.contains(&room.room_id)), RoomKind::Group, unread)
            } else {
                room_rows(list.rooms, RoomKind::Group, unread)
            };
            Ok(rooms)
        }
        Rooms::Show(target) => {
            let (room, via) = room_target(&target.room)?;
            // For an alias or a remote room, the preview API can honor the via servers.
            if let Some(joined) = list.rooms.iter().find(|joined| joined.room_id == room) {
                return Ok(json!(joined));
            }
            client
                .run(Command::PreviewRoom { account, room, via }, |id, event| match event {
                    Event::RoomPreview { request, preview } if *request == id => Some(json!(preview)),
                    _ => None,
                })
                .await
        }
        Rooms::Discover(query) => discover(client, account, query.query, DirectoryKind::Rooms).await,
        Rooms::Create(args) => create(client, account, args.create, args.space, false, !args.unencrypted).await,
        Rooms::Join(args) => join(client, account, args).await,
        Rooms::Knock { target, reason } => {
            let (room, via) = room_target(&target.room)?;
            client.terminal_action(&account, Action::Knock { room, via, reason }).await
        }
        Rooms::Leave(target) => leave(client, account, target).await,
        Rooms::Forget(target) => {
            let terminal = client.terminal;
            tokio::task::spawn_blocking(move || terminal.confirm(t("Forget this room?"))).await??;
            let room = client.resolve(&account, &target.room).await?;
            client.terminal_action(&account, Action::Forget(room)).await
        }
        Rooms::Members { target, membership } => {
            let room = client.resolve(&account, &target.room).await?;
            let membership = match membership {
                args::Membership::Joined => Membership::Joined,
                args::Membership::Invited => Membership::Invited,
                args::Membership::Banned => Membership::Banned,
                args::Membership::Knocking => Membership::Knocking,
            };
            client.terminal_action(&account, Action::Members(room, membership)).await
        }
        Rooms::Invite(member) => {
            let room_id = client.resolve(&account, &member.target.room).await?;
            client.change(Command::Invite { account, room_id, user_id: member.user_id }).await
        }
        Rooms::Kick(member) => moderate(client, account, member, Moderation::Kick).await,
        Rooms::Ban(member) => moderate(client, account, member, Moderation::Ban).await,
        Rooms::Unban(member) => moderate(client, account, member, Moderation::Unban).await,
        Rooms::Approve(member) => moderate(client, account, member, Moderation::Admit).await,
        Rooms::Deny(member) => moderate(client, account, member, Moderation::Deny).await,
        Rooms::PowerLevel { member, level } => moderate(client, account, member, Moderation::Level(level)).await,
        Rooms::MarkRead { target, through } => {
            let room_id = client.resolve(&account, &target.room).await?;
            match through {
                Some(event) => client.terminal_action(&account, Action::ReadThrough(room_id, event)).await,
                None => client.change(Command::MarkRoomRead { account, room_id }).await,
            }
        }
        Rooms::MarkUnread { target, clear } => {
            let room_id = client.resolve(&account, &target.room).await?;
            client.change(Command::SetUnread { account, room_id, unread: !clear }).await
        }
        Rooms::Priority { target, priority } => {
            let room_id = client.resolve(&account, &target.room).await?;
            let priority = match priority {
                args::Priority::Favourite => RoomPriority::Favourite,
                args::Priority::Low => RoomPriority::Low,
                args::Priority::Normal => RoomPriority::Normal,
            };
            client.change(Command::SetRoomPriority { account, room_id, priority }).await
        }
        Rooms::Settings(settings) => room_settings(client, account, settings).await,
        Rooms::Upgrade { target, version } => {
            let room_id = client.resolve(&account, &target.room).await?;
            let terminal = client.terminal;
            tokio::task::spawn_blocking(move || terminal.confirm(t("Replace this room with an upgraded room?")))
                .await??;
            client.run(Command::UpgradeRoom { account, room_id, version }, room_result).await
        }
    }
}

async fn room_settings(client: &mut Client, account: AccountId, command: Settings) -> Result<Value> {
    let (target, setting) = match command {
        Settings::Show(target) => {
            let room_id = client.resolve(&account, &target.room).await?;
            return client
                .run(Command::LoadRoomSettings { account: account.clone(), room_id: room_id.clone() }, |_, event| {
                    match event {
                        Event::RoomSettings { account: id, settings }
                            if *id == account && settings.room_id == room_id =>
                        {
                            Some(json!(settings))
                        }
                        _ => None,
                    }
                })
                .await;
        }
        Settings::Set { target, setting, value } => (target, setting_value(setting, value)?),
        Settings::Alias(action) => match action {
            Aliases::Add(args) => (args.target, RoomSetting::AddAddress(args.alias)),
            Aliases::Remove(args) => (args.target, RoomSetting::RemoveAddress(args.alias)),
            Aliases::Primary(args) => (args.target, RoomSetting::MainAddress(args.alias)),
        },
        Settings::Permission { target, permission, level } => {
            let mut variant = String::new();
            for word in permission.split('-') {
                let mut letters = word.chars();
                if let Some(first) = letters.next() {
                    variant.extend(first.to_uppercase());
                    variant.extend(letters);
                }
            }
            let permission = serde_json::from_value(json!(variant))
                .map_err(|_| CliError::Input(tf!("unknown room permission: {permission}", permission = permission)))?;
            (target, RoomSetting::Permission { permission, level })
        }
        Settings::Encrypt(target) => {
            let terminal = client.terminal;
            tokio::task::spawn_blocking(move || terminal.confirm(t("Enable end-to-end encryption permanently?")))
                .await??;
            (target, RoomSetting::Encrypt)
        }
    };
    let room_id = client.resolve(&account, &target.room).await?;
    client.change(Command::ChangeRoom { account, room_id, setting }).await
}

fn setting_value(setting: Setting, value: String) -> Result<RoomSetting> {
    Ok(match setting {
        Setting::Name => RoomSetting::Name(value),
        Setting::Topic => RoomSetting::Topic(value),
        Setting::Avatar => RoomSetting::Avatar((value != "none").then(|| value.into())),
        Setting::JoinRule => {
            RoomSetting::JoinRule(join_rule(args::JoinRule::from_str(&value, false).map_err(CliError::Input)?))
        }
        Setting::History => RoomSetting::History(match value.as_str() {
            "world-readable" => HistoryVisibility::WorldReadable,
            "shared" => HistoryVisibility::Shared,
            "invited" => HistoryVisibility::Invited,
            "joined" => HistoryVisibility::Joined,
            _ => return Err(CliError::Input(t("history must be world-readable, shared, invited, or joined").into())),
        }),
        Setting::GuestAccess => RoomSetting::GuestAccess(boolean(&value)?),
        Setting::Directory => RoomSetting::Published(boolean(&value)?),
    })
}

fn boolean(value: &str) -> Result<bool> {
    value.parse().map_err(|_| CliError::Input(t("use true or false").into()))
}

pub(crate) async fn spaces(client: &mut Client, account: AccountId, command: Spaces) -> Result<Value> {
    match command {
        Spaces::List => Ok(json!(client.rooms(&account).spaces)),
        Spaces::Show(target) => {
            let space_id = client.resolve(&account, &target.room).await?;
            let list = client.rooms(&account);
            let space =
                list.spaces.iter().find(|space| space.room_id == space_id).ok_or_else(|| unknown_space(&space_id))?;
            Ok(json!(space))
        }
        Spaces::Discover(query) => discover(client, account, query.query, DirectoryKind::Spaces).await,
        Spaces::Create { create: args, parent } => create(client, account, args, parent, true, false).await,
        Spaces::Join(args) => join(client, account, args).await,
        Spaces::Leave(target) => {
            let space = client.resolve(&account, &target.room).await?;
            if !client.rooms(&account).spaces.iter().any(|joined| joined.room_id == space) {
                return Err(unknown_space(&space));
            }
            client.change(Command::LeaveSpace { account, space, rooms: Vec::new() }).await
        }
        Spaces::Children(children) => {
            let (space, change) = match children {
                Children::List { target, recursive } => {
                    let space = client.resolve(&account, &target.room).await?;
                    return client.terminal_action(&account, Action::Children { space, recursive }).await;
                }
                Children::Add(args) => (args.target, SpaceChange::Add(client.resolve(&account, &args.child).await?)),
                Children::Remove(args) => {
                    (args.target, SpaceChange::Remove(client.resolve(&account, &args.child).await?))
                }
                Children::Order { target, child_ids } => (target, SpaceChange::Order(child_ids)),
            };
            let space = client.resolve(&account, &space.room).await?;
            client.change(Command::ChangeSpace { account, space, change }).await
        }
    }
}

pub(crate) async fn dms(client: &mut Client, account: AccountId, command: Dms) -> Result<Value> {
    match command {
        Dms::List { unread } => Ok(room_rows(client.rooms(&account).rooms, RoomKind::Direct, unread)),
        Dms::Open(user) => client.run(Command::OpenDirectChat { account, user_id: user.user_id }, room_result).await,
        Dms::NoteToSelf => {
            client.run(Command::OpenNoteToSelf { account, name: t("Note to self").into() }, room_result).await
        }
        Dms::Leave(target) => {
            let room_id = client.resolve(&account, &target.room).await?;
            if !client.rooms(&account).rooms.iter().any(|room| room.room_id == room_id && room.kind == RoomKind::Direct)
            {
                return Err(CliError::Input(tf!("{room} is not a joined direct chat", room = room_id)));
            }
            client.change(Command::LeaveRoom { account, room_id, forget: false }).await
        }
    }
}

pub(crate) async fn invites(client: &mut Client, account: AccountId, command: Invites) -> Result<Value> {
    let list = client.rooms(&account).invitations;
    match command {
        Invites::List => Ok(json!(list)),
        Invites::Show(target) => {
            let room = client.resolve(&account, &target.room).await?;
            list.into_iter()
                .find(|invite| invite.room_id == room)
                .map(|invite| json!(invite))
                .ok_or_else(|| CliError::Input(tf!("no invitation for {room}", room = room)))
        }
        Invites::Accept(target) => {
            let (_, via) = room_target(&target.room)?;
            let room_id = client.resolve(&account, &target.room).await?;
            require_invite(&list, &room_id)?;
            client.run(Command::JoinRoom { account, room_id, via }, room_result).await
        }
        Invites::Decline(target) => {
            let room = client.resolve(&account, &target.room).await?;
            require_invite(&list, &room)?;
            client.terminal_action(&account, Action::DeclineInvite(room)).await
        }
    }
}

fn require_invite(invites: &[matrix_core::Invitation], room: &str) -> Result<()> {
    if invites.iter().any(|invite| invite.room_id == room) {
        Ok(())
    } else {
        Err(CliError::Input(tf!("no invitation for {room}", room = room)))
    }
}

async fn discover(client: &mut Client, account: AccountId, term: String, kind: DirectoryKind) -> Result<Value> {
    client
        .run(Command::SearchDirectory { account, term, kind }, |id, event| match event {
            Event::DirectoryFound { request, rooms } if *request == id => Some(json!(rooms)),
            _ => None,
        })
        .await
}

async fn join(client: &mut Client, account: AccountId, args: Join) -> Result<Value> {
    let (room_id, mut via) = room_target(&args.target.room)?;
    via.extend(args.via);
    client.run(Command::JoinRoom { account, room_id, via }, room_result).await
}

async fn leave(client: &mut Client, account: AccountId, target: Target) -> Result<Value> {
    let room_id = client.resolve(&account, &target.room).await?;
    client.change(Command::LeaveRoom { account, room_id, forget: false }).await
}

async fn moderate(
    client: &mut Client,
    account: AccountId,
    member: args::MemberTarget,
    action: Moderation,
) -> Result<Value> {
    let room_id = client.resolve(&account, &member.target.room).await?;
    client.change(Command::ModerateMember { account, room_id, user_id: member.user_id, action }).await
}

async fn create(
    client: &mut Client,
    account: AccountId,
    args: CreationOptions,
    parent: Option<String>,
    space: bool,
    encrypted: bool,
) -> Result<Value> {
    let parent = client.optional_room(&account, parent.as_deref()).await?;
    let room = NewRoom {
        name: args.name,
        topic: args.topic,
        space,
        parent,
        join_rule: join_rule(args.join_rule),
        address: args.alias,
        published: args.publish,
        encrypted,
    };
    client.run(Command::CreateRoom { account, room }, room_result).await
}

fn join_rule(rule: args::JoinRule) -> matrix_core::JoinRule {
    match rule {
        args::JoinRule::Private => matrix_core::JoinRule::Invite,
        args::JoinRule::Public => matrix_core::JoinRule::Public,
        args::JoinRule::Knock => matrix_core::JoinRule::Knock,
        args::JoinRule::Restricted => matrix_core::JoinRule::Restricted,
    }
}

fn room_rows(rooms: impl IntoIterator<Item = RoomSummary>, kind: RoomKind, unread: bool) -> Value {
    json!(
        rooms
            .into_iter()
            .filter(|room| {
                room.kind == kind && (!unread || room.unread > 0 || room.mentions > 0 || room.marked_unread)
            })
            .collect::<Vec<_>>()
    )
}

fn space_rooms(space: &str, spaces: &[matrix_core::SpaceSummary]) -> Result<HashSet<String>> {
    let root = spaces.iter().find(|child| child.room_id == space).ok_or_else(|| unknown_space(space))?;
    let mut pending: Vec<_> = root.children.iter().collect();
    let mut rooms = HashSet::new();
    while let Some(id) = pending.pop() {
        if rooms.insert(id.clone())
            && let Some(child) = spaces.iter().find(|child| child.room_id == *id)
        {
            pending.extend(&child.children);
        }
    }
    Ok(rooms)
}

fn unknown_space(id: &str) -> CliError {
    CliError::Input(tf!("{id} is not a joined space", id = id))
}

fn room_result(id: matrix_core::RequestId, event: &Event) -> Option<Value> {
    match event {
        Event::RoomJoined { request, room_id }
        | Event::RoomCreated { request, room_id }
        | Event::DirectChat { request, room_id }
            if *request == id =>
        {
            Some(json!({ "room_id": room_id }))
        }
        _ => None,
    }
}
