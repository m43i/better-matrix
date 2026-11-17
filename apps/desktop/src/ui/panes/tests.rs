use rune::{Cursor, Key, KeyEvent, Mods, Runtime};

use crate::{
    demo,
    state::{Pane, SidePanel, State, Zoom},
};

use super::super::tests::{press, runtime};

fn drag(rt: &mut Runtime<State>, key: &str, dx: f32) {
    let (x, y) = rt.find(key).expect("resize handle");
    rt.pointer_down(x, y);
    rt.frame(0.0);
    rt.pointer_move(x + dx, y);
    rt.frame(0.0);
    rt.pointer_up(x + dx, y);
    rt.frame(0.0);
}

fn width(rt: &Runtime<State>, key: &str) -> f32 {
    rt.presented(key).expect("pane").0[2]
}

fn settle(rt: &mut Runtime<State>) {
    for _ in 0..60 {
        rt.frame(1.0 / 60.0);
    }
}

fn room(s: &mut State) {
    s.select_room(demo::DEMO_THREAD.0);
}

#[test]
fn dragging_the_sidebar_should_resize_it_without_changing_the_rail() {
    let mut rt = runtime(room);
    let rail = rt.presented("rail").unwrap().0;
    drag(&mut rt, "sidebar-resize", 80.0);
    assert_eq!(width(&rt, "sidebar"), 340.0);
    assert_eq!(rt.presented("rail").unwrap().0, rail);
}

#[test]
fn the_sidebar_should_stop_at_its_minimum_width() {
    let mut rt = runtime(room);
    drag(&mut rt, "sidebar-resize", -1000.0);
    assert_eq!(width(&rt, "sidebar"), 220.0);
}

#[test]
fn the_sidebar_should_stop_at_its_maximum_width() {
    let mut rt = runtime(room);
    drag(&mut rt, "sidebar-resize", 1000.0);
    assert_eq!(width(&rt, "sidebar"), 420.0);
}

#[test]
fn dragging_the_side_panel_left_should_make_it_wider() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    drag(&mut rt, "side-panel-resize", -80.0);
    assert_eq!(width(&rt, "info-panel"), 400.0);
}

#[test]
fn a_side_panel_should_stop_at_its_minimum_width() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    drag(&mut rt, "side-panel-resize", 1000.0);
    assert_eq!(width(&rt, "info-panel"), 300.0);
}

#[test]
fn a_side_panel_should_stop_before_crowding_the_conversation() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    drag(&mut rt, "side-panel-resize", -1000.0);
    assert!(width(&rt, "room") >= 320.0);
    assert!(width(&rt, "sidebar") >= 220.0);
    assert!(width(&rt, "info-panel") <= 640.0);
}

#[test]
fn a_side_panel_should_stop_at_its_maximum_in_a_wide_window() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    rt.set_viewport((1800.0, 760.0), 1.0);
    settle(&mut rt);
    drag(&mut rt, "side-panel-resize", -1000.0);
    assert_eq!(width(&rt, "info-panel"), 640.0);
}

#[test]
fn every_side_panel_should_keep_the_chosen_width() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    drag(&mut rt, "side-panel-resize", -80.0);
    for (panel, key) in [
        (SidePanel::Members, "members-panel"),
        (SidePanel::Threads, "threads-panel"),
        (SidePanel::Files, "files-panel"),
        (SidePanel::Pins, "pins-panel"),
        (SidePanel::Info, "info-panel"),
    ] {
        rt.update(|s, tx| {
            s.show_side(panel);
            tx.instant();
        });
        rt.frame(0.0);
        assert_eq!(width(&rt, key), 400.0, "{panel:?}");
    }
}

#[test]
fn a_thread_should_resize_from_its_left_edge() {
    let mut rt = runtime(|s| {
        room(s);
        s.open_thread(demo::DEMO_THREAD.1);
    });
    drag(&mut rt, "thread-resize", -80.0);
    assert_eq!(width(&rt, "thread-panel"), 500.0);
}

#[test]
fn a_narrow_thread_should_keep_every_formatting_control_inside_the_composer() {
    let rt = runtime(|s| {
        room(s);
        s.open_thread(demo::DEMO_THREAD.1);
        s.panes.set(Pane::Thread, 300.0);
        s.thread_composer.toggle_formatting();
    });
    let panel = rt.presented("thread-panel").unwrap().0;
    for format in crate::state::Format::ALL {
        let control = rt.presented(&format!("thread-format-{}", format.id())).unwrap().0;
        assert!(control[0] >= panel[0] && control[0] + control[2] <= panel[0] + panel[2]);
    }
}

