//! The location dialog: a search for a place by name (or typed coordinates), the places found,
//! and a MapLibre preview of the chosen place.

use rune::{El, col, el, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::t;
use crate::state::{MediaCache, Modal, State};
use matrix_core::{AccountId, Location, MessageContent, TimelineKey};

pub fn dialog_for(s: &State, p: &Palette) -> El {
    let form = &s.location;
    let search = field_input(&form.search, t!("location.search_placeholder"), p)
        .key("location-search")
        .label(t!("location.search_placeholder").to_owned())
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
    if let (Some(place), Some(owner)) = (&form.chosen, s.active_account()) {
        let face = super::components::account_avatar(&owner.summary, &s.media, 36.0);
        content = content
            .child(map_view(
                "location-picker",
                &owner.summary.id,
                &place.geo_uri,
                if form.estimated == Some(true) { 10 } else { 15 },
                412.0,
                face,
                &s.media,
                s.settings.show_maps,
                p,
            ))
            .child(text(place.name.clone()).font(font::BODY_STRONG).color(p.text).wrap());
    }
    content = content.child(
        button(t!("location.use_device"), ButtonKind::Secondary, form.locating.is_none(), p)
            .key("location-device")
            .on_press(|s: &mut State| s.locate_position(true)),
    );
    if let (Some(account), Some(key)) = (&s.nav.account, form.target.and_then(|target| s.composer_key(target)))
        && key.thread.is_none()
    {
        if let Some(request) = s.live_share(account, &key.room_id) {
            content = content.child(stop_button(request, p));
        } else {
            content = content.child(
                button(t!("location.start_live"), ButtonKind::Secondary, form.estimated == Some(false), p)
                    .key("location-live")
                    .on_press(State::start_live_location),
            );
        }
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

pub fn title(location: &Location) -> String {
    if let Some(until) = location.live_until {
        return if until > crate::state::now_ms() { t!("location.live") } else { t!("location.ended") }.into();
    }
    location.description.clone().unwrap_or_else(|| t!("location.shared").into())
}

#[expect(clippy::too_many_arguments, reason = "the map, its marker and its presentation")]
pub fn map_view(
    key: &str,
    account: &AccountId,
    uri: &str,
    zoom: u8,
    width: f32,
    face: El,
    media: &MediaCache,
    shown: bool,
    p: &Palette,
) -> El {
    if !shown {
        return el().size(0.0, 0.0);
    }
    let height = width * 0.59;
    let mut map = el().size(width, height).clip_rounded(10.0).fill(p.bg).center();
    if let Some(source) = matrix_core::geo::styled_map(uri, p.dark, zoom) {
        if let Some(image) = media.image(&source) {
            map = map.pixels_fit(image.pixels(), rune::SurfaceFit::Fill);
        } else {
            if media.has_failed(&source) {
                map =
                    map.child(text(t!("location.unavailable")).font(font::META).color(p.text_muted).absolute(8.0, 8.0));
            }
            if media.wants(&source, std::time::Instant::now()) {
                let owner = account.clone();
                map = map.key(format!("{key}-image-{uri}-{zoom}-{}", p.dark)).on_layout_tx(
                    move |s: &mut State, _, tx| {
                        s.request_map(&owner, source.clone());
                        tx.live_only();
                    },
                );
            }
        }
        map = map.child(
            text(t!("location.credit"))
                .font(font::BADGE)
                .color(p.text_muted)
                .fill(p.surface)
                .px(4.0)
                .absolute(4.0, height - 18.0),
        );
    }
    if matrix_core::geo::coordinates(uri).is_some() {
        map = map.child(
            el().size(42.0, 42.0)
                .radius(21.0)
                .fill(p.accent)
                .center()
                .child(el().size(36.0, 36.0).clip_rounded(18.0).child(face)),
        );
    }
    map
}

fn stop_button(request: matrix_core::RequestId, p: &Palette) -> El {
    button(t!("location.stop"), ButtonKind::Secondary, true, p)
        .key("location-stop")
        .on_press(move |s: &mut State| s.stop_live_location(request))
}

pub fn viewer(s: &State, account: &AccountId, key: &TimelineKey, event_id: &str, zoom: u8, p: &Palette) -> El {
    let message = s
        .accounts
        .get(account)
        .and_then(|data| data.timeline(key))
        .and_then(|timeline| timeline.messages().find(|m| m.event_id == event_id));
    let mut content = col().gap(8.0);
    let mut actions = Vec::new();
    let mut heading = t!("location.shared").to_owned();
    if let Some(message) = message
        && let MessageContent::Location(location) = &message.content
    {
        heading = title(location);
        let face = super::components::media_avatar(
            &message.sender_id,
            &message.sender_name,
            message.sender_avatar.as_ref(),
            &s.media,
            36.0,
        );
        content = content.child(map_view(
            "location-viewer",
            account,
            &location.geo_uri,
            zoom,
            412.0,
            face,
            &s.media,
            s.settings.show_maps,
            p,
        ));
        if let Some((lat, lon)) = matrix_core::geo::coordinates(&location.geo_uri) {
            content = content.child(text(format!("{lat}, {lon}")).font(font::META).color(p.text_muted));
            if s.settings.show_maps {
                let mut controls = row().gap(8.0);
                for (key, label, next) in
                    [("out", t!("location.zoom_out"), zoom.saturating_sub(1)), ("in", t!("location.zoom_in"), zoom + 1)]
                {
                    controls = controls.child(
                        button(label, ButtonKind::Secondary, (2..=19).contains(&next), p)
                            .key(format!("map-zoom-{key}"))
                            .on_press(move |s: &mut State| {
                                if let Some(Modal::LocationMap { zoom, .. }) = &mut s.modal {
                                    *zoom = next;
                                }
                            }),
                    );
                }
                content = content.child(controls);
            }
        }
        if let Some(link) = matrix_core::geo::web_link(&location.geo_uri) {
            actions.push(
                button(t!("location.open"), ButtonKind::Primary, true, p)
                    .key("map-external")
                    .on_press_tx(move |s: &mut State, tx| s.open_link(&link, tx)),
            );
        } else {
            content = content.child(text(t!("location.waiting")).font(font::META).color(p.text_muted));
        }
        if location.live_until.is_some_and(|until| until > crate::state::now_ms())
            && message.sender_id == account.as_str()
            && let Some(request) = s.live_share(account, &key.room_id)
        {
            content = content.child(stop_button(request, p));
        }
    }
    super::components::closable_dialog("location-viewer", &heading, content, actions, p)
}
