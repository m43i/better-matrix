//! The menu behind the user's avatar: the account's status, every signed-in account, then
//! adding one, the account's saved messages, profile, settings and signing out.

use rune::{Align, El, Side, col, el, icon, row, text, text_input};

use super::components::{
    ButtonKind, account_avatar, button, divider, icon_button, menu_icon, menu_row, popover_panel, status_dot,
};
use super::icons;
use super::rail::ACCOUNT_BUTTON;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, EmojiFor, Popover, Scope, State, leading_emoji, status_suggestions};
use matrix_core::{AccountId, Status};

pub fn account_menu(s: &State, p: &Palette) -> El {
    let mut panel = popover_panel(p).key("account-menu");
    if let Some(account) = s.active_account() {
        // Writing a status message, the menu is only that: it stays short and on screen.
        if s.status_draft.as_ref() == Some(&account.summary.id) {
            return col().child(panel.child(status_message(s, account, p))).anchored(
                ACCOUNT_BUTTON,
                Side::Right,
                Align::End,
                10.0,
            );
        }
        panel = panel.child(status_message(s, account, p)).child(divider(p));
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
            menu_row(menu_icon(&icons::BOOKMARK, p), t!("saved.title"), None, None, p)
                .key("menu-saved")
                .on_press(State::open_saved)
        }))
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

/// The status message of the account on screen: the one set (pressed, it is written again; ×
/// clears it), or, while writing, a field with an emoji button, Save, and messages to pick.
fn status_message(s: &State, account: &AccountData, p: &Palette) -> El {
    let id = account.summary.id.clone();
    let writing = s.status_draft.as_ref() == Some(&id);
    let field = |focused: bool| {
        row()
            .h(36.0)
            .px(6.0)
            .gap(8.0)
            .center_y()
            .radius(8.0)
            .fill(p.bg)
            .border(1.0, if focused { p.accent } else { p.divider })
    };
    if !writing {
        let set = account.status_message();
        let leading = match set.and_then(leading_emoji) {
            Some(emoji) => el().size(24.0, 24.0).center().child(text(emoji.to_owned()).font(font::BODY)),
            None => el().size(24.0, 24.0).center().child(icon(&icons::SMILE, 18.0).color(p.text_muted)),
        };
        let words =
            set.map(|message| leading_emoji(message).map_or(message, |emoji| message[emoji.len()..].trim_start()));
        let shown = match words {
            Some(words) => text(words.to_owned()).color(p.text),
            None => text(t!("status_message.set")).color(p.text_muted),
        };
        let clear = set.map(|_| {
            let id = id.clone();
            icon_button(&icons::CLOSE, t!("status_message.clear"), false, p.text_muted, p)
                .key("status-message-clear")
                .on_press(move |s: &mut State| s.set_status_message(&id, ""))
        });
        return col().w_full().px(6.0).pt(6.0).pb(4.0).child(
            field(false)
                .key("status-message")
                .w_full()
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .label(set.unwrap_or(t!("status_message.set")).to_owned())
                .child(leading)
                .child(shown.font(font::BODY).grow(1.0).min_w(0.0).truncate())
                .children(clear)
                .on_press(State::edit_status_message),
        );
    }
    let input = text_input(&s.status_edit, font::BODY)
        .key("status-message-field")
        .single_line()
        .placeholder(t!("status_message.placeholder"), p.text_muted)
        .color(p.text)
        .grow(1.0)
        .min_w(0.0)
        .on_edit(|s: &mut State| &mut s.status_edit)
        .on_input_key(State::status_message_key)
        .on_submit(|s: &mut State, _| s.save_status_message());
    let emoji = icon_button(&icons::SMILE, t!("status_message.emoji"), false, p.text_muted, p)
        .key("status-message-emoji")
        .on_press(|s: &mut State| s.toggle_popover(Popover::Emoji(EmojiFor::StatusMessage)));
    let save = button(t!("status_message.save"), ButtonKind::Primary, true, p)
        .key("status-message-save")
        .on_press(State::save_status_message);
    let mut section = col()
        .w_full()
        .px(6.0)
        .pt(6.0)
        .gap(2.0)
        .preserve_focus_on_press(true)
        .child(field(true).w_full().child(emoji).child(input))
        .child(text(t!("status_message.suggestions")).font(font::SECTION).color(p.text_muted).px(6.0).pt(8.0).pb(2.0));
    for (key, emoji, words) in status_suggestions() {
        let leading = el().size(28.0, 28.0).center().child(text(emoji).font(font::BODY));
        let (id, chosen) = (id.clone(), format!("{emoji} {words}"));
        section = section.child(
            menu_row(leading, words, None, None, p)
                .key(format!("status-suggestion-{key}"))
                .on_press(move |s: &mut State| s.set_status_message(&id, &chosen)),
        );
    }
    section.child(row().w_full().px(6.0).pt(4.0).pb(6.0).justify_end().child(save))
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
