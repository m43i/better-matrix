use super::*;
use matrix_core::AccountId;

#[test]
fn the_last_used_account_should_survive_saving_and_loading_settings() {
    let settings = Settings { last_account: Some(AccountId::new("@ada:example.org")), ..Settings::fixed_english() };
    let mut prefs = Prefs::memory();
    write_settings(&mut prefs, &settings).unwrap();

    assert_eq!(load_settings(&prefs), settings);
}

#[test]
fn older_settings_should_have_no_last_used_account() {
    let mut prefs = Prefs::memory();
    prefs.set(LANGUAGE, "en").unwrap();

    assert_eq!(load_settings(&prefs).last_account, None);
}

#[test]
fn older_settings_should_close_to_the_tray_by_default() {
    assert!(load_settings(&Prefs::memory()).close_to_tray);
}

#[test]
fn quitting_during_a_settings_save_should_preserve_the_latest_close_preference() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prefs.json");
    let mut shell = Shell::headless();
    shell.prefs = Arc::new(Mutex::new(Prefs::at(&file)));
    let prefs = Arc::clone(&shell.prefs);
    let (held, waiting) = std::sync::mpsc::channel();
    let other_writer = std::thread::spawn(move || {
        let _guard = prefs.lock().unwrap();
        held.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
    });
    waiting.recv().unwrap();
    shell.save_settings(Settings { close_to_tray: false, ..Settings::default() });
    drop(shell);

    assert!(!load_settings(&Prefs::at(file)).close_to_tray);
    other_writer.join().unwrap();
}

#[test]
fn clearing_the_last_used_account_should_survive_saving_and_loading_settings() {
    let mut prefs = Prefs::memory();
    prefs.set(LAST_ACCOUNT, "@ada:example.org").unwrap();
    write_settings(&mut prefs, &Settings::fixed_english()).unwrap();

    assert_eq!(load_settings(&prefs).last_account, None);
}

#[test]
fn the_emoticons_choice_should_survive_saving_and_loading_settings() {
    let settings = Settings { emoticons: Emoticons::AsTyped, ..Settings::fixed_english() };
    let mut prefs = Prefs::memory();
    write_settings(&mut prefs, &settings).unwrap();

    assert_eq!(load_settings(&prefs).emoticons, Emoticons::AsTyped);
}

#[test]
fn older_settings_should_turn_emoticons_into_emoji() {
    let mut prefs = Prefs::memory();
    prefs.set(LANGUAGE, "en").unwrap();

    assert_eq!(load_settings(&prefs).emoticons, Emoticons::AsEmoji);
}

#[test]
fn the_message_layout_should_survive_saving_and_loading_settings() {
    let settings = Settings { message_layout: MessageLayout::Bubbles, ..Settings::fixed_english() };
    let mut prefs = Prefs::memory();
    write_settings(&mut prefs, &settings).unwrap();

    assert_eq!(load_settings(&prefs).message_layout, MessageLayout::Bubbles);
}

#[test]
fn the_rooms_opened_should_read_back_as_saved() {
    let mut visits = Visits::default();
    visits.visit(&AccountId::new("@ada:example.org"), "!room:example.org", false);
    let shell = Shell::headless();
    shell.prefs.lock().unwrap().set(VISITS, &visits).unwrap();

    assert_eq!(shell.visits(), visits);
}

/// Waits up to two seconds for `secrets` to hold `expected` as the GIPHY key.
fn wait_for_key(secrets: &dyn matrix_core::SecretStore, expected: Option<&str>) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let kept = secrets.get(GIPHY_SECRET).unwrap();
        if kept.as_deref() == expected || std::time::Instant::now() > deadline {
            return kept;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_gif_key_should_be_kept_in_the_secret_store_and_removed_from_it() {
    let secrets: Arc<dyn matrix_core::SecretStore> = Arc::new(matrix_core::MemorySecrets::default());
    let shell = Shell::headless().with_secrets(Arc::clone(&secrets));
    shell.save_gif_key(Some("first".into()), None);
    assert_eq!(wait_for_key(secrets.as_ref(), Some("first")).as_deref(), Some("first"));
    shell.save_gif_key(Some("second".into()), Some("first".into()));
    assert_eq!(wait_for_key(secrets.as_ref(), Some("second")).as_deref(), Some("second"));
    shell.save_gif_key(None, Some("second".into()));
    assert_eq!(wait_for_key(secrets.as_ref(), None), None);
}

#[test]
fn a_gif_key_should_stay_out_of_the_saved_settings() {
    let mut prefs = Prefs::memory();
    write_settings(&mut prefs, &Settings::fixed_english()).unwrap();
    assert!(prefs.get::<String>("giphy_key").is_none());
}

/// Keeps a key like the GIPHY key in this machine's real keychain (the Secret Service, Keychain
/// or the Credential Manager) and takes it out again, under a test name: the user's own key
/// stays untouched.
#[test]
#[ignore = "touches the system keychain"]
fn a_key_should_round_trip_through_the_system_keychain() {
    let secrets = system_keychain();
    let name = format!("{GIPHY_SECRET}-test");
    secrets.set(&name, "keychain-test").unwrap();
    assert_eq!(secrets.get(&name).unwrap().as_deref(), Some("keychain-test"));
    secrets.delete(&name).unwrap();
    assert_eq!(secrets.get(&name).unwrap(), None);
}
