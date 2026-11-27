//! The location dialog: a search for a place by name (or typed coordinates), the places found,
//! and a MapLibre preview of the chosen place.

use rune::{Color, El, col, el, icon, row, text};

use super::components::{ButtonKind, button, dialog, field_input};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{Camera, MAP_ZOOMS, MapFrame, MapPin, MapViewer, MediaCache, State};
use matrix_core::{AccountId, LiveDuration, Location};

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
        button(t!("location.use_device"), ButtonKind::Secondary, !s.requesting_position(), p)
            .key("location-device")
            .on_press_tx(State::request_device_position),
    );
    if let (Some(account), Some(key)) = (&s.nav.account, form.target.and_then(|target| s.composer_key(target)))
        && key.thread.is_none()
    {
        if let Some(request) = s.live_share(account, &key.room_id, None) {
            content = content.child(stop_button(request, p));
        } else if !s
            .accounts
            .get(account)
            .and_then(|data| data.room(&key.room_id))
            .is_some_and(|room| room.can_share_live_location)
        {
            content = content.child(text(t!("location.permission_needed")).font(font::META).color(p.text_muted).wrap());
        } else {
            // Only a device fix can go live: an estimate or a searched place can't follow anyone.
            let enabled = form.estimated == Some(false);
            let mut choices = row().gap(8.0).wrap();
            for (key, label, duration) in [
                ("hour", t!("location.live_hour"), LiveDuration::Hour),
                ("day", t!("location.live_day"), LiveDuration::Day),
                ("always", t!("location.live_always"), LiveDuration::UntilStopped),
            ] {
                choices = choices.child(
                    button(label, ButtonKind::Secondary, enabled, p)
                        .key(format!("location-live-{key}"))
                        .on_press(move |s: &mut State| s.start_live_location(duration)),
                );
            }
            content = content.child(
                col()
                    .key("location-live")
                    .gap(6.0)
                    .child(text(t!("location.share_live")).font(font::SECTION).color(p.text))
                    .child(choices),
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

pub fn watch_expiry(element: El, location: &Location) -> El {
    if let Some(until) = location.live_until.filter(|until| *until > crate::state::now_ms()) {
        return element.on_layout_tx(move |s: &mut State, _, tx| s.watch_location_expiry(until, tx));
    }
    element
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

/// The map viewer's bar: its title, a way to open the focused place in Google Maps, and close.
const VIEWER_BAR: f32 = 56.0;
/// The widest the map viewer gets.
const VIEWER_MAX: [f32; 2] = [1600.0, 1100.0];
/// Map pictures are fetched in steps of this many points, so a window resized a little reuses them.
const FRAME_STEP: f32 = 64.0;

/// The map viewer: every recent place shared in the conversation on one map that drags and zooms
/// (`Modal::LocationMap`), with the people on it in a list over the map.
pub fn viewer(s: &State, viewer: &MapViewer, window: [f32; 2], p: &Palette) -> El {
    let size = [(window[0] - 48.0).clamp(320.0, VIEWER_MAX[0]), (window[1] - 64.0).clamp(280.0, VIEWER_MAX[1])];
    let now = crate::state::now_ms();
    let pins = s.map_pins(viewer, now);
    let focus = s.map_focus(viewer);
    let focused = pins.iter().find(|pin| Some(pin.message.sender_id.as_str()) == focus);
    let link = focused.and_then(|pin| matrix_core::geo::web_link(&pin.location.geo_uri));
    let bar = row()
        .key("location-viewer-bar")
        .w_full()
        .h(VIEWER_BAR)
        .pl(20.0)
        .pr(12.0)
        .gap(4.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], p.divider)
        .child(text(t!("location.viewer_title")).font(font::HEADER).color(p.text).grow(1.0).min_w(0.0).truncate())
        .children(link.map(|link| {
            super::components::icon_button(&icons::EXTERNAL, t!("location.open"), false, p.text_muted, p)
                .key("map-external")
                .on_press_tx(move |s: &mut State, tx| s.open_link(&link, tx))
        }))
        .child(
            super::components::icon_button(&icons::CLOSE, t!("common.close"), false, p.text_muted, p)
                .key("location-viewer-close")
                .on_press(State::close_modal),
        );
    let map = [size[0], size[1] - VIEWER_BAR];
    let mut area = el().key("location-viewer-map").w_full().h(map[1]).fill(p.bg).clip_soft(0.0);
    area = match s.map_center(viewer) {
        Some(center) if s.settings.show_maps => area.child(moving_map(s, viewer, center, &pins, map, p)),
        Some((lat, lon)) => area.center().child(text(format!("{lat}, {lon}")).font(font::BODY).color(p.text_muted)),
        None => area.center().child(text(t!("location.waiting")).font(font::BODY).color(p.text_muted)),
    };
    if !pins.is_empty() {
        area = area.child(people(s, viewer, &pins, now, map, p));
    }
    col()
        .key("location-viewer")
        .w(size[0])
        .h(size[1])
        .radius(14.0)
        .clip_rounded(14.0)
        .fill(p.surface)
        .border(1.0, p.text_muted.alpha(0.35))
        .shadow(16.0, 48.0, Color::BLACK.alpha(0.5))
        .role("dialog")
        .label(t!("location.viewer_title").to_owned())
        .child(bar)
        .child(area)
}

/// The map itself, `size` points: its picture moved and scaled into place (an older one while the
/// camera's own is on its way), the pins over it, and its zoom and recenter buttons.
fn moving_map(
    s: &State,
    viewer: &MapViewer,
    center: (f64, f64),
    pins: &[MapPin<'_>],
    size: [f32; 2],
    p: &Palette,
) -> El {
    let zoom = f64::from(viewer.zoom);
    let origin = matrix_core::geo::project(center.0, center.1, zoom);
    // Where a place is drawn, in points from the map's top left: on the world's nearest copy, so
    // places either side of the antimeridian stay side by side.
    let world = matrix_core::geo::project(0.0, 180.0, zoom).0;
    let place = |(lat, lon): (f64, f64)| {
        let (x, y) = matrix_core::geo::project(lat, lon, zoom);
        let dx = x - origin.0;
        let dx = dx - world * (dx / world).round();
        [dx as f32 + size[0] / 2.0 + viewer.drag.0, (y - origin.1) as f32 + size[1] / 2.0 + viewer.drag.1]
    };
    let step = |points: f32| ((points / FRAME_STEP).ceil() * FRAME_STEP).clamp(FRAME_STEP, 2048.0) as u32;
    let frame = MapFrame {
        latitude: center.0,
        longitude: center.1,
        zoom: viewer.zoom,
        width: step(size[0]),
        height: step(size[1]),
    };
    let current = frame.media(p.dark);
    // A picture centred on `frame`, drawn where its centre lies now and at the zoom now shown.
    let picture = |frame: MapFrame, image: &rune::Image| {
        let [x, y] = place((frame.latitude, frame.longitude));
        let (w, h) = (frame.width as f32, frame.height as f32);
        el().key(format!("map-frame-{}", frame.key(p.dark)))
            .size(w, h)
            .absolute(x - w / 2.0, y - h / 2.0)
            .scale((zoom - f64::from(frame.zoom)).exp2() as f32)
            .pixels_fit(image.pixels(), rune::SurfaceFit::Fill)
    };
    let mut map = el()
        .key("map-viewport")
        .w_full()
        .h_full()
        .cursor(if viewer.drag == (0.0, 0.0) { rune::Cursor::Grab } else { rune::Cursor::Grabbing })
        .on_drag(|s: &mut State, drag, tx| {
            // The map stays under the pointer rather than springing after it.
            tx.instant();
            s.drag_map(drag.phase, drag.dx, drag.dy);
        })
        .on_wheel(move |s: &mut State, wheel, _| {
            s.wheel_map(wheel.dy, wheel.precise, (wheel.x - size[0] / 2.0, wheel.y - size[1] / 2.0))
        });
    if let Some(image) = s.media.image(&current) {
        map = map.child(picture(frame, image).on_layout(move |s: &mut State, _| s.map_drawn(frame)));
    } else {
        if let Some(drawn) = viewer.drawn.filter(|drawn| *drawn != frame)
            && let Some(image) = s.media.image(&drawn.media(p.dark))
        {
            map = map.child(picture(drawn, image));
        }
        if s.media.has_failed(&current) {
            map = map.child(
                text(t!("location.unavailable"))
                    .font(font::META)
                    .color(p.text_muted)
                    .fill(p.surface)
                    .px(8.0)
                    .py(4.0)
                    .radius(6.0)
                    .absolute(size[0] / 2.0 - 60.0, 16.0),
            );
        }
        if s.media.wants(&current, std::time::Instant::now()) {
            let account = viewer.account.clone();
            let dark = p.dark;
            map = map.child(el().key(format!("map-request-{}", frame.key(p.dark))).size(0.0, 0.0).on_layout_tx(
                move |s: &mut State, _, tx| {
                    s.request_map_frame(&account, frame, dark);
                    tx.live_only();
                },
            ));
        }
    }
    // The focused pin last, over the others.
    let mut ordered: Vec<&MapPin<'_>> = pins.iter().collect();
    let focus = s.map_focus(viewer);
    ordered.sort_by_key(|pin| Some(pin.message.sender_id.as_str()) == focus);
    for pin in ordered {
        let [x, y] = place(pin.at);
        let focused = Some(pin.message.sender_id.as_str()) == focus;
        map = map.child(marker(s, pin, focused, p).absolute(x - PIN / 2.0, y - PIN_TIP));
    }
    map.child(
        text(t!("location.credit")).font(font::BADGE).color(p.text_muted).fill(p.surface.alpha(0.85)).px(4.0).pin(
            None,
            None,
            Some(4.0),
            Some(4.0),
        ),
    )
    .child(controls(viewer, p).pin(None, None, Some(16.0), Some(28.0)))
}

/// A pin's avatar disc, in points.
const PIN: f32 = 44.0;
/// From a pin's top to the point it marks: the disc, a stem, and the dot's centre.
const PIN_TIP: f32 = PIN + 8.0 + 6.0;

/// A person's pin: their avatar in a ring, over a dot on the very spot. The focused one is in the
/// accent.
fn marker(s: &State, pin: &MapPin<'_>, focused: bool, p: &Palette) -> El {
    let message = pin.message;
    let ring = if focused { p.accent } else { p.surface };
    let face = super::components::media_avatar(
        &message.sender_id,
        &s.settings.person_name(&message.sender_name),
        message.sender_avatar.as_ref(),
        &s.media,
        PIN - 6.0,
    );
    let sender = message.sender_id.clone();
    col()
        .key(format!("map-pin-{}", message.event_id))
        .w(PIN)
        .items_center()
        .role("button")
        .label(s.settings.person_name(&message.sender_name))
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .child(
            el().size(PIN, PIN)
                .radius(PIN / 2.0)
                .fill(ring)
                .shadow(2.0, 8.0, Color::BLACK.alpha(0.35))
                .center()
                .child(el().size(PIN - 6.0, PIN - 6.0).clip_rounded((PIN - 6.0) / 2.0).child(face)),
        )
        .child(el().size(2.0, 8.0).fill(ring))
        .child(el().size(12.0, 12.0).radius(6.0).fill(p.accent).border(2.0, Color::WHITE))
        .on_press(move |s: &mut State| s.focus_map_pin(sender.clone()))
}

/// The map's buttons, at its bottom right: back to the focused place once moved away, and zoom.
fn controls(viewer: &MapViewer, p: &Palette) -> El {
    let raised = |child: El| {
        child.fill(p.surface).radius(8.0).border(1.0, p.divider).shadow(2.0, 10.0, Color::BLACK.alpha(0.3)).padding(4.0)
    };
    let mut column = col().gap(10.0).items_end();
    if viewer.camera != Camera::Following {
        column = column.child(raised(
            super::components::icon_button(&icons::LOCATE, t!("location.recenter"), false, p.text, p)
                .key("map-recenter")
                .on_press(State::recenter_map),
        ));
    }
    let zoom = |key: &str, glyph, label: &str, step: i32, enabled: bool| {
        let button =
            super::components::icon_button(glyph, label, false, if enabled { p.text } else { p.text_muted }, p)
                .key(format!("map-zoom-{key}"));
        if enabled { button.on_press(move |s: &mut State| s.zoom_map(step, (0.0, 0.0))) } else { button }
    };
    column.child(raised(
        col()
            .gap(4.0)
            .child(zoom("in", &icons::PLUS, t!("location.zoom_in"), 1, viewer.zoom < *MAP_ZOOMS.end()))
            .child(el().w_full().h(1.0).fill(p.divider))
            .child(zoom("out", &icons::MINUS, t!("location.zoom_out"), -1, viewer.zoom > *MAP_ZOOMS.start())),
    ))
}

/// Who is on the map, over its top left: a row each, newest first; a row turns the map to them.
fn people(s: &State, viewer: &MapViewer, pins: &[MapPin<'_>], now: u64, map: [f32; 2], p: &Palette) -> El {
    let mut list = col()
        .key("map-people-list")
        .w_full()
        .gap(2.0)
        .shrink()
        .min_h(0.0)
        .scroll(rune::ScrollSpec { stick_bottom: false });
    let focus = s.map_focus(viewer);
    for pin in pins {
        let focused = Some(pin.message.sender_id.as_str()) == focus;
        list = list.child(person_row(s, viewer, pin, focused, now, p));
    }
    col()
        .key("map-people")
        .w(300.0_f32.min(map[0] - 32.0))
        .max_h((map[1] - 32.0).max(120.0))
        .absolute(16.0, 16.0)
        .padding(8.0)
        .gap(4.0)
        .radius(12.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .shadow(4.0, 18.0, Color::BLACK.alpha(0.35))
        .child(text(t!("location.on_map")).font(font::SECTION).color(p.text_muted).px(8.0).pt(4.0))
        .child(list)
}

/// When a live share ends: its time today, else its day and time (a share until stopped ends
/// weeks on).
fn until_label(until: u64, now: u64) -> String {
    use super::components::{clock_time, local_ms, when};
    if local_ms(until) / 86_400_000 == local_ms(now) / 86_400_000 { clock_time(local_ms(until)) } else { when(until) }
}

fn person_row(s: &State, viewer: &MapViewer, pin: &MapPin<'_>, focused: bool, now: u64, p: &Palette) -> El {
    let message = pin.message;
    let own = message.sender_id == viewer.account.as_str();
    let name = if own { t!("call.you").to_owned() } else { s.settings.person_name(&message.sender_name).into_owned() };
    let when = match pin.location.live_until.filter(|_| pin.live(now)) {
        Some(until) => tf!("location.live_until", time = until_label(until, now)),
        None => super::components::when(message.timestamp_ms),
    };
    let detail = match &pin.location.description {
        Some(place) if !pin.live(now) => format!("{place} · {when}"),
        _ => when,
    };
    let face = super::components::media_avatar(
        &message.sender_id,
        &s.settings.person_name(&message.sender_name),
        message.sender_avatar.as_ref(),
        &s.media,
        32.0,
    );
    let stop = (own && pin.live(now))
        .then(|| s.live_share(&viewer.account, &viewer.key.room_id, Some(&message.event_id)))
        .flatten()
        .map(|request| stop_button(request, p));
    let sender = message.sender_id.clone();
    let row = row()
        .key(format!("map-person-{}", message.event_id))
        .w_full()
        .px(8.0)
        .py(6.0)
        .gap(10.0)
        .center_y()
        .radius(8.0)
        .fill(if focused { p.accent.alpha(0.12) } else { Color::TRANSPARENT })
        .hover(p.hover)
        .role("button")
        .label(name.clone())
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .child(el().size(32.0, 32.0).clip_rounded(16.0).child(face))
        .child(
            col().grow(1.0).min_w(0.0).child(text(name).font(font::BODY_STRONG).color(p.text).truncate()).child(
                row()
                    .gap(4.0)
                    .center_y()
                    .children(pin.live(now).then(|| icon(&icons::MAP_PIN, 12.0).color(p.accent)))
                    .child(text(detail).font(font::META).color(p.text_muted).truncate().shrink()),
            ),
        )
        .children(stop)
        .on_press(move |s: &mut State| s.focus_map_pin(sender.clone()));
    // Redrawn as it leaves the map, even while nothing else changes.
    let leaves = pin.leaves_at();
    if leaves > now {
        return row.on_layout_tx(move |s: &mut State, _, tx| s.watch_location_expiry(leaves + 1, tx));
    }
    row
}
