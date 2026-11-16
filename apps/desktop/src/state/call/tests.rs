use super::*;
use matrix_core::{AccountId, CallParticipant, CallPhase, CallVia, Person};

fn info(participants: Vec<CallParticipant>) -> CallInfo {
    CallInfo {
        call: 1,
        account: AccountId::new("@me:example.org"),
        room_id: "!r".into(),
        via: CallVia::Rtc,
        phase: CallPhase::Connected,
        video: true,
        microphone: true,
        camera: None,
        screen: None,
        participants,
    }
}

fn person(key: &str, camera: Option<u64>) -> CallParticipant {
    CallParticipant {
        key: key.into(),
        person: Person { id: key.into(), name: key.into(), avatar: None },
        muted: false,
        camera: camera.map(VideoId),
        screen: None,
    }
}

#[test]
fn a_new_camera_should_be_watched_and_a_gone_one_dropped() {
    let mut call = Call::new(info(Vec::new()));
    let first = call.update(info(vec![person("a", Some(1))]));
    assert_eq!(first.added, [VideoId(1)]);
    let second = call.update(info(vec![person("a", Some(2))]));
    assert_eq!((second.added, second.removed), (vec![VideoId(2)], vec![VideoId(1)]));
}

#[test]
fn someone_starting_to_speak_should_change_the_view() {
    let mut call = Call::new(info(vec![person("a", None)]));
    let now = Instant::now();
    assert!(call.heard(vec![("a".into(), 200)], now));
    assert!(!call.heard(vec![("a".into(), 180)], now));
    assert!(call.heard(vec![("a".into(), 0)], now + SPEAKING_HOLD));
}

#[test]
fn a_pause_between_words_should_not_stop_the_speaking_ring() {
    let mut call = Call::new(info(vec![person("a", None)]));
    let now = Instant::now();
    call.heard(vec![("a".into(), 200)], now);
    assert!(!call.heard(vec![("a".into(), 0)], now + Duration::from_millis(100)));
    assert!(call.speaking.contains("a"));
}

#[test]
fn the_glow_should_follow_the_user_then_everyone_else() {
    let mut call = Call::new(info(vec![person("a", None), person("b", None)]));
    call.heard(vec![("b".into(), 255), (String::new(), 0)], Instant::now());
    assert_eq!(call.glow_levels(), [0.0, 0.0, 1.0]);
}

#[test]
fn a_pin_should_survive_its_camera_restarting() {
    let mut call = Call::new(info(vec![person("a", Some(1))]));
    let pin = CallTile::Camera("a".into());
    call.layout = CallLayout::Focus(Some(pin.clone()));
    call.update(info(vec![person("a", Some(2))]));
    let video = call.video_id(&pin);
    assert_eq!((call.layout, video), (CallLayout::Focus(Some(pin)), Some(VideoId(2))));
}

#[test]
fn a_departed_pin_should_restore_the_automatic_layout() {
    let mut call = Call::new(info(vec![person("a", Some(1))]));
    call.layout = CallLayout::Focus(Some(CallTile::Camera("a".into())));
    call.update(info(Vec::new()));
    assert_eq!(call.layout, CallLayout::Auto);
}

#[test]
fn an_ended_pinned_share_should_restore_the_automatic_layout() {
    let mut shared = person("a", Some(1));
    shared.screen = Some(VideoId(2));
    let mut call = Call::new(info(vec![shared]));
    call.layout = CallLayout::Focus(Some(CallTile::Screen("a".into())));
    call.update(info(vec![person("a", Some(1))]));
    assert_eq!(call.layout, CallLayout::Auto);
}

#[test]
fn a_brief_interjection_should_not_switch_focus() {
    let mut call = Call::new(info(vec![person("a", Some(1)), person("b", Some(2))]));
    call.layout = CallLayout::Focus(None);
    let now = Instant::now();
    call.heard(vec![("a".into(), 200)], now);
    call.heard(vec![("a".into(), 200)], now + SPEAKER_HOLD);
    call.heard(vec![("b".into(), 240)], now + SPEAKER_HOLD + Duration::from_millis(100));
    assert_eq!(call.plan().tiles[0].key, CallTile::Camera("a".into()));
}

#[test]
fn sustained_speech_should_switch_focus() {
    let mut call = Call::new(info(vec![person("a", Some(1)), person("b", Some(2))]));
    call.layout = CallLayout::Focus(None);
    let now = Instant::now();
    call.heard(vec![("b".into(), 240)], now);
    call.heard(vec![("b".into(), 240)], now + SPEAKER_HOLD);
    assert_eq!(call.plan().tiles[0].key, CallTile::Camera("b".into()));
}

#[test]
fn pauses_between_words_should_not_restart_speaker_focus() {
    let mut call = Call::new(info(vec![person("a", Some(1)), person("b", Some(2))]));
    call.layout = CallLayout::Focus(None);
    let now = Instant::now();
    for tick in 0..=15 {
        let level = if tick == 8 || tick == 12 { 0 } else { 240 };
        call.heard(vec![("b".into(), level)], now + Duration::from_millis(tick * 100));
    }
    assert_eq!(call.plan().tiles[0].key, CallTile::Camera("b".into()));
}

#[test]
fn hiding_self_view_should_keep_the_camera_on() {
    let mut state = State::detached();
    let mut info = info(vec![person("a", Some(1))]);
    info.camera = Some(VideoId(2));
    state.call = Some(Call::new(info));
    state.toggle_call_self();
    let call = state.call.as_ref().unwrap();
    assert!(call.info.camera == Some(VideoId(2)) && !call.visible_videos().contains(&VideoId(2)));
}

#[test]
fn another_frame_with_the_same_aspect_should_not_rebuild_the_view() {
    let mut call = Call::new(info(Vec::new()));
    call.update(info(vec![person("a", Some(1))]));
    let pixels = &call.videos[&VideoId(1)];
    pixels.set(2, 4, vec![0; 32]);
    assert!(call.measure_videos());
    call.videos[&VideoId(1)].set(4, 8, vec![0; 128]);
    assert!(!call.measure_videos());
}

#[test]
fn a_portrait_camera_should_offer_both_full_and_cropped_views() {
    let mut state = State::detached();
    let mut call = Call::new(info(Vec::new()));
    call.update(info(vec![person("a", Some(1))]));
    call.videos[&VideoId(1)].set(2, 4, vec![0; 32]);
    call.measure_videos();
    state.call = Some(call);
    let key = CallTile::Camera("a".into());
    state.toggle_call_fit(key.clone());
    assert_eq!(state.call.as_ref().unwrap().full_video.get(&key), Some(&false));
}

#[test]
fn compact_chat_should_page_through_every_surface() {
    let mut state = State::detached();
    let mut shared = person("a", Some(1));
    shared.screen = Some(VideoId(3));
    let mut call = Call::new(info(vec![shared, person("b", Some(2))]));
    call.chat = true;
    call.room_size = [400.0, 600.0];
    call.layout = CallLayout::Grid;
    state.call = Some(call);
    let mut seen = HashSet::new();
    for _ in 0..4 {
        let call = state.call.as_ref().unwrap();
        assert_eq!(call.plan().tiles.len(), 1);
        seen.insert(call.plan().tiles[0].key.clone());
        state.page_call(true);
    }
    assert_eq!(
        seen,
        [
            CallTile::Camera("a".into()),
            CallTile::Camera("b".into()),
            CallTile::Screen("a".into()),
            CallTile::Camera(String::new())
        ]
        .into_iter()
        .collect()
    );
}
