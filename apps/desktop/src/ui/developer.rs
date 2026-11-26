//! Developer tools and widgets: the source dialog and the room's widgets panel.

use matrix_core::{EventSource, RoomSummary, Widget};
use rune::{El, col, el, icon, row, text};

use super::components::{ButtonKind, Tone, button, closable_dialog, divider, icon_button, menu_action, popover_panel};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf, tp};
use crate::state::{AccountData, Popover, SourceView, State};

/// An event's JSON, selectable, with a button that copies it.
pub fn source(view: &SourceView, p: &Palette) -> El {
    let (content, copy) = match view {
        SourceView::Loading(_) => (text(t!("info.loading")).font(font::BODY).color(p.text_muted), None),
        SourceView::Shown(source) => (shown(source, p), Some(copy_button(source, p))),
    };
    closable_dialog("source-dialog", t!("actions.view_source"), content, copy.into_iter().collect(), p).w(640.0)
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

/// The room's widgets, as the member list shows people: their count beside a plus that opens
/// what can be added (a meeting, a widget, bots and integrations), then one list, meetings first.
/// A press opens a widget: a meeting joins in the browser, any other opens in a window of its own.
pub fn widgets(s: &State, account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let listed = account.widgets.get(&room.room_id);
    let menu_open = s.popover == Some(Popover::WidgetsMenu);
    let count = match listed {
        Some(listed) => tp!("widgets.count", listed.widgets.len()),
        None => t!("info.loading").to_owned(),
    };
    let top = row()
        .w_full()
        .px(16.0)
        .pt(14.0)
        .pb(8.0)
        .gap(10.0)
        .center_y()
        .child(text(count).key("widget-count").font(font::SECTION).color(p.text_muted).grow(1.0).truncate())
        .child(
            el().key(WIDGETS_MENU)
                .size(38.0, 38.0)
                .radius(19.0)
                .center()
                .fill(p.hover.alpha(p.hover.a * 2.0))
                .hover(p.hover.alpha(p.hover.a * 3.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .expanded(menu_open)
                .label(t!("widgets.menu").to_owned())
                .help(t!("widgets.menu").to_owned())
                .child(icon(&icons::PLUS, 18.0).color(p.text_muted))
                .on_press(State::toggle_widgets_menu),
        );
    let mut list = col().w_full();
    match listed {
        Some(listed) if listed.widgets.is_empty() => {
            list = list.child(
                text(t!("widgets.none"))
                    .key("widgets-none")
                    .font(font::BODY)
                    .color(p.text_muted)
                    .px(16.0)
                    .py(8.0)
                    .wrap(),
            );
        }
        Some(listed) => {
            let (meetings, others): (Vec<&Widget>, Vec<&Widget>) =
                listed.widgets.iter().partition(|widget| widget.meeting.is_some());
            list = list
                .children(meetings.into_iter().map(|meeting| meeting_row(meeting, room, p)))
                .children(others.into_iter().map(|widget| widget_row(widget, room, p)));
        }
        None => {}
    }
    let content = col().w_full().child(top).child(divider(p)).child(list);
    super::info::frame("widgets-panel", t!("widgets.title"), content, None, p)
}

/// The panel's plus, which the menu of what can be added hangs from.
pub const WIDGETS_MENU: &str = "widgets-menu-button";

/// What can be added to the room shown: a meeting where the homeserver offers them, a widget
/// where the user may pin one, and bots and integrations.
pub fn widgets_menu(account: &AccountData, room: &RoomSummary, p: &Palette) -> El {
    let can_add = account.widgets.get(&room.room_id).is_some_and(|listed| listed.can_add);
    let room_id = room.room_id.clone();
    let panel = popover_panel(p)
        .key("widgets-menu")
        .w(260.0)
        .children(room.can_start_meeting.then(|| {
            menu_action(&icons::MEETING, t!("widgets.start_meeting"), Tone::Normal, p)
                .key("widget-start-meeting")
                .on_press(State::start_meeting)
        }))
        .children(can_add.then(|| {
            menu_action(&icons::LAYOUT, t!("widgets.add"), Tone::Normal, p)
                .key("widget-add")
                .on_press(move |s: &mut State| s.start_add_widget(&room_id))
        }))
        .child(
            menu_action(&icons::PACKAGE, t!("widgets.integrations_open"), Tone::Normal, p)
                .key("widget-integrations")
                .on_press(|s: &mut State| {
                    s.open_integrations();
                }),
        );
    col().child(panel).anchored(WIDGETS_MENU, rune::Side::Bottom, rune::Align::End, 6.0)
}

/// One widget's row, as plain as a member's: its picture, name and host, then `trailing`. The
/// caller adds its press.
fn row_with(widget: &Widget, glyph: &'static rune::Icon, trailing: Vec<El>, p: &Palette) -> El {
    row()
        .key(format!("widget-{}", widget.id))
        .w_full()
        .h(52.0)
        .px(16.0)
        .gap(12.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(widget.name.clone())
        .child(
            el().size(36.0, 36.0)
                .radius(18.0)
                .fill(p.hover.alpha(p.hover.a * 2.0))
                .center()
                .child(icon(glyph, 18.0).color(p.text_muted)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(1.0)
                .child(text(widget.name.clone()).font(font::BODY).color(p.text).truncate())
                .child(text(widget.host.clone()).font(font::META).color(p.text_muted).truncate()),
        )
        .children(trailing)
}

/// Remove, for a widget the user may take off the room.
fn remove_button(widget: &Widget, room: &RoomSummary, p: &Palette) -> Option<El> {
    widget.can_remove.then(|| {
        let (room_id, id, name) = (room.room_id.clone(), widget.id.clone(), widget.name.clone());
        icon_button(&icons::TRASH, t!("widgets.remove"), false, p.text_muted, p)
            .key(format!("widget-remove-{}", widget.id))
            .help(t!("widgets.remove"))
            .on_press(move |s: &mut State| s.ask_to_remove_widget(&room_id, &id, &name))
    })
}

/// A meeting: a press joins it in the browser, which its muted "Join" says.
fn meeting_row(meeting: &Widget, room: &RoomSummary, p: &Palette) -> El {
    let id = meeting.id.clone();
    let join = text(t!("call.join")).key(format!("widget-join-{}", meeting.id)).font(font::META).color(p.text_muted);
    let trailing = std::iter::once(join).chain(remove_button(meeting, room, p)).collect();
    row_with(meeting, &icons::MEETING, trailing, p)
        .help(tf!("widgets.join_meeting_named", name = meeting.name))
        .on_press(move |s: &mut State| {
            s.run_widget(&id, true);
        })
}

/// Any other widget: a press opens it in a window of its own, Open in browser there.
fn widget_row(widget: &Widget, room: &RoomSummary, p: &Palette) -> El {
    let (pressed, in_browser) = (widget.id.clone(), widget.id.clone());
    let open = icon_button(&icons::EXTERNAL, t!("widgets.open"), false, p.text_muted, p)
        .key(format!("widget-open-{}", widget.id))
        .help(t!("widgets.open"))
        .on_press(move |s: &mut State| {
            s.run_widget(&in_browser, true);
        });
    let trailing = std::iter::once(open).chain(remove_button(widget, room, p)).collect();
    row_with(widget, &icons::LAYOUT, trailing, p).on_press(move |s: &mut State| {
        s.run_widget(&pressed, false);
    })
}
