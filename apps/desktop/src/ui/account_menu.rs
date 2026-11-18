//! The menu behind the user's avatar: the account's status, every signed-in account, then
//! adding one, settings and signing out.

use rune::{Align, El, Side, col, el, icon, text};

use super::components::{account_avatar, divider, icon_button, menu_icon, menu_row, popover_panel, status_dot};
use super::icons;
use super::rail::ACCOUNT_BUTTON;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{Scope, State};
use matrix_core::{AccountId, Status};

pub fn account_menu(s: &State, p: &Palette) -> El {
    let mut panel = popover_panel(p).key("account-menu");
    if let Some(account) = s.active_account() {
        panel = status_rows(panel, &account.summary.id, account.status, p).child(divider(p));
    }
    panel = panel.child(text(t!("accounts.heading")).font(font::SECTION).color(p.text_muted).px(12.0).py(6.0));
    for account in s.accounts.iter() {
        let id = account.summary.id.clone();
        let active = s.nav.account.as_ref() == Some(&id);
        let trailing = if active {
            Some(icon(&icons::CHECK, 18.0).color(p.accent))
        } else if account.has_unread(&Scope::Home) {
            Some(el().size(8.0, 8.0).radius(4.0).fill(p.mention))
        } else {
            None
        };
        let row = menu_row(
            account_avatar(&account.summary, &s.media, 28.0),
            &account.summary.display_name,
            Some(id.as_str()),
            trailing,
            p,
        );
        panel = panel
            .child(row.key(format!("switch-{id}")).on_press_tx(crate::state::navigate(move |s| s.select_account(&id))));
    }
    // Accounts signed out softly: their data waits here; pressing one signs in again.
    for id in &s.suspended {
        let (resume, forget) = (id.clone(), id.clone());
        let remove = icon_button(&icons::TRASH, t!("session_ended.remove"), false, p.text_muted, p)
            .key(format!("forget-{id}"))
            .on_press(move |s: &mut State| s.forget_suspended(&forget));
        panel = panel.child(
            menu_row(menu_icon(&icons::USER, p), id.as_str(), Some(t!("accounts.signed_out")), Some(remove), p)
                .key(format!("resume-{id}"))
                .on_press_tx(crate::state::navigate(move |s| s.sign_in_as(&resume))),
        );
    }
    panel = panel
        .child(divider(p))
        .child(
            menu_row(menu_icon(&icons::PLUS, p), t!("accounts.add"), None, None, p)
                .key("menu-add-account")
                .on_press_tx(crate::state::navigate(State::start_adding_account)),
        )
        .children(s.active_account().map(|_| {
            menu_row(menu_icon(&icons::USER, p), t!("settings.profile"), None, None, p)
                .key("menu-profile")
                .on_press(State::open_profile)
        }))
        .child(
            menu_row(menu_icon(&icons::SETTINGS, p), t!("settings.title"), None, None, p)
                .key("menu-settings")
                .on_press(State::open_settings),
        );
    if let Some(account) = s.active_account() {
        let id = account.summary.id.clone();
        panel = panel.child(
            menu_row(
                menu_icon(&icons::SIGN_OUT, p),
                &tf!("accounts.sign_out", name = account.summary.display_name),
                None,
                None,
                p,
            )
            .key("menu-sign-out")
            .on_press(move |s: &mut State| s.sign_out(&id)),
        );
    }
    col().child(panel).anchored(ACCOUNT_BUTTON, Side::Right, Align::End, 10.0)
}

/// The status choices for the account on screen: online, away, hidden; a check marks the current.
fn status_rows(mut panel: El, id: &AccountId, current: Status, p: &Palette) -> El {
    panel = panel.child(text(t!("status.heading")).font(font::SECTION).color(p.text_muted).px(12.0).py(6.0));
    for (status, key, title, hint) in [
        (Status::Online, "online", t!("status.online"), None),
        (Status::Away, "away", t!("status.away"), None),
        (Status::Hidden, "hidden", t!("status.hidden"), Some(t!("status.hidden_hint"))),
    ] {
        let dot = el().size(28.0, 28.0).center().child(status_dot(status, 12.0, p.surface, p));
        let check = (status == current).then(|| icon(&icons::CHECK, 18.0).color(p.accent));
        let id = id.clone();
        panel = panel.child(
            menu_row(dot, title, hint, check, p)
                .key(format!("status-{key}"))
                .role("menuitemradio")
                .checked(status == current)
                .on_press(move |s: &mut State| s.set_status(&id, status)),
        );
    }
    panel
}
