//! The menu behind the user's avatar: the account's status; every signed-in account and adding
//! one; profile and settings; and, set apart in red, signing out. Saved messages open from the
//! top bar.

use rune::{Align, El, Side, col, el, icon, row, text, text_input};

use super::components::{
    ButtonKind, Tone, account_avatar, button, divider, field_input, icon_button, labelled, menu_action, menu_icon,
    menu_row, popover_panel, select_option, status_dot, when,
};
use super::icons;
use super::rail::ACCOUNT_BUTTON;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{
    AccountData, EmojiFor, Popover, Scope, State, StatusExpiry, StatusExpiryPicker, leading_emoji, status_suggestions,
};
use matrix_core::{AccountId, Status};

/// The menu, at most as tall as the `window` leaves room for: in a short window it scrolls, so
/// Settings and Sign out stay reachable.
pub fn account_menu(s: &State, window: [f32; 2], p: &Palette) -> El {
    let mut panel = popover_panel(p).key("account-menu");
    if let Some(account) = s.active_account() {
        // Writing a status message, the menu is only that: it stays short and on screen.
        if s.status_draft.as_ref() == Some(&account.summary.id) {
            let editor = panel.w(360.0).child(status_message(s, account, p));
            return col()
                .child(editor.max_h((window[1] - 16.0).max(100.0)).scroll(rune::ScrollSpec { stick_bottom: false }))
                .anchored(ACCOUNT_BUTTON, Side::Right, Align::End, 10.0);
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
            &s.settings.person_name(&account.summary.display_name),
            Some(id.as_str()),
            trailing,
            p,
        );
        panel = panel
            .child(row.key(format!("switch-{id}")).on_press_tx(crate::state::navigate(move |s| s.switch_account(&id))));
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
        .child(
            menu_row(menu_icon(&icons::PLUS, p), t!("accounts.add"), None, None, p)
                .key("menu-add-account")
                .on_press_tx(crate::state::navigate(State::start_adding_account)),
        )
        .child(divider(p))
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
        let title = tf!("accounts.sign_out", name = s.settings.person_name(&account.summary.display_name));
        panel = panel.child(divider(p)).child(
            menu_action(&icons::SIGN_OUT, &title, Tone::Danger, p)
                .key("menu-sign-out")
                .on_press(move |s: &mut State| s.sign_out(&id)),
        );
    }
    let panel = panel.max_h((window[1] - 16.0).max(100.0)).scroll(rune::ScrollSpec { stick_bottom: false });
    col().child(panel).anchored(ACCOUNT_BUTTON, Side::Right, Align::End, 10.0)
}

/// The status message of the account on screen: the one set (pressed, it is written again; ×
/// clears it), or, while writing, a field with an emoji button, Save, and messages to pick.
fn status_message(s: &State, account: &AccountData, p: &Palette) -> El {
    let id = account.summary.id.clone();
    let writing = s.status_draft.as_ref() == Some(&id);
    let field = || row().h(36.0).px(6.0).gap(8.0).center_y().radius(8.0).fill(p.bg).border(1.0, p.divider);
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
        return col()
            .w_full()
            .px(6.0)
            .pt(6.0)
            .pb(4.0)
            .child(
                field()
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
            )
            .children(account.status_message_clear_at().filter(|at| *at > crate::state::now_ms()).map(|at| {
                text(tf!("status_message.expires_at", time = when(at)))
                    .font(font::META)
                    .color(p.text_muted)
                    .px(8.0)
                    .pt(5.0)
            }));
    }
    let input = text_input(&s.status_edit, font::BODY)
        .key("status-message-field")
        .single_line()
        .placeholder(t!("status_message.placeholder"), p.text_muted)
        .label(t!("status_message.set"))
        .color(p.text)
        .grow(1.0)
        .min_w(0.0)
        .on_edit(|s: &mut State| &mut s.status_edit)
        .on_input_key(State::status_message_key)
        .on_submit(|s: &mut State, _| s.save_status_message());
    let emoji = icon_button(&icons::SMILE, t!("status_message.emoji"), false, p.text_muted, p)
        .key("status-message-emoji")
        .on_press(|s: &mut State| s.toggle_popover(Popover::Emoji(EmojiFor::StatusMessage)));
    let expiry = &s.status_expiry;
    let picking = expiry.picker != StatusExpiryPicker::Closed;
    let choice = expiry.choice.current(crate::state::now_ms());
    let current = match choice {
        StatusExpiry::At(at) => when(at),
        other => other.label().to_owned(),
    };
    let expiry_label = format!("{} · {current}", t!("status_message.expiry_label"));
    let calendar = icon_button(
        &icons::CALENDAR,
        &expiry_label,
        picking,
        if choice == StatusExpiry::Never { p.text_muted } else { p.accent },
        p,
    )
    .key("status-expiry")
    .help(expiry_label)
    .expanded(picking)
    .on_press(State::toggle_status_expiry);
    let ready = s.status_message_ready();
    let mut section = col()
        .w_full()
        .px(10.0)
        .py(6.0)
        .gap(8.0)
        .preserve_focus_on_press(true)
        .child(field().w_full().focus_within_ring(1.0, -1.0, p.accent).child(emoji).child(input).child(calendar));
    if picking {
        section = section.child(expiry_picker(s, choice, ready, p));
    } else {
        let mut suggestions = col()
            .w_full()
            .gap(2.0)
            .child(text(t!("status_message.suggestions")).font(font::SECTION).color(p.text_muted).px(6.0).pb(2.0));
        for (key, emoji, words) in status_suggestions() {
            let leading = el().size(26.0, 26.0).center().child(text(emoji).font(font::BODY));
            let chosen = format!("{emoji} {words}");
            suggestions = suggestions.child(
                menu_row(leading, words, None, None, p)
                    .key(format!("status-suggestion-{key}"))
                    .on_press(move |s: &mut State| s.choose_status_suggestion(&chosen)),
            );
        }
        section = section.child(suggestions);
    }
    section.child(
        row().w_full().justify_end().child(
            button(t!("status_message.save"), ButtonKind::Primary, ready, p)
                .key("status-message-save")
                .on_press(State::save_status_message),
        ),
    )
}

