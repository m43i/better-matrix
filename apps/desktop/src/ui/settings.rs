//! The settings modal: a section list on the left, the chosen section on the right.

use std::borrow::Cow;

use rune::{Color, El, ScrollSpec, TextEdit, col, el, icon, row, text};

use super::components::{
    ButtonKind, account_avatar, button, choice_row, field_input, icon_button, select_button, select_menu,
    select_option, status_line, switch_row,
};
use super::icons;
use super::room_menu::{mode_id, modes};
use super::theme::{Palette, font};
use crate::i18n::{Language, t, tf};
use crate::state::{
    AccountData, Appearance, ContactStep, Emoticons, GifKey, MediaCache, MessageLayout, Popover, SelectFor,
    SettingsSection, State, Zoom,
};
use crate::third_party;
use matrix_core::{ContactDetail, ContactKind, OfflineDevice, RecoveryStatus, Session, SignInMethod};

/// How much of the whole window (rail and sidebar included) the modal takes, each way.
const SHARE: f32 = 0.9;

pub fn settings(s: &State, section: SettingsSection, window: [f32; 2], p: &Palette) -> El {
    let [w, h] = window;
    let nav_width = (w * SHARE * 0.30).clamp(148.0, 220.0);
    let content_width = w * SHARE - nav_width;
    let gutter = if content_width >= 600.0 { 28.0 } else { 16.0 };
    let nav = col()
        .key("settings-nav")
        .w(nav_width)
        .h_full()
        .min_h(0.0)
        // A short window at a large interface size scrolls the sections rather than cut Info off.
        .scroll(ScrollSpec { stick_bottom: false })
        .padding(10.0)
        .gap(2.0)
        .fill(p.sidebar_bg)
        .border_sides([0.0, 1.0, 0.0, 0.0], p.divider)
        .child(text(t!("settings.title")).font(font::WORKSPACE).color(p.text).px(10.0).py(10.0))
        .child(nav_heading(t!("settings.preferences_group"), p))
        .children(SettingsSection::PREFERENCES.map(|item| nav_item(item, section, p)))
        .child(nav_heading(t!("settings.account_group"), p).mt(12.0))
        .children(SettingsSection::ACCOUNT.map(|item| nav_item(item, section, p)))
        .child(el().min_h(12.0).grow(1.0))
        .child(nav_item(SettingsSection::Info, section, p));

    let body = match section {
        SettingsSection::General => general(s, p),
        SettingsSection::Appearance => appearance(s, p),
        SettingsSection::MessagesMedia => messages_media(s, p),
        SettingsSection::Profile => profile(s, p),
        SettingsSection::Notifications => notifications(s, p),
        SettingsSection::Safety => safety(s, p),
        SettingsSection::Account => account(s, p),
        SettingsSection::Sessions => sessions(s, p),
        SettingsSection::VoiceVideo => voice_video(s, p),
        SettingsSection::Info => info(s, p),
    };
    let header = row()
        .w_full()
        .h(56.0)
        .px(gutter)
        .center_y()
        .fill(p.surface)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(section.label()).font(font::WORKSPACE).color(p.text).grow(1.0).min_w(0.0).truncate())
        .child(
            icon_button(&icons::CLOSE, t!("settings.close"), false, p.text_muted, p)
                .key("settings-close")
                .on_press(State::close_modal),
        );
    let content = col().instant_changes().grow(1.0).h_full().min_w(0.0).child(header).child(
        col()
            .key("settings-body")
            .w_full()
            .items_center()
            .fill(p.bg)
            .grow(1.0)
            .shrink()
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            // Scope descendant identities to the section, so Rune cannot tween an old
            // control's colours into an unrelated heading or avatar.
            .child(body.key(format!("settings-section-{}", section.id())).max_w(840.0).padding(gutter)),
    );

    // The border sits on a frame around the clipped panes, so the sidebar's fill can't cover it.
    col()
        .key("settings-modal")
        .size(w * SHARE, h * SHARE)
        .padding(BORDER)
        .radius(RADIUS)
        .fill(p.surface)
        .border(BORDER, p.text_muted.alpha(0.35))
        .shadow(16.0, 48.0, Color::BLACK.alpha(0.5))
        .role("dialog")
        .label(t!("settings.title"))
        .child(row().w_full().h_full().radius(RADIUS - BORDER).clip_rounded(RADIUS - BORDER).child(nav).child(content))
}

fn nav_heading(title: &str, p: &Palette) -> El {
    text(title.to_owned()).font(font::META).color(p.text_muted).px(10.0).py(4.0)
}

fn nav_item(item: SettingsSection, section: SettingsSection, p: &Palette) -> El {
    let selected = item == section;
    let glyph = match item {
        SettingsSection::General => &icons::SETTINGS,
        SettingsSection::Appearance => &icons::SUN_MOON,
        SettingsSection::MessagesMedia => &icons::THREAD,
        SettingsSection::Profile => &icons::USER,
        SettingsSection::Notifications => &icons::BELL,
        SettingsSection::Safety => &icons::EYE_OFF,
        SettingsSection::Account => &icons::SHIELD,
        SettingsSection::Sessions => &icons::LAPTOP,
        SettingsSection::VoiceVideo => &icons::MIC,
        SettingsSection::Info => &icons::INFO,
    };
    row()
        .key(format!("settings-{}", item.id()))
        .w_full()
        .min_h(34.0)
        .px(9.0)
        .py(6.0)
        .gap(9.0)
        .center_y()
        .radius(7.0)
        .fill(if selected { p.accent.alpha(0.13) } else { Color::TRANSPARENT })
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("tab")
        .cursor(rune::Cursor::Pointer)
        .checked(selected)
        .focusable(true)
        .label(item.label())
        .child(icon(glyph, 16.0).color(if selected { p.accent } else { p.text_muted }))
        .child(
            text(item.label())
                .font(if selected { font::BODY_STRONG } else { font::BODY })
                .color(p.text)
                .grow(1.0)
                .min_w(0.0)
                .wrap(),
        )
        .on_press(move |s: &mut State| s.show_settings_section(item))
}

fn cards() -> El {
    col().w_full().gap(16.0)
}

fn card(frame: El, p: &Palette) -> El {
    frame.w_full().padding(16.0).radius(12.0).border(1.0, p.divider).fill(p.surface)
}

