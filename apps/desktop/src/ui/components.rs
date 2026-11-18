//! The building blocks every screen is made of. Components take data and a [`Palette`] and
//! return an [`El`]; the caller attaches keys and handlers, so one component serves every
//! screen and state type.

use rune::{Color, El, Icon, Image, SurfaceFit, TextEdit, col, el, icon, row, text, text_input};

use super::theme::{Palette, font, identity_color};
use crate::i18n::{t, tf};

/// A square avatar with rounded corners, showing initials over the identity's colour. Used for
/// people, spaces and accounts.
pub fn avatar(identity: &str, name: &str, size: f32) -> El {
    let mut letters = matrix_core::model::initials(name);
    // Below this size a second letter no longer fits.
    if size < 24.0 {
        letters.truncate(letters.chars().next().map_or(0, char::len_utf8));
    }
    el().size(size, size)
        .radius(size * 0.22)
        .fill(identity_color(identity))
        .center()
        .child(text(letters).font(rune::TextStyle::new(size * 0.45, 700.0)).color(Color::WHITE))
}

/// An avatar showing `picture` once it is loaded, initials until then.
pub fn picture_avatar(identity: &str, name: &str, picture: Option<&Image>, size: f32) -> El {
    match picture {
        Some(image) => el().size(size, size).radius(size * 0.22).pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => avatar(identity, name, size),
    }
}

/// An avatar for a person, room or space: its picture once loaded, its initials until then or
/// without one.
pub fn media_avatar(
    identity: &str,
    name: &str,
    picture: Option<&matrix_core::MediaRef>,
    media: &crate::state::MediaCache,
    size: f32,
) -> El {
    picture_avatar(identity, name, picture.and_then(|picture| media.image(picture)), size)
}

/// A signed-in account's avatar: its profile picture, or its initials.
pub fn account_avatar(summary: &matrix_core::AccountSummary, media: &crate::state::MediaCache, size: f32) -> El {
    let picture = summary.avatar.as_ref().and_then(|media_ref| media.image(media_ref));
    picture_avatar(summary.id.as_str(), &summary.display_name, picture, size)
}

/// A small dot marking unread activity, placed over a tile's top-right corner.
pub fn unread_dot(tile_size: f32, ring: Color, p: &Palette) -> El {
    el().size(10.0, 10.0).radius(5.0).fill(p.mention).border(2.0, ring).absolute(tile_size - 7.0, -3.0)
}

/// A dot showing the user's status: filled green when online, amber when away, a hollow ring when
/// hidden (as others see them: offline). `ring` is the surface it sits on.
pub fn status_dot(status: matrix_core::Status, size: f32, ring: Color, p: &Palette) -> El {
    let dot = el().size(size, size).radius(size / 2.0);
    match status {
        matrix_core::Status::Online => dot.fill(p.success).border(2.0, ring),
        matrix_core::Status::Away => dot.fill(p.warning).border(2.0, ring),
        matrix_core::Status::Hidden => dot.fill(ring).border(2.0, p.text_muted),
    }
}

/// A dot showing whether someone is around, as [`status_dot`] draws the user's own: green
/// online, amber away, a hollow ring offline. None when their server shares no presence.
pub fn presence_dot(presence: matrix_core::Presence, size: f32, ring: Color, p: &Palette) -> Option<El> {
    let status = match presence {
        matrix_core::Presence::Online => matrix_core::Status::Online,
        matrix_core::Presence::Away => matrix_core::Status::Away,
        matrix_core::Presence::Offline => matrix_core::Status::Hidden,
        matrix_core::Presence::Unknown => return None,
    };
    Some(status_dot(status, size, ring, p))
}

/// `face`, an avatar `size` across, with the dot of what its person `shown` over its
/// bottom-right corner. `ring` is the surface it sits on.
pub fn with_presence(face: El, size: f32, shown: Option<&matrix_core::PersonPresence>, ring: Color, p: &Palette) -> El {
    let dot = (size * 0.33).round().clamp(9.0, 14.0);
    let at = size - dot + 2.0;
    el().size(size, size)
        .child(face)
        .children(shown.and_then(|shown| presence_dot(shown.presence, dot, ring, p)).map(|d| d.absolute(at, at)))
}

