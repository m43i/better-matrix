//! The attachment preview: the image, PDF, video or audio clip full size over a dark,
//! see-through layer, with download and close at the top right. A batch of attachments gets a
//! strip of thumbnails along the bottom and arrows at the sides; the arrow keys step through it,
//! round from either end. A press beside the file, or Esc, closes it.

use matrix_core::{Attachment, AttachmentKind};
use rune::{Color, El, ScrollSpec, SurfaceFit, col, el, icon, row, text};

use super::icons;
use super::theme::{Palette, font};
use super::timeline::{Clips, audio_player, minutes};
use crate::i18n::{t, tf};
use crate::platform::video::{self, Playback};
use crate::state::{
    AUDIO_STEP, DocumentStatus, ImageStatus, Opens, Popover, Preview, PreviewContent, State, VideoStatus,
};

/// The widest a PDF page draws, in points.
const PAGE_WIDTH: f32 = 820.0;
/// Room around the file for the bar above it and the arrows beside it.
const MARGIN: f32 = 72.0;
/// The strip of the batch's thumbnails: each one's side, and the strip's height with its padding.
const THUMB: f32 = 56.0;
const STRIP: f32 = THUMB + 28.0;
/// The video's controls under it.
const CONTROLS: f32 = 52.0;
/// The scrubber's dot and the line it runs along.
const KNOB: f32 = 12.0;
const TRACK: f32 = 4.0;
/// The audio player's width.
const AUDIO_WIDTH: f32 = 420.0;

#[cfg(test)]
mod tests;

