//! Administering rooms and spaces (ADR 0008): what a new room or space is made with, a room's or
//! space's settings (one section at a time, only the parts the user may change), a space's rooms,
//! leaving a space with some of its rooms, a member's own power level, and following an upgraded
//! room to the room that replaced it.

use matrix_core::{JoinRule, NewRoom, Permission, RoomSetting, RoomSettings, SpaceChange};

use super::*;

/// The levels the permission editor offers by name; anything else is a custom level.
pub const PRESET_LEVELS: [i64; 3] = [100, 50, 0];

/// What the room form is filled in with.
#[derive(Clone, Debug, PartialEq)]
pub struct RoomFields {
    pub name: TextEdit,
    pub topic: TextEdit,
    /// The main address's local part, for a public one.
    pub address: TextEdit,
    pub join_rule: JoinRule,
    /// Listed in the server's public directory (public ones only).
    pub published: bool,
    /// Encryption as the user set it; until they do, it follows the join rule: off for a
    /// public room, on otherwise.
    pub encrypt: Option<bool>,
}

impl Default for RoomFields {
    fn default() -> Self {
        Self {
            name: focused(TextEdit::new("")),
            topic: TextEdit::new(""),
            address: TextEdit::new(""),
            join_rule: JoinRule::Invite,
            published: false,
            encrypt: None,
        }
    }
}

impl RoomFields {
    /// Whether a new room would be encrypted.
    pub fn encrypted(&self) -> bool {
        self.encrypt.unwrap_or(self.join_rule != JoinRule::Public)
    }
}

/// A text field of the room form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomFormField {
    Name,
    Topic,
    Address,
}

/// The local part of an address as typed: without `#` and without a server.
pub fn local_part(typed: &str) -> Option<String> {
    let typed = typed.trim().trim_start_matches('#');
    let local = typed.split(':').next().unwrap_or_default().trim();
    (!local.is_empty()).then(|| local.to_owned())
}

/// The server of a user ID (`example.org` for `@ada:example.org`): where new addresses are made.
pub fn server_of(user_id: &str) -> &str {
    user_id.split_once(':').map_or("", |(_, server)| server)
}

/// A part of the room settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomSettingsSection {
    /// Name, topic and picture.
    General,
    /// Who may join, guest access, history and the directory listing.
    Access,
    Addresses,
    /// Turning on encryption.
    Security,
    Permissions,
    /// The room version and upgrading it.
    Advanced,
}

impl RoomSettingsSection {
    pub const ALL: [Self; 6] =
        [Self::General, Self::Access, Self::Addresses, Self::Security, Self::Permissions, Self::Advanced];

    pub fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Access => "access",
            Self::Addresses => "addresses",
            Self::Security => "security",
            Self::Permissions => "permissions",
            Self::Advanced => "advanced",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::General => t!("room_settings.general"),
            Self::Access => t!("room_settings.access"),
            Self::Addresses => t!("room_settings.addresses"),
            Self::Security => t!("room_settings.security"),
            Self::Permissions => t!("room_settings.permissions"),
            Self::Advanced => t!("room_settings.advanced"),
        }
    }

    /// Whether it holds anything the user may change: only those sections show.
    pub fn shown(self, settings: &RoomSettings) -> bool {
        let rights = &settings.rights;
        match self {
            Self::General => rights.name || rights.topic || rights.avatar,
            Self::Access => {
                rights.join_rule
                    || rights.guest_access
                    || (rights.history && !settings.space)
                    || (rights.addresses && settings.published.is_some())
            }
            Self::Addresses => rights.addresses,
            Self::Security => rights.encryption,
            Self::Permissions => rights.permissions,
            Self::Advanced => rights.upgrade,
        }
    }

    /// The sections `settings` shows, in order.
    pub fn all_shown(settings: &RoomSettings) -> Vec<Self> {
        Self::ALL.into_iter().filter(|section| section.shown(settings)).collect()
    }
}

