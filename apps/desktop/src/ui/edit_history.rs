//! The edit history dialog: every version of an edited message, newest first, each with when
//! it was written and its text as it read then (formatting, mentions and all).

use rune::{El, col, row, text};

use super::components::{ButtonKind, button, dialog, when};
use super::message_text::{Look, formatted};
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{EditHistory, State};

pub fn dialog_for(s: &State, history: &EditHistory, p: &Palette) -> El {
    let mut content = col().w_full().gap(12.0);
    match (&history.versions, &history.error) {
        (Some(versions), _) => {
            let count = versions.len();
            for (index, version) in versions.iter().enumerate().rev() {
                let label = match index {
                    0 => t!("edits.original"),
                    i if i + 1 == count => t!("edits.current"),
                    _ => t!("edits.earlier"),
                };
                let words = match &version.html {
                    Some(html) => {
                        let edit = rune::TextEdit::from_html(&matrix_core::html::to_pills(
                            html,
                            matrix_core::html::Pills::Display,
                        ));
                        let look = Look {
                            event_id: &version.event_id,
                            me: history.account.as_str(),
                            revealed: &s.revealed,
                            media: &s.media,
                            pictures: s.shows_pictures_of(&history.account, &history.room_id, &history.event_id),
                        };
                        formatted(&format!("edit-{}", version.event_id), &edit, look, p)
                    }
                    None => text(version.body.clone()).font(font::BODY).color(p.text).wrap_anywhere().selectable(true),
                };
                content = content.child(
                    col()
                        .key(format!("edit-version-{index}"))
                        .w_full()
                        .gap(4.0)
                        .padding(12.0)
                        .radius(8.0)
                        .fill(if index + 1 == count { p.accent.alpha(0.08) } else { p.bg })
                        .border(1.0, p.divider)
                        .child(
                            row()
                                .w_full()
                                .gap(8.0)
                                .child(text(label).font(font::BODY_STRONG).color(p.text))
                                .child(text(when(version.timestamp_ms)).font(font::META).color(p.text_muted)),
                        )
                        .child(words),
                );
            }
        }
        (None, Some(error)) => {
            content = content.child(text(error.clone()).font(font::BODY).color(p.danger).wrap());
        }
        (None, None) => content = content.child(text(t!("edits.loading")).font(font::BODY).color(p.text_muted)),
    }
    dialog(
        "edit-history",
        t!("edits.title"),
        content,
        vec![
            button(t!("common.close"), ButtonKind::Secondary, true, p)
                .key("edit-history-close")
                .on_press(State::close_modal),
        ],
        p,
    )
}