pub fn preview(s: &State, preview: &Preview, window: [f32; 2], p: &Palette) -> El {
    let backdrop = el()
        .key("preview-backdrop")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(Color::BLACK.alpha(0.86))
        .on_press(State::close_preview);
    let [w, h] = window;
    let batch = preview.batch.len() > 1;
    // The room the file has: under the bar, over the strip.
    let bottom = if batch { STRIP } else { 0.0 };
    let (width, height) = ((w - 2.0 * MARGIN).max(100.0), (h - 2.0 * MARGIN - bottom).max(100.0));
    let content = match &preview.content {
        PreviewContent::Image { thumbnail, status: state } => {
            let image = match state {
                ImageStatus::Ready(image) => Some(image),
                _ => thumbnail.as_ref(),
            };
            let mut content = col().items_center();
            if let Some(image) = image {
                content = content.size(width, height).child(
                    el().key("preview-image")
                        .size(width, height)
                        .role("img")
                        .label(preview.attachment.name.clone())
                        .pixels_fit(image.pixels(), SurfaceFit::Fit),
                );
            }
            let message = match state {
                ImageStatus::Failed(error) => Some(tf!("preview.failed", error = error)),
                ImageStatus::Downloading | ImageStatus::Decoding(_) => Some(t!("preview.loading").to_owned()),
                ImageStatus::Ready(_) => None,
            };
            if let Some(message) = message {
                let label = status(&message);
                content = content.child(if image.is_some() {
                    col().absolute(0.0, height + 8.0).w_full().items_center().child(label)
                } else {
                    label
                });
            }
            content
        }
        PreviewContent::Document { pages, status: state } => {
            let mut list = col().key("preview-pages").items_center().gap(16.0).py(MARGIN);
            let width = PAGE_WIDTH.min(w - 2.0 * MARGIN).max(100.0);
            for (index, (pixels, (pw, ph))) in pages.iter().enumerate() {
                let height = width * (*ph as f32) / (*pw as f32).max(1.0);
                list = list.child(
                    el().key(format!("preview-page-{index}"))
                        .size(width, height)
                        .radius(4.0)
                        .shadow(4.0, 24.0, Color::BLACK.alpha(0.5))
                        .pixels_fit(pixels, SurfaceFit::Fit),
                );
            }
            match state {
                DocumentStatus::Failed(error) => list = list.child(status(&tf!("preview.failed", error = error))),
                DocumentStatus::Downloading | DocumentStatus::Rendering(_) if pages.is_empty() => {
                    list = list.child(status(t!("preview.loading")));
                }
                _ => {}
            }
            // The pages scroll between the bar and the strip.
            col()
                .key("preview-scroll")
                .w_full()
                .h(h - bottom)
                .self_start()
                .scroll(ScrollSpec { stick_bottom: false })
                .child(list)
        }
        PreviewContent::Video { thumbnail, status: state } => {
            video_view(&preview.attachment, thumbnail.as_ref(), state, (width, height - CONTROLS), p)
        }
        PreviewContent::Audio => {
            let clips = Clips { audio: s.audio.as_ref(), decoded: &s.decoded_audio, speed: s.settings.playback_speed };
            col()
                .key("preview-audio")
                .items_center()
                .gap(20.0)
                .child(
                    el().size(96.0, 96.0)
                        .radius(48.0)
                        .fill(p.surface)
                        .border(1.0, p.divider)
                        .center()
                        .child(icon(&icons::AUDIO, 44.0).color(p.accent)),
                )
                .child(audio_player("preview", &preview.attachment, &clips, AUDIO_WIDTH.min(width), p))
        }
    };
    let mut layer = el()
        .key("preview-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .role("dialog")
        .modal(false, State::close_preview)
        .label(preview.attachment.name.clone())
        .on_secondary_press(|s: &mut State, point, _| {
            s.popover = Some(Popover::AttachmentMenu { at: (point.x as i32, point.y as i32) });
        })
        .child(backdrop)
        .child(col().w_full().h_full().pb(bottom).center().child(content))
        .child(bar(preview));
    if batch {
        layer = layer
            .child(side(&icons::CHEVRON_LEFT, t!("preview.previous"), "preview-previous", -1, h - bottom))
            .child(side(&icons::CHEVRON_RIGHT, t!("preview.next"), "preview-next", 1, h - bottom))
            .child(strip(s, preview, w, p));
    }
    layer
}

/// The file's name at the left, with its place in the batch; download and close at the right.
fn bar(preview: &Preview) -> El {
    let mut bar =
        row().key("preview-bar").absolute(0.0, 0.0).w_full().px(20.0).py(14.0).gap(10.0).center_y().child(
            text(preview.attachment.name.clone()).font(font::BODY_STRONG).color(Color::WHITE).shrink().truncate(),
        );
    if preview.batch.len() > 1 {
        let place = tf!("preview.position", index = preview.index + 1, count = preview.batch.len());
        bar = bar.child(text(place).font(font::META).color(Color::WHITE.alpha(0.7)));
    }
    bar.child(el().grow(1.0))
        .child(action(&icons::DOWNLOAD, t!("preview.download"), "preview-download").on_press(State::download_preview))
        .child(action(&icons::CLOSE, t!("preview.close"), "preview-close").on_press(State::close_preview))
}

/// A round button on the dark layer.
fn action(glyph: &'static rune::Icon, label: &str, key: &str) -> El {
    el().key(key.to_owned())
        .size(40.0, 40.0)
        .radius(20.0)
        .center()
        .fill(Color::WHITE.alpha(0.12))
        .hover(Color::WHITE.alpha(0.22))
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(label.to_owned())
        .focusable(true)
        .child(icon(glyph, 20.0).color(Color::WHITE))
}

/// The previous (`steps` back) or next arrow at its side, halfway down the room the file has.
fn side(glyph: &'static rune::Icon, label: &str, key: &str, steps: i32, height: f32) -> El {
    let top = Some((height - 40.0) / 2.0);
    let (left, right) = if steps < 0 { (Some(16.0), None) } else { (None, Some(16.0)) };
    action(glyph, label, key).pin(left, top, right, None).on_press(move |s: &mut State| s.step_preview(steps))
}

/// The batch's thumbnails along the bottom, the one showing ringed; pressing one shows it. Too
/// many to fit scroll sideways.
fn strip(s: &State, preview: &Preview, window_width: f32, p: &Palette) -> El {
    let thumbs = preview.batch.iter().enumerate().map(|(index, item)| {
        let current = index == preview.index;
        let attachment = &item.attachment;
        let image = attachment.preview().and_then(|(media, _)| s.media.image(&media));
        let mut thumb = el()
            .key(format!("preview-thumb-{index}"))
            .size(THUMB, THUMB)
            .radius(8.0)
            .clip_rounded(8.0)
            .center()
            .role("button")
            .label(attachment.name.clone())
            .cursor(rune::Cursor::Pointer)
            .opacity(if current { 1.0 } else { 0.6 })
            .on_press(move |s: &mut State| s.show_preview_item(index));
        thumb = match image {
            Some(image) => thumb.pixels_fit(image.pixels(), SurfaceFit::Fill),
            None => thumb.fill(Color::WHITE.alpha(0.12)).child(icon(kind_icon(attachment), 22.0).color(Color::WHITE)),
        };
        if attachment.kind == AttachmentKind::Video && image.is_some() {
            thumb = thumb.child(
                el().size(24.0, 24.0)
                    .radius(12.0)
                    .fill(Color::BLACK.alpha(0.55))
                    .center()
                    .child(icon(&icons::PLAY, 12.0).color(Color::WHITE)),
            );
        }
        // The ring sits outside the picture, so the current one keeps its size.
        el().padding(3.0).radius(11.0).border(2.0, if current { p.accent } else { Color::TRANSPARENT }).child(thumb)
    });
    col().absolute(0.0, 0.0).w_full().h_full().justify_end().items_center().pb(12.0).child(
        row()
            .key("preview-strip")
            .max_w((window_width - 2.0 * MARGIN).max(THUMB))
            .scroll_x()
            .child(row().gap(6.0).px(4.0).children(thumbs)),
    )
}

/// The glyph for a file without a picture.
fn kind_icon(attachment: &Attachment) -> &'static rune::Icon {
    match Opens::of(attachment) {
        Opens::Image => &icons::IMAGE,
        Opens::Video => &icons::VIDEO,
        Opens::Audio => &icons::AUDIO,
        Opens::Document | Opens::Download => &icons::FILE,
    }
}

