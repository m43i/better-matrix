//! Calls: header actions, the planned stage and its layout controls, the call bar in the
//! sidebar elsewhere, and ringing cards. Chat can sit beside or below the stage.

use matrix_core::{CallPhase, CallVia, DeviceKind, IncomingCall, RoomSummary};
use rune::{Color, El, SurfaceFit, col, el, icon, row, text};

use super::components::{account_avatar, icon_button, media_avatar, menu_row, popover_panel};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, Call, CallLayout, CallTile, Corner, Popover, SelfView, State, VideoOption};

/// The size of a face on a voice call's stage.
const FACE: f32 = 96.0;
/// The size of the self-view's corner that resizes it.
const RESIZE_CORNER: f32 = 20.0;
/// The size of a round control button.
const CONTROL: f32 = 48.0;

/// The header's call buttons: Join while a call is under way in the room, else the voice call;
/// then the video button, which meets by video the room's default way (a video call, else its
/// meeting, else a new one), with a chevron beside it listing the other ways. None while the
/// room's own call shows here.
pub fn header_buttons(s: &State, room: &RoomSummary, p: &Palette) -> Vec<El> {
    if let Some(call) = s.call_here() {
        let mut buttons = Vec::new();
        if call.chat {
            buttons.push(
                pill(&icons::CALL, t!("call.show_stage"), p.success, p)
                    .key("call-back")
                    .on_press(State::toggle_call_chat),
            );
        }
        return buttons;
    }
    let voice = if room.call.via.is_none() {
        None
    } else if !room.call.participants.is_empty() {
        let label = tf!("call.people", count = room.call.participants.len());
        Some(
            pill(&icons::CALL, t!("call.join"), p.success, p)
                .key("call-join")
                .help(label)
                .on_press(|s: &mut State| s.start_call(false)),
        )
    } else {
        Some(
            icon_button(&icons::CALL, t!("call.start"), false, p.text_muted, p)
                .key("call-start")
                .help(t!("call.start"))
                .on_press(|s: &mut State| s.start_call(false)),
        )
    };
    voice.into_iter().chain(video_button(s, p)).collect()
}

/// The video button's key.
pub const VIDEO: &str = "call-start-video";
/// The key of the chevron beside it, which its menu hangs from.
pub const VIDEO_OPTIONS: &str = "video-options";

/// What a way to meet by video says, and its icon.
fn video_option_label(option: &VideoOption) -> (String, &'static rune::Icon) {
    match option {
        VideoOption::Call => (t!("call.start_video").to_owned(), &icons::VIDEO),
        VideoOption::Join { name, .. } => (tf!("widgets.join_meeting_named", name = name), &icons::MEETING),
        VideoOption::Start => (t!("widgets.start_meeting").to_owned(), &icons::MEETING),
    }
}

/// A stable key part per way to meet.
fn video_option_key(option: &VideoOption) -> String {
    match option {
        VideoOption::Call => "video-option-call".to_owned(),
        VideoOption::Join { widget_id, .. } => format!("video-option-join-{widget_id}"),
        VideoOption::Start => "video-option-start".to_owned(),
    }
}

/// The video button, for the room's default way to meet by video, and the chevron listing the
/// others when there are any. None without a way.
fn video_button(s: &State, p: &Palette) -> Option<El> {
    let options = s.video_options();
    let (label, _) = video_option_label(options.first()?);
    let button = icon_button(&icons::VIDEO, &label, false, p.text_muted, p)
        .key(VIDEO)
        .help(label.clone())
        .on_press(State::start_video);
    if options.len() < 2 {
        return Some(button);
    }
    let open = s.popover == Some(Popover::VideoOptions);
    let chevron = chevron_button(VIDEO_OPTIONS, t!("call.video_options"), &icons::CHEVRON_DOWN, 28.0, p)
        .expanded(open)
        .on_press(State::toggle_video_options);
    Some(row().gap(2.0).center_y().child(button).child(chevron))
}

