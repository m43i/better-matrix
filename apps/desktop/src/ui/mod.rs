//! The view: one function from [`State`] to elements, composed from the screens below, which
//! are composed from [`components`]. Popovers draw last, at the root, over everything.

mod account_menu;
mod backdrop;
mod call;
mod components;
mod composer;
mod confirm;
mod emoji;
mod icons;
mod info;
mod members;
mod message_text;
mod overview;
mod people;
mod poll;
mod preview;
mod rail;
mod room;
mod room_discovery;
mod room_menu;
mod security;
mod settings;
mod sidebar;
mod sign_in;
mod theme;
mod thread;
mod timeline;

#[cfg(test)]
mod tests;

use crate::i18n::t;
use rune::{Color, Cx, El, col, el, row, text};

use crate::state::{EmojiFor, Modal, Popover, Screen, State};
use theme::{Palette, font, palette};

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    cx.zoom(s.settings.zoom.factor());
    // What the state's own animations (the backdrop, the voice water) read when they step.
    s.reduced_motion.set(cx.is_reduced_motion());
    let p = palette(s.dark(cx.system_dark()));
    let account = s.active_account();
    let content = match (s.screen(), account) {
        (Screen::Chat, Some(account)) => row()
            .grow(1.0)
            .h_full()
            .min_w(0.0)
            .children((!s.call_here().is_some_and(|call| call.hide_sidebar)).then(|| sidebar::sidebar(s, account, &p)))
            .child(room::room_pane(s, account, &p, cx)),
        (Screen::Chat, None) => room::empty_state(t!("room.pick_account"), &p),
        (Screen::SignIn, _) => {
            let front = sign_in::sign_in(s, &p);
            backdrop::behind(s, front, s.dark(cx.system_dark()), &p)
        }
        (Screen::Restoring, _) => backdrop::behind(s, restoring(&p), s.dark(cx.system_dark()), &p),
    };
    let mut body = row().w_full().grow(1.0).min_h(0.0);
    if !s.accounts.is_empty() {
        body = body.child(rail::rail(s, account, &p));
    }
    let mut base = col().w_full().h_full().fill(p.bg);

    if s.screen() == Screen::Chat && account.and_then(|a| a.security).is_some_and(|security| !security.session_verified)
    {
        base = base.child(unverified_bar(&p));
    }
    if let (Screen::Chat, Some(account)) = (s.screen(), account) {
        base = base.child(people::top_bar(s, account, &p));
    }
    let mut root = el().key("root").w_full().h_full().child(base.child(body.child(content)));
    let panels = match (&s.popover, account) {
        (Some(Popover::AccountSwitcher), _) => vec![account_menu::account_menu(s, &p)],
        (Some(Popover::RoomMenu { room_id, notifications }), Some(account)) => {
            room_menu::room_menu(account, room_id, *notifications, &p)
        }
        (Some(Popover::Search), Some(account)) => vec![people::search_results(s, account, &p)],
        (Some(Popover::SectionMenu(id)), Some(account)) => vec![sidebar::section_menu(s, account, id, &p)],
        (Some(Popover::NewChat), Some(account)) => vec![sidebar::new_chat_panel(s, account, &p)],
        (Some(Popover::CreateMenu), Some(account)) => vec![sidebar::create_menu(s, account, &p)],
        (Some(Popover::SpaceMenu(id)), Some(account)) => vec![rail::space_menu(account, id, &p)],
        (Some(Popover::CallDevices(kind)), _) => vec![call::device_menu(s, *kind, &p)],
        (Some(Popover::CallLayout), _) => s.call_here().map(|call| call::layout_menu(call, &p)).into_iter().collect(),
        (Some(Popover::MemberMenu { user_id, submenu }), Some(account)) => {
            members::menu(s, account, user_id, *submenu, &p)
        }
        (Some(Popover::MessageMenu { key, event_id }), _) => timeline::message_menu(s, key, event_id, &p),
        (Some(Popover::Emoji(chosen_for)), _) => {
            let anchor = match chosen_for {
                EmojiFor::Reaction { key, event_id } => match key.thread {
                    None => format!("react-{event_id}"),
                    Some(_) => format!("thread-react-{event_id}"),
                },
                EmojiFor::Composer(target) => composer::emoji_button(*target),
            };
            vec![emoji::picker(s, chosen_for, anchor, &p)]
        }
        _ => Vec::new(),
    };
    // A select's list belongs to the settings modal, so it goes over the modal, below.
    let select = match &s.popover {
        Some(Popover::Select(of)) => Some(settings::select_options(s, of, &p)),
        _ => None,
    };
    if !panels.is_empty() {
        root = root.child(scrim()).children(panels);
    }
    let dialog = match &s.modal {
        Some(Modal::Settings(section)) => {
            // The window's size, from the root's last layout; a guess until the first one.
            let window = cx.layout_size("root").unwrap_or([1200.0, 760.0]);
            Some(settings::settings(s, *section, window, &p))
        }
        Some(Modal::Confirm(action)) => Some(confirm::confirm(action, &p)),
        Some(Modal::Poll) => Some(poll::form(s, &p)),
        Some(Modal::Invite { room_id, invited, .. }) => {
            account.map(|account| people::invite_dialog(s, account, room_id, invited, &p))
        }
        Some(Modal::Invitation { account, room_id }) => room_discovery::invitation(s, account, room_id, &p),
        Some(Modal::PublicRooms { account, space }) => Some(room_discovery::public_rooms(s, account, space, &p)),
        Some(other) => security::dialog_for(other, &p),
        None => None,
    };
    // The preview goes over everything, dialogs included.
    let preview = s.preview.as_ref().map(|preview| {
        let window = cx.layout_size("root").unwrap_or([1200.0, 760.0]);
        preview::preview(preview, window)
    });
    if let Some(dialog) = dialog {
        // A dialog stays inside the window, its content scrolling when the window is short.
        let window = cx.layout_size("root").unwrap_or([1200.0, 760.0]);
        root = root.child(
            el().key("modal-layer")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .center()
                .child(settings::backdrop())
                .child(dialog.max_h((window[1] - 48.0).max(200.0))),
        );
    }
    if let Some(select) = select {
        root = root.child(scrim()).child(select);
    }
    if let Some(preview) = preview {
        root = root.child(preview);
    }
    // The top right corner: a notice (it goes by itself), then the calls ringing, one under the
    // other so neither hides the other.
    let notice = s.notice.as_deref().map(|message| toast(message, &p));
    let rings = call::rings(s, &p);
    if notice.is_some() || rings.is_some() {
        root = root.child(
            col()
                .key("corner")
                .pin(None, Some(56.0), Some(20.0), None)
                .gap(12.0)
                .items_end()
                .children(notice)
                .children(rings),
        );
    }
    if let (Some(drag), Some(account)) = (&s.dragging, account) {
        root = root.child(sidebar::drag_ghost(drag, account, &p));
    }
    root
}