/// A text field of the room settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsField {
    Name,
    Topic,
    /// A new address's local part.
    Address,
    /// The custom level being typed for a permission.
    CustomLevel,
}

/// A change in the room settings that asks first: neither can be undone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsConfirm {
    Encrypt,
    Upgrade,
}

/// A space order shown before the server has it: what was shown before, and what is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceOrder {
    pub account: AccountId,
    pub space: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// The room settings dialog: whose room, the section shown and what is being typed. The
/// settings themselves are the account's ([`AccountData::room_settings`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RoomSettingsDialog {
    pub account: AccountId,
    pub room_id: String,
    pub section: RoomSettingsSection,
    pub name: TextEdit,
    pub topic: TextEdit,
    pub address: TextEdit,
    /// The permission whose custom level is being typed, and the level.
    pub custom: Option<(Permission, TextEdit)>,
    /// The irreversible change waiting for the user's word.
    pub confirm: Option<SettingsConfirm>,
    /// The name and topic the fields were last filled with, once read: a field still holding
    /// them takes the fresh ones a later read brings, one the user edited keeps their text.
    filled: Option<(String, String)>,
    /// Name and topic saves on their way: the request, the field and the text it sent. A field
    /// being saved isn't refreshed until its save answers.
    saving: Vec<(RequestId, SettingsField, String)>,
}

impl RoomSettingsDialog {
    fn new(account: AccountId, room_id: String) -> Self {
        Self {
            account,
            room_id,
            section: RoomSettingsSection::General,
            name: TextEdit::new(""),
            topic: TextEdit::new(""),
            address: TextEdit::new(""),
            custom: None,
            confirm: None,
            filled: None,
            saving: Vec::new(),
        }
    }
}

/// The rooms and subspaces of a space the account has joined, in the space's order.
pub fn joined_children<'a>(account: &'a AccountData, space: &str) -> Vec<&'a str> {
    let Some(space) = account.space(space) else { return Vec::new() };
    space
        .children
        .iter()
        .filter(|id| account.room(id).is_some() || account.space(id).is_some())
        .map(String::as_str)
        .collect()
}

/// The account's group rooms a space doesn't list yet, which it may add.
pub fn addable_rooms<'a>(account: &'a AccountData, space: &str) -> Vec<&'a matrix_core::RoomSummary> {
    let children = account.space(space).map(|space| space.children.as_slice()).unwrap_or_default();
    account
        .rooms
        .iter()
        .filter(|room| room.kind == matrix_core::RoomKind::Group && !children.contains(&room.room_id))
        .collect()
}

impl State {
    // ----- the room form ---------------------------------------------------------------------

    /// Opens the form for a new room or space.
    pub fn open_room_form(&mut self, form: RoomForm) {
        self.popover = None;
        self.modal = Some(Modal::RoomForm { form, fields: Box::default() });
    }

    fn room_fields(&mut self) -> Option<&mut RoomFields> {
        match &mut self.modal {
            Some(Modal::RoomForm { fields, .. }) => Some(fields),
            _ => None,
        }
    }

    /// A text field of the room form.
    pub fn room_form_field(&mut self, field: RoomFormField) -> &mut TextEdit {
        let Some(Modal::RoomForm { fields, .. }) = &mut self.modal else { return &mut self.scratch };
        match field {
            RoomFormField::Name => &mut fields.name,
            RoomFormField::Topic => &mut fields.topic,
            RoomFormField::Address => &mut fields.address,
        }
    }

    /// Chooses who may join the new room or space.
    pub fn set_room_form_rule(&mut self, rule: JoinRule) {
        if let Some(fields) = self.room_fields() {
            fields.join_rule = rule;
        }
    }

    pub fn toggle_room_form_published(&mut self) {
        if let Some(fields) = self.room_fields() {
            fields.published = !fields.published;
        }
    }