/// The ways to meet by video besides the video button's, under its chevron.
pub fn video_options_menu(s: &State, p: &Palette) -> El {
    let mut panel = popover_panel(p).key("video-options-menu").w(280.0);
    for option in s.video_options().into_iter().skip(1) {
        let (label, glyph) = video_option_label(&option);
        let leading = el().size(28.0, 28.0).center().child(icon(glyph, 18.0).color(p.text_muted));
        let key = video_option_key(&option);
        panel = panel.child(
            menu_row(leading, &label, None, None, p)
                .key(key)
                .role("menuitem")
                .on_press(move |s: &mut State| s.choose_video_option(&option)),
        );
    }
    col().child(panel).anchored(VIDEO_OPTIONS, rune::Side::Bottom, rune::Align::End, 6.0)
}

/// A rounded button with an icon and a label, filled with `color`.
pub fn pill(glyph: &'static rune::Icon, label: &str, color: Color, p: &Palette) -> El {
    row()
        .h(32.0)
        .px(12.0)
        .gap(6.0)
        .center_y()
        .radius(16.0)
        .fill(color)
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(label.to_owned())
        .child(icon(glyph, 16.0).color(p.on_accent))
        .child(text(label.to_owned()).font(font::BODY_STRONG).color(p.on_accent))
}

/// The stage of the call in the room shown: the glow, everyone in the call, the controls.
pub fn stage(s: &State, account: &AccountData, call: &Call, p: &Palette) -> El {
    let plan = call.plan();
    let mut viewport = el().key("call-viewport").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).on_layout_tx(
        |s: &mut State, rect, tx| {
            if !s.resize_call([rect[2], rect[3]]) {
                tx.live_only();
            }
        },
    );
    let floating = plan.tiles.iter().any(|placed| placed.inset);
    for placed in &plan.tiles {
        let [x, y, w, h] = placed.rect;
        let tile = if call.video() {
            let layer = match (placed.inset, floating) {
                (true, _) => Layer::SelfView,
                (false, true) => Layer::Under(call.self_view.corner),
                (false, false) => Layer::Alone,
            };
            video_tile(s, account, call, &placed.key, [w, h], layer, p)
        } else {
            voice_tile(s, account, call, &placed.key, [w, h], p)
        };
        let tile = tile.absolute(x, y).size(w, h);
        viewport = viewport.child(if placed.inset { self_view(tile, call, p) } else { tile });
    }
    if let Some([x, y, w, h]) = plan.divider {
        let beside = plan.strip_beside;
        let total = if beside { call.size[0] } else { call.size[1] };
        viewport = viewport.child(
            el().key("call-divider")
                .absolute(x, y)
                .size(w, h)
                .role("slider")
                .label(t!("call.resize_strip").to_owned())
                .cursor(if beside { rune::Cursor::ResizeColumn } else { rune::Cursor::ResizeRow })
                .center()
                .child(
                    el().size(if beside { 3.0 } else { 32.0 }, if beside { 32.0 } else { 3.0 })
                        .radius(2.0)
                        .fill(p.divider),
                )
                .on_drag(move |s: &mut State, drag, _| {
                    if let Some(call) = s.call.as_mut() {
                        match drag.phase {
                            rune::Phase::Down => drag.data = call.strip,
                            rune::Phase::Move => {
                                let travel = if beside { drag.dx } else { drag.dy };
                                call.strip = (drag.data - travel / total.max(1.0)).clamp(0.18, 0.4);
                            }
                            rune::Phase::Up => {
                                if drag.cancelled() {
                                    call.strip = drag.data;
                                }
                            }
                        }
                    }
                    s.sync_watching();
                })
                .on_step(|s: &mut State, delta, _| {
                    if let Some(call) = s.call.as_mut() {
                        call.strip = (call.strip + delta as f32 * 0.02).clamp(0.18, 0.4);
                    }
                    s.sync_watching();
                }),
        );
    }
    let paging = row()
        .key("call-paging")
        .h(32.0)
        .w_full()
        .gap(12.0)
        .center()
        .children((plan.pages > 1).then(|| {
            icon_button(&icons::CHEVRON_LEFT, t!("call.previous_page"), false, p.text, p)
                .key("call-page-previous")
                .disabled(plan.page == 0)
                .on_press(|s: &mut State| s.page_call(false))
        }))
        .children((plan.pages > 1).then(|| {
            text(tf!("call.page", page = plan.page + 1, pages = plan.pages)).font(font::META).color(p.text_muted)
        }))
        .children((plan.pages > 1).then(|| {
            icon_button(&icons::CHEVRON_RIGHT, t!("call.next_page"), false, p.text, p)
                .key("call-page-next")
                .disabled(plan.page + 1 >= plan.pages)
                .on_press(|s: &mut State| s.page_call(true))
        }));
    let mut content = col().w_full().h_full().padding(if call.compact() { 12.0 } else { 16.0 }).gap(8.0);
    if !call.compact() {
        content = content.child(layout_controls(s, call, p));
    }
    if let Some(status) = status(s, call) {
        content = content.child(text(status).key("call-status").font(font::BODY).color(p.text_muted));
    }
    content = content.child(viewport);
    if !call.compact() || !call.video() || plan.pages > 1 {
        content = content.child(paging);
    }
    col()
        .key("call-stage")
        .grow(1.0)
        .w_full()
        .min_w(0.0)
        .min_h(0.0)
        .fill(p.bg)
        .clip_rounded(0.0)
        .child(
            el().key("call-glow")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .pixels_fit(&call.glow.pixels, SurfaceFit::Fill)
                .a11y_hidden()
                .on_layout(|s: &mut State, rect| {
                    if let Some(call) = s.call.as_mut() {
                        call.glow.resized([rect[2], rect[3]]);
                    }
                }),
        )
        .child(content.grow(1.0).min_h(0.0))
        .child(controls(call, p))
}