/// The launch screen while saved accounts are signed back in.
fn restoring(p: &Palette) -> El {
    col()
        .key("restoring")
        .w_full()
        .h_full()
        .center()
        .gap(12.0)
        .child(rune::icon(&icons::LOGO, 72.0).color(p.text))
        .child(text(t!("restoring.title")).font(font::HEADER).color(p.text))
        .child(text(t!("restoring.hint")).font(font::BODY).color(p.text_muted))
}

/// A transparent layer under a popover: a press outside the popover closes it.
fn scrim() -> El {
    el().key("scrim").absolute(0.0, 0.0).w_full().h_full().fill(Color::TRANSPARENT).on_press(State::close_popover)
}

/// Nudges the user to verify this session, which can't read earlier encrypted messages until
/// then.
fn unverified_bar(p: &Palette) -> El {
    row()
        .key("unverified-bar")
        .w_full()
        .px(16.0)
        .py(8.0)
        .gap(12.0)
        .center_y()
        .fill(p.warning.alpha(0.18))
        .child(rune::icon(&icons::LOCK, 16.0).color(p.warning))
        .child(text(t!("account.banner")).font(font::BODY).color(p.text).grow(1.0).wrap())
        .child(
            text(t!("account.banner_action"))
                .key("unverified-verify")
                .font(font::BODY_STRONG)
                .color(p.accent)
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .on_press(State::open_account_settings),
        )
}

/// A message at the window's top right, under the search bar: what went wrong, and a way to close it.
fn toast(message: &str, p: &Palette) -> El {
    row()
        .key("notice")
        .max_w(420.0)
        .pl(14.0)
        .pr(6.0)
        .py(8.0)
        .gap(10.0)
        .center_y()
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .shadow(4.0, 24.0, Color::BLACK.alpha(0.35))
        .role("alert")
        .child(rune::icon(&icons::ALERT, 18.0).color(p.danger))
        .child(text(message.to_owned()).font(font::BODY).color(p.text).shrink().wrap())
        .child(
            components::icon_button(&icons::CLOSE, t!("common.dismiss"), false, p.text_muted, p)
                .key("dismiss-notice")
                .on_press(State::dismiss_notice),
        )
}
