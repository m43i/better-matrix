use super::*;

#[test]
fn an_image_copy_should_preserve_the_original_dimensions() {
    let png = include_bytes!("../../../../../packaging/icons/better-matrix-256.png");
    assert_eq!(clipboard_image(png).unwrap().size(), (256, 256));
}

#[test]
fn a_jpeg_should_be_converted_to_a_clipboard_png() {
    let original = image::DynamicImage::new_rgb8(3, 2);
    let mut jpeg = Cursor::new(Vec::new());
    original.write_to(&mut jpeg, image::ImageFormat::Jpeg).unwrap();
    let copied = clipboard_image(jpeg.get_ref()).unwrap();
    assert_eq!(copied.size(), (3, 2));
    assert!(image::load_from_memory(copied.png()).is_ok());
}

#[test]
fn invalid_image_bytes_should_fail_to_copy() {
    assert!(clipboard_image(b"not an image").is_err());
}

#[test]
fn opening_attachments_should_keep_their_paths_inside_scratch() {
    let shell = Shell::headless();
    for name in ["../../photo.png", "C:\\users\\photo.png", "/photo.png"] {
        let path = shell.attachment_path(name);
        assert_eq!(path.parent(), Some(shell.scratch()));
        assert!(path.file_name().unwrap().to_str().unwrap().ends_with("-photo.png"));
    }
}

#[test]
fn files_with_the_same_name_should_have_separate_open_paths() {
    let shell = Shell::headless();
    assert_ne!(shell.attachment_path("photo.png"), shell.attachment_path("photo.png"));
}

#[test]
fn attachment_open_paths_should_replace_characters_windows_cannot_save() {
    let path = Shell::headless().attachment_path("Screenshot 12:30?.png ");
    assert!(path.file_name().unwrap().to_str().unwrap().ends_with("-Screenshot 12_30_.png"));
}