/// What the stage says above the people, while the call isn't simply under way.
fn status(s: &State, call: &Call) -> Option<String> {
    let info = &call.info;
    match info.phase {
        CallPhase::Calling => {
            let name = info.participants.first().map_or("", |person| person.person.name.as_str());
            Some(tf!("call.calling", name = s.settings.person_name(name)))
        }
        CallPhase::Connecting => Some(t!("call.connecting").to_owned()),
        CallPhase::Reconnecting => Some(t!("call.reconnecting").to_owned()),
        CallPhase::Connected if info.participants.is_empty() => Some(t!("call.alone").to_owned()),
        CallPhase::Connected => None,
    }
}

fn participant<'a>(call: &'a Call, key: &CallTile) -> Option<&'a matrix_core::CallParticipant> {
    call.info.participants.iter().find(|person| person.key == key.participant())
}

fn tile_name(s: &State, call: &Call, key: &CallTile) -> String {
    let name = participant(call, key)
        .map_or_else(|| t!("call.you").to_owned(), |person| s.settings.person_name(&person.person.name).into_owned());
    if key.screen() { tf!("call.screen_of", name = name) } else { name }
}

fn tile_avatar(s: &State, account: &AccountData, call: &Call, key: &CallTile, size: f32) -> El {
    match participant(call, key) {
        Some(person) => media_avatar(
            &person.person.id,
            &s.settings.person_name(&person.person.name),
            person.person.avatar.as_ref(),
            &s.media,
            size,
        ),
        None => account_avatar(&account.summary, &s.media, size),
    }
}

fn muted(call: &Call, key: &CallTile) -> bool {
    participant(call, key).map_or(!call.info.microphone, |person| person.muted)
}

fn tile_key(key: &CallTile) -> String {
    let who = if key.participant().is_empty() { "you" } else { key.participant() };
    format!("call-{}-{who}", if key.screen() { "screen" } else { "tile" })
}

fn voice_tile(s: &State, account: &AccountData, call: &Call, key: &CallTile, bounds: [f32; 2], p: &Palette) -> El {
    let size = FACE.min(bounds[0] - 16.0).min(bounds[1] - 40.0).max(24.0);
    let avatar = tile_avatar(s, account, call, key, size);
    let avatar = if call.speaking.contains(key.participant()) { speaking_ring(avatar, p) } else { avatar };
    col().key(tile_key(key)).gap(10.0).center().child(avatar).child(
        row()
            .max_w(160.0)
            .gap(6.0)
            .center_y()
            .child(text(tile_name(s, call, key)).font(font::BODY_STRONG).color(p.text).truncate())
            .children(muted(call, key).then(|| icon(&icons::MIC_OFF, 14.0).color(p.text_muted))),
    )
}

