//! The room list for the active account's current scope: Home or one space.

use crate::i18n::t;
use matrix_core::{NotificationMode, PersonPresence, RoomKind, RoomSummary};
use rune::{El, ScrollSpec, col, el, icon, row, text};

use super::components::{NavEmphasis, badge, media_avatar, nav_row, with_presence};
use super::icons;
use super::theme::{Palette, font};
use crate::state::{
    AccountData, Dragged, DropTarget, FAVOURITES, LOW_PRIORITY, Popover, ROOMS, Scope, SectionView, State, is_builtin,
    sections, wants_attention,
};

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
                .on_press_tx(crate::state::navigate(State::show_overview))
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
    // The user's own notes, above everything.
    list = list.children(crate::state::note_to_self(account, &scope).map(|room| {
        let selected = open == Some(room.room_id.as_str());
        col().key(NOTE_TO_SELF_ROW).w_full().child(room_row(room, None, selected, false, s, p))
    }));
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
        .w_full()
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
    let title = section_title(view);
    let toggle = id.clone();
    let news = view.rooms.iter().any(|room| wants_attention(room));
    let mentions: u64 = view.rooms.iter().map(|room| room.mentions).sum();
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
        // Something new in a room sets the title in bold, and the section counts their mentions,
        // so both show while it is collapsed.
        .child(
            text(title)
                .font(if news { font::SECTION_STRONG } else { font::SECTION })
                .color(if news { p.sidebar_text } else { p.sidebar_text_muted })
                .grow(1.0)
                .truncate(),
        )
        .children(badge(mentions, p).map(|badge| badge.key(format!("section-mentions-{id}"))))
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
        let partner = dm_partner(room).and_then(|id| s.active_account().and_then(|account| account.presence.get(id)));
        block = block.child(room_row(room, partner, selected, dragged == Some(room.room_id.as_str()), s, p));
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

/// A section's name: the user's for their own, the catalog's for a built-in one.
fn section_title(view: &SectionView<'_>) -> String {
    match (view.id.as_str(), &view.name) {
        (_, Some(name)) => name.clone(),
        (ROOMS, None) => t!("sidebar.rooms").to_owned(),
        (FAVOURITES, None) => t!("sidebar.favourites").to_owned(),
        (LOW_PRIORITY, None) => t!("sidebar.low_priority").to_owned(),
        _ => t!("sidebar.direct").to_owned(),
    }
}

/// A room's row, built again only when something it shows changed: the sidebar of hundreds of
/// rooms then costs nearly nothing per commit.
fn room_row(
    room: &RoomSummary,
    partner: Option<&PersonPresence>,
    selected: bool,
    dragged: bool,
    s: &State,
    p: &Palette,
) -> El {
    let media = &s.media;
    let name = s.settings.room_name(room);
    let picture_ready = room.avatar.as_ref().is_some_and(|picture| media.image(picture).is_some());
    let version = (
        (name.as_ref(), room.unread, room.mentions, room.marked_unread, room.notifications, room.kind, &room.dm_users),
        room.note_to_self,
        partner,
        (&room.avatar, picture_ready, selected, dragged, colour_bits(p.sidebar_bg)),
    );
    rune::memo(format!("room-{}", room.room_id), version, || {
        build_room_row(room, &name, partner, selected, dragged, media, p)
    })
}

/// A colour as its exact bits, for a memo's version: the palette's identity, without formatting
/// it for every row.
fn colour_bits(colour: rune::Color) -> [u32; 4] {
    [colour.r.to_bits(), colour.g.to_bits(), colour.b.to_bits(), colour.a.to_bits()]
}

/// The one other person in a direct chat of two, when it is one: a chat marked direct with one
/// person may since have been joined by, or invited, more.
pub fn dm_partner(room: &RoomSummary) -> Option<&str> {
    match room.dm_users.as_slice() {
        [partner] if room.kind == RoomKind::Direct && room.active_member_count <= 2 => Some(partner),
        _ => None,
    }
}

