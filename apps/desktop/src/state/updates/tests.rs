use super::*;

#[test]
fn discovering_an_update_should_only_offer_it() {
    let mut state = State::detached();
    state.update_found(crate::demo::update());
    assert!(matches!(state.update, Update::Available(_)));
}

#[test]
fn clicking_update_should_start_installation() {
    let mut state = State::detached();
    state.update_found(crate::demo::update());
    state.install_update();
    assert!(matches!(state.update, Update::Installing(_)));
}

#[test]
fn a_failed_update_should_offer_a_retry() {
    let mut state = State::detached();
    state.update_found(crate::demo::update());
    state.install_update();
    state.update_failed("offline".into());
    assert!(matches!(state.update, Update::Available(_)));
}

#[test]
fn a_failed_update_should_explain_the_failure() {
    let mut state = State::detached();
    state.update_failed("offline".into());
    assert!(state.notice.as_deref().is_some_and(|notice| notice.contains("offline")));
}

#[test]
fn a_check_should_not_interrupt_installation() {
    let mut state = State::detached();
    state.update_found(crate::demo::update());
    state.install_update();
    state.update_found(crate::demo::update());
    assert!(matches!(state.update, Update::Installing(_)));
}

#[test]
fn repeated_clicks_should_send_only_one_install_request() {
    let mut shell = crate::platform::Shell::headless();
    let (sender, receiver) = std::sync::mpsc::channel();
    shell.record_updates(sender);
    let mut state = State::new(matrix_core::EngineHandle::detached().0, shell, crate::state::Settings::fixed_english());
    state.update_found(crate::demo::update());
    state.install_update();
    state.install_update();
    assert_eq!(receiver.try_iter().count(), 1);
}

#[test]
fn discovery_should_not_send_an_install_request() {
    let mut shell = crate::platform::Shell::headless();
    let (sender, receiver) = std::sync::mpsc::channel();
    shell.record_updates(sender);
    let mut state = State::new(matrix_core::EngineHandle::detached().0, shell, crate::state::Settings::fixed_english());
    state.update_found(crate::demo::update());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn an_interrupted_update_should_show_a_localized_notice() {
    let mut state = State::detached();
    state.update_interrupted();
    assert_eq!(state.notice.as_deref(), Some(crate::i18n::t!("update.incomplete")));
}