fn layout_controls(s: &State, call: &Call, p: &Palette) -> El {
    let mut bar = row().key("call-layout-controls").w_full().gap(6.0).center_y().child(el().grow(1.0)).child(
        icon_button(
            &icons::EYE_OFF,
            if call.show_self { t!("call.hide_self") } else { t!("call.show_self") },
            !call.show_self,
            p.text_muted,
            p,
        )
        .key("call-self")
        .help(if call.show_self { t!("call.hide_self") } else { t!("call.show_self") })
        .on_press(State::toggle_call_self),
    );
    if call.video() {
        bar = bar.child(
            icon_button(&icons::LAYOUT, t!("call.layout"), s.popover == Some(Popover::CallLayout), p.text_muted, p)
                .key("call-layout")
                .help(t!("call.layout"))
                .expanded(s.popover == Some(Popover::CallLayout))
                .on_press(State::open_call_layout),
        );
    }
    bar
}

/// Layout choices live in the shared popover path, aligned with the stage's right edge.
pub fn layout_menu(call: &Call, p: &Palette) -> El {
    let mut panel = popover_panel(p).key("call-layout-menu").w(220.0);
    for (layout, label, key) in [
        (CallLayout::Auto, t!("call.layout_auto"), "call-layout-auto"),
        (CallLayout::Grid, t!("call.layout_grid"), "call-layout-grid"),
        (CallLayout::Focus(None), t!("call.layout_focus"), "call-layout-focus"),
    ] {
        let active = std::mem::discriminant(&call.layout) == std::mem::discriminant(&layout);
        let mark = el().size(28.0, 28.0).center().children(active.then(|| icon(&icons::CHECK, 16.0).color(p.accent)));
        panel = panel.child(
            menu_row(mark, label, None, None, p)
                .key(key)
                .role("menuitemradio")
                .checked(active)
                .help(if matches!(layout, CallLayout::Focus(_)) { t!("call.follow_speaker") } else { label })
                .on_press(move |s: &mut State| s.set_call_layout(layout.clone())),
        );
    }
    col().child(panel).anchored("call-layout", rune::Side::Bottom, rune::Align::End, 6.0)
}

/// The sidebar toggle belongs next to the room name, where the sidebar ends.
pub fn sidebar_button(call: &Call, p: &Palette) -> El {
    let label = if call.hide_sidebar { t!("call.show_sidebar") } else { t!("call.hide_sidebar") };
    icon_button(&icons::SIDEBAR, label, call.hide_sidebar, p.text_muted, p)
        .key("call-sidebar")
        .help(label)
        .on_press(State::toggle_call_sidebar)
}

/// Someone speaking: a ring in the accent and a soft glow of it.
fn speaking_ring(face: El, p: &Palette) -> El {
    face.outline(2.0, 3.0, p.accent).box_shadow(rune::Shadow {
        blur: 22.0,
        spread: 2.0,
        color: p.accent.alpha(0.55),
        ..rune::Shadow::default()
    })
}

/// How a video tile sits among the others: on its own, under the one-to-one self-view (which
/// covers one of its corners), or as that self-view.
#[derive(Clone, Copy)]
enum Layer {
    Alone,
    Under(Corner),
    SelfView,
}

