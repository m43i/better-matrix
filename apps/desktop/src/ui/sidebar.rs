//! The room list for the active account's current scope: Home or one space.

use crate::i18n::t;
use matrix_core::{NotificationMode, RoomKind, RoomSummary};
use rune::{El, ScrollSpec, col, el, icon, row, text};

use super::components::{NavEmphasis, media_avatar, nav_row};
use super::icons;
use super::theme::{Palette, font};
use crate::state::{
    AccountData, Dragged, DropTarget, Popover, ROOMS, Scope, SectionView, State, is_builtin, sections, wants_attention,
};

const WIDTH: f32 = 260.0;
/// The new-chat button's key, which its search anchors to.
const NEW_CHAT: &str = "sidebar-new-chat";
/// The create button's key, which its menu anchors to.
const CREATE: &str = "sidebar-create";

pub fn sidebar(s: &State, account: &AccountData, p: &Palette) -> El {
    let scope = s.active_scope();
    let title = match &scope {
        Scope::Home => t!("common.home").to_owned(),
        Scope::Space(id) => account.space(id).map_or_else(|| id.clone(), |space| space.name.clone()),
    };
    // The scope's name (which shows its overview), then new chat and the create menu.
    let action = |glyph: &'static rune::Icon, label: &str, key: &str, open: bool| {
        el().key(key.to_owned())
            .size(30.0, 30.0)
            .radius(8.0)
            .center()
            .fill(if open { p.sidebar_hover } else { rune::Color::TRANSPARENT })
            .hover(p.sidebar_hover)
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(label.to_owned())
            .help(label.to_owned())
            .child(icon(glyph, 18.0).color(p.sidebar_text))
    };
    let header = row()
        .key("sidebar-header")
        .w_full()
        .h(super::theme::HEADER_HEIGHT)
        .pl(8.0)
        .pr(10.0)
        .gap(2.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.sidebar_divider)
        .child(
            row()
                .key("sidebar-title")
                .grow(1.0)
                .min_w(0.0)
                .h(36.0)
                .px(8.0)
                .center_y()
                .radius(8.0)
                .hover(p.sidebar_hover)
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(t!("overview.show"))
                .on_press(State::show_overview)
                .child(text(title).font(font::WORKSPACE).color(p.sidebar_text).truncate()),
        )
        .child(
            action(&icons::CHAT, t!("create.chat"), NEW_CHAT, s.popover == Some(Popover::NewChat))
                .on_press(State::open_new_chat),
        )
        .child(
            action(&icons::PLUS, t!("create.menu"), CREATE, s.popover == Some(Popover::CreateMenu))
                .on_press(|s: &mut State| s.toggle_popover(Popover::CreateMenu)),
        );

    let open = s.active_room_id();
    let mut list = col().w_full().px(8.0).py(12.0).gap(12.0);
    list = list.children(super::room_discovery::invitations(s, account, p));
    if account.rooms.is_empty() {
        list = list.child(text(t!("sidebar.syncing")).font(font::NAV).color(p.sidebar_text_muted).px(12.0));
    }
    let target = s.dragging.as_ref().and_then(|drag| drag.target.as_ref());
    let dragged_section = match s.dragging.as_ref().map(|drag| &drag.item) {
        Some(Dragged::Section(id)) => Some(id.as_str()),
        _ => None,
    };
    // Where a dragged section would land, as a line between sections.
    let line = || row().key("section-drop").w_full().px(8.0).child(el().grow(1.0).h(2.0).radius(1.0).fill(p.accent));
    let mut place = 0;
    for view in sections(account, &account.sidebar, &scope) {
        // The built-in sections show once they hold something; the user's always do.
        if is_builtin(&view.id) && view.rooms.is_empty() {
            continue;
        }
        let dragged = dragged_section == Some(view.id.as_str());
        if !dragged {
            if target == Some(&DropTarget::Section { index: place }) {
                list = list.child(line());
            }
            place += 1;
        }
        let drop_at = match target {
            Some(DropTarget::Room { section, index }) if *section == view.id => Some(*index),
            _ => None,
        };
        list = list.child(section_block(s, &view, open, drop_at, p).opacity(if dragged { 0.35 } else { 1.0 }));
    }
    if matches!(target, Some(DropTarget::Section { index }) if *index >= place) {
        list = list.child(line());
    }
    list = list.child(
        row()
            .key("add-section")
            .w_full()
            .h(28.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .radius(6.0)
            .hover(p.sidebar_hover)
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .child(icon(&icons::PLUS, 14.0).color(p.sidebar_text_muted))
            .child(text(t!("sections.new")).font(font::NAV).color(p.sidebar_text_muted))
            .on_press(State::new_section),
    );

    col()
        .key("sidebar")
        .w(WIDTH)
        .h_full()
        .fill(p.sidebar_bg)
        .border_sides([0.0, 1.0, 0.0, 0.0], p.sidebar_divider)
        .child(header)
        .child(
            col()
                .key("room-list")
                .w_full()
                .grow(1.0)
                .shrink()
                .min_h(0.0)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(list),
        )
        .children(super::call::call_bar(s, p))
}