/// A video fitted to `room` by its own size (else the sender's, else 16:9), its poster showing
/// until the first frame, and its controls under it. Pressing the picture plays or pauses it.
fn video_view(
    attachment: &Attachment,
    poster: Option<&rune::Image>,
    state: &VideoStatus,
    room: (f32, f32),
    p: &Palette,
) -> El {
    let player = match state {
        VideoStatus::Open(player) => Some(player),
        _ => None,
    };
    let (vw, vh) = player
        .and_then(video::Player::natural_size)
        .or(attachment.dimensions)
        .or_else(|| poster.and_then(rune::Image::size).map(|[w, h]| (w, h)))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map_or((16.0, 9.0), |(w, h)| (w as f32, h as f32));
    let scale = (room.0 / vw).min(room.1.max(100.0) / vh);
    let (width, height) = ((vw * scale).round(), (vh * scale).round());
    let mut frame = el()
        .key("preview-video")
        .size(width, height)
        .radius(8.0)
        .clip_rounded(8.0)
        .fill(Color::BLACK)
        .role("img")
        .label(attachment.name.clone())
        .cursor(rune::Cursor::Pointer)
        .on_press(State::toggle_preview_playback);
    if let Some(poster) = poster {
        frame = frame.pixels_fit(poster.pixels(), SurfaceFit::Fit);
    }
    if let Some(player) = player {
        frame = frame.child(
            player.show(el().key("preview-video-frame").absolute(0.0, 0.0).size(width, height), SurfaceFit::Fit),
        );
    }
    let message = match state {
        VideoStatus::Failed(error) => Some(tf!("preview.failed", error = error)),
        VideoStatus::Open(player) => match player.playback() {
            Playback::Failed(error) => Some(tf!("preview.failed", error = error)),
            Playback::Loading => Some(t!("preview.loading").to_owned()),
            _ => None,
        },
        VideoStatus::Downloading => Some(t!("preview.loading").to_owned()),
    };
    let below = match message {
        Some(message) => row().h(CONTROLS).center().child(status(&message)),
        None => controls(player, width, p),
    };
    col().items_center().child(frame).child(below)
}

