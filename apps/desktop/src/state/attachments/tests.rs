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
        audio: None,
        sticker: false,
    }
}

#[test]
fn copying_an_image_should_fetch_the_original_with_its_own_account() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@ada:example.org");
    s.act_on_attachment(account.clone(), &image(), AttachmentAction::CopyImage, &mut rune::Tx::default());
    assert!(matches!(commands.try_next(), Some((_, Command::FetchMedia {
        account: requested, media, size: MediaSize::Original,
    })) if requested == account && Some(&media) == image().media.as_ref()));
}

#[test]
fn opening_a_file_should_download_it_before_asking_the_os_to_open_it() {
    let (mut s, mut commands) = state();
    let account = AccountId::new("@ada:example.org");
    s.act_on_attachment(account.clone(), &image(), AttachmentAction::Open, &mut rune::Tx::default());
    let (request, command) = commands.try_next().unwrap();
    let Command::SaveMedia { account: requested, media, path } = command else { panic!("download original") };
    assert_eq!(requested, account);
    assert_eq!(Some(media), image().media);
    assert_eq!(s.pending.get(&request), Some(&Purpose::OpenFile(path)));
}

#[test]
fn a_failed_image_copy_should_show_a_notice() {
    let (mut s, mut commands) = state();
    s.act_on_attachment(
        AccountId::new("@ada:example.org"),
        &image(),
        AttachmentAction::CopyImage,
        &mut rune::Tx::default(),
    );
    let (request, _) = commands.try_next().unwrap();
    s.apply(Event::Finished { request, result: Err("offline".into()) }, &mut rune::Tx::default());
    assert!(s.notice.as_deref().is_some_and(|notice| notice.contains("offline")));
}

#[test]
fn a_missing_file_should_not_start_an_attachment_action() {
    let (mut s, mut commands) = state();
    let file = Attachment { media: None, ..image() };
    for action in [AttachmentAction::CopyImage, AttachmentAction::Open, AttachmentAction::Download] {
        s.act_on_attachment(AccountId::new("@ada:example.org"), &file, action, &mut rune::Tx::default());
    }
    assert!(commands.try_next().is_none());
}

#[test]
fn opening_an_executable_attachment_should_not_launch_it_directly() {
    let (mut s, mut commands) = state();
    for name in [
        "invoice.pdf.exe",
        "SETUP.EXE ",
        "install.msi",
        "start.command",
        "script.sh",
        "link.lnk",
        "launcher.desktop",
        "program.AppImage",
        "unknown.bin",
    ] {
        let file = Attachment { name: name.into(), kind: AttachmentKind::File, ..image() };
        s.act_on_attachment(
            AccountId::new("@ada:example.org"),
            &file,
            AttachmentAction::Open,
            &mut rune::Tx::default(),
        );
        assert!(commands.try_next().is_none(), "{name} should stay available through Download only");
    }
}

#[test]
fn closing_a_preview_should_dismiss_its_context_menu() {
    let (mut s, _) = state();
    crate::demo::seed(&mut s);
    s.open_attachment(
        &matrix_core::TimelineKey::room(crate::demo::DEMO_THREAD.0),
        "$attachment",
        &image(),
        &mut rune::Tx::default(),
    );
    s.popover = Some(super::super::Popover::AttachmentMenu { at: (100, 100) });
    s.close_preview();
    assert!(s.popover.is_none());
}

#[test]
fn a_chosen_download_path_should_save_the_original_to_that_path() {
    let (s, mut commands) = state();
    let mut rt = rune::Runtime::new(s, crate::ui::view, (1200.0, 760.0), rune::Env::new(true, 1.0));
    let account = AccountId::new("@ada:example.org");
    let file = image();
    rt.update(|s, tx| s.act_on_attachment(account.clone(), &file, AttachmentAction::Download, tx));
    rt.frame(0.0);
    let (kind, options) = rt.file_dialog().unwrap();
    assert_eq!((kind, options.file_name.as_deref()), (rune::FileDialogKind::Save, Some("photo.png")));

    let path = std::env::temp_dir().join("photo.png");
    rt.resolve_file_dialog(Some(path.clone())).unwrap();
    rt.frame(0.0);

    assert!(matches!(
        commands.try_next(),
        Some((_, Command::SaveMedia { account: requested, media, path: chosen }))
            if requested == account && Some(&media) == file.media.as_ref() && chosen == path
    ));
}