fn setting_icon(glyph: &'static rune::Icon, p: &Palette) -> El {
    el().size(32.0, 32.0).radius(8.0).fill(p.accent.alpha(0.11)).center().child(icon(glyph, 17.0).color(p.accent))
}

/// A titled setting with controls below its explanation.
fn setting_card(glyph: &'static rune::Icon, title: &str, hint: &str, controls: El, p: &Palette) -> El {
    card(col().gap(14.0), p)
        .child(
            row().w_full().gap(11.0).items_start().child(setting_icon(glyph, p)).child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(4.0)
                    .child(text(title.to_owned()).font(font::HEADER).color(p.text).wrap())
                    .child(text(hint.to_owned()).font(font::META).color(p.text_muted).wrap()),
            ),
        )
        .child(controls)
}

fn switch_card(glyph: &'static rune::Icon, toggle: El, p: &Palette) -> El {
    card(row().gap(11.0).center_y(), p).py(8.0).child(setting_icon(glyph, p)).child(toggle.grow(1.0).min_w(0.0))
}

/// The modal's corner radius and border width.
const RADIUS: f32 = 14.0;
const BORDER: f32 = 1.0;

/// The app's name and version, and the third-party software it is built from: a count until
/// asked for, then every component with its version and license.
fn info(s: &State, p: &Palette) -> El {
    let app = row().gap(14.0).center_y().child(icon(&icons::LOGO, 48.0).color(p.accent)).child(
        col().gap(2.0).child(text(t!("app.name")).font(font::WORKSPACE).color(p.text)).child(
            text(tf!("info.version", version = env!("CARGO_PKG_VERSION")))
                .key("app-version")
                .font(font::BODY)
                .color(p.text_muted),
        ),
    );
    let components = third_party::all();
    let shown = s.third_party_shown;
    let label = if shown {
        t!("info.hide_third_party").to_owned()
    } else {
        tf!("info.show_third_party", count = components.len())
    };
    let mut list = col().w_full().gap(8.0).child(
        button(&label, ButtonKind::Secondary, true, p).key("third-party-toggle").on_press(State::toggle_third_party),
    );
    if shown {
        // Only the rows in view are built: the list runs to hundreds.
        let palette = *p;
        list = list.child(
            col()
                .key("third-party")
                .w_full()
                .growth(rune::GrowthSpec::default())
                .virtual_rows(
                    components.len(),
                    THIRD_PARTY_ROW,
                    |_: &State, i| {
                        let c = third_party::all().get(i);
                        Cow::Owned(format!("component-{}-{}", c.map_or("", |c| c.name), c.map_or("", |c| c.version)))
                    },
                    move |_: &State, i| third_party::all().get(i).map_or_else(el, |c| component_row(c, &palette)),
                )
                .virtual_rows_version(1),
        );
    }
    cards().child(app).child(setting_card(
        &icons::PACKAGE,
        t!("info.third_party"),
        t!("info.third_party_hint"),
        list,
        p,
    ))
}

/// The height of a third-party row, for the list's estimate.
const THIRD_PARTY_ROW: f32 = 36.0;

/// A component's name and version, with its license at the end.
fn component_row(component: &third_party::Component, p: &Palette) -> El {
    row()
        .w_full()
        .h(THIRD_PARTY_ROW)
        .px(4.0)
        .gap(10.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(component.name).font(font::BODY).color(p.text).min_w(0.0).truncate())
        .child(text(component.version).font(font::META).color(p.text_muted))
        .child(el().grow(1.0))
        .child(text(component.license).font(font::META).color(p.text_muted).min_w(0.0).truncate())
}

fn general(s: &State, p: &Palette) -> El {
    let languages = select(s, SelectFor::Language, &s.settings.language.label(), None, p);
    let (close_label, close_hint) = if cfg!(target_os = "macos") {
        (t!("settings.close_to_menu_bar"), t!("settings.close_to_menu_bar_hint"))
    } else {
        (t!("settings.close_to_tray"), t!("settings.close_to_tray_hint"))
    };
    let close_to_tray = switch_row(close_label, Some(close_hint), s.settings.close_to_tray, p)
        .key("close-to-tray")
        .on_press(|s: &mut State| s.set_close_to_tray(!s.settings.close_to_tray));
    let hidden = switch_row(
        t!("settings.hidden_events_on"),
        Some(t!("settings.hidden_events_on_hint")),
        s.settings.show_hidden_events,
        p,
    )
    .key("hidden-events")
    .on_press(|s: &mut State| s.set_show_hidden_events(!s.settings.show_hidden_events));
    cards()
        .child(setting_card(&icons::LANGUAGE, t!("settings.language"), t!("settings.language_hint"), languages, p))
        .child(switch_card(&icons::CLOSE, close_to_tray, p))
        .child(setting_card(&icons::CODE, t!("settings.developer"), t!("settings.developer_hint"), hidden, p))
}

fn messages_media(s: &State, p: &Palette) -> El {
    let emoticons = Emoticons::ALL.into_iter().fold(col().w_full().gap(6.0), |list, emoticons| {
        list.child(
            choice_row(emoticons.label(), Some(emoticons.hint()), s.settings.emoticons == emoticons, p)
                .key(format!("emoticons-{}", emoticons.id()))
                .on_press(move |s: &mut State| s.set_emoticons(emoticons)),
        )
    });
    let room_changes = switch_row(
        t!("settings.room_changes"),
        Some(t!("settings.room_changes_hint")),
        s.settings.show_room_changes,
        p,
    )
    .key("room-changes")
    .on_press(|s: &mut State| s.set_show_room_changes(!s.settings.show_room_changes));
    let preview_hint = format!("{} {}", t!("settings.previews_hint"), t!("settings.previews_on_hint"));
    let previews = switch_row(t!("settings.previews"), Some(&preview_hint), s.settings.show_link_previews, p)
        .key("previews")
        .on_press(|s: &mut State| s.set_show_link_previews(!s.settings.show_link_previews));
    let gif_hint = format!("{} {}", t!("settings.gifs_hint"), t!("settings.gifs_on_hint"));
    let gifs = switch_row(t!("settings.gifs"), Some(&gif_hint), s.settings.show_link_gifs, p)
        .key("gifs")
        .on_press(|s: &mut State| s.set_show_link_gifs(!s.settings.show_link_gifs));
    let map_hint = format!("{} {}", t!("settings.maps_hint"), t!("settings.maps_on_hint"));
    let maps = switch_row(t!("settings.maps"), Some(&map_hint), s.settings.show_maps, p)
        .key("maps")
        .on_press(|s: &mut State| s.set_show_maps(!s.settings.show_maps));
    cards()
        .child(setting_card(&icons::SMILE, t!("settings.emoticons"), t!("settings.emoticons_hint"), emoticons, p))
        .child(switch_card(&icons::LINK, previews, p))
        .child(switch_card(&icons::IMAGE, gifs, p))
        .child(setting_card(&icons::KEY, t!("settings.giphy"), t!("settings.giphy_hint"), giphy_key(s, p), p))
        .child(switch_card(&icons::MAP_PIN, maps, p))
        .child(switch_card(&icons::HISTORY, room_changes, p))
}

