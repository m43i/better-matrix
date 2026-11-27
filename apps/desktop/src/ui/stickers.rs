//! The sticker and GIF picker, anchored to the composer's sticker button: two tabs. Stickers:
//! a search, the stickers sent lately, then each pack's under its name; a press sends one, and
//! the gear opens the sticker manager. GIFs: GIPHY's, searched with the user's own key (asked
//! for here until given), in two columns; a press sends one. GIPHY is credited as it asks.

use rune::{Align, Color, El, ScrollSpec, Side, col, el, icon, row, text};

use super::components::{ButtonKind, button, field_input, icon_button, popover_panel};
use super::composer::sticker_button;
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{ComposerTarget, GifKey, PickerTab, State};
use matrix_core::{FoundGif, ImagePack, PackImage};

/// The side of a sticker in the grid.
const STICKER: f32 = 72.0;
/// The picker's width.
const WIDTH: f32 = 380.0;
/// The width of a GIF column.
const GIF_COLUMN: f32 = (WIDTH - 16.0 - 6.0) / 2.0;
/// The height of the picker's scrolling content.
const CONTENT_HEIGHT: f32 = 320.0;

pub fn picker(s: &State, target: ComposerTarget, p: &Palette) -> El {
    let tab = s.picker.tab;
    let tab_button = |which: PickerTab, label: &str, key: &str| {
        let on = tab == which;
        el().key(key.to_owned())
            .h(28.0)
            .px(12.0)
            .radius(6.0)
            .center()
            .fill(if on { p.hover.alpha(p.hover.a * 3.0) } else { Color::TRANSPARENT })
            .hover(p.hover.alpha(p.hover.a * 2.0))
            .role("tab")
            .checked(on)
            .label(label.to_owned())
            .cursor(rune::Cursor::Pointer)
            .child(text(label.to_owned()).font(font::BODY_STRONG).color(if on { p.text } else { p.text_muted }))
            .on_press(move |s: &mut State| s.show_picker_tab(which))
    };
    let mut header = row()
        .w_full()
        .px(8.0)
        .pt(8.0)
        .pb(4.0)
        .gap(4.0)
        .center_y()
        .child(tab_button(PickerTab::Stickers, t!("picker.stickers"), "picker-tab-stickers"))
        .child(tab_button(PickerTab::Gifs, t!("picker.gifs"), "picker-tab-gifs"))
        .child(el().grow(1.0));
    if tab == PickerTab::Stickers {
        header = header.child(
            icon_button(&icons::SETTINGS, t!("picker.manage"), false, p.text_muted, p)
                .key("picker-manage")
                .help(t!("picker.manage"))
                .on_press(State::open_pack_manager),
        );
    }
    let body = match tab {
        PickerTab::Stickers => stickers(s, target, p),
        PickerTab::Gifs => gifs(s, target, p),
    };
    let panel = popover_panel(p).key("sticker-picker").w(WIDTH).preserve_focus_on_press(true).child(header).child(body);
    col().child(panel).anchored(sticker_button(target), Side::Top, Align::Start, 6.0)
}

/// What a pack is called in the picker and the manager: the user's own pack, unless it names
/// itself, is "Your stickers".
pub fn pack_name(pack: &ImagePack) -> String {
    if pack.id == "user" && pack.name.trim().is_empty() { t!("packs.yours").to_owned() } else { pack.name.clone() }
}

/// One sticker to press.
fn sticker_tile(s: &State, target: ComposerTarget, key: String, image: &PackImage, p: &Palette) -> El {
    let picture = el().size(STICKER - 8.0, STICKER - 8.0);
    let picture = match s.media.image(&image.media) {
        Some(loaded) => picture.pixels_fit(loaded.pixels(), rune::SurfaceFit::Fit),
        None => picture.radius(8.0).fill(p.hover),
    };
    let sent = image.clone();
    el().key(key)
        .size(STICKER, STICKER)
        .radius(8.0)
        .center()
        .hover(p.hover.alpha(p.hover.a * 3.0))
        .role("button")
        .label(image.body.clone())
        .help(image.body.clone())
        .cursor(rune::Cursor::Pointer)
        .child(picture)
        .on_press_tx(move |s: &mut State, tx| s.send_sticker(target, sent.clone(), tx))
}

