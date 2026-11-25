//! The confirmation dialog for actions that can't be undone.

use rune::{El, col, text};

use super::components::{ButtonKind, button, check_row, dialog};
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{Confirm, State};
use matrix_core::Moderation;

/// Title, explanation and confirm-button label for each confirmable action.
fn wording(confirm: &Confirm) -> (String, String, &'static str) {
    match confirm {
        Confirm::LeaveRoom { name, .. } => {
            (tf!("confirm.leave_title", room = name), t!("confirm.leave_body").to_owned(), t!("confirm.leave"))
        }
        Confirm::DeleteChat { name, .. } => {
            (tf!("confirm.delete_title", name = name), tf!("confirm.delete_body", name = name), t!("confirm.delete"))
        }
        Confirm::Moderate { name, action: Moderation::Kick, .. } => {
            (tf!("confirm.kick_title", name = name), t!("confirm.kick_body").to_owned(), t!("confirm.kick"))
        }
        Confirm::Moderate { name, action: Moderation::Ban, .. } => {
            (tf!("confirm.ban_title", name = name), t!("confirm.ban_body").to_owned(), t!("confirm.ban"))
        }
        Confirm::Moderate { name, action: Moderation::Role(_) | Moderation::Level(_), .. } => {
            (tf!("confirm.promote_title", name = name), t!("confirm.promote_body").to_owned(), t!("confirm.promote"))
        }
        // These happen at once, unasked; worded by what they do should one ever ask.
        Confirm::Moderate { name, action: Moderation::Unban, .. } => (name.clone(), String::new(), t!("members.unban")),
        Confirm::Moderate { name, action: Moderation::Admit, .. } => {
            (name.clone(), String::new(), t!("members.approve"))
        }
        Confirm::Moderate { name, action: Moderation::Deny, .. } => (name.clone(), String::new(), t!("members.deny")),
        Confirm::DeleteMessage { .. } => (
            t!("confirm.delete_message_title").to_owned(),
            t!("confirm.delete_message_body").to_owned(),
            t!("actions.delete"),
        ),
        Confirm::Ignore { name, decline: None, .. } => {
            (tf!("confirm.ignore_title", name = name), t!("confirm.ignore_body").to_owned(), t!("safety.ignore"))
        }
        Confirm::Ignore { name, decline: Some(_), .. } => (
            tf!("confirm.ignore_title", name = name),
            t!("confirm.ignore_inviter_body").to_owned(),
            t!("confirm.ignore_inviter"),
        ),
        Confirm::RemoveWidget { name, .. } => (
            tf!("confirm.remove_widget_title", name = name),
            t!("confirm.remove_widget_body").to_owned(),
            t!("widgets.remove"),
        ),
        // The question can be long: it goes in the body, which scrolls.
        Confirm::EndPoll { question, .. } => {
            (t!("confirm.end_poll_title").to_owned(), tf!("confirm.end_poll_body", question = question), t!("poll.end"))
        }
        Confirm::Deactivate { account, .. } => {
            (t!("deactivate.title").to_owned(), tf!("deactivate.body", account = account), t!("deactivate.confirm"))
        }
        Confirm::CopyToStickers { .. } => (
            t!("confirm.copy_sticker_title").to_owned(),
            t!("confirm.copy_sticker_body").to_owned(),
            t!("confirm.copy_sticker"),
        ),
    }
}

pub fn confirm(confirm: &Confirm, p: &Palette) -> El {
    let (title, body, action) = wording(confirm);
    let mut content = col().w_full().gap(12.0).child(text(body).font(font::BODY).color(p.text_muted).wrap());
    if let Confirm::Deactivate { erase, .. } = confirm {
        content = content.child(
            check_row(t!("deactivate.erase"), Some(t!("deactivate.erase_hint")), *erase, p)
                .key("deactivate-erase")
                .on_press(State::toggle_deactivate_erase),
        );
    }
    dialog(
        "confirm-dialog",
        &title,
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("confirm-cancel")
                .on_press(State::close_modal),
            button(action, ButtonKind::Danger, true, p).key("confirm-ok").on_press(State::confirm),
        ],
        p,
    )
}