/// The one-to-one self-view, floating in a corner of the other person's video: dragged, it
/// follows the pointer and snaps to the nearest corner when let go; dragging its inner corner
/// resizes it. Nothing marks that corner but the resize cursor over it.
fn self_view(tile: El, call: &Call, p: &Palette) -> El {
    let corner = call.self_view.corner;
    let [dx, dy] = call.self_drag.unwrap_or_default();
    // The corner facing the middle of the video.
    let (left, right) = if corner.left() { (None, Some(0.0)) } else { (Some(0.0), None) };
    let (top, bottom) = if corner.top() { (None, Some(0.0)) } else { (Some(0.0), None) };
    let grip = el()
        .key("call-self-resize")
        .size(RESIZE_CORNER, RESIZE_CORNER)
        .pin(left, top, right, bottom)
        .cursor(if corner.left() == corner.top() { rune::Cursor::ResizeNwse } else { rune::Cursor::ResizeNesw })
        .role("slider")
        .label(t!("call.resize_self").to_owned())
        .on_drag(move |s: &mut State, drag, _| {
            let Some(call) = s.call.as_mut() else { return };
            let Some((own, main)) = call.self_view_rects() else { return };
            let across = main[2].max(1.0);
            match drag.phase {
                // The scale the user chose, which a cancel restores; the width shown may differ
                // from it where a limit holds it.
                rune::Phase::Down => drag.data = call.self_view.scale,
                rune::Phase::Move => {
                    let aspect = own[2] / own[3].max(1.0);
                    let start = SelfView { scale: drag.data, ..call.self_view }.place(main, aspect)[2];
                    // Towards the middle of the video grows it, back towards its corner shrinks
                    // it, along whichever way the pointer went further.
                    let sideways = if corner.left() { drag.dx } else { -drag.dx };
                    let upright = if corner.top() { drag.dy } else { -drag.dy } * aspect;
                    let grow = if sideways.abs() >= upright.abs() { sideways } else { upright };
                    call.resize_self_view((start + grow) / across);
                }
                rune::Phase::Up => {
                    if drag.cancelled() {
                        call.resize_self_view(drag.data);
                    }
                }
            }
        })
        .on_step(|s: &mut State, delta, _| {
            if let Some(call) = s.call.as_mut() {
                call.resize_self_view(call.self_view.scale + delta as f32 * 0.05);
            }
        });
    tile.border(1.0, p.divider)
        .shadow(4.0, 18.0, Color::BLACK.alpha(0.35))
        .offset(dx, dy)
        .cursor(if call.self_drag.is_some() { rune::Cursor::Grabbing } else { rune::Cursor::Grab })
        .help(t!("call.move_self"))
        .on_drag(|s: &mut State, drag, _| {
            let Some(call) = s.call.as_mut() else { return };
            match drag.phase {
                rune::Phase::Down => {}
                rune::Phase::Move => call.drag_self_view([drag.dx, drag.dy]),
                rune::Phase::Up => {
                    let dropped = (!drag.cancelled()).then(|| call.self_drag.unwrap_or_default());
                    call.drop_self_view(dropped);
                }
            }
        })
        .child(grip)
}