/// The user's GIPHY API key for the GIF picker, kept in the keychain: once kept it is never
/// shown, only replaced or removed; without one, a field to give one.
fn giphy_key(s: &State, p: &Palette) -> El {
    if s.picker.giphy == GifKey::Loading {
        return text(t!("picker.key_loading")).font(font::META).color(p.text_muted);
    }
    if s.asks_gif_key() {
        return col().w_full().gap(8.0).child(super::stickers::gif_key_field(s, "settings-giphy-key", p)).child(
            button(t!("picker.get_key"), ButtonKind::Link, true, p)
                .key("settings-giphy-get")
                .on_press_tx(|s: &mut State, tx| s.open_giphy_keys(tx)),
        );
    }
    row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child({
            let said = if matches!(s.picker.giphy, GifKey::Session(_)) {
                t!("settings.giphy_session")
            } else {
                t!("settings.giphy_saved")
            };
            text(said).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).wrap()
        })
        .child(
            button(t!("picker.replace_key"), ButtonKind::Secondary, true, p)
                .key("settings-giphy-replace")
                .on_press(State::replace_gif_key),
        )
        .child(
            button(t!("settings.giphy_remove"), ButtonKind::Secondary, true, p)
                .key("settings-giphy-remove")
                .on_press(State::remove_gif_key),
        )
}

/// How the app looks: the theme, message layout and interface size.
fn appearance(s: &State, p: &Palette) -> El {
    let themes = Appearance::ALL.into_iter().fold(col().w_full().gap(8.0), |list, appearance| {
        list.child(
            choice_row(appearance.label(), None, s.settings.appearance == appearance, p)
                .key(format!("appearance-{}", appearance.id()))
                .on_press(move |s: &mut State| s.set_appearance(appearance)),
        )
    });
    let layouts = MessageLayout::ALL.into_iter().fold(col().w_full().gap(8.0), |list, layout| {
        list.child(
            choice_row(layout.label(), Some(layout.hint()), s.settings.message_layout == layout, p)
                .key(format!("message-layout-{}", layout.id()))
                .on_press(move |s: &mut State| s.set_message_layout(layout)),
        )
    });
    cards()
        .child(setting_card(&icons::SUN_MOON, t!("settings.theme"), t!("settings.theme_hint"), themes, p))
        .child(setting_card(&icons::THREAD, t!("settings.messages"), t!("settings.messages_hint"), layouts, p))
        .child(setting_card(&icons::ZOOM, t!("settings.zoom"), t!("settings.zoom_hint"), zoom(s, p), p))
}

/// Smaller, the current size, larger, and back to the default.
fn zoom(s: &State, p: &Palette) -> El {
    let zoom = s.settings.zoom;
    row()
        .gap(8.0)
        .center_y()
        .child(
            button("−", ButtonKind::Secondary, zoom.smaller() != zoom, p)
                .key("zoom-out")
                .label(t!("settings.zoom_out"))
                .on_press(move |s: &mut State| s.set_zoom(zoom.smaller())),
        )
        .child(
            el().key("zoom-level")
                .w(56.0)
                .center()
                .child(text(format!("{}%", zoom.percent())).font(font::BODY_STRONG).color(p.text)),
        )
        .child(
            button("+", ButtonKind::Secondary, zoom.larger() != zoom, p)
                .key("zoom-in")
                .label(t!("settings.zoom_in"))
                .on_press(move |s: &mut State| s.set_zoom(zoom.larger())),
        )
        .children((zoom != Zoom::DEFAULT).then(|| {
            button(t!("settings.zoom_reset"), ButtonKind::Link, true, p)
                .key("zoom-reset")
                .ml(8.0)
                .on_press(|s: &mut State| s.set_zoom(Zoom::DEFAULT))
        }))
}

/// The default notification mode, per account: push rules live on each account's server.
fn notifications(s: &State, p: &Palette) -> El {
    let accounts = s.accounts.iter().fold(col().w_full().gap(20.0), |list, account| {
        let summary = &account.summary;
        let heading = row().gap(10.0).center_y().child(account_avatar(summary, &s.media, 28.0)).child(
            col()
                .child(text(summary.display_name.clone()).font(font::BODY_STRONG).color(p.text))
                .child(text(summary.id.to_string()).font(font::META).color(p.text_muted)),
        );
        let (title, hint) = modes()
            .into_iter()
            .find(|(mode, ..)| *mode == summary.default_notifications)
            .map_or(("", ""), |(_, title, hint, _)| (title, hint));
        let choice = select(s, SelectFor::Notifications(summary.id.clone()), title, Some(hint), p);
        list.child(col().w_full().gap(10.0).child(heading).child(choice).child(keywords(s, account, p)))
    });
    // How an incoming call rings: aloud, or only the notification and the card.
    let (title, hint) =
        ring_choices().into_iter().find(|(on, ..)| *on == s.settings.ring_sound).map_or(("", ""), |(_, t, h)| (t, h));
    let ring = select(s, SelectFor::Ring, title, Some(hint), p);
    cards()
        .child(setting_card(&icons::CALL, t!("settings.ring"), t!("settings.ring_hint"), ring, p))
        .child(setting_card(&icons::BELL, t!("settings.notifications"), t!("settings.notifications_hint"), accounts, p))
}

