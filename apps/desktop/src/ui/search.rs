//! The top bar’s grouped search results, filters and fuzzy match highlights.

use super::components::{media_avatar, popover_panel};
use super::icons;
use super::people::person_row;
use super::room::display_name;
use super::theme::{Palette, font};
use crate::i18n::{t, tf, tp};
use crate::state::{AccountData, SearchFilter, SearchStatus, State};
use matrix_core::search::fuzzy_match;
use matrix_core::{DirectoryKind, LinkTarget, Permalink, Person, RoomKind, RoomSummary};
use rune::{Align, Color, El, ScrollSpec, Side, Span, col, el, icon, row, text};

const RESULTS: usize = 6;

pub fn highlighted(value: &str, query: &str, p: &Palette) -> El {
    let positions = matrix_core::search::fuzzy_match(value, query).map_or_else(Vec::new, |m| m.positions);
    highlights(value, &positions, p)
}

pub fn highlights(value: &str, positions: &[usize], p: &Palette) -> El {
    let spans = value
        .char_indices()
        .enumerate()
        .filter(|(i, _)| positions.binary_search(i).is_ok())
        .map(|(_, (byte, c))| Span::new(byte, byte + c.len_utf8()).color(p.accent).weight(700.0))
        .collect();
    rune::rich(value.to_owned(), spans).font(font::BODY).color(p.text).truncate()
}

