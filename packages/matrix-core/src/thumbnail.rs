//! Thumbnails that fit the box they were asked for.
//!
//! A homeserver answers a thumbnail request with the nearest size it has made, which can be
//! well over the box (Synapse answers 128×128 with its 320×240 rendition), and the SDK answers
//! one for an encrypted file with the whole original. Apps size their image budgets by the box,
//! so a thumbnail larger than it is scaled down here, once, off the async threads, before it
//! leaves the engine.

use std::{io::Cursor, sync::Arc};

use image::{
    DynamicImage, ExtendedColorType, ImageBuffer, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits, Rgba,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::{self, FilterType},
    metadata::Orientation,
};

/// The largest image scaled, a side: what the apps' decoders take too (Rune's limit). Larger
/// ones pass through for the app to refuse.
const MAX_SIDE: u32 = 8192;
/// The most a decode may allocate.
const MAX_ALLOC: u64 = 256 << 20;
/// Images scaled at once: a large photo takes a few hundred megabytes while it decodes.
static SCALING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// [`fit`] on a blocking thread, a couple at a time.
pub(crate) async fn fit_off_thread(bytes: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    let _permit = SCALING.acquire().await;
    let bytes = Arc::new(bytes);
    let source = Arc::clone(&bytes);
    let scaled = tokio::task::spawn_blocking(move || scaled_safely(&source, width, height)).await.ok().flatten();
    scaled.unwrap_or_else(|| Arc::try_unwrap(bytes).unwrap_or_else(|shared| shared.as_ref().clone()))
}

/// `bytes` scaled down to fit `width`×`height` when the image is larger, keeping its aspect
/// ratio, colour profile and alpha, and turned upright by its EXIF orientation; else `bytes`
/// unchanged. Only still PNGs and JPEGs up to [`MAX_SIDE`] a side are scaled (other formats,
/// animated PNGs among them, pass through), and anything that fails to decode or encode passes
/// through too.
#[cfg(test)]
fn fit(bytes: Vec<u8>, width: u32, height: u32) -> Vec<u8> {
    scaled_safely(&bytes, width, height).unwrap_or(bytes)
}

/// [`scaled`], with a codec that panics on a malformed file counting as a failure.
fn scaled_safely(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scaled(bytes, width, height))).ok().flatten()
}

fn scaled(bytes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let format = reader.format().filter(|format| matches!(format, ImageFormat::Png | ImageFormat::Jpeg))?;
    if format == ImageFormat::Png && animated_png(bytes) {
        return None;
    }
    let mut decoder = reader.into_decoder().ok()?;
    // Read from the header: an image that already fits is never decoded.
    let (w, h) = decoder.dimensions();
    if w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (shown_w, shown_h) = if swaps_sides(orientation) { (h, w) } else { (w, h) };
    let scale = (f64::from(width) / f64::from(shown_w.max(1))).min(f64::from(height) / f64::from(shown_h.max(1)));
    if scale >= 1.0 {
        // It fits: a JPEG turned by EXIF keeps its bytes too, since the apps' decoders read EXIF.
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    // The decoded image itself counts against the limit, not just the decoder's own buffers.
    limits.reserve(decoder.total_bytes()).ok()?;
    decoder.set_limits(limits).ok()?;
    let icc = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    let image = resized(&image, side(shown_w), side(shown_h));
    let mut out = Vec::new();
    match format {
        ImageFormat::Jpeg => {
            let rgb = image.to_rgb8();
            let mut encoder = JpegEncoder::new_with_quality(&mut out, 85);
            if let Some(icc) = icc {
                let _ = encoder.set_icc_profile(icc);
            }
            encoder.write_image(&rgb, rgb.width(), rgb.height(), ExtendedColorType::Rgb8).ok()?;
        }
        _ => {
            let mut encoder = PngEncoder::new(&mut out);
            if let Some(icc) = icc {
                let _ = encoder.set_icc_profile(icc);
            }
            let (w, h) = (image.width(), image.height());
            encoder.write_image(image.as_bytes(), w, h, image.color().into()).ok()?;
        }
    }
    Some(out)
}

/// `image` at `width`×`height`. One with alpha is scaled premultiplied, so its transparent
/// pixels don't darken the edges of what shows.
fn resized(image: &DynamicImage, width: u32, height: u32) -> DynamicImage {
    if !image.color().has_alpha() {
        return image.resize_exact(width, height, FilterType::Triangle);
    }
    let mut rgba = image.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    let mut scaled: ImageBuffer<Rgba<u8>, Vec<u8>> = imageops::resize(&rgba, width, height, FilterType::Triangle);
    for pixel in scaled.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            continue;
        }
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    DynamicImage::ImageRgba8(scaled)
}

/// Whether a PNG is animated: an `acTL` chunk comes before its first `IDAT`.
fn animated_png(bytes: &[u8]) -> bool {
    let mut at = 8;
    while let Some(header) = bytes.get(at..at + 8) {
        match &header[4..8] {
            b"acTL" => return true,
            b"IDAT" => return false,
            _ => {}
        }
        let Ok(length) = <[u8; 4]>::try_from(&header[..4]) else { return false };
        at += 12 + u32::from_be_bytes(length) as usize;
    }
    false
}

/// Whether showing an image upright swaps its width and height.
fn swaps_sides(orientation: Orientation) -> bool {
    use Orientation::{FlipHorizontal, FlipVertical, NoTransforms, Rotate180};
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

    fn decoded(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).expect("decodes")
    }

    fn size(bytes: &[u8]) -> (u32, u32) {
        decoded(bytes).to_rgba8().dimensions()
    }

    /// A PNG chunk: length, type, data and a CRC the scan doesn't check.
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(data.len()).expect("a short chunk").to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0; 4]);
        out
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
    fn a_tall_image_should_fit_the_height_of_its_box() {
        assert_eq!(size(&fit(jpeg(320, 640), 128, 128)), (64, 128));
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

    #[test]
    fn an_image_too_large_to_decode_should_pass_through() {
        // Its header claims more pixels a side than any app decodes.
        let mut huge = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = 20_000u32.to_be_bytes().to_vec();
        header.extend_from_slice(&20_000u32.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        huge.extend(chunk(b"IHDR", &header));
        huge.extend(chunk(b"IEND", &[]));
        assert_eq!(fit(huge.clone(), 128, 128), huge);
    }

    #[test]
    fn an_animated_png_should_be_told_apart_from_a_still_one() {
        let still = png(4, 4);
        let mut animated = still[..8].to_vec();
        animated.extend(chunk(b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]));
        animated.extend_from_slice(&still[8..]);
        assert_eq!((animated_png(&still), animated_png(&animated)), (false, true));
    }

    #[test]
    fn a_scaled_picture_with_transparent_edges_should_keep_its_colour() {
        // Red at 40% opacity, with fully transparent pixels around it.
        let mut picture = image::RgbaImage::new(256, 256);
        for (x, y, pixel) in picture.enumerate_pixels_mut() {
            let inside = (64..192).contains(&x) && (64..192).contains(&y);
            *pixel = if inside { Rgba([255, 0, 0, 102]) } else { Rgba([0; 4]) };
        }
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(picture).write_to(&mut out, ImageFormat::Png).expect("encodes");
        let scaled = decoded(&fit(out.into_inner(), 64, 64)).to_rgba8();
        // An edge pixel, partly covered: its colour stays red, only its alpha falls.
        let edge = scaled.get_pixel(16, 32);
        assert!(edge[3] > 0 && edge[0] >= 250, "{edge:?}");
    }
}
