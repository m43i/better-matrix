//! The attachment preview: an image at full size, a PDF's pages, a video or an audio clip over
//! a dimmed window, with the rest of its batch to step through.

use matrix_core::{AccountId, Attachment, AttachmentKind, MediaRef, Message, MessageContent};
use rune::{Image, Pixels};

use super::accounts::{Entry, EntryView, continues_group};
use crate::platform::video;

/// What a preview shows.
#[derive(Debug)]
pub enum PreviewContent {
    /// The original picture, with the timeline's thumbnail while it loads.
    Image { thumbnail: Option<Image>, status: ImageStatus },
    /// A PDF's pages as they are rendered.
    Document { pages: Pages, status: DocumentStatus },
    /// A video, with the timeline's thumbnail as its poster while the file downloads.
    Video { thumbnail: Option<Image>, status: VideoStatus },
    /// An audio clip: it plays as the timeline's cards do, through `State::audio`.
    Audio,
}

/// A PDF's pages, each with its size in pixels.
pub type Pages = Vec<(Pixels, (u32, u32))>;

/// A video downloads to the scratch folder whole, then plays from there.
#[derive(Debug)]
pub enum VideoStatus {
    Downloading,
    Open(video::Player),
    Failed(String),
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
    /// Its pages come from the render this numbers ([`crate::platform::Shell::render_document`]).
    Rendering(u64),
    Done,
    Failed(String),
}

/// An open preview.
#[derive(Debug)]
pub struct Preview {
    pub account: AccountId,
    /// The attachment shown: `batch[index]`'s.
    pub attachment: Attachment,
    /// The file as uploaded.
    pub media: MediaRef,
    pub content: PreviewContent,
    /// The attachment batch the opened one belongs to, the previewable ones in timeline order:
    /// the strip under the file and the arrow keys step through it. Just the opened one when
    /// it stands alone.
    pub batch: Vec<PreviewItem>,
    pub index: usize,
}

/// One attachment of a preview's batch.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewItem {
    pub event_id: String,
    pub attachment: Attachment,
}

/// What clicking an attachment does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    Image,
    Document,
    Video,
    Audio,
    /// Anything else: there is nothing to show, so it saves.
    Download,
}

impl Opens {
    pub fn of(attachment: &Attachment) -> Self {
        match attachment.kind {
            AttachmentKind::Image => Self::Image,
            AttachmentKind::Video => Self::Video,
            AttachmentKind::Audio => Self::Audio,
            AttachmentKind::File if attachment.name.to_ascii_lowercase().ends_with(".pdf") => Self::Document,
            AttachmentKind::File => Self::Download,
        }
    }

    /// Whether the preview shows it, rather than saving it.
    pub fn previews(attachment: &Attachment) -> bool {
        attachment.media.is_some() && Self::of(attachment) != Self::Download
    }
}

/// The attachment batch of `event_id` in `entries`: the run of attachments its sender posted
/// together around it (messages the timeline groups under one header, with nothing else
/// between them; entries it doesn't draw don't break the run), the previewable ones in order.
/// Empty when `event_id` isn't a previewable attachment there.
pub fn batch(entries: &[Entry], event_id: &str) -> Vec<PreviewItem> {
    let Some(at) = entries.iter().position(|entry| entry.message().is_some_and(|m| m.event_id == event_id)) else {
        return Vec::new();
    };
    let opened = entries[at].message().filter(|m| matches!(m.content, MessageContent::Attachment(_)));
    let Some(opened) = opened else { return Vec::new() };
    let mut before = Vec::new();
    let mut later = opened;
    for entry in entries[..at].iter().rev() {
        match attachment(entry) {
            None => continue,
            Some(Ok(message)) if continues_group(message, later) => {
                before.push(message);
                later = message;
            }
            Some(_) => break,
        }
    }
    let mut after = Vec::new();
    let mut earlier = opened;
    for entry in &entries[at + 1..] {
        match attachment(entry) {
            None => continue,
            Some(Ok(message)) if continues_group(earlier, message) => {
                after.push(message);
                earlier = message;
            }
            Some(_) => break,
        }
    }
    before.into_iter().rev().chain(std::iter::once(opened)).chain(after).filter_map(item).collect()
}

/// What an entry is to a batch: nothing (the timeline doesn't draw it), an attachment message
/// that may join, or something that ends the run.
fn attachment(entry: &Entry) -> Option<Result<&Message, ()>> {
    match &entry.kind {
        EntryView::Message(item) => match &item.message.content {
            MessageContent::Attachment(_) => Some(Ok(&item.message)),
            _ => Some(Err(())),
        },
        EntryView::ReadMarker | EntryView::Other => None,
        _ => Some(Err(())),
    }
}