/// What someone shows, in words: "Away", or "Away · 🌴 On vacation". Nothing when their server
/// shares no presence.
pub fn presence_text(shown: &matrix_core::PersonPresence) -> Option<String> {
    let presence = match shown.presence {
        matrix_core::Presence::Online => t!("presence.online"),
        matrix_core::Presence::Away => t!("presence.away"),
        matrix_core::Presence::Offline => t!("presence.offline"),
        matrix_core::Presence::Unknown => return None,
    };
    Some(match shown.status_message.as_deref() {
        Some(message) => tf!("presence.with_message", presence = presence, message = message),
        None => presence.to_owned(),
    })
}

/// A square button showing an icon; `active` draws it pressed (a toggled format, say).
pub fn icon_button(glyph: &'static Icon, label: &str, active: bool, color: Color, p: &Palette) -> El {
    el().size(28.0, 28.0)
        .radius(6.0)
        .center()
        .fill(if active { p.hover.alpha(p.hover.a * 3.0) } else { Color::TRANSPARENT })
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(label.to_owned())
        .focusable(true)
        .child(icon(glyph, 18.0).color(if active { p.text } else { color }))
}

/// A floating panel for popovers; the caller anchors it.
pub fn popover_panel(p: &Palette) -> El {
    col().w(320.0).py(6.0).radius(10.0).fill(p.surface).border(1.0, p.divider).shadow(
        8.0,
        24.0,
        Color::BLACK.alpha(0.35),
    )
}

/// How strongly a control speaks: normally, or as a warning for something that can't be undone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Danger,
}

/// One row in a popover menu: a leading element, a title, an optional subtitle and trailing mark.
pub fn menu_row(leading: El, title: &str, subtitle: Option<&str>, trailing: Option<El>, p: &Palette) -> El {
    menu_row_toned(leading, title, subtitle, trailing, Tone::Normal, p)
}

/// A one-line menu action with an icon, in the given tone.
pub fn menu_action(glyph: &'static Icon, title: &str, tone: Tone, p: &Palette) -> El {
    let color = match tone {
        Tone::Normal => p.text_muted,
        Tone::Danger => p.danger,
    };
    let leading = el().size(28.0, 28.0).center().child(icon(glyph, 18.0).color(color));
    menu_row_toned(leading, title, None, None, tone, p)
}

/// A menu row that opens a submenu: a chevron at the end.
pub fn submenu_row(glyph: &'static Icon, title: &str, open: bool, p: &Palette) -> El {
    let chevron = icon(&super::icons::CHEVRON_RIGHT, 16.0).color(p.text_muted);
    let row = menu_row(menu_icon(glyph, p), title, None, Some(chevron), p).expanded(open);
    if open { row.fill(p.hover.alpha(p.hover.a * 2.0)) } else { row }
}

fn menu_row_toned(
    leading: El,
    title: &str,
    subtitle: Option<&str>,
    trailing: Option<El>,
    tone: Tone,
    p: &Palette,
) -> El {
    let title_color = match tone {
        Tone::Normal => p.text,
        Tone::Danger => p.danger,
    };
    let mut titles =
        col().grow(1.0).min_w(0.0).child(text(title.to_owned()).font(font::BODY_STRONG).color(title_color).wrap());
    if let Some(subtitle) = subtitle {
        titles = titles.child(text(subtitle.to_owned()).font(font::META).color(p.text_muted).wrap());
    }
    let r = row()
        .w_full()
        .min_h(36.0)
        .px(12.0)
        .py(6.0)
        .gap(10.0)
        .center_y()
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("menuitem")
        .cursor(rune::Cursor::Pointer)
        .label(title.to_owned())
        .focusable(true)
        .child(leading)
        .child(titles);
    match trailing {
        Some(t) => r.child(t),
        None => r,
    }
}

