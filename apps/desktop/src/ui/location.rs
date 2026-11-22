//! The location dialog: a search for a place by name (or typed coordinates), the places found,
//! and a map of the one chosen, from OpenStreetMap, credited as its licence asks.

use rune::{El, col, el, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{MAP_PIXELS, State};

pub fn dialog_for(s: &State, p: &Palette) -> El {
    let form = &s.location;
    let search = field_input(&form.search, t!("location.search_placeholder"), p)
        .key("location-search")
        .grow(1.0)
        .on_edit(|s: &mut State| &mut s.location.search)
        .on_submit(|s: &mut State, _| s.search_places());
    let find = button(t!("location.search"), ButtonKind::Secondary, form.searching.is_none(), p)
        .key("location-find")
        .on_press(State::search_places);
    let mut content = col().w_full().gap(12.0).child(row().w_full().gap(8.0).center_y().child(search).child(find));
    if form.searching.is_some() {
        content = content.child(text(t!("location.searching")).font(font::META).color(p.text_muted));
    }
    if let Some(error) = &form.error {
        content = content.child(text(error.clone()).font(font::META).color(p.danger).wrap());
    }
    if let Some(places) = &form.places {
        if places.is_empty() {
            content = content.child(text(t!("location.none")).font(font::BODY).color(p.text_muted));
        }
        let mut list = col().w_full().gap(2.0);
        for (index, place) in places.iter().enumerate() {
            let chosen = form.chosen.as_ref().is_some_and(|chosen| chosen.geo_uri == place.geo_uri);
            let picked = place.clone();
            list = list.child(
                row()
                    .key(format!("location-place-{index}"))
                    .w_full()
                    .px(8.0)
                    .py(6.0)
                    .gap(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(if chosen { p.accent.alpha(0.12) } else { rune::Color::TRANSPARENT })
                    .hover(p.hover)
                    .role("radio")
                    .checked(chosen)
                    .focusable(true)
                    .cursor(rune::Cursor::Pointer)
                    .child(icon(&icons::MAP_PIN, 16.0).color(if chosen { p.accent } else { p.text_muted }))
                    .child(
                        text(place.name.clone())
                            .font(font::BODY)
                            .color(p.text)
                            .grow(1.0)
                            .min_w(0.0)
                            .max_lines(2)
                            .truncate(),
                    )
                    .on_press(move |s: &mut State| s.choose_place(picked.clone())),
            );
        }
        content = content.child(list);
    }
    if let Some(place) = &form.chosen {
        let (w, h) = (412.0, 412.0 * MAP_PIXELS.1 as f32 / MAP_PIXELS.0 as f32);
        let map = el()
            .key("location-map")
            .size(w, h)
            .radius(8.0)
            .fill(p.bg)
            .border(1.0, p.divider)
            .role("img")
            .label(t!("location.map"));
        let map = match place.map.as_ref().and_then(|map| s.media.image(map)) {
            Some(image) => map.clip_rounded(8.0).pixels_fit(image.pixels(), rune::SurfaceFit::Fill),
            None => map.center().child(icon(&icons::MAP_PIN, 28.0).color(p.text_muted)),
        };
        content = content.child(map).child(text(t!("location.credit")).font(font::META).color(p.text_muted));
    }
    dialog(
        "location-dialog",
        t!("location.title"),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("location-cancel")
                .on_press(State::close_modal),
            button(t!("location.send"), ButtonKind::Primary, form.chosen.is_some(), p)
                .key("location-send")
                .on_press_tx(State::send_location),
        ],
        p,
    )
}
