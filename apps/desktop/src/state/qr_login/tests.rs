use matrix_core::{QrLoginStep, RequestId};
use rune::TextEdit;

use super::QrLogin;

fn asking() -> QrLogin {
    let mut flow = QrLogin::started(RequestId(1));
    flow.moved(RequestId(1), QrLoginStep::EnterCheckCode);
    flow
}

#[test]
fn a_step_for_another_request_should_be_ignored() {
    let mut flow = QrLogin::started(RequestId(1));
    assert!(!flow.moved(RequestId(2), QrLoginStep::Syncing));
}

#[test]
fn being_asked_for_the_check_code_should_focus_an_empty_field() {
    let flow = asking();
    assert!(flow.code.focused && flow.code.text.is_empty());
}

#[test]
fn two_digits_should_answer_as_the_check_code() {
    let mut flow = asking();
    flow.code = TextEdit::new("07");
    assert_eq!(flow.answer(), Some((RequestId(1), 7)));
}

#[test]
fn anything_but_two_digits_should_not_answer() {
    for typed in ["", "7", "123", "a1", "-1"] {
        let mut flow = asking();
        flow.code = TextEdit::new(typed);
        assert_eq!(flow.answer(), None, "{typed:?}");
    }
}

#[test]
fn the_check_code_should_answer_once_per_question() {
    let mut flow = asking();
    flow.code = TextEdit::new("42");
    flow.answer();
    assert_eq!(flow.answer(), None);
}

#[test]
fn a_failure_should_end_the_sign_in_with_its_error() {
    let mut flow = asking();
    flow.fail(RequestId(1), "expired".into());
    assert!(flow.ended() && flow.error.as_deref() == Some("expired"));
}
