//! Where calls are possible: which standard a room's call would speak, if any.

use super::CallVia;

/// How a call would start in a room: MatrixRTC when the homeserver names a LiveKit service
/// (`rtc_service`) and the user may join the room's session (`may_join`, its power level for
/// call memberships); else the one-to-one standard when the homeserver names TURN servers
/// (`turn`: without a relay two devices behind different routers rarely get through) and the
/// room is a direct chat with exactly one other member the user may send call events to
/// (`one_to_one`); else none, and the room offers its meetings instead.
pub fn via(rtc_service: bool, turn: bool, may_join: bool, one_to_one: bool) -> Option<CallVia> {
    if rtc_service && may_join {
        Some(CallVia::Rtc)
    } else if turn && one_to_one {
        Some(CallVia::Direct)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_on_a_server_with_a_call_service_should_call_through_it() {
        assert_eq!(via(true, false, true, true), Some(CallVia::Rtc));
    }

    #[test]
    fn a_direct_chat_without_a_call_service_should_call_device_to_device() {
        assert_eq!(via(false, true, true, true), Some(CallVia::Direct));
    }

    #[test]
    fn a_group_room_without_a_call_service_should_offer_no_call() {
        assert_eq!(via(false, true, true, false), None);
    }

    #[test]
    fn a_room_that_forbids_joining_its_session_should_fall_back_or_offer_none() {
        assert_eq!(via(true, true, false, true), Some(CallVia::Direct));
        assert_eq!(via(true, true, false, false), None);
    }

    #[test]
    fn a_direct_chat_on_a_server_without_turn_or_a_call_service_should_offer_no_call() {
        assert_eq!(via(false, false, true, true), None);
        assert_eq!(via(true, false, false, true), None);
    }
}
