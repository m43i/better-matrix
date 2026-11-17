use super::*;
use crate::platform::Shell;
use crate::state::Settings;
use matrix_core::{DetachedCommands, EngineHandle, Event, MediaRef};

fn state() -> (State, DetachedCommands) {
    let (engine, commands) = EngineHandle::detached();
    (State::new(engine, Shell::headless(), Settings::fixed_english()), commands)
}

fn image() -> Attachment {
    Attachment {
        kind: AttachmentKind::Image,
        name: "photo.png".into(),
        size: None,
        media: Some(MediaRef::opaque("original")),
        thumbnail: Some(MediaRef::opaque("thumbnail")),
        dimensions: None,
    }
}

#[test]
fn copying_an_image_should_fetch_the_original_with_its_own_account() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@ada:example.org");
    s.act_on_attachment(account.clone(), &image(), AttachmentAction::CopyImage);
    assert!(matches!(commands.try_next(), Some((_, Command::FetchMedia {
        account: requested, media, size: MediaSize::Original,
    })) if requested == account && Some(&media) == image().media.as_ref()));
}

#[test]
fn opening_a_file_should_download_it_before_asking_the_os_to_open_it() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@ada:example.org");
    s.act_on_attachment(account.clone(), &image(), AttachmentAction::Open);
    let (request, command) = commands.try_next().unwrap();
    let Command::SaveMedia { account: requested, media, path } = command else { panic!("download original") };
    assert_eq!(requested, account);
    assert_eq!(Some(media), image().media);
    assert_eq!(s.pending.get(&request), Some(&Purpose::OpenFile(path)));
}

#[test]
fn a_failed_image_copy_should_show_a_notice() {
    let (mut s, mut commands) = state();
    s.act_on_attachment(AccountId::new("@ada:example.org"), &image(), AttachmentAction::CopyImage);
    let (request, _) = commands.try_next().unwrap();
    s.apply(Event::Finished { request, result: Err("offline".into()) }, &mut rune::Tx::default());
    assert!(s.notice.as_deref().is_some_and(|notice| notice.contains("offline")));
}

#[test]
fn a_missing_file_should_not_start_an_attachment_action() {
    let (mut s, mut commands) = state();
    let file = Attachment { media: None, ..image() };
    for action in [AttachmentAction::CopyImage, AttachmentAction::Open, AttachmentAction::Download] {
        s.act_on_attachment(AccountId::new("@ada:example.org"), &file, action);
    }
    assert!(commands.try_next().is_none());
}

#[test]
fn closing_a_preview_should_dismiss_its_context_menu() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    s.open_attachment(&image());
    s.popover = Some(super::super::Popover::AttachmentMenu { at: (100, 100) });
    s.close_preview();
    assert!(s.popover.is_none());
}