#[test]
fn a_narrow_thread_should_keep_the_recording_controls_inside_the_composer() {
    let rt = runtime(|s| {
        room(s);
        s.open_thread(demo::DEMO_THREAD.1);
        s.panes.set(Pane::Thread, 300.0);
        s.start_voice_memo(crate::state::ComposerTarget::Thread);
    });
    let panel = rt.presented("thread-panel").unwrap().0;
    for key in ["thread-voice-cancel", "thread-voice-send"] {
        let control = rt.presented(key).expect("recording control").0;
        assert!(control[0] >= panel[0] && control[0] + control[2] <= panel[0] + panel[2]);
    }
}

#[test]
fn resize_handles_should_show_the_column_resize_cursor_while_hovered_and_dragged() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    for key in ["sidebar-resize", "side-panel-resize"] {
        let (x, y) = rt.find(key).unwrap();
        rt.pointer_move(x, y);
        rt.frame(0.0);
        assert_eq!(rt.cursor(), Cursor::ResizeColumn, "hovering {key}");
        rt.pointer_down(x, y);
        rt.frame(0.0);
        rt.pointer_move(x + 60.0, y);
        rt.frame(0.0);
        assert_eq!(rt.cursor(), Cursor::ResizeColumn, "dragging {key}");
        rt.pointer_up(x + 60.0, y);
        rt.frame(0.0);
    }
}

#[test]
fn escape_should_restore_the_width_before_the_drag() {
    let mut rt = runtime(room);
    let (x, y) = rt.find("sidebar-resize").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_move(x + 80.0, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    settle(&mut rt);
    assert_eq!(width(&rt, "sidebar"), 260.0);
}

#[test]
fn a_narrow_window_should_keep_the_conversation_and_panel_readable() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    rt.set_viewport((720.0, 760.0), 1.0);
    settle(&mut rt);
    assert!(rt.find("sidebar-resize").is_none());
    assert!(width(&rt, "room") >= 320.0);
    assert!(width(&rt, "info-panel") >= 300.0);
    let panel = rt.presented("info-panel").unwrap().0;
    assert!(panel[0] + panel[2] <= 720.0);
}

#[test]
fn closing_a_panel_in_a_narrow_window_should_restore_the_sidebar() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    rt.set_viewport((720.0, 760.0), 1.0);
    settle(&mut rt);
    press(&mut rt, "info-panel-close");
    settle(&mut rt);
    assert_eq!(width(&rt, "sidebar"), 260.0);
}

#[test]
fn widening_the_window_should_restore_the_requested_widths() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
    });
    drag(&mut rt, "sidebar-resize", 60.0);
    drag(&mut rt, "side-panel-resize", -80.0);
    rt.set_viewport((720.0, 760.0), 1.0);
    settle(&mut rt);
    rt.set_viewport((1200.0, 760.0), 1.0);
    settle(&mut rt);
    assert_eq!((width(&rt, "sidebar"), width(&rt, "info-panel")), (320.0, 400.0));
}

#[test]
fn pane_limits_should_follow_the_interface_zoom() {
    let mut rt = runtime(|s| {
        room(s);
        s.show_side(SidePanel::Info);
        s.settings.zoom = Zoom::from_percent(125);
    });
    drag(&mut rt, "side-panel-resize", -1000.0);
    assert!(width(&rt, "room") >= 320.0);
    assert!(width(&rt, "info-panel") >= 300.0);
    let panel = rt.presented("info-panel").unwrap().0;
    assert!(panel[0] + panel[2] <= 960.0);
}

#[test]
fn the_resize_handle_should_allow_keyboard_adjustment() {
    let mut rt = runtime(room);
    press(&mut rt, "sidebar-resize");
    rt.key(KeyEvent { key: Key::Right, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(width(&rt, "sidebar"), 270.0);
}

#[test]
fn losing_focus_should_restore_a_constrained_panes_preferred_width() {
    let mut rt = runtime(|s| {
        room(s);
        s.panes.set(Pane::Sidebar, 420.0);
    });
    rt.set_viewport((720.0, 760.0), 1.0);
    settle(&mut rt);
    let (x, y) = rt.find("sidebar-resize").unwrap();
    rt.window_focus(true);
    rt.pointer_down(x, y);
    rt.pointer_move(x - 40.0, y);
    rt.frame(0.0);
    rt.window_focus(false);
    rt.frame(0.0);
    rt.set_viewport((1200.0, 760.0), 1.0);
    settle(&mut rt);
    assert_eq!(width(&rt, "sidebar"), 420.0);
}

#[test]
fn clicking_a_constrained_handle_should_keep_its_preferred_width() {
    let mut rt = runtime(|s| {
        room(s);
        s.panes.set(Pane::Sidebar, 420.0);
    });
    rt.set_viewport((720.0, 760.0), 1.0);
    settle(&mut rt);
    press(&mut rt, "sidebar-resize");
    rt.set_viewport((1200.0, 760.0), 1.0);
    settle(&mut rt);
    assert_eq!(width(&rt, "sidebar"), 420.0);
}