/// One surface: shared content is always fitted, while cameras offer a full-frame view.
fn video_tile(
    s: &State,
    account: &AccountData,
    call: &Call,
    key: &CallTile,
    size: [f32; 2],
    layer: Layer,
    p: &Palette,
) -> El {
    let name = tile_name(s, call, key);
    let focus = key.clone();
    let mut cell = el()
        .key(tile_key(key))
        .radius(14.0)
        .fill(Color::BLACK.alpha(0.55))
        .clip_rounded(14.0)
        .role("button")
        .focusable(true)
        .label(tf!("call.focus_on", name = name))
        .cursor(rune::Cursor::Pointer)
        .on_press(move |s: &mut State| s.set_call_layout(CallLayout::Focus(Some(focus.clone()))));
    let pixels = call.video_id(key).and_then(|id| call.videos.get(&id));
    let full = key.screen() || call.full_video.get(key).copied().unwrap_or_else(|| call.aspect(key) < 1.0);
    cell = match pixels {
        Some(pixels) if pixels.size() != (0, 0) => {
            cell.child(el().w_full().h_full().pixels_fit(pixels, if full { SurfaceFit::Fit } else { SurfaceFit::Fill }))
        }
        _ => cell.child(col().w_full().h_full().center().child(tile_avatar(
            s,
            account,
            call,
            key,
            (size[1] * 0.45).clamp(24.0, 72.0),
        ))),
    };
    // The self-view fills its own camera's shape: nothing to fit.
    if !key.screen() && pixels.is_some() && !matches!(layer, Layer::SelfView) {
        let fit = key.clone();
        // Clear of the self-view floating over this tile's top right.
        let pin = if matches!(layer, Layer::Under(Corner::TopRight)) {
            (Some(6.0), Some(6.0), None, None)
        } else {
            (None, Some(6.0), Some(6.0), None)
        };
        cell = cell.child(
            icon_button(
                &icons::EXTERNAL,
                if full { t!("call.crop_video") } else { t!("call.full_video") },
                false,
                Color::WHITE,
                p,
            )
            .key(format!("{}-fit", tile_key(key)))
            .pin(pin.0, pin.1, pin.2, pin.3)
            .fill(Color::BLACK.alpha(0.5))
            .help(if full { t!("call.crop_video") } else { t!("call.full_video") })
            .on_press(move |s: &mut State| s.toggle_call_fit(fit.clone())),
        );
    }
    // Clear of the self-view floating over this tile's bottom left.
    let label_at = if matches!(layer, Layer::Under(Corner::BottomLeft)) {
        (None, None, Some(10.0), Some(10.0))
    } else {
        (Some(10.0), None, None, Some(10.0))
    };
    let mut label = row()
        .pin(label_at.0, label_at.1, label_at.2, label_at.3)
        .h(26.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(13.0)
        .fill(Color::BLACK.alpha(0.5))
        .max_w((size[0] - 20.0).max(0.0))
        .child(text(name).font(font::META).color(Color::WHITE).truncate());
    if !key.screen() && muted(call, key) {
        label = label.child(icon(&icons::MIC_OFF, 12.0).color(Color::WHITE));
    }
    cell = cell.child(label);
    // Whoever speaks: a ring just inside the tile, drawn over the video.
    if !key.screen() && call.speaking.contains(key.participant()) {
        cell = cell.child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(14.0).border(2.5, p.accent).a11y_hidden(),
        );
    }
    cell
}

/// The call's controls, floating at the bottom of the stage.
fn controls(call: &Call, p: &Palette) -> El {
    let info = &call.info;
    let size = if call.compact() { 36.0 } else { CONTROL };
    let button =
        |glyph, label: &str, fill, color| control(glyph, label, fill, color, p).size(size, size).radius(size / 2.0);
    let toggle = |glyph, label: &str, on: bool, key: &str| {
        button(glyph, label, if on { p.surface } else { p.text.alpha(0.85) }, if on { p.text } else { p.bg })
            .key(key.to_owned())
    };
    let mut bar = row()
        .key("call-controls")
        .gap(8.0)
        .padding(8.0)
        .mb(12.0)
        .flex_wrap()
        .max_w((call.size[0] - 8.0).max(120.0))
        .center_y()
        .radius(CONTROL / 2.0 + 12.0)
        .fill(p.surface.alpha(0.85))
        .border(1.0, p.divider)
        .shadow(6.0, 24.0, Color::BLACK.alpha(0.3));
    let microphone = if info.microphone {
        toggle(&icons::MIC, t!("call.mute"), true, "call-mic")
    } else {
        toggle(&icons::MIC_OFF, t!("call.unmute"), false, "call-mic")
    };
    bar = bar.child(with_devices(microphone.on_press(State::toggle_microphone), DeviceKind::Microphone, size, p));
    let camera = if info.camera.is_some() {
        toggle(&icons::VIDEO, t!("call.camera_off"), true, "call-camera")
    } else {
        toggle(&icons::VIDEO_OFF, t!("call.camera_on"), false, "call-camera")
    };
    // Device to device, the connection carries the camera it started with (if any) and no
    // screen.
    if info.via == CallVia::Rtc || info.video {
        bar = bar.child(with_devices(camera.on_press(State::toggle_camera), DeviceKind::Camera, size, p));
    }
    if info.via == CallVia::Rtc {
        let sharing = info.screen.is_some();
        let label = if sharing { t!("call.stop_share") } else { t!("call.share") };
        let (fill, color) = if sharing { (p.accent, p.on_accent) } else { (p.surface, p.text) };
        bar = bar
            .child(button(&icons::SCREEN_SHARE, label, fill, color).key("call-screen").on_press(State::toggle_screen));
    }
    bar = bar
        .child(
            button(
                &icons::CHAT,
                if call.chat { t!("call.hide_chat") } else { t!("call.show_chat") },
                p.surface,
                p.text,
            )
            .key("call-chat")
            .on_press(State::toggle_call_chat),
        )
        .child(
            button(&icons::HANG_UP, t!("call.hang_up"), p.danger, Color::WHITE)
                .key("call-hang-up")
                .on_press(State::hang_up),
        );
    row().w_full().center().child(bar)
}