/// The search's results under the field: a typed link or address to open, rooms, direct chats
/// and spaces on this device, then people from the user directory.
pub fn search_results(s: &State, account: &AccountData, p: &Palette, window: [f32; 2]) -> El {
    let query = s.search.query();
    let filter = s.search.filter;
    let mut list = col().w_full().px(6.0).pb(6.0);
    let mut any = false;
    if let Some(link) = Permalink::parse_typed(query).filter(|link| match link.target {
        LinkTarget::User(_) => filter.includes(SearchFilter::People),
        LinkTarget::Room { .. } => filter.includes(SearchFilter::Rooms) || filter.includes(SearchFilter::Spaces),
    }) {
        any = true;
        let target = match link.target {
            LinkTarget::User(id) | LinkTarget::Room { room: id, .. } => id,
        };
        let typed = query.to_owned();
        list = list.child(section(t!("search.link"), p)).child(
            row()
                .key("found-link")
                .w_full()
                .px(10.0)
                .py(6.0)
                .gap(10.0)
                .center_y()
                .radius(6.0)
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("button")
                .focusable(true)
                .cursor(rune::Cursor::Pointer)
                .child(el().size(28.0, 28.0).center().child(icon(&icons::LINK, 16.0).color(p.text_muted)))
                .child(
                    text(tf!("search.open_link", target = target)).font(font::BODY).color(p.text).grow(1.0).truncate(),
                )
                .on_press_tx(crate::state::navigate(move |s| s.open_typed_link(&typed))),
        );
    }
    let room_row = |room: &RoomSummary| {
        let id = room.room_id.clone();
        let mark = match room.kind {
            _ if room.note_to_self => note_mark(p),
            RoomKind::Group => el().size(28.0, 28.0).center().child(text("#").font(font::HEADER).color(p.text_muted)),
            RoomKind::Direct => media_avatar(
                room.dm_users.first().unwrap_or(&room.room_id),
                &room.name,
                room.avatar.as_ref(),
                &s.media,
                28.0,
            ),
        };
        row()
            .key(format!("found-room-{id}"))
            .w_full()
            .px(10.0)
            .py(6.0)
            .gap(10.0)
            .center_y()
            .radius(6.0)
            .hover(p.hover.alpha(p.hover.a * 2.0))
            .role("button")
            .focusable(true)
            .cursor(rune::Cursor::Pointer)
            .child(mark)
            .child(highlighted(&display_name(room), query, p).grow(1.0))
            .on_press_tx(crate::state::navigate(move |s| s.open_found_room(&id)))
    };
    // Typing its name offers the note to self until there is one, which is then found as a room.
    let offer_notes = !query.is_empty()
        && filter.includes(SearchFilter::Rooms)
        && !account.rooms.iter().any(|room| room.note_to_self)
        && fuzzy_match(t!("note_to_self.name"), query).is_some();
    for (kind, title) in [(RoomKind::Group, t!("search.rooms")), (RoomKind::Direct, t!("search.direct"))] {
        let mut found: Vec<_> = account
            .rooms
            .iter()
            .filter(|room| !query.is_empty() && filter.includes(SearchFilter::Rooms) && room.kind == kind)
            .filter_map(|room| fuzzy_match(&room.name, query).map(|matched| (room, matched.rank)))
            .collect();
        found.sort_by_key(|(room, rank)| (std::cmp::Reverse(account.search_usage.room(&room.room_id)), *rank));
        found.truncate(RESULTS);
        let notes = kind == RoomKind::Direct && offer_notes;
        if !found.is_empty() || notes {
            any = true;
            list = list.child(section(title, p));
            for (room, _) in found {
                list = list.child(room_row(room));
            }
        }
        if notes {
            list = list.child(
                row()
                    .key(NOTE_TO_SELF)
                    .w_full()
                    .px(10.0)
                    .py(6.0)
                    .gap(10.0)
                    .center_y()
                    .radius(6.0)
                    .hover(p.hover.alpha(p.hover.a * 2.0))
                    .role("button")
                    .focusable(true)
                    .cursor(rune::Cursor::Pointer)
                    .child(note_mark(p))
                    .child(highlighted(t!("note_to_self.name"), query, p).grow(1.0))
                    .on_press(State::open_note_to_self),
            );
        }
    }
    let mut spaces: Vec<_> = account
        .spaces
        .iter()
        .filter(|_| !query.is_empty() && filter.includes(SearchFilter::Spaces))
        .filter_map(|space| fuzzy_match(&space.name, query).map(|matched| (space, matched.rank)))
        .collect();
    spaces.sort_by_key(|(_, rank)| *rank);
    spaces.truncate(RESULTS);
    if !spaces.is_empty() {
        any = true;
        list = list.child(section(t!("search.spaces"), p));
        for (space, _) in spaces {
            let id = space.room_id.clone();
            list = list.child(
                person_row(
                    format!("found-space-{id}"),
                    &Person { id: space.room_id.clone(), name: space.name.clone(), avatar: space.avatar.clone() },
                    &s.media,
                    p,
                    query,
                )
                .on_press_tx(crate::state::navigate(move |s| s.open_found_space(&id))),
            );
        }
    }
    for (category, kind, title) in [
        (SearchFilter::Spaces, DirectoryKind::Spaces, t!("search.public_spaces")),
        (SearchFilter::Rooms, DirectoryKind::Rooms, t!("search.public_rooms")),
    ] {
        if !filter.includes(category) || query.is_empty() {
            continue;
        }
        let source = s.search.directory(kind);
        let rooms: Vec<_> = source
            .rooms
            .iter()
            .filter(|room| {
                !account.spaces.iter().any(|space| space.room_id == room.room_id)
                    && account.room(&room.room_id).is_none()
            })
            .take(RESULTS)
            .collect();
        if rooms.is_empty() && source.status == SearchStatus::Ready {
            continue;
        }
        any = true;
        list = list.child(section(title, p));
        for room in rooms {
            let room = room.clone();
            let name = room.name.as_deref().or(room.alias.as_deref()).unwrap_or(&room.room_id);
            let details = tp!("overview.members", room.member_count);
            list = list.child(
                row()
                    .key(format!("found-public-{}", room.room_id))
                    .w_full()
                    .px(10.0)
                    .py(6.0)
                    .gap(10.0)
                    .center_y()
                    .radius(6.0)
                    .hover(p.hover)
                    .role("button")
                    .focusable(true)
                    .cursor(rune::Cursor::Pointer)
                    .child(media_avatar(&room.room_id, name, room.avatar.as_ref(), &s.media, 28.0))
                    .child(
                        col()
                            .grow(1.0)
                            .min_w(0.0)
                            .child(highlighted(name, query, p))
                            .children(
                                room.alias
                                    .as_ref()
                                    .map(|alias| highlighted(alias, query, p).font(font::META).color(p.text_muted)),
                            )
                            .child(text(details).font(font::META).color(p.text_muted)),
                    )
                    .on_press_tx(crate::state::navigate(move |s| s.open_directory_result(&room))),
            );
        }
        if source.status != SearchStatus::Ready {
            let label = if source.status == SearchStatus::Failed {
                t!("search.directory_failed")
            } else {
                t!("search.searching")
            };
            list = list.child(text(label).font(font::META).color(p.text_muted).px(10.0).py(6.0));
        }
    }
    let me = account.summary.id.as_str();
    let mut people: Vec<&Person> =
        s.search.people.iter().filter(|person| filter.includes(SearchFilter::People) && person.id != me).collect();
    people.sort_by_key(|person| {
        (!person.id.eq_ignore_ascii_case(query), std::cmp::Reverse(account.search_usage.person(&person.id)))
    });
    people.truncate(RESULTS);
    if !people.is_empty() {
        any = true;
        list = list.child(section(t!("search.people"), p));
        for person in people {
            let id = person.id.clone();
            list = list.child(
                person_row(format!("found-person-{id}"), person, &s.media, p, query)
                    .on_press(move |s: &mut State| s.open_direct_chat(&id)),
            );
        }
    }
    let failed = s.search.message_status == SearchStatus::Failed;
    let searching = s.search.message_request().is_some();
    // Hits from people the user ignores don't show, those found before the ignore included.
    let mut messages: Vec<_> = s
        .search
        .messages
        .iter()
        .filter(|hit| account.room(&hit.room_id).is_some() && !account.ignores(&hit.sender))
        .collect();
    if filter.includes(SearchFilter::Messages) && (!messages.is_empty() || searching || failed) {
        any = true;
        list = list.child(section(t!("search.messages"), p));
        messages.sort_by_key(|hit| std::cmp::Reverse(account.search_usage.message(&hit.room_id, &hit.sender)));
        for hit in messages {
            let hit = hit.clone();
            let room = account.room(&hit.room_id).map_or_else(|| hit.room_id.clone(), display_name);
            let sender = account
                .members
                .get(&hit.room_id)
                .and_then(|members| members.people.iter().find(|m| m.person.id == hit.sender))
                .map_or(hit.sender.as_str(), |m| m.person.name.as_str());
            let meta = format!(
                "{room} · {sender} · {}",
                super::timeline::date_label(
                    super::components::local_ms(hit.timestamp_ms),
                    super::components::local_ms(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_millis() as u64)
                    )
                )
            );
            list = list.child(
                col()
                    .key(format!("found-message-{}-{}", hit.room_id, hit.event_id))
                    .w_full()
                    .px(10.0)
                    .py(8.0)
                    .gap(3.0)
                    .radius(6.0)
                    .hover(p.hover)
                    .role("button")
                    .focusable(true)
                    .cursor(rune::Cursor::Pointer)
                    .child(text(meta).font(font::META).color(p.text_muted).truncate())
                    .child(highlights(&hit.body, &hit.highlights, p))
                    .on_press_tx(crate::state::navigate(move |s| s.open_found_message(&hit))),
            );
        }
        if searching || failed {
            let label = if failed { t!("search.messages_failed") } else { t!("search.searching") };
            list = list.child(text(label).font(font::META).color(p.text_muted).px(10.0).py(6.0));
        }
    }
    if !any {
        let note = if query.is_empty() {
            t!("search.start")
        } else if s.search.waiting() || s.search.request.is_some() {
            t!("search.searching")
        } else {
            t!("search.nothing")
        };
        list = list.child(text(note).font(font::BODY).color(p.text_muted).px(10.0).py(10.0));
    }
    let panel = popover_panel(p)
        .key("search-results")
        .w(560.0_f32.min(window[0] - 24.0))
        .preserve_focus_on_press(true)
        .child(filter_row(filter, p))
        .child(
            col()
                .w_full()
                .max_h((window[1] - 112.0).clamp(60.0, 460.0))
                .scroll(ScrollSpec { stick_bottom: false })
                .child(list),
        );
    col().child(panel).anchored("search-anchor", Side::Bottom, Align::Center, 6.0)
}

