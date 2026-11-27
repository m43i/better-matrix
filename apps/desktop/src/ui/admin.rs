//! Administering rooms and spaces (ADR 0008): the new-room and new-space form, a room's or
//! space's settings (only the parts the user may change), a space's rooms, leaving a space with
//! some of its rooms, a member's own power level, and the banner of a room that was upgraded.

use matrix_core::{HistoryVisibility, JoinRule, Permission, RoomSetting, RoomSettings, RoomSummary};
use rune::{Color, El, ScrollSpec, col, el, icon, row, text};

use super::components::{
    ButtonKind, button, choice_row, closable_dialog, dialog, field_input, icon_button, labelled, media_avatar,
    pick_row, select_button, select_menu, select_option, setting_group, switch_row,
};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{
    AccountData, Modal, PRESET_LEVELS, Popover, RoomFields, RoomForm, RoomFormField, RoomSettingsDialog,
    RoomSettingsSection, SelectFor, SettingsConfirm, SettingsField, State, addable_rooms, joined_children, server_of,
};

/// The dialog for an administration modal, if `modal` is one.
pub fn dialog_for(s: &State, modal: &Modal, window: [f32; 2], p: &Palette) -> Option<El> {
    let account = s.active_account()?;
    Some(match modal {
        Modal::RoomForm { form, fields } => room_form(account, form, fields, p),
        Modal::RoomSettings(dialog) => room_settings(s, dialog, window, p),
        Modal::SpaceRooms { space, .. } => space_rooms(s, account, space, p),
        Modal::LeaveSpace { space, chosen, .. } => leave_space(s, account, space, chosen, p),
        Modal::MemberLevel { name, edit, .. } => member_level(s, name, edit, p),
        _ => return None,
    })
}

// ----- who may join ----------------------------------------------------------------------------

/// A stable key part per join rule.
fn rule_id(rule: JoinRule) -> &'static str {
    match rule {
        JoinRule::Invite => "private",
        JoinRule::Restricted => "space-members",
        JoinRule::Knock => "ask",
        JoinRule::Public => "public",
    }
}

/// A join rule's name and what it means.
fn rule_words(rule: JoinRule) -> (&'static str, &'static str) {
    match rule {
        JoinRule::Invite => (t!("join_rule.invite"), t!("join_rule.invite_hint")),
        JoinRule::Restricted => (t!("join_rule.restricted"), t!("join_rule.restricted_hint")),
        JoinRule::Knock => (t!("join_rule.knock"), t!("join_rule.knock_hint")),
        JoinRule::Public => (t!("join_rule.public"), t!("join_rule.public_hint")),
    }
}

/// The join rules offered: Space members only where there is a space to take them from, Ask to
/// join only where the room can take requests (never a space: nowhere to answer them).
fn rules(space_members: bool, knock: bool) -> Vec<JoinRule> {
    let mut rules = vec![JoinRule::Invite];
    if space_members {
        rules.push(JoinRule::Restricted);
    }
    if knock {
        rules.push(JoinRule::Knock);
    }
    rules.push(JoinRule::Public);
    rules
}

/// One radio list of join rules; `choose` runs with the one pressed.
fn rule_choices(
    key: &str,
    offered: Vec<JoinRule>,
    current: Option<JoinRule>,
    choose: fn(&mut State, JoinRule),
    p: &Palette,
) -> El {
    offered.into_iter().fold(col().w_full().gap(6.0), |list, rule| {
        let (title, hint) = rule_words(rule);
        list.child(
            choice_row(title, Some(hint), current == Some(rule), p)
                .key(format!("{key}-{}", rule_id(rule)))
                .on_press(move |s: &mut State| choose(s, rule)),
        )
    })
}

/// An address field: `#`, the local part typed, `:server`.
fn address_field(server: &str, key: &str, field: El, p: &Palette) -> El {
    row()
        .key(format!("{key}-row"))
        .w_full()
        .gap(6.0)
        .center_y()
        .child(text("#").font(font::BODY_STRONG).color(p.text_muted))
        .child(col().grow(1.0).min_w(0.0).child(field))
        .child(text(format!(":{server}")).font(font::BODY).color(p.text_muted).shrink().truncate())
}

// ----- the room form ---------------------------------------------------------------------------