/// What the active account keeps away (ADR 0011): where media shows, strangers' invitations
/// (with those hidden, to open), pictures in invitations, and the people the user ignores.
fn safety(s: &State, p: &Palette) -> El {
    use matrix_core::{SafetyChange, ShowMedia};
    let Some(account) = s.settings_account() else { return col() };
    let id = &account.summary.id;
    let safety = &account.safety;
    let media = [
        (ShowMedia::Always, t!("safety.media_always"), None),
        (ShowMedia::Private, t!("safety.media_private"), Some(t!("safety.media_private_hint"))),
        (ShowMedia::Never, t!("safety.media_never"), None),
    ]
    .into_iter()
    .fold(col().w_full().gap(8.0), |list, (choice, title, hint)| {
        let owner = id.clone();
        list.child(
            choice_row(title, hint, safety.media == choice, p)
                .key(format!("safety-media-{choice:?}"))
                .on_press(move |s: &mut State| s.set_safety(&owner, SafetyChange::Media(choice))),
        )
    });
    let mut strangers = [
        (false, t!("safety.strangers_show"), None),
        (true, t!("safety.strangers_hide"), Some(t!("safety.strangers_hide_hint"))),
    ]
    .into_iter()
    .fold(col().w_full().gap(8.0), |list, (hide, title, hint)| {
        let owner = id.clone();
        list.child(
            choice_row(title, hint, safety.hide_stranger_invites == hide, p)
                .key(format!("safety-strangers-{hide}"))
                .on_press(move |s: &mut State| s.set_safety(&owner, SafetyChange::HideStrangerInvites(hide))),
        )
    });
    let hidden: Vec<_> = account.hidden_invitations().collect();
    if !hidden.is_empty() {
        strangers = strangers.child(
            text(tf!("safety.hidden_invitations", count = hidden.len()))
                .key("safety-hidden-heading")
                .font(font::SECTION)
                .color(p.text_muted)
                .mt(8.0),
        );
        for invite in hidden {
            let room_id = invite.room_id.clone();
            let from =
                invite.inviter.as_ref().map(|inviter| tf!("invitations.from", name = inviter.name, user = inviter.id));
            let mut titles = col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text(invite.name.clone()).font(font::BODY_STRONG).color(p.text).truncate());
            if let Some(from) = from {
                titles = titles.child(text(from).font(font::META).color(p.text_muted).truncate());
            }
            strangers = strangers.child(
                row()
                    .key(format!("safety-hidden-{room_id}"))
                    .w_full()
                    .px(14.0)
                    .py(10.0)
                    .gap(12.0)
                    .center_y()
                    .radius(8.0)
                    .border(1.0, p.divider)
                    .hover(p.hover.alpha(p.hover.a * 2.0))
                    .role("button")
                    .cursor(rune::Cursor::Pointer)
                    .focusable(true)
                    .label(invite.name.clone())
                    .child(super::room_discovery::invitation_picture(s, account, invite, 28.0))
                    .child(titles)
                    .child(icon(&icons::CHEVRON_RIGHT, 18.0).color(p.text_muted))
                    .on_press(move |s: &mut State| s.show_invitation(&room_id)),
            );
        }
    }
    let pictures = [(true, t!("safety.pictures_show")), (false, t!("safety.pictures_hide"))].into_iter().fold(
        col().w_full().gap(8.0),
        |list, (shown, title)| {
            let owner = id.clone();
            list.child(
                choice_row(title, None, safety.invite_pictures == shown, p)
                    .key(format!("safety-pictures-{shown}"))
                    .on_press(move |s: &mut State| s.set_safety(&owner, SafetyChange::InvitePictures(shown))),
            )
        },
    );
    let ignored = if safety.ignored.is_empty() {
        col().child(text(t!("safety.ignored_none")).key("safety-ignored-none").font(font::BODY).color(p.text_muted))
    } else {
        safety.ignored.iter().fold(col().w_full().gap(8.0), |list, user_id| {
            let (owner, user) = (id.clone(), user_id.clone());
            list.child(
                row()
                    .key(format!("safety-ignored-{user_id}"))
                    .w_full()
                    .px(14.0)
                    .py(8.0)
                    .gap(12.0)
                    .center_y()
                    .radius(8.0)
                    .border(1.0, p.divider)
                    .child(super::components::avatar(user_id, user_id, 28.0))
                    .child(text(user_id.clone()).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).truncate())
                    .child(
                        button(t!("safety.stop_ignoring"), ButtonKind::Secondary, true, p)
                            .key(format!("safety-unignore-{user_id}"))
                            .on_press(move |s: &mut State| s.stop_ignoring(&owner, &user)),
                    ),
            )
        })
    };
    cards()
        .child(account_heading(account, &s.media, p))
        .child(setting_card(&icons::IMAGE, t!("safety.media"), t!("safety.media_hint"), media, p))
        .child(setting_card(&icons::USER_PLUS, t!("safety.strangers"), t!("safety.strangers_hint"), strangers, p))
        .child(setting_card(&icons::EYE_OFF, t!("safety.pictures"), t!("safety.pictures_hint"), pictures, p))
        .child(setting_card(&icons::USER_X, t!("safety.ignored"), t!("safety.ignored_hint"), ignored, p))
}

