//! The sidebar's sections for one scope: the built-in Rooms and Direct messages, which always
//! exist and list the latest conversation first, and the user's own, which hold the rooms and
//! chats dragged into them in the order they were placed. Also the order of the spaces on the
//! rail. Changes edit a [`SidebarLayout`]; the state saves it to the account.

use matrix_core::{RoomKind, RoomSummary, ScopeLayout, Section, SidebarLayout, SpaceSummary};

use super::Scope;
use super::accounts::AccountData;

/// The built-in sections' IDs: they can be collapsed, never removed.
pub const ROOMS: &str = "rooms";
/// Invitations keep this collapse key, but always precede the reorderable sections.
pub const INVITATIONS: &str = "invitations";
pub const DIRECT: &str = "direct";

/// Where a scope's layout is kept: `home`, or the space's room ID.
pub fn scope_key(scope: &Scope) -> String {
    match scope {
        Scope::Home => "home".to_owned(),
        Scope::Space(id) => id.clone(),
    }
}

/// One section as the sidebar lists it.
#[derive(Debug)]
pub struct SectionView<'a> {
    pub id: String,
    /// The user's name for it; `None` for a built-in section, which the view names.
    pub name: Option<String>,
    pub collapsed: bool,
    pub rooms: Vec<&'a RoomSummary>,
}

/// A scope's sections in order: the user's own, then Rooms and Direct messages with whatever
/// isn't in one of the user's.
pub fn sections<'a>(account: &'a AccountData, layout: &SidebarLayout, scope: &Scope) -> Vec<SectionView<'a>> {
    let mut out = unordered(account, layout, scope);
    let order = layout.scopes.get(&scope_key(scope)).map(|l| l.order.as_slice()).unwrap_or_default();
    // Named sections in the saved order; the rest keep their place after them.
    out.sort_by_key(|view| order.iter().position(|id| *id == view.id).unwrap_or(usize::MAX));
    out
}

/// The sections before ordering: the user's own as kept, then the built-in ones.
fn unordered<'a>(account: &'a AccountData, layout: &SidebarLayout, scope: &Scope) -> Vec<SectionView<'a>> {
    let scope_layout = layout.scopes.get(&scope_key(scope));
    let collapsed = |id: &str| scope_layout.is_some_and(|l| l.collapsed.iter().any(|c| c == id));
    let in_scope: Vec<&RoomSummary> =
        [RoomKind::Group, RoomKind::Direct].into_iter().flat_map(|kind| account.rooms_in(scope, kind)).collect();
    let mut placed = std::collections::HashSet::new();
    let mut out = Vec::new();
    for section in scope_layout.map(|l| l.sections.as_slice()).unwrap_or_default() {
        let rooms: Vec<&RoomSummary> =
            section.rooms.iter().filter_map(|id| in_scope.iter().find(|room| room.room_id == *id).copied()).collect();
        placed.extend(rooms.iter().map(|room| room.room_id.as_str()));
        out.push(SectionView {
            id: section.id.clone(),
            name: Some(section.name.clone()),
            collapsed: collapsed(&section.id),
            rooms,
        });
    }
    for (id, kind) in [(ROOMS, RoomKind::Group), (DIRECT, RoomKind::Direct)] {
        let mut rooms: Vec<&RoomSummary> = in_scope
            .iter()
            .filter(|room| room.kind == kind && !placed.contains(room.room_id.as_str()))
            .copied()
            .collect();
        // The latest conversation first; rooms with nothing known yet after them, by name.
        let now = super::accounts::now_ms();
        rooms.sort_by_cached_key(|room| (std::cmp::Reverse(account.activity(room, now)), room.name.to_lowercase()));
        out.push(SectionView { id: id.to_owned(), name: None, collapsed: collapsed(id), rooms });
    }
    out
}

fn scope_mut<'a>(layout: &'a mut SidebarLayout, scope: &Scope) -> &'a mut ScopeLayout {
    layout.scopes.entry(scope_key(scope)).or_default()
}