fn room_form(account: &AccountData, form: &RoomForm, fields: &RoomFields, p: &Palette) -> El {
    let (title, in_space, is_room) = match form {
        RoomForm::NewRoom { space } => (t!("create.room_title"), space.is_some(), true),
        RoomForm::NewSpace => (t!("create.space_title"), false, false),
    };
    let field = |edit: &rune::TextEdit, placeholder: &str, which: RoomFormField, key: &str| {
        field_input(edit, placeholder, p)
            .key(key.to_owned())
            .on_edit(move |s: &mut State| s.room_form_field(which))
            .on_submit(|s: &mut State, _| s.submit_room_form())
    };
    let mut content = col()
        .w_full()
        .gap(10.0)
        .child(field(&fields.name, t!("create.name"), RoomFormField::Name, "room-form-name"))
        .child(field(&fields.topic, t!("create.topic"), RoomFormField::Topic, "room-form-topic"));
    let choices = rule_choices(
        "room-form-rule",
        rules(in_space, is_room),
        Some(fields.join_rule),
        |s, rule| s.set_room_form_rule(rule),
        p,
    );
    content = content.child(labelled(t!("create.who_can_join"), choices, p).mt(6.0));
    if fields.join_rule == JoinRule::Public {
        let server = server_of(account.summary.id.as_str());
        let input =
            field(&fields.address, t!("create.address_placeholder"), RoomFormField::Address, "room-form-address");
        content = content
            .child(labelled(t!("create.address"), address_field(server, "room-form-address", input, p), p))
            .child(
                switch_row(t!("create.publish"), Some(t!("create.publish_hint")), fields.published, p)
                    .key("room-form-publish")
                    .on_press(State::toggle_room_form_published),
            );
    }
    if matches!(form, RoomForm::NewRoom { .. }) {
        content = content.child(
            switch_row(t!("create.encrypt"), Some(t!("create.encrypt_hint")), fields.encrypted(), p)
                .key("room-form-encrypt")
                .on_press(State::toggle_room_form_encryption),
        );
    }
    dialog(
        "room-form",
        title,
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("room-form-cancel")
                .on_press(State::close_modal),
            button(t!("create.create"), ButtonKind::Primary, !fields.name.text.trim().is_empty(), p)
                .key("room-form-submit")
                .on_press(State::submit_room_form),
        ],
        p,
    )
}

// ----- room settings ---------------------------------------------------------------------------

/// The modal's corner radius.
const RADIUS: f32 = 14.0;