/// A leading icon for a menu row, in the row's muted colour.
pub fn menu_icon(glyph: &'static Icon, p: &Palette) -> El {
    el().size(28.0, 28.0).center().child(icon(glyph, 18.0).color(p.text_muted))
}

/// One option of a single choice (a radio button with a title and an optional hint): settings
/// lists. The caller adds `key` and `on_press`.
pub fn choice_row(title: &str, hint: Option<&str>, selected: bool, p: &Palette) -> El {
    let ring = if selected { p.accent } else { p.text_muted };
    let mut radio = el().size(18.0, 18.0).radius(9.0).border(2.0, ring).center();
    if selected {
        radio = radio.child(el().size(8.0, 8.0).radius(4.0).fill(p.accent));
    }
    let mut titles =
        col().grow(1.0).min_w(0.0).gap(2.0).child(text(title.to_owned()).font(font::BODY_STRONG).color(p.text));
    if let Some(hint) = hint {
        titles = titles.child(text(hint.to_owned()).font(font::META).color(p.text_muted).wrap());
    }
    row()
        .w_full()
        .px(14.0)
        .py(10.0)
        .gap(12.0)
        .center_y()
        .radius(8.0)
        .border(1.0, if selected { p.accent } else { p.divider })
        .fill(if selected { p.accent.alpha(0.08) } else { Color::TRANSPARENT })
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("radio")
        .cursor(rune::Cursor::Pointer)
        .label(title.to_owned())
        .checked(selected)
        .focusable(true)
        .child(radio)
        .child(titles)
}

/// How wide a select and its list are.
pub const SELECT_WIDTH: f32 = 380.0;

/// A select, closed: the current choice (and its explanation) in a bordered button with a
/// chevron. Pressing it opens its list ([`select_menu`]).
pub fn select_button(title: &str, hint: Option<&str>, open: bool, p: &Palette) -> El {
    let mut titles =
        col().grow(1.0).min_w(0.0).gap(2.0).child(text(title.to_owned()).font(font::BODY).color(p.text).truncate());
    if let Some(hint) = hint {
        titles = titles.child(text(hint.to_owned()).font(font::META).color(p.text_muted).truncate());
    }
    row()
        .w_full()
        .max_w(SELECT_WIDTH)
        .min_h(40.0)
        .px(12.0)
        .py(8.0)
        .gap(10.0)
        .center_y()
        .radius(8.0)
        .fill(p.bg)
        .border(1.0, if open { p.accent } else { p.divider })
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("combobox")
        .cursor(rune::Cursor::Pointer)
        .label(title.to_owned())
        .expanded(open)
        .focusable(true)
        .child(titles)
        .child(icon(&super::icons::CHEVRON_DOWN, 16.0).color(p.text_muted))
}

