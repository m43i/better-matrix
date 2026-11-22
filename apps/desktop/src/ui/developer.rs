//! Developer tools and widgets: the source dialog and the room's widgets panel.

use matrix_core::{EventSource, RoomSummary};
use rune::{El, col, el, icon, row, text};

use super::components::{ButtonKind, button, dialog, icon_button};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, SourceView, State};

/// An event's JSON, selectable, with a button that copies it.
pub fn source(view: &SourceView, p: &Palette) -> El {
    let close =
        button(t!("common.close"), ButtonKind::Secondary, true, p).key("source-close").on_press(State::close_modal);
    let (content, copy) = match view {
        SourceView::Loading(_) => (text(t!("info.loading")).font(font::BODY).color(p.text_muted), None),
        SourceView::Shown(source) => (shown(source, p), Some(copy_button(source, p))),
    };
    dialog("source-dialog", t!("actions.view_source"), content, copy.into_iter().chain([close]).collect(), p).w(640.0)
}

fn shown(source: &EventSource, p: &Palette) -> El {
    let block = |label: &str, json: &str| {
        col().w_full().gap(6.0).child(text(label.to_owned()).font(font::SECTION).color(p.text_muted)).child(
            el().w_full()
                .padding(12.0)
                .radius(8.0)
                .fill(p.bg)
                .child(text(json.to_owned()).font(font::META.mono()).color(p.text).wrap().selectable(true)),
        )
    };
    let ids = tf!("source.ids", room = source.room_id, event = source.event_id);
    let mut content =
        col().w_full().gap(14.0).child(text(ids).font(font::META.mono()).color(p.text_muted).wrap().selectable(true));
    match &source.encrypted {
        Some(encrypted) => {
            content = content
                .child(block(t!("source.decrypted"), &source.json))
                .child(block(t!("source.encrypted"), encrypted));
        }
        None => content = content.child(block(t!("source.event"), &source.json)),
    }
    content
}

fn copy_button(source: &EventSource, p: &Palette) -> El {
    let json = source.json.clone();
    button(t!("source.copy"), ButtonKind::Secondary, true, p).key("source-copy").on_press_tx(
        move |s: &mut State, tx| {
            tx.copy(json.clone());
            s.notice = Some(t!("source.copied").to_owned());
        },
    )
}

/// The room's widgets: each opens in the browser; those allowed add and remove them.
pub fn widgets(account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let muted = |s: &str| text(s.to_owned()).font(font::BODY).color(p.text_muted).px(16.0).py(12.0).wrap();
    let listed = account.widgets.get(&room.room_id);
    let mut list = col().w_full();
    match listed {
        None => list = list.child(muted(t!("info.loading"))),
        Some(listed) if listed.widgets.is_empty() => list = list.child(muted(t!("widgets.none"))),
        Some(listed) => {
            for widget in &listed.widgets {
                let url = widget.url.clone();
                let mut entry = row()
                    .key(format!("widget-{}", widget.id))
                    .w_full()
                    .px(16.0)
                    .py(10.0)
                    .gap(12.0)
                    .center_y()
                    .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
                    .child(
                        el().size(36.0, 36.0)
                            .radius(8.0)
                            .fill(p.accent.alpha(0.12))
                            .center()
                            .child(icon(&icons::LAYOUT, 18.0).color(p.accent)),
                    )
                    .child(
                        col()
                            .grow(1.0)
                            .min_w(0.0)
                            .child(text(widget.name.clone()).font(font::BODY).color(p.text).truncate())
                            .child(text(widget.host.clone()).font(font::META).color(p.text_muted).truncate()),
                    )
                    .child(
                        icon_button(&icons::EXTERNAL, t!("widgets.open"), false, p.text_muted, p)
                            .key(format!("widget-open-{}", widget.id))
                            .help(t!("widgets.open"))
                            .on_press_tx(move |s: &mut State, tx| s.open_widget(&url, tx)),
                    );
                if widget.can_remove {
                    let (room_id, id, name) = (room.room_id.clone(), widget.id.clone(), widget.name.clone());
                    entry = entry.child(
                        icon_button(&icons::TRASH, t!("widgets.remove"), false, p.text_muted, p)
                            .key(format!("widget-remove-{}", widget.id))
                            .help(t!("widgets.remove"))
                            .on_press(move |s: &mut State| s.ask_to_remove_widget(&room_id, &id, &name)),
                    );
                }
                list = list.child(entry);
            }
        }
    }
    let footer = listed.filter(|listed| listed.can_add).map(|_| {
        let room_id = room.room_id.clone();
        col().w_full().padding(12.0).child(
            button(t!("widgets.add"), ButtonKind::Secondary, true, p)
                .key("widget-add")
                .on_press(move |s: &mut State| s.start_add_widget(&room_id)),
        )
    });
    let content = col()
        .w_full()
        .child(text(t!("widgets.hint")).font(font::META).color(p.text_muted).px(16.0).pt(12.0).wrap())
        .child(list);
    super::info::frame("widgets-panel", t!("widgets.title"), content, footer, p)
}