fn room_settings(s: &State, dialog: &RoomSettingsDialog, window: [f32; 2], p: &Palette) -> El {
    let settings = s.shown_room_settings(dialog);
    let name = s
        .accounts
        .get(&dialog.account)
        .and_then(|account| {
            account
                .room(&dialog.room_id)
                .map(|room| room.name.clone())
                .or_else(|| account.space(&dialog.room_id).map(|space| space.name.clone()))
        })
        .unwrap_or_else(|| dialog.room_id.clone());
    let sections = settings.map(RoomSettingsSection::all_shown).unwrap_or_default();
    let mut nav = col()
        .key("room-settings-nav")
        .w(200.0)
        .h_full()
        .padding(12.0)
        .gap(2.0)
        .fill(p.surface.mix(p.bg, 0.5))
        .border_sides([0.0, 1.0, 0.0, 0.0], p.divider)
        .child(text(t!("room_settings.title")).font(font::HEADER).color(p.text).px(10.0).pt(8.0))
        .child(text(name).font(font::META).color(p.text_muted).px(10.0).pb(8.0).truncate());
    for section in &sections {
        let (section, selected) = (*section, *section == dialog.section);
        let glyph = match section {
            RoomSettingsSection::General => &icons::SETTINGS,
            RoomSettingsSection::Access => &icons::USER_PLUS,
            RoomSettingsSection::Addresses => &icons::AT,
            RoomSettingsSection::Security => &icons::LOCK,
            RoomSettingsSection::Permissions => &icons::SHIELD,
            RoomSettingsSection::Advanced => &icons::PACKAGE,
        };
        nav = nav.child(
            row()
                .key(format!("room-settings-{}", section.id()))
                .w_full()
                .h(36.0)
                .px(10.0)
                .gap(10.0)
                .center_y()
                .radius(6.0)
                .fill(if selected { p.accent.alpha(0.12) } else { Color::TRANSPARENT })
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("tab")
                .cursor(rune::Cursor::Pointer)
                .checked(selected)
                .focusable(true)
                .child(icon(glyph, 18.0).color(if selected { p.accent } else { p.text_muted }))
                .child(text(section.label()).font(if selected { font::BODY_STRONG } else { font::BODY }).color(p.text))
                .on_press(move |s: &mut State| s.show_room_settings_section(section)),
        );
    }
    let body = match settings {
        None => text(t!("room_settings.loading")).font(font::BODY).color(p.text_muted),
        Some(settings) => match dialog.section {
            RoomSettingsSection::General => general(s, dialog, settings, p),
            RoomSettingsSection::Access => access(settings, p),
            RoomSettingsSection::Addresses => addresses(dialog, settings, p),
            RoomSettingsSection::Security => security(p),
            RoomSettingsSection::Permissions => permissions(s, dialog, settings, p),
            RoomSettingsSection::Advanced => advanced(settings, p),
        },
    };
    let title = if settings.is_some() { dialog.section.label() } else { t!("room_settings.title") };
    let header = row()
        .w_full()
        .h(56.0)
        .px(24.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(title).font(font::HEADER).color(p.text).grow(1.0))
        .child(
            icon_button(&icons::CLOSE, t!("common.close"), false, p.text_muted, p)
                .key("room-settings-close")
                .on_press(State::close_modal),
        );
    let content = col().instant_changes().grow(1.0).h_full().min_w(0.0).child(header).child(
        col()
            .key("room-settings-body")
            .w_full()
            .grow(1.0)
            .shrink()
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(body.key(format!("room-settings-section-{}", dialog.section.id())).padding(24.0)),
    );
    let [w, h] = window;
    let card = col()
        .key("room-settings")
        .size((w * 0.9).min(820.0), (h * 0.9).min(620.0))
        .padding(1.0)
        .radius(RADIUS)
        .fill(p.surface)
        .border(1.0, p.text_muted.alpha(0.35))
        .shadow(16.0, 48.0, Color::BLACK.alpha(0.5))
        .role("dialog")
        .label(t!("room_settings.title"))
        .child(row().w_full().h_full().radius(RADIUS - 1.0).clip_rounded(RADIUS - 1.0).child(nav).child(content));
    match dialog.confirm {
        // The question takes the settings' place; cancelling it goes back to them.
        Some(confirm) => settings_question(confirm, settings, p),
        None => card,
    }
}

/// Asking before turning on encryption or upgrading.
fn settings_question(confirm: SettingsConfirm, settings: Option<&RoomSettings>, p: &Palette) -> El {
    let (title, body, action) = match confirm {
        SettingsConfirm::Encrypt => (
            t!("room_settings.encrypt_title").to_owned(),
            t!("room_settings.encrypt_body").to_owned(),
            t!("room_settings.encrypt_confirm"),
        ),
        SettingsConfirm::Upgrade => {
            let version = settings.and_then(|settings| settings.upgrade_to.clone()).unwrap_or_default();
            (
                t!("room_settings.upgrade_title").to_owned(),
                tf!("room_settings.upgrade_body", version = version),
                t!("room_settings.upgrade_confirm"),
            )
        }
    };
    dialog(
        "room-settings-confirm",
        &title,
        text(body).font(font::BODY).color(p.text_muted).wrap(),
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("room-settings-confirm-cancel")
                .on_press(State::cancel_room_settings_question),
            button(action, ButtonKind::Danger, true, p)
                .key("room-settings-confirm-ok")
                .on_press(State::confirm_room_settings),
        ],
        p,
    )
}

/// A field and its save button side by side.
fn field_with_save(field: El, save: El) -> El {
    row().w_full().gap(8.0).center_y().child(col().grow(1.0).min_w(0.0).child(field)).child(save)
}