/// An account's notification keywords, each with its remove button, then the field adding one.
fn keywords(s: &State, account: &AccountData, p: &Palette) -> El {
    let id = account.summary.id.clone();
    let mut list = col()
        .w_full()
        .gap(8.0)
        .child(text(t!("settings.keywords")).font(font::BODY_STRONG).color(p.text))
        .child(text(t!("settings.keywords_hint")).font(font::META).color(p.text_muted).wrap());
    match &account.keywords {
        None => list = list.child(text(t!("profile.loading")).font(font::BODY).color(p.text_muted)),
        Some(keywords) => {
            for keyword in keywords {
                let (account, removed) = (id.clone(), keyword.clone());
                list = list.child(
                    row()
                        .key(format!("keyword-{id}-{keyword}"))
                        .w_full()
                        .h(40.0)
                        .px(12.0)
                        .gap(10.0)
                        .center_y()
                        .radius(8.0)
                        .border(1.0, p.divider)
                        .child(text(keyword.clone()).font(font::BODY).color(p.text).grow(1.0).truncate())
                        .child(
                            icon_button(
                                &icons::TRASH,
                                &tf!("settings.keyword_remove", keyword = keyword),
                                false,
                                p.text_muted,
                                p,
                            )
                            .key(format!("keyword-remove-{id}-{keyword}"))
                            .on_press(move |s: &mut State| s.remove_keyword(&account, &removed)),
                        ),
                );
            }
        }
    }
    let empty = TextEdit::default();
    let draft = s.keyword_drafts.get(&id).unwrap_or(&empty);
    let typed = !draft.text.trim().is_empty();
    let (edit, submit, add) = (id.clone(), id.clone(), id.clone());
    let field = field_input(draft, t!("settings.keyword_placeholder"), p)
        .key(format!("keyword-new-{id}"))
        .label(t!("settings.keyword_placeholder"))
        .on_edit(move |s: &mut State| s.keyword_drafts.entry(edit.clone()).or_default())
        .on_submit(move |s: &mut State, _| s.add_keyword(&submit));
    list.child(
        row().w_full().gap(8.0).center_y().child(col().grow(1.0).min_w(0.0).child(field)).child(
            button(t!("settings.keyword_add"), ButtonKind::Secondary, typed, p)
                .key(format!("keyword-add-{id}"))
                .on_press(move |s: &mut State| s.add_keyword(&add)),
        ),
    )
}

fn ring_choices() -> [(bool, &'static str, &'static str); 2] {
    [
        (true, t!("settings.ring_on"), t!("settings.ring_on_hint")),
        (false, t!("settings.ring_off"), t!("settings.ring_off_hint")),
    ]
}

/// The microphone, speaker and camera for calls and voice messages: the system's default, or one
/// picked by name.
fn voice_video(s: &State, p: &Palette) -> El {
    use matrix_core::DeviceKind;
    let group = |kind: DeviceKind, glyph: &'static rune::Icon, title: &str, hint: &str| {
        let current = s.settings.devices.get(kind).unwrap_or(t!("settings.system_default"));
        setting_card(glyph, title, hint, select(s, SelectFor::Device(kind), current, None, p), p)
    };
    cards()
        .child(group(DeviceKind::Microphone, &icons::MIC, t!("settings.microphone"), t!("settings.microphone_hint")))
        .child(group(DeviceKind::Speaker, &icons::SPEAKER, t!("settings.speaker"), t!("settings.speaker_hint")))
        .child(group(DeviceKind::Camera, &icons::VIDEO, t!("settings.camera"), t!("settings.camera_hint")))
}

/// A select's key: its button, and what its list anchors to.
fn select_key(of: &SelectFor) -> String {
    match of {
        SelectFor::Language => "select-language".into(),
        SelectFor::Notifications(account) => format!("select-default-{account}"),
        SelectFor::Ring => "select-ring".into(),
        SelectFor::Device(kind) => format!("select-device-{kind:?}"),
        SelectFor::Permission(permission) => super::admin::permission_key(*permission),
    }
}

/// A select showing the current choice; pressing it opens (or closes) its list.
fn select(s: &State, of: SelectFor, title: &str, hint: Option<&str>, p: &Palette) -> El {
    let open = s.popover.as_ref() == Some(&Popover::Select(of.clone()));
    let key = select_key(&of);
    select_button(title, hint, open, p).key(key).on_press(move |s: &mut State| s.open_select(of.clone()))
}

/// The options of the open select, under its button.
pub fn select_options(s: &State, of: &SelectFor, p: &Palette) -> El {
    let options = match of {
        SelectFor::Language => Language::all()
            .map(|language| {
                select_option(&language.label(), None, s.settings.language == language, p)
                    .key(format!("language-{}", language.id()))
                    .on_press(move |s: &mut State| {
                        s.popover = None;
                        s.set_language(language);
                    })
            })
            .collect(),
        SelectFor::Notifications(account) => {
            let current = s.accounts.get(account).map(|data| data.summary.default_notifications);
            modes()
                .into_iter()
                .map(|(mode, title, hint, _)| {
                    let id = account.clone();
                    select_option(title, Some(hint), current == Some(mode), p)
                        .key(format!("default-{account}-{}", mode_id(mode)))
                        .on_press(move |s: &mut State| {
                            s.popover = None;
                            s.set_default_notifications(&id, mode);
                        })
                })
                .collect()
        }
        SelectFor::Ring => ring_choices()
            .into_iter()
            .map(|(on, title, hint)| {
                select_option(title, Some(hint), s.settings.ring_sound == on, p)
                    .key(format!("ring-sound-{on}"))
                    .on_press(move |s: &mut State| {
                        s.popover = None;
                        s.set_ring_sound(on);
                    })
            })
            .collect(),
        SelectFor::Device(kind) => device_options(s, *kind, p),
        // The room settings' permission selects are the room settings' to list.
        SelectFor::Permission(permission) => return super::admin::permission_options(s, *permission, p),
    };
    select_menu(select_key(of), options, p)
}

/// The system's default, then each device of a kind; a picked device that's gone still shows,
/// so the choice stays visible.
fn device_options(s: &State, kind: matrix_core::DeviceKind, p: &Palette) -> Vec<El> {
    use matrix_core::DeviceKind;
    let picked = s.settings.devices.get(kind);
    let mut names: Vec<String> = s
        .call_devices
        .as_ref()
        .map(|devices| match kind {
            DeviceKind::Microphone => &devices.microphones,
            DeviceKind::Speaker => &devices.speakers,
            DeviceKind::Camera => &devices.cameras,
        })
        .into_iter()
        .flatten()
        .map(|device| device.name.clone())
        .collect();
    if let Some(picked) = picked.filter(|picked| !names.iter().any(|name| name == picked)) {
        names.push(picked.to_owned());
    }
    let mut options = vec![
        select_option(t!("settings.system_default"), None, picked.is_none(), p)
            .key(format!("device-{kind:?}-default"))
            .on_press(move |s: &mut State| s.choose_device(kind, None)),
    ];
    for name in names {
        let chosen = name.clone();
        options.push(
            select_option(&name, None, picked == Some(name.as_str()), p)
                .key(format!("device-{kind:?}-{name}"))
                .on_press(move |s: &mut State| s.choose_device(kind, Some(chosen.clone()))),
        );
    }
    if s.call_devices.is_none() {
        options.push(text(t!("settings.devices_loading")).font(font::META).color(p.text_muted).px(10.0).py(8.0));
    }
    options
}

