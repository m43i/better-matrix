//! Thumbnails that fit the box they were asked for.
//!
//! A homeserver answers a thumbnail request with the nearest size it has made, which can be
//! well over the box (Synapse answers 128×128 with its 320×240 rendition), or with the original
//! when it makes none. Apps size their image budgets by the box, so a thumbnail larger than it
//! is scaled down here, once, off the async threads, before it leaves the engine.

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, imageops::FilterType};

/// `bytes` scaled down to fit `width`×`height` when the image is larger, keeping its aspect
/// ratio and turned upright by its EXIF orientation; else `bytes` unchanged. Only PNG and JPEG
/// are scaled (other formats, animated ones among them, pass through), and anything that fails
/// to decode or encode passes through too.
pub(crate) fn fit(bytes: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    scaled(&bytes, width, height).unwrap_or(bytes)
}

fn scaled(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let format = reader.format().filter(|format| matches!(format, ImageFormat::Png | ImageFormat::Jpeg))?;
    let mut decoder = reader.into_decoder().ok()?;
    // Read from the header: an image that already fits is never decoded.
    let (w, h) = decoder.dimensions();
    let orientation = decoder.orientation().ok()?;
    let turned = !matches!(orientation, image::metadata::Orientation::NoTransforms);
    let (shown_w, shown_h) = if swaps_sides(orientation) { (h, w) } else { (w, h) };
    let scale = (f64::from(width) / f64::from(shown_w.max(1))).min(f64::from(height) / f64::from(shown_h.max(1)));
    if scale >= 1.0 {
        // It fits: a JPEG turned by EXIF keeps its bytes too, since the apps' decoders read EXIF.
        return None;
    }
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    if turned {
        image.apply_orientation(orientation);
    }
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let image = image.resize_exact(side(shown_w), side(shown_h), FilterType::Triangle);
    let mut out = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
            image.to_rgb8().write_with_encoder(encoder).ok()?;
        }
        _ => image.write_to(&mut out, ImageFormat::Png).ok()?,
    }
    Some(out.into_inner())
}

/// Whether showing an image upright swaps its width and height.
fn swaps_sides(orientation: image::metadata::Orientation) -> bool {
    use image::metadata::Orientation::{FlipHorizontal, FlipVertical, NoTransforms, Rotate180};
    !matches!(orientation, NoTransforms | Rotate180 | FlipHorizontal | FlipVertical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::new_rgba8(width, height).write_to(&mut out, ImageFormat::Png).expect("encodes");
        out.into_inner()
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(width, height).write_to(&mut out, ImageFormat::Jpeg).expect("encodes");
        out.into_inner()
    }

    fn size(bytes: &[u8]) -> (u32, u32) {
        image::load_from_memory(bytes).expect("decodes").to_rgba8().dimensions()
    }

    #[test]
    fn a_thumbnail_larger_than_its_box_should_be_scaled_to_fit_it() {
        assert_eq!(size(&fit(png(320, 240), 128, 128)), (128, 96));
    }

    #[test]
    fn a_jpeg_larger_than_its_box_should_stay_a_jpeg() {
        let scaled = fit(jpeg(640, 480), 128, 128);
        assert_eq!((image::guess_format(&scaled).expect("a format"), size(&scaled)), (ImageFormat::Jpeg, (128, 96)));
    }

    #[test]
    fn a_thumbnail_that_fits_its_box_should_keep_its_bytes() {
        let small = png(96, 96);
        assert_eq!(fit(small.clone(), 128, 128), small);
    }

    #[test]
    fn bytes_that_are_not_an_image_should_pass_through() {
        assert_eq!(fit(b"not an image".to_vec(), 16, 16), b"not an image");
    }

    #[test]
    fn a_gif_should_pass_through_so_it_keeps_its_frames() {
        // A GIF's header, which is all the format is told by.
        let gif = b"GIF89a\x2c\x01\x2c\x01\x00\x00\x00;".to_vec();
        assert_eq!(fit(gif.clone(), 64, 64), gif);
    }
}