/// Collapses a section, or expands it again.
pub fn toggle_collapsed(layout: &mut SidebarLayout, scope: &Scope, id: &str) {
    let collapsed = &mut scope_mut(layout, scope).collapsed;
    match collapsed.iter().position(|c| c == id) {
        Some(index) => {
            collapsed.remove(index);
        }
        None => collapsed.push(id.to_owned()),
    }
}

/// Adds a section of the user's, first in the scope; returns its ID.
pub fn add_section(layout: &mut SidebarLayout, scope: &Scope, name: &str) -> String {
    let scope_layout = scope_mut(layout, scope);
    let id = (1..)
        .map(|n| format!("section-{n}"))
        .find(|id| !scope_layout.sections.iter().any(|s| s.id == *id))
        .unwrap_or_default();
    scope_layout.sections.insert(0, Section { id: id.clone(), name: name.trim().to_owned(), rooms: Vec::new() });
    if !scope_layout.order.is_empty() {
        scope_layout.order.insert(0, id.clone());
    }
    id
}

/// Moves a section to `index` among the scope's sections as shown (`shown`, in their current
/// order), built-in ones included.
pub fn move_section(layout: &mut SidebarLayout, scope: &Scope, shown: &[String], id: &str, index: usize) {
    let mut order: Vec<String> = shown.iter().filter(|known| *known != id).cloned().collect();
    order.insert(index.min(order.len()), id.to_owned());
    scope_mut(layout, scope).order = order;
}

/// The spaces in their rail order: those the layout names, in its order, then the rest as they
/// come (by name).
pub fn ordered_spaces<'a>(spaces: &'a [SpaceSummary], layout: &SidebarLayout) -> Vec<&'a SpaceSummary> {
    let mut ordered: Vec<&SpaceSummary> = spaces.iter().collect();
    ordered.sort_by_key(|space| layout.spaces.iter().position(|id| *id == space.room_id).unwrap_or(usize::MAX));
    ordered
}

/// Moves a space to `index` among the spaces `shown` on the rail, in their order.
pub fn move_space(layout: &mut SidebarLayout, shown: &[String], id: &str, index: usize) {
    let mut order: Vec<String> = shown.iter().filter(|known| *known != id).cloned().collect();
    order.insert(index.min(order.len()), id.to_owned());
    layout.spaces = order;
}

pub fn rename_section(layout: &mut SidebarLayout, scope: &Scope, id: &str, name: &str) {
    if let Some(section) = scope_mut(layout, scope).sections.iter_mut().find(|s| s.id == id) {
        section.name = name.trim().to_owned();
    }
}

/// Removes a section of the user's; its rooms go back to the built-in sections.
pub fn delete_section(layout: &mut SidebarLayout, scope: &Scope, id: &str) {
    let scope_layout = scope_mut(layout, scope);
    scope_layout.sections.retain(|s| s.id != id);
    scope_layout.collapsed.retain(|c| c != id);
    scope_layout.order.retain(|o| o != id);
}

/// Moves a room into a section at `index` among its rooms: one of the user's, or a built-in
/// one, which takes it out of the user's sections (it then shows under the built-in section of
/// its kind, in its place by recency).
pub fn move_room(layout: &mut SidebarLayout, scope: &Scope, room_id: &str, to: &str, index: usize) {
    let sections = &mut scope_mut(layout, scope).sections;
    for section in sections.iter_mut() {
        section.rooms.retain(|room| room != room_id);
    }
    if let Some(section) = sections.iter_mut().find(|s| s.id == to) {
        section.rooms.insert(index.min(section.rooms.len()), room_id.to_owned());
    }
}