/// The dimmed layer behind any modal; a press on it closes the modal.
pub fn backdrop() -> El {
    el().key("modal-backdrop")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(Color::BLACK.alpha(0.45))
        .on_press(State::close_modal)
}

/// Whose account the page shows, with avatar, name and ID.
fn account_heading(account: &AccountData, media: &MediaCache, p: &Palette) -> El {
    let summary = &account.summary;
    row().gap(12.0).center_y().child(account_avatar(summary, media, 40.0)).child(
        col()
            .child(text(summary.display_name.clone()).font(font::BODY_STRONG).color(p.text))
            .child(text(format!("{} · {}", summary.id, summary.homeserver)).font(font::META).color(p.text_muted)),
    )
}

/// The account's picture, display name and contact details. Where the server leaves contact
/// details to an OAuth service's account page, they show read-only with a link there.
fn profile(s: &State, p: &Palette) -> El {
    let Some(account) = s.settings_account() else { return col() };
    let summary = &account.summary;
    let form = &s.profile;
    let has_picture = summary.avatar.is_some();
    let picture = row().w_full().gap(20.0).center_y().child(account_avatar(summary, &s.media, 88.0)).child(
        col()
            .gap(10.0)
            .min_w(0.0)
            .child(
                col()
                    .child(text(summary.display_name.clone()).font(font::HEADER).color(p.text).truncate())
                    .child(text(summary.id.to_string()).font(font::META).color(p.text_muted)),
            )
            .child(
                row()
                    .gap(8.0)
                    .child(
                        button(
                            if form.saving_avatar { t!("profile.uploading") } else { t!("profile.upload") },
                            ButtonKind::Secondary,
                            !form.saving_avatar,
                            p,
                        )
                        .key("avatar-upload")
                        .on_press(State::pick_avatar),
                    )
                    .children(has_picture.then(|| {
                        button(t!("profile.remove_picture"), ButtonKind::Secondary, !form.saving_avatar, p)
                            .key("avatar-remove")
                            .on_press(|s: &mut State| s.set_avatar(None))
                    })),
            ),
    );

    let name_field = field_input(&form.name, t!("profile.name_placeholder"), p)
        .key("profile-name")
        .on_edit(|s: &mut State| &mut s.profile.name)
        .on_submit(|s: &mut State, _| s.save_display_name());
    let name = row().w_full().gap(8.0).center_y().child(col().grow(1.0).min_w(0.0).child(name_field)).child(
        button(
            if form.saving_name { t!("profile.saving") } else { t!("common.save") },
            ButtonKind::Primary,
            s.display_name_changed() && !form.saving_name,
            p,
        )
        .key("profile-name-save")
        .on_press(State::save_display_name),
    );

    let mut page = cards().child(picture).child(setting_card(
        &icons::PENCIL,
        t!("profile.name"),
        t!("profile.name_hint"),
        name,
        p,
    ));
    // Contact details change here where the server allows it; elsewhere they show as they are,
    // with the way to the page that manages them.
    let changeable = account.contacts.as_ref().is_none_or(|contacts| contacts.changeable);
    let details = account.contacts.as_ref().map(|contacts| contacts.details.as_slice());
    if changeable || details.is_some_and(|details| !details.is_empty()) {
        for (kind, glyph, title, hint) in [
            (ContactKind::Email, &icons::MAIL, t!("profile.emails"), t!("profile.emails_hint")),
            (ContactKind::Phone, &icons::PHONE, t!("profile.phones"), t!("profile.phones_hint")),
        ] {
            // A kind the user can't add to shows only when something is linked.
            if !changeable && !details.is_some_and(|details| details.iter().any(|contact| contact.kind == kind)) {
                continue;
            }
            let section = contact_section(s, kind, details, changeable, p);
            page = page.child(setting_card(glyph, title, hint, section, p));
        }
    }
    if !changeable && summary.sign_in == SignInMethod::OAuth {
        let link = col().w_full().gap(12.0).items_start().child(
            button(t!("profile.open_account_page"), ButtonKind::Secondary, true, p)
                .key("account-page")
                .on_press(State::open_account_page),
        );
        page = page.child(setting_card(
            &icons::EXTERNAL,
            t!("profile.account_page"),
            &tf!("profile.account_page_hint", server = summary.homeserver),
            link,
            p,
        ));
    }
    page
}

/// How a contact detail reads: phone numbers get their `+` back.
fn contact_label(contact: &ContactDetail) -> String {
    match contact.kind {
        ContactKind::Email => contact.address.clone(),
        ContactKind::Phone => format!("+{}", contact.address),
    }
}

