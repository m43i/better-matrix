//! The confirmation dialog for actions that can't be undone.

use rune::{El, text};

use super::components::{ButtonKind, button, dialog};
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
        Confirm::Moderate { name, action: Moderation::Role(_), .. } => {
            (tf!("confirm.promote_title", name = name), t!("confirm.promote_body").to_owned(), t!("confirm.promote"))
        }
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
    }
}

pub fn confirm(confirm: &Confirm, p: &Palette) -> El {
    let (title, body, action) = wording(confirm);
    dialog(
        "confirm-dialog",
        &title,
        text(body).font(font::BODY).color(p.text_muted).wrap(),
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("confirm-cancel")
                .on_press(State::close_modal),
            button(action, ButtonKind::Danger, true, p).key("confirm-ok").on_press(State::confirm),
        ],
        p,
    )
}