/// One option in an open select: a check when it's the current one, its title and explanation.
pub fn select_option(title: &str, hint: Option<&str>, selected: bool, p: &Palette) -> El {
    let check =
        el().size(20.0, 20.0).center().children(selected.then(|| icon(&super::icons::CHECK, 16.0).color(p.accent)));
    let mut titles = col().grow(1.0).min_w(0.0).gap(2.0).child(
        text(title.to_owned()).font(if selected { font::BODY_STRONG } else { font::BODY }).color(p.text).truncate(),
    );
    if let Some(hint) = hint {
        titles = titles.child(text(hint.to_owned()).font(font::META).color(p.text_muted).wrap());
    }
    row()
        .w_full()
        .px(10.0)
        .py(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("option")
        .cursor(rune::Cursor::Pointer)
        .checked(selected)
        .focusable(true)
        .label(title.to_owned())
        .child(check)
        .child(titles)
}

/// An open select's list, under its button `anchor`; long lists scroll.
pub fn select_menu(anchor: String, options: Vec<El>, p: &Palette) -> El {
    let list = col().w_full().padding(4.0).gap(2.0).children(options);
    let panel = popover_panel(p)
        .key("select-menu")
        .w(SELECT_WIDTH)
        .py(0.0)
        .child(col().w_full().max_h(320.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(list));
    col().child(panel).anchored(anchor, rune::Side::Bottom, rune::Align::Start, 4.0)
}

/// A titled block in a settings page: icon and heading, explanation, then its controls.
pub fn setting_group(glyph: &'static Icon, title: &str, hint: &str, controls: El, p: &Palette) -> El {
    let heading = row()
        .gap(10.0)
        .center_y()
        .child(icon(glyph, 20.0).color(p.text_muted))
        .child(text(title.to_owned()).font(font::HEADER).color(p.text));
    col()
        .w_full()
        .gap(8.0)
        .child(heading)
        .child(text(hint.to_owned()).font(font::BODY).color(p.text_muted).wrap())
        .child(controls.mt(4.0))
}

/// A dialog card for the modal layer: a title, its content, and buttons at the bottom right.
pub fn dialog(key: &str, title: &str, content: El, buttons: Vec<El>, p: &Palette) -> El {
    col()
        .key(key.to_owned())
        .w(460.0)
        .padding(24.0)
        .gap(14.0)
        .radius(14.0)
        .fill(p.surface)
        .border(1.0, p.text_muted.alpha(0.35))
        .shadow(16.0, 48.0, Color::BLACK.alpha(0.5))
        .role("dialog")
        .label(title.to_owned())
        // Two lines at most (a title may carry a long name): the rest is the content's.
        .child(text(title.to_owned()).font(font::HEADER).color(p.text).max_lines(2))
        // In a window too short for it, the content scrolls and the buttons stay in view.
        .child(
            col()
                .key(format!("{key}-content"))
                .w_full()
                .shrink()
                .min_h(0.0)
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(content),
        )
        .child(row().w_full().gap(8.0).mt(8.0).justify_end().children(buttons))
}

/// A short status line with an icon: verified, needs attention.
pub fn status_line(glyph: &'static Icon, message: &str, color: Color, p: &Palette) -> El {
    row()
        .w_full()
        .gap(10.0)
        .center_y()
        .padding(12.0)
        .radius(8.0)
        .fill(color.alpha(0.12))
        .child(icon(glyph, 18.0).color(color))
        .child(text(message.to_owned()).font(font::BODY_STRONG).color(p.text).grow(1.0).wrap())
}

/// A thin rule between groups in a popover or toolbar.
pub fn divider(p: &Palette) -> El {
    el().w_full().h(1.0).my(6.0).fill(p.divider)
}

/// A pill with a count, for unread mentions. Hidden at zero.
pub fn badge(count: u64, p: &Palette) -> Option<El> {
    (count > 0).then(|| {
        let label = if count > 99 { "99+".to_owned() } else { count.to_string() };
        el().h(18.0)
            .min_w(18.0)
            .px(6.0)
            .radius(9.0)
            .fill(p.mention)
            .center()
            .child(text(label).font(font::BADGE).color(Color::WHITE))
    })
}

/// How prominent a navigation row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavEmphasis {
    Normal,
    Unread,
    Selected,
    /// Muted: dimmer than normal.
    Muted,
}

/// One row in the sidebar: a leading mark, a label and an optional status mark. Something new
/// in it lights it up and sets its label in bold; how many mentions is its section's to say.
pub fn nav_row(leading: El, label: &str, emphasis: NavEmphasis, status: Option<El>, p: &Palette) -> El {
    let (fill, color, style) = match emphasis {
        NavEmphasis::Selected => (p.sidebar_selected, p.sidebar_selected_text, font::NAV_STRONG),
        NavEmphasis::Unread => (p.sidebar_unread, p.sidebar_text, font::NAV_STRONG),
        NavEmphasis::Normal => (Color::TRANSPARENT, p.sidebar_text_muted, font::NAV),
        NavEmphasis::Muted => (Color::TRANSPARENT, p.sidebar_text_muted.alpha(p.sidebar_text_muted.a * 0.6), font::NAV),
    };
    let mut r = row()
        .w_full()
        .h(30.0)
        .px(12.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .fill(fill)
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(label.to_owned())
        .focusable(true)
        .child(leading)
        .child(text(label.to_owned()).font(style).color(color).grow(1.0).truncate());
    if emphasis != NavEmphasis::Selected {
        r = r.hover(p.sidebar_hover);
    }
    if let Some(status) = status {
        r = r.child(status);
    }
    r
}

/// Button styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    /// Confirms something that can't be undone.
    Danger,
    /// A way out or aside, set as a link inside content: "Forgot your recovery key?".
    Link,
}

/// A labelled button; the caller adds `key` and `on_press`/`on_press_tx`.
pub fn button(label: &str, kind: ButtonKind, enabled: bool, p: &Palette) -> El {
    let (fill, color, border) = match kind {
        ButtonKind::Primary => (p.accent, p.on_accent, Color::TRANSPARENT),
        ButtonKind::Secondary => (Color::TRANSPARENT, p.text, p.divider),
        ButtonKind::Danger => (p.danger, Color::WHITE, Color::TRANSPARENT),
        ButtonKind::Link => (Color::TRANSPARENT, p.accent, Color::TRANSPARENT),
    };
    el().h(if kind == ButtonKind::Link { 24.0 } else { 36.0 })
        .px(if kind == ButtonKind::Link { 0.0 } else { 16.0 })
        .radius(8.0)
        .fill(fill)
        .border(1.0, border)
        .center()
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(label.to_owned())
        .focusable(enabled)
        .opacity(if enabled { 1.0 } else { 0.55 })
        .child(text(label.to_owned()).font(font::BODY_STRONG).color(color))
}

/// A labelled single-line field; the caller adds `key`, `on_edit` and `on_submit` to the
/// returned input.
pub fn field_input(edit: &TextEdit, placeholder: &str, p: &Palette) -> El {
    text_input(edit, font::BODY)
        .single_line()
        .placeholder(placeholder.to_owned(), p.text_muted)
        .color(p.text)
        .w_full()
        .px(12.0)
        .py(10.0)
        .radius(8.0)
        .fill(p.bg)
        .border(1.0, p.divider)
}

/// A field with its label above it.
pub fn labelled(label: &str, input: El, p: &Palette) -> El {
    col().w_full().gap(6.0).child(text(label.to_owned()).font(font::SECTION).color(p.text)).child(input)
}

/// A UTC time as the same reading on the user's clock: shifted by the local time zone's offset
/// at that moment (so summer time applies to summer dates). Times and dates show in these.
pub fn local_ms(utc_ms: u64) -> u64 {
    use chrono::{Offset, TimeZone};
    let Ok(ms) = i64::try_from(utc_ms) else { return utc_ms };
    let Some(at) = chrono::Local.timestamp_millis_opt(ms).single() else { return utc_ms };
    let offset = i64::from(at.offset().fix().local_minus_utc()) * 1000;
    u64::try_from(ms + offset).unwrap_or(utc_ms)
}

/// `HH:MM` of a time from [`local_ms`].
pub fn clock_time(timestamp_ms: u64) -> String {
    let minutes = timestamp_ms / 60_000;
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

/// When something happened, as a list shows it: the day in the user's language, then the time.
pub fn when(ms: u64) -> String {
    let now = crate::state::now_ms();
    let (at, now) = (local_ms(ms), local_ms(now));
    format!("{}, {}", super::timeline::date_label(at, now), clock_time(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_time_should_format_hours_and_minutes() {
        assert_eq!(clock_time(((9 * 60 + 5) * 60 + 30) * 1000), "09:05");
    }

    #[test]
    fn clock_time_should_wrap_at_midnight() {
        assert_eq!(clock_time(25 * 3_600_000), "01:00");
    }
}