fn stickers(s: &State, target: ComposerTarget, p: &Palette) -> El {
    let filter = s.picker.filter.text.trim();
    let packs = match (s.nav.account.as_ref(), s.active_room_id()) {
        (Some(account), Some(room_id)) => s.sticker_packs(account, room_id, filter),
        _ => Vec::new(),
    };
    let search = field_input(&s.picker.filter, t!("picker.search_stickers"), p)
        .key("sticker-filter")
        .on_edit(|s: &mut State| &mut s.picker.filter);
    let mut grid = col().w_full().px(8.0).pb(8.0).gap(6.0);
    let recent = match (s.nav.account.as_ref(), s.active_room_id()) {
        (Some(account), Some(room_id)) => s.recent_stickers_here(account, room_id),
        _ => Vec::new(),
    };
    if filter.is_empty() && !recent.is_empty() {
        let set = recent.iter().fold(row().w_full().flex_wrap().gap(4.0), |set, image| {
            set.child(sticker_tile(s, target, format!("sticker-recent-{}", image.url), image, p))
        });
        grid = grid.child(section(t!("picker.recent"), p)).child(set);
    }
    if packs.is_empty() {
        let (message, offer) =
            if filter.is_empty() { (t!("stickers.none"), true) } else { (t!("picker.no_match"), false) };
        grid = grid.child(text(message).font(font::BODY).color(p.text_muted).px(4.0).py(12.0).wrap());
        if offer {
            grid = grid.child(
                button(t!("picker.add_stickers"), ButtonKind::Secondary, true, p)
                    .key("picker-add-stickers")
                    .on_press(State::open_pack_manager),
            );
        }
    }
    for (pack, stickers) in packs {
        let set = stickers.into_iter().fold(row().w_full().flex_wrap().gap(4.0), |set, image| {
            set.child(sticker_tile(s, target, format!("sticker-{}-{}", pack.id, image.shortcode), image, p))
        });
        grid = grid.child(section(&pack_name(pack), p)).child(set);
    }
    col().w_full().child(el().w_full().px(8.0).pb(6.0).child(search)).child(
        col().key("sticker-grid").w_full().max_h(CONTENT_HEIGHT).scroll(ScrollSpec { stick_bottom: false }).child(grid),
    )
}

fn section(title: &str, p: &Palette) -> El {
    text(title.to_owned()).font(font::SECTION).color(p.text_muted).px(4.0).pt(4.0)
}

/// The GIF tab: GIPHY's GIFs, or what a key is and where to get one.
fn gifs(s: &State, target: ComposerTarget, p: &Palette) -> El {
    if s.picker.giphy == GifKey::Loading {
        return text(t!("picker.key_loading")).font(font::META).color(p.text_muted).padding(12.0);
    }
    if s.asks_gif_key() {
        return gif_key_form(s, "picker-gif-key", p).padding(12.0);
    }
    let search = s.picker.gifs.field.text.trim().is_empty();
    let field = field_input(&s.picker.gifs.field, t!("picker.search_gifs"), p)
        .key("gif-search")
        .on_edit(|s: &mut State| &mut s.picker.gifs.field)
        .on_submit(|s: &mut State, _| s.submit_gif_search());
    let found = &s.picker.gifs;
    let mut content = col().w_full().px(8.0).pb(8.0).gap(6.0);
    if search && !found.gifs.is_empty() {
        content = content.child(section(t!("picker.trending"), p));
    }
    // Two to a row, in GIPHY's order: reading and keyboard order follow it too.
    for pair in found.gifs.chunks(2) {
        let tiles = pair.iter().map(|gif| gif_tile(s, target, gif, p));
        content = content.child(row().w_full().gap(6.0).items_start().children(tiles));
    }
    if found.searching() {
        content = content.child(text(t!("picker.searching")).font(font::META).color(p.text_muted).px(4.0));
    } else if let Some(error) = &found.error {
        content = content
            .child(text(crate::i18n::tf!("picker.gif_error", error = error)).font(font::META).color(p.danger).wrap())
            .child(
                row()
                    .gap(8.0)
                    .child(
                        button(t!("picker.try_again"), ButtonKind::Secondary, true, p)
                            .key("picker-gif-retry")
                            .on_press(State::retry_gifs),
                    )
                    // A refused key is one reason it fails: it can be replaced from here.
                    .child(
                        button(t!("picker.replace_key"), ButtonKind::Secondary, true, p)
                            .key("picker-replace-key")
                            .on_press(State::replace_gif_key),
                    ),
            );
    } else if found.gifs.is_empty() && found.searched().is_some() {
        content = content.child(text(t!("picker.no_gifs")).font(font::BODY).color(p.text_muted).px(4.0).py(12.0));
    } else if s.more_gifs_offered() {
        content = content.child(
            button(t!("picker.more_gifs"), ButtonKind::Secondary, true, p).key("gif-more").on_press(State::more_gifs),
        );
    }
    col()
        .w_full()
        .child(el().w_full().px(8.0).pb(6.0).child(field))
        .child(
            col().key("gif-grid").w_full().h(CONTENT_HEIGHT).scroll(ScrollSpec { stick_bottom: false }).child(content),
        )
        .child(giphy_credit(s, p))
}