/// Picture, name and topic, each with its right.
fn general(s: &State, dialog: &RoomSettingsDialog, settings: &RoomSettings, p: &Palette) -> El {
    let rights = &settings.rights;
    let mut page = col().w_full().gap(28.0);
    if rights.avatar {
        let name = settings.name.as_deref().unwrap_or_default();
        let mut controls = row()
            .gap(14.0)
            .center_y()
            .child(media_avatar(&settings.room_id, name, settings.avatar.as_ref(), &s.media, 64.0))
            .child(
                button(t!("room_settings.upload_picture"), ButtonKind::Secondary, true, p)
                    .key("room-settings-picture")
                    .on_press_tx(State::pick_room_avatar),
            );
        if settings.avatar.is_some() {
            controls = controls.child(
                button(t!("common.remove"), ButtonKind::Link, true, p)
                    .key("room-settings-picture-remove")
                    .on_press(|s: &mut State| s.change_room(RoomSetting::Avatar(None))),
            );
        }
        page = page.child(setting_group(
            &icons::IMAGE,
            t!("room_settings.picture"),
            t!("room_settings.picture_hint"),
            controls,
            p,
        ));
    }
    if rights.name {
        let field = field_input(&dialog.name, t!("create.name"), p)
            .key("room-settings-name")
            .on_edit(|s: &mut State| s.room_settings_field(SettingsField::Name))
            .on_submit(|s: &mut State, _| s.save_room_name());
        let save = button(t!("common.save"), ButtonKind::Primary, s.room_name_changed(), p)
            .key("room-settings-name-save")
            .on_press(State::save_room_name);
        page = page.child(setting_group(
            &icons::PENCIL,
            t!("room_settings.name"),
            t!("room_settings.name_hint"),
            field_with_save(field, save),
            p,
        ));
    }
    if rights.topic {
        let field = field_input(&dialog.topic, t!("room_settings.topic_placeholder"), p)
            .key("room-settings-topic")
            .on_edit(|s: &mut State| s.room_settings_field(SettingsField::Topic))
            .on_submit(|s: &mut State, _| s.save_room_topic());
        let save = button(t!("common.save"), ButtonKind::Primary, s.room_topic_changed(), p)
            .key("room-settings-topic-save")
            .on_press(State::save_room_topic);
        page = page.child(setting_group(
            &icons::INFO,
            t!("room_settings.topic"),
            t!("room_settings.topic_hint"),
            field_with_save(field, save),
            p,
        ));
    }
    page
}

/// A stable key part per history setting.
fn history_id(history: HistoryVisibility) -> &'static str {
    match history {
        HistoryVisibility::WorldReadable => "anyone",
        HistoryVisibility::Shared => "members",
        HistoryVisibility::Invited => "invited",
        HistoryVisibility::Joined => "joined",
    }
}

/// Who may join, guests, who may read history and the directory listing, each with its right.
fn access(settings: &RoomSettings, p: &Palette) -> El {
    let rights = &settings.rights;
    let mut page = col().w_full().gap(28.0);
    if rights.join_rule {
        let restricted = settings.can_restrict || settings.join_rule == Some(JoinRule::Restricted);
        let choices = rule_choices(
            "room-settings-rule",
            rules(restricted, settings.can_knock || settings.join_rule == Some(JoinRule::Knock)),
            settings.join_rule,
            |s, rule| s.change_room(RoomSetting::JoinRule(rule)),
            p,
        );
        page = page.child(setting_group(
            &icons::USER_PLUS,
            t!("create.who_can_join"),
            t!("room_settings.join_hint"),
            choices,
            p,
        ));
    }
    let mut switches = col().w_full().gap(4.0);
    let mut any_switch = false;
    if rights.guest_access {
        let on = settings.guest_access;
        any_switch = true;
        switches = switches.child(
            switch_row(t!("room_settings.guests"), Some(t!("room_settings.guests_hint")), on, p)
                .key("room-settings-guests")
                .on_press(move |s: &mut State| s.change_room(RoomSetting::GuestAccess(!on))),
        );
    }
    if rights.addresses
        && let Some(published) = settings.published
    {
        any_switch = true;
        switches = switches.child(
            switch_row(t!("create.publish"), Some(t!("create.publish_hint")), published, p)
                .key("room-settings-publish")
                .on_press(move |s: &mut State| s.change_room(RoomSetting::Published(!published))),
        );
    }
    if any_switch {
        page = page.child(setting_group(
            &icons::GLOBE,
            t!("room_settings.visibility"),
            t!("room_settings.visibility_hint"),
            switches,
            p,
        ));
    }
    if rights.history && !settings.space {
        let choices = [
            (HistoryVisibility::WorldReadable, t!("history.anyone"), t!("history.anyone_hint")),
            (HistoryVisibility::Shared, t!("history.members"), t!("history.members_hint")),
            (HistoryVisibility::Invited, t!("history.invited"), t!("history.invited_hint")),
            (HistoryVisibility::Joined, t!("history.joined"), t!("history.joined_hint")),
        ]
        .into_iter()
        .fold(col().w_full().gap(6.0), |list, (history, title, hint)| {
            list.child(
                choice_row(title, Some(hint), settings.history == history, p)
                    .key(format!("room-settings-history-{}", history_id(history)))
                    .on_press(move |s: &mut State| s.change_room(RoomSetting::History(history))),
            )
        });
        page = page.child(setting_group(
            &icons::EYE_OFF,
            t!("room_settings.history"),
            t!("room_settings.history_hint"),
            choices,
            p,
        ));
    }
    page
}

