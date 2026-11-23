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
fn the_emoji_style_should_survive_saving_and_loading_settings() {
    let settings = Settings { emoji_style: EmojiStyle::Monochrome, ..Settings::fixed_english() };
    let mut prefs = Prefs::memory();
    write_settings(&mut prefs, &settings).unwrap();

    assert_eq!(load_settings(&prefs).emoji_style, EmojiStyle::Monochrome);
}

#[test]
fn older_settings_should_use_color_emoji() {
    assert_eq!(load_settings(&Prefs::memory()).emoji_style, EmojiStyle::Color);
}

#[test]
fn the_message_layout_should_survive_saving_and_loading_settings() {
    let settings = Settings { message_layout: MessageLayout::Bubbles, ..Settings::fixed_english() };
    let mut prefs = Prefs::memory();
    write_settings(&mut prefs, &settings).unwrap();

    assert_eq!(load_settings(&prefs).message_layout, MessageLayout::Bubbles);
}
