//! Calls: the buttons in a room's header, the stage that replaces the room's timeline during its
//! call (faces for a voice call, a grid of videos for a video call, over a glow that wanders and
//! swells with the voices), its controls, the call bar in the sidebar elsewhere, and the cards of
//! calls ringing.

use matrix_core::{CallParticipant, CallPhase, CallVia, DeviceKind, IncomingCall, RoomSummary, VideoId};
use rune::{Color, El, SurfaceFit, col, el, icon, row, text};

use super::components::{account_avatar, icon_button, media_avatar, menu_row, popover_panel};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{AccountData, Call, State};

/// The size of a face on a voice call's stage.
const FACE: f32 = 96.0;
/// The size of a round control button.
const CONTROL: f32 = 48.0;
/// A video tile's aspect, width over height.
const TILE_ASPECT: f32 = 16.0 / 9.0;

/// The header's call buttons: Join while a call is under way in the room, else voice and video
/// call. None while the room's own call shows here, or where the room allows no call.
pub fn header_buttons(s: &State, room: &RoomSummary, p: &Palette) -> Vec<El> {
    if room.call.via.is_none() {
        return Vec::new();
    }
    if let Some(call) = s.call_here() {
        // In the call, looking at the chat: the way back to the stage.
        return if call.chat {
            vec![
                pill(&icons::CALL, t!("call.show_stage"), p.success, p)
                    .key("call-back")
                    .on_press(State::toggle_call_chat),
            ]
        } else {
            Vec::new()
        };
    }
    if !room.call.participants.is_empty() {
        let label = tf!("call.people", count = room.call.participants.len());
        return vec![
            pill(&icons::CALL, t!("call.join"), p.success, p)
                .key("call-join")
                .help(label)
                .on_press(|s: &mut State| s.start_call(false)),
        ];
    }
    vec![
        icon_button(&icons::CALL, t!("call.start"), false, p.text_muted, p)
            .key("call-start")
            .help(t!("call.start"))
            .on_press(|s: &mut State| s.start_call(false)),
        icon_button(&icons::VIDEO, t!("call.start_video"), false, p.text_muted, p)
            .key("call-start-video")
            .help(t!("call.start_video"))
            .on_press(|s: &mut State| s.start_call(true)),
    ]
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
    let info = &call.info;
    let video = info.video
        || info.camera.is_some()
        || info.screen.is_some()
        || info.participants.iter().any(|person| person.camera.is_some() || person.screen.is_some());
    let people = if video { grid(s, account, call, p) } else { faces(s, account, call, p) };
    let mut content = col().w_full().h_full().padding(24.0).gap(16.0).center();
    if let Some(status) = status(call) {
        content = content.child(text(status).key("call-status").font(font::BODY).color(p.text_muted));
    }
    content = content.child(people);
    col()
        .key("call-stage")
        .grow(1.0)
        .w_full()
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
fn status(call: &Call) -> Option<String> {
    let info = &call.info;
    match info.phase {
        CallPhase::Calling => {
            let name = info.participants.first().map_or("", |person| person.person.name.as_str());
            Some(tf!("call.calling", name = name))
        }
        CallPhase::Connecting => Some(t!("call.connecting").to_owned()),
        CallPhase::Reconnecting => Some(t!("call.reconnecting").to_owned()),
        CallPhase::Connected if info.participants.is_empty() => Some(t!("call.alone").to_owned()),
        CallPhase::Connected => None,
    }
}

/// A voice call: everyone's face, the user's own first, a ring around whoever speaks.
fn faces(s: &State, account: &AccountData, call: &Call, p: &Palette) -> El {
    let own = account_avatar(&account.summary, &s.media, FACE);
    let mut list = row().key("call-faces").gap(32.0).center().flex_wrap();
    list = list.child(face(own, t!("call.you"), call.speaking.contains(""), !call.info.microphone, "call-face-you", p));
    for person in &call.info.participants {
        let avatar =
            media_avatar(&person.person.id, &person.person.name, person.person.avatar.as_ref(), &s.media, FACE);
        let key = format!("call-face-{}", person.key);
        list =
            list.child(face(avatar, &person.person.name, call.speaking.contains(&person.key), person.muted, &key, p));
    }
    list
}

fn face(avatar: El, name: &str, speaking: bool, muted: bool, key: &str, p: &Palette) -> El {
    let ring = if speaking { speaking_ring(avatar, p) } else { avatar };
    let mut label =
        row().gap(6.0).center_y().child(text(name.to_owned()).font(font::BODY_STRONG).color(p.text).truncate());
    if muted {
        label = label.child(icon(&icons::MIC_OFF, 14.0).color(p.text_muted));
    }
    col().key(key.to_owned()).w(FACE + 40.0).gap(10.0).items_center().child(ring).child(label)
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

/// One tile of the video grid.
struct Tile<'a> {
    key: String,
    name: String,
    video: Option<VideoId>,
    /// Shown in full rather than cropped: a shared screen.
    screen: bool,
    person: Option<&'a CallParticipant>,
    speaking: bool,
    muted: bool,
}

/// A video call: shared screens first, then everyone's camera (or face), the user's own last.
fn grid(s: &State, account: &AccountData, call: &Call, p: &Palette) -> El {
    let info = &call.info;
    let mut tiles: Vec<Tile> = Vec::new();
    for person in &info.participants {
        if let Some(screen) = person.screen {
            let name = tf!("call.screen_of", name = person.person.name);
            tiles.push(Tile {
                key: format!("call-screen-{}", person.key),
                name,
                video: Some(screen),
                screen: true,
                person: Some(person),
                speaking: false,
                muted: false,
            });
        }
    }
    if let Some(screen) = info.screen {
        let name = tf!("call.screen_of", name = t!("call.you"));
        tiles.push(Tile {
            key: "call-screen-you".into(),
            name,
            video: Some(screen),
            screen: true,
            person: None,
            speaking: false,
            muted: false,
        });
    }
    for person in &info.participants {
        tiles.push(Tile {
            key: format!("call-tile-{}", person.key),
            name: person.person.name.clone(),
            video: person.camera,
            screen: false,
            person: Some(person),
            speaking: call.speaking.contains(&person.key),
            muted: person.muted,
        });
    }
    tiles.push(Tile {
        key: "call-tile-you".into(),
        name: t!("call.you").to_owned(),
        video: info.camera,
        screen: false,
        person: None,
        speaking: call.speaking.contains(""),
        muted: !info.microphone,
    });
    let mut rows = col().key("call-grid").w_full().grow(1.0).min_h(0.0).gap(12.0);
    // Faces in the strip below a shared screen are smaller, to fit its short tiles.
    let line = |tiles: &[Tile], grow: f32, face: f32| {
        tiles.iter().fold(row().w_full().grow(grow).min_h(0.0).gap(12.0).center(), |cells, tile| {
            cells.child(video_tile(s, account, call, tile, face, p))
        })
    };
    // A shared screen takes most of the stage, everyone's camera in a strip below it.
    let screens = tiles.iter().take_while(|tile| tile.screen).count();
    if screens > 0 {
        return rows.child(line(&tiles[..screens], 3.0, 72.0)).child(line(&tiles[screens..], 1.0, 44.0));
    }
    // As square a grid as the tiles make: two columns from two tiles, three from five, …
    let columns = (tiles.len() as f32).sqrt().ceil().max(1.0) as usize;
    for chunk in tiles.chunks(columns) {
        rows = rows.child(line(chunk, 1.0, 72.0));
    }
    rows
}

fn video_tile(s: &State, account: &AccountData, call: &Call, tile: &Tile, face: f32, p: &Palette) -> El {
    let mut cell = el()
        .key(tile.key.clone())
        .grow(1.0)
        .h_full()
        .max_w(960.0)
        .aspect_ratio(TILE_ASPECT)
        .radius(14.0)
        .fill(Color::BLACK.alpha(0.55))
        .clip_rounded(14.0);
    let pixels = tile.video.and_then(|id| call.videos.get(&id));
    cell = match pixels {
        Some(pixels) => cell.child(
            el().w_full().h_full().pixels_fit(pixels, if tile.screen { SurfaceFit::Fit } else { SurfaceFit::Fill }),
        ),
        // No video: the face, centred on the tile.
        None => {
            let avatar = match tile.person {
                Some(person) => {
                    media_avatar(&person.person.id, &person.person.name, person.person.avatar.as_ref(), &s.media, face)
                }
                None => account_avatar(&account.summary, &s.media, face),
            };
            cell.child(col().w_full().h_full().center().child(avatar))
        }
    };
    let mut label = row()
        .pin(Some(10.0), None, None, Some(10.0))
        .h(26.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(13.0)
        .fill(Color::BLACK.alpha(0.5))
        .child(text(tile.name.clone()).font(font::META).color(Color::WHITE).truncate());
    if tile.muted {
        label = label.child(icon(&icons::MIC_OFF, 12.0).color(Color::WHITE));
    }
    cell = cell.child(label);
    // Whoever speaks: a ring just inside the tile, drawn over the video.
    if tile.speaking {
        cell = cell.child(
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(14.0).border(2.5, p.accent).a11y_hidden(),
        );
    }
    cell
}

/// The call's controls, floating at the bottom of the stage.
fn controls(call: &Call, p: &Palette) -> El {
    let info = &call.info;
    let toggle = |glyph, label: &str, on: bool, key: &str| {
        control(glyph, label, if on { p.surface } else { p.text.alpha(0.85) }, if on { p.text } else { p.bg }, p)
            .key(key.to_owned())
    };
    let mut bar = row()
        .key("call-controls")
        .gap(14.0)
        .padding(12.0)
        .mb(24.0)
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
    bar = bar.child(with_devices(microphone.on_press(State::toggle_microphone), DeviceKind::Microphone, p));
    let camera = if info.camera.is_some() {
        toggle(&icons::VIDEO, t!("call.camera_off"), true, "call-camera")
    } else {
        toggle(&icons::VIDEO_OFF, t!("call.camera_on"), false, "call-camera")
    };
    // Device to device, the connection carries the camera it started with (if any) and no
    // screen.
    if info.via == CallVia::Rtc || info.video {
        bar = bar.child(with_devices(camera.on_press(State::toggle_camera), DeviceKind::Camera, p));
    }
    if info.via == CallVia::Rtc {
        let sharing = info.screen.is_some();
        let label = if sharing { t!("call.stop_share") } else { t!("call.share") };
        let (fill, color) = if sharing { (p.accent, p.on_accent) } else { (p.surface, p.text) };
        bar = bar.child(
            control(&icons::SCREEN_SHARE, label, fill, color, p).key("call-screen").on_press(State::toggle_screen),
        );
    }
    bar = bar
        .child(
            control(&icons::CHAT, t!("call.show_chat"), p.surface, p.text, p)
                .key("call-chat")
                .on_press(State::toggle_call_chat),
        )
        .child(
            control(&icons::HANG_UP, t!("call.hang_up"), p.danger, Color::WHITE, p)
                .key("call-hang-up")
                .on_press(State::hang_up),
        );
    row().w_full().center().child(bar)
}

/// A control with a chevron beside it that opens the devices of its kind.
fn with_devices(button: El, kind: DeviceKind, p: &Palette) -> El {
    let chevron = el()
        .key(devices_key(kind))
        .size(20.0, CONTROL)
        .radius(10.0)
        .center()
        .hover(p.hover.alpha(p.hover.a * 2.0))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .focusable(true)
        .label(t!("call.devices").to_owned())
        .help(t!("call.devices").to_owned())
        .child(icon(&icons::CHEVRON_UP, 14.0).color(p.text_muted))
        .on_press(move |s: &mut State| s.open_call_devices(kind));
    row().gap(2.0).center_y().child(button).child(chevron)
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
    if s.call_here().is_some_and(|call| !call.chat) {
        return None;
    }
    let room = s.accounts.get(&call.info.account).and_then(|account| account.room(&call.info.room_id));
    let name = room.map_or_else(|| call.info.room_id.clone(), super::room::display_name);
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
            .on_press(State::open_call)
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
            .on_press(move |s: &mut State| s.answer_call(&account, &room_id, false))
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
                .on_press(move |s: &mut State| s.answer_call(&account, &room_id, true)),
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