/// A control with a chevron beside it that opens the devices of its kind.
fn with_devices(button: El, kind: DeviceKind, size: f32, p: &Palette) -> El {
    let chevron = chevron_button(&devices_key(kind), t!("call.devices"), &icons::CHEVRON_UP, size, p)
        .on_press(move |s: &mut State| s.open_call_devices(kind));
    row().gap(2.0).center_y().child(button).child(chevron)
}

/// The narrow chevron beside a control that opens more of it: its devices, its other options.
/// The caller adds its press.
fn chevron_button(key: &str, label: &str, glyph: &'static rune::Icon, height: f32, p: &Palette) -> El {
    el().key(key.to_owned())
        .size(20.0, height)
        .radius(10.0)
        .center()
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(label.to_owned())
        .help(label.to_owned())
        .child(icon(glyph, 14.0).color(p.text_muted))
}

fn devices_key(kind: DeviceKind) -> String {
    format!("call-devices-{kind:?}")
}

/// The devices to pick from, above the chevron that opened it: microphones and speakers beside
/// the microphone, cameras beside the camera. The picked one is checked.
pub fn device_menu(s: &State, kind: DeviceKind, p: &Palette) -> El {
    let kinds: &[(DeviceKind, &str)] = match kind {
        DeviceKind::Camera => &[(DeviceKind::Camera, t!("settings.camera"))],
        _ => &[(DeviceKind::Microphone, t!("settings.microphone")), (DeviceKind::Speaker, t!("settings.speaker"))],
    };
    let mut panel = popover_panel(p).key("call-devices").w(300.0);
    for (kind, title) in kinds {
        let kind = *kind;
        panel = panel.child(text((*title).to_owned()).font(font::SECTION).color(p.text_muted).px(14.0).pt(8.0).pb(4.0));
        let names: Vec<String> = s
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
        let picked = s.settings.devices.get(kind);
        let check =
            |on: bool| el().size(28.0, 28.0).center().children(on.then(|| icon(&icons::CHECK, 16.0).color(p.accent)));
        panel = panel.child(
            menu_row(check(picked.is_none()), t!("settings.system_default"), None, None, p)
                .key(format!("call-device-{kind:?}-default"))
                .on_press(move |s: &mut State| s.choose_device(kind, None)),
        );
        for name in names {
            let chosen = name.clone();
            panel = panel.child(
                menu_row(check(picked == Some(name.as_str())), &name, None, None, p)
                    .key(format!("call-device-{kind:?}-{name}"))
                    .on_press(move |s: &mut State| s.choose_device(kind, Some(chosen.clone()))),
            );
        }
    }
    col().child(panel).anchored(devices_key(kind), rune::Side::Top, rune::Align::Center, 8.0)
}

fn control(glyph: &'static rune::Icon, label: &str, fill: Color, color: Color, p: &Palette) -> El {
    el().size(CONTROL, CONTROL)
        .radius(CONTROL / 2.0)
        .center()
        .fill(fill)
        .border(1.0, p.divider)
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(label.to_owned())
        .help(label.to_owned())
        .child(icon(glyph, 22.0).color(color))
}