/// The linked details of one kind, then the form adding another.
fn contact_section(
    s: &State,
    kind: ContactKind,
    contacts: Option<&[ContactDetail]>,
    changeable: bool,
    p: &Palette,
) -> El {
    let id = match kind {
        ContactKind::Email => "email",
        ContactKind::Phone => "phone",
    };
    let mut list = col().w_full().gap(8.0);
    match contacts {
        None => list = list.child(text(t!("profile.loading")).font(font::BODY).color(p.text_muted)),
        Some(contacts) => {
            let mut any = false;
            for contact in contacts.iter().filter(|contact| contact.kind == kind) {
                any = true;
                let label = contact_label(contact);
                let removed = contact.clone();
                let mut entry = row()
                    .key(format!("contact-{id}-{}", contact.address))
                    .w_full()
                    .h(44.0)
                    .px(12.0)
                    .gap(10.0)
                    .center_y()
                    .radius(8.0)
                    .border(1.0, p.divider)
                    .child(text(label.clone()).font(font::BODY).color(p.text).grow(1.0).truncate());
                if changeable {
                    entry = entry.child(
                        icon_button(&icons::TRASH, &tf!("profile.remove", address = label), false, p.text_muted, p)
                            .key(format!("contact-remove-{id}-{}", contact.address))
                            .on_press(move |s: &mut State| s.remove_contact(removed.clone())),
                    );
                }
                list = list.child(entry);
            }
            if !any {
                let none = match kind {
                    ContactKind::Email => t!("profile.no_emails"),
                    ContactKind::Phone => t!("profile.no_phones"),
                };
                list = list.child(text(none).font(font::BODY).color(p.text_muted));
            }
        }
    }

    if !changeable {
        return list;
    }
    let form = s.profile.contact(kind);
    let adding = match &form.step {
        ContactStep::Entering | ContactStep::Requesting(_) => {
            let requesting = matches!(form.step, ContactStep::Requesting(_));
            let (placeholder, action) = match kind {
                ContactKind::Email => (t!("profile.email_placeholder"), t!("profile.send_link")),
                ContactKind::Phone => (t!("profile.phone_placeholder"), t!("profile.send_code")),
            };
            let field = field_input(&form.address, placeholder, p)
                .key(format!("contact-new-{id}"))
                .on_edit(move |s: &mut State| &mut s.profile.contact_mut(kind).address)
                .on_submit(move |s: &mut State, _| s.request_contact_code(kind));
            row().w_full().gap(8.0).center_y().child(col().grow(1.0).min_w(0.0).child(field)).child(
                button(
                    if requesting { t!("profile.sending") } else { action },
                    ButtonKind::Secondary,
                    !requesting && !form.address.text.trim().is_empty(),
                    p,
                )
                .key(format!("contact-request-{id}"))
                .on_press(move |s: &mut State| s.request_contact_code(kind)),
            )
        }
        ContactStep::Sent { pending, busy } => {
            let cancel = button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key(format!("contact-cancel-{id}"))
                .on_press(move |s: &mut State| s.cancel_contact(kind));
            let confirm = |label: &str, enabled: bool| {
                button(label, ButtonKind::Primary, enabled && !busy, p)
                    .key(format!("contact-confirm-{id}"))
                    .on_press(move |s: &mut State| s.confirm_contact(kind))
            };
            match kind {
                ContactKind::Email => col()
                    .w_full()
                    .gap(10.0)
                    .child(status_line(&icons::MAIL, &tf!("profile.link_sent", address = pending.address), p.accent, p))
                    .child(row().gap(8.0).child(confirm(t!("profile.link_opened"), true)).child(cancel)),
                ContactKind::Phone => {
                    let code = field_input(&form.code, t!("profile.code_placeholder"), p)
                        .key(format!("contact-code-{id}"))
                        .on_edit(move |s: &mut State| &mut s.profile.contact_mut(kind).code)
                        .on_submit(move |s: &mut State, _| s.confirm_contact(kind));
                    col()
                        .w_full()
                        .gap(10.0)
                        .child(status_line(
                            &icons::PHONE,
                            &tf!("profile.code_sent", number = pending.address),
                            p.accent,
                            p,
                        ))
                        .child(
                            row()
                                .w_full()
                                .gap(8.0)
                                .center_y()
                                .child(col().grow(1.0).min_w(0.0).child(code))
                                .child(confirm(t!("profile.confirm_code"), !form.code.text.trim().is_empty()))
                                .child(cancel),
                        )
                }
            }
        }
    };
    let mut section = col().w_full().gap(12.0).child(list).child(adding);
    if let Some(error) = &form.error {
        section = section.child(text(error.clone()).font(font::BODY).color(p.danger).wrap());
    }
    section
}

/// This session's verification, and the account's recovery key.
fn account(s: &State, p: &Palette) -> El {
    let Some(account) = s.settings_account() else { return col() };
    let security = account.security.unwrap_or_default();
    let verification = if security.session_verified {
        status_line(&icons::CHECK, t!("account.verified"), p.success, p)
    } else {
        col()
            .w_full()
            .gap(12.0)
            .child(status_line(&icons::LOCK, t!("account.unverified"), p.warning, p))
            .child(text(t!("account.unverified_hint")).font(font::BODY).color(p.text_muted).wrap())
            .child(
                row()
                    .gap(8.0)
                    .child(
                        button(t!("account.verify_device"), ButtonKind::Primary, true, p)
                            .key("verify-device")
                            .on_press(State::verify_with_other_device),
                    )
                    .child(
                        button(t!("account.verify_key"), ButtonKind::Secondary, true, p)
                            .key("verify-key")
                            .on_press(State::open_recovery_key_entry),
                    ),
            )
            .child(
                row()
                    .gap(6.0)
                    .center_y()
                    .flex_wrap()
                    .child(text(t!("reset.lost_everything")).font(font::BODY).color(p.text_muted))
                    .child(
                        button(t!("reset.title_short"), ButtonKind::Link, true, p)
                            .key("reset-identity")
                            .on_press(State::start_identity_reset),
                    ),
            )
    };
    let recovery = match security.recovery {
        RecoveryStatus::Enabled => col()
            .w_full()
            .gap(12.0)
            .items_start()
            .child(status_line(&icons::KEY, t!("recovery.enabled"), p.success, p))
            .child(
                button(t!("recovery.reset"), ButtonKind::Secondary, security.session_verified, p)
                    .key("recovery-reset")
                    .on_press(|s: &mut State| s.set_up_recovery(true)),
            ),
        RecoveryStatus::Disabled => col()
            .w_full()
            .gap(12.0)
            .items_start()
            .child(status_line(&icons::KEY, t!("recovery.disabled"), p.warning, p))
            .child(
                button(t!("recovery.set_up"), ButtonKind::Primary, true, p)
                    .key("recovery-set-up")
                    .on_press(|s: &mut State| s.set_up_recovery(false)),
            ),
        RecoveryStatus::Incomplete => status_line(&icons::KEY, t!("recovery.incomplete"), p.warning, p),
        RecoveryStatus::Unknown => status_line(&icons::KEY, t!("recovery.unknown"), p.text_muted, p),
    };
    let oauth = account.summary.sign_in == SignInMethod::OAuth;
    let mut page = cards()
        .child(account_heading(account, &s.media, p))
        .child(setting_card(&icons::SHIELD, t!("account.session"), t!("account.session_hint"), verification, p))
        .child(setting_card(&icons::KEY, t!("recovery.title"), t!("recovery.hint"), recovery, p));
    if let Some(offline) = offline_device(security, p) {
        page =
            page.child(setting_card(&icons::CLOUD, t!("offline_device.title"), t!("offline_device.hint"), offline, p));
    }
    let keys = row()
        .gap(8.0)
        .child(
            button(t!("keys.export_button"), ButtonKind::Secondary, true, p)
                .key("keys-export")
                .on_press(State::export_room_keys),
        )
        .child(
            button(t!("keys.import_button"), ButtonKind::Secondary, true, p)
                .key("keys-import")
                .on_press(State::import_room_keys),
        );
    let (password_hint, password_label) = if oauth {
        (t!("password.hint_oauth"), t!("password.change_on_page"))
    } else {
        (t!("password.hint"), t!("password.change_title"))
    };
    let password = row().child(
        button(password_label, ButtonKind::Secondary, true, p)
            .key("password-change")
            .on_press(State::open_change_password),
    );
    let deactivate = row().child(
        button(t!("deactivate.button"), ButtonKind::Danger, true, p)
            .key("deactivate-account")
            .on_press(State::ask_to_deactivate),
    );
    page.child(setting_card(&icons::FILE, t!("keys.title"), t!("keys.hint"), keys, p))
        .child(setting_card(&icons::LOCK, t!("password.title"), password_hint, password, p))
        .child(setting_card(
            &icons::ALERT,
            t!("deactivate.heading"),
            if oauth { t!("deactivate.hint_oauth") } else { t!("deactivate.hint") },
            deactivate,
            p,
        ))
}

