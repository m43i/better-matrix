use super::{Pane, PaneWidths};

#[test]
fn the_sidebar_should_leave_room_for_the_conversation() {
    let mut widths = PaneWidths::default();
    widths.set(Pane::Sidebar, 420.0);
    let layout = widths.layout(652.0, true, None);
    assert_eq!(layout.sidebar.unwrap().width, 332.0);
}

#[test]
fn a_right_panel_should_leave_room_for_the_sidebar_and_conversation() {
    let mut widths = PaneWidths::default();
    widths.set(Pane::SidePanel, 640.0);
    let layout = widths.layout(1132.0, true, Some(Pane::SidePanel));
    let sidebar = layout.sidebar.unwrap().width;
    let right = layout.right.unwrap().1.width;
    assert!(1132.0 - sidebar - right >= 320.0);
}

#[test]
fn the_sidebar_should_yield_to_an_open_panel_in_a_narrow_window() {
    let layout = PaneWidths::default().layout(652.0, true, Some(Pane::Thread));
    assert!(layout.sidebar.is_none());
    assert_eq!(layout.right.unwrap().1.width, 332.0);
}

#[test]
fn a_wider_window_should_restore_the_preferred_width() {
    let mut widths = PaneWidths::default();
    widths.set(Pane::Thread, 600.0);
    let _narrow = widths.layout(652.0, true, Some(Pane::Thread));
    let layout = widths.layout(1600.0, true, Some(Pane::Thread));
    assert_eq!(layout.right.unwrap().1.width, 600.0);
}

#[test]
fn hiding_the_sidebar_for_a_call_should_free_its_space() {
    let layout = PaneWidths::default().layout(900.0, false, Some(Pane::SidePanel));
    assert_eq!(layout.right.unwrap().1.max, 580.0);
    assert!(layout.sidebar.is_none());
}

#[test]
fn an_extremely_zoomed_panel_should_fit_the_available_width() {
    let layout = PaneWidths::default().layout(250.0, true, Some(Pane::Thread));
    assert_eq!(layout.right.unwrap().1.width, 250.0);
}