/// A message's attachment as a batch item, when the preview shows it.
fn item(message: &Message) -> Option<PreviewItem> {
    match &message.content {
        MessageContent::Attachment(attachment) if Opens::previews(attachment) => {
            Some(PreviewItem { event_id: message.event_id.clone(), attachment: attachment.clone() })
        }
        _ => None,
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
        Attachment { kind, name: name.into(), size: None, media: None, thumbnail: None, dimensions: None, audio: None }
    }

    #[test]
    fn images_pdfs_videos_and_audio_should_preview_and_other_files_download() {
        assert_eq!(Opens::of(&file("a.png", AttachmentKind::Image)), Opens::Image);
        assert_eq!(Opens::of(&file("a.pdf", AttachmentKind::File)), Opens::Document);
        assert_eq!(Opens::of(&file("a.mp4", AttachmentKind::Video)), Opens::Video);
        assert_eq!(Opens::of(&file("a.ogg", AttachmentKind::Audio)), Opens::Audio);
        assert_eq!(Opens::of(&file("a.zip", AttachmentKind::File)), Opens::Download);
    }

    /// A message from `sender` at `minute`, carrying `content`.
    fn message(id: &str, sender: &str, minute: u64, content: MessageContent) -> Entry {
        Entry::from(matrix_core::TimelineEntry {
            id: id.into(),
            kind: matrix_core::EntryKind::Message(Box::new(Message {
                event_id: format!("${id}"),
                sender_id: sender.into(),
                timestamp_ms: minute * 60_000,
                content,
                ..Message::default()
            })),
        })
    }

    /// An image `id` from `sender` at `minute`.
    fn picture(id: &str, sender: &str, minute: u64) -> Entry {
        let mut attachment = file(&format!("{id}.png"), AttachmentKind::Image);
        attachment.media = Some(MediaRef::opaque(id));
        message(id, sender, minute, MessageContent::Attachment(attachment))
    }

    fn text(id: &str, sender: &str, minute: u64) -> Entry {
        message(id, sender, minute, MessageContent::Text { body: "hi".into(), html: None })
    }

    fn ids(batch: &[PreviewItem]) -> Vec<&str> {
        batch.iter().map(|item| item.event_id.as_str()).collect()
    }

    #[test]
    fn a_senders_attachments_posted_together_should_form_one_batch() {
        let entries = [picture("a", "@g", 0), picture("b", "@g", 1), picture("c", "@g", 1)];
        assert_eq!(ids(&batch(&entries, "$b")), ["$a", "$b", "$c"]);
    }

    #[test]
    fn a_text_message_should_end_a_batch() {
        let entries = [picture("a", "@g", 0), text("t", "@g", 0), picture("b", "@g", 1), picture("c", "@g", 1)];
        assert_eq!(ids(&batch(&entries, "$c")), ["$b", "$c"]);
    }

    #[test]
    fn another_senders_attachment_should_end_a_batch() {
        let entries = [picture("a", "@g", 0), picture("b", "@h", 0), picture("c", "@g", 0)];
        assert_eq!(ids(&batch(&entries, "$a")), ["$a"]);
    }

    #[test]
    fn an_attachment_posted_long_after_should_start_its_own_batch() {
        let entries = [picture("a", "@g", 0), picture("b", "@g", 6)];
        assert_eq!(ids(&batch(&entries, "$b")), ["$b"]);
    }

    #[test]
    fn a_reply_should_start_its_own_batch() {
        let mut reply = picture("b", "@g", 1);
        if let EntryView::Message(item) = &mut reply.kind {
            item.message.reply_to =
                Some(matrix_core::Quote { event_id: "$x".into(), sender_name: None, body: None, call: None });
        }
        let entries = [picture("a", "@g", 0), reply];
        assert_eq!(ids(&batch(&entries, "$a")), ["$a"]);
    }

    #[test]
    fn entries_the_timeline_doesnt_draw_should_not_break_a_batch() {
        let marker =
            Entry::from(matrix_core::TimelineEntry { id: "m".into(), kind: matrix_core::EntryKind::ReadMarker });
        let entries = [picture("a", "@g", 0), marker, picture("b", "@g", 1)];
        assert_eq!(ids(&batch(&entries, "$a")), ["$a", "$b"]);
    }

    #[test]
    fn files_the_preview_cant_show_should_be_left_out_of_a_batch_without_ending_it() {
        let mut zip = file("notes.zip", AttachmentKind::File);
        zip.media = Some(MediaRef::opaque("zip"));
        let entries =
            [picture("a", "@g", 0), message("z", "@g", 0, MessageContent::Attachment(zip)), picture("b", "@g", 1)];
        assert_eq!(ids(&batch(&entries, "$a")), ["$a", "$b"]);
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