    pub fn toggle_room_form_encryption(&mut self) {
        if let Some(fields) = self.room_fields() {
            fields.encrypt = Some(!fields.encrypted());
        }
    }

    /// What the room form would make: `None` without a name.
    pub fn room_form_request(&self) -> Option<NewRoom> {
        let Some(Modal::RoomForm { form, fields }) = &self.modal else { return None };
        let name = fields.name.text.trim();
        if name.is_empty() {
            return None;
        }
        let (space, parent) = match form {
            RoomForm::NewRoom { space } => (false, space.clone()),
            RoomForm::NewSpace => (true, None),
        };
        // Space members needs a space to take them from.
        let join_rule = match fields.join_rule {
            JoinRule::Restricted if parent.is_none() => JoinRule::Invite,
            rule => rule,
        };
        let public = join_rule == JoinRule::Public;
        Some(NewRoom {
            name: name.to_owned(),
            topic: Some(fields.topic.text.trim().to_owned()).filter(|topic| !topic.is_empty()),
            space,
            parent,
            join_rule,
            address: public.then(|| local_part(&fields.address.text)).flatten(),
            published: public && fields.published,
            encrypted: !space && fields.encrypted(),
        })
    }

    pub fn submit_room_form(&mut self) {
        let Some(account) = self.nav.account.clone() else { return };
        let Some(room) = self.room_form_request() else { return };
        self.modal = None;
        let purpose = Purpose::Create { account: account.clone(), space: room.space };
        self.dispatch(Command::CreateRoom { account, room }, purpose);
    }

    // ----- room settings ---------------------------------------------------------------------

    /// Opens a room's or space's settings, reading them afresh.
    pub fn open_room_settings(&mut self, room_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.modal = Some(Modal::RoomSettings(Box::new(RoomSettingsDialog::new(account.clone(), room_id.to_owned()))));
        self.fill_room_settings();
        let command = Command::LoadRoomSettings { account, room_id: room_id.to_owned() };
        self.dispatch(command, Purpose::ReadRoomSettings);
    }

    /// The settings the open room settings show, once read.
    pub fn shown_room_settings(&self, dialog: &RoomSettingsDialog) -> Option<&RoomSettings> {
        self.accounts.get(&dialog.account)?.room_settings.get(&dialog.room_id)
    }

    fn settings_dialog(&mut self) -> Option<&mut RoomSettingsDialog> {
        match &mut self.modal {
            Some(Modal::RoomSettings(dialog)) => Some(dialog),
            _ => None,
        }
    }

