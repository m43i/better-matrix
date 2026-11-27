use std::sync::Arc;

use matrix_core::{AccountId, Attachment, AttachmentKind, MediaRef};
use rune::{Env, Key, KeyEvent, Mods, Runtime};

use super::*;
use crate::demo;
use crate::ui::tests::{press, runtime};

#[test]
fn the_cached_thumbnail_should_stay_in_place_when_the_original_arrives() {
    let decoder = rune::ImageCache::new(Arc::new(|| {}));
    let image = decoder.load(include_bytes!("../../../../../packaging/icons/better-matrix-256.png").as_slice());
    image.wait().unwrap();
    let file = Preview {
        account: AccountId::new("@a:example.org"),
        room_id: "!room:example.org".into(),
        attachment: Attachment {
            kind: AttachmentKind::Image,
            name: "picture.png".into(),
            size: None,
            media: None,
            thumbnail: None,
            dimensions: None,
            audio: None,
            sticker: false,
        },
        media: MediaRef::opaque("picture"),
        batch: Vec::new(),
        index: 0,
        visit: 0,
        content: PreviewContent::Image { thumbnail: Some(image.clone()), status: ImageStatus::Downloading },
    };
    let mut state = State::detached();
    state.preview = Some(file);
    let mut rt = Runtime::new(
        state,
        |s, _| {
            let p = crate::ui::theme::palette(true);
            el().w_full().h_full().children(s.preview.as_ref().map(|file| preview(s, file, [1200.0, 760.0], &p)))
        },
        (1200.0, 760.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    let thumbnail = rt.presented("preview-image").expect("the thumbnail shows before the original arrives").0;
    rt.update(|s, _| {
        if let Some(file) = &mut s.preview {
            file.content = PreviewContent::Image { thumbnail: None, status: ImageStatus::Ready(image) };
        }
    });
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.presented("preview-image").unwrap().0, thumbnail);
}

fn key(rt: &mut Runtime<State>, key: Key) {
    rt.on_key = Some(crate::on_key);
    rt.key(KeyEvent { key, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
}

fn index(rt: &Runtime<State>) -> Option<usize> {
    rt.state.preview.as_ref().map(|preview| preview.index)
}

#[test]
fn a_batch_should_show_a_thumbnail_for_each_of_its_attachments() {
    let rt = runtime(demo::preview_batch);
    assert!((0..4).all(|i| rt.find(&format!("preview-thumb-{i}")).is_some()));
}

#[test]
fn a_lone_attachment_should_show_no_strip() {
    let rt = runtime(demo::attachment_preview);
    assert!(rt.find("preview-strip").is_none());
    assert!(rt.find("preview-next").is_none());
}

#[test]
fn pressing_a_thumbnail_should_show_its_attachment() {
    let mut rt = runtime(demo::preview_batch);
    press(&mut rt, "preview-thumb-3");
    assert_eq!(index(&rt), Some(3));
    assert!(rt.find("preview-pages").is_some());
}

#[test]
fn the_arrow_keys_should_step_through_the_batch_round_from_either_end() {
    let mut rt = runtime(demo::preview_batch);
    key(&mut rt, Key::Left);
    assert_eq!(index(&rt), Some(3));
    key(&mut rt, Key::Right);
    key(&mut rt, Key::Right);
    assert_eq!(index(&rt), Some(1));
}

#[test]
fn the_side_arrows_should_step_through_the_batch() {
    let mut rt = runtime(demo::preview_batch);
    press(&mut rt, "preview-next");
    assert_eq!(index(&rt), Some(1));
    press(&mut rt, "preview-previous");
    assert_eq!(index(&rt), Some(0));
}

#[test]
fn a_video_should_show_its_controls() {
    let rt = runtime(demo::preview_video);
    for control in ["preview-video", "preview-play", "preview-seek", "preview-sound"] {
        assert!(rt.find(control).is_some(), "{control} should show");
    }
}

#[test]
fn pressing_play_should_play_a_paused_video() {
    let mut rt = runtime(demo::preview_video);
    press(&mut rt, "preview-play");
    let playing = match &rt.state.preview {
        Some(Preview { content: PreviewContent::Video { status: VideoStatus::Open(player), .. }, .. }) => {
            player.is_playing()
        }
        _ => false,
    };
    assert!(playing);
}

#[test]
fn space_should_play_a_previewed_clip() {
    let mut rt = runtime(demo::preview_audio);
    assert!(rt.find("audio-play-preview").is_some());
    key(&mut rt, Key::Text(" ".into()));
    assert!(rt.state.audio.as_ref().is_some_and(|clip| clip.play));
}

#[test]
fn pressing_an_audio_card_beside_its_controls_should_open_it_in_the_preview() {
    let mut rt = runtime(demo::batch_messages);
    let ([x, y, _, _], _) = rt.presented("audio-$batch-2").expect("the recording's card");
    // Its top left corner, by the name: not play, the bars or the speed.
    rt.pointer_down(x + 4.0, y + 4.0);
    rt.pointer_up(x + 4.0, y + 4.0);
    rt.frame(1.0 / 60.0);
    assert!(matches!(&rt.state.preview, Some(Preview { index: 2, content: PreviewContent::Audio, .. })));
    assert!(rt.state.audio.is_none(), "opening it should not play it");
}

#[test]
fn opening_the_last_of_a_long_batch_should_scroll_its_thumbnail_into_view() {
    let files: Vec<Attachment> = (0..40)
        .map(|i| Attachment {
            kind: AttachmentKind::Image,
            name: format!("photo-{i}.png"),
            size: None,
            media: Some(MediaRef::opaque(&format!("photo-{i}"))),
            thumbnail: None,
            dimensions: Some((100, 100)),
            audio: None,
            sticker: false,
        })
        .collect();
    let last = files[39].clone();
    let mut rt = runtime(move |s| {
        demo::post_batch(s, files);
        s.open_attachment(
            &matrix_core::TimelineKey::room(demo::DEMO_THREAD.0),
            "$batch-39",
            &last,
            &mut rune::Tx::default(),
        );
    });
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let ([strip_x, _, strip_w, _], _) = rt.presented("preview-strip").expect("the strip");
    let ([x, _, w, _], _) = rt.presented("preview-thumb-39").expect("the last thumbnail");
    assert!(x >= strip_x - 0.5 && x + w <= strip_x + strip_w + 0.5, "{x}+{w} outside {strip_x}+{strip_w}");
}

#[test]
fn the_video_slider_should_seek_to_the_end_with_the_end_key() {
    let mut rt = runtime(demo::preview_video);
    rt.update(|_, tx| tx.focus("preview-seek", true));
    rt.frame(0.0);
    key(&mut rt, Key::End);
    let Some(Preview { content: PreviewContent::Video { status: VideoStatus::Open(player), .. }, .. }) =
        &rt.state.preview
    else {
        panic!("the preview should have a video player");
    };
    assert_eq!(player.progress(), 1.0);
}

#[test]
fn setting_the_video_sliders_accessible_value_should_seek_to_that_fraction() {
    use rune::accesskit::{Action, ActionData};

    let mut rt = runtime(demo::preview_video);
    assert!(rt.accessibility_action("preview-seek", Action::SetValue, Some(ActionData::NumericValue(0.375))));
    rt.frame(0.0);
    let Some(Preview { content: PreviewContent::Video { status: VideoStatus::Open(player), .. }, .. }) =
        &rt.state.preview
    else {
        panic!("the preview should have a video player");
    };
    assert_eq!(player.progress(), 0.375);
}

#[test]
fn a_video_slider_should_use_the_space_between_the_time_and_sound_button() {
    let mut state = State::detached();
    demo::preview_video(&mut state);
    state.settings.zoom = crate::state::Zoom::from_percent(150);
    let mut rt = Runtime::new(state, crate::ui::view, (720.0, 480.0), Env::new(true, 1.5));
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }

    let [time_x, _, time_w, _] = rt.presented("preview-time").unwrap().0;
    let [seek_x, _, seek_w, _] = rt.presented("preview-seek").unwrap().0;
    let [sound_x, _, _, _] = rt.presented("preview-sound").unwrap().0;
    assert!(seek_x >= time_x + time_w && seek_x + seek_w <= sound_x);
}
