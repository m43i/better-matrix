//! File actions shared by message menus and the full-size attachment preview.

use matrix_core::{AccountId, Attachment, AttachmentKind};
use rune::{El, col};

use super::components::{Tone, menu_action, popover_panel};
use super::{icons, theme::Palette};
use crate::i18n::t;
use crate::state::{AttachmentAction, State, can_open_attachment};

pub fn actions(account: &AccountId, file: &Attachment, p: &Palette) -> Vec<El> {
    if file.media.is_none() {
        return Vec::new();
    }
    let mut actions =
        vec![(AttachmentAction::Download, &icons::DOWNLOAD, t!("preview.download"), "attachment-download")];
    if can_open_attachment(file) {
        actions.insert(0, (AttachmentAction::Open, &icons::EXTERNAL, t!("attachment.open"), "attachment-open"));
    }
    if file.kind == AttachmentKind::Image {
        actions.insert(
            0,
            (AttachmentAction::CopyImage, &icons::COPY, t!("attachment.copy_image"), "attachment-copy-image"),
        );
    }
    actions
        .into_iter()
        .map(|(action, glyph, label, key)| {
            let (account, file) = (account.clone(), file.clone());
            menu_action(glyph, label, Tone::Normal, p).key(key).on_press(move |s: &mut State| {
                s.act_on_attachment(account.clone(), &file, action);
            })
        })
        .collect()
}

pub fn menu(account: &AccountId, file: &Attachment, at: (i32, i32), window: [f32; 2], p: &Palette) -> El {
    col()
        .key("attachment-menu")
        .role("menu")
        .focus_group()
        .modal(true, State::close_popover)
        .absolute(
            (at.0 as f32).clamp(4.0, (window[0] - 268.0).max(4.0)),
            (at.1 as f32).clamp(4.0, (window[1] - 132.0).max(4.0)),
        )
        .child(popover_panel(p).w(260.0).children(actions(account, file, p)))
}
