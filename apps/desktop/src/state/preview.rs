//! The attachment preview: an image at full size, or a PDF's pages, over a dimmed window.

use matrix_core::{AccountId, Attachment, AttachmentKind, MediaRef};
use rune::{Image, Pixels};

/// What a preview shows.
#[derive(Debug)]
pub enum PreviewContent {
    /// The original picture, with the timeline's thumbnail while it loads.
    Image { thumbnail: Option<Image>, status: ImageStatus },
    /// A PDF's pages as they are rendered, each with its size in pixels.
    Document { pages: Vec<(Pixels, (u32, u32))>, status: DocumentStatus },
}

/// Download and decode are separate steps; either finishing must refresh the viewer.
#[derive(Debug)]
pub enum ImageStatus {
    Downloading,
    Decoding(Image),
    Ready(Image),
    Failed(String),
}

impl PreviewContent {
    /// Promotes a completed decode once, so the setup waker rebuilds the viewer and then rests.
    pub fn poll(&mut self) -> bool {
        let Self::Image { thumbnail, status } = self else { return false };
        let ImageStatus::Decoding(image) = status else { return false };
        if !image.is_ready() {
            return false;
        }
        *status = match image.error() {
            Some(error) => ImageStatus::Failed(error.to_string()),
            None => {
                thumbnail.take();
                ImageStatus::Ready(image.clone())
            }
        };
        true
    }
}

/// How far a PDF has got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    Downloading,
    Rendering,
    Done,
    Failed(String),
}

/// An open preview.
#[derive(Debug)]
pub struct Preview {
    pub account: AccountId,
    pub attachment: Attachment,
    /// The file as uploaded.
    pub media: MediaRef,
    pub content: PreviewContent,
}

/// What clicking an attachment does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    Image,
    Document,
    /// Anything else: there is nothing to show, so it saves.
    Download,
}

impl Opens {
    pub fn of(attachment: &Attachment) -> Self {
        if attachment.kind == AttachmentKind::Image {
            Self::Image
        } else if attachment.name.to_ascii_lowercase().ends_with(".pdf") {
            Self::Document
        } else {
            Self::Download
        }
    }
}

/// Renders a PDF's pages, calling `page` with each as it is ready: at most `limit` pages, and
/// stopping early when the pages so far fill the pixel budget or `keep_going` says no (the
/// preview closed).
pub fn render_document(
    bytes: &[u8],
    limit: usize,
    keep_going: impl Fn() -> bool,
    mut page: impl FnMut(Pixels, (u32, u32)),
) -> Result<(), String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::vello_cpu::peniko::ImageAlphaType;
    use hayro::{PixmapSettings, RenderCache, RenderSettings};

    /// Pages draw at twice their point size: sharp on a 2x display at their natural width.
    const SCALE: f32 = 2.0;
    /// The most pixels one page draws with (an A4 page at 2x has 2 million): a poster-sized
    /// page draws smaller instead.
    const PAGE_PIXELS: f32 = 8_000_000.0;
    /// The most pixels all pages hold together, at four bytes each: 160 MiB.
    const TOTAL_PIXELS: u64 = 40_000_000;
    let pdf = Pdf::new(bytes.to_vec()).map_err(|error| format!("{error:?}"))?;
    let cache = RenderCache::new();
    let (interpreter, render) = (InterpreterSettings::default(), RenderSettings::default());
    let mut total = 0u64;
    for source in pdf.pages().iter().take(limit) {
        if !keep_going() || total >= TOTAL_PIXELS {
            break;
        }
        let (width, height) = source.render_dimensions();
        let scale = SCALE.min((PAGE_PIXELS / (width * height).max(1.0)).sqrt());
        let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
        let rendered = hayro::render(source, &cache, &interpreter, &render, &pixmap);
        let size = (u32::from(rendered.width()), u32::from(rendered.height()));
        total += u64::from(size.0) * u64::from(size.1);
        let pixels = Pixels::new();
        pixels.set(size.0, size.1, rendered.take_rgba8(ImageAlphaType::Alpha));
        page(pixels, size);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, kind: AttachmentKind) -> Attachment {
        Attachment { kind, name: name.into(), size: None, media: None, thumbnail: None, dimensions: None }
    }

    #[test]
    fn images_and_pdfs_should_preview_and_other_files_download() {
        assert_eq!(Opens::of(&file("a.png", AttachmentKind::Image)), Opens::Image);
        assert_eq!(Opens::of(&file("a.pdf", AttachmentKind::File)), Opens::Document);
        assert_eq!(Opens::of(&file("a.zip", AttachmentKind::File)), Opens::Download);
    }

    #[test]
    fn a_file_that_is_no_pdf_should_fail_to_render() {
        assert!(render_document(b"not a pdf", 1, || true, |_, _| {}).is_err());
    }

    #[test]
    fn an_image_that_cannot_decode_should_stop_loading_and_report_its_error() {
        let decoder = rune::ImageCache::new(std::sync::Arc::new(|| {}));
        let image = decoder.load(b"not an image".as_slice());
        assert!(image.wait().is_err());
        let mut content = PreviewContent::Image { thumbnail: None, status: ImageStatus::Decoding(image) };
        assert!(content.poll());
        assert!(matches!(content, PreviewContent::Image { status: ImageStatus::Failed(_), .. }));
        assert!(!content.poll());
    }
}