/// "Powered by GIPHY", where its GIFs are browsed, as GIPHY asks.
fn giphy_credit(s: &State, p: &Palette) -> El {
    let (w, h) = crate::state::GIPHY_MARK_SIZE;
    // GIPHY's own mark; its words until the picture is decoded (or headless).
    let mark = match s.media.image(&crate::state::giphy_mark()) {
        Some(mark) => el().size(18.0 * w as f32 / h as f32, 18.0).pixels_fit(mark.pixels(), rune::SurfaceFit::Fit),
        None => text(t!("picker.powered_by_giphy")).font(font::META).color(p.text_muted),
    };
    row()
        .key("giphy-credit")
        .w_full()
        .px(12.0)
        .py(6.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], p.divider)
        .child(el().grow(1.0))
        .child(mark.role("img").label(t!("picker.powered_by_giphy")))
}

/// The height a GIF is shown at in its column.
fn gif_height(gif: &FoundGif) -> f32 {
    match gif.preview_size {
        Some((w, h)) if w > 0 => (GIF_COLUMN * h as f32 / w as f32).clamp(60.0, 260.0),
        _ => GIF_COLUMN * 0.75,
    }
}

fn gif_tile(s: &State, target: ComposerTarget, gif: &FoundGif, p: &Palette) -> El {
    let frame = el()
        .key(format!("gif-{}", gif.id))
        .size(GIF_COLUMN, gif_height(gif))
        .radius(6.0)
        .clip_rounded(6.0)
        .fill(p.hover)
        .role("button")
        .label(gif.title.clone())
        .help(gif.title.clone())
        .cursor(rune::Cursor::Pointer)
        .instant_changes();
    let frame = match s.picker.players.player(&gif.preview) {
        Some(player) => frame.pixels_fit(player.pixels(), rune::SurfaceFit::Fill),
        None => frame,
    };
    let (sent, hovered) = (gif.clone(), gif.preview.clone());
    frame
        .on_hover_change(move |s: &mut State, inside, _| s.hover_gif(&hovered, inside))
        .on_press_tx(move |s: &mut State, tx| s.send_gif(target, sent.clone(), tx))
}

/// A field for a GIPHY API key, empty (a key kept is never shown), with Save, and Cancel while
/// replacing the one kept. `key` names the field.
pub fn gif_key_field(s: &State, key: &str, p: &Palette) -> El {
    let field = field_input(&s.picker.key, t!("picker.key_placeholder"), p)
        .key(key.to_owned())
        .label(t!("picker.key_title"))
        .on_edit(|s: &mut State| &mut s.picker.key)
        .on_submit(|s: &mut State, _| s.save_gif_key());
    let save = button(t!("picker.save_key"), ButtonKind::Primary, !s.picker.key.text.trim().is_empty(), p)
        .key(format!("{key}-save"))
        .on_press(State::save_gif_key);
    let cancel = s.picker.replacing.then(|| {
        button(t!("picker.cancel_key"), ButtonKind::Secondary, true, p)
            .key(format!("{key}-cancel"))
            .on_press(State::cancel_gif_key)
    });
    // The field takes what's left: a long key scrolls inside it rather than pushing Save out.
    row().w_full().gap(8.0).center_y().child(col().grow(1.0).min_w(0.0).child(field)).children(cancel).child(save)
}

/// Asks for the user's GIPHY API key: what it is for, where to make one, a field to put it in.
pub fn gif_key_form(s: &State, key: &str, p: &Palette) -> El {
    col()
        .w_full()
        .gap(10.0)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(icon(&icons::IMAGE, 18.0).color(p.text_muted))
                .child(text(t!("picker.key_title")).font(font::BODY_STRONG).color(p.text)),
        )
        .child(text(t!("picker.key_hint")).font(font::BODY).color(p.text_muted).wrap())
        .child(
            button(t!("picker.get_key"), ButtonKind::Link, true, p)
                .key(format!("{key}-get"))
                .on_press_tx(|s: &mut State, tx| s.open_giphy_keys(tx)),
        )
        .child(gif_key_field(s, key, p))
}