/// One address with what can be done to it.
fn address_row(address: &str, main: bool, p: &Palette) -> El {
    let mut entry = row()
        .key(format!("address-{address}"))
        .w_full()
        .min_h(44.0)
        .px(12.0)
        .gap(10.0)
        .center_y()
        .radius(8.0)
        .border(1.0, p.divider)
        .child(
            text(address.to_owned()).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).truncate().selectable(true),
        );
    if !main {
        let id = address.to_owned();
        entry = entry.child(
            button(t!("room_settings.make_main"), ButtonKind::Link, true, p)
                .key(format!("address-main-{address}"))
                .on_press(move |s: &mut State| s.change_room(RoomSetting::MainAddress(id.clone()))),
        );
    }
    let id = address.to_owned();
    entry.child(
        icon_button(&icons::TRASH, t!("common.remove"), false, p.text_muted, p)
            .key(format!("address-remove-{address}"))
            .on_press(move |s: &mut State| s.change_room(RoomSetting::RemoveAddress(id.clone()))),
    )
}

/// The main address, the others, and a new one.
fn addresses(dialog: &RoomSettingsDialog, settings: &RoomSettings, p: &Palette) -> El {
    let main = match &settings.main_address {
        Some(address) => address_row(address, true, p),
        None => text(t!("room_settings.no_address")).key("no-address").font(font::BODY).color(p.text_muted),
    };
    let mut page = col().w_full().gap(28.0).child(setting_group(
        &icons::AT,
        t!("room_settings.main_address"),
        t!("room_settings.main_address_hint"),
        main,
        p,
    ));
    if !settings.addresses.is_empty() {
        let others = settings
            .addresses
            .iter()
            .fold(col().w_full().gap(6.0), |list, address| list.child(address_row(address, false, p)));
        page = page.child(setting_group(
            &icons::LINK,
            t!("room_settings.other_addresses"),
            t!("room_settings.other_addresses_hint"),
            others,
            p,
        ));
    }
    let input = field_input(&dialog.address, t!("create.address_placeholder"), p)
        .key("room-settings-address")
        .on_edit(|s: &mut State| s.room_settings_field(SettingsField::Address))
        .on_submit(|s: &mut State, _| s.add_room_address());
    let typed = crate::state::local_part(&dialog.address.text).is_some();
    let add = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(col().grow(1.0).min_w(0.0).child(address_field(&settings.server, "room-settings-address", input, p)))
        .child(
            button(t!("room_settings.add"), ButtonKind::Primary, typed, p)
                .key("room-settings-address-add")
                .on_press(State::add_room_address),
        );
    page.child(setting_group(
        &icons::PLUS,
        t!("room_settings.add_address"),
        &tf!("room_settings.add_address_hint", server = settings.server),
        add,
        p,
    ))
}

/// Turning on encryption, which can't be undone.
fn security(p: &Palette) -> El {
    let controls = button(t!("room_settings.encrypt"), ButtonKind::Danger, true, p)
        .key("room-settings-encrypt")
        .on_press(|s: &mut State| s.ask_room_settings(SettingsConfirm::Encrypt));
    setting_group(&icons::LOCK, t!("room_settings.encryption"), t!("room_settings.not_encrypted"), controls, p)
}

/// A permission's name.
fn permission_label(permission: Permission) -> &'static str {
    match permission {
        Permission::DefaultLevel => t!("permission.default_level"),
        Permission::SendMessages => t!("permission.send_messages"),
        Permission::Invite => t!("permission.invite"),
        Permission::Kick => t!("permission.kick"),
        Permission::Ban => t!("permission.ban"),
        Permission::RemoveMessages => t!("permission.remove_messages"),
        Permission::MentionRoom => t!("permission.mention_room"),
        Permission::ChangeSettings => t!("permission.change_settings"),
        Permission::Name => t!("permission.name"),
        Permission::Topic => t!("permission.topic"),
        Permission::Avatar => t!("permission.avatar"),
        Permission::Addresses => t!("permission.addresses"),
        Permission::Access => t!("permission.access"),
        Permission::LiveLocation => t!("permission.live_location"),
        Permission::Pins => t!("permission.pins"),
        Permission::Permissions => t!("permission.permissions"),
        Permission::Encryption => t!("permission.encryption"),
        Permission::Upgrade => t!("permission.upgrade"),
        Permission::SpaceRooms => t!("permission.space_rooms"),
    }
}