fn build_room_row(
    room: &RoomSummary,
    name: &str,
    partner: Option<&PersonPresence>,
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
    // With nothing to say about notifications, a direct chat shows the emoji its person's
    // status message starts with; the whole message is on hover.
    let message = partner.and_then(|partner| partner.status_message.as_deref());
    let status = match room.notifications {
        NotificationMode::AllMessages => message.and_then(|message| {
            crate::state::leading_emoji(message).map(|emoji| {
                text(emoji.to_owned())
                    .font(font::NAV)
                    .key(format!("status-emoji-{}", room.room_id))
                    .help(message.to_owned())
            })
        }),
        NotificationMode::MentionsOnly => Some(icon(&icons::AT, 14.0).color(status_color)),
        NotificationMode::Mute => Some(icon(&icons::BELL_OFF, 14.0).color(status_color)),
    };
    let leading = match room.kind {
        _ if room.note_to_self => el().w(18.0).center().child(icon(&icons::NOTE, 16.0).color(p.sidebar_text_muted)),
        RoomKind::Group => el().w(18.0).center().child(text("#").font(font::NAV).color(p.sidebar_text_muted)),
        RoomKind::Direct => {
            let face =
                media_avatar(room.dm_users.first().unwrap_or(&room.room_id), name, room.avatar.as_ref(), media, 18.0);
            let ring = if selected { p.sidebar_selected } else { p.sidebar_bg };
            with_presence(face, 18.0, partner, ring, p)
        }
    };
    let (open, menu) = (room.room_id.clone(), room.room_id.clone());
    let row = nav_row(leading, name, emphasis, status, p)
        .key(format!("room-{}", room.room_id))
        .opacity(if dragged { 0.35 } else { 1.0 })
        .on_press_tx(crate::state::navigate(move |s| s.select_room(&open)))
        .on_secondary_press(move |s: &mut State, _, _| s.toggle_room_menu(&menu));
    // The note to self keeps its place above the sections: it is not dragged into one.
    if room.note_to_self {
        return row;
    }
    let (drag, zone) = (room.room_id.clone(), room.room_id.clone());
    row.on_drag(move |s: &mut State, gesture, _| s.drag_in_sidebar(Dragged::Room(drag.clone()), gesture)).on_layout_tx(
        move |s: &mut State, rect, tx| {
            s.row_zone(&zone, rect);
            tx.live_only();
        },
    )
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
pub fn drag_ghost(s: &State, drag: &crate::state::SidebarDrag, account: &AccountData, p: &Palette) -> El {
    let name = match &drag.item {
        Dragged::Room(id) => {
            account.room(id).map_or_else(|| id.clone(), |room| super::room::display_name(room, &s.settings))
        }
        Dragged::Space(id) => account.space(id).map_or_else(|| id.clone(), |space| space.name.clone()),
        Dragged::Section(id) => match id.as_str() {
            ROOMS => t!("sidebar.rooms").to_owned(),
            crate::state::DIRECT => t!("sidebar.direct").to_owned(),
            FAVOURITES => t!("sidebar.favourites").to_owned(),
            LOW_PRIORITY => t!("sidebar.low_priority").to_owned(),
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
        .on_edit(|s: &mut State| &mut s.chat_search.field)
        .on_input_key(State::popover_input_key);
    let me = account.summary.id.as_str();
    let mut list = col().w_full();
    // The user's own notes come first: before anything is typed, for their name, and for the
    // user's own ID, which the people found leave out.
    let typed = s.chat_search.query();
    if typed.is_empty()
        || typed.eq_ignore_ascii_case(me)
        || matrix_core::search::fuzzy_match(t!("note_to_self.name"), typed).is_some()
    {
        let mark = el().size(28.0, 28.0).center().child(icon(&icons::NOTE, 18.0).color(p.text_muted));
        list = list.child(
            new_chat_row(mark, t!("note_to_self.name").to_owned(), t!("note_to_self.hint").to_owned(), p)
                .key(NOTE_TO_SELF)
                .on_press(State::open_note_to_self),
        );
    }
    if s.chat_search.query().is_empty() && !s.chat_search.people.is_empty() {
        list = list.child(text(t!("create.suggested")).font(font::SECTION).color(p.text_muted).px(8.0).pt(4.0).pb(4.0));
    }
    for person in s.chat_search.people.iter().filter(|person| person.id != me) {
        let id = person.id.clone();
        let face =
            media_avatar(&person.id, &s.settings.person_name(&person.name), person.avatar.as_ref(), &s.media, 28.0);
        list = list.child(
            new_chat_row(face, s.settings.person_name(&person.name).into_owned(), person.id.clone(), p)
                .key(format!("new-chat-{id}"))
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

/// The sidebar's place for the note to self, above everything else.
pub const NOTE_TO_SELF_ROW: &str = "sidebar-note-to-self";

/// The new-chat search's row for the user's note to self.
pub const NOTE_TO_SELF: &str = "new-chat-note-to-self";

/// One choice in the new-chat search: a face, a name and a line under it.
fn new_chat_row(face: El, name: String, under: String, p: &Palette) -> El {
    row()
        .w_full()
        .px(8.0)
        .py(6.0)
        .gap(10.0)
        .center_y()
        .radius(6.0)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .child(face)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(name).font(font::BODY).color(p.text).truncate())
                .child(text(under).font(font::META).color(p.text_muted).truncate()),
        )
}

/// What can be made here: a section always; a room from Home or inside a space the user may
/// add rooms to. Spaces are created from the rail.
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
        }
    }
    col().child(panel).anchored(CREATE, rune::Side::Bottom, rune::Align::End, 6.0)
}