/// Play or pause (replay once ended), the time, the scrubber and the sound, under the video.
fn controls(player: Option<&video::Player>, width: f32, p: &Palette) -> El {
    let Some(player) = player else { return el().h(CONTROLS) };
    let playback = player.playback();
    let (glyph, label) = match playback {
        Playback::Playing => (&icons::PAUSE, t!("audio.pause")),
        Playback::Ended => (&icons::REPLAY, t!("audio.play")),
        _ => (&icons::PLAY, t!("audio.play")),
    };
    let progress = if playback == Playback::Ended { 1.0 } else { player.progress() as f32 };
    let time = match player.duration() {
        Some(duration) => tf!("preview.time", at = minutes(player.time().min(duration)), length = minutes(duration)),
        None => minutes(player.time()),
    };
    let (sound, sound_label) = if player.muted() {
        (&icons::SPEAKER_OFF, t!("preview.unmute"))
    } else {
        (&icons::SPEAKER, t!("preview.mute"))
    };
    row()
        .key("preview-controls")
        .w(width.max(320.0))
        .h(CONTROLS)
        .gap(12.0)
        .center_y()
        .child(action(glyph, label, "preview-play").on_press(State::toggle_preview_playback))
        .child(text(time).font(font::META).color(Color::WHITE.alpha(0.85)))
        .child(scrubber(progress, p))
        .child(action(sound, sound_label, "preview-sound").on_press(State::toggle_video_sound))
}

/// The line the video runs along, filled as it plays, with a dot to drag; pressing anywhere
/// along it jumps there, and focused, the arrow keys step it.
fn scrubber(progress: f32, p: &Palette) -> El {
    el().key("preview-seek")
        .grow(1.0)
        .h(KNOB + 8.0)
        .role("slider")
        .label(t!("audio.seek"))
        .range_value(f64::from(progress), 0.0, 1.0)
        .range_step(AUDIO_STEP)
        .focusable(true)
        .cursor(rune::Cursor::Pointer)
        .on_drag(|s: &mut State, drag, _| {
            let width = (drag.w - KNOB).max(1.0);
            s.seek_video(f64::from((drag.x - KNOB / 2.0) / width));
        })
        .on_step(|s: &mut State, steps, _| s.step_video(steps))
        .on_step_limit(|s: &mut State, end, _| s.seek_video(if end { 1.0 } else { 0.0 }))
        .child(
            // The track runs between the dot's centres at either end; the dot rides the filled
            // part's end.
            row()
                .pin(Some(KNOB / 2.0), Some((KNOB + 8.0 - TRACK) / 2.0), Some(KNOB / 2.0), None)
                .h(TRACK)
                .radius(TRACK / 2.0)
                .fill(Color::WHITE.alpha(0.25))
                .child(
                    el().h_full().w_frac(progress).radius(TRACK / 2.0).fill(p.accent).child(
                        el().pin(None, Some((TRACK - KNOB) / 2.0), Some(-KNOB / 2.0), None)
                            .size(KNOB, KNOB)
                            .radius(KNOB / 2.0)
                            .fill(p.accent)
                            .border(2.0, Color::WHITE),
                    ),
                ),
        )
}

fn status(message: &str) -> El {
    text(message.to_owned()).font(font::BODY).color(Color::WHITE.alpha(0.8))
}