/// One section: its header (which collapses it; the user's own also have a menu), then its
/// rooms. While a room is dragged over it, it lights up, and a line shows where in the user's
/// own sections the room would land.
fn section_block(s: &State, view: &SectionView<'_>, open: Option<&str>, drop_at: Option<usize>, p: &Palette) -> El {
    let id = view.id.clone();
    let title = match (view.id.as_str(), &view.name) {
        (_, Some(name)) => name.clone(),
        (ROOMS, None) => t!("sidebar.rooms").to_owned(),
        _ => t!("sidebar.direct").to_owned(),
    };
    let toggle = id.clone();
    let mut header = row()
        .key(format!("section-header-{id}"))
        .w_full()
        .h(28.0)
        .px(6.0)
        .gap(4.0)
        .center_y()
        .radius(6.0)
        .hover(p.sidebar_hover)
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .expanded(!view.collapsed)
        .child(
            icon(if view.collapsed { &icons::CHEVRON_RIGHT } else { &icons::CHEVRON_DOWN }, 14.0)
                .color(p.sidebar_text_muted),
        )
        .child(text(title).font(font::SECTION).color(p.sidebar_text_muted).grow(1.0).truncate())
        .on_press(move |s: &mut State| s.toggle_section(&toggle))
        .on_drag({
            let id = id.clone();
            move |s: &mut State, gesture, _| s.drag_in_sidebar(Dragged::Section(id.clone()), gesture)
        });
    let menu = id.clone();
    header = header.on_secondary_press(move |s: &mut State, _, _| s.toggle_popover(Popover::SectionMenu(menu.clone())));
    let dragged = match s.dragging.as_ref().map(|drag| &drag.item) {
        Some(Dragged::Room(id)) => Some(id.as_str()),
        _ => None,
    };
    let mut block = col().key(format!("section-{id}")).w_full().gap(2.0).radius(8.0).child(header);
    if drop_at.is_some() {
        block = block.fill(p.accent.alpha(0.12)).border(1.0, p.accent.alpha(0.6));
    }
    // Where a dropped room goes, in the user's own sections; the built-in ones order by recency.
    let line = || row().w_full().px(8.0).child(el().grow(1.0).h(2.0).radius(1.0).fill(p.accent));
    let mut shown = 0;
    for room in &view.rooms {
        let selected = open == Some(room.room_id.as_str());
        if view.collapsed && !selected {
            continue;
        }
        if dragged != Some(room.room_id.as_str()) {
            if !is_builtin(&id) && drop_at == Some(shown) {
                block = block.child(line());
            }
            shown += 1;
        }
        block = block.child(room_row(room, selected, dragged == Some(room.room_id.as_str()), &s.media, p));
    }
    if !is_builtin(&id) && drop_at.is_some_and(|at| at >= shown) && !view.collapsed {
        block = block.child(line());
    }
    if view.rooms.is_empty() && !view.collapsed {
        block = block.child(text(t!("sections.empty")).font(font::META).color(p.sidebar_text_muted).px(12.0).py(4.0));
    }
    block.on_layout_tx(move |s: &mut State, rect, tx| {
        s.section_zone(&id, rect);
        tx.live_only();
    })
}

/// A room's row, built again only when something it shows changed: the sidebar of hundreds of
/// rooms then costs nearly nothing per commit.
fn room_row(room: &RoomSummary, selected: bool, dragged: bool, media: &crate::state::MediaCache, p: &Palette) -> El {
    let picture_ready = room.avatar.as_ref().is_some_and(|picture| media.image(picture).is_some());
    let version = (
        (&room.name, room.unread, room.mentions, room.notifications, room.kind, &room.dm_users),
        (&room.avatar, picture_ready, selected, dragged, format!("{:?}", p.sidebar_bg)),
    );
    rune::memo(format!("room-{}", room.room_id), version, || build_room_row(room, selected, dragged, media, p))
}

