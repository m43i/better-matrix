//! The messages the user saved, from the top bar: a dialog listing them across the account's
//! rooms, most recently saved first. A search above the list finds a message by its text, its
//! sender or where it was sent; the filter button beside it narrows the list to a time span or
//! to rooms, direct chats or one conversation. A row opens its message where it was sent; its
//! bookmark button stops saving it.

use matrix_core::SavedMessage;
use rune::{Align, El, Side, col, row, text};

use super::components::{
    ButtonKind, button, dialog, dialog_with_bar, divider, field_input, filter_chip, icon_button, media_avatar,
    popover_panel, select_option, when,
};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, SavedIn, SavedList, SavedSince, State, saved_room_label, saved_rooms};

/// The filter button, which the filter menu hangs from.
pub const FILTER: &str = "saved-filter";
/// The filter menu's scrolling choices. A choice scrolls into view when the menu opens on it
/// and when it is chosen (the first filter adds Clear filters below, taking room from them).
pub const CHOICES: &str = "saved-filter-choices";

/// The dialog's size where the window has room for it; a smaller window shrinks it.
const WIDTH: f32 = 720.0;
const HEIGHT: f32 = 680.0;
/// Room kept between the dialog and the window's sides.
const MARGIN: f32 = 24.0;
/// Below this window height the search scrolls with the list, so the list keeps room to show.
const SHORT: f32 = 360.0;

/// The saved messages of `list.account`, which may not be the account on screen (a call
/// answered for another account switches it): every row acts on `list.account`'s own.
/// `window` is the window's size, which the dialog stays inside.
pub fn saved(s: &State, list: &SavedList, account: &AccountData, window: [f32; 2], p: &Palette) -> El {
    let close =
        button(t!("common.close"), ButtonKind::Secondary, true, p).key("saved-close").on_press(State::close_modal);
    let (bar, messages) = (bar(s, list, p), messages(s, list, account, p));
    let dialog = if window[1] < SHORT {
        dialog("saved-dialog", t!("saved.title"), col().w_full().gap(14.0).child(bar).child(messages), vec![close], p)
    } else {
        dialog_with_bar("saved-dialog", t!("saved.title"), bar, messages, vec![close], p).h(HEIGHT)
    };
    dialog.w(WIDTH.min(window[0] - 2.0 * MARGIN).max(280.0))
}

/// The search and filter button, then a chip for each filter that applies.
fn bar(s: &State, list: &SavedList, p: &Palette) -> El {
    let open = list.filters_open;
    let filters = icon_button(&icons::FILTER, t!("saved.filters"), open || list.filter.active(), p.text_muted, p)
        .key(FILTER)
        .size(42.0, 42.0)
        .radius(8.0)
        .border(1.0, if list.filter.active() { p.accent } else { p.divider })
        .expanded(open)
        .help(t!("saved.filters"))
        .on_press_tx(|s: &mut State, tx| {
            s.toggle_saved_filters();
            // The menu opens on the chosen time span, which a short window may leave below its
            // fold.
            if let Some(list) = s.saved_list().filter(|list| list.filters_open) {
                tx.scroll_into_view_in(CHOICES, since_key(list.filter.since));
            }
        });
    let search = field_input(&list.search, t!("saved.search"), p)
        .key("saved-search")
        .label(t!("saved.search"))
        .on_edit(State::modal_field);
    let mut bar = col()
        .w_full()
        .gap(10.0)
        .child(row().w_full().gap(8.0).center_y().child(search.grow(1.0).w(0.0)).child(filters));
    let mut chips = Vec::new();
    // A chip for each filter that applies; pressing it removes the filter.
    if list.filter.since != SavedSince::AnyTime {
        let label = since_label(list.filter.since);
        chips.push(
            filter_chip(label, true, p)
                .key("saved-chip-since")
                .label(tf!("saved.remove_filter", filter = label))
                .on_press(|s: &mut State| s.filter_saved_since(SavedSince::AnyTime)),
        );
    }
    if let Some(label) = s.accounts.get(&list.account).and_then(|account| place_label(account, &list.filter.place)) {
        chips.push(
            filter_chip(&label, true, p)
                .key("saved-chip-in")
                .label(tf!("saved.remove_filter", filter = label))
                .on_press(|s: &mut State| s.filter_saved_in(SavedIn::Anywhere)),
        );
    }
    if !chips.is_empty() {
        bar = bar.child(row().key("saved-chips").w_full().gap(6.0).wrap().children(chips));
    }
    bar
}

