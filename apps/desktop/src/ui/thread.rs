//! The thread panel, beside the room like Slack's: the thread's messages (its root first) and
//! its own composer. Replies sent here join the thread.

use matrix_core::{RoomSummary, TimelineKey};
use rune::{Cx, El, col, row, text};

use super::components::icon_button;
use super::composer::composer;
use super::icons;
use super::room::inline_name;
use super::theme::{Palette, font};
use super::timeline::{Context, timeline};
use crate::i18n::t;
use crate::state::{AccountData, ComposerTarget, State};

pub fn thread_panel(
    s: &State,
    account: &AccountData,
    room: &RoomSummary,
    root: &str,
    p: &Palette,
    ui: &mut Cx<State>,
) -> El {
    let key = TimelineKey::thread(&room.room_id, root);
    let cx = Context::new(s, account, room, &key);
    let header = row()
        .key("thread-header")
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .px(16.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(t!("thread.title")).font(font::HEADER).color(p.text))
                .child(text(inline_name(room)).font(font::META).color(p.text_muted).truncate()),
        )
        .child(
            icon_button(&icons::CLOSE, t!("thread.close"), false, p.text_muted, p)
                .key("thread-close")
                .on_press(State::close_thread),
        );
    col()
        .key("thread-panel")
        .w_full()
        .h_full()
        .fill(p.bg)
        .border_sides([0.0, 0.0, 0.0, 1.0], p.divider)
        .child(header)
        .child(timeline(&cx, account.timeline(&key), "thread-timeline", &s.thread_near_top, None, p))
        // A replaced room takes no more replies either: the way to the new one shows instead.
        .child(
            super::admin::successor_banner(s, room, p)
                .unwrap_or_else(|| composer(s, ComposerTarget::Thread, t!("thread.placeholder"), p, ui)),
        )
        .on_drop_files(|s: &mut State, files, _| s.attach_files_to(ComposerTarget::Thread, files))
}