fn build_room_row(
    room: &RoomSummary,
    selected: bool,
    dragged: bool,
    media: &crate::state::MediaCache,
    p: &Palette,
) -> El {
    let emphasis = if selected {
        NavEmphasis::Selected
    } else if wants_attention(room) {
        NavEmphasis::Unread
    } else if room.notifications == NotificationMode::Mute {
        NavEmphasis::Muted
    } else {
        NavEmphasis::Normal
    };
    let status_color = if selected { p.sidebar_selected_text } else { p.sidebar_text_muted };
    let status = match room.notifications {
        NotificationMode::AllMessages => None,
        NotificationMode::MentionsOnly => Some(icon(&icons::AT, 14.0).color(status_color)),
        NotificationMode::Mute => Some(icon(&icons::BELL_OFF, 14.0).color(status_color)),
    };
    let leading = match room.kind {
        RoomKind::Group => el().w(18.0).center().child(text("#").font(font::NAV).color(p.sidebar_text_muted)),
        RoomKind::Direct => {
            media_avatar(room.dm_users.first().unwrap_or(&room.room_id), &room.name, room.avatar.as_ref(), media, 18.0)
        }
    };
    let (open, menu) = (room.room_id.clone(), room.room_id.clone());
    let (drag, zone) = (room.room_id.clone(), room.room_id.clone());
    nav_row(leading, &room.name, emphasis, status, room.mentions, p)
        .key(format!("room-{}", room.room_id))
        .opacity(if dragged { 0.35 } else { 1.0 })
        .on_press(move |s: &mut State| s.select_room(&open))
        .on_secondary_press(move |s: &mut State, _, _| s.toggle_room_menu(&menu))
        .on_drag(move |s: &mut State, gesture, _| s.drag_in_sidebar(Dragged::Room(drag.clone()), gesture))
        .on_layout_tx(move |s: &mut State, rect, tx| {
            s.row_zone(&zone, rect);
            tx.live_only();
        })
}

/// A section's menu: collapse or expand it; the user's own sections can also be renamed or
/// deleted (their rooms go back to the built-in sections).
pub fn section_menu(s: &State, account: &AccountData, id: &str, p: &Palette) -> El {
    use super::components::{Tone, menu_action, popover_panel};
    let collapsed = sections(account, &account.sidebar, &s.active_scope())
        .into_iter()
        .find(|view| view.id == id)
        .is_some_and(|view| view.collapsed);
    let toggle = id.to_owned();
    let label = if collapsed { t!("sections.expand") } else { t!("sections.collapse") };
    let glyph = if collapsed { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT };
    let mut panel = popover_panel(p).key("section-menu").w(220.0).child(
        menu_action(glyph, label, Tone::Normal, p).key("section-toggle").on_press(move |s: &mut State| {
            s.popover = None;
            s.toggle_section(&toggle);
        }),
    );
    if !is_builtin(id) {
        let (rename, delete) = (id.to_owned(), id.to_owned());
        panel = panel
            .child(
                menu_action(&icons::PENCIL, t!("sections.rename"), Tone::Normal, p)
                    .key("section-rename")
                    .on_press(move |s: &mut State| s.rename_section(&rename)),
            )
            .child(
                menu_action(&icons::TRASH, t!("sections.delete"), Tone::Danger, p)
                    .key("section-delete")
                    .on_press(move |s: &mut State| s.delete_section(&delete)),
            );
    }
    col().child(panel).anchored(format!("section-header-{id}"), rune::Side::Bottom, rune::Align::Start, 4.0)
}