/// At the bottom of the sidebar while the call's room isn't shown: where the call is, a mute
/// button and the hang-up; pressing it goes back to the call.
pub fn call_bar(s: &State, p: &Palette) -> Option<El> {
    let call = s.call.as_ref()?;
    if s.call_here().is_some() {
        return None;
    }
    let room = s.accounts.get(&call.info.account).and_then(|account| account.room(&call.info.room_id));
    let name = room.map_or_else(|| call.info.room_id.clone(), |room| super::room::display_name(room, &s.settings));
    let info = &call.info;
    Some(
        row()
            .key("call-bar")
            .w_full()
            .px(12.0)
            .py(10.0)
            .gap(10.0)
            .center_y()
            .fill(p.success.alpha(0.16))
            .border_sides([1.0, 0.0, 0.0, 0.0], p.sidebar_divider)
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(t!("call.return").to_owned())
            .on_press_tx(crate::state::navigate(State::open_call))
            .child(el().size(8.0, 8.0).radius(4.0).fill(p.success))
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .child(text(t!("call.in_call")).font(font::NAV_STRONG).color(p.sidebar_text))
                    .child(text(name).font(font::META).color(p.sidebar_text_muted).truncate()),
            )
            .child(
                icon_button(
                    if info.microphone { &icons::MIC } else { &icons::MIC_OFF },
                    if info.microphone { t!("call.mute") } else { t!("call.unmute") },
                    !info.microphone,
                    p.sidebar_text,
                    p,
                )
                .key("call-bar-mic")
                .on_press(State::toggle_microphone),
            )
            .child(
                icon_button(&icons::HANG_UP, t!("call.hang_up"), false, p.danger, p)
                    .key("call-bar-hang-up")
                    .on_press(State::hang_up),
            ),
    )
}

/// How wide a ring card is: the caller and the controls on one line.
const RING_WIDTH: f32 = 460.0;

/// The calls ringing, as cards for the corner: who calls, decline, answer (with video when they
/// called with it).
pub fn rings(s: &State, p: &Palette) -> Option<El> {
    if s.rings.is_empty() {
        return None;
    }
    let mut stack = col().key("call-rings").w(RING_WIDTH).gap(10.0);
    for ring in &s.rings {
        stack = stack.child(ring_card(s, ring, p));
    }
    Some(stack)
}

fn ring_card(s: &State, ring: &IncomingCall, p: &Palette) -> El {
    let caller = &ring.caller;
    let kind = if ring.video { t!("call.incoming_video") } else { t!("call.incoming") };
    let (account, room_id) = (ring.account.clone(), ring.room_id.clone());
    let answer = {
        let (account, room_id) = (account.clone(), room_id.clone());
        control(&icons::CALL, t!("call.answer"), p.success, Color::WHITE, p)
            .key(format!("ring-answer-{}-{}", ring.account, ring.room_id))
            .on_press_tx(crate::state::navigate(move |s| s.answer_call(&account, &room_id, false)))
    };
    let decline = {
        let (account, room_id) = (account.clone(), room_id.clone());
        control(&icons::HANG_UP, t!("call.decline"), p.danger, Color::WHITE, p)
            .key(format!("ring-decline-{}-{}", ring.account, ring.room_id))
            .on_press(move |s: &mut State| s.decline_call(&account, &room_id))
    };
    let mut buttons = row().gap(10.0).child(decline);
    if ring.video {
        let (account, room_id) = (account, room_id);
        buttons = buttons.child(
            control(&icons::VIDEO, t!("call.answer_video"), p.success, Color::WHITE, p)
                .key(format!("ring-answer-video-{}-{}", ring.account, ring.room_id))
                .on_press_tx(crate::state::navigate(move |s| s.answer_call(&account, &room_id, true))),
        );
    }
    buttons = buttons.child(answer);
    // One line: who calls, then the controls; a long name is cut short, never the controls.
    row()
        .key(format!("ring-{}-{}", ring.account, ring.room_id))
        .w_full()
        .padding(14.0)
        .gap(12.0)
        .center_y()
        .radius(14.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .shadow(6.0, 28.0, Color::BLACK.alpha(0.35))
        .role("alert")
        .label(caller.name.clone())
        .child(speaking_ring(media_avatar(&caller.id, &caller.name, caller.avatar.as_ref(), &s.media, 44.0), p))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(caller.name.clone()).font(font::BODY_STRONG).color(p.text).truncate())
                .child(text(kind).font(font::META).color(p.text_muted).truncate()),
        )
        .child(buttons)
}
