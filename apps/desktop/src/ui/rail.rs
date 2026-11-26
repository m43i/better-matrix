//! The rail on the far left: Home and the active account's spaces at the top, the signed-in
//! user at the bottom, which opens the account menu. Home stays first; the spaces follow in the
//! user's order, and dragging one moves it among them. The plus after them creates a space.
//! A newer release shows an update button above the avatar.

use crate::i18n::{t, tf};
use rune::{El, ScrollSpec, col, el, icon, row, text};

use super::components::{account_avatar, media_avatar, status_dot, unread_dot};
use super::icons;
use super::theme::{Palette, font};
use crate::platform::updates::Upgrade;
use crate::state::{AccountData, Dragged, DropTarget, Popover, Scope, State, Update, ordered_spaces};

pub const WIDTH: f32 = 68.0;
const TILE: f32 = 38.0;
const GAP: f32 = 12.0;
const EDGE: f32 = 16.0;
const ACTION_GAP: f32 = 16.0;
const BLUR_EDGE: f32 = 32.0;
/// The key the account menu anchors to.
pub const ACCOUNT_BUTTON: &str = "account-button";
/// The key the update panel anchors to.
const UPDATE_BUTTON: &str = "rail-update";

pub fn rail(s: &State, account: Option<&AccountData>, p: &Palette) -> El {
    let update = s.update.release();
    let bottom = TILE + ACTION_GAP + EDGE + if update.is_some() { TILE + ACTION_GAP } else { 0.0 };
    let mut scopes = col().w_full().gap(GAP).items_center();
    if let Some(account) = account {
        let current = s.active_scope();
        let home = el()
            .size(TILE, TILE)
            .radius(TILE * 0.22)
            .fill(p.sidebar_hover)
            .center()
            .child(icon(&icons::HOME, 20.0).color(p.sidebar_text));
        let breathing = s.window_focused && s.breath.breathing();
        scopes = scopes.child(scope_slot(home, Scope::Home, t!("common.home"), &current, account, breathing, p));
        let (dragged, target) = match &s.dragging {
            Some(drag) => match &drag.item {
                Dragged::Space(id) => (Some(id.as_str()), drag.target.as_ref()),
                _ => (None, None),
            },
            None => (None, None),
        };
        // Where a dragged space would land: a short accent line between the tiles.
        let line = || el().key("space-drop").w(TILE).h(2.0).radius(1.0).fill(p.accent);
        let mut place = 0;
        for space in ordered_spaces(&account.spaces, &account.sidebar) {
            let is_dragged = dragged == Some(space.room_id.as_str());
            if !is_dragged {
                if target == Some(&DropTarget::Space { index: place }) {
                    scopes = scopes.child(line());
                }
                place += 1;
            }
            let scope = Scope::Space(space.room_id.clone());
            let tile = media_avatar(&space.room_id, &space.name, space.avatar.as_ref(), &s.media, TILE);
            let (drag, zone) = (space.room_id.clone(), space.room_id.clone());
            let slot = scope_slot(tile, scope, &space.name, &current, account, breathing, p)
                .opacity(if is_dragged { 0.35 } else { 1.0 })
                .on_drag(move |s: &mut State, gesture, _| s.drag_in_sidebar(Dragged::Space(drag.clone()), gesture))
                .on_layout_tx(move |s: &mut State, rect, tx| {
                    s.space_zone(&zone, rect);
                    tx.live_only();
                });
            scopes = scopes.child(slot);
        }
        if matches!(target, Some(DropTarget::Space { index }) if *index >= place) {
            scopes = scopes.child(line());
        }
        scopes = scopes.child(
            el().key("rail-space-actions")
                .w_full()
                .h(TILE)
                .center()
                .sticky_bottom(bottom)
                .child(
                    el().key("rail-space-blur")
                        .pin(Some(0.0), Some(-BLUR_EDGE), Some(0.0), None)
                        .h(BLUR_EDGE + TILE + bottom)
                        .clip_fade(0.0, BLUR_EDGE, 0.0, 0.0)
                        .stuck_reveal(8.0)
                        .child(el().w_full().h_full().backdrop_blur(8.0).linear_gradient(
                            std::f32::consts::PI,
                            [(0.0, p.rail_bg.alpha(0.0)), (0.25, p.rail_bg.alpha(0.95)), (1.0, p.rail_bg)],
                        )),
                )
                .child(
                    el().key("rail-create-space")
                        .size(TILE, TILE)
                        .radius(TILE * 0.22)
                        .center()
                        .hover(p.sidebar_hover)
                        .role("button")
                        .cursor(rune::Cursor::Pointer)
                        .label(t!("create.space"))
                        .help(t!("create.space"))
                        .focusable(true)
                        .child(icon(&icons::PLUS, 20.0).color(p.sidebar_text))
                        .on_press(|s: &mut State| s.open_room_form(crate::state::RoomForm::NewSpace)),
                ),
        );
    }

    let mut me = el().key(ACCOUNT_BUTTON).size(TILE, TILE);
    if let Some(account) = account {
        let open = s.popover == Some(Popover::AccountSwitcher);
        let face = account_avatar(&account.summary, &s.media, TILE);
        me = me
            .child(highlighted_tile(face, open, s.window_focused, p))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(tf!("rail.switch_account", name = account.summary.display_name))
            .focusable(true)
            .on_press(|s: &mut State| s.toggle_popover(Popover::AccountSwitcher))
            .child(status_dot(account.status, 14.0, p.rail_bg, p).absolute(TILE - 10.0, TILE - 10.0));
        if s.other_accounts_unread() {
            me = me.child(unread_dot(TILE, p.rail_bg, p));
        }
    }

    let mut rail = el()
        .key("rail")
        .w(WIDTH)
        .h_full()
        .fill(p.rail_bg)
        .child(
            col()
                .key("rail-scopes")
                .w_full()
                .h_full()
                .py(EDGE)
                .scroll(ScrollSpec { stick_bottom: false })
                .child(scopes.pb(bottom)),
        )
        .child(me.pin(Some((WIDTH - TILE) / 2.0), None, None, Some(EDGE)));
    if let Some(release) = update {
        let installing = matches!(s.update, Update::Installing(_));
        let label = if installing {
            t!("update.installing").to_owned()
        } else if matches!(s.update, Update::Ready(_)) {
            tf!("update.install", version = release.version)
        } else {
            tf!("update.available", version = release.version)
        };
        rail = rail.child(
            el().key(UPDATE_BUTTON)
                .size(TILE, TILE)
                .pin(Some((WIDTH - TILE) / 2.0), None, None, Some(EDGE + TILE + ACTION_GAP))
                .radius(TILE * 0.22)
                .center()
                .fill(p.accent.alpha(0.15))
                .hover(p.sidebar_hover)
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(label.clone())
                .help(label)
                .focusable(true)
                .disabled(installing)
                .child(icon(if installing { &icons::PACKAGE } else { &icons::UPDATE }, 20.0).color(p.accent))
                .on_press(State::install_update),
        );
    }
    rail
}