/// What is dragged, under the pointer: the room's or section's name.
pub fn drag_ghost(drag: &crate::state::SidebarDrag, account: &AccountData, p: &Palette) -> El {
    let name = match &drag.item {
        Dragged::Room(id) => account.room(id).map_or_else(|| id.clone(), super::room::display_name),
        Dragged::Space(id) => account.space(id).map_or_else(|| id.clone(), |space| space.name.clone()),
        Dragged::Section(id) => match id.as_str() {
            ROOMS => t!("sidebar.rooms").to_owned(),
            crate::state::DIRECT => t!("sidebar.direct").to_owned(),
            _ => account
                .sidebar
                .scopes
                .values()
                .flat_map(|scope| &scope.sections)
                .find(|section| section.id == *id)
                .map_or_else(|| id.clone(), |section| section.name.clone()),
        },
    };
    let [x, y] = drag.point;
    row()
        .key("drag-ghost")
        .absolute(x + 12.0, y + 6.0)
        .h(30.0)
        .px(12.0)
        .center_y()
        .radius(6.0)
        .fill(p.sidebar_selected)
        .shadow(4.0, 16.0, rune::Color::BLACK.alpha(0.4))
        .child(text(name).font(font::NAV_STRONG).color(p.sidebar_selected_text))
}

/// Starting a direct chat: a search for people, each opening the chat with them.
pub fn new_chat_panel(s: &State, account: &AccountData, p: &Palette) -> El {
    let field = rune::text_input(&s.chat_search.field, font::BODY)
        .key("new-chat-search")
        .single_line()
        .placeholder(t!("create.chat_placeholder"), p.text_muted)
        .color(p.text)
        .w_full()
        .px(12.0)
        .py(8.0)
        .radius(8.0)
        .fill(p.bg)
        .border(1.0, p.divider)
        .autofocus()
        .on_edit(|s: &mut State| &mut s.chat_search.field);
    let me = account.summary.id.as_str();
    let mut list = col().w_full();
    if s.chat_search.query().is_empty() && !s.chat_search.people.is_empty() {
        list = list.child(text(t!("create.suggested")).font(font::SECTION).color(p.text_muted).px(8.0).pt(4.0).pb(4.0));
    }
    for person in s.chat_search.people.iter().filter(|person| person.id != me) {
        let id = person.id.clone();
        list = list.child(
            row()
                .key(format!("new-chat-{id}"))
                .w_full()
                .px(8.0)
                .py(6.0)
                .gap(10.0)
                .center_y()
                .radius(6.0)
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .child(media_avatar(&person.id, &person.name, person.avatar.as_ref(), &s.media, 28.0))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .child(text(person.name.clone()).font(font::BODY).color(p.text).truncate())
                        .child(text(person.id.clone()).font(font::META).color(p.text_muted).truncate()),
                )
                .on_press(move |s: &mut State| s.open_direct_chat(&id)),
        );
    }
    let panel = super::components::popover_panel(p).key("new-chat-panel").w(320.0).child(
        col()
            .w_full()
            .px(8.0)
            .pb(6.0)
            .gap(6.0)
            .child(field)
            .child(col().w_full().max_h(320.0).scroll(ScrollSpec { stick_bottom: false }).child(list)),
    );
    col().child(panel).anchored(NEW_CHAT, rune::Side::Bottom, rune::Align::Start, 6.0)
}

/// What can be made here: a section always; a room inside a space the user may add rooms to;
/// a space from Home.
pub fn create_menu(s: &State, account: &AccountData, p: &Palette) -> El {
    use super::components::{Tone, menu_action};
    let mut panel = super::components::popover_panel(p).key("create-menu").w(220.0).child(
        menu_action(&icons::BULLETS, t!("sections.new"), Tone::Normal, p)
            .key("create-section")
            .on_press(State::new_section),
    );
    match s.active_scope() {
        Scope::Space(id) => {
            if account.space(&id).is_some_and(|space| space.can_add_rooms) {
                panel = panel.child(
                    menu_action(&icons::PLUS, t!("create.room"), Tone::Normal, p).key("create-room").on_press(
                        move |s: &mut State| {
                            s.open_room_form(crate::state::RoomForm::NewRoom { space: Some(id.clone()) })
                        },
                    ),
                );
            }
        }
        Scope::Home => {
            panel = panel.child(
                menu_action(&icons::PLUS, t!("create.room"), Tone::Normal, p)
                    .key("create-room")
                    .on_press(|s: &mut State| s.open_room_form(crate::state::RoomForm::NewRoom { space: None })),
            );
            panel = panel.child(
                menu_action(&icons::HOME, t!("create.space"), Tone::Normal, p)
                    .key("create-space")
                    .on_press(|s: &mut State| s.open_room_form(crate::state::RoomForm::NewSpace)),
            );
        }
    }
    col().child(panel).anchored(CREATE, rune::Side::Bottom, rune::Align::End, 6.0)
}
