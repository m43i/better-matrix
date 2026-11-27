//! Running widgets: what a widget asks to be allowed to do, and the integration manager's
//! terms. (Meetings are among the header's ways to meet by video, `ui::call`.)

use matrix_core::{PermissionKind, Policy, WidgetAccess, WidgetAccessKind, WidgetPermission, WidgetUrlField};
use rune::{El, col, icon, row, text};

use super::components::{ButtonKind, button, check_row, dialog};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{PromptKey, State, WidgetPermissions};

/// One permission in words, by what it allows and for what.
pub fn permission_text(permission: &WidgetPermission) -> String {
    let subject = permission.subject.as_deref().unwrap_or_default();
    match permission.kind {
        PermissionKind::ReadEvents => tf!("widgets.permission.read_events", subject = subject),
        PermissionKind::SendEvents => tf!("widgets.permission.send_events", subject = subject),
        PermissionKind::ReadState => tf!("widgets.permission.read_state", subject = subject),
        PermissionKind::SendState => tf!("widgets.permission.send_state", subject = subject),
        PermissionKind::ReadToDevice => tf!("widgets.permission.read_to_device", subject = subject),
        PermissionKind::SendToDevice => tf!("widgets.permission.send_to_device", subject = subject),
        PermissionKind::DelayedEvents => t!("widgets.permission.delayed_events").to_owned(),
        PermissionKind::DownloadFiles => t!("widgets.permission.download_files").to_owned(),
        PermissionKind::CallServices => t!("widgets.permission.call_services").to_owned(),
        PermissionKind::StayInApp => t!("widgets.permission.stay_in_app").to_owned(),
        PermissionKind::Identity => tf!("widgets.permission.identity", subject = subject),
        PermissionKind::Other => permission.capability.clone(),
    }
}

/// What a widget asks to be allowed to do, each ticked to grant it; Allow grants the ticked
/// ones, Deny none.
pub fn permissions(asked: &WidgetPermissions, p: &Palette) -> El {
    let key = asked.key();
    let id = key.id();
    let list = asked.permissions.iter().zip(&asked.granted).enumerate().fold(
        col().w_full().gap(2.0).children(asked.context.as_ref().map(|access| context(access, p))),
        |list, (index, (permission, granted))| {
            list.child(
                check_row(&permission_text(permission), None, *granted, p)
                    .key(format!("widget-permission-{id}-{index}"))
                    .on_press(move |s: &mut State| s.toggle_widget_permission(key, index)),
            )
        },
    );
    dialog(
        &format!("widget-permissions-{id}"),
        &tf!("widgets.permissions_title", widget = asked.widget),
        list,
        vec![
            button(t!("widgets.deny"), ButtonKind::Secondary, true, p)
                .key(format!("widget-permissions-deny-{id}"))
                .on_press(move |s: &mut State| s.answer_widget_permissions(key, false)),
            button(t!("widgets.allow"), ButtonKind::Primary, asked.granted.contains(&true), p)
                .key(format!("widget-permissions-allow-{id}"))
                .on_press(move |s: &mut State| s.answer_widget_permissions(key, true)),
        ],
        p,
    )
}

fn context(access: &WidgetAccess, p: &Palette) -> El {
    text(tf!("widgets.access_context", site = access.origin, room = access.room, user = access.user_id))
        .font(font::BODY)
        .color(p.text_muted)
        .wrap()
}

fn url_field(field: WidgetUrlField) -> &'static str {
    match field {
        WidgetUrlField::UserId => t!("widgets.field.user_id"),
        WidgetUrlField::RoomId => t!("widgets.field.room_id"),
        WidgetUrlField::DisplayName => t!("widgets.field.display_name"),
        WidgetUrlField::AvatarUrl => t!("widgets.field.avatar_url"),
        WidgetUrlField::DeviceId => t!("widgets.field.device_id"),
        WidgetUrlField::Homeserver => t!("widgets.field.homeserver"),
    }
}

/// The confirmation shown before loading a site, connecting to the provider, or resetting grants.
pub fn access(key: PromptKey, access: &WidgetAccess, p: &Palette) -> El {
    let (title, body, confirm) = match &access.kind {
        WidgetAccessKind::Open(fields) => (
            tf!("widgets.open_title", widget = access.name),
            if fields.is_empty() {
                t!("widgets.open_site").to_owned()
            } else {
                let fields = fields.iter().map(|field| url_field(*field)).collect::<Vec<_>>().join(", ");
                tf!("widgets.open_shares", fields = fields)
            },
            t!("common.continue"),
        ),
        WidgetAccessKind::Manager { hosted } => (
            t!("widgets.integrations_open").to_owned(),
            (if *hosted { t!("widgets.manager_hosted") } else { t!("widgets.manager_configured") }).to_owned(),
            t!("common.continue"),
        ),
        WidgetAccessKind::Saved(permissions) => (
            tf!("widgets.permissions_title", widget = access.name),
            if permissions.is_empty() {
                t!("widgets.no_saved_permissions").to_owned()
            } else {
                permissions.iter().map(permission_text).collect::<Vec<_>>().join("\n")
            },
            t!("widgets.reset_permissions"),
        ),
    };
    let id = key.id();
    dialog(
        &format!("widget-access-{id}"),
        &title,
        col().w_full().gap(12.0).child(context(access, p)).child(text(body).font(font::BODY).color(p.text).wrap()),
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key(format!("widget-access-cancel-{id}"))
                .on_press(move |s: &mut State| s.confirm_widget_access(key, false)),
            button(confirm, ButtonKind::Primary, true, p)
                .key(format!("widget-access-confirm-{id}"))
                .on_press(move |s: &mut State| s.confirm_widget_access(key, true)),
        ],
        p,
    )
}

/// The integration manager's terms, each a link to read; Agree opens the manager. Keyed by the
/// run they were shown for, like everything a press on them answers.
pub fn terms(key: PromptKey, policies: &[Policy], p: &Palette) -> El {
    let id = key.id();
    let list = policies.iter().enumerate().fold(col().w_full().gap(8.0), |list, (index, policy)| {
        let url = policy.url.clone();
        list.child(
            row()
                .key(format!("integration-policy-{id}-{index}"))
                .gap(8.0)
                .center_y()
                .role("link")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .label(policy.name.clone())
                .child(icon(&icons::EXTERNAL, 16.0).color(p.accent))
                .child(text(policy.name.clone()).font(font::BODY).color(p.accent))
                .on_press_tx(move |s: &mut State, tx| s.open_link(&url, tx)),
        )
    });
    let content = col()
        .w_full()
        .gap(12.0)
        .child(text(t!("widgets.terms_hint")).font(font::BODY).color(p.text_muted).wrap())
        .child(list);
    dialog(
        &format!("integration-terms-{id}"),
        t!("widgets.terms_title"),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key(format!("integration-terms-cancel-{id}"))
                .on_press(move |s: &mut State| s.decline_integration_terms(key)),
            button(t!("widgets.agree"), ButtonKind::Primary, true, p)
                .key(format!("integration-terms-agree-{id}"))
                .on_press(move |s: &mut State| s.agree_to_integration_terms(key)),
        ],
        p,
    )
}

#[cfg(test)]
mod tests;
