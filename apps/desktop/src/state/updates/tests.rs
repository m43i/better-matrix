use super::*;

#[test]
fn a_downloaded_update_should_wait_for_a_restart() {
    let mut state = State::detached();
    state.update_ready(crate::demo::update());
    assert!(matches!(state.update, Update::Ready(_)));
}

#[test]
fn an_update_that_could_not_be_staged_again_should_offer_its_page() {
    let mut state = State::detached();
    state.update_ready(crate::demo::update());
    state.update_found(crate::demo::page_update());
    assert!(matches!(state.update, Update::Available(_)));
}

#[test]
fn clicking_restart_should_start_installation() {
    let mut state = State::detached();
    state.update_ready(crate::demo::update());
    state.install_update();
    assert!(matches!(state.update, Update::Installing(_)));
}

#[test]
fn a_failed_restart_should_wait_for_the_update_to_be_staged_again() {
    let mut state = State::detached();
    state.update_ready(crate::demo::update());
    state.install_update();
    state.update_failed("offline".into());
    assert!(matches!(state.update, Update::Current));
    state.update_ready(crate::demo::update());
    assert!(matches!(state.update, Update::Ready(_)));
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
    state.update_ready(crate::demo::update());
    state.install_update();
    state.update_ready(crate::demo::update());
    assert!(matches!(state.update, Update::Installing(_)));
}

#[test]
fn repeated_clicks_should_send_only_one_install_request() {
    let mut shell = crate::platform::Shell::headless();
    let (sender, receiver) = std::sync::mpsc::channel();
    shell.record_updates(sender);
    let mut state = State::new(matrix_core::EngineHandle::detached().0, shell, crate::state::Settings::fixed_english());
    state.update_ready(crate::demo::update());
    state.install_update();
    state.install_update();
    assert_eq!(receiver.try_iter().count(), 1);
}

#[test]
fn a_download_should_not_restart_without_the_user() {
    let mut shell = crate::platform::Shell::headless();
    let (sender, receiver) = std::sync::mpsc::channel();
    shell.record_updates(sender);
    let mut state = State::new(matrix_core::EngineHandle::detached().0, shell, crate::state::Settings::fixed_english());
    state.update_ready(crate::demo::update());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn an_interrupted_update_should_show_a_localized_notice() {
    let mut state = State::detached();
    state.update_interrupted();
    assert_eq!(state.notice.as_deref(), Some(crate::i18n::t!("update.incomplete")));
}

#[test]
fn clicking_a_package_update_should_show_its_command_instead_of_installing() {
    let mut state = State::detached();
    state.update_found(crate::demo::package_update());
    state.install_update();
    assert!(matches!(state.update, Update::Available(_)));
    assert_eq!(state.popover, Some(Popover::Update));
}

#[test]
fn an_update_outside_the_app_should_never_send_an_install_request() {
    let mut shell = crate::platform::Shell::headless();
    let (sender, receiver) = std::sync::mpsc::channel();
    shell.record_updates(sender);
    let mut state = State::new(matrix_core::EngineHandle::detached().0, shell, crate::state::Settings::fixed_english());
    state.update_found(crate::demo::page_update());
    state.install_update();
    assert!(receiver.try_recv().is_err());
}

#[test]
fn clicking_the_update_again_should_close_its_panel() {
    let mut state = State::detached();
    state.update_found(crate::demo::package_update());
    state.install_update();
    state.install_update();
    assert_eq!(state.popover, None);
}