/// A level by its role's name, or as a number.
fn level_label(level: i64) -> String {
    match level {
        100 => t!("members.admin").to_owned(),
        50 => t!("members.moderator").to_owned(),
        0 => t!("members.member").to_owned(),
        level => tf!("room_settings.custom_level", level = level),
    }
}

/// The key of a permission's select.
pub fn permission_key(permission: Permission) -> String {
    format!("permission-{permission:?}")
}

/// Each permission with the role it needs; those above the user's level can't be changed.
fn permissions(s: &State, dialog: &RoomSettingsDialog, settings: &RoomSettings, p: &Palette) -> El {
    let own = settings.own_level;
    let mut list = col().w_full().gap(4.0);
    for shown in &settings.permissions {
        let (permission, level) = (shown.permission, shown.level);
        let locked = !shown.editable;
        let open = s.popover == Some(Popover::Select(SelectFor::Permission(permission)));
        let mut select = select_button(&level_label(level), None, open, p).key(permission_key(permission));
        select = if locked {
            select.opacity(0.55).focusable(false).help(t!("room_settings.locked").to_owned())
        } else {
            select.on_press(move |s: &mut State| s.open_select(SelectFor::Permission(permission)))
        };
        list = list.child(
            row()
                .key(format!("permission-row-{permission:?}"))
                .w_full()
                .min_h(48.0)
                .gap(12.0)
                .center_y()
                .child(text(permission_label(permission)).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).wrap())
                .child(col().w(200.0).child(select)),
        );
        if let Some((editing, edit)) = &dialog.custom
            && *editing == permission
        {
            let field = field_input(edit, t!("room_settings.level_placeholder"), p)
                .key("permission-custom-level")
                .on_edit(|s: &mut State| s.room_settings_field(SettingsField::CustomLevel))
                .on_submit(|s: &mut State, _| s.save_custom_level());
            let save = button(t!("common.save"), ButtonKind::Primary, s.custom_level().is_some(), p)
                .key("permission-custom-save")
                .on_press(State::save_custom_level);
            let cancel = button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("permission-custom-cancel")
                .on_press(State::cancel_custom_level);
            list = list.child(
                col()
                    .w_full()
                    .gap(6.0)
                    .pb(8.0)
                    .child(field_with_save(field, row().gap(8.0).child(cancel).child(save)))
                    .child(text(tf!("room_settings.level_hint", level = own)).font(font::META).color(p.text_muted)),
            );
        }
    }
    setting_group(
        &icons::SHIELD,
        t!("room_settings.permissions"),
        &tf!("room_settings.permissions_hint", level = own),
        list,
        p,
    )
}

/// The options of a permission's select: the roles up to the user's level, and a custom level.
pub fn permission_options(s: &State, permission: Permission, p: &Palette) -> El {
    let settings = match &s.modal {
        Some(Modal::RoomSettings(dialog)) => s.shown_room_settings(dialog),
        _ => None,
    };
    let own = settings.map_or(0, |settings| settings.own_level);
    let current = settings
        .and_then(|settings| settings.permissions.iter().find(|shown| shown.permission == permission))
        .map(|shown| shown.level);
    let mut options: Vec<El> = PRESET_LEVELS
        .into_iter()
        .filter(|level| *level <= own)
        .map(|level| {
            select_option(&level_label(level), None, current == Some(level), p)
                .key(format!("permission-level-{level}"))
                .on_press(move |s: &mut State| s.set_permission_level(permission, level))
        })
        .collect();
    let custom = current.is_some_and(|level| !PRESET_LEVELS.contains(&level));
    options.push(
        select_option(t!("room_settings.custom"), None, custom, p)
            .key("permission-level-custom")
            .on_press(move |s: &mut State| s.edit_custom_level(permission)),
    );
    select_menu(permission_key(permission), options, p)
}

