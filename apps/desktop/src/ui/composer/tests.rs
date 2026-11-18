use rune::{Env, KeyEvent, Mods, Runtime};

use super::*;
use crate::{demo, ui};

fn long_draft(target: ComposerTarget) -> Runtime<State> {
    let mut state = State::detached();
    demo::seed(&mut state);
    state.select_room(demo::DEMO_THREAD.0);
    if target == ComposerTarget::Thread {
        state.open_thread(demo::DEMO_THREAD.1);
    }
    let mut rt = Runtime::new(state, ui::view, (1200.0, 760.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt.paste("Another line\n".repeat(40));
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    rt
}

fn field(rt: &Runtime<State>, target: ComposerTarget) -> rune::ElementInfo {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(composer_field(target))).unwrap()
}

#[test]
fn the_mouse_wheel_should_scroll_over_composer_padding() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let mut rt = long_draft(target);
        let before = field(&rt, target).scrolled[1];
        let editor = rt.presented(&key(target, "composer-editor")).unwrap().0;
        rt.wheel(editor[0] + 4.0, editor[1] + editor[3] / 2.0, 120.0);
        rt.frame(1.0 / 60.0);
        assert!(field(&rt, target).scrolled[1] < before, "the padding belongs to the editor's scroll area");
    }
}

#[test]
fn arrow_navigation_and_keyboard_selection_should_keep_the_caret_visible() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        for selecting in [false, true] {
            let mut rt = long_draft(target);
            for _ in 0..36 {
                rt.key(KeyEvent { key: Key::Up, mods: Mods { shift: selecting, ..Default::default() } });
                rt.frame(1.0 / 60.0);
                let viewport = rt.presented(&key(target, "composer-scroll")).unwrap().0;
                let caret = field(&rt, target).caret.unwrap();
                assert!(
                    caret[1] >= viewport[1] - 1.0 && caret[1] + caret[3] <= viewport[1] + viewport[3] + 1.0,
                    "{target:?}, selecting {selecting}: caret {caret:?}, viewport {viewport:?}"
                );
            }
            assert!(rt.state.composer(target).edit.focus < 100);
            let edit = &rt.state.composer(target).edit;
            assert_eq!(edit.anchor != edit.focus, selecting);
        }
    }
}

#[test]
fn an_overflowing_composer_should_have_a_scroll_indicator() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let rt = long_draft(target);
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("scroll-bar")));
    }
}

#[test]
fn dragging_a_selection_above_the_composer_should_reveal_earlier_lines() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let mut rt = long_draft(target);
        let viewport = rt.presented(&key(target, "composer-scroll")).unwrap().0;
        let before = field(&rt, target).scrolled[1];
        let (x, start, end) = (viewport[0] + 30.0, viewport[1] + viewport[3] - 24.0, viewport[1] - 30.0);
        rt.pointer_down(x, start);
        rt.frame(1.0 / 60.0);
        rt.pointer_move(x, end);
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
        }
        rt.pointer_up(x, end);
        rt.frame(1.0 / 60.0);
        let edit = &rt.state.composer(target).edit;
        assert_ne!(edit.anchor, edit.focus);
        assert!(field(&rt, target).scrolled[1] < before);
        let caret = field(&rt, target).caret.unwrap();
        assert!(caret[1] >= viewport[1] - 1.0 && caret[1] + caret[3] <= viewport[1] + viewport[3] + 1.0);
    }
}

#[test]
fn scrolling_a_selected_draft_should_leave_the_caret_offscreen() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let mut rt = long_draft(target);
        rt.key(KeyEvent { key: Key::Up, mods: Mods { shift: true, ..Default::default() } });
        rt.frame(1.0 / 60.0);
        let before = field(&rt, target).scrolled[1];
        let (x, y) = rt.find(&key(target, "composer-scroll")).unwrap();
        rt.wheel(x, y, 220.0);
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
        }
        assert!(field(&rt, target).scrolled[1] < before - 100.0);
        let viewport = rt.presented(&key(target, "composer-scroll")).unwrap().0;
        let caret = field(&rt, target).caret.unwrap();
        assert!(caret[1] > viewport[1] + viewport[3]);
    }
}

#[test]
fn dragging_the_composer_scrollbar_should_reveal_earlier_lines_without_losing_focus() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let mut rt = long_draft(target);
        rt.set_system_scroll_bars(rune::ScrollBars::Always);
        rt.frame(1.0 / 60.0);
        rt.settle_all();
        rt.frame(1.0 / 60.0);
        let before = field(&rt, target).scrolled[1];
        let area = key(target, "composer-scroll-scroll-area");
        let thumb = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("thumb") && e.path.contains(&area))
            .unwrap()
            .rect;
        let (x, y) = (thumb[0] + thumb[2] / 2.0, thumb[1] + thumb[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_move(x, y - 80.0);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(x, y - 80.0);
        rt.frame(1.0 / 60.0);
        assert!(field(&rt, target).scrolled[1] < before);
        assert!(rt.state.composer(target).edit.focused);
    }
}

#[test]
fn a_composer_should_stay_focused_after_sending_so_the_next_message_can_follow() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let mut state = State::detached();
        demo::seed(&mut state);
        state.select_room(demo::DEMO_THREAD.0);
        state.open_thread(demo::DEMO_THREAD.1);
        let mut rt = Runtime::new(state, ui::view, (1200.0, 760.0), Env { system_dark: true, scale: 1.0 });
        rt.frame(0.0);
        rt.update(move |_, tx| tx.focus(composer_field(target), false));
        rt.frame(1.0 / 60.0);
        rt.paste("First");
        rt.frame(1.0 / 60.0);
        rt.key(KeyEvent { key: rune::Key::Enter, mods: Mods::default() });
        rt.frame(1.0 / 60.0);
        assert!(rt.state.composer(target).is_empty(), "the first message is sent");
        rt.paste("Second");
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.composer(target).edit.text, "Second");
    }
}

#[test]
fn a_touchpad_gesture_should_scroll_a_long_draft() {
    for target in [ComposerTarget::Room, ComposerTarget::Thread] {
        let mut rt = long_draft(target);
        let before = field(&rt, target).scrolled[1];
        let scroll = rt.presented(&key(target, "composer-scroll")).unwrap().0;
        let (x, y) = (scroll[0] + scroll[2] / 2.0, scroll[1] + scroll[3] / 2.0);
        let finger = |phase, dy| rune::WheelEvent::new(0.0, dy).at(x, y).phase(phase).precise(true);
        rt.wheel_event(finger(rune::GesturePhase::Began, 0.0));
        for _ in 0..20 {
            rt.wheel_event(finger(rune::GesturePhase::Moved, 4.0));
            rt.frame(1.0 / 60.0);
        }
        rt.wheel_event(finger(rune::GesturePhase::Ended, 0.0));
        rt.settle_all();
        assert!(field(&rt, target).scrolled[1] < before, "two fingers reach earlier draft lines");
    }
}