/// Whether a section is one of the built-in ones.
pub fn is_builtin(id: &str) -> bool {
    id == ROOMS || id == DIRECT || id == INVITATIONS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_section_should_come_first_and_get_its_own_id() {
        let mut layout = SidebarLayout::default();
        let first = add_section(&mut layout, &Scope::Home, "Team");
        let second = add_section(&mut layout, &Scope::Home, "Friends");
        let names: Vec<_> = layout.scopes["home"].sections.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Friends", "Team"]);
        assert_ne!(first, second);
    }

    #[test]
    fn moving_a_room_should_take_it_out_of_its_old_section() {
        let mut layout = SidebarLayout::default();
        let a = add_section(&mut layout, &Scope::Home, "A");
        let b = add_section(&mut layout, &Scope::Home, "B");
        move_room(&mut layout, &Scope::Home, "!r", &a, 0);
        move_room(&mut layout, &Scope::Home, "!r", &b, 0);
        let held: Vec<_> = layout.scopes["home"].sections.iter().map(|s| s.rooms.len()).collect();
        assert_eq!(held, [1, 0]);
    }

    #[test]
    fn moving_a_room_to_a_built_in_section_should_free_it() {
        let mut layout = SidebarLayout::default();
        let a = add_section(&mut layout, &Scope::Home, "A");
        move_room(&mut layout, &Scope::Home, "!r", &a, 0);
        move_room(&mut layout, &Scope::Home, "!r", ROOMS, 0);
        assert!(layout.scopes["home"].sections[0].rooms.is_empty());
    }

    #[test]
    fn a_room_should_land_where_it_is_dropped() {
        let mut layout = SidebarLayout::default();
        let a = add_section(&mut layout, &Scope::Home, "A");
        for (room, index) in [("!x", 0), ("!y", 1), ("!z", 1)] {
            move_room(&mut layout, &Scope::Home, room, &a, index);
        }
        assert_eq!(layout.scopes["home"].sections[0].rooms, ["!x", "!z", "!y"]);
    }

    #[test]
    fn a_section_should_move_to_where_it_is_dropped() {
        let mut layout = SidebarLayout::default();
        let shown = ["mine".to_owned(), ROOMS.to_owned(), DIRECT.to_owned()];
        move_section(&mut layout, &Scope::Home, &shown, DIRECT, 0);
        assert_eq!(layout.scopes["home"].order, [DIRECT, "mine", ROOMS]);
    }

    fn space(id: &str) -> SpaceSummary {
        SpaceSummary { room_id: id.into(), name: id.into(), ..SpaceSummary::default() }
    }

    fn ids(spaces: &[&SpaceSummary]) -> Vec<String> {
        spaces.iter().map(|space| space.room_id.clone()).collect()
    }

    #[test]
    fn a_space_should_move_to_where_it_is_dropped_on_the_rail() {
        let mut layout = SidebarLayout::default();
        let shown = ["!a".to_owned(), "!b".to_owned(), "!c".to_owned()];
        move_space(&mut layout, &shown, "!c", 0);
        assert_eq!(layout.spaces, ["!c", "!a", "!b"]);
    }

    #[test]
    fn spaces_the_order_does_not_name_should_follow_by_name() {
        let layout = SidebarLayout { spaces: vec!["!c".into(), "!a".into()], ..SidebarLayout::default() };
        let spaces = [space("!a"), space("!b"), space("!c"), space("!d")];
        assert_eq!(ids(&ordered_spaces(&spaces, &layout)), ["!c", "!a", "!b", "!d"]);
    }

    #[test]
    fn sections_should_be_kept_per_scope() {
        let mut layout = SidebarLayout::default();
        add_section(&mut layout, &Scope::Space("!space".into()), "Ops");
        assert!(!layout.scopes.contains_key("home"));
        assert_eq!(layout.scopes["!space"].sections.len(), 1);
    }

    #[test]
    fn collapsing_twice_should_expand_again() {
        let mut layout = SidebarLayout::default();
        toggle_collapsed(&mut layout, &Scope::Home, DIRECT);
        toggle_collapsed(&mut layout, &Scope::Home, DIRECT);
        assert!(layout.scopes["home"].collapsed.is_empty());
    }

    #[test]
    fn deleting_a_section_should_forget_it() {
        let mut layout = SidebarLayout::default();
        let a = add_section(&mut layout, &Scope::Home, "A");
        toggle_collapsed(&mut layout, &Scope::Home, &a);
        delete_section(&mut layout, &Scope::Home, &a);
        assert!(layout.scopes["home"].sections.is_empty() && layout.scopes["home"].collapsed.is_empty());
    }
}
