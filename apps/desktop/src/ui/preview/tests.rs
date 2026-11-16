use std::sync::Arc;

use matrix_core::{AccountId, Attachment, AttachmentKind, MediaRef};
use rune::{Env, Runtime, el};

use super::*;

#[test]
fn the_cached_thumbnail_should_stay_in_place_when_the_original_arrives() {
    let decoder = rune::ImageCache::new(Arc::new(|| {}));
    let image = decoder.load(include_bytes!("../../../../../packaging/icons/better-matrix-256.png").as_slice());
    image.wait().unwrap();
    let file = Preview {
        account: AccountId::new("@a:example.org"),
        attachment: Attachment {
            kind: AttachmentKind::Image,
            name: "picture.png".into(),
            size: None,
            media: None,
            thumbnail: None,
            dimensions: None,
        },
        media: MediaRef::opaque("picture"),
        content: PreviewContent::Image { thumbnail: Some(image.clone()), status: ImageStatus::Downloading },
    };
    let mut rt = Runtime::new(
        file,
        |file, _| el().w_full().h_full().child(preview(file, [1200.0, 760.0])),
        (1200.0, 760.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    let thumbnail = rt.presented("preview-image").expect("the thumbnail shows before the original arrives").0;
    rt.update(|file, _| {
        file.content = PreviewContent::Image { thumbnail: None, status: ImageStatus::Ready(image) };
    });
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.presented("preview-image").unwrap().0, thumbnail);
}