/// The list itself, or why it's empty.
fn messages(s: &State, list: &SavedList, account: &AccountData, p: &Palette) -> El {
    let mut column = col().w_full();
    if let Some(error) = &account.saved_error {
        let failed = tf!("saved.failed", error = error);
        column = column.child(
            col().w_full().py(8.0).gap(8.0).child(text(failed).font(font::BODY).color(p.danger).wrap()).child(
                button(t!("saved.retry"), ButtonKind::Secondary, true, p)
                    .key("saved-retry")
                    .on_press(State::retry_saved),
            ),
        );
    }
    let shown = list.shown(account, crate::state::now_ms());
    match &account.saved {
        None if account.saved_error.is_none() => {
            column = column.child(text(t!("info.loading")).font(font::BODY).color(p.text_muted).py(8.0));
        }
        Some(_) if account.saved_messages().next().is_none() => {
            column = column.child(text(t!("saved.empty")).font(font::BODY).color(p.text_muted).py(8.0).wrap());
        }
        Some(_) if shown.is_empty() => {
            column = column.child(
                col()
                    .key("saved-no-matches")
                    .w_full()
                    .py(8.0)
                    .gap(8.0)
                    .items_start()
                    .child(text(t!("saved.no_matches")).font(font::BODY).color(p.text_muted).wrap())
                    .child(
                        button(t!("saved.clear"), ButtonKind::Link, true, p)
                            .key("saved-clear")
                            .on_press(State::show_all_saved),
                    ),
            );
        }
        _ => {}
    }
    for saved in shown {
        column = column.child(entry(s, &list.account, account, saved, p));
    }
    column
}

/// One saved message: who sent it where, its text and when; pressing it opens it.
fn entry(s: &State, owner: &matrix_core::AccountId, account: &AccountData, saved: &SavedMessage, p: &Palette) -> El {
    let message = &saved.preview;
    let room = saved_room_label(account, &saved.room_id);
    let (removed, room_id, event_id) = (owner.clone(), saved.room_id.clone(), message.event_id.clone());
    let header = row()
        .w_full()
        .min_h(28.0)
        .gap(6.0)
        .center_y()
        .child(text(message.sender_name.clone()).font(font::BODY_STRONG).color(p.text).shrink().truncate())
        .child(text(tf!("saved.in_room", room = room)).font(font::META).color(p.text_muted).grow(1.0).truncate())
        .child(
            icon_button(&icons::UNBOOKMARK, t!("actions.unsave"), false, p.text_muted, p)
                .key(format!("saved-remove-{room_id}-{event_id}"))
                .help(t!("actions.unsave"))
                .on_press(move |s: &mut State| s.set_message_saved(&removed, &room_id, &event_id, false)),
        );
    let mut body = col().grow(1.0).min_w(0.0).gap(3.0).child(header).child(
        text(message.body.clone().unwrap_or_else(|| t!("message.unavailable").to_owned()))
            .font(font::BODY)
            .color(if message.body.is_some() { p.text } else { p.text_muted })
            .max_lines(4)
            .wrap(),
    );
    if let Some(at) = message.timestamp_ms {
        body = body.child(text(when(at)).font(font::META).color(p.text_muted));
    }
    let mut entry = row()
        .key(format!("saved-open-{}-{}", saved.room_id, message.event_id))
        .w_full()
        .px(8.0)
        .py(10.0)
        .gap(10.0)
        .items_start()
        .radius(8.0)
        .child(
            media_avatar(&message.sender_id, &message.sender_name, message.sender_avatar.as_ref(), &s.media, 32.0)
                .mt(3.0),
        )
        .child(body);
    if message.body.is_some() {
        let (opened, room_id, thread, event_id) =
            (owner.clone(), saved.room_id.clone(), message.thread.clone(), message.event_id.clone());
        entry = entry
            .role("button")
            .label(t!("message.open"))
            .focusable(true)
            .cursor(rune::Cursor::Pointer)
            .hover(p.hover)
            .on_press_tx(crate::state::navigate(move |s| {
                s.open_saved_message(&opened, &room_id, thread.as_deref(), &event_id);
            }));
    }
    entry
}