/// The search's offer to make the note to self.
pub const NOTE_TO_SELF: &str = "found-note-to-self";

/// The note to self's mark in a result: its note, where a room has `#` and a chat a face.
fn note_mark(p: &Palette) -> El {
    el().size(28.0, 28.0).center().child(icon(&icons::NOTE, 18.0).color(p.text_muted))
}

fn filter_details(filter: SearchFilter) -> (&'static str, &'static str) {
    match filter {
        SearchFilter::All => ("all", t!("search.clear_filter")),
        SearchFilter::Spaces => ("spaces", t!("search.spaces")),
        SearchFilter::Rooms => ("rooms", t!("search.rooms")),
        SearchFilter::People => ("people", t!("search.people")),
        SearchFilter::Messages => ("messages", t!("search.messages")),
    }
}

fn filter_row(selected: SearchFilter, p: &Palette) -> El {
    row()
        .key("search-filters")
        .w_full()
        .px(10.0)
        .pt(4.0)
        .pb(10.0)
        .gap(6.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .children(
            [SearchFilter::Spaces, SearchFilter::Rooms, SearchFilter::People, SearchFilter::Messages].into_iter().map(
                |filter| {
                    let (key, label) = filter_details(filter);
                    let active = selected == filter;
                    row()
                        .key(format!("search-filter-{key}"))
                        .h(30.0)
                        .px(10.0)
                        .gap(6.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if active { p.accent.alpha(0.16) } else { Color::TRANSPARENT })
                        .border(1.0, if active { p.accent } else { p.divider })
                        .hover(p.hover)
                        .role("button")
                        .focusable(true)
                        .label(label)
                        .focus_selected(active)
                        .cursor(rune::Cursor::Pointer)
                        .child(text(label).font(font::BODY).color(if active { p.accent } else { p.text_muted }))
                        .children(active.then(|| icon(&icons::CLOSE, 12.0).color(p.accent)))
                        .on_press(move |s: &mut State| s.select_search_filter(filter))
                },
            ),
        )
}

pub fn active_filter(s: &State, p: &Palette) -> Option<El> {
    (s.search.filter != SearchFilter::All).then(|| {
        let (_, label) = filter_details(s.search.filter);
        row()
            .key("search-active-filter")
            .h(24.0)
            .px(7.0)
            .gap(6.0)
            .center_y()
            .radius(6.0)
            .fill(p.sidebar_divider)
            .preserve_focus_on_press(true)
            .child(text(label).font(font::BODY).color(p.sidebar_text))
            .child(
                el().key("search-clear-filter")
                    .size(16.0, 20.0)
                    .center()
                    .role("button")
                    .focusable(true)
                    .label(t!("search.clear_filter"))
                    .cursor(rune::Cursor::Pointer)
                    .child(icon(&icons::CLOSE, 12.0).color(p.sidebar_text_muted))
                    .on_press(|s: &mut State| s.select_search_filter(SearchFilter::All)),
            )
    })
}

fn section(title: &str, p: &Palette) -> El {
    text(title.to_owned()).font(font::SECTION).color(p.text_muted).px(10.0).pt(8.0).pb(4.0)
}

#[cfg(test)]
mod tests;
