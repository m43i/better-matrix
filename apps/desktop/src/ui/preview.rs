//! The attachment preview: the image or PDF full size over a dark, see-through layer, with
//! download and close at the top right. A press beside the file, or Esc, closes it.

use rune::{Color, El, ScrollSpec, SurfaceFit, col, el, icon, row, text};

use super::icons;
use super::theme::font;
use crate::i18n::{t, tf};
use crate::state::{DocumentStatus, ImageStatus, Preview, PreviewContent, State};

/// The widest a PDF page draws, in points.
const PAGE_WIDTH: f32 = 820.0;
/// Room around the file for the bar above it.
const MARGIN: f32 = 72.0;

#[cfg(test)]
mod tests;

pub fn preview(preview: &Preview, window: [f32; 2]) -> El {
    let backdrop = el()
        .key("preview-backdrop")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(Color::BLACK.alpha(0.86))
        .on_press(State::close_preview);
    let [w, h] = window;
    let content = match &preview.content {
        PreviewContent::Image { thumbnail, status: state } => {
            let image = match state {
                ImageStatus::Ready(image) => Some(image),
                _ => thumbnail.as_ref(),
            };
            let (width, height) = ((w - 2.0 * MARGIN).max(100.0), (h - 2.0 * MARGIN).max(100.0));
            let mut content = col().items_center();
            if let Some(image) = image {
                content = content.size(width, height).child(
                    el().key("preview-image")
                        .size(width, height)
                        .role("img")
                        .label(preview.attachment.name.clone())
                        .pixels_fit(image.pixels(), SurfaceFit::Fit),
                );
            }
            let message = match state {
                ImageStatus::Failed(error) => Some(tf!("preview.failed", error = error)),
                ImageStatus::Downloading | ImageStatus::Decoding(_) => Some(t!("preview.loading").to_owned()),
                ImageStatus::Ready(_) => None,
            };
            if let Some(message) = message {
                let label = status(&message);
                content = content.child(if image.is_some() {
                    col().absolute(0.0, height + 8.0).w_full().items_center().child(label)
                } else {
                    label
                });
            }
            content
        }
        PreviewContent::Document { pages, status: state } => {
            let mut list = col().key("preview-pages").items_center().gap(16.0).py(MARGIN);
            let width = PAGE_WIDTH.min(w - 2.0 * MARGIN).max(100.0);
            for (index, (pixels, (pw, ph))) in pages.iter().enumerate() {
                let height = width * (*ph as f32) / (*pw as f32).max(1.0);
                list = list.child(
                    el().key(format!("preview-page-{index}"))
                        .size(width, height)
                        .radius(4.0)
                        .shadow(4.0, 24.0, Color::BLACK.alpha(0.5))
                        .pixels_fit(pixels, SurfaceFit::Fit),
                );
            }
            match state {
                DocumentStatus::Failed(error) => list = list.child(status(&tf!("preview.failed", error = error))),
                DocumentStatus::Downloading | DocumentStatus::Rendering if pages.is_empty() => {
                    list = list.child(status(t!("preview.loading")));
                }
                _ => {}
            }
            col().key("preview-scroll").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(list)
        }
    };
    el().key("preview-layer")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .role("dialog")
        .label(preview.attachment.name.clone())
        .child(backdrop)
        .child(content)
        .child(bar(preview))
}

/// The file's name at the left; download and close at the right.
fn bar(preview: &Preview) -> El {
    let action = |glyph: &'static rune::Icon, label: &str, key: &str| {
        el().key(key.to_owned())
            .size(40.0, 40.0)
            .radius(20.0)
            .center()
            .fill(Color::WHITE.alpha(0.12))
            .hover(Color::WHITE.alpha(0.22))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(label.to_owned())
            .focusable(true)
            .child(icon(glyph, 20.0).color(Color::WHITE))
    };
    row()
        .key("preview-bar")
        .absolute(0.0, 0.0)
        .w_full()
        .px(20.0)
        .py(14.0)
        .gap(10.0)
        .center_y()
        .child(text(preview.attachment.name.clone()).font(font::BODY_STRONG).color(Color::WHITE).grow(1.0).truncate())
        .child(action(&icons::DOWNLOAD, t!("preview.download"), "preview-download").on_press(State::download_preview))
        .child(action(&icons::CLOSE, t!("preview.close"), "preview-close").on_press(State::close_preview))
}

fn status(message: &str) -> El {
    text(message.to_owned()).font(font::BODY).color(Color::WHITE.alpha(0.8))
}