/// The ring around the selected entry and the open account button: a thin line in the accent
/// over a soft glow of it, which breathes slowly while the window has focus: a detail for
/// whoever looks, too slow to draw the eye. The glow is its own layer behind the tile, so the
/// loop fades only its opacity and never commits.
fn highlighted_tile(tile: El, highlighted: bool, breathing: bool, p: &Palette) -> El {
    // Starts and ends at full strength, as it rests (`state::breath`).
    const BREATH: &[(f32, f32)] = &[(0.0, 1.0), (0.5, 0.45), (1.0, 1.0)];
    let mut halo = el()
        .key("rail-halo")
        .absolute(0.0, 0.0)
        .size(TILE, TILE)
        .radius(TILE * 0.22)
        .fill(p.rail_bg)
        .opacity(if highlighted { 1.0 } else { 0.0 })
        .box_shadow(rune::Shadow { blur: 18.0, spread: 3.0, color: p.accent.alpha(0.6), ..rune::Shadow::default() });
    if highlighted && breathing {
        #[expect(clippy::cast_possible_truncation, reason = "a period of a few seconds")]
        let period = crate::state::breath::PERIOD as f32;
        halo = halo.cycle(rune::Cycle { period, phase: 0.0, stops: BREATH, still: 0.8, fps: 20.0 });
    }
    // Keep the tile mounted in the same place so its contents never crossfade with the highlight.
    let ring = if highlighted { p.accent.alpha(0.55) } else { rune::Color::TRANSPARENT };
    el().size(TILE, TILE).child(halo).child(tile.key("rail-tile").outline(1.0, 2.0, ring))
}