/// The offline device's state and the switch for it, where the server keeps one.
fn offline_device(security: matrix_core::Security, p: &Palette) -> Option<El> {
    let column = col().w_full().gap(12.0).items_start();
    match security.offline_device {
        OfflineDevice::On => Some(
            column.child(status_line(&icons::CLOUD, t!("offline_device.on"), p.success, p)).child(
                button(t!("offline_device.turn_off"), ButtonKind::Secondary, true, p)
                    .key("offline-device-off")
                    .on_press(|s: &mut State| s.set_offline_device(false)),
            ),
        ),
        OfflineDevice::Off => {
            // It keeps its key with the recovery key, from a verified session.
            let ready = security.session_verified && security.recovery == RecoveryStatus::Enabled;
            let mut column = column.child(status_line(&icons::CLOUD, t!("offline_device.off"), p.text_muted, p));
            if !ready {
                column =
                    column.child(text(t!("offline_device.needs_recovery")).font(font::BODY).color(p.text_muted).wrap());
            }
            Some(column.child(
                button(t!("offline_device.turn_on"), ButtonKind::Primary, ready, p).key("offline-device-on").on_press(
                    move |s: &mut State| {
                        if ready {
                            s.set_offline_device(true);
                        }
                    },
                ),
            ))
        }
        OfflineDevice::Unknown | OfflineDevice::Unsupported => None,
    }
}

/// Every signed-in session: this one first, each with its last activity, verification, and
/// ways to rename it or sign it out.
fn sessions(s: &State, p: &Palette) -> El {
    let Some(account) = s.settings_account() else { return col() };
    let list = match &account.sessions {
        None => col().child(text(t!("sessions.loading")).font(font::BODY).color(p.text_muted)),
        Some(sessions) => {
            sessions.iter().fold(col().w_full().gap(10.0), |list, session| list.child(session_card(session, p)))
        }
    };
    let mut page = cards().child(account_heading(account, &s.media, p));
    // A QR sign-in hands this session's secrets over: OAuth accounts, from a verified session.
    if account.summary.sign_in == SignInMethod::OAuth {
        let verified = account.security.is_some_and(|security| security.session_verified);
        let mut link = col().w_full().gap(10.0).items_start();
        if !verified {
            link = link.child(text(t!("link_device.verify_first")).font(font::BODY).color(p.text_muted).wrap());
        }
        link = link.child(
            button(t!("link_device.button"), ButtonKind::Primary, verified, p)
                .key("link-device")
                .on_press(State::link_device),
        );
        page = page.child(setting_card(&icons::QR, t!("link_device.title"), t!("link_device.hint"), link, p));
    }
    page.child(setting_card(&icons::LAPTOP, t!("settings.sessions"), t!("sessions.hint"), list, p))
}

fn session_card(session: &Session, p: &Palette) -> El {
    let name = session.name.clone().unwrap_or_else(|| session.device_id.clone());
    let mut details = vec![session.device_id.clone()];
    if let Some(at) = session.last_seen_ms {
        details.push(tf!(
            "sessions.last_seen",
            when = super::timeline::date_label(super::components::local_ms(at), super::components::local_ms(now_ms()))
        ));
    }
    if let Some(ip) = &session.last_seen_ip {
        details.push(ip.clone());
    }
    let mut badges = row().gap(6.0);
    if session.current {
        badges = badges.child(badge_chip(t!("sessions.this"), p.accent));
    }
    badges = badges.child(if session.verified {
        badge_chip(t!("sessions.verified"), p.success)
    } else {
        badge_chip(t!("sessions.unverified"), p.warning)
    });
    let (id, rename_name) = (session.device_id.clone(), name.clone());
    let mut actions = row().gap(4.0).child(
        icon_button(&icons::PENCIL, t!("sessions.rename"), false, p.text_muted, p)
            .key(format!("rename-{}", session.device_id))
            .on_press(move |s: &mut State| s.start_renaming_session(&id, &rename_name)),
    );
    if !session.current {
        let id = session.device_id.clone();
        actions = actions.child(
            button(t!("sessions.sign_out"), ButtonKind::Danger, true, p)
                .key(format!("end-{}", session.device_id))
                .on_press(move |s: &mut State| s.end_session(&id)),
        );
    }
    row()
        .key(format!("session-{}", session.device_id))
        .w_full()
        .padding(14.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .border(1.0, if session.current { p.accent } else { p.divider })
        .child(icon(&icons::LAPTOP, 22.0).color(p.text_muted))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(row().gap(8.0).center_y().child(text(name).font(font::BODY_STRONG).color(p.text)).child(badges))
                .child(text(details.join(" · ")).font(font::META).color(p.text_muted).truncate()),
        )
        .child(actions)
}

fn badge_chip(label: &str, color: Color) -> El {
    el().px(8.0)
        .py(2.0)
        .radius(10.0)
        .fill(color.alpha(0.15))
        .child(text(label.to_owned()).font(font::BADGE).color(color))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