fn expiry_picker(s: &State, choice: StatusExpiry, ready: bool, p: &Palette) -> El {
    let expiry = &s.status_expiry;
    let label = text(t!("status_message.expiry_label")).font(font::SECTION).color(p.text).px(6.0).grow(1.0);
    let picker =
        col().key("status-expiry-picker").w_full().gap(6.0).padding(6.0).radius(8.0).fill(p.bg).border(1.0, p.divider);
    if expiry.picker == StatusExpiryPicker::Choices {
        picker.child(label.py(4.0)).child(
            col().w_full().focus_group().role("listbox").label(t!("status_message.expiry_label")).children(
                StatusExpiry::OPTIONS.into_iter().map(|option| {
                    let selected =
                        choice == option || (matches!(choice, StatusExpiry::At(_)) && option == StatusExpiry::Custom);
                    select_option(option.label(), None, selected, p)
                        .key(format!("status-expiry-{}", option.id()))
                        .on_press(move |s: &mut State| s.choose_status_expiry(option))
                }),
            ),
        )
    } else {
        let back = icon_button(&icons::CHEVRON_LEFT, t!("status_message.expiry_back"), false, p.text_muted, p)
            .key("status-expiry-back")
            .on_press(|s: &mut State| s.status_expiry.picker = StatusExpiryPicker::Choices);
        let time_label = t!("status_message.expiry_time");
        let date_label = t!("status_message.expiry_date");
        let time = field_input(&expiry.time, t!("status_message.expiry_time_placeholder"), p)
            .key("status-expiry-time")
            .focus_within_ring(1.0, -1.0, p.accent)
            .label(time_label)
            .on_edit(|s: &mut State| &mut s.status_expiry.time)
            .on_input_key(State::status_message_key)
            .on_submit(|s: &mut State, _| s.save_status_message());
        let date = field_input(&expiry.date, t!("status_message.expiry_date_placeholder"), p)
            .key("status-expiry-date")
            .focus_within_ring(1.0, -1.0, p.accent)
            .label(date_label)
            .on_edit(|s: &mut State| &mut s.status_expiry.date)
            .on_input_key(State::status_message_key)
            .on_submit(|s: &mut State, _| s.save_status_message());
        let hint = if !ready && !expiry.time.text.is_empty() {
            t!("status_message.expiry_time_invalid")
        } else {
            t!("status_message.expiry_time_hint")
        };
        picker
            .child(row().w_full().center_y().child(back).child(label))
            .child(
                row()
                    .w_full()
                    .gap(10.0)
                    .child(labelled(time_label, time, p).w(100.0))
                    .child(labelled(date_label, date, p).grow(1.0).min_w(0.0)),
            )
            .child(text(hint).font(font::META).color(p.text_muted).wrap())
    }
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