    /// The open dialog and its settings, to change them.
    fn settings_open(&self) -> Option<(AccountId, String, &RoomSettings)> {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return None };
        let settings = self.shown_room_settings(dialog)?;
        Some((dialog.account.clone(), dialog.room_id.clone(), settings))
    }

    /// Fills the fields of the open settings from what was read (once), and keeps a section
    /// shown that the user may change something in.
    pub(super) fn fill_room_settings(&mut self) {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return };
        let Some(settings) = self.shown_room_settings(dialog).cloned() else { return };
        let Some(dialog) = self.settings_dialog() else { return };
        let (name, topic) = (settings.name.clone().unwrap_or_default(), settings.topic.clone().unwrap_or_default());
        let (was_name, was_topic) = dialog.filled.clone().unzip();
        let saving = |field| dialog.saving.iter().any(|(_, saved, _)| *saved == field);
        let (saving_name, saving_topic) = (saving(SettingsField::Name), saving(SettingsField::Topic));
        // A field being saved waits for its save; the others take a fresh value unless edited.
        let name = if saving_name {
            was_name.unwrap_or_default()
        } else {
            if was_name.is_none_or(|was| dialog.name.text == was) && dialog.name.text != name {
                dialog.name = TextEdit::new(name.clone());
            }
            name
        };
        let topic = if saving_topic {
            was_topic.unwrap_or_default()
        } else {
            if was_topic.is_none_or(|was| dialog.topic.text == was) && dialog.topic.text != topic {
                dialog.topic = TextEdit::new(topic.clone());
            }
            topic
        };
        dialog.filled = Some((name, topic));
        // An upgrade asked about can't happen any more (another admin replaced the room).
        if dialog.confirm == Some(SettingsConfirm::Upgrade) && settings.upgrade_to.is_none() {
            dialog.confirm = None;
        }
        // A permission being edited that may no longer be changed (the room was replaced, or the
        // user's level fell) stops being edited.
        let editable = |permission: Permission| {
            settings.permissions.iter().any(|shown| shown.permission == permission && shown.editable)
        };
        if dialog.custom.as_ref().is_some_and(|(permission, _)| !editable(*permission)) {
            dialog.custom = None;
        }

        let shown = RoomSettingsSection::all_shown(&settings);
        if !shown.contains(&dialog.section)
            && let Some(first) = shown.first()
        {
            dialog.section = *first;
        }
        if let Some(Popover::Select(SelectFor::Permission(permission))) = self.popover
            && !editable(permission)
        {
            self.popover = None;
        }
    }

    pub fn show_room_settings_section(&mut self, section: RoomSettingsSection) {
        self.popover = None;
        if let Some(dialog) = self.settings_dialog() {
            dialog.section = section;
            dialog.custom = None;
        }
    }

    /// A text field of the room settings.
    pub fn room_settings_field(&mut self, field: SettingsField) -> &mut TextEdit {
        let Some(Modal::RoomSettings(dialog)) = &mut self.modal else { return &mut self.scratch };
        match field {
            SettingsField::Name => &mut dialog.name,
            SettingsField::Topic => &mut dialog.topic,
            SettingsField::Address => &mut dialog.address,
            SettingsField::CustomLevel => match &mut dialog.custom {
                Some((_, edit)) => edit,
                None => &mut self.scratch,
            },
        }
    }

    /// Changes one setting of the room or space whose settings are open.
    pub fn change_room(&mut self, setting: RoomSetting) {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return };
        let (account, room_id) = (dialog.account.clone(), dialog.room_id.clone());
        self.popover = None;
        self.dispatch(Command::ChangeRoom { account, room_id, setting }, Purpose::RoomSettings);
    }

    /// Whether the name field differs from the room's name, so it may be saved.
    pub fn room_name_changed(&self) -> bool {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return false };
        let typed = dialog.name.text.trim();
        let saved = self.shown_room_settings(dialog).and_then(|settings| settings.name.as_deref()).unwrap_or("");
        !typed.is_empty() && typed != saved
    }

    /// Whether the topic field differs from the room's topic.
    pub fn room_topic_changed(&self) -> bool {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return false };
        let saved = self.shown_room_settings(dialog).and_then(|settings| settings.topic.as_deref()).unwrap_or("");
        dialog.topic.text.trim() != saved.trim()
    }

    pub fn save_room_name(&mut self) {
        if self.room_name_changed() {
            self.save_room_text(SettingsField::Name);
        }
    }

    pub fn save_room_topic(&mut self) {
        if self.room_topic_changed() {
            self.save_room_text(SettingsField::Topic);
        }
    }

    /// Saves the name or the topic as typed, remembering what was sent: until the room answers,
    /// that field isn't refreshed; then it takes the room's value unless typed in again since,
    /// and a failure leaves the draft to try again.
    fn save_room_text(&mut self, field: SettingsField) {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return };
        let (account, room_id) = (dialog.account.clone(), dialog.room_id.clone());
        let typed = match field {
            SettingsField::Name => dialog.name.text.clone(),
            _ => dialog.topic.text.clone(),
        };
        let setting = match field {
            SettingsField::Name => RoomSetting::Name(typed.trim().to_owned()),
            _ => RoomSetting::Topic(typed.trim().to_owned()),
        };
        self.popover = None;
        let request = self.dispatch(Command::ChangeRoom { account, room_id, setting }, Purpose::RoomText);
        if let Some(dialog) = self.settings_dialog() {
            dialog.saving.push((request, field, typed));
        }
    }

    /// A name or topic save answered: on success the field takes the room's value unless the
    /// user typed since; on failure the draft stays.
    pub(super) fn room_text_saved(&mut self, request: RequestId, saved: bool) {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return };
        let settings = self.shown_room_settings(dialog).cloned();
        let Some(dialog) = self.settings_dialog() else { return };
        let Some(at) = dialog.saving.iter().position(|(sent, _, _)| *sent == request) else { return };
        let (_, field, typed) = dialog.saving.remove(at);
        let (Some(settings), true) = (settings, saved) else { return };
        let Some((name, topic)) = dialog.filled.as_mut() else { return };
        let (edit, baseline, fresh) = match field {
            SettingsField::Name => (&mut dialog.name, name, settings.name.unwrap_or_default()),
            _ => (&mut dialog.topic, topic, settings.topic.unwrap_or_default()),
        };
        if edit.text == typed && edit.text != fresh {
            *edit = TextEdit::new(fresh.clone());
        }
        *baseline = fresh;
    }

    /// Adds the typed address (on the account's server) to the room.
    pub fn add_room_address(&mut self) {
        let Some(dialog) = self.settings_dialog() else { return };
        let Some(local) = local_part(&dialog.address.text) else { return };
        dialog.address = TextEdit::new("");
        self.change_room(RoomSetting::AddAddress(local));
    }

    /// Asks for a new picture for the room whose settings are open.
    pub fn pick_room_avatar(&mut self) {
        let Some((account, room_id, _)) = self.settings_open() else { return };
        self.shell.pick_files(PickFor::RoomAvatar { account, room_id });
    }

    /// Gives a permission the level `level`, never above the user's own, unless it already asks
    /// for more than the user has (then it isn't theirs to change).
    pub fn set_permission_level(&mut self, permission: Permission, level: i64) {
        let Some((_, _, settings)) = self.settings_open() else { return };
        let own = settings.own_level;
        let shown = settings.permissions.iter().find(|shown| shown.permission == permission);
        if !shown.is_some_and(|shown| shown.editable) {
            return;
        }
        let current = shown.map(|shown| shown.level);
        let level = level.min(own);
        if let Some(dialog) = self.settings_dialog() {
            dialog.custom = None;
        }
        // Send messages and Access show the highest of several levels: the same level may still
        // raise or lower one of them, so it is always sent.
        let several = matches!(permission, Permission::SendMessages | Permission::Access);
        if several || current != Some(level) {
            self.change_room(RoomSetting::Permission { permission, level });
        } else {
            self.popover = None;
        }
    }

    /// Starts typing a custom level for a permission, from its current one.
    pub fn edit_custom_level(&mut self, permission: Permission) {
        let current = self
            .settings_open()
            .and_then(|(_, _, settings)| settings.permissions.iter().find(|shown| shown.permission == permission))
            .map(|shown| shown.level);
        self.popover = None;
        if let (Some(dialog), Some(current)) = (self.settings_dialog(), current) {
            dialog.custom = Some((permission, focused(TextEdit::new(current.to_string()))));
        }
    }

    /// The custom level typed, if it is a number.
    pub fn custom_level(&self) -> Option<(Permission, i64)> {
        let Some(Modal::RoomSettings(dialog)) = &self.modal else { return None };
        let (permission, edit) = dialog.custom.as_ref()?;
        Some((*permission, edit.text.trim().parse().ok()?))
    }

    pub fn save_custom_level(&mut self) {
        if let Some((permission, level)) = self.custom_level() {
            self.set_permission_level(permission, level);
        }
    }

    pub fn cancel_custom_level(&mut self) {
        if let Some(dialog) = self.settings_dialog() {
            dialog.custom = None;
        }
    }

    /// Asks before an irreversible change.
    pub fn ask_room_settings(&mut self, confirm: SettingsConfirm) {
        if let Some(dialog) = self.settings_dialog() {
            dialog.confirm = Some(confirm);
        }
    }

    /// Carries out the change the settings asked about.
    pub fn confirm_room_settings(&mut self) {
        let Some(dialog) = self.settings_dialog() else { return };
        let Some(confirm) = dialog.confirm.take() else { return };
        match confirm {
            SettingsConfirm::Encrypt => self.change_room(RoomSetting::Encrypt),
            SettingsConfirm::Upgrade => {
                let Some((account, room_id, settings)) = self.settings_open() else { return };
                let Some(version) = settings.upgrade_to.clone() else { return };
                let purpose = Purpose::Upgrade { account: account.clone(), from: room_id.clone() };
                self.dispatch(Command::UpgradeRoom { account, room_id, version }, purpose);
            }
        }
    }

    /// Goes back from a question to the settings, without the change.
    pub fn cancel_room_settings_question(&mut self) {
        if let Some(dialog) = self.settings_dialog() {
            dialog.confirm = None;
        }
    }

    // ----- a space's rooms -------------------------------------------------------------------

    /// Opens the list of a space's rooms, to add, remove and order them.
    pub fn open_space_rooms(&mut self, space: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        self.popover = None;
        self.modal = Some(Modal::SpaceRooms { account, space: space.to_owned() });
    }

    /// The open space-rooms dialog's space, while the user may still arrange it.
    fn space_rooms_open(&self) -> Option<(AccountId, String)> {
        let Some(Modal::SpaceRooms { account, space }) = &self.modal else { return None };
        let allowed = self.accounts.get(account).and_then(|data| data.space(space)).is_some_and(|s| s.can_add_rooms);
        allowed.then(|| (account.clone(), space.clone()))
    }

    /// Moves a room of the open space `by` places (up when negative), at once and on the server.
    pub fn move_space_room(&mut self, room_id: &str, by: isize) {
        let Some((account, space)) = self.space_rooms_open() else { return };
        let Some(before) = self.accounts.get(&account).and_then(|data| data.space(&space)).map(|s| s.children.clone())
        else {
            return;
        };
        let mut order = before.clone();
        let Some(from) = order.iter().position(|id| id == room_id) else { return };
        let Some(to) = from.checked_add_signed(by).filter(|to| *to < order.len()) else { return };
        let moved = order.remove(from);
        order.insert(to, moved);
        self.accounts.order_space(&account, &space, order.clone());
        let shown = SpaceOrder { account: account.clone(), space: space.clone(), before, after: order.clone() };
        let command = Command::ChangeSpace { account, space, change: SpaceChange::Order(order) };
        self.dispatch(command, Purpose::SpaceRooms { order: Some(shown) });
    }

    /// Puts back the order shown before a refused one, unless another order followed it.
    pub(super) fn unorder_space(&mut self, order: SpaceOrder) {
        let SpaceOrder { account, space, before, after } = order;
        let unchanged =
            self.accounts.get(&account).and_then(|data| data.space(&space)).is_some_and(|s| s.children == after);
        if unchanged {
            self.accounts.order_space(&account, &space, before);
        }
    }

    /// What the open admin dialogs depend on of `account`'s rights: each room's and space's
    /// `can_change_settings`, each space's `can_add_rooms`, and each room's own rights.
    pub(super) fn admin_rights(&self, account: &AccountId) -> Vec<(String, bool, bool, bool, bool)> {
        let Some(data) = self.accounts.get(account) else { return Vec::new() };
        let open = match &self.modal {
            Some(Modal::RoomSettings(dialog)) if &dialog.account == account => dialog.room_id.clone(),
            Some(Modal::SpaceRooms { account: owner, space }) if owner == account => space.clone(),
            _ => return Vec::new(),
        };
        // A successor counts too: the room can't be upgraded again once it has one.
        let room =
            data.room_unindexed(&open).map(|r| (r.can_change_settings, r.can_rename, r.can_pin, r.successor.is_some()));
        let space = data.space(&open).map(|s| (s.can_change_settings, s.can_rename, s.can_add_rooms, false));
        room.or(space).map(|(a, b, c, d)| (open, a, b, c, d)).into_iter().collect()
    }

    /// A room just replaced shows its successor banner in place of its composers: a memo being
    /// recorded there would go on with no way to stop it, so it is dropped.
    pub(super) fn cancel_memos_in_replaced_rooms(&mut self, account: &AccountId) {
        for target in [ComposerTarget::Room, ComposerTarget::Thread] {
            let replaced = self.composer(target).voice.as_ref().is_some_and(|memo| {
                let (owner, key) = &memo.timeline;
                owner == account
                    && self
                        .accounts
                        .get(account)
                        .and_then(|data| data.room_unindexed(&key.room_id))
                        .is_some_and(|room| room.successor.is_some())
            });
            if replaced && let Some(memo) = self.composer_mut(target).voice.take() {
                memo.recorder.cancel();
            }
        }
    }

    /// The user's rights in the room or space of an open admin dialog changed (or it is gone):
    /// close what they may no longer use, and read the settings again.
    pub(super) fn admin_rights_changed(&mut self, account: &AccountId) {
        let Some(data) = self.accounts.get(account) else { return };
        match &self.modal {
            Some(Modal::SpaceRooms { space, .. }) if !data.space(space).is_some_and(|s| s.can_add_rooms) => {
                self.modal = None;
            }
            Some(Modal::RoomSettings(dialog)) => {
                let room_id = dialog.room_id.clone();
                let allowed = data
                    .room_unindexed(&room_id)
                    .map(|r| r.can_change_settings)
                    .or_else(|| data.space(&room_id).map(|s| s.can_change_settings))
                    .unwrap_or(false);
                if allowed {
                    let command = Command::LoadRoomSettings { account: account.clone(), room_id };
                    self.dispatch(command, Purpose::ReadRoomSettings);
                } else {
                    self.modal = None;
                }
            }
            _ => {}
        }
    }

    /// Takes a room out of the open space.
    pub fn remove_space_room(&mut self, room_id: &str) {
        let Some((account, space)) = self.space_rooms_open() else { return };
        let change = SpaceChange::Remove(room_id.to_owned());
        self.dispatch(Command::ChangeSpace { account, space, change }, Purpose::SpaceRooms { order: None });
    }

    /// Adds a joined room to the open space.
    pub fn add_space_room(&mut self, room_id: &str) {
        let Some((account, space)) = self.space_rooms_open() else { return };
        let change = SpaceChange::Add(room_id.to_owned());
        self.dispatch(Command::ChangeSpace { account, space, change }, Purpose::SpaceRooms { order: None });
    }

    // ----- leaving a space -------------------------------------------------------------------

    /// Asks before leaving a space, and which of its rooms to leave with it (none at first).
    pub fn ask_to_leave_space(&mut self, space: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        if self.accounts.get(&account).and_then(|data| data.space(space)).is_none() {
            return;
        }
        self.popover = None;
        self.modal = Some(Modal::LeaveSpace { account, space: space.to_owned(), chosen: Vec::new() });
    }

    /// Chooses whether to leave one of the space's rooms too.
    pub fn toggle_leave_with(&mut self, room_id: &str) {
        let Some(Modal::LeaveSpace { chosen, .. }) = &mut self.modal else { return };
        if let Some(index) = chosen.iter().position(|id| id == room_id) {
            chosen.remove(index);
        } else {
            chosen.push(room_id.to_owned());
        }
    }

    /// Chooses every joined room of the space, or none when all are chosen.
    pub fn toggle_leave_all(&mut self) {
        let Some(Modal::LeaveSpace { account, space, chosen }) = &self.modal else { return };
        let Some(data) = self.accounts.get(account) else { return };
        let all: Vec<String> = joined_children(data, space).into_iter().map(ToOwned::to_owned).collect();
        let every = all.iter().all(|id| chosen.contains(id));
        if let Some(Modal::LeaveSpace { chosen, .. }) = &mut self.modal {
            *chosen = if every { Vec::new() } else { all };
        }
    }

    /// Leaves the space and the rooms chosen, in the space's order.
    pub fn leave_space(&mut self) {
        let Some(Modal::LeaveSpace { account, space, chosen }) = self.modal.take() else { return };
        let rooms = self
            .accounts
            .get(&account)
            .map(|data| joined_children(data, &space).into_iter().filter(|id| chosen.iter().any(|c| c == id)))
            .into_iter()
            .flatten()
            .map(ToOwned::to_owned)
            .collect();
        self.dispatch(Command::LeaveSpace { account, space, rooms }, Purpose::Leave);
    }

    // ----- a member's power level ------------------------------------------------------------

    /// Opens the dialog for giving a member of the open room a level of the user's choosing.
    pub fn open_member_level(&mut self, user_id: &str) {
        let (Some(account), Some(room_id)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned))
        else {
            return;
        };
        let Some(member) = self.accounts.get(&account).and_then(|data| data.member(&room_id, user_id)) else { return };
        let (name, level) = (member.person.name.clone(), member.level);
        self.popover = None;
        let edit = focused(TextEdit::new(level.to_string()));
        self.modal = Some(Modal::MemberLevel { account, room_id, user_id: user_id.to_owned(), name, edit });
    }

    /// The level typed in the member level dialog, when the user may give it.
    pub fn member_level(&self) -> Option<i64> {
        let Some(Modal::MemberLevel { account, room_id, user_id, edit, .. }) = &self.modal else { return None };
        let level: i64 = edit.text.trim().parse().ok()?;
        let data = self.accounts.get(account)?;
        let members = data.members.get(room_id)?;
        let member = data.member(room_id, user_id)?;
        members.may(matrix_core::Moderation::Level(level), member).then_some(level)
    }

    pub fn submit_member_level(&mut self) {
        let Some(level) = self.member_level() else { return };
        let Some(Modal::MemberLevel { user_id, .. }) = self.modal.take() else { return };
        self.moderate(&user_id, matrix_core::Moderation::Level(level));
    }

    // ----- upgraded rooms --------------------------------------------------------------------

    /// Goes from the open room to the room that replaced it: opens it when joined, else joins
    /// it and opens it once joined.
    pub fn follow_successor(&mut self) {
        let (Some(account), Some(from)) = (self.nav.account.clone(), self.active_room_id().map(str::to_owned)) else {
            return;
        };
        let Some(data) = self.accounts.get(&account) else { return };
        let Some(successor) = data.room(&from).and_then(|room| room.successor.clone()) else { return };
        if data.room(&successor.room_id).is_some() {
            self.open_room(&account, &successor.room_id);
            return;
        }
        if self.following_successor(&account, &from) {
            return;
        }
        let command =
            Command::JoinRoom { account: account.clone(), room_id: successor.room_id.clone(), via: successor.via };
        self.dispatch(command, Purpose::Successor { account, from, to: successor.room_id });
    }

    /// Whether the room that replaced `from` is being joined.
    pub fn following_successor(&self, account: &AccountId, from: &str) -> bool {
        self.pending.values().any(
            |purpose| matches!(purpose, Purpose::Successor { account: a, from: f, .. } if a == account && f == from),
        )
    }

    /// The successor was joined: it opens if the user still looks at the room it replaced.
    pub(super) fn successor_joined(&mut self, account: &AccountId, from: &str, to: &str) {
        if self.nav.account.as_ref() == Some(account) && self.active_room_id() == Some(from) {
            self.open_room(account, to);
        }
    }
}
