use super::*;

#[test]
fn zooming_in_should_step_to_the_next_level() {
    assert_eq!(Zoom::DEFAULT.larger().percent(), 110);
}

#[test]
fn zooming_out_should_step_to_the_previous_level() {
    assert_eq!(Zoom::DEFAULT.smaller().percent(), 90);
}

#[test]
fn one_hundred_percent_should_draw_at_the_design_size() {
    assert!((Zoom::DEFAULT.factor() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn zoom_should_stop_at_its_smallest_and_largest_levels() {
    assert_eq!(Zoom::from_percent(67).smaller().percent(), 67);
    assert_eq!(Zoom::from_percent(175).larger().percent(), 175);
}

#[test]
fn a_saved_zoom_should_snap_to_the_nearest_level() {
    assert_eq!(Zoom::from_percent(93).percent(), 90);
}

#[test]
fn an_appearance_should_read_back_from_its_id() {
    assert_eq!(Appearance::from_id(Appearance::Dark.id()), Appearance::Dark);
}

#[test]
fn an_unknown_appearance_should_follow_the_system() {
    assert_eq!(Appearance::from_id("sepia"), Appearance::System);
}

#[test]
fn messages_should_lay_out_in_classic_rows_by_default() {
    assert_eq!(Settings::default().message_layout, MessageLayout::Rows);
}

#[test]
fn a_message_layout_should_read_back_from_its_id() {
    assert_eq!(MessageLayout::from_id(MessageLayout::Bubbles.id()), MessageLayout::Bubbles);
}

#[test]
fn an_unknown_message_layout_should_show_classic_rows() {
    assert_eq!(MessageLayout::from_id("cards"), MessageLayout::Rows);
}