/// The filter menu, under the filter button, while it is open: when the message was sent,
/// then where it was saved, each a group the arrow keys move through. A choice applies at once
/// and the menu stays open for another; Escape or a press outside closes it. It shrinks to fit
/// the window, scrolling its choices.
pub fn filters(s: &State, list: &SavedList, window: [f32; 2], p: &Palette) -> Option<El> {
    let account = s.accounts.get(&list.account)?;
    let filter = &list.filter;
    let heading = |label: &str| text(label.to_owned()).font(font::SECTION).color(p.text_muted).px(10.0).pt(6.0).pb(2.0);
    let group = |key: &str, label: &str, choices: Vec<El>| {
        col()
            .key(key.to_owned())
            .w_full()
            .gap(2.0)
            .role("listbox")
            .label(label.to_owned())
            .focus_group()
            .children(choices)
    };
    let times = SavedSince::ALL
        .into_iter()
        .map(|since| {
            select_option(since_label(since), None, filter.since == since, p)
                .key(since_key(since))
                .focus_selected(filter.since == since)
                .focus_highlight(p.hover.alpha(p.hover.a * 2.0))
                .on_press_tx(move |s: &mut State, tx| {
                    s.filter_saved_since(since);
                    tx.scroll_into_view_in(CHOICES, since_key(since));
                })
        })
        .collect();
    let mut options = col()
        .w_full()
        .padding(4.0)
        .gap(2.0)
        .child(heading(t!("saved.sent")))
        .child(group("saved-since", t!("saved.sent"), times))
        .child(divider(p))
        .child(heading(t!("saved.saved_in")));
    let mut places = vec![
        (SavedIn::Anywhere, t!("saved.anywhere").to_owned(), "saved-in-anywhere".to_owned()),
        (SavedIn::Rooms, t!("sidebar.rooms").to_owned(), "saved-in-rooms".to_owned()),
        (SavedIn::Direct, t!("sidebar.direct").to_owned(), "saved-in-direct".to_owned()),
    ];
    let mut rooms = saved_rooms(account);
    // A room chosen whose messages are no longer saved still shows, so the choice stays visible.
    if let SavedIn::Room(chosen) = &filter.place
        && !rooms.iter().any(|(id, _)| id == chosen)
    {
        rooms.push((chosen.clone(), saved_room_label(account, chosen)));
    }
    places
        .extend(rooms.into_iter().map(|(id, label)| (SavedIn::Room(id.clone()), label, format!("saved-in-room-{id}"))));
    let places = places
        .into_iter()
        .map(|(place, label, key)| {
            let selected = filter.place == place;
            select_option(&label, None, selected, p)
                .key(key.clone())
                .focus_selected(selected)
                .focus_highlight(p.hover.alpha(p.hover.a * 2.0))
                .on_press_tx(move |s: &mut State, tx| {
                    s.filter_saved_in(place.clone());
                    tx.scroll_into_view_in(CHOICES, key.clone());
                })
        })
        .collect();
    options = options.child(group("saved-in", t!("saved.saved_in"), places));
    // The panel shrinks to the room `anchored_fit` leaves it and scrolls its choices.
    let mut panel = popover_panel(p)
        .key("saved-filters")
        .w(300.0)
        .py(0.0)
        .max_h((window[1] - 16.0).max(120.0))
        .shrink()
        .min_h(0.0)
        .child(
            col()
                .key(CHOICES)
                .w_full()
                .shrink()
                .min_h(0.0)
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(options),
        );
    if filter.active() {
        panel = panel.child(
            row().w_full().px(8.0).pb(8.0).justify_end().child(
                button(t!("saved.clear_filters"), ButtonKind::Link, true, p)
                    .key("saved-clear-filters")
                    .on_press(State::clear_saved_filters),
            ),
        );
    }
    let menu = col()
        .key("saved-filters-layer")
        .min_h(0.0)
        .modal(true, State::close_saved_filters)
        .child(panel)
        .anchored_fit(FILTER, Side::Bottom, Align::End, 6.0);
    Some(menu)
}

fn since_label(since: SavedSince) -> &'static str {
    match since {
        SavedSince::AnyTime => t!("saved.any_time"),
        SavedSince::Today => t!("saved.today"),
        SavedSince::Week => t!("saved.week"),
        SavedSince::Month => t!("saved.month"),
        SavedSince::Year => t!("saved.year"),
    }
}

/// A time span's choice in the filter menu, by a stable ID.
fn since_key(since: SavedSince) -> String {
    let id = match since {
        SavedSince::AnyTime => "any",
        SavedSince::Today => "today",
        SavedSince::Week => "week",
        SavedSince::Month => "month",
        SavedSince::Year => "year",
    };
    format!("saved-since-{id}")
}

/// What a place filter shows on its chip; none for anywhere.
fn place_label(account: &AccountData, place: &SavedIn) -> Option<String> {
    match place {
        SavedIn::Anywhere => None,
        SavedIn::Rooms => Some(t!("sidebar.rooms").to_owned()),
        SavedIn::Direct => Some(t!("sidebar.direct").to_owned()),
        SavedIn::Room(room_id) => Some(saved_room_label(account, room_id)),
    }
}