/// One rail entry: the tile, a ring when selected, a dot for unread activity elsewhere.
fn scope_slot(
    tile: El,
    scope: Scope,
    name: &str,
    current: &Scope,
    account: &AccountData,
    breathing: bool,
    p: &Palette,
) -> El {
    let selected = &scope == current;
    let scope_for_menu = scope.clone();
    let key = match &scope {
        Scope::Home => "scope-home".to_owned(),
        Scope::Space(id) => format!("scope-{id}"),
    };
    let mut slot = el().key(key).size(TILE, TILE).child(if selected {
        highlighted_tile(tile, true, breathing, p)
    } else {
        tile.opacity(0.8)
    });
    if !selected && account.has_unread(&scope) {
        slot = slot.child(unread_dot(TILE, p.rail_bg, p));
    }
    slot.role("button")
        .cursor(rune::Cursor::Pointer)
        .label(name.to_owned())
        .focusable(true)
        .on_secondary_press({
            let scope = scope_for_menu;
            move |s: &mut State, _, _| {
                if let Scope::Space(id) = &scope {
                    s.toggle_popover(Popover::SpaceMenu(id.clone()));
                }
            }
        })
        .on_press_tx(crate::state::navigate(move |s| s.select_scope(scope.clone())))
}

/// A space's menu on the rail: rename it (when the user may), or leave it.
pub fn space_menu(account: &AccountData, space_id: &str, p: &Palette) -> El {
    use super::components::{Tone, menu_action, popover_panel};
    let mut panel = popover_panel(p).key("space-menu").w(220.0);
    let space = account.space(space_id);
    if space.is_some_and(|space| space.can_change_settings) {
        let id = space_id.to_owned();
        panel = panel.child(
            menu_action(&icons::SETTINGS, t!("room_settings.title"), Tone::Normal, p)
                .key("space-settings")
                .on_press(move |s: &mut State| s.open_room_settings(&id)),
        );
    }
    if space.is_some_and(|space| space.can_add_rooms) {
        let id = space_id.to_owned();
        panel = panel.child(
            menu_action(&icons::LAYOUT, t!("space.rooms"), Tone::Normal, p)
                .key("space-rooms")
                .on_press(move |s: &mut State| s.open_space_rooms(&id)),
        );
    }
    let id = space_id.to_owned();
    panel = panel.child(
        menu_action(&icons::LEAVE, t!("space.leave"), Tone::Danger, p)
            .key("space-leave")
            .on_press(move |s: &mut State| s.ask_to_leave_space(&id)),
    );
    col().child(panel).anchored(format!("scope-{space_id}"), rune::Side::Right, rune::Align::Start, 8.0)
}

/// How to install the offered release where the app can't install it itself: the terminal
/// command for the way it was installed, to copy, or the release's page.
pub fn update_panel(s: &State, p: &Palette) -> Option<El> {
    use super::components::{ButtonKind, button, popover_panel};
    let release = s.update.release()?;
    let (hint, content, action) = match &release.upgrade {
        Upgrade::Archive { .. } => return None,
        Upgrade::Command(command) => (
            t!("update.run_command"),
            Some(
                // A long command scrolls, so Copy stays in reach on the smallest window.
                col()
                    .key("update-command")
                    .w_full()
                    .max_h(96.0)
                    .scroll(ScrollSpec { stick_bottom: false })
                    .padding(12.0)
                    .radius(8.0)
                    .fill(p.bg)
                    .child(
                        text(command.clone())
                            .key("update-command-text")
                            .font(font::META.mono())
                            .color(p.text)
                            .wrap_anywhere()
                            .selectable(true),
                    ),
            ),
            button(t!("update.copy"), ButtonKind::Primary, true, p)
                .key("update-copy")
                .on_press_tx(State::copy_update_command),
        ),
        Upgrade::Page(_) => (
            t!("update.download"),
            None,
            button(t!("update.open_page"), ButtonKind::Primary, true, p)
                .key("update-page")
                .on_press_tx(State::open_update_page),
        ),
    };
    let panel = popover_panel(p)
        .key("update-panel")
        .px(16.0)
        .py(14.0)
        .gap(10.0)
        .child(text(tf!("update.available", version = release.version)).font(font::BODY_STRONG).color(p.text).wrap())
        .child(text(hint.to_owned()).font(font::BODY).color(p.text_muted).wrap())
        .children(content)
        .child(row().w_full().justify_end().child(action));
    Some(col().child(panel).anchored(UPDATE_BUTTON, rune::Side::Right, rune::Align::End, 8.0))
}