/// The room version, and an upgrade when a newer one is offered.
fn advanced(settings: &RoomSettings, p: &Palette) -> El {
    let controls = match &settings.upgrade_to {
        Some(version) => row().w_full().child(
            button(&tf!("room_settings.upgrade", version = version), ButtonKind::Secondary, true, p)
                .key("room-settings-upgrade")
                .on_press(|s: &mut State| s.ask_room_settings(SettingsConfirm::Upgrade)),
        ),
        None => col().child(text(t!("room_settings.up_to_date")).font(font::BODY).color(p.text_muted).wrap()),
    };
    let hint = if settings.upgrade_to.is_some() { t!("room_settings.upgrade_hint") } else { "" };
    setting_group(
        &icons::PACKAGE,
        &tf!("room_settings.version", version = settings.version),
        hint,
        controls.key("room-settings-version"),
        p,
    )
}

// ----- a space's rooms -------------------------------------------------------------------------

/// A room's or subspace's name as the account knows it, or its ID.
fn child_name(s: &State, account: &AccountData, room_id: &str) -> String {
    account
        .room(room_id)
        .map(|room| super::room::display_name(room, &s.settings))
        .or_else(|| account.space(room_id).map(|space| space.name.clone()))
        .unwrap_or_else(|| room_id.to_owned())
}

fn space_rooms(s: &State, account: &AccountData, space_id: &str, p: &Palette) -> El {
    let Some(space) = account.space(space_id) else { return el() };
    let last = space.children.len().saturating_sub(1);
    let mut listed = col().key("space-rooms-list").w_full().gap(2.0);
    if space.children.is_empty() {
        listed = listed.child(text(t!("space_rooms.empty")).font(font::BODY).color(p.text_muted).py(8.0));
    }
    for (index, child) in space.children.iter().enumerate() {
        let joined = account.room(child).is_some() || account.space(child).is_some();
        let move_button = |glyph, label: &str, key: String, by: isize, enabled: bool| {
            let id = child.clone();
            let button = icon_button(glyph, label, false, p.text_muted, p).key(key);
            if enabled {
                button.on_press(move |s: &mut State| s.move_space_room(&id, by))
            } else {
                button.opacity(0.3).focusable(false)
            }
        };
        let id = child.clone();
        listed = listed.child(
            row()
                .key(format!("space-room-{child}"))
                .w_full()
                .h(44.0)
                .px(8.0)
                .gap(6.0)
                .center_y()
                .radius(8.0)
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .child(
                    text(child_name(s, account, child))
                        .font(font::BODY)
                        .color(if joined { p.text } else { p.text_muted })
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(move_button(
                    &icons::CHEVRON_UP,
                    t!("space_rooms.up"),
                    format!("space-room-up-{child}"),
                    -1,
                    index > 0,
                ))
                .child(move_button(
                    &icons::CHEVRON_DOWN,
                    t!("space_rooms.down"),
                    format!("space-room-down-{child}"),
                    1,
                    index < last,
                ))
                .child(
                    icon_button(&icons::CLOSE, t!("space_rooms.remove"), false, p.text_muted, p)
                        .key(format!("space-room-remove-{child}"))
                        .on_press(move |s: &mut State| s.remove_space_room(&id)),
                ),
        );
    }
    let candidates = addable_rooms(account, space_id);
    let mut adding = col().key("space-rooms-add").w_full().gap(2.0);
    if candidates.is_empty() {
        adding = adding.child(text(t!("space_rooms.nothing_to_add")).font(font::BODY).color(p.text_muted).py(8.0));
    }
    for room in candidates {
        let id = room.room_id.clone();
        adding = adding.child(
            row()
                .key(format!("space-candidate-{}", room.room_id))
                .w_full()
                .h(44.0)
                .px(8.0)
                .gap(10.0)
                .center_y()
                .child(
                    text(super::room::display_name(room, &s.settings))
                        .font(font::BODY)
                        .color(p.text)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(
                    button(t!("space_rooms.add"), ButtonKind::Secondary, true, p)
                        .key(format!("space-room-add-{}", room.room_id))
                        .on_press(move |s: &mut State| s.add_space_room(&id)),
                ),
        );
    }
    let content = col()
        .w_full()
        .gap(16.0)
        .child(listed)
        .child(text(t!("space_rooms.add_heading")).font(font::SECTION).color(p.text_muted))
        .child(adding);
    closable_dialog("space-rooms", &tf!("space_rooms.title", space = space.name), content, Vec::new(), p)
}

// ----- leaving a space -------------------------------------------------------------------------

fn leave_space(s: &State, account: &AccountData, space_id: &str, chosen: &[String], p: &Palette) -> El {
    let name = account.space(space_id).map(|space| space.name.clone()).unwrap_or_default();
    let children = joined_children(account, space_id);
    let mut content =
        col().w_full().gap(12.0).child(text(t!("leave_space.body")).font(font::BODY).color(p.text_muted).wrap());
    if !children.is_empty() {
        let every = children.iter().all(|id| chosen.iter().any(|c| c == id));
        let heading = row()
            .w_full()
            .center_y()
            .child(text(t!("leave_space.rooms")).font(font::SECTION).color(p.text).grow(1.0))
            .child(
                button(if every { t!("leave_space.none") } else { t!("leave_space.all") }, ButtonKind::Link, true, p)
                    .key("leave-space-all")
                    .on_press(State::toggle_leave_all),
            );
        let list = children.iter().fold(col().w_full().gap(2.0), |list, child| {
            let id = (*child).to_owned();
            let checked = chosen.iter().any(|c| c == child);
            list.child(
                pick_row(None, &child_name(s, account, child), checked, p)
                    .key(format!("leave-with-{child}"))
                    .on_press(move |s: &mut State| s.toggle_leave_with(&id)),
            )
        });
        content = content.child(heading).child(list);
    }
    dialog(
        "leave-space",
        &tf!("leave_space.title", space = name),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("leave-space-cancel")
                .on_press(State::close_modal),
            button(t!("space.leave"), ButtonKind::Danger, true, p).key("leave-space-ok").on_press(State::leave_space),
        ],
        p,
    )
}

// ----- a member's level ------------------------------------------------------------------------

fn member_level(s: &State, name: &str, edit: &rune::TextEdit, p: &Palette) -> El {
    let own = match &s.modal {
        Some(Modal::MemberLevel { account, room_id, .. }) => {
            s.accounts.get(account).and_then(|data| data.members.get(room_id)).map_or(0, |members| members.own_level)
        }
        _ => 0,
    };
    let field = field_input(edit, t!("room_settings.level_placeholder"), p)
        .key("member-level-field")
        .on_edit(State::modal_field)
        .on_submit(|s: &mut State, _| s.submit_member_level());
    let content = col()
        .w_full()
        .gap(10.0)
        .child(text(tf!("member_level.body", level = own)).font(font::BODY).color(p.text_muted).wrap())
        .child(field);
    dialog(
        "member-level",
        &tf!("member_level.title", name = name),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("member-level-cancel")
                .on_press(State::close_modal),
            button(t!("common.save"), ButtonKind::Primary, s.member_level().is_some(), p)
                .key("member-level-save")
                .on_press(State::submit_member_level),
        ],
        p,
    )
}

