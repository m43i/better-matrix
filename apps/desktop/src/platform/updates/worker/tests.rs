use super::*;

#[test]
fn updates_should_be_checked_on_startup() {
    let now = SystemTime::now();
    assert!(Schedule::new(now).due(now));
}

#[test]
fn updates_should_not_be_checked_again_before_an_hour() {
    let now = SystemTime::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert!(!schedule.due(now + HOUR - Duration::from_secs(1)));
}

#[test]
fn updates_should_be_checked_again_after_an_hour() {
    let now = SystemTime::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert!(schedule.due(now + HOUR));
}

#[test]
fn waking_after_several_hours_should_only_check_once() {
    let now = SystemTime::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert!(schedule.due(now + HOUR * 8));
    assert!(!schedule.due(now + HOUR * 8));
}

#[test]
fn the_worker_should_look_at_the_clock_again_within_a_few_minutes() {
    let now = SystemTime::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert_eq!(schedule.wait(now), GLANCE);
    assert_eq!(schedule.wait(now + HOUR - Duration::from_secs(1)), Duration::from_secs(1));
}

#[test]
fn a_clock_set_back_should_not_hold_off_checks() {
    let now = SystemTime::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert!(schedule.due(now - HOUR * 24));
}

#[test]
fn only_a_later_release_should_replace_a_staged_one() {
    assert!(newer("0.0.19", "0.0.18"));
    assert!(!newer("0.0.18", "0.0.18") && !newer("0.0.17", "0.0.18"));
}
