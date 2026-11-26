use super::*;

#[test]
fn updates_should_be_checked_on_startup() {
    let now = Instant::now();
    assert!(Schedule::new(now).due(now));
}

#[test]
fn updates_should_not_be_checked_again_before_an_hour() {
    let now = Instant::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert!(!schedule.due(now + HOUR - Duration::from_secs(1)));
}

#[test]
fn updates_should_be_checked_again_after_an_hour() {
    let now = Instant::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    assert!(schedule.due(now + HOUR));
}

#[test]
fn waking_after_several_hours_should_only_check_once() {
    let now = Instant::now();
    let mut schedule = Schedule::new(now);
    schedule.due(now);
    schedule.due(now + HOUR * 8);
    assert_eq!(schedule.wait(now + HOUR * 8), HOUR);
}

#[test]
fn only_a_later_release_should_replace_a_staged_one() {
    assert!(newer("0.0.19", "0.0.18"));
    assert!(!newer("0.0.18", "0.0.18") && !newer("0.0.17", "0.0.18"));
}