// ----- an upgraded room ------------------------------------------------------------------------

/// In the composer's place in a room that was replaced: why, and the way to the new room.
pub fn successor_banner(s: &State, room: &RoomSummary, p: &Palette) -> Option<El> {
    let successor = room.successor.as_ref()?;
    let following = s.nav.account.as_ref().is_some_and(|account| s.following_successor(account, &room.room_id));
    let mut words = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(2.0)
        .child(text(t!("tombstone.title")).key("tombstone-title").font(font::BODY_STRONG).color(p.text).wrap());
    if let Some(reason) = successor.reason.as_deref().filter(|reason| !reason.trim().is_empty()) {
        words =
            words.child(text(reason.to_owned()).key("tombstone-reason").font(font::BODY).color(p.text_muted).wrap());
    }
    let label = if following { t!("tombstone.joining") } else { t!("tombstone.go") };
    let mut go = button(label, ButtonKind::Primary, !following, p).key("tombstone-follow");
    if !following {
        go = go.on_press(State::follow_successor);
    }
    let banner = row()
        .key("tombstone")
        .w_full()
        .padding(16.0)
        .gap(14.0)
        .center_y()
        .radius(10.0)
        .fill(p.accent.alpha(0.08))
        .border(1.0, p.accent.alpha(0.4))
        .role("status")
        .child(icon(&icons::UPDATE, 22.0).color(p.accent))
        .child(words)
        .child(go);
    Some(col().w_full().px(20.0).pt(8.0).pb(20.0).child(banner))
}
