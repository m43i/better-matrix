use super::*;
use crate::Message;

fn message(id: &str, time: u64) -> TimelineEntry {
    TimelineEntry {
        id: id.into(),
        kind: EntryKind::Message(Box::new(Message {
            event_id: format!("${time}"),
            timestamp_ms: time,
            ..Default::default()
        })),
    }
}

#[test]
fn first_replies_should_show_before_the_sdk_has_loaded_them() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    let diff = bridge.show(vec![message("early", 1)]);
    assert_eq!(diff, Some(TimelineDiff::Reset(vec![message("root", 0), message("early", 1)])));
}

#[test]
fn live_replies_should_merge_with_the_first_page_without_duplicates() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    bridge.show(vec![message("early", 1)]);
    let diffs = bridge.update(vec![TimelineDiff::PushBack(message("new", 2))]);
    assert_eq!(diffs, [TimelineDiff::Reset(vec![message("root", 0), message("early", 1), message("new", 2)])]);
}

#[test]
fn sdk_updates_should_keep_a_first_replys_row_key() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    bridge.show(vec![message("early", 1)]);
    bridge.update(vec![TimelineDiff::PushBack(message("sdk", 1))]);
    let diff = bridge.update(vec![TimelineDiff::Set { index: 1, entry: message("sdk", 1) }]);
    assert_eq!(diff, [TimelineDiff::Set { index: 1, entry: message("early", 1) }]);
}

#[test]
fn an_sdk_page_with_fewer_thread_replies_should_keep_the_remaining_first_replies() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    bridge.show(vec![message("early", 1), message("early-2", 2)]);
    let diff = bridge.update(vec![TimelineDiff::PushBack(message("sdk-2", 2))]);
    assert_eq!(diff, [TimelineDiff::Reset(vec![message("root", 0), message("early", 1), message("early-2", 2)])]);
}

#[test]
fn first_replies_should_keep_server_order_despite_clock_skew() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 10)]), "$10");
    let diff = bridge.show(vec![message("first", 30), message("second", 2)]);
    assert_eq!(diff, Some(TimelineDiff::Reset(vec![message("root", 10), message("first", 30), message("second", 2)])));
}

#[test]
fn removing_an_already_observed_reply_should_not_restore_its_first_page_copy() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    bridge.show(vec![message("early", 1), message("early-2", 2)]);
    bridge.update(vec![TimelineDiff::PushBack(message("sdk-2", 2))]);
    let diff = bridge.update(vec![TimelineDiff::Remove { index: 1 }]);
    assert_eq!(diff, [TimelineDiff::Reset(vec![message("root", 0), message("early", 1)])]);
}

#[test]
fn a_reply_added_and_removed_in_one_batch_should_not_reappear_from_the_first_page() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    bridge.show(vec![message("early", 1)]);
    let diff = bridge.update(vec![TimelineDiff::PushBack(message("sdk", 1)), TimelineDiff::Remove { index: 1 }]);
    assert_eq!(diff, [TimelineDiff::Reset(vec![message("root", 0)])]);
}

#[test]
fn a_reply_removed_before_the_direct_response_should_not_reappear_when_it_arrives() {
    let mut bridge = Bootstrap::new(&TimelineDiff::Reset(vec![message("root", 0)]), "$0");
    bridge.update(vec![TimelineDiff::PushBack(message("sdk", 1))]);
    bridge.update(vec![TimelineDiff::Remove { index: 1 }]);
    let diff = bridge.show(vec![message("early", 1)]);
    assert_eq!(diff, Some(TimelineDiff::Reset(vec![message("root", 0)])));
}
